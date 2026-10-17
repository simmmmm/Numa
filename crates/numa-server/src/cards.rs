use std::path::{Path as FilePath, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use numa_io::catalog::{Library, Scan};
use numa_io::import::{self, Found, Place, Shoot};
use numa_io::offload::{self, Item, Stage, Target};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::auth::Signed;
use crate::home::when;
use crate::{Fail, Server};

pub struct Card {
    root: PathBuf,
    camera: Option<String>,
    found: Vec<Found>,

    new: Vec<bool>,

    shoots: Vec<Shoot>,
    places: Vec<Place>,
}

#[derive(Default)]
pub struct Cards {
    card: Mutex<Option<Card>>,

    done: AtomicUsize,
    total: AtomicUsize,
    importing: AtomicBool,

    pub stale: AtomicBool,

    said: Mutex<Option<(String, Vec<String>)>>,
}

fn cards_under(root: &FilePath) -> Vec<PathBuf> {
    let mut found = Vec::new();
    let mut pending = vec![(root.to_path_buf(), 0)];
    while let Some((dir, depth)) = pending.pop() {
        if dir.join("DCIM").is_dir() {
            found.push(dir);
            continue;
        }
        if depth < 2 {
            let children = std::fs::read_dir(&dir).map(|entries| entries.flatten().map(|entry| entry.path()).filter(|path| path.is_dir()).collect::<Vec<_>>());
            pending.extend(children.unwrap_or_default().into_iter().map(|child| (child, depth + 1)));
        }
    }
    found.sort();
    found
}

pub fn watch(server: Arc<Server>, root: PathBuf) {
    let _ = std::thread::Builder::new().name("numa-cards".into()).spawn(move || loop {
        let present = cards_under(&root).into_iter().next();
        let known = server.cards.card.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).as_ref().map(|card| card.root.clone());
        let stale = server.cards.stale.load(Ordering::Relaxed);
        if !server.cards.importing.load(Ordering::Relaxed) && (present != known || (stale && present.is_some())) && server.scanned.load(Ordering::Relaxed) {
            server.cards.stale.store(false, Ordering::Relaxed);
            let card = present.map(|root| read(&server, root));
            *server.cards.said.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
            *server.cards.card.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = card;
        }
        std::thread::sleep(Duration::from_secs(5));
    });
}

fn archive(server: &Server) -> Vec<Library> {
    server.home.all_libraries().into_iter().filter(|library| library.path.starts_with(&server.home.photos)).collect()
}

fn read(server: &Server, root: PathBuf) -> Card {
    let found = import::scan(&root);
    let libraries = archive(server);
    let held: Vec<PathBuf> = libraries.iter().flat_map(|library| server.home.files(library.id)).map(|photo| photo.path).collect();
    let already = import::already(&found, &import::by_name(held));
    let new: Vec<bool> = already.iter().map(Option::is_none).collect();
    let all: Vec<usize> = (0..found.len()).collect();
    let shoots = import::shoots(&found, &all);
    let spans: Vec<import::Span> = libraries
        .iter()
        .map(|library| {
            let times: Vec<i64> = server.home.files(library.id).iter().map(when).collect();
            (library.clone(), times.iter().min().copied(), times.iter().max().copied())
        })
        .collect();

    let places = shoots.iter().map(|shoot| inside_archive(server, import::suggest(shoot, &spans, 0), shoot)).collect();
    let camera = found.last().and_then(|photo| numa_io::raw::summary(&photo.path)).and_then(|summary| summary.camera);
    log::info!("card at {}: {} photographs, {} new, {} shoots", root.display(), found.len(), new.iter().filter(|new| **new).count(), shoots.len());
    Card { root, camera, found, new, shoots, places }
}

fn inside_archive(server: &Server, place: Place, shoot: &Shoot) -> Place {
    match place {
        Place::New(path) if !path.starts_with(&server.home.photos) => {
            let day = numa_io::import::day(shoot.first, 0);
            Place::New(server.home.photos.join(&day[..4]).join(day))
        }
        place => place,
    }
}

pub async fn status(State(server): State<Arc<Server>>, Signed(_): Signed) -> Json<Value> {
    let cards = &server.cards;
    let card = cards.card.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let Some(card) = card.as_ref() else { return Json(Value::Null) };
    let shoots: Vec<Value> = card
        .shoots
        .iter()
        .zip(&card.places)
        .map(|(shoot, place)| {
            let (library, folder) = match place {
                Place::Library(library) => (Some(library.id), library.path.clone()),
                Place::New(path) => (None, path.clone()),
            };
            let folder = folder.strip_prefix(&server.home.photos).unwrap_or(&folder).to_string_lossy().into_owned();
            let new = shoot.photos.iter().filter(|&&at| card.new[at]).count();
            json!({ "first": shoot.first, "last": shoot.last, "count": new, "photos": shoot.photos.len(), "library": library, "folder": folder })
        })
        .collect();
    let importing = cards.importing.load(Ordering::Relaxed).then(|| json!({ "done": cards.done.load(Ordering::Relaxed), "total": cards.total.load(Ordering::Relaxed) }));
    let said = cards.said.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone().map(|(said, problems)| json!({ "said": said, "problems": problems }));
    let new = card.new.iter().filter(|new| **new).count();
    let new_ones = || card.shoots.iter().filter(|shoot| shoot.photos.iter().any(|&at| card.new[at]));
    Json(json!({
        "card": card.root.file_name().map(|name| name.to_string_lossy()),
        "camera": card.camera,
        "photos": card.found.len(),
        "new": new,
        "first": new_ones().next().map(|shoot| shoot.first),
        "last": new_ones().next_back().map(|shoot| shoot.last),
        "shoots": shoots,
        "importing": importing,
        "done": said,
    }))
}

#[derive(Deserialize)]
pub struct Destination {
    library: Option<i64>,
    name: Option<String>,

    photos: Option<Vec<usize>>,
}

#[derive(Deserialize)]
pub struct Asked {
    shoots: Vec<Destination>,
}

pub async fn start(State(server): State<Arc<Server>>, Signed(_): Signed, Json(asked): Json<Asked>) -> Result<StatusCode, Fail> {
    let libraries = archive(&server);
    let (items, places) = {
        let card = server.cards.card.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let card = card.as_ref().ok_or_else(|| Fail(StatusCode::NOT_FOUND, "There is no card in the NAS.".into()))?;
        if asked.shoots.len() != card.shoots.len() {
            return Err(Fail::bad("Say where each shoot goes."));
        }
        let mut places = Vec::new();
        for ((destination, shoot), suggested) in asked.shoots.iter().zip(&card.shoots).zip(&card.places) {
            places.push(match destination.library {
                Some(id) => Place::Library(libraries.iter().find(|library| library.id == id).cloned().ok_or_else(|| Fail::bad("There is no such library."))?),
                None => Place::New(new_folder(&server, suggested, shoot, destination.name.as_deref())?),
            });
        }
        let ticked: Vec<Vec<usize>> = asked
            .shoots
            .iter()
            .zip(&card.shoots)
            .map(|(destination, shoot)| match &destination.photos {
                Some(ticked) => shoot.photos.iter().copied().filter(|at| ticked.contains(at)).collect(),
                None => shoot.photos.iter().copied().filter(|&at| card.new[at]).collect(),
            })
            .collect();
        let items = plan(card, &places, &ticked);
        if items.is_empty() {
            return Err(Fail::bad("Tick a photograph to import."));
        }
        (items, places)
    };
    if server.cards.importing.swap(true, Ordering::SeqCst) {
        return Err(Fail(StatusCode::CONFLICT, "An import is already under way.".into()));
    }
    server.cards.done.store(0, Ordering::Relaxed);
    server.cards.total.store(items.len(), Ordering::Relaxed);
    *server.cards.said.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
    let running = server.clone();
    std::thread::Builder::new()
        .name("numa-import".into())
        .spawn(move || {
            let said = import_now(&running, items, &places);
            *running.cards.said.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(said);

            let root = running.cards.card.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).as_ref().map(|card| card.root.clone());
            let card = root.map(|root| read(&running, root));
            *running.cards.card.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = card;
            running.cards.importing.store(false, Ordering::SeqCst);
            running.jobs.nudge();
        })
        .map_err(Fail::internal)?;
    Ok(StatusCode::ACCEPTED)
}

pub async fn photos(State(server): State<Arc<Server>>, Signed(_): Signed) -> Json<Value> {
    let card = server.cards.card.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let Some(card) = card.as_ref() else { return Json(Value::Null) };
    let shoots: Vec<Vec<Value>> = card
        .shoots
        .iter()
        .map(|shoot| {
            shoot
                .photos
                .iter()
                .map(|&at| {
                    let photo = &card.found[at];
                    let name = photo.path.file_name().map(|name| name.to_string_lossy().into_owned());
                    json!({ "n": at, "name": name, "taken": photo.when, "new": card.new[at], "raw": numa_io::raw::is_raw(&photo.path) })
                })
                .collect()
        })
        .collect();
    Json(json!({ "card": card.root.file_name().map(|name| name.to_string_lossy()), "shoots": shoots }))
}

pub async fn thumb(State(server): State<Arc<Server>>, Signed(_): Signed, axum::extract::Path(at): axum::extract::Path<usize>) -> Result<axum::response::Response, Fail> {
    let path = {
        let card = server.cards.card.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        card.as_ref().and_then(|card| card.found.get(at)).map(|photo| photo.path.clone())
    };
    let path = path.ok_or_else(|| Fail(StatusCode::NOT_FOUND, "There is no such photograph on the card.".into()))?;
    crate::manage::embedded(&server, path, 320).await
}

pub async fn names(State(server): State<Arc<Server>>, Signed(_): Signed) -> Result<Json<Vec<Value>>, Fail> {
    let shoots: Vec<Vec<PathBuf>> = {
        let card = server.cards.card.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let Some(card) = card.as_ref() else { return Ok(Json(Vec::new())) };
        card.shoots.iter().map(|shoot| shoot.photos.iter().map(|&at| card.found[at].path.clone()).collect()).collect()
    };

    let answer = tokio::task::spawn_blocking(move || {
        shoots
            .iter()
            .map(|paths| {
                let times: Vec<i64> = paths.iter().step_by((paths.len() / 120).max(1)).filter_map(|path| numa_io::exif::taken(path)).collect();
                crate::places::names_for(&server, paths, &times)
            })
            .collect()
    })
        .await
        .map_err(|err| Fail(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;
    Ok(Json(answer))
}

pub fn progress(server: &Server) -> Option<(usize, usize)> {
    let cards = &server.cards;
    cards.importing.load(Ordering::Relaxed).then(|| (cards.done.load(Ordering::Relaxed), cards.total.load(Ordering::Relaxed)))
}

fn new_folder(server: &Server, suggested: &Place, shoot: &Shoot, name: Option<&str>) -> Result<PathBuf, Fail> {
    let base = match suggested {
        Place::New(path) => path.clone(),
        Place::Library(_) => {
            let day = numa_io::import::day(shoot.first, 0);
            server.home.photos.join(&day[..4]).join(day)
        }
    };
    let Some(name) = name.map(str::trim).filter(|name| !name.is_empty()) else { return Ok(base) };
    if !crate::manage::plain(name) {
        return Err(Fail::bad("A folder's name is one plain name."));
    }
    Ok(base.parent().unwrap_or(&server.home.photos).join(name))
}

fn plan(card: &Card, places: &[Place], ticked: &[Vec<usize>]) -> Vec<Item> {
    let mut items = Vec::new();
    for (shoot, place) in ticked.iter().zip(places) {
        let folder = match place {
            Place::Library(library) => library.path.clone(),
            Place::New(path) => path.clone(),
        };
        for &at in shoot {
            let photo = &card.found[at];
            items.push(Item { card: photo.path.clone(), size: photo.size, to: Target::Free(folder.clone(), import::named("", photo, 0, 0)), second: None });
        }
    }
    items
}

fn import_now(server: &Arc<Server>, items: Vec<Item>, places: &[Place]) -> (String, Vec<String>) {
    let started = std::time::Instant::now();
    let total = items.len();
    let counted = server.clone();
    let step: offload::Step = Arc::new(move |stage: Stage| {
        if stage == Stage::Library {
            counted.cards.done.fetch_add(1, Ordering::Relaxed);
        }
    });
    let (report, later) = offload::run(items, true, Arc::new(AtomicBool::new(false)), step);

    let report = later.join(report);

    let roots: Vec<PathBuf> = places
        .iter()
        .filter(|place| match place {
            Place::Library(_) => true,
            Place::New(path) => path.is_dir(),
        })
        .map(|place| match place {
            Place::Library(library) => library.path.clone(),
            Place::New(path) => path.clone(),
        })
        .fold(Vec::new(), |mut roots, root| {
            if !roots.contains(&root) {
                roots.push(root);
            }
            roots
        });
    let ratings: Vec<(PathBuf, u8)> = report.landed.iter().filter_map(|landed| Some((landed.to.clone(), landed.rating?))).collect();
    let home = server.clone();
    let catalogued = server.home.blocking(move |catalog| {
        let mut rated = 0;
        for root in roots.iter().filter(|root| root.is_dir()) {
            let Ok(library) = catalog.add_library(root) else { continue };
            let known = catalog.known_files(&library).unwrap_or_default();
            let found: Scan = numa_io::catalog::scan(root, &known);
            let _ = catalog.apply_scan(&library, &found);

            for photo in catalog.photos(library.id, &Default::default()).unwrap_or_default() {
                if let Some((_, stars)) = ratings.iter().find(|(path, _)| *path == photo.path).filter(|_| photo.rating == 0) {
                    rated += usize::from(catalog.set_rating(photo.id, *stars).is_ok());
                }
            }
        }
        home.home.set_found(crate::manage::found(&home));
        home.shelve(None, catalog);
        rated
    });

    let stamp = numa_io::import::day(crate::store::now(), 0);
    let header = vec![
        format!("Numa Home import receipt · {stamp}"),
        format!("{} photographs · each copy read back and the same as the card", report.landed.len()),
    ];
    let files: Vec<numa_io::receipt::Entry> =
        report.landed.iter().map(|landed| (landed.hash, landed.to.parent().unwrap_or(&landed.to).to_path_buf(), landed.to.clone(), None)).collect();
    numa_io::receipt::write_all(&files, None, &stamp, &header);
    log::info!("import: {} of {total} photographs in {:.1} s, {} problems", report.landed.len(), started.elapsed().as_secs_f32(), report.problems.len());

    let problems: Vec<String> = report
        .problems
        .iter()
        .map(|problem| {
            let name = problem.card.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
            match &problem.fault {
                offload::Fault::Unreadable(_) => format!("{name}: could not be read — reshoot while you can"),
                offload::Fault::Differs => format!("{name}: the copy is not the same as the card"),
                offload::Fault::NotCopied(err) => format!("{name}: could not be copied: {err}"),
                other => format!("{name}: {other:?}"),
            }
        })
        .collect();
    let rated = catalogued.unwrap_or(0);
    let stars = if rated > 0 { format!(" {rated} kept the camera's stars.") } else { String::new() };
    let said = match problems.is_empty() {
        true => format!("Every photograph is on the NAS and checked; the card can be ejected.{stars}"),
        false => format!("{} of {total} photographs are on the NAS and checked. Keep the card: the others are not.", report.landed.len()),
    };
    (said, problems)
}
