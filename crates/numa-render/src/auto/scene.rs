use std::sync::Arc;

use numa_core::document::{Document, Perspective};
use numa_core::image::LinearImage;
use numa_core::plane::Plane;
use serde::{Deserialize, Serialize};

use super::evidence::{self, Evidence, Lean, Reading, Source, BUILDINGS};
use super::sea;
use crate::segment::{self, Segmentation};

const VERSION: u32 = 3;

const SALIENT: usize = 48;

const EDGE: u32 = 900;

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Camera {

    pub focal35: Option<f32>,

    pub roll: Option<f32>,
    pub bias: Option<f32>,
    pub program: Option<u16>,
    pub dynamic_range: Option<u16>,

    #[serde(default)]
    pub focus: Option<[f32; 2]>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Scene {

    pub asked: String,

    pub size: [u32; 2],

    #[serde(skip)]
    pub classes: (usize, usize, Vec<u8>),

    pub faces: Vec<[f32; 4]>,

    #[serde(default)]
    pub landmarks: Vec<[[f32; 2]; 5]>,

    #[serde(default)]
    pub objects: Vec<(u8, [f32; 4])>,

    #[serde(default)]
    pub salient: (usize, usize, Vec<u8>),

    #[serde(default)]
    pub bright: (usize, usize, Vec<u8>),
    pub camera: Camera,
    pub level: Vec<Evidence>,
    pub upright: Option<Lean>,
}

impl Scene {

    pub fn frame_of(document: &Document) -> Document {
        let mut frame = Document::new(document.source.path.clone());
        frame.set_rotation(document.rotation());
        frame.set_mirrored(document.mirrored());
        frame
    }

    pub fn asked(document: &Document) -> String {
        let segmenter = segment::is_installed().then(segment::asked).unwrap_or_default();
        let faces = numa_cull::faces::is_installed();
        let objects = crate::distractions::is_installed();
        let matte = crate::matte::answering();
        format!("{VERSION}\0{}\0{}\0{segmenter}\0{faces}\0{objects}\0{matte}", document.rotation(), document.mirrored())
    }

    pub fn same_frame(document: &Document) -> bool {
        document.crop().is_none() && document.perspective() == Default::default()
    }

    pub fn share(&self, classes: &[u16]) -> f32 {
        let cells = &self.classes.2;
        cells.iter().filter(|class| classes.contains(&(**class as u16))).count() as f32 / cells.len().max(1) as f32
    }

    pub fn class_at(&self, u: f32, v: f32) -> Option<u16> {
        let (width, height, cells) = &self.classes;
        let x = ((u * *width as f32) as usize).min(width.checked_sub(1)?);
        let y = ((v * *height as f32) as usize).min(height.checked_sub(1)?);
        cells.get(y * width + x).map(|class| *class as u16)
    }

    pub fn frame(&self, perspective: Perspective, current: f32) -> evidence::Frame {
        let (width, height) = (self.size[0] as f32, self.size[1] as f32);
        let focal = self.camera.focal35.map(|focal| focal / 43.27 * width.hypot(height));
        evidence::Frame { width, height, perspective, current, pitch: self.upright.zip(focal) }
    }

    pub fn horizon(&self) -> Option<[f32; 4]> {
        self.level.iter().find_map(|e| match (e.source, e.reading) {
            (Source::Sea | Source::Water, Reading::Line(line)) => Some(line),
            _ => None,
        })
    }
}

pub fn read(document: &Document, working: &LinearImage, found: Option<Arc<Segmentation>>, camera: Camera) -> Scene {
    let geometry = Scene::frame_of(document);
    let frame = crate::apply_stack(&geometry, working, 1.0);
    let found = found.or_else(|| segment::of(&frame).map(Arc::new));
    let (width, height) = (frame.width() as f32, frame.height() as f32);
    let found_faces = numa_cull::faces::detect(&frame).unwrap_or_default();
    let faces = found_faces.iter().map(|face| [face.x / width, face.y / height, face.width / width, face.height / height]).collect();
    let landmarks = found_faces.iter().map(|face| face.landmarks.map(|(x, y)| [x / width, y / height])).collect();
    let objects = crate::distractions::objects(&frame)
        .unwrap_or_default()
        .into_iter()
        .map(|(class, [l, t, r, b])| (class as u8, [l / width, t / height, (r - l) / width, (b - t) / height]))
        .collect();
    let luma = log_luma(&crate::geometry_only(&geometry, working));
    let (level, upright) = evidence_of(&luma, found.as_deref());
    let classes = found.as_deref().map_or((0, 0, Vec::new()), |found| {
        let (width, height, winners) = found.winners();
        (width, height, winners.iter().map(|class| (*class).min(255) as u8).collect())
    });
    let mut scene = Scene { asked: Scene::asked(document), size: [frame.width(), frame.height()], classes, faces, landmarks, objects, camera, level, upright, ..Default::default() };
    scene.bright = bright(&frame);

    if !super::crop::named(&scene) {
        let photo = found.as_deref().map_or(&frame, |found| found.photo());
        let (w, h) = (photo.width() as usize, photo.height() as usize);
        let (cw, ch) = if w >= h { (SALIENT, (SALIENT * h / w).max(1)) } else { ((SALIENT * w / h).max(1), SALIENT) };
        if let Some(alpha) = crate::matte::subject(photo, cw, ch) {
            scene.salient = (cw, ch, alpha.data.iter().map(|a| (a.clamp(0.0, 1.0) * 255.0).round() as u8).collect());
        }
    }
    scene
}

fn bright(frame: &image::RgbImage) -> (usize, usize, Vec<u8>) {
    let (w, h) = (frame.width() as usize, frame.height() as usize);
    let (cw, ch) = if w >= h { (SALIENT, (SALIENT * h / w).max(1)) } else { ((SALIENT * w / h).max(1), SALIENT) };
    let mut lit = vec![0u32; cw * ch];
    let mut all = vec![0u32; cw * ch];
    for (x, y, pixel) in frame.enumerate_pixels() {
        let cell = (y as usize * ch / h) * cw + x as usize * cw / w;
        all[cell] += 1;
        lit[cell] += (pixel.0.iter().any(|v| *v >= 235)) as u32;
    }
    (cw, ch, lit.iter().zip(&all).map(|(l, a)| (*l * 255 / (*a).max(1)) as u8).collect())
}

fn log_luma(image: &LinearImage) -> Plane {
    let small = image.downscaled(EDGE);
    let small = small.as_ref().unwrap_or(image);
    Plane::new(
        small.width as usize,
        small.height as usize,
        small.data.chunks_exact(3).map(|p| (0.2126 * p[0] + 0.7152 * p[1] + 0.0722 * p[2]).max(1e-4).ln()).collect(),
    )
}

fn evidence_of(luma: &Plane, found: Option<&Segmentation>) -> (Vec<Evidence>, Option<Lean>) {
    let mut level = Vec::new();
    let buildings = found.map(|found| found.coarse(&BUILDINGS));
    let votes = evidence::votes(luma, |u, v| match (found, &buildings) {
        (Some(found), Some(plane)) => {
            let x = ((u * plane.width as f32) as usize).min(plane.width - 1);
            let y = ((v * plane.height as f32) as usize).min(plane.height - 1);
            (found.class_at(u, v), plane.data[y * plane.width + x])
        }
        _ => (u16::MAX, 0.0),
    });

    if let Some(found) = found {
        let planes = [vec![2u16], vec![26], vec![21, 128], sea::IN_THE_WAY.to_vec()].map(|classes| found.coarse(&classes));
        let horizon = sea::horizon(&sea::Planes {
            sky: &planes[0],
            sea: &planes[1],
            other_water: &planes[2],
            in_the_way: &planes[3],
            guide: found.guide(),
        });
        if let Some(horizon) = horizon {
            let source = if horizon.sea { Source::Sea } else { Source::Water };
            level.push(Evidence { source, reading: Reading::Line(horizon.line), sigma: horizon.sigma });
        }
    }

    let built = buildings.as_ref().map_or(0.0, |plane| {
        plane.data.iter().filter(|p| **p > 0.5).count() as f32 / plane.data.len().max(1) as f32
    });
    let upright = evidence::vanishing(&votes, luma.width, luma.height, built);
    if let Some(lean) = upright {
        level.push(Evidence { source: Source::Buildings, reading: Reading::Lean(lean.lean), sigma: lean.sigma });
    } else if let Some(lean) = evidence::verticals(&votes, luma.width, luma.height) {
        level.push(Evidence { source: Source::Verticals, reading: Reading::Lean(lean.lean), sigma: lean.sigma });
    }
    (level, upright)
}
