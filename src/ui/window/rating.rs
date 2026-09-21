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

    let known = state.grid.cards.borrow().get(&id).map(|(photo, _)| (photo.rating, photo.flag));
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

    if let Some((photo, badge)) = state.grid.cards.borrow_mut().get_mut(&id) {
        photo.rating = rating;
        photo.flag = flag;
        badge.set_text(&badge_text(rating, flag));
        style_badge(badge, rating, flag);
    }
    if let Some(badge) = state.filmstrip.badges.borrow().get(&id) {
        badge.set_text(&strip_badge_text(rating, flag));
        badge.set_visible(!badge.text().is_empty());
    }
    write_rating_button(state, rating, flag);

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

pub(super) fn write_rating_button(state: &App, rating: u8, flag: Flag) {

    let label = match (rating, flag) {
        (0, Flag::None) => "\u{2606}".to_string(),
        (0, Flag::Picked) => "\u{2606} \u{2691}".to_string(),
        (0, Flag::Rejected) => "\u{2606} \u{2715}".to_string(),
        (n, Flag::Picked) => format!("\u{2605} {n} \u{2691}"),
        (n, Flag::Rejected) => format!("\u{2605} {n} \u{2715}"),
        (n, Flag::None) => format!("\u{2605} {n}"),
    };
    state.editor_page.rating_button.set_label(&label);

    state.editor_page.rating_button.remove_css_class("rated");
    state.editor_page.rating_button.remove_css_class("rejected");
    if flag == Flag::Rejected {
        state.editor_page.rating_button.add_css_class("rejected");
    } else if rating > 0 || flag == Flag::Picked {
        state.editor_page.rating_button.add_css_class("rated");
    }
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
    hold.set_propagation_phase(gtk::PropagationPhase::Capture);
    hold.connect_key_pressed(glib::clone!(
        #[strong] state,
        move |controller, key, _, _| {
            let typing = controller
                .widget()
                .and_then(|window| window.root())
                .and_then(|root| root.focus())
                .is_some_and(|focus| focus.is::<gtk::Editable>() || focus.is::<gtk::TextView>());
            if key != gtk::gdk::Key::space || typing || state.stack.visible_child_name().as_deref() != Some("editor") {
                return glib::Propagation::Proceed;
            }
            state.editor_page.before.set_active(true);
            glib::Propagation::Stop
        }
    ));
    hold.connect_key_released(glib::clone!(
        #[strong] state,
        move |_, key, _, _| {
            if key == gtk::gdk::Key::space {
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
            rate_open_photo(&state, action);
            return glib::Propagation::Stop;
        }

        if matches!(key.to_unicode(), Some('g' | 'G')) {
            cycle_guides(&state);
            return glib::Propagation::Stop;
        }

        if matches!(key.to_unicode(), Some('i' | 'I')) {
            let info = &state.info.button;
            if info.is_active() {
                info.popdown();
            } else {
                info.popup();
            }
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

    apply_to_selection(&state, action);
    refresh_loupe_bar(&state);
    glib::Propagation::Stop
}

#[derive(Clone, Copy)]
pub(super) enum Action {
    Rate(u8),
    Flag(Flag),
}

pub(super) fn apply_to_selection(state: &App, action: Action) {
    let selected = selected_cards(state);
    if selected.is_empty() {
        state.toast("Select a photo first");
        return;
    }
    let ids: Vec<i64> = selected.iter().filter_map(|child| child.widget_name().parse().ok()).collect();
    apply_to_ids(state, &ids, action);
}

pub(super) fn apply_to_ids(state: &App, ids: &[i64], action: Action) {
    let cards = state.grid.cards.borrow();

    for &id in ids {

        let result = match action {
            Action::Rate(rating) => state.catalog.set_rating(id, rating),
            Action::Flag(flag) => state.catalog.set_flag(id, flag),
        };

        if let Err(err) = result {
            drop(cards);
            state.toast(&format!("Could not save: {err}"));
            return;
        }

        if let Some((_, badge)) = cards.get(&id) {
            let (rating, flag) = match action {
                Action::Rate(rating) => (rating, flag_from_badge(&badge.text())),
                Action::Flag(flag) => (rating_from_badge(&badge.text()), flag),
            };
            badge.set_text(&badge_text(rating, flag));
            style_badge(badge, rating, flag);
        }
    }

    drop(cards);

    let filter = state.libraries.filter.borrow();
    let narrowing = filter.min_rating > 0 || filter.flag.is_some();
    drop(filter);
    if narrowing {
        reload_grid(state);
    }
}

pub(super) fn rating_from_badge(text: &str) -> u8 {
    text.chars().filter(|c| *c == '★').count() as u8
}

pub(super) fn flag_from_badge(text: &str) -> Flag {
    if text.contains('⚑') {
        Flag::Picked
    } else if text.contains('✕') {
        Flag::Rejected
    } else {
        Flag::None
    }
}

pub(super) struct SeenFace {

    pub(super) at: [f32; 4],
    pub(super) embedding: [f32; cull::people::LENGTH],
    pub(super) portrait: image::RgbImage,
}
