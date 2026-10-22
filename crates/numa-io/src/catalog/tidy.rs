use std::collections::HashSet;

use rusqlite::params;

use super::Catalog;
use crate::tidy::{Known, Ledger};

impl Ledger for Catalog {
    fn settled(&self) -> HashSet<String> {
        let read = || -> rusqlite::Result<HashSet<String>> {
            let mut statement = self.home.prepare("SELECT key FROM tidy_settled")?;
            let rows = statement.query_map([], |row| row.get(0))?;
            rows.collect()
        };
        read().unwrap_or_default()
    }

    fn settle(&self, keys: &[String]) {
        for key in keys {
            if let Err(err) = self.home.execute("INSERT OR IGNORE INTO tidy_settled (key) VALUES (?1)", [key]) {
                log::warn!("tidy: {err}");
            }
        }
    }

    fn hashes(&self) -> Known {
        let read = || -> rusqlite::Result<Known> {
            let mut statement = self.home.prepare("SELECT path, mtime, size, hash FROM tidy_hashes")?;
            let rows = statement.query_map([], |row| Ok((row.get(0)?, (row.get(1)?, row.get::<_, i64>(2)? as u64, row.get(3)?))))?;
            rows.collect()
        };
        read().unwrap_or_default()
    }

    fn note_hash(&self, path: &str, mtime: i64, size: u64, hash: &str) {
        let sql = "INSERT OR REPLACE INTO tidy_hashes (path, mtime, size, hash) VALUES (?1, ?2, ?3, ?4)";
        if let Err(err) = self.home.execute(sql, params![path, mtime, size as i64, hash]) {
            log::warn!("tidy: {err}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_settled_set_and_a_hash_are_kept() {
        let catalog = Catalog::in_memory().unwrap();
        catalog.settle(&["bursts:1:a.raf".into(), "bursts:1:a.raf".into()]);
        assert_eq!(catalog.settled(), HashSet::from(["bursts:1:a.raf".to_string()]));
        catalog.note_hash("/p/a.raf", 5, 9, "ff");
        catalog.note_hash("/p/a.raf", 6, 9, "ee");
        assert_eq!(catalog.hashes().get("/p/a.raf"), Some(&(6, 9, "ee".to_string())));
    }
}
