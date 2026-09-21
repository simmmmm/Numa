use std::path::PathBuf;
use std::sync::OnceLock;

use image::{imageops, RgbImage};

use numa_core::mask::Alpha;
use numa_infer::Model;
use crate::matte;
use crate::segment::{Segmentation, MATTEABLE};

const ANIMAL: u16 = 126;

const EDGE: u32 = 224;

const MEAN: [f32; 3] = [0.485, 0.456, 0.406];
const STD: [f32; 3] = [0.229, 0.224, 0.225];

const CONFIDENT: f32 = 0.6;

const PAD: f32 = 0.25;

const INSIDE: f32 = 0.5;

const GROUPS: &[(&str, &[(usize, usize)])] = &[

    ("Bird", &[(7, 24), (80, 100), (127, 146)]),

    ("Dog", &[(151, 268)]),

    ("Cat", &[(281, 285)]),

    ("Horse", &[(339, 339)]),

    ("Cow", &[(345, 347)]),

    ("Sheep", &[(348, 349)]),
    ("Bear", &[(294, 297)]),

    ("Rabbit", &[(330, 332)]),

    ("Fish", &[(0, 6), (389, 397)]),

    ("Insect", &[(300, 326)]),
];

fn model_path() -> Option<PathBuf> {
    numa_core::paths::model_file(&["image_classification_ppresnet50_2022jan.onnx"])
}

fn plan() -> Option<&'static Model> {
    static PLAN: OnceLock<Option<Model>> = OnceLock::new();
    PLAN.get_or_init(|| Model::load(&model_path()?)).as_ref()
}

pub fn is_installed() -> bool {
    model_path().is_some()
}

#[derive(Debug, Clone, PartialEq)]
pub struct Guess {
    pub name: &'static str,
    pub confidence: f32,
}

pub fn animal(found: &Segmentation) -> Option<Guess> {
    let model = plan()?;
    let best = groups(model, found.photo(), &region(found)?)?.into_iter().next()?;
    log::debug!("the subject is most like {}, {:.0} %", best.name, best.confidence * 100.0);
    (best.confidence >= CONFIDENT).then_some(best)
}

fn region(found: &Segmentation) -> Option<Alpha> {
    let things = found.found();
    if things.iter().any(|thing| thing.classes.contains(&ANIMAL)) {

        let coarse = found.coarse(&[ANIMAL]);
        let peak = coarse.data.iter().copied().fold(0.0f32, f32::max).max(f32::EPSILON);
        let data = coarse.data.iter().map(|value| value / peak).collect();
        return Some(Alpha::new(coarse.width, coarse.height, data));
    }
    if things.iter().any(|thing| thing.classes.iter().any(|c| MATTEABLE.contains(c))) {
        return None;
    }

    let photo = found.photo();
    matte::subject(photo, (photo.width() / 4).max(1) as usize, (photo.height() / 4).max(1) as usize)
}

fn groups(model: &Model, photo: &RgbImage, region: &Alpha) -> Option<Vec<Guess>> {
    let probability = probabilities(model, &crop(photo, region)?)?;
    let mut groups: Vec<Guess> = GROUPS
        .iter()
        .map(|(name, ranges)| Guess {
            name,
            confidence: ranges.iter().flat_map(|(a, b)| &probability[*a..=*b]).sum(),
        })
        .collect();
    groups.sort_by(|a, b| b.confidence.total_cmp(&a.confidence));
    Some(groups)
}

fn crop(photo: &RgbImage, region: &Alpha) -> Option<RgbImage> {
    let (mut left, mut top, mut right, mut bottom) = (usize::MAX, usize::MAX, 0, 0);
    for (index, value) in region.data.iter().enumerate() {
        if *value >= INSIDE {
            let (x, y) = (index % region.width, index / region.width);
            left = left.min(x);
            top = top.min(y);
            right = right.max(x);
            bottom = bottom.max(y);
        }
    }
    if left > right || top > bottom {
        return None;
    }

    let scale_x = photo.width() as f32 / region.width as f32;
    let scale_y = photo.height() as f32 / region.height as f32;
    let (x0, x1) = (left as f32 * scale_x, (right + 1) as f32 * scale_x);
    let (y0, y1) = (top as f32 * scale_y, (bottom + 1) as f32 * scale_y);
    let side = ((x1 - x0).max(y1 - y0) * (1.0 + 2.0 * PAD))
        .min(photo.width().min(photo.height()) as f32)
        .max(1.0);
    let place = |centre: f32, limit: u32| (centre - side / 2.0).clamp(0.0, limit as f32 - side) as u32;
    let (x, y) = (place((x0 + x1) / 2.0, photo.width()), place((y0 + y1) / 2.0, photo.height()));
    Some(imageops::crop_imm(photo, x, y, side as u32, side as u32).to_image())
}

fn probabilities(model: &Model, image: &RgbImage) -> Option<Vec<f32>> {
    let square = imageops::resize(image, EDGE, EDGE, imageops::FilterType::Triangle);
    let edge = EDGE as usize;
    let mut input = ndarray::Array4::<f32>::zeros((1, 3, edge, edge));
    for (x, y, pixel) in square.enumerate_pixels() {
        for channel in 0..3 {
            input[[0, channel, y as usize, x as usize]] =
                (pixel[channel] as f32 / 255.0 - MEAN[channel]) / STD[channel];
        }
    }
    let outputs = match model.run(vec![input.into_dyn().into()]) {
        Ok(outputs) => outputs,
        Err(err) => {
            log::warn!("classification failed: {err}");
            return None;
        }
    };
    let scores: Vec<f32> = outputs.first()?.iter().copied().collect();
    if scores.len() != 1000 {
        log::warn!("the classifier answered with {} scores, not ImageNet's thousand", scores.len());
        return None;
    }
    Some(softmax(scores))
}

fn softmax(scores: Vec<f32>) -> Vec<f32> {
    let peak = scores.iter().copied().fold(f32::MIN, f32::max);
    let exp: Vec<f32> = scores.iter().map(|s| (s - peak).exp()).collect();
    let total: f32 = exp.iter().sum();
    exp.into_iter().map(|e| e / total).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_groups_do_not_overlap_and_stay_inside_the_thousand() {
        let mut seen = [false; 1000];
        for (name, ranges) in GROUPS {
            for (a, b) in *ranges {
                assert!(a <= b && *b < 1000, "{name}");
                for (index, taken) in seen.iter_mut().enumerate().take(*b + 1).skip(*a) {
                    assert!(!*taken, "{name} claims {index} twice");
                    *taken = true;
                }
            }
        }
    }

    #[test]
    fn a_crop_is_square_and_inside_the_frame() {
        let photo = RgbImage::new(400, 200);

        let mut data = vec![0.0; 40 * 20];
        data[0] = 1.0;
        data[41] = 1.0;
        let square = crop(&photo, &Alpha::new(40, 20, data)).unwrap();
        assert_eq!(square.width(), square.height());
        assert!(square.width() <= 200);
        assert!(crop(&photo, &Alpha::new(40, 20, vec![0.0; 800])).is_none());
    }

    #[test]
    #[ignore]
    fn classifiers_on_frames() {
        let Ok(frames) = std::env::var("FRAMES") else { return };
        let labels: Vec<String> = std::env::var("LABELS")
            .map(|p| std::fs::read_to_string(p).unwrap().lines().map(|l| l.split(',').next().unwrap().to_string()).collect())
            .unwrap_or_default();
        let dir = numa_core::paths::models_dir();
        let models: Vec<(&str, Model)> = [
            ("mobilenetv2", "image_classification_mobilenetv2_2022apr.onnx"),
            ("ppresnet50", "image_classification_ppresnet50_2022jan.onnx"),
        ]
        .into_iter()
        .map(|(tag, file)| (tag, Model::load(&dir.join(file)).unwrap()))
        .collect();
        for path in frames.split(':') {
            let path = std::path::PathBuf::from(path);
            let linear = numa_io::raw::decode_linear_any(&path).unwrap();
            let document = numa_core::document::Document::new(path.display().to_string());
            let working = crate::to_working_space(&document, &linear, &Default::default());
            let frame = crate::apply_stack(&document, &working, 1.0);
            let found = crate::segment::of(&frame).expect("segmentation");
            let photo = found.photo();
            let name = path.file_name().unwrap().to_string_lossy().to_string();
            let animal: f32 = found.present().iter().filter(|(c, _)| *c == ANIMAL).map(|(_, s)| s).sum();
            let person: f32 = found.present().iter().filter(|(c, _)| *c == 12).map(|(_, s)| s).sum();

            let forced = std::env::var("MATTE").is_ok().then(|| {
                matte::subject(photo, photo.width() as usize / 4, photo.height() as usize / 4)
            });
            let Some(region) = forced.unwrap_or_else(|| region(&found)) else {
                println!("{name}: no chip to name (animal {:.2} %, person {:.2} %)", animal * 100.0, person * 100.0);
                continue;
            };
            println!("{name}: animal {:.2} %, person {:.2} %", animal * 100.0, person * 100.0);
            if let Ok(out) = std::env::var("OUT") {
                let name = path.file_stem().unwrap().to_string_lossy().to_string();
                crop(photo, &region).unwrap().save(format!("{out}/crop-{name}.png")).unwrap();
            }
            for (tag, model) in &models {
                let started = std::time::Instant::now();
                let probability = probabilities(model, &crop(photo, &region).unwrap()).unwrap();
                let elapsed = started.elapsed();
                let groups = groups(model, photo, &region).unwrap();
                let mut top: Vec<usize> = (0..1000).collect();
                top.sort_by(|a, b| probability[*b].total_cmp(&probability[*a]));
                let classes: Vec<String> = top[..3]
                    .iter()
                    .map(|i| format!("{} {:.2}", labels.get(*i).cloned().unwrap_or(i.to_string()), probability[*i]))
                    .collect();
                println!(
                    "  {tag:<12} {:>4} ms  {} {:.2}, {} {:.2}  | {}",
                    elapsed.as_millis(),
                    groups[0].name,
                    groups[0].confidence,
                    groups[1].name,
                    groups[1].confidence,
                    classes.join("; ")
                );
            }
        }
    }

    #[test]
    #[ignore]
    fn which_frames_have_animals() {
        let Ok(list) = std::env::var("PATHS") else { return };
        for path in std::fs::read_to_string(list).unwrap().lines() {
            let Ok(preview) = numa_io::raw::load_scaled(std::path::Path::new(path), 1024) else { continue };
            let Some(found) = crate::segment::of(&preview) else { continue };
            let present = found.present();
            let share = |c: u16| present.iter().find(|(k, _)| *k == c).map(|(_, s)| *s).unwrap_or(0.0);
            let (animal, person) = (share(126), share(12));
            if animal > 0.002 || person > 0.05 {
                println!("{path} animal {:.2} % person {:.2} %", animal * 100.0, person * 100.0);
            }
        }
    }
}
