use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::{header, StatusCode};
use axum::response::Response;
use axum::routing::get;
use axum::{Json, Router};
use numa_io::catalog::{AlbumKind, Photo};
use rusqlite::params;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::album::{load, place_row, Loaded, Narrow};
use crate::auth::Signed;
use crate::grants::Level;
use crate::home::when;
use crate::share::{self, closed, file, paced, robots, Share, DAYS, MOST_DAYS};
use crate::store::{now, Account};
use crate::timeline::{seen, Marks};
use crate::{Fail, Server};

const SMALL: u32 = 1024;
const LARGE: u32 = 2048;

const WORLD: u8 = 6;

const DETAIL: u8 = 14;
const BESIDE: u8 = 10;

const AT_ONCE: f32 = 600.0;
const A_SECOND: f32 = 10.0;

struct Extra {
    route: bool,
    follows: bool,
    kept: Option<String>,
}

#[derive(Serialize, Deserialize)]
struct Kept {
    guest: Value,
    ids: Vec<i64>,
}

struct Open {
    share: Share,
    owner: Account,
    key: String,
    extra: Extra,
}

fn extra(server: &Server, id: i64) -> Result<Extra, Fail> {
    server
        .store
        .with(|conn| {
            conn.query_row("SELECT route, follows, kept FROM shares WHERE id = ?1", [id], |row| {
                Ok(Extra { route: row.get::<_, i64>(0)? != 0, follows: row.get::<_, i64>(1)? != 0, kept: row.get(2)? })
            })
        })
        .map_err(Fail::internal)
}

async fn still(server: &Arc<Server>, share: Share) -> Result<Open, Fail> {
    let key = share.album.clone().ok_or_else(closed)?;
    let owner = share::owner(server, &share.owner).await?;
    seen(server, &owner, &key).await.map_err(|_| closed())?;
    let asked = key.clone();
    let kind = server.home.ask(move |catalog| catalog.album_kind(&asked)).await?.map_err(Fail::internal)?;
    if kind != AlbumKind::Trip {
        return Err(closed());
    }
    let extra = extra(server, share.id)?;
    Ok(Open { share, owner, key, extra })
}

async fn open(server: &Arc<Server>, token: &str) -> Result<Open, Fail> {
    still(server, share::find_kind(&server.store, token, now(), "trip")?).await
}

struct Shown<'a> {
    marks: Marks,
    photos: HashMap<i64, &'a Photo>,
    captions: &'a HashMap<i64, String>,
    ids: Vec<i64>,
    taken: Vec<i64>,
}

impl<'a> Shown<'a> {
    fn of(server: &Server, loaded: &'a Loaded) -> Self {
        Shown {
            marks: Marks::of(server, loaded.photos.iter()),
            photos: loaded.photos.iter().map(|photo| (photo.id, photo)).collect(),
            captions: &loaded.captions,
            ids: Vec::new(),
            taken: Vec::new(),
        }
    }

    fn tiles(&mut self, shown: &[i64]) -> Vec<Value> {
        let mut rows = Vec::new();
        for photo in shown.iter().filter_map(|id| self.photos.get(id)).copied() {
            let mut row = json!({ "n": self.ids.len(), "aspect": self.marks.shaped(photo)["aspect"] });
            if let Some(caption) = self.captions.get(&photo.id) {
                row["caption"] = json!(caption);
            }
            self.ids.push(photo.id);
            self.taken.push(when(photo));
            rows.push(row);
        }
        rows
    }
}

fn told(server: &Server, loaded: &Loaded, by: &str, expires: i64, route: bool) -> Kept {
    let mut shown = Shown::of(server, loaded);
    let chapters: Vec<Value> = loaded
        .chapters
        .iter()
        .zip(&loaded.layout.chapters)
        .map(|(chapter, laid)| {
            let photos = shown.tiles(&laid.shown);
            json!({
                "name": chapter.name, "words": chapter.words, "place": chapter.place.as_ref().filter(|_| route).map(place_row),
                "days": chapter.days.map(|(first, last)| [first, last]), "count": photos.len(), "photos": photos,
            })
        })
        .collect();
    let days: Vec<Value> = loaded
        .layout
        .days
        .iter()
        .map(|day| {
            let places = if route { day.towns.clone() } else { Vec::new() };
            json!({ "day": numa_io::import::day(day.day * 86_400, 0), "places": places, "photos": shown.tiles(&day.shown) })
        })
        .collect();
    let positions: Vec<(f64, f64)> = loaded.chapters.iter().filter_map(|chapter| chapter.place.as_ref()).map(|place| (place.latitude, place.longitude)).collect();
    let guest = json!({
        "name": loaded.name, "by": by, "expires": expires, "route": route,
        "first": shown.taken.iter().min(), "last": shown.taken.iter().max(), "count": shown.ids.len(), "km": route.then(|| numa_io::album_page::route_km(&positions)),
        "chapters": chapters, "days": days,
    });
    Kept { guest, ids: shown.ids }
}

async fn lay(server: &Arc<Server>, owner: &Account, key: &str, by: String, expires: i64, route: bool) -> Result<Kept, Fail> {
    let loaded = load(server, owner, key, &Narrow::link()).await?;
    let server = server.clone();
    tokio::task::spawn_blocking(move || told(&server, &loaded, &by, expires, route)).await.map_err(Fail::internal)
}

async fn laid(server: &Arc<Server>, open: &Open) -> Result<Kept, Fail> {
    if open.extra.follows {
        return lay(server, &open.owner, &open.key, open.share.watermark.clone(), open.share.expires, open.extra.route).await;
    }
    open.extra.kept.as_deref().and_then(|text| serde_json::from_str(text).ok()).ok_or_else(closed)
}

async fn trip(State(server): State<Arc<Server>>, Path(token): Path<String>) -> Result<Response, Fail> {
    let share = share::find_kind(&server.store, &token, now(), "trip")?;

    paced(share.id, AT_ONCE, A_SECOND)?;
    let open = still(&server, share).await?;
    Ok(robots(Json(laid(&server, &open).await?.guest)))
}

#[derive(Deserialize)]
struct Edge {
    edge: Option<u32>,
}

async fn picture(State(server): State<Arc<Server>>, Path((token, n)): Path<(String, usize)>, Query(asked): Query<Edge>) -> Result<Response, Fail> {
    let share = share::find_kind(&server.store, &token, now(), "trip")?;
    paced(share.id, AT_ONCE, A_SECOND)?;
    let open = still(&server, share).await?;
    let id = *laid(&server, &open).await?.ids.get(n).ok_or_else(closed)?;

    let photo = server.home.photo(&open.owner, id, Level::Edit).map_err(|_| closed())?;
    let edits = crate::photos::edits(&server, &photo).await?;
    let edge = if asked.edge.unwrap_or(LARGE) <= SMALL { SMALL } else { LARGE };
    let _turn = server.decodes.acquire().await.map_err(Fail::internal)?;
    let share = open.share.id;
    let bytes = tokio::task::spawn_blocking(move || numa_io::thumbs::load(&photo.path, photo.mtime, edge, edits.as_deref()).and_then(|picture| crate::proof::jpeg(&picture, 85)))
        .await
        .map_err(Fail::internal)?

        .map_err(|err| {
            log::warn!("trip link {share}: {err}");
            Fail(StatusCode::UNPROCESSABLE_ENTITY, "Numa could not read this photograph.".into())
        })?;
    Ok(robots(([(header::CONTENT_TYPE, "image/jpeg")], bytes)))
}

fn tile_of((latitude, longitude): (f64, f64), z: u8) -> (u32, u32) {
    let n = f64::from(1u32 << z);
    let (x, y) = ((longitude + 180.0) / 360.0 * n, (1.0 - latitude.clamp(-85.0, 85.0).to_radians().tan().asinh() / std::f64::consts::PI) / 2.0 * n);
    (x.floor().clamp(0.0, n - 1.0) as u32, y.floor().clamp(0.0, n - 1.0) as u32)
}

pub(crate) fn may_send(places: &[(f64, f64)], z: u8, x: u32, y: u32) -> bool {
    match z {
        0..=WORLD => true,
        z if z <= DETAIL => places.iter().any(|&place| {
            let (at_x, at_y) = tile_of(place, z);
            let reach = u32::from(z >= BESIDE);
            at_x.abs_diff(x) <= reach && at_y.abs_diff(y) <= reach
        }),
        _ => false,
    }
}

async fn places_of(server: &Arc<Server>, open: &Open) -> Result<Vec<(f64, f64)>, Fail> {
    if open.extra.follows {
        let key = open.key.clone();
        let chapters = server.home.ask(move |catalog| catalog.chapters(&key)).await?.map_err(Fail::internal)?;
        return Ok(chapters.iter().filter_map(|chapter| chapter.place.as_ref()).map(|place| (place.latitude, place.longitude)).collect());
    }
    let kept = laid(server, open).await?;
    let at = |place: &Value| Some((place["position"][0].as_f64()?, place["position"][1].as_f64()?));
    Ok(kept.guest["chapters"].as_array().into_iter().flatten().filter_map(|chapter| at(&chapter["place"])).collect())
}

async fn tile(State(server): State<Arc<Server>>, Path((token, z, x, y)): Path<(String, u8, u32, u32)>) -> Result<Response, Fail> {
    let open = open(&server, &token).await?;
    if !open.extra.route {
        return Err(closed());
    }
    if !may_send(&places_of(&server, &open).await?, z, x, y) {
        return Ok(robots(StatusCode::NO_CONTENT));
    }
    let file = server.data.join(crate::map::FILE);
    let data = tokio::task::spawn_blocking(move || numa_io::map::tile(&file, z, x, y)).await.map_err(Fail::internal)?;
    Ok(match data {
        Some(data) if !data.is_empty() => robots(([(header::CONTENT_TYPE, "application/vnd.mapbox-vector-tile"), (header::CONTENT_ENCODING, "gzip")], data)),
        _ => robots(StatusCode::NO_CONTENT),
    })
}

#[derive(Deserialize)]
struct New {
    route: Option<bool>,
    follows: Option<bool>,
    days: Option<i64>,
}

async fn settle(server: &Arc<Server>, me: &Account, key: &str, id: i64, (route, follows): (bool, bool)) -> Result<(), Fail> {
    let kept = match follows {
        true => None,
        false => {
            let share = share::of(&server.store, id).ok_or_else(closed)?;
            let kept = lay(server, me, key, share.watermark, share.expires, route).await?;
            Some(serde_json::to_string(&kept).map_err(Fail::internal)?)
        }
    };
    server.store.with(|conn| conn.execute("UPDATE shares SET route = ?2, follows = ?3, kept = ?4 WHERE id = ?1", params![id, route, follows, kept])).map_err(Fail::internal)?;
    Ok(())
}

async fn create(State(server): State<Arc<Server>>, Signed(me): Signed, Path(key): Path<String>, Json(new): Json<New>) -> Result<Json<Value>, Fail> {
    seen(&server, &me, &key).await?;
    let asked = key.clone();
    let (kind, name) = server
        .home
        .ask(move |catalog| -> Result<_, String> {
            let name = catalog.albums()?.into_iter().find(|(have, _)| *have == asked).map(|(_, name)| name).unwrap_or_default();
            Ok((catalog.album_kind(&asked)?, name))
        })
        .await?
        .map_err(Fail::internal)?;
    if kind != AlbumKind::Trip {
        return Err(Fail::bad("Only a trip has a link of its own."));
    }
    let days = new.days.unwrap_or(DAYS).clamp(1, MOST_DAYS);
    let (id, token) = share::add_kind(&server.store, &me.name, &name, &me.name, Some(&key), &[], days, ("trip", 0, 0, None)).map_err(Fail::internal)?;
    if let Err(err) = settle(&server, &me, &key, id, (new.route.unwrap_or(true), new.follows.unwrap_or(true))).await {
        let _ = server.store.with(|conn| conn.execute("DELETE FROM shares WHERE id = ?1", [id]));
        return Err(err);
    }
    let expires = share::of(&server.store, id).map(|share| share.expires);
    Ok(Json(json!({ "id": id, "link": format!("/t/{token}"), "base": crate::reach::base(&server.store), "expires": expires })))
}

async fn list(State(server): State<Arc<Server>>, Signed(me): Signed, Path(key): Path<String>) -> Result<Json<Vec<Value>>, Fail> {
    seen(&server, &me, &key).await?;
    let rows = server
        .store
        .with(|conn| {
            let mut statement = conn.prepare(
                "SELECT id, made, expires, route, follows FROM shares
                 WHERE kind = 'trip' AND album = ?1 AND revoked IS NULL AND expires > ?2 AND (?3 OR owner = ?4) ORDER BY made DESC, id DESC",
            )?;
            let rows = statement.query_map(params![key, now(), me.admin, me.name], |row| {
                Ok(json!({ "id": row.get::<_, i64>(0)?, "made": row.get::<_, i64>(1)?, "expires": row.get::<_, i64>(2)?, "route": row.get::<_, i64>(3)? != 0, "follows": row.get::<_, i64>(4)? != 0 }))
            })?;
            rows.collect::<rusqlite::Result<Vec<Value>>>()
        })
        .map_err(Fail::internal)?;
    Ok(Json(rows))
}

pub fn routes() -> Router<Arc<Server>> {
    Router::new()
        .route("/t/{token}", get(|| async { file("text/html; charset=utf-8", include_bytes!("web/trip.html")) }))
        .route("/trip.js", get(|| async { file("text/javascript; charset=utf-8", include_bytes!("web/trip.js")) }))
        .route("/trip.css", get(|| async { file("text/css; charset=utf-8", include_bytes!("web/trip.css")) }))
        .route("/api/v1/t/{token}", get(trip))
        .route("/api/v1/t/{token}/{n}", get(picture))
        .route("/api/v1/t/{token}/map/{z}/{x}/{y}", get(tile))
        .route("/api/v1/albums/{key}/links", get(list).post(create))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::share::tests::http::{account, ask, fixture, gallery_by, get};

    const KYOTO: [f64; 2] = [35.0116, 135.7681];
    const TOKYO: [f64; 2] = [35.6895, 139.6917];

    fn json_of(method: &str, path: &str, who: &str, body: Value) -> (u16, Value) {
        let body = if body.is_null() { String::new() } else { body.to_string() };
        let answer = ask(method, path, Some(who), &body, "");
        let said = if answer.body.is_empty() { Value::Null } else { serde_json::from_slice(&answer.body).unwrap_or(Value::Null) };
        (answer.status, said)
    }

    fn a_trip(name: &str, ids: &[i64]) -> (String, Vec<String>) {
        let sam = &fixture().sam;
        let (status, album) = json_of("POST", "/api/v1/photos/album", sam, json!({ "ids": ids, "name": name, "inside": true }));
        assert_eq!(status, 200, "{album}");
        let key = album["album"].as_str().unwrap().to_string();
        assert_eq!(json_of("PUT", &format!("/api/v1/albums/{key}/kind"), sam, json!({ "kind": "trip" })).0, 200);
        let mut chapters = Vec::new();
        for (words, place) in [("Four days of walking.", KYOTO), ("", TOKYO)] {
            let (status, made) = json_of("POST", &format!("/api/v1/albums/{key}/chapters"), sam, json!({ "name": if place == KYOTO { "Kyoto" } else { "Tokyo" } }));
            assert_eq!(status, 200, "{made}");
            let chapter = made["key"].as_str().unwrap().to_string();
            let said = json!({ "words": words, "place": { "name": if place == KYOTO { "Kyoto" } else { "Tokyo" }, "position": place }, "days": [20_548, 20_550] });
            assert_eq!(json_of("PUT", &format!("/api/v1/chapters/{chapter}"), sam, said).0, 204);
            chapters.push(chapter);
        }
        assert_eq!(json_of("POST", &format!("/api/v1/chapters/{}/photos", chapters[0]), sam, json!({ "ids": &ids[..1], "inside": true })).0, 200);
        (key, chapters)
    }

    fn link(who: &str, key: &str, wanted: Value) -> (i64, String) {
        let (status, made) = json_of("POST", &format!("/api/v1/albums/{key}/links"), who, wanted);
        assert_eq!(status, 200, "{made}");
        assert!(made["expires"].as_i64().unwrap() > now());
        (made["id"].as_i64().unwrap(), made["link"].as_str().unwrap().strip_prefix("/t/").unwrap().to_string())
    }

    fn captions() -> std::sync::MutexGuard<'static, ()> {
        static CAPTIONS: std::sync::Mutex<()> = std::sync::Mutex::new(());
        CAPTIONS.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn not_open() -> Vec<u8> {
        get("/api/v1/t/not-a-token").body
    }

    fn assert_closed(path: &str) {
        let answer = get(path);
        assert_eq!((answer.status, answer.body), (404, not_open()), "{path}");
    }

    fn assert_ended(token: &str) {
        for path in [format!("/api/v1/t/{token}"), format!("/api/v1/t/{token}/0"), format!("/api/v1/t/{token}/map/3/1/2")] {
            assert_closed(&path);
        }
    }

    #[test]
    fn a_trips_link_reads_its_chapters_and_every_photograph_is_a_picture() {
        let fixture = fixture();

        let (key, _) = a_trip("Link Shape Holiday", &fixture.ids[..2]);
        let (_, token) = link(&fixture.sam, &key, json!({ "days": 7 }));
        let page = get(&format!("/t/{token}"));
        assert_eq!((page.status, page.header("x-robots-tag").is_some(), page.header("cache-control")), (200, true, Some("no-store")));
        let answer = get(&format!("/api/v1/t/{token}"));
        assert_eq!((answer.status, answer.header("cache-control"), answer.header("x-robots-tag").is_some()), (200, Some("no-store"), true));
        let trip = answer.json();
        assert_eq!((trip["name"].as_str(), trip["by"].as_str(), trip["route"].as_bool(), trip["count"].as_i64()), (Some("Link Shape Holiday"), Some("Sam"), Some(true), Some(2)));
        assert!(trip["expires"].as_i64().unwrap() <= now() + 7 * 86_400 && trip["first"].as_i64().is_some() && trip["last"].as_i64().is_some());

        assert!((350..400).contains(&trip["km"].as_i64().unwrap()), "{trip}");
        let chapters = trip["chapters"].as_array().unwrap();
        assert_eq!(chapters.len(), 2);
        assert_eq!((chapters[0]["name"].as_str(), chapters[0]["words"].as_str(), chapters[0]["days"].clone(), chapters[0]["count"].as_i64()), (Some("Kyoto"), Some("Four days of walking."), json!([20_548, 20_550]), Some(1)));
        assert_eq!((chapters[0]["place"]["name"].as_str(), chapters[0]["place"]["position"].clone()), (Some("Kyoto"), json!(KYOTO)));
        assert_eq!((chapters[0]["photos"][0]["n"].as_i64(), chapters[0]["photos"][0]["aspect"].is_number()), (Some(0), true));
        assert_eq!((chapters[1]["photos"].clone(), chapters[1]["count"].as_i64()), (json!([]), Some(0)), "what is ahead is listed");
        let days = trip["days"].as_array().unwrap();
        let loose: Vec<i64> = days.iter().flat_map(|day| day["photos"].as_array().unwrap()).map(|photo| photo["n"].as_i64().unwrap()).collect();
        assert_eq!(loose, [1], "the days go on where the chapters stopped");
        assert!(days[0]["day"].as_str().unwrap().len() == 10 && days[0]["places"].is_array());

        for n in 0..2 {
            let picture = get(&format!("/api/v1/t/{token}/{n}"));
            assert_eq!((picture.status, picture.header("content-type"), picture.header("cache-control")), (200, Some("image/jpeg"), Some("no-store")), "n = {n}");
            let long = image::load_from_memory(&picture.body).unwrap();
            assert_eq!(long.width().max(long.height()), 2048);
        }
        let small = image::load_from_memory(&get(&format!("/api/v1/t/{token}/0?edge=1024")).body).unwrap();
        assert_eq!(small.width().max(small.height()), 1024);
        let odd = image::load_from_memory(&get(&format!("/api/v1/t/{token}/0?edge=500")).body).unwrap();
        assert_eq!(odd.width().max(odd.height()), 1024, "anything else is one of the two");
        assert_closed(&format!("/api/v1/t/{token}/2"));
    }

    #[test]
    fn a_trips_link_says_no_names() {
        let fixture = fixture();
        let (key, chapters) = a_trip("Link Names Holiday", &fixture.ids);
        let _captions = captions();
        let caption = "From the hotel roof.";
        assert_eq!(json_of("PUT", &format!("/api/v1/photos/{}/caption", fixture.ids[0]), &fixture.sam, json!({ "caption": caption })).0, 200);
        for wanted in [json!({}), json!({ "route": false }), json!({ "follows": false }), json!({ "route": false, "follows": false })] {
            let (_, token) = link(&fixture.sam, &key, wanted.clone());
            let text = String::from_utf8(get(&format!("/api/v1/t/{token}")).body).unwrap();
            assert!(text.contains(caption), "the caption is the guest's: {wanted}");
            let mut hidden: Vec<String> = ["DSC_0001", "DSC_0002", "DSC_0003", "IMG_0100", "Annas Wedding", ".jpg", "/photos", "\"id\"", "\"key\""].iter().map(|text| text.to_string()).collect();
            hidden.push(key.clone());
            hidden.extend(chapters.clone());
            hidden.extend(fixture.ids.iter().map(|id| id.to_string()));
            for name in hidden {
                assert!(!text.contains(&name), "{name} in {wanted}: {text}");
            }
        }
        assert_eq!(json_of("PUT", &format!("/api/v1/photos/{}/caption", fixture.ids[0]), &fixture.sam, json!({ "caption": "" })).0, 200);
    }

    #[test]
    fn a_link_with_the_map_off_has_no_places_and_no_tiles() {
        let fixture = fixture();
        let (key, _) = a_trip("Link No Map Holiday", &fixture.ids);
        let (_, token) = link(&fixture.sam, &key, json!({ "route": false }));
        let answer = get(&format!("/api/v1/t/{token}"));
        let text = String::from_utf8(answer.body.clone()).unwrap();
        let trip = answer.json();
        assert_eq!((trip["route"].as_bool(), trip["km"].is_null()), (Some(false), true));
        assert!(!text.contains("position") && !text.contains("35.0") && !text.contains("135.7"), "{text}");
        for chapter in trip["chapters"].as_array().unwrap() {
            assert!(chapter["place"].is_null(), "{chapter}");
        }
        assert_eq!(trip["chapters"][0]["name"], "Kyoto", "a chapter made from a place is still called by it");
        assert_eq!(trip["chapters"][0]["days"], json!([20_548, 20_550]), "its days are dates");
        for day in trip["days"].as_array().unwrap() {
            assert_eq!(day["places"], json!([]));
        }
        assert_closed(&format!("/api/v1/t/{token}/map/3/1/2"));
        assert_closed(&format!("/api/v1/t/{token}/map/0/0/0"));
        let (_, with) = link(&fixture.sam, &key, json!({}));
        let on = get(&format!("/api/v1/t/{with}/map/3/1/2"));
        assert!(matches!(on.status, 200 | 204), "{}", on.status);
        assert_eq!(on.header("cache-control"), Some("no-store"));
    }

    #[test]
    fn a_link_sends_only_the_tiles_a_places_view_needs() {
        let places = [(35.0116, 135.7681)];
        for z in 0..=6 {
            assert!(may_send(&[], z, 0, 0), "the world at zoom {z}");
        }
        for z in [7, 9, 10, 14] {
            let (x, y) = tile_of(places[0], z);
            assert!(may_send(&places, z, x, y), "its own tile at {z}");
            assert!(!may_send(&[], z, x, y), "no place, no tile at {z}");
        }
        let (x, y) = tile_of(places[0], 10);
        assert!(may_send(&places, 10, x + 1, y) && may_send(&places, 10, x, y - 1) && may_send(&places, 10, x + 1, y + 1), "beside it at 10");
        let (x, y) = tile_of(places[0], 9);
        assert!(!may_send(&places, 9, x + 1, y) && !may_send(&places, 9, x, y + 1), "not beside it at 9");
        let (x, y) = tile_of(places[0], 14);
        assert!(may_send(&places, 14, x - 1, y + 1) && !may_send(&places, 14, x + 2, y), "one beside at 14, not two");
        for z in 7..=14 {
            assert!(!may_send(&places, z, 0, 0), "a far tile at {z}");
            let (x, y) = tile_of((-33.9, 151.2), z);
            assert!(!may_send(&places, z, x, y), "the other side of the world at {z}");
        }
        assert!(!may_send(&places, 15, 0, 0), "beyond the map's detail");
        assert_eq!(tile_of((0.0, 0.0), 1), (1, 1));
        assert_eq!(tile_of((89.0, -181.0), 3), (0, 0), "off the map is clamped to its edge");
    }

    #[test]
    fn kinds_of_links_do_not_cross() {
        let fixture = fixture();
        let (key, _) = a_trip("Link Kinds Holiday", &fixture.ids);
        let (_, token) = link(&fixture.sam, &key, json!({}));
        let (_, proof) = gallery_by(&fixture.sam, json!({ "ids": &fixture.ids[..2] }));
        assert_eq!(get(&format!("/api/v1/s/{proof}")).status, 200, "the proof opens as a proof");
        for path in [format!("/api/v1/s/{token}"), format!("/api/v1/s/{token}/0"), format!("/api/v1/c/{token}"), format!("/api/v1/t/{proof}"), format!("/api/v1/t/{proof}/0"), format!("/api/v1/t/{proof}/map/3/1/2")] {
            let (answer, unknown) = (get(&path), get(&path.replace(&token, "unknown").replace(&proof, "unknown")));
            assert_eq!((answer.status, answer.body), (unknown.status, unknown.body), "{path}");
        }
        assert_eq!(get(&format!("/api/v1/t/{token}")).status, 200);
    }

    #[test]
    fn a_link_ends_when_it_is_revoked_runs_out_or_its_trip_goes() {
        let fixture = fixture();
        let sam = &fixture.sam;
        let (key, _) = a_trip("Link Ends Holiday", &fixture.ids);
        let (revoked, a) = link(sam, &key, json!({}));
        let (late, b) = link(sam, &key, json!({}));
        let (_, c) = link(sam, &key, json!({ "follows": false }));
        for token in [&a, &b, &c] {
            assert_eq!(get(&format!("/api/v1/t/{token}/0")).status, 200);
        }
        assert_eq!(json_of("POST", &format!("/api/v1/shares/{revoked}/revoke"), sam, Value::Null).0, 204);
        fixture.server.store.with(|conn| conn.execute("UPDATE shares SET expires = ?2 WHERE id = ?1", params![late, now() - 1])).unwrap();
        assert_ended(&a);
        assert_ended(&b);

        assert_eq!(json_of("PUT", &format!("/api/v1/albums/{key}/kind"), sam, json!({ "kind": "album" })).0, 200);
        assert_ended(&c);
        assert_eq!(json_of("PUT", &format!("/api/v1/albums/{key}/kind"), sam, json!({ "kind": "trip" })).0, 200);
        assert_eq!(get(&format!("/api/v1/t/{c}")).status, 200);
        assert_eq!(json_of("DELETE", &format!("/api/v1/albums/{key}"), sam, Value::Null).0, 200);
        assert_ended(&c);
    }

    #[test]
    fn only_who_may_change_a_trip_makes_or_lists_its_links() {
        let fixture = fixture();
        let sam = &fixture.sam;
        let (key, _) = a_trip("Link Whose Holiday", &fixture.ids);
        let (id, _) = link(sam, &key, json!({ "route": false, "follows": false }));
        let path = format!("/api/v1/albums/{key}/links");
        let looker = account("Link Looker", Level::View);
        let editor = account("Link Editor", Level::Edit);
        let stranger = account("Link Stranger", Level::None);
        for who in [&looker, &editor] {
            assert_eq!(json_of("POST", &path, who, json!({})).0, 403);
            assert_eq!(json_of("GET", &path, who, Value::Null).0, 403, "another's links are not listed");
        }
        assert_eq!(json_of("POST", &path, &stranger, json!({})).0, 404);
        assert_eq!(json_of("GET", &path, &stranger, Value::Null).0, 404);
        assert_eq!(ask("POST", &path, None, "{}", "").status, 401);
        assert_eq!(json_of("POST", &format!("/api/v1/shares/{id}/revoke"), &looker, Value::Null).0, 404, "and ends none");

        let (status, listed) = json_of("GET", &path, sam, Value::Null);
        let listed = listed.as_array().unwrap().clone();
        assert_eq!((status, listed.len(), listed[0]["id"].as_i64(), listed[0]["route"].as_bool(), listed[0]["follows"].as_bool()), (200, 1, Some(id), Some(false), Some(false)));
        assert!(listed[0]["made"].is_number() && listed[0]["expires"].is_number());
        assert!(!listed[0].to_string().contains("token") && !listed[0].to_string().contains("/t/"), "never the token");

        let album = a_plain("Link Plain Album", &fixture.ids);
        let (status, refused) = json_of("POST", &format!("/api/v1/albums/{album}/links"), sam, json!({}));
        assert_eq!((status, refused["error"].as_str()), (400, Some("Only a trip has a link of its own.")));
        let (status, made) = json_of("POST", &path, sam, json!({ "days": 4000 }));
        assert_eq!(status, 200, "a longer wish is a year");
        assert!(made["expires"].as_i64().unwrap() <= now() + 365 * 86_400 + 5);
        assert_eq!(json_of("GET", &path, sam, Value::Null).1.as_array().unwrap().len(), 2);
        assert_eq!(json_of("POST", &format!("/api/v1/shares/{id}/revoke"), sam, Value::Null).0, 204);
        assert_eq!(json_of("GET", &path, sam, Value::Null).1.as_array().unwrap().len(), 1, "an ended link is not listed");
    }

    fn a_plain(name: &str, ids: &[i64]) -> String {
        let (status, album) = json_of("POST", "/api/v1/photos/album", &fixture().sam, json!({ "ids": ids, "name": name, "inside": true }));
        assert_eq!(status, 200, "{album}");
        album["album"].as_str().unwrap().to_string()
    }

    #[test]
    fn a_link_that_follows_shows_what_is_added_and_a_kept_one_does_not() {
        let fixture = fixture();
        let sam = &fixture.sam;
        let (key, _) = a_trip("Link Follows Holiday", &fixture.ids[..1]);
        let _captions = captions();
        let (_, follows) = link(sam, &key, json!({ "follows": true }));
        let (_, kept) = link(sam, &key, json!({ "follows": false }));
        let (before, kept_before) = (get(&format!("/api/v1/t/{follows}")).json(), get(&format!("/api/v1/t/{kept}")).json());
        assert_eq!((before["count"].as_i64(), before["chapters"].as_array().unwrap().len()), (Some(1), 2));
        assert_eq!(before["chapters"], kept_before["chapters"]);

        let (status, added) = json_of("POST", &format!("/api/v1/albums/{key}/chapters"), sam, json!({ "name": "Osaka" }));
        assert_eq!(status, 200);
        let osaka = added["key"].as_str().unwrap().to_string();
        assert_eq!(json_of("POST", &format!("/api/v1/chapters/{osaka}/photos"), sam, json!({ "ids": [fixture.ids[1]], "inside": true })).0, 200);

        let (now_follows, now_kept) = (get(&format!("/api/v1/t/{follows}")).json(), get(&format!("/api/v1/t/{kept}")).json());
        assert_eq!((now_follows["count"].as_i64(), now_follows["chapters"].as_array().unwrap().len()), (Some(2), 3));
        assert_eq!(now_follows["chapters"][2]["name"], "Osaka");
        assert_eq!(now_kept, kept_before, "kept is the trip as it was");
        assert_eq!(get(&format!("/api/v1/t/{follows}/1")).status, 200);
        assert_closed(&format!("/api/v1/t/{kept}/1"));
        assert_eq!(get(&format!("/api/v1/t/{kept}/0")).status, 200, "a kept link still serves its own n");

        assert!(matches!(get(&format!("/api/v1/t/{kept}/map/3/1/2")).status, 200 | 204));
    }

    #[test]
    fn a_link_leaves_out_what_its_owner_may_no_longer_edit() {
        let fixture = fixture();
        let owner = "Link Owner";
        let token = account(owner, Level::Edit);
        let (status, album) = json_of("POST", "/api/v1/photos/album", &token, json!({ "ids": &fixture.ids, "name": "Link Owner Holiday", "inside": true }));
        assert_eq!(status, 200, "{album}");
        let key = album["album"].as_str().unwrap().to_string();
        assert_eq!(json_of("PUT", &format!("/api/v1/albums/{key}/kind"), &token, json!({ "kind": "trip" })).0, 200);
        let (_, link_token) = link(&token, &key, json!({}));
        assert_eq!(get(&format!("/api/v1/t/{link_token}")).json()["count"], 3);

        fixture.server.store.set_grants(owner, &crate::grants::Grants { access: Level::View, ..Default::default() }).unwrap();
        let shrunk = get(&format!("/api/v1/t/{link_token}")).json();
        assert_eq!((shrunk["count"].as_i64(), shrunk["chapters"].as_array().unwrap().len()), (Some(0), 0), "{shrunk}");
        assert_closed(&format!("/api/v1/t/{link_token}/0"));
        fixture.server.store.set_grants(owner, &crate::grants::Grants { access: Level::Edit, ..Default::default() }).unwrap();
        assert_eq!(get(&format!("/api/v1/t/{link_token}")).json()["count"], 3, "and they are back with the right");

        fixture.server.store.with(|conn| conn.execute("UPDATE shares SET owner = 'Nobody Here' WHERE token = ?1", [crate::auth::digest(&link_token)])).unwrap();
        assert_ended(&link_token);
    }

    #[test]
    fn the_picture_carries_its_pixels_and_the_reservation_and_nothing_else() {
        let fixture = fixture();
        let (key, _) = a_trip("Link Picture Holiday", &fixture.ids);
        let (_, token) = link(&fixture.sam, &key, json!({}));
        let bytes = get(&format!("/api/v1/t/{token}/0")).body;
        assert_eq!(&bytes[..2], &[0xff, 0xd8], "a JPEG");
        let holds = |needle: &[u8]| bytes.windows(needle.len()).any(|window| window == needle);
        for needle in ["Exif", "GPS", "DSC_0001", "Annas Wedding", "Link Picture Holiday", "dc:description", "dc:subject", "xmp:Rating"] {
            assert!(!holds(needle.as_bytes()), "{needle}");
        }
        assert!(holds(b"DMI-PROHIBITED-AIMLTRAINING"), "the reservation every file Home sends carries");
    }

    #[test]
    fn a_link_asked_too_much_says_wait_and_is_not_closed() {
        let fixture = fixture();
        let (key, _) = a_trip("Link Pace Holiday", &fixture.ids);
        let (id, token) = link(&fixture.sam, &key, json!({}));
        assert_eq!(get(&format!("/api/v1/t/{token}")).status, 200);

        share::spend(id, 60.0 * A_SECOND);
        for path in [format!("/api/v1/t/{token}"), format!("/api/v1/t/{token}/0")] {
            let answer = get(&path);
            assert_eq!(answer.status, 429, "{path}");
            assert_ne!(answer.body, not_open(), "{path}");
        }

        assert!(matches!(get(&format!("/api/v1/t/{token}/map/0/0/0")).status, 200 | 204));
    }

    #[test]
    fn the_proofs_list_does_not_hold_a_trips_link() {
        let fixture = fixture();
        let (key, _) = a_trip("Link Proofs Holiday", &fixture.ids);
        let (id, _) = link(&fixture.sam, &key, json!({}));
        let proof = gallery_by(&fixture.sam, json!({ "ids": &fixture.ids[..1] }));
        let listed = ask("GET", "/api/v1/shares", Some(&fixture.sam), "", "").json();
        let ids: Vec<i64> = listed.as_array().unwrap().iter().map(|row| row["id"].as_i64().unwrap()).collect();
        assert!(!ids.contains(&id) && ids.contains(&proof.0), "{ids:?}");

        let (status, again) = json_of("POST", &format!("/api/v1/shares/{id}/link"), &fixture.sam, Value::Null);
        assert_eq!(status, 200);
        assert!(again["link"].as_str().unwrap().starts_with("/t/"), "{again}");
    }
}
