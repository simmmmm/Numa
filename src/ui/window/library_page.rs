use super::*;

fn file_drop(state: &App) -> gtk::DropTarget {
    let drop = gtk::DropTarget::new(gtk::gdk::FileList::static_type(), gtk::gdk::DragAction::COPY);
    drop.connect_drop(glib::clone!(
        #[strong] state,
        move |_, value, _, _| {
            let Ok(files) = value.get::<gtk::gdk::FileList>() else { return false };
            let dropped: Vec<PathBuf> = files.files().iter().filter_map(|file| file.path()).collect();
            let Some(library) = state.libraries.current.borrow().clone() else { return false };
            if dropped.is_empty() {
                return false;
            }

            if state.libraries.filter.borrow().spans_libraries() {
                state.toast("Pick a library to copy these into");
                return false;
            }
            copy_into_library(&state, library, dropped);
            true
        }
    ));
    drop
}

pub(super) fn build_library_page(state: &App, window: &adw::ApplicationWindow) -> gtk::Box {
    let page = gtk::Box::new(gtk::Orientation::Vertical, 0);

    words::build_search_bar(state);
    words::install_search_action(state, window);
    install_place_back(state, window);

    let head = build_head(state);
    state.grid.welcome.bind_property("visible", &head, "visible").invert_boolean().sync_create().build();

    state.libraries.head_slot.append(&head);
    let bar = build_filter_bar(state, window);

    state.grid.welcome.bind_property("visible", &bar, "visible").invert_boolean().sync_create().build();
    page.append(&bar);
    let keys = "hint-library-keys";
    let hint = key_hint(
        state,
        keys,
        "Cull from the keyboard: 0–5 rate, P picks, X rejects, U clears — Ctrl+/ lists every shortcut",
    );

    let offer = &state.grid.extras_banner;
    offer.connect_revealed_notify(glib::clone!(
        #[strong] state,
        #[weak] hint,
        move |offer| hint.set_revealed(!offer.is_revealed() && first_time(&state, keys))
    ));
    offer.bind_property("revealed", &state.grid.update_banner, "visible").invert_boolean().sync_create().build();
    state.grid.welcome.bind_property("visible", &hint, "visible").invert_boolean().sync_create().build();
    page.append(&hint);

    state.loupe.reveal.connect_reveal_child_notify(glib::clone!(
        #[strong] state,
        #[weak] bar,
        #[weak] hint,
        #[weak] head,
        move |reveal| {
            let choosing = reveal.reveals_child();
            bar.set_reveal_child(!choosing);
            head.set_visible(!choosing && !state.grid.welcome.is_visible());
            hint.set_visible(!choosing && !state.grid.welcome.is_visible());

            state.grid.header_end.set_visible(!choosing && !state.grid.welcome.is_visible());
        }
    ));
    page.append(&state.grid.offline);

    state.grid.clock_offer.set_transition_type(gtk::RevealerTransitionType::SlideDown);
    state.grid.clock_offer.set_transition_duration(200);
    page.append(&state.grid.clock_offer);

    state.grid.empty.set_vexpand(true);
    page.append(&state.grid.empty);

    let welcome = state.grid.welcome.clone();
    welcome.set_vexpand(true);
    welcome.set_visible(false);
    no_libraries_yet(state, &welcome);
    page.append(&welcome);

    let scroller = build_grid_scroller(state);

    page.add_controller(file_drop(state));

    let over = gtk::Overlay::new();

    let row = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    row.append(&scroller);
    row.append(&state.grid.scrubber.area);
    over.set_child(Some(&row));
    over.add_overlay(&state.grid.scrubber.pill);
    scrubber::install(state);
    over.add_overlay(&build_loupe(state));

    over.add_overlay(&build_compare(state));

    let bar = cullbar::build(state);
    bar.add_css_class("floating-bar");
    bar.set_halign(gtk::Align::Center);
    bar.set_valign(gtk::Align::End);
    bar.set_margin_bottom(16);
    over.add_overlay(&bar);
    page.append(&over);

    let loupe_keys = gtk::EventControllerKey::new();
    loupe_keys.set_propagation_phase(gtk::PropagationPhase::Capture);
    loupe_keys.connect_key_pressed(glib::clone!(
        #[strong] state,
        move |_, key, _, modifiers| loupe_key(&state, key, modifiers)
    ));
    window.add_controller(loupe_keys);

    let compare_keys = gtk::EventControllerKey::new();
    compare_keys.set_propagation_phase(gtk::PropagationPhase::Capture);
    compare_keys.connect_key_pressed(glib::clone!(
        #[strong] state,
        move |_, key, _, _| compare_key(&state, key)
    ));
    window.add_controller(compare_keys);

    state.grid.wall.connect_card_activated(glib::clone!(
        #[strong] state,
        move |_, index| {
            if let Some(id) = id_at(&state, index) {
                open_photo(&state, id);
            }
        }
    ));
    install_photo_menu(state, window);

    page
}

#[derive(Clone)]
pub(super) struct Head {
    back: gtk::Button,
    title: gtk::Label,
    caption: gtk::Label,

    tokens: gtk::Label,

    more: gtk::MenuButton,

    kinds: gtk::Box,
    book: gtk::Button,
    rename: gtk::Button,
    files: gtk::Button,
    stop: gtk::Button,
    tidy: gtk::Button,

    sync: gtk::Button,

    pencil: gtk::Button,
    edit: gtk::Entry,
    titles_stack: gtk::Stack,
    editing: Rc<Cell<bool>>,

    owner: Rc<Cell<&'static str>>,
}

impl Head {
    pub(super) fn new() -> Self {
        let back = gtk::Button::from_icon_name("go-previous-symbolic");
        back.add_css_class("flat");
        back.add_css_class("circular");
        back.set_valign(gtk::Align::Center);
        let title = gtk::Label::new(None);
        title.set_xalign(0.0);
        title.add_css_class("heading");
        title.set_ellipsize(gtk::pango::EllipsizeMode::End);
        let caption = gtk::Label::new(None);
        caption.set_xalign(0.0);
        caption.add_css_class("caption");
        caption.add_css_class("dim-label");
        caption.add_css_class("numeric");
        let tokens = gtk::Label::new(None);
        tokens.set_xalign(0.0);
        tokens.add_css_class("section-header");
        tokens.set_ellipsize(gtk::pango::EllipsizeMode::End);
        let item = |label: &str| {
            let button = gtk::Button::with_label(label);
            button.add_css_class("flat");
            if let Some(child) = button.child().and_downcast::<gtk::Label>() {
                child.set_xalign(0.0);
            }
            button
        };
        let book = item("For a Book…");
        book.set_action_name(Some("win.book"));
        book.set_tooltip_text(Some("The picks here, as a photo book's chapter folders"));
        let rename = item("Rename…");
        let files = item("Show in Files");
        let stop = item("Stop Syncing…");
        let tidy = item("Tidy Up");
        tidy.set_tooltip_text(Some("Copies, runs of the same frame and photographs past use, in this library"));
        let sync = gtk::Button::new();
        sync.set_valign(gtk::Align::Center);

        sync.add_css_class("flat");
        sync.add_css_class("circular");
        let more = gtk::MenuButton::new();
        more.set_icon_name("view-more-symbolic");
        more.set_valign(gtk::Align::Center);
        more.set_tooltip_text(Some("More"));
        let kinds = gtk::Box::new(gtk::Orientation::Vertical, 0);
        for (label, action) in [("Add a Chapter…", "album.add-chapter"), ("Make a Trip", "album.make-trip"), ("Make a Wedding", "album.make-wedding")] {
            let button = item(label);
            button.set_action_name(Some(action));
            button.connect_clicked(glib::clone!(
                #[weak] more,
                move |_| more.popdown()
            ));
            kinds.append(&button);
        }
        kinds.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
        let pencil = gtk::Button::from_icon_name("document-edit-symbolic");
        pencil.add_css_class("flat");
        pencil.add_css_class("circular");

        pencil.add_css_class("head-small");
        pencil.set_valign(gtk::Align::Center);
        pencil.set_opacity(0.0);
        pencil.set_tooltip_text(Some("Rename"));
        let edit = gtk::Entry::new();
        edit.add_css_class("heading");
        edit.set_valign(gtk::Align::Center);
        let titles_stack = gtk::Stack::new();
        titles_stack.set_halign(gtk::Align::Start);

        titles_stack.set_vhomogeneous(false);
        let editing = Rc::new(Cell::new(false));
        Self { back, title, caption, tokens, more, kinds, book, rename, files, stop, tidy, sync, pencil, edit, titles_stack, editing, owner: Rc::new(Cell::new("libraries")) }
    }
}

fn build_head(state: &App) -> gtk::Box {
    let head = &state.libraries.head;

    head.back.connect_clicked(glib::clone!(
        #[strong] state,
        move |_| match state.libraries.head.owner.get() {
            "albums" => show_albums(&state),
            "people" => show_people(&state),
            _ => show_folders(&state),
        }
    ));

    head.titles_stack.add_named(&head.title, Some("title"));
    head.titles_stack.add_named(&head.edit, Some("edit"));
    head.titles_stack.set_visible_child_name("title");
    let name_line = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    name_line.append(&head.titles_stack);
    name_line.append(&head.pencil);
    let hover = gtk::EventControllerMotion::new();
    hover.connect_enter(glib::clone!(
        #[weak(rename_to = pencil)] head.pencil,
        move |_, _, _| pencil.set_opacity(1.0)
    ));
    hover.connect_leave(glib::clone!(
        #[weak(rename_to = pencil)] head.pencil,
        move |_| pencil.set_opacity(0.0)
    ));
    name_line.add_controller(hover);
    let twice = gtk::GestureClick::new();
    twice.connect_pressed(glib::clone!(
        #[strong] state,
        move |_, presses, _, _| {
            if presses == 2 {
                start_rename(&state);
            }
        }
    ));
    head.title.add_controller(twice);
    head.pencil.connect_clicked(glib::clone!(
        #[strong] state,
        move |_| start_rename(&state)
    ));
    head.edit.connect_activate(glib::clone!(
        #[strong] state,
        move |_| finish_rename(&state, true)
    ));
    let keys = gtk::EventControllerKey::new();
    keys.connect_key_pressed(glib::clone!(
        #[strong] state,
        move |_, key, _, _| match key {
            gtk::gdk::Key::Escape => {
                finish_rename(&state, false);
                glib::Propagation::Stop
            }
            _ => glib::Propagation::Proceed,
        }
    ));
    head.edit.add_controller(keys);
    let leaving = gtk::EventControllerFocus::new();
    leaving.connect_leave(glib::clone!(
        #[strong] state,
        move |_| finish_rename(&state, true)
    ));
    head.edit.add_controller(leaving);

    let titles = gtk::Box::new(gtk::Orientation::Vertical, 0);
    titles.set_hexpand(true);
    titles.set_valign(gtk::Align::Center);
    titles.append(&name_line);
    head.caption.set_ellipsize(gtk::pango::EllipsizeMode::End);
    head.caption.set_xalign(0.0);
    titles.append(&head.caption);
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    row.add_css_class("place-head");
    row.set_hexpand(true);
    row.append(&titles);
    head.tidy.connect_clicked(glib::clone!(
        #[strong] state,
        move |_| {
            if let Place::Library(library) = current_place(&state) {
                tidy::show_library(&state, library.id);
            }
        }
    ));
    head.sync.connect_clicked(glib::clone!(
        #[strong] state,
        move |_| {
            if let Place::Library(library) = current_place(&state) {
                home_pair::run(&state, &library);
            }
        }
    ));
    row.append(&head.sync);

    let list = gtk::Box::new(gtk::Orientation::Vertical, 0);
    list.append(&head.kinds);
    for button in [&head.tidy, &head.book, &head.rename, &head.files, &head.stop] {
        list.append(button);
        button.connect_clicked(glib::clone!(
            #[weak(rename_to = more)] head.more,
            move |_| more.popdown()
        ));
    }
    let popover = gtk::Popover::new();
    popover.set_child(Some(&list));
    head.more.set_popover(Some(&popover));
    head.rename.connect_clicked(glib::clone!(
        #[strong] state,
        move |_| start_rename(&state)
    ));
    head.files.connect_clicked(glib::clone!(
        #[strong] state,
        move |_| {
            if let Place::Library(library) = current_place(&state) {
                gtk::FileLauncher::new(Some(&gio::File::for_path(&library.path))).launch(
                    state.stack.root().and_downcast::<gtk::Window>().as_ref(),
                    None::<&gio::Cancellable>,
                    |result| {
                        if let Err(err) = result {
                            log::warn!("could not open the folder: {err}");
                        }
                    },
                );
            }
        }
    ));
    head.stop.connect_clicked(glib::clone!(
        #[strong] state,
        move |_| {
            if let Place::Library(library) = current_place(&state) {
                home_pair::run(&state, &library);
            }
        }
    ));
    row.append(&head.more);
    cullbar::keep_focus(row.upcast_ref());
    row
}

fn start_rename(state: &App) {
    let head = &state.libraries.head;
    let Place::Library(library) = current_place(state) else { return };
    if head.editing.get() || home_sync::is_home(state, library.id) {
        return;
    }
    head.editing.set(true);
    head.edit.set_text(&library.label());
    head.titles_stack.set_visible_child_name("edit");
    head.edit.grab_focus();
    head.edit.select_region(0, -1);
}

fn finish_rename(state: &App, save: bool) {
    let head = &state.libraries.head;
    if !head.editing.replace(false) {
        return;
    }
    head.titles_stack.set_visible_child_name("title");
    let name = head.edit.text().trim().to_string();
    let Place::Library(library) = current_place(state) else { return };
    if save && !name.is_empty() && name != library.label() {
        library_bar::rename_now(state, &library, &name);
    }
}

pub(super) fn install_place_back(state: &App, window: &adw::ApplicationWindow) {
    let action = gio::SimpleAction::new("place-back", None);
    action.connect_activate(glib::clone!(
        #[strong] state,
        move |_, _| {
            if state.stack.visible_child_name().as_deref() == Some("library") && !state.libraries.head.owner.get().is_empty() && !state.loupe.reveal.reveals_child() {
                state.libraries.head.back.emit_clicked();
            }
        }
    ));
    window.add_action(&action);
    if let Some(app) = window.application() {
        app.set_accels_for_action("win.place-back", &["<alt>Left"]);
    }
}

pub(super) fn show_head(state: &App) {
    let head = &state.libraries.head;
    let place = current_place(state);
    let read = places::row(state, &place);
    head.title.set_label(&places::name_of(state, &place));
    let searched = filter::searched(state);
    let caption = match places::period(read.first, read.last) {
        _ if searched.is_some() => places::photos(state.grid.wall.len()),
        Some(period) => format!("{period} \u{b7} {}", places::photos(read.count as usize)),
        None => places::photos(read.count as usize),
    };

    let synced = match &place {
        Place::Library(library) if searched.is_none() => home_sync::sync_said(state, library.id).map(|said| (library.id, said)),
        _ => None,
    };
    match synced {
        Some((id, said)) => {
            head.caption.set_label(&format!("{caption} \u{b7} {said}"));
            home_sync::follow_caption(id, &head.caption, &format!("{caption} \u{b7} "));
        }
        None => head.caption.set_label(&caption),
    }
    head.tokens.set_visible(searched.is_some());
    head.tokens.set_label(&searched.unwrap_or_default().to_uppercase());
    let (owner, back) = match place {
        Place::Everywhere => ("", ""),
        Place::Library(_) => ("libraries", "Libraries"),
        Place::Album(_) => ("albums", "Albums"),
        Place::Person(_) => ("people", "People"),
    };
    head.owner.set(owner);
    head.back.set_tooltip_text(Some(back));
    let book = matches!(place, Place::Library(_) | Place::Album(_));
    head.book.set_visible(book);

    head.kinds.set_visible(matches!(&place, Place::Album(key) if !album_page::has_page(state, key)));
    let here = matches!(&place, Place::Library(library) if !home_sync::is_home(state, library.id));
    head.tidy.set_visible(here);
    head.rename.set_visible(here);
    head.pencil.set_visible(here);

    head.editing.set(false);
    head.titles_stack.set_visible_child_name("title");
    let commands = match &place {
        Place::Library(library) => Some(home_pair::commands(state, library)),
        _ => None,
    };

    let offer = commands.filter(|commands| commands.here || commands.home);
    let said = offer.and_then(home_pair::label);
    head.sync.set_icon_name(match offer {
        Some(home_pair::Commands { here: true, .. }) => "folder-download-symbolic",
        _ => "network-server-symbolic",
    });
    head.sync.set_tooltip_text(said);
    head.sync.update_property(&[gtk::accessible::Property::Label(said.unwrap_or_default())]);
    head.sync.set_visible(said.is_some());
    let stop = commands.is_some_and(|commands| commands.stop && !commands.here && !commands.home);
    head.stop.set_visible(stop);
    let on_disk = matches!(&place, Place::Library(library) if !home_sync::is_home(state, library.id));
    head.files.set_visible(on_disk);
    head.more.set_visible(book || here || stop || on_disk);

    head.tokens.set_visible(false);
}

pub(super) fn mark_texture() -> Option<gtk::gdk::Texture> {
    let image: &'static [u8] = match adw::StyleManager::default().is_dark() {
        true => include_bytes!("../../../data/branding/mark.png"),
        false => include_bytes!("../../../data/branding/icon-light@2x.png"),
    };
    gtk::gdk::Texture::from_bytes(&glib::Bytes::from_static(image)).ok()
}

pub(super) fn no_libraries_yet(state: &App, page: &adw::StatusPage) {
    page.set_paintable(mark_texture().as_ref());
    page.set_title("No Libraries Yet");
    page.set_description(Some("Add a folder of photographs to start. Numa never moves or changes them."));
    let add = primary_button("Add Folder…");
    add.add_css_class("pill");
    let import = gtk::Button::with_label("Import from Card…");
    import.add_css_class("flat");
    for (button, card) in [(&add, false), (&import, true)] {
        button.set_halign(gtk::Align::Center);
        button.connect_clicked(glib::clone!(
            #[strong] state,
            move |_| {
                let Some(window) = state.stack.root().and_downcast::<adw::ApplicationWindow>() else { return };
                match card {
                    true => import_dialog(&state, &window, None),
                    false => add_library_dialog(&state, &window),
                }
            }
        ));
    }
    let buttons = gtk::Box::new(gtk::Orientation::Vertical, 12);
    buttons.append(&add);
    buttons.append(&import);
    page.set_child(Some(&buttons));
}

fn build_grid_scroller(state: &App) -> gtk::ScrolledWindow {

    state.grid.wall.add_css_class("photo-rows");

    state.grid.wall.set_factory(
        glib::clone!(
            #[strong] state,
            move || make_card(&state)
        ),
        glib::clone!(
            #[strong] state,
            move |card, index| bind_card(&state, card, index)
        ),
    );

    let scroller = state.grid.scroller.clone();
    scroller.add_css_class("numa-content");
    scroller.set_hexpand(true);
    scroller.set_vexpand(true);

    scroller.set_child(Some(&state.grid.wall));

    let adjustment = scroller.vadjustment();
    adjustment.connect_value_changed(glib::clone!(
        #[strong] state,
        move |_| schedule_thumbnails(&state)
    ));
    adjustment.connect_changed(glib::clone!(
        #[strong] state,
        move |_| schedule_thumbnails(&state)
    ));
    scroller.connect_map(glib::clone!(
        #[strong] state,
        move |_| schedule_thumbnails(&state)
    ));
    time_scrolling(&scroller, &adjustment, "grid");
    scroller
}

const OFF_UNTIL_USED: [&str; 2] = ["photo-paste", "photo-paste-choose"];

pub(super) fn debug_assert_missing_actions(menu: &gio::Menu, window: &adw::ApplicationWindow) {
    fn walk(model: &gio::MenuModel, into: &mut Vec<String>) {
        for index in 0..model.n_items() {
            if let Some(action) = model
                .item_attribute_value(index, gio::MENU_ATTRIBUTE_ACTION, None)
                .and_then(|value| value.get::<String>())
            {
                into.push(action);
            }
            for link in [gio::MENU_LINK_SECTION, gio::MENU_LINK_SUBMENU] {
                if let Some(child) = model.item_link(index, link) {
                    walk(&child, into);
                }
            }
        }
    }

    let mut named = Vec::new();
    walk(menu.upcast_ref::<gio::MenuModel>(), &mut named);
    for action in named {
        let Some(bare) = action.strip_prefix("win.") else { continue };
        if !window.has_action(bare) {
            log::error!("menu entry points at win.{bare}, which is not a registered action");
        } else if !window.is_action_enabled(bare) && !OFF_UNTIL_USED.contains(&bare) {

            log::error!("menu entry win.{bare} exists but is disabled");
        }
    }
}

pub(super) fn install_photo_menu(state: &App, window: &adw::ApplicationWindow) {
    let menu = photo_menu_model(state);
    let (edit, rate, flag, delete_photos) = photo_actions(state, window);
    install_canvas_and_mask_actions(state, window);

    let export = gio::SimpleAction::new("photo-export", None);
    export.connect_activate(glib::clone!(
        #[strong] state,
        #[weak] window,
        move |_, _| export_selection(&state, &window)
    ));
    window.add_action(&export);

    let copy = gio::SimpleAction::new("photo-copy", None);
    copy.connect_activate(glib::clone!(
        #[strong] state,
        move |_, _| copy_from_selection(&state)
    ));
    window.add_action(&copy);

    for (paste, ask) in state.copy_paste.paste.iter().zip([false, true]) {
        paste.connect_activate(glib::clone!(
            #[strong] state,
            #[weak] window,
            move |_, _| paste_settings(&state, &window, ask)
        ));
        window.add_action(paste);
    }

    for action in [&edit, &rate, &flag, &delete_photos] {
        window.add_action(action);
    }
    install_preset_actions(state, window);
    install_client_actions(state, window);
    install_copy_action(state, window);

    debug_assert_missing_actions(&menu, window);

    install_photo_menu_popover(state, menu);
}

pub(super) fn photo_menu_model(state: &App) -> gio::Menu {
    let menu = gio::Menu::new();
    menu.append(Some("Edit"), Some("win.photo-edit"));

    let rating = gio::Menu::new();
    for stars in 0..=5i32 {
        let label = match stars {
            0 => "No Rating".to_string(),
            n => "★".repeat(n as usize),
        };
        let item = gio::MenuItem::new(Some(&label), None);
        item.set_action_and_target_value(Some("win.photo-rate"), Some(&stars.to_variant()));
        rating.append_item(&item);
    }
    menu.append_submenu(Some("Rating"), &rating);

    let flags = gio::Menu::new();
    for (label, which) in [("Pick", "pick"), ("Reject", "reject"), ("Clear Flag", "none")] {
        let item = gio::MenuItem::new(Some(label), None);
        item.set_action_and_target_value(Some("win.photo-flag"), Some(&which.to_variant()));
        flags.append_item(&item);
    }
    menu.append_section(None, &flags);

    menu.append_section(Some("Copy"), &copy_menu());

    let settings = gio::Menu::new();
    settings.append(Some("Export…"), Some("win.photo-export"));

    settings.append(Some("Copy File Names"), Some("win.copy-names"));

    settings.append(Some("Copy Settings"), Some("win.photo-copy"));
    settings.append(Some("Paste Settings"), Some("win.photo-paste"));
    settings.append(Some("Choose What to Paste…"), Some("win.photo-paste-choose"));
    settings.append_submenu(Some("Presets"), &state.copy_paste.presets_menu);
    menu.append_section(None, &settings);
    menu.append_section(None, &state.libraries.albums_menu);

    let destructive = gio::Menu::new();
    destructive.append(Some("Move to Trash…"), Some("win.photo-delete"));
    menu.append_section(None, &destructive);
    menu
}

fn photo_actions(
    state: &App,
    window: &adw::ApplicationWindow,
) -> (gio::SimpleAction, gio::SimpleAction, gio::SimpleAction, gio::SimpleAction) {

    let edit = gio::SimpleAction::new("photo-edit", None);
    edit.connect_activate(glib::clone!(
        #[strong] state,
        move |_, _| {
            let Some(id) = selected_ids(&state).first().copied() else { return };
            open_photo(&state, id);
        }
    ));

    let rate = gio::SimpleAction::new("photo-rate", Some(&i32::static_variant_type()));
    rate.connect_activate(glib::clone!(
        #[strong] state,
        move |_, stars| {
            let stars = stars.and_then(|value| value.get::<i32>()).unwrap_or(0);
            rate_here(&state, Action::Rate(stars.clamp(0, 5) as u8));
        }
    ));

    let flag = gio::SimpleAction::new("photo-flag", Some(&String::static_variant_type()));
    flag.connect_activate(glib::clone!(
        #[strong] state,
        move |_, which| {
            let which = which.and_then(|value| value.get::<String>()).unwrap_or_default();
            let flag = match which.as_str() {
                "pick" => Flag::Picked,
                "reject" => Flag::Rejected,
                _ => Flag::None,
            };
            rate_here(&state, Action::Flag(flag));
        }
    ));

    let delete_photos = gio::SimpleAction::new("photo-delete", None);
    delete_photos.connect_activate(glib::clone!(
        #[strong] state,
        #[weak] window,
        move |_, _| confirm_delete(&state, &window)
    ));
    (edit, rate, flag, delete_photos)
}

fn install_canvas_and_mask_actions(state: &App, window: &adw::ApplicationWindow) {

    let zoom_action = gio::SimpleAction::new("zoom", Some(&String::static_variant_type()));
    zoom_action.connect_activate(glib::clone!(
        #[strong] state,
        move |_, level| {
            let target = match level.and_then(|value| value.get::<String>()).as_deref() {
                Some("fit") | None => FIT_ZOOM,
                Some(percent) => match percent.parse::<f64>() {
                    Ok(percent) => percent / 100.0,
                    Err(_) => return,
                },
            };
            set_zoom(&state, target);
        }
    ));
    window.add_action(&zoom_action);

    let add_mask_action = gio::SimpleAction::new("add-mask", Some(&String::static_variant_type()));
    add_mask_action.connect_activate(glib::clone!(
        #[strong] state,
        move |_, kind| {
            let kind = match kind.and_then(|value| value.get::<String>()).as_deref() {
                Some("radial") => MaskKind::Radial,
                Some("brush") => MaskKind::Brush,
                Some("click") => MaskKind::Click,
                Some("colour-range") => MaskKind::ColourRange,
                Some("luminance-range") => MaskKind::LuminanceRange,
                _ => MaskKind::Linear,
            };
            add_mask(&state, kind);
        }
    ));
    window.add_action(&add_mask_action);

    let pick = gio::SimpleAction::new("pick-mask", Some(&i32::static_variant_type()));
    pick.connect_activate(glib::clone!(
        #[strong] state,
        move |_, index| {
            let index = index.and_then(|value| value.get::<i32>()).unwrap_or(-1);
            select_mask(&state, usize::try_from(index).ok());
        }
    ));
    window.add_action(&pick);

    let invert = gio::SimpleAction::new("invert-mask", None);
    invert.connect_activate(glib::clone!(
        #[strong] state,
        move |_, _| {
            let Some(index) = state.mask_overlay.selected_mask.get() else { return };
            let inverted = mask_at(&state, index).is_some_and(|mask| mask.inverted);
            set_mask_inverted(&state, index, !inverted);
        }
    ));
    window.add_action(&invert);

    for (name, hair) in [("refine-edge", false), ("refine-hair", true)] {
        let look = gio::SimpleAction::new(name, None);
        look.connect_activate(glib::clone!(
            #[strong] state,
            move |_, _| {
                let Some(index) = state.mask_overlay.selected_mask.get() else { return };
                match hair {
                    true => trace_hair(&state, index),
                    false => refine_mask_edge(&state, index),
                }
            }
        ));
        window.add_action(&look);
    }

    let duplicate = gio::SimpleAction::new("duplicate-mask", None);
    duplicate.connect_activate(glib::clone!(
        #[strong] state,
        move |_, _| {
            let Some(index) = state.mask_overlay.selected_mask.get() else { return };
            duplicate_mask(&state, index);
        }
    ));
    window.add_action(&duplicate);

    let delete_mask = gio::SimpleAction::new("delete-mask", None);
    delete_mask.connect_activate(glib::clone!(
        #[strong] state,
        move |_, _| {
            let Some(index) = state.mask_overlay.selected_mask.get() else { return };
            remove_mask(&state, index);
        }
    ));
    window.add_action(&delete_mask);
}

fn install_photo_menu_popover(state: &App, menu: gio::Menu) {

    let rows_popover = gtk::PopoverMenu::from_model(Some(&menu));
    rows_popover.set_parent(&state.grid.wall);
    rows_popover.set_has_arrow(false);
    rows_popover.set_halign(gtk::Align::Start);
    let click = gtk::GestureClick::new();
    click.set_button(gtk::gdk::BUTTON_SECONDARY);
    click.connect_pressed(glib::clone!(
        #[strong] state,
        #[weak] rows_popover,
        move |_, _, x, y| {
            let Some(index) = state.grid.wall.index_at_point(x, y) else { return };
            if !state.grid.wall.is_selected(index) {
                state.grid.wall.select_only(index);
            }

            state.export.copy.set_enabled(state.grid.wall.selected().len() == 1);
            rows_popover.set_pointing_to(Some(&gtk::gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
            rows_popover.popup();
        }
    ));

    let copy = state.export.copy.clone();
    rows_popover.connect_closed(move |_| copy.set_enabled(true));
    state.grid.wall.add_controller(click);
}

pub(super) fn selected_ids(state: &App) -> Vec<i64> {
    let lazy = state.grid.lazy.borrow();
    state.grid.wall.selected().into_iter().filter_map(|index| lazy.get(index).map(|card| card.id)).collect()
}

pub(super) fn confirm_delete(state: &App, window: &adw::ApplicationWindow) {
    let selected = selected_ids(state);
    if selected.is_empty() {
        state.toast("Select a photo first");
        return;
    }
    if refused_offline(state, selected.iter().copied(), "moving to the trash") {
        return;
    }

    let doomed: Vec<(i64, PathBuf)> = {
        let cards = state.grid.cards.borrow();
        selected.iter().filter_map(|id| cards.get(id).map(|photo| (*id, photo.path.clone()))).collect()
    };
    if doomed.is_empty() {
        return;
    }

    let title = match doomed.as_slice() {
        [(_, path)] => format!(
            "Move {} to the trash?",
            path.file_name().unwrap_or_default().to_string_lossy()
        ),
        many => format!("Move {} photographs to the trash?", many.len()),
    };

    let synced = doomed.iter().any(|(id, _)| home_sync::is_linked(state, numa::io::catalog::library_of(*id)));
    let dialog = adw::AlertDialog::new(
        Some(&title),
        Some(match synced {
            true => "The file goes to your desktop's trash and Numa Home's to its own, where each can be put back. \
                     Put back on either side, it comes back on both, with its edits.",
            false => "The file goes to your desktop's trash, where it can be put back. \
                      Its rating, flag and edits are forgotten here and those do not come back.",
        }),
    );
    dialog.add_response("cancel", "Cancel");
    dialog.add_response("trash", "Move to Trash");
    dialog.set_response_appearance("trash", adw::ResponseAppearance::Destructive);
    dialog.set_default_response(Some("cancel"));
    dialog.set_close_response("cancel");

    let state = state.clone();
    dialog.connect_response(None, move |dialog, response| {
        dialog.close();
        if response != "trash" {
            return;
        }
        let (state, doomed) = (state.clone(), doomed.clone());
        glib::spawn_future_local(async move {

            let paths: Vec<PathBuf> = doomed.iter().map(|(_, path)| path.clone()).collect();
            let label = match paths.len() {
                1 => "Moving it to the trash…".to_string(),
                many => format!("Moving {many} photographs to the trash…"),
            };
            let Ok(results) = busy(&state, &label, move || {
                paths.iter().map(|path| gio::File::for_path(path).trash(gio::Cancellable::NONE).map_err(|err| err.to_string())).collect::<Vec<_>>()
            })
            .await
            else {
                return;
            };

            let mut trashed = 0usize;
            let mut gone = Vec::new();
            let mut failures = Vec::new();

            for ((id, path), result) in doomed.iter().zip(results) {
                match result {
                    Ok(()) => {
                        if let Err(err) = state.catalog.remove_photo(*id) {
                            log::warn!("{}: {err}", path.display());
                        }
                        gone.push((*id, path.clone()));
                        trashed += 1;
                    }
                    Err(err) => failures.push(format!("{}: {err}", path.display())),
                }
            }

            home_sync::note_trashed(&state, &gone);
            reload_grid(&state);
            state.toast(&match failures.as_slice() {
                [] => format!("Moved {trashed} photo(s) to the trash"),
                [only] => format!("Could not delete — {only}"),
                many => format!("Moved {trashed}, could not delete {}", many.len()),
            });
            for failure in &failures {
                log::warn!("{failure}");
            }
        });
    });

    dialog.present(Some(window));
}
