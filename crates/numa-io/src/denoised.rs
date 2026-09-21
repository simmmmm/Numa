use std::path::{Path, PathBuf};

use numa_core::denoise::{Denoised, MODEL, SHARPEN_MODEL};

const VERSION: u32 = 1;

fn dir() -> PathBuf {
    numa_core::paths::cache_dir().join("denoised")
}

pub fn cache_path(photo: &Path) -> Option<PathBuf> {
    path_for(photo, MODEL)
}

pub fn sharpened_path(photo: &Path, on_denoised: bool) -> Option<PathBuf> {
    path_for(photo, if on_denoised { "sharpen-on-denoised" } else { SHARPEN_MODEL })
}

fn path_for(photo: &Path, pass: &str) -> Option<PathBuf> {
    let modified = std::fs::metadata(photo).ok()?.modified().ok()?;
    let seconds = modified.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs();
    Some(dir().join(key(photo, seconds, pass)))
}

fn key(photo: &Path, mtime: u64, pass: &str) -> String {
    let text = format!("{VERSION}\0{pass}\0{}\0{mtime}", photo.display());
    let hash = text.bytes().fold(0xcbf2_9ce4_8422_2325u64, |hash, byte| (hash ^ byte as u64).wrapping_mul(0x100_0000_01b3));
    format!("{hash:016x}.png")
}

pub fn is_cached(photo: &Path) -> bool {
    cache_path(photo).is_some_and(|path| path.is_file())
}

pub fn load(photo: &Path) -> Option<Denoised> {
    image::open(cache_path(photo)?).ok().map(|image| image.into_rgb16())
}

pub fn is_sharpened(photo: &Path, on_denoised: bool) -> bool {
    sharpened_path(photo, on_denoised).is_some_and(|path| path.is_file())
}

pub fn load_sharpened(photo: &Path, on_denoised: bool) -> Option<Denoised> {
    image::open(sharpened_path(photo, on_denoised)?).ok().map(|image| image.into_rgb16())
}

pub fn save(photo: &Path, denoised: &Denoised) -> Result<(), String> {
    let path = cache_path(photo).ok_or_else(|| format!("{} cannot be read", photo.display()))?;
    write_at(&path, denoised)
}

fn write_at(path: &Path, denoised: &Denoised) -> Result<(), String> {
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
        assert_eq!(key(photo, 1_700_000_000, MODEL), key(photo, 1_700_000_000, MODEL));
        assert_ne!(key(photo, 1_700_000_000, MODEL), key(photo, 1_700_000_001, MODEL), "saved again is a different photograph");
        assert_ne!(key(photo, 1_700_000_000, MODEL), key(Path::new("/photos/DSCF0002.RAF"), 1_700_000_000, MODEL));
        assert_ne!(key(photo, 1_700_000_000, MODEL), key(photo, 1_700_000_000, SHARPEN_MODEL), "each pass its own");
    }
}

pub fn ensure(
    photo: &Path,
    full: &numa_core::image::LinearImage,
    progress: impl FnMut(usize, usize) -> bool,
) -> Result<bool, String> {
    if is_cached(photo) {
        return Ok(true);
    }
    if cache_path(photo).is_none() {
        return Ok(false);
    }
    let Some(denoised) = numa_render::ai_denoise::denoise(full, progress)? else {
        return Ok(false);
    };
    save(photo, &denoised)?;
    Ok(true)
}

pub fn ensure_sharpened(
    photo: &Path,
    full: &numa_core::image::LinearImage,
    on_denoised: bool,
    mut progress: impl FnMut(usize, usize) -> bool,
) -> Result<bool, String> {
    if is_sharpened(photo, on_denoised) {
        return Ok(true);
    }
    let Some(path) = sharpened_path(photo, on_denoised) else { return Ok(false) };
    let denoised = match on_denoised {
        true => {
            if !ensure(photo, full, &mut progress)? {
                return Ok(false);
            }
            let stored = load(photo).ok_or("the denoised frame could not be read")?;
            let mut whole = numa_core::document::Document::new(photo.display().to_string());
            whole.ai_denoise = 100.0;
            numa_render::ai_denoise::for_render(&whole, full, Some(&stored))
        }
        false => None,
    };
    let Some(sharpened) = numa_render::ai_denoise::sharpen(denoised.as_ref().unwrap_or(full), progress)? else {
        return Ok(false);
    };
    write_at(&path, &sharpened)?;
    Ok(true)
}
