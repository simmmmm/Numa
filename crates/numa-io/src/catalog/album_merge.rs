use rusqlite::params;

use super::{text, Catalog};

impl Catalog {

    pub fn merge_albums(&self, keep: &str, other: &str) -> Result<(), String> {
        let libraries = self.libraries()?;
        if let Some(away) = libraries.iter().find(|library| self.is_offline(library.id)) {
            return Err(format!("{} is not connected: connect it first, it may hold photographs of this album", away.label()));
        }
        for library in &libraries {
            let open = self.library(library.id)?;
            open.connected()?;

            open.conn.execute("UPDATE OR IGNORE album_photos SET album = ?1 WHERE album = ?2", params![keep, other]).map_err(text)?;
            open.conn.execute("DELETE FROM album_photos WHERE album = ?1", params![other]).map_err(text)?;
        }
        self.merge_chapters(keep, other)?;
        self.home.execute("DELETE FROM albums WHERE key = ?1", params![other]).map_err(text)?;
        Ok(())
    }
}
