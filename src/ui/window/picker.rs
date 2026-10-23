use super::*;

pub(super) fn shelve_libraries(libraries: &[Library]) -> (Vec<&Library>, Vec<(String, Vec<&Library>)>) {
    let mut runs: Vec<(String, Vec<&Library>)> = Vec::new();
    for library in libraries {
        let parent = library
            .path
            .parent()
            .and_then(|path| path.file_name())
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_default();
        match runs.last_mut() {
            Some((holding, run)) if *holding == parent && !parent.is_empty() => run.push(library),
            _ => runs.push((parent, vec![library])),
        }
    }

    let (grouped, alone): (Vec<_>, Vec<_>) = match runs.len() {
        0 | 1 => (Vec::new(), runs),
        _ => runs.into_iter().partition(|(_, run)| run.len() > 1),
    };
    (alone.into_iter().flat_map(|(_, run)| run).collect(), grouped)
}

#[derive(Clone)]
pub(super) enum Place {

    Everywhere,
    Library(Library),

    Album(String),

    Person(String),
}

pub(super) fn refresh_places(state: &App) {
    let names = state.catalog.names().unwrap_or_else(|err| {
        log::warn!("could not read who has a name: {err}");
        Vec::new()
    });
    let albums = state.catalog.albums().unwrap_or_else(|err| {
        log::warn!("could not read the albums: {err}");
        Vec::new()
    });
    let libraries = state.libraries.all.borrow().clone();

    let (at_home, here): (Vec<Library>, Vec<Library>) = libraries.iter().cloned().partition(|library| home_sync::is_home(state, library.id));
    {
        let mut filter = state.libraries.filter.borrow_mut();
        if filter.person.as_ref().is_some_and(|person| !names.iter().any(|name| name.eq_ignore_ascii_case(person))) {
            filter.person = None;
        }
        if filter.album.as_ref().is_some_and(|album| !albums.iter().any(|(key, _)| key == album)) {
            filter.album = None;
        }
    }
    fill_albums_menu(state, &albums);

    let mut here: Vec<&Library> = here.iter().collect();
    places::in_order(state, &mut here);
    let mut at_home: Vec<&Library> = at_home.iter().collect();
    places::in_order(state, &mut at_home);
    let mut places = vec![Place::Everywhere];
    places.extend(here.into_iter().chain(at_home).map(|library| Place::Library(library.clone())));
    places.extend(albums.into_iter().map(|(key, _)| Place::Album(key)));
    places.extend(names.into_iter().map(Place::Person));
    *state.libraries.places.borrow_mut() = places;
    sidebar::fill(state);

    if state.stack.visible_child_name().as_deref() == Some("albums") {
        show_albums(state);
    }
}

pub(super) fn current_place(state: &App) -> Place {
    let filter = state.libraries.filter.borrow();
    match (&filter.album, &filter.person) {
        _ if filter.all_libraries => Place::Everywhere,
        (Some(album), _) => Place::Album(album.clone()),
        (_, Some(person)) => Place::Person(person.clone()),
        _ => state.libraries.current.borrow().clone().map_or(Place::Everywhere, Place::Library),
    }
}

pub(super) fn show_place(state: &App, place: Place) {
    state.stack.set_visible_child_name("library");
    choose_place(state, place);
}

pub(super) fn choose_place(state: &App, place: Place) {
    let unchanged = {
        let filter = state.libraries.filter.borrow();
        match &place {
            Place::Everywhere => filter.all_libraries,
            Place::Album(key) => filter.album.as_ref() == Some(key),
            Place::Person(name) => filter.person.as_ref() == Some(name),
            Place::Library(library) => {
                !filter.spans_libraries() && state.libraries.current.borrow().as_ref().is_some_and(|open| open.id == library.id)
            }
        }
    };

    if unchanged && !state.grid.stale.replace(false) {
        return;
    }

    state.libraries.narrow.replace(Default::default());

    state.libraries.filter.borrow_mut().in_one_library();
    let library = matches!(place, Place::Library(_));
    match place {
        Place::Everywhere => state.libraries.filter.borrow_mut().all_libraries = true,
        Place::Album(key) => state.libraries.filter.borrow_mut().album = Some(key),
        Place::Person(name) => state.libraries.filter.borrow_mut().person = Some(name),
        Place::Library(library) => {
            remember_library(state, Some(&library));
            *state.libraries.current.borrow_mut() = Some(library);
        }
    }
    reload_grid(state);

    offer_clocks(state);

    if library {
        home_sync::opened(state);
    }
}

pub(super) fn rescan_everywhere(state: &App) {
    let libraries: Vec<Library> = state
        .libraries
        .all
        .borrow()
        .iter()
        .filter(|library| !folder_is_missing(&library.path))
        .cloned()
        .collect();
    sync_in_background(state, libraries, |state, added| {
        reload_grid(state);
        state.toast(&format!("Rescanned: {added} new photo(s)"));
    });
}
