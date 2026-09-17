use rayon::prelude::*;

pub fn calibrate(data: &mut [f32], hues: [f32; 3], saturations: [f32; 3], shadow_tint: f32, weights: [f32; 3]) {
    if hues == [0.0; 3] && saturations == [0.0; 3] && shadow_tint == 0.0 {
        return;
    }
    let matrix = calibration_matrix(hues, saturations, weights);
    let tint = shadow_tint / 100.0;
    data.par_chunks_exact_mut(3).for_each(|pixel| {
        let rgb = [pixel[0], pixel[1], pixel[2]];
        for (row, out) in matrix.iter().zip(pixel.iter_mut()) {
            *out = (row[0] * rgb[0] + row[1] * rgb[1] + row[2] * rgb[2]).max(0.0);
        }
        if tint != 0.0 {

            let luma = weights[0] * pixel[0] + weights[1] * pixel[1] + weights[2] * pixel[2];
            let shadow = 1.0 - smoothstep(0.02, 0.25, luma);
            pixel[1] *= 2f32.powf(-0.4 * tint * shadow);
        }
    });
}

const MAX_TURN_DEGREES: f32 = 30.0;

fn calibration_matrix(hues: [f32; 3], saturations: [f32; 3], weights: [f32; 3]) -> [[f32; 3]; 3] {
    let mut columns = [[0.0f32; 3]; 3];
    for primary in 0..3 {
        let mut unit = [0.0f32; 3];
        unit[primary] = 1.0;
        let grey = weights[primary];
        let chroma = unit.map(|value| value - grey);
        let turned = rotate_about_grey(chroma, (hues[primary] / 100.0 * MAX_TURN_DEGREES).to_radians());
        let scale = 1.0 + saturations[primary] / 100.0;
        columns[primary] = turned.map(|value| grey + value * scale);
    }

    let drift = [0, 1, 2].map(|channel| columns.iter().map(|column| column[channel]).sum::<f32>() - 1.0);
    [0, 1, 2].map(|channel| [0, 1, 2].map(|primary| columns[primary][channel] - drift[channel] * weights[primary]))
}

fn rotate_about_grey(v: [f32; 3], angle: f32) -> [f32; 3] {
    let k = 1.0 / 3f32.sqrt();
    let (sin, cos) = angle.sin_cos();
    let cross = [k * (v[2] - v[1]), k * (v[0] - v[2]), k * (v[1] - v[0])];
    let dot = k * (v[0] + v[1] + v[2]);
    [0, 1, 2].map(|i| v[i] * cos + cross[i] * sin + k * dot * (1.0 - cos))
}

pub fn dehaze(data: &mut [f32], width: usize, height: usize, amount: f32) {
    if amount == 0.0 || width == 0 || height == 0 {
        return;
    }
    let strength = (amount / 100.0).clamp(-1.0, 1.0);

    let cell = (width.max(height) / 64).max(1);
    let (grid_w, grid_h) = (width.div_ceil(cell), height.div_ceil(cell));
    let mut dark = vec![f32::MAX; grid_w * grid_h];
    let mut mean = vec![[0.0f32; 4]; grid_w * grid_h];
    for y in 0..height {
        for x in 0..width {
            let index = (y * width + x) * 3;
            let pixel = [data[index], data[index + 1], data[index + 2]];
            let at = (y / cell) * grid_w + x / cell;
            dark[at] = dark[at].min(pixel[0].min(pixel[1]).min(pixel[2]));
            for channel in 0..3 {
                mean[at][channel] += pixel[channel];
            }
            mean[at][3] += 1.0;
        }
    }

    let mut order: Vec<usize> = (0..dark.len()).collect();
    order.sort_by(|a, b| dark[*b].total_cmp(&dark[*a]));
    let take = (order.len() / 1000).max(1);
    let mut airlight = [0.0f32; 3];
    for &at in &order[..take] {
        for channel in 0..3 {
            airlight[channel] += mean[at][channel] / mean[at][3] / take as f32;
        }
    }

    let grey = ((airlight[0] + airlight[1] + airlight[2]) / 3.0).max(1e-4);
    let airlight = [grey; 3];
    let floor = grey;

    let mut transmission: Vec<f32> = dark.iter().map(|d| (1.0 - 0.95 * d / floor).clamp(0.0, 1.0)).collect();
    for _ in 0..2 {
        transmission = box_blur(&transmission, grid_w, grid_h);
    }

    data.par_chunks_exact_mut(width * 3).enumerate().for_each(|(y, row)| {
        let gy = ((y as f32 + 0.5) / cell as f32 - 0.5).clamp(0.0, (grid_h - 1) as f32);
        for (x, pixel) in row.chunks_exact_mut(3).enumerate() {
            let gx = ((x as f32 + 0.5) / cell as f32 - 0.5).clamp(0.0, (grid_w - 1) as f32);
            let t = bilinear(&transmission, grid_w, grid_h, gx, gy);
            for channel in 0..3 {
                let value = pixel[channel];
                let a = airlight[channel];
                pixel[channel] = if strength > 0.0 {

                    let kept = 1.0 - 0.8 * strength * (1.0 - t.max(0.1));
                    ((value - a) / kept + a).max(0.0)
                } else {
                    let added = -strength * 0.25;
                    value * (1.0 - added) + a * added
                };
            }
        }
    });
}

fn box_blur(values: &[f32], width: usize, height: usize) -> Vec<f32> {
    let mut out = vec![0.0; values.len()];
    for y in 0..height {
        for x in 0..width {
            let (mut sum, mut count) = (0.0, 0.0);
            for yy in y.saturating_sub(1)..(y + 2).min(height) {
                for xx in x.saturating_sub(1)..(x + 2).min(width) {
                    sum += values[yy * width + xx];
                    count += 1.0;
                }
            }
            out[y * width + x] = sum / count;
        }
    }
    out
}

fn bilinear(values: &[f32], width: usize, height: usize, x: f32, y: f32) -> f32 {
    let (x0, y0) = (x.floor() as usize, y.floor() as usize);
    let (x1, y1) = ((x0 + 1).min(width - 1), (y0 + 1).min(height - 1));
    let (fx, fy) = (x - x0 as f32, y - y0 as f32);
    let top = values[y0 * width + x0] * (1.0 - fx) + values[y0 * width + x1] * fx;
    let bottom = values[y1 * width + x0] * (1.0 - fx) + values[y1 * width + x1] * fx;
    top * (1.0 - fy) + bottom * fy
}

pub fn vignette(
    data: &mut [f32],
    width: usize,
    height: usize,
    region: [f32; 4],
    frame: [f32; 2],
    settings: [f32; 4],
) {
    let [amount, midpoint, roundness, feather] = settings;
    if amount == 0.0 || width == 0 || height == 0 {
        return;
    }
    let stops = amount / 100.0 * 2.0;
    let aspect = frame[0] / frame[1].max(1e-6);

    let circle = (roundness / 100.0).max(0.0);

    let power = 2.0 + (-roundness / 100.0).max(0.0) * 6.0;
    let start = 0.15 + 0.85 * (midpoint / 100.0);
    let soft = 0.05 + 0.95 * (feather / 100.0);

    data.par_chunks_exact_mut(width * 3).enumerate().for_each(|(y, row)| {
        let v = region[1] + (y as f32 + 0.5) / height as f32 * region[3];
        let dy = (v - 0.5) * 2.0;
        for (x, pixel) in row.chunks_exact_mut(3).enumerate() {
            let u = region[0] + (x as f32 + 0.5) / width as f32 * region[2];
            let dx = (u - 0.5) * 2.0;

            let (cx, cy) = if aspect >= 1.0 { (dx * aspect, dy) } else { (dx, dy / aspect) };
            let (ex, ey) = (dx + (cx - dx) * circle, dy + (cy - dy) * circle);
            let distance = (ex.abs().powf(power) + ey.abs().powf(power)).powf(1.0 / power) / 2f32.sqrt().powf(1.0 - 2.0 / power).max(1.0);
            let weight = smoothstep(start - soft * 0.5, start + soft * 0.5, distance);
            let factor = 2f32.powf(stops * weight);
            for value in pixel.iter_mut() {
                *value *= factor;
            }
        }
    });
}

pub fn grain(
    data: &mut [f32],
    width: usize,
    height: usize,
    region: [f32; 4],
    full: [f32; 2],
    settings: [f32; 3],
    weights: [f32; 3],
) {
    let [amount, size, roughness] = settings;
    if amount == 0.0 || width == 0 || height == 0 {
        return;
    }
    let strength = amount / 100.0 * 0.12;
    let cell = 1.0 + size / 100.0 * 4.0;
    let rough = roughness / 100.0;

    data.par_chunks_exact_mut(width * 3).enumerate().for_each(|(y, row)| {
        let fy = (region[1] + (y as f32 + 0.5) / height as f32 * region[3]) * full[1];
        for (x, pixel) in row.chunks_exact_mut(3).enumerate() {
            let fx = (region[0] + (x as f32 + 0.5) / width as f32 * region[2]) * full[0];

            let (ax, ay) = (0.8 * fx - 0.6 * fy, 0.6 * fx + 0.8 * fy);
            let (bx, by) = (0.28 * fx + 0.96 * fy, -0.96 * fx + 0.28 * fy);
            let coarse = value_noise(ax / cell, ay / cell);
            let fine = value_noise(bx / cell * 2.3 + 17.0, by / cell * 2.3 + 31.0);
            let noise = coarse * (1.0 - rough * 0.5) + fine * rough * 0.5;

            let luma = weights[0] * pixel[0] + weights[1] * pixel[1] + weights[2] * pixel[2];
            if luma <= 0.0 {
                continue;
            }

            let tone = luma.min(1.0).powf(1.0 / 2.2);
            let midtone = 4.0 * tone * (1.0 - tone);
            let grained = (tone + noise * strength * midtone).max(0.0);
            let factor = (grained / tone.max(1e-6)).powf(2.2);
            for value in pixel.iter_mut() {
                *value *= factor;
            }
        }
    });
}

fn value_noise(x: f32, y: f32) -> f32 {
    let (x0, y0) = (x.floor(), y.floor());
    let (fx, fy) = (x - x0, y - y0);
    let (sx, sy) = (fx * fx * (3.0 - 2.0 * fx), fy * fy * (3.0 - 2.0 * fy));
    let (ix, iy) = (x0 as i64, y0 as i64);
    let corner = |dx: i64, dy: i64| hash(ix + dx, iy + dy);
    let top = corner(0, 0) + (corner(1, 0) - corner(0, 0)) * sx;
    let bottom = corner(0, 1) + (corner(1, 1) - corner(0, 1)) * sx;
    top + (bottom - top) * sy
}

fn hash(x: i64, y: i64) -> f32 {
    let mut h = (x as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ (y as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F);
    h ^= h >> 31;
    h = h.wrapping_mul(0xBF58_476D_1CE4_E5B9);
    h ^= h >> 29;
    (h >> 40) as f32 / (1u64 << 24) as f32 * 2.0 - 1.0
}

fn smoothstep(low: f32, high: f32, value: f32) -> f32 {
    let t = ((value - low) / (high - low).max(1e-6)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[cfg(test)]
mod tests {
    use super::*;

    const WEIGHTS: [f32; 3] = [0.2126, 0.7152, 0.0722];

    fn flat(width: usize, height: usize, value: [f32; 3]) -> Vec<f32> {
        (0..width * height).flat_map(|_| value).collect()
    }

    #[test]
    fn calibration_keeps_grey_and_turns_red_towards_yellow() {
        let mut pixels = vec![0.5, 0.5, 0.5, 0.6, 0.1, 0.1];
        calibrate(&mut pixels, [60.0, 0.0, 0.0], [0.0; 3], 0.0, WEIGHTS);
        assert!(pixels[..3].iter().all(|v| (v - 0.5).abs() < 1e-4), "grey stays grey: {pixels:?}");
        assert!(pixels[4] > 0.1 + 0.02 && pixels[5] <= 0.1 + 1e-3, "red gained green, not blue: {pixels:?}");

        let mut desaturated = vec![0.6, 0.1, 0.1];
        calibrate(&mut desaturated, [0.0; 3], [-100.0, 0.0, 0.0], 0.0, WEIGHTS);
        assert!(desaturated[0] - desaturated[1] < 0.5 * 0.5, "less red: {desaturated:?}");

        let mut shadow = vec![0.05, 0.05, 0.05];
        calibrate(&mut shadow, [0.0; 3], [0.0; 3], 100.0, WEIGHTS);
        assert!(shadow[1] < 0.05 && shadow[0] == 0.05, "a magenta shadow: {shadow:?}");
    }

    #[test]
    fn vignette_darkens_corners_not_the_centre_and_tiles_agree() {
        let (w, h) = (64, 40);
        let settings = [-100.0, 50.0, 0.0, 50.0];
        let mut whole = flat(w, h, [0.5; 3]);
        vignette(&mut whole, w, h, [0.0, 0.0, 1.0, 1.0], [w as f32, h as f32], settings);
        let at = |data: &[f32], x: usize, y: usize, w: usize| data[(y * w + x) * 3];
        assert!((at(&whole, 32, 20, w) - 0.5).abs() < 0.01, "centre untouched");
        assert!(at(&whole, 0, 0, w) < 0.25, "corner darker");

        let mut half = flat(w / 2, h, [0.5; 3]);
        vignette(&mut half, w / 2, h, [0.5, 0.0, 0.5, 1.0], [w as f32, h as f32], settings);
        assert!((at(&half, 31, 5, w / 2) - at(&whole, 63, 5, w)).abs() < 1e-5);
    }

    #[test]
    fn grain_is_pinned_to_the_frame_and_spares_black() {
        let (w, h) = (32, 32);
        let settings = [80.0, 25.0, 50.0];
        let mut whole = flat(w, h, [0.18; 3]);
        grain(&mut whole, w, h, [0.0, 0.0, 1.0, 1.0], [w as f32, h as f32], settings, WEIGHTS);
        assert!(whole.iter().any(|v| (v - 0.18).abs() > 0.005), "some grain");

        let mut tile = flat(w / 2, h / 2, [0.18; 3]);
        grain(&mut tile, w / 2, h / 2, [0.5, 0.5, 0.5, 0.5], [w as f32, h as f32], settings, WEIGHTS);
        assert_eq!(tile[0], whole[(16 * w + 16) * 3], "a tile shows its own part of the grain");

        let mut black = flat(4, 4, [0.0; 3]);
        grain(&mut black, 4, 4, [0.0, 0.0, 1.0, 1.0], [4.0, 4.0], settings, WEIGHTS);
        assert!(black.iter().all(|v| *v == 0.0));
    }

    #[test]
    fn dehaze_deepens_a_hazy_frame_and_negative_adds_haze() {
        let (w, h) = (64, 64);

        let hazy: Vec<f32> = (0..w * h)
            .flat_map(|i| {
                let scene = if (i % w) < w / 2 { 0.1 } else { 0.5 };
                [scene * 0.5 + 0.3, scene * 0.5 + 0.3, scene * 0.5 + 0.32]
            })
            .collect();
        let spread = |data: &[f32]| data[(10 * w + 60) * 3] - data[(10 * w + 5) * 3];

        let mut clearer = hazy.clone();
        dehaze(&mut clearer, w, h, 100.0);
        assert!(spread(&clearer) > spread(&hazy) * 1.3, "{} vs {}", spread(&clearer), spread(&hazy));
        assert!(clearer[(10 * w + 5) * 3] < hazy[(10 * w + 5) * 3], "the dark side darker");

        let mut hazier = hazy.clone();
        dehaze(&mut hazier, w, h, -100.0);
        assert!(spread(&hazier) < spread(&hazy));

        let mut untouched = hazy.clone();
        dehaze(&mut untouched, w, h, 0.0);
        assert_eq!(untouched, hazy);
    }
}
