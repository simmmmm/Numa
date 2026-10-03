use super::*;

pub(super) fn add_library_dialog(state: &App, window: &adw::ApplicationWindow) {
    let dialog = gtk::FileDialog::new();
    dialog.set_title("Add Folder to Library");

    let beside = state
        .libraries.current
        .borrow()
        .as_ref()
        .and_then(|library| library.path.parent().map(Path::to_path_buf))
        .filter(|path| path.is_dir());
    if let Some(path) = beside {
        dialog.set_initial_folder(Some(&gio::File::for_path(path)));
    }

    let state = state.clone();
    let window = window.clone();
    glib::spawn_future_local(async move {
        let Ok(file) = dialog.select_folder_future(Some(&window)).await else { return };
        let Some(path) = file.path() else { return };

        let added = match state.catalog.add_library(&path) {
            Ok(library) => sync_library_beside(&state, &library, "Reading the folder…").await.map(|count| (count, library)),
            Err(err) => Err(err),
        };

        match added {
            Ok((count, library)) => {

                state.libraries.filter.borrow_mut().in_one_library();
                reload_libraries(&state);
                select_library(&state, library.id);
                state.toast(&match count {
                    0 => format!("No photos found in {}", path.display()),
                    n => format!("Added {n} photo(s) from {}", path.display()),
                });
                if count > 0 {
                    describe_new_library(&state, &window, library.id, count);
                }
            }

            Err(err) => {
                let alert = adw::AlertDialog::new(Some("Could Not Add This Folder"), Some(&err));
                alert.add_response("ok", "OK");
                alert.present(Some(&window));
            }
        }
    });
}

pub(super) fn remember_recent(path: &Path) {
    let uri = gio::File::for_path(path).uri();
    gtk::RecentManager::default().add_item(&uri);
}

pub(super) fn open_path(state: &App, window: &adw::ApplicationWindow, path: PathBuf) {
    let path = path.canonicalize().unwrap_or(path);
    if !raw::is_supported(&path) {
        state.toast("Numa cannot open that kind of file");
        return;
    }
    let libraries = state.catalog.libraries().unwrap_or_default();

    let holder = libraries
        .iter()
        .filter_map(|library| {
            let root = library.path.canonicalize().unwrap_or_else(|_| library.path.clone());
            Some((library.clone(), library.path.join(path.strip_prefix(&root).ok()?)))
        })
        .max_by_key(|(library, _)| library.path.components().count());

    let show = |state: &App, library: Library, path: PathBuf| {
        let state = state.clone();
        glib::spawn_future_local(async move {

            if let Err(err) = sync_library_beside(&state, &library, "Reading the library…").await {
                state.toast(&format!("Could not read the library: {err}"));
                return;
            }
            show_opened(&state, library, &path);
        });
    };

    if let Some((library, path)) = holder {
        show(state, library, path);
        return;
    }

    let Some(folder) = path.parent().map(Path::to_path_buf) else { return };
    let alert = adw::AlertDialog::new(
        Some("Add this folder as a library?"),
        Some(&format!(
            "Edits are kept in a library, inside its folder, so a photograph is opened as part of one. \
             “{}” is not in any library yet.",
            folder.display()
        )),
    );
    alert.add_response("cancel", "Cancel");
    alert.add_response("add", "Add and Open");
    alert.set_response_appearance("add", adw::ResponseAppearance::Suggested);
    let state = state.clone();
    alert.connect_response(None, move |_, response| {
        if response != "add" {
            return;
        }
        match state.catalog.add_library(&folder) {
            Ok(library) => show(&state, library, path.clone()),
            Err(err) => state.toast(&format!("Could not add the folder: {err}")),
        }
    });
    alert.present(Some(window));
}

fn show_opened(state: &App, library: Library, path: &Path) {
    state.libraries.filter.borrow_mut().in_one_library();
    reload_libraries(state);
    select_library(state, library.id);
    let find = || state.grid.cards.borrow().iter().find(|(_, photo)| photo.path == path).map(|(id, _)| *id);

    let id = find().or_else(|| {
        let (sort, reversed) = {
            let filter = state.libraries.filter.borrow();
            (filter.sort, filter.reversed)
        };
        state.libraries.filter.replace(Filter { sort, reversed, ..Filter::default() });
        reload_grid(state);
        find()
    });
    match id {
        Some(id) => {
            remember_recent(path);

            if !show_in_loupe(state, id) {
                open_photo(state, id);
            }
        }
        None => state.toast("The photograph is not in the library"),
    }
}
