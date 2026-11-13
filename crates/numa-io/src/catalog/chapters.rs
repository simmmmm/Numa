use std::collections::HashMap;

use rusqlite::{params, Connection, OptionalExtension, Row};

use super::{global_id, split_id, text, tidy_caption, Catalog};

pub const CHAPTER_NAME_LIMIT: usize = 80;

pub const CHAPTER_WORDS_LIMIT: usize = super::CAPTION_LIMIT;

pub const WEDDING: [&str; 6] = ["Getting Ready", "Ceremony", "Portraits", "Dinner", "First Dance", "Party"];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AlbumKind {
    Album,
    Trip,
    Wedding,
}

impl AlbumKind {
    pub fn name(self) -> &'static str {
        match self {
            AlbumKind::Album => "album",
            AlbumKind::Trip => "trip",
            AlbumKind::Wedding => "wedding",
        }
    }

    pub fn named(name: &str) -> Option<Self> {
        [AlbumKind::Album, AlbumKind::Trip, AlbumKind::Wedding].into_iter().find(|kind| kind.name() == name)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct ChapterPlace {
    pub name: String,
    pub latitude: f64,
    pub longitude: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AlbumChapter {
    pub key: String,
    pub album: String,
    pub name: String,
    pub words: String,
    pub place: Option<ChapterPlace>,
    pub days: Option<(i64, i64)>,
}

const COLUMNS: &str = "key, album, name, words, place, latitude, longitude, first_day, last_day";

fn read(row: &Row) -> rusqlite::Result<AlbumChapter> {
    let place = match (row.get::<_, Option<String>>(4)?, row.get::<_, Option<f64>>(5)?, row.get::<_, Option<f64>>(6)?) {
        (Some(name), Some(latitude), Some(longitude)) => Some(ChapterPlace { name, latitude, longitude }),
        _ => None,
    };
    let days = match (row.get::<_, Option<i64>>(7)?, row.get::<_, Option<i64>>(8)?) {
        (Some(first), Some(last)) => Some((first, last)),
        _ => None,
    };
    Ok(AlbumChapter { key: row.get(0)?, album: row.get(1)?, name: row.get(2)?, words: row.get(3)?, place, days })
}

fn clean_name(name: &str) -> Result<String, String> {
    let name = name.trim();
    if name.is_empty() {
        return Err("A chapter needs a name.".into());
    }
    if name.chars().count() > CHAPTER_NAME_LIMIT {
        return Err(format!("A chapter's name is at most {CHAPTER_NAME_LIMIT} characters."));
    }
    Ok(name.to_string())
}

fn clean_days(days: Option<(i64, i64)>) -> Result<(Option<i64>, Option<i64>), String> {
    match days {
        Some((first, last)) if last < first => Err("A chapter's last day cannot come before its first.".into()),
        Some((first, last)) => Ok((Some(first), Some(last))),
        None => Ok((None, None)),
    }
}

fn by_library(photo_ids: &[i64]) -> Vec<(i64, Vec<i64>)> {
    let mut found: Vec<(i64, Vec<i64>)> = Vec::new();
    for &id in photo_ids {
        let (library, local) = split_id(id);
        match found.iter_mut().find(|(have, _)| *have == library) {
            Some((_, rows)) => rows.push(local),
            None => found.push((library, vec![local])),
        }
    }
    found
}

pub(super) fn rows_of(conn: &Connection, photo: i64) -> Result<Vec<String>, String> {
    conn.prepare("SELECT chapter FROM chapter_photos WHERE photo_id = ?1")
        .and_then(|mut statement| statement.query_map([photo], |row| row.get(0))?.collect())
        .map_err(text)
}

pub(super) fn give_rows(conn: &Connection, chapters: &[String], photo: i64) -> Result<(), String> {
    for chapter in chapters {
        conn.execute("INSERT OR IGNORE INTO chapter_photos (chapter, photo_id) VALUES (?1, ?2)", params![chapter, photo]).map_err(text)?;
    }
    Ok(())
}

impl Catalog {

    pub fn album_kind(&self, album: &str) -> Result<AlbumKind, String> {
        let kind: Option<String> =
            self.home.query_row("SELECT kind FROM album_kinds WHERE album = ?1", [album], |row| row.get(0)).optional().map_err(text)?;
        Ok(kind.and_then(|kind| AlbumKind::named(&kind)).unwrap_or(AlbumKind::Album))
    }

    pub fn album_kinds(&self) -> Result<HashMap<String, AlbumKind>, String> {
        let mut statement = self.home.prepare("SELECT album, kind FROM album_kinds").map_err(text)?;
        let rows = statement.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))).map_err(text)?;
        let mut kinds = HashMap::new();
        for row in rows {
            let (album, kind) = row.map_err(text)?;
            kinds.extend(AlbumKind::named(&kind).filter(|kind| *kind != AlbumKind::Album).map(|kind| (album, kind)));
        }
        Ok(kinds)
    }

    pub fn set_album_kind(&self, album: &str, kind: AlbumKind) -> Result<(), String> {
        if kind == AlbumKind::Album {
            self.home.execute("DELETE FROM album_kinds WHERE album = ?1", [album]).map_err(text)?;
            return Ok(());
        }
        self.home.execute("INSERT OR REPLACE INTO album_kinds (album, kind) VALUES (?1, ?2)", params![album, kind.name()]).map_err(text)?;
        if kind == AlbumKind::Wedding && self.chapters(album)?.is_empty() {
            for (position, name) in WEDDING.iter().enumerate() {
                self.insert_chapter(album, name, None, None, position)?;
            }
        }
        Ok(())
    }

    pub fn chapters(&self, album: &str) -> Result<Vec<AlbumChapter>, String> {
        let mut statement =
            self.home.prepare(&format!("SELECT {COLUMNS} FROM album_chapters WHERE album = ?1 ORDER BY position")).map_err(text)?;
        let rows = statement.query_map([album], read).map_err(text)?;
        rows.collect::<Result<_, _>>().map_err(text)
    }

    pub fn chapter(&self, key: &str) -> Result<Option<AlbumChapter>, String> {
        self.home.query_row(&format!("SELECT {COLUMNS} FROM album_chapters WHERE key = ?1"), [key], read).optional().map_err(text)
    }

    pub fn add_chapter(&self, album: &str, name: &str, place: Option<ChapterPlace>, days: Option<(i64, i64)>) -> Result<AlbumChapter, String> {
        let name = clean_name(name)?;
        clean_days(days)?;
        let have = self.chapters(album)?;
        let position = match days {
            Some((first, _)) if have.iter().all(|chapter| chapter.days.is_some()) => {
                have.iter().position(|chapter| chapter.days.is_some_and(|(there, _)| there > first)).unwrap_or(have.len())
            }
            _ => have.len(),
        };
        self.home.execute("UPDATE album_chapters SET position = position + 1 WHERE album = ?1 AND position >= ?2", params![album, position as i64]).map_err(text)?;
        let key = self.insert_chapter(album, &name, place, days, position)?;
        self.chapter(&key)?.ok_or_else(|| "The chapter was not kept.".to_string())
    }

    fn insert_chapter(&self, album: &str, name: &str, place: Option<ChapterPlace>, days: Option<(i64, i64)>, position: usize) -> Result<String, String> {
        let key = self.new_chapter_key()?;
        let (first, last) = clean_days(days)?;
        let (town, latitude, longitude) = match place {
            Some(place) => (Some(place.name), Some(place.latitude), Some(place.longitude)),
            None => (None, None, None),
        };
        self.home
            .execute(
                "INSERT INTO album_chapters (key, album, position, name, place, latitude, longitude, first_day, last_day) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![key, album, position as i64, name, town, latitude, longitude, first, last],
            )
            .map_err(text)?;
        Ok(key)
    }

    fn new_chapter_key(&self) -> Result<String, String> {
        let mut nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_err(text)?.as_nanos();
        loop {
            let key = format!("{nanos:x}");
            let taken: bool = self.home.query_row("SELECT EXISTS (SELECT 1 FROM album_chapters WHERE key = ?1)", [&key], |row| row.get(0)).map_err(text)?;
            if !taken {
                return Ok(key);
            }
            nanos += 1;
        }
    }

    pub fn rename_chapter(&self, key: &str, name: &str) -> Result<(), String> {
        let name = clean_name(name)?;
        self.home.execute("UPDATE album_chapters SET name = ?1 WHERE key = ?2", params![name, key]).map_err(text)?;
        Ok(())
    }

    pub fn set_chapter_words(&self, key: &str, words: &str) -> Result<(), String> {
        let words = tidy_caption(words);
        if words.chars().count() > CHAPTER_WORDS_LIMIT {
            return Err(format!("A chapter's words are at most {CHAPTER_WORDS_LIMIT} characters."));
        }
        self.home.execute("UPDATE album_chapters SET words = ?1 WHERE key = ?2", params![words, key]).map_err(text)?;
        Ok(())
    }

    pub fn set_chapter_place(&self, key: &str, place: Option<ChapterPlace>) -> Result<(), String> {
        let (town, latitude, longitude) = match place {
            Some(place) => (Some(place.name), Some(place.latitude), Some(place.longitude)),
            None => (None, None, None),
        };
        self.home
            .execute("UPDATE album_chapters SET place = ?1, latitude = ?2, longitude = ?3 WHERE key = ?4", params![town, latitude, longitude, key])
            .map_err(text)?;
        Ok(())
    }

    pub fn set_chapter_days(&self, key: &str, days: Option<(i64, i64)>) -> Result<(), String> {
        let (first, last) = clean_days(days)?;
        self.home.execute("UPDATE album_chapters SET first_day = ?1, last_day = ?2 WHERE key = ?3", params![first, last, key]).map_err(text)?;
        Ok(())
    }

    fn number(&self, keys: &[String]) -> Result<(), String> {
        for (position, key) in keys.iter().enumerate() {
            self.home.execute("UPDATE album_chapters SET position = ?1 WHERE key = ?2", params![position as i64, key]).map_err(text)?;
        }
        Ok(())
    }

    fn keys_of(&self, album: &str) -> Result<Vec<String>, String> {
        Ok(self.chapters(album)?.into_iter().map(|chapter| chapter.key).collect())
    }

    pub fn move_chapter(&self, key: &str, to: usize) -> Result<(), String> {
        let chapter = self.chapter(key)?.ok_or("There is no such chapter.")?;
        let mut keys = self.keys_of(&chapter.album)?;
        keys.retain(|have| have != key);
        keys.insert(to.min(keys.len()), key.to_string());
        self.number(&keys)
    }

    pub fn create_album_keyed(&self, key: &str, name: &str) -> Result<bool, String> {
        let name = name.trim();
        if name.is_empty() || key.is_empty() {
            return Err("An album needs a name.".into());
        }
        let added = self.home.execute("INSERT OR IGNORE INTO albums (key, name) VALUES (?1, ?2)", params![key, name]).map_err(text)?;
        Ok(added == 1)
    }

    pub fn put_chapter(&self, album: &str, chapter: &AlbumChapter) -> Result<(), String> {
        let (first, last) = clean_days(chapter.days)?;
        let (town, latitude, longitude) = match &chapter.place {
            Some(place) => (Some(place.name.clone()), Some(place.latitude), Some(place.longitude)),
            None => (None, None, None),
        };
        let position = self.keys_of(album)?.len() as i64;
        self.home
            .execute(
                "INSERT INTO album_chapters (key, album, position, name, words, place, latitude, longitude, first_day, last_day) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10) \
                 ON CONFLICT(key) DO UPDATE SET name = ?4, words = ?5, place = ?6, latitude = ?7, longitude = ?8, first_day = ?9, last_day = ?10 WHERE album = ?2",
                params![chapter.key, album, position, chapter.name, chapter.words, town, latitude, longitude, first, last],
            )
            .map_err(text)?;
        Ok(())
    }

    pub fn put_album_kind(&self, album: &str, kind: AlbumKind) -> Result<(), String> {
        match kind {
            AlbumKind::Album => self.home.execute("DELETE FROM album_kinds WHERE album = ?1", [album]),
            kind => self.home.execute("INSERT OR REPLACE INTO album_kinds (album, kind) VALUES (?1, ?2)", params![album, kind.name()]),
        }
        .map_err(text)?;
        Ok(())
    }

    pub fn order_chapters(&self, album: &str, keys: &[String]) -> Result<(), String> {
        let have = self.keys_of(album)?;
        let mut order: Vec<String> = keys.iter().filter(|key| have.contains(key)).cloned().collect();
        order.extend(have.iter().filter(|key| !keys.contains(key)).cloned());
        self.number(&order)
    }

    pub fn rekey_chapter(&self, old: &str, new: &str) -> Result<(), String> {
        self.home.execute("UPDATE album_chapters SET key = ?2 WHERE key = ?1", params![old, new]).map_err(text)?;
        for library in self.libraries()? {
            let Ok(open) = self.library(library.id) else { continue };
            if open.offline {
                continue;
            }
            open.conn.execute("UPDATE OR IGNORE chapter_photos SET chapter = ?2 WHERE chapter = ?1", params![old, new]).map_err(text)?;
        }
        Ok(())
    }

    pub fn albums_at_home(&self) -> Result<Vec<(String, String, Option<String>, Option<String>)>, String> {
        let mut statement = self.home.prepare("SELECT album, id, agreed, digest FROM album_at_home").map_err(text)?;
        let rows = statement.query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))).map_err(text)?;
        rows.collect::<Result<_, _>>().map_err(text)
    }

    pub fn pair_album(&self, album: &str, id: &str, agreed: Option<&str>, digest: Option<&str>) -> Result<(), String> {
        self.home
            .execute("INSERT OR REPLACE INTO album_at_home (album, id, agreed, digest) VALUES (?1, ?2, ?3, ?4)", params![album, id, agreed, digest])
            .map_err(text)?;
        Ok(())
    }

    pub fn unpair_album(&self, album: &str) -> Result<(), String> {
        self.home.execute("DELETE FROM album_at_home WHERE album = ?1", [album]).map_err(text)?;
        Ok(())
    }

    pub fn remove_chapter(&self, key: &str) -> Result<(), String> {
        let Some(chapter) = self.chapter(key)? else { return Ok(()) };
        self.forget_rows(&[key.to_string()])?;
        self.home.execute("DELETE FROM album_chapters WHERE key = ?1", [key]).map_err(text)?;
        let keys = self.keys_of(&chapter.album)?;
        self.number(&keys)
    }

    pub fn chapter_of(&self, album: &str) -> Result<HashMap<i64, String>, String> {
        let keys = self.keys_of(album)?;
        let mut found = HashMap::new();
        for library in self.libraries()? {
            let Ok(open) = self.library(library.id) else { continue };
            if open.offline {
                continue;
            }
            for key in &keys {
                let mut statement = open.conn.prepare_cached("SELECT photo_id FROM chapter_photos WHERE chapter = ?1").map_err(text)?;
                let rows = statement.query_map([key], |row| row.get::<_, i64>(0)).map_err(text)?;
                for local in rows {
                    found.insert(global_id(library.id, local.map_err(text)?), key.clone());
                }
            }
        }
        Ok(found)
    }

    pub fn chapters_of(&self, photo_id: i64) -> Result<Vec<String>, String> {
        let (open, local) = self.photo(photo_id)?;
        let mut keys = Vec::new();
        for key in rows_of(&open.conn, local)? {

            if self.chapter(&key)?.is_some() {
                keys.push(key);
            }
        }
        Ok(keys)
    }

    pub fn put_in_chapter(&self, album: &str, chapter: Option<&str>, photo_ids: &[i64]) -> Result<(), String> {
        let keys = self.keys_of(album)?;
        if chapter.is_some_and(|chapter| !keys.iter().any(|key| key == chapter)) {
            return Err("That chapter is not in this album.".into());
        }
        let others: Vec<&String> = keys.iter().filter(|key| Some(key.as_str()) != chapter).collect();

        for (library, _) in by_library(photo_ids) {
            self.library(library)?.connected()?;
        }
        if chapter.is_some() {
            self.set_in_album(album, photo_ids, true)?;
        }
        for (library, rows) in by_library(photo_ids) {
            let open = self.library(library)?;
            let tx = open.conn.unchecked_transaction().map_err(text)?;
            for row in rows {
                for other in &others {
                    tx.execute("DELETE FROM chapter_photos WHERE chapter = ?1 AND photo_id = ?2", params![other, row]).map_err(text)?;
                }
                if let Some(chapter) = chapter {
                    tx.execute("INSERT OR IGNORE INTO chapter_photos (chapter, photo_id) VALUES (?1, ?2)", params![chapter, row]).map_err(text)?;
                }
            }
            tx.commit().map_err(text)?;
        }
        Ok(())
    }

    pub(super) fn leave_chapters_of(&self, album: &str, photo_ids: &[i64]) -> Result<(), String> {
        let keys = self.keys_of(album)?;
        for (library, rows) in by_library(photo_ids) {
            let open = self.library(library)?;
            open.connected()?;
            for row in rows {
                for key in &keys {
                    open.conn.execute("DELETE FROM chapter_photos WHERE chapter = ?1 AND photo_id = ?2", params![key, row]).map_err(text)?;
                }
            }
        }
        Ok(())
    }

    fn forget_rows(&self, keys: &[String]) -> Result<(), String> {
        for library in self.libraries()? {
            let Ok(open) = self.library(library.id) else { continue };
            for key in keys {
                open.conn.execute("DELETE FROM chapter_photos WHERE chapter = ?1", [key]).map_err(text)?;
            }
        }
        Ok(())
    }

    pub(super) fn forget_chapters_of(&self, album: &str) -> Result<(), String> {
        let keys = self.keys_of(album)?;
        self.forget_rows(&keys)?;
        self.home.execute("DELETE FROM album_chapters WHERE album = ?1", [album]).map_err(text)?;
        self.home.execute("DELETE FROM album_kinds WHERE album = ?1", [album]).map_err(text)?;
        Ok(())
    }

    pub(super) fn merge_chapters(&self, keep: &str, other: &str) -> Result<(), String> {
        let (kept, moved) = (self.keys_of(keep)?, self.keys_of(other)?);
        for library in self.libraries()? {
            let Ok(open) = self.library(library.id) else { continue };
            for (moved, kept) in moved.iter().flat_map(|moved| kept.iter().map(move |kept| (moved, kept))) {
                open.conn
                    .execute(
                        "DELETE FROM chapter_photos WHERE chapter = ?1 AND photo_id IN (SELECT photo_id FROM chapter_photos WHERE chapter = ?2)",
                        params![moved, kept],
                    )
                    .map_err(text)?;
            }
        }
        self.home.execute("UPDATE album_chapters SET album = ?1 WHERE album = ?2", params![keep, other]).map_err(text)?;
        self.home.execute("DELETE FROM album_kinds WHERE album = ?1", [other]).map_err(text)?;
        self.number(&[kept, moved].concat())
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Filter, Library};
    use super::*;
    use std::path::PathBuf;

    fn two_libraries(tag: &str) -> (Catalog, Library, Library, PathBuf) {
        let dir = std::env::temp_dir().join(format!("numa-chapters-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for (folder, files) in [("Holiday", ["A.JPG", "B.JPG"]), ("Summer", ["C.JPG", "D.JPG"])] {
            std::fs::create_dir_all(dir.join(folder)).unwrap();
            for file in files {
                std::fs::write(dir.join(folder).join(file), b"not really a photograph").unwrap();
            }
        }
        let catalog = Catalog::open(&dir.join("home.db")).unwrap();
        let holiday = catalog.add_library(&dir.join("Holiday")).unwrap();
        let summer = catalog.add_library(&dir.join("Summer")).unwrap();
        catalog.sync_library(&holiday).unwrap();
        catalog.sync_library(&summer).unwrap();
        (catalog, holiday, summer, dir)
    }

    fn id_of(catalog: &Catalog, library: &Library, name: &str) -> i64 {
        let photos = catalog.photos(library.id, &Filter::default()).unwrap();
        photos.iter().find(|photo| photo.path.ends_with(name)).map(|photo| photo.id).expect(name)
    }

    fn names(catalog: &Catalog, album: &str) -> Vec<String> {
        catalog.chapters(album).unwrap().into_iter().map(|chapter| chapter.name).collect()
    }

    fn in_album(catalog: &Catalog, library: &Library, album: &str) -> usize {
        catalog.photos(library.id, &Filter { album: Some(album.to_string()), ..Filter::default() }).unwrap().len()
    }

    fn kyoto() -> ChapterPlace {
        ChapterPlace { name: "Kyoto".into(), latitude: 35.02, longitude: 135.75 }
    }

    #[test]
    fn chapters_are_kept_in_his_order_with_their_words_and_survive_reopening() {
        let (catalog, _, _, dir) = two_libraries("kept");
        let album = catalog.create_album("Holiday").unwrap();
        let first = catalog.add_chapter(&album, "  Arrival ", None, None).unwrap();
        let second = catalog.add_chapter(&album, "Kyoto", Some(kyoto()), Some((20553, 20557))).unwrap();
        let third = catalog.add_chapter(&album, "Home", None, None).unwrap();
        assert_ne!(first.key, second.key);
        assert_eq!(names(&catalog, &album), ["Arrival", "Kyoto", "Home"]);
        catalog.set_chapter_words(&second.key, "  Four days\r\nof walking. \n").unwrap();

        catalog.move_chapter(&third.key, 0).unwrap();
        assert_eq!(names(&catalog, &album), ["Home", "Arrival", "Kyoto"]);
        catalog.move_chapter(&third.key, 99).unwrap();
        assert_eq!(names(&catalog, &album), ["Arrival", "Kyoto", "Home"]);
        catalog.remove_chapter(&first.key).unwrap();
        let rest = catalog.chapters(&album).unwrap();
        assert_eq!(rest.iter().map(|chapter| chapter.name.as_str()).collect::<Vec<_>>(), ["Kyoto", "Home"]);
        let positions: Vec<i64> = catalog.home.prepare("SELECT position FROM album_chapters ORDER BY position").unwrap().query_map([], |row| row.get(0)).unwrap().collect::<Result<_, _>>().unwrap();
        assert_eq!(positions, [0, 1], "no gap after a chapter goes");

        drop(catalog);
        let catalog = Catalog::open(&dir.join("home.db")).unwrap();
        let kept = catalog.chapter(&second.key).unwrap().unwrap();
        assert_eq!((kept.name.as_str(), kept.words.as_str()), ("Kyoto", "Four days\nof walking."));
        assert_eq!((kept.place, kept.days), (Some(kyoto()), Some((20553, 20557))));
        assert_eq!(catalog.chapter(&first.key).unwrap(), None);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_name_that_is_empty_or_too_long_is_refused_and_not_cut() {
        let (catalog, _, _, dir) = two_libraries("names");
        let album = catalog.create_album("Holiday").unwrap();
        assert!(catalog.add_chapter(&album, "   ", None, None).is_err());
        assert!(catalog.add_chapter(&album, &"x".repeat(CHAPTER_NAME_LIMIT + 1), None, None).is_err());
        let long = "é".repeat(CHAPTER_NAME_LIMIT);
        let chapter = catalog.add_chapter(&album, &long, None, None).unwrap();
        assert_eq!(chapter.name, long, "characters, not bytes");
        assert!(catalog.rename_chapter(&chapter.key, " ").is_err());
        assert!(catalog.set_chapter_words(&chapter.key, &"x".repeat(CHAPTER_WORDS_LIMIT + 1)).is_err());
        catalog.set_chapter_words(&chapter.key, "   ").unwrap();
        assert_eq!(catalog.chapter(&chapter.key).unwrap().unwrap().words, "");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_chapter_with_days_stands_where_its_first_day_puts_it() {
        let (catalog, _, _, dir) = two_libraries("order");
        let album = catalog.create_album("Japan").unwrap();
        for (name, days) in [("Kyoto", (20553, 20557)), ("Tokyo", (20547, 20551)), ("Osaka", (20560, 20561))] {
            catalog.add_chapter(&album, name, None, Some(days)).unwrap();
        }
        assert_eq!(names(&catalog, &album), ["Tokyo", "Kyoto", "Osaka"], "the order travelled");

        catalog.add_chapter(&album, "Packing", None, None).unwrap();
        catalog.add_chapter(&album, "Nara", None, Some((20550, 20550))).unwrap();
        assert_eq!(names(&catalog, &album), ["Tokyo", "Kyoto", "Osaka", "Packing", "Nara"]);
        assert!(catalog.add_chapter(&album, "Back", None, Some((5, 4))).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_wedding_gives_its_six_chapters_once_and_only_to_an_album_without() {
        let (catalog, _, _, dir) = two_libraries("wedding");
        let plain = catalog.create_album("Summer").unwrap();
        let wedding = catalog.create_album("Alex and Sam").unwrap();
        let planned = catalog.create_album("Day").unwrap();
        catalog.add_chapter(&planned, "Morning", None, None).unwrap();
        assert!(catalog.album_kinds().unwrap().is_empty(), "no kind is asked when an album is made");
        assert_eq!(catalog.album_kind(&wedding).unwrap(), AlbumKind::Album);

        catalog.set_album_kind(&wedding, AlbumKind::Wedding).unwrap();
        assert_eq!(names(&catalog, &wedding), WEDDING);
        catalog.set_album_kind(&wedding, AlbumKind::Wedding).unwrap();
        assert_eq!(catalog.chapters(&wedding).unwrap().len(), 6, "not twice");
        let keys: std::collections::HashSet<String> = catalog.chapters(&wedding).unwrap().into_iter().map(|chapter| chapter.key).collect();
        assert_eq!(keys.len(), 6, "six keys, made in one breath");

        catalog.set_album_kind(&planned, AlbumKind::Wedding).unwrap();
        assert_eq!(names(&catalog, &planned), ["Morning"], "an album with chapters keeps them as they are");
        catalog.set_album_kind(&plain, AlbumKind::Trip).unwrap();
        assert!(catalog.chapters(&plain).unwrap().is_empty());

        catalog.set_album_kind(&planned, AlbumKind::Album).unwrap();
        catalog.set_album_kind(&wedding, AlbumKind::Album).unwrap();
        assert_eq!(catalog.album_kinds().unwrap(), HashMap::from([(plain.clone(), AlbumKind::Trip)]));
        assert_eq!(catalog.album_kind(&plain).unwrap(), AlbumKind::Trip);
        assert_eq!(names(&catalog, &wedding), WEDDING);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_photograph_put_in_a_chapter_is_in_the_album_and_in_that_chapter_only() {
        let (catalog, holiday, summer, dir) = two_libraries("put");
        let (a, b) = (id_of(&catalog, &holiday, "A.JPG"), id_of(&catalog, &holiday, "B.JPG"));
        let c = id_of(&catalog, &summer, "C.JPG");
        let album = catalog.create_album("Holiday").unwrap();
        let other = catalog.create_album("Summer").unwrap();
        let (kyoto, tokyo) = (catalog.add_chapter(&album, "Kyoto", None, None).unwrap(), catalog.add_chapter(&album, "Tokyo", None, None).unwrap());
        let elsewhere = catalog.add_chapter(&other, "Elsewhere", None, None).unwrap();
        assert!(catalog.put_in_chapter(&album, Some(&elsewhere.key), &[a]).is_err(), "a chapter of another album");

        catalog.put_in_chapter(&album, Some(&kyoto.key), &[a, b, c]).unwrap();
        assert_eq!((in_album(&catalog, &holiday, &album), in_album(&catalog, &summer, &album)), (2, 1), "in the album too, over two libraries");
        catalog.put_in_chapter(&other, Some(&elsewhere.key), &[a]).unwrap();
        catalog.put_in_chapter(&album, Some(&tokyo.key), &[b, c]).unwrap();

        let of = catalog.chapter_of(&album).unwrap();
        assert_eq!(of, HashMap::from([(a, kyoto.key.clone()), (b, tokyo.key.clone()), (c, tokyo.key.clone())]));
        assert_eq!(catalog.chapter_of(&other).unwrap(), HashMap::from([(a, elsewhere.key.clone())]));
        let mut of_a = catalog.chapters_of(a).unwrap();
        of_a.sort();
        let mut want = vec![kyoto.key.clone(), elsewhere.key.clone()];
        want.sort();
        assert_eq!(of_a, want, "one chapter per album");

        catalog.put_in_chapter(&album, None, &[b, c]).unwrap();
        assert_eq!(catalog.chapter_of(&album).unwrap(), HashMap::from([(a, kyoto.key.clone())]));
        assert_eq!(in_album(&catalog, &holiday, &album) + in_album(&catalog, &summer, &album), 3, "they stay in the album");

        catalog.remove_chapter(&kyoto.key).unwrap();
        assert!(catalog.chapter_of(&album).unwrap().is_empty());
        assert_eq!(in_album(&catalog, &holiday, &album), 2);
        assert!(catalog.chapters_of(a).unwrap().iter().all(|key| *key != kyoto.key));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn taking_a_photograph_out_of_an_album_takes_it_out_of_its_chapter() {
        let (catalog, holiday, _, dir) = two_libraries("leave");
        let (a, b) = (id_of(&catalog, &holiday, "A.JPG"), id_of(&catalog, &holiday, "B.JPG"));
        let album = catalog.create_album("Holiday").unwrap();
        let chapter = catalog.add_chapter(&album, "Kyoto", None, None).unwrap();
        catalog.put_in_chapter(&album, Some(&chapter.key), &[a, b]).unwrap();

        catalog.set_in_album(&album, &[a], false).unwrap();
        assert_eq!(catalog.chapter_of(&album).unwrap(), HashMap::from([(b, chapter.key.clone())]));
        assert_eq!(in_album(&catalog, &holiday, &album), 1);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn deleting_an_album_forgets_its_kind_its_chapters_and_their_rows() {
        let (catalog, holiday, summer, dir) = two_libraries("delete");
        let (a, c) = (id_of(&catalog, &holiday, "A.JPG"), id_of(&catalog, &summer, "C.JPG"));
        let album = catalog.create_album("Holiday").unwrap();
        let keep = catalog.create_album("Summer").unwrap();
        catalog.set_album_kind(&album, AlbumKind::Wedding).unwrap();
        let kept = catalog.add_chapter(&keep, "Alex", None, None).unwrap();
        let ceremony = catalog.chapters(&album).unwrap()[1].key.clone();
        catalog.put_in_chapter(&album, Some(&ceremony), &[a, c]).unwrap();
        catalog.put_in_chapter(&keep, Some(&kept.key), &[a]).unwrap();

        catalog.delete_album(&album).unwrap();
        assert!(catalog.chapters(&album).unwrap().is_empty());
        assert!(catalog.album_kinds().unwrap().is_empty());
        assert_eq!(catalog.chapter(&ceremony).unwrap(), None);
        assert!(catalog.chapters_of(c).unwrap().is_empty());
        let rows: i64 = summer_rows(&catalog, &summer);
        assert_eq!(rows, 0, "the rows of the chapters are gone");
        assert_eq!(catalog.chapter_of(&keep).unwrap(), HashMap::from([(a, kept.key)]), "another album's chapters stay");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    fn summer_rows(catalog: &Catalog, library: &Library) -> i64 {
        let open = catalog.library(library.id).unwrap();
        open.conn.query_row("SELECT COUNT(*) FROM chapter_photos", [], |row| row.get(0)).unwrap()
    }

    #[test]
    fn a_photograph_the_catalog_forgets_leaves_its_chapter() {
        let (catalog, holiday, _, dir) = two_libraries("forgotten");
        let (a, b) = (id_of(&catalog, &holiday, "A.JPG"), id_of(&catalog, &holiday, "B.JPG"));
        let album = catalog.create_album("Holiday").unwrap();
        let chapter = catalog.add_chapter(&album, "Kyoto", None, None).unwrap();
        catalog.put_in_chapter(&album, Some(&chapter.key), &[a, b]).unwrap();

        std::fs::remove_file(dir.join("Holiday/A.JPG")).unwrap();
        catalog.sync_library(&holiday).unwrap();
        assert_eq!(catalog.chapter_of(&album).unwrap(), HashMap::from([(b, chapter.key.clone())]));
        assert_eq!(summer_rows(&catalog, &holiday), 1, "by the cascade, no row is left");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn merging_two_albums_keeps_both_albums_chapters_and_the_first_ones_first() {
        let (catalog, holiday, summer, dir) = two_libraries("merge");
        let (a, b, c) = (id_of(&catalog, &holiday, "A.JPG"), id_of(&catalog, &holiday, "B.JPG"), id_of(&catalog, &summer, "C.JPG"));
        let keep = catalog.create_album("Holiday").unwrap();
        let other = catalog.create_album("Holiday at Home").unwrap();
        catalog.set_album_kind(&other, AlbumKind::Trip).unwrap();
        let (one, two) = (catalog.add_chapter(&keep, "Arrival", None, None).unwrap(), catalog.add_chapter(&keep, "Kyoto", None, None).unwrap());
        let three = catalog.add_chapter(&other, "Osaka", None, None).unwrap();
        let four = catalog.add_chapter(&other, "Nara", None, None).unwrap();
        catalog.put_in_chapter(&keep, Some(&one.key), &[a]).unwrap();

        catalog.put_in_chapter(&other, Some(&three.key), &[a, b, c]).unwrap();

        catalog.merge_albums(&keep, &other).unwrap();

        assert_eq!(names(&catalog, &keep), ["Arrival", "Kyoto", "Osaka", "Nara"]);
        assert!(catalog.chapters(&other).unwrap().is_empty());
        assert!(catalog.album_kinds().unwrap().is_empty(), "the other's kind goes");
        assert_eq!(catalog.chapter_of(&keep).unwrap(), HashMap::from([(a, one.key.clone()), (b, three.key.clone()), (c, three.key.clone())]));
        let _ = (two, four);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
