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

    install_header_actions(state, window);

    let (menu_button, main_menu) = build_header_menu();

    let library_menu = library_menu(&main_menu);
    menu_button.set_menu_model(Some(&library_menu));
    cullbar::back_to_grid(state, &menu_button);

    let editor_menu = state.editor_page.photo_menu.clone();
    editor_menu.append_section(None, &main_menu);
    header.insert_action_group("editor", Some(&state.editor_page.actions));
    header.pack_end(&menu_button);
    let end = gtk::Stack::new();
    end.set_transition_type(gtk::StackTransitionType::Crossfade);
    end.set_transition_duration(300);

    end.set_hhomogeneous(false);

    let library_end = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    library_end.append(&state.grid.header_end);
    end.add_named(&library_end, Some("library"));
    end.add_named(&gtk::Box::new(gtk::Orientation::Horizontal, 0), Some("folders"));
    end.add_named(&state.editor_page.header_end, Some("editor"));
    header.pack_end(&end);

    state.stack.connect_visible_child_name_notify(move |stack| {
        if let Some(name) = stack.visible_child_name() {
            title.set_visible_child_name(&name);
            start.set_visible_child_name(&name);
            end.set_visible_child_name(&name);
            menu_button.set_menu_model(Some(match name.as_str() {
                "editor" => &editor_menu,
                "library" => &library_menu,
                _ => &main_menu,
            }));
        }
    });

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

    let search = words::search_button(state);
    library_actions.append(&search);

    let view = rapid::view_button(state);
    view.set_margin_start(6);
    library_actions.append(&view);

    let rating = build_loupe_rating(state);
    library_actions.append(&rating);
    state.loupe.reveal.bind_property("reveal-child", &rating, "visible").sync_create().build();
    for button in [add.upcast_ref::<gtk::Widget>(), search.upcast_ref(), view.upcast_ref()] {
        state.loupe.reveal.bind_property("reveal-child", button, "visible").invert_boolean().sync_create().build();
    }

    let pill = camera_pill();
    library_actions.insert_child_after(&pill, Some(&import));
    let reveal = state.loupe.reveal.clone();
    let one_camera = glib::clone!(
        #[weak] import,
        #[weak] pill,
        move || import.set_visible(!reveal.reveals_child() && !pill.is_visible())
    );
    pill.connect_visible_notify(glib::clone!(
        #[strong] one_camera,
        move |_| one_camera()
    ));
    state.loupe.reveal.connect_reveal_child_notify(glib::clone!(
        #[strong] one_camera,
        move |_| one_camera()
    ));
    one_camera();

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
    start.set_transition_type(gtk::StackTransitionType::Crossfade);
    start.set_transition_duration(300);
    start.add_named(&library_actions, Some("library"));
    start.add_named(&folder_actions, Some("folders"));

    let editor_start = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    editor_start.append(&back);
    editor_start.append(&state.editor_page.header_start);
    start.add_named(&editor_start, Some("editor"));
    start.set_visible_child_name("library");

    start
}

fn build_header_title(state: &App) -> gtk::Stack {

    let title = gtk::Stack::new();
    title.set_transition_type(gtk::StackTransitionType::Crossfade);
    title.set_transition_duration(300);

    let picker_holder = gtk::Box::new(gtk::Orientation::Horizontal, 4);

    let balance = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    picker_holder.append(&balance);
    let libraries = libraries_crumb(state);
    libraries.add_css_class("dim-label");
    picker_holder.append(&libraries);
    let arrow = crumb_arrow();
    arrow.add_css_class("dim-label");

    state.libraries.picker.bind_property("visible", &arrow, "visible").sync_create().build();
    picker_holder.append(&arrow);
    picker_holder.append(&state.libraries.picker);
    let mut child = state.libraries.picker.first_child();
    while let Some(widget) = child {
        if let Some(button) = widget.downcast_ref::<gtk::ToggleButton>() {
            button.add_css_class("flat");
            arrow_on_hover(button);
            balance_picker(button, &balance);
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

fn balance_picker(button: &gtk::ToggleButton, balance: &gtk::Box) {
    button.connect_map(glib::clone!(
        #[weak] balance,
        move |button| {
            let (button, balance) = (button.clone(), balance.clone());
            glib::idle_add_local_once(move || {
                let Some(inner) = button.child() else { return };
                let Some(shown) = inner.last_child().and_then(|arrow| arrow.prev_sibling()) else { return };
                let Some(bounds) = shown.compute_bounds(&inner) else { return };

                let room = inner.width() - (bounds.x() + bounds.width()).round() as i32 - 4;
                if room > 0 {
                    balance.set_width_request(room);
                }
            });
        }
    ));
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

    tonight::install_tonight_action(state, window);
    tracks::install_track_action(state, window);

    let shortcuts = gio::SimpleAction::new("shortcuts", None);
    shortcuts.connect_activate(glib::clone!(
        #[weak] window,
        move |_, _| shortcuts_dialog(&window)
    ));
    window.add_action(&shortcuts);
    if let Some(app) = window.application() {

        app.set_accels_for_action("win.shortcuts", &["<primary>question", "<primary>slash"]);
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
    let feedback = gio::SimpleAction::new("feedback", None);
    feedback.connect_activate(glib::clone!(
        #[strong] state,
        #[weak] window,
        move |_, _| feedback_dialog(&state, &window)
    ));
    window.add_action(&feedback);

    let open_file = gio::SimpleAction::new("open-file", None);
    open_file.connect_activate(glib::clone!(
        #[strong] state,
        #[weak] window,
        move |_, _| {
            let dialog = gtk::FileDialog::new();
            dialog.set_title("Open a Photograph");
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
    install_rescan_action(state, window);
    book::install(state, window);
}

fn install_rescan_action(state: &App, window: &adw::ApplicationWindow) {
    let rescan = gio::SimpleAction::new("rescan", None);
    rescan.connect_activate(glib::clone!(
        #[strong] state,
        move |_, _| {
            let Some(library) = state.libraries.current.borrow().clone() else {
                state.toast("No library selected — pick one to rescan");
                return;
            };
            if state.libraries.filter.borrow().spans_libraries() {
                rescan_everywhere(&state);
                return;
            }

            follow_drive(&state);
            if state.catalog.is_offline(library.id) {
                state.toast(&format!("{} is not connected — a rescan waits until it is back", library.label()));
                return;
            }
            sync_in_background(&state, vec![library], |state, added| {
                reload_grid(state);
                state.toast(&format!("Rescanned: {added} new photo(s)"));
            });
        }
    ));
    window.add_action(&rescan);
    if let Some(app) = window.application() {
        app.set_accels_for_action("win.rescan", &["F5"]);
    }
    install_tethering(state, window);

    let people = gio::SimpleAction::new("people", None);
    people.connect_activate(glib::clone!(
        #[strong] state,
        #[weak] window,
        move |_, _| people_dialog(&state, &window)
    ));
    window.add_action(&people);
    install_clock_action(state, window);
}

fn library_menu(main_menu: &gio::Menu) -> gio::Menu {
    let menu = gio::Menu::new();
    let library = gio::Menu::new();
    library.append(Some("Rescan Library"), Some("win.rescan"));
    library.append(Some("Camera Clocks…"), Some("win.clocks"));
    library.append(Some("Add Places from a Track…"), Some("win.places-from-track"));

    library.append(Some("Paste the Client's Picks…"), Some("win.client-picks"));

    let out = gio::Menu::new();
    out.append(Some("Tonight…"), Some("win.tonight"));
    out.append(Some("For a Book…"), Some("win.book"));
    menu.append_section(None, &library);
    menu.append_section(None, &out);
    menu.append_section(None, main_menu);
    menu
}

fn build_header_menu() -> (gtk::MenuButton, gio::Menu) {
    let menu = gio::Menu::new();
    menu.append(Some("Open…"), Some("win.open-file"));

    let stop = gio::MenuItem::new(Some("Stop Tethering"), Some("win.tether-stop"));
    stop.set_attribute_value("hidden-when", Some(&"action-disabled".to_variant()));
    menu.append_item(&stop);
    menu.append(Some("Libraries…"), Some("win.libraries"));
    menu.append(Some("Albums…"), Some("win.albums"));
    menu.append(Some("Preferences"), Some("win.preferences"));
    menu.append(Some("Keyboard Shortcuts"), Some("win.shortcuts"));
    menu.append(Some("Send Feedback…"), Some("win.feedback"));
    menu.append(Some("About"), Some("win.about"));

    let menu_button = gtk::MenuButton::new();
    menu_button.set_menu_model(Some(&menu));
    menu_button.set_icon_name("open-menu-symbolic");

    menu_button.set_tooltip_text(Some("Main Menu"));
    (menu_button, menu)
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

    add_legal_sections(&about);

    about.add_link("Third-Party Licences", LICENCES_LINK);
    about.add_link("Model Licences", MODELS_LINK);
    about.add_link("Source Code", "https://github.com/simmmmm/Numa");
    about.connect_activate_link(|about, link| {
        let (title, text) = match link {
            LICENCES_LINK => ("Third-Party Licences", include_str!("../../../data/THIRD_PARTY_LICENSES.txt")),
            MODELS_LINK => ("Model Licences", include_str!("../../../data/MODELS-LICENSES.txt")),
            _ => return false,
        };
        show_text(about, title, text);
        true
    });

    about.set_debug_info(&debug_info(state));
    about.set_debug_info_filename("numa-debug-info.txt");
    about.set_issue_url("https://github.com/simmmmm/Numa/issues/new?template=problem.yml");
    about.set_support_url("https://numa.photo/support");

    about.present(parent);
}

fn feedback_dialog(state: &App, window: &adw::ApplicationWindow) {
    let text = gtk::TextView::new();
    text.set_wrap_mode(gtk::WrapMode::WordChar);
    text.set_top_margin(8);
    text.set_bottom_margin(8);
    text.set_left_margin(8);
    text.set_right_margin(8);
    let scroll = gtk::ScrolledWindow::builder()
        .child(&text)
        .min_content_height(140)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .build();
    scroll.add_css_class("card");
    let debug = gtk::CheckButton::with_label("Include Debug Information");
    let content = gtk::Box::new(gtk::Orientation::Vertical, 12);
    content.append(&scroll);
    content.append(&debug);

    let ask = adw::AlertDialog::new(
        Some("Send Feedback"),
        Some("An idea or a tip. On GitHub others can read and add to it; a mail goes to one person."),
    );
    ask.set_extra_child(Some(&content));
    ask.add_response("cancel", "Cancel");
    ask.add_response("mail", "Send by Mail");
    ask.add_response("github", "Post on GitHub");
    ask.set_response_appearance("github", adw::ResponseAppearance::Suggested);
    ask.set_close_response("cancel");
    for response in ["mail", "github"] {
        ask.set_response_enabled(response, false);
    }
    text.buffer().connect_changed(glib::clone!(
        #[weak] ask,
        move |buffer| {
            let written = !buffer.text(&buffer.start_iter(), &buffer.end_iter(), false).trim().is_empty();
            for response in ["mail", "github"] {
                ask.set_response_enabled(response, written);
            }
        }
    ));
    let (state, parent, typed) = (state.clone(), window.clone(), text.clone());
    ask.connect_response(None, move |_, response| {
        if response == "cancel" {
            return;
        }
        let buffer = typed.buffer();
        let written = buffer.text(&buffer.start_iter(), &buffer.end_iter(), false);
        let debug = debug.is_active().then(|| debug_info(Some(&state)));
        let url = feedback_url(response == "github", written.trim(), debug.as_deref());
        gtk::UriLauncher::new(&url).launch(Some(&parent), gio::Cancellable::NONE, |_| {});
    });
    ask.present(Some(window));
    text.grab_focus();
}

pub(super) fn feedback_url(github: bool, written: &str, debug: Option<&str>) -> String {
    let escape = |text: &str| glib::Uri::escape_string(text, None, false).to_string();
    let version = env!("CARGO_PKG_VERSION");
    if github {
        let title: String = written.lines().next().unwrap_or_default().chars().take(80).collect();
        let mut body = written.to_string();
        if let Some(debug) = debug {
            body.push_str(&format!("\n\n<details><summary>Debug information</summary>\n\n```\n{debug}\n```\n</details>"));
        }
        format!(
            "https://github.com/simmmmm/Numa/discussions/new?category=ideas&title={}&body={}",
            escape(&title),
            escape(&body)
        )
    } else {
        let mut body = written.to_string();
        if let Some(debug) = debug {
            body.push_str(&format!("\n\n—\n{debug}"));
        }
        format!("mailto:support@numa.photo?subject={}&body={}", escape(&format!("Numa {version}")), escape(&body))
    }
}

pub(super) fn offer_crash_report(state: &App, window: &adw::ApplicationWindow) {
    let Some(report) = crate::crash::last_time() else { return };
    let toast = adw::Toast::new("Numa closed unexpectedly");
    toast.set_button_label(Some("Send Report…"));
    toast.set_timeout(0);
    let (state, window) = (state.clone(), window.clone());
    toast.connect_button_clicked(glib::clone!(
        #[strong] state,
        move |_| crash_dialog(&state, &window, &report)
    ));
    state.toasts.add_toast(toast);
}

fn crash_dialog(state: &App, window: &adw::ApplicationWindow, report: &str) {

    #[cfg(feature = "gpu")]
    let card = numa::gpu::describe(numa::io::raw::card_frugal());
    #[cfg(not(feature = "gpu"))]
    let card: Option<String> = None;
    let card = format!("\nGraphics card: {}\n", card.unwrap_or_else(|| "none in use".to_string()));
    let report = report.replacen('\n', &card, 1);

    let note = gtk::Label::new(Some(
        "This is everything it left behind; nothing leaves unless you send it. \
         Without a GitHub account, copy it into a mail to support@numa.photo.",
    ));
    note.set_wrap(true);
    note.set_xalign(0.0);
    note.set_margin_start(12);
    note.set_margin_end(12);
    note.set_margin_bottom(6);
    let copy = gtk::Button::with_label("Copy");
    let github = primary_button("Open on GitHub");
    let header = adw::HeaderBar::new();
    header.pack_start(&copy);
    header.pack_end(&github);
    let bar = adw::ToolbarView::new();
    bar.add_top_bar(&header);
    bar.add_top_bar(&note);
    bar.set_content(Some(&text_scroll(&report)));
    let dialog = adw::Dialog::new();
    dialog.set_title("Numa Closed Unexpectedly");
    dialog.set_content_width(720);
    dialog.set_content_height(560);
    dialog.set_child(Some(&bar));

    let (state, parent, text) = (state.clone(), window.clone(), report.clone());
    copy.connect_clicked(move |_| {
        parent.clipboard().set_text(&text);
        state.toasts.add_toast(adw::Toast::new("Report copied"));
    });
    let parent = window.clone();
    github.connect_clicked(glib::clone!(
        #[weak] dialog,
        move |_| {
            let (url, cut) = crash_url(&report);
            if cut {
                parent.clipboard().set_text(&report);
            }
            gtk::UriLauncher::new(&url).launch(Some(&parent), gio::Cancellable::NONE, |_| {});
            dialog.close();
        }
    ));
    dialog.present(Some(window));
}

pub(super) fn crash_url(report: &str) -> (String, bool) {
    let escape = |text: &str| glib::Uri::escape_string(text, None, false).to_string();
    let cut = report.chars().count() > 2500;
    let debug = match cut {
        true => report.chars().take(2500).collect::<String>() + "\n… cut here: paste the whole report from the clipboard",
        false => report.to_string(),
    };
    let url = format!(
        "https://github.com/simmmmm/Numa/issues/new?template=problem.yml&title={}&debug={}",
        escape("Numa closed unexpectedly"),
        escape(&debug)
    );
    (url, cut)
}

const LICENCES_LINK: &str = "numa:third-party-licences";
const MODELS_LINK: &str = "numa:model-licences";

fn show_text(over: &adw::AboutDialog, title: &str, text: &str) {
    let bar = adw::ToolbarView::new();
    bar.add_top_bar(&adw::HeaderBar::new());
    bar.set_content(Some(&text_scroll(text)));
    let dialog = adw::Dialog::new();
    dialog.set_title(title);
    dialog.set_content_width(720);
    dialog.set_content_height(640);
    dialog.set_child(Some(&bar));
    dialog.present(Some(over));
}

fn text_scroll(text: &str) -> gtk::ScrolledWindow {
    let view = gtk::TextView::new();
    view.set_editable(false);
    view.set_monospace(true);
    view.set_wrap_mode(gtk::WrapMode::WordChar);
    view.set_left_margin(12);
    view.set_right_margin(12);
    view.set_top_margin(12);
    view.set_bottom_margin(12);
    view.buffer().set_text(text);
    let scroll = gtk::ScrolledWindow::new();
    scroll.set_child(Some(&view));
    scroll
}

fn add_legal_sections(about: &adw::AboutDialog) {
    about.set_copyright("© 2026 Tijmen");
    about.set_license_type(gtk::License::Gpl30);

    about.add_legal_section(
        "rawler",
        Some("Daniel Vogelbacher, Pedro Côrte-Real and the dnglab contributors"),
        gtk::License::Lgpl21Only,
        None,
    );
    about.add_legal_section(
        "LensFun lens database",
        Some("The LensFun community"),
        gtk::License::Custom,
        Some(
            "Creative Commons Attribution-ShareAlike 3.0. \
             <a href=\"https://creativecommons.org/licenses/by-sa/3.0/\">Read the licence</a>",
        ),
    );

    about.add_legal_section("lenscorrect-ofx (MODELS.md)", Some("Murtaza Tunio"), gtk::License::MitX11, None);
    about.add_legal_section("ONNX Runtime", Some("Microsoft Corporation"), gtk::License::MitX11, None);
    about.add_legal_section("ort", Some("pyke.io"), gtk::License::MitX11, None);

    about.add_legal_section(
        "option-ext",
        Some("Simon Ochsenreither"),
        gtk::License::Custom,
        Some("Mozilla Public License 2.0; its source is at <a href=\"https://github.com/soc/option-ext\">github.com/soc/option-ext</a>. <a href=\"https://mozilla.org/MPL/2.0/\">Read the licence</a>"),
    );
    about.add_legal_section(
        "webpki-root-certs",
        Some("The rustls project, from Mozilla's root certificates"),
        gtk::License::Custom,
        Some("Community Data License Agreement – Permissive 2.0. <a href=\"https://cdla.dev/permissive-2-0/\">Read the licence</a>"),
    );
    about.add_legal_section(
        "ONNX Runtime WebGPU plugin",
        Some("Microsoft Corporation · downloaded separately, with its third-party notices"),
        gtk::License::MitX11,
        None,
    );

    about.add_legal_section(
        "Camera profiles",
        Some("RawTherapee's DCP profiles, most by Maciej Dworak · downloaded separately"),
        gtk::License::Gpl30,
        None,
    );

    for (name, description, _) in numa::io::models::MODELS {
        let author = numa::io::models::AUTHORS.iter().find(|(model, _)| *model == name).map_or("", |(_, author)| author);
        let (licence, terms) = match numa::io::models::licence(description) {
            "MIT" => (gtk::License::MitX11, None),
            numa::io::models::NONCOMMERCIAL => (
                gtk::License::Custom,
                Some("For non-commercial use only: its weights or their training data do not allow commercial use. See Model Licences."),
            ),
            _ => (gtk::License::Apache20, None),
        };
        about.add_legal_section(&format!("{name} model"), Some(&format!("{author} · downloaded separately")), licence, terms);
    }
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
        ("Clean edges (BiRefNet)", numa::render::matte::is_installed()),
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
                .filter_map(|photo| numa::io::raw::summary(&photo.path)?.camera)
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
