use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::Json;
use numa_cull::Scale;
use numa_io::catalog::{library_of, Library, Photo};
use numa_io::tidy::{self as shared, Card, Ledger};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::auth::Signed;
use crate::manage::{may_manage, within};
use crate::store::Account;
use crate::{Fail, Server};

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
    let everyone = Account { name: String::new(), admin: true, manage: true, must_change: false };
    let mut counts = HashMap::new();
    for card in cards(server, &everyone, None, None) {
        *counts.entry(card.kind).or_default() += 1;
    }
    counts
}

fn copies(server: &Server, hash: bool) -> (Vec<Vec<i64>>, usize) {
    let photos = server.home.all_libraries().into_iter().flat_map(|library| server.home.photos(&[library.id]));
    let mut known = server.tidy.hashes.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let known = known.get_or_insert_with(|| server.store.hashes());
    shared::copies(photos, known, |photo, size| {
        if !hash || server.activity.paused() {
            return shared::Hash::Later;
        }
        let made = shared::hash_file(photo);
        if let shared::Hash::Made(made) = &made {
            server.store.note_hash(&photo.path.to_string_lossy(), photo.mtime, size, made);
        }
        made
    })
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

pub fn scale(server: &Server, library: i64) -> Scale {
    shared::scale(&server.home.files(library))
}

struct Shelved<'a>(&'a Server);

impl shared::Shelf for Shelved<'_> {
    fn library(&self, id: i64) -> Option<Library> {
        self.0.home.library(id)
    }
    fn photos(&self, library: &Library) -> Vec<Photo> {
        self.0.home.photos(&[library.id])
    }
    fn scale(&self, library: &Library) -> Scale {
        scale(self.0, library.id)
    }
    fn archive(&self, photo: &Photo) -> bool {
        photo.path.starts_with(&self.0.home.photos)
    }
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

fn sets(server: &Server, me: &Account, copies: &[Vec<i64>], mine: Option<&HashSet<i64>>, only: Option<i64>) -> Vec<Card> {
    let ours: Vec<Library> = server.home.libraries(&me.name).into_iter().filter(|library| may_manage(server, me, library)).collect();
    let copies: Vec<Vec<Photo>> = copies.iter().map(|group| group.iter().filter_map(|id| server.home.photo(&me.name, *id).ok()).collect()).collect();
    shared::cards(&Shelved(server), &ours, &copies, &server.store.settled(), mine, only)
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
    let waits = shared::per_library(
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
        let going = shared::going(&card, &kept.keep);
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
