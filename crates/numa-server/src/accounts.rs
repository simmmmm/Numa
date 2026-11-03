use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

use axum::extract::{ConnectInfo, Path, State};
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::auth::{self, Signed};
use crate::grants::{Grants, Level};
use crate::store::{now, Account};
use crate::{Fail, Server};

#[derive(Deserialize)]
pub struct NewAccount {
    name: String,
    password: String,
    #[serde(default)]
    admin: bool,

    code: Option<String>,

    access: Option<Level>,
}

#[derive(Deserialize)]
pub struct Login {
    name: String,
    password: String,
    device: Option<String>,
}

pub async fn status(State(server): State<Arc<Server>>, ConnectInfo(peer): ConnectInfo<SocketAddr>, headers: HeaderMap) -> Json<serde_json::Value> {
    let setup = !server.store.has_accounts();
    let code = setup && !auth::in_house(peer, &headers);
    Json(serde_json::json!({ "version": env!("CARGO_PKG_VERSION"), "setup": setup, "code": code }))
}

pub async fn setup(State(server): State<Arc<Server>>, ConnectInfo(peer): ConnectInfo<SocketAddr>, headers: HeaderMap, Json(new): Json<NewAccount>) -> Result<StatusCode, Fail> {
    if !auth::in_house(peer, &headers) {
        let keys = auth::limit_keys("setup", auth::client(peer, &headers).0, true);
        let left = server.limit.left(&keys, now());
        if left > 0 {
            return Err(Fail(StatusCode::TOO_MANY_REQUESTS, format!("Too many wrong codes. {}", auth::try_again(left))));
        }
        if !auth::same_code(new.code.as_deref().unwrap_or(""), &server.setup_code) {
            server.limit.fail(&keys, now());
            return Err(Fail(StatusCode::FORBIDDEN, "Set up Numa Home from your home network, or give the setup code its log shows.".into()));
        }
    }
    let made = make(&server, new.name.trim(), &new.password, true, true).await?;
    crate::start::begin(&server, new.name.trim()).map_err(Fail::internal)?;
    Ok(made)
}

pub async fn add_user(State(server): State<Arc<Server>>, Signed(me): Signed, Json(new): Json<NewAccount>) -> Result<StatusCode, Fail> {
    if !me.admin {
        return Err(Fail(StatusCode::FORBIDDEN, "Only an admin makes accounts.".into()));
    }
    let made = make(&server, new.name.trim(), &new.password, new.admin, false).await?;
    if let Some(access) = new.access.filter(|_| !new.admin) {
        server.store.set_grants(new.name.trim(), &Grants { access, ..Grants::default() }).map_err(Fail::internal)?;
    }
    Ok(made)
}

pub async fn users(State(server): State<Arc<Server>>, Signed(me): Signed) -> Result<Json<Vec<Account>>, Fail> {

    let mut accounts = server.store.accounts().map_err(Fail::internal)?;
    if !me.sight.household {
        accounts.retain(|account| account.name.eq_ignore_ascii_case(&me.name));
    }
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

#[derive(Deserialize)]
pub struct GivenGrants {
    access: Level,
    #[serde(default)]
    libraries: HashMap<i64, Level>,
    #[serde(default)]
    albums: HashMap<String, Level>,
}

pub async fn grants(State(server): State<Arc<Server>>, Signed(me): Signed, Path(name): Path<String>) -> Result<Json<Value>, Fail> {
    if !me.admin {
        return Err(Fail(StatusCode::FORBIDDEN, "Only an admin says what an account sees.".into()));
    }
    if server.store.account(&name).is_none() {
        return Err(Fail(StatusCode::NOT_FOUND, "There is no account with that name.".into()));
    }
    let given = server.store.grants(&name).map_err(Fail::internal)?;
    let mut libraries: Vec<_> = server.home.all_libraries().into_iter().filter(|library| library.path.starts_with(&server.home.photos)).collect();
    libraries.sort_by_key(|library| library.label().to_lowercase());
    let libraries: Vec<Value> =
        libraries.iter().map(|library| json!({ "id": library.id, "name": library.label(), "level": given.libraries.get(&library.id) })).collect();
    let albums = server.home.ask(|catalog| catalog.albums().unwrap_or_default()).await?;
    let albums: Vec<Value> = albums.into_iter().map(|(key, name)| json!({ "level": given.albums.get(&key), "key": key, "name": name })).collect();
    Ok(Json(json!({ "access": given.access, "libraries": libraries, "albums": albums })))
}

pub async fn set_grants(State(server): State<Arc<Server>>, Signed(me): Signed, Path(name): Path<String>, Json(given): Json<GivenGrants>) -> Result<StatusCode, Fail> {
    if !me.admin {
        return Err(Fail(StatusCode::FORBIDDEN, "Only an admin says what an account sees.".into()));
    }
    let grants = Grants { access: given.access, libraries: given.libraries, albums: given.albums };
    match server.store.set_grants(&name, &grants).map_err(Fail::internal)? {
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

pub fn reset_password(store: &crate::store::Store, name: &str) -> Result<String, String> {

    let (token, _) = auth::token();
    let password = token[..20].to_string();
    let hash = auth::hash(&password)?;
    if !store.set_password(name, &hash, true)? {
        return Err(format!("There is no account called {name}."));
    }
    store.end_sessions(name, None);
    Ok(password)
}

async fn hashed(password: &str) -> Result<String, Fail> {
    if password.chars().count() < 8 {
        return Err(Fail::bad("A password is at least 8 characters."));
    }
    let password = password.to_string();
    auth::argon(move || auth::hash(&password)).await?.map_err(Fail::internal)
}

async fn checked(server: &Server, name: &str, password: String, address: std::net::IpAddr, outside: bool) -> Result<Option<(String, String, bool)>, Fail> {
    let keys = auth::limit_keys(name, address, outside);
    let left = server.limit.reserve(&keys, now());
    if left > 0 {
        return Err(Fail(StatusCode::TOO_MANY_REQUESTS, format!("Too many wrong passwords. {}", auth::try_again(left))));
    }
    let known = server.store.account(name.trim());
    let hash = known.as_ref().map(|(_, hash, _)| hash.clone());
    let right = auth::argon(move || auth::verify(&password, hash.as_deref())).await?;
    if right {
        server.limit.clear(&keys);
    }
    Ok(known.filter(|_| right))
}

fn tried_name(name: &str) -> String {
    match name.trim() {
        long if long.chars().count() > 40 => "(name too long)".into(),
        name => name.into(),
    }
}

fn cookie(value: &str, max_age: i64, https: bool) -> String {
    format!("{}={value}; HttpOnly; SameSite=Strict; Path=/; Max-Age={max_age}{}", auth::COOKIE, if https { "; Secure" } else { "" })
}

pub async fn login(State(server): State<Arc<Server>>, ConnectInfo(peer): ConnectInfo<SocketAddr>, headers: HeaderMap, Json(mut login): Json<Login>) -> Result<Response, Fail> {
    login.name = tried_name(&login.name);
    let (address, https) = auth::client(peer, &headers);
    let outside = !auth::in_house(peer, &headers);
    let tried = checked(&server, &login.name, login.password, address, outside).await;
    let result = match &tried {
        Ok(Some(_)) => "ok",
        Ok(None) => "wrong",
        Err(Fail(StatusCode::TOO_MANY_REQUESTS, _)) => "locked",
        Err(_) => "",
    };
    if !result.is_empty() {
        server.store.log_sign_in(&login.name, &address.to_string(), outside, result);
        if outside && result != "ok" {
            log::warn!("sign-in from outside: {result} for {:?} from {address}", login.name.trim());
        }
    }
    let Some((name, _, admin)) = tried? else {
        let text = "That name and password do not match an account.";
        return Err(Fail(StatusCode::UNAUTHORIZED, match server.limit.left(&auth::limit_keys(&login.name, address, outside), now()) {
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
    if !me.must_change && checked(&server, &me.name, asked.old.unwrap_or_default(), auth::client(peer, &headers).0, !auth::in_house(peer, &headers)).await?.is_none() {
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

    answer["household"] = me.sight.household.into();
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_session_cookie_is_closed_to_scripts_and_other_sites() {
        let https = cookie("abc", 100, true);
        for part in ["numa_session=abc", "HttpOnly", "SameSite=Strict", "Path=/", "Secure"] {
            assert!(https.split("; ").any(|one| one == part), "{part} in {https}");
        }
        assert!(!cookie("abc", 100, false).contains("Secure"), "plain http cannot carry it");
        assert!(cookie("", 0, true).contains("Max-Age=0"));
    }

    #[test]
    fn sign_ins_are_kept() {
        let store = crate::store::Store::open(std::path::Path::new(":memory:")).unwrap();
        store.log_sign_in(" Sam ", "203.0.113.7", true, "wrong");
        store.log_sign_in("Sam", "192.168.1.5", false, "ok");
        let rows = store.sign_ins(10);
        assert_eq!((rows[0].1.as_str(), rows[0].3, rows[0].4.as_str()), ("Sam", false, "ok"));
        assert_eq!((rows[1].1.as_str(), rows[1].2.as_str(), rows[1].3), ("Sam", "203.0.113.7", true));
        for _ in 0..600 {
            store.log_sign_in(&"x".repeat(100), "203.0.113.7", true, "wrong");
        }
        assert_eq!(store.sign_ins(1000).len(), 500);
        assert_eq!(store.sign_ins(1)[0].1.len(), 40, "a long name is cut");
    }

    #[test]
    fn a_name_longer_than_a_name_is_short_in_the_lock_and_the_log() {
        assert_eq!(tried_name("  Sam "), "Sam");
        assert_eq!(tried_name(&"x".repeat(40)), "x".repeat(40));
        let long = tried_name(&"x".repeat(2_000_000));
        assert!(long.len() < 40 && !auth::valid_name(&long), "{long}");
        assert_eq!(tried_name(&format!("{}y", " ".repeat(5_000))), "y", "spaces are not length");
        let key = auth::limit_keys(&long, "203.0.113.7".parse().unwrap(), true);
        assert!(key.iter().all(|key| key.len() < 80));
    }

    #[test]
    fn reset_password_gives_a_password_that_works_and_must_be_changed() {
        let folder = std::env::temp_dir().join(format!("numa-reset-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let store = crate::store::Store::open(&folder.join("server.db")).unwrap();
        store.add_account("anna", &auth::hash("the old one").unwrap(), true, true).unwrap();
        store.add_session("digest", "anna", None, "").unwrap();

        let password = reset_password(&store, "ANNA").unwrap();
        let (_, hash, _) = store.account("anna").unwrap();
        assert!(auth::verify(&password, Some(&hash)));
        assert!(!auth::verify("the old one", Some(&hash)));
        assert!(store.session("digest").is_none(), "signed out everywhere");
        assert!(reset_password(&store, "nobody").is_err());
        std::fs::remove_dir_all(&folder).unwrap();
    }
}
