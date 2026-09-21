use super::*;

pub(super) fn build_library_page(state: &App, window: &adw::ApplicationWindow) -> gtk::Box {
    let page = gtk::Box::new(gtk::Orientation::Vertical, 0);

    let bar = build_filter_bar(state, window);

    state.grid.welcome.bind_property("visible", &bar, "visible").invert_boolean().sync_create().build();
    page.append(&bar);
    let hint = key_hint(
        state,
        "hint-library-keys",
        "Cull from the keyboard: 0–5 rate, P picks, X rejects, U clears — Ctrl+/ lists every shortcut",
    );
    state.grid.welcome.bind_property("visible", &hint, "visible").invert_boolean().sync_create().build();
    page.append(&hint);

    state.grid.empty.set_vexpand(true);
    page.append(&state.grid.empty);

    let welcome = state.grid.welcome.clone();
    welcome.set_vexpand(true);
    welcome.set_visible(false);
    welcome.set_icon_name(Some("folder-pictures-symbolic"));
    welcome.set_title("Add a Folder of Photographs");
    welcome.set_description(Some(
        "Numa shows your photographs where they are. They are never moved, copied or changed.\n\n\
         Ratings, edits and names are kept in a hidden .numa folder inside the folder you add, \
         so it needs to be a folder you can write to — and they go wherever the folder goes.",
    ));
    let add = gtk::Button::with_label("Add Folder…");
    add.set_halign(gtk::Align::Center);
    add.add_css_class("pill");
    add.add_css_class("suggested-action");
    add.connect_clicked(glib::clone!(
        #[strong] state,
        #[weak] window,
        move |_| add_library_dialog(&state, &window)
    ));
    welcome.set_child(Some(&add));
    page.append(&welcome);

    state.grid.wall.add_css_class("photo-rows");
    state.grid.wall.set_margin_top(12);
    state.grid.wall.set_margin_bottom(12);
    state.grid.wall.set_margin_start(12);
    state.grid.wall.set_margin_end(12);

    let scroller = state.grid.scroller.clone();
    scroller.set_hexpand(true);
    scroller.set_vexpand(true);

    let viewport = gtk::Viewport::new(gtk::Adjustment::NONE, gtk::Adjustment::NONE);
    viewport.set_scroll_to_focus(false);
    viewport.set_child(Some(&state.grid.wall));
    scroller.set_child(Some(&viewport));

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

    page.add_controller(drop);

    let over = gtk::Overlay::new();
    over.set_child(Some(&scroller));
    over.add_overlay(&build_loupe(state));

    over.add_overlay(&cullbar::build(state));

    over.add_overlay(&build_compare(state));
    page.append(&over);

    let loupe_keys = gtk::EventControllerKey::new();
    loupe_keys.set_propagation_phase(gtk::PropagationPhase::Capture);
    loupe_keys.connect_key_pressed(glib::clone!(
        #[strong] state,
        move |_, key, _, _| loupe_key(&state, key)
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
        move |_, card| open_in_editor(&state, card)
    ));
    install_photo_menu(state, window);

    page
}

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
        } else if !window.is_action_enabled(bare) {

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

    for (name, ask) in [("photo-paste", false), ("photo-paste-choose", true)] {
        let paste = gio::SimpleAction::new(name, None);
        paste.connect_activate(glib::clone!(
            #[strong] state,
            #[weak] window,
            move |_, _| paste_settings(&state, &window, ask)
        ));
        window.add_action(&paste);
    }

    for action in [&edit, &rate, &flag, &delete_photos] {
        window.add_action(action);
    }
    install_preset_actions(state, window);

    debug_assert_missing_actions(&menu, window);

    install_photo_menu_popover(state, menu);
}

fn photo_menu_model(state: &App) -> gio::Menu {
    let menu = gio::Menu::new();
    menu.append(Some("Edit"), Some("win.photo-edit"));

    let rating = gio::Menu::new();
    for stars in 0..=5i32 {
        let label = match stars {
            0 => "No rating".to_string(),
            n => "★".repeat(n as usize),
        };
        let item = gio::MenuItem::new(Some(&label), None);
        item.set_action_and_target_value(Some("win.photo-rate"), Some(&stars.to_variant()));
        rating.append_item(&item);
    }
    menu.append_submenu(Some("Rating"), &rating);

    let flags = gio::Menu::new();
    for (label, which) in [("Pick", "pick"), ("Reject", "reject"), ("Clear flag", "none")] {
        let item = gio::MenuItem::new(Some(label), None);
        item.set_action_and_target_value(Some("win.photo-flag"), Some(&which.to_variant()));
        flags.append_item(&item);
    }
    menu.append_section(None, &flags);

    let settings = gio::Menu::new();
    settings.append(Some("Export…"), Some("win.photo-export"));

    settings.append(Some("Copy settings"), Some("win.photo-copy"));
    settings.append(Some("Paste settings"), Some("win.photo-paste"));
    settings.append(Some("Choose what to paste…"), Some("win.photo-paste-choose"));
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
            let Some(child) = selected_cards(&state).first().cloned() else { return };
            open_in_editor(&state, &child);
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
            let Some(card) = state.grid.wall.card_at(x, y) else { return };
            if !state.grid.wall.is_selected(&card) {
                state.grid.wall.select_only(&card);
            }
            rows_popover.set_pointing_to(Some(&gtk::gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
            rows_popover.popup();
        }
    ));
    state.grid.wall.add_controller(click);
}

pub(super) fn selected_cards(state: &App) -> Vec<gtk::Widget> {
    state.grid.wall.selected()
}

pub(super) fn confirm_delete(state: &App, window: &adw::ApplicationWindow) {
    let selected = selected_cards(state);
    if selected.is_empty() {
        state.toast("Select a photo first");
        return;
    }

    let doomed: Vec<(i64, PathBuf)> = {
        let cards = state.grid.cards.borrow();
        selected
            .iter()
            .filter_map(|child| child.widget_name().parse::<i64>().ok())
            .filter_map(|id| cards.get(&id).map(|(photo, _)| (id, photo.path.clone())))
            .collect()
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

    let dialog = adw::AlertDialog::new(
        Some(&title),
        Some(
            "The file goes to your desktop's trash, where it can be put back.              Its rating, flag and edits are forgotten here and those do not come back.",
        ),
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

        let mut trashed = 0usize;
        let mut failures = Vec::new();
        for (id, path) in &doomed {

            match gio::File::for_path(path).trash(gio::Cancellable::NONE) {
                Ok(()) => {
                    if let Err(err) = state.catalog.remove_photo(*id) {
                        log::warn!("{}: {err}", path.display());
                    }
                    trashed += 1;
                }
                Err(err) => failures.push(format!("{}: {err}", path.display())),
            }
        }

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

    dialog.present(Some(window));
}
