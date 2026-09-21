use rayon::prelude::*;

use numa_core::image::LinearImage;
use crate::align;

const NOISE_FLOOR: f32 = 0.02;

const CLIP_START: f32 = 0.85;

pub struct Frame {
    pub image: LinearImage,

    pub exposure: f32,
}

fn smoothstep(edge0: f32, edge1: f32, value: f32) -> f32 {
    let t = ((value - edge0) / (edge1 - edge0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn weight(pixel: &[f32]) -> f32 {
    let high = pixel[0].max(pixel[1]).max(pixel[2]);
    let low = pixel[0].min(pixel[1]).min(pixel[2]);

    smoothstep(0.0, NOISE_FLOOR, low) * (1.0 - smoothstep(CLIP_START, 1.0, high))
}

pub fn merge(frames: &[Frame]) -> Result<LinearImage, String> {
    let Some(first) = frames.first() else {
        return Err("nothing to merge".to_string());
    };
    if frames.len() < 2 {
        return Err("a bracket needs at least two frames".to_string());
    }

    let (width, height) = (first.image.width, first.image.height);
    for frame in frames {
        if frame.image.width != width || frame.image.height != height {
            return Err(format!(
                "frames differ in size: {}x{} and {}x{}",
                width, height, frame.image.width, frame.image.height
            ));
        }
        if !(frame.exposure > 0.0) {
            return Err("a frame reports no exposure".to_string());
        }
    }

    let reference = frames
        .iter()
        .map(|frame| frame.exposure)
        .fold(f32::MIN, f32::max);

    let mut order: Vec<usize> = (0..frames.len()).collect();
    order.sort_by(|&a, &b| frames[a].exposure.total_cmp(&frames[b].exposure));

    let darkest = order[0];

    let anchor = order[frames.len() / 2];
    let images: Vec<&LinearImage> = frames.iter().map(|frame| &frame.image).collect();
    let shifts = align::offsets(&images, anchor);
    let (columns, rows) = (width as i64, height as i64);

    let mut data = vec![0.0f32; (width as usize) * (height as usize) * 3];

    data.par_chunks_exact_mut(3)
        .enumerate()
        .for_each(|(index, target)| {
            let (x, y) = ((index as i64) % columns, (index as i64) / columns);

            let at = |frame: usize| {
                let (fx, fy) = (x + shifts[frame].0 as i64, y + shifts[frame].1 as i64);
                ((0..columns).contains(&fx) && (0..rows).contains(&fy)).then(|| (fy * columns + fx) as usize * 3)
            };
            let mut sum = [0.0f32; 3];
            let mut total = 0.0f32;

            for (number, frame) in frames.iter().enumerate() {
                let Some(offset) = at(number) else { continue };
                let pixel = &frame.image.data[offset..offset + 3];
                let w = weight(pixel);
                if w <= 0.0 {
                    continue;
                }

                let scale = reference / frame.exposure;
                for channel in 0..3 {
                    sum[channel] += w * pixel[channel] * scale;
                }
                total += w;
            }

            if total > 1e-6 {
                for channel in 0..3 {
                    target[channel] = sum[channel] / total;
                }
            } else {

                let (fallback, offset) = at(darkest)
                    .map_or((&frames[anchor], index * 3), |offset| (&frames[darkest], offset));
                let scale = reference / fallback.exposure;
                for channel in 0..3 {
                    target[channel] = fallback.image.data[offset + channel] * scale;
                }
            }
        });

    Ok(LinearImage {
        white_point: None,
        width,
        height,
        data,
        profile: first.image.profile,

        clip: None,
        rendering: first.image.rendering.clone(),
        film_mode: first.image.film_mode.clone(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_merge_keeps_the_colour_description() {
        let mut bright = frame(&[[0.4, 0.4, 0.4]], 1.0);
        bright.image.film_mode = Some("Classic Chrome".to_string());
        let dark = frame(&[[0.1, 0.1, 0.1]], 0.25);

        let merged = merge(&[bright, dark]).unwrap();
        assert_eq!(merged.film_mode.as_deref(), Some("Classic Chrome"));
    }

    fn frame(values: &[[f32; 3]], exposure: f32) -> Frame {
        let data: Vec<f32> = values.iter().flatten().copied().collect();
        Frame {
            image: LinearImage::new(values.len() as u32, 1, data),
            exposure,
        }
    }

    #[test]
    fn rejects_what_it_cannot_merge() {
        assert!(merge(&[]).is_err());
        assert!(merge(&[frame(&[[0.5; 3]], 1.0)]).is_err(), "one frame is not a bracket");

        let mismatched = [frame(&[[0.5; 3]], 1.0), frame(&[[0.5; 3], [0.5; 3]], 2.0)];
        assert!(merge(&mismatched).unwrap_err().contains("differ in size"));

        let no_exposure = [frame(&[[0.5; 3]], 1.0), frame(&[[0.5; 3]], 0.0)];
        assert!(merge(&no_exposure).unwrap_err().contains("no exposure"));
    }

    #[test]
    fn agreeing_frames_land_on_the_same_radiance() {

        let merged = merge(&[
            frame(&[[0.4; 3]], 4.0),
            frame(&[[0.1; 3]], 1.0),
        ])
        .unwrap();

        assert!((merged.data[0] - 0.4).abs() < 1e-4, "got {}", merged.data[0]);
    }

    #[test]
    fn the_dark_frame_rescues_a_clipped_highlight() {

        let merged = merge(&[
            frame(&[[1.0; 3]], 4.0),
            frame(&[[0.5; 3]], 1.0),
        ])
        .unwrap();

        assert!(merged.data[0] > 1.8, "clipped highlight not recovered: {}", merged.data[0]);
    }

    #[test]
    fn the_bright_frame_rescues_a_noisy_shadow() {

        let merged = merge(&[
            frame(&[[0.24; 3]], 4.0),
            frame(&[[0.001; 3]], 1.0),
        ])
        .unwrap();

        assert!((merged.data[0] - 0.24).abs() < 0.02, "shadow came from the noise: {}", merged.data[0]);
    }

    #[test]
    fn a_pixel_clipped_in_every_frame_still_reads_bright() {
        let merged = merge(&[
            frame(&[[1.0; 3]], 4.0),
            frame(&[[1.0; 3]], 1.0),
        ])
        .unwrap();

        assert!(merged.data[0] >= 1.0, "fell into a hole: {}", merged.data[0]);
        assert!(merged.data[0].is_finite());
    }

    #[test]
    fn frame_order_does_not_matter() {
        let forwards = merge(&[frame(&[[1.0; 3]], 4.0), frame(&[[0.5; 3]], 1.0)]).unwrap();
        let backwards = merge(&[frame(&[[0.5; 3]], 1.0), frame(&[[1.0; 3]], 4.0)]).unwrap();
        assert!((forwards.data[0] - backwards.data[0]).abs() < 1e-5);
    }
}
