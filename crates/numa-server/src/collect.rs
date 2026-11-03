use std::collections::HashMap;
use std::path::{Path as FilePath, PathBuf};
use std::sync::{Arc, Mutex};

use axum::body::Body;
use axum::extract::{DefaultBodyLimit, Path, Query, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post, put};
use axum::{Json, Router};
use numa_io::catalog::Library;
use rusqlite::params;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::auth::{self, Signed};
use crate::send::{self, Part};
use crate::share::{self, closed, robots};
use crate::store::{now, Store};
use crate::{Fail, Server};

const DAY: i64 = 86_400;
const DAYS: i64 = 7;
const MOST_DAYS: i64 = 90;

const FILES: i64 = 200;
const MOST_FILES: i64 = 1000;

const ROOM: i64 = 30 << 20;

const LARGEST: u64 = 100 << 20;

const AT_ONCE: u32 = 4;

const MOST_PIXELS: u64 = 150_000_000;

const FLOOR: u64 = 2 << 30;

const KEPT_DAYS: i64 = 30;

fn looks_like(extension: &str, head: &[u8]) -> bool {
    match extension {
        "jpg" | "jpeg" => head.starts_with(&[0xff, 0xd8, 0xff]),
        "png" => head.starts_with(&[0x89, b'P', b'N', b'G']),
        "webp" => head.len() >= 12 && &head[..4] == b"RIFF" && &head[8..12] == b"WEBP",
        "heic" | "heif" => head.len() >= 12 && &head[4..8] == b"ftyp",
        "tif" | "tiff" => head.starts_with(b"II*\0") || head.starts_with(b"MM\0*"),
        _ => false,
    }
}

fn extension(name: &str) -> Option<String> {
    FilePath::new(name).extension().map(|extension| extension.to_string_lossy().to_lowercase())
}

fn guest_name(asked: Option<&str>) -> String {
    asked.unwrap_or_default().chars().filter(|c| !c.is_control()).take(40).collect::<String>().trim().to_string()
}

fn folder_name(title: &str) -> String {
    let clean: String = title.chars().map(|c| if c.is_alphanumeric() || " -_.()".contains(c) { c } else { '_' }).collect();
    let clean = clean.trim().trim_start_matches('.').trim().chars().take(60).collect::<String>();
    match clean.is_empty() {
        true => "Guests".into(),
        false => clean,
    }
}

fn staging(library: &Library, share: i64) -> PathBuf {
    library.path.join(numa_io::catalog::LIBRARY_DIR).join("guests").join(share.to_string())
}

fn free_on(path: &FilePath) -> Option<u64> {
    use std::os::unix::ffi::OsStrExt;
    let path = std::ffi::CString::new(path.as_os_str().as_bytes()).ok()?;
    let mut disk = std::mem::MaybeUninit::<libc::statvfs>::zeroed();

    let read = unsafe { libc::statvfs(path.as_ptr(), disk.as_mut_ptr()) };

    (read == 0).then(|| unsafe { disk.assume_init() }).map(|disk| disk.f_bavail as u64 * disk.f_frsize as u64)
}

fn too_full(free: u64, largest: u64) -> bool {
    free < FLOOR.saturating_add(largest)
}

struct Under(i64);

fn under_way() -> std::sync::MutexGuard<'static, HashMap<i64, u32>> {
    static COUNTS: std::sync::OnceLock<Mutex<HashMap<i64, u32>>> = std::sync::OnceLock::new();
    COUNTS.get_or_init(Mutex::default).lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

impl Under {
    fn begin(share: i64) -> Result<Self, Fail> {
        let mut counts = under_way();
        let count = counts.entry(share).or_insert(0);
        if *count >= AT_ONCE {
            return Err(Fail(StatusCode::TOO_MANY_REQUESTS, "Too many photographs at once; wait a moment.".into()));
        }
        *count += 1;
        Ok(Self(share))
    }
}

impl Drop for Under {
    fn drop(&mut self) {
        if let Some(count) = under_way().get_mut(&self.0) {
            *count = count.saturating_sub(1);
        }
    }
}

fn taken(store: &Store, share: i64) -> (i64, i64) {
    store
        .with(|conn| conn.query_row("SELECT COUNT(*), COALESCE(SUM(size), 0) FROM guest_files WHERE share = ?1", [share], |row| Ok((row.get(0)?, row.get(1)?))))
        .unwrap_or((0, 0))
}

fn take(store: &Store, share: &share::Share, name: &str, guest: &str, size: i64, sha256: &str, path: &str) -> Result<Option<i64>, Fail> {
    store
        .with(|conn| {
            if conn.query_row("SELECT EXISTS (SELECT 1 FROM guest_files WHERE share = ?1 AND sha256 = ?2)", params![share.id, sha256], |row| row.get::<_, bool>(0))? {
                return Ok(Err(None));
            }
            let (count, bytes): (i64, i64) = conn.query_row("SELECT COUNT(*), COALESCE(SUM(size), 0) FROM guest_files WHERE share = ?1", [share.id], |row| Ok((row.get(0)?, row.get(1)?)))?;
            if count >= share.max_files || bytes + size > share.max_bytes {
                return Ok(Err(Some(())));
            }
            conn.execute(
                "INSERT INTO guest_files (share, name, guest, size, sha256, added, path) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                params![share.id, name, guest, size, sha256, now(), path],
            )?;
            Ok(Ok(conn.last_insert_rowid()))
        })
        .map_err(Fail::internal)?
        .map(Some)
        .or_else(|full| match full {
            None => Ok(None),
            Some(()) => Err(Fail(StatusCode::PAYLOAD_TOO_LARGE, "This link has no room for more photographs.".into())),
        })
}

async fn open_link(server: &Arc<Server>, token: &str) -> Result<(share::Share, Library), Fail> {
    let share = share::find_kind(&server.store, token, now(), "collect")?;
    let owner = share::owner(server, &share.owner).await?;
    let library = share.dest.and_then(|id| server.home.editable(&owner).into_iter().find(|library| library.id == id)).ok_or_else(closed)?;
    Ok((share, library))
}

async fn page(State(server): State<Arc<Server>>, Path(token): Path<String>) -> Result<Response, Fail> {
    let (share, _) = open_link(&server, &token).await?;
    let (count, bytes) = taken(&server.store, share.id);
    Ok(robots(Json(json!({
        "title": share.title, "by": share.watermark, "expires": share.expires,
        "left": (share.max_files - count).max(0), "room": (share.max_bytes - bytes).max(0), "largest": LARGEST,
    }))))
}

#[derive(Deserialize)]
struct Guest {
    by: Option<String>,
}

async fn send_in(State(server): State<Arc<Server>>, Path((token, asked)): Path<(String, String)>, Query(guest): Query<Guest>, headers: HeaderMap, body: Body) -> Result<Response, Fail> {
    let (share, library) = open_link(&server, &token).await?;
    share::limited(-share.id)?;
    let name = send::file_name(&asked)?;
    let ext = extension(&name).filter(|ext| looks_like_known(ext)).ok_or_else(|| Fail(StatusCode::UNSUPPORTED_MEDIA_TYPE, format!("{name} is not a kind of photograph this link takes.")))?;
    let _under = Under::begin(share.id)?;
    let (count, bytes) = taken(&server.store, share.id);
    let room = (share.max_bytes - bytes).max(0) as u64;
    if count >= share.max_files || room == 0 {
        return Err(Fail(StatusCode::PAYLOAD_TOO_LARGE, "This link has no room for more photographs.".into()));
    }
    let told = headers.get(header::CONTENT_LENGTH).and_then(|value| value.to_str().ok()).and_then(|value| value.parse::<u64>().ok());
    if told.is_some_and(|told| told > LARGEST.min(room)) {
        return Err(Fail(StatusCode::PAYLOAD_TOO_LARGE, "That photograph is too large for this link.".into()));
    }
    let folder = staging(&library, share.id);
    tokio::fs::create_dir_all(&folder).await.map_err(Fail::internal)?;
    if free_on(&folder).is_some_and(|free| too_full(free, LARGEST.min(room))) {
        return Err(Fail(StatusCode::INSUFFICIENT_STORAGE, "Numa Home has no room for photographs right now.".into()));
    }
    let part = Part(folder.join(format!("{}.part", &auth::token().0[..16])));
    let sha256 = send::receive(body, &part.0, LARGEST.min(room), "That photograph is too large for this link.").await?;
    let size = std::fs::metadata(&part.0).map_err(Fail::internal)?.len() as i64;
    let head = {
        use std::io::Read;
        let mut head = [0u8; 16];
        let read = std::fs::File::open(&part.0).and_then(|mut file| file.read(&mut head)).map_err(Fail::internal)?;
        head[..read].to_vec()
    };
    if size == 0 || !looks_like(&ext, &head) {
        return Err(Fail(StatusCode::UNSUPPORTED_MEDIA_TYPE, format!("{name} is not the kind of photograph its name says.")));
    }
    if numa_io::raw::header_pixels(&part.0).is_some_and(|pixels| pixels > MOST_PIXELS) {
        return Err(Fail(StatusCode::PAYLOAD_TOO_LARGE, format!("{name} has more pixels than this link takes.")));
    }

    share::find_kind(&server.store, &token, now(), "collect")?;
    let guest = guest_name(guest.by.as_deref());
    let placed = format!("{}.{ext}", &auth::token().0[..16]);
    let Some(id) = take(&server.store, &share, &name, &guest, size, &sha256, &placed)? else {
        return Ok((StatusCode::OK, Json(json!({ "duplicate": true }))).into_response());
    };
    if let Err(err) = std::fs::rename(&part.0, folder.join(&placed)) {
        let _ = server.store.with(|conn| conn.execute("DELETE FROM guest_files WHERE id = ?1", [id]));
        return Err(Fail::internal(err));
    }
    Ok((StatusCode::CREATED, Json(json!({ "duplicate": false, "left": (share.max_files - count - 1).max(0) }))).into_response())
}

fn looks_like_known(extension: &str) -> bool {
    ["jpg", "jpeg", "png", "webp", "heic", "heif", "tif", "tiff"].contains(&extension)
}

#[derive(Deserialize)]
struct New {
    title: Option<String>,

    by: Option<String>,
    days: Option<i64>,
    count: Option<i64>,
    album: Option<String>,
}

async fn create(State(server): State<Arc<Server>>, Signed(me): Signed, Json(new): Json<New>) -> Result<Json<Value>, Fail> {
    sweep(&server);
    let title = new.title.map(|title| title.trim().to_string()).filter(|title| !title.is_empty()).unwrap_or_else(|| "Photos".into());
    let by = new.by.map(|by| guest_name(Some(&by))).filter(|by| !by.is_empty()).unwrap_or_else(|| me.name.clone());
    if title.chars().count() > 80 {
        return Err(Fail::bad("The title is at most 80 characters."));
    }
    if let Some(key) = &new.album {
        if !crate::shared::may_add(&server.store, &me, key) {
            return Err(Fail(StatusCode::FORBIDDEN, "You may look at this album, not add to it.".into()));
        }
    }
    let library = send::destination(&server, &me, None).await?;
    let (files, days) = (new.count.unwrap_or(FILES).clamp(1, MOST_FILES), new.days.unwrap_or(DAYS).clamp(1, MOST_DAYS));
    let (id, token) = share::add_kind(&server.store, &me.name, &title, &by, new.album.as_deref(), &[], days, ("collect", files, files * ROOM, Some(library.id))).map_err(Fail::internal)?;
    Ok(Json(json!({ "id": id, "link": format!("/c/{token}") })))
}

async fn list(State(server): State<Arc<Server>>, Signed(me): Signed) -> Result<Json<Vec<Value>>, Fail> {
    sweep(&server);
    let rows = server
        .store
        .with(|conn| {
            let mut statement = conn.prepare(
                "SELECT s.id, s.title, s.album, s.made, s.expires, s.revoked, s.max_files,
                        (SELECT COUNT(*) FROM guest_files f WHERE f.share = s.id AND f.kept = 0),
                        (SELECT COUNT(*) FROM guest_files f WHERE f.share = s.id AND f.kept = 1)
                 FROM shares s WHERE s.owner = ?1 AND s.kind = 'collect' ORDER BY s.made DESC",
            )?;
            let rows = statement.query_map([&me.name], |row| {
                let (expires, revoked): (i64, Option<i64>) = (row.get(4)?, row.get(5)?);
                Ok(json!({
                    "id": row.get::<_, i64>(0)?, "title": row.get::<_, String>(1)?, "folder": folder_name(&row.get::<_, String>(1)?), "album": row.get::<_, Option<String>>(2)?,
                    "made": row.get::<_, i64>(3)?, "expires": expires, "revoked": revoked.is_some(), "open": revoked.is_none() && now() < expires,
                    "max": row.get::<_, i64>(6)?, "waiting": row.get::<_, i64>(7)?, "accepted": row.get::<_, i64>(8)?,
                }))
            })?;
            rows.collect::<rusqlite::Result<Vec<Value>>>()
        })
        .map_err(Fail::internal)?;
    Ok(Json(rows))
}

fn mine(server: &Server, me: &crate::store::Account, id: i64) -> Result<share::Share, Fail> {
    share::own(server, me, id).and_then(|share| match share.kind == "collect" {
        true => Ok(share),
        false => Err(Fail(StatusCode::NOT_FOUND, "There is no such gallery.".into())),
    })
}

fn waiting(server: &Server, share: &share::Share, ids: Option<&[i64]>) -> Vec<(i64, String, String, i64, PathBuf)> {
    let Some(library) = share.dest.and_then(|id| server.home.library(id)) else { return Vec::new() };
    let folder = staging(&library, share.id);
    server
        .store
        .with(|conn| {
            let mut statement = conn.prepare("SELECT id, name, sha256, size, path FROM guest_files WHERE share = ?1 AND kept = 0 ORDER BY id")?;
            let rows = statement.query_map([share.id], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?, row.get::<_, String>(2)?, row.get::<_, i64>(3)?, row.get::<_, String>(4)?)))?;
            rows.collect::<rusqlite::Result<Vec<_>>>()
        })
        .unwrap_or_default()
        .into_iter()
        .filter(|(id, ..)| ids.is_none_or(|ids| ids.contains(id)))
        .map(|(id, name, sha256, size, path)| (id, name, sha256, size, folder.join(path)))
        .collect()
}

async fn files(State(server): State<Arc<Server>>, Signed(me): Signed, Path(id): Path<i64>) -> Result<Json<Vec<Value>>, Fail> {
    let share = mine(&server, &me, id)?;
    let guests: HashMap<i64, (String, i64)> = server
        .store
        .with(|conn| {
            let mut statement = conn.prepare("SELECT id, guest, added FROM guest_files WHERE share = ?1")?;
            let rows = statement.query_map([id], |row| Ok((row.get(0)?, (row.get(1)?, row.get(2)?))))?;
            rows.collect()
        })
        .map_err(Fail::internal)?;
    Ok(Json(
        waiting(&server, &share, None)
            .into_iter()
            .map(|(id, name, _, size, _)| {
                let (guest, added) = guests.get(&id).cloned().unwrap_or_default();
                json!({ "id": id, "name": name, "size": size, "guest": guest, "added": added })
            })
            .collect(),
    ))
}

async fn thumb(State(server): State<Arc<Server>>, Signed(me): Signed, Path((id, file)): Path<(i64, i64)>) -> Result<Response, Fail> {
    let share = mine(&server, &me, id)?;
    let (_, _, _, _, path) = waiting(&server, &share, Some(&[file])).into_iter().next().ok_or_else(|| Fail(StatusCode::NOT_FOUND, "There is no such photograph.".into()))?;
    let mtime = std::fs::metadata(&path).and_then(|meta| meta.modified()).ok().and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |since| since.as_secs() as i64);
    let _turn = server.decodes.acquire().await.map_err(Fail::internal)?;
    let bytes = tokio::task::spawn_blocking(move || -> Result<Vec<u8>, String> {
        let picture = numa_io::thumbs::load(&path, mtime, numa_io::thumbs::LIBRARY_EDGE, None)?;
        let mut bytes = Vec::new();
        picture.write_to(&mut std::io::Cursor::new(&mut bytes), image::ImageFormat::Jpeg).map_err(|err| err.to_string())?;
        Ok(bytes)
    })
    .await
    .map_err(Fail::internal)?
    .map_err(|err| {
        log::warn!("collect {id}: {err}");
        Fail(StatusCode::UNPROCESSABLE_ENTITY, "Numa could not read this photograph.".into())
    })?;
    Ok(([(header::CONTENT_TYPE, "image/jpeg"), (header::CACHE_CONTROL, "private, no-cache")], bytes).into_response())
}

#[derive(Deserialize)]
struct Which {
    files: Option<Vec<i64>>,
}

fn claim(server: &Server, share: &share::Share, ids: Option<&[i64]>) -> Vec<(i64, String, String, i64, PathBuf)> {
    waiting(server, share, ids)
        .into_iter()
        .filter(|(id, ..)| server.store.with(|conn| conn.execute("UPDATE guest_files SET kept = 2 WHERE id = ?1 AND kept = 0", [id])).is_ok_and(|changed| changed == 1))
        .collect()
}

async fn accept(State(server): State<Arc<Server>>, Signed(me): Signed, Path(id): Path<i64>, Json(which): Json<Which>) -> Result<Json<Value>, Fail> {
    let share = mine(&server, &me, id)?;
    let library = share.dest.and_then(|id| server.home.editable(&me).into_iter().find(|library| library.id == id)).ok_or_else(|| Fail(StatusCode::NOT_FOUND, "There is no such library.".into()))?;
    let folder = send::folder_in(&format!("Guests/{}", folder_name(&share.title)))?;
    let (mut accepted, mut duplicates, mut photos) = (0, 0, Vec::new());
    for (file, name, sha256, _, path) in claim(&server, &share, which.files.as_deref()) {
        let there = server.store.sent(&me.name, &sha256, library.id).map(|kept| library.path.join(kept)).and_then(|path| server.home.at(&path));
        let landed = match there {
            Some(photo) => {
                let _ = std::fs::remove_file(&path);
                Ok((photo, true))
            }
            None => {
                let placed = {
                    let (library, folder, name, sha256, path) = (library.clone(), folder.clone(), name.clone(), sha256.clone(), path.clone());
                    tokio::task::spawn_blocking(move || send::place(&path, &library, Some(folder), &name, &sha256, None, false)).await.map_err(Fail::internal)?
                };
                match placed {
                    Ok((target, duplicate)) => {
                        let kept = target.strip_prefix(&library.path).unwrap_or(&target).to_string_lossy().into_owned();
                        let _ = server.store.note_sent(&me.name, &sha256, library.id, &kept);
                        match server.home.at(&target) {
                            Some(photo) => Ok((photo, duplicate)),
                            None => send::catalogue(&server, &library, target).await.map(|photo| (photo, duplicate)),
                        }
                    }
                    Err(err) => Err(err),
                }
            }
        };
        match landed {
            Ok((photo, duplicate)) => {
                let _ = server.store.with(|conn| conn.execute("UPDATE guest_files SET kept = 1 WHERE id = ?1", [file]));
                photos.push(photo);
                match duplicate {
                    true => duplicates += 1,
                    false => accepted += 1,
                }
            }
            Err(err) => {
                let _ = server.store.with(|conn| conn.execute("UPDATE guest_files SET kept = 0 WHERE id = ?1", [file]));
                log::warn!("collect {id}: {}", err.1);
            }
        }
    }
    if let (Some(key), false) = (share.album.clone(), photos.is_empty()) {
        let ids = photos.clone();
        let _ = server.home.ask(move |catalog| catalog.set_in_album(&key, &ids, true)).await?;
        server.home.forget_albums();
    }
    server.jobs.nudge();
    Ok(Json(json!({ "accepted": accepted, "duplicates": duplicates })))
}

async fn reject(State(server): State<Arc<Server>>, Signed(me): Signed, Path(id): Path<i64>, Json(which): Json<Which>) -> Result<Json<Value>, Fail> {
    let share = mine(&server, &me, id)?;
    let ids = which.files.filter(|files| !files.is_empty()).ok_or_else(|| Fail::bad("Choose the photographs to reject."))?;
    let mut rejected = 0;
    for (file, _, _, _, path) in claim(&server, &share, Some(&ids)) {
        let _ = std::fs::remove_file(&path);
        let _ = server.store.with(|conn| conn.execute("DELETE FROM guest_files WHERE id = ?1", [file]));
        rejected += 1;
    }
    Ok(Json(json!({ "rejected": rejected })))
}

pub(crate) fn sweep(server: &Server) {
    let before = now() - KEPT_DAYS * DAY;
    let old: Vec<share::Share> = server
        .store
        .with(|conn| {
            let mut statement = conn.prepare(&format!("SELECT {} FROM shares WHERE kind = 'collect' AND (expires < ?1 OR revoked < ?1)", share::COLUMNS))?;
            let rows = statement.query_map([before], share::row)?;
            rows.collect::<rusqlite::Result<Vec<_>>>()
        })
        .unwrap_or_default();
    for share in old {
        for (file, _, _, _, path) in waiting(server, &share, None) {
            let _ = std::fs::remove_file(&path);
            let _ = server.store.with(|conn| conn.execute("DELETE FROM guest_files WHERE id = ?1", [file]));
        }
    }
}

pub fn routes() -> Router<Arc<Server>> {
    Router::new()
        .route("/c/{token}", get(|| async { share::file("text/html; charset=utf-8", include_bytes!("web/collect.html")) }))
        .route("/collect.js", get(|| async { share::file("text/javascript; charset=utf-8", include_bytes!("web/collect.js")) }))
        .route("/collect.css", get(|| async { share::file("text/css; charset=utf-8", include_bytes!("web/collect.css")) }))
        .route("/api/v1/c/{token}", get(page))
        .route("/api/v1/c/{token}/files/{name}", put(send_in).layer(DefaultBodyLimit::disable()))
        .route("/api/v1/collects", get(list).post(create))
        .route("/api/v1/collects/{id}/files", get(files))
        .route("/api/v1/collects/{id}/files/{file}/thumb", get(thumb))
        .route("/api/v1/collects/{id}/accept", post(accept))
        .route("/api/v1/collects/{id}/reject", post(reject))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn store() -> Store {
        let store = Store::open(FilePath::new(":memory:")).unwrap();
        share::tables(&store).unwrap();
        store
    }

    #[test]
    fn a_file_is_what_its_bytes_say() {
        assert!(looks_like("jpg", &[0xff, 0xd8, 0xff, 0xe0, 0, 0]));
        assert!(!looks_like("jpg", b"<html><script>"), "a page named photo.jpg");
        assert!(!looks_like("png", &[0xff, 0xd8, 0xff]), "a jpeg named .png");
        assert!(looks_like("heic", b"\0\0\0\x18ftypheic"));
        assert!(looks_like("webp", b"RIFF\x10\0\0\0WEBPVP8 "));
        assert!(!looks_like("exe", b"MZ\x90\0"), "no other kind");
        assert!(!looks_like("jpg", &[]));
        assert!(extension("a.JPG").as_deref().is_some_and(looks_like_known) && !looks_like_known("raf") && !looks_like_known("svg"));
    }

    #[test]
    fn a_guest_leaves_the_disk_its_floor() {
        assert!(!too_full(10 << 30, 100 << 20));
        assert!(too_full(2 << 30, 1), "nothing is taken from the floor itself");
        assert!(too_full((2 << 30) + (99 << 20), 100 << 20));
        assert!(!too_full((2 << 30) + (100 << 20), 100 << 20));
        assert!(too_full(u64::MAX - 1, u64::MAX), "no overflow");
        assert!(free_on(&std::env::temp_dir()).is_some_and(|free| free > 0));
        assert!(free_on(FilePath::new("/no/such/place")).is_none());
    }

    #[test]
    fn names_are_plain() {
        assert_eq!(folder_name("Anna & Tom's party / 2026"), "Anna _ Tom_s party _ 2026");
        assert_eq!(folder_name("../../etc"), "_.._etc");
        assert_eq!(folder_name("  "), "Guests");
        assert_eq!(folder_name("..."), "Guests");
        assert!(send::folder_in(&format!("Guests/{}", folder_name("../../etc"))).is_ok());
        assert_eq!(guest_name(Some("Mo\nne\u{7}")), "Mone");
        assert_eq!(guest_name(Some(&"x".repeat(100))).len(), 40);
    }

    #[test]
    fn a_link_takes_what_it_has_room_for() {
        let store = store();
        let (id, token) = share::add_kind(&store, "Sam", "Party", "", None, &[], 7, ("collect", 2, 100, Some(1))).unwrap();
        let share = share::find_kind(&store, &token, now(), "collect").unwrap();
        assert_eq!(share.id, id);
        assert_eq!(take(&store, &share, "a.jpg", "Mo", 40, "aa", "1.jpg").unwrap().is_some(), true);
        assert_eq!(take(&store, &share, "a.jpg", "Mo", 40, "aa", "2.jpg").unwrap(), None, "the same bytes");
        assert_eq!(take(&store, &share, "b.jpg", "Mo", 70, "bb", "3.jpg").unwrap_err().0, StatusCode::PAYLOAD_TOO_LARGE, "past the bytes");
        assert!(take(&store, &share, "b.jpg", "Mo", 60, "bb", "3.jpg").unwrap().is_some());
        assert_eq!(take(&store, &share, "c.jpg", "Mo", 1, "cc", "4.jpg").unwrap_err().0, StatusCode::PAYLOAD_TOO_LARGE, "past the count");
        assert_eq!(taken(&store, id), (2, 100));
    }

    #[test]
    fn the_two_kinds_of_link_do_not_open_each_other() {
        let store = store();
        let (_, collect) = share::add_kind(&store, "Sam", "Party", "", None, &[], 1, ("collect", 5, 500, Some(1))).unwrap();
        let (_, proof) = share::add(&store, "Sam", "Wedding", "© Sam", None, &[11], 30).unwrap();
        assert!(share::find_kind(&store, &collect, now(), "collect").is_ok());
        assert_eq!(share::find(&store, &collect, now()).unwrap_err().0, StatusCode::NOT_FOUND);
        assert_eq!(share::find_kind(&store, &proof, now(), "collect").unwrap_err().0, StatusCode::NOT_FOUND);
        assert_eq!(share::find_kind(&store, &collect, now() + DAY + 1, "collect").unwrap_err().0, StatusCode::NOT_FOUND, "ended");
    }
}
