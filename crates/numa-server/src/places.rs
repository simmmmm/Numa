use std::io::Read;
use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::Json;
use serde_json::{json, Value};

use crate::auth::Signed;
use crate::{Fail, Server};

struct Place {
    name: String,
    latitude: f32,
    longitude: f32,
    region: String,
    country: String,
    people: u64,
}

fn places() -> &'static [Place] {
    static PLACES: OnceLock<Vec<Place>> = OnceLock::new();
    PLACES.get_or_init(|| {
        let mut text = String::new();
        let packed: &[u8] = include_bytes!("../../../data/places/places.tsv.gz");
        if let Err(err) = flate2::read::GzDecoder::new(packed).read_to_string(&mut text) {
            log::warn!("places: {err}");
        }
        text.lines()
            .filter_map(|line| {
                let mut parts = line.split('\t');
                Some(Place {
                    name: parts.next()?.to_string(),
                    latitude: parts.next()?.parse().ok()?,
                    longitude: parts.next()?.parse().ok()?,
                    region: parts.next()?.to_string(),
                    country: parts.next()?.to_string(),
                    people: parts.next()?.parse().ok()?,
                })
            })
            .collect()
    })
}

fn distance((a_lat, a_lon): (f32, f32), (b_lat, b_lon): (f32, f32)) -> f32 {
    let x = (b_lon - a_lon).to_radians() * ((a_lat + b_lat) / 2.0).to_radians().cos();
    let y = (b_lat - a_lat).to_radians();
    6371.0 * (x * x + y * y).sqrt()
}

fn nearest(at: (f32, f32)) -> Option<&'static Place> {
    let (mut any, mut town): (Option<(f32, &Place)>, Option<&Place>) = (None, None);
    for place in places() {
        let far = distance(at, (place.latitude, place.longitude));
        if any.is_none_or(|(best, _)| far < best) {
            any = Some((far, place));
        }

        if town.is_none() && place.people >= 15_000 && far <= 15.0 {
            town = Some(place);
        }
    }
    town.or(any.filter(|(far, _)| *far <= 50.0).map(|(_, place)| place))
}

fn joined(names: &[&str]) -> String {
    match names {
        [] => String::new(),
        [one] => one.to_string(),
        [rest @ .., last] => format!("{} & {last}", rest.join(", ")),
    }
}

pub fn from_positions(positions: &[(f64, f64)]) -> Vec<String> {
    let found: Vec<&Place> = positions.iter().filter_map(|&(lat, lon)| nearest((lat as f32, lon as f32))).collect();
    if found.is_empty() {
        return Vec::new();
    }
    let n = found.len() as f32;
    let tally = |key: fn(&Place) -> &str| {
        let mut counts: Vec<(&str, usize)> = Vec::new();
        for place in &found {
            match counts.iter_mut().find(|(name, _)| *name == key(place)) {
                Some((_, count)) => *count += 1,
                None => counts.push((key(place), 1)),
            }
        }
        counts.sort_by_key(|(_, count)| std::cmp::Reverse(*count));
        counts
    };
    let (towns, regions, countries) = (tally(|place| &place.name), tally(|place| &place.region), tally(|place| &place.country));
    let share = |count: usize| count as f32 / n;
    let mut names = Vec::new();
    if share(towns[0].1) >= 0.6 {
        names.push(towns[0].0.to_string());
    } else {
        let main: Vec<&str> = towns.iter().filter(|(_, count)| share(*count) >= 0.15).take(3).map(|(name, _)| *name).collect();
        if !main.is_empty() {
            names.push(joined(&main));
        }
    }
    if share(regions[0].1) >= 0.8 && !regions[0].0.is_empty() {
        names.push(regions[0].0.to_string());
    }
    let lands: Vec<&str> = countries.iter().filter(|(_, count)| share(*count) >= 0.2).take(3).map(|(name, _)| *name).collect();
    if !lands.is_empty() {
        names.push(joined(&lands));
    }
    names.dedup();
    names.truncate(3);
    names
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
mod tests {
    use super::*;

    #[test]
    fn a_trip_is_named_after_its_towns_and_country() {

        let japan = [(35.690, 139.692), (35.680, 139.760), (35.011, 135.768), (35.003, 135.778), (34.694, 135.502)];
        assert_eq!(from_positions(&japan), ["Tokyo, Kyoto & Osaka", "Japan"]);

        let maastricht = [(50.851, 5.691), (50.849, 5.688)];
        assert_eq!(from_positions(&maastricht), ["Maastricht", "Limburg", "The Netherlands"]);

        let patagonia = [(-49.331, -72.886), (-49.331, -72.886), (-50.338, -72.264), (-51.729, -72.506)];
        let named = from_positions(&patagonia);
        assert_eq!(named.last().map(String::as_str), Some("Argentina & Chile"), "{named:?}");

        assert!(from_positions(&[(0.0, -30.0)]).is_empty());
        assert!(from_positions(&[]).is_empty());
    }
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
