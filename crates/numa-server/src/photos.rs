use std::path::Path as FilePath;
use std::sync::Arc;

use axum::body::{Body, Bytes};
use axum::extract::{Path, Query, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use numa_io::catalog::{library_of, Flag, Photo};
use numa_io::thumbs;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::auth::Signed;
use crate::home::when;
use crate::{Fail, Server};

pub async fn detail(State(server): State<Arc<Server>>, Signed(me): Signed, Path(id): Path<i64>) -> Result<Json<Value>, Fail> {
    let photo = server.home.photo(&me.name, id)?;
    let path = photo.path.clone();
    let summary = tokio::task::spawn_blocking(move || numa_io::raw::summary(&path)).await.map_err(Fail::internal)?;
    let (people, albums) = server
        .home
        .ask(move |catalog| {
            let people = catalog.people(library_of(id)).unwrap_or_default();
            let people: Vec<String> = people.into_iter().filter(|(_, ids)| ids.contains(&id)).map(|(name, _)| name).collect();
            (people, catalog.albums_of(&[id]).unwrap_or_default().remove(&id).unwrap_or_default())
        })
        .await?;

    let library = server.home.libraries(&me.name).into_iter().filter(|library| library.id == library_of(id)).collect::<Vec<_>>();
    let by = crate::by::owners(&server, &library).await?.remove(&id);
    let chosen = library.first().is_some_and(|library| server.store.authors().contains_key(&(library.id, crate::manage::within(library, &photo.path))));
    let summary = summary.as_ref();

    let exposure: Vec<String> = [
        summary.and_then(|summary| summary.shutter_text()),
        summary.and_then(|summary| summary.aperture).map(|aperture| format!("f/{aperture:.1}")),
        summary.and_then(|summary| summary.iso).map(|iso| format!("ISO {iso}")),
        summary.and_then(|summary| summary.focal_length).map(|focal| format!("{focal:.0} mm")),
    ]
    .into_iter()
    .flatten()
    .collect();
    let size = summary.map(|summary| summary.sensor).filter(|(w, h)| *w > 0 && *h > 0).map(|(w, h)| format!("{w} × {h}"));
    Ok(Json(json!({
        "id": id,
        "name": photo.path.file_name().map(|name| name.to_string_lossy()),
        "taken": when(&photo),
        "camera": summary.and_then(|summary| summary.camera.clone()),
        "lens": summary.and_then(|summary| summary.lens.clone()),
        "exposure": (!exposure.is_empty()).then(|| exposure.join(" · ")),
        "size": size,
        "people": people,
        "albums": albums,
        "rating": photo.rating,
        "flag": flag_name(photo.flag),
        "edited": photo.edited,
        "library": library_of(id),
        "by": by,
        "chosen": chosen,
    })))
}

#[derive(Deserialize)]
pub struct Edge {
    edge: Option<u32>,
}

pub async fn thumb(state: State<Arc<Server>>, signed: Signed, id: Path<i64>, Query(edge): Query<Edge>, headers: HeaderMap) -> Result<Response, Fail> {
    let edge = match edge.edge.unwrap_or(256) {
        0..=320 => thumbs::LIBRARY_EDGE,
        _ => thumbs::STEPS[0],
    };
    picture(state, signed, id, edge, headers).await
}

pub async fn preview(state: State<Arc<Server>>, signed: Signed, id: Path<i64>, headers: HeaderMap) -> Result<Response, Fail> {
    picture(state, signed, id, 2048, headers).await
}

async fn edits(server: &Server, photo: &Photo) -> Result<Option<String>, Fail> {
    if !photo.edited {
        return Ok(None);
    }
    let id = photo.id;
    server.home.ask(move |catalog| catalog.edits_json(id)).await?.map_err(Fail::internal)
}

async fn picture(State(server): State<Arc<Server>>, Signed(me): Signed, Path(id): Path<i64>, edge: u32, headers: HeaderMap) -> Result<Response, Fail> {
    let photo = server.home.photo(&me.name, id)?;
    let edits = edits(&server, &photo).await?;
    let tag = format!("\"{:x}\"", fnv(&format!("{}\0{}\0{edge}\0{}", photo.path.display(), photo.mtime, edits.as_deref().unwrap_or(""))));
    if headers.get(header::IF_NONE_MATCH).is_some_and(|asked| asked.as_bytes() == tag.as_bytes()) {
        return Ok((StatusCode::NOT_MODIFIED, [(header::ETAG, tag)]).into_response());
    }
    let bytes = match thumbs::cached(&photo.path, photo.mtime, edge, edits.as_deref()) {
        Some(file) => tokio::fs::read(file).await.ok(),
        None => None,
    };
    let bytes = match bytes {
        Some(bytes) => bytes,
        None => {
            let _turn = server.decodes.acquire().await.map_err(Fail::internal)?;
            tokio::task::spawn_blocking(move || -> Result<Vec<u8>, String> {
                let image = thumbs::load(&photo.path, photo.mtime, edge, edits.as_deref())?;

                if let Some(bytes) = thumbs::cached(&photo.path, photo.mtime, edge, edits.as_deref()).and_then(|file| std::fs::read(file).ok()) {
                    return Ok(bytes);
                }
                let mut bytes = Vec::new();
                image.write_to(&mut std::io::Cursor::new(&mut bytes), image::ImageFormat::Jpeg).map_err(|err| err.to_string())?;
                Ok(bytes)
            })
            .await
            .map_err(Fail::internal)?
            .map_err(|err| Fail(StatusCode::UNPROCESSABLE_ENTITY, format!("Numa could not read this photograph: {err}")))?
        }
    };
    let headers = [(header::CONTENT_TYPE, "image/jpeg".to_string()), (header::CACHE_CONTROL, "private, no-cache".to_string()), (header::ETAG, tag)];
    Ok((headers, bytes).into_response())
}

fn fnv(text: &str) -> u64 {
    text.bytes().fold(0xcbf2_9ce4_8422_2325u64, |hash, byte| (hash ^ byte as u64).wrapping_mul(0x100_0000_01b3))
}

pub async fn original(State(server): State<Arc<Server>>, Signed(me): Signed, Path(id): Path<i64>) -> Result<Response, Fail> {
    let photo = server.home.photo(&me.name, id)?;
    let name = photo.path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
    let file = tokio::fs::File::open(&photo.path).await.map_err(|_| Fail(StatusCode::NOT_FOUND, "The file is no longer there.".into()))?;
    let length = file.metadata().await.map(|meta| meta.len()).ok();
    let mut response = Body::from_stream(tokio_util::io::ReaderStream::new(file)).into_response();
    let headers = response.headers_mut();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static(kind(&photo.path)));
    headers.insert(header::CONTENT_DISPOSITION, attachment(&name));
    if let Some(length) = length {
        headers.insert(header::CONTENT_LENGTH, HeaderValue::from(length));
    }
    Ok(response)
}

fn kind(path: &FilePath) -> &'static str {
    match path.extension().map(|ext| ext.to_string_lossy().to_lowercase()).as_deref() {
        Some("jpg" | "jpeg") => "image/jpeg",
        Some("png") => "image/png",
        Some("heic" | "heif" | "hif") => "image/heif",
        Some("tif" | "tiff") => "image/tiff",
        _ => "application/octet-stream",
    }
}

pub fn attachment(name: &str) -> HeaderValue {
    let plain: String = name.chars().map(|c| if c.is_ascii_alphanumeric() || "._- ".contains(c) { c } else { '_' }).collect();
    let encoded: String = name
        .bytes()
        .map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'.' | b'-' | b'_' => (byte as char).to_string(),
            _ => format!("%{byte:02X}"),
        })
        .collect();
    HeaderValue::from_str(&format!("attachment; filename=\"{plain}\"; filename*=UTF-8''{encoded}")).unwrap_or(HeaderValue::from_static("attachment"))
}

pub async fn jpeg(State(server): State<Arc<Server>>, Signed(me): Signed, Path(id): Path<i64>) -> Result<Response, Fail> {
    let photo = server.home.photo(&me.name, id)?;
    if !photo.edited && kind(&photo.path) == "image/jpeg" {
        return original(State(server), Signed(me), Path(id)).await;
    }
    let edits = match edits(&server, &photo).await? {
        Some(edits) => edits,
        None => serde_json::to_string(&numa_core::document::Document::new(photo.path.to_string_lossy().into_owned())).map_err(Fail::internal)?,
    };
    let stem = photo.path.file_stem().map(|stem| stem.to_string_lossy().into_owned()).unwrap_or_else(|| "photograph".into());
    let scratch = server.data.join("tmp").join(format!("{id}-{}.jpg", std::process::id()));
    let _turn = server.renders.acquire().await.map_err(Fail::internal)?;
    let bytes = tokio::task::spawn_blocking(move || -> Result<Vec<u8>, String> {
        let image = thumbs::render(&photo.path, u32::MAX, &edits)?;
        std::fs::create_dir_all(scratch.parent().unwrap_or(&scratch)).map_err(|err| err.to_string())?;
        let written = numa_io::export::save(&image, &scratch, &numa_io::export::ExportSettings::default(), Some(&photo.path));
        let bytes = written.and_then(|()| std::fs::read(&scratch).map_err(|err| err.to_string()));
        let _ = std::fs::remove_file(&scratch);
        bytes
    })
    .await
    .map_err(Fail::internal)?
    .map_err(|err| Fail(StatusCode::UNPROCESSABLE_ENTITY, format!("Numa could not develop this photograph: {err}")))?;
    let headers = [(header::CONTENT_TYPE, HeaderValue::from_static("image/jpeg")), (header::CONTENT_DISPOSITION, attachment(&format!("{stem}.jpg")))];
    Ok((headers, bytes).into_response())
}

pub async fn edits_put(State(server): State<Arc<Server>>, Signed(me): Signed, Path(id): Path<i64>, body: Bytes) -> Result<Json<Value>, Fail> {
    let photo = server.home.photo(&me.name, id)?;
    let mut document = match body.iter().all(u8::is_ascii_whitespace) {
        true => numa_core::document::Document::new(String::new()),
        false => serde_json::from_slice::<numa_core::document::Document>(&body)
            .map_err(|err| Fail::bad(format!("That is not an edit stack Numa reads: {err}")))?,
    };

    document.source.path = photo.path.to_string_lossy().into_owned();
    document.moment = None;
    changed(&server, id, move |catalog| catalog.save_edits(id, &document)).await
}

pub async fn edits_get(State(server): State<Arc<Server>>, Signed(me): Signed, Path(id): Path<i64>) -> Result<Json<Value>, Fail> {
    let photo = server.home.photo(&me.name, id)?;

    let edits = server.home.ask(move |catalog| catalog.edits_json(id)).await?.map_err(Fail::internal)?;
    let edits: Option<Value> = edits.map(|edits| serde_json::from_str(&edits)).transpose().map_err(Fail::internal)?;
    Ok(Json(json!({ "edits": edits, "updated": updated(&server, &photo) })))
}

#[derive(Deserialize)]
pub struct Marks {
    rating: u8,
    flag: String,
}

pub async fn marks(State(server): State<Arc<Server>>, Signed(me): Signed, Path(id): Path<i64>, Json(marks): Json<Marks>) -> Result<Json<Value>, Fail> {
    server.home.photo(&me.name, id)?;
    let flag = flag_from(&marks.flag)?;
    if marks.rating > 5 {
        return Err(Fail::bad("A rating is 0 to 5 stars."));
    }
    changed(&server, id, move |catalog| catalog.set_rating(id, marks.rating).and_then(|()| catalog.set_flag(id, flag))).await
}

pub async fn changed(
    server: &Arc<Server>,
    id: i64,
    change: impl FnOnce(&numa_io::catalog::Catalog) -> Result<(), String> + Send + 'static,
) -> Result<Json<Value>, Fail> {
    let home = server.clone();
    server
        .home
        .ask(move |catalog| {
            let done = change(catalog);
            home.shelve(Some(library_of(id)), catalog);
            done
        })
        .await?
        .map_err(Fail::internal)?;
    server.jobs.nudge();
    Ok(Json(json!({ "updated": server.store.touch(id) })))
}

pub fn updated(server: &Server, photo: &Photo) -> i64 {
    server.store.updated().get(&photo.id).copied().unwrap_or(0).max(photo.mtime)
}

pub fn flag_from(name: &str) -> Result<Flag, Fail> {
    match name {
        "none" => Ok(Flag::None),
        "pick" => Ok(Flag::Picked),
        "reject" => Ok(Flag::Rejected),
        _ => Err(Fail::bad("A flag is none, pick or reject.")),
    }
}

pub fn flag_name(flag: Flag) -> &'static str {
    match flag {
        Flag::None => "none",
        Flag::Picked => "pick",
        Flag::Rejected => "reject",
    }
}
