use image::RgbImage;

use crate::core::mask::Alpha;
use crate::core::profile::rgb_to_hsv;

const FALLOFF: f32 = 0.5;

pub fn luminance(frame: &RgbImage, low: f32, high: f32, softness: f32, width: usize, height: usize) -> Alpha {
    let (low, high) = (low.min(high), low.max(high));
    let edge = (softness.clamp(0.0, 1.0) * FALLOFF).max(1e-3);

    sample_into(frame, width, height, |pixel| {
        let luma = 0.2126 * pixel[0] + 0.7152 * pixel[1] + 0.0722 * pixel[2];

        let below = ramp((luma - (low - edge)) / edge);
        let above = ramp(((high + edge) - luma) / edge);
        below.min(above)
    })
}

pub fn colour(
    frame: &RgbImage,
    hue: f32,
    spread: f32,
    saturation: f32,
    width: usize,
    height: usize,
) -> Alpha {
    let wanted = hue.rem_euclid(360.0) / 60.0;
    let spread = (spread.clamp(0.0, 180.0) / 60.0).max(1e-3);
    let floor = saturation.clamp(0.0, 1.0);

    sample_into(frame, width, height, |pixel| {
        let [pixel_hue, pixel_saturation, value] = rgb_to_hsv(pixel);

        let apart = (pixel_hue - wanted).rem_euclid(6.0);
        let apart = apart.min(6.0 - apart);
        let matched = ramp((spread * (1.0 + FALLOFF) - apart) / (spread * FALLOFF).max(1e-4));

        let saturated = ramp((pixel_saturation - floor) / 0.12_f32.max(1e-4));
        let lit = ramp(value / 0.06);
        matched * saturated * lit
    })
}

fn sample_into(
    frame: &RgbImage,
    width: usize,
    height: usize,
    of: impl Fn([f32; 3]) -> f32 + Sync,
) -> Alpha {
    use rayon::prelude::*;

    let mut data = vec![0.0f32; width * height];
    if width == 0 || height == 0 || frame.width() == 0 || frame.height() == 0 {
        return Alpha::new(width, height, data);
    }

    data.par_chunks_mut(width).enumerate().for_each(|(y, row)| {
        let sy = ((y as f32 + 0.5) / height as f32 * frame.height() as f32) as u32;
        let sy = sy.min(frame.height() - 1);
        for (x, cell) in row.iter_mut().enumerate() {
            let sx = ((x as f32 + 0.5) / width as f32 * frame.width() as f32) as u32;
            let sx = sx.min(frame.width() - 1);
            let pixel = frame.get_pixel(sx, sy).0;
            *cell = of([
                pixel[0] as f32 / 255.0,
                pixel[1] as f32 / 255.0,
                pixel[2] as f32 / 255.0,
            ]);
        }
    });

    Alpha::new(width, height, data)
}

fn ramp(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bands() -> RgbImage {
        RgbImage::from_fn(64, 8, |x, _| match x / 16 {
            0 => image::Rgb([8, 8, 8]),
            1 => image::Rgb([128, 128, 128]),
            2 => image::Rgb([246, 246, 246]),
            _ => image::Rgb([200, 40, 40]),
        })
    }

    #[test]
    fn a_luminance_range_takes_the_band_it_names() {
        let alpha = luminance(&bands(), 0.35, 0.65, 0.2, 64, 8);
        let at = |x: usize| alpha.data[4 * 64 + x];

        assert!(at(24) > 0.95, "the mid grey is in: {}", at(24));
        assert!(at(8) < 0.05, "the black is out: {}", at(8));
        assert!(at(40) < 0.05, "the white is out: {}", at(40));
    }

    #[test]
    fn a_colour_range_takes_the_colour_and_not_the_greys() {
        let alpha = colour(&bands(), 0.0, 30.0, 0.25, 64, 8);
        let at = |x: usize| alpha.data[4 * 64 + x];

        assert!(at(56) > 0.9, "the red is in: {}", at(56));
        for (name, x) in [("black", 8), ("grey", 24), ("white", 40)] {
            assert!(at(x) < 0.02, "the {name} is out: {}", at(x));
        }

        let blue = colour(&bands(), 220.0, 30.0, 0.25, 64, 8);
        assert!(blue.data[4 * 64 + 56] < 0.02, "red is not blue");
    }

    #[test]
    fn a_red_range_wraps_around_the_end_of_the_wheel() {
        let warm = RgbImage::from_fn(8, 8, |x, _| match x < 4 {

            true => image::Rgb([220, 60, 40]),
            false => image::Rgb([220, 40, 60]),
        });
        let alpha = colour(&warm, 0.0, 25.0, 0.25, 8, 8);
        assert!(alpha.data[4 * 8 + 1] > 0.9, "the one above zero");
        assert!(alpha.data[4 * 8 + 6] > 0.9, "and the one below three sixty");
    }
}
