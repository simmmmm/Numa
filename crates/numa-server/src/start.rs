use std::collections::HashSet;
use std::path::{Path as FilePath, PathBuf};
use std::sync::Arc;

use axum::extract::{Query, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::auth::Signed;
use crate::store::Account;
use crate::{Fail, Server};

const STEPS: [&str; 4] = ["photographs", "household", "phones", "done"];
const KEY: &str = "first-start:";

const MADE: &str = "made-in-numa";

const CORE_SECONDS: f64 = 2.0;

fn key(name: &str) -> String {
    format!("{KEY}{name}")
}

pub fn step(server: &Server, name: &str) -> Option<String> {
    server.store.setting(&key(name)).filter(|step| step != "done")
}

pub fn begin(server: &Server, name: &str) -> Result<(), String> {
    let made: Vec<String> = crate::home::found_under(&server.home.photos)
        .into_iter()
        .filter(|folder| folder.join(numa_io::catalog::LIBRARY_DIR).is_dir())
        .map(|folder| relative(server, &folder))
        .collect();
    server.store.set_setting(MADE, &serde_json::to_string(&made).unwrap_or_default())?;
    server.store.set_setting(&key(name), STEPS[0])
}

pub fn may_read(server: &Server) -> bool {
    server.store.has_accounts() && !server.store.settings_under(KEY).iter().any(|(_, step)| step == STEPS[0])
}

fn relative(server: &Server, folder: &FilePath) -> String {
    folder.strip_prefix(&server.home.photos).unwrap_or(folder).to_string_lossy().into_owned()
}

fn admin(me: &Account) -> Result<(), Fail> {
    match me.admin {
        true => Ok(()),
        false => Err(Fail(StatusCode::FORBIDDEN, "Only an admin sets up Numa Home.".into())),
    }
}

fn library(server: &Server, folder: &str) -> Result<PathBuf, Fail> {
    let folder = server.home.photos.join(crate::send::folder_in(folder)?);
    match crate::home::found_under(&server.home.photos).contains(&folder) {
        true => Ok(folder),
        false => Err(Fail(StatusCode::NOT_FOUND, "There is no such library in the photos folder.".into())),
    }
}

pub async fn libraries(State(server): State<Arc<Server>>, Signed(me): Signed) -> Result<Json<Vec<Value>>, Fail> {
    admin(&me)?;
    let made: HashSet<String> = server.store.setting(MADE).and_then(|made| serde_json::from_str(&made).ok()).unwrap_or_default();
    let out = server.store.left_out();
    let rows: Vec<Value> = crate::home::found_under(&server.home.photos)
        .into_iter()
        .map(|folder| {
            let relative = relative(&server, &folder);
            let name = folder.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
            json!({ "folder": relative, "name": name, "numa": made.contains(&relative), "out": out.contains(&relative) })
        })
        .collect();
    Ok(Json(rows))
}

#[derive(Deserialize)]
pub struct Folder {
    folder: String,
}

pub async fn count(State(server): State<Arc<Server>>, Signed(me): Signed, Query(asked): Query<Folder>) -> Result<Json<Value>, Fail> {
    admin(&me)?;
    let folder = library(&server, &asked.folder)?;
    let read = tokio::task::spawn_blocking(move || counted(&folder)).await.map_err(Fail::internal)?;
    Ok(Json(read))
}

fn counted(folder: &FilePath) -> Value {
    let scan = numa_io::catalog::scan(folder, &Default::default());
    let twins = numa_io::analysis::twins(scan.files.iter().enumerate().map(|(n, (path, _, stamp))| (n as i64, path.as_path(), stamp.taken)), numa_io::raw::is_raw);
    let mut photos: Vec<(&PathBuf, Option<i64>)> =
        scan.files.iter().enumerate().filter(|(n, _)| !twins.contains_key(&(*n as i64))).map(|(_, (path, _, stamp))| (path, stamp.taken)).collect();
    photos.sort_by_key(|(path, taken)| (taken.is_none(), *taken, (*path).clone()));
    let taken: Vec<i64> = photos.iter().filter_map(|(_, taken)| *taken).collect();
    let n = photos.len();

    let mut show: Vec<usize> = vec![0, n / 2, n.saturating_sub(1)];
    show.dedup();
    let show: Vec<String> = show.into_iter().filter_map(|at| photos.get(at)).map(|(path, _)| path.strip_prefix(folder).unwrap_or(path).to_string_lossy().into_owned()).collect();
    json!({
        "photos": n, "first": taken.first(), "last": taken.last(), "show": show,
        "seconds": (n as f64 * CORE_SECONDS / crate::jobs::half_the_cores() as f64).round(),
    })
}

#[derive(Deserialize)]
pub struct Shown {
    folder: String,
    file: String,
}

pub async fn thumb(State(server): State<Arc<Server>>, Signed(me): Signed, Query(asked): Query<Shown>) -> Result<Response, Fail> {
    admin(&me)?;
    let path = library(&server, &asked.folder)?.join(crate::send::folder_in(&asked.file)?);
    if !path.is_file() {
        return Err(Fail(StatusCode::NOT_FOUND, "There is no such photograph.".into()));
    }
    crate::manage::embedded(&server, path, 256).await
}

#[derive(Deserialize)]
pub struct Moved {
    step: String,

    out: Option<Vec<String>>,
}

pub async fn moved(State(server): State<Arc<Server>>, Signed(me): Signed, Json(asked): Json<Moved>) -> Result<StatusCode, Fail> {
    admin(&me)?;
    if step(&server, &me.name).is_none() {
        return Err(Fail(StatusCode::CONFLICT, "Numa Home is already set up.".into()));
    }
    if !STEPS.contains(&asked.step.as_str()) {
        return Err(Fail::bad("The steps are photographs, household, phones and done."));
    }
    if let Some(out) = &asked.out {
        for folder in crate::home::found_under(&server.home.photos) {
            let folder = relative(&server, &folder);
            server.store.leave_out(&folder, out.contains(&folder)).map_err(Fail::internal)?;
        }
    }
    server.store.set_setting(&key(&me.name), &asked.step).map_err(Fail::internal)?;
    if asked.out.is_some() {
        let working = server.clone();
        tokio::task::spawn_blocking(move || crate::manage::reshelve(&working)).await.map_err(Fail::internal)?;
    }
    server.jobs.nudge();
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
pub struct Address {
    address: String,
}

pub async fn qr(Signed(_): Signed, Query(asked): Query<Address>) -> Result<Response, Fail> {
    let address = asked.address.trim();
    if !(address.starts_with("http://") || address.starts_with("https://")) || address.len() > 300 || address.chars().any(char::is_control) {
        return Err(Fail::bad("An address starts with http:// or https://."));
    }
    let code = qrcode::QrCode::new(address.as_bytes()).map_err(Fail::internal)?;
    let svg = code
        .render::<qrcode::render::svg::Color>()
        .min_dimensions(240, 240)
        .dark_color(qrcode::render::svg::Color("#000000"))
        .light_color(qrcode::render::svg::Color("#ffffff"))
        .build();
    Ok(([(header::CONTENT_TYPE, "image/svg+xml"), (header::CACHE_CONTROL, "private, max-age=3600")], svg).into_response())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_folder_is_counted_without_its_catalog() {
        let root = std::env::temp_dir().join(format!("numa-home-start-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(root.join(".numa")).unwrap();
        for name in ["a.jpg", "b.jpg", ".numa/c.jpg", "notes.txt"] {
            std::fs::write(root.join(name), b"").unwrap();
        }
        let read = counted(&root);

        assert_eq!(read["photos"], 2);
        assert_eq!(read["first"], Value::Null);
        assert_eq!(read["show"].as_array().unwrap().len(), 2);
        let _ = std::fs::remove_dir_all(&root);
    }
}
