use std::path::PathBuf;
use std::sync::OnceLock;

use image::{imageops, RgbImage};
use crate::infer::Model;

use crate::core::mask::Alpha;
use crate::render::local::{self, Plane};

const EDGE: usize = 1024;

const MEAN: [f32; 3] = [0.485, 0.456, 0.406];
const STD: [f32; 3] = [0.229, 0.224, 0.225];

const REFINE: usize = 2048;

const REFINE_RADIUS: f32 = 1.0 / 256.0;
const REFINE_EPSILON: f32 = 1e-5;

const CERTAINTY: f32 = 3.0;

pub fn model_path() -> PathBuf {

    let file = std::env::var("NUMA_SEGMENTER").unwrap_or_else(|_| "efficientvit_seg_b2_ade20k_1024.onnx".to_string());
    crate::cull::faces::model_dir().join(file)
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

pub struct Segmentation {

    width: usize,
    height: usize,

    probability: Vec<f32>,

    winner: Vec<u16>,

    guide: Plane,

    photo: RgbImage,
}

impl Segmentation {

    pub fn class_at(&self, u: f32, v: f32) -> u16 {
        let x = ((u * self.width as f32) as usize).min(self.width.saturating_sub(1));
        let y = ((v * self.height as f32) as usize).min(self.height.saturating_sub(1));
        self.winner.get(y * self.width + x).copied().unwrap_or(0)
    }

    pub fn present(&self) -> Vec<(u16, f32)> {
        let cells = self.winner.len().max(1) as f32;
        let mut counts = vec![0usize; LABELS.len()];
        for class in &self.winner {

            if let Some(count) = counts.get_mut(*class as usize) {
                *count += 1;
            }
        }
        let mut found: Vec<(u16, f32)> = counts
            .into_iter()
            .enumerate()
            .filter(|(_, count)| *count > 0)
            .map(|(class, count)| (class as u16, count as f32 / cells))
            .collect();
        found.sort_by(|a, b| b.1.total_cmp(&a.1));
        found
    }

    pub fn centre_of(&self, class: u16) -> Option<[f32; 2]> {
        let (mut x, mut y, mut n) = (0.0f64, 0.0f64, 0u32);
        for (cell, winner) in self.winner.iter().enumerate() {
            if *winner != class {
                continue;
            }
            x += (cell % self.width) as f64;
            y += (cell / self.width) as f64;
            n += 1;
        }
        (n > 0).then(|| {
            [
                ((x / n as f64 + 0.5) / self.width as f64) as f32,
                ((y / n as f64 + 0.5) / self.height as f64) as f32,
            ]
        })
    }

    pub fn photo(&self) -> &RgbImage {
        &self.photo
    }

    pub fn alpha(&self, classes: &[u16]) -> Alpha {

        self.refine(self.coarse(classes))
    }

    pub fn coarse(&self, classes: &[u16]) -> Plane {
        let mut coarse = Plane::new(
            self.width,
            self.height,
            vec![0.0; self.width * self.height],
        );
        for class in classes {
            let offset = *class as usize * self.width * self.height;
            let Some(plane) = self.probability.get(offset..offset + coarse.data.len()) else {
                continue;
            };

            for (total, value) in coarse.data.iter_mut().zip(plane) {
                *total += value;
            }
        }
        coarse
    }

    pub fn found(&self) -> Vec<Found> {
        let present = self.present();
        let share_of = |class: u16| {
            present.iter().find(|(c, _)| *c == class).map(|(_, share)| *share).unwrap_or(0.0)
        };

        let mut found: Vec<Found> = Vec::new();
        for (name, classes) in PRESETS {
            let here: Vec<u16> =
                classes.iter().copied().filter(|class| share_of(*class) > 0.001).collect();
            let share: f32 = here.iter().map(|class| share_of(*class)).sum();
            if share >= FOUND_FLOOR {
                found.push(Found { name: name.to_string(), classes: here, share });
            }
        }
        found.sort_by(|a, b| b.share.total_cmp(&a.share));
        found
    }

    pub fn region_at(&self, u: f32, v: f32) -> (u16, Alpha) {
        let class = self.class_at(u, v);
        let cells = self.width * self.height;
        let start = {
            let x = ((u * self.width as f32) as usize).min(self.width.saturating_sub(1));
            let y = ((v * self.height as f32) as usize).min(self.height.saturating_sub(1));
            y * self.width + x
        };

        let mut inside = vec![false; cells];
        let mut queue = vec![start];
        inside[start] = true;
        while let Some(cell) = queue.pop() {
            let (x, y) = (cell % self.width, cell / self.width);
            let mut visit = |x: usize, y: usize, queue: &mut Vec<usize>| {
                let next = y * self.width + x;
                if !inside[next] && self.winner[next] == class {
                    inside[next] = true;
                    queue.push(next);
                }
            };
            if x > 0 {
                visit(x - 1, y, &mut queue);
            }
            if x + 1 < self.width {
                visit(x + 1, y, &mut queue);
            }
            if y > 0 {
                visit(x, y - 1, &mut queue);
            }
            if y + 1 < self.height {
                visit(x, y + 1, &mut queue);
            }
        }

        let offset = class as usize * cells;
        let mut coarse = Plane::new(self.width, self.height, vec![0.0; cells]);
        for cell in 0..cells {
            if inside[cell] {
                coarse.data[cell] = self.probability.get(offset + cell).copied().unwrap_or(0.0);
            }
        }

        (class, self.refine(coarse))
    }

    fn refine(&self, coarse: Plane) -> Alpha {
        let (width, height) = (self.guide.width, self.guide.height);
        let upsampled = local::upsample(&coarse, width, height);
        let radius = ((width.max(height) as f32 * REFINE_RADIUS) as usize).max(1);
        let refined = local::guided_by(&self.guide, &upsampled, radius, REFINE_EPSILON);

        Alpha::new(
            width,
            height,
            refined
                .data
                .iter()
                .map(|value| {
                    let t = ((value - 0.5) * CERTAINTY + 0.5).clamp(0.0, 1.0);
                    t * t * (3.0 - 2.0 * t)
                })
                .collect(),
        )
    }
}

pub fn of(image: &RgbImage) -> Option<Segmentation> {
    let plan = plan()?;

    let square = imageops::resize(image, EDGE as u32, EDGE as u32, imageops::FilterType::Triangle);
    let mut input = ndarray::Array4::<f32>::zeros((1, 3, EDGE, EDGE));
    for (x, y, pixel) in square.enumerate_pixels() {
        for channel in 0..3 {
            input[[0, channel, y as usize, x as usize]] =
                (pixel[channel] as f32 / 255.0 - MEAN[channel]) / STD[channel];
        }
    }

    let outputs = match plan.run(vec![input.into_dyn().into()]) {
        Ok(outputs) => outputs,
        Err(err) => {
            log::warn!("segmentation failed: {err}");
            return None;
        }
    };

    let logits = &outputs[0];
    let shape = logits.shape();
    if shape.len() != 4 || shape[1] != LABELS.len() {
        log::warn!(
            "the segmentation model answered with {shape:?}, not a grid of {} classes",
            LABELS.len()
        );
        return None;
    }
    let (classes, height, width) = (shape[1], shape[2], shape[3]);
    let cells = width * height;

    let mut probability = vec![0.0f32; classes * cells];
    let mut winner = vec![0u16; cells];
    for cell in 0..cells {
        let (y, x) = (cell / width, cell % width);
        let mut highest = f32::MIN;
        let mut best = 0u16;
        for class in 0..classes {
            let value = logits[[0, class, y, x]];
            if value > highest {
                highest = value;
                best = class as u16;
            }
        }
        winner[cell] = best;

        let mut total = 0.0;
        for class in 0..classes {
            let value = (logits[[0, class, y, x]] - highest).exp();
            probability[class * cells + cell] = value;
            total += value;
        }
        if total > 0.0 {
            for class in 0..classes {
                probability[class * cells + cell] /= total;
            }
        }
    }

    Some(Segmentation {
        width,
        height,
        probability,
        winner,
        guide: luminance(image, REFINE),
        photo: fit(image, REFINE),
    })
}

fn fit(image: &RgbImage, long_edge: usize) -> RgbImage {
    let scale = long_edge as f32 / image.width().max(image.height()) as f32;
    let width = ((image.width() as f32 * scale).round() as u32).max(1);
    let height = ((image.height() as f32 * scale).round() as u32).max(1);

    imageops::resize(image, width, height, imageops::FilterType::Lanczos3)
}

fn luminance(image: &RgbImage, long_edge: usize) -> Plane {
    let scale = long_edge as f32 / image.width().max(image.height()) as f32;
    let (width, height) = (
        ((image.width() as f32 * scale).round() as u32).max(1),
        ((image.height() as f32 * scale).round() as u32).max(1),
    );
    let small = imageops::resize(image, width, height, imageops::FilterType::Triangle);

    Plane::new(
        width as usize,
        height as usize,
        small
            .pixels()
            .map(|p| {
                (0.2126 * p[0] as f32 + 0.7152 * p[1] as f32 + 0.0722 * p[2] as f32) / 255.0
            })
            .collect(),
    )
}

pub const MATTEABLE: [u16; 2] = [12, 126];

#[derive(Debug, Clone, PartialEq)]
pub struct Found {
    pub name: String,
    pub classes: Vec<u16>,
    pub share: f32,
}

const FOUND_FLOOR: f32 = 0.005;

pub const PRESETS: &[(&str, &[u16])] = &[
    ("Sky", &[2]),
    ("Buildings", &[0, 1, 25, 48]),

    ("Person", &[12]),
    ("Animal", &[126]),
    ("Greenery", &[4, 9, 17, 66, 72]),
    ("Ground", &[3, 6, 11, 13, 29, 46, 52]),
    ("Water", &[21, 26, 60, 113, 128]),
];

pub fn name_for(classes: &[u16]) -> String {
    if let Some((name, _)) = PRESETS.iter().find(|(_, preset)| *preset == classes) {
        return name.to_string();
    }

    let mut names: Vec<&str> = classes.iter().filter_map(|class| label(*class)).collect();
    match names.len() {
        0 => "Empty".to_string(),
        1..=2 => names.join(" + "),
        _ => {
            names.truncate(2);
            format!("{} + {} more", names.join(" + "), classes.len() - 2)
        }
    }
}

pub fn label(class: u16) -> Option<&'static str> {
    LABELS.get(class as usize).copied()
}

pub const LABELS: [&str; 150] = [
    "wall", "building", "sky", "floor", "tree", "ceiling", "road", "bed", "windowpane",
    "grass", "cabinet", "sidewalk", "person", "earth", "door", "table", "mountain", "plant",
    "curtain", "chair", "car", "water", "painting", "sofa", "shelf", "house", "sea",
    "mirror", "rug", "field", "armchair", "seat", "fence", "desk", "rock", "wardrobe",
    "lamp", "bathtub", "railing", "cushion", "base", "box", "column", "signboard",
    "chest of drawers", "counter", "sand", "sink", "skyscraper", "fireplace", "refrigerator",
    "grandstand", "path", "stairs", "runway", "case", "pool table", "pillow", "screen door",
    "stairway", "river", "bridge", "bookcase", "blind", "coffee table", "toilet", "flower",
    "book", "hill", "bench", "countertop", "stove", "palm", "kitchen island", "computer",
    "swivel chair", "boat", "bar", "arcade machine", "hovel", "bus", "towel", "light",
    "truck", "tower", "chandelier", "awning", "streetlight", "booth", "television receiver",
    "airplane", "dirt track", "apparel", "pole", "land", "bannister", "escalator",
    "ottoman", "bottle", "buffet", "poster", "stage", "van", "ship", "fountain",
    "conveyer belt", "canopy", "washer", "plaything", "swimming pool", "stool", "barrel",
    "basket", "waterfall", "tent", "bag", "minibike", "cradle", "oven", "ball", "food",
    "step", "tank", "trade name", "microwave", "pot", "animal", "bicycle", "lake",
    "dishwasher", "screen", "blanket", "sculpture", "hood", "sconce", "vase",
    "traffic light", "tray", "ashcan", "fan", "pier", "crt screen", "plate", "monitor",
    "bulletin board", "shower", "radiator", "glass", "clock", "flag",
];

#[cfg(test)]
mod tests {
    use super::*;

    fn drawn(width: usize, height: usize, winner: Vec<u16>) -> Segmentation {
        let cells = width * height;
        let mut probability = vec![0.0f32; LABELS.len() * cells];
        for (cell, class) in winner.iter().enumerate() {
            probability[*class as usize * cells + cell] = 1.0;
        }
        Segmentation {
            width,
            height,
            probability,
            winner,
            guide: Plane::new(width, height, vec![0.5; cells]),
            photo: RgbImage::new(width as u32, height as u32),
        }
    }

    #[test]
    fn what_was_found_is_named_and_ordered_by_size() {
        let (w, h) = (40usize, 20usize);

        let winner: Vec<u16> = (0..w * h)
            .map(|cell| {
                let (x, y) = (cell % w, cell / w);
                match (y < 10, x < 16, y == 19, cell == 799) {
                    (_, _, _, true) => 138,
                    (true, _, _, _) => 2,
                    (false, true, false, _) => 1,
                    (false, false, false, _) => 4,
                    (false, _, true, _) => 20,
                }
            })
            .collect();
        let found = drawn(w, h, winner).found();
        let names: Vec<&str> = found.iter().map(|f| f.name.as_str()).collect();

        assert_eq!(names[0], "Sky", "the biggest thing first: {names:?}");
        assert!(names.contains(&"Buildings"), "a preset name for a preset class: {names:?}");
        assert!(names.contains(&"Greenery"), "{names:?}");
        assert!(
            !names.iter().any(|n| n.eq_ignore_ascii_case("car")),
            "a class outside the groups is not a chip, whatever its share: {names:?}"
        );
        assert!(!names.iter().any(|n| n.eq_ignore_ascii_case("ashcan")), "one cell is a smudge: {names:?}");

        let buildings = found.iter().find(|f| f.name == "Buildings").unwrap();
        assert_eq!(buildings.classes, vec![1], "only the preset's classes that are actually here");
    }

    #[test]
    #[ignore]
    fn another_segmenter_on_a_frame() {
        let (Ok(model), Ok(photo), Ok(out)) =
            (std::env::var("MODEL"), std::env::var("PHOTO"), std::env::var("OUT"))
        else {
            return;
        };
        let edge: usize = std::env::var("SEG_EDGE").ok().and_then(|v| v.parse().ok()).unwrap_or(512);
        let class: usize = std::env::var("CLASS").ok().and_then(|v| v.parse().ok()).unwrap_or(126);
        let tag = std::env::var("TAG").unwrap_or("model".into());
        let photo = image::open(photo).unwrap().to_rgb8();

        let started = std::time::Instant::now();
        let plan = Model::load(std::path::Path::new(&model)).unwrap();
        println!("{tag}: planned in {:?}", started.elapsed());

        let square = imageops::resize(&photo, edge as u32, edge as u32, imageops::FilterType::Triangle);
        let mut input = ndarray::Array4::<f32>::zeros((1, 3, edge, edge));
        for (x, y, pixel) in square.enumerate_pixels() {
            for c in 0..3 {
                input[[0, c, y as usize, x as usize]] = (pixel[c] as f32 / 255.0 - MEAN[c]) / STD[c];
            }
        }
        let started = std::time::Instant::now();
        let outputs = plan.run(vec![input.into_dyn().into()]).unwrap();
        let logits = &outputs[0];
        let shape = logits.shape().to_vec();
        println!("{tag}: ran in {:?}, output {:?}", started.elapsed(), shape);
        let (classes, h, w) = (shape[1], shape[2], shape[3]);

        let mut counts = vec![0usize; classes];
        let mut alpha = image::GrayImage::new(w as u32, h as u32);
        for y in 0..h {
            for x in 0..w {
                let mut best = 0;
                let mut top = f32::MIN;
                let mut sum = 0.0f32;
                let mut chosen = 0.0f32;
                let peak = (0..classes).map(|c| logits[[0, c, y, x]]).fold(f32::MIN, f32::max);
                for c in 0..classes {
                    let v = logits[[0, c, y, x]];
                    if v > top {
                        top = v;
                        best = c;
                    }
                    let e = (v - peak).exp();
                    sum += e;
                    if c == class {
                        chosen = e;
                    }
                }
                counts[best] += 1;
                alpha.put_pixel(x as u32, y as u32, image::Luma([(chosen / sum * 255.0) as u8]));
            }
        }
        let cells = (w * h) as f32;
        let mut seen: Vec<(usize, f32)> = counts.iter().enumerate().map(|(c, n)| (c, *n as f32 / cells)).filter(|(_, f)| *f > 0.0005).collect();
        seen.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
        for (c, f) in seen.iter().take(10) {
            println!("  class {c:>3} {:<14} {:>6.2}%", label(*c as u16).unwrap_or("?"), f * 100.0);
        }
        imageops::resize(&alpha, photo.width(), photo.height(), imageops::FilterType::Triangle)
            .save(format!("{out}/seg-{tag}-class{class}.png"))
            .unwrap();
    }

    #[test]
    #[ignore]
    fn where_a_click_lands() {
        let Ok(path) = std::env::var("FRAME") else { return };
        let path = std::path::PathBuf::from(path);
        let linear = crate::io::raw::decode_linear(&path).unwrap();
        let document = crate::core::document::Document::new(path.display().to_string());
        let working = crate::render::to_working_space(&document, &linear);
        let frame = crate::render::apply_stack(&document, &working, 1.0);
        let Some(embedding) = crate::render::sam::encode(&frame) else { return };

        print!("        ");
        for column in 0..11 {
            print!("{:>6.2}", 0.30 + column as f32 * 0.04);
        }
        println!();
        for row in 0..9 {
            let v = 0.34 + row as f32 * 0.04;
            print!("v={v:>5.2} ");
            for column in 0..11 {
                let u = 0.30 + column as f32 * 0.04;
                let share = embedding
                    .at(u, v)
                    .map(|alpha| {
                        alpha.data.iter().map(|value| *value as f64).sum::<f64>()
                            / alpha.data.len() as f64
                            * 100.0
                    })
                    .unwrap_or(-1.0);
                print!("{share:>6.1}");
            }
            println!();
        }
    }

    #[test]
    #[ignore]
    fn what_is_found() {
        let Ok(path) = std::env::var("FRAME") else {
            println!("set FRAME to a RAW file");
            return;
        };
        let path = std::path::PathBuf::from(path);
        let linear = crate::io::raw::decode_linear(&path).unwrap();
        let document = crate::core::document::Document::new(path.display().to_string());
        let working = crate::render::to_working_space(&document, &linear);
        let frame = crate::render::apply_stack(&document, &working, 1.0);
        println!("frame {} x {}", frame.width(), frame.height());

        let found = of(&frame).expect("the semantic model");
        println!("\nwat het model ziet:");
        let mut present = found.present();
        present.sort_by(|a, b| b.1.total_cmp(&a.1));
        for (class, share) in present.iter().take(10) {
            println!("  {:>3} {:<28} {:>6.2}%", class, label(*class).unwrap_or("?"), share * 100.0);
        }

        let animal = found.alpha(&[126]);
        let covered: f64 = animal.data.iter().map(|v| *v as f64).sum::<f64>()
            / animal.data.len() as f64;
        println!("\n'Animal' (klasse 126) dekt {:.3}% van het beeld", covered * 100.0);

        let at: Vec<f32> = std::env::var("AT")
            .unwrap_or_else(|_| "0.45,0.42".to_string())
            .split(',')
            .filter_map(|n| n.trim().parse().ok())
            .collect();
        let (u, v) = (at[0], at[1]);
        println!("klik op ({u}, {v}) — klasse daar: {}", label(found.class_at(u, v)).unwrap_or("?"));

        let (class, region) = found.region_at(u, v);
        let flood: f64 =
            region.data.iter().map(|v| *v as f64).sum::<f64>() / region.data.len() as f64;
        println!("  flood fill van klasse {} ({}): {:.3}%", class, label(class).unwrap_or("?"), flood * 100.0);

        match crate::render::sam::encode(&frame) {
            Some(embedding) => match embedding.at(u, v) {
                Some(alpha) => {
                    let share: f64 =
                        alpha.data.iter().map(|v| *v as f64).sum::<f64>() / alpha.data.len() as f64;
                    println!("  SAM: {:.3}% van het beeld", share * 100.0);
                }
                None => println!("  SAM gaf niets terug"),
            },
            None => println!("  SAM niet geinstalleerd"),
        }
    }

    #[test]
    fn the_labels_line_up_with_the_classes_the_presets_name() {
        assert_eq!(label(2), Some("sky"));
        assert_eq!(label(12), Some("person"));
        assert_eq!(label(150), None);

        for (name, classes) in PRESETS {
            assert!(!classes.is_empty(), "{name} names nothing");
            for class in *classes {
                assert!(label(*class).is_some(), "{name} names class {class}, which is not one");
            }
        }
    }

    #[test]
    fn the_matting_classes_are_the_two_subject_ones() {
        assert!(MATTEABLE.contains(&12), "person");
        assert!(MATTEABLE.contains(&126), "animal");
        for (name, preset) in PRESETS {
            let all = preset.iter().all(|class| MATTEABLE.contains(class));
            assert_eq!(
                all,
                matches!(*name, "Person" | "Animal"),
                "{name} should{} turn the matte on",
                if all { "" } else { " not" }
            );
        }
    }

    #[test]
    fn a_mask_is_named_after_the_preset_it_matches_or_what_is_in_it() {
        assert_eq!(name_for(&[2]), "Sky");
        assert_eq!(name_for(&[12]), "Person");
        assert_eq!(name_for(&[126]), "Animal");

        assert_eq!(name_for(&[12, 126]), "person + animal");
        assert_eq!(name_for(&[2, 4]), "sky + tree");
        assert_eq!(name_for(&[2, 4, 9, 16]), "sky + tree + 2 more");
        assert_eq!(name_for(&[]), "Empty");
    }
}
