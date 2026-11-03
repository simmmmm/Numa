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
        let mut account = presented(&parts.headers)
            .and_then(|token| server.store.session(&digest(&token)))
            .ok_or_else(|| Fail(StatusCode::UNAUTHORIZED, "Sign in first.".into()))?;
        if account.must_change && !parts.uri.path().starts_with("/api/v1/me") {

            return Err(Fail(StatusCode::FORBIDDEN, "Choose a new password on Numa Home's page in a browser first.".into()));
        }
        account.sight = Arc::new(crate::grants::sight(server, &account).await?);
        Ok(Signed(account))
    }
}

#[derive(Default)]
pub struct Limit(Mutex<HashMap<String, (u32, i64)>>);

const FREE: u32 = 5;
const FIRST_WAIT: i64 = 30;
const LONGEST_WAIT: i64 = 15 * 60;
const FORGET: i64 = 86_400;

const OUTSIDE: &str = "out-";
const OUTSIDE_FREE: u32 = 3;
const OUTSIDE_FIRST_WAIT: i64 = 60;
const OUTSIDE_LONGEST_WAIT: i64 = 60 * 60;

fn wait(key: &str, failures: u32) -> i64 {
    let (free, first, longest) = match key.starts_with(OUTSIDE) {
        true => (OUTSIDE_FREE, OUTSIDE_FIRST_WAIT, OUTSIDE_LONGEST_WAIT),
        false => (FREE, FIRST_WAIT, LONGEST_WAIT),
    };
    match failures.checked_sub(free) {
        None => 0,
        Some(past) => (first << past.min(6)).min(longest),
    }
}

impl Limit {
    fn map(&self) -> std::sync::MutexGuard<'_, HashMap<String, (u32, i64)>> {
        self.0.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    pub fn left(&self, keys: &[String], now: i64) -> i64 {
        let map = self.map();
        Self::longest(&map, keys, now)
    }

    fn longest(map: &HashMap<String, (u32, i64)>, keys: &[String], now: i64) -> i64 {
        keys.iter().filter_map(|key| map.get(key).map(|entry| (key, entry))).filter(|(_, (_, last))| now - last < FORGET).map(|(key, (failures, last))| last + wait(key, *failures) - now).max().unwrap_or(0).max(0)
    }

    pub fn reserve(&self, keys: &[String], now: i64) -> i64 {
        let mut map = self.map();
        let left = Self::longest(&map, keys, now);
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

pub fn limit_keys(name: &str, address: IpAddr, outside: bool) -> [String; 2] {
    let address = match address {
        IpAddr::V6(v6) => IpAddr::V6((u128::from(v6) & !(u64::MAX as u128)).into()),
        v4 => v4,
    };
    let from = if outside { OUTSIDE } else { "" };
    [format!("{from}name:{}", name.trim().to_lowercase()), format!("{from}address:{address}")]
}

pub fn same_bytes(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0u8, |diff, (x, y)| diff | (x ^ y)) == 0
}

pub fn local(address: IpAddr) -> bool {
    match address.to_canonical() {
        IpAddr::V4(v4) => v4.is_loopback() || v4.is_private() || v4.is_link_local() || (v4.octets()[0] == 100 && v4.octets()[1] & 0xc0 == 64),
        IpAddr::V6(v6) => v6.is_loopback() || (v6.segments()[0] & 0xfe00) == 0xfc00 || (v6.segments()[0] & 0xffc0) == 0xfe80,
    }
}

pub fn in_house(peer: SocketAddr, headers: &HeaderMap) -> bool {
    in_house_among(peer.ip().to_canonical(), headers, proxies())
}

pub fn proxy_named() -> bool {
    !proxies().is_empty()
}

pub fn is_named(address: &str) -> bool {
    address.parse::<IpAddr>().is_ok_and(|address| proxies().iter().any(|proxy| within(address, *proxy)))
}

pub fn unnamed_forwarder(peer: SocketAddr, headers: &HeaderMap) -> Option<IpAddr> {
    unnamed_among(peer.ip().to_canonical(), headers, proxies())
}

const PROXIED: [&str; 8] = ["x-forwarded-for", "forwarded", "x-real-ip", "x-forwarded-proto", "x-forwarded-host", "x-forwarded-port", "via", "cf-connecting-ip"];

fn proxied(headers: &HeaderMap) -> bool {
    PROXIED.iter().any(|name| headers.contains_key(*name))
}

fn unnamed_among(peer: IpAddr, headers: &HeaderMap, proxies: &[(IpAddr, u8)]) -> Option<IpAddr> {
    (proxied(headers) && !proxies.iter().any(|proxy| within(peer, *proxy))).then_some(peer)
}

fn in_house_among(peer: IpAddr, headers: &HeaderMap, proxies: &[(IpAddr, u8)]) -> bool {
    if !proxied(headers) {
        return local(peer);
    }
    let named = proxies.iter().any(|proxy| within(peer, *proxy));
    named && headers.contains_key("x-forwarded-for") && local(client_among(peer, headers, proxies).0)
}

pub fn setup_code() -> String {
    let (token, _) = token();
    token[..12].as_bytes().chunks(4).map(|part| String::from_utf8_lossy(part).into_owned()).collect::<Vec<_>>().join("-")
}

pub fn same_code(given: &str, code: &str) -> bool {
    let plain = |text: &str| text.chars().filter(char::is_ascii_alphanumeric).map(|c| c.to_ascii_lowercase()).collect::<String>();
    !code.is_empty() && same_bytes(plain(given).as_bytes(), plain(code).as_bytes())
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

const UNKNOWN: IpAddr = IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED);

fn client_among(peer: IpAddr, headers: &HeaderMap, proxies: &[(IpAddr, u8)]) -> (IpAddr, bool) {
    let trusted = |address: IpAddr| proxies.iter().any(|proxy| within(address, *proxy));
    let values = |name: &str| headers.get_all(name).iter().flat_map(|value| String::from_utf8_lossy(value.as_bytes()).split(',').map(|part| part.trim().to_string()).collect::<Vec<_>>()).collect::<Vec<_>>();
    if !trusted(peer) {
        static SAID: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
        if headers.contains_key("x-forwarded-for") && !SAID.swap(true, std::sync::atomic::Ordering::Relaxed) {
            log::warn!("a request came through a proxy at {peer}, which NUMA_HOME_PROXY does not name: its X-Forwarded-For is not believed");
        }
        return (peer, false);
    }

    let hops: Vec<Option<IpAddr>> = values("x-forwarded-for")
        .into_iter()
        .map(|hop| hop.parse::<IpAddr>().ok().or_else(|| hop.parse::<SocketAddr>().ok().map(|socket| socket.ip())).map(|hop| hop.to_canonical()))
        .collect();
    let address = match hops.iter().rev().find(|hop| !hop.is_some_and(trusted)) {
        Some(Some(hop)) => *hop,
        Some(None) => UNKNOWN,
        None => hops.first().copied().flatten().unwrap_or(peer),
    };
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
    fn a_forwarder_nobody_named_is_found() {
        let proxies = parse_proxies("172.18.0.1");
        let mut headers = HeaderMap::new();
        assert_eq!(unnamed_among("172.18.0.2".parse().unwrap(), &headers, &proxies), None, "no forwarding, no proxy");
        headers.insert("x-forwarded-for", "93.184.216.34".parse().unwrap());
        assert_eq!(unnamed_among("172.18.0.2".parse().unwrap(), &headers, &proxies), Some("172.18.0.2".parse().unwrap()));
        assert_eq!(unnamed_among("172.18.0.1".parse().unwrap(), &headers, &proxies), None, "named");
    }

    #[test]
    fn only_the_house_sets_up_without_the_code() {
        let ip = |text: &str| text.parse::<IpAddr>().unwrap();
        let forwarded = |from: &str| {
            let mut headers = HeaderMap::new();
            headers.insert("x-forwarded-for", from.parse().unwrap());
            headers
        };
        let proxies = [(ip("172.18.0.0"), 16)];
        assert!(in_house_among(ip("192.168.1.9"), &HeaderMap::new(), &[]), "the house, directly");
        assert!(in_house_among(ip("100.64.0.7"), &HeaderMap::new(), &[]), "its tunnel");
        assert!(!in_house_among(ip("203.0.113.7"), &HeaderMap::new(), &[]), "a stranger, directly");
        assert!(!in_house_among(ip("172.17.0.1"), &forwarded("203.0.113.7"), &[]), "a stranger through a proxy not named");
        assert!(!in_house_among(ip("172.17.0.1"), &forwarded("192.168.1.9"), &[]), "a proxy not named is not believed either way");
        assert!(!in_house_among(ip("172.18.0.1"), &forwarded("203.0.113.7"), &proxies), "a stranger through a named proxy");
        assert!(in_house_among(ip("172.18.0.1"), &forwarded("192.168.1.9"), &proxies), "the house through a named proxy");
    }

    #[test]
    fn a_hop_that_cannot_be_read_is_not_the_house() {
        let ip = |text: &str| text.parse::<IpAddr>().unwrap();
        let proxies = [(ip("127.0.0.1"), 32)];
        let said = |value: &[u8]| {
            let mut headers = HeaderMap::new();
            headers.insert("x-forwarded-for", axum::http::HeaderValue::from_bytes(value).unwrap());
            headers
        };
        let peer = ip("127.0.0.1");
        assert_eq!(client_among(peer, &said(b"\xff, 203.0.113.7"), &proxies).0, ip("203.0.113.7"), "the hop the proxy appended is read");
        assert!(!in_house_among(peer, &said(b"\xff, 203.0.113.7"), &proxies));
        assert!(!in_house_among(peer, &said(b"192.168.1.5, \xff"), &proxies), "an unreadable last hop is nobody's");
        assert!(!in_house_among(peer, &said(b"192.168.1.5, unix:"), &proxies));
        assert!(!in_house_among(peer, &said(b""), &proxies), "an empty line");
        assert!(!in_house_among(peer, &said(b"192.168.1.5, 203.0.113.7"), &proxies), "a house address the client made up");
        assert!(in_house_among(peer, &said(b"\xff, 192.168.1.5"), &proxies), "the house through the proxy still is");
        assert!(in_house_among(peer, &HeaderMap::new(), &proxies), "no forwarding at all: the proxy's own host");
    }

    #[test]
    fn a_proxied_request_without_an_address_is_not_the_house() {
        let ip = |text: &str| text.parse::<IpAddr>().unwrap();
        let proxies = [(ip("127.0.0.1"), 32), (ip("172.18.0.1"), 32)];
        let with = |name: &'static str, value: &str| {
            let mut headers = HeaderMap::new();
            headers.insert(name, value.parse().unwrap());
            headers
        };
        for name in PROXIED.iter().filter(|name| **name != "x-forwarded-for") {
            assert!(!in_house_among(ip("127.0.0.1"), &with(name, "203.0.113.7"), &proxies), "named, {name} alone");
            assert!(!in_house_among(ip("127.0.0.1"), &with(name, "https"), &[]), "unnamed, {name} alone");
            assert_eq!(unnamed_among(ip("127.0.0.1"), &with(name, "https"), &[]), Some(ip("127.0.0.1")), "found: {name}");
        }
        assert!(in_house_among(ip("172.18.0.1"), &HeaderMap::new(), &proxies), "the gateway's LAN traffic");
        assert!(in_house_among(ip("127.0.0.1"), &HeaderMap::new(), &[]), "this host");
        let mut both = with("x-real-ip", "203.0.113.7");
        both.insert("x-forwarded-for", "192.168.1.5".parse().unwrap());
        assert!(in_house_among(ip("127.0.0.1"), &both, &proxies), "a named proxy that names the house");
    }

    #[test]
    fn only_one_is_the_first_admin() {
        let store = Arc::new(crate::store::Store::open(std::path::Path::new(":memory:")).unwrap());
        let tries: Vec<_> = (0..16)
            .map(|at| {
                let store = store.clone();
                std::thread::spawn(move || store.add_account(&format!("name{at}"), "hash", true, true).unwrap())
            })
            .collect();
        let made = tries.into_iter().map(|one| one.join().unwrap()).filter(|made| *made).count();
        assert_eq!(made, 1);
        assert_eq!(store.accounts().unwrap().len(), 1);
    }

    #[test]
    fn sign_ins_at_once_are_counted_before_they_are_checked() {
        let limit = Limit::default();
        let tom = limit_keys("tom", "203.0.113.7".parse().unwrap(), false);
        assert_eq!((0..5).map(|_| limit.reserve(&tom, 3000)).collect::<Vec<_>>(), [0; 5]);
        assert_eq!(limit.reserve(&tom, 3000), 30);
        limit.clear(&tom);
        assert_eq!(limit.reserve(&tom, 3000), 0, "a right password clears it");
    }

    #[test]
    fn wrong_passwords_slow_down() {
        assert_eq!((0..=11).map(|n| wait("name:x", n)).collect::<Vec<_>>(), [0, 0, 0, 0, 0, 30, 60, 120, 240, 480, 900, 900]);
        let limit = Limit::default();
        let here: IpAddr = "203.0.113.7".parse().unwrap();
        let anna = limit_keys(" Anna", here, false);
        assert_eq!(anna, ["name:anna".to_string(), "address:203.0.113.7".to_string()]);
        for _ in 0..4 {
            limit.fail(&anna, 1000);
        }
        assert_eq!(limit.left(&anna, 1000), 0, "four wrong are free");
        limit.fail(&anna, 1000);
        assert_eq!(limit.left(&anna, 1000), 30);
        assert_eq!(limit.left(&anna, 1029), 1);
        assert_eq!(limit.left(&anna, 1030), 0);

        assert_eq!(limit.left(&limit_keys("ANNA", "198.51.100.1".parse().unwrap(), false), 1010), 20);
        assert_eq!(limit.left(&limit_keys("Tom", here, false), 1010), 20);
        assert_eq!(limit.left(&limit_keys("Tom", "198.51.100.1".parse().unwrap(), false), 1010), 0);
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

        let six = |text: &str| limit_keys("x", text.parse().unwrap(), false)[1].clone();
        assert_eq!(six("2001:db8:1:2::5"), six("2001:db8:1:2:aaaa::9"));
        assert_ne!(six("2001:db8:1:2::5"), six("2001:db8:1:3::5"));
        assert_eq!(try_again(4), "Try again in 4 seconds.");

        assert_eq!(try_again(200), "Try again in 4 minutes.");
    }

    #[test]
    fn outside_the_lock_is_harder_and_apart() {
        assert_eq!((0..=11).map(|n| wait("out-name:x", n)).collect::<Vec<_>>(), [0, 0, 0, 60, 120, 240, 480, 960, 1920, 3600, 3600, 3600]);
        let here: IpAddr = "203.0.113.7".parse().unwrap();
        let (outer, inner) = (limit_keys("Sam", here, true), limit_keys("Sam", here, false));
        assert_eq!(outer, ["out-name:sam".to_string(), "out-address:203.0.113.7".to_string()]);
        let limit = Limit::default();
        assert_eq!((0..3).map(|_| limit.reserve(&outer, 100)).collect::<Vec<_>>(), [0; 3]);
        assert_eq!(limit.reserve(&outer, 100), 60, "the fourth waits a minute");
        assert_eq!(limit.left(&limit_keys("SAM", "198.51.100.9".parse().unwrap(), true), 100), 60, "the name waits from any address");
        assert_eq!(limit.left(&inner, 100), 0, "inside is not locked by outside");
        assert_eq!(limit.reserve(&outer, 160), 0, "the lock ends");
        assert_eq!(limit.reserve(&outer, 160), 120, "and the next is longer");
        limit.clear(&outer);
        assert_eq!(limit.left(&outer, 160), 0);
    }

    #[test]
    fn same_bytes_tells_equal_from_different() {
        assert!(same_bytes(b"abc", b"abc") && same_bytes(b"", b""));
        assert!(!same_bytes(b"abc", b"abd") && !same_bytes(b"abc", b"ab") && !same_bytes(b"", b"a"));
    }

    #[test]
    fn local_addresses_and_the_setup_code() {
        for near in ["127.0.0.1", "192.168.1.20", "10.0.0.5", "172.18.0.1", "100.101.102.103", "::1", "fd12::5", "fe80::1", "::ffff:192.168.1.2"] {
            assert!(local(near.parse().unwrap()), "{near}");
        }
        for far in ["203.0.113.7", "8.8.8.8", "100.128.0.1", "2001:db8::1"] {
            assert!(!local(far.parse().unwrap()), "{far}");
        }
        let code = setup_code();
        assert_eq!(code.len(), 14);
        assert!(same_code(&code.to_uppercase().replace('-', " "), &code));
        assert!(!same_code("", &code) && !same_code("0000-0000-0000", &code));
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
