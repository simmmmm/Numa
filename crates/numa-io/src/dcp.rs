use std::io::Cursor;
use std::path::{Path, PathBuf};

use rawler::formats::tiff::reader::TiffReader;
use rawler::formats::tiff::{Entry, GenericTiffReader};
use rawler::tags::DngTag;

use numa_core::profile::{DngProfile, HsvTable, Matrix3, ValueEncoding};

const DCP_MAGIC: u16 = 0x4352;
const TIFF_MAGIC: u16 = 42;

fn matrix(entry: Option<&Entry>) -> Option<Matrix3> {
    let entry = entry?;
    if entry.count() < 9 {
        return None;
    }
    let mut out = [[0.0f32; 3]; 3];
    for row in 0..3 {
        for column in 0..3 {
            out[row][column] = entry.value.force_f32(row * 3 + column);
        }
    }
    Some(out)
}

fn triples(entry: Option<&Entry>, expected: usize) -> Option<Vec<[f32; 3]>> {
    let entry = entry?;
    if entry.count() as usize != expected * 3 {
        return None;
    }
    Some(
        (0..expected)
            .map(|index| {
                [
                    entry.value.force_f32(index * 3),
                    entry.value.force_f32(index * 3 + 1),
                    entry.value.force_f32(index * 3 + 2),
                ]
            })
            .collect(),
    )
}

fn table(
    dims: Option<&Entry>,
    first: Option<&Entry>,
    second: Option<&Entry>,
    encoding: Option<&Entry>,
) -> Option<HsvTable> {
    let dims = dims?;
    if dims.count() < 3 {
        return None;
    }

    let mut table = HsvTable {

        value_encoding: match encoding.map(|entry| entry.force_u32(0)) {
            Some(1) => ValueEncoding::Srgb,
            _ => ValueEncoding::Linear,
        },
        hue_divisions: dims.value.force_usize(0),
        sat_divisions: dims.value.force_usize(1),
        val_divisions: dims.value.force_usize(2),
        entries: Vec::new(),
        second: None,
    };

    if table.hue_divisions == 0 || table.sat_divisions == 0 || table.val_divisions == 0 {
        return None;
    }

    let expected = table.len();
    table.entries = triples(first, expected)?;
    table.second = triples(second, expected);
    Some(table)
}

pub fn read(path: &Path) -> Result<DngProfile, String> {
    let fail = |err: String| format!("{}: {}", path.display(), err);

    let mut bytes = std::fs::read(path).map_err(|err| fail(err.to_string()))?;
    if bytes.len() < 8 {
        return Err(fail("too short to be a profile".to_string()));
    }

    let little_endian = &bytes[0..2] == b"II";
    let magic = if little_endian {
        u16::from_le_bytes([bytes[2], bytes[3]])
    } else {
        u16::from_be_bytes([bytes[2], bytes[3]])
    };

    if magic == DCP_MAGIC {
        let patched = if little_endian {
            TIFF_MAGIC.to_le_bytes()
        } else {
            TIFF_MAGIC.to_be_bytes()
        };
        bytes[2..4].copy_from_slice(&patched);
    }

    let tiff = GenericTiffReader::new(&mut Cursor::new(&bytes), 0, 0, None, &[])
        .map_err(|err| fail(format!("not a readable DCP: {err}")))?;
    let ifd = tiff.root_ifd();

    let get = |tag: DngTag| ifd.get_entry(tag);

    let tone_curve = get(DngTag::ProfileToneCurve).map(|entry| {
        (0..entry.count() as usize / 2)
            .map(|index| {
                (
                    entry.value.force_f32(index * 2),
                    entry.value.force_f32(index * 2 + 1),
                )
            })
            .collect::<Vec<_>>()
    });

    let profile = DngProfile {
        camera: get(DngTag::UniqueCameraModel).and_then(|entry| entry.value.as_string().cloned()),
        name: get(DngTag::ProfileName)
            .and_then(|entry| entry.value.as_string().cloned())
            .unwrap_or_else(|| {
                path.file_stem()
                    .map_or_else(|| "profile".into(), |stem| stem.to_string_lossy().to_string())
            }),
        color_matrix: [
            matrix(get(DngTag::ColorMatrix1)),
            matrix(get(DngTag::ColorMatrix2)),
        ],
        forward_matrix: [
            matrix(get(DngTag::ForwardMatrix1)),
            matrix(get(DngTag::ForwardMatrix2)),
        ],
        illuminant: [
            get(DngTag::CalibrationIlluminant1).map(|entry| entry.force_u16(0)),
            get(DngTag::CalibrationIlluminant2).map(|entry| entry.force_u16(0)),
        ],
        hue_sat_map: table(
            get(DngTag::ProfileHueSatMapDims),
            get(DngTag::ProfileHueSatMapData1),
            get(DngTag::ProfileHueSatMapData2),
            get(DngTag::ProfileHueSatMapEncoding),
        ),
        look_table: table(
            get(DngTag::ProfileLookTableDims),
            get(DngTag::ProfileLookTableData),
            None,
            get(DngTag::ProfileLookTableEncoding),
        ),
        tone_curve,
    };

    if profile.color_matrix[0].is_none() && profile.forward_matrix[0].is_none() {
        return Err(fail("no colour or forward matrix".to_string()));
    }

    Ok(profile)
}

pub fn write(path: &Path, profile: &DngProfile) -> std::io::Result<()> {
    const ASCII: u16 = 2;
    const SHORT: u16 = 3;
    const LONG: u16 = 4;
    const SRATIONAL: u16 = 10;
    const FLOAT: u16 = 11;

    let ascii = |text: &str| {
        let mut bytes = text.as_bytes().to_vec();
        bytes.push(0);
        (bytes.len() as u32, bytes)
    };
    let floats = |values: &mut dyn Iterator<Item = f32>| {
        let bytes: Vec<u8> = values.flat_map(f32::to_le_bytes).collect();
        ((bytes.len() / 4) as u32, bytes)
    };
    let matrix = |m: &Matrix3| {
        let bytes: Vec<u8> = m
            .iter()
            .flatten()
            .flat_map(|v| [((v * 10_000.0).round() as i32).to_le_bytes(), 10_000i32.to_le_bytes()])
            .flatten()
            .collect();
        (9, bytes)
    };
    let longs = |values: &[u32]| (values.len() as u32, values.iter().flat_map(|v| v.to_le_bytes()).collect());
    let encoding = |table: &HsvTable| match table.value_encoding {
        ValueEncoding::Linear => 0,
        ValueEncoding::Srgb => 1,
    };
    let triples = |entries: &[[f32; 3]]| floats(&mut entries.iter().flatten().copied());
    let dims = |t: &HsvTable| longs(&[t.hue_divisions as u32, t.sat_divisions as u32, t.val_divisions as u32]);

    let mut tags: Vec<(u16, u16, (u32, Vec<u8>))> = Vec::new();
    let mut push = |tag: DngTag, kind: u16, value: (u32, Vec<u8>)| tags.push((tag as u16, kind, value));

    if let Some(camera) = &profile.camera {
        push(DngTag::UniqueCameraModel, ASCII, ascii(camera));
    }
    push(DngTag::ProfileName, ASCII, ascii(&profile.name));
    let calibrations = [
        (DngTag::ColorMatrix1, DngTag::ForwardMatrix1, DngTag::CalibrationIlluminant1),
        (DngTag::ColorMatrix2, DngTag::ForwardMatrix2, DngTag::CalibrationIlluminant2),
    ];
    for (index, (colour, forward, illuminant)) in calibrations.into_iter().enumerate() {
        if let Some(m) = &profile.color_matrix[index] {
            push(colour, SRATIONAL, matrix(m));
        }
        if let Some(m) = &profile.forward_matrix[index] {
            push(forward, SRATIONAL, matrix(m));
        }
        if let Some(code) = profile.illuminant[index] {
            push(illuminant, SHORT, (1, code.to_le_bytes().to_vec()));
        }
    }
    if let Some(table) = &profile.hue_sat_map {
        push(DngTag::ProfileHueSatMapDims, LONG, dims(table));
        push(DngTag::ProfileHueSatMapData1, FLOAT, triples(&table.entries));
        if let Some(second) = &table.second {
            push(DngTag::ProfileHueSatMapData2, FLOAT, triples(second));
        }
        push(DngTag::ProfileHueSatMapEncoding, LONG, longs(&[encoding(table)]));
    }
    if let Some(table) = &profile.look_table {
        push(DngTag::ProfileLookTableDims, LONG, dims(table));
        push(DngTag::ProfileLookTableData, FLOAT, triples(&table.entries));
        push(DngTag::ProfileLookTableEncoding, LONG, longs(&[encoding(table)]));
    }
    if let Some(curve) = &profile.tone_curve {
        push(DngTag::ProfileToneCurve, FLOAT, floats(&mut curve.iter().flat_map(|(a, b)| [*a, *b])));
    }

    tags.sort_by_key(|(tag, ..)| *tag);

    let ifd_len = 2 + tags.len() * 12 + 4;
    let mut out = Vec::new();
    out.extend_from_slice(b"II");
    out.extend_from_slice(&DCP_MAGIC.to_le_bytes());
    out.extend_from_slice(&8u32.to_le_bytes());
    out.extend_from_slice(&(tags.len() as u16).to_le_bytes());

    let mut data = Vec::new();
    let data_start = 8 + ifd_len;
    for (tag, kind, (count, bytes)) in &tags {
        out.extend_from_slice(&tag.to_le_bytes());
        out.extend_from_slice(&kind.to_le_bytes());
        out.extend_from_slice(&count.to_le_bytes());
        if bytes.len() <= 4 {
            let mut inline = [0u8; 4];
            inline[..bytes.len()].copy_from_slice(bytes);
            out.extend_from_slice(&inline);
        } else {
            out.extend_from_slice(&((data_start + data.len()) as u32).to_le_bytes());
            data.extend_from_slice(bytes);

            if data.len() % 2 == 1 {
                data.push(0);
            }
        }
    }
    out.extend_from_slice(&0u32.to_le_bytes());
    out.extend_from_slice(&data);
    std::fs::write(path, out)
}

pub fn search_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    paths.extend(profiles_dir());
    paths.push(downloaded_profiles_dir());

    if let Ok(exe) = std::env::current_exe() {
        if let Some(prefix) = exe.parent().and_then(|bin| bin.parent()) {
            paths.push(prefix.join("share").join(crate::NAME).join("profiles"));
        }
    }

    paths.push(PathBuf::from("/usr/share/rawtherapee/dcpprofiles"));
    paths
}

fn normalise(text: &str) -> String {
    text.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

pub fn find(make: &str, model: &str) -> Option<DngProfile> {
    ranked(make, model).into_iter().find_map(|(rank, profile)| rank.map(|_| profile))
}

fn standard_rank(profile: &DngProfile, yours: bool, model: &str) -> Option<u8> {
    if profile.name.eq_ignore_ascii_case("Adobe Standard") {
        return Some(0);
    }
    if !yours {
        return Some(1);
    }
    let (name, model) = (normalise(&profile.name), normalise(model));
    let named_as_recorded = profile.camera.as_deref().is_some_and(|camera| normalise(camera) == name);
    (named_as_recorded || (!model.is_empty() && name.ends_with(&model))).then_some(1)
}

pub fn for_camera(make: &str, model: &str) -> Vec<DngProfile> {
    ranked(make, model).into_iter().map(|(_, profile)| profile).collect()
}

fn ends_with_model_of_make(claimed: &str, wanted: &str, make: &str) -> bool {
    let Some(prefix) = claimed.strip_suffix(wanted) else { return false };
    let make = normalise(make);
    let first_word = make_first_word(&make);
    prefix.is_empty() || make.starts_with(prefix) || prefix.starts_with(first_word)
}

fn make_first_word(make: &str) -> &str {
    ["corporation", "cameraag", "imagingcorp", "company", "corp", "gmbh", "co"]
        .iter()
        .find_map(|suffix| make.strip_suffix(suffix).filter(|rest| !rest.is_empty()))
        .unwrap_or(make)
}

fn ranked(make: &str, model: &str) -> Vec<(Option<u8>, DngProfile)> {
    let own = profiles_dir();
    let wanted = normalise(model);
    if wanted.is_empty() {
        return Vec::new();
    }
    let full = normalise(&format!("{make}{model}"));

    let mut found = Vec::new();
    for path in installed() {
        let Ok(profile) = read(&path) else { continue };

        let claimed = profile
            .camera
            .as_deref()
            .map(normalise)
            .unwrap_or_else(|| {
                path.file_stem()
                    .map(|stem| normalise(&stem.to_string_lossy()))
                    .unwrap_or_default()
            });

        if found.iter().any(|(_, have): &(Option<u8>, DngProfile)| have.name == profile.name) {
            continue;
        }
        if claimed == full || claimed == wanted || ends_with_model_of_make(&claimed, &wanted, make) {
            let yours = own.as_deref().is_some_and(|own| path.parent() == Some(own));
            found.push((standard_rank(&profile, yours, model), profile));
        }
    }

    found.sort_by_key(|(rank, _)| rank.unwrap_or(u8::MAX));
    found
}

pub fn by_name(name: &str) -> Option<std::sync::Arc<DngProfile>> {
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex, OnceLock};

    static CACHE: OnceLock<Mutex<HashMap<String, Option<Arc<DngProfile>>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let mut cache = cache.lock().ok()?;

    cache
        .entry(name.to_string())
        .or_insert_with(|| {
            installed()
                .into_iter()
                .find_map(|path| read(&path).ok().filter(|profile| profile.name == name))
                .map(Arc::new)
        })
        .clone()
}

pub fn names_for_camera(make: &str, model: &str) -> Vec<String> {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};

    static CACHE: OnceLock<Mutex<HashMap<String, Vec<String>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));
    let Ok(mut cache) = cache.lock() else { return Vec::new() };

    cache
        .entry(format!("{make}|{model}"))
        .or_insert_with(|| for_camera(make, model).into_iter().map(|p| p.name).collect())
        .clone()
}

pub fn profiles_dir() -> Option<PathBuf> {
    Some(numa_core::paths::data_dir().join("profiles"))
}

pub fn downloaded_profiles_dir() -> PathBuf {
    numa_core::paths::data_dir().join("rawtherapee-dcpprofiles")
}

fn installed() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    for directory in search_paths() {
        let Ok(entries) = std::fs::read_dir(&directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("dcp")) {
                paths.push(path);
            }
        }
    }
    paths
}

#[cfg(test)]
mod tests {

    #[test]
    fn a_short_model_name_does_not_take_another_makes_profile() {
        assert!(ends_with_model_of_make("fujifilmxt5", "xt5", "FUJIFILM"));
        assert!(ends_with_model_of_make("olympusem10", "em10", "OLYMPUS CORPORATION"));
        assert!(ends_with_model_of_make("nikonz6", "z6", "NIKON CORPORATION"));
        assert!(!ends_with_model_of_make("olympusem10", "m10", "Leica Camera AG"));
        assert!(!ends_with_model_of_make("fujifilmxt50", "xt5", "FUJIFILM"));
    }

    #[test]
    fn automatic_takes_only_a_standard_profile() {
        let profile = |name: &str, camera: &str| DngProfile {
            name: name.into(),
            camera: Some(camera.into()),
            color_matrix: [None, None],
            forward_matrix: [None, None],
            illuminant: [None, None],
            hue_sat_map: None,
            look_table: None,
            tone_curve: None,
        };

        assert_eq!(standard_rank(&profile("Adobe Standard", "Fujifilm X-T5"), true, "X-T5"), Some(0));
        assert_eq!(standard_rank(&profile("FUJIFILM X-T4", "FUJIFILM X-T4"), true, "X-T4"), Some(1));
        let aliases = "Canon EOS 100D/Canon EOS Rebel SL1/Canon EOS Kiss X7";
        assert_eq!(standard_rank(&profile(aliases, aliases), true, "Canon EOS 100D"), Some(1));

        assert_eq!(standard_rank(&profile("Camera CLASSIC CHROME", "Fujifilm X-T5"), true, "X-T5"), None);
        assert_eq!(standard_rank(&profile("Chrome", "Fujifilm X-T5"), true, "X-T5"), None);
        assert_eq!(standard_rank(&profile("My portrait profile", "Fujifilm X-T5"), true, "X-T5"), None);

        assert_eq!(standard_rank(&profile("X-Mod", "Fujifilm X-E2"), false, "X-E2"), Some(1));
    }

    use super::*;

    fn sample() -> Option<PathBuf> {
        let path = PathBuf::from("/usr/share/rawtherapee/dcpprofiles/FUJIFILM X-T4.dcp");
        path.exists().then_some(path)
    }

    #[test]
    fn reads_a_real_dual_illuminant_profile() {
        let Some(path) = sample() else {
            eprintln!("no system DCP profiles; skipping");
            return;
        };

        let profile = read(&path).unwrap();
        assert_eq!(profile.name, "FUJIFILM X-T4");
        assert_eq!(profile.camera.as_deref(), Some("FUJIFILM X-T4"));

        assert!(profile.color_matrix[0].is_some() && profile.color_matrix[1].is_some());
        assert!(profile.forward_matrix[0].is_some() && profile.forward_matrix[1].is_some());
        assert_eq!(
            profile.illuminant,
            [Some(17), Some(23)],
            "Standard Light A and D50"
        );

        let map = profile.hue_sat_map.expect("the profile carries a hue/sat map");
        assert_eq!(
            (map.hue_divisions, map.sat_divisions, map.val_divisions),
            (90, 30, 1)
        );
        assert_eq!(map.entries.len(), 90 * 30);
        assert_eq!(map.second.as_ref().map(Vec::len), Some(90 * 30));

        let forward = profile.forward_matrix[1].unwrap();
        let white: Vec<f32> = forward.iter().map(|row| row.iter().sum()).collect();
        let d50 = [0.9642, 1.0, 0.8249];
        for channel in 0..3 {
            assert!(
                (white[channel] - d50[channel]).abs() < 0.01,
                "forward matrix rows sum to {white:?}, not D50 {d50:?}"
            );
        }

        for value_index in 0..map.val_divisions {
            for hue_index in 0..map.hue_divisions {
                let index = (value_index * map.hue_divisions + hue_index) * map.sat_divisions;
                assert_eq!(map.entries[index][2], 1.0, "neutral axis must be left alone");
            }
        }
    }

    #[test]
    fn finds_only_an_exact_match() {
        if sample().is_none() {
            eprintln!("no system DCP profiles; skipping");
            return;
        }

        let exact = find("FUJIFILM", "X-T4").expect("X-T4 ships with RawTherapee");
        assert_eq!(exact.camera.as_deref(), Some("FUJIFILM X-T4"));

        for (make, model) in [("FUJIFILM", "X-T50"), ("FUJIFILM", "X-T400")] {
            if let Some(found) = find(make, model) {
                let camera = found.camera.unwrap_or_default().to_lowercase();
                assert!(
                    camera.replace(['-', ' '], "").ends_with(&model.replace('-', "").to_lowercase()),
                    "{model} matched a profile for {camera}"
                );
            }
        }

        assert!(find("Nonesuch", "Model Q").is_none());
        assert!(find("FUJIFILM", "").is_none(), "an empty model matches nothing");
    }

    #[test]
    fn a_written_profile_reads_back() {
        let entries: Vec<[f32; 3]> = (0..4 * 3 * 2).map(|i| [i as f32 * 0.5 - 3.0, 1.0 + i as f32 * 0.01, 0.9]).collect();
        let profile = DngProfile {
            name: "Numa X-T5 Test".into(),
            camera: Some("Fujifilm X-T5".into()),
            color_matrix: [Some([[0.5, -0.25, 0.1], [0.0, 1.0, 0.0], [-0.1, 0.2, 0.7]]), None],
            forward_matrix: [Some([[0.4361, 0.3851, 0.1431], [0.2225, 0.7169, 0.0606], [0.0139, 0.0971, 0.7142]]), None],
            illuminant: [Some(21), None],
            hue_sat_map: Some(HsvTable {
                value_encoding: ValueEncoding::Srgb,
                hue_divisions: 4,
                sat_divisions: 3,
                val_divisions: 2,
                entries: entries.clone(),
                second: None,
            }),
            look_table: None,
            tone_curve: None,
        };

        let path = std::env::temp_dir().join(format!("numa-roundtrip-{}.dcp", std::process::id()));
        write(&path, &profile).unwrap();
        let back = read(&path);
        std::fs::remove_file(&path).unwrap();
        let back = back.unwrap();

        assert_eq!(back.name, profile.name);
        assert_eq!(back.camera, profile.camera);
        assert_eq!(back.illuminant, [Some(21), None]);
        assert_eq!(back.forward_matrix[1], None);
        for (a, b) in back.color_matrix[0].unwrap().iter().flatten().zip(profile.color_matrix[0].unwrap().iter().flatten()) {
            assert!((a - b).abs() < 1e-4, "{a} vs {b}");
        }
        for (a, b) in back.forward_matrix[0].unwrap().iter().flatten().zip(profile.forward_matrix[0].unwrap().iter().flatten()) {
            assert!((a - b).abs() < 1e-4, "{a} vs {b}");
        }
        let map = back.hue_sat_map.unwrap();
        assert_eq!((map.hue_divisions, map.sat_divisions, map.val_divisions), (4, 3, 2));
        assert_eq!(map.value_encoding, ValueEncoding::Srgb);
        assert_eq!(map.entries, entries);
        assert!(map.second.is_none() && back.look_table.is_none());
    }

    #[test]
    fn rejects_files_that_are_not_profiles() {
        let path = std::env::temp_dir().join("numa-not-a.dcp");
        std::fs::write(&path, b"this is not a TIFF").unwrap();
        assert!(read(&path).is_err());
        std::fs::remove_file(&path).unwrap();
    }
}
