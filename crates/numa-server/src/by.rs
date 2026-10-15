use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::{Arc, Mutex};

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use numa_io::catalog::Library;
use numa_io::words::folded;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::auth::Signed;
use crate::{Fail, Server};

pub const UNKNOWN: &str = "Unknown";

const SAMPLE: usize = 3;

#[derive(Default)]
pub struct Artists(Mutex<HashMap<String, String>>);

impl Artists {
    fn all(&self) -> HashMap<String, String> {
        self.0.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone()
    }
}

fn artist_of(path: &Path) -> Option<String> {
    numa_io::exif::all(path).into_iter().find(|(name, _)| name == "Artist").map(|(_, value)| value).filter(|value| !value.is_empty())
}

pub fn learn(server: &Server) {
    let known: HashSet<String> = server.artists.all().into_keys().collect();
    for library in server.home.all_libraries() {
        let id = library.id;
        let Some(Ok(bodies)) = server.home.blocking(move |catalog| catalog.bodies(id)) else { continue };
        let paths: HashMap<i64, std::path::PathBuf> = server.home.files(id).into_iter().map(|photo| (photo.id, photo.path)).collect();
        let mut of: HashMap<String, Vec<i64>> = HashMap::new();
        for (photo, camera) in bodies.into_iter().filter(|(_, camera)| !camera.is_empty() && !known.contains(camera)) {
            of.entry(camera).or_default().push(photo);
        }
        for (camera, mut photos) in of {
            photos.sort_unstable();
            let read: Vec<String> = photos.iter().rev().take(SAMPLE).filter_map(|photo| artist_of(paths.get(photo)?)).collect();
            let most = read.iter().max_by_key(|artist| read.iter().filter(|other| other == artist).count()).cloned().unwrap_or_default();
            server.artists.0.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).entry(camera).or_insert(most);
        }
    }
}

fn named(artist: &str, accounts: &[String]) -> Option<String> {
    let artist = folded(artist.trim());
    accounts.iter().find(|account| {
        let account = folded(account);
        artist == account || artist.starts_with(&format!("{account} "))
    }).cloned()
}

pub async fn owners(server: &Arc<Server>, libraries: &[Library]) -> Result<HashMap<i64, String>, Fail> {
    let accounts: Vec<String> = server.store.accounts().map_err(Fail::internal)?.into_iter().map(|account| account.name).collect();
    let spelled = |name: &str| accounts.iter().find(|account| account.eq_ignore_ascii_case(name)).cloned();
    let links = server.store.cameras();
    let artists = server.artists.all();
    let sent: HashMap<(i64, String), String> = server.store.sendings().into_iter().map(|(name, library, path)| ((library, path), name)).collect();
    let ids: Vec<i64> = libraries.iter().map(|library| library.id).collect();
    let bodies = server.home.ask(move |catalog| ids.into_iter().filter_map(|id| catalog.bodies(id).ok()).collect::<Vec<_>>()).await?;
    let mut owners = HashMap::new();
    for (library, bodies) in libraries.iter().zip(bodies) {

        let own = (library.path.parent() == Some(server.home.sent.as_path()))
            .then(|| library.path.file_name().and_then(|name| spelled(&name.to_string_lossy())))
            .flatten();
        for photo in server.home.files(library.id) {
            let within = photo.path.strip_prefix(&library.path).unwrap_or(&photo.path).to_string_lossy().into_owned();
            let camera = bodies.get(&photo.id).filter(|camera| !camera.is_empty());
            let owner = own
                .clone()
                .or_else(|| sent.get(&(library.id, within)).and_then(|name| spelled(name)))
                .or_else(|| camera.and_then(|camera| links.get(camera)).and_then(|name| spelled(name)))
                .or_else(|| camera.and_then(|camera| artists.get(camera)).and_then(|artist| named(artist, &accounts)));
            if let Some(owner) = owner {
                owners.insert(photo.id, owner);
            }
        }
    }
    Ok(owners)
}

pub async fn counts(State(server): State<Arc<Server>>, Signed(me): Signed) -> Result<Json<Vec<Value>>, Fail> {
    let libraries = server.home.libraries(&me.name);
    let owners = owners(&server, &libraries).await?;
    let ids: Vec<i64> = libraries.iter().map(|library| library.id).collect();
    let mut counts: HashMap<Option<&str>, usize> = HashMap::new();
    let photos = server.home.photos(&ids);
    for photo in &photos {
        *counts.entry(owners.get(&photo.id).map(String::as_str)).or_default() += 1;
    }
    let mut rows: Vec<(Option<&str>, usize)> = counts.into_iter().collect();

    rows.sort_by_key(|(account, count)| (account.is_none(), std::cmp::Reverse(*count)));
    Ok(Json(rows.into_iter().map(|(account, count)| json!({ "account": account, "count": count })).collect()))
}

fn admin(me: &crate::store::Account) -> Result<(), Fail> {
    match me.admin {
        true => Ok(()),
        false => Err(Fail(StatusCode::FORBIDDEN, "Only an admin links cameras to people.".into())),
    }
}

pub async fn cameras(State(server): State<Arc<Server>>, Signed(me): Signed) -> Result<Json<Vec<Value>>, Fail> {
    admin(&me)?;
    let ids: Vec<i64> = server.home.all_libraries().iter().map(|library| library.id).collect();
    let found = server.home.ask(move |catalog| ids.into_iter().flat_map(|id| catalog.cameras(id).unwrap_or_default()).collect::<Vec<_>>()).await?;
    let mut counts: Vec<(String, i64)> = Vec::new();
    for camera in found {
        match counts.iter_mut().find(|(key, _)| *key == camera.key) {
            Some((_, count)) => *count += camera.photos,
            None => counts.push((camera.key, camera.photos)),
        }
    }
    counts.sort_by_key(|(_, count)| std::cmp::Reverse(*count));
    let accounts: Vec<String> = server.store.accounts().map_err(Fail::internal)?.into_iter().map(|account| account.name).collect();
    let (links, artists) = (server.store.cameras(), server.artists.all());
    let rows = counts.into_iter().map(|(key, count)| {
        let mut parts = key.splitn(3, '|');
        let (make, model, serial) = (parts.next().unwrap_or(""), parts.next().unwrap_or(""), parts.next().unwrap_or(""));
        let artist = artists.get(&key).filter(|artist| !artist.is_empty());
        json!({
            "camera": key, "make": make, "model": model, "serial": (!serial.is_empty()).then_some(serial), "count": count,
            "account": links.get(&key), "artist": artist, "hint": artist.and_then(|artist| named(artist, &accounts)),
        })
    });
    Ok(Json(rows.collect()))
}

#[derive(Deserialize)]
pub struct Link {
    camera: String,
    account: Option<String>,
}

pub async fn link(State(server): State<Arc<Server>>, Signed(me): Signed, Json(link): Json<Link>) -> Result<StatusCode, Fail> {
    admin(&me)?;
    let account = match link.account.as_deref().map(str::trim).filter(|name| !name.is_empty()) {
        Some(name) => Some(server.store.account(name).ok_or_else(|| Fail::bad("There is no such account."))?.0),
        None => None,
    };
    server.store.link_camera(&link.camera, account.as_deref()).map_err(Fail::internal)?;
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    #[test]
    fn an_artist_names_an_account_whole_or_by_its_first_words() {
        let accounts = vec!["Frédérique".to_string(), "Tijmen".to_string()];
        assert_eq!(super::named("FREDERIQUE", &accounts).as_deref(), Some("Frédérique"));
        assert_eq!(super::named("Tijmen de Vries", &accounts).as_deref(), Some("Tijmen"));
        assert_eq!(super::named("Tijmenson", &accounts), None);
        assert_eq!(super::named("", &accounts), None);
    }
}
