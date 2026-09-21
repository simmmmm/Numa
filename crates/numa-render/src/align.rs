use rayon::prelude::*;

use numa_core::image::LinearImage;

const LEVELS: usize = 6;

const FENCE: u8 = 4;

struct Bitmap {
    width: usize,
    height: usize,
    bits: Vec<u8>,
}

pub fn offsets(images: &[&LinearImage], reference: usize) -> Vec<(i32, i32)> {
    let pyramids: Vec<Vec<Bitmap>> = images.par_iter().map(|image| pyramid(image)).collect();
    let target = &pyramids[reference];

    pyramids
        .par_iter()
        .enumerate()
        .map(|(index, pyramid)| {
            if index == reference {
                return (0, 0);
            }
            let mut offset = (0, 0);
            for level in (0..LEVELS).rev() {
                let centre = (offset.0 * 2, offset.1 * 2);

                offset = centre;
                let mut best = disagreement(&target[level], &pyramid[level], centre, centre);
                for dy in -1..=1 {
                    for dx in -1..=1 {
                        let candidate = (centre.0 + dx, centre.1 + dy);
                        let error = disagreement(&target[level], &pyramid[level], candidate, centre);
                        if error < best {
                            best = error;
                            offset = candidate;
                        }
                    }
                }
            }
            offset
        })
        .collect()
}

fn pyramid(image: &LinearImage) -> Vec<Bitmap> {
    let mut grey = grey(image);
    let (mut width, mut height) = (image.width as usize, image.height as usize);
    let mut levels = Vec::with_capacity(LEVELS);

    for level in 0..LEVELS {
        levels.push(bitmap(&grey, width, height));
        if level + 1 < LEVELS {
            (grey, width, height) = halve(&grey, width, height);
        }
    }
    levels
}

fn grey(image: &LinearImage) -> Vec<u8> {
    let luma = |pixel: &[f32]| 0.2126 * pixel[0] + 0.7152 * pixel[1] + 0.0722 * pixel[2];

    let mut sample: Vec<f32> = image.data.chunks_exact(3).step_by(16).map(luma).collect();
    let middle = sample.len() / 2;
    let median = if sample.is_empty() {
        1.0
    } else {
        *sample.select_nth_unstable_by(middle, f32::total_cmp).1
    };
    let scale = 0.18 / median.max(1e-6);

    let width = image.width as usize;
    let mut grey = vec![0u8; image.data.len() / 3];
    if width == 0 {
        return grey;
    }
    grey.par_chunks_mut(width)
        .zip(image.data.par_chunks(width * 3))
        .for_each(|(row, pixels)| {
            for (value, pixel) in row.iter_mut().zip(pixels.chunks_exact(3)) {
                let linear = (luma(pixel) * scale).clamp(0.0, 1.0);
                *value = (linear.powf(1.0 / 2.2) * 255.0).round() as u8;
            }
        });
    grey
}

fn halve(grey: &[u8], width: usize, height: usize) -> (Vec<u8>, usize, usize) {
    let (half_width, half_height) = (width / 2, height / 2);
    let mut half = vec![0u8; half_width * half_height];
    if half_width == 0 {
        return (half, half_width, half_height);
    }
    half.par_chunks_mut(half_width).enumerate().for_each(|(y, row)| {
        let (above, below) = (&grey[2 * y * width..], &grey[(2 * y + 1) * width..]);
        for (x, value) in row.iter_mut().enumerate() {
            let sum = above[2 * x] as u16 + above[2 * x + 1] as u16 + below[2 * x] as u16 + below[2 * x + 1] as u16;
            *value = ((sum + 2) / 4) as u8;
        }
    });
    (half, half_width, half_height)
}

fn bitmap(grey: &[u8], width: usize, height: usize) -> Bitmap {
    let mut histogram = [0usize; 256];
    for &value in grey {
        histogram[value as usize] += 1;
    }
    let mut seen = 0;
    let median = histogram
        .iter()
        .position(|&count| {
            seen += count;
            seen * 2 >= grey.len()
        })
        .unwrap_or(0) as u8;

    let bits = grey
        .par_iter()
        .map(|&value| (value > median) as u8 | ((value.abs_diff(median) > FENCE) as u8) << 1)
        .collect();
    Bitmap { width, height, bits }
}

fn disagreement(target: &Bitmap, moved: &Bitmap, offset: (i32, i32), centre: (i32, i32)) -> u64 {
    let span = |length: usize, other: usize, centre: i32| {
        let start = (1 - centre as i64).max(0);
        let end = (other as i64 - 1 - centre as i64).min(length as i64);
        (start as usize, end.max(start) as usize)
    };
    let (x0, x1) = span(target.width, moved.width, centre.0);
    let (y0, y1) = span(target.height, moved.height, centre.1);
    if x0 >= x1 || y0 >= y1 {
        return u64::MAX;
    }

    (y0..y1)
        .into_par_iter()
        .map(|y| {
            let row = &target.bits[y * target.width + x0..y * target.width + x1];
            let from = (y as i64 + offset.1 as i64) as usize * moved.width + (x0 as i64 + offset.0 as i64) as usize;
            let other = &moved.bits[from..from + row.len()];
            row.iter()
                .zip(other)
                .map(|(&a, &b)| ((a ^ b) & (a >> 1) & (b >> 1) & 1) as u32)
                .sum::<u32>() as u64
        })
        .sum()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scene(x: i32, y: i32) -> f32 {
        let hash = |a: i32, b: i32, salt: u32| {
            let mut h = (a as u32).wrapping_mul(0x9E37_79B1) ^ (b as u32).wrapping_mul(0x85EB_CA77) ^ salt;
            h ^= h >> 15;
            h = h.wrapping_mul(0x2C1B_3C6D);
            h ^= h >> 12;
            (h & 0xFFFF) as f32 / 65535.0
        };
        let coarse = hash(x.div_euclid(37), y.div_euclid(29), 1);
        let fine = hash(x.div_euclid(6), y.div_euclid(5), 2);
        let gradient = 0.5 + 0.5 * ((x as f32) * 0.01).sin() * ((y as f32) * 0.013).cos();
        0.02 + 0.5 * (0.5 * coarse + 0.3 * fine + 0.2 * gradient)
    }

    fn shot(shift: (i32, i32), exposure: f32) -> LinearImage {
        let (width, height) = (400, 300);
        let mut data = Vec::with_capacity(width * height * 3);
        for y in 0..height as i32 {
            for x in 0..width as i32 {
                let value = (scene(x - shift.0, y - shift.1) * exposure).min(1.0);
                data.extend_from_slice(&[value, value * 0.9, value * 1.1]);
            }
        }
        LinearImage::new(width as u32, height as u32, data)
    }

    #[test]
    fn recovers_known_shifts_across_exposures() {
        let reference = shot((0, 0), 1.0);
        let dark = shot((7, -3), 0.25);
        let bright = shot((-12, 5), 4.0);

        let found = offsets(&[&dark, &reference, &bright], 1);
        assert_eq!(found, vec![(7, -3), (0, 0), (-12, 5)]);
    }

    #[test]
    fn identical_frames_stay_put() {
        let frame = shot((0, 0), 1.0);
        let dark = shot((0, 0), 0.25);
        assert_eq!(offsets(&[&frame, &frame.clone(), &dark], 0), vec![(0, 0); 3]);
    }

    #[test]
    fn a_flat_frame_does_not_wander() {
        let flat = LinearImage::new(400, 300, vec![0.3; 400 * 300 * 3]);
        let reference = shot((0, 0), 1.0);
        assert_eq!(offsets(&[&reference, &flat], 0), vec![(0, 0), (0, 0)]);
    }

    #[test]
    fn a_handheld_bracket_merges_like_a_tripod_one() {
        use crate::bracket::{merge, Frame};
        let bracket = |shifts: [(i32, i32); 3]| {
            merge(&[
                Frame { image: shot(shifts[0], 0.25), exposure: 0.25 },
                Frame { image: shot(shifts[1], 1.0), exposure: 1.0 },
                Frame { image: shot(shifts[2], 4.0), exposure: 4.0 },
            ])
            .unwrap()
        };
        let tripod = bracket([(0, 0); 3]);
        let handheld = bracket([(7, -3), (0, 0), (-12, 5)]);

        for y in 10..290 {
            for x in 20..390 {
                let index = (y * 400 + x) * 3;
                assert_eq!(tripod.data[index], handheld.data[index], "at {x}, {y}");
            }
        }
    }

    #[test]
    fn frames_too_small_to_search_are_left_alone() {
        let one = LinearImage::new(1, 1, vec![0.5; 3]);
        assert_eq!(offsets(&[&one, &one.clone()], 1), vec![(0, 0), (0, 0)]);
    }
}
