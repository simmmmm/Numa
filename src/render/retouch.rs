use crate::core::retouch::{Retouch, Spot};

pub(crate) const RING: f32 = 1.35;

pub fn apply(
    retouch: &Retouch,
    data: &mut [f32],
    width: usize,
    height: usize,
    region: [f32; 4],
) {
    if retouch.is_identity() || width == 0 || height == 0 {
        return;
    }
    if region[2] <= 0.0 || region[3] <= 0.0 {
        return;
    }

    let long_edge = (width as f32 / region[2]).max(height as f32 / region[3]);

    for spot in &retouch.spots {
        if spot.is_idle() {
            continue;
        }
        let local = Spot {
            at: [
                (spot.at[0] - region[0]) / region[2],
                (spot.at[1] - region[1]) / region[3],
            ],
            from: [
                (spot.from[0] - region[0]) / region[2],
                (spot.from[1] - region[1]) / region[3],
            ],
            ..*spot
        };
        apply_one(&local, data, width, height, long_edge);
    }
}

fn apply_one(spot: &Spot, data: &mut [f32], width: usize, height: usize, long_edge: f32) {
    let radius = spot.radius * long_edge;
    if radius < 0.5 {
        return;
    }

    let to = [spot.at[0] * width as f32, spot.at[1] * height as f32];
    let from = [spot.from[0] * width as f32, spot.from[1] * height as f32];

    let reach = radius.ceil() as i32;
    let side = (reach * 2 + 1) as usize;
    let mut patch = vec![0.0f32; side * side * 3];
    for row in 0..side {
        for column in 0..side {
            let sample = sample(
                data,
                width,
                height,
                from[0] + column as f32 - reach as f32,
                from[1] + row as f32 - reach as f32,
            );
            patch[(row * side + column) * 3..][..3].copy_from_slice(&sample);
        }
    }

    let scale = if spot.heal {
        let here = ring_mean(data, width, height, to, radius);
        let there = ring_mean(data, width, height, from, radius);
        let mut scale = [1.0f32; 3];
        for channel in 0..3 {

            if there[channel] > 1e-5 && here[channel] > 0.0 {
                scale[channel] = here[channel] / there[channel];
            }
        }
        scale
    } else {
        [1.0; 3]
    };

    let left = (to[0] - radius).floor().max(0.0) as usize;
    let right = ((to[0] + radius).ceil() as usize).min(width.saturating_sub(1));
    let top = (to[1] - radius).floor().max(0.0) as usize;
    let bottom = ((to[1] + radius).ceil() as usize).min(height.saturating_sub(1));

    for y in top..=bottom {
        for x in left..=right {
            let dx = x as f32 + 0.5 - to[0];
            let dy = y as f32 + 0.5 - to[1];
            let alpha = spot.coverage(dx.hypot(dy), radius);
            if alpha <= 0.0 {
                continue;
            }

            let column = (dx + reach as f32).round().clamp(0.0, (side - 1) as f32) as usize;
            let row = (dy + reach as f32).round().clamp(0.0, (side - 1) as f32) as usize;
            let source = &patch[(row * side + column) * 3..][..3];

            let target = &mut data[(y * width + x) * 3..][..3];
            for channel in 0..3 {
                let replacement = source[channel] * scale[channel];
                target[channel] += (replacement - target[channel]) * alpha;
            }
        }
    }
}

fn ring_mean(data: &[f32], width: usize, height: usize, centre: [f32; 2], radius: f32) -> [f32; 3] {
    let outer = radius * RING;
    let mut total = [0.0f64; 3];
    let mut count = 0.0f64;

    const STEPS: usize = 64;
    for step in 0..STEPS {
        let angle = step as f32 / STEPS as f32 * std::f32::consts::TAU;
        for band in [radius, (radius + outer) * 0.5, outer] {
            let x = centre[0] + angle.cos() * band;
            let y = centre[1] + angle.sin() * band;
            if x < 0.0 || y < 0.0 || x >= width as f32 || y >= height as f32 {
                continue;
            }
            let sample = sample(data, width, height, x, y);
            for channel in 0..3 {
                total[channel] += sample[channel] as f64;
            }
            count += 1.0;
        }
    }

    if count == 0.0 {
        return [0.0; 3];
    }
    [
        (total[0] / count) as f32,
        (total[1] / count) as f32,
        (total[2] / count) as f32,
    ]
}

fn sample(data: &[f32], width: usize, height: usize, x: f32, y: f32) -> [f32; 3] {
    let x = x.clamp(0.0, (width - 1) as f32);
    let y = y.clamp(0.0, (height - 1) as f32);
    let (x0, y0) = (x.floor() as usize, y.floor() as usize);
    let x1 = (x0 + 1).min(width - 1);
    let y1 = (y0 + 1).min(height - 1);
    let (fx, fy) = (x - x0 as f32, y - y0 as f32);

    let mut out = [0.0f32; 3];
    for channel in 0..3 {
        let at = |cx: usize, cy: usize| data[(cy * width + cx) * 3 + channel];
        let top = at(x0, y0) + (at(x1, y0) - at(x0, y0)) * fx;
        let bottom = at(x0, y1) + (at(x1, y1) - at(x0, y1)) * fx;
        out[channel] = top + (bottom - top) * fy;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const WHOLE: [f32; 4] = [0.0, 0.0, 1.0, 1.0];

    fn frame() -> (Vec<f32>, usize, usize) {
        let (width, height) = (128, 128);
        let mut data = vec![0.0f32; width * height * 3];
        for y in 0..height {
            for x in 0..width {
                let value = if x < width / 2 { 0.1 } else { 0.6 };
                data[(y * width + x) * 3..][..3].copy_from_slice(&[value; 3]);
            }
        }
        (data, width, height)
    }

    fn blemish(data: &mut [f32], width: usize, centre: (usize, usize), radius: i32) {
        for dy in -radius..=radius {
            for dx in -radius..=radius {
                if dx * dx + dy * dy > radius * radius {
                    continue;
                }
                let x = (centre.0 as i32 + dx) as usize;
                let y = (centre.1 as i32 + dy) as usize;
                data[(y * width + x) * 3..][..3].copy_from_slice(&[0.95, 0.1, 0.1]);
            }
        }
    }

    fn at(data: &[f32], width: usize, x: usize, y: usize) -> [f32; 3] {
        let pixel = &data[(y * width + x) * 3..][..3];
        [pixel[0], pixel[1], pixel[2]]
    }

    #[test]
    fn clone_takes_the_source_as_it_is() {
        let (mut data, width, height) = frame();
        blemish(&mut data, width, (96, 64), 6);
        assert!(at(&data, width, 96, 64)[0] > 0.9, "the fixture has a blemish");

        let retouch = Retouch {
            spots: vec![Spot {
                at: [96.0 / 128.0, 0.5],
                from: [112.0 / 128.0, 0.5],
                radius: 10.0 / 128.0,
                feather: 0.3,
                opacity: 1.0,
                heal: false,
            }],
        };
        apply(&retouch, &mut data, width, height, WHOLE);

        let covered = at(&data, width, 96, 64);
        assert!(covered[0] < 0.7, "the blemish should be gone: {covered:?}");
        assert!((covered[0] - 0.6).abs() < 0.05, "and read as its side of the frame");
    }

    #[test]
    fn heal_takes_its_brightness_from_where_it_lands() {
        let (mut data, width, height) = frame();
        blemish(&mut data, width, (32, 64), 6);

        let mut healed = data.clone();
        let mut cloned = data.clone();
        let spot = Spot {
            at: [32.0 / 128.0, 0.5],

            from: [96.0 / 128.0, 0.5],
            radius: 10.0 / 128.0,
            feather: 0.3,
            opacity: 1.0,
            heal: true,
        };
        apply(&Retouch { spots: vec![spot] }, &mut healed, width, height, WHOLE);
        apply(
            &Retouch { spots: vec![Spot { heal: false, ..spot }] },
            &mut cloned,
            width,
            height,
            WHOLE,
        );

        let healed_here = at(&healed, width, 32, 64)[0];
        let cloned_here = at(&cloned, width, 32, 64)[0];
        assert!(
            (healed_here - 0.1).abs() < 0.03,
            "heal should land at the destination's own tone, not {healed_here}"
        );
        assert!(
            cloned_here > 0.5,
            "clone should have brought the bright side with it: {cloned_here}"
        );
    }

    #[test]
    fn nothing_outside_the_patch_is_touched() {
        let (mut data, width, height) = frame();
        let before = data.clone();
        let retouch = Retouch {
            spots: vec![Spot {
                at: [0.25, 0.5],
                from: [0.75, 0.5],
                radius: 8.0 / 128.0,
                ..Default::default()
            }],
        };
        apply(&retouch, &mut data, width, height, WHOLE);

        let radius = 8.0f32;
        let centre = [0.25 * width as f32, 0.5 * height as f32];
        for y in 0..height {
            for x in 0..width {
                let distance = (x as f32 + 0.5 - centre[0]).hypot(y as f32 + 0.5 - centre[1]);
                if distance <= radius + 1.5 {
                    continue;
                }
                assert_eq!(
                    at(&data, width, x, y),
                    at(&before, width, x, y),
                    "pixel {x},{y} is {distance:.1} away and changed"
                );
            }
        }
    }

    #[test]
    fn an_overlapping_source_does_not_smear() {
        let (mut data, width, height) = frame();
        blemish(&mut data, width, (96, 64), 4);
        let retouch = Retouch {
            spots: vec![Spot {
                at: [96.0 / 128.0, 0.5],
                from: [100.0 / 128.0, 0.5],
                radius: 8.0 / 128.0,
                feather: 0.2,
                opacity: 1.0,
                heal: false,
            }],
        };
        apply(&retouch, &mut data, width, height, WHOLE);

        let covered = at(&data, width, 96, 64);
        assert!(covered[0] < 0.75, "{covered:?}");
    }
}
