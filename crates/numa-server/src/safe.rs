use std::collections::BTreeMap;
use std::ffi::CString;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::MetadataExt;
use std::path::Path;
use std::sync::Arc;

use axum::extract::State;
use axum::http::HeaderMap;
use axum::Json;
use serde_json::{json, Value};

use crate::auth::{self, Signed};
use crate::store::{now, Account};
use crate::Server;

const DAY: i64 = 86_400;

const LATE: i64 = 3 * DAY;

const GONE: i64 = 30 * DAY;

const NEARLY_FULL: f64 = 0.1;

pub async fn show(State(server): State<Arc<Server>>, Signed(me): Signed, headers: HeaderMap) -> Json<Value> {
    let now = now();
    let asking = auth::presented(&headers).and_then(|token| server.store.session_device(&auth::digest(&token))).unwrap_or_default();
    let mut devices: Vec<Value> = Vec::new();
    let mut attention: Vec<Value> = Vec::new();
    for (account, device, sent) in server.store.devices() {
        let mine = account.eq_ignore_ascii_case(&me.name);
        if !me.admin && !mine {
            continue;
        }
        let label = label(&account, &device);
        let late = late(now - sent);
        if late {
            attention.push(json!({ "text": format!("{label} has not come home for {} days", (now - sent) / DAY), "link": "#/safe" }));
        }
        let this = mine && device == asking;
        devices.push(json!({ "label": label, "account": account, "kind": "app", "sent": sent, "count": null, "late": late, "this": this }));
    }
    for (camera, value) in server.store.settings_under("card:") {
        let mut parts = value.split(' ').map(|part| part.parse::<i64>().unwrap_or(0));
        let (sent, count) = (parts.next().unwrap_or(0), parts.next().unwrap_or(0));
        devices.push(json!({ "label": camera, "account": null, "kind": "card", "sent": sent, "count": count, "late": false, "this": false }));
    }
    devices.sort_by_key(|device| std::cmp::Reverse(device["sent"].as_i64().unwrap_or(0)));

    let disks = disks(&[("Photos", &server.home.photos), ("Sent Home", &server.home.sent), ("Numa Home", &server.data)]);
    for (names, total, free) in &disks {
        if *total > 0 && (*free as f64) < NEARLY_FULL * *total as f64 {
            attention.push(json!({ "text": format!("Only {} left on the disk of {}", size(*free), names.join(" and ")), "link": "#/safe" }));
        }
    }

    attention.extend(crate::copy::attention(&server, now).into_iter().map(|text| json!({ "text": text, "link": "#/safe" })));
    let accounts = server.store.accounts().unwrap_or_default();
    Json(json!({
        "version": env!("CARGO_PKG_VERSION"),
        "role": role(&me, &accounts),
        "devices": devices,
        "disks": disks.iter().map(|(names, total, free)| json!({ "names": names, "total": total, "free": free })).collect::<Vec<_>>(),
        "attention": attention,
        "copy": crate::copy::status(&server),
    }))
}

pub fn label(account: &str, device: &str) -> String {
    match device.trim() {
        "" => format!("{account}'s device"),
        device if device.to_lowercase().contains(&account.to_lowercase()) => device.to_string(),
        device => format!("{account}'s {device}"),
    }
}

fn role(me: &Account, accounts: &[Account]) -> String {
    let others = |may: fn(&Account) -> bool| {
        let names: Vec<&str> = accounts.iter().filter(|one| may(one) && !one.name.eq_ignore_ascii_case(&me.name)).map(|one| one.name.as_str()).collect();
        match names.split_last() {
            Some((last, [])) => last.to_string(),
            Some((last, rest)) => format!("{} and {last}", rest.join(", ")),
            None => "nobody else".into(),
        }
    };
    match (me.admin, me.manage) {
        (true, _) => "You can do everything here, and make accounts.".into(),
        (false, true) => format!("You can add, rate, edit, move and delete photographs. Making accounts: {}.", others(|one| one.admin)),
        (false, false) => format!("You can add, rate and edit photographs. Moving and deleting: {}.", others(|one| one.admin || one.manage)),
    }
}

fn late(quiet: i64) -> bool {
    quiet > LATE && quiet < GONE
}

pub fn size(bytes: u64) -> String {
    let gigabytes = bytes as f64 / 1e9;
    match gigabytes >= 1000.0 {
        true => format!("{:.1} TB", gigabytes / 1000.0),
        false => format!("{} GB", gigabytes.round() as u64),
    }
}

fn disks(folders: &[(&str, &Path)]) -> Vec<(Vec<String>, u64, u64)> {
    let mut found: BTreeMap<u64, (Vec<String>, u64, u64)> = BTreeMap::new();
    for (name, path) in folders {
        let (Ok(meta), Some((total, free))) = (std::fs::metadata(path), space(path)) else { continue };
        found.entry(meta.dev()).or_insert_with(|| (Vec::new(), total, free)).0.push(name.to_string());
    }
    found.into_values().collect()
}

pub fn space(path: &Path) -> Option<(u64, u64)> {
    let path = CString::new(path.as_os_str().as_bytes()).ok()?;

    let mut stat: libc::statvfs = unsafe { std::mem::zeroed() };
    if unsafe { libc::statvfs(path.as_ptr(), &mut stat) } != 0 {
        return None;
    }
    let block = stat.f_frsize as u64;
    Some((stat.f_blocks as u64 * block, stat.f_bavail as u64 * block))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_read_the_same_everywhere() {
        assert_eq!(label("Tom", "iPhone"), "Tom's iPhone");
        assert_eq!(label("Anna", "Anna's iPad"), "Anna's iPad");
        assert_eq!(label("Anna", ""), "Anna's device");
        assert!(!late(DAY) && late(4 * DAY) && !late(40 * DAY));
        assert_eq!(size(120_400_000_000), "120 GB");
        assert_eq!(size(3_800_000_000_000), "3.8 TB");
        let here = disks(&[("Photos", Path::new("/")), ("Sent Home", Path::new("/"))]);
        assert_eq!(here.len(), 1);
        assert_eq!(here[0].0, ["Photos", "Sent Home"]);
        let one = |name: &str, admin, manage| Account { name: name.into(), admin, manage, must_change: false };
        let house = [one("Anna", true, false), one("Tom", false, true), one("Ella", false, false)];
        assert_eq!(role(&house[2], &house), "You can add, rate and edit photographs. Moving and deleting: Anna and Tom.");
        assert_eq!(role(&house[1], &house), "You can add, rate, edit, move and delete photographs. Making accounts: Anna.");
        assert_eq!(role(&house[0], &house), "You can do everything here, and make accounts.");
    }
}
