use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use axum::extract::{Path as Route, Query, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use numa_io::catalog::Photo;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::activity::Download;
use crate::auth::Signed;
use crate::timeline::Wanted;
use crate::{Fail, Server};

const FILE: &str = "map/tiles.db";

pub const NAME: &str = "Map";

type Named = (&'static str, &'static str, &'static str);

#[derive(Clone, Copy)]
pub struct Spot {
    pub at: (f32, f32),
    pub place: Option<Named>,
}

#[derive(Default)]
pub struct Map {
    found: Mutex<HashMap<i64, (i64, Option<Spot>)>>,
    reading: AtomicBool,
    fetching: AtomicBool,

    why: Mutex<Option<String>>,
}

fn found(server: &Server) -> MutexGuard<'_, HashMap<i64, (i64, Option<Spot>)>> {
    server.map.found.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn why(server: &Server) -> MutexGuard<'_, Option<String>> {
    server.map.why.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn known(found: &HashMap<i64, (i64, Option<Spot>)>, photo: &Photo) -> bool {
    found.get(&photo.id).is_some_and(|(mtime, _)| *mtime == photo.mtime)
}

fn spot((lat, lon): (f64, f64), named: &mut HashMap<(u32, u32), Option<Named>>) -> Spot {
    let at = (lat as f32, lon as f32);
    Spot { at, place: *named.entry((at.0.to_bits(), at.1.to_bits())).or_insert_with(|| crate::places::named(at)) }
}

fn keep(server: &Server, read: Vec<(i64, i64, Option<(f64, f64)>)>) {
    let kept = read.clone();
    if let Some(Err(err)) = server.home.blocking(move |catalog| catalog.keep_where(&kept)) {
        log::warn!("places: {err}");
    }
    let mut named = HashMap::new();
    let spots: Vec<(i64, (i64, Option<Spot>))> = read.into_iter().map(|(id, mtime, at)| (id, (mtime, at.map(|at| spot(at, &mut named))))).collect();
    found(server).extend(spots);
}

pub fn kept(server: &Server, catalog: &numa_io::catalog::Catalog, ids: &[i64]) {
    let photos: Vec<Photo> = ids.iter().filter_map(|&id| server.home.any_photo(id)).collect();
    let photos: Vec<Photo> = {
        let found = found(server);
        photos.into_iter().filter(|photo| !known(&found, photo)).collect()
    };
    let (at, unread) = match catalog.where_taken(&photos) {
        Ok(kept) => kept,
        Err(err) => return log::warn!("places: {err}"),
    };
    let unread: HashSet<i64> = unread.iter().map(|photo| photo.id).collect();
    let mut named = HashMap::new();
    let spots: Vec<(i64, (i64, Option<Spot>))> = photos
        .iter()
        .filter(|photo| !unread.contains(&photo.id))
        .map(|photo| (photo.id, (photo.mtime, at.get(&photo.id).map(|&at| spot(at, &mut named)))))
        .collect();
    found(server).extend(spots);
}

pub fn spots(server: &Arc<Server>, photos: &[Photo]) -> (HashMap<i64, Option<Spot>>, bool) {
    let found = found(server);
    let mut spots = HashMap::new();
    let mut missing = false;
    for photo in photos {
        match found.get(&photo.id).filter(|(mtime, _)| *mtime == photo.mtime) {
            Some((_, spot)) => drop(spots.insert(photo.id, *spot)),
            None => missing = true,
        }
    }
    drop(found);
    if missing && !server.map.reading.swap(true, Ordering::SeqCst) {
        let server = server.clone();
        std::thread::spawn(move || {
            read(&server);
            server.map.reading.store(false, Ordering::SeqCst);
            if !server.activity.paused() && crate::start::may_read(&server) {
                let _ = bring(&server);
            }
        });
    }
    (spots, missing)
}

fn read(server: &Server) {
    let everyone: Vec<i64> = server.home.all_libraries().iter().map(|library| library.id).collect();
    let photos = server.home.photos(&everyone);
    let unread: Vec<Photo> = {
        let found = found(server);
        photos.into_iter().filter(|photo| !known(&found, photo)).collect()
    };
    for batch in unread.chunks(256) {
        if server.activity.paused() {
            return;
        }
        keep(server, numa_io::catalog::read_where(batch));
    }
}

pub fn spot_of(server: &Server, photo: &Photo) -> Option<Spot> {
    if let Some((_, spot)) = found(server).get(&photo.id).filter(|(mtime, _)| *mtime == photo.mtime) {
        return *spot;
    }
    keep(server, numa_io::catalog::read_where(std::slice::from_ref(photo)));
    found(server).get(&photo.id).and_then(|(_, spot)| *spot)
}

pub fn said(spot: Option<Spot>) -> Value {
    json!({
        "position": spot.map(|spot| [spot.at.0, spot.at.1]),
        "place": spot.and_then(|spot| spot.place).map(|(town, _, country)| format!("{town}, {country}")),
    })
}

pub fn taken_in(server: &Arc<Server>, photos: &[Photo], name: &str, region: bool) -> HashSet<i64> {
    let (spots, _) = spots(server, photos);
    let named = |(town, area, country): Named| match region {
        true => area.eq_ignore_ascii_case(name),
        false => town.eq_ignore_ascii_case(name) || country.eq_ignore_ascii_case(name),
    };
    spots.into_iter().filter(|(_, spot)| spot.and_then(|spot| spot.place).is_some_and(named)).map(|(id, _)| id).collect()
}

#[derive(Deserialize)]
pub struct Count {
    count: Option<String>,
}

pub async fn places(State(server): State<Arc<Server>>, Signed(me): Signed, Query(wanted): Query<Wanted>, Query(count): Query<Count>) -> Result<Json<Value>, Fail> {
    let photos = crate::timeline::chosen(&server, &me.name, &wanted).await?;
    let (spots, looking) = spots(&server, &photos);
    let liked: Vec<(i64, bool)> = photos.iter().map(|photo| (photo.id, photo.rating >= 3 || crate::photos::flag_name(photo.flag) == "pick")).collect();
    let (towns, points, none) = answer(&liked, &spots);
    if count.count.is_some() {
        return Ok(Json(json!({ "count": towns.len(), "looking": looking })));
    }
    let why = why(&server).clone();
    let map = match numa_io::map::ready(&server.data.join(FILE)) {
        true => "ready",
        false if server.map.fetching.load(Ordering::SeqCst) => "fetching",
        false if why.is_some() => "failed",
        false => "none",
    };
    let towns: Vec<[&str; 3]> = towns.into_iter().map(|(town, region, country)| [town, region, country]).collect();
    Ok(Json(json!({ "looking": looking, "none": none, "map": map, "why": why, "towns": towns, "points": points })))
}

fn answer(photos: &[(i64, bool)], spots: &HashMap<i64, Option<Spot>>) -> (Vec<Named>, Vec<Value>, usize) {
    let (mut towns, mut index, mut points, mut none) = (Vec::new(), HashMap::new(), Vec::new(), 0);
    for (id, _) in photos.iter().filter(|(_, liked)| *liked).chain(photos.iter().filter(|(_, liked)| !*liked)) {
        match spots.get(id) {
            Some(Some(Spot { at, place: Some(place) })) => {
                let town = *index.entry(*place).or_insert_with(|| {
                    towns.push(*place);
                    towns.len() - 1
                });
                points.push(json!([id, at.0, at.1, town]));
            }
            Some(_) => none += 1,
            None => {}
        }
    }
    (towns, points, none)
}

pub async fn tile(State(server): State<Arc<Server>>, Signed(_): Signed, Route((z, x, y)): Route<(u8, u32, u32)>) -> Result<Response, Fail> {
    let file = server.data.join(FILE);
    let data = tokio::task::spawn_blocking(move || numa_io::map::tile(&file, z, x, y)).await.map_err(Fail::internal)?;
    Ok(match data {
        Some(data) if !data.is_empty() => (
            [(header::CONTENT_TYPE, "application/vnd.mapbox-vector-tile"), (header::CONTENT_ENCODING, "gzip"), (header::CACHE_CONTROL, "private, max-age=86400")],
            data,
        )
            .into_response(),
        _ => StatusCode::NO_CONTENT.into_response(),
    })
}

pub fn start(server: &Server) {
    if !server.activity.paused() {
        let _ = bring(server);
    }
}

pub fn night(server: &Server, report: &mut crate::night::Report) {
    read(server);
    match bring(server) {
        Ok((0, _)) => {}
        Ok((tiles, bytes)) => report.done.push(format!(
            "The map brought up to date: {} more, {}",
            crate::night::plural(tiles, "tile", "tiles"),
            crate::night::megabytes(bytes)
        )),
        Err(err) => report.needs.push(crate::night::Need { text: format!("The map could not be fetched ({err}); is the NAS on the internet?"), link: "#/activity".into() }),
    }
}

pub async fn again(State(server): State<Arc<Server>>, Signed(_): Signed) -> Json<Value> {
    if begin(&server) {
        std::thread::spawn(move || brought(&server));
    }
    Json(json!({}))
}

fn bring(server: &Server) -> Result<(usize, u64), String> {
    if !begin(server) {
        return Ok((0, 0));
    }
    brought(server)
}

fn begin(server: &Server) -> bool {
    if server.map.fetching.swap(true, Ordering::SeqCst) {
        return false;
    }
    why(server).take();
    shown(server, 0, None, "fetching");
    true
}

fn shown(server: &Server, total: u64, part: Option<&Path>, state: &'static str) {
    let mut downloads = server.activity.downloads();
    downloads.retain(|row| row.name != NAME);
    downloads.push(Download { name: NAME.into(), total, parts: part.map(Path::to_path_buf).into_iter().collect(), state });
}

fn brought(server: &Server) -> Result<(usize, u64), String> {
    let began = std::time::Instant::now();
    let dir = server.data.join("map");
    let mut coming = (0, None);
    let mut started = |total, part: &Path| {
        coming = (total, Some(part.to_path_buf()));
        shown(server, total, Some(part), "fetching");
    };

    let brought = numa_io::map::bring(&dir, &[], &mut started).and_then(|(tiles, bytes)| {
        if tiles > 0 {
            started(0, &dir.join("tiles.part"));
        }
        let positions: Vec<(f32, f32)> = found(server).values().filter_map(|(_, spot)| spot.map(|spot| spot.at)).collect();
        numa_io::map::bring(&dir, &positions, &mut started).map(|(more, more_bytes)| (tiles + more, bytes + more_bytes))
    });
    *why(server) = brought.as_ref().err().map(|err| numa_io::map::plainly(err));
    server.map.fetching.store(false, Ordering::SeqCst);
    match &brought {
        Ok((0, _)) => server.activity.downloads().retain(|row| row.name != NAME),
        Ok(_) => shown(server, coming.0, coming.1.as_deref(), "done"),
        Err(_) => shown(server, coming.0, coming.1.as_deref(), "failed"),
    }
    match &brought {
        Ok((0, _)) => {}
        Ok((tiles, bytes)) => log::info!("map: {tiles} tiles, {} MB in {:.0} s", bytes / 1_000_000, began.elapsed().as_secs_f32()),
        Err(err) => log::warn!("map: {err}; tried again tonight"),
    }
    brought
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn places_and_points_the_liked_first() {
        let spot = |town: &'static str, lat| Some(Spot { at: (lat, 1.0), place: Some((town, "Region", "Country")) });
        let spots = HashMap::from([(1, spot("A", 1.0)), (2, spot("B", 2.0)), (3, spot("A", 3.0)), (4, None), (5, Some(Spot { at: (0.0, -30.0), place: None }))]);

        let (towns, points, none) = answer(&[(1, false), (2, false), (3, true), (4, false), (5, false), (6, false)], &spots);
        assert_eq!(towns, [("A", "Region", "Country"), ("B", "Region", "Country")]);
        assert_eq!(points, [json!([3, 3.0, 1.0, 0]), json!([1, 1.0, 1.0, 0]), json!([2, 2.0, 1.0, 1])]);
        assert_eq!(none, 2);
    }
}
