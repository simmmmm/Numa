use super::{answer, base, curl, escaped, id_text, parsed, words, Failure};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::ffi::OsString;
use std::path::{Component, Path, PathBuf};

pub const LINK: &str = "numa-home";

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
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Kept {
    pub id: String,

    pub edits: String,
    pub rating: u8,
    pub flag: String,

    pub updated: i64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Local {
    pub path: String,
    pub edits: Option<String>,
    pub rating: u8,
    pub flag: String,
}

impl Local {
    fn edits_agree(&self, kept: &Kept) -> bool {
        digest(self.edits.as_deref()) == kept.edits
    }

    fn marks_agree(&self, kept: &Kept) -> bool {
        self.rating == kept.rating && self.flag == kept.flag
    }

    fn untouched(&self) -> bool {
        self.edits.is_none() && self.rating == 0 && self.flag == "none"
    }

    pub fn kept(&self, id: &str, updated: i64) -> Kept {
        Kept { id: id.to_string(), edits: digest(self.edits.as_deref()), rating: self.rating, flag: self.flag.clone(), updated }
    }
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct Remote {
    #[serde(deserialize_with = "id_text")]
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
}

fn no_flag() -> String {
    "none".to_string()
}

pub fn digest(edits: Option<&str>) -> String {
    edits.map_or_else(String::new, |edits| crate::offload::hex(twox_hash::XxHash3_128::oneshot(edits.as_bytes())))
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
}

impl Parts {
    pub const ALL: Parts = Parts { edits: true, marks: true };

    pub fn any(self) -> bool {
        self.edits || self.marks
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Step {

    Arrive(Remote),

    Both { path: String, id: String, theirs: Option<Remote>, down: Parts, up: Parts },

    Send(String),
}

pub fn plan(link: &Link, remote: &[Remote], local: &[Local]) -> Vec<Step> {
    let here: HashMap<&str, &Local> = local.iter().map(|photo| (photo.path.as_str(), photo)).collect();
    let listed: HashSet<&str> = remote.iter().map(|photo| photo.path.as_str()).collect();

    let known: HashSet<&str> = link.photos.values().map(|kept| kept.id.as_str()).collect();
    let mut steps = Vec::new();
    let both = |ours: &Local, id: &str, theirs: Option<&Remote>, down: Parts, up: Parts| {
        (down.any() || up.any()).then(|| Step::Both { path: ours.path.clone(), id: id.to_string(), theirs: theirs.cloned(), down, up })
    };
    for theirs in remote {
        let step = match (link.photos.get(&theirs.path), here.get(theirs.path.as_str())) {

            (Some(_), None) => None,
            (None, None) if known.contains(theirs.id.as_str()) => None,
            (None, None) => Some(Step::Arrive(theirs.clone())),

            (None, Some(ours)) if ours.untouched() => both(ours, &theirs.id, Some(theirs), Parts::ALL, Parts::default()),
            (None, Some(ours)) => both(ours, &theirs.id, Some(theirs), Parts::default(), Parts::ALL),
            (Some(kept), Some(ours)) => {

                let up = Parts { edits: !ours.edits_agree(kept), marks: !ours.marks_agree(kept) };
                let changed = theirs.updated > kept.updated;
                let marked = theirs.rating != kept.rating || theirs.flag != kept.flag;
                let down = Parts { edits: changed && !up.edits, marks: changed && marked && !up.marks };
                both(ours, &kept.id, Some(theirs), down, up)
            }
        };
        steps.extend(step);
    }
    for ours in local.iter().filter(|ours| !listed.contains(ours.path.as_str())) {
        let step = match link.photos.get(&ours.path) {
            None => Some(Step::Send(ours.path.clone())),
            Some(kept) => both(ours, &kept.id, None, Parts::default(), Parts { edits: !ours.edits_agree(kept), marks: !ours.marks_agree(kept) }),
        };
        steps.extend(step);
    }
    steps
}

pub fn listed_after(link: &Link) -> i64 {
    match link.everything {
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

pub fn photos(out: &str) -> Result<Vec<Remote>, Failure> {
    parsed(answer(out)?)
}

pub fn original(address: &str, id: &str, to: &Path) -> Vec<OsString> {
    let rest = words(&["--header", "@-", "--output"]).into_iter().chain([to.into()]);
    curl(rest, format!("{}/api/v1/photos/{}/original", base(address), escaped(id)))
}

pub fn thumbnails(address: &str, wanted: &[(String, PathBuf)]) -> Vec<OsString> {
    let start = ["curl", "--silent", "--show-error", "--fail", "--connect-timeout", "10", "--max-time", "600", "--header", "@-", "--write-out", "%{http_code}\n"];
    let each = wanted.iter().flat_map(|(id, to)| {
        ["--output".into(), to.into(), format!("{}/api/v1/photos/{}/thumb?edge=256", base(address), escaped(id)).into()]
    });
    words(&start).into_iter().chain(each).collect()
}

pub fn thumbnails_came(out: &str) -> Result<Vec<bool>, Failure> {
    let statuses: Vec<u16> = out.lines().map(|line| line.trim().parse().unwrap_or(0)).collect();
    if statuses.contains(&401) {
        return Err(Failure::SignIn("Sign in again".to_string()));
    }
    if statuses.iter().all(|status| *status == 0) {
        return Err(Failure::Unreachable);
    }
    Ok(statuses.iter().map(|status| *status == 200).collect())
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
