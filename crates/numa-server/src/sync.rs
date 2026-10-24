use std::collections::HashMap;
use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::Json;
use serde_json::{json, Value};

use crate::auth::{self, Signed};
use crate::{Fail, Server};

const STALE: i64 = 30 * 86_400;

pub fn deletes(server: &Server, library: i64, since: i64) -> Vec<Value> {
    let rows = server.store.trashed().into_iter().filter(|row| row.library == library && row.at > since);
    rows.map(|row| {
        let id = serde_json::from_str::<Value>(&row.kept).ok().and_then(|kept| kept["photo"].as_i64()).filter(|id| *id != 0);
        json!({ "id": id, "path": row.path, "deleted": row.at, "updated": row.at })
    })
    .collect()
}

pub async fn with_albums(server: &Server, rows: &mut [Value]) -> Result<(), Fail> {
    let ids: Vec<i64> = rows.iter().filter_map(|row| row["id"].as_i64()).collect();
    let albums: HashMap<i64, Vec<String>> = server.home.ask(move |catalog| catalog.albums_of(&ids)).await?.map_err(Fail::internal)?;
    for row in rows {
        let names = row["id"].as_i64().and_then(|id| albums.get(&id)).cloned().unwrap_or_default();
        row["albums"] = json!(names);
    }
    Ok(())
}

pub fn note(server: &Server, headers: &HeaderMap, account: &str, library: i64) {
    let device = auth::presented(headers).and_then(|token| server.store.session_device(&auth::digest(&token))).unwrap_or_default();
    server.store.note_synced(library, account, &device);
}

pub async fn stop(State(server): State<Arc<Server>>, Signed(me): Signed, headers: HeaderMap, Path(id): Path<i64>) -> Result<StatusCode, Fail> {
    let device = auth::presented(&headers).and_then(|token| server.store.session_device(&auth::digest(&token))).unwrap_or_default();
    server.store.unsynced(id, &me.name, &device);
    Ok(StatusCode::NO_CONTENT)
}

pub async fn devices(State(server): State<Arc<Server>>, Signed(me): Signed) -> Json<Value> {
    let seen: Vec<i64> = server.home.libraries(&me.name).iter().map(|library| library.id).collect();
    let mut by_library: HashMap<i64, Vec<(String, String)>> = HashMap::new();
    for (library, account, device) in server.store.synced(crate::store::now() - STALE) {
        if seen.contains(&library) {
            by_library.entry(library).or_default().push((account, device));
        }
    }
    let said: serde_json::Map<String, Value> = by_library.into_iter().map(|(library, devices)| (library.to_string(), json!(labels(devices)))).collect();
    Json(Value::Object(said))
}

fn labels(mut devices: Vec<(String, String)>) -> Vec<String> {
    devices.sort_by_key(|(account, device)| (account.to_lowercase(), device.to_lowercase()));
    let mut last: Option<String> = None;
    devices
        .into_iter()
        .map(|(account, device)| {
            let label = numa_io::home::sync::device_label(&account, &device);
            let again = last.as_deref().is_some_and(|last| last.eq_ignore_ascii_case(&account));
            last = Some(account.clone());
            match again {
                true => label.strip_prefix(&format!("{account}'s ")).map(str::to_string).unwrap_or(label),
                false => label,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn one_accounts_devices_say_its_name_once() {
        let devices = vec![("Tijmen".into(), "iPhone".into()), ("Anna".into(), "Anna's iPad".into()), ("Tijmen".into(), "MacBook".into())];
        assert_eq!(labels(devices), ["Anna's iPad", "Tijmen's iPhone", "MacBook"]);
    }
}
