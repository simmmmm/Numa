use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use image::{imageops, Rgb, RgbImage};
use numa_infer::Model;

use numa_core::mask::Alpha;

use crate::local::{self, Plane};
use crate::segment;

pub const ANSWERS: &str = "sam-5: logits up, guided, cleaned, closer, the rim, the card's positions";

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

fn encoder() -> Option<std::sync::Arc<Model>> {
    static PLAN: numa_infer::Kept = numa_infer::Kept::new();
    PLAN.get_or_init(|| {
        let path = model_path(ENCODER)?;

        #[cfg(target_os = "ios")]
        if let Err(err) = numa_infer::rewrite::prepare(&path) {
            log::warn!("{}: attention not rewritten: {err}", path.display());
        }
        Model::load(&path)
    })
}

fn squashes(encoder: &Model) -> bool {
    encoder.describe().iter().filter(|port| port.starts_with("out")).count() != 2
}

fn decoder() -> Option<std::sync::Arc<Model>> {
    static PLAN: numa_infer::Kept = numa_infer::Kept::new();
    PLAN.get_or_init(|| Model::load(&model_path(DECODER)?))
}

pub fn prepare() {
    let _ = decoder();
}

pub struct Embedding {
    whole: Encoded,

    guide: Plane,

    photo: RgbImage,

    asked: Mutex<HashMap<[u32; 2], Asked>>,

    looking: Mutex<()>,
}

#[derive(Clone)]
struct Asked {

    coarse: Arc<Plane>,
    closer: Closer,
}

#[derive(Clone)]
enum Closer {

    NotNeeded,

    Owed([f32; 3]),

    Looked(Option<Arc<Plane>>, [f32; 3]),
}

struct Encoded {
    features: Vec<ndarray::ArrayD<f32>>,

    covered: (f32, f32),
}

pub fn encode(photo: &RgbImage) -> Option<Embedding> {
    Some(embedding(encoded(photo)?, photo))
}

fn embedding(whole: Encoded, photo: &RgbImage) -> Embedding {
    Embedding {
        whole,
        guide: segment::luminance(photo, segment::REFINE),
        photo: photo.clone(),
        asked: Mutex::default(),
        looking: Mutex::default(),
    }
}

pub fn model_id() -> Option<String> {
    let path = model_path(ENCODER)?;
    let size = std::fs::metadata(&path).ok()?.len();
    Some(format!("{}:{size}", path.file_name()?.to_string_lossy()))
}

impl Embedding {

    pub fn kept(&self) -> Option<(&ndarray::ArrayD<f32>, (f32, f32))> {
        match &self.whole.features[..] {
            [image, _positional] => Some((image, self.whole.covered)),
            _ => None,
        }
    }
}

pub fn from_kept(photo: &RgbImage, image: ndarray::ArrayD<f32>, covered: (f32, f32)) -> Option<Embedding> {
    Some(embedding(Encoded { features: vec![image, positional(None)?], covered }, photo))
}

fn positional(made: Option<&ndarray::ArrayD<f32>>) -> Option<ndarray::ArrayD<f32>> {
    static KNOWN: Mutex<Option<(String, ndarray::ArrayD<f32>)>> = Mutex::new(None);
    let model = model_id()?;
    let file = numa_core::paths::cache_dir().join("sam").join(format!("{}.positional", model.replace(['/', ':'], "-")));
    let mut known = KNOWN.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if let Some(made) = made {
        if known.as_ref().is_none_or(|(id, _)| *id != model) {
            *known = Some((model, made.clone()));
            if !file.is_file() {
                let mut bytes: Vec<u8> = (made.ndim() as u32).to_le_bytes().to_vec();
                made.shape().iter().for_each(|dim| bytes.extend((*dim as u32).to_le_bytes()));
                made.iter().for_each(|value| bytes.extend(value.to_le_bytes()));
                let _ = std::fs::create_dir_all(file.parent()?);
                let partial = file.with_extension("partial");
                if std::fs::write(&partial, bytes).is_ok() {
                    let _ = std::fs::rename(&partial, &file);
                }
            }
        }
        return Some(made.clone());
    }
    if let Some((id, grid)) = known.as_ref().filter(|(id, _)| *id == model) {
        let _ = id;
        return Some(grid.clone());
    }
    let bytes = std::fs::read(&file).ok()?;
    let word = |at: usize| Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?) as usize);
    let rank = word(0)?;
    let shape: Vec<usize> = (0..rank).map(|k| word(4 + 4 * k)).collect::<Option<_>>()?;
    let values: Vec<f32> = bytes.get(4 + 4 * rank..)?.chunks_exact(4).map(|v| f32::from_le_bytes([v[0], v[1], v[2], v[3]])).collect();
    let grid = ndarray::ArrayD::from_shape_vec(shape, values).ok()?;
    *known = Some((model, grid.clone()));
    Some(grid)
}

fn encoded(photo: &RgbImage) -> Option<Encoded> {
    let plan = encoder()?;
    let (width, height) = (photo.width(), photo.height());
    if width == 0 || height == 0 {
        return None;
    }

    let scale = EDGE as f32 / width.max(height) as f32;
    let fitted = match squashes(&plan) {
        true => (EDGE as u32, EDGE as u32),
        false => (
            ((width as f32 * scale).round() as u32).clamp(1, EDGE as u32),
            ((height as f32 * scale).round() as u32).clamp(1, EDGE as u32),
        ),
    };
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

    let features = match &outputs[..] {
        [image, grid] => vec![image.clone(), positional(Some(grid))?],
        _ => outputs,
    };
    Some(Encoded { features, covered: (fitted.0 as f32, fitted.1 as f32) })
}

impl Embedding {

    pub fn at(&self, u: f32, v: f32) -> Option<Alpha> {
        let asked = self.first(u, v)?;
        let (width, height) = (self.guide.width, self.guide.height);
        let logits = match &asked.closer {
            Closer::Looked(Some(crop), [left, top, side]) => {

                let (w, h) = (self.photo.width() as f32, self.photo.height() as f32);
                let mut placed = Plane::new(width, height, vec![-SURE; width * height]);
                for (index, value) in placed.data.iter_mut().enumerate() {
                    let x = ((index % width) as f32 + 0.5) / width as f32 * w;
                    let y = ((index / width) as f32 + 0.5) / height as f32 * h;
                    let (cx, cy) = ((x - left) / side, (y - top) / side);
                    if (0.0..1.0).contains(&cx) && (0.0..1.0).contains(&cy) {
                        *value = sample(crop, cx, cy);
                    }
                }
                placed
            }

            _ => local::upsample(&asked.coarse, width, height),
        };
        let ramp = Plane::new(width, height, logits.data.iter().map(|logit| ramp(*logit)).collect());
        Some(segment::refine(&self.guide, &ramp))
    }

    pub fn owes_a_closer_look(&self, u: f32, v: f32) -> bool {
        self.first(u, v).is_some_and(|asked| matches!(asked.closer, Closer::Owed(_)))
    }

    pub fn look_closer(&self, u: f32, v: f32) -> bool {
        let _one = self.looking.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let Some(asked) = self.first(u, v) else { return false };
        let Closer::Owed(rect) = asked.closer else { return false };
        let answer = self.closer(&asked.coarse, u, v, rect).map(Arc::new);
        let found = answer.is_some();
        if let Some(kept) = self.asked.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).get_mut(&key(u, v)) {
            kept.closer = Closer::Looked(answer, rect);
        }
        found
    }

    fn first(&self, u: f32, v: f32) -> Option<Asked> {
        if let Some(asked) = self.asked.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).get(&key(u, v)) {
            return Some(asked.clone());
        }
        let coarse = self.whole.answer(u, v, None)?;
        let closer = match self.crop_for(&coarse, u, v) {
            Some(rect) => Closer::Owed(rect),
            None => Closer::NotNeeded,
        };
        let asked = Asked { coarse: Arc::new(coarse), closer };
        self.asked.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).insert(key(u, v), asked.clone());
        Some(asked)
    }

    fn crop_for(&self, coarse: &Plane, u: f32, v: f32) -> Option<[f32; 3]> {
        let (mut l, mut t, mut r, mut b) = (usize::MAX, usize::MAX, 0, 0);
        for (index, value) in coarse.data.iter().enumerate() {
            if *value > 0.0 {
                let (x, y) = (index % coarse.width, index / coarse.width);
                (l, t, r, b) = (l.min(x), t.min(y), r.max(x), b.max(y));
            }
        }
        if l > r {
            return None;
        }
        let long = coarse.width.max(coarse.height) as f32;
        let extent = (r - l + 1).max(b - t + 1) as f32 / long;
        if extent > SMALL {
            return None;
        }

        let (w, h) = (self.photo.width() as f32, self.photo.height() as f32);
        let cell = w.max(h) / long;
        let side = (extent * MARGIN).max(CLOSEST) * w.max(h);
        let side = side.min(w.min(h));

        let middle = |from: usize, to: usize, click: f32, size: f32| {
            let centre = ((from + to + 1) as f32 * cell * 0.5 + click * size) * 0.5;
            (centre - side * 0.5).clamp(0.0, size - side)
        };
        Some([middle(l, r, u, w), middle(t, b, v, h), side])
    }

    fn closer(&self, coarse: &Plane, u: f32, v: f32, [left, top, side]: [f32; 3]) -> Option<Plane> {
        let (w, h) = (self.photo.width() as f32, self.photo.height() as f32);
        let crop = imageops::crop_imm(&self.photo, left as u32, top as u32, side as u32, side as u32).to_image();
        let close = encoded(&crop)?;
        let (cu, cv) = ((u * w - left) / side, (v * h - top) / side);

        let (gw, gh) = close.grid();
        let like = Plane::new(
            gw,
            gh,
            (0..gw * gh)
                .map(|cell| {
                    let x = left + ((cell % gw) as f32 + 0.5) / gw as f32 * side;
                    let y = top + ((cell / gw) as f32 + 0.5) / gh as f32 * side;
                    sample(coarse, x / w, y / h)
                })
                .collect(),
        );
        let answer = close.answer(cu, cv, Some(&like))?;

        let touches = (0..answer.width)
            .flat_map(|x| [x, (answer.height - 1) * answer.width + x])
            .chain((0..answer.height).flat_map(|y| [y * answer.width, y * answer.width + answer.width - 1]))
            .filter(|index| answer.data[*index] > 0.0)
            .count();
        if touches > 0 {
            return None;
        }
        Some(answer)
    }
}

const CLOSE: f32 = 2.5;

const STEADY: f32 = 0.6;

const AGREE: f32 = 0.5;

fn key(u: f32, v: f32) -> [u32; 2] {
    [u.to_bits(), v.to_bits()]
}

const SMALL: f32 = 0.2;

const MARGIN: f32 = 2.5;

const CLOSEST: f32 = 0.125;

fn ramp(logit: f32) -> f32 {
    (logit * 0.5 + 0.5).clamp(0.0, 1.0)
}

const SURE: f32 = 8.0;

fn sample(plane: &Plane, u: f32, v: f32) -> f32 {
    let x = (u * plane.width as f32 - 0.5).clamp(0.0, plane.width as f32 - 1.0);
    let y = (v * plane.height as f32 - 0.5).clamp(0.0, plane.height as f32 - 1.0);
    let (x0, y0) = (x as usize, y as usize);
    let (x1, y1) = ((x0 + 1).min(plane.width - 1), (y0 + 1).min(plane.height - 1));
    let (fx, fy) = (x - x0 as f32, y - y0 as f32);
    let at = |x: usize, y: usize| plane.data[y * plane.width + x];
    let top = at(x0, y0) + (at(x1, y0) - at(x0, y0)) * fx;
    let bottom = at(x0, y1) + (at(x1, y1) - at(x0, y1)) * fx;
    top + (bottom - top) * fy
}

impl Encoded {

    fn grid(&self) -> (usize, usize) {
        let quarter = EDGE as f32 / GRID as f32;
        (
            ((self.covered.0 / quarter).round() as usize).clamp(1, GRID),
            ((self.covered.1 / quarter).round() as usize).clamp(1, GRID),
        )
    }

    fn answer(&self, u: f32, v: f32, like: Option<&Plane>) -> Option<Plane> {
        let plan = decoder()?;

        let point = ndarray::Array4::<f32>::from_shape_vec(
            (1, 1, 1, 2),
            vec![u * self.covered.0, v * self.covered.1],
        )
        .ok()?;

        let label = ndarray::Array3::<i64>::from_shape_vec((1, 1, 1), vec![1]).ok()?;

        let mut inputs: Vec<numa_infer::Input> = vec![point.into_dyn().into(), label.into_dyn().into()];

        if plan.describe().iter().any(|port| port.starts_with("in  input_boxes")) {
            inputs.push(ndarray::Array3::<f32>::zeros((1, 0, 4)).into_dyn().into());
        }
        inputs.extend(self.features.iter().map(|feature| feature.clone().into()));
        let outputs = match plan.run(inputs) {
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

        let quarter = EDGE as f32 / GRID as f32;
        let (width, height) = self.grid();
        let clicked = ((v * self.covered.1 / quarter) as usize).min(height - 1) * width
            + ((u * self.covered.0 / quarter) as usize).min(width - 1);
        let logit = |k: usize, cell: usize| masks[[0, 0, k, cell / width, cell % width]];

        let (px, py) = (u * self.covered.0 / quarter - 0.5, v * self.covered.1 / quarter - 0.5);
        let under = |k: usize| {
            let (x0, y0) = (px.clamp(0.0, (width - 1) as f32), py.clamp(0.0, (height - 1) as f32));
            let (x1, y1) = ((x0 as usize + 1).min(width - 1), (y0 as usize + 1).min(height - 1));
            let (fx, fy) = (x0.fract(), y0.fract());
            let at = |x: usize, y: usize| logit(k, y * width + x);
            let top = at(x0 as usize, y0 as usize) * (1.0 - fx) + at(x1, y0 as usize) * fx;
            let bottom = at(x0 as usize, y1) * (1.0 - fx) + at(x1, y1) * fx;
            top * (1.0 - fy) + bottom * fy
        };
        let contains = |k: usize| under(k) > -1.0;

        let close = |k: usize| under(k) > -CLOSE;

        let steady = |k: usize| {
            let (mut firm, mut loose) = (0usize, 0usize);
            for cell in 0..width * height {
                let value = logit(k, cell);
                firm += (value > 1.0) as usize;
                loose += (value > -1.0) as usize;
            }
            firm as f32 / loose.max(1) as f32 >= STEADY
        };
        let steady: [bool; 3] = [steady(0), steady(1), steady(2)];

        let believed = |k: usize| {
            let steady_here = steady[k] && close(k);
            (steady_here, !steady_here && contains(k), scores[[0, 0, k]])
        };

        let best = match like {
            None => (0..3)
                .max_by(|a, b| believed(*a).partial_cmp(&believed(*b)).unwrap_or(std::cmp::Ordering::Equal))
                .unwrap_or(0),

            Some(like) => {
                let agreement = |k: usize| {
                    let (mut both, mut either) = (0usize, 0usize);
                    for (cell, value) in like.data.iter().enumerate() {
                        let (a, b) = (logit(k, cell) > 0.0, *value > 0.0);
                        both += (a && b) as usize;
                        either += (a || b) as usize;
                    }
                    both as f32 / either.max(1) as f32
                };
                let (best, agreed) = (0..3)
                    .map(|k| (k, agreement(k)))
                    .max_by(|a, b| contains(a.0).cmp(&contains(b.0)).then(a.1.total_cmp(&b.1)))?;
                if agreed < AGREE {
                    return None;
                }
                best
            }
        };

        let mut coarse = Plane::new(width, height, vec![0.0; width * height]);
        for (cell, value) in coarse.data.iter_mut().enumerate() {
            *value = logit(best, cell);
        }
        keep_what_was_clicked(&mut coarse, clicked);
        Some(coarse)
    }
}

fn keep_what_was_clicked(coarse: &mut Plane, clicked: usize) {
    let (width, height) = (coarse.width, coarse.height);
    let inside: Vec<bool> = coarse.data.iter().map(|logit| *logit > 0.0).collect();
    let (label, sizes) = pieces(&inside, width, height);
    let main = match inside[clicked] {
        true => label[clicked],
        false => match sizes.iter().enumerate().max_by_key(|(_, size)| **size) {
            Some((id, _)) => id as u32,
            None => return,
        },
    };
    let body = sizes[main as usize] as f32;

    let outside: Vec<bool> = inside.iter().map(|inside| !inside).collect();
    let (gaps, gap_sizes) = pieces(&outside, width, height);
    let mut open = vec![false; gap_sizes.len()];
    for x in 0..width {
        for y in [0, height - 1] {
            if let Some(o) = open.get_mut(gaps[y * width + x] as usize) {
                *o = true;
            }
        }
    }
    for y in 0..height {
        for x in [0, width - 1] {
            if let Some(o) = open.get_mut(gaps[y * width + x] as usize) {
                *o = true;
            }
        }
    }

    for (cell, value) in coarse.data.iter_mut().enumerate() {
        if inside[cell] && label[cell] != main && (sizes[label[cell] as usize] as f32) < body * ISLAND {
            *value = -SURE;
        }
        if !inside[cell] && !open[gaps[cell] as usize] && (gap_sizes[gaps[cell] as usize] as f32) < body * HOLE {
            *value = SURE;
        }
    }
}

const ISLAND: f32 = 0.2;

const HOLE: f32 = 0.01;

fn pieces(inside: &[bool], width: usize, height: usize) -> (Vec<u32>, Vec<usize>) {
    let mut label = vec![u32::MAX; inside.len()];
    let mut sizes = Vec::new();
    let mut stack = Vec::new();
    for start in 0..inside.len() {
        if !inside[start] || label[start] != u32::MAX {
            continue;
        }
        let id = sizes.len() as u32;
        label[start] = id;
        stack.push(start);
        let mut size = 0;
        while let Some(cell) = stack.pop() {
            size += 1;
            let (x, y) = (cell % width, cell / width);
            let near = [
                (x > 0).then(|| cell - 1),
                (x + 1 < width).then(|| cell + 1),
                (y > 0).then(|| cell - width),
                (y + 1 < height).then(|| cell + width),
            ];
            for next in near.into_iter().flatten() {
                if inside[next] && label[next] == u32::MAX {
                    label[next] = id;
                    stack.push(next);
                }
            }
        }
        sizes.push(size);
    }
    (label, sizes)
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
    fn specks_go_and_pinholes_fill_but_a_second_piece_stays() {
        let (w, h) = (40, 20);
        let mut coarse = Plane::new(w, h, vec![-SURE; w * h]);
        let mut fill = |x0: usize, y0: usize, x1: usize, y1: usize, value: f32| {
            for y in y0..y1 {
                for x in x0..x1 {
                    coarse.data[y * w + x] = value;
                }
            }
        };
        fill(2, 2, 14, 14, SURE);
        fill(7, 7, 8, 8, -SURE);
        fill(30, 2, 31, 3, SURE);
        fill(20, 2, 26, 8, SURE);
        keep_what_was_clicked(&mut coarse, 5 * w + 5);
        assert_eq!(coarse.data[7 * w + 7], SURE, "the pinhole is filled");
        assert_eq!(coarse.data[2 * w + 30], -SURE, "the speck is gone");
        assert_eq!(coarse.data[4 * w + 22], SURE, "the second piece stays");
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
