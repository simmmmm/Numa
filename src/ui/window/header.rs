use super::*;

pub(super) fn build_header(state: &App, window: &adw::ApplicationWindow) -> adw::HeaderBar {
    let header = adw::HeaderBar::new();

    let start = build_header_start(state, window);
    header.pack_start(&start);

    places::centre(state, &header);
    state.libraries.picker.connect_selected_notify(glib::clone!(
        #[strong] state,
        move |picker| {
            if state.libraries.switching.get() {
                return;
            }
            let Some(place) = state.libraries.places.borrow().get(picker.selected() as usize).cloned() else {
                return;
            };
            choose_place(&state, place);
        }
    ));
    let title = build_header_title(state);
    header.set_title_widget(Some(&title));

    state.stack.connect_visible_child_name_notify(move |stack| {
        if let Some(name) = stack.visible_child_name() {
            title.set_visible_child_name(&name);
            start.set_visible_child_name(&name);
        }
    });

    install_header_actions(state, window);

    let menu_button = build_header_menu();
    header.pack_end(&menu_button);

    header
}

fn build_header_start(state: &App, window: &adw::ApplicationWindow) -> gtk::Stack {
    let add = gtk::Button::from_icon_name("folder-open-symbolic");
    add.set_tooltip_text(Some("Add folder to library"));
    add.connect_clicked(glib::clone!(
        #[strong] state,
        #[weak] window,
        move |_| add_library_dialog(&state, &window)
    ));

    let rescan = gtk::Button::from_icon_name("view-refresh-symbolic");
    rescan.set_tooltip_text(Some("Rescan this folder"));
    rescan.connect_clicked(glib::clone!(
        #[strong] state,
        move |_| {
            let Some(library) = state.libraries.current.borrow().clone() else {
                state.toast("No library selected — pick one to rescan");
                return;
            };
            if state.libraries.filter.borrow().spans_libraries() {
                rescan_everywhere(&state);
                return;
            }
            match state.catalog.sync_library(&library) {
                Ok(added) => {
                    reload_grid(&state);
                    state.toast(&format!("Rescanned: {added} new photo(s)"));
                }
                Err(err) => state.toast(&format!("Rescan failed: {err}")),
            }
        }
    ));

    let library_actions = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    library_actions.append(&add);

    let import = gtk::Button::from_icon_name("camera-photo-symbolic");
    import.set_tooltip_text(Some("Import from a card or camera"));
    import.connect_clicked(glib::clone!(
        #[strong] state,
        #[weak] window,
        move |_| import_dialog(&state, &window, None)
    ));
    library_actions.append(&import);

    state.libraries.picker.bind_property("visible", &rescan, "visible").sync_create().build();
    library_actions.append(&rescan);

    let back = gtk::Button::from_icon_name("go-previous-symbolic");
    back.set_tooltip_text(Some("Back to the library"));
    back.set_valign(gtk::Align::Center);
    back.connect_clicked(glib::clone!(
        #[strong] state,
        move |_| close_editor(&state)
    ));

    let folder_actions = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    for (icon, tip, import) in [("folder-open-symbolic", "Add folder to library", false), ("camera-photo-symbolic", "Import from a card or camera", true)] {
        let button = gtk::Button::from_icon_name(icon);
        button.set_tooltip_text(Some(tip));
        button.connect_clicked(glib::clone!(
            #[strong] state,
            #[weak] window,
            move |_| match import {
                true => import_dialog(&state, &window, None),
                false => add_library_dialog(&state, &window),
            }
        ));
        folder_actions.append(&button);
    }

    let start = gtk::Stack::new();
    start.add_named(&library_actions, Some("library"));
    start.add_named(&folder_actions, Some("folders"));
    start.add_named(&back, Some("editor"));
    start.set_visible_child_name("library");

    start
}

fn build_header_title(state: &App) -> gtk::Stack {

    let title = gtk::Stack::new();

    let picker_holder = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    let libraries = libraries_crumb(state);
    libraries.add_css_class("dim-label");
    picker_holder.append(&libraries);
    let arrow = crumb_arrow();
    arrow.add_css_class("dim-label");
    picker_holder.append(&arrow);
    picker_holder.append(&state.libraries.picker);
    let mut child = state.libraries.picker.first_child();
    while let Some(widget) = child {
        if let Some(button) = widget.downcast_ref::<gtk::ToggleButton>() {
            button.add_css_class("flat");
            arrow_on_hover(button);
        }
        child = widget.next_sibling();
    }
    title.add_named(&picker_holder, Some("library"));
    title.add_named(&adw::WindowTitle::new("Libraries", ""), Some("folders"));
    title.add_named(&build_crumbs(state), Some("editor"));
    title.set_visible_child_name("library");
    title
}

fn arrow_on_hover(button: &gtk::ToggleButton) {

    let Some(arrow) = button.child().and_then(|inner| inner.last_child()) else {
        return;
    };
    arrow.set_opacity(0.0);
    let shown = |yes: bool| if yes { 1.0 } else { 0.0 };
    let motion = gtk::EventControllerMotion::new();
    motion.connect_contains_pointer_notify(glib::clone!(
        #[weak] arrow,
        #[weak] button,
        move |motion| arrow.set_opacity(shown(motion.contains_pointer() || button.is_active()))
    ));
    button.connect_active_notify(glib::clone!(
        #[weak] arrow,
        #[weak] motion,
        move |button| arrow.set_opacity(shown(motion.contains_pointer() || button.is_active()))
    ));
    button.add_controller(motion);
}

fn install_header_actions(state: &App, window: &adw::ApplicationWindow) {

    let manage = gio::SimpleAction::new("libraries", None);
    manage.connect_activate(glib::clone!(
        #[strong] state,
        #[weak] window,
        move |_, _| libraries_dialog(&state, &window)
    ));
    window.add_action(&manage);
    install_album_actions(state, window);

    let shortcuts = gio::SimpleAction::new("shortcuts", None);
    shortcuts.connect_activate(glib::clone!(
        #[weak] window,
        move |_, _| shortcuts_dialog(&window)
    ));
    window.add_action(&shortcuts);
    if let Some(app) = window.application() {
        app.set_accels_for_action("win.shortcuts", &["<primary>slash"]);
    }

    let preferences = gio::SimpleAction::new("preferences", None);
    preferences.connect_activate(glib::clone!(
        #[strong] state,
        #[weak] window,
        move |_, _| preferences_dialog(&state, &window)
    ));
    window.add_action(&preferences);
    if let Some(app) = window.application() {
        app.set_accels_for_action("win.preferences", &["<primary>comma"]);
    }

    let about = gio::SimpleAction::new("about", None);
    about.connect_activate(glib::clone!(
        #[strong] state,
        #[weak] window,
        move |_, _| show_about(Some(window.upcast_ref()), Some(&state))
    ));
    window.add_action(&about);

    let open_file = gio::SimpleAction::new("open-file", None);
    open_file.connect_activate(glib::clone!(
        #[strong] state,
        #[weak] window,
        move |_, _| {
            let dialog = gtk::FileDialog::new();
            dialog.set_title("Open a photograph");
            let (state, parent) = (state.clone(), window.clone());
            dialog.open(Some(&window), gio::Cancellable::NONE, move |chosen| {
                if let Some(path) = chosen.ok().and_then(|file| file.path()) {
                    open_path(&state, &parent, path);
                }
            });
        }
    ));
    window.add_action(&open_file);
    if let Some(app) = window.application() {
        app.set_accels_for_action("win.open-file", &["<primary>o"]);
    }
    let open_path_action = gio::SimpleAction::new("open-path", Some(&String::static_variant_type()));
    open_path_action.connect_activate(glib::clone!(
        #[strong] state,
        #[weak] window,
        move |_, path| {
            if let Some(path) = path.and_then(|path| path.get::<String>()) {
                open_path(&state, &window, PathBuf::from(path));
            }
        }
    ));
    window.add_action(&open_path_action);
}

fn build_header_menu() -> gtk::MenuButton {
    let menu = gio::Menu::new();
    menu.append(Some("Open…"), Some("win.open-file"));
    menu.append(Some("Libraries…"), Some("win.libraries"));
    menu.append(Some("Albums…"), Some("win.albums"));
    menu.append(Some("Preferences"), Some("win.preferences"));
    menu.append(Some("Keyboard shortcuts"), Some("win.shortcuts"));
    menu.append(Some("About"), Some("win.about"));
    menu.append(Some("Quit"), Some("app.quit"));
    let menu_button = gtk::MenuButton::new();
    menu_button.set_menu_model(Some(&menu));
    menu_button.set_icon_name("open-menu-symbolic");
    menu_button
}

pub fn show_app_about(parent: Option<&gtk::Window>) {
    show_about(parent, None);
}

pub(super) fn show_about(parent: Option<&gtk::Window>, state: Option<&App>) {
    let about = adw::AboutDialog::new();
    about.set_application_name("Numa");

    let dark = format!("{}-dark", crate::APP_ID);
    let themed = gtk::gdk::Display::default().map(|display| gtk::IconTheme::for_display(&display));
    let icon = match &themed {
        Some(theme) if adw::StyleManager::default().is_dark() && theme.has_icon(&dark) => &dark,
        _ => crate::APP_ID,
    };
    about.set_application_icon(icon);
    about.set_developers(&["Tijmen"]);
    about.set_version(env!("CARGO_PKG_VERSION"));

    about.add_legal_section(
        "Camera profiles",
        Some("RawTherapee's DCP profiles, most by Maciej Dworak"),
        gtk::License::Gpl30,
        None,
    );

    about.set_debug_info(&debug_info(state));
    about.set_debug_info_filename("numa-debug-info.txt");
    about.set_issue_url("https://github.com/simmmmm/Numa/issues/new");

    about.present(parent);
}

pub(super) fn debug_info(state: Option<&App>) -> String {
    let mut lines = vec![
        format!("Numa {}", env!("CARGO_PKG_VERSION")),
        format!("GTK {}.{}.{}", gtk::major_version(), gtk::minor_version(), gtk::micro_version()),
        format!("libadwaita {}.{}.{}", adw::major_version(), adw::minor_version(), adw::micro_version()),
        format!(
            "Renderer: {}",
            std::env::var("GSK_RENDERER").unwrap_or_else(|_| "default (GSK_RENDERER unset)".to_string())
        ),
        format!("AppImage: {}", std::env::var("APPIMAGE").unwrap_or_else(|_| "no".to_string())),
        String::new(),
        "Models:".to_string(),
    ];
    for (name, installed) in [
        ("Found masks (EfficientViT)", numa::render::segment::is_installed()),
        ("Click to select (SlimSAM)", numa::render::sam::is_installed()),
        ("Clean edges (IS-Net)", numa::render::matte::is_installed()),
        ("Faces (YuNet)", numa::cull::faces::is_installed()),
        ("People (SFace)", numa::cull::people::is_installed()),
        ("Animals (PP-ResNet50)", numa::render::classify::is_installed()),
    ] {
        lines.push(format!("  {name}: {}", if installed { "installed" } else { "not installed" }));
    }

    if let Some(state) = state {
        if state.libraries.current.borrow().is_some() {
            lines.push(String::new());
            lines.push(format!("Library: {} photo(s)", state.grid.cards.borrow().len()));

            let mut cameras: Vec<String> = state
                .grid.cards
                .borrow()
                .values()
                .take(40)
                .filter_map(|(photo, _)| numa::io::raw::summary(&photo.path)?.camera)
                .collect();
            cameras.sort();
            cameras.dedup();
            lines.push(format!("Cameras (sampled): {}", if cameras.is_empty() { "none read".to_string() } else { cameras.join(", ") }));
        }
    }

    lines.push(String::new());
    lines.push("Logs go to the terminal Numa was started from. For more, start it with".to_string());
    lines.push("RUST_LOG=numa=debug and include that output.".to_string());
    lines.join("\n")
}
