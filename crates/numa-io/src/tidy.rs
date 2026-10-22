use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use numa_cull::Scale;

use crate::catalog::{library_of, Flag, Library, Photo};

pub const RUN: usize = 3;

pub const PER_CARD: usize = 6;

pub const KINDS: [(&str, &str, &str); 4] = [
    ("copies", "Exact Copies", "The same file more than once, byte for byte."),
    ("bursts", "The Same Frame", "Three frames or more of one moment."),
    ("unusable", "Unusable", "Blurred past use, nearly black or white, or blown out."),
    ("who", "Who Took These?", "Photographs nobody in the house is known to have taken."),
];

pub type Known = HashMap<String, (i64, u64, String)>;

pub trait Ledger {
    fn settled(&self) -> HashSet<String>;
    fn settle(&self, keys: &[String]);
    fn hashes(&self) -> Known;
    fn note_hash(&self, path: &str, mtime: i64, size: u64, hash: &str);
}

pub enum Hash {
    Made(String),

    Later,

    Failed,
}

pub fn hash_file(photo: &Photo) -> Hash {
    match crate::offload::hash_file(&photo.path) {
        Ok(made) => Hash::Made(crate::offload::hex(made)),
        Err(err) => {
            log::info!("tidy: {}: {err}", photo.path.display());
            Hash::Failed
        }
    }
}

pub fn copies(photos: impl IntoIterator<Item = Photo>, known: &mut Known, mut hash: impl FnMut(&Photo, u64) -> Hash) -> (Vec<Vec<i64>>, usize) {
    let mut by_size: HashMap<u64, Vec<Photo>> = HashMap::new();
    for photo in photos {
        if let Ok(meta) = std::fs::metadata(&photo.path) {
            by_size.entry(meta.len()).or_default().push(photo);
        }
    }
    let (mut groups, mut pending) = (HashMap::<String, Vec<i64>>::new(), 0);
    for (size, photos) in by_size.into_iter().filter(|(_, photos)| photos.len() > 1) {
        for photo in photos {
            let path = photo.path.to_string_lossy().into_owned();
            let cached = known.get(&path).filter(|(mtime, was, _)| *mtime == photo.mtime && *was == size).map(|(.., hash)| hash.clone());
            let hash = match cached {
                Some(hash) => hash,
                None => match hash(&photo, size) {
                    Hash::Made(made) => {
                        known.insert(path, (photo.mtime, size, made.clone()));
                        made
                    }
                    Hash::Failed => continue,
                    Hash::Later => {
                        pending += 1;
                        continue;
                    }
                },
            };
            groups.entry(format!("{size}:{hash}")).or_default().push(photo.id);
        }
    }
    let mut groups: Vec<Vec<i64>> = groups.into_values().filter(|ids| ids.len() > 1).collect();
    groups.iter_mut().for_each(|ids| ids.sort_unstable());
    groups.sort_unstable();
    (groups, pending)
}

pub struct Card {
    pub key: String,
    pub kind: &'static str,
    pub photos: Vec<Photo>,

    pub keep: Vec<i64>,

    pub why: String,

    pub settle: Vec<String>,

    pub notes: HashMap<i64, Vec<String>>,
}

pub fn cared(photo: &Photo) -> bool {
    photo.rating > 0 || photo.flag == Flag::Picked || photo.edited
}

pub fn going(card: &Card, keep: &[i64]) -> Vec<i64> {
    card.photos.iter().filter(|photo| !keep.contains(&photo.id) && !cared(photo)).map(|photo| photo.id).collect()
}

pub fn scale(photos: &[Photo]) -> Scale {
    Scale::of(photos.iter().filter_map(|photo| photo.face_sharpness.or(photo.sharpness)))
}

pub fn within(library: &Library, path: &std::path::Path) -> String {
    path.strip_prefix(&library.path).unwrap_or(path).to_string_lossy().into_owned()
}

fn when(photo: &Photo) -> i64 {
    photo.taken.unwrap_or(photo.mtime)
}

fn sharp(photo: &Photo) -> f32 {
    photo.face_sharpness.or(photo.sharpness).unwrap_or(0.0)
}

pub fn best_frame(run: &[Photo]) -> Option<&Photo> {
    run.iter().max_by(|a, b| {
        let key = |photo: &Photo| (photo.eyes_closed != Some(true), photo.best_of_burst);
        key(a).cmp(&key(b)).then(a.suggested.unwrap_or(0.0).total_cmp(&b.suggested.unwrap_or(0.0))).then(sharp(a).total_cmp(&sharp(b)))
    })
}

pub fn burst_why(best: &Photo, run: &[Photo]) -> String {
    let mut parts = Vec::new();
    if best.best_of_burst {
        parts.push(if best.face_sharpness.is_some() { "the sharpest face of the run" } else { "the sharpest of the run" }.to_string());
    }
    if best.eyes_closed != Some(true) && run.iter().any(|photo| photo.eyes_closed == Some(true)) {
        parts.push("eyes open".into());
    }
    if let Some(stars) = best.suggested {
        parts.push(format!("suggested {stars:.1}★"));
    }
    match parts.is_empty() {
        true => "Numa's pick by sharpness".into(),
        false => format!("Numa's pick: {}", parts.join(", ")),
    }
}

pub fn best_copy<'a>(photos: &'a [Photo], archive: impl Fn(&Photo) -> bool) -> (&'a Photo, &'static str) {
    let best = photos
        .iter()
        .max_by_key(|photo| (photo.edited, photo.rating, photo.flag == Flag::Picked, archive(photo), std::cmp::Reverse(photo.path.as_os_str().len()), std::cmp::Reverse(photo.id)))
        .expect("a copy");
    let why = if best.edited && photos.iter().any(|photo| !photo.edited) {
        "Keeps the one with edits"
    } else if best.rating > 0 && photos.iter().any(|photo| photo.rating < best.rating) {
        "Keeps the one with stars"
    } else if archive(best) && photos.iter().any(|photo| !archive(photo)) {
        "Keeps the one in the archive"
    } else {
        "The same file; keeps the first"
    };
    (best, why)
}

pub fn unusable(photo: &Photo, scale: &Scale) -> Option<String> {
    photo.sharpness?;
    if cared(photo) {
        return None;
    }
    let frame = crate::notes::frame_of(photo);
    if let Some(note) = frame.blank_note() {
        return Some(note.to_string());
    }
    if frame.blown_fraction() >= numa_cull::BLOWN_FULL {
        return Some(format!("blown out: {:.0} % of it white", frame.blown_fraction() * 100.0));
    }
    if sharp(photo) < scale.dull {
        let what = if photo.face_sharpness.is_some() { "the face out of focus" } else { "out of focus" };
        return Some(if frame.is_slow() { format!("{what}, slow shutter") } else { what.to_string() });
    }
    None
}

pub fn own(photos: &[Photo], mine: Option<&HashSet<i64>>, least: usize) -> Option<Vec<Photo>> {
    let kept: Vec<Photo> = photos.iter().filter(|photo| mine.is_none_or(|mine| mine.contains(&photo.id))).cloned().collect();
    (kept.len() >= least).then_some(kept)
}

pub trait Shelf {

    fn library(&self, id: i64) -> Option<Library>;

    fn photos(&self, library: &Library) -> Vec<Photo>;

    fn scale(&self, library: &Library) -> Scale;

    fn archive(&self, _photo: &Photo) -> bool {
        false
    }
}

fn keeping(best: i64, photos: &[Photo]) -> Vec<i64> {
    let mut keep = vec![best];
    keep.extend(photos.iter().filter(|photo| photo.id != best && cared(photo)).map(|photo| photo.id));
    keep
}

pub fn cards(shelf: &impl Shelf, ours: &[Library], copies: &[Vec<Photo>], settled: &HashSet<String>, mine: Option<&HashSet<i64>>, only: Option<i64>) -> Vec<Card> {
    let ids: HashSet<i64> = ours.iter().map(|library| library.id).collect();
    let mut cards = Vec::new();

    for all in copies {
        let Some(photos) = own(all, mine, 2) else { continue };
        let here = |photo: &Photo| only.is_none_or(|only| library_of(photo.id) == only);
        if photos.iter().any(|photo| !ids.contains(&library_of(photo.id))) || !photos.iter().any(here) {
            continue;
        }
        let key = all
            .iter()
            .filter_map(|photo| Some(format!("copies:{}:{}", library_of(photo.id), within(&shelf.library(library_of(photo.id))?, &photo.path))))
            .min()
            .unwrap_or_default();
        if settled.contains(&key) {
            continue;
        }
        let (best, why) = best_copy(&photos, |photo| shelf.archive(photo));
        let keep = keeping(best.id, &photos);
        cards.push(Card { key: key.clone(), kind: "copies", keep, why: why.into(), settle: vec![key], notes: HashMap::new(), photos });
    }

    for library in ours.iter().filter(|library| only.is_none_or(|only| library.id == only)) {
        let scale = shelf.scale(library);
        let photos = shelf.photos(library);
        let mut runs: HashMap<i64, Vec<Photo>> = HashMap::new();
        for photo in photos.iter().filter(|photo| photo.burst.is_some()) {
            runs.entry(photo.burst.unwrap_or_default()).or_default().push(photo.clone());
        }
        let mut in_run = HashSet::new();
        let mut runs: Vec<Vec<Photo>> = runs.into_values().filter(|run| run.len() >= RUN).collect();
        runs.sort_by_key(|run| std::cmp::Reverse(run.iter().map(when).max()));
        for mut run in runs {
            run.sort_by_key(when);
            in_run.extend(run.iter().map(|photo| photo.id));
            let key = format!("bursts:{}:{}", library.id, within(library, &run[0].path));
            if settled.contains(&key) {
                continue;
            }
            let Some(run) = own(&run, mine, RUN) else { continue };
            let Some(best) = best_frame(&run) else { continue };
            let (keep, why) = (keeping(best.id, &run), burst_why(best, &run));
            let notes = run.iter().map(|photo| (photo.id, crate::notes::issues(photo, &scale))).collect();
            cards.push(Card { key: key.clone(), kind: "bursts", keep, why, settle: vec![key], notes, photos: run });
        }
        let found: Vec<(Photo, String, String)> = photos
            .into_iter()
            .filter(|photo| !in_run.contains(&photo.id) && mine.is_none_or(|mine| mine.contains(&photo.id)))
            .filter_map(|photo| {
                let key = format!("unusable:{}:{}", library.id, within(library, &photo.path));
                let why = unusable(&photo, &scale).filter(|_| !settled.contains(&key))?;
                Some((photo, why, key))
            })
            .collect();
        for chunk in found.chunks(PER_CARD) {
            let key = format!("unusable:{}:{}", library.id, chunk[0].0.id);
            let reasons: HashSet<&str> = chunk.iter().map(|(_, why, _)| why.as_str()).collect();
            let why = match reasons.len() {
                1 => format!("Each is {}", chunk[0].1),
                _ => "Each is blurred, blank or blown out past use".to_string(),
            };
            cards.push(Card {
                key,
                kind: "unusable",
                keep: Vec::new(),
                why,
                settle: chunk.iter().map(|(.., key)| key.clone()).collect(),
                notes: chunk.iter().map(|(photo, why, _)| (photo.id, vec![why.clone()])).collect(),
                photos: chunk.iter().map(|(photo, ..)| photo.clone()).collect(),
            });
        }
    }
    cards
}

#[derive(Default)]
pub struct Waits {
    pub photos: usize,
    pub sets: usize,
    pub covers: Vec<i64>,
}

pub fn per_library(sets: impl Iterator<Item = Vec<i64>>) -> BTreeMap<i64, Waits> {
    let mut waits: BTreeMap<i64, Waits> = BTreeMap::new();
    let mut counted = HashSet::new();
    for ids in sets {
        for library in ids.iter().map(|id| library_of(*id)).collect::<BTreeSet<_>>() {
            waits.entry(library).or_default().sets += 1;
        }
        for id in ids {
            let waiting = waits.entry(library_of(id)).or_default();
            if counted.insert(id) {
                waiting.photos += 1;
            }
            if waiting.covers.len() < 3 && !waiting.covers.contains(&id) {
                waiting.covers.push(id);
            }
        }
    }
    waits
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn frame(id: i64, sharpness: f32, best: bool, eyes_closed: Option<bool>, suggested: f32) -> Photo {
        Photo {
            id, path: PathBuf::from(format!("/p/{id}.raf")), mtime: 0, taken: None, rating: 0, flag: Flag::None,
            sharpness: Some(sharpness), blown: None, best_of_burst: best, burst: None, echo: None, exposure: None, focal35: None,
            raw_clipped: None, raw_dark: None, eyes_closed, faces: None, face_sharpness: None, brightness: None, contrast: None,
            suggested: Some(suggested), edited: false, aspect: None,
        }
    }

    #[test]
    fn a_run_keeps_open_eyes_before_the_sharpest() {
        let run = [frame(1, 0.9, true, Some(true), 4.0), frame(2, 0.7, false, Some(false), 3.0), frame(3, 0.2, false, None, 1.0)];
        assert_eq!(best_frame(&run).map(|photo| photo.id), Some(2));
        let run = [frame(1, 0.9, true, None, 4.4), frame(2, 0.7, false, None, 3.0)];
        assert_eq!(best_frame(&run).map(|photo| photo.id), Some(1));
        assert_eq!(burst_why(&run[0], &run), "Numa's pick: the sharpest of the run, suggested 4.4★");
    }

    #[test]
    fn a_set_is_cut_down_to_the_accounts_own_and_needs_enough_of_them_left() {
        let run: Vec<Photo> = (1..=5).map(|id| frame(id, 0.5, false, None, 3.0)).collect();
        let ids = |kept: Option<Vec<Photo>>| kept.map(|photos| photos.iter().map(|photo| photo.id).collect::<Vec<_>>());
        let mine: HashSet<i64> = [1, 3, 4, 9].into();
        assert_eq!(ids(own(&run, Some(&mine), RUN)), Some(vec![1, 3, 4]));
        let few: HashSet<i64> = [1, 5].into();
        assert_eq!(ids(own(&run, Some(&few), RUN)), None, "two of five frames are not a run of one's own");
        assert_eq!(ids(own(&run, Some(&few), 2)), Some(vec![1, 5]), "but they are a pair");
        assert_eq!(ids(own(&run, None, RUN)), Some(vec![1, 2, 3, 4, 5]), "everyone's keeps them all");
        assert_eq!(ids(own(&run[..1], None, 2)), None, "a pair needs both");
    }

    #[test]
    fn a_library_counts_its_photographs_once_and_shows_three_of_them() {
        let at = |library: i64, row: i64| (library << 32) | row;

        let sets = vec![vec![at(1, 1), at(2, 1)], vec![at(2, 1), at(2, 2), at(2, 3), at(2, 4)], vec![at(1, 5)]];
        let waits = per_library(sets.into_iter());
        let (first, second) = (&waits[&1], &waits[&2]);
        assert_eq!((first.photos, first.sets), (2, 2));
        assert_eq!((second.photos, second.sets), (4, 2), "the copy that is also in the run is one photograph");
        assert_eq!(second.covers, [at(2, 1), at(2, 2), at(2, 3)]);
        assert_eq!(first.covers, [at(1, 1), at(1, 5)]);
        assert!(!waits.contains_key(&3), "a library with nothing waiting is not listed");
    }

    #[test]
    fn unusable_is_blank_blown_or_past_the_librarys_soft_end() {
        let scale = Scale { dull: 0.15, crisp: 1.15, soft: 0.25 };
        let mut photo = frame(1, 0.5, false, None, 3.0);
        assert_eq!(unusable(&photo, &scale), None);
        photo.sharpness = Some(0.1);
        assert_eq!(unusable(&photo, &scale).as_deref(), Some("out of focus"));
        photo.rating = 2;
        assert_eq!(unusable(&photo, &scale), None, "a frame with stars is someone's");
        photo.rating = 0;
        photo.contrast = Some(0.001);
        photo.brightness = Some(0.02);
        assert_eq!(unusable(&photo, &scale).as_deref(), Some("nearly black"));
        let mut blown = frame(2, 0.5, false, None, 3.0);
        blown.blown = Some(0.6);
        assert_eq!(unusable(&blown, &scale).as_deref(), Some("blown out: 60 % of it white"));
    }

    #[test]
    fn a_photograph_someone_cared_for_never_goes() {
        let mut run: Vec<Photo> = (1..=4).map(|id| frame(id, 0.5, false, None, 3.0)).collect();
        run[1].rating = 3;
        run[2].flag = Flag::Picked;
        run[3].edited = true;
        assert_eq!(keeping(1, &run), [1, 2, 3, 4], "Numa suggests keeping them");
        let card = Card { key: String::new(), kind: "bursts", photos: run, keep: Vec::new(), why: String::new(), settle: Vec::new(), notes: HashMap::new() };
        assert_eq!(going(&card, &[]), [1], "and Move to Trash on all of them keeps them");
    }

    #[test]
    fn copies_are_the_same_bytes_and_a_file_not_yet_hashed_waits() {
        let dir = std::env::temp_dir().join(format!("numa-tidy-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = |id: i64, bytes: &[u8]| {
            let path = dir.join(format!("{id}.jpg"));
            std::fs::write(&path, bytes).unwrap();
            Photo { path, ..frame(id, 0.5, false, None, 3.0) }
        };
        let photos = vec![file(1, b"abcd"), file(2, b"abcd"), file(3, b"abce"), file(4, b"lonely one")];
        let mut known = Known::new();
        let (groups, pending) = copies(photos.clone(), &mut known, |photo, _| hash_file(photo));
        assert_eq!((groups, pending), (vec![vec![1, 2]], 0));
        assert_eq!(known.len(), 3, "the file of its own size is never read");
        let (again, pending) = copies(photos.clone(), &mut known, |_, _| Hash::Later);
        assert_eq!((again, pending), (vec![vec![1, 2]], 0), "what is known is not asked again");
        let (none, pending) = copies(photos, &mut Known::new(), |_, _| Hash::Later);
        assert_eq!((none.len(), pending), (0, 3));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
