use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use argon2::password_hash::rand_core::{OsRng, RngCore};
use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderName, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post, put};
use axum::{Json, Router};
use numa_io::catalog::Filter;
use rusqlite::{params, OptionalExtension};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::auth::{self, Signed};
use crate::proof;
use crate::grants::Level;
use crate::store::{now, Account, Store};
use crate::{Fail, Server};

const DAY: i64 = 86_400;

pub(crate) const DAYS: i64 = 30;
pub(crate) const MOST_DAYS: i64 = 365;
const MOST_PHOTOS: usize = 2000;

const ROBOTS: &str = "noindex, nofollow, noarchive, noimageindex, noai, noimageai";

pub fn tables(store: &Store) -> Result<(), String> {
    store.with(|conn| {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS shares (
                 id INTEGER PRIMARY KEY, token TEXT NOT NULL UNIQUE, owner TEXT NOT NULL COLLATE NOCASE,
                 title TEXT NOT NULL, watermark TEXT NOT NULL, album TEXT, photos TEXT NOT NULL,
                 made INTEGER NOT NULL, expires INTEGER NOT NULL, revoked INTEGER,
                 mark INTEGER NOT NULL UNIQUE, secret TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS share_picks (share INTEGER NOT NULL, photo INTEGER NOT NULL, PRIMARY KEY (share, photo));
             CREATE TABLE IF NOT EXISTS guest_files (
                 id INTEGER PRIMARY KEY, share INTEGER NOT NULL, name TEXT NOT NULL, guest TEXT NOT NULL, size INTEGER NOT NULL,
                 sha256 TEXT NOT NULL, added INTEGER NOT NULL, path TEXT NOT NULL, kept INTEGER NOT NULL DEFAULT 0);
             CREATE INDEX IF NOT EXISTS guest_files_share ON guest_files (share);",
        )?;

        for column in ["kind TEXT NOT NULL DEFAULT 'proof'", "max_files INTEGER", "max_bytes INTEGER", "dest INTEGER"] {
            let _ = conn.execute(&format!("ALTER TABLE shares ADD COLUMN {column}"), []);
        }

        for column in ["route INTEGER NOT NULL DEFAULT 1", "follows INTEGER NOT NULL DEFAULT 1", "kept TEXT"] {
            let _ = conn.execute(&format!("ALTER TABLE shares ADD COLUMN {column}"), []);
        }

        let _ = conn.execute("UPDATE guest_files SET kept = 0 WHERE kept = 2", []);
        Ok(())
    })
}

#[derive(Debug)]
pub struct Share {
    pub id: i64,
    pub owner: String,
    pub title: String,
    pub watermark: String,
    pub album: Option<String>,
    pub photos: Vec<i64>,
    pub made: i64,
    pub expires: i64,
    pub revoked: Option<i64>,
    pub mark: u32,
    pub secret: String,
    pub kind: String,
    pub max_files: i64,
    pub max_bytes: i64,
    pub dest: Option<i64>,
}

pub(crate) const COLUMNS: &str = "id, owner, title, watermark, album, photos, made, expires, revoked, mark, secret, kind, max_files, max_bytes, dest";

pub(crate) fn row(row: &rusqlite::Row) -> rusqlite::Result<Share> {
    Ok(Share {
        id: row.get(0)?,
        owner: row.get(1)?,
        title: row.get(2)?,
        watermark: row.get(3)?,
        album: row.get(4)?,
        photos: serde_json::from_str(&row.get::<_, String>(5)?).unwrap_or_default(),
        made: row.get(6)?,
        expires: row.get(7)?,
        revoked: row.get(8)?,
        mark: row.get::<_, i64>(9)? as u32,
        secret: row.get(10)?,
        kind: row.get(11)?,
        max_files: row.get::<_, Option<i64>>(12)?.unwrap_or(0),
        max_bytes: row.get::<_, Option<i64>>(13)?.unwrap_or(0),
        dest: row.get(14)?,
    })
}

impl Share {
    pub fn open(&self, at: i64) -> bool {
        self.revoked.is_none() && at < self.expires
    }
}

pub(crate) fn closed() -> Fail {
    Fail(StatusCode::NOT_FOUND, "This gallery is not open.".into())
}

pub fn find(store: &Store, token: &str, at: i64) -> Result<Share, Fail> {
    find_kind(store, token, at, "proof")
}

pub fn find_kind(store: &Store, token: &str, at: i64, kind: &str) -> Result<Share, Fail> {
    let digest = auth::digest(token);
    let share = store.with(|conn| conn.query_row(&format!("SELECT {COLUMNS} FROM shares WHERE token = ?1 AND kind = ?2"), params![digest, kind], row).optional()).map_err(Fail::internal)?;
    share.filter(|share| share.open(at)).ok_or_else(closed)
}

pub fn photo_at(store: &Store, token: &str, n: usize, at: i64) -> Result<(Share, i64), Fail> {
    let share = find(store, token, at)?;
    let id = *share.photos.get(n).ok_or_else(closed)?;
    Ok((share, id))
}

pub(crate) fn of(store: &Store, id: i64) -> Option<Share> {
    store.with(|conn| conn.query_row(&format!("SELECT {COLUMNS} FROM shares WHERE id = ?1"), [id], row).optional()).ok().flatten()
}

#[allow(clippy::too_many_arguments)]
pub fn add(store: &Store, owner: &str, title: &str, watermark: &str, album: Option<&str>, photos: &[i64], days: i64) -> Result<(i64, String), String> {
    add_kind(store, owner, title, watermark, album, photos, days, ("proof", 0, 0, None))
}

#[allow(clippy::too_many_arguments)]
pub fn add_kind(store: &Store, owner: &str, title: &str, watermark: &str, album: Option<&str>, photos: &[i64], days: i64, limits: (&str, i64, i64, Option<i64>)) -> Result<(i64, String), String> {
    let (kind, max_files, max_bytes, dest) = limits;
    let (token, digest) = auth::token();
    let (secret, _) = auth::token();
    let made = now();
    let photos = serde_json::to_string(photos).map_err(|err| err.to_string())?;

    for _ in 0..8 {
        let mark = OsRng.next_u32() as i64;
        let added = store.with(|conn| {
            conn.execute(
                "INSERT OR IGNORE INTO shares (token, owner, title, watermark, album, photos, made, expires, mark, secret, kind, max_files, max_bytes, dest)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
                params![digest, owner, title, watermark, album, photos, made, made + days * DAY, mark, secret, kind, max_files, max_bytes, dest],
            )?;
            Ok((conn.changes() == 1).then(|| conn.last_insert_rowid()))
        })?;
        if let Some(id) = added {
            return Ok((id, token));
        }
    }
    Err("no free mark".into())
}

fn key(store: &Store) -> Result<[u8; 32], Fail> {
    let (fresh, _) = auth::token();
    let hex = store
        .with(|conn| {
            conn.execute("INSERT OR IGNORE INTO settings (key, value) VALUES ('proof:key', ?1)", [fresh])?;
            conn.query_row("SELECT value FROM settings WHERE key = 'proof:key'", [], |row| row.get::<_, String>(0))
        })
        .map_err(Fail::internal)?;
    let bytes: Vec<u8> = (0..hex.len() / 2).filter_map(|at| u8::from_str_radix(hex.get(at * 2..at * 2 + 2)?, 16).ok()).collect();
    bytes.try_into().map_err(|_| Fail::internal("proof:key is not 32 bytes"))
}

pub(crate) fn limited(share: i64) -> Result<(), Fail> {
    paced(share, 120.0, 2.0)
}

pub(crate) fn paced(share: i64, most: f32, a_second: f32) -> Result<(), Fail> {
    let mut buckets = BUCKETS.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let (tokens, then) = buckets.get_or_insert_with(HashMap::new).entry(share).or_insert((most, Instant::now()));
    *tokens = (*tokens + then.elapsed().as_secs_f32() * a_second).min(most);
    *then = Instant::now();
    if *tokens < 1.0 {
        return Err(Fail(StatusCode::TOO_MANY_REQUESTS, "Too many pictures at once; wait a moment.".into()));
    }
    *tokens -= 1.0;
    Ok(())
}

static BUCKETS: Mutex<Option<HashMap<i64, (f32, Instant)>>> = Mutex::new(None);

#[cfg(test)]
pub(crate) fn spend(share: i64, ahead: f32) {
    let mut buckets = BUCKETS.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    buckets.get_or_insert_with(HashMap::new).insert(share, (-ahead, Instant::now()));
}

pub(crate) fn robots<R: IntoResponse>(response: R) -> Response {
    let mut response = response.into_response();
    response.headers_mut().insert(HeaderName::from_static("x-robots-tag"), HeaderValue::from_static(ROBOTS));
    response.headers_mut().insert(header::CACHE_CONTROL, HeaderValue::from_static("no-store"));
    response
}

pub(crate) fn file(kind: &'static str, body: &'static [u8]) -> Response {
    robots(([(header::CONTENT_TYPE, kind)], body))
}

pub(crate) async fn owner(server: &Server, name: &str) -> Result<Account, Fail> {
    let mut account = server.store.accounts().ok().and_then(|all| all.into_iter().find(|account| account.name == name)).ok_or_else(closed)?;
    account.sight = Arc::new(crate::grants::sight(server, &account).await?);
    Ok(account)
}

async fn gallery(State(server): State<Arc<Server>>, Path(token): Path<String>) -> Result<Response, Fail> {
    let share = find(&server.store, &token, now())?;
    let picks = picks(&server.store, share.id);
    let (owner, ids) = (owner(&server, &share.owner).await?, share.photos.clone());
    let aspects: Vec<Option<f32>> = ids.iter().map(|&id| server.home.photo(&owner, id, Level::Edit).ok().and_then(|photo| photo.aspect)).collect();
    let photos: Vec<Value> = ids.iter().zip(aspects).enumerate().map(|(n, (id, aspect))| json!({ "n": n, "aspect": aspect.unwrap_or(1.5), "picked": picks.contains(id) })).collect();
    Ok(robots(Json(json!({ "title": share.title, "by": share.watermark, "expires": share.expires, "photos": photos }))))
}

#[derive(Deserialize)]
struct Edge {
    edge: Option<u32>,
}

async fn picture(State(server): State<Arc<Server>>, Path((token, n)): Path<(String, usize)>, Query(asked): Query<Edge>) -> Result<Response, Fail> {
    let (share, id) = photo_at(&server.store, &token, n, now())?;
    limited(share.id)?;

    let photo = server.home.photo(&owner(&server, &share.owner).await?, id, Level::Edit).map_err(|_| closed())?;
    let edits = crate::photos::edits(&server, &photo).await?;
    let edge = if asked.edge.unwrap_or(proof::MAX_EDGE) <= numa_io::thumbs::STEPS[0] { numa_io::thumbs::STEPS[0] } else { proof::MAX_EDGE };
    let key = key(&server.store)?;
    let seed = OsRng.next_u32();
    let _turn = server.decodes.acquire().await.map_err(Fail::internal)?;
    let (bytes, width, height) = tokio::task::spawn_blocking(move || -> Result<_, String> {
        let picture = numa_io::thumbs::load(&photo.path, photo.mtime, edge, edits.as_deref())?;
        let layout = proof::seed_of(&[share.secret.as_bytes(), &id.to_le_bytes()]);
        let picture = proof::proof(picture, &share.watermark, layout, &key, share.mark);
        let bytes = proof::jpeg(&proof::scramble(&picture, seed), 85)?;
        Ok((bytes, picture.width(), picture.height()))
    })
    .await
    .map_err(Fail::internal)?

    .map_err(|err| {
        log::warn!("share {}: {err}", share.id);
        Fail(StatusCode::UNPROCESSABLE_ENTITY, "Numa could not read this photograph.".into())
    })?;
    let mut response = robots(([(header::CONTENT_TYPE, "image/jpeg")], bytes));
    let said = format!("{seed} {width} {height} {}", proof::TILE);
    response.headers_mut().insert(HeaderName::from_static("x-proof"), HeaderValue::from_str(&said).map_err(Fail::internal)?);
    Ok(response)
}

fn picks(store: &Store, share: i64) -> Vec<i64> {
    store
        .with(|conn| {
            let mut statement = conn.prepare("SELECT photo FROM share_picks WHERE share = ?1")?;
            let rows = statement.query_map([share], |row| row.get(0))?;
            rows.collect()
        })
        .unwrap_or_default()
}

#[derive(Deserialize)]
struct Picked {
    picked: bool,
}

async fn pick(State(server): State<Arc<Server>>, Path((token, n)): Path<(String, usize)>, Json(asked): Json<Picked>) -> Result<StatusCode, Fail> {
    let (share, id) = photo_at(&server.store, &token, n, now())?;
    let sql = match asked.picked {
        true => "INSERT OR IGNORE INTO share_picks (share, photo) VALUES (?1, ?2)",
        false => "DELETE FROM share_picks WHERE share = ?1 AND photo = ?2",
    };
    server.store.with(|conn| conn.execute(sql, params![share.id, id])).map_err(Fail::internal)?;
    Ok(StatusCode::NO_CONTENT)
}

fn said(share: &Share, picks: Vec<i64>) -> Value {
    json!({
        "id": share.id, "title": share.title, "watermark": share.watermark, "album": share.album,
        "count": share.photos.len(), "made": share.made, "expires": share.expires,
        "revoked": share.revoked.is_some(), "open": share.open(now()), "picks": picks,
    })
}

async fn list(State(server): State<Arc<Server>>, Signed(me): Signed) -> Result<Json<Vec<Value>>, Fail> {
    let shares = server
        .store
        .with(|conn| {
            let mut statement = conn.prepare(&format!("SELECT {COLUMNS} FROM shares WHERE owner = ?1 AND kind = 'proof' ORDER BY made DESC"))?;
            let rows = statement.query_map([&me.name], row)?;
            rows.collect::<rusqlite::Result<Vec<Share>>>()
        })
        .map_err(Fail::internal)?;
    Ok(Json(shares.iter().map(|share| said(share, picks(&server.store, share.id))).collect()))
}

#[derive(Deserialize)]
struct New {
    title: Option<String>,
    watermark: Option<String>,
    album: Option<String>,
    ids: Option<Vec<i64>>,
    days: Option<i64>,
}

async fn create(State(server): State<Arc<Server>>, Signed(me): Signed, Json(new): Json<New>) -> Result<Json<Value>, Fail> {
    let libraries: Vec<i64> = server.home.editable(&me).iter().map(|library| library.id).collect();
    let (mut photos, album_name) = match (&new.album, &new.ids) {
        (Some(album), _) => {
            let album = album.clone();
            server
                .home
                .ask(move |catalog| {
                    let filter = Filter { album: Some(album.clone()), ..Filter::default() };
                    let photos: Vec<_> = libraries.iter().flat_map(|&library| catalog.photos(library, &filter).unwrap_or_default()).collect();
                    let name = catalog.albums().unwrap_or_default().into_iter().find(|(key, _)| *key == album).map(|(_, name)| name);
                    (photos, name)
                })
                .await?
        }
        (None, Some(ids)) => (ids.iter().map(|&id| server.home.photo(&me, id, Level::Edit)).collect::<Result<Vec<_>, _>>()?, None),
        (None, None) => return Err(Fail::bad("Choose an album or photographs to share.")),
    };
    photos.retain(|photo| !numa_io::video::is_video(&photo.path));
    if photos.is_empty() || photos.len() > MOST_PHOTOS {
        return Err(Fail::bad(format!("A gallery holds 1 to {MOST_PHOTOS} photographs.")));
    }
    photos.sort_by_key(crate::home::when);
    let ids: Vec<i64> = photos.iter().map(|photo| photo.id).collect();
    let title = new.title.map(|title| title.trim().to_string()).filter(|title| !title.is_empty()).or(album_name).unwrap_or_else(|| "Proofs".into());
    let watermark = new.watermark.map(|text| text.trim().to_string()).filter(|text| !text.is_empty()).unwrap_or_else(|| format!("© {}", me.name));
    if title.chars().count() > 80 || watermark.chars().count() > 60 {
        return Err(Fail::bad("The title is at most 80 characters, the watermark 60."));
    }
    let days = new.days.unwrap_or(DAYS).clamp(1, MOST_DAYS);
    let (id, token) = add(&server.store, &me.name, &title, &watermark, new.album.as_deref(), &ids, days).map_err(Fail::internal)?;
    Ok(Json(json!({ "id": id, "link": format!("/s/{token}"), "base": crate::reach::base(&server.store) })))
}

pub(crate) fn own(server: &Server, me: &crate::store::Account, id: i64) -> Result<Share, Fail> {
    of(&server.store, id).filter(|share| share.owner.eq_ignore_ascii_case(&me.name)).ok_or_else(|| Fail(StatusCode::NOT_FOUND, "There is no such gallery.".into()))
}

async fn revoke(State(server): State<Arc<Server>>, Signed(me): Signed, Path(id): Path<i64>) -> Result<StatusCode, Fail> {
    own(&server, &me, id)?;
    server.store.with(|conn| conn.execute("UPDATE shares SET revoked = ?2 WHERE id = ?1 AND revoked IS NULL", params![id, now()])).map_err(Fail::internal)?;
    Ok(StatusCode::NO_CONTENT)
}

async fn relink(State(server): State<Arc<Server>>, Signed(me): Signed, Path(id): Path<i64>) -> Result<Json<Value>, Fail> {
    let share = own(&server, &me, id)?;
    if !share.open(now()) {
        return Err(closed());
    }
    let (token, digest) = auth::token();
    server.store.with(|conn| conn.execute("UPDATE shares SET token = ?2 WHERE id = ?1", params![id, digest])).map_err(Fail::internal)?;
    let at = match share.kind.as_str() {
        "collect" => "c",
        "trip" => "t",
        _ => "s",
    };
    Ok(Json(json!({ "link": format!("/{at}/{token}"), "base": crate::reach::base(&server.store) })))
}

async fn whose(State(server): State<Arc<Server>>, Signed(me): Signed, body: Bytes) -> Result<Json<Value>, Fail> {
    let key = key(&server.store)?;
    let _turn = server.decodes.acquire().await.map_err(Fail::internal)?;
    let found = tokio::task::spawn_blocking(move || -> Result<_, String> {
        let picture = image::load_from_memory(&body).map_err(|err| err.to_string())?.to_rgb8();
        Ok(crate::mark::read(&picture, &key))
    })
    .await
    .map_err(Fail::internal)?
    .map_err(|err| Fail::bad(format!("Numa Home could not read this picture: {err}")))?;
    let nothing = || Fail(StatusCode::NOT_FOUND, "Numa Home found no gallery's mark in this picture.".into());
    let (mark, clearly) = found.ok_or_else(nothing)?;
    let share = server
        .store
        .with(|conn| conn.query_row(&format!("SELECT {COLUMNS} FROM shares WHERE mark = ?1 AND kind = 'proof'"), [mark as i64], row).optional())
        .map_err(Fail::internal)?
        .filter(|share| me.admin || share.owner.eq_ignore_ascii_case(&me.name))
        .ok_or_else(nothing)?;
    let mut answer = said(&share, picks(&server.store, share.id));
    answer["owner"] = json!(share.owner);
    answer["clearly"] = json!(clearly);
    Ok(Json(answer))
}

pub fn routes() -> Router<Arc<Server>> {
    Router::new()
        .route("/s/{token}", get(|| async { file("text/html; charset=utf-8", include_bytes!("web/proof.html")) }))
        .route("/proof.js", get(|| async { file("text/javascript; charset=utf-8", include_bytes!("web/proof.js")) }))
        .route("/proof.css", get(|| async { file("text/css; charset=utf-8", include_bytes!("web/proof.css")) }))
        .route("/share.js", get(|| async { file("text/javascript; charset=utf-8", include_bytes!("web/share.js")) }))
        .route("/api/v1/s/{token}", get(gallery))
        .route("/api/v1/s/{token}/{n}", get(picture))
        .route("/api/v1/s/{token}/{n}/pick", put(pick))
        .route("/api/v1/shares", get(list).post(create))
        .route("/api/v1/shares/{id}/revoke", post(revoke))
        .route("/api/v1/shares/{id}/link", post(relink))
        .route("/api/v1/shares/whose", post(whose).layer(axum::extract::DefaultBodyLimit::max(64 << 20)))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    fn store() -> Store {
        let store = Store::open(std::path::Path::new(":memory:")).unwrap();
        tables(&store).unwrap();
        store
    }

    #[test]
    fn a_link_opens_only_its_own_photographs() {
        let store = store();
        let (_, mine) = add(&store, "Sam", "Wedding", "© Sam", None, &[11, 12], DAYS).unwrap();
        let (_, theirs) = add(&store, "Sam", "Portraits", "© Sam", None, &[99], DAYS).unwrap();
        assert_eq!(photo_at(&store, &mine, 1, now()).unwrap().1, 12);
        assert_eq!(photo_at(&store, &mine, 2, now()).unwrap_err().0, StatusCode::NOT_FOUND, "past its end");
        assert_eq!(photo_at(&store, &theirs, 0, now()).unwrap().1, 99);
        assert!(find(&store, &auth::digest(&mine), now()).is_err(), "the kept digest is no link");
        assert!(find(&store, "0000", now()).is_err());
    }

    #[test]
    fn ended_and_revoked_links_are_closed() {
        let store = store();
        let (id, token) = add(&store, "Sam", "Wedding", "© Sam", None, &[11], 1).unwrap();
        assert!(find(&store, &token, now()).is_ok());
        assert_eq!(find(&store, &token, now() + DAY + 1).unwrap_err().0, StatusCode::NOT_FOUND, "a day later it has ended");
        store.with(|conn| conn.execute("UPDATE shares SET revoked = ?2 WHERE id = ?1", params![id, now()])).unwrap();
        assert_eq!(photo_at(&store, &token, 0, now()).unwrap_err().0, StatusCode::NOT_FOUND, "revoked");
    }

    #[test]
    fn the_token_is_long_and_kept_only_as_its_hash() {
        let store = store();
        let (id, token) = add(&store, "Sam", "Wedding", "© Sam", None, &[11], DAYS).unwrap();
        let (_, other) = add(&store, "Sam", "Wedding", "© Sam", None, &[11], DAYS).unwrap();
        assert_ne!(token, other);
        assert_eq!(token.len(), 64, "32 bytes in hex; #57 asks at least 128 bits");
        assert!(token.bytes().all(|byte| byte.is_ascii_hexdigit()));
        let (kept, secret) = store.with(|conn| conn.query_row("SELECT token, secret FROM shares WHERE id = ?1", [id], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))).unwrap();
        assert_eq!(kept, auth::digest(&token));
        assert_ne!(secret, token);
        assert!(store.session(&auth::digest(&token)).is_none());
    }

    #[test]
    fn a_share_keeps_a_pace() {
        for _ in 0..120 {
            assert!(limited(i64::MAX).is_ok());
        }
        assert_eq!(limited(i64::MAX).unwrap_err().0, StatusCode::TOO_MANY_REQUESTS);
        assert!(limited(i64::MAX - 1).is_ok());
    }

    pub(crate) mod http {
        use super::*;
        use crate::grants::Grants;
        use std::io::{Read, Write};
        use std::net::{SocketAddr, TcpStream};
        use std::path::PathBuf;
        use std::sync::OnceLock;

        pub(crate) struct Answer {
            pub status: u16,
            pub head: String,
            pub body: Vec<u8>,
        }

        impl Answer {
            pub fn header(&self, name: &str) -> Option<&str> {
                self.head.lines().find_map(|line| {
                    let (key, value) = line.split_once(':')?;
                    key.eq_ignore_ascii_case(name).then(|| value.trim())
                })
            }

            pub fn json(&self) -> Value {
                serde_json::from_slice(&self.body).unwrap()
            }

            fn edge(&self) -> u32 {
                let said: Vec<u32> = self.header("x-proof").unwrap().split(' ').map(|part| part.parse().unwrap()).collect();
                said[1].max(said[2])
            }
        }

        pub(crate) struct Fixture {
            pub server: Arc<Server>,
            address: SocketAddr,
            pub sam: String,
            pub photos: PathBuf,

            pub ids: [i64; 3],
        }

        const FOLDER: &str = "Annas Wedding 2026";

        pub(crate) fn fixture() -> &'static Fixture {
            static FIXTURE: OnceLock<Fixture> = OnceLock::new();
            FIXTURE.get_or_init(|| {

                let dir = std::env::current_exe().unwrap().with_file_name("share-http");
                let _ = std::fs::remove_dir_all(&dir);
                let (photos, sent, data) = (dir.join("photos"), dir.join("sent"), dir.join("data"));
                for folder in [photos.join(FOLDER), sent.clone(), data.clone()] {
                    std::fs::create_dir_all(folder).unwrap();
                }

                std::env::set_var("XDG_DATA_HOME", &data);
                std::env::set_var("XDG_CACHE_HOME", data.join("cache"));
                let picture = image::RgbImage::from_fn(3000, 2000, |x, y| image::Rgb([(x / 12) as u8, (y / 8) as u8, ((x + y) / 20) as u8]));
                for name in ["DSC_0001.jpg", "DSC_0002.jpg", "DSC_0003.jpg", "IMG_0100.jpg"] {
                    picture.save(photos.join(FOLDER).join(name)).unwrap();
                }
                std::fs::write(photos.join(FOLDER).join("IMG_0100.mov"), clip(2)).unwrap();
                std::fs::write(photos.join(FOLDER).join("CLIP_0001.mp4"), clip(7)).unwrap();
                let store = Store::open(&data.join("server.db")).unwrap();
                tables(&store).unwrap();
                crate::shared::tables(&store).unwrap();
                assert!(store.add_account("Sam", "no password", true, true).unwrap());
                let server = Server::new(store, photos.clone(), sent, data).unwrap();
                let sam = session(&server, "Sam");
                crate::jobs::scan(&server, false);
                let libraries: Vec<i64> = server.home.all_libraries().iter().map(|library| library.id).collect();
                let id = |name: &str| server.home.photos(&libraries).iter().find(|photo| photo.path.ends_with(name)).unwrap().id;
                let ids = [id("DSC_0001.jpg"), id("DSC_0002.jpg"), id("DSC_0003.jpg")];
                let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
                let address = listener.local_addr().unwrap();
                listener.set_nonblocking(true).unwrap();
                let app = crate::router(server.clone()).into_make_service_with_connect_info::<SocketAddr>();
                std::thread::spawn(move || {
                    let runtime = tokio::runtime::Runtime::new().unwrap();
                    runtime.block_on(async move { axum::serve(tokio::net::TcpListener::from_std(listener).unwrap(), app).await.unwrap() });
                });
                Fixture { server, address, sam, photos, ids }
            })
        }

        fn clip(seconds: u32) -> Vec<u8> {
            let boxed = |kind: &[u8], content: &[u8]| [&((content.len() + 8) as u32).to_be_bytes()[..], kind, content].concat();
            let mut mvhd = vec![0u8; 100];
            mvhd[4..8].copy_from_slice(&3_800_000_000u32.to_be_bytes());
            mvhd[12..16].copy_from_slice(&1000u32.to_be_bytes());
            mvhd[16..20].copy_from_slice(&(seconds * 1000).to_be_bytes());
            let mut tkhd = vec![0u8; 84];
            tkhd[40..44].copy_from_slice(&0x0001_0000u32.to_be_bytes());
            tkhd[76..80].copy_from_slice(&(1920u32 << 16).to_be_bytes());
            tkhd[80..84].copy_from_slice(&(1080u32 << 16).to_be_bytes());
            let moov = [boxed(b"mvhd", &mvhd), boxed(b"trak", &boxed(b"tkhd", &tkhd))].concat();
            let body: Vec<u8> = (0..seconds * 500).map(|n| (n % 251) as u8).collect();
            [boxed(b"ftyp", b"isom\0\0\0\0isom"), boxed(b"mdat", &body), boxed(b"moov", &moov)].concat()
        }

        fn id_of(name: &str) -> i64 {
            let server = &fixture().server;
            let libraries = server.home.all_libraries();
            libraries.iter().flat_map(|library| server.home.files(library.id)).find(|photo| photo.path.ends_with(name)).unwrap().id
        }

        pub(crate) fn session(server: &Server, name: &str) -> String {
            let (token, digest) = auth::token();
            server.store.add_session(&digest, name, None, "127.0.0.1").unwrap();
            token
        }

        pub(crate) fn account(name: &str, access: Level) -> String {
            let server = &fixture().server;
            server.store.add_account(name, "no password", false, false).unwrap();
            server.store.set_grants(name, &Grants { access, ..Grants::default() }).unwrap();
            session(server, name)
        }

        pub(crate) fn ask(method: &str, path: &str, bearer: Option<&str>, body: &str, extra: &str) -> Answer {
            let mut stream = TcpStream::connect(fixture().address).unwrap();
            let signed = bearer.map(|token| format!("Authorization: Bearer {token}\r\n")).unwrap_or_default();
            write!(stream, "{method} {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nContent-Type: application/json\r\nContent-Length: {}\r\n{signed}{extra}\r\n{body}", body.len()).unwrap();
            let mut bytes = Vec::new();
            stream.read_to_end(&mut bytes).unwrap();
            let split = bytes.windows(4).position(|window| window == b"\r\n\r\n").unwrap();
            let head = String::from_utf8_lossy(&bytes[..split]).into_owned();
            Answer { status: head[9..12].parse().unwrap(), head, body: bytes[split + 4..].to_vec() }
        }

        pub(crate) fn raw(method: &str, path: &str, bearer: Option<&str>, body: &[u8]) -> Answer {
            let mut stream = TcpStream::connect(fixture().address).unwrap();
            let signed = bearer.map(|token| format!("Authorization: Bearer {token}\r\n")).unwrap_or_default();
            let mut request = format!("{method} {path} HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\nContent-Type: application/octet-stream\r\nContent-Length: {}\r\n{signed}\r\n", body.len()).into_bytes();
            request.extend_from_slice(body);
            stream.write_all(&request).unwrap();
            let mut bytes = Vec::new();
            stream.read_to_end(&mut bytes).unwrap();
            let split = bytes.windows(4).position(|window| window == b"\r\n\r\n").unwrap();
            let head = String::from_utf8_lossy(&bytes[..split]).into_owned();
            Answer { status: head[9..12].parse().unwrap(), head, body: bytes[split + 4..].to_vec() }
        }

        pub(crate) fn get(path: &str) -> Answer {
            ask("GET", path, None, "", "")
        }

        pub(crate) fn gallery_by(who: &str, wanted: Value) -> (i64, String) {
            let made = ask("POST", "/api/v1/shares", Some(who), &wanted.to_string(), "");
            assert_eq!(made.status, 200, "{}", String::from_utf8_lossy(&made.body));
            let made = made.json();
            (made["id"].as_i64().unwrap(), made["link"].as_str().unwrap().strip_prefix("/s/").unwrap().to_string())
        }

        fn gallery_of_two() -> (i64, String) {
            let fixture = fixture();
            gallery_by(&fixture.sam, json!({ "ids": &fixture.ids[..2] }))
        }

        #[test]
        fn a_video_is_streamed_in_ranges_to_whoever_may_see_it() {
            let fixture = fixture();
            let (clip, live, moving) = (id_of("CLIP_0001.mp4"), id_of("IMG_0100.jpg"), id_of("IMG_0100.mov"));
            let file = std::fs::read(fixture.photos.join(FOLDER).join("CLIP_0001.mp4")).unwrap();
            let path = format!("/api/v1/photos/{clip}/video");

            assert_eq!(get(&path).status, 401);
            let stranger = account("Video Stranger", Level::None);
            assert_eq!(ask("GET", &path, Some(&stranger), "", "").status, 404);
            let looker = account("Video Looker", Level::View);

            let whole = ask("GET", &path, Some(&looker), "", "");
            assert_eq!((whole.status, whole.header("content-type"), whole.header("accept-ranges")), (200, Some("video/mp4"), Some("bytes")));
            assert_eq!(whole.body, file);

            let part = ask("GET", &path, Some(&looker), "", "Range: bytes=100-199\r\n");
            assert_eq!(part.status, 206);
            assert_eq!(part.header("content-range"), Some(format!("bytes 100-199/{}", file.len()).as_str()));
            assert_eq!(part.body, file[100..200]);
            let tail = ask("GET", &path, Some(&looker), "", "Range: bytes=-16\r\n");
            assert_eq!((tail.status, tail.body), (206, file[file.len() - 16..].to_vec()));
            let open = ask("GET", &path, Some(&looker), "", &format!("Range: bytes={}-\r\n", file.len() - 4));
            assert_eq!((open.status, open.body), (206, file[file.len() - 4..].to_vec()));
            let outside = ask("GET", &path, Some(&looker), "", &format!("Range: bytes={}-\r\n", file.len()));
            assert_eq!((outside.status, outside.header("content-range")), (416, Some(format!("bytes */{}", file.len()).as_str())));

            assert_eq!(ask("GET", &format!("/api/v1/photos/{}/video", fixture.ids[0]), Some(&looker), "", "").status, 404, "a photograph is no video");

            let months: Vec<Value> = ask("GET", "/api/v1/timeline", Some(&looker), "", "").json().as_array().unwrap().clone();
            let tiles: Vec<Value> = months
                .iter()
                .flat_map(|month| ask("GET", &format!("/api/v1/timeline/{}", month["month"].as_str().unwrap()), Some(&looker), "", "").json().as_array().unwrap().clone())
                .collect();
            let tile = |id: i64| tiles.iter().find(|tile| tile["id"] == id);
            let film = tile(clip).expect("the video is listed");
            assert_eq!((film["video"].as_bool(), film["seconds"].as_f64()), (Some(true), Some(7.0)));
            assert!((film["aspect"].as_f64().unwrap() - 16.0 / 9.0).abs() < 0.01, "its shape, before any picture of it");
            assert!(tile(live).is_some_and(|still| still["live"] == true && still.get("video").is_none()));
            assert!(tile(moving).is_none(), "the moving half is the still's, not a tile");

            let hold = ask("GET", &format!("/api/v1/photos/{live}/live"), Some(&looker), "", "Range: bytes=0-31\r\n");
            assert_eq!(hold.status, 206);
            assert_eq!(hold.body, std::fs::read(fixture.photos.join(FOLDER).join("IMG_0100.mov")).unwrap()[..32]);
            assert_eq!(ask("GET", &format!("/api/v1/photos/{live}/live"), Some(&stranger), "", "").status, 404);
            assert_eq!(ask("GET", &format!("/api/v1/photos/{}/live", fixture.ids[0]), Some(&looker), "", "").status, 404, "a still that does not move");
            assert_eq!(ask("GET", &format!("/api/v1/photos/{moving}/live"), Some(&looker), "", "").status, 404);

            let poster = ask("GET", &format!("/api/v1/photos/{clip}/thumb?edge=256"), Some(&looker), "", "");
            assert_eq!((poster.status, poster.header("content-type")), (200, Some("image/jpeg")));
            assert!(poster.body.starts_with(&[0xFF, 0xD8]));
            assert_eq!(ask("GET", &format!("/api/v1/photos/{clip}"), Some(&looker), "", "").json()["video"]["seconds"], 7.0);

            let sam = &fixture.sam;
            assert_eq!(ask("POST", "/api/v1/shares", Some(sam), &json!({ "ids": [clip] }).to_string(), "").status, 400, "a gallery is of photographs");
            let step = |extra: &str| -> Vec<i64> {
                let library = numa_io::catalog::library_of(clip);
                ask("GET", &format!("/api/v1/libraries/{library}/photos?{extra}"), Some(&looker), "", "").json().as_array().unwrap().iter().map(|row| row["id"].as_i64().unwrap()).collect()
            };
            assert!(!step("since=0").contains(&clip), "a device that does not ask for videos gets photographs");
            assert!(step("since=0&videos=1").contains(&clip));
        }

        #[test]
        fn a_share_link_shows_no_video() {
            let fixture = fixture();
            let clip = id_of("CLIP_0001.mp4");
            let alone = ask("POST", "/api/v1/shares", Some(&fixture.sam), &json!({ "ids": [clip] }).to_string(), "");
            assert_eq!(alone.status, 400);
            let (_, token) = gallery_by(&fixture.sam, json!({ "ids": [clip, fixture.ids[0]] }));
            assert_eq!(get(&format!("/api/v1/s/{token}/0?edge=640")).status, 200);
            assert_eq!(get(&format!("/api/v1/s/{token}/1?edge=640")).status, 404, "the video did not join the gallery");
            for path in [format!("/api/v1/photos/{clip}/video"), format!("/api/v1/photos/{clip}/live"), format!("/api/v1/photos/{clip}/thumb")] {
                assert_eq!(crate::reach::verdict(crate::reach::Mode::Links, true, true, true, &path), crate::reach::Verdict::Closed, "{path}");
                assert_eq!(crate::reach::verdict(crate::reach::Mode::All, true, true, true, &path), crate::reach::Verdict::Allow, "{path}");
            }
        }

        #[test]
        fn hashes_are_of_the_file_for_whoever_sees_it() {
            let fixture = fixture();
            let path = fixture.server.home.photos(&fixture.server.home.all_libraries().iter().map(|library| library.id).collect::<Vec<_>>()).into_iter().find(|photo| photo.id == fixture.ids[0]).unwrap().path;
            let body = json!({ "ids": [fixture.ids[0]] }).to_string();
            let answer = ask("POST", "/api/v1/photos/hashes", Some(&fixture.sam), &body, "");
            assert_eq!(answer.status, 200, "{}", String::from_utf8_lossy(&answer.body));
            assert_eq!(answer.json()[0]["hash"].as_str(), numa_io::home::same::hash_here(&path).as_deref());
            assert_eq!(ask("POST", "/api/v1/photos/hashes", None, &body, "").status, 401);
            let stranger = account("Hash Stranger", Level::None);
            assert_eq!(ask("POST", "/api/v1/photos/hashes", Some(&stranger), &body, "").status, 404);
            let many = json!({ "ids": vec![fixture.ids[0]; numa_io::home::same::SAMPLE + 1] }).to_string();
            assert_eq!(ask("POST", "/api/v1/photos/hashes", Some(&fixture.sam), &many, "").status, 400);
        }

        #[test]
        fn revoke_stops_every_size() {
            let fixture = fixture();
            let (id, token) = gallery_of_two();
            let (grid, loupe) = (format!("/api/v1/s/{token}/0?edge=640"), format!("/api/v1/s/{token}/0?edge=2048"));
            let shown = get(&grid);
            assert_eq!(shown.status, 200);
            assert_eq!(shown.header("cache-control"), Some("no-store"), "or a browser or proxy keeps what was revoked");
            assert_eq!(get(&loupe).status, 200);

            let stranger = account("Stranger", Level::Edit);
            assert_eq!(ask("POST", &format!("/api/v1/shares/{id}/revoke"), Some(&stranger), "", "").status, 404);
            assert_eq!(get(&grid).status, 200, "not theirs to revoke");

            assert_eq!(ask("POST", &format!("/api/v1/shares/{id}/revoke"), Some(&fixture.sam), "", "").status, 204);
            for path in [&grid, &loupe, &format!("/api/v1/s/{token}")] {
                assert_eq!(get(path).status, 404, "{path}");
            }
            assert_eq!(ask("PUT", &format!("/api/v1/s/{token}/0/pick"), None, r#"{"picked":true}"#, "").status, 404);

            let (id, token) = gallery_of_two();
            let link = ask("POST", &format!("/api/v1/shares/{id}/link"), Some(&fixture.sam), "", "").json();
            let new = link["link"].as_str().unwrap().strip_prefix("/s/").unwrap().to_string();
            assert_eq!(get(&format!("/api/v1/s/{token}/0?edge=640")).status, 404, "the old link");
            assert_eq!(get(&format!("/api/v1/s/{new}/0?edge=640")).status, 200);
        }

        #[test]
        fn a_link_opens_no_photograph_by_id_and_nothing_signed() {
            let fixture = fixture();
            let (_, token) = gallery_of_two();
            let [one, _, outside] = fixture.ids;
            assert_eq!(get(&format!("/api/v1/s/{token}/{one}")).status, 404, "an id is not a place");
            assert_eq!(get(&format!("/api/v1/s/{token}/{outside}")).status, 404);
            assert_eq!(get(&format!("/api/v1/s/{token}/2")).status, 404, "one past the end");
            assert_eq!(ask("PUT", &format!("/api/v1/s/{token}/2/pick"), None, r#"{"picked":true}"#, "").status, 404);
            for path in [format!("/api/v1/s/{token}/original"), format!("/api/v1/s/{token}/zip"), format!("/api/v1/s/{token}/0/original"), format!("/api/v1/s/{token}/0/jpeg"), format!("/api/v1/s/{token}/0/zip")] {
                let answer = get(&path);
                assert!(answer.status >= 400 && answer.status != 500, "{path}: {}", answer.status);
                assert!(!answer.body.starts_with(&[0xFF, 0xD8]), "{path} sent a picture");
            }
            let signed = [
                format!("/api/v1/photos/{one}"),
                format!("/api/v1/photos/{one}/original"),
                format!("/api/v1/photos/{one}/jpeg?edge=8192"),
                format!("/api/v1/photos/{one}/thumb"),
                format!("/api/v1/photos/{one}/preview"),
                format!("/api/v1/photos/zip?ids={one}"),
                "/api/v1/timeline".to_string(),
                "/api/v1/people".to_string(),
                "/api/v1/places".to_string(),
                "/api/v1/albums".to_string(),
                "/api/v1/search?q=a".to_string(),
                "/api/v1/libraries".to_string(),
                "/api/v1/shares".to_string(),
            ];
            for path in signed {
                assert_eq!(ask("GET", &path, Some(&token), "", "").status, 401, "{path}, the link as a bearer");
                assert_eq!(ask("GET", &path, None, "", &format!("Cookie: {}={token}\r\n", auth::COOKIE)).status, 401, "{path}, the link as a cookie");
                assert_eq!(get(&path).status, 401, "{path}, nobody");
            }
        }

        #[test]
        fn the_edge_is_clamped() {
            let (_, token) = gallery_of_two();
            let at = |query: &str| {
                let answer = get(&format!("/api/v1/s/{token}/0{query}"));
                assert_eq!(answer.status, 200, "{query}");
                let (width, height) = image::load_from_memory(&answer.body).unwrap().to_rgb8().dimensions();
                assert!(width.max(height) <= answer.edge().div_ceil(proof::TILE) * proof::TILE);
                answer.edge()
            };
            for query in ["", "?edge=641", "?edge=2048", "?edge=2049", "?edge=8192", "?edge=4294967295"] {
                assert_eq!(at(query), proof::MAX_EDGE, "{query}");
            }
            for query in ["?edge=0", "?edge=1", "?edge=640"] {
                assert_eq!(at(query), 640, "{query}");
            }
            assert_eq!(get(&format!("/api/v1/s/{token}/0?edge=99999999999")).status, 400);
        }

        #[test]
        fn a_gallery_says_no_names() {
            let (_, token) = gallery_of_two();
            let gallery = get(&format!("/api/v1/s/{token}"));
            let said = gallery.json();
            let keys = |value: &Value| {
                let mut keys: Vec<String> = value.as_object().unwrap().keys().cloned().collect();
                keys.sort();
                keys
            };
            assert_eq!(keys(&said), ["by", "expires", "photos", "title"]);
            assert_eq!(said["title"], "Proofs");
            assert_eq!(said["photos"].as_array().unwrap().len(), 2);
            for photo in said["photos"].as_array().unwrap() {
                assert_eq!(keys(photo), ["aspect", "n", "picked"]);
            }
            let picture = get(&format!("/api/v1/s/{token}/0"));
            assert_eq!(picture.status, 200);
            assert_eq!(picture.header("x-proof").unwrap().split(' ').count(), 4, "seed, width, height, tile: no id");
            assert!(picture.header("content-disposition").is_none(), "no file's name");
            for text in [String::from_utf8_lossy(&gallery.body).into_owned(), gallery.head, picture.head, String::from_utf8_lossy(&picture.body).into_owned()] {
                for name in [FOLDER, "DSC_", "photos/", "Exif"] {
                    assert!(!text.contains(name), "{name}");
                }
            }
        }

        #[test]
        fn an_unreadable_photograph_says_no_path() {
            let fixture = fixture();
            let (_, token) = gallery_by(&fixture.sam, json!({ "ids": [fixture.ids[2]] }));
            std::fs::write(fixture.photos.join(FOLDER).join("DSC_0003.jpg"), b"not a picture").unwrap();
            let answer = get(&format!("/api/v1/s/{token}/0"));
            assert_eq!(answer.status, 422);
            let text = String::from_utf8_lossy(&answer.body);
            assert!(!text.contains(FOLDER) && !text.contains("DSC_") && !text.contains('/'), "{text}");
        }

        #[test]
        fn a_closed_link_says_nothing_about_why() {
            let fixture = fixture();
            let store = &fixture.server.store;
            let (ended, ended_token) = gallery_of_two();
            store.with(|conn| conn.execute("UPDATE shares SET expires = ?2 WHERE id = ?1", params![ended, now() - 1])).unwrap();
            let (revoked, revoked_token) = gallery_of_two();
            assert_eq!(ask("POST", &format!("/api/v1/shares/{revoked}/revoke"), Some(&fixture.sam), "", "").status, 204);
            let (relinked, old_token) = gallery_of_two();
            assert_eq!(ask("POST", &format!("/api/v1/shares/{relinked}/link"), Some(&fixture.sam), "", "").status, 200);
            let (kept, kept_token) = gallery_of_two();
            let kept_digest = store.with(|conn| conn.query_row("SELECT token FROM shares WHERE id = ?1", [kept], |row| row.get::<_, String>(0))).unwrap();
            let (open, open_token) = gallery_of_two();
            assert_eq!(get(&format!("/api/v1/s/{open_token}/0?edge=640")).status, 200, "the same gallery, open, {open}");
            let tokens = ["0".repeat(64), "nonsense".into(), ended_token, revoked_token, old_token, kept_digest];
            let answers = |token: &str| {
                [
                    get(&format!("/api/v1/s/{token}")),
                    get(&format!("/api/v1/s/{token}/0")),
                    get(&format!("/api/v1/s/{token}/0?edge=640")),
                    ask("PUT", &format!("/api/v1/s/{token}/0/pick"), None, r#"{"picked":true}"#, ""),
                ]
            };
            let plain = |answer: &Answer| (answer.status, answer.body.clone(), answer.header("content-type").map(str::to_string), answer.header("x-robots-tag").map(str::to_string));
            let unknown = plain(&answers(&tokens[0])[0]);
            assert_eq!(unknown.0, 404);
            for token in &tokens {
                for answer in answers(token) {
                    assert_eq!(plain(&answer), unknown, "{token}");
                }
            }
            assert_eq!(plain(&get(&format!("/api/v1/s/{kept_token}/2"))), unknown, "past the end of an open one");

            assert_eq!(unknown.1, serde_json::to_vec(&json!({ "error": "This gallery is not open." })).unwrap());

            let (who, gone) = (account("Lapsed", Level::Edit), account("Gone", Level::Edit));
            let (_, lapsed_token) = gallery_by(&who, json!({ "ids": [fixture.ids[0]] }));
            let (_, gone_token) = gallery_by(&gone, json!({ "ids": [fixture.ids[0]] }));
            assert_eq!(get(&format!("/api/v1/s/{lapsed_token}/0?edge=640")).status, 200);
            store.set_grants("Lapsed", &Grants::default()).unwrap();
            assert_eq!(plain(&get(&format!("/api/v1/s/{lapsed_token}/0?edge=640"))), unknown);
            store.with(|conn| conn.execute("DELETE FROM accounts WHERE name = 'Gone'", [])).unwrap();
            assert_eq!(plain(&get(&format!("/api/v1/s/{gone_token}"))), unknown);
            assert_eq!(plain(&get(&format!("/api/v1/s/{gone_token}/0?edge=640"))), unknown);
        }

        #[test]
        fn a_links_answers_carry_the_signals() {
            let (_, token) = gallery_of_two();
            let page = get(&format!("/s/{token}"));
            let gallery = get(&format!("/api/v1/s/{token}"));
            let picture = get(&format!("/api/v1/s/{token}/0?edge=640"));
            for answer in [&page, &gallery, &picture] {
                assert_eq!(answer.status, 200);
                assert_eq!(answer.header("x-robots-tag"), Some(ROBOTS));
                assert_eq!(answer.header("cache-control"), Some("no-store"));
                assert_eq!(answer.header("cross-origin-resource-policy"), Some("same-origin"));
                assert_eq!(answer.header("referrer-policy"), Some("same-origin"));
            }
            let html = String::from_utf8_lossy(&page.body);
            assert!(html.contains(&format!(r#"<meta name="robots" content="{ROBOTS}">"#)));
            assert!(html.contains(r#"<meta name="referrer" content="no-referrer">"#));
            assert_eq!(page.body, get("/s/not-a-link").body);
            assert!(String::from_utf8_lossy(&picture.body).contains("DMI-PROHIBITED-AIMLTRAINING"));
            let robots = get("/robots.txt");
            assert_eq!(robots.body, b"User-agent: *\nDisallow: /\n");

            let forged = ask("GET", &format!("/api/v1/s/{token}"), None, "", "X-Forwarded-Proto: https\r\n");
            assert!(forged.header("strict-transport-security").is_none());
        }

        #[test]
        fn what_a_new_gallery_may_not_ask() {
            let fixture = fixture();
            let one = fixture.ids[0];
            let (_, token) = gallery_by(&fixture.sam, json!({ "ids": [one], "watermark": "   ", "days": 99999 }));
            let said = get(&format!("/api/v1/s/{token}")).json();
            assert_eq!(said["by"], "© Sam");
            assert!(said["expires"].as_i64().unwrap() <= now() + MOST_DAYS * DAY);
            let (_, token) = gallery_by(&fixture.sam, json!({ "ids": [one], "days": -5 }));
            let expires = get(&format!("/api/v1/s/{token}")).json()["expires"].as_i64().unwrap();
            assert!(expires > now() && expires <= now() + DAY, "at least a day, not already over");
            let only_looks = account("Looks", Level::View);
            let refused = ask("POST", "/api/v1/shares", Some(&only_looks), &json!({ "ids": [one] }).to_string(), "");
            assert_eq!(refused.status, 404);
            let nobody = account("Nobody", Level::None);
            assert_eq!(ask("POST", "/api/v1/shares", Some(&nobody), &json!({ "ids": [one] }).to_string(), "").status, 404);
            assert_eq!(ask("POST", "/api/v1/shares", None, &json!({ "ids": [one] }).to_string(), "").status, 401);
        }

        #[test]
        fn a_gallery_that_asked_too_much_waits() {
            let (id, token) = gallery_of_two();
            let (_, other) = gallery_of_two();
            while limited(id).is_ok() {}
            assert_eq!(get(&format!("/api/v1/s/{token}/0?edge=640")).status, 429);
            assert_eq!(get(&format!("/api/v1/s/{other}/0?edge=640")).status, 200);
        }

        #[test]
        fn the_owners_list_keeps_the_secrets() {
            let fixture = fixture();
            let (id, _) = gallery_of_two();
            let (digest, secret) = fixture.server.store.with(|conn| conn.query_row("SELECT token, secret FROM shares WHERE id = ?1", [id], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))).unwrap();
            let list = ask("GET", "/api/v1/shares", Some(&fixture.sam), "", "");
            let text = String::from_utf8_lossy(&list.body);
            assert!(text.contains(&format!("\"id\":{id}")));
            assert!(!text.contains(&digest) && !text.contains(&secret) && !text.contains("\"mark\""));
        }
    }
}
