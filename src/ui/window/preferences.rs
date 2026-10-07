use super::*;

pub(super) fn preferences_dialog(state: &App, window: &adw::ApplicationWindow) {
    let dialog = adw::PreferencesDialog::new();
    dialog.set_search_enabled(true);
    let folder_row = |title: &str, dir: PathBuf| folder_row(window, title, dir);
    dialog.add(&general_page(state, &dialog));
    dialog.add(&addons_page(state, &dialog, &folder_row));
    dialog.add(&storage_page(state, &folder_row));
    dialog.present(Some(window));
}

fn folder_row(window: &adw::ApplicationWindow, title: &str, dir: PathBuf) -> adw::ActionRow {
    let row = adw::ActionRow::new();
    row.set_title(title);
    set_row_subtitle(&row, &dir.display().to_string());
    row.set_subtitle_selectable(true);
    let open = gtk::Button::from_icon_name("folder-open-symbolic");
    open.set_tooltip_text(Some("Open in the file manager"));
    open.set_valign(gtk::Align::Center);
    open.add_css_class("flat");
    open.connect_clicked(glib::clone!(
        #[weak] window,
        move |_| {
            if let Err(err) = std::fs::create_dir_all(&dir) {
                log::warn!("could not create {}: {err}", dir.display());
            }
            gtk::FileLauncher::new(Some(&gio::File::for_path(&dir))).launch(
                Some(&window),
                None::<&gio::Cancellable>,
                |result| {
                    if let Err(err) = result {
                        log::warn!("could not open the folder: {err}");
                    }
                },
            );
        }
    ));
    row.add_suffix(&open);
    row
}

fn general_page(state: &App, dialog: &adw::PreferencesDialog) -> adw::PreferencesPage {
    let page = adw::PreferencesPage::new();
    page.set_title("General");
    page.set_icon_name(Some("emblem-system-symbolic"));

    page.add(&colour_group(state));
    page.add(&opening_group(dialog));

    let imports = adw::PreferencesGroup::new();
    imports.set_title("Imports");
    imports.set_description(Some(
        "Every photograph an import listed in its receipt and still there is read again and compared. \
         Only a photograph that has changed is mentioned.",
    ));
    imports.add(&import_done::recheck_row(state));
    page.add(&imports);

    if let Some(path) = appimage() {
        let menu = adw::PreferencesGroup::new();
        menu.set_title("Applications Menu");
        let row = adw::SwitchRow::new();
        row.set_title("Show Numa in the applications menu");
        set_row_subtitle(&row, &path.display().to_string());
        row.set_active(menu_entry_target().is_some());
        row.connect_active_notify(glib::clone!(
            #[weak] dialog,
            move |row| {
                if row.is_active() {
                    if let Err(err) = write_menu_entry(&path) {
                        dialog.add_toast(adw::Toast::new(&format!("Could not add the menu entry: {err}")));
                        row.set_active(false);
                    }
                } else {
                    remove_menu_entry();
                }
            }
        ));
        menu.add(&row);
        page.add(&menu);
    }

    if checks_itself() {
        let updates = adw::PreferencesGroup::new();
        updates.set_title("Updates");
        let check = adw::SwitchRow::new();
        check.set_title("Check for new versions");
        check.set_subtitle("Once a day, from the releases page on GitHub. Nothing about you or your photographs is sent.");
        check.set_active(state.catalog.setting(UPDATE_CHECK).as_deref() == Some("yes"));
        check.connect_active_notify(glib::clone!(
            #[strong] state,
            move |row| {
                let _ = state.catalog.set_setting(UPDATE_CHECK, if row.is_active() { "yes" } else { "no" });
            }
        ));
        updates.add(&check);
        page.add(&updates);
    }
    page
}

fn addons_page(
    state: &App,
    dialog: &adw::PreferencesDialog,
    folder_row: &dyn Fn(&str, PathBuf) -> adw::ActionRow,
) -> adw::PreferencesPage {
    let page = adw::PreferencesPage::new();
    page.set_title("Add-ons");
    page.set_icon_name(Some("application-x-addon-symbolic"));
    page.add(&downloads::models_group(dialog, folder_row));
    page.add(&downloads::gpu_group(state, dialog));
    let profiles = downloads::profiles_group(dialog, folder_row);

    profiles.add(&own_profile::row(state, dialog));
    page.add(&profiles);
    page
}

fn storage_page(state: &App, folder_row: &dyn Fn(&str, PathBuf) -> adw::ActionRow) -> adw::PreferencesPage {
    let page = adw::PreferencesPage::new();
    page.set_title("Storage");
    page.set_icon_name(Some("drive-harddisk-symbolic"));
    page.add(&previews_group(state));

    let storage = adw::PreferencesGroup::new();
    storage.set_title("Numa's Files");
    storage.set_description(Some(
        "Ratings, edits and names are kept in a hidden .numa folder inside each library, \
         so they travel with the photographs. These are Numa's own files on this computer.",
    ));
    let rows = [
        ("Settings and list of libraries", numa::core::paths::data_dir()),
        ("Models", numa::core::paths::models_dir()),
        ("Presets", numa::io::presets::dir()),
        ("LUTs", numa::core::paths::luts_dir()),
        ("Thumbnails", numa::io::thumbs::cache_dir()),
        ("Masks the models made", numa::io::mask_store::dir()),
    ];

    let own_rows: Vec<PathBuf> = rows.iter().map(|(_, dir)| dir.clone()).collect();
    for (title, dir) in rows {
        let row = folder_row(title, dir.clone());
        let others = own_rows.iter().filter(|other| **other != dir).cloned().collect();
        show_size(&row, dir.clone(), others);
        if dir == numa::io::thumbs::cache_dir() || dir == numa::io::mask_store::dir() {
            row.add_suffix(&clear_button(&row, dir));
        }
        storage.add(&row);
    }
    page.add(&storage);

    let uninstall = adw::PreferencesGroup::new();
    uninstall.set_title("Removing Numa");
    let how = if sandboxed() { "Uninstall Numa in Software" } else { "Delete the AppImage" };
    uninstall.set_description(Some(&format!(
        "{how}, and the folders above if their contents should go too. \
         Each library keeps its ratings, edits and names in a hidden .numa folder inside it: \
         delete that as well only if that work should go with it. The photographs themselves \
         are never touched."
    )));
    page.add(&uninstall);
    page
}

const PREVIEW_STOPS: [u64; 7] = [0, 512 << 20, 1 << 30, 2 << 30, 5 << 30, 10 << 30, 20 << 30];
const PREVIEW_BUDGET: &str = "preview-budget";

pub(super) fn start_preview_budget(catalog: &Catalog) {
    if let Some(bytes) = catalog.setting(PREVIEW_BUDGET).and_then(|bytes| bytes.parse().ok()) {
        numa::io::previews::set_budget(bytes);
    }
}

fn previews_group(state: &App) -> adw::PreferencesGroup {
    let group = adw::PreferencesGroup::new();
    group.set_title("Previews");
    group.set_description(Some(
        "A photograph opened before opens from its developed preview instead of being developed again. \
         They are kept in each library's own .numa folder, on the drive with the photographs.",
    ));
    let row = adw::ActionRow::new();
    row.set_title("Room in each library");
    let scale = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, (PREVIEW_STOPS.len() - 1) as f64, 1.0);
    scale.set_round_digits(0);
    scale.set_draw_value(true);
    scale.set_value_pos(gtk::PositionType::Left);
    scale.set_format_value_func(|_, at| match PREVIEW_STOPS[at.round() as usize] {
        0 => "Off".to_string(),
        bytes => format!("{} GB", bytes as f64 / (1u64 << 30) as f64),
    });
    for at in 0..PREVIEW_STOPS.len() {
        scale.add_mark(at as f64, gtk::PositionType::Bottom, None);
    }
    scale.set_width_request(240);
    scale.set_valign(gtk::Align::Center);
    let now = numa::io::previews::budget();
    let at = PREVIEW_STOPS.iter().position(|stop| *stop >= now).unwrap_or(PREVIEW_STOPS.len() - 1);
    scale.set_value(at as f64);
    row.add_suffix(&scale);
    show_previews(state, &row);

    scale.connect_value_changed(glib::clone!(
        #[strong] state,
        #[weak] row,
        move |scale| {

            let at = scale.value().round();
            if scale.value() != at {
                scale.set_value(at);
                return;
            }
            let bytes = PREVIEW_STOPS[at as usize];
            if bytes == numa::io::previews::budget() {
                return;
            }
            numa::io::previews::set_budget(bytes);
            let _ = state.catalog.set_setting(PREVIEW_BUDGET, &bytes.to_string());

            let libraries: Vec<PathBuf> = state.libraries.all.borrow().iter().map(|library| library.path.clone()).collect();
            glib::spawn_future_local(glib::clone!(
                #[strong] state,
                #[weak] row,
                async move {
                    let _ = gio::spawn_blocking(move || {
                        for library in libraries {
                            numa::io::previews::trim(&library.join(numa::io::catalog::LIBRARY_DIR).join("previews"), bytes);
                        }
                    })
                    .await;
                    show_previews(&state, &row);
                }
            ));
        }
    ));
    group.add(&row);
    group
}

fn show_previews(state: &App, row: &adw::ActionRow) {
    let Some(library) = state.libraries.current.borrow().as_ref().map(|library| library.path.clone()) else {
        row.set_subtitle("Per library · no library is open");
        return;
    };
    let dir = library.join(numa::io::catalog::LIBRARY_DIR).join("previews");
    glib::spawn_future_local(glib::clone!(
        #[weak] row,
        async move {
            let Ok((bytes, count)) = gio::spawn_blocking(move || numa::io::previews::usage(&dir)).await else { return };
            row.set_subtitle(&match count {
                0 if numa::io::previews::budget() == 0 => "Per library · off, so none are kept".to_string(),
                0 => "Per library · none kept in this one yet".to_string(),
                1 => format!("Per library · {} used here · one photograph", glib::format_size(bytes)),
                _ => format!("Per library · {} used here · {count} photographs", glib::format_size(bytes)),
            });
        }
    ));
}

fn opening_group(dialog: &adw::PreferencesDialog) -> adw::PreferencesGroup {
    let opening = adw::PreferencesGroup::new();
    opening.set_title("Opening Photographs");
    if sandboxed() {

        opening.set_description(Some(
            "In the file manager, right-click a photograph, choose Open With, pick Numa and \
             switch on Always use for this file type.",
        ));
    } else {
        let default = adw::SwitchRow::new();
        default.set_title("Open photographs with Numa");
        default.set_subtitle(
            "RAW files, HEIF, JPEG, PNG, TIFF, WebP and BMP, double-clicked in the file manager. \
             They open in the loupe, with the folder they came from behind it.",
        );
        default.set_active(opens_photographs());
        default.connect_active_notify(glib::clone!(
            #[weak] dialog,
            move |row| {

                if row.is_active() == opens_photographs() {
                    return;
                }
                if let Err(err) = set_opens_photographs(row.is_active()) {
                    dialog.add_toast(adw::Toast::new(&err));
                    row.set_active(!row.is_active());
                }
            }
        ));
        opening.add(&default);
    }
    opening
}

fn colour_group(state: &App) -> adw::PreferencesGroup {
    let colour = adw::PreferencesGroup::new();
    colour.set_title("Colour");
    let display = adw::ActionRow::new();
    display.set_title("Display");
    display.set_subtitle(&crate::ui::display::described());
    display.set_subtitle_selectable(true);
    colour.add(&display);
    colour.add(&automatic_profile_row(state));
    colour
}

const AUTOMATIC_PROFILE: &str = "automatic-camera-profile";
const AUTOMATIC_CHOICES: [(dcp::Automatic, &str, &str); 3] = [
    (dcp::Automatic::Numa, "numa", "Numa's own"),
    (dcp::Automatic::Standard, "standard", "RawTherapee or Adobe"),
    (dcp::Automatic::Matrix, "matrix", "Camera matrix"),
];

pub(super) fn start_automatic_profile(catalog: &Catalog) {
    let saved = catalog.setting(AUTOMATIC_PROFILE);
    if let Some((choice, ..)) = AUTOMATIC_CHOICES.iter().find(|(_, name, _)| Some(*name) == saved.as_deref()) {
        dcp::set_automatic(*choice);
    }
}

fn automatic_profile_row(state: &App) -> adw::ComboRow {
    let row = adw::ComboRow::new();
    row.set_title("Camera profile");

    row.set_subtitle("What Automatic uses on every photograph");
    row.set_model(Some(&gtk::StringList::new(&AUTOMATIC_CHOICES.map(|(_, _, label)| label))));
    let now = AUTOMATIC_CHOICES.iter().position(|(choice, ..)| *choice == dcp::automatic());
    row.set_selected(now.unwrap_or(0) as u32);
    row.connect_selected_notify(glib::clone!(
        #[strong] state,
        move |row| {
            let Some((choice, name, _)) = AUTOMATIC_CHOICES.get(row.selected() as usize) else { return };
            if *choice == dcp::automatic() {
                return;
            }
            dcp::set_automatic(*choice);
            let _ = state.catalog.set_setting(AUTOMATIC_PROFILE, name);

            prefetch::forget(&state);
            let on_automatic = state.open.borrow().as_ref().is_some_and(|photo| photo.document.colour_profile.is_none());
            if let Some(id) = open_id(&state).filter(|_| on_automatic) {
                open_photo(&state, id);
            }
            reload_grid(&state);
        }
    ));
    row
}

fn show_size(row: &adw::ActionRow, dir: PathBuf, others: Vec<PathBuf>) {
    let shown = dir.display().to_string();
    glib::spawn_future_local(glib::clone!(
        #[weak] row,
        async move {
            let Ok(bytes) = gio::spawn_blocking(move || folder_size(&dir, &others)).await else { return };
            row.set_subtitle(&format!("{shown} · {}", glib::format_size(bytes)));
        }
    ));
}

fn clear_button(row: &adw::ActionRow, dir: PathBuf) -> gtk::Button {
    let clear = gtk::Button::with_label("Clear");
    clear.set_valign(gtk::Align::Center);
    clear.set_tooltip_text(Some("Delete these; they are made again when needed"));
    clear.connect_clicked(glib::clone!(
        #[weak] row,
        move |button| {
            button.set_sensitive(false);
            let dir = dir.clone();
            glib::spawn_future_local(glib::clone!(
                #[weak] row,
                #[weak] button,
                async move {
                    let target = dir.clone();
                    let _ = gio::spawn_blocking(move || {
                        for entry in std::fs::read_dir(&target).into_iter().flatten().flatten() {
                            let _ = std::fs::remove_file(entry.path());
                        }
                    })
                    .await;
                    show_size(&row, dir, Vec::new());
                    button.set_sensitive(true);
                }
            ));
        }
    ));
    clear
}

pub(super) fn folder_size(dir: &std::path::Path, skip: &[PathBuf]) -> u64 {
    let mut total = 0;
    let mut pending = vec![dir.to_path_buf()];
    while let Some(dir) = pending.pop() {
        for entry in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
            match entry.metadata() {
                Ok(meta) if meta.is_dir() => {
                    if !skip.contains(&entry.path()) {
                        pending.push(entry.path());
                    }
                }
                Ok(meta) => total += meta.len(),
                Err(_) => {}
            }
        }
    }
    total
}

pub(super) fn shortcuts_dialog(window: &adw::ApplicationWindow) {
    let dialog = adw::Dialog::new();
    dialog.set_title("Keyboard Shortcuts");

    dialog.set_content_width(600);
    dialog.set_content_height(560);

    let page = adw::PreferencesPage::new();

    let row = |title: &str, keys: &str| {
        let row = adw::ActionRow::new();
        row.set_title(title);
        let label = gtk::Label::new(Some(keys));
        label.add_css_class("dim-label");
        row.add_suffix(&label);
        row
    };

    let app_group = adw::PreferencesGroup::new();
    app_group.set_title("Application");
    app_group.add(&row("Quit", "Ctrl+Q"));
    app_group.add(&row("Keyboard shortcuts", "Ctrl+? / Ctrl+/"));
    app_group.add(&row("Preferences", "Ctrl+,"));
    app_group.add(&row("Open a photograph", "Ctrl+O"));
    page.add(&app_group);

    let library_group = adw::PreferencesGroup::new();
    library_group.set_title("Library");
    library_group.add(&row("Look at one photograph, and put it away again", "Space"));
    library_group.add(&row("Previous / next photograph in the loupe", "Left / Right"));
    library_group.add(&row("Open the photograph in the loupe in the editor", "Enter"));
    library_group.add(&row("Pick in the loupe and go on to the next", "Up"));
    library_group.add(&row("Reject in the loupe and go on to the next", "Down"));
    library_group.add(&row("Take back the last mark given in the loupe", "Backspace"));
    library_group.add(&row("Previous / next burst, on its best frame", "Ctrl+Left / Ctrl+Right"));
    library_group.add(&row("Reject the rest of the burst and go on to the next", "Shift+Down"));
    library_group.add(&row("The neighbours beside the photograph in the loupe, on or off", "F"));
    library_group.add(&row("Review the shoot at full size, and stop", "Shift+Space"));
    library_group.add(&row("Choose frames on the tape up to the one clicked", "Shift+click"));
    library_group.add(&row("Let the frames chosen on the tape go", "Escape"));
    library_group.add(&row("Compare the selection side by side", "C"));
    library_group.add(&row("Select all", "Ctrl+A"));
    library_group.add(&row("Clear the selection", "Ctrl+Shift+A"));
    library_group.add(&row("Rescan the library", "F5"));
    library_group.add(&row("Search the library by what is in the photographs", "Ctrl+F"));
    library_group.add(&row("Rate the selection 0–5 stars", "0–5"));
    library_group.add(&row("Flag the selection picked, again to clear", "P"));
    library_group.add(&row("Flag the selection rejected, again to clear", "X"));
    library_group.add(&row("Clear the selection's flag", "U"));
    library_group.add(&row("Paste copied edits onto the selection", "Ctrl+V"));
    library_group.add(&row("Move the selection to the trash, after asking", "Delete"));
    page.add(&library_group);

    let rapid_group = adw::PreferencesGroup::new();
    rapid_group.set_title("Moments");
    rapid_group.add(&row("Pick, reject or clear the photograph and go on", "P / X / U"));
    rapid_group.add(&row("Look at it in the loupe, a stack side by side", "Space"));
    rapid_group.add(&row("A stack — a burst — side by side", "Enter or a click"));
    rapid_group.add(&row("Start a new moment here", "S"));
    rapid_group.add(&row("Merge the moment with the one before", "M"));
    rapid_group.add(&row("The next moment not yet seen", "Page Down"));
    rapid_group.add(&row("Where the photographs clip, on or off", "J"));
    rapid_group.add(&row("A note in your own words", "N"));
    rapid_group.add(&row("In First Look, into the teaser or out", "P"));
    page.add(&rapid_group);

    let editor_group = adw::PreferencesGroup::new();
    editor_group.set_title("Editor");
    editor_group.add(&row("Previous photo in the filmstrip", "Left / Page Up"));
    editor_group.add(&row("Next photo in the filmstrip", "Right / Page Down"));
    editor_group.add(&row("Rate the open photo 0–5 stars", "0–5"));
    editor_group.add(&row("Flag the open photo picked, again to clear", "P"));
    editor_group.add(&row("Flag the open photo rejected, again to clear", "X"));
    editor_group.add(&row("Clear the open photo's flag", "U"));
    editor_group.add(&row("Hold to compare against the as-shot original", "Space"));
    editor_group.add(&row("Auto: level the photograph and set its light", "A"));
    editor_group.add(&row("Hold to compare against the photograph before Auto, while its card is up", "Shift+Space"));
    editor_group.add(&row("Show what the camera recorded", "I"));
    editor_group.add(&row("Guides: none, thirds, grid", "G"));
    editor_group.add(&row("Panel tabs, in order", "Alt+1 – Alt+9"));
    editor_group.add(&row("Copy this photograph's edits", "Ctrl+C"));
    editor_group.add(&row("Copy the edited picture", "Ctrl+Shift+C"));
    editor_group.add(&row("Paste edits onto this photograph", "Ctrl+V"));
    editor_group.add(&row("Leave the mask", "Esc"));
    editor_group.add(&row("Undo", "Ctrl+Z"));
    editor_group.add(&row("Redo", "Ctrl+Shift+Z"));
    page.add(&editor_group);

    let bar = adw::ToolbarView::new();
    bar.add_top_bar(&adw::HeaderBar::new());
    bar.set_content(Some(&page));
    dialog.set_child(Some(&bar));
    dialog.present(Some(window));
}
