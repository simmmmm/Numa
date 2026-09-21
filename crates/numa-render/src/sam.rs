use std::path::PathBuf;
use std::sync::OnceLock;

use image::{imageops, Rgb, RgbImage};
use numa_infer::Model;

use numa_core::mask::Alpha;

const EDGE: usize = 1024;

const GRID: usize = 256;

const MEAN: [f32; 3] = [123.675, 116.28, 103.53];
const STD: [f32; 3] = [58.395, 57.12, 57.375];

const ENCODER: &[&str] = &["sam_encoder.onnx", "vision_encoder.onnx"];
const DECODER: &[&str] = &["sam_decoder.onnx", "prompt_encoder_mask_decoder.onnx"];

fn model_path(names: &[&str]) -> Option<PathBuf> {
    numa_core::paths::model_file(names)
}

pub fn is_installed() -> bool {
    model_path(ENCODER).is_some() && model_path(DECODER).is_some()
}

fn encoder() -> Option<&'static Model> {
    static PLAN: OnceLock<Option<Model>> = OnceLock::new();
    PLAN.get_or_init(|| Model::load(&model_path(ENCODER)?)).as_ref()
}

fn decoder() -> Option<&'static Model> {
    static PLAN: OnceLock<Option<Model>> = OnceLock::new();
    PLAN.get_or_init(|| Model::load(&model_path(DECODER)?)).as_ref()
}

pub struct Embedding {
    image: ndarray::ArrayD<f32>,
    positional: ndarray::ArrayD<f32>,

    covered: (f32, f32),
}

pub fn encode(photo: &RgbImage) -> Option<Embedding> {
    let plan = encoder()?;
    let (width, height) = (photo.width(), photo.height());
    if width == 0 || height == 0 {
        return None;
    }

    let scale = EDGE as f32 / width.max(height) as f32;
    let fitted = (
        ((width as f32 * scale).round() as u32).clamp(1, EDGE as u32),
        ((height as f32 * scale).round() as u32).clamp(1, EDGE as u32),
    );
    let small = imageops::resize(photo, fitted.0, fitted.1, imageops::FilterType::Triangle);
    let mut padded = RgbImage::from_pixel(EDGE as u32, EDGE as u32, Rgb([0, 0, 0]));
    imageops::replace(&mut padded, &small, 0, 0);

    let mut input = ndarray::Array4::<f32>::zeros((1, 3, EDGE, EDGE));
    for (x, y, pixel) in padded.enumerate_pixels() {
        for channel in 0..3 {
            input[[0, channel, y as usize, x as usize]] =
                (pixel[channel] as f32 - MEAN[channel]) / STD[channel];
        }
    }

    let outputs = match plan.run(vec![input.into_dyn().into()]) {
        Ok(outputs) => outputs,
        Err(err) => {
            log::warn!("sam encoder failed: {err}");
            return None;
        }
    };
    Some(Embedding {
        image: outputs.first()?.clone(),
        positional: outputs.get(1)?.clone(),
        covered: (fitted.0 as f32, fitted.1 as f32),
    })
}

impl Embedding {

    pub fn at(&self, u: f32, v: f32) -> Option<Alpha> {
        let plan = decoder()?;

        let point = ndarray::Array4::<f32>::from_shape_vec(
            (1, 1, 1, 2),
            vec![u * self.covered.0, v * self.covered.1],
        )
        .ok()?;

        let label = ndarray::Array3::<i64>::from_shape_vec((1, 1, 1), vec![1]).ok()?;

        let outputs = match plan.run(vec![
            point.into_dyn().into(),
            label.into_dyn().into(),
            self.image.clone().into(),
            self.positional.clone().into(),
        ]) {
            Ok(outputs) => outputs,
            Err(err) => {
                log::warn!("sam decoder failed: {err}");
                return None;
            }
        };

        let scores = outputs.first()?;
        let masks = outputs.get(1)?;
        if masks.shape() != [1, 1, 3, GRID, GRID] || scores.len() != 3 {
            log::warn!("the sam decoder answered with {:?}", masks.shape());
            return None;
        }

        let best = (0..3)
            .max_by(|a, b| scores[[0, 0, *a]].total_cmp(&scores[[0, 0, *b]]))
            .unwrap_or(0);

        let quarter = EDGE as f32 / GRID as f32;
        let width = ((self.covered.0 / quarter).round() as usize).clamp(1, GRID);
        let height = ((self.covered.1 / quarter).round() as usize).clamp(1, GRID);

        let mut alpha = Alpha::new(width, height, vec![0.0; width * height]);
        for y in 0..height {
            for x in 0..width {

                let logit = masks[[0, 0, best, y, x]];
                alpha.data[y * width + x] = (logit * 0.5 + 0.5).clamp(0.0, 1.0);
            }
        }
        Some(alpha)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_point_lands_where_the_photograph_is_on_the_square() {
        let photo = RgbImage::new(3000, 2000);
        let scale = EDGE as f32 / 3000.0;
        let covered = (3000.0 * scale, 2000.0 * scale);
        assert_eq!(covered.0, 1024.0);
        assert!((covered.1 - 682.7).abs() < 0.5, "{covered:?}");

        let middle = (0.5 * covered.0, 0.5 * covered.1);
        assert!(middle.1 < EDGE as f32 / 2.0);
        let _ = photo;
    }

    #[test]
    fn the_answer_is_cropped_to_the_photograph() {
        let quarter = EDGE as f32 / GRID as f32;
        let covered = (1024.0f32, 682.7f32);
        let width = ((covered.0 / quarter).round() as usize).clamp(1, GRID);
        let height = ((covered.1 / quarter).round() as usize).clamp(1, GRID);
        assert_eq!(width, GRID, "a landscape frame fills the width");
        assert_eq!(height, 171, "and two thirds of the height");
    }
}
