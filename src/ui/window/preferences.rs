use super::*;

pub(super) fn preferences_dialog(state: &App, window: &adw::ApplicationWindow) {
    let dialog = adw::PreferencesDialog::new();
    let page = adw::PreferencesPage::new();
    page.set_title("Add-ons and storage");
    page.set_icon_name(Some("folder-symbolic"));

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

    page.add(&colour_group());

    let folder_row = |title: &str, dir: PathBuf| {
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
    };

    page.add(&downloads::models_group(state, &dialog, &folder_row));

    page.add(&downloads::profiles_group(&dialog, &folder_row));

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

    page.add(&opening_group(&dialog));

    let storage = adw::PreferencesGroup::new();
    storage.set_title("Storage");
    storage.set_description(Some(
        "Ratings, edits and names are kept in a hidden .numa folder inside each library, \
         so they travel with the photographs. These are Numa's own files on this computer.",
    ));

    for (title, dir) in [
        ("Settings and list of libraries", numa::core::paths::data_dir()),
        ("Models", numa::core::paths::models_dir()),
        ("Presets", numa::io::presets::dir()),
        ("Thumbnails (safe to delete)", numa::io::thumbs::cache_dir()),
    ] {
        let row = folder_row(title, dir.clone());
        let shown = dir.display().to_string();
        glib::spawn_future_local(glib::clone!(
            #[weak] row,
            async move {
                let Ok(bytes) = gio::spawn_blocking(move || folder_size(&dir)).await else { return };
                row.set_subtitle(&format!("{shown} · {}", glib::format_size(bytes)));
            }
        ));
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

    dialog.add(&page);
    dialog.present(Some(window));
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

fn colour_group() -> adw::PreferencesGroup {
    let colour = adw::PreferencesGroup::new();
    colour.set_title("Colour");
    let display = adw::ActionRow::new();
    display.set_title("Display");
    display.set_subtitle(&crate::ui::display::described());
    display.set_subtitle_selectable(true);
    colour.add(&display);
    colour
}

pub(super) fn folder_size(dir: &std::path::Path) -> u64 {
    let mut total = 0;
    let mut pending = vec![dir.to_path_buf()];
    while let Some(dir) = pending.pop() {
        for entry in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
            match entry.metadata() {
                Ok(meta) if meta.is_dir() => pending.push(entry.path()),
                Ok(meta) => total += meta.len(),
                Err(_) => {}
            }
        }
    }
    total
}

pub(super) fn shortcuts_dialog(window: &adw::ApplicationWindow) {
    let dialog = adw::Dialog::new();
    dialog.set_title("Keyboard shortcuts");
    dialog.set_content_width(420);
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
    app_group.add(&row("Keyboard shortcuts", "Ctrl+/"));
    app_group.add(&row("Preferences", "Ctrl+,"));
    page.add(&app_group);

    let library_group = adw::PreferencesGroup::new();
    library_group.set_title("Library");
    library_group.add(&row("Look at one photograph, and put it away again", "Space"));
    library_group.add(&row("Previous / next photograph in the loupe", "Left / Right"));
    library_group.add(&row("Open the photograph in the loupe in the editor", "Enter"));
    library_group.add(&row("Rate the selection 0–5 stars", "0–5"));
    library_group.add(&row("Flag the selection picked", "P"));
    library_group.add(&row("Flag the selection rejected", "X"));
    library_group.add(&row("Clear the selection's flag", "U"));
    library_group.add(&row("Paste copied edits onto the selection", "Ctrl+V"));
    library_group.add(&row("Move the selection to the trash, after asking", "Delete"));
    page.add(&library_group);

    let editor_group = adw::PreferencesGroup::new();
    editor_group.set_title("Editor");
    editor_group.add(&row("Previous photo in the filmstrip", "Left / Page Up"));
    editor_group.add(&row("Next photo in the filmstrip", "Right / Page Down"));
    editor_group.add(&row("Rate the open photo 0–5 stars", "0–5"));
    editor_group.add(&row("Flag the open photo picked", "P"));
    editor_group.add(&row("Flag the open photo rejected", "X"));
    editor_group.add(&row("Clear the open photo's flag", "U"));
    editor_group.add(&row("Hold to compare against the as-shot original", "Space"));
    editor_group.add(&row("Show what the camera recorded", "I"));
    editor_group.add(&row("Guides: none, thirds, grid", "G"));
    editor_group.add(&row("Panel tabs, in order", "Alt+1 – Alt+9"));
    editor_group.add(&row("Copy this photograph's edits", "Ctrl+C"));
    editor_group.add(&row("Copy the edited picture", "Ctrl+Shift+C"));
    editor_group.add(&row("Paste edits onto this photograph", "Ctrl+V"));
    editor_group.add(&row("Undo", "Ctrl+Z"));
    editor_group.add(&row("Redo", "Ctrl+Shift+Z"));
    page.add(&editor_group);

    let bar = adw::ToolbarView::new();
    bar.add_top_bar(&adw::HeaderBar::new());
    bar.set_content(Some(&page));
    dialog.set_child(Some(&bar));
    dialog.present(Some(window));
}
