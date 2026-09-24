use numa_core::beautify::{self, Beautify, Portrait};
use rayon::prelude::*;
use crate::local::{self, Plane};

const SPOT_RADIUS: f32 = 0.0025;
const SPOT_SURROUND: f32 = 0.030;

const SPOT_DEPTH: f32 = 0.030;
const SPOT_REDNESS: f32 = 0.009;

const SPOT_TOO_DEEP: f32 = 0.10;

const SPOT_CEILING: f32 = 0.10;

const SPOT_SHADE: f32 = 0.62;
const SPOT_HOPELESS: f32 = 0.24;

const SKIN_RADIUS: f32 = 0.09;
const TEXTURE_RADIUS: f32 = 0.012;
const COLOUR_RADIUS: f32 = 0.14;

const SKIN_EPSILON: f32 = 6.0e-3;
const TEXTURE_EPSILON: f32 = 2.0e-4;
const COLOUR_EPSILON: f32 = 8.0e-3;

pub(crate) fn skin_like(rgb: [f32; 3]) -> f32 {
    let (r, g, b) = (rgb[0].max(0.0), rgb[1].max(0.0), rgb[2].max(0.0));
    let total = r + g + b;
    if total <= 1e-5 {
        return 0.0;
    }

    let (rr, bb) = (r / total, b / total);

    let warm = ((rr - 0.33) / 0.06).clamp(0.0, 1.0);
    let cool = ((0.32 - bb) / 0.08).clamp(0.0, 1.0);
    warm * cool
}

pub fn apply(
    settings: &Beautify,
    faces: &[Portrait],
    data: &mut [f32],
    width: usize,
    height: usize,
    region: [f32; 4],
) {
    if settings.is_identity() || faces.is_empty() || width == 0 || height == 0 {
        return;
    }
    if region[2] <= 0.0 || region[3] <= 0.0 {
        return;
    }

    let local_u = |x: usize| (x as f32 + 0.5) / width as f32 * region[2] + region[0];
    let local_v = |y: usize| (y as f32 + 0.5) / height as f32 * region[3] + region[1];

    if settings.wants_skin() {
        smooth_skin(settings, faces, data, width, height, &local_u, &local_v);
    }
    if settings.red_eye > 0.0 {
        take_red_out(settings.red_eye / 100.0, faces, data, width, &local_u, &local_v);
    }
    if settings.teeth > 0.0 {
        whiten(settings.teeth / 100.0, faces, data, width, &local_u, &local_v);
    }
}

fn smooth_skin(
    settings: &Beautify,
    faces: &[Portrait],
    data: &mut [f32],
    width: usize,
    height: usize,
    local_u: &(impl Fn(usize) -> f32 + Sync),
    local_v: &(impl Fn(usize) -> f32 + Sync),
) {

    let mut skin = vec![0.0f32; width * height];
    let mut biggest = 0.0f32;
    for face in faces {
        biggest = biggest.max(face.at[2].max(face.at[3]));
    }

    skin.par_chunks_mut(width).enumerate().for_each(|(y, row)| {
        let v = local_v(y);
        for (x, value) in row.iter_mut().enumerate() {
            let u = local_u(x);
            let shape = faces.iter().fold(0.0f32, |most, face| most.max(beautify::skin_at(face, u, v)));
            if shape <= 0.0 {
                continue;
            }
            let at = (y * width + x) * 3;
            *value = shape * skin_like([data[at], data[at + 1], data[at + 2]]);
        }
    });
    let Some([left, top, right, bottom]) = covered(&skin, width) else { return };

    let long_edge = width.max(height) as f32;
    let radius = |fraction: f32| {
        ((biggest * long_edge * fraction) as usize).clamp(1, width.min(height).max(2) / 2)
    };

    let reach = 2 * [SPOT_SURROUND, SKIN_RADIUS, COLOUR_RADIUS, TEXTURE_RADIUS]
        .into_iter()
        .map(radius)
        .max()
        .unwrap_or(1)
        + 2;
    let (x0, y0) = (left.saturating_sub(reach), top.saturating_sub(reach));
    let (x1, y1) = ((right + reach).min(width - 1), (bottom + reach).min(height - 1));
    let (w, h) = (x1 + 1 - x0, y1 + 1 - y0);
    if w == width && h == height {
        treat_skin(settings, &skin, data, width, height, &radius);
        return;
    }
    let mut part: Vec<f32> = (y0..=y1).flat_map(|y| data[(y * width + x0) * 3..(y * width + x1 + 1) * 3].iter().copied()).collect();
    let part_skin: Vec<f32> = (y0..=y1).flat_map(|y| skin[y * width + x0..=y * width + x1].iter().copied()).collect();
    treat_skin(settings, &part_skin, &mut part, w, h, &radius);
    for (row, y) in (y0..=y1).enumerate() {
        data[(y * width + x0) * 3..(y * width + x1 + 1) * 3].copy_from_slice(&part[row * w * 3..(row + 1) * w * 3]);
    }
}

fn covered(mask: &[f32], width: usize) -> Option<[usize; 4]> {
    let rows: Vec<Option<(usize, usize)>> = mask
        .par_chunks(width)
        .map(|row| Some((row.iter().position(|value| *value > 0.0)?, row.iter().rposition(|value| *value > 0.0)?)))
        .collect();
    let top = rows.iter().position(Option::is_some)?;
    let bottom = rows.iter().rposition(Option::is_some)?;
    let (left, right) = rows.iter().flatten().fold((usize::MAX, 0), |(l, r), (a, b)| (l.min(*a), r.max(*b)));
    Some([left, top, right, bottom])
}

fn treat_skin(
    settings: &Beautify,
    skin: &[f32],
    data: &mut [f32],
    width: usize,
    height: usize,
    radius: &impl Fn(f32) -> usize,
) {

    let luma_of = |data: &[f32]| {
        Plane::new(
            width,
            height,
            (0..width * height)
                .map(|index| {
                    let at = index * 3;
                    0.2126 * data[at] + 0.7152 * data[at + 1] + 0.0722 * data[at + 2]
                })
                .collect(),
        )
    };
    let mut guide = luma_of(data);

    if settings.spots > 0.0 {
        remove_spots(settings, skin, data, width, height, radius);

        guide = luma_of(data);
    }

    if settings.skin > 0.0 {
        smooth_brightness(settings, skin, &guide, data, radius);
    }

    if settings.evenness > 0.0 {
        even_colour(settings, skin, &guide, data, width, height, radius);
    }
}

fn remove_spots(
    settings: &Beautify,
    skin: &[f32],
    data: &mut [f32],
    width: usize,
    height: usize,
    radius: &impl Fn(f32) -> usize,
) {
    let amount = settings.spots / 100.0;
    let small = radius(SPOT_RADIUS);
    let wide = radius(SPOT_SURROUND);

    let bands: Vec<(Plane, Plane)> = (0..3)
        .map(|channel| {
            let plane = Plane::new(
                width,
                height,
                (0..width * height)
                    .map(|index| data[index * 3 + channel].max(0.0).sqrt())
                    .collect(),
            );
            (local::blur(&plane, small), local::blur(&plane, wide))
        })
        .collect();

    let luma = |at: &dyn Fn(usize) -> f32| {
        0.2126 * at(0) + 0.7152 * at(1) + 0.0722 * at(2)
    };

    let (sum, weight_sum) = (0..width * height).fold((0.0f32, 0.0f32), |(sum, count), index| {
        let weight = skin[index];
        (sum + luma(&|channel| bands[channel].1.data[index]) * weight, count + weight)
    });
    let reference = if weight_sum > 0.0 { sum / weight_sum } else { 0.0 };

    data.par_chunks_exact_mut(3).enumerate().for_each(|(index, pixel)| {
        let weight = skin[index] * amount;
        if weight <= 0.0 {
            return;
        }
        let near = luma(&|channel| bands[channel].0.data[index]);
        let far = luma(&|channel| bands[channel].1.data[index]);
        let ground = far.max(1e-4);
        let lit = ((far / reference.max(1e-4) - SPOT_SHADE) / 0.2).clamp(0.0, 1.0);
        if lit <= 0.0 {
            return;
        }

        let dark = (far - near) / ground;

        let redness =
            bands[0].0.data[index] / near.max(1e-4) - bands[0].1.data[index] / ground;
        let signal = (dark / SPOT_DEPTH).max(redness / SPOT_REDNESS);
        if signal <= 1.0 {
            return;
        }

        let found = ((signal - 1.0) * 0.8).clamp(0.0, 1.0)
            * (1.0
                - ((dark.abs() - SPOT_TOO_DEEP) / (SPOT_HOPELESS - SPOT_TOO_DEEP))
                    .clamp(0.0, 1.0));

                for channel in 0..3 {

            let ceiling = ground * SPOT_CEILING;
            let band = (bands[channel].1.data[index] - bands[channel].0.data[index])
                .clamp(-ceiling, ceiling);
            let moved =
                (pixel[channel].max(0.0).sqrt() + band * found * weight * lit).max(0.0);
            pixel[channel] = moved * moved;
        }
    });
}

fn smooth_brightness(
    settings: &Beautify,
    skin: &[f32],
    guide: &Plane,
    data: &mut [f32],
    radius: &impl Fn(f32) -> usize,
) {
    let amount = settings.skin / 100.0;
    let flat = local::guided_by(guide, guide, radius(SKIN_RADIUS), SKIN_EPSILON);
    let grain = local::guided_by(guide, guide, radius(TEXTURE_RADIUS), TEXTURE_EPSILON);
    data.par_chunks_exact_mut(3).enumerate().for_each(|(index, pixel)| {
        let weight = skin[index] * amount;
        if weight <= 0.0 {
            return;
        }
        let from = guide.data[index];
        if from <= 1e-6 {
            return;
        }

        let wanted = flat.data[index] + (from - grain.data[index]);

        let scale = 1.0 + (wanted / from - 1.0) * weight;
        for value in pixel.iter_mut() {
            *value *= scale;
        }
    });
}

fn even_colour(
    settings: &Beautify,
    skin: &[f32],
    guide: &Plane,
    data: &mut [f32],
    width: usize,
    height: usize,
    radius: &impl Fn(f32) -> usize,
) {
    let amount = settings.evenness / 100.0;
    let radius = radius(COLOUR_RADIUS);

    let mut channels: Vec<Plane> = (0..3)
        .map(|channel| {
            Plane::new(
                width,
                height,
                (0..width * height)
                    .map(|index| {
                        let luma = guide.data[index].max(1e-6);
                        data[index * 3 + channel] / luma
                    })
                    .collect(),
            )
        })
        .collect();

    for (channel, plane) in channels.iter_mut().enumerate() {
        let smoothed = local::guided_by(guide, plane, radius, COLOUR_EPSILON);
        data.par_chunks_exact_mut(3).enumerate().for_each(|(index, pixel)| {
            let weight = skin[index] * amount;
            if weight <= 0.0 {
                return;
            }
            let luma = guide.data[index].max(1e-6);
            let ratio = plane.data[index] + (smoothed.data[index] - plane.data[index]) * weight;
            pixel[channel] = ratio * luma;
        });
    }
}

fn take_red_out(
    amount: f32,
    faces: &[Portrait],
    data: &mut [f32],
    width: usize,
    local_u: &(impl Fn(usize) -> f32 + Sync),
    local_v: &(impl Fn(usize) -> f32 + Sync),
) {
    data.par_chunks_exact_mut(width * 3).enumerate().for_each(|(y, row)| {
        for (x, pixel) in row.chunks_exact_mut(3).enumerate() {
            let (u, v) = (local_u(x), local_v(y));
            let where_eyes = faces
                .iter()
                .fold(0.0f32, |most, face| most.max(beautify::eyes_at(face, u, v)));
            if where_eyes <= 0.0 {
                continue;
            }

            let (r, g, b) = (pixel[0], pixel[1], pixel[2]);
            let other = g.max(b).max(1e-6);

            let excess = ((r / other - 2.0) / 1.5).clamp(0.0, 1.0);
            if excess <= 0.0 {
                continue;
            }
            let weight = where_eyes * excess * amount;
            pixel[0] = r + (other - r) * weight;
        }
    });
}

fn whiten(
    amount: f32,
    faces: &[Portrait],
    data: &mut [f32],
    width: usize,
    local_u: &(impl Fn(usize) -> f32 + Sync),
    local_v: &(impl Fn(usize) -> f32 + Sync),
) {
    data.par_chunks_exact_mut(width * 3).enumerate().for_each(|(y, row)| {
        for (x, pixel) in row.chunks_exact_mut(3).enumerate() {
            let (u, v) = (local_u(x), local_v(y));
            let where_mouth = faces
                .iter()
                .fold(0.0f32, |most, face| most.max(beautify::mouth_at(face, u, v)));
            if where_mouth <= 0.0 {
                continue;
            }

            let (r, g, b) = (pixel[0], pixel[1], pixel[2]);
            let luma = 0.2126 * r + 0.7152 * g + 0.0722 * b;
            if luma <= 1e-5 {
                continue;
            }

            let bright = ((luma / (luma + 0.12)) * 2.0 - 0.7).clamp(0.0, 1.0);

            let neutral = ((g / r.max(1e-6) - 0.70) / 0.20).clamp(0.0, 1.0);
            let short_of_blue = ((1.0 - b / luma - 0.05) / 0.20).clamp(0.0, 1.0);
            let yellow = neutral * short_of_blue;
            let weight = where_mouth * bright * yellow * amount;
            if weight <= 0.0 {
                continue;
            }

            for (channel, value) in [r, g, b].into_iter().enumerate() {
                pixel[channel] = value + (luma - value) * weight * 0.8;
            }
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    const WHOLE: [f32; 4] = [0.0, 0.0, 1.0, 1.0];

    fn portrait() -> Portrait {
        Portrait {
            at: [0.3, 0.2, 0.4, 0.5],
            points: [[0.40, 0.36], [0.60, 0.36], [0.50, 0.46], [0.43, 0.58], [0.57, 0.58]],
        }
    }

    fn frame(width: usize, height: usize) -> Vec<f32> {
        let mut data = vec![0.0f32; width * height * 3];
        for index in 0..width * height {
            data[index * 3..][..3].copy_from_slice(&[0.42, 0.28, 0.22]);
        }
        data
    }

    fn at(data: &[f32], width: usize, u: f32, v: f32, height: usize) -> [f32; 3] {
        let x = (u * width as f32) as usize;
        let y = (v * height as f32) as usize;
        let p = &data[(y * width + x) * 3..][..3];
        [p[0], p[1], p[2]]
    }

    #[test]
    fn skin_reads_as_skin_and_sky_does_not() {
        assert!(skin_like([0.42, 0.28, 0.22]) > 0.9, "a warm mid tone");
        assert!(skin_like([0.55, 0.38, 0.30]) > 0.9, "a lighter one");
        assert!(skin_like([0.12, 0.08, 0.06]) > 0.9, "and a darker one");
        assert_eq!(skin_like([0.2, 0.35, 0.7]), 0.0, "sky");
        assert_eq!(skin_like([0.1, 0.4, 0.15]), 0.0, "foliage");
    }

    #[test]
    fn spots_go_and_shading_and_hair_stay() {
        let (w, h) = (512usize, 512usize);
        let face = portrait();
        let skin_at = |x: usize, y: usize| {
            beautify::skin_at(&face, (x as f32 + 0.5) / w as f32, (y as f32 + 0.5) / h as f32)
        };
        let (pimple, highlight, shadow, hair) = ((200, 250), (200, 300), (300, 240), (332, 260));
        for (name, at) in
            [("pimple", pimple), ("highlight", highlight), ("shadow", shadow), ("hair", hair)]
        {
            assert!(skin_at(at.0, at.1) > 0.5, "{name} has to be on skin to prove anything");
        }

        let mut data = frame(w, h);
        let disc = |x: usize, y: usize, at: (usize, usize), radius: f32| {
            (((x as f32 - at.0 as f32).powi(2) + (y as f32 - at.1 as f32).powi(2)).sqrt()) < radius
        };
        for y in 0..h {
            for x in 0..w {
                let index = (y * w + x) * 3;
                if disc(x, y, pimple, 2.5) {

                    data[index] *= 1.18;
                    data[index + 1] *= 0.95;
                    data[index + 2] *= 0.95;
                } else if disc(x, y, shadow, 30.0) {
                    data[index] *= 1.18;
                    data[index + 1] *= 0.95;
                    data[index + 2] *= 0.95;
                } else if disc(x, y, highlight, 2.5) {
                    for channel in 0..3 {
                        data[index + channel] *= 1.35;
                    }
                } else if (330..335).contains(&x) && (210..310).contains(&y) {
                    for channel in 0..3 {
                        data[index + channel] *= 0.2;
                    }
                }
            }
        }

        let before = data.clone();
        apply(
            &Beautify { spots: 100.0, ..Default::default() },
            &[face],
            &mut data,
            w,
            h,
            WHOLE,
        );

        let cheek = |source: &[f32], channel: usize| source[(100 * w + 256) * 3 + channel];
        let gap = |source: &[f32], at: (usize, usize), channel: usize| {
            (source[(at.1 * w + at.0) * 3 + channel] - cheek(source, channel)).abs()
        };

        assert!(
            gap(&data, pimple, 0) < gap(&before, pimple, 0) * 0.5,
            "the pimple is mostly gone: {} was {}",
            gap(&data, pimple, 0),
            gap(&before, pimple, 0),
        );
        for (name, at, channel) in
            [("the shadow", shadow, 0), ("the highlight", highlight, 1), ("the hair", hair, 1)]
        {
            assert!(
                gap(&data, at, channel) > gap(&before, at, channel) * 0.9,
                "{name} is still there: {} was {}",
                gap(&data, at, channel),
                gap(&before, at, channel),
            );
        }
    }

    #[test]
    fn smoothing_flattens_blotches_and_keeps_texture() {

        let (w, h) = (256usize, 256usize);
        let mut data = frame(w, h);
        for y in 0..h {
            for x in 0..w {
                let index = (y * w + x) * 3;
                let blotch = (((x as f32 - 96.0) / 8.0).powi(2)
                    + ((y as f32 - 124.0) / 8.0).powi(2))
                .max(0.0);
                if blotch < 1.0 {
                    data[index] += 0.05;
                }
                let grain = if (x + y) % 2 == 0 { 0.012 } else { -0.012 };
                for channel in 0..3 {
                    data[index + channel] += grain;
                }
            }
        }

        let before = data.clone();
        apply(
            &Beautify { skin: 100.0, ..Default::default() },
            &[portrait()],
            &mut data,
            w,
            h,
            WHOLE,
        );

        let reading = |source: &[f32], x: usize, y: usize| source[(y * w + x) * 3];
        let gap_before = reading(&before, 96, 124) - reading(&before, 96, 90);
        let gap_after = reading(&data, 96, 124) - reading(&data, 96, 90);
        assert!(
            gap_after.abs() < gap_before.abs(),
            "the blotch should flatten: {gap_before} to {gap_after}"
        );

        let grain = (reading(&data, 130, 140) - reading(&data, 131, 140)).abs();
        assert!(grain > 0.002, "the texture was smoothed away: {grain}");
    }

    #[test]
    fn the_rest_of_the_photograph_is_left_alone() {
        let (w, h) = (96usize, 96usize);
        let mut data = frame(w, h);
        let before = data.clone();
        apply(
            &Beautify { spots: 100.0, skin: 100.0, evenness: 100.0, red_eye: 100.0, teeth: 100.0 },
            &[portrait()],
            &mut data,
            w,
            h,
            WHOLE,
        );

        for (name, u, v) in [("top left", 0.04, 0.04), ("bottom right", 0.95, 0.95)] {
            let after = at(&data, w, u, v, h);
            let was = at(&before, w, u, v, h);
            for channel in 0..3 {
                assert!(
                    (after[channel] - was[channel]).abs() < 1e-5,
                    "{name} moved: {was:?} to {after:?}"
                );
            }
        }
    }

    #[test]
    fn red_eye_comes_out_and_a_brown_iris_stays() {
        let (w, h) = (96usize, 96usize);
        let face = portrait();

        let mut red = frame(w, h);
        let mut brown = frame(w, h);
        for (data, colour) in [(&mut red, [0.9, 0.12, 0.10]), (&mut brown, [0.20, 0.13, 0.08])] {
            let (x, y) = ((face.points[0][0] * w as f32) as usize,
                          (face.points[0][1] * h as f32) as usize);
            data[(y * w + x) * 3..][..3].copy_from_slice(&colour);
        }

        let settings = Beautify { red_eye: 100.0, ..Default::default() };
        apply(&settings, &[face], &mut red, w, h, WHOLE);
        apply(&settings, &[face], &mut brown, w, h, WHOLE);

        let fixed = at(&red, w, face.points[0][0], face.points[0][1], h);
        assert!(fixed[0] < 0.2, "the flash should be gone: {fixed:?}");
        let iris = at(&brown, w, face.points[0][0], face.points[0][1], h);
        assert!((iris[0] - 0.20).abs() < 1e-4, "a brown iris is not red eye: {iris:?}");
    }

    #[test]
    fn a_shut_mouth_is_not_whitened() {
        let (w, h) = (96usize, 96usize);
        let face = portrait();
        let mut data = frame(w, h);

        let (x, y) = ((0.50 * w as f32) as usize, (0.58 * h as f32) as usize);
        data[(y * w + x) * 3..][..3].copy_from_slice(&[0.30, 0.12, 0.11]);
        let before = at(&data, w, 0.50, 0.58, h);

        apply(&Beautify { teeth: 100.0, ..Default::default() }, &[face], &mut data, w, h, WHOLE);

        let after = at(&data, w, 0.50, 0.58, h);
        for channel in 0..3 {
            assert!(
                (after[channel] - before[channel]).abs() < 0.01,
                "lips were whitened: {before:?} to {after:?}"
            );
        }
    }
}
