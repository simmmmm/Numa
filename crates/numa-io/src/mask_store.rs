use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use numa_core::mask::{Mask, Pixels, Shape, Stored};

const MAGIC: &[u8; 8] = b"NUMAMSK2";

fn hash(parts: &[&[u8]]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for part in parts {
        for byte in part.iter().chain(&[0xff]) {
            hash ^= *byte as u64;
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    hash
}

pub fn key(photo: &Path, framing: &str, recipe: &str) -> String {
    let (size, modified) = std::fs::metadata(photo)
        .map(|meta| {
            let modified = meta
                .modified()
                .ok()
                .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                .map_or(0, |time| time.as_secs());
            (meta.len(), modified)
        })
        .unwrap_or((0, 0));
    let path = photo.to_string_lossy();
    format!(
        "{:016x}",
        hash(&[path.as_bytes(), &size.to_le_bytes(), &modified.to_le_bytes(), framing.as_bytes(), recipe.as_bytes()])
    )
}

pub fn recipe(mask: &Mask) -> String {
    answered_by(mask, &numa_render::matte::answering())
}

fn answered_by(mask: &Mask, subject_model: &str) -> String {
    let mut bare = mask.clone();
    bare.basic = Default::default();
    bare.curve = Default::default();
    bare.channel_curves = Default::default();
    bare.mixer = Default::default();
    bare.point_colours = Default::default();
    bare.grading = Default::default();
    bare.colour = Default::default();
    bare.opacity = 1.0;
    bare.visible = true;
    bare.inverted = false;
    bare.name = None;
    bare.feather = 0.0;
    bare.shift = 0.0;
    bare.minus_masks = Vec::new();
    bare.id = 0;
    let recipe = serde_json::to_string(&bare).unwrap_or_default();

    let recipe = match mask.points.is_empty() {
        true => recipe,
        false => format!("{recipe}{}", numa_render::sam::ANSWERS),
    };
    match mask.shape == Shape::Subject || mask.matte {
        true => format!("{recipe}{subject_model}"),
        false => recipe,
    }
}

pub fn worth_keeping(mask: &Mask) -> bool {
    !matches!(mask.shape, Shape::Linear { .. } | Shape::Radial { .. })
        && (mask.shape.is_found() || !mask.points.is_empty() || mask.matte || !mask.minus.is_empty())
}

pub fn dir() -> PathBuf {
    numa_core::paths::cache_dir().join("masks")
}

fn file(key: &str) -> PathBuf {
    dir().join(format!("{key}.mask"))
}

pub fn load(key: &str) -> Option<(Pixels, bool)> {
    let bytes = std::fs::read(file(key)).ok()?;
    let (head, rest) = bytes.split_at_checked(8 + 4 + 4 + 1)?;
    if &head[..8] != MAGIC {
        return None;
    }
    let width = u32::from_le_bytes(head[8..12].try_into().ok()?) as usize;
    let height = u32::from_le_bytes(head[12..16].try_into().ok()?) as usize;
    let matted = head[16] == 1;
    let mut raw = Vec::new();
    flate2::read::DeflateDecoder::new(rest).read_to_end(&mut raw).ok()?;
    if raw.len() != width * height * 2 {
        return None;
    }
    let cells = raw.chunks_exact(2).map(|pair| u16::from_le_bytes([pair[0], pair[1]])).collect();
    Some((Pixels(Some(std::sync::Arc::new(Stored::of_cells(width, height, cells)))), matted))
}

pub fn save(key: &str, pixels: &Pixels, matted: bool) {
    let Some(stored) = pixels.0.as_deref() else { return };
    let path = file(key);
    if let Some(folder) = path.parent() {
        let _ = std::fs::create_dir_all(folder);
    }
    let mut bytes = Vec::with_capacity(17 + stored.cells().len());
    bytes.extend_from_slice(MAGIC);
    bytes.extend_from_slice(&(stored.width as u32).to_le_bytes());
    bytes.extend_from_slice(&(stored.height as u32).to_le_bytes());
    bytes.push(matted as u8);
    let mut encoder = flate2::write::DeflateEncoder::new(bytes, flate2::Compression::fast());
    for cell in stored.cells() {
        if encoder.write_all(&cell.to_le_bytes()).is_err() {
            return;
        }
    }
    if let Ok(bytes) = encoder.finish() {

        let partial = path.with_extension("partial");
        if std::fs::write(&partial, bytes).is_ok() {
            let _ = std::fs::rename(partial, path);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use numa_core::mask::Alpha;

    #[test]
    fn a_kept_mask_reads_back_as_it_was() {
        numa_core::paths::use_test_cache(std::env::temp_dir().join(format!("numa-mask-store-{}", std::process::id())));
        let alpha = Alpha::new(4, 3, (0..12).map(|index| index as f32 / 11.0).collect());
        let pixels = Pixels::of(&alpha);

        let frame = format!("/nowhere/{}-{:?}.raf", std::process::id(), std::time::SystemTime::now());
        let key = key(Path::new(&frame), "framing", "recipe");
        assert!(load(&key).is_none());
        save(&key, &pixels, true);
        let (back, matted) = load(&key).expect("kept");
        assert!(matted);
        let (kept, read) = (pixels.0.unwrap(), back.0.unwrap());
        assert_eq!((kept.width, kept.height), (read.width, read.height));
        assert_eq!(kept.cells(), read.cells());
        assert_ne!(key, super::key(Path::new(&frame), "framing", "another recipe"));
    }

    #[test]
    fn a_subject_is_kept_per_subject_model() {
        let subject = Mask::new(Shape::Subject);
        assert_ne!(answered_by(&subject, "isnet.onnx"), answered_by(&subject, "birefnet.onnx"));
        let mut person = Mask::new(Shape::Segment { classes: vec![12] });
        person.set_matte(true);
        assert_ne!(answered_by(&person, "isnet.onnx"), answered_by(&person, "birefnet.onnx"));
        let sky = Mask::new(Shape::Segment { classes: vec![2] });
        assert_eq!(answered_by(&sky, "isnet.onnx"), answered_by(&sky, "birefnet.onnx"));
    }
}
