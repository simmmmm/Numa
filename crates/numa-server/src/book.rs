use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::sync::Arc;

use axum::body::Body;
use axum::extract::{Query, State};
use axum::http::{header, HeaderValue};
use axum::response::{IntoResponse, Response};
use axum::Json;
use numa_io::book::{self, By, Chapter, Shot};
use numa_io::catalog::{library_of, Flag, Photo};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::auth::Signed;
use crate::store::Account;
use crate::timeline::Wanted;
use crate::{Fail, Server};

#[derive(Deserialize)]
pub struct Asked {

    ids: Option<String>,

    by: Option<String>,

    mm: Option<f32>,

    leave: Option<String>,
}

struct Book {
    name: String,
    by: By,
    albums: bool,
    chapters: Vec<Chapter>,
    photos: HashMap<i64, Photo>,
    edits: HashMap<i64, String>,
}

fn list(text: Option<&str>) -> Vec<i64> {
    text.unwrap_or("").split(',').filter_map(|one| one.trim().parse().ok()).collect()
}

async fn plan(server: &Arc<Server>, me: &Account, asked: &Asked, wanted: &Wanted) -> Result<Book, Fail> {
    let ids = list(asked.ids.as_deref());
    let photos: Vec<Photo> = match ids.is_empty() {
        false => crate::selection::chosen(server, me, &ids, false, false)?.into_iter().map(|(_, photo)| photo).collect(),
        true => crate::timeline::chosen(server, &me.name, wanted).await?.into_iter().filter(|photo| photo.flag == Flag::Picked).collect(),
    };
    if photos.is_empty() {
        return Err(Fail::bad("Pick the photographs for the book first (P), or choose them."));
    }
    let ids: Vec<i64> = photos.iter().map(|photo| photo.id).collect();
    let edited: Vec<i64> = photos.iter().filter(|photo| photo.edited).map(|photo| photo.id).collect();
    let (mut albums, edits) = server
        .home
        .ask(move |catalog| {
            let albums = catalog.albums_of(&ids).unwrap_or_default();
            let edits: HashMap<i64, String> = edited.into_iter().filter_map(|id| Some((id, catalog.edits_json(id).ok()??))).collect();
            (albums, edits)
        })
        .await?;
    let shots: Vec<Shot> = photos
        .iter()
        .map(|photo| Shot { id: photo.id, taken: photo.taken.unwrap_or(photo.mtime), albums: albums.remove(&photo.id).unwrap_or_default() })
        .collect();
    let in_albums = shots.iter().any(|shot| !shot.albums.is_empty());
    let by = match asked.by.as_deref() {
        Some("day") => By::Day,
        Some("album") => By::Album,
        _ if in_albums => By::Album,
        _ => By::Day,
    };

    let first = library_of(photos[0].id);
    let name = match photos.iter().all(|photo| library_of(photo.id) == first) {
        true => server.home.libraries(&me.name).into_iter().find(|library| library.id == first).map(|library| format!("{} Book", library.label())),
        false => None,
    };
    Ok(Book {
        name: name.unwrap_or_else(|| "Photo Book".into()),
        by,
        albums: in_albums,
        chapters: book::chapters(&shots, by),
        photos: photos.into_iter().map(|photo| (photo.id, photo)).collect(),
        edits,
    })
}

pub async fn show(State(server): State<Arc<Server>>, Signed(me): Signed, Query(asked): Query<Asked>, Query(wanted): Query<Wanted>) -> Result<Json<Value>, Fail> {
    let book = plan(&server, &me, &asked, &wanted).await?;
    let chapters: Vec<Value> = book.chapters.iter().map(|chapter| json!({ "name": chapter.name, "count": chapter.ids.len() })).collect();
    let products: Vec<Value> = book::PRODUCTS.iter().map(|product| json!({ "name": product.name, "mm": product.long_mm })).collect();
    Ok(Json(json!({
        "name": book.name,
        "count": book.photos.len(),
        "by": if book.by == By::Album { "album" } else { "day" },
        "albums": book.albums,
        "chapters": chapters,
        "products": products,
    })))
}

pub async fn zip(State(server): State<Arc<Server>>, Signed(me): Signed, Query(asked): Query<Asked>, Query(wanted): Query<Wanted>) -> Result<Response, Fail> {
    let book = plan(&server, &me, &asked, &wanted).await?;
    let edge = book::long_edge(asked.mm.unwrap_or(300.0).clamp(100.0, 1000.0));
    let leave: HashSet<usize> = list(asked.leave.as_deref()).into_iter().map(|at| at as usize).collect();
    let chapters: Vec<Chapter> = book.chapters.into_iter().enumerate().filter(|(at, _)| !leave.contains(at)).map(|(_, chapter)| chapter).collect();
    if chapters.is_empty() {
        return Err(Fail::bad("Leave at least one chapter in the book."));
    }
    let (tx, rx) = tokio::sync::mpsc::channel(4);
    let runtime = tokio::runtime::Handle::current();
    let (photos, edits, scratch) = (book.photos, book.edits, server.data.join("tmp"));
    let renders = server.clone();
    tokio::task::spawn_blocking(move || {
        let mut zip = zip::ZipWriter::new_stream(std::io::BufWriter::with_capacity(1 << 20, Pipe(tx)));
        let options = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        let mut left_out = Vec::new();
        for (at, chapter) in chapters.iter().enumerate() {
            let folder = book::folder(at + 1, &chapter.name);
            for (number, id) in chapter.ids.iter().enumerate() {
                let photo = &photos[id];
                let edits = edits.get(id).cloned().unwrap_or_else(|| {
                    serde_json::to_string(&numa_core::document::Document::new(photo.path.to_string_lossy().into_owned())).unwrap_or_default()
                });

                let developed = runtime.block_on(renders.renders.acquire()).map_err(|err| err.to_string()).and_then(|_turn| develop(photo, edge, &edits, &scratch));
                let name = format!("{folder}/{}.jpg", book::file(number + 1, &photo.path));
                match developed {
                    Ok(bytes) => {
                        if zip.start_file(name, options).is_err() || zip.write_all(&bytes).is_err() {
                            return;
                        }
                    }
                    Err(why) => left_out.push(format!("{name}: {why}")),
                }
            }
        }
        if !left_out.is_empty() && zip.start_file("Left Out.txt", options).is_ok() {
            let _ = zip.write_all(format!("Numa could not develop these:\n\n{}\n", left_out.join("\n")).as_bytes());
        }
        let _ = zip.finish();
    });
    let stream = futures_util::stream::unfold(rx, |mut rx| async move { rx.recv().await.map(|chunk| (chunk, rx)) });
    let mut response = Body::from_stream(stream).into_response();
    let headers = response.headers_mut();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("application/zip"));
    headers.insert(header::CONTENT_DISPOSITION, crate::photos::attachment(&format!("{}.zip", book.name)));
    Ok(response)
}

fn develop(photo: &Photo, edge: u32, edits: &str, scratch: &std::path::Path) -> Result<Vec<u8>, String> {
    let mut image = numa_io::thumbs::render(&photo.path, edge, edits)?;
    if image.width().max(image.height()) < edge {
        let whole = numa_io::thumbs::render(&photo.path, u32::MAX, edits)?;
        image = match whole.width().max(whole.height()) > edge {
            true => image::DynamicImage::ImageRgb8(whole).thumbnail(edge, edge).into_rgb8(),
            false => whole,
        };
    }
    let at = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |since| since.as_nanos());
    let file = scratch.join(format!("book-{}-{}-{at}.jpg", std::process::id(), photo.id));
    std::fs::create_dir_all(scratch).map_err(|err| err.to_string())?;
    let settings = numa_io::export::ExportSettings { quality: 95, ..Default::default() };
    let written = numa_io::export::save(&image, &file, &settings, Some(&photo.path));
    let bytes = written.and_then(|()| std::fs::read(&file).map_err(|err| err.to_string()));
    let _ = std::fs::remove_file(&file);
    bytes
}

struct Pipe(tokio::sync::mpsc::Sender<Result<Vec<u8>, std::io::Error>>);

impl Write for Pipe {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        self.0.blocking_send(Ok(bytes.to_vec())).map_err(|_| std::io::Error::from(std::io::ErrorKind::BrokenPipe))?;
        Ok(bytes.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
