use rayon::prelude::*;

pub struct Plane {
    pub width: usize,
    pub height: usize,
    pub data: Vec<f32>,
}

impl Plane {
    pub fn new(width: usize, height: usize, data: Vec<f32>) -> Self {
        debug_assert_eq!(data.len(), width * height);
        Self { width, height, data }
    }

    pub(crate) fn filled(width: usize, height: usize) -> Self {
        Self { width, height, data: vec![0.0; width * height] }
    }
}

pub(crate) fn blur_rows(source: &Plane, radius: usize) -> Plane {
    let (width, height) = (source.width, source.height);
    let mut out = Plane::filled(width, height);
    if width == 0 || height == 0 {
        return out;
    }
    let window = (2 * radius + 1) as f32;

    out.data
        .par_chunks_mut(width)
        .enumerate()
        .for_each(|(y, target)| {
            let row = &source.data[y * width..(y + 1) * width];
            let at = |x: isize| row[x.clamp(0, width as isize - 1) as usize];

            let mut sum: f32 = (-(radius as isize)..=radius as isize).map(at).sum();
            for x in 0..width {
                target[x] = sum / window;
                sum += at(x as isize + radius as isize + 1) - at(x as isize - radius as isize);
            }
        });

    out
}

fn blur_columns(source: &Plane, radius: usize) -> Plane {
    let (width, height) = (source.width, source.height);
    let mut out = Plane::filled(width, height);
    if width == 0 || height == 0 {
        return out;
    }
    let window = (2 * radius + 1) as f32;

    const STRIP: usize = 64;
    let starts: Vec<usize> = (0..width).step_by(STRIP).collect();
    let strips: Vec<Vec<f32>> = starts
        .par_iter()
        .map(|&x0| {
            let span = STRIP.min(width - x0);
            let row_at = |y: isize| {
                let y = y.clamp(0, height as isize - 1) as usize;
                &source.data[y * width + x0..y * width + x0 + span]
            };

            let mut sums = vec![0.0f32; span];
            for dy in -(radius as isize)..=radius as isize {
                for (sum, value) in sums.iter_mut().zip(row_at(dy)) {
                    *sum += value;
                }
            }
            let mut strip = vec![0.0f32; span * height];
            for y in 0..height {
                for (target, sum) in strip[y * span..(y + 1) * span].iter_mut().zip(&sums) {
                    *target = sum / window;
                }
                let arriving = row_at(y as isize + radius as isize + 1);
                let leaving = row_at(y as isize - radius as isize);
                for ((sum, add), sub) in sums.iter_mut().zip(arriving).zip(leaving) {
                    *sum += add - sub;
                }
            }
            strip
        })
        .collect();

    out.data.par_chunks_mut(width).enumerate().for_each(|(y, row)| {
        for (strip, &x0) in strips.iter().zip(&starts) {
            let span = STRIP.min(width - x0);
            row[x0..x0 + span].copy_from_slice(&strip[y * span..(y + 1) * span]);
        }
    });

    out
}

pub fn blur(source: &Plane, radius: usize) -> Plane {
    blur_columns(&blur_rows(source, radius), radius)
}

pub fn subsample(source: &Plane, factor: usize) -> Plane {
    let width = (source.width / factor).max(1);
    let height = (source.height / factor).max(1);
    let mut out = Plane::filled(width, height);

    out.data
        .par_chunks_mut(width)
        .enumerate()
        .for_each(|(y, row)| {
            for (x, target) in row.iter_mut().enumerate() {
                let mut sum = 0.0;
                let mut count = 0.0;
                for dy in 0..factor {
                    let sy = (y * factor + dy).min(source.height - 1);
                    for dx in 0..factor {
                        let sx = (x * factor + dx).min(source.width - 1);
                        sum += source.data[sy * source.width + sx];
                        count += 1.0;
                    }
                }
                *target = sum / count;
            }
        });

    out
}

pub fn upsample(source: &Plane, width: usize, height: usize) -> Plane {
    let mut out = Plane::filled(width, height);
    let scale_x = source.width as f32 / width as f32;
    let scale_y = source.height as f32 / height as f32;

    out.data
        .par_chunks_mut(width)
        .enumerate()
        .for_each(|(y, row)| {
            let fy = ((y as f32 + 0.5) * scale_y - 0.5).max(0.0);
            let y0 = (fy as usize).min(source.height - 1);
            let y1 = (y0 + 1).min(source.height - 1);
            let ty = fy - y0 as f32;

            for (x, target) in row.iter_mut().enumerate() {
                let fx = ((x as f32 + 0.5) * scale_x - 0.5).max(0.0);
                let x0 = (fx as usize).min(source.width - 1);
                let x1 = (x0 + 1).min(source.width - 1);
                let tx = fx - x0 as f32;

                let top = source.data[y0 * source.width + x0] * (1.0 - tx)
                    + source.data[y0 * source.width + x1] * tx;
                let bottom = source.data[y1 * source.width + x0] * (1.0 - tx)
                    + source.data[y1 * source.width + x1] * tx;
                *target = top * (1.0 - ty) + bottom * ty;
            }
        });

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn blur_columns_one_at_a_time(source: &Plane, radius: usize) -> Plane {
        let (width, height) = (source.width, source.height);
        let mut out = Plane::filled(width, height);
        let window = (2 * radius + 1) as f32;
        for x in 0..width {
            let at = |y: isize| source.data[y.clamp(0, height as isize - 1) as usize * width + x];
            let mut sum: f32 = (-(radius as isize)..=radius as isize).map(at).sum();
            for y in 0..height {
                out.data[y * width + x] = sum / window;
                sum += at(y as isize + radius as isize + 1) - at(y as isize - radius as isize);
            }
        }
        out
    }

    #[test]
    fn the_strip_blur_is_the_column_blur_to_the_bit() {
        let (width, height) = (131usize, 37usize);
        let data: Vec<f32> =
            (0..width * height).map(|i| ((i * 7919) % 1000) as f32 / 999.0 + (i % 3) as f32).collect();
        let plane = Plane::new(width, height, data);
        for radius in [0, 1, 3, 40, 100] {
            let strips = blur_columns(&plane, radius);
            let reference = blur_columns_one_at_a_time(&plane, radius);
            assert!(
                strips.data.iter().zip(&reference.data).all(|(a, b)| a.to_bits() == b.to_bits()),
                "radius {radius} differs"
            );
        }
    }

}
