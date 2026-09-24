use image::{imageops, GrayImage, RgbImage};

use crate::Frame;

pub const MEASURE_EDGE: u32 = 512;

const HASH_WIDTH: u32 = 9;
const HASH_HEIGHT: u32 = 8;

pub fn of(image: &RgbImage) -> Frame {
    Frame {
        sharpness: sharpness(image),
        blown: blown(image),
        hash: hash(image),
        shape: shape(image),
        ..tone(image)
    }
}

pub fn sharpness(image: &RgbImage) -> f32 {
    let grey = reduced(image, MEASURE_EDGE);
    let (width, height) = (grey.width() as usize, grey.height() as usize);
    if width < 3 || height < 3 {
        return 0.0;
    }

    let values: Vec<f32> = grey.pixels().map(|pixel| pixel[0] as f32).collect();
    let mean = values.iter().sum::<f32>() / values.len() as f32;
    let variance = values.iter().map(|v| (v - mean).powi(2)).sum::<f32>() / values.len() as f32;

    let deviation = variance.sqrt();
    if deviation < 1e-3 {
        return 0.0;
    }

    let mut energy = 0.0f64;
    for y in 1..height - 1 {
        for x in 1..width - 1 {
            let at = |x: usize, y: usize| (values[y * width + x] - mean) / deviation;
            let laplacian = at(x - 1, y) + at(x + 1, y) + at(x, y - 1) + at(x, y + 1)
                - 4.0 * at(x, y);
            energy += (laplacian * laplacian) as f64;
        }
    }

    let counted = ((width - 2) * (height - 2)) as f64;
    (energy / counted).sqrt() as f32
}

pub fn blown(image: &RgbImage) -> f32 {
    let total = image.pixels().len();
    if total == 0 {
        return 0.0;
    }

    let clipped = image
        .pixels()
        .filter(|pixel| pixel.0.iter().any(|value| *value == 255))
        .count();

    clipped as f32 / total as f32
}

pub fn hash(image: &RgbImage) -> u64 {
    let grey = imageops::resize(
        &imageops::grayscale(image),
        HASH_WIDTH,
        HASH_HEIGHT,
        imageops::FilterType::Triangle,
    );

    let mut bits = 0u64;
    for y in 0..HASH_HEIGHT {
        for x in 0..HASH_WIDTH - 1 {
            let left = grey.get_pixel(x, y)[0];
            let right = grey.get_pixel(x + 1, y)[0];
            bits = (bits << 1) | u64::from(left > right);
        }
    }
    bits
}

pub fn shape(image: &RgbImage) -> u64 {
    const N: usize = 32;
    const LOW: usize = 8;
    let grey = imageops::resize(&imageops::grayscale(image), N as u32, N as u32, imageops::FilterType::Lanczos3);
    let cosine: Vec<f32> = (0..LOW * N)
        .map(|at| {
            let (k, n) = (at / N, at % N);
            (std::f32::consts::PI * (2 * n + 1) as f32 * k as f32 / (2 * N) as f32).cos()
        })
        .collect();
    let pixel = |x: usize, y: usize| grey.get_pixel(x as u32, y as u32)[0] as f32;

    let mut low = Vec::with_capacity(LOW * LOW - 1);
    for u in 0..LOW {
        for v in 0..LOW {
            if u == 0 && v == 0 {
                continue;
            }
            let mut sum = 0.0f32;
            for y in 0..N {
                let row: f32 = (0..N).map(|x| cosine[v * N + x] * pixel(x, y)).sum();
                sum += cosine[u * N + y] * row;
            }
            low.push(sum);
        }
    }
    let mut sorted = low.clone();
    sorted.sort_by(f32::total_cmp);
    let median = sorted[sorted.len() / 2];
    low.iter().fold(0u64, |bits, value| (bits << 1) | u64::from(*value > median))
}

pub fn tone(image: &RgbImage) -> Frame {
    let count = image.pixels().len() as f64;
    if count == 0.0 {
        return Frame::default();
    }

    let (mut luma, mut luma2) = (0.0f64, 0.0f64);
    let (mut rg, mut rg2, mut yb, mut yb2) = (0.0f64, 0.0f64, 0.0f64, 0.0f64);
    for pixel in image.pixels() {
        let [r, g, b] = pixel.0.map(|value| value as f64 / 255.0);
        let y = 0.2126 * r + 0.7152 * g + 0.0722 * b;
        let (a, c) = (r - g, 0.5 * (r + g) - b);
        luma += y;
        luma2 += y * y;
        rg += a;
        rg2 += a * a;
        yb += c;
        yb2 += c * c;
    }

    let spread = |sum: f64, squares: f64| (squares / count - (sum / count).powi(2)).max(0.0).sqrt();
    let (mean_rg, mean_yb) = (rg / count, yb / count);
    Frame {
        brightness: (luma / count) as f32,
        contrast: spread(luma, luma2) as f32,
        colourfulness: (spread(rg, rg2).hypot(spread(yb, yb2)) + 0.3 * mean_rg.hypot(mean_yb)) as f32,
        ..Frame::default()
    }
}

fn reduced(image: &RgbImage, edge: u32) -> GrayImage {
    let grey = imageops::grayscale(image);
    if grey.width().max(grey.height()) <= edge {
        return grey;
    }

    let scale = edge as f32 / grey.width().max(grey.height()) as f32;
    imageops::resize(
        &grey,
        ((grey.width() as f32 * scale).round() as u32).max(1),
        ((grey.height() as f32 * scale).round() as u32).max(1),
        imageops::FilterType::Triangle,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn checkerboard(size: u32, cell: u32, high: u8, low: u8) -> RgbImage {
        RgbImage::from_fn(size, size, |x, y| {
            let on = ((x / cell) + (y / cell)) % 2 == 0;
            image::Rgb([if on { high } else { low }; 3])
        })
    }

    fn ramp(size: u32) -> RgbImage {
        RgbImage::from_fn(size, size, |x, _| {
            image::Rgb([(x * 255 / size.max(1)) as u8; 3])
        })
    }

    #[test]
    fn the_shape_hash_follows_the_scene_not_the_exposure() {
        let scene = RgbImage::from_fn(320, 240, |x, y| {
            let v = ((x / 40 + y / 60) % 3 * 90 + (x % 7) as u32) as u8;
            image::Rgb([v, v, v])
        });
        let brighter = RgbImage::from_fn(320, 240, |x, y| {
            let p = scene.get_pixel(x, y)[0];
            image::Rgb([p.saturating_add(30); 3])
        });
        let turned = imageops::rotate180(&scene);
        let far = |a: &RgbImage, b: &RgbImage| (shape(a) ^ shape(b)).count_ones();
        assert!(far(&scene, &brighter) <= 4, "exposure moved it {} bits", far(&scene, &brighter));
        assert!(far(&scene, &turned) >= 16, "a different picture only {} bits away", far(&scene, &turned));
    }

    #[test]
    fn detail_scores_above_a_smooth_gradient() {
        assert!(
            sharpness(&checkerboard(256, 2, 255, 0)) > sharpness(&ramp(256)) * 10.0,
            "a checkerboard must be far sharper than a ramp"
        );
    }

    #[test]
    fn contrast_does_not_pass_for_sharpness() {

        let strong = sharpness(&checkerboard(256, 2, 255, 0));
        let faint = sharpness(&checkerboard(256, 2, 140, 115));

        assert!(
            (strong - faint).abs() / strong < 0.05,
            "contrast leaked into the measure: {strong} against {faint}"
        );
    }

    #[test]
    fn a_coarser_pattern_is_less_detailed_than_a_fine_one() {
        let fine = sharpness(&checkerboard(256, 2, 255, 0));
        let coarse = sharpness(&checkerboard(256, 16, 255, 0));
        assert!(fine > coarse, "{fine} should beat {coarse}");
    }

    #[test]
    fn a_blank_frame_has_no_detail_rather_than_infinite_detail() {
        let blank = RgbImage::from_pixel(64, 64, image::Rgb([31, 31, 31]));
        assert_eq!(sharpness(&blank), 0.0);
        assert_eq!(sharpness(&RgbImage::new(1, 1)), 0.0, "too small to measure");
    }

    #[test]
    fn blown_counts_only_the_top_end() {
        let mut image = RgbImage::from_pixel(10, 10, image::Rgb([128, 128, 128]));
        assert_eq!(blown(&image), 0.0);

        for x in 0..5 {
            for y in 0..5 {
                image.put_pixel(x, y, image::Rgb([0, 0, 0]));
            }
        }
        assert_eq!(blown(&image), 0.0, "shadows are not blown highlights");

        for x in 0..10 {
            image.put_pixel(x, 9, image::Rgb([255, 200, 200]));
        }
        assert!((blown(&image) - 0.1).abs() < 1e-6);
    }

    #[test]
    fn tone_tells_grey_from_colour_and_flat_from_contrasty() {
        let grey = RgbImage::from_pixel(8, 8, image::Rgb([128, 128, 128]));
        let red = RgbImage::from_pixel(8, 8, image::Rgb([200, 30, 30]));
        let (grey, red) = (tone(&grey), tone(&red));
        assert!((grey.brightness - 0.5).abs() < 0.01);
        assert!(grey.contrast < 1e-4, "one value has no spread");
        assert!(grey.colourfulness < 1e-6 && red.colourfulness > 0.2, "{grey:?} against {red:?}");

        let board = tone(&checkerboard(16, 1, 255, 0));
        assert!(board.contrast > 0.45, "{board:?}");
    }

    #[test]
    fn the_hash_follows_the_scene_and_ignores_the_exposure() {
        let bright = checkerboard(128, 16, 255, 60);
        let dark = checkerboard(128, 16, 190, 10);
        assert_eq!(hash(&bright), hash(&dark), "a stop of exposure is the same scene");

        let other = ramp(128);
        assert!(
            crate::distance(hash(&bright), hash(&other)) > 8,
            "these should not look alike"
        );
    }
}
