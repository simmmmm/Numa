use rayon::prelude::*;

pub use numa_core::plane::{blur, subsample, upsample, Plane};

pub struct ToneGuide {

    base: Plane,

    bloom: Option<Plane>,
    pivot: f32,

    settings: [f32; 3],
}

#[derive(Clone, Copy)]
pub enum Tone<'a> {

    Own,

    Record(&'a std::cell::RefCell<Option<ToneGuide>>),

    Measure(&'a std::cell::RefCell<Option<ToneGuide>>),

    Guided(&'a ToneGuide, [f32; 4]),
}

fn sample_region(plane: &Plane, width: usize, height: usize, region: [f32; 4]) -> Plane {
    let (sw, sh) = (plane.width, plane.height);
    let mut out = vec![0.0f32; width * height];
    out.par_chunks_mut(width).enumerate().for_each(|(y, row)| {
        let v = region[1] + (y as f32 + 0.5) / height as f32 * region[3];
        let fy = (v * sh as f32 - 0.5).max(0.0);
        let y0 = (fy as usize).min(sh - 1);
        let y1 = (y0 + 1).min(sh - 1);
        let ty = fy - y0 as f32;
        for (x, target) in row.iter_mut().enumerate() {
            let u = region[0] + (x as f32 + 0.5) / width as f32 * region[2];
            let fx = (u * sw as f32 - 0.5).max(0.0);
            let x0 = (fx as usize).min(sw - 1);
            let x1 = (x0 + 1).min(sw - 1);
            let tx = fx - x0 as f32;
            let top = plane.data[y0 * sw + x0] * (1.0 - tx) + plane.data[y0 * sw + x1] * tx;
            let bottom = plane.data[y1 * sw + x0] * (1.0 - tx) + plane.data[y1 * sw + x1] * tx;
            *target = top * (1.0 - ty) + bottom * ty;
        }
    });
    Plane::new(width, height, out)
}

const RADIUS_FRACTION: f32 = 1.0 / 28.0;

const EDGE_EPSILON: f32 = 0.04;

const MAX_COMPRESSION: f32 = 0.65;

const MAX_CLARITY: f32 = 1.0;

const TEXTURE_FRACTION: f32 = 1.0 / 220.0;

const MAX_TEXTURE: f32 = 1.4;

const BLOOM_FRACTION: f32 = 1.0 / 14.0;

const MAX_BLOOM: f32 = 0.85;

const SUBSAMPLE: usize = 4;

pub fn guided(image: &Plane, radius: usize, epsilon: f32) -> Plane {
    guided_by(image, image, radius, epsilon)
}

pub fn guided_by(guide: &Plane, target: &Plane, radius: usize, epsilon: f32) -> Plane {
    debug_assert_eq!(guide.data.len(), target.data.len());
    let (width, height) = (guide.width, guide.height);

    let same = std::ptr::eq(guide, target);

    let squared = Plane::new(width, height, guide.data.par_iter().map(|value| value * value).collect());
    let crossed = (!same).then(|| {
        Plane::new(
            width,
            height,
            guide.data.par_iter().zip(target.data.par_iter()).map(|(g, t)| g * t).collect(),
        )
    });

    let mean = blur(guide, radius);
    let mean_squared = blur(&squared, radius);
    let mean_target = (!same).then(|| blur(target, radius));
    let mean_crossed = crossed.as_ref().map(|crossed| blur(crossed, radius));
    let mean_target = mean_target.as_ref().unwrap_or(&mean);
    let mean_crossed = mean_crossed.as_ref().unwrap_or(&mean_squared);

    let (a, b): (Vec<f32>, Vec<f32>) = (0..guide.data.len())
        .into_par_iter()
        .map(|index| {
            let variance =
                (mean_squared.data[index] - mean.data[index] * mean.data[index]).max(0.0);
            let covariance = mean_crossed.data[index] - mean.data[index] * mean_target.data[index];
            let weight = covariance / (variance + epsilon);
            (weight, mean_target.data[index] - weight * mean.data[index])
        })
        .unzip();

    let mean_a = blur(&Plane::new(width, height, a), radius);
    let mean_b = blur(&Plane::new(width, height, b), radius);

    Plane::new(
        width,
        height,
        (0..guide.data.len())
            .into_par_iter()
            .map(|index| mean_a.data[index] * guide.data[index] + mean_b.data[index])
            .collect(),
    )
}

pub fn tone_map(
    data: &mut [f32],
    width: usize,
    height: usize,
    compress: f32,
    clarity: f32,
    texture: f32,
    tone: Tone,
) {
    if (compress == 0.0 && clarity == 0.0 && texture == 0.0) || width == 0 || height == 0 {
        return;
    }
    let settings = [compress, clarity, texture];

    let compress = compress.clamp(-1.0, 1.0) * MAX_COMPRESSION;
    let clarity = clarity.clamp(-1.0, 1.0) * MAX_CLARITY;
    let texture = texture.clamp(-1.0, 1.0) * MAX_TEXTURE;

    let luminance: Vec<f32> = data
        .par_chunks_exact(3)
        .map(|pixel| {
            let luma = 0.2126 * pixel[0] + 0.7152 * pixel[1] + 0.0722 * pixel[2];
            luma.max(1e-5).log2()
        })
        .collect();

    let log_luminance = Plane::new(width, height, luminance);

    let handed = match tone {
        Tone::Guided(guide, region) if guide.settings == settings => Some((guide, region)),
        _ => None,
    };
    let (base, bloom, pivot, frame_long) = match handed {
        Some((guide, region)) => (
            sample_region(&guide.base, width, height, region),
            guide.bloom.as_ref().map(|glow| sample_region(glow, width, height, region)),
            guide.pivot,

            (width as f32 / region[2]).max(height as f32 / region[3]),
        ),
        None => {
            let radius = ((width.max(height) as f32 * RADIUS_FRACTION) as usize).max(1);

            let small = subsample(&log_luminance, SUBSAMPLE);
            let small_radius = (radius / SUBSAMPLE).max(1);
            let base_small = guided(&small, small_radius, EDGE_EPSILON);
            let base = upsample(&base_small, width, height);

            let bloom_small = (clarity < 0.0).then(|| {
                let radius = ((width.max(height) as f32 * BLOOM_FRACTION) as usize).max(1);
                blur(&small, (radius / SUBSAMPLE).max(1))
            });
            let bloom = bloom_small.as_ref().map(|glow| upsample(glow, width, height));

            let pivot = (base.data.iter().map(|v| *v as f64).sum::<f64>() / base.data.len() as f64) as f32;
            match tone {
                Tone::Record(cell) => {
                    *cell.borrow_mut() = Some(ToneGuide { base: base_small, bloom: bloom_small, pivot, settings });
                }
                Tone::Measure(cell) => {
                    *cell.borrow_mut() = Some(ToneGuide { base: base_small, bloom: bloom_small, pivot, settings });
                    return;
                }
                _ => {}
            }
            (base, bloom, pivot, width.max(height) as f32)
        }
    };

    let mid = (texture != 0.0).then(|| {
        let radius = ((frame_long * TEXTURE_FRACTION) as usize).max(1);
        guided(&log_luminance, radius, EDGE_EPSILON)
    });

    let base_scale = 1.0 - compress;

    let detail_scale = 1.0 + clarity.max(0.0);
    let glow = (-clarity).max(0.0) * MAX_BLOOM;
    let texture_scale = 1.0 + texture;

    data.par_chunks_exact_mut(3)
        .enumerate()
        .for_each(|(index, pixel)| {
            let original = log_luminance.data[index];
            let low = base.data[index];

            let detail = match &mid {
                Some(mid) => {
                    let middle = mid.data[index];
                    (middle - low) * detail_scale + (original - middle) * texture_scale
                }
                None => (original - low) * detail_scale,
            };

            let mut mapped = pivot + (low - pivot) * base_scale + detail;

            if let Some(bloom) = &bloom {
                let spill = (bloom.data[index] - original).max(0.0);
                mapped += spill * glow;
            }

            let gain = (mapped - original).exp2();
            for channel in pixel.iter_mut() {
                *channel = (*channel * gain).max(0.0);
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn guided_equals_guided_by_itself_to_the_bit() {
        let (width, height) = (96usize, 48usize);
        let data: Vec<f32> = (0..width * height).map(|i| ((i * 31) % 97) as f32 / 96.0).collect();
        let plane = Plane::new(width, height, data);
        let copy = Plane::new(width, height, plane.data.clone());
        let short = guided(&plane, 4, 0.01);
        let long = guided_by(&plane, &copy, 4, 0.01);
        assert!(short.data.iter().zip(&long.data).all(|(a, b)| a.to_bits() == b.to_bits()));
    }

    fn ramp_and_edge() -> Plane {

        let (width, height) = (64, 32);
        let mut data = Vec::with_capacity(width * height);
        for y in 0..height {
            for x in 0..width {
                let step = if x < width / 2 { 0.0 } else { 4.0 };
                let texture = if (x + y) % 4 == 0 { 0.15 } else { -0.15 };
                data.push(step + texture);
            }
        }
        Plane::new(width, height, data)
    }

    #[test]
    fn blur_preserves_a_flat_field() {
        let flat = Plane::new(16, 16, vec![0.7; 256]);
        let blurred = blur(&flat, 3);
        for value in &blurred.data {
            assert!((value - 0.7).abs() < 1e-5, "edges must extend, not darken: {value}");
        }
    }

    #[test]
    fn blur_keeps_the_mean() {
        let image = Plane::new(32, 32, (0..1024).map(|i| (i % 17) as f32 * 0.1).collect());
        let before = image.data.iter().sum::<f32>() / 1024.0;
        let after = blur(&image, 2).data.iter().sum::<f32>() / 1024.0;
        assert!((before - after).abs() < 0.05, "{before} vs {after}");
    }

    #[test]
    fn guided_filter_keeps_edges_that_a_blur_destroys() {
        let image = ramp_and_edge();

        let radius = 6;

        let blurred = blur(&image, radius);
        let filtered = guided(&image, radius, EDGE_EPSILON);

        let band = |plane: &Plane, range: std::ops::Range<usize>| {
            let count = range.len() as f32;
            range.map(|x| plane.data[16 * plane.width + x]).sum::<f32>() / count
        };
        let step = |plane: &Plane| band(plane, 33..38) - band(plane, 26..31);

        assert!(
            step(&filtered) > step(&blurred) * 1.3,
            "guided kept {:.2} of the step, blur kept {:.2} — no better than a blur",
            step(&filtered),
            step(&blurred)
        );
        assert!(
            step(&filtered) > step(&image) * 0.8,
            "guided lost too much of the {:.2} step: {:.2}",
            step(&image),
            step(&filtered)
        );

        let texture = |plane: &Plane| {
            (6..28)
                .map(|x: usize| {
                    (plane.data[16 * plane.width + x + 1] - plane.data[16 * plane.width + x]).abs()
                })
                .sum::<f32>()
        };

        assert!(
            texture(&filtered) < texture(&image) * 0.5,
            "texture survived the filter: {:.2} -> {:.2}",
            texture(&image),
            texture(&filtered)
        );
    }

    #[test]
    fn tone_map_at_zero_changes_nothing() {
        let mut data = vec![0.1, 0.3, 0.9, 0.4, 0.2, 0.05, 1.4, 0.8, 0.3, 0.02, 0.02, 0.02];
        let before = data.clone();
        tone_map(&mut data, 2, 2, 0.0, 0.0, 0.0, Tone::Own);
        assert_eq!(data, before);
    }

    #[test]
    fn compression_narrows_the_range_but_keeps_local_detail() {

        let (width, height) = (64, 64);
        let mut data = Vec::new();
        for y in 0..height {
            for x in 0..width {
                let level = if x < width / 2 { 0.02 } else { 1.6 };
                let texture = if (x + y) % 3 == 0 { 1.25 } else { 0.8 };
                data.extend_from_slice(&[level * texture; 3]);
            }
        }

        let luma = |d: &[f32]| -> Vec<f32> { d.chunks_exact(3).map(|p| p[0]).collect() };
        let before = luma(&data);

        let mut mapped = data.clone();
        tone_map(&mut mapped, width, height, 0.6, 0.0, 0.0, Tone::Own);
        let after = luma(&mapped);

        let at = |v: &[f32], x: usize, y: usize| v[y * width + x];

        let gap = |v: &[f32]| (at(v, 48, 8) / at(v, 16, 8).max(1e-6)).log2();
        assert!(
            gap(&after) < gap(&before) * 0.8,
            "range did not compress: {} -> {}",
            gap(&before),
            gap(&after)
        );

        let texture = |v: &[f32]| (at(v, 15, 9) / at(v, 16, 9).max(1e-6)).log2().abs();
        assert!(
            texture(&after) > texture(&before) * 0.7,
            "local detail was flattened: {} -> {}",
            texture(&before),
            texture(&after)
        );
    }

    #[test]
    fn the_pivot_survives_a_full_resolution_frame() {

        let count = 40_000_000usize;
        let value = |i: usize| -12.0 + (i % 9973) as f32 / 9973.0 * 13.0;

        let exact = (0..count).map(|i| value(i) as f64).sum::<f64>() / count as f64;
        let naive = (0..count).map(value).sum::<f32>() / count as f32;
        let careful = ((0..count).map(|i| value(i) as f64).sum::<f64>() / count as f64) as f32;

        assert!(
            (careful as f64 - exact).abs() < 1e-3,
            "f64 accumulation should be exact, got {careful} vs {exact}"
        );

        assert!(
            (naive as f64 - exact).abs() > 0.1,
            "if f32 no longer drifts here the test has stopped testing anything: {naive}"
        );
    }

    #[test]
    fn clarity_boosts_local_detail() {
        let (width, height) = (48, 48);
        let mut data = Vec::new();
        for y in 0..height {
            for x in 0..width {
                let value = 0.3 * if (x / 2 + y / 2) % 2 == 0 { 1.3 } else { 0.77 };
                data.extend_from_slice(&[value; 3]);
            }
        }

        let mut boosted = data.clone();
        tone_map(&mut boosted, width, height, 0.0, 0.5, 0.0, Tone::Own);

        let spread = |d: &[f32]| {
            let values: Vec<f32> = d.chunks_exact(3).map(|p| p[0]).collect();
            let mean = values.iter().sum::<f32>() / values.len() as f32;
            (values.iter().map(|v| (v - mean).powi(2)).sum::<f32>() / values.len() as f32).sqrt()
        };

        assert!(
            spread(&boosted) > spread(&data) * 1.05,
            "clarity did not increase local contrast: {} -> {}",
            spread(&data),
            spread(&boosted)
        );
    }

    #[test]
    fn texture_and_clarity_reach_different_scales() {
        let (w, h) = (256usize, 256usize);
        let make = || {
            let mut data = vec![0.0f32; w * h * 3];
            for y in 0..h {
                for x in 0..w {

                    let broad = 0.18 + 0.06 * ((x / 64) as f32);
                    let fine = if (x / 2 + y / 2) % 2 == 0 { 0.012 } else { -0.012 };
                    data[(y * w + x) * 3..][..3].copy_from_slice(&[broad + fine; 3]);
                }
            }
            data
        };

        let fine_contrast = |data: &[f32]| {
            let mut total = 0.0f64;
            for y in 100..150 {
                for x in 100..150 {
                    let a = data[(y * w + x) * 3];
                    let b = data[(y * w + x + 2) * 3];
                    total += (a - b).abs() as f64;
                }
            }
            total
        };

        let before = fine_contrast(&make());

        let mut with_texture = make();
        tone_map(&mut with_texture, w, h, 0.0, 0.0, 1.0, Tone::Own);
        let mut with_clarity = make();
        tone_map(&mut with_clarity, w, h, 0.0, 1.0, 0.0, Tone::Own);

        let textured = fine_contrast(&with_texture);
        let clarified = fine_contrast(&with_clarity);

        assert!(textured > before * 1.2, "texture should lift it: {before} to {textured}");
        assert!(
            clarified < textured,
            "clarity works at a coarser scale: {clarified} against {textured}"
        );
    }

    #[test]
    fn texture_at_zero_leaves_the_two_band_result_alone() {
        let (w, h) = (64usize, 64usize);
        let base: Vec<f32> = (0..w * h * 3).map(|i| 0.2 + (i % 7) as f32 * 0.004).collect();

        let mut two = base.clone();
        tone_map(&mut two, w, h, 0.4, 0.5, 0.0, Tone::Own);
        let mut again = base.clone();
        tone_map(&mut again, w, h, 0.4, 0.5, 0.0, Tone::Own);
        assert_eq!(two, again, "and it is deterministic");
    }

    #[test]
    fn negative_clarity_glows_rather_than_blurs() {
        let (w, h) = (256usize, 256usize);
        let make = || {
            let mut data = vec![0.0f32; w * h * 3];
            for y in 0..h {
                for x in 0..w {
                    let distance =
                        ((x as f32 - 128.0).powi(2) + (y as f32 - 128.0).powi(2)).sqrt() / 40.0;
                    let base = if distance < 1.0 { 0.75 } else { 0.06 };
                    let grain = if (x / 2 + y / 2) % 2 == 0 { 0.008 } else { -0.008 };
                    data[(y * w + x) * 3..][..3].copy_from_slice(&[base + grain; 3]);
                }
            }
            data
        };
        let at = |data: &[f32], x: usize, y: usize| data[(y * w + x) * 3];

        let before = make();
        let mut after = make();
        tone_map(&mut after, w, h, 0.0, -1.0, 0.0, Tone::Own);

        let beside = (at(&before, 128, 175), at(&after, 128, 175));
        assert!(beside.1 > beside.0 * 1.5, "no glow: {beside:?}");

        assert!((at(&after, 128, 128) - at(&before, 128, 128)).abs() < 0.01, "the disc moved");
        assert!((at(&after, 20, 20) - at(&before, 20, 20)).abs() < 0.01, "the corner moved");

        let grain = |data: &[f32]| (at(data, 60, 60) - at(data, 62, 60)).abs();
        assert!(grain(&after) > grain(&before) * 0.4, "texture gone: {}", grain(&after));
    }
}
