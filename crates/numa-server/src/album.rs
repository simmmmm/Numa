use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::Json;
use numa_io::album_page::{is_grid, lay_out, short, town_at, Facts, Layout};
use numa_io::catalog::{library_of, AlbumChapter, AlbumKind, ChapterPlace, Photo};
use numa_io::chapters::{Shot, Town};
use serde::{Deserialize, Deserializer};
use serde_json::{json, Value};

use crate::auth::Signed;
use crate::grants::Level;
use crate::home::when;
use crate::map::Spot;
use crate::store::Account;
use crate::timeline::{chosen, seen, Marks, Wanted};
use crate::{Fail, Server};

const MOST_FOUND: usize = 8;

#[derive(Deserialize, Default)]
pub struct Narrow {
    flag: Option<String>,
    rating: Option<u8>,

    #[serde(skip)]
    pub(crate) link: bool,
}

impl Narrow {

    pub(crate) fn link() -> Self {
        Narrow { link: true, ..Narrow::default() }
    }
}

pub(crate) struct Loaded {
    key: String,
    pub name: String,
    pub kind: AlbumKind,
    may_change: bool,
    looking: bool,
    pub photos: Vec<Photo>,
    pub captions: HashMap<i64, String>,
    pub chapters: Vec<AlbumChapter>,
    pub layout: Layout,
}

fn shots_of(photos: &[Photo], spots: &HashMap<i64, Option<Spot>>, captions: &HashMap<i64, String>, memo: &mut HashMap<(u32, u32), Option<Town>>) -> Vec<Shot> {
    photos
        .iter()
        .map(|photo| Shot {
            id: photo.id,
            taken: when(photo),
            town: town_at(spots.get(&photo.id).copied().flatten().map(|spot| spot.at), memo),
            captioned: captions.contains_key(&photo.id),
            picked: crate::photos::flag_name(photo.flag) == "pick",
        })
        .collect()
}

async fn may_change(server: &Arc<Server>, me: &Account, key: &str) -> Result<bool, Fail> {
    match seen(server, me, key).await {
        Ok(()) => Ok(true),
        Err(Fail(StatusCode::NOT_FOUND, why)) => Err(Fail(StatusCode::NOT_FOUND, why)),
        Err(_) => Ok(false),
    }
}

async fn grid(server: &Arc<Server>, me: &Account, key: &str) -> Result<Option<Value>, Fail> {
    let may_change = may_change(server, me, key).await?;
    let asked = key.to_string();
    let (name, kind, chapters) = server
        .home
        .ask(move |catalog| -> Result<_, String> {
            let name = catalog.albums()?.into_iter().find(|(have, _)| *have == asked).map(|(_, name)| name).unwrap_or_default();
            Ok((name, catalog.album_kind(&asked)?, catalog.chapters(&asked)?.len()))
        })
        .await?
        .map_err(Fail::internal)?;
    if kind != AlbumKind::Album || chapters > 0 {
        return Ok(None);
    }
    let times: Vec<i64> = chosen(server, me, &Wanted::of_album(key, None, None)).await?.iter().map(when).collect();
    Ok(is_grid(kind, chapters, &times).then(|| {
        json!({ "key": key, "name": name, "kind": kind.name(), "may_change": may_change, "count": times.len(), "first": times.iter().min(), "last": times.iter().max(), "grid": true })
    }))
}

pub(crate) async fn load(server: &Arc<Server>, me: &Account, key: &str, narrow: &Narrow) -> Result<Loaded, Fail> {
    let may_change = may_change(server, me, key).await?;
    let filtered = narrow.flag.as_deref().is_some_and(|flag| !flag.trim().is_empty()) || narrow.rating.is_some_and(|stars| stars > 0);
    let mut photos = chosen(server, me, &Wanted::of_album(key, narrow.flag.clone(), narrow.rating)).await?;

    if narrow.link {
        photos.retain(|photo| me.sight.level(photo.id) >= Level::Edit && !numa_io::video::is_video(&photo.path));
    }
    let (album, ids): (String, HashSet<i64>) = (key.to_string(), photos.iter().map(|photo| photo.id).collect());
    let libraries: HashSet<i64> = ids.iter().map(|&id| library_of(id)).collect();
    let (name, kind, chapters, chapter_of, captions) = server
        .home
        .ask(move |catalog| -> Result<_, String> {
            let name = catalog.albums()?.into_iter().find(|(have, _)| *have == album).map(|(_, name)| name).unwrap_or_default();
            let mut captions: HashMap<i64, String> =
                libraries.iter().flat_map(|&library| catalog.captions(library).unwrap_or_default()).map(|(id, (text, _))| (id, text)).collect();
            captions.retain(|id, _| ids.contains(id));
            Ok((name, catalog.album_kind(&album)?, catalog.chapters(&album)?, catalog.chapter_of(&album)?, captions))
        })
        .await?
        .map_err(Fail::internal)?;

    let offering = may_change && !filtered && !narrow.link;
    let outside: Vec<Photo> = match offering && chapters.iter().any(|chapter| chapter.place.is_some()) {
        true => {
            let inside: HashSet<i64> = server.home.album(key).await?.into_iter().collect();
            let libraries: Vec<i64> = server.home.reach(me).iter().map(|library| library.id).collect();
            server.home.shown(me, &libraries).into_iter().filter(|photo| !inside.contains(&photo.id) && me.sight.level(photo.id) >= Level::Edit).collect()
        }
        false => Vec::new(),
    };
    let (trip, link, owned, server) = (kind == AlbumKind::Trip, narrow.link, key.to_string(), server.clone());
    tokio::task::spawn_blocking(move || {
        let (spots, reading) = crate::map::spots(&server, &photos);
        let (further, reading_outside) = match outside.is_empty() {
            true => (HashMap::new(), false),
            false => crate::map::spots(&server, &outside),
        };
        let mut memo = HashMap::new();
        let shots = shots_of(&photos, &spots, &captions, &mut memo);
        let others = shots_of(&outside, &further, &HashMap::new(), &mut memo);
        let unread: HashSet<i64> = photos.iter().chain(&outside).map(|photo| photo.id).filter(|id| !spots.contains_key(id) && !further.contains_key(id)).collect();
        let layout = lay_out(&Facts { shots: &shots, chapter_of: &chapter_of, chapters: &chapters, outside: &others, unread: &unread, trip, may_change: may_change && !link, filtered });
        Loaded { key: owned, name, kind, may_change, looking: reading || reading_outside, photos, captions, chapters, layout }
    })
    .await
    .map_err(Fail::internal)
}

pub(crate) fn place_row(place: &ChapterPlace) -> Value {
    json!({ "name": place.name, "position": [place.latitude, place.longitude] })
}

fn chapter_row(chapter: &AlbumChapter) -> Value {
    json!({
        "key": chapter.key, "name": chapter.name, "words": chapter.words,
        "place": chapter.place.as_ref().map(place_row), "days": chapter.days.map(|(first, last)| [first, last]),
    })
}

fn said(server: &Server, loaded: &Loaded) -> Value {
    let marks = Marks::of(server, loaded.photos.iter());
    let photos: HashMap<i64, &Photo> = loaded.photos.iter().map(|photo| (photo.id, photo)).collect();

    let tiles = |ids: &[i64]| -> Vec<Value> {
        ids.iter()
            .filter_map(|id| photos.get(id))
            .map(|photo| {
                let mut row = marks.shaped(photo);
                if let Some(caption) = loaded.captions.get(&photo.id) {
                    row["caption"] = json!(caption);
                }
                row
            })
            .collect()
    };
    let name_of = |key: &str| loaded.chapters.iter().find(|chapter| chapter.key == key).map(|chapter| chapter.name.as_str());
    let layout = &loaded.layout;
    let chapters: Vec<Value> = loaded
        .chapters
        .iter()
        .zip(&layout.chapters)
        .map(|(chapter, laid)| {
            let offers: Vec<Value> = laid
                .offers
                .iter()
                .map(|offer| {
                    let after = offer.after.as_deref().and_then(name_of);
                    json!({ "why": offer.why, "count": offer.ids.len(), "ids": offer.ids, "first": offer.first, "last": offer.last, "after": after })
                })
                .collect();
            let mut row = chapter_row(chapter);
            row["count"] = json!(laid.count);
            row["first"] = json!(laid.first);
            row["last"] = json!(laid.last);
            row["shown"] = json!(tiles(&laid.shown));
            row["offers"] = json!(offers);
            row
        })
        .collect();
    let days: Vec<Value> = layout
        .days
        .iter()
        .map(|day| json!({ "day": numa_io::import::day(day.day * 86_400, 0), "places": day.towns, "count": day.count, "shown": tiles(&day.shown) }))
        .collect();
    let places: Vec<Value> = layout
        .places
        .iter()
        .map(|offered| {
            let town = &offered.town;
            json!({
                "name": town.name, "region": town.region, "country": town.country, "position": [town.latitude, town.longitude],
                "count": offered.ids.len(), "days": [offered.first_day, offered.last_day],
            })
        })
        .collect();
    json!({
        "key": loaded.key, "name": loaded.name, "kind": loaded.kind.name(), "may_change": loaded.may_change, "grid": false,
        "count": layout.count, "picks": layout.picks, "first": layout.first, "last": layout.last, "looking": loaded.looking,
        "chapters": chapters, "days": days, "loose": layout.loose, "places": places,
    })
}

pub async fn page(State(server): State<Arc<Server>>, Signed(me): Signed, Path(key): Path<String>, Query(narrow): Query<Narrow>) -> Result<Json<Value>, Fail> {
    if let Some(short) = grid(&server, &me, &key).await? {
        return Ok(Json(short));
    }
    let loaded = load(&server, &me, &key, &narrow).await?;
    let body = tokio::task::spawn_blocking(move || said(&server, &loaded)).await.map_err(Fail::internal)?;
    Ok(Json(body))
}

#[derive(Deserialize)]
pub struct Kind {
    kind: String,
}

pub async fn kind(State(server): State<Arc<Server>>, Signed(me): Signed, Path(key): Path<String>, Json(asked): Json<Kind>) -> Result<Json<Value>, Fail> {
    seen(&server, &me, &key).await?;
    let kind = AlbumKind::named(&asked.kind).ok_or_else(|| Fail::bad("An album is made an album, a trip or a wedding."))?;
    server.home.ask(move |catalog| catalog.set_album_kind(&key, kind)).await?.map_err(Fail::bad)?;
    Ok(Json(json!({ "kind": kind.name() })))
}

#[derive(Deserialize)]
pub struct PlaceIn {
    name: String,
    position: [f64; 2],
}

impl PlaceIn {
    fn kept(self) -> Result<ChapterPlace, Fail> {
        let [latitude, longitude] = self.position;
        if self.name.trim().is_empty() {
            return Err(Fail::bad("A place needs a name."));
        }
        if !(latitude.abs() <= 90.0 && longitude.abs() <= 180.0) {
            return Err(Fail::bad("A place's position is a latitude and a longitude."));
        }
        Ok(ChapterPlace { name: self.name.trim().to_string(), latitude, longitude })
    }
}

#[derive(Deserialize)]
pub struct NewChapter {
    name: String,
    place: Option<PlaceIn>,
    days: Option<[i64; 2]>,
}

pub async fn add_chapter(State(server): State<Arc<Server>>, Signed(me): Signed, Path(key): Path<String>, Json(asked): Json<NewChapter>) -> Result<Json<Value>, Fail> {
    seen(&server, &me, &key).await?;
    let place = asked.place.map(PlaceIn::kept).transpose()?;
    let days = asked.days.map(|[first, last]| (first, last));
    let chapter = server.home.ask(move |catalog| catalog.add_chapter(&key, &asked.name, place, days)).await?.map_err(Fail::bad)?;
    let mut row = chapter_row(&chapter);
    row["count"] = json!(0);
    row["first"] = Value::Null;
    row["last"] = Value::Null;
    Ok(Json(row))
}

async fn guarded(server: &Arc<Server>, me: &Account, key: &str) -> Result<AlbumChapter, Fail> {
    let asked = key.to_string();
    let chapter = server.home.ask(move |catalog| catalog.chapter(&asked)).await?.map_err(Fail::internal)?;
    let chapter = chapter.ok_or_else(|| Fail(StatusCode::NOT_FOUND, "There is no such chapter.".into()))?;
    seen(server, me, &chapter.album).await?;
    Ok(chapter)
}

pub async fn take_places(State(server): State<Arc<Server>>, Signed(me): Signed, Path(key): Path<String>) -> Result<Json<Value>, Fail> {
    seen(&server, &me, &key).await?;
    let loaded = load(&server, &me, &key, &Narrow::default()).await?;
    let places = loaded.layout.places;
    let added = places.len();
    let taken = server
        .home
        .ask(move |catalog| -> Result<Vec<i64>, String> {
            let mut taken = Vec::new();
            for offered in &places {
                let place = ChapterPlace { name: offered.town.name.clone(), latitude: offered.town.latitude, longitude: offered.town.longitude };
                let chapter = catalog.add_chapter(&key, &offered.town.name, Some(place), Some((offered.first_day, offered.last_day)))?;
                catalog.put_in_chapter(&key, Some(&chapter.key), &offered.ids)?;
                taken.extend(offered.ids.iter().copied());
            }
            Ok(taken)
        })
        .await?
        .map_err(Fail::bad)?;
    server.home.forget_albums();
    for id in &taken {
        server.store.touch(*id);
    }
    Ok(Json(json!({ "added": added })))
}

fn present<'de, T: Deserialize<'de>, D: Deserializer<'de>>(from: D) -> Result<Option<Option<T>>, D::Error> {
    Option::<T>::deserialize(from).map(Some)
}

#[derive(Deserialize)]
pub struct Change {
    name: Option<String>,
    words: Option<String>,
    #[serde(default, deserialize_with = "present")]
    place: Option<Option<PlaceIn>>,
    #[serde(default, deserialize_with = "present")]
    days: Option<Option<[i64; 2]>>,
    position: Option<usize>,
}

pub async fn change_chapter(State(server): State<Arc<Server>>, Signed(me): Signed, Path(key): Path<String>, Json(asked): Json<Change>) -> Result<StatusCode, Fail> {
    guarded(&server, &me, &key).await?;
    let place = asked.place.map(|place| place.map(PlaceIn::kept).transpose()).transpose()?;
    let days = asked.days.map(|days| days.map(|[first, last]| (first, last)));
    server
        .home
        .ask(move |catalog| -> Result<(), String> {

            if let Some(days) = days {
                catalog.set_chapter_days(&key, days)?;
            }
            if let Some(name) = &asked.name {
                catalog.rename_chapter(&key, name)?;
            }
            if let Some(words) = &asked.words {
                catalog.set_chapter_words(&key, words)?;
            }
            if let Some(place) = place {
                catalog.set_chapter_place(&key, place)?;
            }
            asked.position.map_or(Ok(()), |at| catalog.move_chapter(&key, at))
        })
        .await?
        .map_err(Fail::bad)?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn remove_chapter(State(server): State<Arc<Server>>, Signed(me): Signed, Path(key): Path<String>) -> Result<StatusCode, Fail> {
    guarded(&server, &me, &key).await?;
    server.home.ask(move |catalog| catalog.remove_chapter(&key)).await?.map_err(Fail::bad)?;
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
pub struct Photos {
    ids: Vec<i64>,
    inside: bool,
}

pub async fn put_photos(State(server): State<Arc<Server>>, Signed(me): Signed, Path(key): Path<String>, Json(asked): Json<Photos>) -> Result<Json<Value>, Fail> {
    let chapter = guarded(&server, &me, &key).await?;
    if asked.ids.is_empty() {
        return Err(Fail::bad("Choose a photograph first."));
    }
    for id in &asked.ids {
        server.home.photo(&me, *id, Level::Edit)?;
    }
    if asked.inside && !crate::shared::may_add(&server.store, &me, &chapter.album) {
        return Err(Fail(StatusCode::FORBIDDEN, "You may look at this album, not add to it.".into()));
    }

    let before: HashSet<i64> = server.home.album(&chapter.album).await?.into_iter().collect();
    let mut new: Vec<i64> = asked.ids.iter().copied().filter(|id| !before.contains(id)).collect();
    new.sort_unstable();
    new.dedup();
    let (album, inside) = (chapter.album, asked.inside);
    let changed = server
        .home
        .ask(move |catalog| -> Result<Vec<i64>, String> {
            if inside {
                catalog.put_in_chapter(&album, Some(&key), &asked.ids)?;
                return Ok(asked.ids);
            }
            let there = catalog.chapter_of(&album)?;
            let out: Vec<i64> = asked.ids.into_iter().filter(|id| there.get(id) == Some(&key)).collect();
            catalog.put_in_chapter(&album, None, &out)?;
            Ok(out)
        })
        .await?
        .map_err(Fail::bad)?;
    server.home.forget_albums();

    let updated = changed.iter().map(|id| server.store.touch(*id)).max();
    match inside {
        true => Ok(Json(json!({ "updated": updated, "new": new }))),
        false => Ok(Json(json!({ "updated": updated }))),
    }
}

#[derive(Deserialize)]
pub struct Find {
    q: Option<String>,
}

pub async fn find_places(Signed(_): Signed, Query(asked): Query<Find>) -> Json<Vec<Value>> {
    let towns = numa_io::places::find(asked.q.as_deref().unwrap_or(""), MOST_FOUND);
    let row = |town: &numa_io::places::Place| {
        json!({ "name": town.name, "region": town.region, "country": town.country, "position": [short(town.latitude), short(town.longitude)] })
    };
    Json(towns.into_iter().map(row).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::share::tests::http::{account, ask, fixture};

    fn json_of(method: &str, path: &str, who: &str, body: Value) -> (u16, Value) {
        let body = if body.is_null() { String::new() } else { body.to_string() };
        let answer = ask(method, path, Some(who), &body, "");
        let said = if answer.body.is_empty() { Value::Null } else { serde_json::from_slice(&answer.body).unwrap_or(Value::Null) };
        (answer.status, said)
    }

    fn made(who: &str, name: &str, ids: &[i64]) -> String {
        let (status, said) = json_of("POST", "/api/v1/photos/album", who, json!({ "ids": ids, "name": name, "inside": true }));
        assert_eq!(status, 200, "{said}");
        said["album"].as_str().unwrap().to_string()
    }

    fn page(who: &str, key: &str) -> (u16, Value) {
        json_of("GET", &format!("/api/v1/albums/{key}/page"), who, Value::Null)
    }

    fn names(page: &Value) -> Vec<String> {
        page["chapters"].as_array().unwrap().iter().map(|chapter| chapter["name"].as_str().unwrap().to_string()).collect()
    }

    #[test]
    fn an_empty_album_is_its_makers_to_list_read_and_make_a_wedding() {
        let sam = &fixture().sam;
        let key = made(sam, "Empty Wedding Plans", &[]);
        let listed = ask("GET", "/api/v1/albums", Some(sam), "", "").json();
        let row = listed.as_array().unwrap().iter().find(|row| row["key"] == key.as_str()).expect("listed for its maker");
        assert_eq!((row["count"].as_i64(), row["kind"].as_str(), row["first"].is_null(), row["last"].is_null()), (Some(0), Some("album"), true, true));

        let (status, empty) = page(sam, &key);
        assert_eq!(status, 200, "{empty}");
        assert_eq!((empty["name"].as_str(), empty["kind"].as_str(), empty["may_change"].as_bool(), empty["count"].as_i64(), empty["loose"].as_i64()), (Some("Empty Wedding Plans"), Some("album"), Some(true), Some(0), Some(0)));
        assert!(empty["chapters"].as_array().unwrap().is_empty() && empty["days"].as_array().unwrap().is_empty() && empty["places"].as_array().unwrap().is_empty());

        let stranger = account("Album Stranger", Level::None);
        assert!(ask("GET", "/api/v1/albums", Some(&stranger), "", "").json().as_array().unwrap().iter().all(|row| row["key"] != key.as_str()), "not listed for a stranger");
        assert_eq!(page(&stranger, &key).0, 404);
        for (method, path, body) in [
            ("PUT", format!("/api/v1/albums/{key}/kind"), json!({ "kind": "wedding" })),
            ("POST", format!("/api/v1/albums/{key}/chapters"), json!({ "name": "Cake" })),
            ("POST", format!("/api/v1/albums/{key}/places"), json!({})),
        ] {
            assert_eq!(json_of(method, &path, &stranger, body).0, 404, "{method} {path}");
        }

        let (status, said) = json_of("PUT", &format!("/api/v1/albums/{key}/kind"), sam, json!({ "kind": "wedding" }));
        assert_eq!((status, said["kind"].as_str()), (200, Some("wedding")));
        let (_, wedding) = page(sam, &key);
        assert_eq!(names(&wedding), ["Getting Ready", "Ceremony", "Portraits", "Dinner", "First Dance", "Party"]);
        assert_eq!(wedding["kind"], "wedding");
        assert!(wedding["chapters"][0]["place"].is_null() && wedding["chapters"][0]["days"].is_null() && wedding["chapters"][0]["first"].is_null());
        assert_eq!(wedding["chapters"][0]["count"], 0);
        let kinds = ask("GET", "/api/v1/albums", Some(sam), "", "").json();
        assert_eq!(kinds.as_array().unwrap().iter().find(|row| row["key"] == key.as_str()).unwrap()["kind"], "wedding");
        assert_eq!(json_of("PUT", &format!("/api/v1/albums/{key}/kind"), sam, json!({ "kind": "boat" })).0, 400);

        let (status, added) = json_of("POST", &format!("/api/v1/albums/{key}/chapters"), sam, json!({ "name": "  Speeches " }));
        assert_eq!((status, added["name"].as_str(), added["count"].as_i64()), (200, Some("Speeches"), Some(0)));
        let chapter = added["key"].as_str().unwrap().to_string();
        let path = format!("/api/v1/chapters/{chapter}");
        assert_eq!(json_of("PUT", &path, &stranger, json!({ "name": "Mine" })).0, 404, "a stranger changes no chapter");
        assert_eq!(json_of("PUT", &path, sam, json!({ "name": "Toasts", "words": "  Three toasts. " })).0, 204);
        assert_eq!(json_of("PUT", &path, sam, json!({ "position": 0 })).0, 204);
        let (_, moved) = page(sam, &key);
        assert_eq!((names(&moved)[0].as_str(), moved["chapters"][0]["words"].as_str()), ("Toasts", Some("Three toasts.")));
        assert_eq!(names(&moved).len(), 7);

        let (status, refused) = json_of("POST", &format!("/api/v1/albums/{key}/chapters"), sam, json!({ "name": "   " }));
        assert_eq!((status, refused["error"].as_str()), (400, Some("A chapter needs a name.")));
        assert_eq!(json_of("PUT", &path, sam, json!({ "name": "x".repeat(81) })).0, 400);
        let (status, refused) = json_of("PUT", &path, sam, json!({ "days": [20_550, 20_548] }));
        assert_eq!((status, refused["error"].as_str()), (400, Some("A chapter's last day cannot come before its first.")));
        assert_eq!(json_of("PUT", &path, sam, json!({ "place": { "name": "Kyoto", "position": [35.02, 135.75] }, "days": [20_548, 20_550] })).0, 204);
        let (_, placed) = page(sam, &key);
        assert_eq!((placed["chapters"][0]["place"]["name"].as_str(), placed["chapters"][0]["days"].clone()), (Some("Kyoto"), json!([20_548, 20_550])));
        assert_eq!(json_of("PUT", &path, sam, json!({ "place": null, "days": null })).0, 204);
        let (_, plain) = page(sam, &key);
        assert!(plain["chapters"][0]["place"].is_null() && plain["chapters"][0]["days"].is_null());

        let (status, gone) = json_of("PUT", "/api/v1/chapters/ffffffffffffffff", sam, json!({ "name": "Nothing" }));
        assert_eq!((status, gone["error"].as_str()), (404, Some("There is no such chapter.")));
        assert_eq!(json_of("DELETE", &path, &stranger, Value::Null).0, 404);
        assert_eq!(json_of("DELETE", &path, sam, Value::Null).0, 204);
        assert_eq!(names(&page(sam, &key).1).len(), 6);
    }

    fn months(who: &str, query: &str) -> i64 {
        let months = ask("GET", &format!("/api/v1/timeline?{query}"), Some(who), "", "").json();
        months.as_array().unwrap().iter().map(|month| month["count"].as_i64().unwrap()).sum()
    }

    #[test]
    fn photographs_in_a_chapter_are_counted_shown_found_and_leave_with_the_album() {
        let fixture = fixture();
        let (sam, ids) = (&fixture.sam, fixture.ids);
        let key = made(sam, "Chapter Holiday", &ids);
        let (_, added) = json_of("POST", &format!("/api/v1/albums/{key}/chapters"), sam, json!({ "name": "Beach" }));
        let chapter = added["key"].as_str().unwrap().to_string();
        let photos = format!("/api/v1/chapters/{chapter}/photos");

        assert_eq!(json_of("POST", &photos, sam, json!({ "ids": [], "inside": true })).0, 400);
        let (status, said) = json_of("POST", &photos, sam, json!({ "ids": [ids[0], ids[1]], "inside": true }));
        assert_eq!(status, 200, "{said}");
        assert!(said["updated"].as_i64().is_some());
        assert_eq!(said["new"], json!([]), "all three were in the album already");
        let (_, whole) = page(sam, &key);
        assert_eq!((whole["count"].as_i64(), whole["loose"].as_i64()), (Some(3), Some(1)));
        let beach = &whole["chapters"][0];
        assert_eq!(beach["count"], 2);
        let shown: Vec<i64> = beach["shown"].as_array().unwrap().iter().map(|tile| tile["id"].as_i64().unwrap()).collect();
        assert!(shown.contains(&ids[0]) && shown.contains(&ids[1]) && shown.len() == 2, "{shown:?}");
        assert!(beach["shown"][0]["aspect"].as_f64().is_some(), "a tile with its shape");
        assert!(beach["first"].as_i64().is_some() && beach["first"].as_i64() <= beach["last"].as_i64());
        assert_eq!(whole["days"].as_array().unwrap().iter().map(|day| day["count"].as_i64().unwrap()).sum::<i64>(), 1, "the days are the photographs in no chapter");

        assert_eq!(months(sam, &format!("album={key}&chapter={chapter}")), 2);
        assert_eq!(months(sam, &format!("album={key}&chapter=none")), 1);
        assert_eq!(months(sam, &format!("album={key}")), 3);
        assert_eq!(months(sam, &format!("album={key}&chapter=ffff")), 0, "a chapter that is none of its");

        let (_, narrowed) = json_of("GET", &format!("/api/v1/albums/{key}/page?flag=pick"), sam, Value::Null);
        assert_eq!((narrowed["count"].as_i64(), narrowed["places"].as_array().map(Vec::len)), (Some(0), Some(0)));

        let looker = account("Album Looker", Level::View);
        let (status, seen) = page(&looker, &key);
        assert_eq!((status, seen["may_change"].as_bool(), seen["chapters"][0]["count"].as_i64()), (200, Some(false), Some(2)));
        assert!(seen["chapters"][0]["offers"].as_array().unwrap().is_empty());
        assert_eq!(json_of("POST", &photos, &looker, json!({ "ids": [ids[2]], "inside": true })).0, 403);
        assert_eq!(json_of("PUT", &format!("/api/v1/chapters/{chapter}"), &looker, json!({ "name": "Mine" })).0, 403);

        let listed = |who: &str| ask("GET", "/api/v1/albums", Some(who), "", "").json().as_array().unwrap().iter().find(|row| row["key"] == key.as_str()).cloned().unwrap();
        assert_eq!((listed(sam)["may_change"].as_bool(), listed(&looker)["may_change"].as_bool()), (Some(true), Some(false)));
        let stranger = account("Album Stranger Two", Level::None);
        for (method, path, body) in [
            ("GET", format!("/api/v1/albums/{key}/page"), Value::Null),
            ("POST", photos.clone(), json!({ "ids": [ids[2]], "inside": true })),
            ("PUT", format!("/api/v1/chapters/{chapter}"), json!({ "name": "Mine" })),
            ("DELETE", format!("/api/v1/chapters/{chapter}"), Value::Null),
        ] {
            assert_eq!(json_of(method, &path, &stranger, body).0, 404, "{method} {path}");
        }

        let (status, said) = json_of("POST", &photos, sam, json!({ "ids": [ids[1]], "inside": false }));
        assert_eq!((status, said["updated"].as_i64().is_some()), (200, true));
        let (_, out) = page(sam, &key);
        assert_eq!((out["count"].as_i64(), out["chapters"][0]["count"].as_i64(), out["loose"].as_i64()), (Some(3), Some(1), Some(2)));

        let (status, said) = json_of("POST", "/api/v1/photos/album", sam, json!({ "ids": [ids[0]], "album": key, "inside": false }));
        assert_eq!(status, 200, "{said}");
        let (_, left) = page(sam, &key);
        assert_eq!((left["count"].as_i64(), left["chapters"][0]["count"].as_i64()), (Some(2), Some(0)));
        assert_eq!(months(sam, &format!("album={key}&chapter={chapter}")), 0);

        let deleted = ask("DELETE", &format!("/api/v1/albums/{key}"), Some(sam), "", "");
        assert_eq!(deleted.status, 200);
        assert_eq!(page(sam, &key).0, 404);
        let (status, gone) = json_of("PUT", &format!("/api/v1/chapters/{chapter}"), sam, json!({ "name": "Back" }));
        assert_eq!((status, gone["error"].as_str()), (404, Some("There is no such chapter.")));
        assert_eq!(json_of("POST", &photos, sam, json!({ "ids": [ids[0]], "inside": true })).0, 404);
    }

    #[test]
    fn places_are_found_by_what_is_typed() {
        let sam = &fixture().sam;
        let (status, found) = json_of("GET", "/api/v1/places/find?q=kyo", sam, Value::Null);
        assert_eq!(status, 200);
        let found = found.as_array().unwrap();
        assert!(found.len() <= 8 && found.iter().any(|town| town["name"] == "Kyoto"), "{found:?}");
        let kyoto = found.iter().find(|town| town["name"] == "Kyoto").unwrap();
        assert!(kyoto["country"].as_str().is_some() && kyoto["position"].as_array().is_some_and(|at| at.len() == 2));
        assert!((kyoto["position"][0].as_f64().unwrap() - 35.02).abs() < 0.1, "{kyoto}");
        assert_eq!(json_of("GET", "/api/v1/places/find?q=k", sam, Value::Null).1, json!([]));
        assert_eq!(ask("GET", "/api/v1/places/find?q=kyo", None, "", "").status, 401);
    }

    #[test]
    fn a_plain_album_has_no_places_to_take() {
        let (sam, ids) = (&fixture().sam, fixture().ids);
        let key = made(sam, "Plain Places Holiday", &ids[..1]);
        let (status, said) = json_of("POST", &format!("/api/v1/albums/{key}/places"), sam, json!({}));
        assert_eq!((status, said["added"].as_i64()), (200, Some(0)));
    }

    #[test]
    fn a_put_in_a_chapter_says_which_photographs_were_new_to_the_album() {
        let (sam, ids) = (&fixture().sam, fixture().ids);
        let key = made(sam, "New To Album Holiday", &ids[..1]);
        let (_, added) = json_of("POST", &format!("/api/v1/albums/{key}/chapters"), sam, json!({ "name": "Hills" }));
        let path = format!("/api/v1/chapters/{}/photos", added["key"].as_str().unwrap());
        let (status, said) = json_of("POST", &path, sam, json!({ "ids": [ids[0], ids[1], ids[1]], "inside": true }));
        assert_eq!((status, said["new"].clone()), (200, json!([ids[1]])), "{said}");
        let (_, again) = json_of("POST", &path, sam, json!({ "ids": [ids[1]], "inside": true }));
        assert_eq!(again["new"], json!([]));
        let (_, out) = json_of("POST", &path, sam, json!({ "ids": [ids[1]], "inside": false }));
        assert!(out["updated"].as_i64().is_some() && out.get("new").is_none(), "{out}");
    }

    #[test]
    fn a_long_plain_album_without_chapters_is_the_grid_and_the_rest_is_not() {
        let (sam, ids) = (&fixture().sam, fixture().ids);
        let key = made(sam, "Short Plain Holiday", &ids);
        let (_, short) = page(sam, &key);
        assert_eq!((short["grid"].as_bool(), short["count"].as_i64()), (Some(false), Some(3)));
        assert!(short["chapters"].is_array() && short["days"].is_array());
    }

    #[test]
    fn what_a_photograph_carries_through_the_trash_includes_its_chapter() {
        let (sam, ids) = (&fixture().sam, fixture().ids);
        let key = made(sam, "Kept Chapter Holiday", &ids[..1]);
        let (_, added) = json_of("POST", &format!("/api/v1/albums/{key}/chapters"), sam, json!({ "name": "Sea" }));
        let chapter = added["key"].as_str().unwrap().to_string();
        assert_eq!(json_of("POST", &format!("/api/v1/chapters/{chapter}/photos"), sam, json!({ "ids": [ids[0]], "inside": true })).0, 200);
        let server = &fixture().server;
        let photo = server.home.any_photo(ids[0]).unwrap();
        let kept = server.home.blocking(move |catalog| crate::manage::keep(catalog, &photo, None)).unwrap();
        let said = serde_json::to_value(&kept).unwrap();
        assert!(said["chapters"].as_array().unwrap().contains(&json!(chapter)), "{said}");

        let (album, id) = (key.clone(), ids[0]);
        server.home.blocking(move |catalog| {
            catalog.put_in_chapter(&album, None, &[id]).unwrap();
            catalog.set_in_album(&album, &[id], false).unwrap();
        });
        let (path, gives) = (server.home.any_photo(ids[0]).unwrap().path, kept);
        server.home.blocking(move |catalog| crate::manage::give(catalog, id, &path, &gives));
        server.home.forget_albums();
        let (_, back) = page(sam, &key);
        assert_eq!((back["count"].as_i64(), back["chapters"][0]["count"].as_i64()), (Some(1), Some(1)));
        let old: crate::manage::Kept = serde_json::from_value(json!({ "rating": 0, "flag": "none", "edits": null, "albums": [] })).unwrap();
        assert_eq!(serde_json::to_value(&old).unwrap()["chapters"], json!([]), "a trash from before has none");
    }
}
