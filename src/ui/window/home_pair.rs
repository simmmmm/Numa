use super::home::{account, curl, said, sign_in_first, Running, NUMA_HOME};
use super::*;
use numa::io::home::sync::{self, Link, Way};
use numa::io::home::{self as api, Failure};

#[derive(Clone, Copy, Default, PartialEq)]
pub(super) struct Commands {

    pub(super) here: bool,

    pub(super) home: bool,

    pub(super) stop: bool,
}

pub(super) fn commands(state: &App, library: &Library) -> Commands {
    match home_sync::way_of(state, library.id) {
        _ if home_sync::is_synced(state, library.id) => Commands { stop: true, ..Commands::default() },
        Some(Way::Down) => Commands { here: true, ..Commands::default() },
        Some(Way::Up) => Commands::default(),

        None => Commands { home: !account(state).address.is_empty() && !state.catalog.is_offline(library.id), ..Commands::default() },
    }
}

pub(super) fn offered(state: &App, libraries: &[Library]) -> Commands {
    let each: Vec<Commands> = libraries.iter().map(|library| commands(state, library)).collect();
    let all = |pick: fn(&Commands) -> bool| !each.is_empty() && each.iter().all(pick);
    let ways: Vec<Option<Way>> = libraries.iter().map(|library| home_sync::way_of(state, library.id)).collect();
    let one_way = ways.windows(2).all(|pair| pair[0] == pair[1]);
    Commands { here: all(|it| it.here), home: all(|it| it.home), stop: all(|it| it.stop) && one_way }
}

pub(super) fn label(commands: Commands) -> Option<&'static str> {
    match commands {
        Commands { here: true, .. } => Some("Sync on This Device"),
        Commands { home: true, .. } => Some("Sync to Numa Home"),
        Commands { stop: true, .. } => Some("Stop Syncing…"),
        _ => None,
    }
}

pub(super) fn run(state: &App, library: &Library) {
    match commands(state, library) {
        Commands { stop: true, .. } => stop_syncing(state, vec![library.clone()]),
        Commands { here: true, .. } | Commands { home: true, .. } => start_syncing(state, vec![library.clone()]),
        _ => {}
    }
}

fn window(state: &App) -> Option<adw::ApplicationWindow> {
    state.stack.root().and_downcast::<adw::ApplicationWindow>()
}

fn named(libraries: &[Library]) -> String {
    match libraries {
        [one] => format!("\u{201c}{}\u{201d}", one.label()),
        many => format!("{} Libraries", places::grouped(many.len() as i64)),
    }
}

fn redraw(state: &App) {
    home_sync::offer(state);
    if state.stack.visible_child_name().as_deref() == Some("libraries") {
        show_folders(state);
    } else {
        show_head(state);
    }
}

pub(super) fn start_syncing(state: &App, libraries: Vec<Library>) {
    let offered = offered(state, &libraries);
    if offered.here {
        sync_here(state, libraries);
    } else if offered.home {
        sync_to_home(state, libraries);
    }
}

fn sync_here(state: &App, libraries: Vec<Library>) {
    let Some(window) = window(state) else { return };
    let (count, bytes) = libraries
        .iter()
        .flat_map(|library| state.catalog.at_home(library.id).unwrap_or_default().into_values())
        .fold((0usize, 0u64), |(count, bytes), (_, size)| (count + 1, bytes + size));
    let free = libraries.first().and_then(|library| {
        let info = gio::File::for_path(&library.path).query_filesystem_info("filesystem::free", gio::Cancellable::NONE).ok()?;
        Some(info.attribute_uint64("filesystem::free"))
    });
    let room = free.is_none_or(|free| free > bytes);
    let body = match (count, room) {
        (0, _) => "Every photograph is here already. From now on only changes go back and forth.".to_string(),
        (_, true) => format!("{}, {}. They come once; after that only changes go back and forth.", home_sync::photographs(count), glib::format_size(bytes)),
        (_, false) => format!(
            "{}, {}. This computer has {} free: make room first.",
            home_sync::photographs(count),
            glib::format_size(bytes),
            glib::format_size(free.unwrap_or(0))
        ),
    };
    let dialog = adw::AlertDialog::new(Some(&format!("Sync {} on This Device?", named(&libraries))), Some(&body));
    dialog.add_response("cancel", "Cancel");
    dialog.add_response("sync", "Sync on This Device");
    dialog.set_response_enabled("sync", room);
    dialog.set_default_response(Some(if room { "sync" } else { "cancel" }));
    dialog.set_close_response("cancel");
    let state = state.clone();
    dialog.connect_response(None, move |_, response| {
        if response != "sync" {
            return;
        }
        for library in &libraries {
            home_sync::keep_here(&state, library, true);
            home_sync::sync_now(&state, library.clone());
        }
        redraw(&state);
    });
    dialog.present(Some(&window));
}

fn sync_to_home(state: &App, libraries: Vec<Library>) {
    let account = account(state);
    if !account.signed_in() {
        return state.toast(&sign_in_first(&account, &api::host(&account.address)));
    }
    let paths: Vec<PathBuf> = libraries.iter().flat_map(|library| state.catalog.photos(library.id, &Filter::default()).unwrap_or_default()).map(|photo| photo.path).collect();
    let state = state.clone();
    glib::spawn_future_local(async move {
        let count = paths.len();
        let bytes = gio::spawn_blocking(move || paths.iter().filter_map(|path| std::fs::metadata(path).ok()).map(|meta| meta.len()).sum::<u64>()).await.unwrap_or(0);
        let there = match sync::libraries(&curl(sync::list_libraries(&account.address), account.header(), &Running::default()).await) {
            Ok(there) => there,
            Err(failure) => return state.toast(&said(failure, &account)),
        };
        let same = |library: &Library| there.iter().find(|it| it.name.eq_ignore_ascii_case(&library.label())).cloned();

        let linked_here: Vec<String> = state.catalog.libraries().unwrap_or_default().iter().filter_map(|it| home_sync::linked(&state, it.id)).map(|link| link.library).collect();
        if let Some((library, _)) = libraries.iter().filter_map(|library| Some((library, same(library)?))).find(|(_, it)| linked_here.contains(&it.id)) {
            return state.toast(&format!("\u{201c}{}\u{201d} is on this computer as a {NUMA_HOME} library already: sync that one on this device", library.label()));
        }
        let joined: Vec<(Library, Option<sync::Library>)> = libraries.iter().map(|library| (library.clone(), same(library))).collect();
        ask_to_home(&state, joined, count, bytes);
    });
}

fn ask_to_home(state: &App, joined: Vec<(Library, Option<sync::Library>)>, count: usize, bytes: u64) {
    let Some(window) = window(state) else { return };
    let size = format!("{}, {}", home_sync::photographs(count), glib::format_size(bytes));
    let existing: Vec<&str> = joined.iter().filter_map(|(_, there)| there.as_ref().map(|there| there.name.as_str())).collect();
    let libraries: Vec<Library> = joined.iter().map(|(library, _)| library.clone()).collect();
    let (title, body, go) = match existing.as_slice() {
        [] => (format!("Sync {} to {NUMA_HOME}?", named(&libraries)), format!("{size}. They go once; after that only changes go back and forth."), "Sync to Numa Home"),
        [name] if libraries.len() == 1 => (
            "Sync with the Existing One?".to_string(),
            format!(
                "{NUMA_HOME} has a library called \u{201c}{name}\u{201d} already. The two become one: of these {size}, what it lacks goes to it once, and what only it has comes here; after that only changes go back and forth."
            ),
            "Sync with It",
        ),
        names => (
            format!("Sync {} to {NUMA_HOME}?", named(&libraries)),
            format!("{size}. They go once; after that only changes go back and forth. {} join the libraries of their names at {NUMA_HOME}.", names.join(", ")),
            "Sync to Numa Home",
        ),
    };
    let dialog = adw::AlertDialog::new(Some(&title), Some(&body));
    dialog.add_response("cancel", "Cancel");
    dialog.add_response("sync", go);
    dialog.set_default_response(Some("sync"));
    dialog.set_close_response("cancel");
    let state = state.clone();
    dialog.connect_response(None, move |_, response| {
        if response != "sync" {
            return;
        }
        let address = account(&state).address;

        let linked_here: Vec<String> = state.catalog.libraries().unwrap_or_default().iter().filter_map(|it| home_sync::linked(&state, it.id)).map(|link| link.library).collect();
        for (library, there) in &joined {
            if let Some(there) = there.as_ref().filter(|there| linked_here.contains(&there.id)) {
                state.toast(&format!("\u{201c}{}\u{201d} is on this computer as a {NUMA_HOME} library already: sync that one on this device", there.name));
                continue;
            }
            let id = there.as_ref().map(|there| there.id.clone()).unwrap_or_default();
            let link = Link { address: address.clone(), library: id, name: library.label(), way: Some(Way::Up), ..Link::default() };
            home_sync::keep(&state, library.id, &link);
            home_sync::sync_now(&state, library.clone());
        }
        redraw(&state);
    });
    dialog.present(Some(&window));
}

pub(super) fn stop_syncing(state: &App, libraries: Vec<Library>) {
    let Some(window) = window(state) else { return };
    let Some(way) = libraries.first().and_then(|library| home_sync::way_of(state, library.id)) else { return };
    let (body, keep, other) = match way {
        Way::Down => (format!("{NUMA_HOME} keeps everything."), "Keep the Photos Here", "Remove from This Device"),
        Way::Up => ("This computer keeps its photographs.".to_string(), "Keep the Library at Numa Home", "Move It to Numa Home's Trash"),
    };
    let dialog = adw::AlertDialog::new(Some(&format!("Stop Syncing {}?", named(&libraries))), Some(&body));
    dialog.add_response("cancel", "Cancel");
    dialog.add_response("keep", keep);
    dialog.add_response("other", other);
    dialog.set_response_appearance("other", adw::ResponseAppearance::Destructive);
    dialog.set_default_response(Some("cancel"));
    dialog.set_close_response("cancel");
    let state = state.clone();
    dialog.connect_response(None, move |_, response| {
        if response == "cancel" {
            return;
        }
        let other = response == "other";
        for library in &libraries {
            match way {
                Way::Down => stop_here(&state, library, other),
                Way::Up => stop_home(&state, library, other),
            }
        }
        redraw(&state);
    });
    dialog.present(Some(&window));
}

fn tell_home(state: &App, link: &Link) {
    let account = account(state);
    if !account.signed_in() || link.library.is_empty() {
        return;
    }
    let argv = sync::stop_syncing(&account.address, &link.library);
    glib::spawn_future_local(async move {
        if let Err(failure) = api::taken(&curl(argv, account.header(), &Running::default()).await) {
            log::info!("Numa Home: {}", said(failure, &account));
        }
    });
}

fn stop_here(state: &App, library: &Library, remove: bool) {
    let Some(link) = home_sync::linked(state, library.id) else { return };
    home_sync::keep_here(state, library, false);
    tell_home(state, &link);
    if !remove {
        return;
    }
    let at_home = state.catalog.at_home(library.id).unwrap_or_default();
    let going: Vec<(i64, PathBuf, String)> = state
        .catalog
        .photos(library.id, &Filter::default())
        .unwrap_or_default()
        .into_iter()
        .filter(|photo| !at_home.contains_key(&photo.id))
        .filter_map(|photo| {
            let path = photo.path.strip_prefix(&library.path).ok()?.to_string_lossy().into_owned();
            Some((photo.id, photo.path.clone(), link.photos.get(&path)?.id.clone()))
        })
        .collect();
    let (state, library) = (state.clone(), library.clone());
    glib::spawn_future_local(async move {
        let files: Vec<PathBuf> = going.iter().map(|(_, file, _)| file.clone()).collect();
        let label = format!("Removing {} from this computer…", home_sync::photographs(files.len()));
        let Ok(results) = busy(&state, &label, move || {
            files.iter().map(|file| (std::fs::metadata(file).map_or(0, |meta| meta.len()), gio::File::for_path(file).trash(gio::Cancellable::NONE))).collect::<Vec<_>>()
        })
        .await
        else {
            return;
        };
        let mut removed = 0;
        for ((photo, file, id), (size, trashed)) in going.iter().zip(results) {
            match trashed.map_err(|err| err.to_string()).and_then(|()| state.catalog.back_home(*photo, id, size)) {
                Ok(()) => removed += 1,
                Err(err) => log::warn!("Numa Home: {}: {err}", file.display()),
            }
        }
        state.toast(&format!("{} removed from this computer; {NUMA_HOME} keeps them", home_sync::photographs(removed)));
        if state.libraries.current.borrow().as_ref().is_some_and(|open| open.id == library.id) {
            reload_grid(&state);
        }
    });
}

fn stop_home(state: &App, library: &Library, trash: bool) {
    let Some(link) = home_sync::linked(state, library.id) else { return };
    if !trash || link.library.is_empty() {
        tell_home(state, &link);
        return home_sync::unlink(state, library.id);
    }
    let account = account(state);
    let (state, library) = (state.clone(), library.clone());
    glib::spawn_future_local(async move {
        tell_home(&state, &link);
        match api::taken(&curl(sync::delete_library(&account.address, &link.library), account.header(), &Running::default()).await) {
            Ok(()) => {
                home_sync::unlink(&state, library.id);
                state.toast(&format!("\u{201c}{}\u{201d} moved to {NUMA_HOME}'s trash; this computer keeps its photographs", link.name));
            }
            Err(Failure::Said(sentence)) => state.toast(&format!("{NUMA_HOME} kept \u{201c}{}\u{201d}, so it is still synced: {sentence}", link.name)),
            Err(failure) => state.toast(&said(failure, &account)),
        }
        redraw(&state);
    });
}
