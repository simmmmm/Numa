use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use numa_core::curve::Curve;
use numa_core::document::{Document, EditParts};
use crate::foreign;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Preset {
    pub parts: EditParts,
    pub document: Document,
}

pub fn dir() -> PathBuf {
    numa_core::paths::data_dir().join("presets")
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

    let mut all = Vec::new();
    let mut ungrouped = names(folder);
    for group in groups {
        let Some(label) = group.file_name().map(|name| name.to_string_lossy().into_owned()) else { continue };
        let named = names(&group).into_iter().map(|name| format!("{label}/{name}"));
        if label == LOOKS_GROUP {
            all.splice(0..0, named);
        } else {
            ungrouped.extend(named);
        }
    }
    all.extend(ungrouped);
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

#[cfg(numa_looks)]
pub const LOOKS: &[(&str, &str)] = &[
    ("Soft Film", include_str!("../looks/Soft Film.json")),
    ("Vivid", include_str!("../looks/Vivid.json")),
    ("Warm", include_str!("../looks/Warm.json")),
    ("Cool Fade", include_str!("../looks/Cool Fade.json")),
    ("Moody", include_str!("../looks/Moody.json")),
    ("Golden Hour", include_str!("../looks/Golden Hour.json")),
    ("Matte", include_str!("../looks/Matte.json")),
    ("Teal & Orange", include_str!("../looks/Teal & Orange.json")),
    ("Cross Process", include_str!("../looks/Cross Process.json")),
    ("Vintage", include_str!("../looks/Vintage.json")),
    ("Silver", include_str!("../looks/Silver.json")),
    ("Noir", include_str!("../looks/Noir.json")),
];

#[cfg(not(numa_looks))]
pub const LOOKS: &[(&str, &str)] = &[];

const LOOKS_GROUP: &str = "Numa";

const LOOKS_SEEDED: &str = ".looks-seeded";

pub fn seed_looks(folder: &Path) -> Result<(), String> {
    let marker = folder.join(LOOKS_SEEDED);
    let seeded = std::fs::read_to_string(&marker).unwrap_or_default();
    let new: Vec<&(&str, &str)> = LOOKS.iter().filter(|(name, _)| !seeded.lines().any(|line| line == *name)).collect();
    if new.is_empty() {
        return Ok(());
    }
    let group = folder.join(LOOKS_GROUP);
    std::fs::create_dir_all(&group).map_err(|err| format!("{}: {err}", group.display()))?;
    let mut record = seeded;
    for (name, json) in new {
        let path = group.join(format!("{name}.json"));

        if !path.exists() {
            std::fs::write(&path, json).map_err(|err| format!("{}: {err}", path.display()))?;
        }
        record.push_str(name);
        record.push('\n');
    }
    std::fs::write(&marker, record).map_err(|err| format!("{}: {err}", marker.display()))
}

pub fn at_strength(base: &Document, preset: &Preset, amount: f32) -> Document {
    let mut target = base.clone();
    target.copy_from(&preset.document, preset.parts);

    if !crate::raw::is_raw(Path::new(&base.source.path)) {
        target.white_balance = base.white_balance;
    }
    let t = amount.clamp(0.0, 1.0);
    if t >= 1.0 {
        return target;
    }
    if t <= 0.0 {
        return base.clone();
    }

    let mut out = if t >= 0.5 { target.clone() } else { base.clone() };

    out.set_basic(mix(&base.basic(), &target.basic(), t));
    out.set_mixer(mix(&base.mixer(), &target.mixer(), t));
    out.set_point_colours(mix(&base.point_colours(), &target.point_colours(), t));

    let (from, to) = (base.grading(), target.grading());
    let mut grading = mix(&from, &to, t);

    for (range, (a, b)) in [
        (&mut grading.shadows, (from.shadows, to.shadows)),
        (&mut grading.midtones, (from.midtones, to.midtones)),
        (&mut grading.highlights, (from.highlights, to.highlights)),
        (&mut grading.global, (from.global, to.global)),
    ] {
        range.hue = if a.saturation == 0.0 {
            b.hue
        } else if b.saturation == 0.0 {
            a.hue
        } else {
            let turn = (b.hue - a.hue + 540.0).rem_euclid(360.0) - 180.0;
            (a.hue + turn * t).rem_euclid(360.0)
        };
    }
    out.set_grading(grading);

    let curves = base.curves().into_iter().zip(target.curves()).map(|(a, b)| {
        let a = if a.is_identity() { Curve::new(b.points().iter().map(|p| [p[0], p[0]])) } else { a };
        mix(&a, &b, t)
    });
    out.set_curves(curves.collect::<Vec<_>>().try_into().unwrap_or_else(|_| target.curves()));

    out.ai_denoise = base.ai_denoise + (target.ai_denoise - base.ai_denoise) * t;
    out.ai_sharpen = base.ai_sharpen + (target.ai_sharpen - base.ai_sharpen) * t;

    if let Some(mut lut) = target.lut.clone() {
        let from = base.lut.as_ref().filter(|was| was.name == lut.name).map_or(0.0, |was| was.amount);
        lut.amount = from + (lut.amount - from) * t;
        out.lut = Some(lut);
    }
    if let (Some(a), Some(b)) = (base.white_balance, target.white_balance) {
        out.white_balance = Some(mix(&a, &b, t));
    }
    out
}

fn mix<T: Serialize + serde::de::DeserializeOwned + Clone>(a: &T, b: &T, t: f32) -> T {
    let (Ok(from), Ok(to)) = (serde_json::to_value(a), serde_json::to_value(b)) else { return b.clone() };
    serde_json::from_value(lerp(&from, &to, t as f64)).unwrap_or_else(|_| if t >= 0.5 { b.clone() } else { a.clone() })
}

fn lerp(a: &serde_json::Value, b: &serde_json::Value, t: f64) -> serde_json::Value {
    use serde_json::Value;
    match (a, b) {

        (Value::Number(x), Value::Number(y)) if x.is_f64() || y.is_f64() => {
            let (x, y) = (x.as_f64().unwrap_or(0.0), y.as_f64().unwrap_or(0.0));
            serde_json::Number::from_f64(x + (y - x) * t).map_or_else(|| b.clone(), Value::Number)
        }
        (Value::Array(x), Value::Array(y)) if x.len() == y.len() => {
            Value::Array(x.iter().zip(y).map(|(x, y)| lerp(x, y, t)).collect())
        }
        (Value::Object(x), Value::Object(y)) => Value::Object(
            y.iter().map(|(key, y)| (key.clone(), x.get(key).map_or_else(|| y.clone(), |x| lerp(x, y, t)))).collect(),
        ),
        _ if t >= 0.5 => b.clone(),
        _ => a.clone(),
    }
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
    use numa_core::document::Basic;

    #[test]
    fn a_preset_is_saved_listed_imported_and_never_overwritten() {
        let root = std::env::temp_dir().join("numa-test-presets");
        let _ = std::fs::remove_dir_all(&root);
        let (folder, elsewhere) = (root.join("presets"), root.join("elsewhere"));
        std::fs::create_dir_all(elsewhere.join("Film Pack")).unwrap();

        let mut document = Document::new(String::new());
        document.set_basic(Basic::with(|b| b.tone.exposure = 0.5));
        let preset = Preset { parts: EditParts { tone: true, ..EditParts::nothing() }, document };

        save(&folder, "Warm evening", &preset).unwrap();
        assert!(save(&folder, "Warm evening", &preset).is_err(), "not over an existing one");
        assert!(save(&folder, "a/b", &preset).is_err());
        let back = load(&folder, "Warm evening").unwrap();
        assert_eq!(back.parts, preset.parts);
        assert_eq!(back.document.basic().tone.exposure, 0.5);

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
        assert_eq!(load(&folder, "Film Pack/Portra 1-2").unwrap().document.basic().effects.grain, 20.0);

        let again = import(&folder, &[elsewhere.clone()]);
        assert_eq!((again.presets, again.existing), (0, 2), "already there");

        std::fs::remove_dir_all(&root).unwrap();
    }

    #[cfg(numa_looks)]
    #[test]
    fn the_looks_arrive_once_and_a_deleted_one_stays_deleted() {
        let folder = std::env::temp_dir().join(format!("numa-test-looks-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);

        seed_looks(&folder).unwrap();
        let listed = list(&folder);
        assert_eq!(listed.len(), LOOKS.len());
        assert!(listed.iter().all(|name| name.starts_with("Numa/")), "{listed:?}");

        std::fs::create_dir_all(folder.join("Film")).unwrap();
        std::fs::write(folder.join("A mine.json"), "{}").unwrap();
        std::fs::write(folder.join("Film/Portra.json"), "{}").unwrap();
        let listed = list(&folder);
        assert!(listed[0].starts_with("Numa/") && listed[LOOKS.len()..] == ["A mine", "Film/Portra"], "{listed:?}");
        std::fs::remove_file(folder.join("A mine.json")).unwrap();
        std::fs::remove_dir_all(folder.join("Film")).unwrap();

        std::fs::remove_file(folder.join("Numa/Vivid.json")).unwrap();
        seed_looks(&folder).unwrap();
        assert_eq!(list(&folder).len(), LOOKS.len() - 1);
        assert!(!list(&folder).contains(&"Numa/Vivid".to_string()));

        std::fs::write(folder.join(LOOKS_SEEDED), "Soft Film\n").unwrap();
        std::fs::remove_file(folder.join("Numa/Warm.json")).unwrap();
        seed_looks(&folder).unwrap();
        assert!(list(&folder).contains(&"Numa/Warm".to_string()));

        std::fs::remove_dir_all(&folder).unwrap();
    }

    #[cfg(numa_looks)]
    #[test]
    fn every_look_applies_and_keeps_the_photographs_own() {
        use numa_core::color::WhiteBalance;
        for (name, json) in LOOKS {
            let look: Preset = serde_json::from_str(json).unwrap_or_else(|err| panic!("{name}: {err}"));
            assert!(!look.parts.white_balance && !look.parts.detail && !look.parts.geometry, "{name}");
            assert_eq!(look.parts.curve, look.document.curves().iter().any(|curve| !curve.is_identity()), "{name}");
            let mut photo = Document::new(String::new());
            photo.white_balance = Some(WhiteBalance { temperature: 4200.0, tint: 5.0 });
            photo.set_crop([0.1, 0.1, 0.8, 0.8], 0.0);
            let before = serde_json::to_string(&photo).unwrap();
            photo.copy_from(&look.document, look.parts);
            assert_ne!(serde_json::to_string(&photo).unwrap(), before, "{name} did nothing");
            assert_eq!(photo.white_balance.map(|wb| wb.temperature), Some(4200.0), "{name}");
            assert_eq!(photo.crop(), Some(([0.1, 0.1, 0.8, 0.8], 0.0)), "{name}");
        }
    }

    #[test]
    fn a_preset_at_a_strength_is_that_far_from_the_base() {
        use numa_core::grading::Grading;
        let mut base = Document::new(String::new());
        base.set_basic(Basic::with(|b| {
            b.tone.exposure = 1.0;
            b.tone.contrast = 10.0;
        }));
        let mut grade = Grading::default();
        grade.shadows.hue = 350.0;
        grade.shadows.saturation = 20.0;
        base.set_grading(grade);

        let mut document = Document::new(String::new());
        document.set_basic(Basic::with(|b| {
            b.tone.exposure = -1.0;
            b.tone.contrast = 30.0;
            b.presence.saturation = -40.0;
        }));
        let mut grade = Grading::default();
        grade.shadows.hue = 10.0;
        grade.shadows.saturation = 40.0;
        grade.highlights.hue = 200.0;
        grade.highlights.saturation = 30.0;
        document.set_grading(grade);
        document.set_curve(Curve::new([[0.0, 0.1], [0.5, 0.4], [1.0, 1.0]]));
        document.colour_profile = Some("Film".into());
        let preset = Preset { parts: EditParts { tone: true, colour: true, curve: true, ..EditParts::nothing() }, document };

        let json = |d: &Document| serde_json::to_string(d).unwrap();
        assert_eq!(json(&at_strength(&base, &preset, 0.0)), json(&base));
        let mut full = base.clone();
        full.copy_from(&preset.document, preset.parts);
        assert_eq!(json(&at_strength(&base, &preset, 1.0)), json(&full));

        let half = at_strength(&base, &preset, 0.5);
        assert_eq!(half.basic().tone.exposure, 0.0);
        assert_eq!(half.basic().tone.contrast, 20.0);
        assert_eq!(half.basic().presence.saturation, -20.0);
        assert_eq!(half.colour_profile.as_deref(), Some("Film"), "a switch, from halfway");
        assert_eq!(at_strength(&base, &preset, 0.4).colour_profile, None);
        let grade = half.grading();
        assert!(grade.shadows.hue.abs() < 1e-3 || (grade.shadows.hue - 360.0).abs() < 1e-3, "{}", grade.shadows.hue);
        assert_eq!(grade.shadows.saturation, 30.0);
        assert_eq!((grade.highlights.hue, grade.highlights.saturation), (200.0, 15.0), "no colour there to come from");
        let points: Vec<f32> = half.curve().points().iter().map(|p| p[1]).collect();
        assert!(points.iter().zip([0.05, 0.45, 1.0]).all(|(y, want)| (y - want).abs() < 1e-5), "{points:?}");
    }

    #[test]
    fn a_presets_white_balance_is_for_raws() {
        use numa_core::color::WhiteBalance;
        let mut document = Document::new(String::new());
        document.white_balance = Some(WhiteBalance { temperature: 3200.0, tint: 0.0 });
        let preset = Preset { parts: EditParts { white_balance: true, ..EditParts::nothing() }, document };
        let raw = at_strength(&Document::new("frame.RAF".into()), &preset, 1.0);
        assert_eq!(raw.white_balance.map(|wb| wb.temperature), Some(3200.0));
        let jpeg = at_strength(&Document::new("frame.jpg".into()), &preset, 1.0);
        assert_eq!(jpeg.white_balance, None);
    }

    #[cfg(numa_looks)]
    #[test]
    fn a_look_at_half_renders_between() {
        use numa_core::image::LinearImage;
        let (_, json) = LOOKS.iter().find(|(name, _)| *name == "Moody").unwrap();
        let look: Preset = serde_json::from_str(json).unwrap();
        let data: Vec<f32> = (0..16 * 16 * 3).map(|i| (i % 97) as f32 / 97.0).collect();
        let image = LinearImage::new(16, 16, data);
        let base = Document::new(String::new());
        let render = |amount: f32| numa_render::apply_stack(&at_strength(&base, &look, amount), &image, 1.0).into_raw();
        let (none, half, full) = (render(0.0), render(0.5), render(1.0));
        let distance = |a: &[u8], b: &[u8]| a.iter().zip(b).map(|(a, b)| (*a as i64 - *b as i64).abs()).sum::<i64>();
        assert!(distance(&none, &half) > 0 && distance(&half, &full) > 0);
        assert!(distance(&none, &half) < distance(&none, &full));
        assert!(distance(&half, &full) < distance(&none, &full));
    }
}
