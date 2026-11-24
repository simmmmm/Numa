use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex};

use argon2::password_hash::rand_core::{OsRng, RngCore};
use axum::extract::{ConnectInfo, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::Response;
use axum::routing::{get, post};
use axum::{Json, Router};
use base64::Engine;
use p256::ecdsa::signature::Verifier;
use p256::ecdsa::{Signature, VerifyingKey};
use p256::pkcs8::DecodePublicKey;
use rusqlite::{params, OptionalExtension};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::auth::{self, Signed};
use crate::store::{now, Store};
use crate::{Fail, Server};

const PRESENT: u8 = 0x01;
const VERIFIED: u8 = 0x04;
const ATTESTED: u8 = 0x40;

const CHALLENGE_TIME: i64 = 300;
const CODE_TIME: i64 = 120;
const MOST_PASSKEYS: usize = 20;
const MOST_CODES: usize = 1000;

pub fn tables(store: &Store) -> Result<(), String> {
    store.with(|conn| {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS passkeys (
                 id TEXT PRIMARY KEY, account TEXT NOT NULL COLLATE NOCASE, key BLOB NOT NULL, count INTEGER NOT NULL,
                 label TEXT NOT NULL, made INTEGER NOT NULL, used INTEGER);
             CREATE TABLE IF NOT EXISTS passkey_users (account TEXT PRIMARY KEY COLLATE NOCASE, handle TEXT NOT NULL);",
        )
    })
}

impl Store {

    pub fn passkeys(&self, account: &str) -> Vec<(String, String, i64, Option<i64>)> {
        self.with(|conn| {
            let mut statement = conn.prepare("SELECT id, label, made, used FROM passkeys WHERE account = ?1 ORDER BY made, id")?;
            let rows = statement.query_map([account], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)))?;
            rows.collect()
        })
        .unwrap_or_default()
    }

    fn passkey(&self, id: &str) -> Option<(String, Vec<u8>, u32)> {
        self.with(|conn| conn.query_row("SELECT account, key, count FROM passkeys WHERE id = ?1", [id], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?))).optional()).ok().flatten()
    }

    fn add_passkey(&self, id: &str, account: &str, key: &[u8], count: u32, label: &str) -> Result<bool, String> {
        self.with(|conn| conn.execute("INSERT OR IGNORE INTO passkeys (id, account, key, count, label, made) VALUES (?1, ?2, ?3, ?4, ?5, ?6)", params![id, account, key, count, label, now()])).map(|rows| rows == 1)
    }

    fn used_passkey(&self, id: &str, count: u32) {
        let _ = self.with(|conn| conn.execute("UPDATE passkeys SET count = ?2, used = ?3 WHERE id = ?1", params![id, count, now()]));
    }

    fn remove_passkey(&self, id: &str, account: &str) -> bool {
        self.with(|conn| conn.execute("DELETE FROM passkeys WHERE id = ?1 AND account = ?2", params![id, account])).is_ok_and(|rows| rows == 1)
    }

    fn passkey_handle(&self, account: &str) -> Result<String, String> {
        let mut bytes = [0u8; 16];
        OsRng.fill_bytes(&mut bytes);
        self.with(|conn| {
            conn.execute("INSERT OR IGNORE INTO passkey_users (account, handle) VALUES (?1, ?2)", params![account, b64(&bytes)])?;
            conn.query_row("SELECT handle FROM passkey_users WHERE account = ?1", [account], |row| row.get(0))
        })
    }
}

pub struct Keys {
    secret: [u8; 32],
    spent: Mutex<HashMap<[u8; 16], i64>>,
    codes: Mutex<HashMap<String, Handoff>>,
}

struct Handoff {
    challenge: String,
    account: String,
    until: i64,
}

impl Default for Keys {
    fn default() -> Self {
        let mut secret = [0u8; 32];
        OsRng.fill_bytes(&mut secret);
        Keys { secret, spent: Mutex::default(), codes: Mutex::default() }
    }
}

#[derive(Clone, Copy)]
enum For {
    SignIn = 1,
    Add = 2,
}

fn b64(bytes: &[u8]) -> String {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

fn unb64(text: &str) -> Option<Vec<u8>> {
    base64::engine::general_purpose::URL_SAFE_NO_PAD.decode(text.trim_end_matches('=')).ok()
}

fn hmac(key: &[u8; 32], message: &[u8]) -> [u8; 32] {
    use hmac::Mac;
    let mut mac = hmac::Hmac::<Sha256>::new_from_slice(key).expect("HMAC takes a key of any length");
    mac.update(message);
    mac.finalize().into_bytes().into()
}

fn seal(keys: &Keys, purpose: For, account: &str, nonce: &[u8], until: &[u8]) -> [u8; 32] {
    hmac(&keys.secret, &[&[purpose as u8], nonce, until, account.to_lowercase().as_bytes()].concat())
}

fn challenge(keys: &Keys, purpose: For, account: &str, now: i64) -> String {
    let mut nonce = [0u8; 16];
    OsRng.fill_bytes(&mut nonce);
    let until = (now + CHALLENGE_TIME).to_be_bytes();
    b64(&[&nonce[..], &until, &seal(keys, purpose, account, &nonce, &until)].concat())
}

fn spend(keys: &Keys, purpose: For, account: &str, text: &str, now: i64) -> bool {
    let Some(bytes) = unb64(text).filter(|bytes| bytes.len() == 56) else { return false };
    let (nonce, until, sealed) = (&bytes[..16], &bytes[16..24], &bytes[24..]);
    if !auth::same_bytes(sealed, &seal(keys, purpose, account, nonce, until)) {
        return false;
    }
    let until = i64::from_be_bytes(until.try_into().unwrap_or_default());
    let mut spent = keys.spent.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    spent.retain(|_, until| *until >= now);
    now <= until && spent.insert(nonce.try_into().unwrap_or_default(), until).is_none()
}

fn relying(store: &Store) -> Result<(String, String), Fail> {
    let address = store.setting("reach:address").unwrap_or_default();
    let authority = address.strip_prefix("https://").filter(|authority| !authority.is_empty()).ok_or_else(|| Fail(StatusCode::CONFLICT, "Save the address of Numa Home on Settings › Reachable first: a passkey belongs to it.".into()))?;
    let authority = authority.strip_suffix(":443").unwrap_or(authority);
    Ok((format!("https://{authority}"), authority.split(':').next().unwrap_or(authority).to_string()))
}

pub fn here(server: &Server, peer: SocketAddr, headers: &HeaderMap) -> bool {
    let Ok((origin, _)) = relying(&server.store) else { return false };
    let asked = |name: &str| headers.get(name).and_then(|value| value.to_str().ok()).map(|host| format!("https://{}", host.to_ascii_lowercase()));
    auth::client(peer, headers).1 && [asked("host"), asked("x-forwarded-host")].into_iter().flatten().any(|asked| asked.strip_suffix(":443").unwrap_or(&asked) == origin)
}

#[derive(Deserialize)]
struct Client {
    #[serde(rename = "type")]
    kind: String,
    challenge: String,
    origin: String,
    #[serde(default, rename = "crossOrigin")]
    cross_origin: bool,
}

fn ceremony<'a>(kind: &str, client: &[u8], data: &'a [u8], origin: &str, host: &str) -> Result<(String, u32, u8, &'a [u8]), &'static str> {
    let said: Client = serde_json::from_slice(client).map_err(|_| "The browser's answer cannot be read.")?;
    if said.kind != kind || said.cross_origin || said.origin != origin {
        return Err("That answer was not made at Numa Home's address.");
    }
    if data.len() < 37 || data[..32] != Sha256::digest(host.as_bytes())[..] {
        return Err("That passkey is for another address.");
    }
    let flags = data[32];
    if flags & PRESENT == 0 || flags & VERIFIED == 0 {
        return Err("The device did not check who you are. Use its fingerprint, face or code.");
    }
    Ok((said.challenge, u32::from_be_bytes([data[33], data[34], data[35], data[36]]), flags, &data[37..]))
}

#[derive(Deserialize)]
pub struct Offered {
    id: String,
    client: String,

    attestation: Option<String>,

    data: Option<String>,

    key: String,
    alg: i64,
    label: Option<String>,
}

const UNREADABLE: &str = "The browser's answer cannot be read.";

fn auth_data(cbor: &[u8]) -> Option<&[u8]> {
    let mut at = 0;
    let (major, count) = cbor_head(cbor, &mut at)?;
    if major != 5 {
        return None;
    }
    let mut found = None;
    for _ in 0..count {
        let (major, length) = cbor_head(cbor, &mut at)?;
        if major != 3 {
            return None;
        }
        if cbor_take(cbor, &mut at, length)? != b"authData" {
            cbor_skip(cbor, &mut at, 1)?;
            continue;
        }
        let (major, length) = cbor_head(cbor, &mut at)?;
        if major != 2 || found.is_some() {
            return None;
        }
        found = Some(cbor_take(cbor, &mut at, length)?);
    }
    found.filter(|_| at == cbor.len())
}

fn cbor_take<'a>(cbor: &'a [u8], at: &mut usize, length: u64) -> Option<&'a [u8]> {
    let end = at.checked_add(usize::try_from(length).ok()?)?;
    let taken = cbor.get(*at..end)?;
    *at = end;
    Some(taken)
}

fn cbor_head(cbor: &[u8], at: &mut usize) -> Option<(u8, u64)> {
    let first = cbor_take(cbor, at, 1)?[0];
    let size = match first & 0x1f {
        info @ 0..=23 => return Some((first >> 5, u64::from(info))),
        24 => 1,
        25 => 2,
        26 => 4,
        27 => 8,
        _ => return None,
    };
    Some((first >> 5, cbor_take(cbor, at, size)?.iter().fold(0, |value, byte| value << 8 | u64::from(*byte))))
}

fn cbor_skip(cbor: &[u8], at: &mut usize, depth: u8) -> Option<()> {
    if depth > 8 {
        return None;
    }
    let (major, value) = cbor_head(cbor, at)?;
    match major {
        2 | 3 => {
            cbor_take(cbor, at, value)?;
        }
        4 => (0..value).try_for_each(|_| cbor_skip(cbor, at, depth + 1))?,
        5 => (0..value).try_for_each(|_| cbor_skip(cbor, at, depth + 1).and_then(|_| cbor_skip(cbor, at, depth + 1)))?,
        6 => cbor_skip(cbor, at, depth + 1)?,

        _ => {}
    }
    Some(())
}

fn accept(offered: &Offered, origin: &str, host: &str) -> Result<(String, Vec<u8>, u32), &'static str> {
    let (client, key, id) = (unb64(&offered.client).ok_or(UNREADABLE)?, unb64(&offered.key).ok_or(UNREADABLE)?, unb64(&offered.id).ok_or(UNREADABLE)?);
    let data = match (&offered.attestation, &offered.data) {
        (Some(attestation), _) => auth_data(&unb64(attestation).ok_or(UNREADABLE)?).ok_or(UNREADABLE)?.to_vec(),
        (None, Some(data)) => unb64(data).ok_or(UNREADABLE)?,
        (None, None) => return Err(UNREADABLE),
    };
    let (challenge, count, flags, rest) = ceremony("webauthn.create", &client, &data, origin, host)?;
    if offered.alg != -7 {
        return Err("This device's passkey is of a kind Numa Home does not read.");
    }

    if flags & ATTESTED == 0 || rest.len() < 18 {
        return Err(UNREADABLE);
    }
    let length = usize::from(u16::from_be_bytes([rest[16], rest[17]]));
    if id.is_empty() || id.len() > 1023 || rest.get(18..18 + length) != Some(&id[..]) {
        return Err(UNREADABLE);
    }
    let key = VerifyingKey::from_public_key_der(&key).map_err(|_| "This device's passkey is of a kind Numa Home does not read.")?;
    Ok((challenge, key.to_encoded_point(true).as_bytes().to_vec(), count))
}

#[derive(Deserialize)]
pub struct Shown {
    id: String,
    client: String,
    data: String,
    signature: String,
    device: Option<String>,
}

fn proven(shown: &Shown, key: &[u8], origin: &str, host: &str) -> Result<(String, u32), &'static str> {
    let (client, data, signature) = (unb64(&shown.client).ok_or(UNREADABLE)?, unb64(&shown.data).ok_or(UNREADABLE)?, unb64(&shown.signature).ok_or(UNREADABLE)?);
    let (challenge, count, _, _) = ceremony("webauthn.get", &client, &data, origin, host)?;
    let key = VerifyingKey::from_sec1_bytes(key).map_err(|_| UNREADABLE)?;
    let signature = Signature::from_der(&signature).map_err(|_| UNREADABLE)?;
    let signed = [&data[..], &Sha256::digest(&client)[..]].concat();
    key.verify(&signed, &signature).map_err(|_| "That passkey is not one of Numa Home's.")?;
    Ok((challenge, count))
}

fn counted_on(kept: u32, new: u32) -> bool {
    (kept == 0 && new == 0) || new > kept
}

async fn list(State(server): State<Arc<Server>>, Signed(me): Signed, ConnectInfo(peer): ConnectInfo<SocketAddr>, headers: HeaderMap) -> Json<Value> {
    let passkeys: Vec<Value> = server.store.passkeys(&me.name).into_iter().map(|(id, label, made, used)| json!({ "id": id, "label": label, "made": made, "used": used })).collect();
    Json(json!({ "here": here(&server, peer, &headers), "address": relying(&server.store).is_ok(), "passkeys": passkeys }))
}

#[derive(Deserialize)]
struct Asked {
    password: String,
}

async fn sure(server: &Server, me: &crate::store::Account, password: String, peer: SocketAddr, headers: &HeaderMap) -> Result<(), Fail> {
    match crate::accounts::checked(server, &me.name, password, auth::client(peer, headers).0, !auth::in_house(peer, headers)).await? {
        Some(_) => Ok(()),
        None => Err(Fail(StatusCode::FORBIDDEN, "That is not your current password.".into())),
    }
}

async fn options(State(server): State<Arc<Server>>, Signed(me): Signed, ConnectInfo(peer): ConnectInfo<SocketAddr>, headers: HeaderMap, Json(asked): Json<Asked>) -> Result<Json<Value>, Fail> {
    let (_, host) = relying(&server.store)?;
    sure(&server, &me, asked.password, peer, &headers).await?;
    let handle = server.store.passkey_handle(&me.name).map_err(Fail::internal)?;
    let have: Vec<String> = server.store.passkeys(&me.name).into_iter().map(|(id, ..)| id).collect();
    Ok(Json(json!({
        "challenge": challenge(&server.keys, For::Add, &me.name, now()),
        "rp": { "id": host, "name": "Numa Home" },
        "user": { "id": handle, "name": me.name },
        "exclude": have,
    })))
}

async fn add(State(server): State<Arc<Server>>, Signed(me): Signed, Json(offered): Json<Offered>) -> Result<StatusCode, Fail> {
    let (origin, host) = relying(&server.store)?;
    let (answered, key, count) = accept(&offered, &origin, &host).map_err(Fail::bad)?;
    if server.store.passkeys(&me.name).len() >= MOST_PASSKEYS {
        return Err(Fail::bad("An account has at most 20 passkeys. Remove one first."));
    }
    if !spend(&server.keys, For::Add, &me.name, &answered, now()) {
        return Err(Fail::bad("That took too long. Try again."));
    }
    let label: String = offered.label.as_deref().unwrap_or("").trim().chars().filter(|c| !c.is_control()).take(60).collect();
    let label = if label.is_empty() { "Passkey".to_string() } else { label };
    if !server.store.add_passkey(&offered.id, &me.name, &key, count, &label).map_err(Fail::internal)? {
        return Err(Fail(StatusCode::CONFLICT, "This passkey is kept already.".into()));
    }
    log::warn!("{} added a passkey ({label})", me.name);
    Ok(StatusCode::CREATED)
}

#[derive(Deserialize)]
struct Removed {
    id: String,
    password: String,
}

async fn remove(State(server): State<Arc<Server>>, Signed(me): Signed, ConnectInfo(peer): ConnectInfo<SocketAddr>, headers: HeaderMap, Json(asked): Json<Removed>) -> Result<StatusCode, Fail> {
    sure(&server, &me, asked.password, peer, &headers).await?;
    match server.store.remove_passkey(&asked.id, &me.name) {
        true => {
            log::warn!("{} removed a passkey", me.name);
            Ok(StatusCode::NO_CONTENT)
        }
        false => Err(Fail(StatusCode::NOT_FOUND, "There is no such passkey.".into())),
    }
}

async fn begin(State(server): State<Arc<Server>>) -> Result<Json<Value>, Fail> {
    let (_, host) = relying(&server.store)?;
    Ok(Json(json!({ "challenge": challenge(&server.keys, For::SignIn, "", now()), "rp": host })))
}

async fn sign_in(State(server): State<Arc<Server>>, ConnectInfo(peer): ConnectInfo<SocketAddr>, headers: HeaderMap, Json(shown): Json<Shown>) -> Result<Response, Fail> {
    let (origin, host) = relying(&server.store)?;
    let (address, https) = auth::client(peer, &headers);
    let outside = !auth::in_house(peer, &headers);
    let keys = [auth::limit_keys("", address, outside)[1].clone()];
    let left = server.limit.reserve(&keys, now());
    if left > 0 {
        server.store.log_sign_in("(passkey)", &address.to_string(), outside, "locked");
        return Err(Fail(StatusCode::TOO_MANY_REQUESTS, format!("Too many wrong tries. {}", auth::try_again(left))));
    }
    let kept = server.store.passkey(&shown.id);
    let proved = kept.as_ref().and_then(|(account, key, count)| {
        let (answered, new) = proven(&shown, key, &origin, &host).ok()?;
        (counted_on(*count, new) && spend(&server.keys, For::SignIn, "", &answered, now())).then(|| (account.clone(), new))
    });
    let known = proved.and_then(|(account, count)| server.store.account(&account).map(|(name, _, admin)| (name, admin, count)));
    let Some((name, admin, count)) = known else {
        let tried = kept.map_or("(passkey)".to_string(), |(account, ..)| account);
        server.store.log_sign_in(&tried, &address.to_string(), outside, "wrong");
        if outside {
            log::warn!("sign-in from outside: a wrong passkey for {tried:?} from {address}");
        }
        return Err(Fail(StatusCode::UNAUTHORIZED, "That passkey is not one of Numa Home's.".into()));
    };
    server.limit.clear(&keys);
    server.store.used_passkey(&shown.id, count);
    server.store.log_sign_in(&name, &address.to_string(), outside, "ok");
    crate::accounts::signed_in(&server, &name, admin, shown.device.as_deref(), &address.to_string(), https)
}

#[derive(Deserialize)]
struct Allowed {

    challenge: String,
}

async fn allow(State(server): State<Arc<Server>>, Signed(me): Signed, Json(asked): Json<Allowed>) -> Result<Json<Value>, Fail> {
    if asked.challenge.len() != 64 || !asked.challenge.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(Fail::bad("Start signing in from the app again."));
    }
    let (code, _) = auth::token();
    let mut codes = server.keys.codes.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    codes.retain(|_, handoff| handoff.until >= now());
    if codes.len() >= MOST_CODES {
        return Err(Fail(StatusCode::TOO_MANY_REQUESTS, "Too many apps are signing in at once. Try again in a few minutes.".into()));
    }
    codes.insert(auth::digest(&code), Handoff { challenge: asked.challenge.to_ascii_lowercase(), account: me.name.clone(), until: now() + CODE_TIME });
    log::info!("{} let an app sign in", me.name);
    Ok(Json(json!({ "code": code })))
}

#[derive(Deserialize)]
struct Claimed {
    code: String,
    verifier: String,
    device: Option<String>,
}

async fn token(State(server): State<Arc<Server>>, ConnectInfo(peer): ConnectInfo<SocketAddr>, headers: HeaderMap, Json(claimed): Json<Claimed>) -> Result<Response, Fail> {
    let (address, https) = auth::client(peer, &headers);
    let outside = !auth::in_house(peer, &headers);
    let keys = [auth::limit_keys("", address, outside)[1].clone()];
    let left = server.limit.reserve(&keys, now());
    if left > 0 {
        return Err(Fail(StatusCode::TOO_MANY_REQUESTS, format!("Too many wrong tries. {}", auth::try_again(left))));
    }
    let handoff = server.keys.codes.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).remove(&auth::digest(&claimed.code));
    let account = handoff
        .filter(|handoff| handoff.until >= now() && auth::same_bytes(auth::digest(&claimed.verifier).as_bytes(), handoff.challenge.as_bytes()))
        .and_then(|handoff| server.store.account(&handoff.account));
    let Some((name, _, admin)) = account else {
        server.store.log_sign_in("(app)", &address.to_string(), outside, "wrong");
        return Err(Fail(StatusCode::UNAUTHORIZED, "That sign-in has ended. Start it from the app again.".into()));
    };
    server.limit.clear(&keys);
    server.store.log_sign_in(&name, &address.to_string(), outside, "ok");
    crate::accounts::signed_in(&server, &name, admin, claimed.device.as_deref(), &address.to_string(), https)
}

pub fn routes() -> Router<Arc<Server>> {
    Router::new()
        .route("/api/v1/passkeys", get(list).post(add))
        .route("/api/v1/passkeys/options", post(options))
        .route("/api/v1/passkeys/remove", post(remove))
        .route("/api/v1/passkey/challenge", post(begin))
        .route("/api/v1/passkey/login", post(sign_in))
        .route("/api/v1/app/allow", post(allow))
        .route("/api/v1/app/token", post(token))
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use p256::ecdsa::signature::Signer;
    use p256::ecdsa::SigningKey;
    use p256::pkcs8::EncodePublicKey;

    const ORIGIN: &str = "https://photos.example.org";
    const HOST: &str = "photos.example.org";

    pub struct Device {
        key: SigningKey,
        pub id: Vec<u8>,
        pub count: u32,
    }

    impl Device {
        pub fn new() -> Self {
            let mut id = vec![0u8; 20];
            OsRng.fill_bytes(&mut id);
            Device { key: SigningKey::random(&mut OsRng), id, count: 0 }
        }

        fn client(kind: &str, challenge: &str, origin: &str) -> Vec<u8> {
            json!({ "type": kind, "challenge": challenge, "origin": origin, "crossOrigin": false }).to_string().into_bytes()
        }

        fn data(&self, host: &str, flags: u8, attested: bool) -> Vec<u8> {
            let mut data = Sha256::digest(host.as_bytes()).to_vec();
            data.push(flags | if attested { ATTESTED } else { 0 });
            data.extend(self.count.to_be_bytes());
            if attested {
                data.extend([0u8; 16]);
                data.extend((self.id.len() as u16).to_be_bytes());
                data.extend(&self.id);
                data.extend(b"the key again, in CBOR, which is not read");
            }
            data
        }

        pub fn offer(&self, challenge: &str, origin: &str, host: &str) -> Offered {
            let key = p256::PublicKey::from(self.key.verifying_key()).to_public_key_der().unwrap();
            let data = self.data(host, PRESENT | VERIFIED, true);
            Offered {
                id: b64(&self.id),
                client: b64(&Self::client("webauthn.create", challenge, origin)),
                attestation: Some(b64(&attestation(&data))),
                data: Some(b64(&data)),
                key: b64(key.as_bytes()),
                alg: -7,
                label: Some("Test".into()),
            }
        }

        pub fn show(&self, challenge: &str, origin: &str, host: &str, flags: u8) -> Shown {
            let (client, data) = (Self::client("webauthn.get", challenge, origin), self.data(host, flags, false));
            let signature: Signature = self.key.sign(&[&data[..], &Sha256::digest(&client)[..]].concat());
            Shown { id: b64(&self.id), client: b64(&client), data: b64(&data), signature: b64(signature.to_der().as_bytes()), device: None }
        }
    }

    fn head(major: u8, value: usize) -> Vec<u8> {
        match value {
            0..=23 => vec![major << 5 | value as u8],
            24..=255 => vec![major << 5 | 24, value as u8],
            _ => [&[major << 5 | 25][..], &(value as u16).to_be_bytes()].concat(),
        }
    }

    fn text(word: &str) -> Vec<u8> {
        [head(3, word.len()), word.as_bytes().to_vec()].concat()
    }

    fn attestation(data: &[u8]) -> Vec<u8> {
        [head(5, 3), text("fmt"), text("none"), text("attStmt"), head(5, 0), text("authData"), head(2, data.len()), data.to_vec()].concat()
    }

    #[test]
    fn the_seal_is_hmac_sha256() {
        let mut key = [0u8; 32];
        key[..4].copy_from_slice(b"Jefe");
        assert_eq!(auth::hex(&hmac(&key, b"what do ya want for nothing?")), "5bdcc146bf60754e6a042426089575c75a003f089d2739839dec58b964ec3843");
    }

    #[test]
    fn a_challenge_is_answered_once_in_time_and_for_what_it_was_made() {
        let keys = Keys::default();
        let made = challenge(&keys, For::Add, "Sam", 1000);
        assert!(!spend(&keys, For::SignIn, "Sam", &made, 1001), "made for adding, not for signing in");
        assert!(!spend(&keys, For::Add, "Kim", &made, 1001), "Sam's, not Kim's");
        assert!(!spend(&keys, For::Add, "Sam", &made, 1000 + CHALLENGE_TIME + 1), "too late");
        assert!(spend(&keys, For::Add, "sam", &made, 1001), "a name is one whatever its case");
        assert!(!spend(&keys, For::Add, "Sam", &made, 1002), "once");
        let other = Keys::default();
        assert!(!spend(&other, For::Add, "Sam", &challenge(&keys, For::Add, "Sam", 1000), 1001), "another start's challenge");
        let mut bent = unb64(&challenge(&keys, For::SignIn, "", 1000)).unwrap();
        bent[20] ^= 1;
        assert!(!spend(&keys, For::SignIn, "", &b64(&bent), 1001), "its time moved on by hand");
        assert!(!spend(&keys, For::SignIn, "", "", 1001) && !spend(&keys, For::SignIn, "", "not a challenge", 1001));
    }

    #[test]
    fn a_passkey_is_made_and_shown() {
        let device = Device::new();
        let (answered, key, count) = accept(&device.offer("c1", ORIGIN, HOST), ORIGIN, HOST).unwrap();
        assert_eq!((answered.as_str(), count, key.len()), ("c1", 0, 33));
        let (answered, count) = proven(&device.show("c2", ORIGIN, HOST, PRESENT | VERIFIED), &key, ORIGIN, HOST).unwrap();
        assert_eq!((answered.as_str(), count), ("c2", 0));
    }

    #[test]
    fn what_is_not_ours_is_refused() {
        let (device, other) = (Device::new(), Device::new());
        let (_, key, _) = accept(&device.offer("c", ORIGIN, HOST), ORIGIN, HOST).unwrap();
        let shown = |origin: &str, host: &str, flags: u8| proven(&device.show("c", origin, host, flags), &key, ORIGIN, HOST);
        assert!(shown(ORIGIN, HOST, PRESENT | VERIFIED).is_ok());
        assert!(shown("https://evil.example", HOST, PRESENT | VERIFIED).is_err(), "asked from another site");
        assert!(shown("http://photos.example.org", HOST, PRESENT | VERIFIED).is_err(), "not over https");
        assert!(shown(ORIGIN, "evil.example", PRESENT | VERIFIED).is_err(), "a passkey of another address");
        assert!(shown(ORIGIN, HOST, PRESENT).is_err(), "nobody showed who they are");
        assert!(shown(ORIGIN, HOST, VERIFIED).is_err(), "nobody was there");
        assert!(proven(&other.show("c", ORIGIN, HOST, PRESENT | VERIFIED), &key, ORIGIN, HOST).is_err(), "another device's key");
        let mut changed = device.show("c", ORIGIN, HOST, PRESENT | VERIFIED);
        changed.client = b64(&Device::client("webauthn.get", "another challenge", ORIGIN));
        assert!(proven(&changed, &key, ORIGIN, HOST).is_err(), "the challenge changed after signing");
        let mut made = device.show("c", ORIGIN, HOST, PRESENT | VERIFIED);
        made.client = b64(&Device::client("webauthn.create", "c", ORIGIN));
        assert!(proven(&made, &key, ORIGIN, HOST).is_err(), "an answer of the other kind");

        assert!(accept(&device.offer("c", "https://evil.example", HOST), ORIGIN, HOST).is_err());
        assert!(accept(&device.offer("c", ORIGIN, "evil.example"), ORIGIN, HOST).is_err());
        let mut rsa = device.offer("c", ORIGIN, HOST);
        rsa.alg = -257;
        assert!(accept(&rsa, ORIGIN, HOST).is_err(), "only ES256");
        let mut another = device.offer("c", ORIGIN, HOST);
        another.id = b64(b"not the id the device signed for");
        assert!(accept(&another, ORIGIN, HOST).is_err(), "the id is the one in the device's data");
        let mut short = device.offer("c", ORIGIN, HOST);
        short.attestation = None;
        short.data = Some(b64(&unb64(short.data.as_deref().unwrap()).unwrap()[..40]));
        assert!(accept(&short, ORIGIN, HOST).is_err());
        let mut none = device.offer("c", ORIGIN, HOST);
        (none.attestation, none.data) = (None, None);
        assert!(accept(&none, ORIGIN, HOST).is_err());
    }

    #[test]
    fn a_passkey_manager_without_the_credential_in_its_data() {
        let device = Device::new();
        let mut offered = device.offer("c", ORIGIN, HOST);
        offered.data = Some(b64(&device.data(HOST, PRESENT | VERIFIED, false)));
        let (answered, key, _) = accept(&offered, ORIGIN, HOST).unwrap();
        assert_eq!((answered.as_str(), key.len()), ("c", 33));
        offered.attestation = None;
        assert_eq!(accept(&offered, ORIGIN, HOST), Err(UNREADABLE), "without the attestation object it cannot be read");
        offered.data = None;
        offered.attestation = Some(b64(&attestation(&device.data(HOST, PRESENT | VERIFIED, false))));
        assert_eq!(accept(&offered, ORIGIN, HOST), Err(UNREADABLE), "nor with the short data inside it");
    }

    #[test]
    fn a_page_of_before_the_attestation_object() {
        let device = Device::new();
        let mut offered = device.offer("c", ORIGIN, HOST);
        offered.attestation = None;
        assert!(accept(&offered, ORIGIN, HOST).is_ok());
    }

    #[test]
    fn an_attestation_object_is_read_strictly() {
        let data = Device::new().data(HOST, PRESENT | VERIFIED, true);
        let whole = attestation(&data);
        assert_eq!(auth_data(&whole), Some(&data[..]));
        for end in 0..whole.len() {
            assert_eq!(auth_data(&whole[..end]), None, "cut at {end}");
        }
        assert_eq!(auth_data(&[whole.clone(), vec![0]].concat()), None, "something after the map");
        let mut bytes = [0u8; 64];
        for seed in 0..2000u32 {
            let mut state = seed.wrapping_mul(2_654_435_761) | 1;
            for byte in bytes.iter_mut() {
                state ^= state << 13;
                state ^= state >> 17;
                state ^= state << 5;
                *byte = state as u8;
            }
            let _ = auth_data(&bytes);
            let _ = auth_data(&[&whole[..20], &bytes[..]].concat());
        }
        let indefinite_map = [&[0xbf][..], &text("authData"), &head(2, 1), &[7, 0xff]].concat();
        assert_eq!(auth_data(&indefinite_map), None, "a map of indefinite length");
        let indefinite_bytes = [&head(5, 1)[..], &text("authData"), &[0x5f, 0x41, 7, 0xff]].concat();
        assert_eq!(auth_data(&indefinite_bytes), None, "a byte string of indefinite length");
        let indefinite_skip = [&head(5, 2)[..], &text("attStmt"), &[0x9f, 0xff], &text("authData"), &head(2, 1), &[7]].concat();
        assert_eq!(auth_data(&indefinite_skip), None, "an array of indefinite length passed over");
        let nested = |depth: usize| [&head(5, 2)[..], &text("attStmt"), &vec![0x81; depth], &[0], &text("authData"), &head(2, 1), &[7]].concat();
        assert_eq!(auth_data(&nested(7)), Some(&[7][..]), "nesting to 8");
        assert_eq!(auth_data(&nested(8)), None, "nesting deeper than 8");
        let huge = [&head(5, 1)[..], &text("authData"), &[0x5b, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff]].concat();
        assert_eq!(auth_data(&huge), None, "a length past the input");
        let many = [&head(5, 2)[..], &text("attStmt"), &[0x9b, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0]].concat();
        assert_eq!(auth_data(&many), None, "a count past the input");
        let twice = [&head(5, 2)[..], &text("authData"), &head(2, 1), &[7], &text("authData"), &head(2, 1), &[8]].concat();
        assert_eq!(auth_data(&twice), None, "authData twice");
        assert_eq!(auth_data(&[&head(5, 1)[..], &head(0, 1), &head(2, 1), &[7]].concat()), None, "a key that is not text");
        assert_eq!(auth_data(&[&head(5, 1)[..], &text("authData"), &text("x")].concat()), None, "authData that is not bytes");
        assert_eq!(auth_data(&[&head(5, 1)[..], &text("fmt"), &text("none")].concat()), None, "no authData");
        let mut offered = Device::new().offer("c", ORIGIN, HOST);
        offered.attestation = Some(b64(b"not CBOR at all"));
        assert_eq!(accept(&offered, ORIGIN, HOST), Err(UNREADABLE), "a bad attestation object is not passed over for the data");
    }

    #[test]
    fn a_device_that_counts_must_count_on() {
        assert!(counted_on(0, 0), "a passkey kept in step says 0 every time");
        assert!(counted_on(0, 1) && counted_on(4, 5));
        assert!(!counted_on(5, 5) && !counted_on(5, 4) && !counted_on(5, 0), "a copy of the device is behind");
    }

    #[test]
    fn passkeys_are_kept_per_account() {
        let store = Store::open(std::path::Path::new(":memory:")).unwrap();
        tables(&store).unwrap();
        assert!(store.add_passkey("a", "Sam", b"key", 0, "Phone").unwrap());
        assert!(!store.add_passkey("a", "Kim", b"other", 0, "Phone").unwrap(), "an id is kept once");
        assert!(!store.passkeys("sam").is_empty() && store.passkeys("Kim").is_empty());
        assert_eq!(store.passkey("a").map(|(account, key, count)| (account, key, count)), Some(("Sam".to_string(), b"key".to_vec(), 0)));
        store.used_passkey("a", 7);
        assert_eq!(store.passkey("a").unwrap().2, 7);
        assert!(!store.remove_passkey("a", "Kim"), "not Kim's to remove");
        assert!(store.remove_passkey("a", "Sam") && store.passkeys("Sam").is_empty());
        let handle = store.passkey_handle("Sam").unwrap();
        assert_eq!(store.passkey_handle("sam").unwrap(), handle, "made once");
        assert_ne!(store.passkey_handle("Kim").unwrap(), handle);
    }

    #[test]
    fn the_address_a_passkey_belongs_to() {
        let store = Store::open(std::path::Path::new(":memory:")).unwrap();
        assert!(relying(&store).is_err(), "none saved yet");
        store.set_setting("reach:address", "https://photos.example.org:443").unwrap();
        assert_eq!(relying(&store).ok(), Some((ORIGIN.to_string(), HOST.to_string())), "a browser leaves 443 out");
        store.set_setting("reach:address", "https://photos.example.org:8443").unwrap();
        assert_eq!(relying(&store).ok(), Some(("https://photos.example.org:8443".to_string(), HOST.to_string())));
    }

    fn status(answer: Result<Response, Fail>) -> u16 {
        match answer {
            Ok(answer) => answer.status().as_u16(),
            Err(Fail(status, _)) => status.as_u16(),
        }
    }

    #[test]
    fn an_account_signs_in_with_its_passkey() {
        let server = crate::share::tests::http::fixture().server.clone();
        let runtime = tokio::runtime::Runtime::new().unwrap();
        server.store.set_setting("reach:address", ORIGIN).unwrap();
        server.store.add_account("Pat Key", &auth::hash("correct horse").unwrap(), false, false).unwrap();
        let me = || Signed(server.store.session(&auth::digest(&crate::share::tests::http::session(&server, "Pat Key"))).unwrap());
        let (house, state) = (ConnectInfo(SocketAddr::from(([192, 168, 1, 20], 4000))), || State(server.clone()));
        let outside = |last: u8| ConnectInfo(SocketAddr::from(([203, 0, 113, last], 4000)));
        let said = |answer: Json<Value>, key: &str| answer.0[key].as_str().unwrap().to_string();
        let password = |from: ConnectInfo<SocketAddr>| {
            let login = crate::accounts::Login { name: "Pat Key".into(), password: "correct horse".into(), device: None };
            status(runtime.block_on(crate::accounts::login(state(), from, HeaderMap::new(), Json(login))))
        };
        assert_eq!((password(house), password(outside(1))), (200, 200), "no passkey yet: the password signs in from anywhere");

        let wrong = runtime.block_on(options(state(), me(), house, HeaderMap::new(), Json(Asked { password: "wrong".into() })));
        assert!(matches!(wrong, Err(Fail(StatusCode::FORBIDDEN, _))), "a passkey is added after the password");
        let offer = runtime.block_on(options(state(), me(), house, HeaderMap::new(), Json(Asked { password: "correct horse".into() }))).ok().unwrap();
        assert_eq!(offer.0["rp"]["id"], HOST);
        let mut device = Device::new();
        let made = said(offer, "challenge");
        let added = |device: &Device| runtime.block_on(add(state(), me(), Json(device.offer(&made, ORIGIN, HOST)))).map_err(|Fail(status, _)| status.as_u16());
        assert_eq!(added(&device), Ok(StatusCode::CREATED));
        assert_eq!(added(&Device::new()), Err(400), "a challenge makes one passkey");
        let listed = runtime.block_on(list(state(), me(), house, HeaderMap::new()));
        assert_eq!((listed.0["passkeys"].as_array().unwrap().len(), listed.0["passkeys"][0]["label"].as_str()), (1, Some("Test")));

        let asked = || said(runtime.block_on(begin(state())).ok().unwrap(), "challenge");
        let shown = |device: &Device, challenge: &str, from: ConnectInfo<SocketAddr>| status(runtime.block_on(sign_in(state(), from, HeaderMap::new(), Json(device.show(challenge, ORIGIN, HOST, PRESENT | VERIFIED)))));
        let once = asked();
        assert_eq!(shown(&device, &once, outside(2)), 200, "the passkey signs in from outside");
        assert_eq!(shown(&device, &once, outside(3)), 401, "an answer is good once");
        assert_eq!(shown(&device, &made, outside(4)), 401, "a challenge for adding does not sign in");
        assert_eq!(shown(&Device::new(), &asked(), outside(5)), 401, "a passkey Numa Home does not keep");
        device.count = 5;
        assert_eq!(shown(&device, &asked(), outside(6)), 200);
        device.count = 3;
        assert_eq!(shown(&device, &asked(), outside(7)), 401, "a copy of the device is behind its count");
        assert_eq!((password(outside(8)), password(house)), (200, 200), "a passkey is offered, not forced: the password still signs in from anywhere");

        let allowed = || said(runtime.block_on(allow(state(), me(), Json(Allowed { challenge: auth::digest("the app's secret") }))).ok().unwrap(), "code");
        let claimed = |code: &str, verifier: &str, from: ConnectInfo<SocketAddr>| status(runtime.block_on(token(state(), from, HeaderMap::new(), Json(Claimed { code: code.into(), verifier: verifier.into(), device: Some("Pat's phone".into()) }))));
        let code = allowed();
        assert_eq!(claimed(&code, "another app's secret", outside(9)), 401, "the code alone opens nothing");
        assert_eq!(claimed(&code, "the app's secret", outside(10)), 401, "and it is good once, also for a wrong try");
        let code = allowed();
        assert_eq!(claimed(&code, "the app's secret", outside(11)), 200);
        assert_eq!(claimed(&code, "the app's secret", outside(12)), 401);
        assert!(runtime.block_on(allow(state(), me(), Json(Allowed { challenge: "short".into() }))).is_err());

        let id = b64(&device.id);
        let removed = |password: &str| runtime.block_on(remove(state(), me(), house, HeaderMap::new(), Json(Removed { id: id.clone(), password: password.into() }))).map_err(|Fail(status, _)| status.as_u16());
        assert_eq!(removed("wrong"), Err(403));
        assert_eq!(removed("correct horse"), Ok(StatusCode::NO_CONTENT));
        assert_eq!(password(outside(13)), 200, "the last passkey gone, the password signs in as before");
    }
}
