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

pub(super) fn refresh_picker(state: &App) {
    let started = std::time::Instant::now();
    let names = state.catalog.names().unwrap_or_else(|err| {
        log::warn!("could not read who has a name: {err}");
        Vec::new()
    });
    let albums = state.catalog.albums().unwrap_or_else(|err| {
        log::warn!("could not read the albums: {err}");
        Vec::new()
    });
    let libraries = state.libraries.all.borrow().clone();

    let everywhere = libraries.len() > 1;
    {
        let mut filter = state.libraries.filter.borrow_mut();
        if filter.person.as_ref().is_some_and(|person| !names.iter().any(|name| name.eq_ignore_ascii_case(person))) {
            filter.person = None;
        }
        if filter.album.as_ref().is_some_and(|album| !albums.iter().any(|(key, _)| key == album)) {
            filter.album = None;
        }
        filter.all_libraries &= everywhere;
    }
    fill_albums_menu(state, &albums);

    let mut places = Vec::new();
    let mut sections: Vec<(String, Vec<String>)> = Vec::new();

    let (mut loose, mut grouped) = shelve_libraries(&libraries);

    places::in_order(state, &mut loose);
    grouped.iter_mut().for_each(|(_, shelf)| places::in_order(state, shelf));

    let mut top = Vec::new();
    if everywhere {
        places.push(Place::Everywhere);
        top.push("All libraries".to_string());
    }
    for library in &loose {
        places.push(Place::Library((*library).clone()));
        top.push(library.label());
    }
    sections.push(("Libraries".to_string(), top));

    for (parent, shelf) in &grouped {
        let mut labels = Vec::new();
        for library in shelf {
            places.push(Place::Library((*library).clone()));
            labels.push(library.label());
        }
        sections.push((parent.clone(), labels));
    }

    places.extend(albums.iter().map(|(key, _)| Place::Album(key.clone())));
    sections.push(("Albums".to_string(), albums.iter().map(|(_, name)| name.clone()).collect()));
    places.extend(names.iter().cloned().map(Place::Person));
    sections.push(("People".to_string(), names));

    let index = {
        let filter = state.libraries.filter.borrow();
        let open = state.libraries.current.borrow().as_ref().map(|open| open.id);
        places.iter().position(|place| match place {
            Place::Everywhere => filter.all_libraries,
            Place::Album(key) => filter.album.as_ref() == Some(key),
            Place::Person(name) => filter.person.as_ref().is_some_and(|person| person.eq_ignore_ascii_case(name)),
            Place::Library(library) => !filter.spans_libraries() && open == Some(library.id),
        })
    };

    state.libraries.switching.set(true);

    let same = state
        .libraries.picker
        .model()
        .and_downcast::<crate::ui::sections::Sections>()
        .is_some_and(|model| model.is(&sections));

    *state.libraries.places.borrow_mut() = places;
    if !same {
        let model = crate::ui::sections::Sections::new(&sections);
        state.libraries.picker.set_model(Some(&model));

        state.libraries.picker.set_header_factory((model.section_count() > 1).then(|| places::headers(&model)).as_ref());

        places::dress(state);
    }
    if let Some(index) = index {
        state.libraries.picker.set_selected(index as u32);
    }
    state.libraries.switching.set(false);

    log::debug!("picker filled in {:.1} ms", started.elapsed().as_secs_f64() * 1000.0);
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
    if unchanged {
        return;
    }

    state.libraries.filter.borrow_mut().in_one_library();
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

pub(super) fn selected_ids(state: &App) -> Vec<i64> {
    selected_cards(state).iter().filter_map(|child| child.widget_name().parse::<i64>().ok()).collect()
}
