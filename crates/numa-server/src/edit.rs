use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::header;
use axum::response::{IntoResponse, Response};
use axum::Json;
use numa_core::document::Document;
use numa_io::catalog::Photo;
use numa_render::auto::{self, evidence::Refusal, plan, scene::Scene};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::auth::Signed;
use crate::photos::changed;
use crate::{Fail, Server};

const PROXY_EDGE: u32 = 1600;

const AUTO_EDGE: u32 = 900;

#[derive(Deserialize)]
pub struct Change {

    #[serde(default)]
    set: HashMap<String, f32>,

    #[serde(default)]
    turn: bool,

    #[serde(default)]
    turns: i32,
    #[serde(default)]
    auto: bool,

    #[serde(default)]
    level: bool,

    #[serde(default)]
    crop: Option<[f32; 4]>,

    #[serde(default)]
    view: Option<[f32; 4]>,
    #[serde(default)]
    angle: Option<f32>,
}

#[derive(Default)]
pub struct Editing(std::sync::Mutex<HashMap<i64, Arc<tokio::sync::Mutex<()>>>>);

impl Editing {
    pub async fn lock(&self, id: i64) -> tokio::sync::OwnedMutexGuard<()> {
        let lock = self.0.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).entry(id).or_default().clone();
        lock.lock_owned().await
    }
}

fn frame_size(document: &Document) -> Result<(f32, f32), String> {
    let (proxy, _) = numa_io::previews::editor_proxy(std::path::Path::new(&document.source.path), PROXY_EDGE)?;
    Ok(oriented(document, proxy.width, proxy.height))
}

fn oriented(document: &Document, width: u32, height: u32) -> (f32, f32) {
    match document.rotation() as i32 {
        90 | 270 => (height as f32, width as f32),
        _ => (width as f32, height as f32),
    }
}

fn quarter(document: &mut Document) {
    document.set_rotation(document.rotation() + 90.0);
    let mut masks = document.masks();
    for mask in masks.iter_mut() {
        mask.turn();
    }
    document.set_masks(masks);
    if let Some(([x, y, width, height], angle)) = document.crop() {
        document.set_crop([1.0 - y - height, x, height, width], angle);
    }
}

fn reframe(document: &mut Document, rect: [f32; 4], angle: f32, (width, height): (f32, f32)) {
    let was = document.crop().unwrap_or(([0.0, 0.0, 1.0, 1.0], 0.0));
    document.set_crop(rect, angle);
    let mut masks = document.masks();
    if masks.is_empty() {
        return;
    }
    let map = numa_core::image::between_frames(width, height, (rect, angle), was, document.perspective());
    let (wide, high) = numa_render::raster_size((rect[2] * width).max(1.0) as u32, (rect[3] * height).max(1.0) as u32, numa_render::MASK_RASTER);
    for mask in masks.iter_mut() {
        mask.remap(map, wide, high);
    }
    document.set_masks(masks);
}

pub async fn frame(State(server): State<Arc<Server>>, Signed(me): Signed, Path(id): Path<i64>) -> Result<Response, Fail> {
    let photo = server.home.photo(&me.name, id)?;
    let document = document(&server, &photo).await?;
    let _one = server.renders.acquire().await.map_err(Fail::internal)?;
    let (jpeg, view, angle) = tokio::task::spawn_blocking(move || -> Result<_, String> {
        let (rect, angle) = document.crop().unwrap_or(([0.0, 0.0, 1.0, 1.0], 0.0));
        let mut whole = document.clone();
        whole.set_crop([0.0, 0.0, 1.0, 1.0], 0.0);
        whole.set_masks(Vec::new());
        let json = serde_json::to_string(&whole).map_err(|err| err.to_string())?;
        let image = numa_io::thumbs::render(&photo.path, 2048, &json)?;
        let (width, height) = (image.width() as f32, image.height() as f32);
        let view = numa_core::image::crop_in_view(width, height, rect, angle, document.perspective());
        let mut jpeg = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut jpeg, 88).encode_image(&image).map_err(|err| err.to_string())?;
        Ok((jpeg, view, angle))
    })
    .await
    .map_err(Fail::internal)?
    .map_err(Fail::internal)?;
    let said = format!("{},{},{},{},{angle}", view[0], view[1], view[2], view[3]);
    Ok(([(header::CONTENT_TYPE, "image/jpeg".to_string()), (header::CACHE_CONTROL, "no-store".to_string()), (header::HeaderName::from_static("x-numa-crop"), said)], jpeg).into_response())
}

pub async fn get(State(server): State<Arc<Server>>, Signed(me): Signed, Path(id): Path<i64>) -> Result<Json<Value>, Fail> {
    let photo = server.home.photo(&me.name, id)?;
    let document = document(&server, &photo).await?;
    Ok(Json(shown(&document)))
}

pub async fn post(State(server): State<Arc<Server>>, Signed(me): Signed, Path(id): Path<i64>, Json(change): Json<Change>) -> Result<Json<Value>, Fail> {
    let photo = server.home.photo(&me.name, id)?;
    let _one = server.editing.lock(id).await;
    let mut document = document(&server, &photo).await?;
    if !change.set.is_empty() {
        let mut basic = document.basic();
        for (name, value) in &change.set {
            match name.as_str() {
                "exposure" => basic.tone.exposure = value.clamp(-5.0, 5.0),
                "contrast" => basic.tone.contrast = value.clamp(-100.0, 100.0),
                "highlights" => basic.tone.highlights = value.clamp(-100.0, 100.0),
                "shadows" => basic.tone.shadows = value.clamp(-100.0, 100.0),
                "whites" => basic.tone.whites = value.clamp(-100.0, 100.0),
                "blacks" => basic.tone.blacks = value.clamp(-100.0, 100.0),
                "vibrance" => basic.presence.vibrance = value.clamp(-100.0, 100.0),
                _ => return Err(Fail::bad(format!("There is no slider called {name}."))),
            }
        }
        document.set_basic(basic);
    }
    for _ in 0..(change.turns + i32::from(change.turn)).rem_euclid(4) {
        quarter(&mut document);
    }
    if change.crop.is_some() || change.view.is_some() {
        let probe = document.clone();
        let size = tokio::task::spawn_blocking(move || frame_size(&probe)).await.map_err(Fail::internal)?.map_err(Fail::internal)?;
        let perspective = document.perspective();
        let (rect, angle) = match (change.view, change.crop) {

            (Some(view), _) => {
                let angle = change.angle.unwrap_or(0.0).clamp(-45.0, 45.0);
                let rect = numa_core::image::crop_from_view(size.0, size.1, view, angle, perspective);
                (numa_core::image::crop_inside(rect, angle, perspective, size.0, size.1), angle)
            }
            (None, Some(rect)) => (rect, document.crop().map_or(0.0, |(_, angle)| angle)),
            (None, None) => unreachable!("one of them, above"),
        };
        reframe(&mut document, rect, angle, size);
    }
    let mut crops = Vec::new();
    if change.auto {
        (document, crops) = with_scene(&server, id, document, press).await?;
    }
    let mut straightened = None;
    if change.level {
        (document, straightened) = with_scene(&server, id, document, straighten).await?;
    }
    let saved = document.clone();
    let updated = changed(&server, id, move |catalog| catalog.save_edits(id, &saved)).await?;
    let mut answer = shown(&document);
    answer["updated"] = updated.0["updated"].clone();
    answer["crops"] = json!(crops);
    answer["straightened"] = json!(straightened);
    Ok(Json(answer))
}

#[allow(clippy::type_complexity)]
async fn with_scene<T: Send + 'static>(
    server: &Server,
    id: i64,
    document: Document,
    work: fn(Document, Option<Scene>) -> Result<(Document, Option<Scene>, T), String>,
) -> Result<(Document, T), Fail> {
    let asked = Scene::asked(&document);
    let kept = server.home.ask(move |catalog| catalog.scene(id, &asked)).await?;
    let _one = server.renders.acquire().await.map_err(Fail::internal)?;
    let (document, read, made) = tokio::task::spawn_blocking(move || work(document, kept)).await.map_err(Fail::internal)?.map_err(Fail::internal)?;
    if let Some(read) = read {
        server.home.ask(move |catalog| catalog.save_scene(id, &read)).await?.map_err(Fail::internal)?;
    }
    Ok((document, made))
}

async fn document(server: &Server, photo: &Photo) -> Result<Document, Fail> {
    let id = photo.id;
    let kept = server.home.ask(move |catalog| catalog.edits_json(id)).await?.map_err(Fail::internal)?;
    let mut document = match kept {
        Some(kept) => serde_json::from_str(&kept).map_err(Fail::internal)?,
        None => Document::new(String::new()),
    };
    document.source.path = photo.path.to_string_lossy().into_owned();
    document.moment = None;
    Ok(document)
}

fn shown(document: &Document) -> Value {
    let basic = document.basic();
    json!({
        "sliders": {
            "exposure": basic.tone.exposure,
            "contrast": basic.tone.contrast,
            "highlights": basic.tone.highlights,
            "shadows": basic.tone.shadows,
            "whites": basic.tone.whites,
            "blacks": basic.tone.blacks,
            "vibrance": basic.presence.vibrance,
        },
        "rotation": document.rotation(),
        "crop": document.crop().map(|(rect, _)| rect),
        "angle": document.crop().map_or(0.0, |(_, angle)| angle),
    })
}

#[allow(clippy::type_complexity)]
fn press(mut document: Document, kept: Option<Scene>) -> Result<(Document, Option<Scene>, Vec<Value>), String> {
    let path = std::path::PathBuf::from(&document.source.path);
    let (proxy, size) = numa_io::previews::editor_proxy(&path, PROXY_EDGE)?;
    let inputs = numa_io::inputs::render_inputs(&document);
    let working = numa_render::to_working_space(&document, &*proxy, &inputs);

    let frame = numa_render::apply_stack(&framing(&document), &working, 1.0);
    let found = numa_render::segment::of(&frame).map(Arc::new);
    let (width, height) = numa_render::raster_size(frame.width(), frame.height(), numa_render::MASK_RASTER);
    let subject = found
        .as_ref()
        .and_then(|found| auto::subject_mask(found, &frame, width, height))
        .and_then(|mask| mask.map.0)
        .map(|stored| stored.to_alpha());
    let small = working.downscaled(AUTO_EDGE).unwrap_or_else(|| working.clone());
    let _applied = auto::tone(&small, subject.as_ref(), &document).apply(&mut document);
    let read = match kept {
        Some(_) => None,
        None => Some(auto::scene::read(&document, &working, found.filter(|_| Scene::same_frame(&document)), Default::default())),
    };
    let scene = kept.as_ref().or(read.as_ref()).expect("kept or read just now");
    if let plan::Level::Apply { rect, angle, .. } = plan::level(scene, &document) {
        let size = oriented(&document, working.width, working.height);
        reframe(&mut document, rect, angle, size);
        plan::apply(&mut document, rect, angle);
    }

    let crops = match document.retouch().is_identity() {
        true => {
            let (rect, angle) = document.crop().unwrap_or(([0.0, 0.0, 1.0, 1.0], 0.0));
            auto::crop::proposals(scene, rect, angle, document.perspective(), size).0
        }
        false => Vec::new(),
    };
    let crops = crops.into_iter().map(|crop| json!({ "rect": crop.rect, "aspect": crop.aspect, "why": crop.why })).collect();
    Ok((document, read, crops))
}

#[allow(clippy::type_complexity)]
fn straighten(mut document: Document, kept: Option<Scene>) -> Result<(Document, Option<Scene>, Option<f32>), String> {
    let path = std::path::PathBuf::from(&document.source.path);
    let (proxy, _) = numa_io::previews::editor_proxy(&path, PROXY_EDGE)?;
    let working = numa_render::to_working_space(&document, &*proxy, &numa_io::inputs::render_inputs(&document));
    let read = match kept {
        Some(_) => None,
        None => Some(auto::scene::read(&document, &working, None, Default::default())),
    };
    let scene = kept.as_ref().or(read.as_ref()).expect("kept or read just now");
    let current = document.crop().map_or(0.0, |(_, angle)| angle);
    let mut asked = document.clone();
    asked.auto.get_or_insert_with(Default::default).angle = Some(current);
    let turned = match plan::level(scene, &asked) {
        plan::Level::Apply { rect, angle, .. } | plan::Level::Offer { rect, angle, .. } => {
            let size = oriented(&document, working.width, working.height);
            reframe(&mut document, rect, angle, size);
            plan::apply(&mut document, rect, angle);
            Some(angle - current)
        }
        plan::Level::Leave(plan::Leave::Refused(Refusal::AlreadyLevel)) => Some(0.0),
        plan::Level::Leave(_) => None,
    };
    Ok((document, read, turned))
}

fn framing(document: &Document) -> Document {
    let mut geometry = Document::new(document.source.path.clone());
    geometry.set_perspective(document.perspective());
    if let Some((rect, angle)) = document.crop() {
        geometry.set_crop(rect, angle);
    }
    geometry.set_rotation(document.rotation());
    geometry.set_mirrored(document.mirrored());
    geometry
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn edits_to_one_photograph_take_turns() {
        let editing = Arc::new(Editing::default());
        let kept = Arc::new(std::sync::Mutex::new(0u32));
        let tasks: Vec<_> = (0..64)
            .map(|_| {
                let (editing, kept) = (editing.clone(), kept.clone());
                tokio::spawn(async move {
                    let _one = editing.lock(7).await;
                    let read = *kept.lock().unwrap();
                    tokio::time::sleep(std::time::Duration::from_millis(1)).await;
                    *kept.lock().unwrap() = read + 1;
                })
            })
            .collect();
        for task in tasks {
            task.await.unwrap();
        }
        assert_eq!(*kept.lock().unwrap(), 64);
    }

    #[test]
    fn four_quarter_turns_are_none_and_the_crop_comes_round() {
        let mut document = Document::new(String::new());
        document.set_crop([0.1, 0.2, 0.5, 0.3], 2.0);
        let (was, angle) = document.crop().unwrap();
        quarter(&mut document);
        assert_eq!(document.rotation(), 90.0);
        let (turned, _) = document.crop().unwrap();
        assert!((turned[2] - was[3]).abs() < 1e-6 && (turned[3] - was[2]).abs() < 1e-6, "width and height trade places");
        for _ in 0..3 {
            quarter(&mut document);
        }
        let (back, again) = document.crop().unwrap();
        assert!(back.iter().zip(was).all(|(a, b)| (a - b).abs() < 1e-5), "{back:?} is not {was:?}");
        assert_eq!(angle, again);
        assert_eq!(document.rotation().rem_euclid(360.0), 0.0);
    }
}
