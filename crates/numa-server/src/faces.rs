use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use axum::extract::{Path, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use numa_cull as cull;
use numa_io::catalog::{library_of, Catalog, StoredFace};
use serde::Deserialize;
use serde_json::{json, Value};

use crate::auth::Signed;
use crate::store::Account;
use crate::{Fail, Server};

fn seen(server: &Server, me: &Account) -> Vec<i64> {
    server.home.editable(me).iter().map(|library| library.id).collect()
}

fn recognised(catalog: &Catalog, library: i64) -> (Vec<StoredFace>, Vec<Option<String>>) {
    let faces = catalog.faces(library).unwrap_or_default();
    let named = catalog.named_faces().unwrap_or_default();
    let known = catalog.known_from(&named).unwrap_or_default();
    let names = faces.iter().map(|face| cull::people::recognise(&face.embedding, &known).map(|(name, _)| name.to_string())).collect();
    (faces, names)
}

static COUNTED: Mutex<Vec<(String, Instant, usize)>> = Mutex::new(Vec::new());

const FRESH: Duration = Duration::from_secs(10 * 60);

pub async fn waiting(server: &Arc<Server>, me: &Account) -> Result<usize, Fail> {
    let account = me.name.as_str();
    let known = {
        let counted = COUNTED.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        counted.iter().find(|(name, at, _)| name == account && at.elapsed() < FRESH).map(|(.., count)| *count)
    };
    match known {
        Some(count) => Ok(count),
        None => Ok(groups(server, me).await?.len()),
    }
}

pub async fn unnamed(State(server): State<Arc<Server>>, Signed(me): Signed) -> Result<Json<Vec<Value>>, Fail> {
    Ok(Json(groups(&server, &me).await?))
}

async fn groups(server: &Arc<Server>, me: &Account) -> Result<Vec<Value>, Fail> {
    let (account, libraries) = (me.name.as_str(), seen(server, me));
    let groups = server
        .home
        .ask(move |catalog| {
            let mut rows = Vec::new();
            for library in libraries {
                let (faces, names) = recognised(catalog, library);
                let (groups, _) = numa_io::analysis::unnamed_groups(&faces, &names);
                for (members, photos) in groups {
                    let ids: Vec<i64> = members.iter().map(|&at| faces[at].id).collect();
                    let shown: Vec<Value> = members.iter().take(3).map(|&at| json!({ "face": faces[at].id, "photo": faces[at].photo_id })).collect();
                    rows.push((photos.len(), json!({ "faces": ids, "photos": photos.len(), "portraits": shown })));
                }
            }
            rows.sort_by(|a, b| b.0.cmp(&a.0));
            rows.into_iter().map(|(_, row)| row).collect::<Vec<Value>>()
        })
        .await?;
    let mut counted = COUNTED.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    counted.retain(|(name, ..)| name != account);
    counted.push((account.to_string(), Instant::now(), groups.len()));
    Ok(groups)
}

pub async fn portrait(State(server): State<Arc<Server>>, Signed(me): Signed, Path(id): Path<i64>) -> Result<Response, Fail> {
    if !seen(&server, &me).contains(&library_of(id)) {
        return Err(Fail(StatusCode::NOT_FOUND, "There is no such face.".into()));
    }
    let face = server.home.ask(move |catalog| catalog.faces(library_of(id)).unwrap_or_default().into_iter().find(|face| face.id == id)).await?;
    match face {
        Some(StoredFace { portrait: Some(bytes), .. }) => Ok(([(header::CONTENT_TYPE, "image/jpeg"), (header::CACHE_CONTROL, "private, max-age=86400")], bytes).into_response()),
        _ => Err(Fail(StatusCode::NOT_FOUND, "This face has no picture of its own.".into())),
    }
}

#[derive(Deserialize)]
pub struct Named {
    faces: Vec<i64>,
    #[serde(default)]
    name: String,
}

async fn chosen(server: &Arc<Server>, me: &Account, ids: Vec<i64>) -> Result<Vec<(i64, [f32; cull::people::LENGTH])>, Fail> {
    let libraries: HashSet<i64> = ids.iter().map(|id| library_of(*id)).collect();
    let seen = seen(server, me);
    if ids.is_empty() || !libraries.iter().all(|library| seen.contains(library)) {
        return Err(Fail(StatusCode::NOT_FOUND, "There are no such faces.".into()));
    }
    let wanted: HashSet<i64> = ids.into_iter().collect();
    server
        .home
        .ask(move |catalog| {
            libraries
                .into_iter()
                .flat_map(|library| catalog.faces(library).unwrap_or_default())
                .filter(|face| wanted.contains(&face.id))
                .map(|face| (face.photo_id, face.embedding))
                .collect()
        })
        .await
}

pub async fn name(State(server): State<Arc<Server>>, Signed(me): Signed, Json(named): Json<Named>) -> Result<StatusCode, Fail> {
    let name = named.name.trim().to_string();
    if name.is_empty() || name.chars().count() > 80 {
        return Err(Fail::bad("A name, please, of at most 80 characters."));
    }
    let faces = chosen(&server, &me, named.faces).await?;
    server
        .home
        .ask(move |catalog| faces.iter().try_for_each(|(photo, embedding)| catalog.name_face(*photo, embedding, &name)))
        .await?
        .map_err(Fail::internal)?;
    Ok(StatusCode::NO_CONTENT)
}

pub async fn ignore(State(server): State<Arc<Server>>, Signed(me): Signed, Json(named): Json<Named>) -> Result<StatusCode, Fail> {
    let faces = chosen(&server, &me, named.faces).await?;
    server.home.ask(move |catalog| catalog.ignore_faces(&faces)).await?.map_err(Fail::internal)?;
    Ok(StatusCode::NO_CONTENT)
}
