use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::sync::{Arc, Mutex};

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::Json;
use numa_cull::Scale;
use numa_io::catalog::{library_of, Flag, Library, Photo};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::auth::Signed;
use crate::manage::{may_manage, within};
use crate::store::Account;
use crate::{Fail, Server};

const RUN: usize = 3;

const PER_CARD: usize = 6;

const SAMPLES: usize = 8;

#[derive(Default)]
pub struct Tidy {
    copies: Mutex<Option<Arc<(Vec<Vec<i64>>, usize)>>>,

    stale: std::sync::atomic::AtomicBool,
    hashes: Mutex<Option<HashMap<String, (i64, u64, String)>>>,
}

impl Tidy {

    pub fn stale(&self) {
        self.stale.store(true, std::sync::atomic::Ordering::Relaxed);
    }
}

pub fn prepare(server: &Server) -> HashMap<&'static str, usize> {
    let found = copies(server, true);
    *server.tidy.copies.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(Arc::new(found));
    server.tidy.stale.store(false, std::sync::atomic::Ordering::Relaxed);
    let everyone = Account { name: String::new(), admin: true, manage: true };
    let mut counts = HashMap::new();
    for card in cards(server, &everyone, None, None) {
        *counts.entry(card.kind).or_default() += 1;
    }
    counts
}

fn copies(server: &Server, hash: bool) -> (Vec<Vec<i64>>, usize) {
    let mut by_size: HashMap<u64, Vec<Photo>> = HashMap::new();
    for library in server.home.all_libraries() {
        for photo in server.home.photos(&[library.id]) {
            if let Ok(meta) = std::fs::metadata(&photo.path) {
                by_size.entry(meta.len()).or_default().push(photo);
            }
        }
    }
    let mut known = server.tidy.hashes.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let known = known.get_or_insert_with(|| server.store.hashes());
    let (mut groups, mut pending) = (HashMap::<String, Vec<i64>>::new(), 0);
    for (size, photos) in by_size.into_iter().filter(|(_, photos)| photos.len() > 1) {
        for photo in photos {
            let path = photo.path.to_string_lossy().into_owned();
            let cached = known.get(&path).filter(|(mtime, was, _)| *mtime == photo.mtime && *was == size).map(|(.., hash)| hash.clone());
            let hash = match cached {
                Some(hash) => hash,
                None if hash && !server.activity.paused() => match numa_io::offload::hash_file(&photo.path) {
                    Ok(made) => {
                        let made = numa_io::offload::hex(made);
                        server.store.note_hash(&path, photo.mtime, size, &made);
                        known.insert(path, (photo.mtime, size, made.clone()));
                        made
                    }
                    Err(err) => {
                        log::info!("tidy: {}: {err}", photo.path.display());
                        continue;
                    }
                },
                None => {
                    pending += 1;
                    continue;
                }
            };
            groups.entry(format!("{size}:{hash}")).or_default().push(photo.id);
        }
    }
    let mut groups: Vec<Vec<i64>> = groups.into_values().filter(|ids| ids.len() > 1).collect();
    groups.iter_mut().for_each(|ids| ids.sort_unstable());
    groups.sort_unstable();
    (groups, pending)
}

fn copies_now(server: &Server) -> Arc<(Vec<Vec<i64>>, usize)> {
    let found = server.tidy.copies.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone();
    if let Some(found) = found.filter(|_| !server.tidy.stale.load(std::sync::atomic::Ordering::Relaxed)) {
        return found;
    }
    server.tidy.stale.store(false, std::sync::atomic::Ordering::Relaxed);
    let found = Arc::new(copies(server, false));
    *server.tidy.copies.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(found.clone());
    found
}

pub struct Card {
    pub key: String,
    pub kind: &'static str,
    pub photos: Vec<Photo>,

    pub keep: Vec<i64>,

    pub why: String,

    settle: Vec<String>,

    notes: HashMap<i64, Vec<String>>,
}

pub fn scale(server: &Server, library: i64) -> Scale {
    Scale::of(server.home.files(library).iter().filter_map(|photo| photo.face_sharpness.or(photo.sharpness)))
}

fn sharp(photo: &Photo) -> f32 {
    photo.face_sharpness.or(photo.sharpness).unwrap_or(0.0)
}

fn best_frame(run: &[Photo]) -> Option<&Photo> {
    run.iter().max_by(|a, b| {
        let key = |photo: &Photo| (photo.eyes_closed != Some(true), photo.best_of_burst);
        key(a).cmp(&key(b)).then(a.suggested.unwrap_or(0.0).total_cmp(&b.suggested.unwrap_or(0.0))).then(sharp(a).total_cmp(&sharp(b)))
    })
}

fn burst_why(best: &Photo, run: &[Photo]) -> String {
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

fn best_copy<'a>(server: &Server, photos: &'a [Photo]) -> (&'a Photo, &'static str) {
    let archive = |photo: &Photo| photo.path.starts_with(&server.home.photos);
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

fn unusable(photo: &Photo, scale: &Scale) -> Option<String> {
    photo.sharpness?;
    if photo.rating > 0 || photo.flag == Flag::Picked || photo.edited {
        return None;
    }
    let frame = numa_io::notes::frame_of(photo);
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

pub fn cards(server: &Server, me: &Account, mine: Option<&HashSet<i64>>, only: Option<i64>) -> Vec<Card> {
    sets(server, me, &copies_now(server).0, mine, only)
}

pub fn waiting(server: &Server, me: &Account, mine: Option<&HashSet<i64>>) -> usize {
    let found = server.tidy.copies.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone().unwrap_or_default();
    sets(server, me, &found.0, mine, None).len()
}

pub async fn mine(server: &Arc<Server>, me: &Account, everyone: bool) -> Result<Option<HashSet<i64>>, Fail> {
    if everyone || server.store.accounts().map_err(Fail::internal)?.len() < 2 {
        return Ok(None);
    }
    let owners = crate::by::owners(server, &server.home.libraries(&me.name)).await?;
    Ok(Some(owners.into_iter().filter(|(_, owner)| owner.eq_ignore_ascii_case(&me.name)).map(|(photo, _)| photo).collect()))
}

fn own(photos: &[Photo], mine: Option<&HashSet<i64>>, least: usize) -> Option<Vec<Photo>> {
    let kept: Vec<Photo> = photos.iter().filter(|photo| mine.is_none_or(|mine| mine.contains(&photo.id))).cloned().collect();
    (kept.len() >= least).then_some(kept)
}

fn sets(server: &Server, me: &Account, copies: &[Vec<i64>], mine: Option<&HashSet<i64>>, only: Option<i64>) -> Vec<Card> {
    let settled = server.store.settled();
    let libraries: Vec<Library> = server.home.libraries(&me.name).into_iter().filter(|library| may_manage(server, me, library)).collect();
    let ours: HashSet<i64> = libraries.iter().map(|library| library.id).collect();
    let mut cards = Vec::new();

    for group in copies {
        let all: Vec<Photo> = group.iter().filter_map(|id| server.home.photo(&me.name, *id).ok()).collect();
        let Some(photos) = own(&all, mine, 2) else { continue };
        let here = |photo: &Photo| only.is_none_or(|only| library_of(photo.id) == only);
        if photos.iter().any(|photo| !ours.contains(&library_of(photo.id))) || !photos.iter().any(here) {
            continue;
        }
        let key = all
            .iter()
            .filter_map(|photo| Some(format!("copies:{}:{}", library_of(photo.id), within(&server.home.library(library_of(photo.id))?, &photo.path))))
            .min()
            .unwrap_or_default();
        if settled.contains(&key) {
            continue;
        }
        let (best, why) = best_copy(server, &photos);
        let keep = vec![best.id];
        cards.push(Card { key: key.clone(), kind: "copies", keep, why: why.into(), settle: vec![key], notes: HashMap::new(), photos });
    }

    for library in libraries.iter().filter(|library| only.is_none_or(|only| library.id == only)) {
        let scale = scale(server, library.id);
        let photos = server.home.photos(&[library.id]);
        let mut runs: HashMap<i64, Vec<Photo>> = HashMap::new();
        for photo in photos.iter().filter(|photo| photo.burst.is_some()) {
            runs.entry(photo.burst.unwrap_or_default()).or_default().push(photo.clone());
        }
        let mut in_run = HashSet::new();
        let mut runs: Vec<Vec<Photo>> = runs.into_values().filter(|run| run.len() >= RUN).collect();
        runs.sort_by_key(|run| std::cmp::Reverse(run.iter().map(crate::home::when).max()));
        for mut run in runs {
            run.sort_by_key(crate::home::when);
            in_run.extend(run.iter().map(|photo| photo.id));
            let key = format!("bursts:{}:{}", library.id, within(library, &run[0].path));
            if settled.contains(&key) {
                continue;
            }
            let Some(run) = own(&run, mine, RUN) else { continue };
            let Some(best) = best_frame(&run) else { continue };
            let (keep, why) = (vec![best.id], burst_why(best, &run));
            let notes = run.iter().map(|photo| (photo.id, numa_io::notes::issues(photo, &scale))).collect();
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

async fn nobodys(server: &Arc<Server>, me: &Account, only: Option<i64>, accounts: &[String]) -> Result<Vec<(i64, String, Vec<Photo>)>, Fail> {
    if accounts.len() < 2 {
        return Ok(Vec::new());
    }
    let settled = server.store.settled();
    let libraries: Vec<Library> = server.home.libraries(&me.name).into_iter().filter(|library| only.is_none_or(|only| library.id == only)).collect();
    let owners = crate::by::owners(server, &libraries).await?;
    let ids: Vec<i64> = libraries.iter().map(|library| library.id).collect();
    let bodies: HashMap<i64, String> =
        server.home.ask(move |catalog| ids.into_iter().filter_map(|id| catalog.bodies(id).ok()).flatten().collect()).await?;
    let mut by_camera: HashMap<(i64, String), Vec<Photo>> = HashMap::new();
    for library in &libraries {
        for photo in server.home.photos(&[library.id]) {
            if !owners.contains_key(&photo.id) {
                by_camera.entry((library.id, bodies.get(&photo.id).cloned().unwrap_or_default())).or_default().push(photo);
            }
        }
    }
    let mut found: Vec<(i64, String, Vec<Photo>)> =
        by_camera.into_iter().filter(|((_, camera), _)| !settled.contains(&format!("who:{camera}"))).map(|((library, camera), photos)| (library, camera, photos)).collect();
    found.sort_by(|a, b| b.2.len().cmp(&a.2.len()).then_with(|| (a.0, &a.1).cmp(&(b.0, &b.1))));
    Ok(found)
}

fn who(camera: &str, photos: &[Photo]) -> Value {
    let mut parts = camera.splitn(3, '|');
    let name = [parts.next().unwrap_or(""), parts.next().unwrap_or("")].join(" ").trim().to_string();
    let name = if name.is_empty() { "A camera that does not say its name".to_string() } else { name };
    let shown: Vec<Value> = photos.iter().take(SAMPLES).map(|photo| json!({ "id": photo.id, "aspect": photo.aspect, "notes": [] })).collect();
    json!({
        "key": format!("who:{camera}"), "kind": "who", "title": name, "why": format!("{} photograph{} by nobody known yet", photos.len(), if photos.len() == 1 { "" } else { "s" }),
        "photos": shown, "ids": photos.iter().map(|photo| photo.id).collect::<Vec<_>>(), "keep": [],
    })
}

#[derive(Default)]
struct Waits {
    photos: usize,
    sets: usize,
    covers: Vec<i64>,
}

fn per_library(sets: impl Iterator<Item = Vec<i64>>) -> BTreeMap<i64, Waits> {
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

fn place(server: &Server, photo: &Photo) -> String {
    match server.home.library(library_of(photo.id)) {
        Some(library) => format!("{}/{}", library.label(), within(&library, &photo.path)),
        None => photo.path.to_string_lossy().into_owned(),
    }
}

#[derive(Deserialize, Default)]
pub struct Asked {
    library: Option<i64>,

    everyone: Option<String>,
}

pub async fn list(State(server): State<Arc<Server>>, Signed(me): Signed, Query(asked): Query<Asked>) -> Result<Json<Value>, Fail> {
    let everyone = matches!(asked.everyone.as_deref(), Some("1" | "true"));
    let seen = server.home.libraries(&me.name);
    let only = match asked.library {
        Some(id) => Some(seen.iter().find(|library| library.id == id).cloned().ok_or_else(|| Fail(StatusCode::NOT_FOUND, "There is no such library.".into()))?),
        None => None,
    };
    let accounts: Vec<String> = server.store.accounts().map_err(Fail::internal)?.into_iter().map(|account| account.name).collect();
    let chosen = mine(&server, &me, everyone).await?;
    let only_id = only.as_ref().map(|library| library.id);
    let (working, asking) = (server.clone(), me.clone());
    let (cards, pending) = tokio::task::spawn_blocking(move || (cards(&working, &asking, chosen.as_ref(), only_id), copies_now(&working).1)).await.map_err(Fail::internal)?;
    let nobody = if everyone { nobodys(&server, &me, only_id, &accounts).await? } else { Vec::new() };
    let waits = per_library(
        cards.iter().map(|card| card.photos.iter().map(|photo| photo.id).collect::<Vec<i64>>()).chain(nobody.iter().map(|(_, _, photos)| photos.iter().take(SAMPLES).map(|photo| photo.id).collect::<Vec<i64>>())),
    );
    let (home, single) = (server.home.sent_folder(&me.name), accounts.len() < 2);

    let Some(library) = only else {
        let mut shown: Vec<Library> = seen.into_iter().filter(|library| waits.contains_key(&library.id)).collect();
        shown.sort_by_cached_key(|library| std::cmp::Reverse(server.home.newest(library.id)));
        let rows: Vec<Value> = shown
            .iter()
            .map(|library| {
                let waiting = &waits[&library.id];
                json!({ "id": library.id, "name": library.label(), "mine": library.path == home, "photos": waiting.photos, "sets": waiting.sets, "covers": waiting.covers })
            })
            .collect();
        return Ok(Json(json!({ "libraries": rows, "everyone": everyone, "single": single, "pending": pending, "accounts": accounts })));
    };

    let mut counts: HashMap<&str, usize> = HashMap::from([("copies", 0), ("bursts", 0), ("unusable", 0), ("who", nobody.len())]);
    let mut rows: Vec<Value> = cards
        .iter()
        .map(|card| {
            *counts.entry(card.kind).or_default() += 1;
            let photos: Vec<Value> = card
                .photos
                .iter()
                .map(|photo| {
                    json!({
                        "id": photo.id, "aspect": photo.aspect, "place": place(&server, photo), "suggested": photo.suggested,
                        "notes": card.notes.get(&photo.id).cloned().unwrap_or_default(),
                    })
                })
                .collect();
            json!({ "key": card.key, "kind": card.kind, "why": card.why, "keep": card.keep, "photos": photos })
        })
        .collect();
    rows.extend(nobody.iter().map(|(_, camera, photos)| who(camera, photos)));
    let may = may_manage(&server, &me, &library);
    let photos = waits.get(&library.id).map_or(0, |waiting| waiting.photos);
    let library = json!({ "id": library.id, "name": library.label(), "mine": library.path == home, "may": may, "photos": photos });
    Ok(Json(json!({ "library": library, "everyone": everyone, "single": single, "counts": counts, "cards": rows, "pending": pending, "accounts": accounts })))
}

#[derive(Deserialize)]
pub struct Kept {
    key: String,
    keep: Vec<i64>,

    library: Option<i64>,
    #[serde(default)]
    everyone: bool,
}

pub async fn keep(State(server): State<Arc<Server>>, Signed(me): Signed, Json(kept): Json<Kept>) -> Result<Json<Value>, Fail> {
    if kept.key.starts_with("who:") {
        server.store.settle(&[kept.key]);
        return Ok(Json(json!({ "trashed": 0 })));
    }
    let chosen = mine(&server, &me, kept.everyone).await?;
    let working = server.clone();
    let trashed = tokio::task::spawn_blocking(move || {
        let server = working;
        let card = cards(&server, &me, chosen.as_ref(), kept.library)
            .into_iter()
            .find(|card| card.key == kept.key)
            .ok_or_else(|| Fail(StatusCode::CONFLICT, "That set has changed since the page was made; it is shown again.".into()))?;
        let going: Vec<i64> = card.photos.iter().map(|photo| photo.id).filter(|id| !kept.keep.contains(id)).collect();
        let mut photos = Vec::new();
        for id in server.home.with_twins(&going) {
            let library = server.home.library(library_of(id)).ok_or_else(|| Fail::internal("a photograph without its library"))?;
            photos.push((library, server.home.photo(&me.name, id)?));
        }
        let trashed = crate::manage::trash(&server, photos);
        server.store.settle(&card.settle);
        server.tidy.stale();
        Ok::<_, Fail>(trashed)
    })
    .await
    .map_err(Fail::internal)??;
    Ok(Json(json!({ "trashed": trashed })))
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
}
