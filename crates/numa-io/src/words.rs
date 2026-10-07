mod things;
mod tokenizer;

pub use things::THINGS;

use std::collections::HashMap;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use image::RgbImage;
use ndarray::{Array2, Array4};

pub const MODEL: &str = "SigLIP 2";
pub const VISION: &str = "siglip2_vision_uint8.onnx";
pub const TEXT: &str = "siglip2_text_int8.onnx";
pub const TOKENIZER: &str = "siglip2_tokenizer.json";

pub const LENGTH: usize = 768;

const SIDE: u32 = 224;

pub const FLOOR: f32 = 0.07;
const MARGIN: f32 = 0.03;

const SAW: f32 = 0.10;
const SAW_TOP: usize = 3;

fn prompt(thing: &str) -> String {
    format!("this is a photo of {thing}.")
}

pub fn installed() -> bool {
    [VISION, TEXT, TOKENIZER].iter().all(|name| numa_core::paths::model_file(&[name]).is_some())
}

static VISION_MODEL: numa_infer::Kept = numa_infer::Kept::new();
static TEXT_MODEL: numa_infer::Kept = numa_infer::Kept::new();

static TOKENS: Mutex<Option<Arc<tokenizer::Tokenizer>>> = Mutex::new(None);

fn model(kept: &'static numa_infer::Kept, name: &str, load: fn(&std::path::Path) -> Option<numa_infer::Model>) -> Result<Arc<numa_infer::Model>, String> {
    let path = numa_core::paths::model_file(&[name]).ok_or_else(|| format!("{name} is not downloaded"))?;
    kept.get_or_init(|| load(&path)).ok_or_else(|| format!("{name} would not load"))
}

fn unit_rows(output: ndarray::ArrayD<f32>) -> Result<Vec<Vec<f32>>, String> {
    let rows = output.into_dimensionality::<ndarray::Ix2>().map_err(|err| err.to_string())?;
    Ok(rows
        .outer_iter()
        .map(|row| {
            let norm = row.dot(&row).sqrt().max(1e-12);
            row.iter().map(|value| value / norm).collect()
        })
        .collect())
}

pub fn embed_images(images: &[RgbImage]) -> Result<Vec<Vec<f32>>, String> {
    let mut pixels = Array4::<f32>::zeros((images.len(), 3, SIDE as usize, SIDE as usize));
    for (at, image) in images.iter().enumerate() {
        let square = image::imageops::resize(image, SIDE, SIDE, image::imageops::FilterType::CatmullRom);
        for (x, y, pixel) in square.enumerate_pixels() {
            for channel in 0..3 {
                pixels[[at, channel, y as usize, x as usize]] = pixel[channel] as f32 / 127.5 - 1.0;
            }
        }
    }
    let model = model(&VISION_MODEL, VISION, numa_infer::Model::load_quiet)?;

    let mut out = model.run(vec![pixels.into_dyn().into()]).map_err(|err| err.to_string())?;
    unit_rows(out.pop().ok_or("the vision model gave nothing")?)
}

pub fn embed_text(text: &str) -> Result<Vec<f32>, String> {
    let tokens = {
        let mut kept = TOKENS.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        match kept.as_ref() {
            Some(tokens) => tokens.clone(),
            None => {
                let path = numa_core::paths::model_file(&[TOKENIZER]).ok_or("the tokenizer is not downloaded")?;
                kept.insert(Arc::new(tokenizer::Tokenizer::load(&path)?)).clone()
            }
        }
    };
    let ids = Array2::from_shape_vec((1, tokenizer::LENGTH), tokens.encode(text)).map_err(|err| err.to_string())?;
    let model = model(&TEXT_MODEL, TEXT, numa_infer::Model::load)?;
    let mut out = model.run(vec![ids.into_dyn().into()]).map_err(|err| err.to_string())?;
    unit_rows(out.pop().ok_or("the text model gave nothing")?)?.pop().ok_or_else(|| "no text".to_string())
}

pub fn release() {
    TOKENS.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).take();
    VISION_MODEL.release();
    TEXT_MODEL.release();
}

pub fn cosine(a: &[f32], b: &[f32]) -> f32 {
    a.iter().zip(b).map(|(a, b)| a * b).sum()
}

pub fn ranked<T>(scored: Vec<(T, f32)>) -> Vec<(T, f32)> {
    let best = scored.iter().map(|(_, score)| *score).fold(f32::MIN, f32::max);
    let floor = FLOOR.max(best - MARGIN);
    let mut kept: Vec<(T, f32)> = scored.into_iter().filter(|(_, score)| *score >= floor).collect();
    kept.sort_by(|a, b| b.1.total_cmp(&a.1));
    kept
}

pub fn things() -> Result<Vec<Vec<f32>>, String> {
    THINGS.iter().map(|thing| embed_text(&prompt(thing))).collect()
}

pub fn saw(photo: &[f32], things: &[Vec<f32>]) -> Vec<usize> {
    let mut scores: Vec<(usize, f32)> = things.iter().map(|thing| cosine(photo, thing)).enumerate().collect();
    scores.sort_by(|a, b| b.1.total_cmp(&a.1));
    scores.into_iter().take(SAW_TOP).filter(|(_, score)| *score >= SAW).map(|(at, _)| at).collect()
}

const MAGIC: &[u8; 8] = b"NUMAWS2U";

pub struct Index {
    file: PathBuf,
    root: PathBuf,
    entries: HashMap<PathBuf, (i64, Vec<f32>)>,

    seen: HashMap<PathBuf, Vec<usize>>,
}

impl Index {

    pub fn open(root: &Path) -> Self {
        let file = root.join(crate::catalog::LIBRARY_DIR).join("words.bin");
        let entries = read(&file).unwrap_or_else(|err| {
            if err.kind() != std::io::ErrorKind::NotFound {
                log::warn!("{}: {err}; reading the library for words again", file.display());
                let _ = std::fs::remove_file(&file);
            }
            HashMap::new()
        });
        Self { file, root: root.to_path_buf(), entries, seen: HashMap::new() }
    }

    pub fn get(&self, path: &Path, mtime: i64) -> Option<&[f32]> {
        let (made, numbers) = self.entries.get(path.strip_prefix(&self.root).unwrap_or(path))?;
        (*made == mtime).then_some(numbers.as_slice())
    }

    pub fn see(&mut self, things: &[Vec<f32>]) {
        use rayon::prelude::*;
        let new: Vec<(PathBuf, Vec<usize>)> = self
            .entries
            .par_iter()
            .filter(|(path, _)| !self.seen.contains_key(*path))
            .map(|(path, (_, numbers))| (path.clone(), saw(numbers, things)))
            .collect();
        self.seen.extend(new);
    }

    pub fn seen(&self) -> impl Iterator<Item = &[usize]> {
        self.seen.values().map(Vec::as_slice)
    }

    pub fn saw(&self, path: &Path, mtime: i64) -> Option<&[usize]> {
        self.get(path, mtime)?;
        self.seen.get(path.strip_prefix(&self.root).unwrap_or(path)).map(Vec::as_slice)
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    pub fn add(&mut self, read: Vec<(PathBuf, i64, Vec<f32>)>) -> std::io::Result<()> {
        let fresh = !self.file.exists();
        if let Some(dir) = self.file.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let mut bytes = Vec::new();
        if fresh {
            bytes.extend_from_slice(MAGIC);
        }
        for (path, mtime, numbers) in read {
            let within = path.strip_prefix(&self.root).unwrap_or(&path).to_path_buf();
            let name = within.to_string_lossy().into_owned();
            bytes.extend_from_slice(&(name.len() as u16).to_le_bytes());
            bytes.extend_from_slice(name.as_bytes());
            bytes.extend_from_slice(&mtime.to_le_bytes());
            for value in &numbers {
                bytes.extend_from_slice(&half::f16::from_f32(*value).to_le_bytes());
            }
            self.seen.remove(&within);
            self.entries.insert(within, (mtime, numbers));
        }
        let mut out = std::fs::OpenOptions::new().create(true).append(true).open(&self.file)?;
        out.write_all(&bytes)?;
        out.flush()
    }
}

fn read(file: &Path) -> std::io::Result<HashMap<PathBuf, (i64, Vec<f32>)>> {
    let mut bytes = Vec::new();
    std::fs::File::open(file)?.read_to_end(&mut bytes)?;
    if bytes.get(..8) != Some(&MAGIC[..]) {
        return Err(std::io::Error::other("made by another model"));
    }
    let mut entries = HashMap::new();
    let mut at = 8;

    while let Some(length) = bytes.get(at..at + 2).map(|two| u16::from_le_bytes([two[0], two[1]]) as usize) {
        let end = at + 2 + length + 8 + LENGTH * 2;
        let Some(record) = bytes.get(at + 2..end) else { break };
        let name = String::from_utf8_lossy(&record[..length]).into_owned();
        let mtime = i64::from_le_bytes(record[length..length + 8].try_into().unwrap());
        let numbers = record[length + 8..].chunks_exact(2).map(|two| half::f16::from_le_bytes([two[0], two[1]]).to_f32()).collect();
        entries.insert(PathBuf::from(name), (mtime, numbers));
        at = end;
    }
    Ok(entries)
}

#[cfg(test)]
mod tests;
