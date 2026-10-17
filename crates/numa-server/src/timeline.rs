use std::collections::{HashMap, HashSet};
use std::sync::{Arc, MutexGuard};

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use numa_io::catalog::{library_of, Filter, Library, Photo};
use numa_io::words::{self, Index};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::auth::Signed;
use crate::home::{month as month_of, when};
use crate::{Fail, Server};

#[derive(Deserialize, Default)]
pub struct Wanted {
    library: Option<i64>,
    person: Option<String>,
    album: Option<String>,
    q: Option<String>,

    thing: Option<String>,

    pub(crate) by: Option<String>,
}

fn given(text: &Option<String>) -> Option<String> {
    text.as_deref().map(str::trim).filter(|text| !text.is_empty()).map(str::to_string)
}

pub async fn libraries(State(server): State<Arc<Server>>, Signed(me): Signed) -> Json<Vec<Value>> {
    let mine = server.home.sent_folder(&me.name);
    let rows = server.home.libraries(&me.name).into_iter().map(|library| {
        json!({ "id": library.id, "name": library.label(), "count": server.home.count(library.id), "mine": library.path == mine })
    });
    Json(rows.collect())
}

pub async fn months(State(server): State<Arc<Server>>, Signed(me): Signed, Query(wanted): Query<Wanted>) -> Result<Json<Vec<Value>>, Fail> {
    let photos = chosen(&server, &me.name, &wanted).await?;

    type Month = (String, usize, Vec<(i64, usize)>);
    let mut months: Vec<Month> = Vec::new();
    for photo in &photos {
        let month = month_of(photo);
        if months.last().is_none_or(|(last, ..)| *last != month) {
            months.push((month, 0, Vec::new()));
        }
        let (_, count, libraries) = months.last_mut().expect("pushed above");
        *count += 1;
        let library = library_of(photo.id);
        match libraries.iter_mut().find(|(id, _)| *id == library) {
            Some((_, n)) => *n += 1,
            None => libraries.push((library, 1)),
        }
    }
    let rows = months.into_iter().map(|(month, count, mut libraries)| {
        libraries.sort_by_key(|library| std::cmp::Reverse(library.1));
        let libraries: Vec<Value> = libraries.into_iter().map(|(id, count)| json!({ "id": id, "count": count })).collect();
        json!({ "month": month, "count": count, "libraries": libraries })
    });
    Ok(Json(rows.collect()))
}

pub async fn month(
    State(server): State<Arc<Server>>,
    Signed(me): Signed,
    Path(month): Path<String>,
    Query(wanted): Query<Wanted>,
) -> Result<Json<Vec<Value>>, Fail> {
    let shaped = month.len() == 7 && month.as_bytes()[4] == b'-' && month.chars().enumerate().all(|(at, c)| at == 4 || c.is_ascii_digit());
    if !shaped {
        return Err(Fail::bad("A month is written 2026-03."));
    }
    let photos: Vec<Photo> = chosen(&server, &me.name, &wanted).await?.into_iter().filter(|photo| month_of(photo) == month).collect();

    let rows = tokio::task::spawn_blocking(move || {
        photos
            .iter()
            .map(|photo| {
                let aspect = photo.aspect.or_else(|| {
                    numa_io::thumbs::cached_size(&photo.path, photo.mtime, numa_io::thumbs::LIBRARY_EDGE).map(|(w, h)| w as f32 / h as f32)
                });
                json!({ "id": photo.id, "taken": when(photo), "aspect": aspect.unwrap_or(1.5), "rating": photo.rating, "edited": photo.edited })
            })
            .collect()
    })
    .await
    .map_err(Fail::internal)?;
    Ok(Json(rows))
}

pub(crate) async fn chosen(server: &Arc<Server>, account: &str, wanted: &Wanted) -> Result<Vec<Photo>, Fail> {
    let mut libraries = server.home.libraries(account);
    if let Some(only) = wanted.library {
        libraries.retain(|library| library.id == only);
        if libraries.is_empty() {
            return Err(Fail(StatusCode::NOT_FOUND, "There is no such library.".into()));
        }
    }
    let ids: Vec<i64> = libraries.iter().map(|library| library.id).collect();
    let mut photos = server.home.photos(&ids);
    if let Some(person) = given(&wanted.person) {
        let keep = server.home.ask(move |catalog| in_person(catalog, &ids, &person)).await?;
        photos.retain(|photo| keep.contains(&photo.id));
    }
    if let Some(album) = given(&wanted.album) {
        let ids: Vec<i64> = libraries.iter().map(|library| library.id).collect();
        let keep = server.home.ask(move |catalog| in_album(catalog, &ids, &album)).await?;
        photos.retain(|photo| keep.contains(&photo.id));
    }
    if let Some(by) = given(&wanted.by) {
        let owners = crate::by::owners(server, &libraries).await?;
        let nobody = by.eq_ignore_ascii_case(crate::by::UNKNOWN);
        photos.retain(|photo| match owners.get(&photo.id) {
            Some(owner) => owner.eq_ignore_ascii_case(&by),
            None => nobody,
        });
    }
    if let Some(thing) = given(&wanted.thing) {
        let at = words::THINGS.iter().position(|known| known.eq_ignore_ascii_case(&thing));
        let at = at.ok_or_else(|| Fail(StatusCode::NOT_FOUND, "Numa does not look for that thing.".into()))?;
        let indexes = indexes(server, &libraries);
        photos.retain(|photo| {
            let saw = indexes.get(&library_of(photo.id)).and_then(|index| index.saw(&photo.path, photo.mtime));
            saw.is_some_and(|saw| saw.contains(&at))
        });
    }
    if let Some(q) = given(&wanted.q) {
        let keep: HashSet<i64> = found(server, libraries, q).await?.into_iter().map(|(id, _)| id).collect();
        photos.retain(|photo| keep.contains(&photo.id));
    }
    Ok(photos)
}

fn in_person(catalog: &numa_io::catalog::Catalog, libraries: &[i64], person: &str) -> HashSet<i64> {
    let everyone = libraries.iter().flat_map(|&library| catalog.people(library).unwrap_or_default());
    everyone.filter(|(name, _)| name.eq_ignore_ascii_case(person)).flat_map(|(_, ids)| ids).collect()
}

fn in_album(catalog: &numa_io::catalog::Catalog, libraries: &[i64], album: &str) -> HashSet<i64> {
    let filter = Filter { album: Some(album.to_string()), ..Filter::default() };
    libraries.iter().flat_map(|&library| catalog.photos(library, &filter).unwrap_or_default()).map(|photo| photo.id).collect()
}

pub async fn people(State(server): State<Arc<Server>>, Signed(me): Signed) -> Result<Json<Vec<Value>>, Fail> {
    let libraries: Vec<i64> = server.home.libraries(&me.name).iter().map(|library| library.id).collect();
    let shown = server.home.photos(&libraries);
    let found = server
        .home
        .ask(move |catalog| libraries.iter().flat_map(|&library| catalog.people(library).unwrap_or_default()).collect::<Vec<_>>())
        .await?;
    let mut everyone: Vec<(String, HashSet<i64>)> = Vec::new();
    for (name, ids) in found {
        match everyone.iter_mut().find(|(have, _)| have.eq_ignore_ascii_case(&name)) {
            Some((_, have)) => have.extend(ids),
            None => everyone.push((name, ids.into_iter().collect())),
        }
    }

    let mut rows: Vec<(String, usize, i64)> = everyone
        .into_iter()
        .filter_map(|(name, ids)| {
            let theirs: Vec<&Photo> = shown.iter().filter(|photo| ids.contains(&photo.id)).collect();
            Some((name, theirs.len(), theirs.first()?.id))
        })
        .collect();
    rows.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.to_lowercase().cmp(&b.0.to_lowercase())));
    Ok(Json(rows.into_iter().map(|(name, count, portrait)| json!({ "name": name, "count": count, "portrait": portrait })).collect()))
}

pub async fn albums(State(server): State<Arc<Server>>, Signed(me): Signed) -> Result<Json<Vec<Value>>, Fail> {
    let libraries: Vec<i64> = server.home.libraries(&me.name).iter().map(|library| library.id).collect();
    let shown: HashSet<i64> = server.home.photos(&libraries).iter().map(|photo| photo.id).collect();
    let albums = server
        .home
        .ask(move |catalog| {
            let albums = catalog.albums().unwrap_or_default();
            albums.into_iter().map(|(key, name)| (in_album(catalog, &libraries, &key), key, name)).collect::<Vec<_>>()
        })
        .await?;

    let rows = albums.into_iter().filter_map(|(ids, key, name)| {
        let count = ids.iter().filter(|id| shown.contains(id)).count();
        (count > 0).then(|| json!({ "key": key, "name": name, "count": count }))
    });
    Ok(Json(rows.collect()))
}

#[derive(Deserialize)]
pub struct Asked {
    q: String,
    by: Option<String>,
}

pub async fn search(State(server): State<Arc<Server>>, Signed(me): Signed, Query(asked): Query<Asked>) -> Result<Json<Vec<Value>>, Fail> {
    let libraries = server.home.libraries(&me.name);
    let mut found = found(&server, libraries.clone(), asked.q.trim().to_string()).await?;
    if let Some(by) = given(&asked.by) {
        let owners = crate::by::owners(&server, &libraries).await?;
        let nobody = by.eq_ignore_ascii_case(crate::by::UNKNOWN);
        found.retain(|(id, _)| owners.get(id).map_or(nobody, |owner| owner.eq_ignore_ascii_case(&by)));
    }
    Ok(Json(found.into_iter().map(|(id, score)| json!({ "id": id, "score": score })).collect()))
}

async fn found(server: &Arc<Server>, libraries: Vec<Library>, q: String) -> Result<Vec<(i64, f32)>, Fail> {
    let named = libraries.clone();
    let (who, rest) = server.home.ask(move |catalog| words::people_in(catalog, &named, &q)).await?;
    let ids: Vec<i64> = libraries.iter().map(|library| library.id).collect();
    let photos = server.home.photos(&ids);
    if rest.trim().is_empty() {
        let who = who.unwrap_or_default();
        return Ok(photos.iter().filter(|photo| who.contains(&photo.id)).map(|photo| (photo.id, 1.0)).collect());
    }
    if !words::installed() {
        let why = "Search in words starts once its model has downloaded; names work already.";
        return Err(Fail(StatusCode::SERVICE_UNAVAILABLE, why.into()));
    }
    let server = server.clone();
    tokio::task::spawn_blocking(move || {
        let numbers = words::embed_text(&rest).map_err(Fail::internal)?;
        let indexes = indexes(&server, &libraries);
        let scored: Vec<(i64, f32)> = photos
            .iter()
            .filter(|photo| who.as_ref().is_none_or(|who| who.contains(&photo.id)))
            .filter_map(|photo| {
                let index = indexes.get(&library_of(photo.id))?;
                Some((photo.id, words::cosine(index.get(&photo.path, photo.mtime)?, &numbers)))
            })
            .collect();
        Ok(words::ranked(scored))
    })
    .await
    .map_err(Fail::internal)?
}

pub fn indexes<'a>(server: &'a Server, libraries: &[Library]) -> MutexGuard<'a, HashMap<i64, Index>> {
    let mut indexes = server.words.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    for library in libraries {
        indexes.entry(library.id).or_insert_with(|| Index::open(&library.path));
    }
    indexes
}

pub async fn things(State(server): State<Arc<Server>>, Signed(me): Signed) -> Result<Json<Vec<Value>>, Fail> {
    let libraries = server.home.libraries(&me.name);
    let ids: Vec<i64> = libraries.iter().map(|library| library.id).collect();
    let photos = server.home.photos(&ids);
    let counts = tokio::task::spawn_blocking(move || {
        let indexes = indexes(&server, &libraries);
        let mut counts = vec![0usize; words::THINGS.len()];
        for photo in &photos {
            let saw = indexes.get(&library_of(photo.id)).and_then(|index| index.saw(&photo.path, photo.mtime));
            saw.into_iter().flatten().for_each(|&at| counts[at] += 1);
        }
        counts
    })
    .await
    .map_err(Fail::internal)?;
    let mut rows: Vec<(usize, &str)> = counts.into_iter().zip(words::THINGS).filter(|(count, _)| *count > 0).collect();
    rows.sort_by_key(|row| std::cmp::Reverse(row.0));
    Ok(Json(rows.into_iter().map(|(count, thing)| json!({ "thing": thing, "count": count })).collect()))
}

#[derive(Deserialize)]
pub struct Since {
    since: Option<i64>,
}

pub async fn in_step(
    State(server): State<Arc<Server>>,
    Signed(me): Signed,
    Path(id): Path<i64>,
    Query(since): Query<Since>,
) -> Result<axum::response::Response, Fail> {
    use axum::response::IntoResponse;
    let now = crate::store::now();
    let library = server.home.libraries(&me.name).into_iter().find(|library| library.id == id);
    let library = library.ok_or_else(|| Fail(StatusCode::NOT_FOUND, "There is no such library.".into()))?;
    let since = since.since.unwrap_or(0);
    let updated = server.store.updated();
    let photos = server.home.files(id);
    let rows = tokio::task::spawn_blocking(move || {
        photos
            .iter()
            .filter_map(|photo| {
                let changed = updated.get(&photo.id).copied().unwrap_or(0).max(photo.mtime);
                (changed > since).then(|| {
                    let path = photo.path.strip_prefix(&library.path).unwrap_or(&photo.path);
                    json!({
                        "id": photo.id,
                        "path": path.to_string_lossy(),
                        "size": std::fs::metadata(&photo.path).map(|meta| meta.len()).ok(),
                        "mtime": photo.mtime,
                        "taken": when(photo),
                        "rating": photo.rating,
                        "flag": crate::photos::flag_name(photo.flag),
                        "updated": changed,
                    })
                })
            })
            .collect::<Vec<Value>>()
    })
    .await
    .map_err(Fail::internal)?;
    Ok(([("x-numa-now", now.to_string())], Json(rows)).into_response())
}
