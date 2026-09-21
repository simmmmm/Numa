use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use image::{imageops, RgbImage};
use numa_infer::Model;

const EDGE: usize = 640;

const STRIDES: [usize; 3] = [8, 16, 32];

const SCORE: f32 = 0.9;
const OVERLAP: f32 = 0.3;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Face {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
    pub score: f32,

    pub landmarks: [(f32, f32); 5],
}

impl Face {
    fn area(&self) -> f32 {
        self.width * self.height
    }

    fn overlap(&self, other: &Face) -> f32 {
        let left = self.x.max(other.x);
        let top = self.y.max(other.y);
        let right = (self.x + self.width).min(other.x + other.width);
        let bottom = (self.y + self.height).min(other.y + other.height);

        let shared = (right - left).max(0.0) * (bottom - top).max(0.0);
        let union = self.area() + other.area() - shared;
        if union <= 0.0 {
            0.0
        } else {
            shared / union
        }
    }
}

pub fn model_dir() -> PathBuf {
    numa_core::paths::models_dir()
}

pub fn model_path() -> PathBuf {
    model_dir().join("face_detection_yunet_2023mar.onnx")
}

pub fn is_installed() -> bool {
    plan().is_some()
}

fn plan() -> Option<&'static Model> {
    static PLAN: OnceLock<Option<Model>> = OnceLock::new();
    PLAN.get_or_init(|| {
        let path = model_path();
        if !path.exists() {
            return None;
        }

        Model::load(&path)
    })
    .as_ref()
}

pub fn detect(image: &RgbImage) -> Option<Vec<Face>> {
    let plan = plan()?;

    let scale = EDGE as f32 / image.width().max(image.height()) as f32;
    let (width, height) = (
        ((image.width() as f32 * scale).round() as u32).clamp(1, EDGE as u32),
        ((image.height() as f32 * scale).round() as u32).clamp(1, EDGE as u32),
    );
    let resized = imageops::resize(image, width, height, imageops::FilterType::Triangle);

    let mut input = ndarray::Array4::<f32>::zeros((1, 3, EDGE, EDGE));
    for (x, y, pixel) in resized.enumerate_pixels() {
        input[[0, 0, y as usize, x as usize]] = pixel[2] as f32;
        input[[0, 1, y as usize, x as usize]] = pixel[1] as f32;
        input[[0, 2, y as usize, x as usize]] = pixel[0] as f32;
    }

    let outputs = match plan.run(vec![input.into_dyn().into()]) {
        Ok(outputs) => outputs,
        Err(err) => {
            log::warn!("face detection failed: {err}");
            return Some(Vec::new());
        }
    };

    let mut faces = Vec::new();
    for (index, stride) in STRIDES.iter().enumerate() {
        let cls = &outputs[index];
        let obj = &outputs[index + 3];
        let boxes = &outputs[index + 6];
        let points = &outputs[index + 9];

        let columns = EDGE / stride;
        for anchor in 0..cls.len() {

            let confidence = (cls[[0, anchor, 0]] * obj[[0, anchor, 0]]).max(0.0).sqrt();
            if confidence < SCORE {
                continue;
            }

            let (column, row) = ((anchor % columns) as f32, (anchor / columns) as f32);
            let stride = *stride as f32;

            let centre_x = (column + boxes[[0, anchor, 0]]) * stride;
            let centre_y = (row + boxes[[0, anchor, 1]]) * stride;
            let box_width = boxes[[0, anchor, 2]].exp() * stride;
            let box_height = boxes[[0, anchor, 3]].exp() * stride;

            let mut landmarks = [(0.0f32, 0.0f32); 5];
            for (point, slot) in landmarks.iter_mut().enumerate() {
                *slot = (
                    ((column + points[[0, anchor, point * 2]]) * stride) / scale,
                    ((row + points[[0, anchor, point * 2 + 1]]) * stride) / scale,
                );
            }

            faces.push(Face {
                x: (centre_x - box_width / 2.0) / scale,
                y: (centre_y - box_height / 2.0) / scale,
                width: box_width / scale,
                height: box_height / scale,
                score: confidence,
                landmarks,
            });
        }
    }

    Some(suppress(faces))
}

fn suppress(mut faces: Vec<Face>) -> Vec<Face> {
    faces.sort_by(|a, b| b.score.total_cmp(&a.score));

    let mut kept: Vec<Face> = Vec::new();
    for face in faces {
        if kept.iter().all(|other| face.overlap(other) < OVERLAP) {
            kept.push(face);
        }
    }
    kept
}

pub fn sharpest_face(image: &RgbImage, faces: &[Face]) -> Option<f32> {
    let face = faces.iter().max_by(|a, b| a.area().total_cmp(&b.area()))?;

    let margin = 0.15;
    let left = (face.x - face.width * margin).max(0.0) as u32;
    let top = (face.y - face.height * margin).max(0.0) as u32;
    let right = ((face.x + face.width * (1.0 + margin)) as u32).min(image.width());
    let bottom = ((face.y + face.height * (1.0 + margin)) as u32).min(image.height());

    if right.saturating_sub(left) < 32 || bottom.saturating_sub(top) < 32 {
        return None;
    }

    let crop = imageops::crop_imm(image, left, top, right - left, bottom - top).to_image();
    Some(super::measure::sharpness(&crop))
}

pub const SOURCE: &str = "dev/fetch-models.sh";

pub fn model_missing_message() -> String {
    format!(
        "No face model installed. Run {SOURCE} to put it in {}",
        model_dir().display()
    )
}

pub fn expected_file() -> &'static Path {
    Path::new("face_detection_yunet_2023mar.onnx")
}

#[cfg(test)]
mod tests {
    use super::*;

    fn face(x: f32, y: f32, size: f32, score: f32) -> Face {
        Face { x, y, width: size, height: size, score, landmarks: [(0.0, 0.0); 5] }
    }

    #[test]
    fn overlapping_detections_collapse_to_the_best_one() {

        let faces = vec![
            face(10.0, 10.0, 100.0, 0.91),
            face(14.0, 12.0, 98.0, 0.97),
            face(8.0, 9.0, 104.0, 0.93),
            face(400.0, 300.0, 80.0, 0.95),
        ];

        let kept = suppress(faces);
        assert_eq!(kept.len(), 2, "one face reported three times");
        assert_eq!(kept[0].score, 0.97, "the strongest detection survives");
        assert!(kept.iter().any(|face| face.x == 400.0), "the second face is not a duplicate");
    }

    #[test]
    fn overlap_is_intersection_over_union() {
        let a = face(0.0, 0.0, 10.0, 1.0);
        assert!((a.overlap(&a) - 1.0).abs() < 1e-6);
        assert_eq!(a.overlap(&face(100.0, 100.0, 10.0, 1.0)), 0.0);

        let half = a.overlap(&face(5.0, 0.0, 10.0, 1.0));
        assert!((half - 1.0 / 3.0).abs() < 1e-5, "got {half}");
    }

    #[test]
    fn a_face_too_small_to_judge_is_not_judged() {
        let image = RgbImage::from_pixel(200, 200, image::Rgb([128, 128, 128]));
        assert_eq!(sharpest_face(&image, &[face(10.0, 10.0, 20.0, 0.95)]), None);
        assert_eq!(sharpest_face(&image, &[]), None, "no faces is not a score of zero");
    }

    #[test]
    fn the_largest_face_is_the_subject() {

        let image = RgbImage::from_fn(300, 300, |x, y| {
            let detailed = x < 120 && y < 120 && (x + y) % 2 == 0;
            image::Rgb([if detailed { 230 } else { 30 }; 3])
        });

        let big = sharpest_face(&image, &[face(0.0, 0.0, 110.0, 0.9)]).unwrap();
        let small = sharpest_face(&image, &[face(180.0, 180.0, 110.0, 0.9)]).unwrap();
        assert!(small == 0.0, "a flat crop has no detail, got {small}");
        assert!(big > 1.0, "the detailed crop should be far from flat, got {big}");

        let both = sharpest_face(
            &image,
            &[face(180.0, 180.0, 110.0, 0.9), face(0.0, 0.0, 160.0, 0.9)],
        )
        .unwrap();
        assert!(both > small, "the subject is the near one: {both} against {small}");
    }
}
