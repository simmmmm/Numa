use std::collections::HashMap;
use std::net::{IpAddr, SocketAddr};
use std::sync::{Arc, Mutex, OnceLock};

use argon2::password_hash::rand_core::{OsRng, RngCore};
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use axum::http::{HeaderMap, StatusCode};
use sha2::{Digest, Sha256};

use crate::store::Account;
use crate::{Fail, Server};

pub const COOKIE: &str = "numa_session";

pub fn hash(password: &str) -> Result<String, String> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default().hash_password(password.as_bytes(), &salt).map(|hash| hash.to_string()).map_err(|err| err.to_string())
}

pub fn verify(password: &str, hash: Option<&str>) -> bool {
    static NOBODY: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    let nobody = NOBODY.get_or_init(|| self::hash("nobody").unwrap_or_default());
    let known = hash.is_some();
    let Ok(parsed) = PasswordHash::new(hash.unwrap_or(nobody)) else { return false };
    Argon2::default().verify_password(password.as_bytes(), &parsed).is_ok() && known
}

pub async fn argon<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> Result<T, Fail> {
    static TURNS: tokio::sync::Semaphore = tokio::sync::Semaphore::const_new(2);
    let _turn = TURNS.acquire().await.map_err(Fail::internal)?;
    tokio::task::spawn_blocking(work).await.map_err(Fail::internal)
}

pub fn token() -> (String, String) {
    let mut bytes = [0u8; 32];
    OsRng.fill_bytes(&mut bytes);
    let token = hex(&bytes);
    let digest = digest(&token);
    (token, digest)
}

pub fn digest(token: &str) -> String {
    hex(&Sha256::digest(token.as_bytes()))
}

pub fn sha256_of(path: &std::path::Path) -> std::io::Result<String> {
    let mut hasher = Sha256::new();
    std::io::copy(&mut std::fs::File::open(path)?, &mut hasher)?;
    Ok(hex(&hasher.finalize()))
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub fn valid_name(name: &str) -> bool {
    let fine = |c: char| c.is_alphanumeric() || c == ' ' || c == '-' || c == '_';
    !name.is_empty() && name.chars().count() <= 40 && name.chars().all(fine) && name.trim() == name
}

pub fn presented(headers: &HeaderMap) -> Option<String> {
    if let Some(bearer) = headers.get("authorization").and_then(|value| value.to_str().ok()).and_then(|value| value.strip_prefix("Bearer ")) {
        return Some(bearer.trim().to_string());
    }
    headers
        .get_all("cookie")
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(';'))
        .find_map(|pair| pair.trim().strip_prefix(COOKIE).and_then(|rest| rest.strip_prefix('=')).map(str::to_string))
}

pub struct Signed(pub Account);

impl FromRequestParts<Arc<Server>> for Signed {
    type Rejection = Fail;

    async fn from_request_parts(parts: &mut Parts, server: &Arc<Server>) -> Result<Self, Fail> {
        let account = presented(&parts.headers)
            .and_then(|token| server.store.session(&digest(&token)))
            .ok_or_else(|| Fail(StatusCode::UNAUTHORIZED, "Sign in first.".into()))?;
        if account.must_change && !parts.uri.path().starts_with("/api/v1/me") {

            return Err(Fail(StatusCode::FORBIDDEN, "Choose a new password on Numa Home's page in a browser first.".into()));
        }
        Ok(Signed(account))
    }
}

#[derive(Default)]
pub struct Limit(Mutex<HashMap<String, (u32, i64)>>);

const FREE: u32 = 5;
const FIRST_WAIT: i64 = 30;
const LONGEST_WAIT: i64 = 15 * 60;
const FORGET: i64 = 86_400;

fn wait(failures: u32) -> i64 {
    match failures.checked_sub(FREE) {
        None => 0,
        Some(past) => (FIRST_WAIT << past.min(5)).min(LONGEST_WAIT),
    }
}

impl Limit {
    fn map(&self) -> std::sync::MutexGuard<'_, HashMap<String, (u32, i64)>> {
        self.0.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub fn left(&self, keys: &[String], now: i64) -> i64 {
        let map = self.map();
        keys.iter().filter_map(|key| map.get(key)).filter(|(_, last)| now - last < FORGET).map(|(failures, last)| last + wait(*failures) - now).max().unwrap_or(0).max(0)
    }

    pub fn reserve(&self, keys: &[String], now: i64) -> i64 {
        let mut map = self.map();
        let left = keys.iter().filter_map(|key| map.get(key)).filter(|(_, last)| now - last < FORGET).map(|(failures, last)| last + wait(*failures) - now).max().unwrap_or(0).max(0);
        if left == 0 {
            Self::count(&mut map, keys, now);
        }
        left
    }

    pub fn fail(&self, keys: &[String], now: i64) {
        Self::count(&mut self.map(), keys, now);
    }

    fn count(map: &mut HashMap<String, (u32, i64)>, keys: &[String], now: i64) {

        if map.len() > 10_000 {
            map.retain(|_, (_, last)| now - *last < FORGET);
        }
        for key in keys {
            let entry = map.entry(key.clone()).or_insert((0, now));
            *entry = (if now - entry.1 < FORGET { entry.0 + 1 } else { 1 }, now);
        }
    }

    pub fn clear(&self, keys: &[String]) {
        let mut map = self.map();
        for key in keys {
            map.remove(key);
        }
    }
}

pub fn limit_keys(name: &str, address: IpAddr) -> [String; 2] {
    let address = match address {
        IpAddr::V6(v6) => IpAddr::V6((u128::from(v6) & !(u64::MAX as u128)).into()),
        v4 => v4,
    };
    [format!("name:{}", name.trim().to_lowercase()), format!("address:{address}")]
}

pub fn try_again(seconds: i64) -> String {
    match seconds {
        ..=59 => format!("Try again in {} seconds.", seconds.max(1)),
        60..=119 => "Try again in a minute.".into(),
        _ => format!("Try again in {} minutes.", (seconds + 59) / 60),
    }
}

fn proxies() -> &'static [(IpAddr, u8)] {
    static PROXIES: OnceLock<Vec<(IpAddr, u8)>> = OnceLock::new();
    PROXIES.get_or_init(|| parse_proxies(&std::env::var("NUMA_HOME_PROXY").unwrap_or_default()))
}

fn parse_proxies(text: &str) -> Vec<(IpAddr, u8)> {
    text.split([',', ' '])
        .filter(|part| !part.is_empty())
        .filter_map(|part| {
            let (address, bits) = part.split_once('/').unwrap_or((part, ""));
            let address: IpAddr = address.parse().map_err(|_| log::warn!("NUMA_HOME_PROXY: {part} is not an address")).ok()?;
            let most = if address.is_ipv4() { 32 } else { 128 };
            Some((address, bits.parse().unwrap_or(most).min(most)))
        })
        .collect()
}

fn within(address: IpAddr, (network, bits): (IpAddr, u8)) -> bool {
    let same = |a: u128, n: u128, width: u32| bits == 0 || (a ^ n) >> (width - bits as u32) == 0;
    match (address.to_canonical(), network) {
        (IpAddr::V4(a), IpAddr::V4(n)) => same(u32::from(a).into(), u32::from(n).into(), 32),
        (IpAddr::V6(a), IpAddr::V6(n)) => same(a.into(), n.into(), 128),
        _ => false,
    }
}

pub fn client(peer: SocketAddr, headers: &HeaderMap) -> (IpAddr, bool) {
    client_among(peer.ip().to_canonical(), headers, proxies())
}

fn client_among(peer: IpAddr, headers: &HeaderMap, proxies: &[(IpAddr, u8)]) -> (IpAddr, bool) {
    let trusted = |address: IpAddr| proxies.iter().any(|proxy| within(address, *proxy));
    let values = |name: &str| headers.get_all(name).iter().filter_map(|value| value.to_str().ok()).flat_map(|value| value.split(',')).map(str::trim).collect::<Vec<_>>();
    if !trusted(peer) {
        static SAID: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
        if headers.contains_key("x-forwarded-for") && !SAID.swap(true, std::sync::atomic::Ordering::Relaxed) {
            log::warn!("a request came through a proxy at {peer}, which NUMA_HOME_PROXY does not name: its X-Forwarded-For is not believed");
        }
        return (peer, false);
    }
    let hops: Vec<IpAddr> = values("x-forwarded-for")
        .into_iter()
        .filter_map(|hop| hop.parse::<IpAddr>().ok().or_else(|| hop.parse::<SocketAddr>().ok().map(|socket| socket.ip())))
        .map(|hop| hop.to_canonical())
        .collect();
    let address = hops.iter().rev().find(|hop| !trusted(**hop)).or(hops.first()).copied().unwrap_or(peer);
    let https = values("x-forwarded-proto").first().is_some_and(|proto| proto.eq_ignore_ascii_case("https"));
    (address, https)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passwords_and_names() {
        let hashed = hash("correct horse").unwrap();
        assert!(verify("correct horse", Some(&hashed)));
        assert!(!verify("wrong", Some(&hashed)));
        assert!(!verify("nobody", None), "an unknown name never signs in");
        assert!(valid_name("Amélie") && valid_name("Sam Jansen"));
        for bad in ["", "..", ".hidden", "a/b", "a\\b", " lead", "x".repeat(41).as_str()] {
            assert!(!valid_name(bad), "{bad:?}");
        }
        let (token, kept) = token();
        assert_eq!(token.len(), 64);
        assert_eq!(digest(&token), kept);
    }

    #[test]
    fn sign_ins_at_once_are_counted_before_they_are_checked() {
        let limit = Limit::default();
        let tom = limit_keys("tom", "203.0.113.7".parse().unwrap());
        assert_eq!((0..5).map(|_| limit.reserve(&tom, 3000)).collect::<Vec<_>>(), [0; 5]);
        assert_eq!(limit.reserve(&tom, 3000), 30);
        limit.clear(&tom);
        assert_eq!(limit.reserve(&tom, 3000), 0, "a right password clears it");
    }

    #[test]
    fn wrong_passwords_slow_down() {
        assert_eq!((0..=11).map(wait).collect::<Vec<_>>(), [0, 0, 0, 0, 0, 30, 60, 120, 240, 480, 900, 900]);
        let limit = Limit::default();
        let here: IpAddr = "203.0.113.7".parse().unwrap();
        let anna = limit_keys(" Anna", here);
        assert_eq!(anna, ["name:anna".to_string(), "address:203.0.113.7".to_string()]);
        for _ in 0..4 {
            limit.fail(&anna, 1000);
        }
        assert_eq!(limit.left(&anna, 1000), 0, "four wrong are free");
        limit.fail(&anna, 1000);
        assert_eq!(limit.left(&anna, 1000), 30);
        assert_eq!(limit.left(&anna, 1029), 1);
        assert_eq!(limit.left(&anna, 1030), 0);

        assert_eq!(limit.left(&limit_keys("ANNA", "198.51.100.1".parse().unwrap()), 1010), 20);
        assert_eq!(limit.left(&limit_keys("Tom", here), 1010), 20);
        assert_eq!(limit.left(&limit_keys("Tom", "198.51.100.1".parse().unwrap()), 1010), 0);
        limit.fail(&anna, 1030);
        assert_eq!(limit.left(&anna, 1030), 60);
        limit.clear(&anna);
        assert_eq!(limit.left(&anna, 1030), 0, "a right password clears it");
        for _ in 0..9 {
            limit.fail(&anna, 2000);
        }
        assert_eq!(limit.left(&anna, 2000), 480);
        assert_eq!(limit.left(&anna, 2000 + FORGET), 0, "a day forgets");
        limit.fail(&anna, 2000 + FORGET);
        assert_eq!(limit.left(&anna, 2000 + FORGET), 0, "and counts from one again");

        let six = |text: &str| limit_keys("x", text.parse().unwrap())[1].clone();
        assert_eq!(six("2001:db8:1:2::5"), six("2001:db8:1:2:aaaa::9"));
        assert_ne!(six("2001:db8:1:2::5"), six("2001:db8:1:3::5"));
        assert_eq!(try_again(4), "Try again in 4 seconds.");

        assert_eq!(try_again(200), "Try again in 4 minutes.");
    }

    #[test]
    fn forwarded_only_from_a_named_proxy() {
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", "10.9.9.9, 203.0.113.7, 172.18.0.5".parse().unwrap());
        headers.insert("x-forwarded-proto", "https".parse().unwrap());
        let proxies = parse_proxies("172.18.0.0/16, 192.168.1.2, nonsense");
        assert_eq!(proxies.len(), 2);
        let ip = |text: &str| text.parse::<IpAddr>().unwrap();
        assert_eq!(client_among(ip("172.18.0.1"), &headers, &proxies), (ip("203.0.113.7"), true), "the last hop that is not a proxy");
        assert_eq!(client_among(ip("192.168.1.9"), &headers, &proxies), (ip("192.168.1.9"), false), "not a proxy: the socket");
        assert_eq!(client_among(ip("192.168.1.2"), &HeaderMap::new(), &proxies), (ip("192.168.1.2"), false));
        assert_eq!(client_among(ip("::ffff:192.168.1.2"), &headers, &proxies).0, ip("203.0.113.7"), "an IPv4 peer seen as IPv6");
        assert!(within(ip("10.1.2.3"), (ip("0.0.0.0"), 0)) && !within(ip("10.1.2.3"), (ip("10.1.2.4"), 32)));
        assert!(within(ip("fd00::1"), (ip("fd00::"), 8)) && !within(ip("fe00::1"), (ip("fd00::"), 8)));
    }
}
