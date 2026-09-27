use gtk::{cairo, pango};
use numa::io::export::{Developed, Watermark};

pub(super) fn stamp(frame: &mut Developed, watermark: &Watermark, fallback: &str) {
    numa::io::export::stamp(frame, watermark, fallback, set);
}

fn set(text: &str, size: f64) -> Option<(Vec<u8>, i64, i64)> {
    let size = size.max(6.0);
    let measure = cairo::Context::new(cairo::ImageSurface::create(cairo::Format::ARgb32, 1, 1).ok()?).ok()?;
    let layout = pangocairo::functions::create_layout(&measure);
    let mut font = pango::FontDescription::from_string("Sans");
    font.set_absolute_size(size * pango::SCALE as f64);
    layout.set_font_description(Some(&font));
    layout.set_text(text);
    let (_, logical) = layout.pixel_extents();
    let shadow = (size / 18.0).max(1.0);
    let pad = shadow.ceil() as i32 + 1;
    let (width, height) = (logical.width() + 2 * pad, logical.height() + 2 * pad);
    let mut surface = cairo::ImageSurface::create(cairo::Format::ARgb32, width, height).ok()?;
    {
        let context = cairo::Context::new(&surface).ok()?;
        let layout = pangocairo::functions::create_layout(&context);
        layout.set_font_description(Some(&font));
        layout.set_text(text);
        context.set_source_rgba(0.0, 0.0, 0.0, 0.35);
        context.move_to(pad as f64 - logical.x() as f64 + shadow, pad as f64 - logical.y() as f64 + shadow);
        pangocairo::functions::show_layout(&context, &layout);
        context.set_source_rgba(1.0, 1.0, 1.0, 0.7);
        context.move_to(pad as f64 - logical.x() as f64, pad as f64 - logical.y() as f64);
        pangocairo::functions::show_layout(&context, &layout);
    }
    surface.flush();
    let stride = surface.stride() as usize;
    let data = surface.data().ok()?;
    let row = width as usize * 4;
    let pixels = (0..height as usize).flat_map(|y| data[y * stride..y * stride + row].to_vec()).collect();
    Some((pixels, width as i64, height as i64))
}
