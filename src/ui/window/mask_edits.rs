use super::*;

pub(super) fn pick_range(state: &App, u: f32, v: f32) {
    let Some(index) = state.mask_overlay.selected_mask.get() else { return };
    let Some(frame) = mask_frame(state) else {
        state.toast("Nothing to sample yet");
        return;
    };

    let x = ((u.clamp(0.0, 1.0) * frame.width() as f32) as u32).min(frame.width() - 1);
    let y = ((v.clamp(0.0, 1.0) * frame.height() as f32) as u32).min(frame.height() - 1);
    let pixel = frame.get_pixel(x, y).0;
    let rgb = [pixel[0] as f32 / 255.0, pixel[1] as f32 / 255.0, pixel[2] as f32 / 255.0];

    let named = {
        let mut open = state.open.borrow_mut();
        let Some(photo) = open.as_mut() else { return };
        let Some(mask) = photo.document.mask_mut(index) else { return };
        match &mut mask.shape {
            Shape::ColourRange { hue, saturation, picked, .. } => {
                *picked = true;
                let [sampled, sampled_saturation, value] =
                    numa::core::profile::rgb_to_hsv(rgb);

                if sampled_saturation < 0.08 || value < 0.04 {
                    None
                } else {
                    *hue = sampled * 60.0;

                    *saturation =
                        saturation.min((sampled_saturation * 0.6).clamp(0.05, 0.9));
                    Some(format!("Matching this colour — {:.0}°", sampled * 60.0))
                }
            }
            Shape::LuminanceRange { low, high, picked, .. } => {
                *picked = true;
                let luma = 0.2126 * rgb[0] + 0.7152 * rgb[1] + 0.0722 * rgb[2];

                let half = ((*high - *low) * 0.5).max(0.05);
                *low = (luma - half).clamp(0.0, 1.0);
                *high = (luma + half).clamp(0.0, 1.0);
                Some(format!("Matching this brightness — {:.0}%", luma * 100.0))
            }
            _ => None,
        }
    };

    let Some(said) = named else {
        state.toast("That pixel has no colour to match — try something less grey");
        return;
    };

    rebuild_mask_map(state, index);
    refresh_masks(state);
    request_render(state);
    show_coverage(state);
    schedule_history_push(state);
    state.toast(&said);
}

pub(super) fn set_mask_range(state: &App, index: usize, which: u8, value: f32) {
    {
        let mut open = state.open.borrow_mut();
        let Some(photo) = open.as_mut() else { return };
        let Some(mask) = photo.document.mask_mut(index) else { return };
        match (&mut mask.shape, which) {
            (Shape::ColourRange { hue, .. }, 0) => *hue = value,
            (Shape::ColourRange { spread, .. }, 1) => *spread = value,
            (Shape::ColourRange { saturation, .. }, 2) => *saturation = value / 100.0,
            (Shape::LuminanceRange { low, .. }, 3) => *low = value / 100.0,
            (Shape::LuminanceRange { high, .. }, 4) => *high = value / 100.0,
            (Shape::LuminanceRange { softness, .. }, 5) => *softness = value / 100.0,
            _ => return,
        }
        photo.view = None;
    }

    rebuild_mask_map(state, index);
    request_render(state);
    show_coverage(state);
    schedule_history_push(state);
}

pub(super) fn set_mask_edge(state: &App, index: usize, which: u8, value: f32) {
    {
        let mut open = state.open.borrow_mut();
        let Some(photo) = open.as_mut() else { return };
        let Some(mask) = photo.document.mask_mut(index) else { return };
        let target = if which == 0 { &mut mask.feather } else { &mut mask.shift };
        if (*target - value).abs() < 1e-4 {
            return;
        }
        *target = value;
    }

    let reshaped = {
        let (width, height) = mask_raster_size(state);
        let mut open = state.open.borrow_mut();
        open.as_mut()
            .and_then(|photo| photo.document.mask_mut(index))
            .is_some_and(|mask| mask.reshape_edge_draft(width, height))
    };
    if reshaped {

        refresh_outline(state);
    } else {
        rebuild_mask_map(state, index);
    }
    request_render(state);
    show_coverage(state);
    schedule_history_push(state);
}

pub(super) fn set_mask_opacity(state: &App, index: usize, opacity: f32) {
    {
        let mut open = state.open.borrow_mut();
        let Some(photo) = open.as_mut() else { return };
        let Some(mask) = photo.document.mask_mut(index) else { return };
        if (mask.opacity - opacity).abs() < 1e-4 {
            return;
        }
        mask.opacity = opacity.clamp(0.0, 1.0);
    }

    request_render(state);
    state.mask_overlay.area.queue_draw();
    schedule_history_push(state);
}

pub(super) fn rename_mask(state: &App, index: usize, name: &str) {
    {
        let mut open = state.open.borrow_mut();
        let Some(photo) = open.as_mut() else { return };
        let Some(mask) = photo.document.mask_mut(index) else { return };
        let trimmed = name.trim();
        let wanted = (!trimmed.is_empty()).then(|| trimmed.to_string());
        if mask.name == wanted {
            return;
        }
        mask.name = wanted;
    }

    refresh_masks(state);
    schedule_history_push(state);
}

pub(super) fn duplicate_mask(state: &App, index: usize) {
    {
        let mut open = state.open.borrow_mut();
        let Some(photo) = open.as_mut() else { return };
        let mut masks = photo.document.masks();
        let Some(original) = masks.get(index).cloned() else { return };
        masks.insert(index + 1, original);
        photo.document.set_masks(masks);
        photo.view = None;
    }

    select_mask(state, Some(index + 1));
    refresh_masks(state);
    request_render(state);
    schedule_history_push(state);
}

pub(super) fn set_mask_inverted(state: &App, index: usize, inverted: bool) {
    {
        let mut open = state.open.borrow_mut();
        let Some(photo) = open.as_mut() else { return };
        let Some(mask) = photo.document.mask_mut(index) else { return };
        if mask.inverted == inverted {
            return;
        }
        mask.inverted = inverted;
        photo.view = None;
    }

    refresh_masks(state);
    request_render(state);
    state.mask_overlay.area.queue_draw();
    schedule_history_push(state);
}

pub(super) fn move_mask(state: &App, from: usize, to: usize) {
    if from == to {
        return;
    }

    let selected = state.mask_overlay.selected_mask.get();
    {
        let mut open = state.open.borrow_mut();
        let Some(photo) = open.as_mut() else { return };
        let mut masks = photo.document.masks();
        if from >= masks.len() || to > masks.len() {
            return;
        }
        let mask = masks.remove(from);
        masks.insert(to.min(masks.len()), mask);
        photo.document.set_masks(masks);
        photo.view = None;
    }

    state.mask_overlay.selected_mask.set(match selected {
        Some(at) if at == from => Some(to.min(index_limit(state))),
        Some(at) if at > from && at <= to => Some(at - 1),
        Some(at) if at < from && at >= to => Some(at + 1),
        other => other,
    });

    state.mask_overlay.sliders_hold.set(state.mask_overlay.selected_mask.get());

    refresh_masks(state);
    request_render(state);
    schedule_history_push(state);
}

pub(super) fn index_limit(state: &App) -> usize {
    state
        .open
        .borrow()
        .as_ref()
        .map(|photo| photo.document.masks().len().saturating_sub(1))
        .unwrap_or(0)
}

pub(super) fn toggle_mask_part(state: &App, index: usize, part: MaskPart) {
    {
        let mut open = state.open.borrow_mut();
        let Some(photo) = open.as_mut() else { return };
        let Some(mask) = photo.document.mask_mut(index) else { return };
        match part {
            MaskPart::Class(class) => {
                if let Some(at) = mask.muted.iter().position(|held| *held == class) {
                    mask.muted.remove(at);
                } else {

                    let Shape::Segment { classes } = &mask.shape else { return };
                    let live = classes.iter().filter(|c| !mask.muted.contains(c)).count();
                    if live <= 1 {
                        return;
                    }
                    mask.muted.push(class);
                }
            }
            MaskPart::Point(at) => match mask.points.get_mut(at) {
                Some(point) => point.enabled = !point.enabled,
                None => return,
            },
            MaskPart::Stroke { at, run } => {
                let Some(first) = mask.strokes.get(at) else { return };
                let wanted = !first.enabled;
                for stroke in mask.strokes.iter_mut().skip(at).take(run) {
                    stroke.enabled = wanted;
                }
            }
        }
        photo.view = None;
    }

    rebuild_mask_map(state, index);
    refresh_masks(state);
    request_render(state);
    show_coverage(state);
    schedule_history_push(state);
}

pub(super) fn remove_mask_part(state: &App, index: usize, part: MaskPart) {
    {
        let mut open = state.open.borrow_mut();
        let Some(photo) = open.as_mut() else { return };
        let Some(mask) = photo.document.mask_mut(index) else { return };
        match part {
            MaskPart::Class(class) => {
                let Shape::Segment { classes } = &mut mask.shape else { return };

                if classes.len() <= 1 {
                    return;
                }
                classes.retain(|held| *held != class);
            }
            MaskPart::Point(at) => {
                if at >= mask.points.len() {
                    return;
                }
                mask.points.remove(at);
            }
            MaskPart::Stroke { at, run } => {
                if at >= mask.strokes.len() {
                    return;
                }

                let end = (at + run).min(mask.strokes.len());
                mask.strokes.drain(at..end);
            }
        }
        photo.view = None;
    }

    rebuild_mask_map(state, index);
    refresh_masks(state);
    request_render(state);
    show_coverage(state);
    schedule_history_push(state);
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum MaskPart {
    Class(u16),
    Point(usize),

    Stroke { at: usize, run: usize },
}

pub(super) fn remove_mask(state: &App, index: usize) {
    busy_sync(state, "Removing the mask…", move |state| remove_mask_now(state, index))
}

pub(super) fn remove_mask_now(state: &App, index: usize) {
    {
        let mut open = state.open.borrow_mut();
        let Some(photo) = open.as_mut() else { return };
        let mut masks = photo.document.masks();
        if index >= masks.len() {
            return;
        }
        masks.remove(index);
        photo.document.set_masks(masks);
        photo.view = None;
    }

    select_mask(state, None);
    schedule_history_push(state);
}

pub(super) fn select_mask(state: &App, index: Option<usize>) {
    timed("select_mask", || select_mask_now(state, index))
}

pub(super) fn select_mask_now(state: &App, index: Option<usize>) {
    state.mask_overlay.selected_mask.set(index);

    let target = {
        let open = state.open.borrow();
        let Some(photo) = open.as_ref() else { return };
        match index.and_then(|index| photo.document.masks().get(index).cloned()) {
            Some(mask) => mask.basic,
            None => {

                state.mask_overlay.selected_mask.set(None);
                photo.document.basic()
            }
        }
    };

    let selected = state.mask_overlay.selected_mask.get();
    state.applying.set(true);
    state.sliders.write(target);
    state.mask_overlay.sliders_hold.set(selected);

    state.colour.mask_temperature.set_value(target.balance.temperature as f64);

    state.colour.mask_tint.set_value(-target.balance.tint as f64);

    state.light.curve_area.queue_draw();
    refresh_slider_marks(state);
    state.applying.set(false);

    if selected != state.mask_overlay.brush_owner.get() {
        state.mask_overlay.brush_owner.set(selected);
        state.masks.brush.set(MaskTool::Off);

        let settled = selected_mask(state).is_some_and(|mask| match mask.shape {
            Shape::Segment { .. } => true,
            Shape::Painted => !is_empty_painted(&mask),
            _ => false,
        });
        state.masks.looking.set(settled);
        state.mask_overlay.area.set_can_target(!settled);
    }

    refresh_masks(state);

    set_panel_scope(state);

    if selected.is_some() {
        show_panel_tab(state, "light");
        show_coverage(state);
    }

    state.mask_overlay.area.set_visible(state.mask_overlay.selected_mask.get().is_some());

    let picking = selected_mask(state)
        .is_some_and(|mask| matches!(mask.shape, Shape::ColourRange { .. } | Shape::LuminanceRange { .. }));
    let cursor = picking.then(|| {
        let fallback = gtk::gdk::Cursor::from_name("crosshair", None);
        gtk::gdk::Cursor::from_name("color-picker", fallback.as_ref())
    });
    state.mask_overlay.area.set_cursor(cursor.flatten().as_ref());

    state.mask_overlay.area.queue_draw();
    request_render(state);
}

pub(super) fn tint_mixer(state: &App) {
    let band = state.colour.mixer_band.get().min(BANDS.len() - 1);
    for scale in &state.colour.mixer_sliders {
        for other in 0..BANDS.len() {
            scale.remove_css_class(&format!("band-{other}"));
        }
        scale.add_css_class(&format!("band-{band}"));
    }
}
