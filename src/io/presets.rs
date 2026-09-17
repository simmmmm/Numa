use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::core::document::{Document, EditParts};
use crate::io::foreign;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Preset {
    pub parts: EditParts,
    pub document: Document,
}

pub fn dir() -> PathBuf {
    crate::io::data_dir().join("presets")
}

pub fn list(folder: &Path) -> Vec<String> {
    let names = |dir: &Path| -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .into_iter()
            .flatten()
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "json"))
            .filter_map(|path| Some(path.file_stem()?.to_string_lossy().into_owned()))
            .collect();
        names.sort_by_key(|name| name.to_lowercase());
        names
    };
    let mut groups: Vec<PathBuf> = std::fs::read_dir(folder)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| path.is_dir())
        .collect();
    groups.sort_by_key(|path| path.to_string_lossy().to_lowercase());

    let mut all = names(folder);
    for group in groups {
        let Some(label) = group.file_name().map(|name| name.to_string_lossy().into_owned()) else { continue };
        all.extend(names(&group).into_iter().map(|name| format!("{label}/{name}")));
    }
    all
}

pub fn load(folder: &Path, name: &str) -> Result<Preset, String> {
    read(&folder.join(format!("{name}.json")))
}

fn read(path: &Path) -> Result<Preset, String> {
    let text = std::fs::read_to_string(path).map_err(|err| format!("{}: {err}", path.display()))?;
    serde_json::from_str(&text).map_err(|err| format!("{} is not a preset: {err}", path.display()))
}

pub fn save(folder: &Path, name: &str, preset: &Preset) -> Result<(), String> {
    save_in(folder, None, name, preset)
}

fn save_in(folder: &Path, group: Option<&str>, name: &str, preset: &Preset) -> Result<(), String> {
    let name = name.trim();
    let valid = |part: &str| !part.is_empty() && !part.starts_with('.') && !part.contains(['/', '\\']);
    if !valid(name) || group.is_some_and(|group| !valid(group)) {
        return Err(format!("“{name}” cannot be a preset name"));
    }
    let folder = match group {
        Some(group) => folder.join(group),
        None => folder.to_path_buf(),
    };
    let path = folder.join(format!("{name}.json"));
    if path.exists() {
        return Err(format!("There is already a preset called “{name}”"));
    }
    std::fs::create_dir_all(&folder).map_err(|err| err.to_string())?;
    let json = serde_json::to_string_pretty(preset).map_err(|err| err.to_string())?;
    std::fs::write(&path, json).map_err(|err| format!("{}: {err}", path.display()))
}

#[derive(Debug, Default)]
pub struct Imported {
    pub presets: usize,

    pub existing: usize,

    pub refused: Vec<String>,

    pub ignored: BTreeMap<&'static str, usize>,
}

pub fn import(folder: &Path, dropped: &[PathBuf]) -> Imported {
    let mut imported = Imported::default();
    let mut pending: Vec<PathBuf> = dropped.to_vec();
    while let Some(path) = pending.pop() {
        let name = path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();

        if name.starts_with("._") || name == "__MACOSX" {
            continue;
        }
        if path.is_dir() {
            if let Ok(entries) = std::fs::read_dir(&path) {
                pending.extend(entries.flatten().map(|entry| entry.path()));
            }
            continue;
        }
        let group = path.parent().and_then(Path::file_name).map(|name| clean(&name.to_string_lossy()));
        let extension = path.extension().map(|ext| ext.to_string_lossy().to_ascii_lowercase()).unwrap_or_default();
        match extension.as_str() {
            "json" => match read(&path) {
                Ok(preset) => {
                    let stem = path.file_stem().map(|stem| stem.to_string_lossy().into_owned()).unwrap_or_default();
                    record(&mut imported, save(folder, &clean(&stem), &preset));
                }
                Err(err) => imported.refused.push(err),
            },
            "costylepack" => import_pack(folder, &path, &mut imported),
            _ if foreign::is_foreign(&path) => match foreign::translate_file(&path) {
                Ok(translated) => keep(folder, group.as_deref(), translated, &mut imported),
                Err(err) => imported.refused.push(err),
            },
            _ => {}
        }
    }
    imported
}

fn import_pack(folder: &Path, path: &Path, imported: &mut Imported) {
    let group = path.file_stem().map(|stem| clean(&stem.to_string_lossy()));
    let archive = std::fs::File::open(path).map_err(|err| err.to_string()).and_then(|file| {
        zip::ZipArchive::new(std::io::BufReader::new(file)).map_err(|err| err.to_string())
    });
    let mut archive = match archive {
        Ok(archive) => archive,
        Err(err) => {
            imported.refused.push(format!("{}: {err}", path.display()));
            return;
        }
    };
    for index in 0..archive.len() {
        let Ok(mut entry) = archive.by_index(index) else { continue };
        let inner = entry.name().to_string();
        let file = inner.rsplit('/').next().unwrap_or(&inner).to_string();
        if !file.to_ascii_lowercase().ends_with(".costyle") || file.starts_with("._") || inner.contains("__MACOSX") {
            continue;
        }
        let mut text = String::new();
        if std::io::Read::read_to_string(&mut entry, &mut text).is_err() {
            imported.refused.push(format!("{}: {inner} could not be read", path.display()));
            continue;
        }
        let stem = file.trim_end_matches(".costyle").trim_end_matches(".COSTYLE");
        match foreign::translate(&text, "costyle", stem) {
            Ok(translated) => keep(folder, group.as_deref(), translated, imported),
            Err(err) => imported.refused.push(format!("{}: {inner}: {err}", path.display())),
        }
    }
}

fn keep(folder: &Path, group: Option<&str>, translated: foreign::Translated, imported: &mut Imported) {
    let saved = save_in(folder, group, &clean(&translated.name), &translated.preset);
    if saved.is_ok() {
        for what in translated.ignored {
            *imported.ignored.entry(what).or_default() += 1;
        }
    }
    record(imported, saved);
}

fn record(imported: &mut Imported, saved: Result<(), String>) {
    match saved {
        Ok(()) => imported.presets += 1,
        Err(err) if err.starts_with("There is already") => imported.existing += 1,
        Err(err) => imported.refused.push(err),
    }
}

fn clean(name: &str) -> String {
    let name: String = name.trim().chars().map(|c| if matches!(c, '/' | '\\') { '-' } else { c }).collect();
    name.trim_start_matches('.').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::document::Basic;

    #[test]
    fn a_preset_is_saved_listed_imported_and_never_overwritten() {
        let root = std::env::temp_dir().join("numa-test-presets");
        let _ = std::fs::remove_dir_all(&root);
        let (folder, elsewhere) = (root.join("presets"), root.join("elsewhere"));
        std::fs::create_dir_all(elsewhere.join("Film Pack")).unwrap();

        let mut document = Document::new(String::new());
        document.set_basic(Basic { exposure: 0.5, ..Default::default() });
        let preset = Preset { parts: EditParts { tone: true, ..EditParts::nothing() }, document };

        save(&folder, "Warm evening", &preset).unwrap();
        assert!(save(&folder, "Warm evening", &preset).is_err(), "not over an existing one");
        assert!(save(&folder, "a/b", &preset).is_err());
        let back = load(&folder, "Warm evening").unwrap();
        assert_eq!(back.parts, preset.parts);
        assert_eq!(back.document.basic().exposure, 0.5);

        std::fs::copy(folder.join("Warm evening.json"), elsewhere.join("Shared.json")).unwrap();
        std::fs::write(elsewhere.join("Not a preset.json"), "{}").unwrap();
        std::fs::write(
            elsewhere.join("Film Pack").join("Portra.lrtemplate"),
            "s = { title = \"Portra 1/2\", value = { settings = { GrainAmount = 20, }, }, }",
        )
        .unwrap();
        std::fs::write(elsewhere.join("Film Pack").join("._Portra.lrtemplate"), [0u8, 5, 22]).unwrap();

        let result = import(&folder, &[elsewhere.clone()]);
        assert_eq!((result.presets, result.existing, result.refused.len()), (2, 0, 1), "{result:?}");
        assert_eq!(list(&folder), ["Shared", "Warm evening", "Film Pack/Portra 1-2"]);
        assert_eq!(load(&folder, "Film Pack/Portra 1-2").unwrap().document.basic().grain, 20.0);

        let again = import(&folder, &[elsewhere.clone()]);
        assert_eq!((again.presets, again.existing), (0, 2), "already there");

        std::fs::remove_dir_all(&root).unwrap();
    }
}
