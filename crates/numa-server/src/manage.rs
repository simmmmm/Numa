use std::collections::HashSet;
use std::path::{Path as FilePath, PathBuf};
use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use numa_io::catalog::{Catalog, Library, Photo};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::auth::Signed;
use crate::store::{now, Account};
use crate::{Fail, Server};

const KEEP_DAYS: i64 = 30;

pub fn within(library: &Library, path: &FilePath) -> String {
    path.strip_prefix(&library.path).unwrap_or(path).to_string_lossy().into_owned()
}

pub fn plain(name: &str) -> bool {
    !name.is_empty() && !name.starts_with('.') && !name.contains(['/', '\\']) && !name.chars().any(char::is_control) && name.len() <= 120
}

fn year(name: &str) -> bool {
    name.len() == 4 && name.chars().all(|c| c.is_ascii_digit())
}

pub fn may_manage(server: &Server, me: &Account, library: &Library) -> bool {
    me.admin || library.path == server.home.sent_folder(&me.name)
}

pub fn refused() -> Fail {
    Fail(StatusCode::FORBIDDEN, "Only an admin, or the account whose library it is, may change it.".into())
}

fn admin(me: &Account) -> Result<(), Fail> {
    match me.admin {
        true => Ok(()),
        false => Err(Fail(StatusCode::FORBIDDEN, "Only an admin changes the libraries in the photos folder.".into())),
    }
}

fn take_in(server: &Arc<Server>, folder: PathBuf) -> Result<Library, Fail> {
    server.home.set_found(crate::home::found_under(&server.home.photos));
    let home = server.clone();
    let library = server
        .home
        .blocking(move |catalog| {
            let library = catalog.add_library(&folder);
            home.shelve(None, catalog);
            library
        })
        .ok_or_else(|| Fail::internal("the catalog stopped"))?
        .map_err(Fail::internal)?;
    crate::jobs::rescan(server, library.clone());
    server.jobs.nudge();
    Ok(library)
}

fn new_folders(server: &Server) -> Vec<PathBuf> {
    let shelved: HashSet<PathBuf> = server.home.all_libraries().into_iter().map(|library| library.path).collect();
    crate::home::found_under(&server.home.photos).into_iter().filter(|folder| !shelved.contains(folder)).collect()
}

pub async fn folders(State(server): State<Arc<Server>>, Signed(me): Signed) -> Result<Json<Vec<Value>>, Fail> {
    admin(&me)?;
    let photos = server.home.photos.clone();
    let rows = new_folders(&server).into_iter().map(|folder| json!({ "folder": folder.strip_prefix(&photos).unwrap_or(&folder).to_string_lossy() }));
    Ok(Json(rows.collect()))
}

#[derive(Deserialize)]
pub struct Made {
    name: Option<String>,
    year: Option<String>,
    folder: Option<String>,
}

pub async fn add(State(server): State<Arc<Server>>, Signed(me): Signed, Json(made): Json<Made>) -> Result<Response, Fail> {
    let folder = match (&made.folder, made.name.as_deref().map(str::trim)) {
        (Some(folder), _) => {
            admin(&me)?;
            let folder = server.home.photos.join(crate::send::folder_in(folder)?);
            if !new_folders(&server).contains(&folder) {
                return Err(Fail::bad("That folder is not one to take in: it is a library already, or inside one."));
            }
            folder
        }
        (None, Some(name)) if plain(name) => {
            let year = made.year.as_deref().map(str::trim).filter(|year| !year.is_empty());
            if year.is_some_and(|year| !self::year(year)) {
                return Err(Fail::bad("A year is four digits."));
            }
            if year.is_none() && self::year(name) {
                return Err(Fail::bad("Four digits alone are a year's folder; give the library a name."));
            }
            let folder = year.map_or(server.home.photos.clone(), |year| server.home.photos.join(year)).join(name);
            if folder.exists() {
                return Err(Fail(StatusCode::CONFLICT, "There is a folder of that name already.".into()));
            }
            std::fs::create_dir_all(&folder).map_err(Fail::internal)?;
            folder
        }
        _ => return Err(Fail::bad("A library's name is one plain name.")),
    };
    let taking = server.clone();
    let library = tokio::task::spawn_blocking(move || take_in(&taking, folder)).await.map_err(Fail::internal)??;
    let row = json!({ "id": library.id, "name": library.label(), "count": 0, "mine": false });
    Ok((StatusCode::CREATED, Json(row)).into_response())
}

#[derive(Deserialize)]
pub struct Renamed {
    name: String,
}

pub async fn rename(State(server): State<Arc<Server>>, Signed(me): Signed, Path(id): Path<i64>, Json(renamed): Json<Renamed>) -> Result<Json<Value>, Fail> {
    let library = server.home.libraries(&me.name).into_iter().find(|library| library.id == id).ok_or_else(|| Fail(StatusCode::NOT_FOUND, "There is no such library.".into()))?;
    if !may_manage(&server, &me, &library) {
        return Err(refused());
    }
    if !library.path.starts_with(&server.home.photos) {
        return Err(Fail::bad("A Sent Home library is named after its account."));
    }
    let name = renamed.name.trim().to_string();
    if !plain(&name) || (library.path.parent() == Some(server.home.photos.as_path()) && year(&name)) {
        return Err(Fail::bad("A library's name is one plain name."));
    }
    let home = server.clone();
    let photos = server.home.photos.clone();
    server
        .home
        .ask(move |catalog| {
            let renamed = catalog.rename_library_folder(id, &name);
            home.home.set_found(crate::home::found_under(&photos));
            home.shelve(None, catalog);
            renamed
        })
        .await?
        .map_err(|err| Fail(StatusCode::CONFLICT, format!("Could not rename it: {err}.")))?;
    Ok(Json(json!({ "id": id, "name": renamed.name.trim() })))
}

#[derive(Serialize, Deserialize, Default)]
pub struct Kept {
    rating: u8,
    flag: String,
    edits: Option<String>,

    albums: Vec<String>,
    pub author: Option<String>,
}

pub fn keep(catalog: &Catalog, photo: &Photo, author: Option<String>) -> Kept {
    let names = catalog.albums_of(&[photo.id]).unwrap_or_default().remove(&photo.id).unwrap_or_default();
    let albums = catalog.albums().unwrap_or_default().into_iter().filter(|(_, name)| names.contains(name)).map(|(key, _)| key).collect();
    let edits = catalog.edits_json(photo.id).ok().flatten();
    Kept { rating: photo.rating, flag: crate::photos::flag_name(photo.flag).into(), edits, albums, author }
}

pub fn give(catalog: &Catalog, id: i64, path: &FilePath, kept: &Kept) {
    let _ = catalog.set_rating(id, kept.rating);
    if let Ok(flag) = crate::photos::flag_from(&kept.flag) {
        let _ = catalog.set_flag(id, flag);
    }
    if let Some(mut document) = kept.edits.as_deref().and_then(|edits| serde_json::from_str::<numa_core::document::Document>(edits).ok()) {
        document.source.path = path.to_string_lossy().into_owned();
        document.moment = None;
        let _ = catalog.save_edits(id, &document);
    }
    for key in &kept.albums {
        let _ = catalog.set_in_album(key, &[id], true);
    }
}

pub fn free(path: PathBuf) -> PathBuf {
    if !path.exists() {
        return path;
    }
    let stem = path.file_stem().map(|stem| stem.to_string_lossy().into_owned()).unwrap_or_default();
    let ext = path.extension().map(|ext| format!(".{}", ext.to_string_lossy())).unwrap_or_default();
    (2..).map(|n| path.with_file_name(format!("{stem} {n}{ext}"))).find(|path| !path.exists()).expect("a free name")
}

pub fn move_file(from: &FilePath, to: &FilePath) -> std::io::Result<()> {
    if let Some(parent) = to.parent() {
        std::fs::create_dir_all(parent)?;
    }
    if std::fs::rename(from, to).is_ok() {
        return Ok(());
    }
    let copied = std::fs::copy(from, to)?;
    if copied != std::fs::metadata(from)?.len() {
        let _ = std::fs::remove_file(to);
        return Err(std::io::Error::other("the copy came out short"));
    }
    if let Ok(modified) = std::fs::metadata(from).and_then(|meta| meta.modified()) {
        let _ = std::fs::File::options().write(true).open(to).and_then(|file| file.set_modified(modified));
    }
    std::fs::remove_file(from)
}

fn trash_dir(library: &Library) -> PathBuf {
    library.path.join(numa_io::catalog::LIBRARY_DIR).join("trash")
}

fn in_trash(library: &Library, row: &crate::store::Trashed) -> PathBuf {
    let name = FilePath::new(&row.path).file_name().map(|name| name.to_os_string()).unwrap_or_default();
    trash_dir(library).join(row.id.to_string()).join(name)
}

pub fn trash(server: &Arc<Server>, photos: Vec<(Library, Photo)>) -> usize {
    let authors = server.store.authors();
    let mut libraries = HashSet::new();
    let mut done = 0;
    for (library, photo) in photos {
        let path = within(&library, &photo.path);
        let author = authors.get(&(library.id, path.clone())).cloned();
        let held = photo.clone();
        let Some(kept) = server.home.blocking(move |catalog| keep(catalog, &held, author)) else { continue };
        let Ok(row) = server.store.trash(library.id, &path, &serde_json::to_string(&kept).unwrap_or_default()) else { continue };
        let row = crate::store::Trashed { id: row, library: library.id, path: path.clone(), kept: String::new(), at: 0 };
        if let Err(err) = move_file(&photo.path, &in_trash(&library, &row)) {
            log::warn!("{}: {err}", photo.path.display());
            server.store.untrash(row.id);
            continue;
        }
        let _ = server.store.set_author(library.id, &path, None);
        server.home.blocking(move |catalog| catalog.remove_photo(photo.id));
        libraries.insert(library.id);
        done += 1;
    }
    shelve(server, libraries);
    done
}

pub fn shelve(server: &Arc<Server>, libraries: HashSet<i64>) {
    let home = server.clone();
    server.home.blocking(move |catalog| libraries.into_iter().for_each(|id| home.shelve(Some(id), catalog)));
}

pub fn empty_old(server: &Server) -> usize {
    let libraries = server.home.all_libraries();
    let mut emptied = 0;
    for row in server.store.trashed().into_iter().filter(|row| now() - row.at > KEEP_DAYS * 86_400) {
        if let Some(library) = libraries.iter().find(|library| library.id == row.library) {
            let file = in_trash(library, &row);
            if file.exists() && std::fs::remove_file(&file).is_err() {
                continue;
            }
            let _ = file.parent().map(std::fs::remove_dir);
        }
        server.store.untrash(row.id);
        emptied += 1;
    }
    emptied
}

fn its_trash(server: &Server, me: &Account) -> Vec<(Library, crate::store::Trashed)> {
    let libraries: Vec<Library> = server.home.libraries(&me.name).into_iter().filter(|library| may_manage(server, me, library)).collect();
    server
        .store
        .trashed()
        .into_iter()
        .filter_map(|row| Some((libraries.iter().find(|library| library.id == row.library)?.clone(), row)))
        .collect()
}

pub async fn list(State(server): State<Arc<Server>>, Signed(me): Signed) -> Json<Vec<Value>> {
    let rows = its_trash(&server, &me).into_iter().map(|(library, row)| {
        let name = FilePath::new(&row.path).file_name().map(|name| name.to_string_lossy().into_owned());
        let left = (KEEP_DAYS - (now() - row.at) / 86_400).max(0);
        json!({ "id": row.id, "library": library.id, "name": name, "path": row.path, "at": row.at, "left": left })
    });
    Json(rows.collect())
}

pub async fn embedded(server: &Server, path: PathBuf, edge: u32) -> Result<Response, Fail> {
    let _turn = server.decodes.acquire().await.map_err(Fail::internal)?;
    let bytes = tokio::task::spawn_blocking(move || -> Result<Vec<u8>, String> {
        let image = numa_io::raw::load_thumbnail(&path, edge)?;
        let mut bytes = Vec::new();
        image::DynamicImage::ImageRgb8(image).write_to(&mut std::io::Cursor::new(&mut bytes), image::ImageFormat::Jpeg).map_err(|err| err.to_string())?;
        Ok(bytes)
    })
    .await
    .map_err(Fail::internal)?
    .map_err(|err| Fail(StatusCode::UNPROCESSABLE_ENTITY, format!("Numa could not read this photograph: {err}")))?;
    Ok(([(header::CONTENT_TYPE, "image/jpeg"), (header::CACHE_CONTROL, "private, max-age=3600")], bytes).into_response())
}

pub async fn thumb(State(server): State<Arc<Server>>, Signed(me): Signed, Path(id): Path<i64>) -> Result<Response, Fail> {
    let (library, row) = its_trash(&server, &me).into_iter().find(|(_, row)| row.id == id).ok_or_else(|| Fail(StatusCode::NOT_FOUND, "That is not in the trash.".into()))?;
    embedded(&server, in_trash(&library, &row), 320).await
}

#[derive(Deserialize)]
pub struct Ids {
    pub ids: Vec<i64>,
}

pub async fn restore(State(server): State<Arc<Server>>, Signed(me): Signed, Json(asked): Json<Ids>) -> Result<Json<Value>, Fail> {
    let rows: Vec<(Library, crate::store::Trashed)> = its_trash(&server, &me).into_iter().filter(|(_, row)| asked.ids.contains(&row.id)).collect();
    let working = server.clone();
    let restored = tokio::task::spawn_blocking(move || {
        let server = working;
        let mut back = Vec::new();
        for (library, row) in rows {
            let to = free(library.path.join(&row.path));
            let from = in_trash(&library, &row);
            if let Err(err) = move_file(&from, &to) {
                log::warn!("{}: {err}", from.display());
                continue;
            }
            let _ = from.parent().map(std::fs::remove_dir);
            server.store.untrash(row.id);
            back.push((library, to, serde_json::from_str::<Kept>(&row.kept).unwrap_or_default()));
        }
        land(&server, back)
    })
    .await
    .map_err(Fail::internal)?;
    Ok(Json(json!({ "restored": restored })))
}

pub fn land(server: &Arc<Server>, landed: Vec<(Library, PathBuf, Kept)>) -> usize {
    let mut libraries: Vec<Library> = landed.iter().map(|(library, ..)| library.clone()).collect();
    libraries.sort_by_key(|library| library.id);
    libraries.dedup_by_key(|library| library.id);
    for library in &libraries {
        crate::jobs::rescan(server, library.clone());
    }
    let mut given = 0;
    for (library, path, kept) in landed {
        let Some(id) = server.home.at(&path) else { continue };
        if let Some(author) = &kept.author {
            let _ = server.store.set_author(library.id, &within(&library, &path), Some(author));
        }
        server.home.blocking(move |catalog| give(catalog, id, &path, &kept));
        server.store.touch(id);
        given += 1;
    }
    shelve(server, libraries.into_iter().map(|library| library.id).collect());
    server.jobs.nudge();
    given
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_taken_name_gets_a_number_and_a_move_lands() {
        let root = std::env::temp_dir().join(format!("numa-home-manage-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("a.jpg"), b"one").unwrap();
        std::fs::write(root.join("b.jpg"), b"two").unwrap();
        let to = free(root.join("b.jpg"));
        assert_eq!(to, root.join("b 2.jpg"));
        move_file(&root.join("a.jpg"), &root.join("deeper/a.jpg")).unwrap();
        assert_eq!(std::fs::read(root.join("deeper/a.jpg")).unwrap(), b"one");
        assert!(!root.join("a.jpg").exists());
        assert!(plain("Japan") && !plain(".numa") && !plain("a/b") && !plain(""));
        let _ = std::fs::remove_dir_all(&root);
    }
}
