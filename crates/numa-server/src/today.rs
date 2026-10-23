use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::{Query, State};
use axum::Json;
use numa_io::catalog::library_of;
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
        let on_this_day: Vec<Value> = numa_io::import::on_this_day(taken, &date, YEARS)
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

        let folders = match me.admin {
            true => {
                let left = server.store.left_out();
                let photos = &server.home.photos;
                crate::manage::new_folders(&server).iter().filter(|folder| !left.contains(&*folder.strip_prefix(photos).unwrap_or(folder).to_string_lossy())).count()
            }
            false => 0,
        };
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
    fn a_day_is_its_month_and_day() {
        assert!(valid("02-29") && !valid("2-29") && !valid("1-018") && !valid("13-01") && !valid("10-18x"));
    }
}
