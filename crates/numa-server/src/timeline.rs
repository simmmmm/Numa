use std::collections::{HashMap, HashSet};
use std::sync::{Arc, MutexGuard};

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use numa_io::catalog::{library_of, Filter, Library, Photo, Sort};
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

    by: Option<String>,

    flag: Option<String>,

    rating: Option<u8>,

    camera: Option<String>,

    place: Option<String>,

    region: Option<String>,

    date: Option<String>,

    sort: Option<String>,

    reversed: Option<String>,
}

impl Wanted {

    fn sorted(&self) -> Option<Sort> {
        match given(&self.sort).as_deref() {
            Some("name") => Some(Sort::Name),
            Some("rating") => Some(Sort::Rating),
            Some("sharpness") => Some(Sort::Sharpness),
            Some("suggestion") => Some(Sort::Suggested),
            _ => None,
        }
    }
}

const EVERYTHING: &str = "all";

fn given(text: &Option<String>) -> Option<String> {
    text.as_deref().map(str::trim).filter(|text| !text.is_empty()).map(str::to_string)
}

pub async fn libraries(State(server): State<Arc<Server>>, Signed(me): Signed) -> Json<Vec<Value>> {
    let mine = server.home.sent_folder(&me.name);

    let mut libraries = server.home.libraries(&me.name);
    libraries.sort_by_cached_key(|library| std::cmp::Reverse(server.home.newest(library.id)));
    let rows = libraries.into_iter().map(|library| {
        json!({ "id": library.id, "name": library.label(), "count": server.home.count(library.id), "mine": library.path == mine })
    });
    Json(rows.collect())
}

pub async fn months(State(server): State<Arc<Server>>, Signed(me): Signed, Query(wanted): Query<Wanted>) -> Result<Json<Vec<Value>>, Fail> {
    let photos = chosen(&server, &me.name, &wanted).await?;

    type Month = (String, usize, Vec<(i64, usize)>);
    let mut months: Vec<Month> = Vec::new();
    let flat = wanted.sorted().is_some();
    for photo in &photos {
        let month = if flat { EVERYTHING.to_string() } else { month_of(photo) };
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
    let shaped = month == EVERYTHING || month.len() == 7 && month.as_bytes()[4] == b'-' && month.chars().enumerate().all(|(at, c)| at == 4 || c.is_ascii_digit());
    if !shaped {
        return Err(Fail::bad("A month is written 2026-03."));
    }
    let photos: Vec<Photo> = chosen(&server, &me.name, &wanted).await?.into_iter().filter(|photo| month == EVERYTHING || month_of(photo) == month).collect();
    let marks = Marks::of(&server, &photos);

    let rows = tokio::task::spawn_blocking(move || {
        photos
            .iter()
            .map(|photo| {
                let aspect = photo.aspect.or_else(|| {
                    numa_io::thumbs::cached_size(&photo.path, photo.mtime, numa_io::thumbs::LIBRARY_EDGE).map(|(w, h)| w as f32 / h as f32)
                });
                let mut row = marks.tile(photo);
                row["aspect"] = json!(aspect.unwrap_or(1.5));
                row
            })
            .collect()
    })
    .await
    .map_err(Fail::internal)?;
    Ok(Json(rows))
}

struct Marks {
    bursts: HashMap<(i64, i64), usize>,
}

impl Marks {
    fn of<'a>(server: &Server, photos: impl IntoIterator<Item = &'a Photo>) -> Self {
        let mut marks = Marks { bursts: HashMap::new() };
        for library in photos.into_iter().map(|photo| library_of(photo.id)).collect::<HashSet<i64>>() {
            let files = server.home.files(library);
            for burst in files.iter().filter_map(|photo| photo.burst) {
                *marks.bursts.entry((library, burst)).or_default() += 1;
            }
        }
        marks
    }

    fn tile(&self, photo: &Photo) -> Value {
        let library = library_of(photo.id);
        let stack = photo.burst.filter(|_| photo.best_of_burst).and_then(|burst| self.bursts.get(&(library, burst)));

        json!({
            "id": photo.id, "name": photo.path.file_name().map(|name| name.to_string_lossy()), "taken": when(photo),
            "rating": photo.rating, "flag": crate::photos::flag_name(photo.flag), "edited": photo.edited, "raw": numa_io::raw::is_raw(&photo.path),
            "stack": stack,
        })
    }
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
    let tests = tests(server, &libraries, &photos, wanted).await?;
    photos.retain(|photo| tests.iter().all(|(_, test)| test(photo)));
    let reversed = given(&wanted.reversed).is_some_and(|value| value == "1");
    match wanted.sorted() {
        Some(sort) => photos.sort_by(|a, b| sort.compare(reversed, a, b)),
        None => {
            let oldest_first = (wanted.library.is_some() || given(&wanted.album).is_some()) != reversed;
            photos.sort_by(|a, b| order(when(a), when(b), oldest_first).then_with(|| a.path.cmp(&b.path)));
        }
    }
    Ok(photos)
}

fn order(a: i64, b: i64, oldest_first: bool) -> std::cmp::Ordering {
    const DAY: i64 = 86_400;
    match oldest_first {
        true => a.cmp(&b),
        false => b.div_euclid(DAY).cmp(&a.div_euclid(DAY)).then(a.cmp(&b)),
    }
}

type Test = (&'static str, Box<dyn Fn(&Photo) -> bool + Send + Sync>);

async fn tests(server: &Arc<Server>, libraries: &[Library], photos: &[Photo], wanted: &Wanted) -> Result<Vec<Test>, Fail> {
    let ids: Vec<i64> = libraries.iter().map(|library| library.id).collect();
    let mut tests: Vec<Test> = Vec::new();
    if let Some(only) = wanted.library {
        tests.push(("library", Box::new(move |photo: &Photo| library_of(photo.id) == only)));
    }
    if let Some(person) = given(&wanted.person) {
        let ids = ids.clone();
        let keep = server.home.ask(move |catalog| in_person(catalog, &ids, &person)).await?;
        tests.push(("person", Box::new(move |photo: &Photo| keep.contains(&photo.id))));
    }
    if let Some(album) = given(&wanted.album) {
        let ids = ids.clone();
        let keep = server.home.ask(move |catalog| in_album(catalog, &ids, &album)).await?;
        tests.push(("album", Box::new(move |photo: &Photo| keep.contains(&photo.id))));
    }
    if let Some(by) = given(&wanted.by) {
        let owners = crate::by::owners(server, libraries).await?;
        let nobody = by.eq_ignore_ascii_case(crate::by::UNKNOWN);
        tests.push(("by", Box::new(move |photo: &Photo| owners.get(&photo.id).map_or(nobody, |owner| owner.eq_ignore_ascii_case(&by)))));
    }
    if let Some(thing) = given(&wanted.thing) {
        let at = words::THINGS.iter().position(|known| known.eq_ignore_ascii_case(&thing));
        let at = at.ok_or_else(|| Fail(StatusCode::NOT_FOUND, "Numa does not look for that thing.".into()))?;
        let keep: HashSet<i64> = saw(server, libraries, photos).into_iter().filter(|(_, saw)| saw.contains(&at)).map(|(id, _)| id).collect();
        tests.push(("thing", Box::new(move |photo: &Photo| keep.contains(&photo.id))));
    }
    if let Some(q) = given(&wanted.q) {
        let keep: HashSet<i64> = found(server, libraries.to_vec(), q).await?.into_iter().map(|(id, _)| id).collect();
        tests.push(("q", Box::new(move |photo: &Photo| keep.contains(&photo.id))));
    }
    if let Some(flag) = given(&wanted.flag) {
        tests.push(("flag", Box::new(move |photo: &Photo| crate::photos::flag_name(photo.flag) == flag)));
    }
    if let Some(stars) = wanted.rating {
        tests.push(("rating", Box::new(move |photo: &Photo| photo.rating >= stars)));
    }
    if let Some(date) = given(&wanted.date) {
        tests.push(("date", Box::new(move |photo: &Photo| numa_io::import::day(when(photo), 0).starts_with(&date))));
    }
    if let Some(camera) = given(&wanted.camera) {
        let names = cameras(server, &ids).await?;
        tests.push(("camera", Box::new(move |photo: &Photo| names.get(&photo.id).is_some_and(|name| name.eq_ignore_ascii_case(&camera)))));
    }
    if let Some(place) = given(&wanted.place) {
        let keep = crate::map::taken_in(server, photos, &place, false);
        tests.push(("place", Box::new(move |photo: &Photo| keep.contains(&photo.id))));
    }
    if let Some(region) = given(&wanted.region) {
        let keep = crate::map::taken_in(server, photos, &region, true);
        tests.push(("region", Box::new(move |photo: &Photo| keep.contains(&photo.id))));
    }
    Ok(tests)
}

async fn cameras(server: &Server, libraries: &[i64]) -> Result<HashMap<i64, String>, Fail> {
    let ids = libraries.to_vec();
    let bodies = server.home.ask(move |catalog| ids.iter().filter_map(|&id| catalog.bodies(id).ok()).flatten().collect::<Vec<(i64, String)>>()).await?;
    Ok(bodies.into_iter().filter(|(_, key)| !key.is_empty()).map(|(id, key)| (id, numa_io::clocks::camera_name(&key, &[]))).collect())
}

fn saw(server: &Server, libraries: &[Library], photos: &[Photo]) -> HashMap<i64, Vec<usize>> {
    let indexes = indexes(server, libraries);
    photos.iter().filter_map(|photo| Some((photo.id, indexes.get(&library_of(photo.id))?.saw(&photo.path, photo.mtime)?.to_vec()))).collect()
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
pub struct Renamed {
    name: String,
}

async fn seen(server: &Arc<Server>, account: &str, key: &str) -> Result<(), Fail> {
    let libraries: Vec<i64> = server.home.libraries(account).iter().map(|library| library.id).collect();
    let shown: HashSet<i64> = server.home.photos(&libraries).iter().map(|photo| photo.id).collect();
    let key = key.to_string();
    let any = server.home.ask(move |catalog| in_album(catalog, &libraries, &key).iter().any(|id| shown.contains(id))).await?;
    any.then_some(()).ok_or_else(|| Fail(StatusCode::NOT_FOUND, "There is no such album.".into()))
}

pub async fn rename_album(State(server): State<Arc<Server>>, Signed(me): Signed, Path(key): Path<String>, Json(renamed): Json<Renamed>) -> Result<Json<Value>, Fail> {
    seen(&server, &me.name, &key).await?;
    let (name, at) = (renamed.name.trim().to_string(), key.clone());
    server.home.ask(move |catalog| catalog.rename_album(&at, &name)).await?.map_err(Fail::bad)?;
    Ok(Json(json!({ "key": key, "name": renamed.name.trim() })))
}

pub async fn delete_album(State(server): State<Arc<Server>>, Signed(me): Signed, Path(key): Path<String>) -> Result<Json<Value>, Fail> {
    seen(&server, &me.name, &key).await?;
    server.home.ask(move |catalog| catalog.delete_album(&key)).await?.map_err(Fail::bad)?;
    Ok(Json(json!({ "deleted": true })))
}

pub async fn search(State(server): State<Arc<Server>>, Signed(me): Signed, Query(wanted): Query<Wanted>) -> Result<Json<Vec<Value>>, Fail> {
    let q = given(&wanted.q).ok_or_else(|| Fail::bad("Say what to look for."))?;
    let found = found(&server, server.home.libraries(&me.name), q).await?;
    let kept: HashMap<i64, Photo> = chosen(&server, &me.name, &Wanted { q: None, ..wanted }).await?.into_iter().map(|photo| (photo.id, photo)).collect();
    let photos: Vec<(&Photo, f32)> = found.into_iter().filter_map(|(id, score)| Some((kept.get(&id)?, score))).collect();
    let marks = Marks::of(&server, photos.iter().map(|(photo, _)| *photo));
    Ok(Json(
        photos
            .iter()
            .map(|(photo, score)| {
                let mut row = marks.tile(photo);
                row["score"] = json!(score);
                row
            })
            .collect(),
    ))
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

pub async fn filters(State(server): State<Arc<Server>>, Signed(me): Signed, Query(wanted): Query<Wanted>) -> Result<Json<Value>, Fail> {
    let libraries = server.home.libraries(&me.name);
    let ids: Vec<i64> = libraries.iter().map(|library| library.id).collect();
    let photos = server.home.photos(&ids);
    let tests = tests(&server, &libraries, &photos, &wanted).await?;
    let people = {
        let ids = ids.clone();
        server.home.ask(move |catalog| ids.iter().flat_map(|&id| catalog.people(id).unwrap_or_default()).collect::<Vec<_>>()).await?
    };
    let mut named: HashMap<i64, Vec<String>> = HashMap::new();
    for (name, photos) in people {
        photos.into_iter().for_each(|id| named.entry(id).or_default().push(name.clone()));
    }
    let cameras = cameras(&server, &ids).await?;
    let owners = crate::by::owners(&server, &libraries).await?;
    let (places, looking) = crate::places::known(&server, &photos);
    let answer = tokio::task::spawn_blocking(move || {
        let saw = saw(&server, &libraries, &photos);

        let within = |own: &str| photos.iter().filter(|photo| tests.iter().all(|(key, test)| *key == own || test(photo))).collect::<Vec<&Photo>>();
        let here: Vec<(&str, &str)> = within("place").iter().filter_map(|photo| places.get(&photo.id).copied()).collect();
        let lands = tally(here.iter().map(|(_, country)| *country)).into_iter().map(|(country, count)| {
            let towns = tally(here.iter().filter(|(_, land)| *land == country).map(|(town, _)| *town));
            json!({ "country": country, "count": count, "towns": towns })
        });
        let mut years = tally(within("date").iter().map(|photo| numa_io::import::day(when(photo), 0)[..4].to_string()));
        years.sort_by(|a, b| b.0.cmp(&a.0));
        let mut by = tally(within("by").iter().map(|photo| owners.get(&photo.id).map(String::as_str)));
        by.sort_by_key(|(account, count)| (account.is_none(), std::cmp::Reverse(*count)));
        let mut ratings = [0usize; 6];
        within("rating").iter().for_each(|photo| ratings[photo.rating.min(5) as usize] += 1);
        json!({
            "libraries": tally(within("library").iter().map(|photo| library_of(photo.id))),
            "people": tally(within("person").iter().flat_map(|photo| named.get(&photo.id).into_iter().flatten().map(String::as_str))),
            "places": lands.collect::<Vec<Value>>(),
            "looking": looking,
            "cameras": tally(within("camera").iter().filter_map(|photo| cameras.get(&photo.id).map(String::as_str))),
            "years": years,
            "by": by,
            "things": tally(within("thing").iter().flat_map(|photo| saw.get(&photo.id).into_iter().flatten().map(|&at| words::THINGS[at]))),
            "flags": tally(within("flag").iter().map(|photo| crate::photos::flag_name(photo.flag))),
            "ratings": ratings,
        })
    })
    .await
    .map_err(Fail::internal)?;
    Ok(Json(answer))
}

fn tally<T: Ord + std::hash::Hash>(values: impl Iterator<Item = T>) -> Vec<(T, usize)> {
    let mut counts: HashMap<T, usize> = HashMap::new();
    values.for_each(|value| *counts.entry(value).or_default() += 1);
    let mut rows: Vec<(T, usize)> = counts.into_iter().collect();
    rows.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));
    rows
}

#[derive(Deserialize)]
pub struct Since {
    since: Option<i64>,

    gone: Option<u8>,

    synced: Option<u8>,
}

pub async fn in_step(
    State(server): State<Arc<Server>>,
    Signed(me): Signed,
    headers: axum::http::HeaderMap,
    Path(id): Path<i64>,
    Query(asked): Query<Since>,
) -> Result<axum::response::Response, Fail> {
    use axum::response::IntoResponse;
    let now = crate::store::now();
    let library = server.home.libraries(&me.name).into_iter().find(|library| library.id == id);
    let library = library.ok_or_else(|| Fail(StatusCode::NOT_FOUND, "There is no such library.".into()))?;
    let since = asked.since.unwrap_or(0);
    let updated = server.store.updated();
    let photos = server.home.files(id);
    let mut rows = tokio::task::spawn_blocking(move || {
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
                        "edited": photo.edited,
                        "updated": changed,
                    })
                })
            })
            .collect::<Vec<Value>>()
    })
    .await
    .map_err(Fail::internal)?;
    if asked.gone == Some(1) {
        crate::sync::with_albums(&server, &mut rows).await?;
        rows.extend(crate::sync::deletes(&server, id, since));
    }
    if asked.synced == Some(1) {
        crate::sync::note(&server, &headers, &me.name, id);
    }
    Ok(([("x-numa-now", now.to_string())], Json(rows)).into_response())
}

#[cfg(test)]
mod tests {
    use super::{order, Sort, Wanted};

    #[test]
    fn a_day_reads_forward_with_the_newest_day_on_top() {
        let day = 86_400;
        let mut times = vec![day + 10, 2 * day + 5, day + 3, 2 * day + 9];
        times.sort_by(|a, b| order(*a, *b, false));
        assert_eq!(times, [2 * day + 5, 2 * day + 9, day + 3, day + 10]);

        times.sort_by(|a, b| order(*a, *b, true));
        assert_eq!(times, [day + 3, day + 10, 2 * day + 5, 2 * day + 9]);
    }

    #[test]
    fn only_a_named_sort_is_a_sort() {
        let sort = |name: &str| Wanted { sort: Some(name.into()), ..Wanted::default() }.sorted();
        assert!(sort("date").is_none() && sort("").is_none() && sort("nonsense").is_none());
        assert!(matches!(sort("name"), Some(Sort::Name)) && matches!(sort("suggestion"), Some(Sort::Suggested)));
    }
}
