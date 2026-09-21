use super::*;

pub(super) fn describe_new_library(state: &App, window: &adw::ApplicationWindow, count: usize) {
    let paths: Vec<PathBuf> = state.grid.cards.borrow().values().map(|(photo, _)| photo.path.clone()).take(300).collect();
    let (state, window) = (state.clone(), window.clone());
    glib::spawn_future_local(async move {
        let found = gio::spawn_blocking(move || {
            let mut cameras: std::collections::BTreeMap<String, (usize, bool)> = Default::default();
            let mut unread = 0;
            for path in &paths {
                match numa::io::raw::summary(path) {
                    Some(summary) if numa::io::raw::is_raw(path) => {
                        let name = summary.camera.clone().unwrap_or_else(|| format!("{} {}", summary.make, summary.model));
                        let entry = cameras.entry(name).or_insert_with(|| {
                            (0, numa::io::dcp::find(&summary.make, &summary.model).is_some())
                        });
                        entry.0 += 1;
                    }
                    Some(_) => {}
                    None if numa::io::raw::is_raw(path) => unread += 1,
                    None => {}
                }
            }
            (cameras, unread, paths.len())
        })
        .await;
        let Ok((cameras, unread, sampled)) = found else { return };

        let mut lines = Vec::new();
        for (camera, (_, profiled)) in &cameras {
            lines.push(if *profiled {
                format!("• {camera} — camera profile found")
            } else {
                format!("• {camera} — no camera profile; colour comes from the matrix in the file, which is fine")
            });
        }
        if unread > 0 {
            lines.push(format!("• {unread} RAW file(s) whose details could not be read"));
        }
        let cameras_text = if lines.is_empty() {
            "No RAW files in the first photographs looked at.".to_string()
        } else {
            format!("Cameras, from {sampled} of the {count} photographs:\n{}", lines.join("\n"))
        };
        let body = format!(
            "{cameras_text}\n\nAnalyse measures sharpness, blown highlights, bursts and faces. \
             Suggested ratings, “best of burst” and People depend on it. It runs while you \
             browse and can be stopped; for {count} photographs expect some minutes."
        );
        let alert = adw::AlertDialog::new(Some("What Numa found"), Some(&body));
        alert.add_response("later", "Later");
        alert.add_response("analyse", "Analyse Now");
        alert.set_response_appearance("analyse", adw::ResponseAppearance::Suggested);
        alert.set_default_response(Some("analyse"));
        let state = state.clone();
        alert.connect_response(None, move |_, response| {
            if response == "analyse" {
                if let Some(button) = state.libraries.analyse_button.borrow().as_ref() {
                    button.emit_clicked();
                }
            }
        });
        alert.present(Some(&window));
    });
}

pub(super) fn refresh_folders(state: &App, library: &Library, photos: &[Photo]) {
    let mut folders: std::collections::BTreeSet<PathBuf> = Default::default();
    for photo in photos {
        let Ok(relative) = photo.path.strip_prefix(&library.path) else { continue };
        let mut parent = relative.parent();
        while let Some(folder) = parent.filter(|folder| !folder.as_os_str().is_empty()) {
            folders.insert(folder.to_path_buf());
            parent = folder.parent();
        }
    }
    let folders: Vec<PathBuf> = folders.into_iter().collect();
    if *state.libraries.folders.borrow() == folders && !folders.is_empty() {
        return;
    }
    let labels: Vec<String> = std::iter::once("All folders".to_string())
        .chain(folders.iter().map(|folder| {
            let depth = folder.components().count().saturating_sub(1);
            let name = folder.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
            format!("{}{name}", "    ".repeat(depth))
        }))
        .collect();
    let chosen = state.libraries.folder.borrow().clone();
    let selected = chosen.and_then(|chosen| folders.iter().position(|folder| *folder == chosen)).map_or(0, |index| index + 1);

    state.applying.set(true);
    let picker = &state.libraries.folder_picker;
    picker.set_model(Some(&gtk::StringList::new(&labels.iter().map(String::as_str).collect::<Vec<_>>())));
    picker.set_selected(selected as u32);
    picker.set_visible(!folders.is_empty());
    state.applying.set(false);
    if selected == 0 {
        state.libraries.folder.replace(None);
    }
    *state.libraries.folders.borrow_mut() = folders;
}

pub(super) fn rescan_in_background(state: &App) {
    let Some(library) = state.libraries.current.borrow().clone() else { return };

    if folder_is_missing(&library.path) {
        return;
    }
    if state.libraries.scanning.replace(true) {
        return;
    }
    let state = state.clone();
    glib::spawn_future_local(async move {
        let root = library.path.clone();
        let found = gio::spawn_blocking(move || numa::io::catalog::scan(&root)).await;
        state.libraries.scanning.set(false);
        let Ok(found) = found else { return };

        if state.libraries.current.borrow().as_ref().map(|open| open.id) != Some(library.id) {
            return;
        }
        let changes = match state.catalog.apply_scan(&library, &found) {
            Ok(changes) => changes,
            Err(err) => {
                log::warn!("rescan of {}: {err}", library.path.display());
                return;
            }
        };
        if !changes.any() {
            return;
        }
        if state.stack.visible_child_name().as_deref() != Some("library") {
            state.grid.stale.set(true);
            return;
        }
        let adjustment = state.grid.scroller.vadjustment();
        let was = adjustment.value();
        reload_grid(&state);
        glib::timeout_add_local_once(std::time::Duration::from_millis(80), move || adjustment.set_value(was));
        if changes.added > 0 {
            state.toast(&match changes.added {
                1 => "1 new photo".to_string(),
                n => format!("{n} new photos"),
            });
        }
    });
}

pub(super) fn folder_is_missing(path: &Path) -> bool {
    thread_local! {
        static SAID: RefCell<std::collections::HashSet<PathBuf>> =
            RefCell::new(std::collections::HashSet::new());
    }
    if path.is_dir() {
        SAID.with(|said| said.borrow_mut().remove(path));
        return false;
    }
    if SAID.with(|said| said.borrow_mut().insert(path.to_path_buf())) {
        log::warn!("{} is not there — is the drive connected?", path.display());
    }
    true
}

pub(super) fn set_row_title(row: &impl IsA<adw::PreferencesRow>, text: &str) {
    let row = row.as_ref();
    row.set_use_markup(false);
    row.set_title(text);
}

pub(super) fn set_row_subtitle(row: &impl IsA<adw::ActionRow>, text: &str) {
    row.as_ref().set_subtitle(&glib::markup_escape_text(text));
}

pub(super) fn reload_libraries(state: &App) {
    let libraries = state.catalog.libraries().unwrap_or_default();

    let previous = state
        .libraries.current
        .borrow()
        .as_ref()
        .map(|library| library.id)
        .or_else(|| state.catalog.setting(LAST_LIBRARY)?.parse().ok());
    let is_empty = libraries.is_empty();

    *state.libraries.all.borrow_mut() = libraries;

    state.libraries.picker.set_visible(!is_empty);

    match previous {
        Some(id) => select_library(state, id),
        None => select_library_index(state, 0),
    }
}

pub(super) fn remember_library(state: &App, library: Option<&Library>) {
    let Some(library) = library else { return };
    if let Err(err) = state.catalog.set_setting(LAST_LIBRARY, &library.id.to_string()) {
        log::warn!("could not remember the library: {err}");
    }
}

pub(super) fn select_library(state: &App, id: i64) {
    let index = state.libraries.all.borrow().iter().position(|library| library.id == id);
    select_library_index(state, index.unwrap_or(0) as u32);
}

pub(super) fn select_library_index(state: &App, index: u32) {
    let selected = state.libraries.all.borrow().get(index as usize).cloned();
    remember_library(state, selected.as_ref());

    if state.libraries.current.borrow().as_ref().map(|library| library.id) != selected.as_ref().map(|library| library.id) {
        state.libraries.folder.replace(None);
    }
    *state.libraries.current.borrow_mut() = selected;

    reload_grid(state);
}

pub(super) fn copy_into_library(state: &App, library: Library, dropped: Vec<PathBuf>) {
    let state = state.clone();
    glib::spawn_future_local(async move {
        let folder = library.path.clone();
        let Ok(copied) = busy(&state, "Copying into the library…", move || {
            numa::io::catalog::copy_into(&dropped, &folder)
        })
        .await
        else {
            state.toast("Copying failed — see the log for which file");
            return;
        };

        if let Err(err) = state.catalog.sync_library(&library) {
            state.toast(&format!("Copied, but the library could not be rescanned: {err}"));
            return;
        }

        if state.libraries.current.borrow().as_ref().map(|open| open.id) == Some(library.id) {
            reload_grid(&state);
        }

        let mut said = match copied.photos {
            0 => "No photographs copied".to_string(),
            1 => "Copied 1 photograph".to_string(),
            n => format!("Copied {n} photographs"),
        };
        if !copied.existing.is_empty() {
            said.push_str(&format!(" · {} already there, left as they were", copied.existing.len()));
        }
        if !copied.failed.is_empty() {
            said.push_str(&format!(" · {} could not be copied", copied.failed.len()));
        }
        state.toast(&said);
    });
}

#[derive(Clone)]
pub(super) struct State {
    pub(super) current: Rc<RefCell<Option<Library>>>,
    pub(super) all: Rc<RefCell<Vec<Library>>>,
    pub(super) filter: Rc<RefCell<Filter>>,

    pub(super) places: Rc<RefCell<Vec<Place>>>,
    pub(super) picker: gtk::DropDown,

    pub(super) folder: Rc<RefCell<Option<PathBuf>>>,
    pub(super) folder_picker: gtk::DropDown,
    pub(super) folders: Rc<RefCell<Vec<PathBuf>>>,

    pub(super) switching: Rc<Cell<bool>>,

    pub(super) scanning: Rc<Cell<bool>>,

    pub(super) albums_menu: gio::Menu,

    pub(super) scale: Rc<Cell<cull::Scale>>,

    pub(super) analyse_button: Rc<RefCell<Option<adw::SplitButton>>>,

    pub(super) shelves: gtk::Box,
}

impl State {
    pub(super) fn new() -> Self {
        Self {
            current: Rc::new(RefCell::new(None)),
            all: Rc::new(RefCell::new(Vec::new())),
            filter: Rc::new(RefCell::new(Filter::default())),
            places: Rc::new(RefCell::new(Vec::new())),
            shelves: gtk::Box::new(gtk::Orientation::Vertical, 0),
            picker: gtk::DropDown::from_strings(&[]),
            folder: Rc::default(),
            folder_picker: gtk::DropDown::from_strings(&["All folders"]),
            folders: Rc::default(),
            switching: Rc::new(Cell::new(false)),
            scanning: Rc::new(Cell::new(false)),
            albums_menu: gio::Menu::new(),
            scale: Rc::new(Cell::new(cull::Scale::default())),
            analyse_button: Rc::default(),
        }
    }
}
