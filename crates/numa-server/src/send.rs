use std::path::{Component, Path as FilePath, PathBuf};
use std::sync::Arc;

use axum::body::Body;
use axum::extract::{Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use futures_util::StreamExt;
use numa_io::catalog::{Library, Scan};
use serde::Deserialize;
use sha2::{Digest, Sha256};
use tokio::io::AsyncWriteExt;

use crate::auth::{self, Signed};
use crate::{Fail, Server};

const LARGEST: u64 = 4 << 30;

#[derive(Deserialize)]
pub struct Into {
    library: Option<i64>,
    dir: Option<String>,
}

fn file_name(asked: &str) -> Result<String, Fail> {
    let name = asked.rsplit(['/', '\\']).next().unwrap_or_default().trim();
    let fine = !name.is_empty() && name.len() <= 255 && !name.starts_with('.') && !name.chars().any(char::is_control);
    match fine {
        true => Ok(name.to_string()),
        false => Err(Fail::bad("That file name cannot be kept.")),
    }
}

pub fn folder_in(dir: &str) -> Result<PathBuf, Fail> {
    let path = FilePath::new(dir.trim_end_matches('/'));
    let mut inside = PathBuf::new();
    for part in path.components() {
        match part {
            Component::Normal(name) if !name.to_string_lossy().starts_with('.') && !name.to_string_lossy().chars().any(char::is_control) => {
                inside.push(name)
            }
            _ => return Err(Fail::bad("That folder is not inside the library.")),
        }
    }
    Ok(inside)
}

pub async fn send(
    State(server): State<Arc<Server>>,
    Signed(me): Signed,
    Path(asked): Path<String>,
    Query(into): Query<Into>,
    headers: HeaderMap,
    body: Body,
) -> Result<Response, Fail> {
    let name = file_name(&asked)?;
    if !numa_io::raw::is_supported(FilePath::new(&name)) {
        return Err(Fail(StatusCode::UNSUPPORTED_MEDIA_TYPE, format!("{name} is not a photograph Numa reads.")));
    }

    let told = match headers.get("x-numa-sha256") {
        None => None,
        Some(value) => Some(
            value
                .to_str()
                .ok()
                .map(|value| value.trim().to_lowercase())
                .filter(|value| value.len() == 64 && value.chars().all(|c| c.is_ascii_hexdigit()))
                .ok_or_else(|| Fail::bad("Send the file's SHA-256, in hex, as X-Numa-Sha256."))?,
        ),
    };
    let hinted: Option<i64> = headers.get("x-numa-taken").and_then(|value| value.to_str().ok()).and_then(|value| value.trim().parse().ok());
    let folder = into.dir.as_deref().map(folder_in).transpose()?;
    let library = destination(&server, &me.name, into.library).await?;

    let already = |sha256: &str| server.store.sent(&me.name, sha256, library.id).map(|kept| library.path.join(kept)).and_then(|path| server.home.at(&path));
    let duplicate = |id: i64| Ok((StatusCode::OK, Json(serde_json::json!({ "id": id, "duplicate": true }))).into_response());
    if let Some(id) = told.as_deref().and_then(already) {
        return duplicate(id);
    }

    let incoming = library.path.join(numa_io::catalog::LIBRARY_DIR).join("incoming");
    tokio::fs::create_dir_all(&incoming).await.map_err(Fail::internal)?;
    let part = incoming.join(format!("{}-{name}", &auth::token().0[..16]));
    let received = receive(body, &part).await;
    let got = match received {
        Ok(got) => got,
        Err(fail) => {
            let _ = tokio::fs::remove_file(&part).await;
            return Err(fail);
        }
    };
    if told.as_ref().is_some_and(|told| *told != got) {
        let _ = tokio::fs::remove_file(&part).await;
        return Err(Fail::bad("The file changed on the way: its SHA-256 is not the one sent with it."));
    }
    if let Some(id) = already(&got) {
        let _ = tokio::fs::remove_file(&part).await;
        return duplicate(id);
    }
    let sha256 = got;

    let placed = {
        let (library, name, sha256) = (library.clone(), name.clone(), sha256.clone());
        tokio::task::spawn_blocking(move || place(&part, &library, folder, &name, &sha256, hinted)).await.map_err(Fail::internal)?
    }?;
    let (target, duplicate) = placed;
    let kept = target.strip_prefix(&library.path).unwrap_or(&target).to_string_lossy().into_owned();
    server.store.note_sent(&me.name, &sha256, library.id, &kept).map_err(Fail::internal)?;

    if let Some(token) = auth::presented(&headers) {
        server.store.note_arrival(&auth::digest(&token));
    }

    let id = match server.home.at(&target) {
        Some(id) => id,
        None => catalogue(&server, &library, target).await?,
    };
    server.jobs.nudge();
    let status = if duplicate { StatusCode::OK } else { StatusCode::CREATED };
    Ok((status, Json(serde_json::json!({ "id": id, "duplicate": duplicate }))).into_response())
}

async fn destination(server: &Arc<Server>, account: &str, asked: Option<i64>) -> Result<Library, Fail> {
    let seen = server.home.libraries(account);
    if let Some(id) = asked {
        return seen.into_iter().find(|library| library.id == id).ok_or_else(|| Fail(StatusCode::NOT_FOUND, "There is no such library.".into()));
    }
    let own = server.home.sent_folder(account);
    if let Some(library) = seen.into_iter().find(|library| library.path == own) {
        return Ok(library);
    }
    std::fs::create_dir_all(&own).map_err(Fail::internal)?;
    let home = server.clone();
    server
        .home
        .ask(move |catalog| {
            let added = catalog.add_library(&own);
            home.shelve(added.as_ref().ok().map(|library| library.id), catalog);
            added
        })
        .await?
        .map_err(Fail::internal)
}

async fn receive(body: Body, part: &FilePath) -> Result<String, Fail> {
    let mut file = tokio::fs::File::create(part).await.map_err(Fail::internal)?;
    let mut hasher = Sha256::new();
    let mut length = 0u64;
    let mut stream = body.into_data_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|_| Fail::bad("The file stopped arriving part-way."))?;
        length += chunk.len() as u64;
        if length > LARGEST {
            return Err(Fail(StatusCode::PAYLOAD_TOO_LARGE, "That file is larger than any photograph.".into()));
        }
        hasher.update(&chunk);
        file.write_all(&chunk).await.map_err(Fail::internal)?;
    }

    file.sync_all().await.map_err(Fail::internal)?;
    Ok(auth::hex(&hasher.finalize()))
}

fn place(part: &FilePath, library: &Library, folder: Option<PathBuf>, name: &str, sha256: &str, hinted: Option<i64>) -> Result<(PathBuf, bool), Fail> {
    let folder = match folder {
        Some(folder) => library.path.join(folder),
        None => {
            let taken = numa_io::exif::taken(part).or(hinted).unwrap_or_else(crate::store::now);
            let day = numa_io::import::day(taken, 0);
            library.path.join(&day[..4]).join(day)
        }
    };
    std::fs::create_dir_all(&folder).map_err(Fail::internal)?;

    let (real, root) = (folder.canonicalize().map_err(Fail::internal)?, library.path.canonicalize().map_err(Fail::internal)?);
    if !real.starts_with(&root) {
        let _ = std::fs::remove_file(part);
        return Err(Fail::bad("That folder is not inside the library."));
    }
    let path = FilePath::new(name);
    let (stem, extension) = (path.file_stem().unwrap_or_default().to_string_lossy(), path.extension().map(|ext| ext.to_string_lossy()));
    for n in 1..1000 {
        let candidate = match (n, &extension) {
            (1, _) => folder.join(name),
            (_, Some(extension)) => folder.join(format!("{stem} ({n}).{extension}")),
            (_, None) => folder.join(format!("{stem} ({n})")),
        };
        if !candidate.exists() {
            std::fs::rename(part, &candidate).map_err(Fail::internal)?;
            return Ok((candidate, false));
        }
        if auth::sha256_of(&candidate).is_ok_and(|there| there == sha256) {
            let _ = std::fs::remove_file(part);
            return Ok((candidate, true));
        }
    }
    let _ = std::fs::remove_file(part);
    Err(Fail::bad("There are too many files of that name in that folder."))
}

async fn catalogue(server: &Arc<Server>, library: &Library, target: PathBuf) -> Result<i64, Fail> {
    let (library, home) = (library.clone(), server.clone());
    let stamp = {
        let target = target.clone();
        tokio::task::spawn_blocking(move || numa_io::exif::stamp(&target)).await.map_err(Fail::internal)?
    };
    let mtime = std::fs::metadata(&target)
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |since| since.as_secs() as i64);
    let scan = Scan { files: vec![(target.clone(), mtime, stamp)], complete: false, unchanged: false, folders: Vec::new() };
    server
        .home
        .ask(move |catalog| {
            let applied = catalog.apply_scan(&library, &scan);
            home.shelve(Some(library.id), catalog);
            applied
        })
        .await?
        .map_err(Fail::internal)?;
    server.home.at(&target).ok_or_else(|| Fail::internal(format!("{} is not in the catalog after its scan", target.display())))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_is_one_component_and_a_folder_stays_inside() {
        assert_eq!(file_name("../../etc/DSCF1.RAF").unwrap(), "DSCF1.RAF");
        assert_eq!(file_name("C:\\x\\IMG_1.HEIC").unwrap(), "IMG_1.HEIC");
        for bad in ["", "..", ".hidden.jpg", "a/", "bad\nname.jpg"] {
            assert!(file_name(bad).is_err(), "{bad:?}");
        }
        assert_eq!(folder_in("2026/Holiday/").unwrap(), PathBuf::from("2026/Holiday"));
        for bad in ["../x", "a/../../b", "/etc", ".numa", "a/.git", "./a"] {
            assert!(folder_in(bad).is_err(), "{bad:?}");
        }
    }
}
