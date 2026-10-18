use std::collections::HashSet;
use std::sync::Arc;

use axum::body::Body;
use axum::extract::{Query, State};
use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use numa_io::catalog::{library_of, Library, Photo};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::auth::Signed;
use crate::manage::{self, within, Ids};
use crate::store::Account;
use crate::{Fail, Server};

pub(crate) fn chosen(server: &Server, me: &Account, ids: &[i64], twins: bool, manage: bool) -> Result<Vec<(Library, Photo)>, Fail> {
    if ids.is_empty() {
        return Err(Fail::bad("Choose a photograph first."));
    }
    let ids = if twins { server.home.with_twins(ids) } else { ids.to_vec() };
    let libraries = server.home.libraries(&me.name);
    let mut photos = Vec::new();
    for id in ids {
        let photo = server.home.photo(&me.name, id)?;
        let library = libraries.iter().find(|library| library.id == library_of(id)).cloned().ok_or_else(|| Fail::internal("a photograph without its library"))?;
        if manage && !manage::may_manage(server, me, &library) {
            return Err(manage::refused());
        }
        photos.push((library, photo));
    }
    Ok(photos)
}

async fn blocking<R: Send + 'static>(server: &Arc<Server>, work: impl FnOnce(&Arc<Server>) -> R + Send + 'static) -> Result<R, Fail> {
    let server = server.clone();
    tokio::task::spawn_blocking(move || work(&server)).await.map_err(Fail::internal)
}

pub async fn remove(State(server): State<Arc<Server>>, Signed(me): Signed, Json(asked): Json<Ids>) -> Result<Json<Value>, Fail> {
    let photos = chosen(&server, &me, &asked.ids, true, true)?;
    let removed = blocking(&server, move |server| {
        let mut libraries = HashSet::new();
        for (library, photo) in &photos {
            if server.store.remove(library.id, &within(library, &photo.path)).is_ok() {
                let id = photo.id;
                server.home.blocking(move |catalog| catalog.remove_photo(id));
                libraries.insert(library.id);
            }
        }
        manage::shelve(server, libraries);
        photos.len()
    })
    .await?;
    Ok(Json(json!({ "removed": removed })))
}

pub async fn delete(State(server): State<Arc<Server>>, Signed(me): Signed, Json(asked): Json<Ids>) -> Result<Json<Value>, Fail> {
    let photos = chosen(&server, &me, &asked.ids, true, true)?;
    let trashed = blocking(&server, move |server| manage::trash(server, photos)).await?;
    Ok(Json(json!({ "trashed": trashed })))
}

pub async fn keep_jpeg(State(server): State<Arc<Server>>, Signed(me): Signed, Json(asked): Json<Ids>) -> Result<Json<Value>, Fail> {
    let raws: Vec<_> = chosen(&server, &me, &asked.ids, false, true)?.into_iter().filter(|(_, photo)| numa_io::raw::is_raw(&photo.path)).collect();
    if raws.is_empty() {
        return Err(Fail::bad("None of these is a raw."));
    }
    let n = raws.len();
    manage::KEEPING.fetch_add(n, std::sync::atomic::Ordering::Relaxed);
    tokio::spawn(manage::keep_jpegs(server.clone(), raws));
    Ok(Json(json!({ "raws": n })))
}

pub async fn keeping(Signed(_): Signed) -> Json<Value> {
    Json(json!({ "left": manage::KEEPING.load(std::sync::atomic::Ordering::Relaxed) }))
}

#[derive(Deserialize)]
pub struct Moved {
    ids: Vec<i64>,
    library: i64,

    dir: Option<String>,
}

pub async fn moved(State(server): State<Arc<Server>>, Signed(me): Signed, Json(asked): Json<Moved>) -> Result<Json<Value>, Fail> {
    let photos = chosen(&server, &me, &asked.ids, true, true)?;
    let target = server.home.libraries(&me.name).into_iter().find(|library| library.id == asked.library).ok_or_else(|| Fail::bad("There is no such library."))?;
    if !manage::may_manage(&server, &me, &target) {
        return Err(manage::refused());
    }
    let folder = target.path.join(crate::send::folder_in(asked.dir.as_deref().unwrap_or(""))?);
    let moved = blocking(&server, move |server| manage::move_photos(server, photos, &target, |_, _| folder.clone())).await?;
    Ok(Json(json!({ "moved": moved })))
}

#[derive(Deserialize)]
pub struct Album {
    ids: Vec<i64>,

    album: Option<String>,
    name: Option<String>,
    inside: bool,
}

pub async fn album(State(server): State<Arc<Server>>, Signed(me): Signed, Json(asked): Json<Album>) -> Result<Json<Value>, Fail> {
    let ids: Vec<i64> = chosen(&server, &me, &asked.ids, false, false)?.into_iter().map(|(_, photo)| photo.id).collect();
    let key = server
        .home
        .ask(move |catalog| {
            let key = match (asked.album, asked.name) {
                (Some(key), _) => key,
                (None, Some(name)) => catalog.create_album(&name)?,
                (None, None) => return Err("Choose an album.".to_string()),
            };
            catalog.set_in_album(&key, &ids, asked.inside).map(|()| key)
        })
        .await?
        .map_err(Fail::bad)?;
    Ok(Json(json!({ "album": key })))
}

#[derive(Deserialize)]
pub struct Marks {
    ids: Vec<i64>,
    rating: Option<u8>,
    flag: Option<String>,
}

pub async fn marks(State(server): State<Arc<Server>>, Signed(me): Signed, Json(asked): Json<Marks>) -> Result<Json<Value>, Fail> {
    let photos = chosen(&server, &me, &asked.ids, false, false)?;
    let flag = asked.flag.as_deref().map(crate::photos::flag_from).transpose()?;
    if asked.rating.is_some_and(|rating| rating > 5) {
        return Err(Fail::bad("A rating is 0 to 5 stars."));
    }
    let ids: Vec<i64> = photos.iter().map(|(_, photo)| photo.id).collect();
    let libraries: HashSet<i64> = photos.iter().map(|(library, _)| library.id).collect();
    let (changed, home) = (ids.clone(), server.clone());
    server
        .home
        .ask(move |catalog| {
            let rated = asked.rating.map_or(Ok(()), |rating| catalog.set_ratings(&changed, rating));
            let flagged = flag.map_or(Ok(()), |flag| catalog.set_flags(&changed, flag));
            libraries.into_iter().for_each(|id| home.shelve(Some(id), catalog));
            rated.and(flagged)
        })
        .await?
        .map_err(Fail::internal)?;
    let updated = ids.iter().map(|id| server.store.touch(*id)).max();
    server.jobs.nudge();
    Ok(Json(json!({ "updated": updated })))
}

#[derive(Deserialize)]
pub struct By {
    ids: Vec<i64>,

    account: Option<String>,
}

pub async fn by(State(server): State<Arc<Server>>, Signed(me): Signed, Json(asked): Json<By>) -> Result<StatusCode, Fail> {
    let photos = chosen(&server, &me, &asked.ids, true, false)?;
    let account = match asked.account.as_deref() {
        Some(name) => Some(server.store.account(name).ok_or_else(|| Fail::bad("There is no such account."))?.0),
        None => None,
    };
    for (library, photo) in photos {
        server.store.set_author(library.id, &within(&library, &photo.path), account.as_deref()).map_err(Fail::internal)?;
    }
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
pub struct Listed {

    ids: String,
}

pub async fn zip(State(server): State<Arc<Server>>, Signed(me): Signed, Query(listed): Query<Listed>) -> Result<Response, Fail> {
    let ids: Vec<i64> = listed.ids.split(',').filter_map(|id| id.trim().parse().ok()).collect();
    let photos = chosen(&server, &me, &ids, false, false)?;
    let scratch = server.data.join("tmp").join(format!("zip-{}-{}.zip", std::process::id(), std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |since| since.as_nanos())));
    let file = tokio::task::spawn_blocking(move || -> Result<std::fs::File, String> {
        std::fs::create_dir_all(scratch.parent().unwrap_or(&scratch)).map_err(|err| err.to_string())?;
        let mut zip = zip::ZipWriter::new(std::fs::File::create(&scratch).map_err(|err| err.to_string())?);
        let options = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored).large_file(true);
        let made = (|| -> Result<std::fs::File, String> {
            for (library, photo) in &photos {
                zip.start_file(format!("{}/{}", library.label(), within(library, &photo.path)), options).map_err(|err| err.to_string())?;
                let mut file = std::fs::File::open(&photo.path).map_err(|err| format!("{}: {err}", photo.path.display()))?;
                std::io::copy(&mut file, &mut zip).map_err(|err| err.to_string())?;
            }
            zip.finish().map_err(|err| err.to_string())?;
            std::fs::File::open(&scratch).map_err(|err| err.to_string())
        })();

        let _ = std::fs::remove_file(&scratch);
        made
    })
    .await
    .map_err(Fail::internal)?
    .map_err(|err| Fail(StatusCode::UNPROCESSABLE_ENTITY, format!("The zip could not be made: {err}")))?;
    let length = file.metadata().map(|meta| meta.len()).ok();
    let mut response = Body::from_stream(tokio_util::io::ReaderStream::new(tokio::fs::File::from_std(file))).into_response();
    let headers = response.headers_mut();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("application/zip"));
    headers.insert(header::CONTENT_DISPOSITION, crate::photos::attachment("Numa Home.zip"));
    if let Some(length) = length {
        headers.insert(header::CONTENT_LENGTH, HeaderValue::from(length));
    }
    Ok(response)
}
