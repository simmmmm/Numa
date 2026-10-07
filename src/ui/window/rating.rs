use super::*;

pub(super) fn rate_open_photo(state: &App, action: Action) {
    let id = state.open.borrow().as_ref().and_then(|photo| match &photo.source {
        Source::Photo { id, .. } => Some(*id),

        Source::Bracket { .. } => None,
    });
    let Some(id) = id else {
        state.toast("A merged image has no place in the catalog yet");
        return;
    };

    let known = state.grid.cards.borrow().get(&id).map(|photo| (photo.rating, photo.flag));
    let (rating, flag) = match (action, known) {
        (Action::Rate(rating), Some((_, flag))) => (rating, flag),
        (Action::Flag(flag), Some((rating, _))) => (rating, flag),
        (Action::Rate(rating), None) => (rating, Flag::None),
        (Action::Flag(flag), None) => (0, flag),
    };

    let saved = match action {
        Action::Rate(rating) => state.catalog.set_rating(id, rating),
        Action::Flag(flag) => state.catalog.set_flag(id, flag),
    };
    if let Err(err) = saved {
        state.toast(&format!("Could not save: {err}"));
        return;
    }

    if let Some(photo) = state.grid.cards.borrow_mut().get_mut(&id) {
        photo.rating = rating;
        photo.flag = flag;
    }
    state.grid.wall.rebind_all();
    rebind_frames(state);
    show_rating(state, rating, flag);

    let filter = state.libraries.filter.borrow();
    if filter.min_rating > 0 || filter.flag.is_some() {
        state.grid.stale.set(true);
    }
}

pub(super) fn rate_here(state: &App, action: Action) {
    if state.stack.visible_child_name().as_deref() == Some("editor") {
        rate_open_photo(state, action);
    } else {
        apply_to_selection(state, action);
    }
}

pub(super) fn build_bar_rating(state: &App) -> gtk::Box {
    let row = state.editor_page.rating.clone();
    row.add_css_class("bar-rating");
    row.set_margin_start(6);

    let stars = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    stars.update_property(&[gtk::accessible::Property::Label("Rating")]);
    for (index, star) in state.editor_page.stars.iter().enumerate() {
        let value = index as u8 + 1;
        star.set_icon_name("non-starred-symbolic");
        star.add_css_class("flat");
        star.set_tooltip_text(Some(&match value {
            1 => "1 Star (1)".to_string(),
            n => format!("{n} Stars ({n})"),
        }));
        star.connect_clicked(glib::clone!(
            #[strong] state,
            move |_| rate_from_bar(&state, Action::Rate(value))
        ));

        let hover = gtk::EventControllerMotion::new();
        hover.connect_enter(glib::clone!(
            #[strong] state,
            move |_, _, _| paint_stars(&state, value)
        ));
        star.add_controller(hover);
        stars.append(star);
    }
    let leave = gtk::EventControllerMotion::new();
    leave.connect_leave(glib::clone!(
        #[strong] state,
        move |_| paint_stars(&state, state.editor_page.shown.get())
    ));
    stars.add_controller(leave);
    row.append(&stars);

    for (button, icon, tip, flag) in [
        (&state.editor_page.pick, "emoji-flags-symbolic", "Pick (P)", Flag::Picked),
        (&state.editor_page.reject, "window-close-symbolic", "Reject (X)", Flag::Rejected),
    ] {
        button.set_icon_name(icon);
        button.add_css_class("flat");
        button.set_tooltip_text(Some(tip));
        button.connect_clicked(glib::clone!(
            #[strong] state,
            move |_| rate_from_bar(&state, Action::Flag(flag))
        ));
        row.append(button);
    }
    state.editor_page.pick.set_margin_start(6);
    row
}

fn rate_from_bar(state: &App, action: Action) {
    let now = audit::open_id(state).and_then(|id| state.grid.cards.borrow().get(&id).map(|photo| (photo.rating, photo.flag)));
    let action = match (action, now) {
        (Action::Rate(value), Some((rating, _))) if rating == value => Action::Rate(0),
        (Action::Flag(flag), Some((_, now))) if now == flag => Action::Flag(Flag::None),
        _ => action,
    };
    rate_open_photo(state, action);
}

fn paint_stars(state: &App, filled: u8) {
    for (index, star) in state.editor_page.stars.iter().enumerate() {
        star.set_icon_name(if (index as u8) < filled { "starred-symbolic" } else { "non-starred-symbolic" });
    }
}

fn mark(widget: &impl IsA<gtk::Widget>, class: &str, on: bool) {
    match on {
        true => widget.add_css_class(class),
        false => widget.remove_css_class(class),
    }
}

pub(super) fn show_rating(state: &App, rating: u8, flag: Flag) {
    let page = &state.editor_page;
    page.shown.set(rating);
    paint_stars(state, rating);
    for (index, star) in page.stars.iter().enumerate() {
        mark(star, "rated", (index as u8) < rating);
    }
    mark(&page.pick, "rated", flag == Flag::Picked);
    mark(&page.reject, "rejected", flag == Flag::Rejected);

    mark(&page.rating, "rejected", flag == Flag::Rejected);
}

pub(super) fn name_icon_buttons(root: &gtk::Widget) {
    let mut pending = vec![root.clone()];
    while let Some(widget) = pending.pop() {
        let unnamed = widget.is::<gtk::Button>() || widget.is::<gtk::MenuButton>() || widget.is::<gtk::ToggleButton>();
        if unnamed {
            let has_label = widget
                .downcast_ref::<gtk::Button>()
                .and_then(|button| button.label())
                .is_some_and(|label| !label.is_empty());
            if let (false, Some(tooltip)) = (has_label, widget.tooltip_text()) {
                widget.update_property(&[gtk::accessible::Property::Label(&tooltip)]);
            }
        }
        let mut child = widget.first_child();
        while let Some(current) = child {
            child = current.next_sibling();
            pending.push(current);
        }
    }
}

pub(super) fn install_rating_shortcuts(state: &App, window: &adw::ApplicationWindow) {
    let keys = gtk::EventControllerKey::new();
    keys.connect_key_pressed(glib::clone!(
        #[strong] state,
        #[strong] window,
        move |_, key, _, modifiers| {
            if state.stack.visible_child_name().as_deref() == Some("editor") {
                return editor_key(&state, &window, key, modifiers);
            }

            if state.stack.visible_child_name().as_deref() != Some("library") {
                return glib::Propagation::Proceed;
            }

            library_key(&state, &window, key, modifiers)
        }
    ));
    window.add_controller(keys);
    window.add_controller(hold_for_before(state));

    window.connect_is_active_notify(glib::clone!(
        #[strong] state,
        move |window| {
            if !window.is_active() {
                state.editor_page.before.set_active(false);
            }
        }
    ));
}

fn hold_for_before(state: &App) -> gtk::EventControllerKey {
    let hold = gtk::EventControllerKey::new();
    let matte_was: Rc<Cell<Option<bool>>> = Rc::new(Cell::new(None));
    let before_auto: Rc<Cell<bool>> = Rc::default();
    hold.set_propagation_phase(gtk::PropagationPhase::Capture);
    hold.connect_key_pressed(glib::clone!(
        #[strong] state,
        #[strong] matte_was,
        #[strong] before_auto,
        move |controller, key, _, modifiers| {
            let typing = controller
                .widget()
                .and_then(|window| window.root())
                .and_then(|root| root.focus())
                .is_some_and(|focus| focus.is::<gtk::Editable>() || focus.is::<gtk::TextView>());
            if key != gtk::gdk::Key::space || typing || state.stack.visible_child_name().as_deref() != Some("editor") {
                return glib::Propagation::Proceed;
            }

            if modifiers.contains(gtk::gdk::ModifierType::SHIFT_MASK) && show_before_auto(&state) {
                before_auto.set(true);
                return glib::Propagation::Stop;
            }
            if state.mask_overlay.selected_mask.get().is_some() {

                if matte_was.get().is_none() {
                    matte_was.set(Some(state.masks.show_matte.replace(true)));
                    state.mask_overlay.area.queue_draw();
                }
                return glib::Propagation::Stop;
            }
            state.editor_page.before.set_active(true);
            glib::Propagation::Stop
        }
    ));
    hold.connect_key_released(glib::clone!(
        #[strong] state,
        #[strong] before_auto,
        move |_, key, _, _| {
            if key == gtk::gdk::Key::space && before_auto.replace(false) {
                end_before_auto(&state);
                return;
            }
            if key == gtk::gdk::Key::space {
                if let Some(was) = matte_was.take() {
                    state.masks.show_matte.set(was);
                    state.mask_overlay.area.queue_draw();
                }
                state.editor_page.before.set_active(false);
            }
        }
    ));
    hold
}

fn editor_key(
    state: &App,
    window: &adw::ApplicationWindow,
    key: gtk::gdk::Key,
    modifiers: gtk::gdk::ModifierType,
) -> glib::Propagation {

    if key == gtk::gdk::Key::Escape && state.mask_overlay.selected_mask.get().is_some() {
        leave_mask(&state);
        return glib::Propagation::Stop;
    }

    match key {
        gtk::gdk::Key::Left | gtk::gdk::Key::Page_Up => {
            step_photo(&state, false);
            return glib::Propagation::Stop;
        }
        gtk::gdk::Key::Right | gtk::gdk::Key::Page_Down => {
            step_photo(&state, true);
            return glib::Propagation::Stop;
        }
        _ => {}
    }

    if !modifiers.intersects(gtk::gdk::ModifierType::CONTROL_MASK | gtk::gdk::ModifierType::ALT_MASK) {
        let action = match key.to_unicode() {
            Some(digit @ '0'..='5') => {
                Some(Action::Rate(digit as u8 - b'0'))
            }
            Some('p' | 'P') => Some(Action::Flag(Flag::Picked)),
            Some('x' | 'X') => Some(Action::Flag(Flag::Rejected)),
            Some('u' | 'U') => Some(Action::Flag(Flag::None)),
            _ => None,
        };
        if let Some(action) = action {
            let open: Vec<i64> = audit::open_id(&state).into_iter().collect();
            rate_open_photo(&state, toggled(&state, &open, action));
            return glib::Propagation::Stop;
        }

        if matches!(key.to_unicode(), Some('g' | 'G')) {
            cycle_guides(&state);
            return glib::Propagation::Stop;
        }

        if matches!(key.to_unicode(), Some('i' | 'I')) {
            toggle_info(&state);
            return glib::Propagation::Stop;
        }

        if matches!(key.to_unicode(), Some('a' | 'A')) && !modifiers.contains(gtk::gdk::ModifierType::SHIFT_MASK) {
            auto_press(&state);
            return glib::Propagation::Stop;
        }
    }

    if modifiers.contains(gtk::gdk::ModifierType::ALT_MASK) {
        if let Some(digit @ '1'..='9') = key.to_unicode() {
            let index = digit as usize - '1' as usize;
            if let Some((name, _, _, _)) = PANEL_TABS.get(index) {
                show_panel_tab(&state, name);
                return glib::Propagation::Stop;
            }
        }
    }

    if !modifiers.contains(gtk::gdk::ModifierType::CONTROL_MASK) {
        return glib::Propagation::Proceed;
    }
    return match key.to_unicode() {

        Some('c' | 'C') if modifiers.contains(gtk::gdk::ModifierType::SHIFT_MASK) => {
            copy_image(&state);
            glib::Propagation::Stop
        }
        Some('c' | 'C') => {
            copy_settings(&state);
            glib::Propagation::Stop
        }
        Some('v' | 'V') => {
            paste_settings(&state, &window, false);
            glib::Propagation::Stop
        }
        Some('z') => {
            step_history(&state, false);
            glib::Propagation::Stop
        }

        Some('Z') => {
            step_history(&state, true);
            glib::Propagation::Stop
        }
        _ => glib::Propagation::Proceed,
    };
}

fn library_key(
    state: &App,
    window: &adw::ApplicationWindow,
    key: gtk::gdk::Key,
    modifiers: gtk::gdk::ModifierType,
) -> glib::Propagation {

    if state.loupe.at.get().is_none() && key == gtk::gdk::Key::space {
        open_loupe(&state);
        return glib::Propagation::Stop;
    }

    if state.loupe.at.get().is_none() && matches!(key, gtk::gdk::Key::c | gtk::gdk::Key::C)
        && !modifiers.intersects(gtk::gdk::ModifierType::CONTROL_MASK | gtk::gdk::ModifierType::ALT_MASK | gtk::gdk::ModifierType::SUPER_MASK)
    {
        open_compare(&state);
        return glib::Propagation::Stop;
    }

    if modifiers.contains(gtk::gdk::ModifierType::CONTROL_MASK) {
        return match key.to_unicode() {
            Some('v' | 'V') => {
                paste_settings(&state, &window, false);
                glib::Propagation::Stop
            }
            _ => glib::Propagation::Proceed,
        };
    }

    if matches!(key, gtk::gdk::Key::Delete | gtk::gdk::Key::KP_Delete) {
        confirm_delete(&state, &window);
        return glib::Propagation::Stop;
    }

    let action = match key.to_unicode() {
        Some(digit @ '0'..='5') => Action::Rate(digit as u8 - b'0'),
        Some('p' | 'P') => Action::Flag(Flag::Picked),
        Some('x' | 'X') => Action::Flag(Flag::Rejected),
        Some('u' | 'U') => Action::Flag(Flag::None),
        _ => return glib::Propagation::Proceed,
    };

    apply_to_selection(&state, toggled(&state, &selected_ids(&state), action));
    refresh_loupe_bar(&state);
    glib::Propagation::Stop
}

#[derive(Clone, Copy, PartialEq)]
pub(super) enum Action {
    Rate(u8),
    Flag(Flag),
}

pub(super) fn apply_to_selection(state: &App, action: Action) {
    let ids = selected_ids(state);
    if ids.is_empty() {
        state.toast("Select a photo first");
        return;
    }
    apply_to_ids(state, &ids, action);
}

pub(super) fn toggled(state: &App, ids: &[i64], action: Action) -> Action {
    let Action::Flag(flag) = action else { return action };
    let cards = state.grid.cards.borrow();
    let all = !ids.is_empty() && ids.iter().all(|id| cards.get(id).is_some_and(|photo| photo.flag == flag));
    Action::Flag(if all { Flag::None } else { flag })
}

pub(super) fn apply_to_ids(state: &App, ids: &[i64], action: Action) {
    let result = match action {
        Action::Rate(rating) => state.catalog.set_ratings(ids, rating),
        Action::Flag(flag) => state.catalog.set_flags(ids, flag),
    };
    if let Err(err) = result {
        state.toast(&format!("Could not save: {err}"));
        return;
    }

    note_mark(state, ids, action);
    if state.loupe.at.get().is_some() {
        let cards = state.grid.cards.borrow();
        let before = ids.iter().filter_map(|id| cards.get(id).map(|photo| (*id, photo.rating, photo.flag))).collect();
        state.loupe.undo.borrow_mut().push(before);
    }

    {
        let mut cards = state.grid.cards.borrow_mut();
        for id in ids {
            if let Some(photo) = cards.get_mut(id) {
                match action {
                    Action::Rate(rating) => photo.rating = rating,
                    Action::Flag(flag) => photo.flag = flag,
                }
            }
        }
    }
    state.grid.wall.rebind_all();
    rebind_frames(state);

    let filter = state.libraries.filter.borrow();
    let narrowing = match action {
        Action::Rate(_) => filter.min_rating > 0,
        Action::Flag(_) => filter.flag.is_some(),
    };
    drop(filter);

    if narrowing && state.loupe.at.get().is_some() {
        state.grid.stale.set(true);
    } else if narrowing {
        reload_grid(state);
    }
}

pub(super) struct SeenFace {

    pub(super) at: [f32; 4],
    pub(super) embedding: [f32; cull::people::LENGTH],
    pub(super) portrait: image::RgbImage,
}
