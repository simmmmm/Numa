use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;

use axum::extract::{ConnectInfo, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::auth::{self, Signed};
use crate::store::{now, Account, Store};
use crate::{Fail, Server};

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    Off,
    Links,
    All,
}

#[derive(PartialEq, Eq, Debug)]
pub enum Verdict {
    Allow,
    NeedHttps,
    Closed,
}

fn mode(store: &Store) -> Mode {
    match store.setting("reach:mode").as_deref() {
        Some("links") => Mode::Links,
        Some("all") => Mode::All,
        _ => Mode::Off,
    }
}

fn word(mode: Mode) -> &'static str {
    match mode {
        Mode::Off => "off",
        Mode::Links => "links",
        Mode::All => "all",
    }
}

fn address(store: &Store) -> Option<String> {
    store.setting("reach:address").filter(|text| !text.is_empty())
}

pub fn base(store: &Store) -> Option<String> {
    address(store).filter(|_| mode(store) != Mode::Off)
}

const PING: &str = "/api/v1/reach/ping";

fn link_path(path: &str) -> bool {
    path.starts_with("/s/")
        || path.starts_with("/api/v1/s/")
        || path.starts_with("/c/")
        || path.starts_with("/api/v1/c/")
        || path.starts_with("/t/")
        || path.starts_with("/api/v1/t/")
        || path.starts_with("/map/fonts/")
        || matches!(
            path,
            PING | "/proof.js" | "/proof.css" | "/share.js" | "/collect.js" | "/collect.css" | "/trip.js" | "/trip.css" | "/mapstyle.js" | "/strings.js" | "/tokens.css" | "/icon.svg" | "/robots.txt"
                | "/map/6.13.0/maplibre-gl.mjs" | "/map/6.13.0/maplibre-gl-worker.mjs" | "/map/6.13.0/maplibre-gl.css"
        )
}

fn inside_only(path: &str) -> bool {
    path == "/api/v1/setup"
}

pub fn verdict(mode: Mode, outside: bool, https: bool, set_up: bool, path: &str) -> Verdict {
    if !outside || !set_up {
        return Verdict::Allow;
    }
    let open = match mode {
        Mode::Off => false,
        Mode::Links => link_path(path),
        Mode::All => !inside_only(path),
    };
    match (open, https) {
        (false, _) => Verdict::Closed,
        (true, false) => Verdict::NeedHttps,
        (true, true) => Verdict::Allow,
    }
}

pub fn judge(mode: Mode, peer: SocketAddr, headers: &HeaderMap, set_up: bool, path: &str) -> Verdict {
    let outside = !auth::in_house(peer, headers);
    verdict(mode, outside, auth::client(peer, headers).1, set_up, path)
}

pub fn gate(server: &Server, peer: SocketAddr, headers: &HeaderMap, path: &str) -> Option<Response> {
    let answer = match judge(mode(&server.store), peer, headers, server.store.has_accounts(), path) {
        Verdict::Allow => return None,
        Verdict::NeedHttps => Fail(StatusCode::FORBIDDEN, "Numa Home is opened to the outside over https only.".into()),
        Verdict::Closed => Fail(StatusCode::NOT_FOUND, "There is nothing at this address.".into()),
    };
    if let Some(proxy) = proxy_to_name(peer, headers) {
        let proxy = proxy.to_string();
        if server.store.setting("reach:unnamed").as_deref() != Some(proxy.as_str()) {
            let _ = server.store.set_setting("reach:unnamed", &proxy);
        }
    }
    Some(answer.into_response())
}

fn proxy_to_name(peer: SocketAddr, headers: &HeaderMap) -> Option<std::net::IpAddr> {
    auth::unnamed_forwarder(peer, headers).filter(|proxy| auth::local(*proxy))
}

pub fn clean_address(text: &str) -> Result<String, &'static str> {
    const WRONG: &str = "Give the address as https://name, such as https://photos.example.com.";
    let text = text.trim().trim_end_matches('/').to_ascii_lowercase();
    let authority = text.strip_prefix("https://").ok_or(WRONG)?;
    let (host, port) = authority.split_once(':').map_or((authority, None), |(host, port)| (host, Some(port)));
    let name = !host.is_empty()
        && host.len() <= 253
        && host.split('.').all(|label| !label.is_empty() && label.len() <= 63 && !label.starts_with('-') && !label.ends_with('-') && label.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-'));
    if !name || port.is_some_and(|port| port.is_empty() || port.len() > 5 || !port.bytes().all(|b| b.is_ascii_digit()) || port.parse::<u32>().map_or(true, |n| n == 0 || n > 65535)) {
        return Err(WRONG);
    }
    if host.parse::<IpAddr>().is_ok_and(auth::local) || !host.contains('.') {
        return Err("That is an address inside the house. Give the name the outside uses.");
    }
    Ok(text)
}

fn authority(headers: &HeaderMap, name: &str) -> Option<String> {
    headers.get(name).and_then(|value| value.to_str().ok()).map(str::to_ascii_lowercase)
}

pub async fn ping(State(server): State<Arc<Server>>, ConnectInfo(peer): ConnectInfo<SocketAddr>, headers: HeaderMap) -> Json<Value> {
    let outside = !auth::in_house(peer, &headers);
    let https = auth::client(peer, &headers).1;
    if outside && https {
        let asked = [authority(&headers, "host"), authority(&headers, "x-forwarded-host")];
        let saved = address(&server.store).and_then(|text| text.strip_prefix("https://").map(str::to_string));
        let last = server.store.setting("reach:seen").and_then(|text| text.parse::<i64>().ok()).unwrap_or(0);
        if saved.is_some_and(|saved| asked.iter().flatten().any(|asked| *asked == saved)) && now() - last >= 60 {
            let _ = server.store.set_setting("reach:seen", &now().to_string());
        }
    }
    Json(json!({ "outside": outside, "https": https }))
}

pub async fn show(State(server): State<Arc<Server>>, Signed(me): Signed, ConnectInfo(peer): ConnectInfo<SocketAddr>, headers: HeaderMap) -> Result<Json<Value>, Fail> {
    admin(&me)?;
    Ok(Json(json!({
        "address": address(&server.store),
        "mode": word(mode(&server.store)),
        "proxy": auth::proxy_named(),
        "unnamed": server.store.setting("reach:unnamed").filter(|proxy| !auth::is_named(proxy)),
        "seen": server.store.setting("reach:seen").and_then(|text| text.parse::<i64>().ok()),
        "here": { "outside": !auth::in_house(peer, &headers), "https": auth::client(peer, &headers).1 },
    })))
}

fn admin(me: &Account) -> Result<(), Fail> {
    match me.admin {
        true => Ok(()),
        false => Err(Fail(StatusCode::FORBIDDEN, "Only an admin opens Numa Home to the outside.".into())),
    }
}

pub async fn sign_ins(State(server): State<Arc<Server>>, Signed(me): Signed) -> Result<Json<Value>, Fail> {
    admin(&me)?;
    let rows = server.store.sign_ins(100);
    Ok(Json(rows.into_iter().map(|(at, name, address, outside, result)| json!({ "at": at, "name": name, "address": address, "outside": outside, "result": result })).collect()))
}

#[derive(Deserialize)]
pub struct Change {
    address: Option<String>,
    mode: String,
    #[serde(default)]
    confirm: bool,
}

pub async fn change(State(server): State<Arc<Server>>, Signed(me): Signed, Json(new): Json<Change>) -> Result<StatusCode, Fail> {
    apply(&server.store, &me, new)
}

fn apply(store: &Store, me: &Account, new: Change) -> Result<StatusCode, Fail> {
    admin(me)?;
    let address = match new.address.as_deref().map(str::trim).filter(|text| !text.is_empty()) {
        Some(text) => clean_address(text).map_err(Fail::bad)?,
        None => String::new(),
    };
    let mode = match new.mode.as_str() {
        "off" | "links" | "all" => new.mode.as_str(),
        _ => return Err(Fail::bad("The way is off, links or all.")),
    };
    if mode != "off" && address.is_empty() {
        return Err(Fail::bad("Save the address of Numa Home first."));
    }
    if store.setting("reach:address").unwrap_or_default() != address {
        store.set_setting("reach:seen", "").map_err(Fail::internal)?;
    }
    if mode == "all" {
        if self::mode(store) != Mode::All && !new.confirm {
            return Err(Fail::bad("Confirm that you understand what opening all of Numa Home means."));
        }
        if store.setting("reach:seen").unwrap_or_default().is_empty() {
            return Err(Fail::bad("Check the address from outside over https first: open the share links, then the check on this page from mobile data."));
        }
    }
    store.set_setting("reach:address", &address).map_err(Fail::internal)?;
    store.set_setting("reach:mode", mode).map_err(Fail::internal)?;
    log::warn!("{} set the way in from outside to {mode}", me.name);
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;

    const EVERY: [&str; 22] = [
        "/", "/app.js", "/api/v1/me", "/api/v1/login", "/api/v1/photos/1/original", "/api/v1/send/Sam", "/api/v1/shares", "/api/v1/reach", "/s", "/api/v1/status",
        "/sx/abc", "/api/v1/setup",

        "/api/v1/map/3/1/2", "/api/v1/albums/x/links", "/api/v1/albums/x/page", "/api/v1/chapters/x", "/map/", "/map/6.13.0/other.js", "/map/fonts", "/map.js", "/t", "/api/v1/t",
    ];
    const LINKS: [&str; 22] = [
        "/s/abc", "/api/v1/s/abc/3", "/api/v1/s/abc/3/pick", "/proof.js", "/api/v1/reach/ping", "/c/abc", "/api/v1/c/abc", "/api/v1/c/abc/files/a.jpg", "/collect.js",
        "/collect.css",
        "/t/abc", "/api/v1/t/abc", "/api/v1/t/abc/3", "/api/v1/t/abc/map/3/1/2", "/trip.js", "/trip.css", "/mapstyle.js", "/map/6.13.0/maplibre-gl.mjs",
        "/map/6.13.0/maplibre-gl-worker.mjs", "/map/6.13.0/maplibre-gl.css", "/map/fonts/Noto%20Sans/0-255.pbf", "/map/fonts/x/y",
    ];

    #[test]
    fn closed_until_opened() {
        for path in EVERY.iter().chain(&LINKS) {
            assert_eq!(verdict(Mode::Off, true, true, true, path), Verdict::Closed, "{path}");
            assert_eq!(verdict(Mode::Off, true, false, true, path), Verdict::Closed, "{path}");
        }
    }

    #[test]
    fn share_links_only_over_https() {
        for path in LINKS {
            assert_eq!(verdict(Mode::Links, true, true, true, path), Verdict::Allow, "{path}");
            assert_eq!(verdict(Mode::Links, true, false, true, path), Verdict::NeedHttps, "plain http is refused: {path}");
        }
        for path in EVERY {
            assert_eq!(verdict(Mode::Links, true, true, true, path), Verdict::Closed, "{path}");
            assert_eq!(verdict(Mode::Links, true, false, true, path), Verdict::Closed, "{path}");
        }
    }

    #[test]
    fn a_guest_uploads_from_outside_in_share_links_mode() {
        let up = "/api/v1/c/abc/files/party.jpg";
        assert_eq!(verdict(Mode::Links, true, true, true, up), Verdict::Allow);
        assert_eq!(verdict(Mode::Links, true, false, true, up), Verdict::NeedHttps);
        assert_eq!(verdict(Mode::Off, true, true, true, up), Verdict::Closed);
        assert_eq!(verdict(Mode::Off, true, false, true, up), Verdict::Closed);
        for owner in ["/api/v1/collects", "/api/v1/collects/3/files", "/api/v1/collects/3/accept", "/api/v1/collects/3/reject", "/api/v1/albums/x/people"] {
            assert_eq!(verdict(Mode::Links, true, true, true, owner), Verdict::Closed, "{owner}");
            assert_eq!(verdict(Mode::All, true, true, true, owner), Verdict::Allow, "{owner}");
        }
    }

    #[test]
    fn all_of_home_over_https_but_not_the_setup() {
        for path in EVERY.iter().chain(&LINKS).filter(|path| **path != "/api/v1/setup") {
            assert_eq!(verdict(Mode::All, true, true, true, path), Verdict::Allow, "{path}");
            assert_eq!(verdict(Mode::All, true, false, true, path), Verdict::NeedHttps, "plain http is refused: {path}");
        }
        assert_eq!(verdict(Mode::All, true, true, true, "/api/v1/setup"), Verdict::Closed, "setting up stays inside");
        assert_eq!(verdict(Mode::All, true, false, true, "/api/v1/setup"), Verdict::Closed);
    }

    #[test]
    fn inside_the_house_and_before_setup_nothing_changes() {
        for mode in [Mode::Off, Mode::Links, Mode::All] {
            for path in EVERY.iter().chain(&LINKS) {
                assert_eq!(verdict(mode, false, false, true, path), Verdict::Allow, "inside: {path}");
                assert_eq!(verdict(mode, true, false, false, path), Verdict::Allow, "no account yet: {path}");
            }
        }
    }

    #[test]
    fn who_is_outside() {
        let headers = |pairs: &[(&'static str, &str)]| {
            let mut map = HeaderMap::new();
            pairs.iter().for_each(|(name, value)| drop(map.insert(*name, value.parse().unwrap())));
            map
        };
        let at = |text: &str| -> SocketAddr { text.parse().unwrap() };
        let none = headers(&[]);
        let unnamed = headers(&[("x-forwarded-for", "93.184.216.34"), ("x-forwarded-proto", "https")]);
        for mode in [Mode::Off, Mode::Links, Mode::All] {
            assert_eq!(judge(mode, at("192.168.1.5:5000"), &none, true, "/api/v1/me"), Verdict::Allow, "the house, {mode:?}");
            assert_eq!(judge(mode, at("[::1]:5000"), &none, true, "/api/v1/me"), Verdict::Allow);

            assert_ne!(judge(mode, at("93.184.216.34:5000"), &none, true, "/api/v1/me"), Verdict::Allow, "{mode:?}");
            assert_ne!(judge(mode, at("172.18.0.2:5000"), &unnamed, true, "/api/v1/me"), Verdict::Allow, "unnamed proxy, {mode:?}");
        }
        assert_eq!(judge(Mode::All, at("172.18.0.2:5000"), &unnamed, true, "/api/v1/me"), Verdict::NeedHttps, "an unnamed proxy's https is not believed");
        assert_eq!(judge(Mode::Off, at("172.18.0.2:5000"), &unnamed, true, "/api/v1/me"), Verdict::Closed);
    }

    #[test]
    fn only_a_proxy_of_the_house_is_offered_to_be_named() {
        let at = |text: &str| -> SocketAddr { text.parse().unwrap() };
        let mut headers = HeaderMap::new();
        headers.insert("x-forwarded-for", "192.168.1.5".parse().unwrap());
        assert_eq!(proxy_to_name(at("203.0.113.7:5000"), &headers), None, "a stranger straight in");
        assert_eq!(proxy_to_name(at("[2001:db8::1]:5000"), &headers), None);
        assert_eq!(proxy_to_name(at("172.18.0.2:5000"), &headers), Some("172.18.0.2".parse().unwrap()));
        assert_eq!(proxy_to_name(at("172.18.0.2:5000"), &HeaderMap::new()), None, "no proxy sign");
    }

    #[test]
    fn an_address_is_a_name_over_https() {
        assert_eq!(clean_address(" https://Photos.Example.com/ "), Ok("https://photos.example.com".into()));
        assert_eq!(clean_address("https://photos.example.com:8443"), Ok("https://photos.example.com:8443".into()));
        for bad in [
            "", "photos.example.com", "http://photos.example.com", "https://", "https://a b.com", "https://user@photos.example.com",
            "https://photos.example.com/path", "https://photos.example.com?x=1", "https://photos.example.com:0", "https://photos.example.com:99999",
            "https://photos.example.com:", "https://-a.example.com", "https://a..example.com", "https://192.168.1.20", "https://127.0.0.1", "https://100.64.0.5",
            "https://nas", "https://[::1]", "javascript://x.com",
        ] {
            assert!(clean_address(bad).is_err(), "{bad:?}");
        }
        assert_eq!(clean_address("https://93.184.216.34"), Ok("https://93.184.216.34".into()));
    }

    #[test]
    fn only_an_admin_changes_it_and_links_need_an_address() {
        let store = Store::open(std::path::Path::new(":memory:")).unwrap();
        let admin = Account { name: "Sam".into(), admin: true, ..Account::default() };
        let run = |who: &Account, address: Option<&str>, mode: &str| apply(&store, who, Change { address: address.map(str::to_string), mode: mode.into(), confirm: false });
        assert_eq!(mode(&store), Mode::Off, "off by default");
        assert_eq!(base(&store), None);
        assert!(run(&admin, None, "links").is_err(), "no address, no links");
        assert!(run(&admin, Some("https://nas"), "links").is_err());
        assert!(run(&admin, Some("https://photos.example.com"), "everything").is_err());
        assert_eq!(mode(&store), Mode::Off, "a refused change changed nothing");
        run(&admin, Some("https://photos.example.com"), "links").unwrap();
        assert_eq!(base(&store).as_deref(), Some("https://photos.example.com"));
        let guest = Account { admin: false, ..admin.clone() };
        assert!(run(&guest, Some("https://other.example.com"), "off").is_err());
        assert_eq!(base(&store).as_deref(), Some("https://photos.example.com"), "a guest changed nothing");
        store.set_setting("reach:seen", "123").unwrap();
        run(&admin, Some("https://photos.example.org"), "links").unwrap();
        assert_eq!(store.setting("reach:seen").as_deref(), Some(""), "a new address has not been reached yet");
        run(&admin, None, "off").unwrap();
        assert_eq!(base(&store), None, "closed at once");
        assert_eq!(mode(&store), Mode::Off);
    }

    #[test]
    fn all_of_home_needs_a_confirmation_a_check_and_an_admin() {
        let store = Store::open(std::path::Path::new(":memory:")).unwrap();
        let admin = Account { name: "Sam".into(), admin: true, ..Account::default() };
        let guest = Account { admin: false, ..admin.clone() };
        let run = |who: &Account, mode: &str, confirm: bool| apply(&store, who, Change { address: Some("https://photos.example.com".into()), mode: mode.into(), confirm });
        run(&admin, "links", false).unwrap();
        assert!(run(&admin, "all", true).is_err(), "no request has come from outside over https yet");
        store.set_setting("reach:seen", "123").unwrap();
        assert!(run(&admin, "all", false).is_err(), "not without confirming");
        assert_eq!(mode(&store), Mode::Links);
        assert!(run(&guest, "all", true).is_err(), "not a guest");
        assert_eq!(mode(&store), Mode::Links);
        assert!(apply(&store, &admin, Change { address: None, mode: "all".into(), confirm: true }).is_err(), "not without an address");
        run(&admin, "all", true).unwrap();
        assert_eq!(mode(&store), Mode::All);
        assert_eq!(base(&store).as_deref(), Some("https://photos.example.com"), "links start with the address in all, too");
        run(&admin, "all", false).unwrap();
        let moved = Change { address: Some("https://other.example.com".into()), mode: "all".into(), confirm: true };
        assert!(apply(&store, &admin, moved).is_err(), "a new address is checked again");
        assert_eq!(mode(&store), Mode::All, "a refused change changed nothing");
        assert!(run(&guest, "off", false).is_err(), "a guest cannot close it either");
        run(&admin, "off", false).unwrap();
        assert_eq!(mode(&store), Mode::Off, "one step back");
        assert_eq!(word(Mode::All), "all");
    }
}
