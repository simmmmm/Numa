use gtk::{cairo, pango};
use numa::io::export::{Corner, Developed, Watermark};

pub(super) fn stamp(frame: &mut Developed, watermark: &Watermark, fallback: &str) {
    let text = match watermark.text.trim() {
        "" => fallback.trim(),
        text => text,
    };
    if !watermark.on || text.is_empty() || matches!(frame, Developed::Dng(..)) {
        return;
    }
    let (width, height) = frame.size();
    let Some((stamp, stamp_width, stamp_height)) = set(text, width.max(height) as f64 * watermark.size as f64 / 1000.0) else {
        return;
    };
    let inset = (width.min(height) as f64 / 40.0) as i64;
    let (left, top) = match watermark.corner {
        Corner::BottomRight => (width as i64 - stamp_width - inset, height as i64 - stamp_height - inset),
        Corner::BottomLeft => (inset, height as i64 - stamp_height - inset),
        Corner::TopRight => (width as i64 - stamp_width - inset, inset),
        Corner::TopLeft => (inset, inset),
    };

    let over = |x: i64, y: i64, blend: &mut dyn FnMut([f32; 4])| {
        if x < 0 || y < 0 || x >= stamp_width || y >= stamp_height {
            return;
        }
        let at = ((y * stamp_width + x) * 4) as usize;
        let [b, g, r, a] = [stamp[at], stamp[at + 1], stamp[at + 2], stamp[at + 3]].map(|v| v as f32 / 255.0);
        if a > 0.0 {
            blend([r, g, b, a]);
        }
    };
    let (x0, y0) = (left.max(0), top.max(0));
    let (x1, y1) = ((left + stamp_width).min(width as i64), (top + stamp_height).min(height as i64));
    for y in y0..y1 {
        for x in x0..x1 {
            match frame {
                Developed::Eight(image) | Developed::Hdr(image, _) => {
                    let pixel = image.get_pixel_mut(x as u32, y as u32);
                    over(x - left, y - top, &mut |[r, g, b, a]| {
                        for (channel, source) in pixel.0.iter_mut().zip([r, g, b]) {
                            *channel = ((source + *channel as f32 / 255.0 * (1.0 - a)) * 255.0).round().clamp(0.0, 255.0) as u8;
                        }
                    });
                }
                Developed::Sixteen(image) => {
                    let pixel = image.get_pixel_mut(x as u32, y as u32);
                    over(x - left, y - top, &mut |[r, g, b, a]| {
                        for (channel, source) in pixel.0.iter_mut().zip([r, g, b]) {
                            *channel = ((source + *channel as f32 / 65535.0 * (1.0 - a)) * 65535.0).round().clamp(0.0, 65535.0) as u16;
                        }
                    });
                }
                Developed::Dng(..) => return,
            }
        }
    }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_mark_goes_in_its_corner() {
        let blank = || Developed::Eight(image::RgbImage::from_pixel(400, 300, image::Rgb([20, 20, 20])));
        let mark = Watermark { on: true, text: "© Numa".into(), corner: Corner::BottomRight, size: 60 };
        let mut frame = blank();
        stamp(&mut frame, &mark, "");
        let Developed::Eight(image) = frame else { unreachable!() };
        let lit = |x0: u32, y0: u32| (x0..x0 + 200).flat_map(|x| (y0..y0 + 150).map(move |y| (x, y))).filter(|&(x, y)| image.get_pixel(x, y)[0] > 60).count();
        assert!(lit(200, 150) > 100, "no text in the bottom right");
        assert_eq!(lit(0, 0), 0, "text outside its corner");

        let mut untouched = blank();
        stamp(&mut untouched, &Watermark { text: String::new(), ..mark }, "");
        let Developed::Eight(image) = untouched else { unreachable!() };
        assert!(image.pixels().all(|pixel| pixel[0] == 20));
    }
}
