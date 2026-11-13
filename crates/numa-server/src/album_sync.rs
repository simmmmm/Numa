use std::collections::BTreeSet;
use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use numa_io::catalog::{AlbumChapter, AlbumKind, ChapterPlace};
use numa_io::home::album_sync::{digest, AlbumDoc, ChapterDoc, PlaceDoc, Sent};
use rusqlite::params;
use serde_json::{json, Value};

use crate::auth::Signed;
use crate::grants::Level;
use crate::store::{now, Account, Store};
use crate::timeline::seen;
use crate::{Fail, Server};

fn table(store: &Store) {
    let _ = store.with(|conn| conn.execute("CREATE TABLE IF NOT EXISTS albums_gone (key TEXT PRIMARY KEY, at INTEGER NOT NULL)", []));
}

pub(crate) fn gone(store: &Store, key: &str) {
    table(store);
    let _ = store.with(|conn| conn.execute("INSERT OR REPLACE INTO albums_gone (key, at) VALUES (?1, ?2)", params![key, now()]));
}

fn back(store: &Store, key: &str) {
    table(store);
    let _ = store.with(|conn| conn.execute("DELETE FROM albums_gone WHERE key = ?1", [key]));
}

fn all_gone(store: &Store) -> Vec<String> {
    table(store);
    store
        .with(|conn| {
            let mut statement = conn.prepare("SELECT key FROM albums_gone ORDER BY at")?;
            let rows = statement.query_map([], |row| row.get(0))?;
            rows.collect::<rusqlite::Result<Vec<String>>>()
        })
        .unwrap_or_default()
}

async fn doc(server: &Arc<Server>, me: &Account, key: &str) -> Result<AlbumDoc, Fail> {
    let asked = key.to_string();
    let (name, kind, chapters, chapter_of) = server
        .home
        .ask(move |catalog| -> Result<_, String> {
            let name = catalog.albums()?.into_iter().find(|(have, _)| *have == asked).map(|(_, name)| name).ok_or("There is no such album.")?;
            Ok((name, catalog.album_kind(&asked)?, catalog.chapters(&asked)?, catalog.chapter_of(&asked)?))
        })
        .await?
        .map_err(|why| Fail(StatusCode::NOT_FOUND, why))?;
    let mut chapters: Vec<ChapterDoc> = chapters
        .iter()
        .map(|chapter| ChapterDoc {
            key: chapter.key.clone(),
            name: chapter.name.clone(),
            words: chapter.words.clone(),
            place: chapter.place.as_ref().map(|place| PlaceDoc { name: place.name.clone(), position: [place.latitude, place.longitude] }),
            days: chapter.days.map(|(first, last)| [first, last]),
            photos: BTreeSet::new(),
        })
        .collect();
    for (photo, chapter) in chapter_of {
        if me.sight.level(photo) >= Level::View {
            if let Some(chapter) = chapters.iter_mut().find(|one| one.key == chapter) {
                chapter.photos.insert(photo.to_string());
            }
        }
    }
    Ok(AlbumDoc { id: key.to_string(), name, kind: kind.name().to_string(), chapters })
}

async fn list(State(server): State<Arc<Server>>, Signed(me): Signed) -> Result<Json<Value>, Fail> {
    let rows = crate::timeline::albums(State(server.clone()), Signed(me.clone())).await?.0;
    let mut albums = Vec::new();
    for row in rows {
        let Some(key) = row["key"].as_str() else { continue };

        let doc = match doc(&server, &me, key).await {
            Err(Fail(StatusCode::NOT_FOUND, _)) => continue,
            doc => doc?,
        };
        albums.push(json!({ "id": key, "name": doc.name, "kind": doc.kind, "may_change": row["may_change"], "digest": digest(&doc) }));
    }
    Ok(Json(json!({ "albums": albums, "gone": all_gone(&server.store) })))
}

async fn one(State(server): State<Arc<Server>>, Signed(me): Signed, Path(key): Path<String>) -> Result<Json<Value>, Fail> {

    match seen(&server, &me, &key).await {
        Err(Fail(StatusCode::NOT_FOUND, why)) => return Err(Fail(StatusCode::NOT_FOUND, why)),
        _ => {}
    }
    let doc = doc(&server, &me, &key).await?;
    let mut said = serde_json::to_value(&doc).map_err(Fail::internal)?;
    said["digest"] = json!(digest(&doc));
    Ok(Json(said))
}

fn kept(album: &str, doc: &ChapterDoc) -> AlbumChapter {
    AlbumChapter {
        key: doc.key.clone(),
        album: album.to_string(),
        name: doc.name.clone(),
        words: doc.words.clone(),
        place: doc.place.as_ref().map(|place| ChapterPlace { name: place.name.clone(), latitude: place.position[0], longitude: place.position[1] }),
        days: doc.days.map(|[first, last]| (first, last)),
    }
}

fn conflict(why: &str) -> Fail {
    Fail(StatusCode::CONFLICT, why.into())
}

async fn put(State(server): State<Arc<Server>>, Signed(me): Signed, Path(key): Path<String>, Json(sent): Json<Sent>) -> Result<Json<Value>, Fail> {
    let asked = key.clone();
    let exists = server.home.ask(move |catalog| catalog.albums().unwrap_or_default().iter().any(|(have, _)| *have == asked)).await?;
    if exists {
        seen(&server, &me, &key).await?;
        if sent.base.as_deref() != Some(digest(&doc(&server, &me, &key).await?).as_str()) {
            return Err(conflict("The album changed at Numa Home in between."));
        }
    } else {
        let (at, name) = (key.clone(), sent.name.clone());
        if !server.home.ask(move |catalog| catalog.create_album_keyed(&at, &name)).await?.map_err(Fail::bad)? {
            return Err(conflict("Another album at Numa Home has this name."));
        }
        let _ = server.store.claim_album(&key, &me.name);
        back(&server.store, &key);
    }

    let mine = |ids: &[String]| -> Vec<i64> { ids.iter().filter_map(|id| id.parse::<i64>().ok()).filter(|id| me.sight.level(*id) >= Level::Edit).collect() };
    let moves: Vec<(String, Vec<i64>, Vec<i64>)> = sent.chapters.iter().map(|chapter| (chapter.chapter.key.clone(), mine(&chapter.into), mine(&chapter.out))).collect();
    let chapters: Vec<AlbumChapter> = sent.chapters.iter().map(|chapter| kept(&key, &chapter.chapter)).collect();
    let (album, name, kind) = (key.clone(), sent.name.trim().to_string(), AlbumKind::named(&sent.kind).unwrap_or(AlbumKind::Album));
    let touched = server
        .home
        .ask(move |catalog| -> Result<Vec<i64>, String> {
            let now = catalog.albums()?.into_iter().find(|(have, _)| *have == album).map(|(_, name)| name).unwrap_or_default();
            if now != name {
                catalog.rename_album(&album, &name).map_err(String::from)?;
            }
            catalog.put_album_kind(&album, kind)?;
            for have in catalog.chapters(&album)? {
                if !chapters.iter().any(|chapter| chapter.key == have.key) {
                    catalog.remove_chapter(&have.key)?;
                }
            }
            for chapter in &chapters {
                catalog.put_chapter(&album, chapter)?;
            }
            catalog.order_chapters(&album, &chapters.iter().map(|chapter| chapter.key.clone()).collect::<Vec<_>>())?;
            let mut touched = Vec::new();
            for (chapter, _, out) in &moves {
                catalog.take_from_chapter(chapter, out)?;
                touched.extend(out);
            }
            for (chapter, into, _) in &moves {
                if !into.is_empty() {
                    catalog.put_in_chapter(&album, Some(chapter), into)?;
                    touched.extend(into);
                }
            }
            Ok(touched)
        })
        .await?
        .map_err(|why| conflict(&why))?;
    server.home.forget_albums();

    for id in touched {
        server.store.touch(id);
    }
    Ok(Json(json!({ "digest": digest(&doc(&server, &me, &key).await?) })))
}

pub fn routes() -> Router<Arc<Server>> {
    Router::new().route("/api/v1/sync/albums", get(list)).route("/api/v1/sync/albums/{key}", get(one).put(put))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::share::tests::http::{account, ask, fixture};
    use numa_io::catalog::{AtHome, Catalog, Filter, Flag};
    use numa_io::home::album_sync::{Done, Round};
    use numa_io::home::sync::LINK;

    fn json_of(method: &str, path: &str, who: &str, body: Value) -> (u16, Value) {
        let body = if body.is_null() { String::new() } else { body.to_string() };
        let answer = ask(method, path, Some(who), &body, "");
        let said = if answer.body.is_empty() { Value::Null } else { serde_json::from_slice(&answer.body).unwrap_or(Value::Null) };
        (answer.status, said)
    }

    fn device(tag: &str) -> (Catalog, [i64; 2]) {
        let dir = std::env::temp_dir().join(format!("numa-album-sync-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("lib")).unwrap();
        let catalog = Catalog::open(&dir.join("home.db")).unwrap();
        let library = catalog.add_library(&dir.join("lib")).unwrap();
        let ids = fixture().ids;
        let rows: Vec<AtHome> = (0..2).map(|n| AtHome { path: format!("p{n}.jpg"), id: ids[n].to_string(), size: 1, mtime: 1_775_304_000 + n as i64, taken: None, rating: 0, flag: Flag::None, caption: String::new() }).collect();
        catalog.add_at_home(library.id, &rows).unwrap();
        let photos: serde_json::Map<String, Value> = (0..2).map(|n| (format!("p{n}.jpg"), json!({ "id": ids[n].to_string(), "edits": "", "rating": 0, "flag": "none", "updated": 0 }))).collect();
        let link = json!({ "address": "home", "library": "1", "name": "lib", "since": 0, "photos": photos });
        catalog.set_library_setting(library.id, LINK, Some(&link.to_string())).unwrap();
        let here = catalog.photos(library.id, &Filter::default()).unwrap();
        let id = |name: &str| here.iter().find(|photo| photo.path.ends_with(name)).unwrap().id;
        (catalog, [id("p0.jpg"), id("p1.jpg")])
    }

    fn round(catalog: &Catalog, who: &str) -> Done {
        let said = |calls: Vec<numa_io::home::album_sync::Call>| -> Vec<(u16, String)> {
            calls.iter().map(|call| {
                let answer = ask(call.method, &format!("/api/v1/{}", call.path), Some(who), call.body.as_deref().unwrap_or(""), "");
                (answer.status, String::from_utf8_lossy(&answer.body).to_string())
            }).collect()
        };
        let (mut round, first) = Round::begin(catalog).unwrap();
        let listed = said(vec![first]);
        let docs = round.listed(catalog, listed[0].0, &listed[0].1).unwrap();
        let ups = round.fetched(catalog, &said(docs)).unwrap();
        round.answered(catalog, &said(ups)).unwrap()
    }

    fn sent(catalog: &Catalog, who: &str) -> bool {
        (0..4).any(|_| round(catalog, who).went >= 1)
    }

    fn key_of(catalog: &Catalog, name: &str) -> Option<String> {
        catalog.albums().unwrap().into_iter().find(|(_, have)| have == name).map(|(key, _)| key)
    }

    fn home_doc(who: &str, key: &str) -> Value {
        json_of("GET", &format!("/api/v1/sync/albums/{key}"), who, Value::Null).1
    }

    #[test]
    fn an_album_is_one_thing_on_two_devices_and_at_home() {
        let fixture = fixture();
        let sam = &fixture.sam;

        let (_, made) = json_of("POST", "/api/v1/photos/album", sam, json!({ "ids": &fixture.ids[..2], "inside": true, "name": "Sync Lisbon" }));
        let home = made["album"].as_str().unwrap().to_string();
        assert_eq!(json_of("PUT", &format!("/api/v1/albums/{home}/kind"), sam, json!({ "kind": "trip" })).0, 200);
        let (_, chapter) = json_of("POST", &format!("/api/v1/albums/{home}/chapters"), sam, json!({ "name": "Alfama" }));
        let alfama = chapter["key"].as_str().unwrap().to_string();
        assert_eq!(json_of("POST", &format!("/api/v1/chapters/{alfama}/photos"), sam, json!({ "ids": &fixture.ids[..1], "inside": true })).0, 200);

        let (a, a_ids) = device("a");
        let mine = a.create_album("sync lisbon").unwrap();
        a.set_in_album(&mine, &a_ids, true).unwrap();

        let done = round(&a, sam);
        assert!(done.came >= 1 && !done.before, "{done:?}");
        assert_eq!(a.albums().unwrap().iter().filter(|(_, name)| name.eq_ignore_ascii_case("sync lisbon")).count(), 1);
        assert_eq!(key_of(&a, "Sync Lisbon"), Some(mine.clone()), "its name is Home's, its key its own");
        assert_eq!(a.album_kind(&mine).unwrap(), AlbumKind::Trip);
        assert_eq!(a.chapter_of(&mine).unwrap().get(&a_ids[0]), Some(&alfama));
        assert_eq!(round(&a, sam).went, 0, "a second round sends nothing");

        a.set_chapter_words(&alfama, "The tram, then the stairs.").unwrap();
        let belem = a.add_chapter(&mine, "Belem", None, None).unwrap().key;
        a.add_to_chapter(&belem, &a_ids[1..]).unwrap();
        assert!(sent(&a, sam));
        let doc = home_doc(sam, &home);
        assert_eq!(doc["chapters"][0]["words"], "The tram, then the stairs.");
        assert_eq!((doc["chapters"][1]["key"].as_str(), doc["chapters"][1]["photos"].clone()), (Some(belem.as_str()), json!([fixture.ids[1].to_string()])));

        let (b, b_ids) = device("b");
        assert!(round(&b, sam).came >= 1);
        assert_eq!(key_of(&b, "Sync Lisbon"), Some(home.clone()));
        assert_eq!(b.chapters(&home).unwrap().iter().map(|chapter| chapter.name.clone()).collect::<Vec<_>>(), ["Alfama", "Belem"]);
        assert_eq!(b.chapter_of(&home).unwrap().get(&b_ids[1]), Some(&belem));
        assert_eq!(b.photos_everywhere(&Filter { album: Some(home.clone()), ..Filter::default() }).unwrap().len(), 2, "being in a chapter is being in the album");

        b.rename_album(&home, "Sync Lisboa").unwrap();
        b.set_chapter_words(&belem, "Pasteis.").unwrap();
        a.set_chapter_words(&belem, "The tower at dusk.").unwrap();
        assert!(sent(&b, sam));
        assert_eq!(home_doc(sam, &home)["name"], "Sync Lisboa");
        assert!(sent(&a, sam));
        assert_eq!(key_of(&a, "Sync Lisboa"), Some(mine.clone()));
        assert_eq!(a.chapter(&belem).unwrap().unwrap().words, "Pasteis.\n\nThe tower at dusk.");
        assert_eq!(home_doc(sam, &home)["chapters"][1]["words"], "Pasteis.\n\nThe tower at dusk.");

        let looker = account("Album Sync Looker", Level::View);
        let (c, _) = device("c");
        assert!(round(&c, &looker).came >= 1);
        c.set_chapter_words(&alfama, "Not mine to say.").unwrap();
        assert_eq!(round(&c, &looker).went, 0);
        assert_eq!(home_doc(sam, &home)["chapters"][0]["words"], "The tram, then the stairs.");
        assert_eq!(json_of("PUT", &format!("/api/v1/sync/albums/{home}"), &looker, json!({ "base": null, "name": "Theirs", "kind": "album", "chapters": [] })).0, 403);

        assert_eq!(json_of("PUT", &format!("/api/v1/sync/albums/{home}"), sam, json!({ "base": "stale", "name": "Other", "kind": "album", "chapters": [] })).0, 409);
        assert_eq!(home_doc(sam, &home)["name"], "Sync Lisboa");

        round(&b, sam);
        a.set_chapter_words(&alfama, "Changed after.").unwrap();
        assert_eq!(json_of("DELETE", &format!("/api/v1/albums/{home}"), sam, Value::Null).0, 200);
        assert!(round(&b, sam).came >= 1);
        assert_eq!(key_of(&b, "Sync Lisboa"), None);
        round(&a, sam);
        assert_eq!(key_of(&a, "Sync Lisboa"), Some(mine.clone()), "what was changed here is not thrown away");

        assert_eq!(home_doc(sam, &mine)["chapters"][0]["words"], "Changed after.");

        a.delete_album(&mine).unwrap();
        assert!(sent(&a, sam));
        assert_eq!(json_of("GET", &format!("/api/v1/sync/albums/{mine}"), sam, Value::Null).0, 404);
    }

    #[test]
    fn a_home_from_before_ends_the_round_with_nothing_done_and_nothing_failed() {
        let (catalog, ids) = device("before");
        let album = catalog.create_album("Sync Before").unwrap();
        catalog.set_in_album(&album, &ids, true).unwrap();
        for status in [404, 405] {
            let (mut round, first) = Round::begin(&catalog).unwrap();
            assert_eq!((first.method, first.path.as_str()), ("GET", "sync/albums"));
            assert!(round.listed(&catalog, status, "{\"error\":\"There is no such address in the API.\"}").unwrap().is_empty());
            assert!(round.fetched(&catalog, &[]).unwrap().is_empty(), "nothing is sent to a Home that does not know albums");
            assert!(round.answered(&catalog, &[]).unwrap().before);
        }
        assert!(catalog.albums_at_home().unwrap().is_empty());
    }

    #[test]
    fn an_album_made_on_a_device_goes_up_only_where_home_has_a_photograph_of_it_or_it_is_empty() {
        let sam = &fixture().sam;
        let (catalog, ids) = device("up");
        let planned = catalog.create_album("Sync Planned Trip").unwrap();
        catalog.set_album_kind(&planned, AlbumKind::Trip).unwrap();
        catalog.add_chapter(&planned, "Porto", Some(ChapterPlace { name: "Porto".into(), latitude: 41.15, longitude: -8.61 }), None).unwrap();
        let filled = catalog.create_album("Sync Filled").unwrap();
        catalog.set_in_album(&filled, &ids[..1], true).unwrap();
        assert_eq!(round(&catalog, sam).went, 2);
        assert_eq!(round(&catalog, sam).went, 0);
        let doc = home_doc(sam, &planned);
        assert_eq!((doc["kind"].as_str(), doc["chapters"][0]["place"]["name"].as_str()), (Some("trip"), Some("Porto")));
        assert_eq!(home_doc(sam, &filled)["name"], "Sync Filled");
        for key in [planned, filled] {
            assert_eq!(json_of("DELETE", &format!("/api/v1/albums/{key}"), sam, Value::Null).0, 200);
        }
    }
}
