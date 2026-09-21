use std::sync::OnceLock;

use image::RgbImage;
use numa_core::retouch::{Kind, Spot};
use numa_infer::Model;

pub const MODEL: &str = "yolox_s.onnx";

const SIDE: usize = 640;

const SURE: f32 = 0.35;

const SMALLER: f32 = 0.25;

const LARGEST: f32 = 0.1;

pub fn is_installed() -> bool {
    numa_core::paths::model_file(&[MODEL]).is_some()
}

fn model() -> Option<&'static Model> {
    static MODEL_ONCE: OnceLock<Option<Model>> = OnceLock::new();
    MODEL_ONCE.get_or_init(|| Model::load(&numa_core::paths::model_file(&[MODEL])?)).as_ref()
}

pub fn people(frame: &RgbImage) -> Option<Vec<Spot>> {
    let boxes = detect(frame)?;
    Some(passers_by(&boxes, frame.width() as f32, frame.height() as f32))
}

fn detect(frame: &RgbImage) -> Option<Vec<[f32; 4]>> {
    let model = model()?;

    let scale = (SIDE as f32 / frame.width() as f32).min(SIDE as f32 / frame.height() as f32);
    let (across, down) = ((frame.width() as f32 * scale) as u32, (frame.height() as f32 * scale) as u32);
    let fitted = image::imageops::resize(frame, across.max(1), down.max(1), image::imageops::FilterType::Triangle);
    let plane = SIDE * SIDE;
    let mut input = vec![114.0f32; 3 * plane];
    for (x, y, pixel) in fitted.enumerate_pixels() {
        let at = y as usize * SIDE + x as usize;
        for (channel, value) in [pixel[2], pixel[1], pixel[0]].into_iter().enumerate() {
            input[channel * plane + at] = value as f32;
        }
    }
    let tensor = ndarray::Array4::from_shape_vec((1, 3, SIDE, SIDE), input).ok()?.into_dyn();
    let output = match model.run(vec![tensor.into()]) {
        Ok(output) => output.into_iter().next()?,
        Err(err) => {
            log::warn!("YOLOX failed: {err}");
            return None;
        }
    };
    let values: Vec<f32> = output.iter().copied().collect();

    let mut found: Vec<(f32, [f32; 4])> = Vec::new();
    let mut row = 0;
    for stride in [8usize, 16, 32] {
        let cells = SIDE / stride;
        for cy in 0..cells {
            for cx in 0..cells {
                let prediction = values.get(row * 85..row * 85 + 85)?;
                row += 1;
                let score = prediction[4] * prediction[5];
                if score < SURE {
                    continue;
                }
                let centre = [(prediction[0] + cx as f32) * stride as f32, (prediction[1] + cy as f32) * stride as f32];
                let size = [prediction[2].exp() * stride as f32, prediction[3].exp() * stride as f32];
                let corners = [centre[0] - size[0] / 2.0, centre[1] - size[1] / 2.0, centre[0] + size[0] / 2.0, centre[1] + size[1] / 2.0];
                found.push((score, corners.map(|value| value / scale)));
            }
        }
    }

    found.sort_by(|a, b| b.0.total_cmp(&a.0));
    let mut kept: Vec<[f32; 4]> = Vec::new();
    for (_, candidate) in found {
        if kept.iter().all(|box_| overlap(box_, &candidate) < 0.45) {
            kept.push(candidate);
        }
    }
    Some(kept)
}

fn overlap(a: &[f32; 4], b: &[f32; 4]) -> f32 {
    let inner = (a[2].min(b[2]) - a[0].max(b[0])).max(0.0) * (a[3].min(b[3]) - a[1].max(b[1])).max(0.0);
    let area = |r: &[f32; 4]| (r[2] - r[0]) * (r[3] - r[1]);
    inner / (area(a) + area(b) - inner).max(1e-6)
}

fn passers_by(boxes: &[[f32; 4]], width: f32, height: f32) -> Vec<Spot> {
    let area = |r: &[f32; 4]| (r[2] - r[0]) * (r[3] - r[1]);
    let Some(subject) = boxes.iter().copied().max_by(|a, b| area(a).total_cmp(&area(b))) else { return Vec::new() };
    let long_edge = width.max(height);
    boxes
        .iter()
        .filter(|box_| area(box_) <= area(&subject) * SMALLER)

        .filter(|box_| {
            let (across, down) = ((subject[2] - subject[0]) / 4.0, (subject[3] - subject[1]) / 4.0);
            let centre = [(box_[0] + box_[2]) / 2.0, (box_[1] + box_[3]) / 2.0];
            !(centre[0] > subject[0] + across && centre[0] < subject[2] - across && centre[1] > subject[1] + down && centre[1] < subject[3] - down)
        })
        .filter_map(|box_| {
            let radius = 0.55 * (box_[2] - box_[0]).hypot(box_[3] - box_[1]) / long_edge;
            let centre = [(box_[0] + box_[2]) / 2.0 / width, (box_[1] + box_[3]) / 2.0 / height];
            (radius <= LARGEST).then_some(Spot {
                at: centre,
                from: centre,
                radius,
                feather: 0.15,
                opacity: 1.0,
                heal: false,
                kind: Kind::Remove,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_subject_stays_and_the_passers_by_go() {
        let boxes = [[300.0, 100.0, 700.0, 900.0], [850.0, 300.0, 880.0, 390.0], [900.0, 310.0, 925.0, 385.0], [470.0, 450.0, 540.0, 620.0]];
        let spots = passers_by(&boxes, 1000.0, 1000.0);
        assert_eq!(spots.len(), 2, "{spots:?}");
        for spot in &spots {
            assert!(spot.at[0] > 0.8 && spot.kind == Kind::Remove);
            assert!(spot.radius > 0.04 && spot.radius < 0.06, "{}", spot.radius);
        }
    }

    #[test]
    fn two_subjects_stay() {
        let boxes = [[100.0, 100.0, 300.0, 700.0], [500.0, 120.0, 690.0, 690.0]];
        assert!(passers_by(&boxes, 1000.0, 800.0).is_empty());
    }
}
