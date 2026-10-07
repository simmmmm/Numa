use super::*;

pub(super) use numa::io::history::{EditState, History};
#[cfg(test)]
pub(super) use numa::io::history::HISTORY_KEPT;

pub(super) type Geometry = (f32, Option<([f32; 4], f32)>, [f32; 2]);

pub(super) fn geometry_of_document(document: &Document) -> Geometry {

    let basic = document.basic();
    (document.rotation(), document.crop(), [basic.optics.lens_distortion, basic.optics.lens_vignetting])
}

#[derive(Clone)]
pub(super) struct ViewTile {

    pub(super) rect: [f32; 4],

    pub(super) key: ColourKey,

    pub(super) geometry: Geometry,

    pub(super) edge: u32,
    pub(super) image: Arc<LinearImage>,
}

pub(super) fn at_rest_of(now: Basic, balance: WhiteBalance, as_shot: WhiteBalance) -> [bool; SLIDER_COUNT] {
    let rest = Basic::default();
    let same = |a: f32, b: f32| (a - b).abs() < 1e-4;
    [

        same(balance.temperature, as_shot.temperature),
        same(balance.tint, as_shot.tint),
        same(now.tone.exposure, rest.tone.exposure),
        same(now.tone.contrast, rest.tone.contrast),
        same(now.tone.highlights, rest.tone.highlights),
        same(now.tone.shadows, rest.tone.shadows),
        same(now.tone.whites, rest.tone.whites),
        same(now.tone.blacks, rest.tone.blacks),
        same(now.presence.hdr, rest.presence.hdr),
        same(now.presence.vibrance, rest.presence.vibrance),
        same(now.presence.saturation, rest.presence.saturation),
        same(now.presence.clarity, rest.presence.clarity),
        same(now.presence.texture, rest.presence.texture),

        same(now.detail.sharpen, rest.detail.sharpen),
        same(now.detail.sharpen_radius, rest.detail.sharpen_radius),
        same(now.detail.sharpen_masking, rest.detail.sharpen_masking),
        same(now.detail.denoise_luma, rest.detail.denoise_luma),
        same(now.detail.denoise_detail, rest.detail.denoise_detail),
        same(now.detail.denoise_contrast, rest.detail.denoise_contrast),
        same(now.detail.denoise_colour, rest.detail.denoise_colour),
        same(now.detail.defringe, rest.detail.defringe),
        same(now.detail.moire, rest.detail.moire),
        same(now.effects.dehaze, rest.effects.dehaze),
        same(now.effects.vignette, rest.effects.vignette),
        same(now.effects.vignette_midpoint, rest.effects.vignette_midpoint),
        same(now.effects.vignette_roundness, rest.effects.vignette_roundness),
        same(now.effects.vignette_feather, rest.effects.vignette_feather),
        same(now.effects.grain, rest.effects.grain),
        same(now.effects.grain_size, rest.effects.grain_size),
        same(now.effects.grain_roughness, rest.effects.grain_roughness),
        same(now.calibration.shadow_tint, rest.calibration.shadow_tint),
        same(now.calibration.red_hue, rest.calibration.red_hue),
        same(now.calibration.red_saturation, rest.calibration.red_saturation),
        same(now.calibration.green_hue, rest.calibration.green_hue),
        same(now.calibration.green_saturation, rest.calibration.green_saturation),
        same(now.calibration.blue_hue, rest.calibration.blue_hue),
        same(now.calibration.blue_saturation, rest.calibration.blue_saturation),
        same(now.optics.lens_distortion, rest.optics.lens_distortion),
        same(now.optics.lens_vignetting, rest.optics.lens_vignetting),
        same(now.effects.mist, rest.effects.mist),
    ]
}

pub(super) fn refresh_slider_marks(state: &App) {
    let as_shot = state
        .open
        .borrow()
        .as_ref()
        .map(|photo| photo.as_shot)
        .unwrap_or(WhiteBalance { temperature: 5500.0, tint: 0.0 });

    let mark = |scale: &gtk::Scale, rest: bool| {
        let row = scale.parent();
        for widget in std::iter::once(scale.clone().upcast::<gtk::Widget>()).chain(row) {
            if rest {
                widget.remove_css_class("touched");
            } else {
                widget.add_css_class("touched");
            }
        }
    };

    state.sliders.white_balance_at_rest(as_shot);

    let panel = state.sliders.each();
    for ((_, scale, _), rest) in panel.iter().zip(state.sliders.at_rest(as_shot)) {
        mark(scale, rest);
    }

    refresh_rail_dots(state);

    REGISTERED.with(|registered| {
        for scale in registered.borrow().iter() {
            if panel.iter().any(|(_, theirs, _)| *theirs == scale) {
                continue;
            }
            mark(scale, (scale.value() - neutral_of(scale).unwrap_or(0.0)).abs() < 1e-4);
        }
    });
}

pub(super) fn build_history(state: &App) -> gtk::Popover {
    let column = gtk::Box::new(gtk::Orientation::Vertical, 8);
    column.set_margin_top(6);
    column.set_margin_bottom(6);
    column.set_margin_start(6);
    column.set_margin_end(6);

    let heading = gtk::Label::new(Some("HISTORY"));
    heading.set_xalign(0.0);
    heading.add_css_class("section-header");
    column.append(&heading);

    state.panel.history_list.set_selection_mode(gtk::SelectionMode::None);
    state.panel.history_list.add_css_class("boxed-list");

    let scroller = gtk::ScrolledWindow::new();
    scroller.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    scroller.set_propagate_natural_height(true);
    scroller.set_max_content_height(420);
    scroller.set_width_request(260);
    scroller.set_child(Some(&state.panel.history_list));
    column.append(&scroller);

    let note = gtk::Label::new(Some("Newest first. Click a step to go back to it."));
    note.set_xalign(0.0);
    note.set_wrap(true);
    note.add_css_class("profile-note");
    column.append(&note);

    let heading = gtk::Label::new(Some("SNAPSHOTS"));
    heading.set_xalign(0.0);
    heading.add_css_class("section-header");
    column.append(&heading);
    let snapshots = gtk::ListBox::new();
    snapshots.set_selection_mode(gtk::SelectionMode::None);
    snapshots.add_css_class("boxed-list");
    column.append(&snapshots);

    let popover = gtk::Popover::new();
    popover.add_css_class("numa-content");
    popover.set_child(Some(&column));

    popover.connect_show(glib::clone!(
        #[strong] state,
        move |_| {
            refresh_history(&state);
            refresh_snapshots(&state, &snapshots);
        }
    ));
    popover
}

pub(super) fn refresh_snapshots(state: &App, list: &gtk::ListBox) {
    while let Some(row) = list.first_child() {
        list.remove(&row);
    }
    let id = match state.open.borrow().as_ref().map(|photo| &photo.source) {
        Some(Source::Photo { id, .. }) => *id,
        _ => return,
    };
    let saved = state.catalog.snapshots(id).unwrap_or_default();

    let save = adw::EntryRow::new();
    save.set_title("Save Snapshot…");
    let button = gtk::Button::from_icon_name("list-add-symbolic");
    button.set_tooltip_text(Some("Save snapshot"));
    button.set_valign(gtk::Align::Center);
    button.add_css_class("flat");
    save.add_suffix(&button);
    let fallback = (saved.len() + 1..)
        .map(|n| format!("Snapshot {n}"))
        .find(|name| saved.iter().all(|(taken, _)| taken != name));
    let commit = glib::clone!(
        #[strong] state,
        #[weak] list,
        #[weak] save,
        move || {
            let text = save.text();
            let name = if text.trim().is_empty() { fallback.clone().unwrap_or_default() } else { text.to_string() };
            let Some(document) = state.open.borrow().as_ref().map(|photo| photo.document.clone()) else { return };
            match state.catalog.save_snapshot(id, &name, &document) {
                Ok(()) => refresh_snapshots(&state, &list),
                Err(err) => state.toast(&err),
            }
        }
    );
    save.connect_entry_activated(glib::clone!(#[strong] commit, move |_| commit()));
    button.connect_clicked(move |_| commit());
    list.append(&save);

    for (name, created) in saved {
        let row = adw::ActionRow::new();
        set_row_title(&row, &name);
        if let Some(when) = glib::DateTime::from_unix_local(created).and_then(|t| t.format("%e %b %Y, %H:%M")).ok() {
            row.set_subtitle(when.trim());
        }
        row.set_activatable(true);
        row.connect_activated(glib::clone!(
            #[strong] state,
            #[strong] name,
            move |_| match state.catalog.load_snapshot(id, &name) {
                Ok(document) => restore_snapshot(&state, &name, &document),
                Err(err) => state.toast(&format!("Could not read the snapshot: {err}")),
            }
        ));

        let delete = gtk::Button::from_icon_name("user-trash-symbolic");
        delete.set_tooltip_text(Some("Delete snapshot"));
        delete.set_valign(gtk::Align::Center);
        delete.add_css_class("flat");
        delete.connect_clicked(glib::clone!(
            #[strong] state,
            #[weak] list,
            move |_| {
                let Ok(document) = state.catalog.load_snapshot(id, &name) else { return };
                if let Err(err) = state.catalog.delete_snapshot(id, &name) {
                    state.toast(&err);
                    return;
                }
                refresh_snapshots(&state, &list);
                let toast = adw::Toast::new(&format!("Deleted “{name}”"));
                toast.set_button_label(Some("Undo"));
                toast.connect_button_clicked(glib::clone!(
                    #[strong] state,
                    #[strong] name,
                    #[weak] list,
                    move |_| {
                        if let Err(err) = state.catalog.save_snapshot(id, &name, &document) {
                            state.toast(&err);
                        }
                        refresh_snapshots(&state, &list);
                    }
                ));
                state.toasts.add_toast(toast);
            }
        ));
        row.add_suffix(&delete);
        list.append(&row);
    }
}

pub(super) fn restore_snapshot(state: &App, name: &str, document: &Document) {
    let stepped = state.open.borrow_mut().as_mut().map(|photo| {
        photo.history.push(EditState::of(&photo.document));
        photo.history.push_named(EditState::of(document), name);
        (photo.history.states[photo.history.position].clone(), photo.as_shot)
    });
    if let Some((edit, as_shot)) = stepped {
        apply_history(state, edit, as_shot);
    }
}

pub(super) fn refresh_history(state: &App) {
    while let Some(row) = state.panel.history_list.first_child() {
        state.panel.history_list.remove(&row);
    }

    let (steps, position) = {
        let open = state.open.borrow();
        let Some(photo) = open.as_ref() else { return };
        (photo.history.steps(), photo.history.position)
    };

    for (index, step) in steps.iter().enumerate().rev() {
        let row = adw::ActionRow::new();
        row.set_title(step);
        row.set_activatable(true);
        row.connect_activated(glib::clone!(
            #[strong] state,
            move |_| jump_history(&state, index)
        ));
        if index == position {
            row.add_css_class("current-step");
        }

        if index > position {
            row.add_css_class("dim-label");
        }
        state.panel.history_list.append(&row);
    }
}
