use std::collections::HashMap;

use rusqlite::{params, OptionalExtension};

use super::{global_id, text, Catalog};

pub const CAPTION_LIMIT: usize = 2000;

pub fn tidy_caption(caption: &str) -> String {
    caption.replace("\r\n", "\n").replace('\r', "\n").trim().to_string()
}

fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |since| since.as_secs() as i64)
}

impl Catalog {

    pub fn caption(&self, photo_id: i64) -> Result<Option<String>, String> {
        let (open, local) = self.photo(photo_id)?;
        open.conn.query_row("SELECT text FROM captions WHERE photo_id = ?1", [local], |row| row.get(0)).optional().map_err(text)
    }

    pub fn set_caption(&self, photo_id: i64, caption: &str) -> Result<(), String> {
        self.write_caption(photo_id, caption, now())
    }

    fn write_caption(&self, photo_id: i64, caption: &str, at: i64) -> Result<(), String> {
        let caption = tidy_caption(caption);
        if caption.chars().count() > CAPTION_LIMIT {
            return Err(format!("A caption is at most {CAPTION_LIMIT} characters."));
        }
        let (open, local) = self.photo(photo_id)?;
        open.connected()?;
        if self.caption(photo_id)?.unwrap_or_default() == caption {
            return Ok(());
        }
        match caption.is_empty() {
            true => open.conn.execute("DELETE FROM captions WHERE photo_id = ?1", [local]),

            false => open.conn.execute(
                "INSERT OR REPLACE INTO captions (photo_id, text, changed) SELECT ?1, ?2, ?3 WHERE EXISTS (SELECT 1 FROM photos WHERE id = ?1)",
                params![local, caption, at],
            ),
        }
        .map(|_| ())
        .map_err(text)
    }

    pub fn captions_of(&self, photo_ids: &[i64]) -> Result<HashMap<i64, String>, String> {
        let mut found = HashMap::new();
        for &id in photo_ids {
            let (open, local) = self.photo(id)?;
            let caption: Option<String> = open
                .conn
                .prepare_cached("SELECT text FROM captions WHERE photo_id = ?1")
                .and_then(|mut statement| statement.query_row([local], |row| row.get(0)).optional())
                .map_err(text)?;
            found.extend(caption.map(|caption| (id, caption)));
        }
        Ok(found)
    }

    pub fn captions(&self, library_id: i64) -> Result<HashMap<i64, (String, i64)>, String> {
        let open = self.library(library_id)?;
        let mut statement = open.conn.prepare("SELECT photo_id, text, changed FROM captions").map_err(text)?;
        let rows = statement.query_map([], |row| Ok((global_id(library_id, row.get(0)?), (row.get(1)?, row.get(2)?)))).map_err(text)?;
        rows.collect::<Result<_, _>>().map_err(text)
    }
}

pub fn caption_says(caption: &str, words: &str) -> bool {
    let caption = caption.to_lowercase();
    let mut words = words.split_whitespace().map(str::to_lowercase).peekable();
    words.peek().is_some() && words.all(|word| caption.contains(&word))
}

#[cfg(test)]
mod tests {
    use super::super::{Filter, Library};
    use super::*;
    use std::path::PathBuf;

    fn catalog_with(tag: &str, names: &[&str]) -> (Catalog, Library, PathBuf) {
        let dir = std::env::temp_dir().join(format!("numa-caption-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("lib")).unwrap();
        for name in names {
            std::fs::write(dir.join("lib").join(name), b"not really a photograph").unwrap();
        }
        let catalog = Catalog::open(&dir.join("home.db")).unwrap();
        let library = catalog.add_library(&dir.join("lib")).unwrap();
        catalog.sync_library(&library).unwrap();
        (catalog, library, dir)
    }

    fn id_of(catalog: &Catalog, library: &Library, name: &str) -> i64 {
        let photos = catalog.photos(library.id, &Filter::default()).unwrap();
        photos.iter().find(|photo| photo.path.ends_with(name)).map(|photo| photo.id).expect(name)
    }

    #[test]
    fn a_caption_is_kept_with_its_photograph() {
        let (catalog, library, dir) = catalog_with("kept", &["A.JPG", "B.JPG"]);
        let (a, b) = (id_of(&catalog, &library, "A.JPG"), id_of(&catalog, &library, "B.JPG"));
        assert_eq!(catalog.caption(a).unwrap(), None);
        catalog.set_caption(a, "  From the roof,\r\nthe first evening. \n").unwrap();

        drop(catalog);
        let catalog = Catalog::open(&dir.join("home.db")).unwrap();
        assert_eq!(catalog.caption(a).unwrap().as_deref(), Some("From the roof,\nthe first evening."));
        assert_eq!(catalog.captions_of(&[a, b]).unwrap(), HashMap::from([(a, "From the roof,\nthe first evening.".to_string())]));
        assert!(dir.join("lib/.numa/catalog.db").is_file(), "in the library's own folder");

        assert_eq!(catalog.descriptions(&[a, b]).unwrap()[&a].caption.as_deref(), Some("From the roof,\nthe first evening."));
        assert_eq!(catalog.descriptions(&[b]).unwrap()[&b].caption, None);

        catalog.set_caption(a, "   ").unwrap();
        assert_eq!(catalog.caption(a).unwrap(), None);
        assert!(catalog.captions(library.id).unwrap().is_empty());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_same_words_again_change_nothing() {
        let (catalog, library, dir) = catalog_with("time", &["A.JPG"]);
        let a = id_of(&catalog, &library, "A.JPG");
        catalog.write_caption(a, "The ferry to the island.", 100).unwrap();
        catalog.write_caption(a, " The ferry to the island. ", 200).unwrap();
        assert_eq!(catalog.captions(library.id).unwrap()[&a], ("The ferry to the island.".to_string(), 100));
        catalog.write_caption(a, "The last ferry to the island.", 300).unwrap();
        assert_eq!(catalog.captions(library.id).unwrap()[&a].1, 300);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_caption_too_long_is_refused_whole() {
        let (catalog, library, dir) = catalog_with("long", &["A.JPG"]);
        let a = id_of(&catalog, &library, "A.JPG");
        catalog.set_caption(a, "Short.").unwrap();
        assert!(catalog.set_caption(a, &"é".repeat(CAPTION_LIMIT)).is_ok(), "the limit counts characters, not bytes");
        assert!(catalog.set_caption(a, &"a".repeat(CAPTION_LIMIT + 1)).is_err());
        assert_eq!(catalog.caption(a).unwrap().map(|caption| caption.chars().count()), Some(CAPTION_LIMIT));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_caption_goes_with_its_photograph() {
        let (catalog, library, dir) = catalog_with("gone", &["A.JPG", "B.JPG"]);
        let (a, b) = (id_of(&catalog, &library, "A.JPG"), id_of(&catalog, &library, "B.JPG"));
        catalog.set_caption(a, "Gone tomorrow.").unwrap();
        catalog.set_caption(b, "Stays.").unwrap();
        std::fs::remove_file(dir.join("lib/A.JPG")).unwrap();
        catalog.sync_library(&library).unwrap();
        assert_eq!(catalog.captions(library.id).unwrap().into_keys().collect::<Vec<_>>(), [b]);
        catalog.set_caption(a, "For nobody.").unwrap();
        assert_eq!(catalog.captions(library.id).unwrap().len(), 1);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_search_reads_every_word_whatever_its_case() {
        assert!(caption_says("From the hotel roof, the first evening.", "ROOF evening"));
        assert!(!caption_says("From the hotel roof, the first evening.", "roof morning"));
        assert!(!caption_says("From the hotel roof.", "  "));
    }
}
