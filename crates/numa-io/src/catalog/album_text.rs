use rusqlite::{params, OptionalExtension, Row};

use super::{text, Catalog};
use crate::markdown::{self, TEXT_LIMIT};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AlbumDay {

    pub chapter: Option<String>,
    pub day: i64,
    pub text: String,

    pub changed: i64,
}

fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |since| since.as_secs() as i64)
}

fn stored(chapter: Option<&str>) -> &str {
    chapter.unwrap_or("")
}

fn read(row: &Row) -> rusqlite::Result<AlbumDay> {
    let chapter: String = row.get(0)?;
    Ok(AlbumDay { chapter: Some(chapter).filter(|key| !key.is_empty()), day: row.get(1)?, text: row.get(2)?, changed: row.get(3)? })
}

pub fn kept_text(words: &str, what: &str) -> Result<String, String> {
    let words = markdown::clean(words);
    match words.chars().count() > TEXT_LIMIT {
        true => Err(format!("{what} is at most {TEXT_LIMIT} characters.")),
        false => Ok(words),
    }
}

pub fn joined(first: &str, second: &str) -> String {
    match (first.trim(), second.trim()) {
        (a, b) if b.is_empty() || a.contains(b) => a.to_string(),
        (a, b) if a.is_empty() || b.contains(a) => b.to_string(),
        (a, b) => format!("{a}\n\n{b}"),
    }
}

impl Catalog {
    fn album_known(&self, album: &str) -> Result<(), String> {
        let known: bool = self.home.query_row("SELECT EXISTS (SELECT 1 FROM albums WHERE key = ?1)", [album], |row| row.get(0)).map_err(text)?;
        if known {
            Ok(())
        } else {
            Err("There is no such album.".into())
        }
    }

    fn chapter_of_album(&self, album: &str, chapter: Option<&str>) -> Result<(), String> {
        match chapter {
            Some(key) if !self.chapter(key)?.is_some_and(|chapter| chapter.album == album) => Err("That chapter is not in this album.".into()),
            _ => Ok(()),
        }
    }

    pub fn album_text_kept(&self, album: &str) -> Result<Option<(String, i64)>, String> {
        self.home.query_row("SELECT text, changed FROM album_texts WHERE album = ?1", [album], |row| Ok((row.get(0)?, row.get(1)?))).optional().map_err(text)
    }

    pub fn album_text(&self, album: &str) -> Result<String, String> {
        Ok(self.album_text_kept(album)?.map(|(words, _)| words).unwrap_or_default())
    }

    pub fn set_album_text(&self, album: &str, words: &str) -> Result<(), String> {
        self.album_known(album)?;
        let words = kept_text(words, "An album's text")?;
        if self.album_text(album)? != words {
            self.put_album_text(album, &words, now())?;
        }
        Ok(())
    }

    pub fn put_album_text(&self, album: &str, words: &str, changed: i64) -> Result<(), String> {
        match words.trim().is_empty() {
            true => self.home.execute("DELETE FROM album_texts WHERE album = ?1", [album]),
            false => self.home.execute("INSERT OR REPLACE INTO album_texts (album, text, changed) VALUES (?1, ?2, ?3)", params![album, words, changed]),
        }
        .map_err(text)?;
        Ok(())
    }

    pub fn album_days(&self, album: &str) -> Result<Vec<AlbumDay>, String> {
        let mut statement = self.home.prepare("SELECT chapter, day, text, changed FROM album_days WHERE album = ?1 ORDER BY chapter, day").map_err(text)?;
        let rows = statement.query_map([album], read).map_err(text)?;
        rows.collect::<Result<_, _>>().map_err(text)
    }

    pub fn add_day(&self, album: &str, chapter: Option<&str>, day: i64) -> Result<(), String> {
        self.album_known(album)?;
        self.chapter_of_album(album, chapter)?;
        self.home
            .execute("INSERT OR IGNORE INTO album_days (album, chapter, day, text, changed) VALUES (?1, ?2, ?3, '', ?4)", params![album, stored(chapter), day, now()])
            .map_err(text)?;
        Ok(())
    }

    pub fn set_day_report(&self, album: &str, chapter: Option<&str>, day: i64, words: &str) -> Result<(), String> {
        self.album_known(album)?;
        self.chapter_of_album(album, chapter)?;
        let words = kept_text(words, "A day's report")?;
        let have: Option<String> = self
            .home
            .query_row("SELECT text FROM album_days WHERE album = ?1 AND chapter = ?2 AND day = ?3", params![album, stored(chapter), day], |row| row.get(0))
            .optional()
            .map_err(text)?;
        if have.as_deref() != Some(words.as_str()) {
            self.put_day(album, &AlbumDay { chapter: chapter.map(str::to_string), day, text: words, changed: now() })?;
        }
        Ok(())
    }

    pub fn remove_day(&self, album: &str, chapter: Option<&str>, day: i64) -> Result<(), String> {
        self.home.execute("DELETE FROM album_days WHERE album = ?1 AND chapter = ?2 AND day = ?3", params![album, stored(chapter), day]).map_err(text)?;
        Ok(())
    }

    fn put_day(&self, album: &str, day: &AlbumDay) -> Result<(), String> {
        self.home
            .execute(
                "INSERT OR REPLACE INTO album_days (album, chapter, day, text, changed) VALUES (?1, ?2, ?3, ?4, ?5)",
                params![album, stored(day.chapter.as_deref()), day.day, day.text, day.changed],
            )
            .map_err(text)?;
        Ok(())
    }

    pub fn put_album_days(&self, album: &str, days: &[AlbumDay]) -> Result<(), String> {
        let tx = self.home.unchecked_transaction().map_err(text)?;
        tx.execute("DELETE FROM album_days WHERE album = ?1", [album]).map_err(text)?;
        for day in days {
            self.put_day(album, day)?;
        }
        tx.commit().map_err(text)
    }

    fn join_day(&self, album: &str, day: &AlbumDay) -> Result<(), String> {
        let have = self.album_days(album)?.into_iter().find(|have| have.chapter == day.chapter && have.day == day.day);
        let day = match have {
            Some(have) => AlbumDay { text: joined(&have.text, &day.text), changed: have.changed.max(day.changed), ..have },
            None => day.clone(),
        };
        self.put_day(album, &day)
    }

    fn move_days(&self, from: &str, chapter: Option<&str>, to: &str, to_chapter: Option<&str>) -> Result<(), String> {
        for day in self.album_days(from)?.into_iter().filter(|day| day.chapter.as_deref() == chapter) {
            self.remove_day(from, chapter, day.day)?;
            self.join_day(to, &AlbumDay { chapter: to_chapter.map(str::to_string), ..day })?;
        }
        Ok(())
    }

    pub(super) fn days_out_of_chapter(&self, album: &str, chapter: &str) -> Result<(), String> {
        self.move_days(album, Some(chapter), album, None)
    }

    pub(super) fn rekey_days(&self, old: &str, new: &str) -> Result<(), String> {
        let Some(chapter) = self.chapter(new)? else { return Ok(()) };
        self.move_days(&chapter.album, Some(old), &chapter.album, Some(new))
    }

    pub(super) fn merge_texts(&self, keep: &str, other: &str) -> Result<(), String> {
        if let Some((words, changed)) = self.album_text_kept(other)? {
            let (kept, at) = self.album_text_kept(keep)?.unwrap_or_default();
            self.put_album_text(keep, &joined(&kept, &words), at.max(changed))?;
        }
        let chapters: Vec<Option<String>> = self.album_days(other)?.into_iter().map(|day| day.chapter).collect();
        let mut seen = std::collections::HashSet::new();
        for chapter in chapters.into_iter().filter(|chapter| seen.insert(chapter.clone())) {
            self.move_days(other, chapter.as_deref(), keep, chapter.as_deref())?;
        }
        self.forget_texts_of(other)
    }

    pub(super) fn forget_texts_of(&self, album: &str) -> Result<(), String> {
        self.home.execute("DELETE FROM album_texts WHERE album = ?1", [album]).map_err(text)?;
        self.home.execute("DELETE FROM album_days WHERE album = ?1", [album]).map_err(text)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::super::AlbumKind;
    use super::*;
    use std::path::PathBuf;

    fn catalog(tag: &str) -> (Catalog, PathBuf) {
        let dir = std::env::temp_dir().join(format!("numa-album-text-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        (Catalog::open(&dir.join("home.db")).unwrap(), dir)
    }

    #[test]
    fn an_albums_text_and_its_days_are_kept_as_the_subset_and_survive_reopening() {
        let (catalog, dir) = catalog("kept");
        let album = catalog.create_album("Baltic Trip").unwrap();
        let tallinn = catalog.add_chapter(&album, "Tallinn", None, None).unwrap().key;
        catalog.set_album_text(&album, "# Ten days\r\n\r\n* by **train**").unwrap();
        assert_eq!(catalog.album_text(&album).unwrap(), "Ten days\n\n- by **train**");
        let at = catalog.album_text_kept(&album).unwrap().unwrap().1;
        catalog.set_album_text(&album, "Ten days\n\n- by **train**").unwrap();
        assert_eq!(catalog.album_text_kept(&album).unwrap().unwrap().1, at, "the same words write nothing");

        catalog.set_day_report(&album, Some(&tallinn), 20_760, "The old town, *in rain*.").unwrap();
        catalog.add_day(&album, None, 20_765).unwrap();
        catalog.add_day(&album, Some(&tallinn), 20_760).unwrap();
        assert!(catalog.add_day(&album, Some("no-such-chapter"), 20_761).is_err());
        assert!(catalog.add_day("no-such-album", None, 20_761).is_err());
        assert!(catalog.set_day_report(&album, None, 1, &"x".repeat(TEXT_LIMIT + 1)).is_err(), "refused, not cut");
        catalog.set_day_report(&album, None, 20_765, "").unwrap();

        drop(catalog);
        let catalog = Catalog::open(&dir.join("home.db")).unwrap();
        let days = catalog.album_days(&album).unwrap();
        let said: Vec<(Option<&str>, i64, &str)> = days.iter().map(|day| (day.chapter.as_deref(), day.day, day.text.as_str())).collect();
        assert_eq!(said, [(None, 20_765, ""), (Some(tallinn.as_str()), 20_760, "The old town, *in rain*.")], "adding a day that is there keeps its words; an emptied written day stays");
        catalog.remove_day(&album, None, 20_765).unwrap();
        assert_eq!(catalog.album_days(&album).unwrap().len(), 1);
        catalog.set_album_text(&album, "  ").unwrap();
        assert_eq!(catalog.album_text_kept(&album).unwrap(), None);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn making_an_album_a_trip_a_wedding_or_an_album_again_keeps_every_text() {
        let (catalog, dir) = catalog("kinds");
        let album = catalog.create_album("Summer").unwrap();
        let chapter = catalog.add_chapter(&album, "Arrival", None, None).unwrap().key;
        catalog.set_album_text(&album, "All of **August**.").unwrap();
        catalog.set_chapter_words(&chapter, "The first *two* days.").unwrap();
        catalog.set_day_report(&album, Some(&chapter), 20_700, "Late train.").unwrap();
        catalog.add_day(&album, None, 20_710).unwrap();
        let before = (catalog.album_text(&album).unwrap(), catalog.chapter(&chapter).unwrap().unwrap().words, catalog.album_days(&album).unwrap());
        for kind in [AlbumKind::Trip, AlbumKind::Album, AlbumKind::Wedding, AlbumKind::Album] {
            catalog.set_album_kind(&album, kind).unwrap();
            let now = (catalog.album_text(&album).unwrap(), catalog.chapter(&chapter).unwrap().unwrap().words, catalog.album_days(&album).unwrap());
            assert_eq!(now, before, "{kind:?}");
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_words_go_with_a_chapter_that_goes_is_rekeyed_or_is_merged_and_go_with_a_deleted_album() {
        let (catalog, dir) = catalog("lifecycle");
        let album = catalog.create_album("Japan").unwrap();
        let (kyoto, nara) = (catalog.add_chapter(&album, "Kyoto", None, None).unwrap().key, catalog.add_chapter(&album, "Nara", None, None).unwrap().key);
        catalog.set_day_report(&album, Some(&kyoto), 20_550, "Temples.").unwrap();
        catalog.set_day_report(&album, None, 20_550, "Packed.").unwrap();
        catalog.set_day_report(&album, Some(&nara), 20_552, "Deer.").unwrap();

        catalog.remove_chapter(&kyoto).unwrap();
        let days = catalog.album_days(&album).unwrap();
        assert_eq!(days.iter().map(|day| (day.chapter.clone(), day.text.as_str())).collect::<Vec<_>>(), [(None, "Packed.\n\nTemples."), (Some(nara.clone()), "Deer.")]);

        catalog.rekey_chapter(&nara, "home-key").unwrap();
        assert_eq!(catalog.album_days(&album).unwrap()[1].chapter.as_deref(), Some("home-key"));

        let other = catalog.create_album("Japan at Home").unwrap();
        catalog.set_album_text(&album, "Ours.").unwrap();
        catalog.set_album_text(&other, "Theirs.").unwrap();
        catalog.set_day_report(&other, None, 20_550, "Also packed.").unwrap();
        catalog.add_day(&other, None, 20_560).unwrap();
        catalog.merge_albums(&album, &other).unwrap();
        assert_eq!(catalog.album_text(&album).unwrap(), "Ours.\n\nTheirs.");
        let days = catalog.album_days(&album).unwrap();
        assert_eq!(days.iter().map(|day| (day.day, day.text.as_str())).collect::<Vec<_>>(), [(20_550, "Packed.\n\nTemples.\n\nAlso packed."), (20_560, ""), (20_552, "Deer.")]);
        assert!(catalog.album_days(&other).unwrap().is_empty() && catalog.album_text_kept(&other).unwrap().is_none());

        catalog.delete_album(&album).unwrap();
        assert!(catalog.album_days(&album).unwrap().is_empty() && catalog.album_text_kept(&album).unwrap().is_none());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn texts_meet_without_saying_anything_twice() {
        assert_eq!(joined("A.", "B."), "A.\n\nB.");
        assert_eq!(joined("A. B.", "B."), "A. B.");
        assert_eq!(joined("", "B."), "B.");
        assert_eq!(joined("A.", " "), "A.");
    }
}
