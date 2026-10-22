use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::{ConnectInfo, Path, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::auth::{self, Signed};
use crate::store::{now, Account};
use crate::{Fail, Server};

#[derive(Deserialize)]
pub struct NewAccount {
    name: String,
    password: String,
    #[serde(default)]
    admin: bool,
}

#[derive(Deserialize)]
pub struct Login {
    name: String,
    password: String,
    device: Option<String>,
}

pub async fn status(State(server): State<Arc<Server>>) -> Json<serde_json::Value> {
    Json(serde_json::json!({ "version": env!("CARGO_PKG_VERSION"), "setup": !server.store.has_accounts() }))
}

pub async fn setup(State(server): State<Arc<Server>>, Json(new): Json<NewAccount>) -> Result<StatusCode, Fail> {
    let made = make(&server, new.name.trim(), &new.password, true, true).await?;
    crate::start::begin(&server, new.name.trim()).map_err(Fail::internal)?;
    Ok(made)
}

pub async fn add_user(State(server): State<Arc<Server>>, Signed(me): Signed, Json(new): Json<NewAccount>) -> Result<StatusCode, Fail> {
    if !me.admin {
        return Err(Fail(StatusCode::FORBIDDEN, "Only an admin makes accounts.".into()));
    }
    make(&server, new.name.trim(), &new.password, new.admin, false).await
}

pub async fn users(State(server): State<Arc<Server>>, Signed(me): Signed) -> Result<Json<Vec<Account>>, Fail> {

    let accounts = server.store.accounts().map_err(Fail::internal)?;
    Ok(Json(
        accounts
            .into_iter()
            .map(|account| Account { admin: account.admin && me.admin, manage: account.manage && me.admin, must_change: account.must_change && me.admin, ..account })
            .collect(),
    ))
}

#[derive(Deserialize)]
pub struct Rights {
    manage: bool,
}

pub async fn set_user(State(server): State<Arc<Server>>, Signed(me): Signed, Path(name): Path<String>, Json(rights): Json<Rights>) -> Result<StatusCode, Fail> {
    if !me.admin {
        return Err(Fail(StatusCode::FORBIDDEN, "Only an admin gives an account its rights.".into()));
    }
    match server.store.set_manage(&name, rights.manage).map_err(Fail::internal)? {
        true => Ok(StatusCode::NO_CONTENT),
        false => Err(Fail(StatusCode::NOT_FOUND, "There is no account with that name.".into())),
    }
}

async fn make(server: &Arc<Server>, name: &str, password: &str, admin: bool, first: bool) -> Result<StatusCode, Fail> {
    if !auth::valid_name(name) {
        return Err(Fail::bad("A name is letters, digits, spaces, - and _, at most 40 of them."));
    }
    let hash = hashed(password).await?;
    if !server.store.add_account(name, &hash, admin, first).map_err(Fail::internal)? {
        return Err(match first {
            true => Fail(StatusCode::CONFLICT, "Numa Home is already set up.".into()),
            false => Fail(StatusCode::CONFLICT, "There is already an account with that name.".into()),
        });
    }
    let folder = server.home.sent_folder(name);
    std::fs::create_dir_all(&folder).map_err(Fail::internal)?;
    let home = server.clone();
    server
        .home
        .ask(move |catalog| {
            let added = catalog.add_library(&folder);
            home.shelve(added.as_ref().ok().map(|library| library.id), catalog);
            added.map(drop)
        })
        .await?
        .map_err(Fail::internal)?;
    Ok(StatusCode::CREATED)
}

async fn hashed(password: &str) -> Result<String, Fail> {
    if password.chars().count() < 8 {
        return Err(Fail::bad("A password is at least 8 characters."));
    }
    let password = password.to_string();
    tokio::task::spawn_blocking(move || auth::hash(&password)).await.map_err(Fail::internal)?.map_err(Fail::internal)
}

async fn checked(server: &Server, name: &str, password: String, address: std::net::IpAddr) -> Result<Option<(String, String, bool)>, Fail> {
    let keys = auth::limit_keys(name, address);
    let left = server.limit.left(&keys, now());
    if left > 0 {
        return Err(Fail(StatusCode::TOO_MANY_REQUESTS, format!("Too many wrong passwords. {}", auth::try_again(left))));
    }
    let known = server.store.account(name.trim());
    let hash = known.as_ref().map(|(_, hash, _)| hash.clone());
    let right = tokio::task::spawn_blocking(move || auth::verify(&password, hash.as_deref())).await.map_err(Fail::internal)?;
    match right {
        true => server.limit.clear(&keys),
        false => server.limit.fail(&keys, now()),
    }
    Ok(known.filter(|_| right))
}

fn cookie(value: &str, max_age: i64, https: bool) -> String {
    format!("{}={value}; HttpOnly; SameSite=Strict; Path=/; Max-Age={max_age}{}", auth::COOKIE, if https { "; Secure" } else { "" })
}

pub async fn login(State(server): State<Arc<Server>>, ConnectInfo(peer): ConnectInfo<SocketAddr>, headers: HeaderMap, Json(login): Json<Login>) -> Result<Response, Fail> {
    let (address, https) = auth::client(peer, &headers);
    let Some((name, _, admin)) = checked(&server, &login.name, login.password, address).await? else {
        let text = "That name and password do not match an account.";
        return Err(Fail(StatusCode::UNAUTHORIZED, match server.limit.left(&auth::limit_keys(&login.name, address), now()) {
            0 => text.into(),
            left => format!("{text} {}", auth::try_again(left)),
        }));
    };
    let (token, digest) = auth::token();
    server.store.add_session(&digest, &name, login.device.as_deref(), &address.to_string()).map_err(Fail::internal)?;
    let must_change = server.store.session(&digest).is_some_and(|account| account.must_change);
    let body = Json(json!({ "token": token, "name": name, "admin": admin, "must_change": must_change }));
    Ok(([(header::SET_COOKIE, cookie(&token, 31_536_000, https))], body).into_response())
}

pub async fn logout(State(server): State<Arc<Server>>, ConnectInfo(peer): ConnectInfo<SocketAddr>, headers: HeaderMap) -> Result<Response, Fail> {
    let token = auth::presented(&headers).ok_or_else(|| Fail(StatusCode::UNAUTHORIZED, "Sign in first.".into()))?;
    server.store.end_session(&auth::digest(&token));
    Ok(([(header::SET_COOKIE, cookie("", 0, auth::client(peer, &headers).1))], StatusCode::NO_CONTENT).into_response())
}

#[derive(Deserialize)]
pub struct NewPassword {
    old: Option<String>,
    new: String,
}

pub async fn change_password(
    State(server): State<Arc<Server>>,
    ConnectInfo(peer): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Signed(me): Signed,
    Json(asked): Json<NewPassword>,
) -> Result<StatusCode, Fail> {
    if !me.must_change && checked(&server, &me.name, asked.old.unwrap_or_default(), auth::client(peer, &headers).0).await?.is_none() {
        return Err(Fail(StatusCode::FORBIDDEN, "That is not your current password.".into()));
    }
    let hash = hashed(&asked.new).await?;
    server.store.set_password(&me.name, &hash, false).map_err(Fail::internal)?;
    let this = auth::presented(&headers).map(|token| auth::digest(&token));
    server.store.end_sessions(&me.name, this.as_deref());
    Ok(StatusCode::NO_CONTENT)
}

#[derive(Deserialize)]
pub struct GivenPassword {
    password: String,
}

pub async fn give_password(State(server): State<Arc<Server>>, Signed(me): Signed, Path(name): Path<String>, Json(given): Json<GivenPassword>) -> Result<StatusCode, Fail> {
    if !me.admin {
        return Err(Fail(StatusCode::FORBIDDEN, "Only an admin gives an account a new password.".into()));
    }
    if name.eq_ignore_ascii_case(&me.name) {
        return Err(Fail::bad("Change your own password in the menu under your name."));
    }
    let hash = hashed(&given.password).await?;
    if !server.store.set_password(&name, &hash, true).map_err(Fail::internal)? {
        return Err(Fail(StatusCode::NOT_FOUND, "There is no account with that name.".into()));
    }
    server.store.end_sessions(&name, None);
    Ok(StatusCode::NO_CONTENT)
}

pub async fn sessions(State(server): State<Arc<Server>>, Signed(me): Signed, headers: HeaderMap) -> Json<Vec<Value>> {
    let this = auth::presented(&headers).map(|token| auth::digest(&token)).unwrap_or_default();
    let rows = server.store.sessions((!me.admin).then_some(me.name.as_str()));
    Json(
        rows.into_iter()
            .map(|one| {
                json!({
                    "id": one.id, "account": one.account, "device": one.device, "label": crate::safe::label(&one.account, &one.device),
                    "made": one.made, "seen": one.seen, "sent": one.sent, "address": one.address, "this": one.digest == this,
                })
            })
            .collect(),
    )
}

pub async fn end_session(State(server): State<Arc<Server>>, Signed(me): Signed, Path(id): Path<i64>) -> Result<StatusCode, Fail> {
    match server.store.end_session_id(id, (!me.admin).then_some(me.name.as_str())) {
        true => Ok(StatusCode::NO_CONTENT),
        false => Err(Fail(StatusCode::NOT_FOUND, "That device is not signed in.".into())),
    }
}

pub async fn end_others(State(server): State<Arc<Server>>, Signed(me): Signed, headers: HeaderMap) -> Json<Value> {
    let this = auth::presented(&headers).map(|token| auth::digest(&token));
    Json(json!({ "ended": server.store.end_sessions(&me.name, this.as_deref()) }))
}

pub async fn me(State(server): State<Arc<Server>>, Signed(me): Signed) -> Json<serde_json::Value> {
    let start = server.store.setting(&format!("start:{}", me.name)).unwrap_or_else(|| "today".into());
    let mut answer = serde_json::to_value(&me).unwrap_or_default();
    answer["start"] = start.into();
    answer["first_start"] = crate::start::step(&server, &me.name).into();
    Json(answer)
}

#[derive(Deserialize)]
pub struct Start {
    start: String,
}

pub async fn set_start(State(server): State<Arc<Server>>, Signed(me): Signed, Json(asked): Json<Start>) -> Result<StatusCode, Fail> {
    if !matches!(asked.start.as_str(), "today" | "photos") {
        return Err(Fail::bad("Numa Home starts on today or photos."));
    }
    server.store.set_setting(&format!("start:{}", me.name), &asked.start).map_err(Fail::internal)?;
    Ok(StatusCode::NO_CONTENT)
}
