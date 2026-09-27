use super::*;

pub(super) fn mixer_in(state: &App, document: &Document) -> Mixer {
    match state.mask_overlay.selected_mask.get().and_then(|index| document.masks().get(index).cloned()) {
        Some(mask) => mask.mixer,
        None => document.mixer(),
    }
}

pub(super) fn set_mixer_in(state: &App, document: &mut Document, mixer: Mixer) {
    match state.mask_overlay.selected_mask.get() {
        Some(index) => in_mask(document, index, |mask| mask.mixer = mixer),
        None => document.set_mixer(mixer),
    }
}

pub(super) fn grading_in(state: &App, document: &Document) -> Grading {
    match state.mask_overlay.selected_mask.get().and_then(|index| document.masks().get(index).cloned()) {
        Some(mask) => mask.grading,
        None => document.grading(),
    }
}

pub(super) fn set_grading_in(state: &App, document: &mut Document, grading: Grading) {
    match state.mask_overlay.selected_mask.get() {
        Some(index) => in_mask(document, index, |mask| mask.grading = grading),
        None => document.set_grading(grading),
    }
}

pub(super) fn points_in(state: &App, document: &Document) -> numa::core::point::PointColours {
    match state.mask_overlay.selected_mask.get().and_then(|index| document.masks().get(index).cloned()) {
        Some(mask) => mask.point_colours,
        None => document.point_colours(),
    }
}

pub(super) fn set_points_in(state: &App, document: &mut Document, points: numa::core::point::PointColours) {
    match state.mask_overlay.selected_mask.get() {
        Some(index) => in_mask(document, index, |mask| mask.point_colours = points),
        None => document.set_point_colours(points),
    }
}

fn in_mask(document: &mut Document, index: usize, change: impl FnOnce(&mut Mask)) {
    let mut masks = document.masks();
    if let Some(mask) = masks.get_mut(index) {
        change(mask);
        document.set_masks(masks);
    }
}

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
        .map(|photo| grading_in(state, &photo.document))
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
        let mut grading = grading_in(state, &photo.document);
        {
            let range = range_of_mut(&mut grading, state.grade.grading_range.get());
            range.hue = state.grade.grading_sliders[0].value() as f32;
            range.saturation = state.grade.grading_sliders[1].value() as f32;
            range.luminance = state.grade.grading_sliders[2].value() as f32;
        }
        grading.blending = state.grade.grading_shape[0].value() as f32;
        grading.balance = state.grade.grading_shape[1].value() as f32;
        set_grading_in(state, &mut photo.document, grading);
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
    let mixer = state.open.borrow().as_ref().map(|photo| mixer_in(state, &photo.document));
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
        let mut mixer = mixer_in(state, &photo.document);
        match mixer.monochrome {
            true => mixer.grey[band] = state.colour.mixer_sliders[2].value() as f32,
            false => {
                for (channel, scale) in state.colour.mixer_sliders.iter().enumerate() {
                    mixer.bands[band][channel] = scale.value() as f32;
                }
            }
        }
        set_mixer_in(state, &mut photo.document, mixer);
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
    let points = state.open.borrow().as_ref().map(|photo| points_in(state, &photo.document));
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
    tint_point_hue(&point);
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
        let mut points = points_in(state, &photo.document);
        let Some(point) = points.points.get_mut(state.colour.point_selected.get()) else { return };
        let value = |index: usize| state.colour.point_sliders[index].value() as f32;
        point.hue = value(0);
        point.saturation = value(1);
        point.luminance = value(2);
        point.range = value(3);
        set_points_in(state, &mut photo.document, points);
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
        let mut points = points_in(state, &photo.document);
        points.points.push(PointColour::picked(sum.map(|value| value / count)));
        state.colour.point_selected.set(points.points.len() - 1);
        set_points_in(state, &mut photo.document, points);
    }
    write_point_colours(state);
    request_render(state);
    schedule_history_push(state);
    show_picked_point(state);
}

fn show_picked_point(state: &App) {
    let section = state.colour.point_controls.clone();
    glib::idle_add_local_once(move || {
        let Some(scroller) = section.ancestor(gtk::ScrolledWindow::static_type()).and_downcast::<gtk::ScrolledWindow>() else { return };
        if let Some(child) = scroller.child() {
            if let Some(bounds) = section.compute_bounds(&child) {
                let adjustment = scroller.vadjustment();
                let (top, bottom) = (bounds.y() as f64, (bounds.y() + bounds.height()) as f64);
                let (shown, page) = (adjustment.value(), adjustment.page_size());
                if top < shown || bottom > shown + page {
                    adjustment.set_value((top - 24.0).max(0.0));
                }
            }
        }
        section.add_css_class("point-flash");
        glib::timeout_add_local_once(std::time::Duration::from_millis(900), move || {
            section.remove_css_class("point-flash");
        });
    });
}

fn tint_point_hue(point: &PointColour) {
    thread_local! {
        static PROVIDER: gtk::CssProvider = {
            let provider = gtk::CssProvider::new();
            if let Some(display) = gtk::gdk::Display::default() {
                gtk::style_context_add_provider_for_display(&display, &provider, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION + 1);
            }
            provider
        };
    }
    let rgb = point.swatch().map(|linear| linear.clamp(0.0, 1.0));
    let (max, min) = (rgb.iter().cloned().fold(0.0f32, f32::max), rgb.iter().cloned().fold(1.0f32, f32::min));
    let delta = max - min;
    let hue = if delta <= 0.0 {
        0.0
    } else if max == rgb[0] {
        60.0 * ((rgb[1] - rgb[2]) / delta).rem_euclid(6.0)
    } else if max == rgb[1] {
        60.0 * ((rgb[2] - rgb[0]) / delta + 2.0)
    } else {
        60.0 * ((rgb[0] - rgb[1]) / delta + 4.0)
    };
    let css = |degrees: f32| {
        let h = degrees.rem_euclid(360.0) / 60.0;
        let x = 1.0 - (h.rem_euclid(2.0) - 1.0).abs();
        let (r, g, b) = match h as u32 {
            0 => (1.0, x, 0.0),
            1 => (x, 1.0, 0.0),
            2 => (0.0, 1.0, x),
            3 => (0.0, x, 1.0),
            4 => (x, 0.0, 1.0),
            _ => (1.0, 0.0, x),
        };
        format!("rgb({},{},{})", (r * 255.0) as u8, (g * 255.0) as u8, (b * 255.0) as u8)
    };
    let rule = format!(
        ".adjustment scale.point-hue trough {{ background: linear-gradient(to right, {}, {}, {}); }}",
        css(hue - 30.0),
        css(hue),
        css(hue + 30.0)
    );
    PROVIDER.with(|provider| provider.load_from_string(&rule));
}
