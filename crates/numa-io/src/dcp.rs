use std::io::Cursor;
use std::path::{Path, PathBuf};

use rawler::formats::tiff::reader::TiffReader;
use rawler::formats::tiff::{Entry, GenericTiffReader};
use rawler::tags::DngTag;

use numa_core::profile::{DngProfile, HsvTable, Matrix3, ValueEncoding};

const DCP_MAGIC: u16 = 0x4352;
const TIFF_MAGIC: u16 = 42;

const SCENE_SIGNATURE: &str = "numa.photo scene-referred";

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
        scene_referred: false,
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

    let scene_referred = get(DngTag::ProfileCalibrationSignature)
        .and_then(|entry| entry.value.as_string().cloned())
        .is_some_and(|signature| signature == SCENE_SIGNATURE);
    let table = |dims, first, second, encoding| {
        table(dims, first, second, encoding).map(|table| HsvTable { scene_referred, ..table })
    };

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
    if profile.hue_sat_map.iter().chain(&profile.look_table).any(|table| table.scene_referred) {
        push(DngTag::ProfileCalibrationSignature, ASCII, ascii(SCENE_SIGNATURE));
    }
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
    paths.extend(numa_profiles_dir());

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
    choose(&ranked(make, model), automatic()).cloned()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Automatic {

    #[default]
    Numa,

    Standard,

    Matrix,
}

static AUTOMATIC: std::sync::atomic::AtomicU8 = std::sync::atomic::AtomicU8::new(Automatic::Numa as u8);

pub fn set_automatic(choice: Automatic) {
    AUTOMATIC.store(choice as u8, std::sync::atomic::Ordering::Relaxed);
}

pub fn automatic() -> Automatic {
    match AUTOMATIC.load(std::sync::atomic::Ordering::Relaxed) {
        1 => Automatic::Standard,
        2 => Automatic::Matrix,
        _ => Automatic::Numa,
    }
}

fn choose(ranked: &[(Option<u8>, DngProfile, Source)], automatic: Automatic) -> Option<&DngProfile> {
    let standard = || ranked.iter().find(|(rank, ..)| rank.is_some());
    let numa = || ranked.iter().find(|(.., source)| *source == Source::Numa);
    let fitted = || ranked.iter().find(|(_, profile, source)| *source == Source::Yours && fitted_by_numa(profile));
    match automatic {
        Automatic::Numa => fitted().or_else(numa).or_else(standard),
        Automatic::Standard => standard(),
        Automatic::Matrix => None,
    }
    .map(|(_, profile, _)| profile)
}

fn fitted_by_numa(profile: &DngProfile) -> bool {
    profile.hue_sat_map.as_ref().is_some_and(|table| table.scene_referred)
}

type Scans = std::sync::Mutex<std::collections::HashMap<String, Vec<(Option<u8>, DngProfile, Source)>>>;
type Names = std::sync::Mutex<std::collections::HashMap<String, Vec<(String, Source)>>>;
type ByName = std::sync::Mutex<std::collections::HashMap<String, Option<std::sync::Arc<DngProfile>>>>;
static SCANS: std::sync::OnceLock<Scans> = std::sync::OnceLock::new();
static NAMES: std::sync::OnceLock<Names> = std::sync::OnceLock::new();
static BY_NAME: std::sync::OnceLock<ByName> = std::sync::OnceLock::new();

static GENERATION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

pub fn generation() -> u64 {
    GENERATION.load(std::sync::atomic::Ordering::Relaxed)
}

pub fn forget() {
    GENERATION.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    if let Some(Ok(mut scans)) = SCANS.get().map(|c| c.lock()) {
        scans.clear();
    }
    if let Some(Ok(mut names)) = NAMES.get().map(|c| c.lock()) {
        names.clear();
    }
    if let Some(Ok(mut by_name)) = BY_NAME.get().map(|c| c.lock()) {
        by_name.clear();
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Source {

    Numa,

    RawTherapee,

    Yours,
}

fn standard_rank(profile: &DngProfile, source: Source, model: &str) -> Option<u8> {
    let yours = source == Source::Yours;
    if source == Source::Numa || (yours && fitted_by_numa(profile)) {
        return None;
    }
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
    ranked(make, model).into_iter().map(|(_, profile, _)| profile).collect()
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

fn ranked(make: &str, model: &str) -> Vec<(Option<u8>, DngProfile, Source)> {
    let cache = SCANS.get_or_init(Default::default);
    let Ok(mut cache) = cache.lock() else { return scan(make, model) };
    cache.entry(format!("{make}|{model}")).or_insert_with(|| scan(make, model)).clone()
}

fn scan(make: &str, model: &str) -> Vec<(Option<u8>, DngProfile, Source)> {
    let own = profiles_dir();
    let numa = numa_profiles_dir();
    let wanted = normalise(model);
    if wanted.is_empty() {
        return Vec::new();
    }
    let full = normalise(&format!("{make}{model}"));

    let mut found = Vec::new();
    for path in installed() {

        if !header_mentions(&path, |camera, _| camera.contains(&wanted), &wanted) {
            continue;
        }
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

        if found.iter().any(|(_, have, _): &(Option<u8>, DngProfile, Source)| have.name == profile.name) {
            continue;
        }
        if is_film_simulation(&profile.name) {
            continue;
        }
        if claimed == full || claimed == wanted || ends_with_model_of_make(&claimed, &wanted, make) {
            let yours = own.as_deref().is_some_and(|own| path.parent() == Some(own));
            let source = if yours {
                Source::Yours
            } else if numa.is_some() && path.parent() == numa.as_deref() {
                Source::Numa
            } else {
                Source::RawTherapee
            };
            found.push((standard_rank(&profile, source, model), profile, source));
        }
    }

    found.sort_by_key(|(rank, ..)| rank.unwrap_or(u8::MAX));
    found
}

pub fn by_name(name: &str) -> Option<std::sync::Arc<DngProfile>> {
    let mut cache = BY_NAME.get_or_init(Default::default).lock().ok()?;

    cache
        .entry(name.to_string())
        .or_insert_with(|| {
            installed()
                .into_iter()

                .filter(|path| header_mentions(path, |_, profile| profile.contains(name), name))
                .find_map(|path| read(&path).ok().filter(|profile| profile.name == name))
                .map(std::sync::Arc::new)
        })
        .clone()
}

pub fn names_for_camera(make: &str, model: &str) -> Vec<(String, Source)> {
    let Ok(mut cache) = NAMES.get_or_init(Default::default).lock() else { return Vec::new() };

    cache
        .entry(format!("{make}|{model}"))
        .or_insert_with(|| ranked(make, model).into_iter().map(|(_, p, source)| (p.name, source)).collect())
        .clone()
}

pub fn profiles_dir() -> Option<PathBuf> {
    Some(numa_core::paths::data_dir().join("profiles"))
}

pub fn downloaded_profiles_dir() -> PathBuf {
    numa_core::paths::data_dir().join("rawtherapee-dcpprofiles")
}

pub fn numa_profiles_dir() -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(dir) = std::env::current_exe().ok().and_then(|exe| exe.parent().map(Path::to_path_buf)) {
        candidates.push(dir.join("NumaProfiles"));
        candidates.push(dir.join("../Resources/NumaProfiles"));
        candidates.push(dir.join("../share").join(crate::NAME).join("profiles").join("numa"));
    }
    candidates.push(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/private-profiles"));
    candidates.into_iter().find(|dir| dir.is_dir())
}

pub fn is_film_simulation(name: &str) -> bool {
    const FILMS: &[&str] = &[
        "provia", "velvia", "astia", "classicchrome", "classicneg", "eterna", "acros", "proneg",
        "nostalgicneg", "realaace", "bleachbypass", "cameramonochrome", "camerasepia",
    ];
    let name = normalise(name);
    FILMS.iter().any(|film| name.contains(film))
}

fn header_mentions(path: &Path, test: impl Fn(&str, &str) -> bool, wanted: &str) -> bool {
    let stem = path.file_stem().map(|stem| stem.to_string_lossy().to_string()).unwrap_or_default();
    if stem.contains(wanted) || normalise(&stem).contains(wanted) {
        return true;
    }
    match header_tags(path) {
        Some((camera, name)) => test(&normalise(&camera), &name),
        None => true,
    }
}

fn header_tags(path: &Path) -> Option<(String, String)> {
    use std::io::{Read, Seek, SeekFrom};
    let mut file = std::fs::File::open(path).ok()?;
    let mut head = [0u8; 8];
    file.read_exact(&mut head).ok()?;
    let little = match &head[..2] {
        b"II" => true,
        b"MM" => false,
        _ => return None,
    };
    let u16_of = |b: &[u8]| if little { u16::from_le_bytes([b[0], b[1]]) } else { u16::from_be_bytes([b[0], b[1]]) };
    let u32_of = |b: &[u8]| {
        let b = [b[0], b[1], b[2], b[3]];
        if little { u32::from_le_bytes(b) } else { u32::from_be_bytes(b) }
    };
    file.seek(SeekFrom::Start(u32_of(&head[4..]) as u64)).ok()?;
    let mut count = [0u8; 2];
    file.read_exact(&mut count).ok()?;
    let mut entries = vec![0u8; u16_of(&count) as usize * 12];
    file.read_exact(&mut entries).ok()?;

    let (mut camera, mut name) = (String::new(), String::new());
    for entry in entries.chunks_exact(12) {
        let text = match u16_of(entry) {
            50708 => &mut camera,
            50936 => &mut name,
            _ => continue,
        };
        let length = u32_of(&entry[4..]) as usize;
        let bytes = if length <= 4 {
            entry[8..8 + length].to_vec()
        } else if length > 1 << 16 {
            return None;
        } else {
            let mut bytes = vec![0u8; length];
            file.seek(SeekFrom::Start(u32_of(&entry[8..]) as u64)).ok()?;
            file.read_exact(&mut bytes).ok()?;
            bytes
        };
        text.push_str(&String::from_utf8_lossy(&bytes));
    }
    Some((camera, name))
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
    fn the_header_names_what_the_file_does() {
        let installed = installed();
        for path in &installed {
            let Ok(profile) = read(path) else { continue };
            let (camera, name) = header_tags(path).unwrap();
            assert_eq!(normalise(&camera), normalise(profile.camera.as_deref().unwrap_or("")), "{}", path.display());
            if !name.is_empty() {
                assert!(name.starts_with(&profile.name), "{}", path.display());
            }
        }
        assert!(installed.is_empty() || installed.iter().any(|path| header_tags(path).is_some()));
    }

    #[test]
    fn a_short_model_name_does_not_take_another_makes_profile() {
        assert!(ends_with_model_of_make("fujifilmxt5", "xt5", "FUJIFILM"));
        assert!(ends_with_model_of_make("olympusem10", "em10", "OLYMPUS CORPORATION"));
        assert!(ends_with_model_of_make("nikonz6", "z6", "NIKON CORPORATION"));
        assert!(!ends_with_model_of_make("olympusem10", "m10", "Leica Camera AG"));
        assert!(!ends_with_model_of_make("fujifilmxt50", "xt5", "FUJIFILM"));
    }

    #[test]
    fn film_simulations_are_not_camera_profiles() {
        for name in ["Camera CLASSIC CHROME", "Camera PROVIA/Standard", "Camera Velvia/Vivid", "Camera PRO Neg. Hi",
            "Numa X-T5 Eterna", "Numa X-T5 Classic Chrome", "Camera ACROS+R FILTER", "Camera MONOCHROME"]
        {
            assert!(is_film_simulation(name), "{name}");
        }
        for name in ["Adobe Standard", "FUJIFILM X-T4", "Numa X-T4", "X-Mod", "Camera Standard", "Adobe Monochrome"] {
            assert!(!is_film_simulation(name), "{name}");
        }
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

        assert_eq!(standard_rank(&profile("Adobe Standard", "Fujifilm X-T5"), Source::Yours, "X-T5"), Some(0));
        assert_eq!(standard_rank(&profile("FUJIFILM X-T4", "FUJIFILM X-T4"), Source::Yours, "X-T4"), Some(1));
        let aliases = "Canon EOS 100D/Canon EOS Rebel SL1/Canon EOS Kiss X7";
        assert_eq!(standard_rank(&profile(aliases, aliases), Source::Yours, "Canon EOS 100D"), Some(1));

        assert_eq!(standard_rank(&profile("Camera CLASSIC CHROME", "Fujifilm X-T5"), Source::Yours, "X-T5"), None);
        assert_eq!(standard_rank(&profile("Chrome", "Fujifilm X-T5"), Source::Yours, "X-T5"), None);
        assert_eq!(standard_rank(&profile("My portrait profile", "Fujifilm X-T5"), Source::Yours, "X-T5"), None);

        assert_eq!(standard_rank(&profile("X-Mod", "Fujifilm X-E2"), Source::RawTherapee, "X-E2"), Some(1));

        assert_eq!(standard_rank(&profile("Numa EOS R5", "Canon EOS R5"), Source::Numa, "EOS R5"), None);
        assert_eq!(standard_rank(&profile("Canon EOS R5", "Canon EOS R5"), Source::Numa, "EOS R5"), None);
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
    fn numas_own_are_offered_as_numas_and_automatic_by_default() {
        if numa_profiles_dir().is_none() {
            eprintln!("no private profiles in this tree; skipping");
            return;
        }
        let names = names_for_camera("Canon", "Canon EOS R");
        assert!(names.contains(&("Numa EOS R".to_string(), Source::Numa)), "{names:?}");
        let ranked = ranked("Canon", "Canon EOS R");
        let name = |automatic| choose(&ranked, automatic).map(|profile| profile.name.as_str());
        assert_eq!(name(Automatic::default()), Some("Numa EOS R"));
        assert_ne!(name(Automatic::Standard), Some("Numa EOS R"));
        assert_eq!(name(Automatic::Matrix), None);
    }

    #[test]
    fn automatic_falls_back_to_the_standard_profile() {
        let profile = |name: &str| DngProfile {
            name: name.into(),
            camera: None,
            color_matrix: [None, None],
            forward_matrix: [None, None],
            illuminant: [None, None],
            hue_sat_map: None,
            look_table: None,
            tone_curve: None,
        };
        let rawtherapee = (Some(1), profile("FUJIFILM X-T4"), Source::RawTherapee);
        let look = (None, profile("My portrait profile"), Source::Yours);
        let numa = (None, profile("Numa X-T4"), Source::Numa);
        let name = |ranked: &[_], automatic| choose(ranked, automatic).map(|profile| profile.name.clone());

        let both = [rawtherapee.clone(), look.clone(), numa];
        assert_eq!(name(&both, Automatic::Numa).as_deref(), Some("Numa X-T4"));
        assert_eq!(name(&both, Automatic::Standard).as_deref(), Some("FUJIFILM X-T4"));
        assert_eq!(name(&both, Automatic::Matrix), None);

        assert_eq!(name(&[rawtherapee, look.clone()], Automatic::Numa).as_deref(), Some("FUJIFILM X-T4"));
        assert_eq!(name(&[look], Automatic::Numa), None, "a look is chosen, never assumed");
    }

    #[test]
    fn automatic_takes_the_photographers_own_fit_first() {
        let profile = |name: &str, table| DngProfile {
            name: name.into(),
            camera: None,
            color_matrix: [None, None],
            forward_matrix: [None, None],
            illuminant: [None, None],
            hue_sat_map: table,
            look_table: None,
            tone_curve: None,
        };
        let table = numa_core::profile::fit_hue_sat_map(&[], [6, 2, 1], 1.0);
        let rawtherapee = (Some(1), profile("FUJIFILM X-T4", None), Source::RawTherapee);
        let numa = (None, profile("Numa X-T4", Some(table.clone())), Source::Numa);
        let fitted = (None, profile("Your X-T4", Some(table.clone())), Source::Yours);
        let all = [rawtherapee, numa, fitted];
        let name = |automatic| choose(&all, automatic).map(|profile| profile.name.clone());
        assert_eq!(name(Automatic::Numa).as_deref(), Some("Your X-T4"));
        assert_eq!(name(Automatic::Standard).as_deref(), Some("FUJIFILM X-T4"));
        assert_eq!(name(Automatic::Matrix), None);

        assert_eq!(standard_rank(&profile("Your X-T4", Some(table)), Source::Yours, "X-T4"), None);
    }

    #[test]
    fn finds_only_an_exact_match() {
        if sample().is_none() {
            eprintln!("no system DCP profiles; skipping");
            return;
        }

        let exact = find("FUJIFILM", "X-T4").expect("X-T4 ships with RawTherapee");
        assert!(exact.camera.as_deref().is_some_and(|camera| camera.eq_ignore_ascii_case("FUJIFILM X-T4")), "{:?}", exact.camera);

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
                scene_referred: true,
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
        assert!(map.scene_referred, "Numa's own fit has to come back as Numa's");
        assert_eq!((map.hue_divisions, map.sat_divisions, map.val_divisions), (4, 3, 2));
        assert_eq!(map.value_encoding, ValueEncoding::Srgb);
        assert_eq!(map.entries, entries);
        assert!(map.second.is_none() && back.look_table.is_none());
    }

    #[test]
    fn rejects_files_that_are_not_profiles() {
        let path = std::env::temp_dir().join(format!("numa-not-a-{}.dcp", std::process::id()));
        std::fs::write(&path, b"this is not a TIFF").unwrap();
        assert!(read(&path).is_err());
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    #[ignore]
    fn cold_lookup() {
        println!("{} profiles installed", installed().len());
        for (make, model) in [("Fujifilm", "X-T30"), ("Sony", "ILCE-7M4"), ("Canon", "EOS R6"), ("Nikon", "Z 6_2"), ("Panasonic", "DC-G9")] {
            let start = std::time::Instant::now();
            let found = for_camera(make, model).len();
            println!("{make} {model}: {found} profiles in {:.1} ms", start.elapsed().as_secs_f64() * 1e3);
        }
    }
}
