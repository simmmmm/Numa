use std::sync::Arc;

use axum::extract::State;
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Deserialize;

use crate::auth::{self, Signed};
use crate::store::Account;
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
    make(&server, new.name.trim(), &new.password, true, true).await
}

pub async fn add_user(State(server): State<Arc<Server>>, Signed(me): Signed, Json(new): Json<NewAccount>) -> Result<StatusCode, Fail> {
    if !me.admin {
        return Err(Fail(StatusCode::FORBIDDEN, "Only an admin makes accounts.".into()));
    }
    make(&server, new.name.trim(), &new.password, new.admin, false).await
}

pub async fn users(State(server): State<Arc<Server>>, Signed(me): Signed) -> Result<Json<Vec<Account>>, Fail> {

    let accounts = server.store.accounts().map_err(Fail::internal)?;
    Ok(Json(accounts.into_iter().map(|account| Account { admin: account.admin && me.admin, ..account }).collect()))
}

async fn make(server: &Arc<Server>, name: &str, password: &str, admin: bool, first: bool) -> Result<StatusCode, Fail> {
    if !auth::valid_name(name) {
        return Err(Fail::bad("A name is letters, digits, spaces, - and _, at most 40 of them."));
    }
    if password.chars().count() < 8 {
        return Err(Fail::bad("A password is at least 8 characters."));
    }
    let password = password.to_string();
    let hash = tokio::task::spawn_blocking(move || auth::hash(&password)).await.map_err(Fail::internal)?.map_err(Fail::internal)?;
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

pub async fn login(State(server): State<Arc<Server>>, Json(login): Json<Login>) -> Result<Response, Fail> {
    let known = server.store.account(login.name.trim());
    let hash = known.as_ref().map(|(_, hash, _)| hash.clone());
    let password = login.password;
    let right = tokio::task::spawn_blocking(move || auth::verify(&password, hash.as_deref())).await.map_err(Fail::internal)?;
    let Some((name, _, admin)) = known.filter(|_| right) else {
        return Err(Fail(StatusCode::UNAUTHORIZED, "That name and password do not match an account.".into()));
    };
    let (token, digest) = auth::token();
    server.store.add_session(&digest, &name, login.device.as_deref()).map_err(Fail::internal)?;
    let cookie = format!("{}={token}; HttpOnly; SameSite=Strict; Path=/; Max-Age=31536000", auth::COOKIE);
    let body = Json(serde_json::json!({ "token": token, "name": name, "admin": admin }));
    Ok(([(header::SET_COOKIE, cookie)], body).into_response())
}

pub async fn logout(State(server): State<Arc<Server>>, headers: HeaderMap) -> Result<Response, Fail> {
    let token = auth::presented(&headers).ok_or_else(|| Fail(StatusCode::UNAUTHORIZED, "Sign in first.".into()))?;
    server.store.end_session(&auth::digest(&token));
    let cookie = format!("{}=; HttpOnly; SameSite=Strict; Path=/; Max-Age=0", auth::COOKIE);
    Ok(([(header::SET_COOKIE, cookie)], StatusCode::NO_CONTENT).into_response())
}

pub async fn me(Signed(me): Signed) -> Json<Account> {
    Json(me)
}
