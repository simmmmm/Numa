use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use image::RgbImage;

use crate::io::catalog::LIBRARY_DIR;
use crate::io::raw;

pub fn cache_dir() -> PathBuf {
    crate::io::cache_dir().join("thumbs")
}

const CACHE_VERSION: u32 = 2;

fn cache_path(path: &Path, mtime: i64, max_edge: u32) -> PathBuf {
    cache_dir().join(key(path, mtime, max_edge))
}

fn key(path: &Path, mtime: i64, max_edge: u32) -> String {
    let joined: Vec<String> = path.components().map(|part| part.as_os_str().to_string_lossy().into_owned()).collect();
    let text = format!("{CACHE_VERSION}\0{}\0{mtime}\0{max_edge}", joined.join("/"));
    let hash = text.bytes().fold(0xcbf2_9ce4_8422_2325u64, |hash, byte| (hash ^ byte as u64).wrapping_mul(0x100_0000_01b3));
    format!("{hash:016x}.jpg")
}

pub const LIBRARY_EDGE: u32 = 320;

fn library_path(path: &Path, mtime: i64, max_edge: u32) -> Option<PathBuf> {
    if max_edge != LIBRARY_EDGE {
        return None;
    }
    let root = library_root(path)?;
    let within = path.strip_prefix(&root).ok()?;
    Some(root.join(LIBRARY_DIR).join("thumbs").join(key(within, mtime, max_edge)))
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

fn candidates(path: &Path, mtime: i64, max_edge: u32) -> impl Iterator<Item = PathBuf> {
    library_path(path, mtime, max_edge).into_iter().chain([cache_path(path, mtime, max_edge)])
}

pub fn is_cached(path: &Path, mtime: i64, max_edge: u32) -> bool {
    candidates(path, mtime, max_edge).any(|cached| cached.exists())
}

pub fn load(path: &Path, mtime: i64, max_edge: u32) -> Result<RgbImage, String> {
    if let Some(image) = candidates(path, mtime, max_edge).find_map(|cached| image::open(cached).ok()) {
        return Ok(image.into_rgb8());
    }

    let image = raw::load_scaled(path, max_edge)?;

    let _ = candidates(path, mtime, max_edge).find(|cached| {
        cached.parent().is_some_and(|parent| std::fs::create_dir_all(parent).is_ok())
            && image.save_with_format(cached, image::ImageFormat::Jpeg).is_ok()
    });

    Ok(image)
}

pub fn cached_size(path: &Path, mtime: i64, max_edge: u32) -> Option<(u32, u32)> {
    use std::io::Read;

    let file = candidates(path, mtime, max_edge).find_map(|cached| std::fs::File::open(cached).ok())?;
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
        assert_eq!(key(Path::new("2024/Japan/DSCF0001.RAF"), 1_700_000_000, 320), key(Path::new("2024/Japan/DSCF0001.RAF"), 1_700_000_000, 320));
        let text = format!("{CACHE_VERSION}\0a/b.RAF\01\0320");
        let expected = text.bytes().fold(0xcbf2_9ce4_8422_2325u64, |hash, byte| (hash ^ byte as u64).wrapping_mul(0x100_0000_01b3));
        assert_eq!(key(Path::new("a/b.RAF"), 1, 320), format!("{expected:016x}.jpg"));
    }

    #[test]
    fn second_load_comes_from_cache() {
        const MTIME: i64 = 1_700_000_000;

        let dir = std::env::temp_dir().join("numa-thumbs-test");
        let _ = std::fs::remove_dir_all(&dir);

        crate::io::use_test_cache(std::env::temp_dir().join("numa-thumbs-test-cache"));
        std::fs::create_dir_all(&dir).unwrap();
        let source = dir.join("photo.png");
        RgbImage::from_fn(900, 600, |x, y| image::Rgb([x as u8, y as u8, 9]))
            .save(&source)
            .unwrap();

        let first = load(&source, MTIME, 200).unwrap();
        assert_eq!((first.width(), first.height()), (200, 133));

        let cached = cache_path(&source, MTIME, 200);
        assert!(cached.exists(), "first load must populate the cache");

        assert_eq!(cached_size(&source, MTIME, 200), Some((200, 133)));
        assert_eq!(cached_size(&source, MTIME, 400), None);

        std::fs::remove_file(&source).unwrap();
        let second = load(&source, MTIME, 200).unwrap();
        assert_eq!((second.width(), second.height()), (200, 133));

        assert!(load(&source, MTIME + 1, 200).is_err(), "newer mtime must invalidate");
        assert!(load(&source, MTIME, 400).is_err(), "another size is another entry");

        std::fs::remove_file(&cached).unwrap();
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_moved_library_brings_its_thumbnails() {
        const MTIME: i64 = 1_700_000_000;

        let dir = std::env::temp_dir().join("numa-thumbs-library-test");
        let _ = std::fs::remove_dir_all(&dir);
        crate::io::use_test_cache(std::env::temp_dir().join("numa-thumbs-test-cache"));
        let here = dir.join("here");
        std::fs::create_dir_all(here.join(LIBRARY_DIR)).unwrap();
        std::fs::create_dir_all(here.join("shoot")).unwrap();
        let source = here.join("shoot/photo.png");
        RgbImage::from_fn(900, 600, |x, y| image::Rgb([x as u8, y as u8, 9])).save(&source).unwrap();

        load(&source, MTIME, LIBRARY_EDGE).unwrap();
        assert!(!cache_path(&source, MTIME, LIBRARY_EDGE).exists(), "the grid's size is not in the cache");
        load(&source, MTIME, 640).unwrap();
        assert!(cache_path(&source, MTIME, 640).exists(), "a larger size is");

        let there = dir.join("there");
        std::fs::rename(&here, &there).unwrap();
        let moved = there.join("shoot/photo.png");
        std::fs::remove_file(&moved).unwrap();
        assert_eq!(cached_size(&moved, MTIME, LIBRARY_EDGE), Some((320, 213)));
        assert!(load(&moved, MTIME, LIBRARY_EDGE).is_ok());

        std::fs::remove_dir_all(&dir).unwrap();
    }
}
