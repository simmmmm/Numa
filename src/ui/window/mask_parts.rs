use super::*;

pub(super) fn refresh_mask_parts(state: &App, masks: &[Mask], selected: Option<usize>) {
    while let Some(row) = state.masks.mask_parts.first_child() {
        state.masks.mask_parts.remove(&row);
    }

    let bar = &state.masks.toolbar;
    let Some(mask) = selected.and_then(|index| masks.get(index)) else {
        state.masks.mask_parts.set_visible(false);
        state.masks.mask_parts_header.set_visible(false);
        bar.parts_status.set_visible(false);
        bar.undo_part.set_visible(false);
        return;
    };
    let index = selected.unwrap_or(0);

    state.masks.mask_parts_header.set_visible(true);
    bar.power_item.set_label(if mask.visible { "Turn Off" } else { "Turn On" });
    let without = refresh_subtract(state, masks, index);
    let say = |parts: usize| {
        let mut words = match parts {
            0 => Vec::new(),
            1 => vec!["1 part".to_string()],
            many => vec![format!("{many} parts")],
        };
        if !without.is_empty() {

            let word = if words.is_empty() { "Without" } else { "without" };
            words.push(format!("{word} {}", without.join(", ")));
        }
        let line = words.join(" · ");
        bar.parts_status.set_visible(!line.is_empty());
        bar.parts_status.set_text(&line);
    };
    bar.undo_part.set_visible(false);

    state.masks.strength.set_value((mask.opacity * 100.0) as f64);
    state.masks.feather.set_value(mask.feather as f64);
    state.masks.edge.set_value(mask.shift as f64);

    if is_gradient(mask) {
        state.masks.mask_parts.set_visible(false);
        say(0);
        return;
    }

    if !append_range_rows(state, mask, index) {
        state.masks.mask_parts.set_visible(state.masks.mask_parts.first_child().is_some());
        say(0);
        return;
    }

    let parts = list_mask_parts(state, mask);
    say(parts.len());

    bar.undo_part.set_visible(parts.last().is_some_and(|(_, part, _)| !matches!(part, MaskPart::Class(_))));

    let last = parts.len() == 1 && matches!(mask.shape, Shape::Segment { .. });
    for (name, part, enabled) in parts {
        append_mask_part_row(state, index, last, name, part, enabled);
    }

    state.masks.mask_parts.set_visible(state.masks.mask_parts.first_child().is_some());
}

pub(super) fn undo_last_part(state: &App) {
    let Some(index) = state.mask_overlay.selected_mask.get() else { return };
    let Some(mask) = mask_at(state, index) else { return };
    let last = match (mask.strokes.len(), mask.points.len()) {
        (0, 0) => return,
        (0, points) => MaskPart::Point(points - 1),
        (strokes, _) => MaskPart::Stroke { at: strokes - 1, run: 1 },
    };
    remove_mask_part(state, index, last);
}

fn refresh_subtract(state: &App, masks: &[Mask], index: usize) -> Vec<String> {
    let bar = &state.masks.toolbar;
    let actions = &bar.refine_actions;
    for name in actions.list_actions() {
        if name.starts_with("minus-") {
            actions.remove_action(&name);
        }
    }
    bar.subtract_menu.remove_all();
    let mask = &masks[index];
    let mut chosen: Vec<String> = Vec::new();

    let item = |section: &gio::Menu, name: String, label: &str, on: bool, run: Box<dyn Fn()>| {
        let action = gio::SimpleAction::new_stateful(&name, None, &on.to_variant());
        let run = Rc::new(run);
        action.connect_activate(move |_, _| {
            let run = run.clone();
            glib::idle_add_local_once(move || run());
        });
        actions.add_action(&action);
        section.append(Some(label), Some(&format!("mask.{name}")));
    };

    let others = gio::Menu::new();
    for (other, them) in masks.iter().enumerate().filter(|(other, _)| *other != index) {
        let label = numa::io::masks::mask_label(masks, other);
        let on = them.id != 0 && mask.minus_masks.contains(&them.id);
        if on {
            chosen.push(label.clone());
        }
        let state = state.clone();
        item(&others, format!("minus-mask-{other}"), &label, on, Box::new(move || subtract_mask_from(&state, index, other, !on)));
    }
    let found = gio::Menu::new();

    let presets = if segment::is_installed() { segment::PRESETS } else { &[] };
    for (at, (name, classes)) in presets.iter().enumerate() {
        let on = !mask.minus.is_empty() && classes.iter().all(|class| mask.minus.contains(class));
        if on {
            chosen.push(name.to_string());
        }
        let (state, classes) = (state.clone(), classes.to_vec());
        item(&found, format!("minus-preset-{at}"), name, on, Box::new(move || {
            let Some(mut minus) = mask_at(&state, index).map(|mask| mask.minus) else { return };
            minus.retain(|class| !classes.contains(class));
            if !on {
                minus.extend(&classes);
            }
            subtract_from_mask(&state, index, minus);
        }));
    }
    bar.subtract_menu.append_section(None, &others);
    bar.subtract_menu.append_section(None, &found);
    chosen
}

fn list_mask_parts(state: &App, mask: &Mask) -> Vec<(String, MaskPart, bool)> {

    let segmentation = (!sam::is_installed())
        .then(|| state.open.borrow().as_ref().and_then(|photo| photo.segmentation.clone()))
        .flatten();
    let kind = |stroke: &Stroke| match (stroke.fill, stroke.erase) {
        (true, false) => "Lassoed in",
        (true, true) => "Lassoed out",
        (false, false) => "Painted",
        (false, true) => "Rubbed out",
    };
    mask.parts()
        .into_iter()
        .map(|row| {
            let name = match row.part {
                MaskPart::Class(class) => segment::label(class).unwrap_or("something").to_string(),
                MaskPart::Point(at) => {
                    let point = &mask.points[at];
                    let named = segmentation
                        .as_ref()
                        .map(|found| found.class_at(point.at[0], point.at[1]))
                        .and_then(segment::label);
                    let doing = if point.subtract { "Without" } else { "With" };
                    match named {
                        Some(name) => format!("{doing} {name}"),
                        None => format!(
                            "{doing} what is at {:.0} %, {:.0} %",
                            point.at[0] * 100.0,
                            point.at[1] * 100.0
                        ),
                    }
                }
                MaskPart::Stroke { at, run } => {
                    let name = kind(&mask.strokes[at]);
                    if run > 1 { format!("{name} \u{00d7}{run}") } else { name.to_string() }
                }
            };
            (name, row.part, row.on)
        })
        .collect()
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
