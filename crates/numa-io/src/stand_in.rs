use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use numa_core::document::{Document, Operation};
use rayon::prelude::*;

use crate::capture::{self, Capture};
use crate::raw;

#[derive(Debug, Clone, PartialEq)]
pub struct StandIn {

    pub raw: usize,
    pub jpeg: PathBuf,

    pub model: String,
}

pub fn same_frame(raw: &Capture, jpeg: &Capture) -> bool {
    let agree = |one: &str, other: &str| one.is_empty() || other.is_empty() || one == other;
    raw.taken.is_some() && raw.taken == jpeg.taken && agree(&raw.serial, &jpeg.serial) && agree(&raw.software, &jpeg.software)
}

pub fn find(raws: &[(usize, PathBuf)], library: &[PathBuf]) -> Vec<StandIn> {
    let stem = |path: &Path| path.file_stem().map(|stem| stem.to_string_lossy().to_lowercase());

    let paired: HashSet<(Option<&Path>, String)> =
        library.iter().filter(|path| raw::is_raw(path)).filter_map(|path| Some((path.parent(), stem(path)?))).collect();
    let mut jpegs: HashMap<String, Vec<&PathBuf>> = HashMap::new();
    for path in library {
        let Some(name) = stem(path) else { continue };
        if is_picture(path) && !paired.contains(&(path.parent(), name.clone())) {
            jpegs.entry(name).or_default().push(path);
        }
    }
    let mut found: Vec<StandIn> = raws
        .par_iter()
        .filter_map(|(at, path)| {
            let candidates = jpegs.get(&stem(path)?)?;
            let raw = capture::read(path);
            let jpeg = candidates.iter().find(|jpeg| same_frame(&raw, &capture::read(jpeg)))?;
            Some(StandIn { raw: *at, jpeg: (*jpeg).clone(), model: raw.model })
        })
        .collect();

    let mut used = HashSet::new();
    found.retain(|stand_in| used.insert(stand_in.jpeg.clone()));
    found
}

fn is_picture(path: &Path) -> bool {
    let extension = path.extension().map(|extension| extension.to_string_lossy().to_lowercase());
    matches!(extension.as_deref(), Some("jpg" | "jpeg" | "heic" | "heif" | "hif"))
}

pub fn carry(edit: &Document, raw: &Path) -> (Option<Document>, bool) {
    let mut carried = Document::new(raw.to_string_lossy().into_owned());
    carried.operations = edit.operations.iter().filter(|operation| matches!(operation, Operation::Crop { .. } | Operation::Rotate { .. })).cloned().collect();
    let behind = carried.operations.len() < edit.operations.len()
        || edit.white_balance.is_some()
        || edit.film_simulation.is_some()
        || edit.colour_profile.is_some()
        || edit.lut.is_some()
        || edit.ai_denoise > 0.0
        || edit.ai_sharpen > 0.0;
    ((!carried.operations.is_empty()).then_some(carried), behind)
}

#[cfg(test)]
mod tests {
    use super::*;
    use numa_core::document::Basic;

    fn at(taken: i64, serial: &str, software: &str) -> Capture {
        Capture { taken: Some(taken), serial: serial.into(), software: software.into(), ..Capture::default() }
    }

    #[test]
    fn the_same_frame_is_the_same_moment_body_and_firmware() {
        let raw = at(1000, "2D005233", "Digital Camera X-T5 Ver4.31");
        assert!(same_frame(&raw, &at(1000, "2D005233", "Digital Camera X-T5 Ver4.31")));

        assert!(same_frame(&raw, &at(1000, "", "")));

        assert!(!same_frame(&raw, &at(1001, "2D005233", "")));
        assert!(!same_frame(&raw, &at(1000, "9X000001", "")));
        assert!(!same_frame(&raw, &at(1000, "", "Snapseed 2.0")));

        assert!(!same_frame(&Capture::default(), &Capture::default()));
    }

    #[test]
    fn a_stand_in_is_found_by_name_then_by_its_head() {
        let dir = std::env::temp_dir().join(format!("numa-stand-in-{}", std::process::id()));
        let (card, phone, pair) = (dir.join("card"), dir.join("phone"), dir.join("pair"));
        for folder in [&card, &phone, &pair] {
            std::fs::create_dir_all(folder).unwrap();
        }
        let head = |date: &str| {
            let fields = [crate::capture::tests::ascii(::exif::Tag::DateTimeOriginal, date)];
            let mut bytes = vec![0xFF, 0xD8, 0xFF, 0xE1, 0, 0];
            bytes.extend(b"Exif\0\0");
            bytes.extend(crate::capture::tests::tiff(&fields));
            bytes
        };

        std::fs::write(card.join("DSCF0001.RAF"), head("2026:10:04 10:00:01")).unwrap();
        std::fs::write(card.join("DSCF0002.RAF"), head("2026:10:04 10:00:02")).unwrap();
        std::fs::write(card.join("DSCF0003.RAF"), head("2026:10:04 10:00:03")).unwrap();
        std::fs::write(phone.join("dscf0001.jpg"), head("2026:10:04 10:00:01")).unwrap();

        std::fs::write(phone.join("DSCF0002.JPG"), head("2025:01:01 09:00:00")).unwrap();
        std::fs::write(phone.join("DSCF0003 edited.JPG"), head("2026:10:04 10:00:03")).unwrap();
        std::fs::write(pair.join("DSCF0003.JPG"), head("2026:10:04 10:00:03")).unwrap();
        std::fs::write(pair.join("DSCF0003.RAF"), head("2026:10:04 10:00:03")).unwrap();
        let library: Vec<PathBuf> = [phone.join("dscf0001.jpg"), phone.join("DSCF0002.JPG"), phone.join("DSCF0003 edited.JPG"), pair.join("DSCF0003.JPG"), pair.join("DSCF0003.RAF")].into();
        let raws: Vec<(usize, PathBuf)> = (1..=3).map(|n| (n - 1, card.join(format!("DSCF000{n}.RAF")))).collect();
        let found = find(&raws, &library);
        std::fs::remove_dir_all(&dir).unwrap();
        assert_eq!(found.len(), 1, "{found:?}");
        assert_eq!((found[0].raw, found[0].jpeg.clone()), (0, phone.join("dscf0001.jpg")));
    }

    #[test]
    #[ignore]
    fn measure_stand_ins() {
        let (Some(raws), Some(jpegs)) = (std::env::var_os("NUMA_RAWS"), std::env::var_os("NUMA_JPEGS")) else {
            println!("NUMA_RAWS and NUMA_JPEGS are not set");
            return;
        };
        let list = |dir: &std::ffi::OsStr| -> Vec<PathBuf> {
            let mut paths: Vec<PathBuf> = std::fs::read_dir(dir).unwrap().flatten().map(|entry| entry.path()).filter(|path| path.is_file()).collect();
            paths.sort();
            paths
        };
        let raws: Vec<(usize, PathBuf)> = list(&raws).into_iter().filter(|path| raw::is_raw(path)).enumerate().collect();
        let library = list(&jpegs);
        let started = std::time::Instant::now();
        let found = find(&raws, &library);
        println!("{} raws, {} files in the library: {} stand-ins found in {:.2?}", raws.len(), library.len(), found.len(), started.elapsed());
        let started = std::time::Instant::now();
        let read: Vec<Capture> = raws.par_iter().map(|(_, path)| capture::read(path)).collect();
        let zoned = read.iter().filter(|capture| capture.zone.is_some()).count();
        let placed = read.iter().filter(|capture| capture.placed).count();
        println!("heads of {} raws read in {:.2?}: {zoned} with a zone, {placed} placed by the camera", read.len(), started.elapsed());
    }

    #[test]
    fn only_the_shape_travels_to_the_raw() {
        let mut edit = Document::new("DSCF0001.JPG".into());
        edit.operations.push(Operation::Basic(Basic::default()));
        edit.set_crop([0.1, 0.1, 0.8, 0.8], 2.0);
        edit.operations.push(Operation::Rotate { degrees: 90.0, mirrored: false });
        let (carried, behind) = carry(&edit, Path::new("/card/DSCF0001.RAF"));
        let carried = carried.expect("the shape");
        assert!(behind);
        assert_eq!(carried.source.path, "/card/DSCF0001.RAF");
        assert_eq!(carried.crop(), edit.crop());
        assert_eq!(carried.operations.len(), 2);

        let mut tone = Document::new("x.jpg".into());
        tone.operations.push(Operation::Basic(Basic::default()));
        assert_eq!(carry(&tone, Path::new("x.raf")).0.map(|d| d.operations.len()), None);
    }
}
