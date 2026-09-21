use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use image::RgbImage;

use crate::catalog::LIBRARY_DIR;
use crate::raw;

mod edited;

pub fn cache_dir() -> PathBuf {
    numa_core::paths::cache_dir().join("thumbs")
}

const CACHE_VERSION: u32 = 2;

fn cache_path(path: &Path, mtime: i64, max_edge: u32, edits: Option<&str>) -> PathBuf {
    cache_dir().join(key(path, mtime, max_edge, edits))
}

fn key(path: &Path, mtime: i64, max_edge: u32, edits: Option<&str>) -> String {
    let joined: Vec<String> = path.components().map(|part| part.as_os_str().to_string_lossy().into_owned()).collect();
    let mut text = format!("{CACHE_VERSION}\0{}\0{mtime}\0{max_edge}", joined.join("/"));

    if let Some(edits) = edits {
        text.push('\0');
        text.push_str(edits);
    }
    let hash = text.bytes().fold(0xcbf2_9ce4_8422_2325u64, |hash, byte| (hash ^ byte as u64).wrapping_mul(0x100_0000_01b3));
    format!("{hash:016x}.jpg")
}

pub const LIBRARY_EDGE: u32 = 320;

fn library_path(path: &Path, mtime: i64, max_edge: u32, edits: Option<&str>) -> Option<PathBuf> {
    if max_edge != LIBRARY_EDGE {
        return None;
    }
    let root = library_root(path)?;
    let within = path.strip_prefix(&root).ok()?;
    Some(root.join(LIBRARY_DIR).join("thumbs").join(key(within, mtime, max_edge, edits)))
}

fn library_root(path: &Path) -> Option<PathBuf> {
    static ROOTS: Mutex<Option<HashMap<PathBuf, Option<PathBuf>>>> = Mutex::new(None);
    let dir = path.parent()?;
    let mut roots = ROOTS.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    roots
        .get_or_insert_with(HashMap::new)
        .entry(dir.to_path_buf())
        .or_insert_with(|| dir.ancestors().find(|folder| folder.join(LIBRARY_DIR).is_dir()).map(Path::to_path_buf))
        .clone()
}

pub const STEPS: [u32; 4] = [640, 960, 1280, 1920];

fn at_least(max_edge: u32) -> impl Iterator<Item = u32> {

    let ladder = (max_edge != LIBRARY_EDGE)
        .then_some(STEPS)
        .unwrap_or_default();
    std::iter::once(max_edge).chain(ladder.into_iter().filter(move |step| *step > max_edge))
}

fn candidates(path: &Path, mtime: i64, max_edge: u32, edits: Option<&str>) -> impl Iterator<Item = PathBuf> {
    library_path(path, mtime, max_edge, edits).into_iter().chain([cache_path(path, mtime, max_edge, edits)])
}

pub fn cached(path: &Path, mtime: i64, max_edge: u32, edits: Option<&str>) -> Option<PathBuf> {
    at_least(max_edge).find_map(|edge| candidates(path, mtime, edge, edits).find(|cached| cached.exists()))
}

pub fn any_cached(path: &Path, mtime: i64, edits: Option<&str>) -> Option<PathBuf> {
    std::iter::once(LIBRARY_EDGE)
        .chain(STEPS)
        .find_map(|edge| candidates(path, mtime, edge, edits).find(|cached| cached.exists()))
}

pub fn is_cached(path: &Path, mtime: i64, max_edge: u32, edits: Option<&str>) -> bool {
    cached(path, mtime, max_edge, edits).is_some()
}

pub fn load(path: &Path, mtime: i64, max_edge: u32, edits: Option<&str>) -> Result<RgbImage, String> {

    let found = at_least(max_edge).find_map(|edge| {
        candidates(path, mtime, edge, edits).find_map(|cached| image::open(cached).ok())
    });
    if let Some(image) = found {
        return Ok(match image.width().max(image.height()) > max_edge {
            true => image.thumbnail(max_edge, max_edge),
            false => image,
        }
        .into_rgb8());
    }

    let image = match edits {
        Some(edits) => edited::render(path, max_edge, edits)?,
        None => raw::load_scaled(path, max_edge)?,
    };

    let _ = candidates(path, mtime, max_edge, edits).find(|cached| {
        cached.parent().is_some_and(|parent| std::fs::create_dir_all(parent).is_ok())
            && image.save_with_format(cached, image::ImageFormat::Jpeg).is_ok()
    });

    if edits.is_some() {
        forget(path, mtime, max_edge, None);
    }

    Ok(image)
}

pub fn store(path: &Path, mtime: i64, max_edge: u32, edits: Option<&str>, image: &RgbImage) {
    let _ = candidates(path, mtime, max_edge, edits).find(|cached| {
        cached.parent().is_some_and(|parent| std::fs::create_dir_all(parent).is_ok())
            && image.save_with_format(cached, image::ImageFormat::Jpeg).is_ok()
    });
    if edits.is_some() {
        forget(path, mtime, max_edge, None);
    }
}

pub fn forget(path: &Path, mtime: i64, max_edge: u32, edits: Option<&str>) {
    for cached in candidates(path, mtime, max_edge, edits) {
        let _ = std::fs::remove_file(cached);
    }
}

pub fn cached_size(path: &Path, mtime: i64, max_edge: u32) -> Option<(u32, u32)> {
    use std::io::Read;

    let file = candidates(path, mtime, max_edge, None).find_map(|cached| std::fs::File::open(cached).ok())?;
    let mut reader = std::io::BufReader::with_capacity(1024, file);
    let mut marker = [0u8; 4];
    reader.read_exact(&mut marker[..2]).ok()?;
    if marker[..2] != [0xFF, 0xD8] {
        return None;
    }

    loop {
        reader.read_exact(&mut marker).ok()?;
        if marker[0] != 0xFF {
            return None;
        }
        let length = u16::from_be_bytes([marker[2], marker[3]]) as usize;

        if matches!(marker[1], 0xC0..=0xCF) && !matches!(marker[1], 0xC4 | 0xC8 | 0xCC) {
            let mut frame = [0u8; 5];
            reader.read_exact(&mut frame).ok()?;
            let height = u16::from_be_bytes([frame[1], frame[2]]) as u32;
            let width = u16::from_be_bytes([frame[3], frame[4]]) as u32;
            return (width > 0 && height > 0).then_some((width, height));
        }
        std::io::copy(&mut reader.by_ref().take(length.checked_sub(2)? as u64), &mut std::io::sink())
            .ok()?;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_thumbnail_name_does_not_depend_on_the_build() {
        assert_eq!(key(Path::new("2024/Japan/DSCF0001.RAF"), 1_700_000_000, 320, None), key(Path::new("2024/Japan/DSCF0001.RAF"), 1_700_000_000, 320, None));
        let text = format!("{CACHE_VERSION}\0a/b.RAF\01\0320");
        let expected = text.bytes().fold(0xcbf2_9ce4_8422_2325u64, |hash, byte| (hash ^ byte as u64).wrapping_mul(0x100_0000_01b3));
        assert_eq!(key(Path::new("a/b.RAF"), 1, 320, None), format!("{expected:016x}.jpg"));

        let edited = key(Path::new("a/b.RAF"), 1, 320, Some(r#"{"exposure":0.5}"#));
        assert_ne!(edited, key(Path::new("a/b.RAF"), 1, 320, None));
        assert_ne!(edited, key(Path::new("a/b.RAF"), 1, 320, Some(r#"{"exposure":0.6}"#)));
    }

    #[test]
    fn second_load_comes_from_cache() {
        const MTIME: i64 = 1_700_000_000;

        let dir = std::env::temp_dir().join("numa-thumbs-test");
        let _ = std::fs::remove_dir_all(&dir);

        numa_core::paths::use_test_cache(std::env::temp_dir().join("numa-thumbs-test-cache"));
        std::fs::create_dir_all(&dir).unwrap();
        let source = dir.join("photo.png");
        RgbImage::from_fn(900, 600, |x, y| image::Rgb([x as u8, y as u8, 9]))
            .save(&source)
            .unwrap();

        let first = load(&source, MTIME, 200, None).unwrap();
        assert_eq!((first.width(), first.height()), (200, 133));

        let cached = cache_path(&source, MTIME, 200, None);
        assert!(cached.exists(), "first load must populate the cache");

        assert_eq!(cached_size(&source, MTIME, 200), Some((200, 133)));
        assert_eq!(cached_size(&source, MTIME, 400), None);

        std::fs::remove_file(&source).unwrap();
        let second = load(&source, MTIME, 200, None).unwrap();
        assert_eq!((second.width(), second.height()), (200, 133));

        assert!(load(&source, MTIME + 1, 200, None).is_err(), "newer mtime must invalidate");
        assert!(load(&source, MTIME, 400, None).is_err(), "another size is another entry");

        std::fs::remove_file(&cached).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn an_edited_thumbnail_is_the_edit() {
        use numa_core::document::{Basic, Document};

        const MTIME: i64 = 1_700_000_000;

        let dir = std::env::temp_dir().join("numa-thumbs-edited-test");
        let _ = std::fs::remove_dir_all(&dir);
        numa_core::paths::use_test_cache(std::env::temp_dir().join("numa-thumbs-test-cache"));
        std::fs::create_dir_all(&dir).unwrap();
        let source = dir.join("photo.png");
        RgbImage::from_pixel(300, 200, image::Rgb([128, 128, 128])).save(&source).unwrap();

        let stack = |exposure: f32| {
            let mut document = Document::new(source.display().to_string());
            document.set_basic(Basic::with(|b| b.tone.exposure = exposure));
            serde_json::to_string(&document).unwrap()
        };
        let (dark, bright) = (stack(-1.0), stack(1.0));

        let grey = |image: &RgbImage| image.get_pixel(image.width() / 2, image.height() / 2).0[0];
        assert!(
            grey(&load(&source, MTIME, 64, Some(&dark)).unwrap()) < grey(&load(&source, MTIME, 64, Some(&bright)).unwrap()),
            "two stops apart has to look two stops apart"
        );

        assert!(cache_path(&source, MTIME, 64, Some(&dark)).exists());
        assert!(cache_path(&source, MTIME, 64, Some(&bright)).exists());
        assert!(!cache_path(&source, MTIME, 64, None).exists(), "nothing was asked for unedited");

        std::fs::remove_file(&source).unwrap();
        assert!(load(&source, MTIME, 64, Some(&dark)).is_ok(), "an edited thumbnail is cached too");
        assert!(load(&source, MTIME, 64, Some(&stack(0.5))).is_err(), "a changed stack is a miss");

        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_moved_library_brings_its_thumbnails() {
        const MTIME: i64 = 1_700_000_000;

        let dir = std::env::temp_dir().join("numa-thumbs-library-test");
        let _ = std::fs::remove_dir_all(&dir);
        numa_core::paths::use_test_cache(std::env::temp_dir().join("numa-thumbs-test-cache"));
        let here = dir.join("here");
        std::fs::create_dir_all(here.join(LIBRARY_DIR)).unwrap();
        std::fs::create_dir_all(here.join("shoot")).unwrap();
        let source = here.join("shoot/photo.png");
        RgbImage::from_fn(900, 600, |x, y| image::Rgb([x as u8, y as u8, 9])).save(&source).unwrap();

        load(&source, MTIME, LIBRARY_EDGE, None).unwrap();
        assert!(!cache_path(&source, MTIME, LIBRARY_EDGE, None).exists(), "the grid's size is not in the cache");
        load(&source, MTIME, 640, None).unwrap();
        assert!(cache_path(&source, MTIME, 640, None).exists(), "a larger size is");

        let there = dir.join("there");
        std::fs::rename(&here, &there).unwrap();
        let moved = there.join("shoot/photo.png");
        std::fs::remove_file(&moved).unwrap();
        assert_eq!(cached_size(&moved, MTIME, LIBRARY_EDGE), Some((320, 213)));
        assert!(load(&moved, MTIME, LIBRARY_EDGE, None).is_ok());

        std::fs::remove_dir_all(&dir).unwrap();
    }
}
