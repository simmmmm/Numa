use std::collections::HashMap;

use rusqlite::params;

use super::{text, Catalog, Error, Named};

impl Catalog {

    pub fn albums(&self) -> Result<Vec<(String, String)>, String> {
        let mut statement =
            self.home.prepare("SELECT key, name FROM albums ORDER BY name COLLATE NOCASE").map_err(text)?;
        let rows = statement.query_map([], |row| Ok((row.get(0)?, row.get(1)?))).map_err(text)?;
        rows.collect::<Result<_, _>>().map_err(text)
    }

    pub fn album_sizes(&self) -> Result<HashMap<String, (u32, u32)>, String> {
        let mut sizes: HashMap<String, (u32, u32)> = HashMap::new();
        for library in self.libraries()? {
            let Ok(open) = self.library(library.id) else { continue };
            let mut statement = open
                .conn
                .prepare("SELECT album, COUNT(*) FROM album_photos JOIN photos ON photos.id = album_photos.photo_id GROUP BY album")
                .map_err(text)?;
            let rows = statement.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, u32>(1)?))).map_err(text)?;
            for row in rows {
                let (album, photos) = row.map_err(text)?;
                sizes.entry(album).or_default().0 += photos;
            }
        }
        let mut statement = self.home.prepare("SELECT album, COUNT(*) FROM album_chapters GROUP BY album").map_err(text)?;
        let rows = statement.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, u32>(1)?))).map_err(text)?;
        for row in rows {
            let (album, chapters) = row.map_err(text)?;
            sizes.entry(album).or_default().1 = chapters;
        }
        Ok(sizes)
    }

    pub fn create_album(&self, name: &str) -> Result<String, Error> {
        let name = album_name(name)?;

        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_err(text)?.as_nanos();
        let key = format!("{nanos:x}");
        self.home
            .execute("INSERT INTO albums (key, name) VALUES (?1, ?2)", params![key, name])
            .map_err(|err| album_taken(err, name))?;
        Ok(key)
    }

    pub fn rename_album(&self, key: &str, name: &str) -> Result<(), Error> {
        let name = album_name(name)?;
        self.home
            .execute("UPDATE albums SET name = ?1 WHERE key = ?2", params![name, key])
            .map_err(|err| album_taken(err, name))?;
        Ok(())
    }

    pub fn delete_album(&self, key: &str) -> Result<(), String> {
        for library in self.libraries()? {
            let Ok(open) = self.library(library.id) else { continue };
            open.conn.execute("DELETE FROM album_photos WHERE album = ?1", params![key]).map_err(text)?;
        }
        self.forget_chapters_of(key)?;
        self.forget_texts_of(key)?;
        self.home.execute("DELETE FROM albums WHERE key = ?1", params![key]).map_err(text)?;
        Ok(())
    }

    pub fn set_in_album(&self, key: &str, photo_ids: &[i64], inside: bool) -> Result<(), String> {
        let sql = if inside {
            "INSERT OR IGNORE INTO album_photos (album, photo_id) VALUES (?1, ?2)"
        } else {
            "DELETE FROM album_photos WHERE album = ?1 AND photo_id = ?2"
        };
        self.update_each(photo_ids, sql, key, None)?;
        if inside {
            return Ok(());
        }
        self.leave_chapters_of(key, photo_ids)
    }
}

fn album_name(name: &str) -> Result<&str, Error> {
    match name.trim() {
        "" => Err(Error::NeedsAName { of: Named::Album }),
        name => Ok(name),
    }
}

fn album_taken(err: rusqlite::Error, name: &str) -> Error {
    match err.sqlite_error_code() {
        Some(rusqlite::ErrorCode::ConstraintViolation) => Error::AlbumExists { name: name.to_string() },
        _ => err.to_string().into(),
    }
}
