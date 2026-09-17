use std::path::{Path, PathBuf};

use crate::render::ai_denoise::{Denoised, MODEL};

const VERSION: u32 = 1;

fn dir() -> PathBuf {
    crate::io::cache_dir().join("denoised")
}

pub fn cache_path(photo: &Path) -> Option<PathBuf> {
    let modified = std::fs::metadata(photo).ok()?.modified().ok()?;
    let seconds = modified.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs();
    Some(dir().join(key(photo, seconds)))
}

fn key(photo: &Path, mtime: u64) -> String {
    let text = format!("{VERSION}\0{MODEL}\0{}\0{mtime}", photo.display());
    let hash = text.bytes().fold(0xcbf2_9ce4_8422_2325u64, |hash, byte| (hash ^ byte as u64).wrapping_mul(0x100_0000_01b3));
    format!("{hash:016x}.png")
}

pub fn is_cached(photo: &Path) -> bool {
    cache_path(photo).is_some_and(|path| path.is_file())
}

pub fn load(photo: &Path) -> Option<Denoised> {
    image::open(cache_path(photo)?).ok().map(|image| image.into_rgb16())
}

pub fn save(photo: &Path, denoised: &Denoised) -> Result<(), String> {
    let path = cache_path(photo).ok_or_else(|| format!("{} cannot be read", photo.display()))?;
    std::fs::create_dir_all(dir()).map_err(|err| err.to_string())?;
    let part = path.with_extension("part");
    let file = std::fs::File::create(&part).map_err(|err| err.to_string())?;
    encode(denoised, std::io::BufWriter::new(file))?;
    std::fs::rename(&part, &path).map_err(|err| err.to_string())
}

pub fn encode(denoised: &Denoised, writer: impl std::io::Write) -> Result<(), String> {
    use image::codecs::png::{CompressionType, FilterType, PngEncoder};
    denoised
        .write_with_encoder(PngEncoder::new_with_quality(writer, CompressionType::Default, FilterType::Adaptive))
        .map_err(|err| err.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_key_follows_the_file() {
        let photo = Path::new("/photos/DSCF0001.RAF");
        assert_eq!(key(photo, 1_700_000_000), key(photo, 1_700_000_000));
        assert_ne!(key(photo, 1_700_000_000), key(photo, 1_700_000_001), "saved again is a different photograph");
        assert_ne!(key(photo, 1_700_000_000), key(Path::new("/photos/DSCF0002.RAF"), 1_700_000_000));
    }
}
