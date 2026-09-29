use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use half::slice::HalfFloatSliceExt;
use numa_core::color::CameraProfile;
use numa_core::image::LinearImage;

use crate::catalog::LIBRARY_DIR;
use crate::raw;

const VERSION: u32 = 2;

const MAGIC: &[u8; 8] = b"NUMAPRX1";

pub const DEFAULT_BUDGET: u64 = 2 << 30;
static BUDGET: AtomicU64 = AtomicU64::new(DEFAULT_BUDGET);

pub fn set_budget(bytes: u64) {
    BUDGET.store(bytes, Ordering::Relaxed);
}

pub fn budget() -> u64 {
    BUDGET.load(Ordering::Relaxed)
}

pub fn dir_for(path: &Path) -> Option<PathBuf> {
    crate::thumbs::library_root(path).map(|root| root.join(LIBRARY_DIR).join("previews"))
}

pub fn editor_proxy(path: &Path, edge: u32) -> Result<(Arc<LinearImage>, (u32, u32)), String> {
    let place = place_of(path, edge);
    if let Some((proxy, full)) = place.as_ref().and_then(|place| read(place, path)) {
        return Ok((Arc::new(proxy), full));
    }
    let started = std::time::Instant::now();
    let (proxy, full) = raw::editor_proxy(path, edge)?;
    DECODED_MS.store(started.elapsed().as_millis() as u64, Ordering::Relaxed);
    let proxy = Arc::new(proxy);
    if let Some(place) = place {
        let proxy = proxy.clone();
        std::thread::spawn(move || write(&place, &proxy, full));
    }
    Ok((proxy, full))
}

struct Place {
    dir: PathBuf,
    file: PathBuf,
}

fn place_of(path: &Path, edge: u32) -> Option<Place> {
    if !raw::is_raw(path) {
        return None;
    }

    let made = format!("{edge}\0{:?}\0{}", crate::dcp::automatic(), env!("CARGO_PKG_VERSION"));
    place_for(path, &made, "proxy")
}

fn place_for(path: &Path, made: &str, extension: &str) -> Option<Place> {
    if budget() == 0 {
        return None;
    }
    let root = crate::thumbs::library_root(path)?;
    let dir = root.join(LIBRARY_DIR).join("previews");
    if SLOW.lock().ok()?.as_ref().and_then(|slow| slow.get(&dir).copied()) == Some(true) {
        return None;
    }
    let meta = std::fs::metadata(path).ok()?;
    let modified = meta.modified().ok()?.duration_since(std::time::UNIX_EPOCH).ok()?.as_nanos();
    let within = path.strip_prefix(&root).ok()?;
    let within: Vec<String> = within.components().map(|part| part.as_os_str().to_string_lossy().into_owned()).collect();

    let text = format!("{VERSION}\0{}\0{modified}\0{}\0{made}", within.join("/"), meta.len());
    let hash = text.bytes().fold(0xcbf2_9ce4_8422_2325u64, |hash, byte| (hash ^ byte as u64).wrapping_mul(0x100_0000_01b3));
    Some(Place { file: dir.join(format!("{hash:016x}.{extension}")), dir })
}

pub fn embedding(path: &Path, framing: &str, frame: &image::RgbImage) -> Option<numa_render::sam::Embedding> {
    let place = embedding_place(path, framing);
    if let Some(kept) = place.as_ref().and_then(|place| read_embedding(place, frame)) {
        return Some(kept);
    }
    let made = numa_render::sam::encode(frame)?;
    if let (Some(place), Some((image, covered))) = (place, made.kept()) {
        let mut bytes = Vec::with_capacity(32 + image.len() * 2);
        bytes.extend_from_slice(EMBEDDING);
        bytes.extend(covered.0.to_le_bytes());
        bytes.extend(covered.1.to_le_bytes());
        bytes.extend((image.ndim() as u32).to_le_bytes());
        image.shape().iter().for_each(|dim| bytes.extend((*dim as u32).to_le_bytes()));
        let values: Vec<f32> = image.iter().copied().collect();
        let mut halves = vec![half::f16::ZERO; values.len()];
        halves.convert_from_f32_slice(&values);
        halves.iter().for_each(|value| bytes.extend_from_slice(&value.to_le_bytes()));
        std::thread::spawn(move || keep(&place, &bytes));
    }
    Some(made)
}

const EMBEDDING: &[u8; 8] = b"NUMASAM2";

fn embedding_place(path: &Path, framing: &str) -> Option<Place> {
    let model = numa_render::sam::model_id()?;
    place_for(path, &format!("sam\0{framing}\0{model}"), "sam")
}

fn read_embedding(place: &Place, frame: &image::RgbImage) -> Option<numa_render::sam::Embedding> {
    let bytes = std::fs::read(&place.file).ok()?;
    let parsed = (|| {
        let mut read = Cursor(&bytes);
        if read.take(8)? != EMBEDDING {
            return None;
        }
        let covered = (read.float()?, read.float()?);
        let rank = read.word()? as usize;
        let shape: Vec<usize> = (0..rank).map(|_| read.word().map(|dim| dim as usize)).collect::<Option<_>>()?;
        let count: usize = shape.iter().product();
        let halves: Vec<half::f16> = read.take(count * 2)?.chunks_exact(2).map(|pair| half::f16::from_le_bytes([pair[0], pair[1]])).collect();
        if !read.0.is_empty() {
            return None;
        }
        let mut values = vec![0.0f32; count];
        halves.convert_to_f32_slice(&mut values);
        Some((ndarray::ArrayD::from_shape_vec(shape, values).ok()?, covered))
    })();
    let Some((image, covered)) = parsed else {
        let _ = std::fs::remove_file(&place.file);
        return None;
    };
    let _ = std::fs::File::options().write(true).open(&place.file).and_then(|file| file.set_modified(std::time::SystemTime::now()));
    numa_render::sam::from_kept(frame, image, covered)
}

fn keep(place: &Place, bytes: &[u8]) {
    if std::fs::create_dir_all(&place.dir).is_err() {
        return;
    }
    tag(&place.dir);
    let partial = place.file.with_extension("partial");
    if std::fs::write(&partial, bytes).is_ok() && std::fs::rename(&partial, &place.file).is_ok() {
        trim(&place.dir, budget());
    } else {
        let _ = std::fs::remove_file(&partial);
    }
}

static DECODED_MS: AtomicU64 = AtomicU64::new(0);
static SLOW: Mutex<Option<HashMap<PathBuf, bool>>> = Mutex::new(None);

fn read(place: &Place, path: &Path) -> Option<(LinearImage, (u32, u32))> {
    let started = std::time::Instant::now();
    let bytes = std::fs::read(&place.file).ok()?;
    let read = started.elapsed().as_millis() as u64;
    let kept = parse(&bytes, path);
    if kept.is_none() {
        let _ = std::fs::remove_file(&place.file);
        return None;
    }

    let mut slow = SLOW.lock().ok()?;
    let slow = slow.get_or_insert_with(HashMap::new);
    if !slow.contains_key(&place.dir) {
        let decoded = DECODED_MS.load(Ordering::Relaxed);
        slow.insert(place.dir.clone(), decoded > 0 && read > decoded);
        log::info!("previews in {}: read in {read} ms, a decode took {decoded}", place.dir.display());
    }

    let _ = std::fs::File::options().write(true).open(&place.file).and_then(|file| file.set_modified(std::time::SystemTime::now()));
    kept
}

fn write(place: &Place, proxy: &LinearImage, full: (u32, u32)) {
    let mut bytes = Vec::with_capacity(64 + proxy.data.len() * 2);
    bytes.extend_from_slice(MAGIC);
    for value in [proxy.width, proxy.height, full.0, full.1] {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    let floats = |bytes: &mut Vec<u8>, values: &[f32]| values.iter().for_each(|value| bytes.extend_from_slice(&value.to_le_bytes()));
    match proxy.profile {
        Some(profile) => {
            bytes.push(1);
            floats(&mut bytes, &profile.as_shot);
            profile.xyz_to_cam.iter().chain(&profile.cam_to_srgb).for_each(|row| floats(&mut bytes, row));
        }
        None => bytes.push(0),
    }
    floats(&mut bytes, &[proxy.clip.unwrap_or(f32::NAN)]);
    let film = proxy.film_mode.as_deref().unwrap_or("");
    bytes.extend_from_slice(&(film.len() as u32).to_le_bytes());
    bytes.extend_from_slice(film.as_bytes());
    let mut halves = vec![half::f16::ZERO; proxy.data.len()];
    halves.convert_from_f32_slice(&proxy.data);
    halves.iter().for_each(|value| bytes.extend_from_slice(&value.to_le_bytes()));
    keep(place, &bytes);
}

fn tag(dir: &Path) {
    let tag = dir.join("CACHEDIR.TAG");
    if !tag.exists() {
        let _ = std::fs::write(
            &tag,
            "Signature: 8a477f597d28d172789f06886806bc55\n\
             # This file is a cache directory tag created by Numa: developed previews,\n\
             # made again from the photographs whenever they are missing.\n\
             # For information about cache directory tags, see https://bford.info/cachedir/\n",
        );
    }
}

fn parse(bytes: &[u8], path: &Path) -> Option<(LinearImage, (u32, u32))> {
    let mut read = Cursor(bytes);
    if read.take(8)? != MAGIC {
        return None;
    }
    let (width, height, full_width, full_height) = (read.word()?, read.word()?, read.word()?, read.word()?);
    let profile = match read.take(1)?[0] {
        1 => Some(CameraProfile {
            as_shot: read.three()?,
            xyz_to_cam: [read.three()?, read.three()?, read.three()?],
            cam_to_srgb: [read.three()?, read.three()?, read.three()?],
        }),
        _ => None,
    };
    let clip = Some(read.float()?).filter(|clip| !clip.is_nan());
    let film = read.word()? as usize;
    let film_mode = Some(String::from_utf8(read.take(film)?.to_vec()).ok()?).filter(|film| !film.is_empty());
    let count = width as usize * height as usize * 3;
    let halves: Vec<half::f16> = read.take(count * 2)?.chunks_exact(2).map(|pair| half::f16::from_le_bytes([pair[0], pair[1]])).collect();
    if !read.0.is_empty() {
        return None;
    }
    let mut data = vec![0.0f32; count];
    halves.convert_to_f32_slice(&mut data);

    let mut image = LinearImage::new(width, height, data).with_rendering(raw::rendering_of(path)).with_film_mode(film_mode);
    image.clip = clip;
    image.profile = profile;
    Some((image, (full_width, full_height)))
}

struct Cursor<'a>(&'a [u8]);

impl<'a> Cursor<'a> {
    fn take(&mut self, count: usize) -> Option<&'a [u8]> {
        let (taken, rest) = (self.0.get(..count)?, self.0.get(count..)?);
        self.0 = rest;
        Some(taken)
    }

    fn word(&mut self) -> Option<u32> {
        Some(u32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }

    fn float(&mut self) -> Option<f32> {
        Some(f32::from_le_bytes(self.take(4)?.try_into().ok()?))
    }

    fn three(&mut self) -> Option<[f32; 3]> {
        Some([self.float()?, self.float()?, self.float()?])
    }
}

pub fn trim(dir: &Path, budget: u64) {
    let mut files = proxies(dir);
    let mut total: u64 = files.iter().map(|(_, size, _)| size).sum();
    if total <= budget {
        return;
    }
    files.sort_by_key(|(used, _, _)| *used);
    let target = budget / 4 * 3;
    for (_, size, path) in files {
        if total <= target {
            break;
        }
        if std::fs::remove_file(&path).is_ok() {
            total -= size;
        }
    }
}

pub fn usage(dir: &Path) -> (u64, usize) {
    let files = proxies(dir);
    (files.iter().map(|(_, size, _)| size).sum(), files.len())
}

fn proxies(dir: &Path) -> Vec<(std::time::SystemTime, u64, PathBuf)> {
    let Ok(entries) = std::fs::read_dir(dir) else { return Vec::new() };
    entries
        .flatten()

        .filter(|entry| entry.path().extension().is_some_and(|extension| extension == "proxy" || extension == "sam"))
        .filter_map(|entry| {
            let meta = entry.metadata().ok().filter(|meta| meta.is_file())?;
            Some((meta.modified().ok()?, meta.len(), entry.path()))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_kept_proxy_reads_back() {
        let dir = std::env::temp_dir().join(format!("numa-previews-{}", std::process::id()));
        let place = Place { file: dir.join("test.proxy"), dir: dir.clone() };
        let profile = CameraProfile { as_shot: [2.0, 1.0, 1.5], xyz_to_cam: [[0.5, 0.1, 0.0]; 3], cam_to_srgb: [[1.2, -0.1, -0.1]; 3] };
        let data: Vec<f32> = (0..6 * 4 * 3).map(|at| at as f32 / 50.0).collect();
        let proxy = LinearImage::new(6, 4, data.clone()).with_profile(profile).with_clip(2.25).with_film_mode(Some("Velvia".into()));
        write(&place, &proxy, (600, 400));

        let bytes = std::fs::read(&place.file).unwrap();
        let (back, full) = parse(&bytes, Path::new("/nowhere.raf")).expect("reads back");
        assert_eq!((back.width, back.height, full), (6, 4, (600, 400)));
        assert_eq!(back.profile, Some(profile));
        assert_eq!((back.clip, back.film_mode.as_deref()), (Some(2.25), Some("Velvia")));
        assert!(back.data.iter().zip(&data).all(|(a, b)| (a - b).abs() <= b.abs() / 1000.0 + 1e-6));
        assert!(dir.join("CACHEDIR.TAG").is_file());
        assert!(parse(&bytes[..bytes.len() - 1], Path::new("/nowhere.raf")).is_none(), "torn");
        assert!(parse(b"NUMAPRX0xxxxxxxxxxxxxxxxxxxx", Path::new("/nowhere.raf")).is_none(), "foreign");

        let size = std::fs::metadata(&place.file).unwrap().len();
        std::fs::copy(&place.file, dir.join("older.proxy")).unwrap();
        let old = std::fs::File::options().write(true).open(dir.join("older.proxy")).unwrap();
        old.set_modified(std::time::SystemTime::now() - std::time::Duration::from_secs(3600)).unwrap();
        assert_eq!(usage(&dir), (2 * size, 2));
        trim(&dir, size + size / 2);
        assert_eq!(usage(&dir), (size, 1));
        assert!(place.file.is_file(), "the newer one stays");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
