use super::*;

pub(super) fn refresh_mask_parts(state: &App, masks: &[Mask], selected: Option<usize>) {
    while let Some(row) = state.masks.mask_parts.first_child() {
        state.masks.mask_parts.remove(&row);
    }

    let Some(mask) = selected.and_then(|index| masks.get(index)) else {
        state.masks.mask_parts.set_visible(false);
        state.masks.mask_parts_header.set_visible(false);
        return;
    };
    let index = selected.unwrap_or(0);

    append_mask_name_row(state, mask, index);
    append_subtract_row(state, mask, index);

    state.masks.strength.set_value((mask.opacity * 100.0) as f64);
    state.masks.feather.set_value(mask.feather as f64);
    state.masks.edge.set_value(mask.shift as f64);

    if is_gradient(mask) {
        state.masks.mask_parts.set_visible(true);
        state.masks.mask_parts_header.set_visible(true);
        return;
    }

    if !append_range_rows(state, mask, index) {
        state.masks.mask_parts.set_visible(true);
        state.masks.mask_parts_header.set_visible(true);
        return;
    }

    let parts = list_mask_parts(state, mask);

    let last = parts.len() == 1 && matches!(mask.shape, Shape::Segment { .. });
    for (name, part, enabled) in parts {
        append_mask_part_row(state, index, last, name, part, enabled);
    }

    let any = state.masks.mask_parts.first_child().is_some();
    state.masks.mask_parts.set_visible(any);
    state.masks.mask_parts_header.set_visible(any);
}

fn append_mask_name_row(state: &App, mask: &Mask, index: usize) {
    let name = adw::EntryRow::new();
    name.set_title("Name");
    name.set_text(mask.name.as_deref().unwrap_or(""));

    name.set_tooltip_text(Some(&format!("Blank for \u{201c}{}\u{201d}", mask_name(mask))));
    name.connect_apply(glib::clone!(
        #[strong] state,
        move |entry| {
            if !state.applying.get() {
                rename_mask(&state, index, &entry.text());
            }
        }
    ));
    state.masks.mask_parts.append(&name);
}

fn append_subtract_row(state: &App, mask: &Mask, index: usize) {
    let masks = state.open.borrow().as_ref().map(|photo| photo.document.masks()).unwrap_or_default();
    let row = adw::ExpanderRow::new();
    row.set_title("Subtract");
    let mut chosen: Vec<String> = Vec::new();

    let tick = |row: &adw::ExpanderRow, title: &str, on: bool, toggled: Box<dyn Fn(bool)>| {
        let check = gtk::CheckButton::new();
        check.set_active(on);
        check.set_valign(gtk::Align::Center);
        let line = adw::ActionRow::new();
        line.set_title(title);
        line.add_prefix(&check);
        line.set_activatable_widget(Some(&check));
        check.connect_toggled(move |check| toggled(check.is_active()));
        row.add_row(&line);
    };

    for (other, them) in masks.iter().enumerate().filter(|(other, _)| *other != index) {
        let label = numa::io::masks::mask_label(&masks, other);
        let on = them.id != 0 && mask.minus_masks.contains(&them.id);
        if on {
            chosen.push(label.clone());
        }
        let state = state.clone();
        tick(&row, &label, on, Box::new(move |on| {
            if state.applying.get() {
                return;
            }

            let state = state.clone();
            glib::idle_add_local_once(move || subtract_mask_from(&state, index, other, on));
        }));
    }
    for (name, classes) in segment::PRESETS {
        let on = !mask.minus.is_empty() && classes.iter().all(|class| mask.minus.contains(class));
        if on {
            chosen.push(name.to_string());
        }
        let state = state.clone();
        let classes = classes.to_vec();
        tick(&row, name, on, Box::new(move |on| {
            if state.applying.get() {
                return;
            }
            let Some(mut minus) = state.open.borrow().as_ref().and_then(|photo| photo.document.masks().get(index).map(|mask| mask.minus.clone()))
            else {
                return;
            };
            minus.retain(|class| !classes.contains(class));
            if on {
                minus.extend(&classes);
            }
            let state = state.clone();
            glib::idle_add_local_once(move || subtract_from_mask(&state, index, minus));
        }));
    }
    row.set_subtitle(&match chosen.is_empty() {
        true => "Nothing".to_string(),
        false => chosen.join(", "),
    });
    state.masks.mask_parts.append(&row);
}

fn list_mask_parts(state: &App, mask: &Mask) -> Vec<(String, MaskPart, bool)> {
    let mut parts: Vec<(String, MaskPart, bool)> = Vec::new();
    if let Shape::Segment { classes } = &mask.shape {
        for class in classes {
            parts.push((
                segment::label(*class).unwrap_or("something").to_string(),
                MaskPart::Class(*class),
                !mask.muted.contains(class),
            ));
        }
    }

    let segmentation = (!sam::is_installed())
        .then(|| state.open.borrow().as_ref().and_then(|photo| photo.segmentation.clone()))
        .flatten();
    for (at, point) in mask.points.iter().enumerate() {
        let named = segmentation
            .as_ref()
            .map(|found| found.class_at(point.at[0], point.at[1]))
            .and_then(segment::label);
        let doing = if point.subtract { "Without" } else { "With" };
        parts.push((
            match named {
                Some(name) => format!("{doing} {name}"),
                None => format!(
                    "{doing} what is at {:.0} %, {:.0} %",
                    point.at[0] * 100.0,
                    point.at[1] * 100.0
                ),
            },
            MaskPart::Point(at),
            point.enabled,
        ));
    }

    let kind = |stroke: &Stroke| match (stroke.fill, stroke.erase) {
        (true, false) => "Lassoed in",
        (true, true) => "Lassoed out",
        (false, false) => "Painted",
        (false, true) => "Rubbed out",
    };
    let mut at = 0;
    while at < mask.strokes.len() {
        let stroke = &mask.strokes[at];
        let (name, on) = (kind(stroke), stroke.enabled);

        let run = mask.strokes[at..]
            .iter()
            .take_while(|next| kind(next) == name && next.enabled == on)
            .count();
        parts.push((
            if run > 1 { format!("{name} \u{00d7}{run}") } else { name.to_string() },
            MaskPart::Stroke { at, run },
            on,
        ));
        at += run;
    }
    parts
}

fn append_mask_part_row(
    state: &App,
    index: usize,
    last: bool,
    name: String,
    part: MaskPart,
    enabled: bool,
) {
    let row = adw::ActionRow::new();

    row.set_title_lines(1);
    row.set_subtitle_lines(1);
    set_row_title(&row, &name);
    row.add_css_class("mask-part");

    let on = gtk::ToggleButton::new();
    on.set_icon_name(if enabled { "view-reveal-symbolic" } else { "view-conceal-symbolic" });
    on.set_active(enabled);
    on.set_valign(gtk::Align::Center);
    on.add_css_class("flat");
    on.set_tooltip_text(Some(if enabled { "Switch this part off" } else { "Switch it back on" }));
    on.connect_toggled(glib::clone!(
        #[strong] state,
        move |button| {
            if !state.applying.get() && button.is_active() != enabled {
                toggle_mask_part(&state, index, part);
            }
        }
    ));
    row.add_suffix(&on);

    let remove = gtk::Button::from_icon_name("window-close-symbolic");
    remove.set_valign(gtk::Align::Center);
    remove.add_css_class("flat");

    remove.set_sensitive(!(last && matches!(part, MaskPart::Class(_))));
    remove.set_tooltip_text(Some("Take this back out of the mask"));
    remove.connect_clicked(glib::clone!(
        #[strong] state,
        move |_| remove_mask_part(&state, index, part)
    ));
    row.add_suffix(&remove);
    state.masks.mask_parts.append(&row);
}

pub(super) fn schedule_save(state: &App) {
    let generation = state.render.save_generation.get().wrapping_add(1);
    state.render.save_generation.set(generation);

    let state = state.clone();
    glib::timeout_add_local_once(std::time::Duration::from_secs(2), move || {
        if state.render.save_generation.get() != generation {
            return;
        }

        save_edits(&state, Saving::StillEditing);
    });
}

pub(super) fn mask_at(state: &App, index: usize) -> Option<Mask> {
    state.open.borrow().as_ref()?.document.masks().get(index).cloned()
}

pub(super) fn set_mask_visible(state: &App, index: usize, visible: bool) {
    {
        let mut open = state.open.borrow_mut();
        let Some(photo) = open.as_mut() else { return };
        let Some(mask) = photo.document.mask_mut(index) else { return };
        if mask.visible == visible {
            return;
        }
        mask.visible = visible;
        photo.view = None;
    }

    refresh_masks(state);
    request_render(state);
    state.mask_overlay.area.queue_draw();
    schedule_history_push(state);
}
