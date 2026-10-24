use super::{answer, base, curl, escaped, id_text, parsed, words, Failure};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::ffi::OsString;
use std::path::{Component, Path, PathBuf};

pub const LINK: &str = "numa-home";

pub const TRASHED: &str = "numa-home-trashed";

pub const RULES: u32 = 1;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Way {

    Down,

    Up,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Link {
    pub address: String,

    pub library: String,
    pub name: String,

    pub since: i64,

    #[serde(default)]
    pub everything: bool,

    #[serde(default)]
    pub keep_here: bool,

    pub photos: BTreeMap<String, Kept>,

    #[serde(default)]
    pub way: Option<Way>,

    #[serde(default)]
    pub gone: BTreeMap<String, Gone>,

    #[serde(default)]
    pub rules: u32,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Gone {

    pub id: String,

    pub here: bool,

    #[serde(default)]
    pub restored: bool,
}

pub fn way(link: &Link, in_home_folder: bool) -> Way {
    link.way.unwrap_or(if in_home_folder { Way::Down } else { Way::Up })
}

pub fn synced(link: &Link, way: Way) -> bool {
    link.keep_here || way == Way::Up
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Kept {
    pub id: String,

    pub edits: String,
    pub rating: u8,
    pub flag: String,

    pub updated: i64,

    #[serde(default)]
    pub canon: Option<String>,

    #[serde(default)]
    pub albums: Option<Vec<String>>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Local {
    pub path: String,
    pub edits: Option<String>,
    pub rating: u8,
    pub flag: String,

    pub albums: Vec<String>,

    pub changed: i64,
}

impl Local {

    fn edits_agree(&self, kept: &Kept) -> bool {
        digest(self.edits.as_deref()) == kept.edits
            || self
                .edits
                .as_deref()
                .and_then(|edits| serde_json::to_string(&serde_json::from_str::<numa_core::document::Document>(edits).ok()?).ok())
                .is_some_and(|again| digest(Some(&again)) == kept.edits)
    }

    fn marks_agree(&self, kept: &Kept) -> bool {
        self.rating == kept.rating && self.flag == kept.flag
    }

    fn untouched(&self) -> bool {
        self.edits.is_none() && self.rating == 0 && self.flag == "none"
    }

    pub fn kept(&self, id: &str, updated: i64) -> Kept {
        Kept {
            id: id.to_string(),
            edits: digest(self.edits.as_deref()),
            rating: self.rating,
            flag: self.flag.clone(),
            updated,
            canon: Some(canon(self.edits.as_deref())),
            albums: None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Remote {
    #[serde(default, deserialize_with = "id_text")]
    pub id: String,
    pub path: String,
    #[serde(default)]
    pub size: u64,
    #[serde(default)]
    pub mtime: i64,
    #[serde(default)]
    pub taken: Option<i64>,
    #[serde(default)]
    pub rating: u8,
    #[serde(default = "no_flag")]
    pub flag: String,

    #[serde(default)]
    pub edited: bool,
    #[serde(default)]
    pub updated: i64,

    #[serde(default)]
    pub albums: Option<Vec<String>>,

    #[serde(default)]
    pub deleted: Option<i64>,
}

impl Remote {

    pub fn at_home(&self) -> crate::catalog::AtHome {
        crate::catalog::AtHome {
            path: self.path.clone(),
            id: self.id.clone(),
            size: self.size,
            mtime: self.mtime,
            taken: self.taken,
            rating: self.rating,
            flag: flag(&self.flag),
        }
    }

    pub fn kept(&self) -> Kept {
        let flag = flag_name(flag(&self.flag)).to_string();
        Kept {
            id: self.id.clone(),
            edits: digest(None),
            rating: self.rating.min(5),
            flag,
            updated: self.updated,
            canon: (!self.edited).then(String::new),
            albums: self.albums.clone(),
        }
    }
}

fn no_flag() -> String {
    "none".to_string()
}

pub fn digest(edits: Option<&str>) -> String {
    edits.map_or_else(String::new, |edits| crate::offload::hex(twox_hash::XxHash3_128::oneshot(edits.as_bytes())))
}

pub fn canon(edits: Option<&str>) -> String {
    let Some(edits) = edits else { return String::new() };
    match serde_json::from_str::<numa_core::document::Document>(edits) {
        Ok(mut document) => {
            document.source.path.clear();
            document.moment = None;
            match document.is_untouched() {
                true => String::new(),
                false => digest(serde_json::to_string(&document).ok().as_deref()),
            }
        }
        Err(_) => digest(Some(edits)),
    }
}

pub fn flag_name(flag: crate::catalog::Flag) -> &'static str {
    match flag {
        crate::catalog::Flag::Picked => "pick",
        crate::catalog::Flag::Rejected => "reject",
        crate::catalog::Flag::None => "none",
    }
}

pub fn flag(name: &str) -> crate::catalog::Flag {
    match name {
        "pick" => crate::catalog::Flag::Picked,
        "reject" => crate::catalog::Flag::Rejected,
        _ => crate::catalog::Flag::None,
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Parts {
    pub edits: bool,
    pub marks: bool,

    pub albums: bool,
}

impl Parts {
    pub const ALL: Parts = Parts { edits: true, marks: true, albums: false };

    pub fn any(self) -> bool {
        self.edits || self.marks || self.albums
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Step {

    Arrive(Remote),

    Both { path: String, id: String, theirs: Option<Remote>, down: Parts, up: Parts, albums: Option<Vec<String>> },

    Send(String),

    Trash { path: String, id: String },

    Delete { path: String, id: String },

    PutBack(Remote),

    Restore(String),
}

pub fn albums(agreed: Option<&[String]>, ours: &[String], theirs: &[String]) -> Vec<String> {
    let key = |name: &String| name.to_lowercase();
    let set = |names: &[String]| names.iter().map(key).collect::<HashSet<String>>();
    let (o, t) = (set(ours), set(theirs));
    let agreed = agreed.map(set);
    let mut merged: BTreeMap<String, String> = BTreeMap::new();
    for name in ours.iter().chain(theirs) {
        let at = key(name);
        let keep = match &agreed {
            None => true,
            Some(agreed) => (o.contains(&at) && t.contains(&at)) || !agreed.contains(&at),
        };
        if keep {
            merged.entry(at).or_insert_with(|| name.clone());
        }
    }
    merged.into_values().collect()
}

pub fn same_albums(a: &[String], b: &[String]) -> bool {
    let set = |names: &[String]| names.iter().map(|name| name.to_lowercase()).collect::<HashSet<String>>();
    set(a) == set(b)
}

pub fn plan(link: &Link, remote: &[Remote], local: &[Local], trashed: &BTreeMap<String, i64>) -> Vec<Step> {
    let here: HashMap<&str, &Local> = local.iter().map(|photo| (photo.path.as_str(), photo)).collect();

    let paired: HashMap<&str, &str> = link.photos.iter().map(|(path, kept)| (kept.id.as_str(), path.as_str())).collect();
    let mut seen: HashSet<&str> = HashSet::new();
    let mut steps = Vec::new();
    for theirs in remote {
        let kept = match link.photos.get_key_value(theirs.path.as_str()) {
            Some((path, kept)) if theirs.id.is_empty() || kept.id == theirs.id => Some((path.as_str(), kept)),
            _ => paired.get(theirs.id.as_str()).and_then(|path| link.photos.get_key_value(*path)).map(|(path, kept)| (path.as_str(), kept)),
        };
        let step = match (kept, theirs.deleted) {

            (None, Some(_)) => None,
            (Some((path, kept)), Some(_)) => {
                seen.insert(path);
                match here.get(path) {

                    Some(ours) if !ours.edits_agree(kept) || !ours.marks_agree(kept) => Some(Step::Restore(path.to_string())),
                    _ => Some(Step::Trash { path: path.to_string(), id: kept.id.clone() }),
                }
            }
            (Some((path, kept)), None) => {
                seen.insert(path);
                match here.get(path) {
                    Some(ours) => both(path, kept, ours, Some(theirs)),

                    None if trashed.contains_key(path) && theirs.updated > kept.updated => {
                        Some(Step::PutBack(Remote { path: path.to_string(), ..theirs.clone() }))
                    }
                    None if trashed.contains_key(path) => Some(Step::Delete { path: path.to_string(), id: kept.id.clone() }),

                    None => None,
                }
            }
            (None, None) => {
                seen.insert(&theirs.path);
                match (here.get(theirs.path.as_str()), link.gone.contains_key(&theirs.path)) {

                    (Some(ours), _) => fresh(ours, theirs),

                    (None, true) => Some(Step::PutBack(theirs.clone())),
                    (None, false) => Some(Step::Arrive(theirs.clone())),
                }
            }
        };
        steps.extend(step);
    }
    for ours in local.iter().filter(|ours| !seen.contains(ours.path.as_str())) {
        let step = match link.photos.get(&ours.path) {
            Some(kept) => both(&ours.path, kept, ours, None),

            None => match link.gone.get(&ours.path) {
                Some(gone) if gone.restored => None,
                Some(_) => Some(Step::Restore(ours.path.clone())),
                None => Some(Step::Send(ours.path.clone())),
            },
        };
        steps.extend(step);
    }
    let gone_here = link.photos.iter().filter(|(path, _)| !seen.contains(path.as_str()) && !here.contains_key(path.as_str()) && trashed.contains_key(*path));
    steps.extend(gone_here.map(|(path, kept)| Step::Delete { path: path.clone(), id: kept.id.clone() }));
    steps
}

fn both(path: &str, kept: &Kept, ours: &Local, theirs: Option<&Remote>) -> Option<Step> {
    let changed = theirs.filter(|theirs| theirs.updated > kept.updated);
    let marked = changed.is_some_and(|theirs| theirs.rating != kept.rating || theirs.flag != kept.flag);
    let marks_here = !ours.marks_agree(kept);

    let theirs_later = changed.is_some_and(|theirs| ours.changed > 0 && theirs.updated > ours.changed);
    let (down_marks, up_marks) = match (marked, marks_here) {
        (true, true) if theirs_later => (true, false),
        (true, true) => (false, true),
        (marked, here) => (marked, here),
    };
    let theirs_albums = theirs.and_then(|theirs| theirs.albums.as_deref()).or(kept.albums.as_deref());
    let albums = theirs_albums.map(|theirs_albums| (self::albums(kept.albums.as_deref(), &ours.albums, theirs_albums), theirs_albums));
    let down = Parts {

        edits: changed.is_some(),
        marks: down_marks,
        albums: albums.as_ref().is_some_and(|(merged, _)| !same_albums(merged, &ours.albums)),
    };
    let up = Parts {
        edits: !ours.edits_agree(kept),
        marks: up_marks,
        albums: albums.as_ref().is_some_and(|(merged, theirs)| !same_albums(merged, theirs)),
    };
    let albums = albums.map(|(merged, _)| merged);

    let first = kept.albums.is_none() && albums.is_some();
    (down.any() || up.any() || first).then(|| Step::Both { path: path.to_string(), id: kept.id.clone(), theirs: theirs.cloned(), down, up, albums })
}

fn fresh(ours: &Local, theirs: &Remote) -> Option<Step> {
    let albums = theirs.albums.as_deref().map(|theirs_albums| (albums(None, &ours.albums, theirs_albums), theirs_albums));
    let moved = |merged: &[String], side: &[String]| !same_albums(merged, side);
    let (mut down, mut up) = match ours.untouched() {
        true => (Parts::ALL, Parts::default()),
        false => (Parts::default(), Parts::ALL),
    };
    down.albums = albums.as_ref().is_some_and(|(merged, _)| moved(merged, &ours.albums));
    up.albums = albums.as_ref().is_some_and(|(merged, theirs)| moved(merged, theirs));
    Some(Step::Both { path: ours.path.clone(), id: theirs.id.clone(), theirs: Some(theirs.clone()), down, up, albums: albums.map(|(merged, _)| merged) })
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Settled {

    Same,

    Up,

    Down,

    UpKeepingTheirs,

    DownKeepingOurs,
}

pub fn settle(kept: &Kept, ours: &Local, theirs: Option<&str>, theirs_updated: i64) -> Settled {
    let home = canon(theirs);
    if home == canon(ours.edits.as_deref()) {
        return Settled::Same;
    }

    let theirs_changed = kept.canon.as_ref().is_none_or(|agreed| *agreed != home);
    match (!ours.edits_agree(kept), theirs_changed) {
        (false, _) => Settled::Down,
        (true, false) => Settled::Up,
        (true, true) if ours.changed > 0 && theirs_updated > ours.changed => Settled::DownKeepingOurs,
        (true, true) => Settled::UpKeepingTheirs,
    }
}

pub fn aside(from_home: bool, when: &str, device: &str) -> String {
    match from_home {
        true => format!("From Numa Home, {when}"),
        false => format!("From {device}"),
    }
}

pub fn device_label(account: &str, device: &str) -> String {
    match device.trim() {
        "" => format!("{account}'s device"),
        device if device.to_lowercase().contains(&account.to_lowercase()) => device.to_string(),
        device => format!("{account}'s {device}"),
    }
}

pub fn listed_after(link: &Link) -> i64 {
    match link.everything && link.rules >= RULES {
        true => link.since,
        false => 0,
    }
}

pub fn next_since(since: i64, remote: &[Remote]) -> i64 {
    remote.iter().map(|photo| photo.updated).fold(since, i64::max)
}

pub fn inside(root: &Path, relative: &str) -> Option<PathBuf> {
    let relative = Path::new(relative);
    let plain = relative.components().all(|part| matches!(part, Component::Normal(_)));
    (plain && relative.components().next().is_some()).then(|| root.join(relative))
}

#[derive(Clone, Debug, PartialEq)]
pub struct Month {

    pub key: String,

    pub name: String,
    pub count: usize,
    pub bytes: u64,
}

pub fn month_of(photo: &Remote) -> String {
    let (year, month, _) = crate::import::civil(photo.taken.unwrap_or(photo.mtime));
    format!("{year}-{month:02}")
}

pub fn months(photos: &[Remote]) -> Vec<Month> {
    const NAMES: [&str; 12] =
        ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
    let mut months: BTreeMap<String, (usize, u64)> = BTreeMap::new();
    for photo in photos {
        let month = months.entry(month_of(photo)).or_default();
        month.0 += 1;
        month.1 += photo.size;
    }
    months
        .into_iter()
        .rev()
        .map(|(key, (count, bytes))| {
            let (year, month) = key.split_once('-').unwrap_or_default();
            let name = format!("{} {year}", NAMES.get(month.parse::<usize>().unwrap_or(1).saturating_sub(1)).unwrap_or(&""));
            Month { key, name, count, bytes }
        })
        .collect()
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Library {
    #[serde(deserialize_with = "id_text")]
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub count: u64,
}

fn get(address: &str, path: &str) -> Vec<OsString> {
    curl(words(&["--max-time", "120", "--header", "@-"]), format!("{}/api/v1/{path}", base(address)))
}

pub fn list_libraries(address: &str) -> Vec<OsString> {
    get(address, "libraries")
}

pub fn libraries(out: &str) -> Result<Vec<Library>, Failure> {
    parsed(answer(out)?)
}

pub fn new_library(address: &str, name: &str) -> Vec<OsString> {
    let body = serde_json::json!({ "name": name }).to_string();
    let rest = ["--max-time", "30", "--request", "POST", "--header", "@-", "--header", "Content-Type: application/json", "--data-binary", &body];
    curl(words(&rest), format!("{}/api/v1/libraries", base(address)))
}

pub fn made_library(out: &str) -> Result<Library, Failure> {
    parsed(answer(out)?)
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Pace {

    pub night: bool,

    pub unmetered: bool,
}

impl Default for Pace {
    fn default() -> Self {
        Self { night: true, unmetered: true }
    }
}

impl Pace {
    pub const NIGHT: std::ops::Range<i32> = 1..6;

    pub fn allows(self, hour: i32, metered: bool) -> bool {
        (!self.night || Self::NIGHT.contains(&hour)) && !(self.unmetered && metered)
    }
}

pub fn get_photo(address: &str, id: &str) -> Vec<OsString> {
    get(address, &format!("photos/{}", escaped(id)))
}

pub fn library_of(out: &str) -> Result<String, Failure> {
    #[derive(Deserialize)]
    struct Answer {
        #[serde(deserialize_with = "id_text")]
        library: String,
    }
    Ok(parsed::<Answer>(answer(out)?)?.library)
}

pub fn list_photos(address: &str, library: &str, since: i64) -> Vec<OsString> {
    get(address, &format!("libraries/{}/photos?since={since}", escaped(library)))
}

pub fn list_changes(address: &str, library: &str, since: i64, synced: bool) -> Vec<OsString> {
    let synced = if synced { "&synced=1" } else { "" };
    get(address, &format!("libraries/{}/photos?since={since}&gone=1{synced}", escaped(library)))
}

fn post(address: &str, path: &str, body: &serde_json::Value) -> Vec<OsString> {
    let body = body.to_string();
    let rest = ["--max-time", "120", "--request", "POST", "--header", "@-", "--header", "Content-Type: application/json", "--data-binary", &body];
    curl(words(&rest), format!("{}/api/v1/{path}", base(address)))
}

fn numbers(ids: &[String]) -> Vec<i64> {
    ids.iter().filter_map(|id| id.parse().ok()).collect()
}

pub fn delete_photos(address: &str, ids: &[String]) -> Vec<OsString> {
    post(address, "photos/delete", &serde_json::json!({ "ids": numbers(ids) }))
}

pub fn restore(address: &str, library: &str, paths: &[String]) -> Vec<OsString> {
    post(address, &format!("libraries/{}/restore", escaped(library)), &serde_json::json!({ "paths": paths }))
}

pub fn restored(out: &str) -> Result<Vec<(String, String)>, Failure> {
    #[derive(Deserialize)]
    struct Back {
        #[serde(deserialize_with = "id_text")]
        id: String,
        path: String,
    }
    #[derive(Deserialize)]
    struct Answer {
        photos: Vec<Back>,
    }
    Ok(parsed::<Answer>(answer(out)?)?.photos.into_iter().map(|back| (back.id, back.path)).collect())
}

pub fn put_in_album(address: &str, ids: &[String], album: Result<&str, &str>, inside: bool) -> Vec<OsString> {
    let body = match album {
        Ok(key) => serde_json::json!({ "ids": numbers(ids), "album": key, "inside": inside }),
        Err(name) => serde_json::json!({ "ids": numbers(ids), "name": name, "inside": inside }),
    };
    post(address, "photos/album", &body)
}

pub fn album_key(out: &str) -> Result<String, Failure> {
    #[derive(Deserialize)]
    struct Answer {
        album: String,
    }
    Ok(parsed::<Answer>(answer(out)?)?.album)
}

pub fn list_albums(address: &str) -> Vec<OsString> {
    get(address, "albums")
}

pub fn albums_at_home(out: &str) -> Result<Vec<(String, String)>, Failure> {
    #[derive(Deserialize)]
    struct Album {
        key: String,
        name: String,
    }
    Ok(parsed::<Vec<Album>>(answer(out)?)?.into_iter().map(|album| (album.key, album.name)).collect())
}

pub fn stop_syncing(address: &str, library: &str) -> Vec<OsString> {
    let rest = ["--max-time", "30", "--request", "DELETE", "--header", "@-"];
    curl(words(&rest), format!("{}/api/v1/libraries/{}/sync", base(address), escaped(library)))
}

pub fn delete_library(address: &str, library: &str) -> Vec<OsString> {
    post(address, "libraries/delete", &serde_json::json!({ "ids": numbers(&[library.to_string()]) }))
}

pub fn photos(out: &str) -> Result<Vec<Remote>, Failure> {
    parsed(answer(out)?)
}

pub fn original(address: &str, id: &str, to: &Path) -> Vec<OsString> {
    let rest = words(&["--header", "@-", "--output"]).into_iter().chain([to.into()]);
    curl(rest, format!("{}/api/v1/photos/{}/original", base(address), escaped(id)))
}

pub fn thumbnails(address: &str, wanted: &[(String, PathBuf)]) -> Vec<OsString> {
    let start = [
        "curl", "--silent", "--show-error", "--fail", "--parallel", "--parallel-max", "8", "--connect-timeout", "10", "--max-time", "600",
        "--header", "@-", "--write-out", "%{http_code} %{filename_effective}\n",
    ];
    let each = wanted.iter().flat_map(|(id, to)| {
        ["--output".into(), to.into(), format!("{}/api/v1/photos/{}/thumb?edge=256", base(address), escaped(id)).into()]
    });
    words(&start).into_iter().chain(each).collect()
}

pub fn thumbnails_came(out: &str) -> Result<HashSet<PathBuf>, Failure> {
    let statuses: Vec<(u16, &str)> = out
        .lines()
        .map(|line| {
            let (status, file) = line.split_once(' ').unwrap_or((line, ""));
            (status.trim().parse().unwrap_or(0), file)
        })
        .collect();
    if statuses.iter().any(|(status, _)| *status == 401) {
        return Err(Failure::SignIn("Sign in again".to_string()));
    }
    if statuses.iter().all(|(status, _)| *status == 0) {
        return Err(Failure::Unreachable);
    }
    Ok(statuses.into_iter().filter(|(status, _)| *status == 200).map(|(_, file)| PathBuf::from(file)).collect())
}

pub fn preview(address: &str, id: &str, to: &Path) -> Vec<OsString> {
    let rest = words(&["--fail", "--max-time", "120", "--header", "@-", "--output"]).into_iter().chain([to.into()]);
    curl(rest, format!("{}/api/v1/photos/{}/preview", base(address), escaped(id)))
}

pub fn get_edits(address: &str, id: &str) -> Vec<OsString> {
    get(address, &format!("photos/{}/edits", escaped(id)))
}

pub fn edits(out: &str) -> Result<(Option<String>, i64), Failure> {
    #[derive(Deserialize)]
    struct Answer {
        edits: Option<serde_json::Value>,
        #[serde(default)]
        updated: i64,
    }
    let answer: Answer = parsed(answer(out)?)?;
    let stack = answer.edits.and_then(|edits| match edits {
        serde_json::Value::String(text) if text.is_empty() => None,
        serde_json::Value::String(text) => Some(text),
        serde_json::Value::Null => None,
        other => Some(other.to_string()),
    });
    Ok((stack, answer.updated))
}

pub fn put_marks(address: &str, id: &str, rating: u8, flag: &str) -> Vec<OsString> {
    let body = serde_json::json!({ "rating": rating, "flag": flag }).to_string();
    let rest = ["--request", "PUT", "--header", "@-", "--header", "Content-Type: application/json", "--data-binary", &body];
    curl(words(&rest), format!("{}/api/v1/photos/{}/marks", base(address), escaped(id)))
}

pub fn updated(out: &str) -> Result<i64, Failure> {
    #[derive(Deserialize)]
    struct Answer {
        #[serde(default)]
        updated: i64,
    }
    Ok(parsed::<Answer>(answer(out)?).map_or(0, |answer| answer.updated))
}

#[cfg(test)]
mod tests;
