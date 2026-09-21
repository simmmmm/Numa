use rayon::prelude::*;

use numa_core::document::Basic;

use super::local::{blur, guided, Plane};

pub fn passes(data: &mut [f32], width: usize, height: usize, basic: &Basic, scale: f32) {
    denoise(
        data,
        width,
        height,
        basic.detail.denoise_luma / 100.0,
        basic.detail.denoise_detail / 100.0,
        basic.detail.denoise_contrast / 100.0,
        scale,
    );
    denoise_colour(data, width, height, basic.detail.denoise_colour / 100.0, scale);
    sharpen(
        data,
        width,
        height,
        basic.detail.sharpen / 100.0,
        basic.detail.sharpen_radius,
        basic.detail.sharpen_masking / 100.0,
        scale,
    );
    defringe(data, width, height, basic.detail.defringe / 100.0, scale);
    moire(data, width, height, basic.detail.moire / 100.0, scale);
}

const MIN_RADIUS: f32 = 0.5;

const MAX_GAIN: f32 = 2.0;

fn noise_floor(amount: f32) -> f32 {
    0.0005 + amount * 0.02
}

#[allow(clippy::too_many_arguments)]
pub fn denoise(
    data: &mut [f32],
    width: usize,
    height: usize,
    luminance: f32,
    detail: f32,
    contrast: f32,
    scale: f32,
) {
    if width == 0 || height == 0 {
        return;
    }

    let luminance = luminance.clamp(0.0, 1.0);

    if luminance > 0.0 {

        let radius = (2.0 * scale).round().max(1.0) as usize;
        if 2.0 * scale >= MIN_RADIUS {
            let log = log_luminance(data, width, height);

            let floor = noise_floor(luminance) * 4.0f32.powf(1.0 - 2.0 * detail.clamp(0.0, 1.0));
            let smoothed = guided(&log, radius, floor);

            let contrast = contrast.clamp(0.0, 1.0);
            let coarse = (contrast > 0.0).then(|| {
                let removed = Plane::new(
                    width,
                    height,
                    log.data.iter().zip(&smoothed.data).map(|(before, after)| before - after).collect(),
                );
                blur(&removed, radius * 2)
            });

            data.par_chunks_exact_mut(3).enumerate().for_each(|(index, pixel)| {
                let back = coarse.as_ref().map_or(0.0, |coarse| coarse.data[index] * contrast);
                let gain = ((smoothed.data[index] + back - log.data[index]) * luminance).exp2();
                for channel in pixel.iter_mut() {
                    *channel = (*channel * gain).max(0.0);
                }
            });
        }
    }

}

pub fn denoise_colour(data: &mut [f32], width: usize, height: usize, colour: f32, scale: f32) {
    if width == 0 || height == 0 {
        return;
    }
    let colour = colour.clamp(0.0, 1.0);
    if colour > 0.0 {
        let radius = (4.0 * scale).round().max(1.0) as usize;
        if 4.0 * scale >= MIN_RADIUS {
            blur_colour(data, width, height, radius, colour);
        }
    }
}

const FRINGE_EDGE: f32 = 0.10;

const FRINGE_CAST: f32 = 0.06;

pub fn defringe(data: &mut [f32], width: usize, height: usize, amount: f32, scale: f32) {
    let amount = amount.clamp(0.0, 1.0);
    if amount <= 0.0 || width < 3 || height < 3 {
        return;
    }

    let luma = log_luminance(data, width, height);
    let radius = ((2.0 * scale).round() as usize).max(1);
    let soft = blur(&luma, radius);

    let strength: Vec<f32> = (0..width * height)
        .map(|index| {
            let (x, y) = (index % width, index / width);
            let at = |dx: isize, dy: isize| {
                let cx = (x as isize + dx).clamp(0, width as isize - 1) as usize;
                let cy = (y as isize + dy).clamp(0, height as isize - 1) as usize;
                soft.data[cy * width + cx]
            };

            let gx = at(1, 0) - at(-1, 0);
            let gy = at(0, 1) - at(0, -1);
            gx.hypot(gy)
        })
        .collect();

    data.par_chunks_exact_mut(3).enumerate().for_each(|(index, pixel)| {

        let edge = ((strength[index] - FRINGE_EDGE) / FRINGE_EDGE).clamp(0.0, 1.0);
        if edge <= 0.0 {
            return;
        }

        let (red, green, blue) = (pixel[0], pixel[1], pixel[2]);
        let reference = (red.max(green).max(blue)).max(1e-5);

        let purple = (red.min(blue) - green) / reference;
        let greenish = (green - red.max(blue)) / reference;
        let (cast, purple_side) = match purple >= greenish {
            true => (purple, true),
            false => (greenish, false),
        };
        let found = ((cast - FRINGE_CAST) / FRINGE_CAST).clamp(0.0, 1.0);
        if found <= 0.0 {
            return;
        }

        let pull = edge * found * amount;
        let neutral = match purple_side {

            true => [green, green, green],

            false => {
                let level = (red + blue) * 0.5;
                [red, level, blue]
            }
        };

        let before = 0.2126 * red + 0.7152 * green + 0.0722 * blue;
        for (channel, target) in pixel.iter_mut().zip(neutral) {
            *channel += (target - *channel) * pull;
        }

        let after = 0.2126 * pixel[0] + 0.7152 * pixel[1] + 0.0722 * pixel[2];
        if after > 1e-6 {
            let gain = before / after;
            for channel in pixel.iter_mut() {
                *channel = (*channel * gain).max(0.0);
            }
        }
    });
}

const MOIRE_WOBBLE: f32 = 0.05;
const MOIRE_TRANSITION: f32 = 1.6;

pub fn moire(data: &mut [f32], width: usize, height: usize, amount: f32, scale: f32) {
    let amount = amount.clamp(0.0, 1.0);
    if amount <= 0.0 || width < 3 || height < 3 {
        return;
    }

    let luma: Vec<f32> = data
        .par_chunks_exact(3)
        .map(|pixel| (0.2126 * pixel[0] + 0.7152 * pixel[1] + 0.0722 * pixel[2]).max(1e-5))
        .collect();

    let chroma: Vec<Plane> = (0..3)
        .map(|channel| {
            Plane::new(
                width,
                height,
                data.par_chunks_exact(3)
                    .zip(luma.par_iter())
                    .map(|(pixel, bright)| pixel[channel] / bright)
                    .collect(),
            )
        })
        .collect();

    let near = ((2.0 * scale).round() as usize).max(1);
    let far = ((10.0 * scale).round() as usize).max(near + 1);
    let close: Vec<Plane> = chroma.iter().map(|plane| blur(plane, near)).collect();
    let wide: Vec<Plane> = chroma.iter().map(|plane| blur(plane, far)).collect();

    data.par_chunks_exact_mut(3).enumerate().for_each(|(index, pixel)| {
        let wobble: f32 = (0..3)
            .map(|k| (chroma[k].data[index] - close[k].data[index]).abs())
            .sum();
        let transition: f32 = (0..3)
            .map(|k| (close[k].data[index] - wide[k].data[index]).abs())
            .sum();

        let beyond = wobble - transition * MOIRE_TRANSITION;
        let found = ((beyond - MOIRE_WOBBLE) / MOIRE_WOBBLE).clamp(0.0, 1.0);
        if found <= 0.0 {
            return;
        }

        let pull = found * amount;
        let bright = luma[index];
        for (channel, value) in pixel.iter_mut().enumerate() {
            let settled = wide[channel].data[index] * bright;
            *value = (*value + (settled - *value) * pull).max(0.0);
        }
    });
}

pub fn sharpen(
    data: &mut [f32],
    width: usize,
    height: usize,
    amount: f32,
    radius: f32,
    threshold: f32,
    scale: f32,
) {
    let amount = amount.clamp(0.0, 1.0);
    if amount == 0.0 || width == 0 || height == 0 {
        return;
    }

    let scaled = radius * scale;
    if scaled < MIN_RADIUS {

        return;
    }

    let log = log_luminance(data, width, height);

    let (below, t) = (scaled.floor(), scaled.fract());
    let base = if t < 1e-3 {
        blur(&log, below as usize)
    } else {
        let blurred;
        let lower = if below < 1.0 {
            &log
        } else {
            blurred = blur(&log, below as usize);
            &blurred
        };
        let upper = blur(&log, below as usize + 1);
        let mixed = lower.data.par_iter().zip(&upper.data).map(|(a, b)| a + (b - a) * t).collect();
        Plane::new(log.width, log.height, mixed)
    };

    let floor = threshold.clamp(0.0, 1.0) * 0.5;

    data.par_chunks_exact_mut(3).enumerate().for_each(|(index, pixel)| {
        let detail = log.data[index] - base.data[index];

        let weight = if floor <= 0.0 {
            1.0
        } else {
            (detail.abs() / floor).min(1.0).powi(2)
        };

        let gain = (amount * detail * weight).exp2().clamp(1.0 / MAX_GAIN, MAX_GAIN);
        for channel in pixel.iter_mut() {
            *channel = (*channel * gain).max(0.0);
        }
    });
}

fn log_luminance(data: &[f32], width: usize, height: usize) -> Plane {
    let values: Vec<f32> = data
        .par_chunks_exact(3)
        .map(|pixel| {
            let luma = 0.2126 * pixel[0] + 0.7152 * pixel[1] + 0.0722 * pixel[2];
            luma.max(1e-5).log2()
        })
        .collect();

    Plane::new(width, height, values)
}

fn blur_colour(data: &mut [f32], width: usize, height: usize, radius: usize, amount: f32) {
    let plane_of = |channel: usize| {
        Plane::new(
            width,
            height,
            data.par_chunks_exact(3).map(|pixel| pixel[channel]).collect(),
        )
    };
    let blurred: Vec<Plane> = (0..3).map(|channel| blur(&plane_of(channel), radius)).collect();

    data.par_chunks_exact_mut(3).enumerate().for_each(|(index, pixel)| {
        let smooth = [blurred[0].data[index], blurred[1].data[index], blurred[2].data[index]];
        let luma = 0.2126 * pixel[0] + 0.7152 * pixel[1] + 0.0722 * pixel[2];
        let smooth_luma = 0.2126 * smooth[0] + 0.7152 * smooth[1] + 0.0722 * smooth[2];

        if smooth_luma <= 1e-6 {
            return;
        }
        let rescale = luma / smooth_luma;

        for (channel, value) in pixel.iter_mut().enumerate() {
            let recoloured = smooth[channel] * rescale;
            *value = (*value + (recoloured - *value) * amount).max(0.0);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colour_that_wobbles_is_taken_out_and_colour_that_steps_is_not() {
        let (w, h) = (96usize, 24usize);
        let mut data = vec![0.0f32; w * h * 3];
        for y in 0..h {
            for x in 0..w {
                let at = (y * w + x) * 3;
                let pixel = if x < 40 {

                    match x % 2 == 0 {
                        true => [0.55, 0.45, 0.40],
                        false => [0.40, 0.45, 0.55],
                    }
                } else if x < 60 {
                    [0.47, 0.47, 0.47]
                } else {

                    [0.20, 0.60, 0.25]
                };
                data[at..at + 3].copy_from_slice(&pixel);
            }
        }

        let before = data.clone();
        moire(&mut data, w, h, 1.0, 1.0);

        let swing = |source: &[f32], from: usize, to: usize| {
            (from..to)
                .map(|x| {
                    let a = (24 / 2 * w + x) * 3;
                    let b = (24 / 2 * w + x + 1) * 3;
                    (0..3).map(|k| (source[a + k] - source[b + k]).abs()).sum::<f32>()
                })
                .fold(0.0f32, f32::max)
        };

        assert!(
            swing(&data, 10, 35) < swing(&before, 10, 35) * 0.3,
            "the wobble settled: {} was {}",
            swing(&data, 10, 35),
            swing(&before, 10, 35)
        );

        let green = (12 * w + 80) * 3;
        for channel in 0..3 {
            assert!(
                (data[green + channel] - before[green + channel]).abs() < 0.02,
                "the edge kept its colour at channel {channel}"
            );
        }

        for index in 0..w * h {
            let at = index * 3;
            let luma = |source: &[f32]| {
                0.2126 * source[at] + 0.7152 * source[at + 1] + 0.0722 * source[at + 2]
            };
            assert!((luma(&data) - luma(&before)).abs() < 1e-3, "brightness moved at {index}");
        }

        let mut untouched = before.clone();
        moire(&mut untouched, w, h, 0.0, 1.0);
        assert!(untouched.iter().zip(&before).all(|(a, b)| a.to_bits() == b.to_bits()));
    }

    #[test]
    fn a_fringe_on_an_edge_goes_and_a_purple_subject_stays() {
        let (w, h) = (64usize, 32usize);
        let mut data = vec![0.0f32; w * h * 3];
        for y in 0..h {
            for x in 0..w {
                let at = (y * w + x) * 3;
                let pixel = if x < 20 {

                    [0.02, 0.02, 0.02]
                } else if x < 23 {

                    [0.55, 0.30, 0.60]
                } else if (40..50).contains(&x) {

                    [0.55, 0.30, 0.60]
                } else {
                    [0.60, 0.60, 0.60]
                };
                data[at..at + 3].copy_from_slice(&pixel);
            }
        }

        let before = data.clone();
        defringe(&mut data, w, h, 1.0, 1.0);

        let violet = |source: &[f32], x: usize| {
            let at = ((h / 2) * w + x) * 3;
            (source[at].min(source[at + 2]) - source[at + 1]).max(0.0)
        };

        let rim = 21;
        assert!(
            violet(&data, rim) < violet(&before, rim) * 0.4,
            "the rim lost its cast: {} was {}",
            violet(&data, rim),
            violet(&before, rim)
        );

        let flower = 45;
        assert!(
            (violet(&data, flower) - violet(&before, flower)).abs() < 0.01,
            "the flower kept its colour: {} was {}",
            violet(&data, flower),
            violet(&before, flower)
        );

        let flat = ((h / 2) * w + 60) * 3;
        for channel in 0..3 {
            assert!((data[flat + channel] - before[flat + channel]).abs() < 1e-4);
        }

        let mut untouched = before.clone();
        defringe(&mut untouched, w, h, 0.0, 1.0);
        assert!(untouched.iter().zip(&before).all(|(a, b)| a.to_bits() == b.to_bits()));
    }

    fn frame(width: usize, height: usize, speckle: f32) -> Vec<f32> {
        let mut data = vec![0.0f32; width * height * 3];
        for y in 0..height {
            for x in 0..width {
                let base = if x < width / 2 { 0.2 } else { 0.6 };
                let jitter = base * if (x + y) % 2 == 0 { speckle } else { -speckle };
                for channel in 0..3 {
                    data[(y * width + x) * 3 + channel] = (base + jitter).max(0.01);
                }
            }
        }
        data
    }

    fn local_contrast(data: &[f32], width: usize, height: usize) -> f32 {
        let mut total = 0.0;
        for y in 0..height {
            for x in 1..width {
                let here = data[(y * width + x) * 3];
                let left = data[(y * width + x - 1) * 3];
                total += (here - left).abs();
            }
        }
        total / ((width - 1) * height) as f32
    }

    #[test]
    fn sharpening_raises_local_contrast() {
        let (width, height) = (64, 64);
        let before = frame(width, height, 0.0);
        let mut after = before.clone();
        sharpen(&mut after, width, height, 0.8, 1.0, 0.0, 1.0);

        assert!(
            local_contrast(&after, width, height) > local_contrast(&before, width, height) * 1.2,
            "the edge should have got harder"
        );
    }

    #[test]
    fn a_flat_frame_is_left_alone() {

        let (width, height) = (32, 32);
        let flat = vec![0.4f32; width * height * 3];
        let mut after = flat.clone();
        sharpen(&mut after, width, height, 1.0, 1.0, 0.0, 1.0);

        for (before, now) in flat.iter().zip(after.iter()) {
            assert!((before - now).abs() < 1e-4, "{before} became {now}");
        }
    }

    #[test]
    fn the_threshold_keeps_sharpening_off_the_noise() {
        let (width, height) = (64, 64);
        let noisy = frame(width, height, 0.05);

        let mut wide_open = noisy.clone();
        sharpen(&mut wide_open, width, height, 1.0, 1.0, 0.0, 1.0);

        let mut masked = noisy.clone();
        sharpen(&mut masked, width, height, 1.0, 1.0, 1.0, 1.0);

        let grain = |data: &[f32]| {
            let mut total = 0.0;
            for y in 0..height {
                for x in 1..width / 2 {
                    total += (data[(y * width + x) * 3] - data[(y * width + x - 1) * 3]).abs();
                }
            }
            total
        };

        assert!(
            grain(&masked) < grain(&wide_open),
            "the threshold did not hold the grain back: {} against {}",
            grain(&masked),
            grain(&wide_open)
        );
    }

    #[test]
    fn a_radius_too_small_to_see_does_nothing() {

        let (width, height) = (32, 32);
        let before = frame(width, height, 0.0);
        let mut after = before.clone();
        sharpen(&mut after, width, height, 1.0, 1.0, 0.0, 0.2);
        assert_eq!(before, after);
    }

    fn rippled(width: usize, height: usize) -> Vec<f32> {
        let mut data = vec![0.0f32; width * height * 3];
        for y in 0..height {
            for x in 0..width {
                let ripple = 0.4 * (1.0 + 0.06 * (x as f32 * std::f32::consts::TAU / 16.0).sin());
                let grain = if (x + y) % 2 == 0 { 1.03 } else { 0.97 };
                data[(y * width + x) * 3..][..3].fill(ripple * grain);
            }
        }
        data
    }

    fn ripple(data: &[f32], width: usize, height: usize) -> f32 {
        let columns: Vec<f32> = (0..width)
            .map(|x| (0..height).map(|y| data[(y * width + x) * 3]).sum::<f32>() / height as f32)
            .collect();
        let (low, high) = columns[8..width - 8]
            .iter()
            .fold((f32::MAX, f32::MIN), |(low, high), value| (low.min(*value), high.max(*value)));
        high - low
    }

    #[test]
    fn detail_keeps_more_and_contrast_gives_the_texture_back() {
        let (width, height) = (64, 64);
        let run = |detail: f32, contrast: f32| {
            let mut data = rippled(width, height);
            denoise(&mut data, width, height, 1.0, detail, contrast, 1.0);
            data
        };

        assert!(local_contrast(&run(1.0, 0.0), width, height) > local_contrast(&run(0.0, 0.0), width, height));

        assert!(ripple(&run(0.0, 1.0), width, height) > ripple(&run(0.0, 0.0), width, height) * 1.1);

    }

    #[test]
    fn noise_reduction_smooths_grain_and_keeps_the_edge() {
        let (width, height) = (64, 64);
        let noisy = frame(width, height, 0.05);
        let mut cleaned = noisy.clone();
        denoise(&mut cleaned, width, height, 1.0, 0.5, 0.0, 1.0);
        denoise_colour(&mut cleaned, width, height, 0.0, 1.0);

        let grain = |data: &[f32]| {
            let mut total = 0.0;
            for y in 0..height {
                for x in 1..width / 2 {
                    total += (data[(y * width + x) * 3] - data[(y * width + x - 1) * 3]).abs();
                }
            }
            total / ((width / 2 - 1) * height) as f32
        };
        assert!(grain(&cleaned) < grain(&noisy) * 0.6, "grain survived");

        let step = |data: &[f32]| {
            let middle = width / 2;
            let row = height / 2;
            (data[(row * width + middle) * 3] - data[(row * width + middle - 1) * 3]).abs()
        };
        assert!(step(&cleaned) > step(&noisy) * 0.5, "the edge was smoothed away");
    }

    #[test]
    fn colour_noise_reduction_keeps_the_brightness_it_found() {
        let (width, height) = (32, 32);

        let mut data = vec![0.0f32; width * height * 3];
        for index in 0..width * height {
            let luma = 0.1 + 0.8 * (index % width) as f32 / width as f32;
            let swing = if index % 2 == 0 { 1.4 } else { 0.6 };
            data[index * 3] = luma * swing;
            data[index * 3 + 1] = luma;
            data[index * 3 + 2] = luma / swing;
        }

        let before: Vec<f32> = data
            .chunks_exact(3)
            .map(|p| 0.2126 * p[0] + 0.7152 * p[1] + 0.0722 * p[2])
            .collect();

        denoise(&mut data, width, height, 0.0, 0.5, 0.0, 1.0);
        denoise_colour(&mut data, width, height, 1.0, 1.0);

        let after: Vec<f32> = data
            .chunks_exact(3)
            .map(|p| 0.2126 * p[0] + 0.7152 * p[1] + 0.0722 * p[2])
            .collect();

        for (index, (was, now)) in before.iter().zip(after.iter()).enumerate() {
            assert!(
                (was - now).abs() < 0.02 * was.max(0.05),
                "pixel {index} changed brightness: {was} to {now}"
            );
        }
    }

    #[test]
    #[ignore]
    fn noise_contrast_at_1_1() {
        let (Ok(path), Ok(out)) = (std::env::var("FRAME"), std::env::var("OUT")) else {
            println!("set FRAME and OUT");
            return;
        };
        let path = std::path::PathBuf::from(path);
        let linear = numa_io::raw::decode_linear(&path).unwrap();
        let document = numa_core::document::Document::new(path.display().to_string());

        let working = crate::to_working_space(&document, &linear, &Default::default());

        let crop: Vec<u32> = std::env::var("CROP")
            .unwrap_or_default()
            .split_whitespace()
            .filter_map(|n| n.parse().ok())
            .collect();

        let luma: f32 = std::env::var("LUMA").ok().and_then(|n| n.parse().ok()).unwrap_or(40.0);
        for contrast in [0.0, 50.0, 100.0] {
            let mut document = document.clone();
            let mut basic = document.basic();

            basic.detail.denoise_luma = luma;
            basic.detail.denoise_detail = 50.0;
            basic.detail.denoise_contrast = contrast;
            document.set_basic(basic);

            let frame = crate::apply_stack(&document, &working, 1.0);
            let (x, y, w, h) = match crop[..] {
                [x, y, w, h] => (x, y, w, h),
                _ => (frame.width() / 2 - 300, frame.height() / 2 - 200, 600, 400),
            };
            let piece = image::imageops::crop_imm(&frame, x, y, w, h).to_image();
            let file = format!("{out}/contrast-{contrast:.0}.png");
            piece.save(&file).unwrap();
            println!("  {file}  {w}x{h} at {x},{y}");
        }
    }

    #[test]
    fn nothing_asked_for_is_nothing_done() {
        let (width, height) = (16, 16);
        let before = frame(width, height, 0.05);

        let mut after = before.clone();
        denoise(&mut after, width, height, 0.0, 0.5, 0.0, 1.0);
        denoise_colour(&mut after, width, height, 0.0, 1.0);
        assert_eq!(before, after);

        let mut after = before.clone();
        sharpen(&mut after, width, height, 0.0, 1.0, 0.0, 1.0);
        assert_eq!(before, after);
    }
}
