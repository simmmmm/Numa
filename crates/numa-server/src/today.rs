use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

use axum::extract::{Query, State};
use axum::Json;
use numa_io::catalog::{library_of, Flag};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::auth::Signed;
use crate::home::when;
use crate::{Fail, Server};

pub const NEWS: i64 = 14 * 86_400;

const YEARS: usize = 6;

const ARRIVALS: usize = 3;
const STRIP: usize = 6;

#[derive(Deserialize)]
pub struct Asked {

    day: Option<String>,
}

fn on_this_day(photos: impl IntoIterator<Item = (i64, i64, u8, Flag)>, today: &str) -> Vec<(i64, Vec<i64>)> {
    let (year, month_day) = today.split_at(today.len() - 6);
    let year: i64 = year.parse().unwrap_or(0);
    let mut years: BTreeMap<i64, Vec<(bool, i64, i64)>> = BTreeMap::new();
    for (id, at, rating, flag) in photos {
        let day = numa_io::import::day(at, 0);
        let (taken, on) = day.split_at(day.len() - 6);
        let Ok(taken) = taken.parse::<i64>() else { continue };
        if on == month_day && taken < year && flag != Flag::Rejected {
            years.entry(taken).or_default().push((flag != Flag::Picked && rating < 3, at, id));
        }
    }
    years
        .into_iter()
        .rev()
        .take(YEARS)
        .map(|(year, mut photos)| {
            photos.sort_unstable();
            (year, photos.into_iter().map(|(.., id)| id).collect())
        })
        .collect()
}

fn valid(day: &str) -> bool {
    let month_day = day.split_once('-').and_then(|(month, day)| Some((month.parse::<u32>().ok()?, day.parse::<u32>().ok()?)));
    day.len() == 5 && day.as_bytes()[2] == b'-' && month_day.is_some_and(|(month, day)| (1..=12).contains(&month) && (1..=31).contains(&day))
}

pub async fn today(State(server): State<Arc<Server>>, Signed(me): Signed, Query(asked): Query<Asked>) -> Result<Json<Value>, Fail> {
    let (_, date) = crate::night::local();
    let date = match asked.day {
        Some(day) if valid(&day) => format!("{}-{day}", &date[..4]),
        Some(_) => return Err(Fail::bad("A day is its month and day, as 10-18.")),
        None => date,
    };
    let faces = crate::faces::waiting(&server, &me.name).await?;

    let tidy = crate::tidy::mine(&server, &me, false).await?;
    let working = server.clone();
    let answer = tokio::task::spawn_blocking(move || {
        let server = working;
        let mine = server.home.sent_folder(&me.name);
        let libraries = server.home.libraries(&me.name);
        let named: HashMap<i64, (String, bool)> = libraries.iter().map(|library| (library.id, (library.label(), library.path == mine))).collect();
        let arrived = server.store.arrived(crate::store::now() - NEWS);
        let (mut photos, mut taken, mut came) = (0, Vec::new(), Vec::new());
        for library in &libraries {

            let shelf = server.home.photos(&[library.id]);
            photos += shelf.len();
            let new: Vec<i64> = shelf.iter().map(|photo| photo.id).filter(|id| arrived.contains_key(id)).collect();
            if let Some(at) = new.iter().map(|id| arrived[id]).max() {
                came.push((at, library.id, new));
            }
            taken.extend(shelf.iter().map(|photo| (photo.id, when(photo), photo.rating, photo.flag)));
        }
        came.sort_unstable_by(|a, b| b.cmp(a));
        let arrived: Vec<Value> = came
            .into_iter()
            .take(ARRIVALS)
            .map(|(at, library, ids)| {
                let (name, mine) = &named[&library];
                json!({ "library": library, "name": name, "mine": mine, "count": ids.len(), "at": at, "ids": &ids[..ids.len().min(STRIP)] })
            })
            .collect();
        let year: i64 = date[..4].parse().unwrap_or(0);
        let on_this_day: Vec<Value> = on_this_day(taken, &date)
            .into_iter()
            .map(|(taken, ids)| {
                let mut from: Vec<i64> = ids.iter().map(|id| library_of(*id)).collect();
                from.sort_unstable();
                from.dedup();

                let (name, mine) = match from[..] {
                    [only] => named.get(&only).map_or((None, false), |(name, mine)| (Some(name.clone()), *mine)),
                    _ => (None, false),
                };
                json!({ "year": taken, "ago": year - taken, "count": ids.len(), "ids": ids, "libraries": from.len(), "name": name, "mine": mine })
            })
            .collect();
        let folders = if me.admin { crate::manage::new_folders(&server).len() } else { 0 };
        json!({
            "date": date, "photos": photos, "on_this_day": on_this_day, "arrived": arrived,
            "waiting": { "tidy": crate::tidy::waiting(&server, &me, tidy.as_ref()), "faces": faces, "folders": folders },
        })
    })
    .await
    .map_err(Fail::internal)?;
    Ok(Json(answer))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn on_this_day_is_earlier_years_newest_first_the_best_first() {
        let at = |text: &str| numa_io::track::iso(text).unwrap();
        let mut photos = vec![
            (1, at("2024-10-18T09:00:00Z"), 0, Flag::None),
            (2, at("2024-10-18T18:00:00Z"), 4, Flag::None),
            (3, at("2024-10-18T12:00:00Z"), 0, Flag::Picked),
            (4, at("2024-10-18T13:00:00Z"), 5, Flag::Rejected),
            (5, at("2024-10-19T12:00:00Z"), 5, Flag::Picked),
            (6, at("2026-10-18T08:00:00Z"), 5, Flag::Picked),
            (7, at("2021-10-18T23:59:00Z"), 0, Flag::None),
        ];

        photos.extend((2010..2018).map(|year| (100 + year, at(&format!("{year}-10-18T12:00:00Z")), 0, Flag::None)));
        let found = on_this_day(photos, "2026-10-18");

        assert_eq!(found[0], (2024, vec![3, 2, 1]));
        assert_eq!(found[1], (2021, vec![7]));
        let years: Vec<i64> = found.iter().map(|(year, _)| *year).collect();
        assert_eq!(years, [2024, 2021, 2017, 2016, 2015, 2014]);
        assert!(valid("02-29") && !valid("2-29") && !valid("1-018") && !valid("13-01") && !valid("10-18x"));
    }
}
