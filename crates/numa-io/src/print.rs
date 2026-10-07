use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq)]
pub struct Profile {
    pub file: String,
    pub name: String,

    pub rgb: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Target {

    pub file: String,

    pub perceptual: bool,
}

pub fn dir() -> PathBuf {
    numa_core::paths::data_dir().join("print-profiles")
}

pub fn profiles() -> Vec<Profile> {
    let Ok(entries) = std::fs::read_dir(dir()) else { return Vec::new() };
    let mut found: Vec<Profile> = entries
        .flatten()
        .filter_map(|entry| {
            let file = entry.file_name().to_str()?.to_string();
            let bytes = std::fs::read(entry.path()).ok()?;
            match numa_render::proof::describe(&bytes) {
                Ok(about) => Some(Profile { file, name: about.name, rgb: about.rgb }),
                Err(err) => {
                    log::warn!("{}: {err}", entry.path().display());
                    None
                }
            }
        })
        .collect();
    found.sort_by_key(|profile| profile.name.to_lowercase());
    found
}

pub fn add(path: &Path) -> Result<Profile, String> {
    let bytes = std::fs::read(path).map_err(|err| format!("{}: {err}", path.display()))?;
    let about = numa_render::proof::describe(&bytes)?;
    let dir = dir();
    std::fs::create_dir_all(&dir).map_err(|err| format!("{}: {err}", dir.display()))?;
    let stem = path.file_stem().and_then(|stem| stem.to_str()).unwrap_or("profile");
    let extension = path.extension().and_then(|extension| extension.to_str()).unwrap_or("icc");
    for number in 1..1000 {
        let file = match number {
            1 => format!("{stem}.{extension}"),
            n => format!("{stem} {n}.{extension}"),
        };
        let into = dir.join(&file);
        match std::fs::read(&into) {
            Ok(there) if there == bytes => return Ok(Profile { file, name: about.name, rgb: about.rgb }),
            Ok(_) => continue,
            Err(_) => {
                std::fs::write(&into, &bytes).map_err(|err| format!("{}: {err}", into.display()))?;
                return Ok(Profile { file, name: about.name, rgb: about.rgb });
            }
        }
    }
    Err(format!("no free name for {stem} in {}", dir.display()))
}

pub fn bytes(file: &str) -> Result<Vec<u8>, String> {

    if file.contains(['/', '\\']) || file.starts_with('.') {
        return Err(format!("{file} is not a profile's name"));
    }
    std::fs::read(dir().join(file)).map_err(|err| format!("the profile {file} is gone: {err}"))
}

pub fn convert(frame: &crate::export::Developed, target: &Target) -> Result<crate::export::Developed, String> {
    use crate::export::Developed;
    let bytes = bytes(&target.file)?;
    Ok(match frame {
        Developed::Eight(image) => Developed::Eight(numa_render::proof::convert8(image, &bytes, target.perceptual)?),
        Developed::Sixteen(image) => Developed::Sixteen(numa_render::proof::convert16(image, &bytes, target.perceptual)?),
        Developed::Hdr(..) | Developed::Dng(..) => return Err("a print profile is for JPEG, PNG and TIFF".to_string()),
    })
}
