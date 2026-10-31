use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::{Query, State};
use axum::Json;
use numa_io::catalog::{library_of, Flag};
use numa_io::import::{day, day_pick, day_seed};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::auth::Signed;
use crate::home::when;
use crate::{Fail, Server};

pub const NEWS: i64 = 14 * 86_400;

const YEARS: usize = 6;

const ARRIVALS: usize = 3;
const STRIP: usize = 6;

const HEROES: usize = 12;

#[derive(Deserialize)]
pub struct Asked {

    day: Option<String>,

    cameras: Option<String>,
    me: Option<String>,
}

fn valid(day: &str) -> bool {
    let month_day = day.split_once('-').and_then(|(month, day)| Some((month.parse::<u32>().ok()?, day.parse::<u32>().ok()?)));
    day.len() == 5 && day.as_bytes()[2] == b'-' && month_day.is_some_and(|(month, day)| (1..=12).contains(&month) && (1..=31).contains(&day))
}

fn ticked(asked: Option<&str>) -> Vec<String> {
    asked.unwrap_or_default().split('\u{1F}').filter(|key| !key.is_empty()).map(str::to_string).collect()
}

fn heroes(frames: &[(i64, i64, u8, Flag)], date: &str) -> Vec<i64> {
    let best = |&&(_, _, rating, flag): &&(i64, i64, u8, Flag)| flag == Flag::Picked || (rating >= 3 && flag != Flag::Rejected);
    let mut pool: Vec<i64> = frames.iter().filter(best).map(|frame| frame.0).collect();
    if pool.is_empty() {
        pool = frames.iter().filter(|frame| frame.3 != Flag::Rejected).map(|frame| frame.0).collect();
    }
    let seed = day_seed(date);
    let mut chosen = Vec::new();
    for turn in 0..pool.len().min(HEROES) as u32 {
        let mut at = day_pick(seed, 0, turn, pool.len());
        while chosen.contains(&pool[at]) {
            at = (at + 1) % pool.len();
        }
        chosen.push(pool[at]);
    }
    chosen
}

pub async fn today(State(server): State<Arc<Server>>, Signed(me): Signed, Query(asked): Query<Asked>) -> Result<Json<Value>, Fail> {
    let (_, date) = crate::night::local();
    let date = match asked.day {
        Some(day) if valid(&day) => format!("{}-{day}", &date[..4]),
        Some(_) => return Err(Fail::bad("A day is its month and day, as 10-18.")),
        None => date,
    };
    let faces = crate::faces::waiting(&server, &me).await?;
    let (cameras, own) = {
        let ticked = ticked(asked.cameras.as_deref());
        let who = asked.me.filter(|name| !name.is_empty());

        let admin = me.admin;
        server
            .home
            .ask(move |catalog| {
                let all = if admin { catalog.all_cameras() } else { Vec::new() };
                let keys: Vec<String> = all.iter().map(|(key, _)| key.clone()).collect();
                let bodies: Vec<Value> = all
                    .into_iter()
                    .map(|(key, photos)| json!({ "name": numa_io::clocks::camera_name(&key, &keys), "key": key, "photos": photos }))
                    .collect();
                (bodies, catalog.mine(&ticked, who.as_deref()))
            })
            .await?
    };

    let tidy = crate::tidy::mine(&server, &me, false).await?;
    let working = server.clone();
    let answer = tokio::task::spawn_blocking(move || {
        let server = working;
        let mine = server.home.sent_folder(&me.name);

        let named: HashMap<i64, (String, bool)> = server.home.libraries(&me).iter().map(|library| (library.id, (library.label(), library.path == mine))).collect();
        let libraries = server.home.reach(&me);
        let arrived = server.store.arrived(crate::store::now() - NEWS);
        let (mut photos, mut taken, mut came, mut shelves) = (0, Vec::new(), Vec::new(), Vec::new());
        for library in &libraries {

            let shelf = server.home.shown(&me, &[library.id]);
            photos += shelf.len();
            let new: Vec<i64> = shelf.iter().map(|photo| photo.id).filter(|id| arrived.contains_key(id)).collect();
            if let Some(at) = new.iter().map(|id| arrived[id]).max().filter(|_| named.contains_key(&library.id)) {
                came.push((at, library.id, new));
            }
            taken.extend(shelf.iter().filter(|photo| !own.is_set() || own.keeps(photo.id)).map(|photo| (photo.id, when(photo), photo.rating, photo.flag)));
            if let Some((name, mine)) = named.get(&library.id).filter(|_| !shelf.is_empty()) {
                let front: Vec<i64> = shelf.iter().take(3).map(|photo| photo.id).collect();
                shelves.push(json!({ "library": library.id, "name": name, "mine": mine, "count": shelf.len(), "ids": front }));
            }
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

        let at: HashMap<i64, i64> = taken.iter().map(|frame| (frame.0, frame.1)).collect();
        let heroes: Vec<Value> = heroes(&taken, &date)
            .into_iter()
            .map(|id| {
                let (name, mine) = named.get(&library_of(id)).map_or((None, false), |(name, mine)| (Some(name.clone()), *mine));
                json!({ "id": id, "name": name, "mine": mine, "date": day(at[&id], 0) })
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
            "date": date, "photos": photos, "cameras": cameras, "heroes": heroes, "on_this_day": on_this_day, "arrived": arrived, "libraries": shelves,
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

    #[test]
    fn the_bodies_asked_for_are_apart() {
        assert_eq!(ticked(Some("A|B|1\u{1F}C|D|")), ["A|B|1", "C|D|"]);
        assert!(ticked(Some("")).is_empty() && ticked(None).is_empty());
    }

    fn frame(id: i64, rating: u8, flag: Flag) -> (i64, i64, u8, Flag) {
        (id, 0, rating, flag)
    }

    #[test]
    fn the_photograph_of_the_day_is_one_of_the_best() {
        let frames = [frame(1, 0, Flag::None), frame(2, 4, Flag::None), frame(3, 0, Flag::Picked), frame(4, 5, Flag::Rejected)];
        let chosen = heroes(&frames, "2026-10-30");
        assert_eq!(chosen.len(), 2);
        assert!(chosen.contains(&2) && chosen.contains(&3));
        assert_eq!(chosen, heroes(&frames, "2026-10-30"));

        let plain = [frame(1, 0, Flag::None), frame(4, 5, Flag::Rejected)];
        assert_eq!(heroes(&plain, "2026-10-30"), [1], "with nothing marked, any but a Reject");
        assert!(heroes(&[], "2026-10-30").is_empty());

        let many: Vec<_> = (0..40).map(|id| frame(id, 3, Flag::None)).collect();
        let chosen = heroes(&many, "2026-10-30");
        assert_eq!(chosen.len(), HEROES);
        assert_eq!(chosen.iter().collect::<std::collections::HashSet<_>>().len(), HEROES);
        assert_ne!(chosen[0], heroes(&many, "2026-10-31")[0], "another day, another photograph");
    }
}
