use std::path::PathBuf;
use std::sync::OnceLock;

use image::{imageops, RgbImage};
use numa_infer::Model;

use crate::faces::Face;

const EDGE: u32 = 32;

const CROP: f32 = 0.28;

const CLOSED_BELOW: f32 = 0.2;
const OPEN_ABOVE: f32 = 0.5;

const SMALLEST: f32 = 240.0;

const FRONTAL: f32 = 0.3;

const SUNGLASSES: f32 = 0.5;

pub fn model_path() -> PathBuf {
    numa_core::paths::models_dir().join("open_closed_eye.onnx")
}

fn plan() -> Option<&'static Model> {
    static PLAN: OnceLock<Option<Model>> = OnceLock::new();
    PLAN.get_or_init(|| {
        let path = model_path();
        path.exists().then(|| Model::load(&path)).flatten()
    })
    .as_ref()
}

pub fn is_installed() -> bool {
    plan().is_some()
}

pub fn closed(frame: &RgbImage, face: &Face, scale: f32) -> Option<bool> {
    let width = face.width * scale;
    let [(rx, ry), (lx, ly)] = [face.landmarks[0], face.landmarks[1]];
    if width < SMALLEST || (rx - lx).hypot(ry - ly) < FRONTAL * face.width {
        return None;
    }

    let face_light = mean_light(frame, face.x * scale, face.y * scale, width, face.height * scale)?;
    let side = width * CROP;
    let eyes = [(rx, ry), (lx, ly)].map(|(x, y)| (x * scale - side / 2.0, y * scale - side / 2.0));
    for (left, top) in eyes {
        if mean_light(frame, left, top, side, side)? < SUNGLASSES * face_light {
            return None;
        }
    }

    let plan = plan()?;
    let mut open = Vec::with_capacity(2);
    for (left, top) in eyes {
        let crop = imageops::crop_imm(frame, left as u32, top as u32, side as u32, side as u32).to_image();
        let small = imageops::resize(&crop, EDGE, EDGE, imageops::FilterType::Triangle);
        let mut input = ndarray::Array4::<f32>::zeros((1, 3, EDGE as usize, EDGE as usize));
        for (px, py, pixel) in small.enumerate_pixels() {
            for (channel, value) in [pixel[2], pixel[1], pixel[0]].into_iter().enumerate() {
                input[[0, channel, py as usize, px as usize]] = (value as f32 - 127.0) / 255.0;
            }
        }
        let outputs = plan.run(vec![input.into_dyn().into()]).ok()?;

        open.push(*outputs.first()?.iter().nth(1)?);
    }
    match (open[0], open[1]) {
        (a, b) if a < CLOSED_BELOW && b < CLOSED_BELOW => Some(true),
        (a, b) if a > OPEN_ABOVE && b > OPEN_ABOVE => Some(false),
        _ => None,
    }
}

fn mean_light(frame: &RgbImage, x: f32, y: f32, width: f32, height: f32) -> Option<f32> {
    let (left, top) = (x.max(0.0) as u32, y.max(0.0) as u32);
    let (right, bottom) = (((x + width) as u32).min(frame.width()), ((y + height) as u32).min(frame.height()));
    if right <= left + 2 || bottom <= top + 2 || x < 0.0 || y < 0.0 {
        return None;
    }
    let mut sum = 0.0f64;
    let mut count = 0u64;

    for py in (top..bottom).step_by(2) {
        for px in (left..right).step_by(2) {
            let [r, g, b] = frame.get_pixel(px, py).0;
            sum += 0.2126 * r as f64 + 0.7152 * g as f64 + 0.0722 * b as f64;
            count += 1;
        }
    }
    Some((sum / count.max(1) as f64) as f32)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn face(width: f32, eyes_apart: f32) -> Face {
        let (cx, cy) = (500.0, 500.0);
        Face {
            x: cx - width / 2.0,
            y: cy - width / 2.0,
            width,
            height: width,
            score: 0.95,
            landmarks: [(cx - eyes_apart / 2.0, cy), (cx + eyes_apart / 2.0, cy), (cx, cy + 10.0), (cx - 10.0, cy + 20.0), (cx + 10.0, cy + 20.0)],
        }
    }

    #[test]
    fn a_face_that_cannot_be_read_is_not_read() {
        let frame = RgbImage::from_pixel(1000, 1000, image::Rgb([180, 150, 130]));
        assert_eq!(closed(&frame, &face(30.0, 12.0), 1.0), None, "too small");
        assert_eq!(closed(&frame, &face(400.0, 60.0), 1.0), None, "a profile's eyes are close together");

        let mut shades = frame.clone();
        for (x, y, pixel) in shades.enumerate_pixels_mut() {
            let near_eye = (y as i32 - 500).abs() < 70 && ((x as i32 - 440).abs() < 70 || (x as i32 - 560).abs() < 70);
            if near_eye {
                *pixel = image::Rgb([20, 20, 25]);
            }
        }
        assert_eq!(closed(&shades, &face(400.0, 120.0), 1.0), None, "sunglasses are not closed eyes");
    }
}
