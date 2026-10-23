use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use numa_io::catalog::Photo;
use numa_io::places::{from_positions, nearest};
use serde_json::{json, Value};

use crate::auth::Signed;
use crate::{Fail, Server};

pub fn named(at: (f32, f32)) -> Option<(&'static str, &'static str, &'static str)> {
    nearest(at).map(|place| (place.name.as_str(), place.region.as_str(), place.country.as_str()))
}

fn some<T>(all: &[T]) -> impl Iterator<Item = &T> {
    all.iter().step_by((all.len() / 120).max(1))
}

pub fn read(paths: &[PathBuf]) -> Vec<(f64, f64)> {
    some(paths).filter_map(|path| numa_io::exif::position(path)).collect()
}

fn positions(server: &Server, photos: &[&Photo]) -> Vec<(f64, f64)> {
    some(photos).filter_map(|photo| crate::map::spot_of(server, photo)).map(|spot| (f64::from(spot.at.0), f64::from(spot.at.1))).collect()
}

pub fn names_for(server: &Server, own: &[(f64, f64)], paths: &[PathBuf], times: &[i64]) -> Value {
    let names = from_positions(own);
    if !names.is_empty() {
        return json!({ "from": "gps", "names": names });
    }
    let mut times = times.to_vec();
    times.sort_unstable();
    let near = |when: i64| {
        let at = times.partition_point(|&time| time < when - HOUR);
        times.get(at).is_some_and(|&time| time <= when + HOUR)
    };
    let files: std::collections::HashSet<&PathBuf> = paths.iter().collect();
    let everyone: Vec<i64> = server.home.all_libraries().iter().map(|library| library.id).collect();
    let others = server.home.photos(&everyone);
    let beside: Vec<&Photo> = others.iter().filter(|photo| !files.contains(&photo.path) && near(crate::home::when(photo))).collect();
    let names = from_positions(&positions(server, &beside));
    json!({ "from": if names.is_empty() { None } else { Some("borrowed") }, "names": names })
}

const HOUR: i64 = 3600;

pub fn known(server: &Arc<Server>, photos: &[Photo]) -> (HashMap<i64, (&'static str, &'static str)>, bool) {
    let (spots, looking) = crate::map::spots(server, photos);
    let places = spots.into_iter().filter_map(|(id, spot)| spot?.place.map(|(town, _, country)| (id, (town, country)))).collect();
    (places, looking)
}

pub async fn library(State(server): State<Arc<Server>>, Signed(me): Signed, Path(id): Path<i64>) -> Result<Json<Value>, Fail> {
    let library = server.home.libraries(&me.name).into_iter().find(|library| library.id == id)
        .ok_or_else(|| Fail(StatusCode::NOT_FOUND, "There is no such library.".into()))?;
    let answer = tokio::task::spawn_blocking(move || {
        let photos = server.home.files(library.id);
        let times: Vec<i64> = photos.iter().map(crate::home::when).collect();
        let own = positions(&server, &photos.iter().collect::<Vec<_>>());
        let paths: Vec<PathBuf> = photos.into_iter().map(|photo| photo.path).collect();
        names_for(&server, &own, &paths, &times)
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
        println!("{folder}: {:?}", from_positions(&read(&paths)));
    }
}
