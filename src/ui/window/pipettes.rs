use super::*;

pub(super) fn arm_band_pipette(state: &App) {
    let armed = state.colour.picking_band.get() || state.colour.picking_white.get() || state.colour.picking_point.get();
    state.canvas.set_cursor(armed.then(pipette_cursor).as_ref());
}

pub(super) fn pipette_cursor() -> gtk::gdk::Cursor {
    use gtk::cairo;
    const SIZE: i32 = 32;
    let fallback = gtk::gdk::Cursor::from_name("crosshair", None);
    let Ok(mut surface) = cairo::ImageSurface::create(cairo::Format::ARgb32, SIZE, SIZE) else {
        return fallback.unwrap_or_else(|| gtk::gdk::Cursor::from_name("default", None).expect("a default cursor"));
    };
    if let Ok(context) = cairo::Context::new(&surface) {
        context.set_line_cap(cairo::LineCap::Round);

        let body = |context: &cairo::Context, width: f64| {
            context.set_line_width(width);
            context.move_to(3.0, 29.0);
            context.line_to(20.0, 12.0);
            let _ = context.stroke();
            context.arc(23.5, 8.5, width * 0.9 + 1.5, 0.0, std::f64::consts::TAU);
            let _ = context.fill();
        };
        context.set_source_rgba(1.0, 1.0, 1.0, 0.95);
        body(&context, 6.0);
        context.set_source_rgba(0.12, 0.12, 0.14, 1.0);
        body(&context, 3.0);

        context.set_line_width(2.0);
        context.move_to(16.0, 11.0);
        context.line_to(21.0, 16.0);
        let _ = context.stroke();
    }
    surface.flush();
    let (width, height, stride) = (surface.width(), surface.height(), surface.stride() as usize);
    let Ok(data) = surface.data() else { return fallback.expect("crosshair") };

    let texture = gtk::gdk::MemoryTexture::new(
        width,
        height,
        gtk::gdk::MemoryFormat::B8g8r8a8Premultiplied,
        &glib::Bytes::from(&data[..]),
        stride,
    );
    gtk::gdk::Cursor::from_texture(&texture, 3, 29, fallback.as_ref())
}

pub(super) fn pick_white(state: &App, u: f32, v: f32) {
    state.applying.set(true);
    state.colour.white_pipette.set_active(false);
    state.applying.set(false);
    state.colour.picking_white.set(false);
    arm_band_pipette(state);

    let Some(frame) = mask_frame(state) else { return };
    let Some(profile) = state.open.borrow().as_ref().and_then(|photo| photo.proxy.profile) else {
        state.toast("This photograph has no camera white balance to set");
        return;
    };
    let (width, height) = (frame.width() as i64, frame.height() as i64);
    let (cx, cy) = ((u.clamp(0.0, 1.0) * width as f32) as i64, (v.clamp(0.0, 1.0) * height as f32) as i64);

    let decode = |byte: u8| numa::core::tone::scene_value_for(byte as f32 / 255.0);
    let mut sum = [0.0f32; 3];
    let mut count = 0.0;
    for y in (cy - 2).max(0)..(cy + 3).min(height) {
        for x in (cx - 2).max(0)..(cx + 3).min(width) {
            let pixel = frame.get_pixel(x as u32, y as u32).0;

            if pixel.iter().any(|channel| *channel >= 250) {
                continue;
            }
            for channel in 0..3 {
                sum[channel] += decode(pixel[channel]);
            }
            count += 1.0;
        }
    }
    let balance = (count > 0.0).then(|| profile.neutral_balance(sum.map(|value| value / count))).flatten();
    let Some(balance) = balance else {
        state.toast("Nothing to measure there — pick something grey that is not blown out");
        return;
    };
    state.sliders.write_white_balance(WhiteBalance {
        temperature: balance.temperature.clamp(2000.0, 15000.0),
        tint: balance.tint,
    });
}

pub(super) fn pick_band(state: &App, u: f32, v: f32) {
    let Some(frame) = mask_frame(state) else { return };
    let x = ((u.clamp(0.0, 1.0) * frame.width() as f32) as u32).min(frame.width() - 1);
    let y = ((v.clamp(0.0, 1.0) * frame.height() as f32) as u32).min(frame.height() - 1);
    let pixel = frame.get_pixel(x, y).0;
    let [hue, saturation, value] = numa::core::profile::rgb_to_hsv([
        pixel[0] as f32 / 255.0,
        pixel[1] as f32 / 255.0,
        pixel[2] as f32 / 255.0,
    ]);

    state.applying.set(true);
    state.colour.band_pipette.set_active(false);
    state.applying.set(false);
    state.colour.picking_band.set(false);
    arm_band_pipette(state);

    if saturation < 0.08 || value < 0.04 {
        state.toast("That pixel has no colour to pick — try something less grey");
        return;
    }

    let degrees = hue * 60.0;
    let (band, name) = BANDS
        .iter()
        .enumerate()
        .min_by(|(_, (_, a)), (_, (_, b))| {
            let apart = |centre: f32| {
                let gap = (degrees - centre).rem_euclid(360.0);
                gap.min(360.0 - gap)
            };
            apart(*a).total_cmp(&apart(*b))
        })
        .map(|(index, (name, _))| (index, *name))
        .unwrap_or((0, "Red"));

    state.colour.mixer_band.set(band);
    state.applying.set(true);
    if let Some(button) = state.colour.mixer_swatches.borrow().get(band) {
        button.set_active(true);
    }
    state.applying.set(false);
    tint_mixer(state);
    write_mixer(state);
    state.toast(&format!("{name} — the sliders are about that now"));
}

pub(super) fn hue_rgb(degrees: f64) -> (f64, f64, f64) {
    let h = degrees.rem_euclid(360.0) / 60.0;
    let x = 1.0 - (h % 2.0 - 1.0).abs();
    match h as u32 {
        0 => (1.0, x, 0.0),
        1 => (x, 1.0, 0.0),
        2 => (0.0, 1.0, x),
        3 => (0.0, x, 1.0),
        4 => (x, 0.0, 1.0),
        _ => (1.0, 0.0, x),
    }
}

pub(super) fn rounded(context: &gtk::cairo::Context, x: f64, y: f64, width: f64, height: f64, radius: f64) {
    use std::f64::consts::PI;
    context.new_sub_path();
    context.arc(x + width - radius, y + radius, radius, -PI / 2.0, 0.0);
    context.arc(x + width - radius, y + height - radius, radius, 0.0, PI / 2.0);
    context.arc(x + radius, y + height - radius, radius, PI / 2.0, PI);
    context.arc(x + radius, y + radius, radius, PI, 1.5 * PI);
    context.close_path();
}
