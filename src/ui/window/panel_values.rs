use super::*;

pub(super) fn tint_grading_hue(state: &App) {
    let idle = state.grade.grading_sliders[1].value() <= 0.0;
    let hue = &state.grade.grading_sliders[0];
    if idle {
        hue.add_css_class("idle");
    } else {
        hue.remove_css_class("idle");
    }
}

pub(super) fn write_grading(state: &App) {
    let grading = state
        .open
        .borrow()
        .as_ref()
        .map(|photo| photo.document.grading())
        .unwrap_or_default();
    let range = range_of(&grading, state.grade.grading_range.get());

    state.applying.set(true);
    state.grade.grading_sliders[0].set_value(range.hue as f64);
    state.grade.grading_sliders[1].set_value(range.saturation as f64);
    state.grade.grading_sliders[2].set_value(range.luminance as f64);
    state.grade.grading_shape[0].set_value(grading.blending as f64);
    state.grade.grading_shape[1].set_value(grading.balance as f64);
    state.applying.set(false);
    tint_grading_hue(state);
}

pub(super) fn read_grading(state: &App) {
    {
        let mut open = state.open.borrow_mut();
        let Some(photo) = open.as_mut() else { return };
        let mut grading = photo.document.grading();
        {
            let range = range_of_mut(&mut grading, state.grade.grading_range.get());
            range.hue = state.grade.grading_sliders[0].value() as f32;
            range.saturation = state.grade.grading_sliders[1].value() as f32;
            range.luminance = state.grade.grading_sliders[2].value() as f32;
        }
        grading.blending = state.grade.grading_shape[0].value() as f32;
        grading.balance = state.grade.grading_shape[1].value() as f32;
        photo.document.set_grading(grading);
    }

    request_render(state);
    schedule_history_push(state);
}

pub(super) fn range_of(grading: &Grading, which: usize) -> Range {
    match which {
        0 => grading.shadows,
        1 => grading.midtones,
        2 => grading.highlights,
        _ => grading.global,
    }
}

pub(super) fn range_of_mut(grading: &mut Grading, which: usize) -> &mut Range {
    match which {
        0 => &mut grading.shadows,
        1 => &mut grading.midtones,
        2 => &mut grading.highlights,
        _ => &mut grading.global,
    }
}

pub(super) fn write_mixer(state: &App) {
    let mixer = state.open.borrow().as_ref().map(|photo| photo.document.mixer());
    let mixer = mixer.unwrap_or_default();
    let band = state.colour.mixer_band.get().min(BANDS.len() - 1);

    state.applying.set(true);
    for (channel, scale) in state.colour.mixer_sliders.iter().enumerate() {

        let value = match (mixer.monochrome, channel) {
            (true, 2) => mixer.grey[band],
            _ => mixer.bands[band][channel],
        };
        scale.set_value(value as f64);
        if let Some(row) = scale.parent().filter(|_| channel < 2) {
            row.set_visible(!mixer.monochrome);
        }
    }
    state.colour.monochrome.set_active(mixer.monochrome);

    for scale in [&state.sliders.presence.vibrance, &state.sliders.presence.saturation] {
        if let Some(row) = scale.parent() {
            row.set_sensitive(!mixer.monochrome);
        }
    }
    state.applying.set(false);
}

pub(super) fn read_mixer(state: &App) {
    let band = state.colour.mixer_band.get().min(BANDS.len() - 1);
    {
        let mut open = state.open.borrow_mut();
        let Some(photo) = open.as_mut() else { return };
        let mut mixer = photo.document.mixer();
        match mixer.monochrome {
            true => mixer.grey[band] = state.colour.mixer_sliders[2].value() as f32,
            false => {
                for (channel, scale) in state.colour.mixer_sliders.iter().enumerate() {
                    mixer.bands[band][channel] = scale.value() as f32;
                }
            }
        }
        photo.document.set_mixer(mixer);

        photo.view = None;
    }

    request_render(state);
    schedule_history_push(state);
}

pub(super) fn disarm_pipettes(state: &App, keep: &str) {
    state.applying.set(true);
    for (name, button, flag) in [
        ("band", &state.colour.band_pipette, &state.colour.picking_band),
        ("white", &state.colour.white_pipette, &state.colour.picking_white),
        ("point", &state.colour.point_pipette, &state.colour.picking_point),
    ] {
        if name != keep {
            button.set_active(false);
            flag.set(false);
        }
    }
    state.applying.set(false);
}

pub(super) fn write_point_colours(state: &App) {
    let points = state.open.borrow().as_ref().map(|photo| photo.document.point_colours());
    let points = points.unwrap_or_default().points;
    let selected = state.colour.point_selected.get().min(points.len().saturating_sub(1));
    state.colour.point_selected.set(selected);

    while let Some(child) = state.colour.point_swatches.first_child() {
        state.colour.point_swatches.remove(&child);
    }
    let mut first: Option<gtk::ToggleButton> = None;
    for (index, point) in points.iter().enumerate() {
        let swatch = gtk::ToggleButton::new();
        swatch.set_tooltip_text(Some("Adjust this colour"));
        swatch.set_valign(gtk::Align::Center);
        let dot = gtk::DrawingArea::new();
        dot.set_content_width(16);
        dot.set_content_height(16);
        let colour = point.swatch().map(|linear| {
            let linear = linear.clamp(0.0, 1.0) as f64;
            if linear <= 0.003_130_8 { linear * 12.92 } else { 1.055 * linear.powf(1.0 / 2.4) - 0.055 }
        });
        dot.set_draw_func(move |_, context, width, height| {
            context.set_source_rgb(colour[0], colour[1], colour[2]);
            let radius = width.min(height) as f64 / 2.0;
            context.arc(width as f64 / 2.0, height as f64 / 2.0, radius, 0.0, std::f64::consts::TAU);
            let _ = context.fill();
        });
        swatch.set_child(Some(&dot));
        match &first {
            None => first = Some(swatch.clone()),
            Some(first) => swatch.set_group(Some(first)),
        }
        swatch.set_active(index == selected);
        swatch.connect_toggled(glib::clone!(
            #[strong] state,
            move |swatch| {
                if swatch.is_active() && state.colour.point_selected.replace(index) != index {
                    write_point_colours(&state);
                }
            }
        ));
        state.colour.point_swatches.append(&swatch);
    }

    state.colour.point_controls.set_visible(!points.is_empty());
    let point = points.get(selected).copied().unwrap_or_default();
    state.applying.set(true);
    if points.is_empty() {
        state.colour.point_show.set_active(false);
    }
    for (scale, value) in state
        .colour
        .point_sliders
        .iter()
        .zip([point.hue, point.saturation, point.luminance, point.range])
    {
        scale.set_value(value as f64);
    }
    state.applying.set(false);

    if state.colour.point_show.is_active() {
        request_render(state);
    }
}

pub(super) fn read_point_colours(state: &App) {
    {
        let mut open = state.open.borrow_mut();
        let Some(photo) = open.as_mut() else { return };
        let mut points = photo.document.point_colours();
        let Some(point) = points.points.get_mut(state.colour.point_selected.get()) else { return };
        let value = |index: usize| state.colour.point_sliders[index].value() as f32;
        point.hue = value(0);
        point.saturation = value(1);
        point.luminance = value(2);
        point.range = value(3);
        photo.document.set_point_colours(points);
    }
    request_render(state);
    schedule_history_push(state);
}

pub(super) fn pick_point(state: &App, u: f32, v: f32) {
    disarm_pipettes(state, "");
    arm_band_pipette(state);

    let Some(frame) = mask_frame(state) else { return };
    let (width, height) = (frame.width() as i64, frame.height() as i64);
    let (cx, cy) = ((u.clamp(0.0, 1.0) * width as f32) as i64, (v.clamp(0.0, 1.0) * height as f32) as i64);
    let decode = |byte: u8| {
        let value = byte as f32 / 255.0;
        if value <= 0.040_45 { value / 12.92 } else { ((value + 0.055) / 1.055).powf(2.4) }
    };
    let mut sum = [0.0f32; 3];
    let mut count = 0.0;
    for y in (cy - 2).max(0)..(cy + 3).min(height) {
        for x in (cx - 2).max(0)..(cx + 3).min(width) {
            let pixel = frame.get_pixel(x as u32, y as u32).0;
            for channel in 0..3 {
                sum[channel] += decode(pixel[channel]);
            }
            count += 1.0;
        }
    }
    if count == 0.0 {
        return;
    }

    {
        let mut open = state.open.borrow_mut();
        let Some(photo) = open.as_mut() else { return };
        let mut points = photo.document.point_colours();
        points.points.push(PointColour::picked(sum.map(|value| value / count)));
        state.colour.point_selected.set(points.points.len() - 1);
        photo.document.set_point_colours(points);
    }
    write_point_colours(state);
    request_render(state);
    schedule_history_push(state);
}
