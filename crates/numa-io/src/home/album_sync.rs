use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::catalog::{AlbumChapter, AlbumKind, Catalog, ChapterPlace, Filter, CHAPTER_WORDS_LIMIT};
use crate::home::sync::{Link, LINK};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct PlaceDoc {
    pub name: String,
    pub position: [f64; 2],
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ChapterDoc {
    pub key: String,
    pub name: String,
    #[serde(default)]
    pub words: String,
    #[serde(default)]
    pub place: Option<PlaceDoc>,
    #[serde(default)]
    pub days: Option<[i64; 2]>,

    #[serde(default)]
    pub photos: BTreeSet<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct AlbumDoc {

    pub id: String,
    pub name: String,

    pub kind: String,
    #[serde(default)]
    pub chapters: Vec<ChapterDoc>,
}

fn three<T: PartialEq + Clone>(agreed: Option<&T>, ours: &T, theirs: &T) -> T {
    match agreed {
        Some(agreed) if theirs == agreed => ours.clone(),
        _ => theirs.clone(),
    }
}

fn words(agreed: Option<&String>, ours: &str, theirs: &str) -> String {
    let (o, t) = (ours.trim(), theirs.trim());
    let unchanged = |side: &str| agreed.is_some_and(|agreed| agreed.trim() == side);
    if o == t || unchanged(o) {
        return theirs.to_string();
    }
    if unchanged(t) {
        return ours.to_string();
    }

    if o.is_empty() || t.contains(o) {
        return theirs.to_string();
    }
    if t.is_empty() || o.contains(t) {
        return ours.to_string();
    }
    let both = format!("{t}\n\n{o}");
    if both.chars().count() > CHAPTER_WORDS_LIMIT {
        theirs.to_string()
    } else {
        both
    }
}

fn set(agreed: Option<&BTreeSet<String>>, ours: &BTreeSet<String>, theirs: &BTreeSet<String>) -> BTreeSet<String> {
    ours.union(theirs).filter(|id| (ours.contains(*id) && theirs.contains(*id)) || !agreed.is_some_and(|agreed| agreed.contains(*id))).cloned().collect()
}

fn said(chapter: &ChapterDoc) -> ChapterDoc {
    ChapterDoc { photos: BTreeSet::new(), ..chapter.clone() }
}

fn chapter(agreed: Option<&ChapterDoc>, ours: &ChapterDoc, theirs: &ChapterDoc) -> ChapterDoc {
    ChapterDoc {
        key: theirs.key.clone(),
        name: three(agreed.map(|a| &a.name), &ours.name, &theirs.name),
        words: words(agreed.map(|a| &a.words), &ours.words, &theirs.words),
        place: three(agreed.map(|a| &a.place), &ours.place, &theirs.place),
        days: three(agreed.map(|a| &a.days), &ours.days, &theirs.days),
        photos: set(agreed.map(|a| &a.photos), &ours.photos, &theirs.photos),
    }
}

fn order(agreed: Option<&AlbumDoc>, ours: &AlbumDoc, theirs: &AlbumDoc, kept: &HashSet<&str>) -> Vec<String> {
    let keys = |doc: &AlbumDoc| -> Vec<String> { doc.chapters.iter().map(|chapter| chapter.key.clone()).filter(|key| kept.contains(key.as_str())).collect() };
    let (o, t) = (keys(ours), keys(theirs));
    let common = |list: &[String], other: &[String]| -> Vec<String> { list.iter().filter(|key| other.contains(key)).cloned().collect() };
    let ours_moved = agreed.is_some_and(|agreed| {
        let a = keys(agreed);
        common(&o, &a) != common(&a, &o) && common(&t, &a) == common(&a, &t)
    });
    let (mut base, other) = if ours_moved { (o, t) } else { (t, o) };
    for (at, key) in other.iter().enumerate() {
        if base.contains(key) {
            continue;
        }

        let after = other[..at].iter().rev().find_map(|before| base.iter().position(|have| have == before));
        base.insert(after.map_or(0, |place| place + 1), key.clone());
    }
    base
}

pub fn merge(agreed: Option<&AlbumDoc>, ours: &AlbumDoc, theirs: &AlbumDoc) -> AlbumDoc {
    let by_key = |doc: &'_ AlbumDoc| -> HashMap<String, ChapterDoc> { doc.chapters.iter().map(|chapter| (chapter.key.clone(), chapter.clone())).collect() };
    let (a, o, t) = (agreed.map(by_key).unwrap_or_default(), by_key(ours), by_key(theirs));
    let mut merged: HashMap<String, ChapterDoc> = HashMap::new();
    for key in o.keys().chain(t.keys()) {
        let was = a.get(key);
        let now = match (o.get(key), t.get(key)) {
            (Some(ours), Some(theirs)) => Some(chapter(was, ours, theirs)),

            (Some(one), None) | (None, Some(one)) => match was {
                Some(was) if said(was) == said(one) => None,
                _ => Some(one.clone()),
            },
            (None, None) => None,
        };
        if let Some(now) = now {
            merged.insert(key.clone(), now);
        }
    }

    let kind = match agreed {
        None if theirs.kind == "album" => ours.kind.clone(),
        _ => three(agreed.map(|a| &a.kind), &ours.kind, &theirs.kind),
    };
    let kept: HashSet<&str> = merged.keys().map(String::as_str).collect();
    let keys = order(agreed, ours, theirs, &kept);
    let mut chapters: Vec<ChapterDoc> = keys.iter().filter_map(|key| merged.get(key).cloned()).collect();

    let mut held: HashMap<String, usize> = HashMap::new();
    for at in 0..chapters.len() {
        for id in chapters[at].photos.clone() {
            match held.get(&id).copied() {
                None => drop(held.insert(id, at)),
                Some(first) => {
                    let theirs_here = t.get(&chapters[at].key).is_some_and(|chapter| chapter.photos.contains(&id));
                    let (out, stays) = if theirs_here { (first, at) } else { (at, first) };
                    chapters[out].photos.remove(&id);
                    held.insert(id, stays);
                }
            }
        }
    }
    AlbumDoc { id: theirs.id.clone(), name: three(agreed.map(|a| &a.name), &ours.name, &theirs.name), kind, chapters }
}

pub struct Held {
    pub home: HashMap<i64, String>,
    pub here: HashMap<String, i64>,
}

pub fn held(catalog: &Catalog) -> Result<Held, String> {
    let mut held = Held { home: HashMap::new(), here: HashMap::new() };
    for library in catalog.libraries()? {
        let Some(link) = catalog.library_setting(library.id, LINK).and_then(|json| serde_json::from_str::<Link>(&json).ok()) else { continue };

        let Ok(photos) = catalog.photos(library.id, &Filter::default()) else { continue };
        for photo in photos {
            let Ok(inside) = photo.path.strip_prefix(&library.path) else { continue };
            let Some(kept) = link.photos.get(&inside.to_string_lossy().replace('\\', "/")) else { continue };
            if !kept.id.is_empty() {
                held.home.insert(photo.id, kept.id.clone());
                held.here.insert(kept.id.clone(), photo.id);
            }
        }
    }
    Ok(held)
}

fn chapter_doc(chapter: &AlbumChapter) -> ChapterDoc {
    ChapterDoc {
        key: chapter.key.clone(),
        name: chapter.name.clone(),
        words: chapter.words.clone(),
        place: chapter.place.as_ref().map(|place| PlaceDoc { name: place.name.clone(), position: [place.latitude, place.longitude] }),
        days: chapter.days.map(|(first, last)| [first, last]),
        photos: BTreeSet::new(),
    }
}

fn chapter_kept(album: &str, doc: &ChapterDoc) -> AlbumChapter {
    AlbumChapter {
        key: doc.key.clone(),
        album: album.to_string(),
        name: doc.name.clone(),
        words: doc.words.clone(),
        place: doc.place.as_ref().map(|place| ChapterPlace { name: place.name.clone(), latitude: place.position[0], longitude: place.position[1] }),
        days: doc.days.map(|[first, last]| (first, last)),
    }
}

pub fn local_doc(catalog: &Catalog, album: &str, id: &str, held: &Held) -> Result<AlbumDoc, String> {
    let name = catalog.albums()?.into_iter().find(|(key, _)| key == album).map(|(_, name)| name).ok_or("There is no such album.")?;
    let mut chapters: Vec<ChapterDoc> = catalog.chapters(album)?.iter().map(chapter_doc).collect();
    for (photo, key) in catalog.chapter_of(album)? {
        if let (Some(home), Some(chapter)) = (held.home.get(&photo), chapters.iter_mut().find(|chapter| chapter.key == key)) {
            chapter.photos.insert(home.clone());
        }
    }
    Ok(AlbumDoc { id: id.to_string(), name, kind: catalog.album_kind(album)?.name().to_string(), chapters })
}

fn within(doc: &AlbumDoc, held: &Held) -> AlbumDoc {
    let mut doc = doc.clone();
    for chapter in &mut doc.chapters {
        chapter.photos.retain(|id| held.here.contains_key(id));
    }
    doc
}

pub fn apply(catalog: &Catalog, album: &str, merged: &AlbumDoc, held: &Held) -> Result<(), String> {
    let now = local_doc(catalog, album, &merged.id, held)?;
    if now.name != merged.name && catalog.rename_album(album, &merged.name).is_ok() {
        rename_in_links(catalog, &now.name, Some(&merged.name))?;
    }
    if now.kind != merged.kind {
        catalog.put_album_kind(album, AlbumKind::named(&merged.kind).unwrap_or(AlbumKind::Album))?;
    }
    for gone in now.chapters.iter().filter(|have| !merged.chapters.iter().any(|chapter| chapter.key == have.key)) {
        catalog.remove_chapter(&gone.key)?;
    }
    for chapter in &merged.chapters {
        catalog.put_chapter(album, &chapter_kept(album, chapter))?;
    }
    catalog.order_chapters(album, &merged.chapters.iter().map(|chapter| chapter.key.clone()).collect::<Vec<_>>())?;

    let here = |ids: &BTreeSet<String>| -> Vec<i64> { ids.iter().filter_map(|id| held.here.get(id).copied()).collect() };
    for chapter in &merged.chapters {
        let before = now.chapters.iter().find(|have| have.key == chapter.key).map(|have| have.photos.clone()).unwrap_or_default();
        let out = here(&before.difference(&chapter.photos).cloned().collect());
        if !out.is_empty() {
            catalog.take_from_chapter(&chapter.key, &out)?;
        }
    }
    for chapter in &merged.chapters {
        let before = now.chapters.iter().find(|have| have.key == chapter.key).map(|have| have.photos.clone()).unwrap_or_default();
        let into = here(&chapter.photos.difference(&before).cloned().collect());
        if !into.is_empty() {

            let _ = catalog.put_in_chapter(album, Some(&chapter.key), &into);
        }
    }
    Ok(())
}

pub fn rename_in_links(catalog: &Catalog, old: &str, new: Option<&str>) -> Result<(), String> {
    for library in catalog.libraries()? {
        let Some(mut link) = catalog.library_setting(library.id, LINK).and_then(|json| serde_json::from_str::<Link>(&json).ok()) else { continue };
        let mut changed = false;
        for kept in link.photos.values_mut() {
            let Some(names) = kept.albums.as_mut() else { continue };
            let Some(at) = names.iter().position(|name| name.eq_ignore_ascii_case(old)) else { continue };
            match new {
                Some(new) => names[at] = new.to_string(),
                None => drop(names.remove(at)),
            }
            changed = true;
        }
        if changed {
            let json = serde_json::to_string(&link).map_err(|err| err.to_string())?;

            let _ = catalog.set_library_setting(library.id, LINK, Some(&json));
        }
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq)]
pub struct Call {
    pub method: &'static str,
    pub path: String,
    pub body: Option<String>,
}

#[derive(Deserialize)]
struct Listed {
    id: String,
    name: String,
    #[serde(default)]
    may_change: bool,
    #[serde(default)]
    digest: String,
}

#[derive(Deserialize, Default)]
struct Listing {
    #[serde(default)]
    albums: Vec<Listed>,
    #[serde(default)]
    gone: Vec<String>,
}

#[derive(Deserialize)]
struct Fetched {
    #[serde(flatten)]
    doc: AlbumDoc,
    #[serde(default)]
    digest: String,
}

enum Waiting {

    Doc { id: String, album: Option<String> },

    Put { album: String, id: String, merged: AlbumDoc },

    Delete { album: String },
}

#[derive(Debug, Default, PartialEq)]
pub struct Done {
    pub came: usize,
    pub went: usize,

    pub before: bool,
}

pub struct Round {
    listing: Listing,
    waiting: Vec<Waiting>,
    held: Held,
    done: Done,

    open: bool,
}

fn ok(status: u16) -> bool {
    (200..300).contains(&status)
}

impl Round {

    pub fn begin(catalog: &Catalog) -> Result<(Round, Call), String> {
        let round = Round { listing: Listing::default(), waiting: Vec::new(), held: held(catalog)?, done: Done::default(), open: false };
        Ok((round, Call { method: "GET", path: "sync/albums".into(), body: None }))
    }

    fn paired(catalog: &Catalog) -> Result<Vec<(String, String, Option<AlbumDoc>, String)>, String> {
        Ok(catalog.albums_at_home()?.into_iter().map(|(album, id, agreed, digest)| (album, id, agreed.and_then(|json| serde_json::from_str(&json).ok()), digest.unwrap_or_default())).collect())
    }

    pub fn listed(&mut self, catalog: &Catalog, status: u16, body: &str) -> Result<Vec<Call>, String> {
        if !ok(status) {
            self.done.before = matches!(status, 404 | 405);
            return Ok(Vec::new());
        }
        self.listing = serde_json::from_str(body).map_err(|err| err.to_string())?;
        self.open = true;
        let paired = Self::paired(catalog)?;
        let here = catalog.albums()?;
        for listed in &self.listing.albums {
            let pair = paired.iter().find(|(album, id, ..)| id == &listed.id && here.iter().any(|(key, _)| key == album));
            match pair {

                Some((_, _, Some(_), digest)) if digest == &listed.digest && !digest.is_empty() => {}
                Some((album, ..)) => self.waiting.push(Waiting::Doc { id: listed.id.clone(), album: Some(album.clone()) }),
                None if paired.iter().any(|(_, id, ..)| id == &listed.id) => {}
                None => {

                    let same = here.iter().find(|(key, name)| name.eq_ignore_ascii_case(&listed.name) && !paired.iter().any(|(album, ..)| album == key));
                    self.waiting.push(Waiting::Doc { id: listed.id.clone(), album: same.map(|(key, _)| key.clone()) });
                }
            }
        }
        Ok(self.waiting.iter().filter_map(|waiting| match waiting {
            Waiting::Doc { id, .. } => Some(Call { method: "GET", path: format!("sync/albums/{id}"), body: None }),
            _ => None,
        }).collect())
    }

    fn settle(&mut self, catalog: &Catalog, album: &str, agreed: Option<&AlbumDoc>, theirs: &AlbumDoc, digest: &str, may_change: bool) -> Result<Option<Call>, String> {
        let theirs_held = within(theirs, &self.held);
        let agreed_held = agreed.map(|agreed| within(agreed, &self.held));

        if agreed.is_none() {
            for chapter in catalog.chapters(album)? {
                let same: Vec<&ChapterDoc> = theirs.chapters.iter().filter(|one| one.name.eq_ignore_ascii_case(&chapter.name)).collect();
                if let [one] = same[..] {
                    if one.key != chapter.key && !catalog.chapters(album)?.iter().any(|have| have.key == one.key) {
                        catalog.rekey_chapter(&chapter.key, &one.key)?;
                    }
                }
            }
        }
        let ours = local_doc(catalog, album, &theirs.id, &self.held)?;
        let merged = merge(agreed_held.as_ref(), &ours, &theirs_held);
        if merged != ours {
            apply(catalog, album, &merged, &self.held)?;
            self.done.came += 1;
        }

        let now = local_doc(catalog, album, &theirs.id, &self.held)?;
        if now == theirs_held || !may_change {
            let kept = if now == theirs_held { Some(serde_json::to_string(&now).map_err(|err| err.to_string())?) } else { agreed.and_then(|agreed| serde_json::to_string(agreed).ok()) };
            catalog.pair_album(album, &theirs.id, kept.as_deref(), Some(digest))?;
            return Ok(None);
        }

        catalog.pair_album(album, &theirs.id, agreed.and_then(|agreed| serde_json::to_string(agreed).ok()).as_deref(), None)?;
        let body = up(&now, &theirs_held, Some(digest));
        self.waiting.push(Waiting::Put { album: album.to_string(), id: theirs.id.clone(), merged: now });
        Ok(Some(Call { method: "PUT", path: format!("sync/albums/{}", theirs.id), body: Some(body) }))
    }

    pub fn fetched(&mut self, catalog: &Catalog, answers: &[(u16, String)]) -> Result<Vec<Call>, String> {
        if !self.open {
            return Ok(Vec::new());
        }
        let asked: Vec<Waiting> = std::mem::take(&mut self.waiting);
        let mut calls = Vec::new();
        let mut settled: HashSet<String> = HashSet::new();
        let may = |listing: &Listing, id: &str| listing.albums.iter().find(|listed| listed.id == id).is_some_and(|listed| listed.may_change);
        for (waiting, (status, body)) in asked.iter().zip(answers) {
            let Waiting::Doc { id, album } = waiting else { continue };
            let Some(fetched) = ok(*status).then(|| serde_json::from_str::<Fetched>(body).ok()).flatten() else { continue };
            let agreed = Self::paired(catalog)?.into_iter().find(|(_, have, ..)| have == id).and_then(|(_, _, agreed, _)| agreed);
            let album = match album {
                Some(album) => album.clone(),

                None => match catalog.create_album_keyed(id, &fetched.doc.name)? {
                    true => id.clone(),
                    false => continue,
                },
            };
            settled.insert(album.clone());
            let may_change = may(&self.listing, id);
            calls.extend(self.settle(catalog, &album, agreed.as_ref(), &fetched.doc, &fetched.digest, may_change)?);
        }
        let here = catalog.albums()?;
        for (album, id, agreed, digest) in Self::paired(catalog)? {
            if settled.contains(&album) {
                continue;
            }
            let listed = self.listing.albums.iter().find(|listed| listed.id == id);
            let exists = here.iter().any(|(key, _)| key == &album);
            match (exists, listed, agreed) {

                (false, Some(listed), _) if listed.digest == digest && listed.may_change => {
                    self.waiting.push(Waiting::Delete { album: album.clone() });
                    calls.push(Call { method: "DELETE", path: format!("albums/{id}"), body: None });
                }
                (false, ..) => catalog.unpair_album(&album)?,

                (true, None, Some(agreed)) if self.listing.gone.contains(&id) => {
                    let unchanged = local_doc(catalog, &album, &id, &self.held)? == within(&agreed, &self.held);
                    catalog.unpair_album(&album)?;
                    if unchanged {
                        rename_in_links(catalog, &agreed.name, None)?;
                        catalog.delete_album(&album)?;
                        self.done.came += 1;
                    }
                }

                (true, Some(listed), Some(agreed)) => {
                    let (digest, may_change) = (listed.digest.clone(), listed.may_change);
                    calls.extend(self.settle(catalog, &album, Some(&agreed), &agreed, &digest, may_change)?);
                }
                _ => {}
            }
        }

        let paired: HashSet<String> = Self::paired(catalog)?.into_iter().map(|(album, ..)| album).collect();

        let here = catalog.albums()?;
        for (album, _) in here.iter().filter(|(key, _)| !paired.contains(key)) {
            let photos = catalog.photos_everywhere(&Filter { album: Some(album.clone()), ..Filter::default() })?;
            if !photos.is_empty() && !photos.iter().any(|photo| self.held.home.contains_key(&photo.id)) {
                continue;
            }
            let ours = local_doc(catalog, album, album, &self.held)?;
            let nothing = AlbumDoc { id: album.clone(), name: ours.name.clone(), kind: "album".into(), chapters: Vec::new() };
            let body = up(&ours, &nothing, None);
            self.waiting.push(Waiting::Put { album: album.clone(), id: album.clone(), merged: ours });
            calls.push(Call { method: "PUT", path: format!("sync/albums/{album}"), body: Some(body) });
        }
        Ok(calls)
    }

    pub fn answered(mut self, catalog: &Catalog, answers: &[(u16, String)]) -> Result<Done, String> {
        for (waiting, (status, body)) in self.waiting.iter().zip(answers) {
            match waiting {
                Waiting::Put { album, id, merged } if ok(*status) => {
                    let digest = serde_json::from_str::<serde_json::Value>(body).ok().and_then(|said| said["digest"].as_str().map(str::to_string));
                    let agreed = serde_json::to_string(merged).map_err(|err| err.to_string())?;
                    catalog.pair_album(album, id, Some(&agreed), digest.as_deref())?;
                    self.done.went += 1;
                }

                Waiting::Delete { album } if ok(*status) || *status == 404 => {
                    catalog.unpair_album(album)?;
                    self.done.went += 1;
                }
                _ => {}
            }
        }
        Ok(self.done)
    }
}

fn up(now: &AlbumDoc, theirs: &AlbumDoc, base: Option<&str>) -> String {
    let chapters: Vec<serde_json::Value> = now
        .chapters
        .iter()
        .map(|chapter| {
            let before = theirs.chapters.iter().find(|have| have.key == chapter.key).map(|have| have.photos.clone()).unwrap_or_default();
            let (into, out): (Vec<&String>, Vec<&String>) = (chapter.photos.difference(&before).collect(), before.difference(&chapter.photos).collect());
            serde_json::json!({ "key": chapter.key, "name": chapter.name, "words": chapter.words, "place": chapter.place, "days": chapter.days, "in": into, "out": out })
        })
        .collect();
    serde_json::json!({ "base": base, "name": now.name, "kind": now.kind, "chapters": chapters }).to_string()
}

#[derive(Deserialize)]
pub struct Sent {
    #[serde(default)]
    pub base: Option<String>,
    pub name: String,
    pub kind: String,
    #[serde(default)]
    pub chapters: Vec<SentChapter>,
}

#[derive(Deserialize)]
pub struct SentChapter {
    #[serde(flatten)]
    pub chapter: ChapterDoc,
    #[serde(default, rename = "in")]
    pub into: Vec<String>,
    #[serde(default)]
    pub out: Vec<String>,
}

pub fn digest(doc: &AlbumDoc) -> String {
    let ordered: BTreeMap<&str, serde_json::Value> = [("doc", serde_json::to_value(doc).unwrap_or_default())].into_iter().collect();
    crate::home::sync::digest(Some(&serde_json::to_string(&ordered).unwrap_or_default()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn chapter(key: &str, name: &str, words: &str, photos: &[&str]) -> ChapterDoc {
        ChapterDoc { key: key.into(), name: name.into(), words: words.into(), place: None, days: None, photos: photos.iter().map(|id| id.to_string()).collect() }
    }

    fn album(name: &str, kind: &str, chapters: Vec<ChapterDoc>) -> AlbumDoc {
        AlbumDoc { id: "h1".into(), name: name.into(), kind: kind.into(), chapters }
    }

    #[test]
    fn what_one_side_changed_goes_to_the_other_and_home_stands_where_both_did() {
        let agreed = album("Japan", "album", vec![chapter("a", "Tokyo", "", &["1"])]);

        let merged = merge(Some(&agreed), &album("Japan 2026", "album", agreed.chapters.clone()), &album("Japan", "trip", agreed.chapters.clone()));
        assert_eq!((merged.name.as_str(), merged.kind.as_str()), ("Japan 2026", "trip"));

        let merged = merge(Some(&agreed), &album("Nippon", "album", vec![]), &album("Nihon", "album", vec![]));
        assert_eq!(merged.name, "Nihon");

        let merged = merge(None, &album("japan", "wedding", vec![]), &album("Japan", "album", vec![]));
        assert_eq!((merged.name.as_str(), merged.kind.as_str()), ("Japan", "wedding"));
    }

    #[test]
    fn words_both_wrote_are_both_kept_and_one_sides_alone_goes_over() {
        let agreed = album("W", "wedding", vec![chapter("a", "Ceremony", "At two.", &[])]);
        let ours = album("W", "wedding", vec![chapter("a", "Ceremony", "At two. Rings: his brother.", &[])]);
        let theirs = album("W", "wedding", vec![chapter("a", "Ceremony", "In the orchard.", &[])]);
        assert_eq!(merge(Some(&agreed), &ours, &theirs).chapters[0].words, "In the orchard.\n\nAt two. Rings: his brother.");
        assert_eq!(merge(Some(&agreed), &ours, &agreed).chapters[0].words, "At two. Rings: his brother.");
        assert_eq!(merge(Some(&agreed), &agreed, &theirs).chapters[0].words, "In the orchard.");

        let cleared = album("W", "wedding", vec![chapter("a", "Ceremony", "", &[])]);
        assert_eq!(merge(Some(&agreed), &cleared, &agreed).chapters[0].words, "");

        let long = "x".repeat(CHAPTER_WORDS_LIMIT - 3);
        let theirs = album("W", "wedding", vec![chapter("a", "Ceremony", &long, &[])]);
        assert_eq!(merge(Some(&agreed), &ours, &theirs).chapters[0].words, long);
    }

    #[test]
    fn chapters_are_added_removed_and_ordered_three_ways() {
        let agreed = album("W", "wedding", vec![chapter("a", "Ready", "", &[]), chapter("b", "Ceremony", "", &[]), chapter("c", "Party", "", &[])]);

        let ours = album("W", "wedding", vec![chapter("a", "Ready", "", &[]), chapter("b", "Ceremony", "", &[]), chapter("n", "Portraits", "", &[]), chapter("c", "Party", "", &[])]);
        let theirs = album("W", "wedding", vec![chapter("b", "Ceremony", "", &[]), chapter("c", "Party", "", &[])]);
        let keys = |doc: &AlbumDoc| doc.chapters.iter().map(|chapter| chapter.key.clone()).collect::<Vec<_>>();
        assert_eq!(keys(&merge(Some(&agreed), &ours, &theirs)), ["b", "n", "c"]);

        let mut changed = ours.clone();
        changed.chapters[0].words = "Dress at the window.".into();
        assert_eq!(keys(&merge(Some(&agreed), &changed, &theirs)), ["a", "b", "n", "c"]);

        let moved = album("W", "wedding", vec![agreed.chapters[2].clone(), agreed.chapters[0].clone(), agreed.chapters[1].clone()]);
        assert_eq!(keys(&merge(Some(&agreed), &moved, &agreed)), ["c", "a", "b"]);
        let theirs_moved = album("W", "wedding", vec![agreed.chapters[1].clone(), agreed.chapters[0].clone(), agreed.chapters[2].clone()]);
        assert_eq!(keys(&merge(Some(&agreed), &moved, &theirs_moved)), ["b", "a", "c"]);

        assert_eq!(keys(&merge(None, &album("W", "album", vec![chapter("x", "Mine", "", &[])]), &theirs)), ["x", "b", "c"]);
    }

    #[test]
    fn photographs_go_in_and_out_three_ways_and_are_in_one_chapter() {
        let agreed = album("J", "trip", vec![chapter("a", "Tokyo", "", &["1", "2"]), chapter("b", "Kyoto", "", &["3"])]);

        let ours = album("J", "trip", vec![chapter("a", "Tokyo", "", &["1", "4"]), chapter("b", "Kyoto", "", &["3"])]);
        let theirs = album("J", "trip", vec![chapter("a", "Tokyo", "", &["1", "2"]), chapter("b", "Kyoto", "", &["3", "5"])]);
        let merged = merge(Some(&agreed), &ours, &theirs);
        let ids = |at: usize| merged.chapters[at].photos.iter().cloned().collect::<Vec<_>>();
        assert_eq!((ids(0), ids(1)), (vec!["1".to_string(), "4".into()], vec!["3".to_string(), "5".into()]));

        let ours = album("J", "trip", vec![chapter("a", "Tokyo", "", &["2"]), chapter("b", "Kyoto", "", &["3"]), chapter("c", "Nara", "", &["1"])]);
        let theirs = album("J", "trip", vec![chapter("a", "Tokyo", "", &["2"]), chapter("b", "Kyoto", "", &["1", "3"])]);
        let merged = merge(Some(&agreed), &ours, &theirs);
        assert!(merged.chapters[1].photos.contains("1") && !merged.chapters[2].photos.contains("1"));
    }

    #[test]
    fn what_goes_up_names_what_changed_and_never_a_whole_set() {
        let theirs = album("J", "trip", vec![chapter("a", "Tokyo", "", &["1", "2"])]);
        let now = album("J", "trip", vec![chapter("a", "Tokyo", "Four days.", &["1", "4"])]);
        let sent: Sent = serde_json::from_str(&up(&now, &theirs, Some("d1"))).unwrap();
        assert_eq!((sent.base.as_deref(), sent.name.as_str(), sent.kind.as_str()), (Some("d1"), "J", "trip"));
        assert_eq!((sent.chapters[0].into.clone(), sent.chapters[0].out.clone(), sent.chapters[0].chapter.words.as_str()), (vec!["4".to_string()], vec!["2".to_string()], "Four days."));
        assert!(sent.chapters[0].chapter.photos.is_empty());
        assert_ne!(digest(&theirs), digest(&now));
        assert_eq!(digest(&theirs), digest(&theirs.clone()));
    }
}
