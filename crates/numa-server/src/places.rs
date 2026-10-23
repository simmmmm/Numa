use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use numa_io::catalog::Photo;
use numa_io::places::{from_positions, nearest, Place};
use serde_json::{json, Value};

use crate::auth::Signed;
use crate::{Fail, Server};

pub fn named(at: (f32, f32)) -> Option<(&'static str, &'static str, &'static str)> {
    nearest(at).map(|place| (place.name.as_str(), place.region.as_str(), place.country.as_str()))
}

fn positions(paths: &[&PathBuf]) -> Vec<(f64, f64)> {
    let step = (paths.len() / 120).max(1);
    paths.iter().step_by(step).filter_map(|path| numa_io::exif::position(path)).collect()
}

pub fn names_for(server: &Server, paths: &[PathBuf], times: &[i64]) -> Value {
    let names = from_positions(&positions(&paths.iter().collect::<Vec<_>>()));
    if !names.is_empty() {
        return json!({ "from": "gps", "names": names });
    }
    let mut times = times.to_vec();
    times.sort_unstable();
    let near = |when: i64| {
        let at = times.partition_point(|&time| time < when - HOUR);
        times.get(at).is_some_and(|&time| time <= when + HOUR)
    };
    let own: std::collections::HashSet<&PathBuf> = paths.iter().collect();
    let everyone: Vec<i64> = server.home.all_libraries().iter().map(|library| library.id).collect();
    let others = server.home.photos(&everyone);
    let beside: Vec<&PathBuf> = others.iter().filter(|photo| !own.contains(&photo.path) && near(crate::home::when(photo))).map(|photo| &photo.path).collect();
    let names = from_positions(&positions(&beside));
    json!({ "from": if names.is_empty() { None } else { Some("borrowed") }, "names": names })
}

const HOUR: i64 = 3600;

#[derive(Default)]
pub struct Known {
    found: Mutex<HashMap<i64, (i64, Option<&'static Place>)>>,
    reading: AtomicBool,
}

pub fn known(server: &Arc<Server>, photos: &[Photo]) -> (HashMap<i64, (&'static str, &'static str)>, bool) {
    let found = server.places.found.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let mut places = HashMap::new();
    let mut missing = false;
    for photo in photos {
        match found.get(&photo.id).filter(|(mtime, _)| *mtime == photo.mtime) {
            Some((_, Some(place))) => drop(places.insert(photo.id, (place.name.as_str(), place.country.as_str()))),
            Some((_, None)) => {}
            None => missing = true,
        }
    }
    drop(found);
    if missing && !server.places.reading.swap(true, Ordering::SeqCst) {
        let server = server.clone();
        std::thread::spawn(move || {
            read_places(&server);
            server.places.reading.store(false, Ordering::SeqCst);
        });
    }
    (places, missing)
}

fn read_places(server: &Server) {
    let everyone: Vec<i64> = server.home.all_libraries().iter().map(|library| library.id).collect();
    for photo in server.home.photos(&everyone) {
        if server.activity.paused() {
            return;
        }
        let read = server.places.found.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).get(&photo.id).is_some_and(|(mtime, _)| *mtime == photo.mtime);
        if !read {
            let place = numa_io::exif::position(&photo.path).and_then(|(latitude, longitude)| nearest((latitude as f32, longitude as f32)));
            server.places.found.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).insert(photo.id, (photo.mtime, place));
        }
    }
}

pub async fn library(State(server): State<Arc<Server>>, Signed(me): Signed, Path(id): Path<i64>) -> Result<Json<Value>, Fail> {
    let library = server.home.libraries(&me.name).into_iter().find(|library| library.id == id)
        .ok_or_else(|| Fail(StatusCode::NOT_FOUND, "There is no such library.".into()))?;
    let answer = tokio::task::spawn_blocking(move || {
        let photos = server.home.files(library.id);
        let times: Vec<i64> = photos.iter().map(crate::home::when).collect();
        let paths: Vec<PathBuf> = photos.into_iter().map(|photo| photo.path).collect();
        names_for(&server, &paths, &times)
    })
    .await
    .map_err(|err| Fail(StatusCode::INTERNAL_SERVER_ERROR, err.to_string()))?;
    Ok(Json(answer))
}

#[cfg(test)]
#[test]
#[ignore]
fn on_real_trips() {
    for folder in std::env::var("NUMA_TRIPS").unwrap().split(';') {
        let mut paths: Vec<PathBuf> = std::fs::read_dir(folder).unwrap().flatten().map(|entry| entry.path()).filter(|path| path.is_file()).collect();
        paths.sort();
        println!("{folder}: {:?}", from_positions(&positions(&paths.iter().collect::<Vec<_>>())));
    }
}
