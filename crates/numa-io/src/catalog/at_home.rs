use std::collections::HashMap;
use std::path::{Path, PathBuf};

use rusqlite::params;

use super::{clocks, global_id, text, Catalog, Flag, OpenLibrary, LIBRARY_DIR};

#[derive(Clone, Debug, PartialEq)]
pub struct AtHome {

    pub path: String,

    pub id: String,

    pub size: u64,
    pub mtime: i64,
    pub taken: Option<i64>,
    pub rating: u8,
    pub flag: Flag,
}

pub fn home_thumb(root: &Path, id: &str) -> PathBuf {
    root.join(LIBRARY_DIR).join("home").join(format!("{id}.jpg"))
}

pub fn home_preview(root: &Path, id: &str) -> PathBuf {
    let text = format!("{}\0{id}", root.display());
    let hash = text.bytes().fold(0xcbf2_9ce4_8422_2325u64, |hash, byte| (hash ^ byte as u64).wrapping_mul(0x100_0000_01b3));
    crate::thumbs::cache_dir().join(format!("home-{hash:016x}.jpg"))
}

pub(super) fn stand_in_all(open: &OpenLibrary) {
    let Ok(mut stmt) = open.conn.prepare("SELECT p.path, h.id FROM at_home h JOIN photos p ON p.id = h.photo_id") else { return };
    let Ok(rows) = stmt.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))) else { return };
    for (path, id) in rows.flatten() {
        crate::thumbs::stand_in(open.absolute(&path), home_thumb(&open.root, &id));
    }
}

pub(super) fn found_here(open: &OpenLibrary, tx: &rusqlite::Transaction, paths: &std::collections::HashSet<String>) -> Result<(), String> {
    let homes: Vec<(i64, String, String)> = {
        let mut stmt = tx.prepare("SELECT h.photo_id, p.path, h.id FROM at_home h JOIN photos p ON p.id = h.photo_id").map_err(text)?;
        let rows = stmt.query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?))).map_err(text)?;
        rows.flatten().filter(|(_, path, _)| paths.contains(path)).collect()
    };
    for (local, path, id) in homes {
        tx.execute("DELETE FROM at_home WHERE photo_id = ?1", params![local]).map_err(text)?;
        crate::thumbs::forget_stand_in(&open.absolute(&path));
        let _ = std::fs::remove_file(home_thumb(&open.root, &id));
        let _ = std::fs::remove_file(home_preview(&open.root, &id));
    }
    Ok(())
}

impl Catalog {

    pub fn add_at_home(&self, library_id: i64, photos: &[AtHome]) -> Result<usize, String> {
        let open = self.library(library_id)?;
        open.connected()?;
        let tx = open.conn.unchecked_transaction().map_err(text)?;
        let mut added = 0;
        for photo in photos {
            let inserted = tx
                .execute(
                    "INSERT OR IGNORE INTO photos (path, mtime, taken, rating, flag) VALUES (?1, ?2, ?3, ?4, ?5)",
                    params![photo.path, photo.mtime, photo.taken, photo.rating.min(5), photo.flag.to_i64()],
                )
                .map_err(text)?;
            if inserted == 0 {
                continue;
            }
            let local = tx.last_insert_rowid();
            tx.execute("INSERT INTO at_home (photo_id, id, size) VALUES (?1, ?2, ?3)", params![local, photo.id, photo.size as i64])
                .map_err(text)?;
            crate::thumbs::stand_in(open.absolute(&photo.path), home_thumb(&open.root, &photo.id));
            added += 1;
        }
        tx.commit().map_err(text)?;
        Ok(added)
    }

    pub fn at_home(&self, library_id: i64) -> Result<HashMap<i64, (String, u64)>, String> {
        let open = self.library(library_id)?;
        let mut stmt = open.conn.prepare("SELECT photo_id, id, size FROM at_home").map_err(text)?;
        let rows = stmt
            .query_map([], |row| Ok((global_id(library_id, row.get(0)?), (row.get::<_, String>(1)?, row.get::<_, i64>(2)?.max(0) as u64))))
            .map_err(text)?;
        rows.collect::<Result<_, _>>().map_err(text)
    }

    pub fn arrived(&self, photo_id: i64, mtime: i64, stamp: &crate::exif::Stamp) -> Result<(), String> {
        let (open, local) = self.photo(photo_id)?;
        open.connected()?;
        let path = open
            .conn
            .query_row("SELECT path FROM photos WHERE id = ?1", params![local], |row| row.get::<_, String>(0))
            .map_err(text)?;
        let tx = open.conn.unchecked_transaction().map_err(text)?;
        tx.execute(
            &format!("UPDATE photos SET mtime = ?1, camera_time = ?2, camera = ?4, taken = ?2 + {} WHERE id = ?3", clocks::OFFSET),
            params![mtime, stamp.taken, local, stamp.camera],
        )
        .map_err(text)?;
        found_here(&open, &tx, &[path].into_iter().collect())?;
        tx.commit().map_err(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::Filter;

    fn at_home(path: &str, id: &str) -> AtHome {
        AtHome { path: path.into(), id: id.into(), size: 3, mtime: 1_700_000_000, taken: Some(1_600_000_000), rating: 4, flag: Flag::Picked }
    }

    #[test]
    fn a_photograph_at_home_is_a_row_the_scan_keeps_until_its_file_is_here() {
        numa_core::paths::use_test_cache(std::env::temp_dir().join(format!("numa-thumbs-test-cache-{}", std::process::id())));
        let root = std::env::temp_dir().join(format!("numa-test-at-home-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        std::fs::write(root.join("here.RAF"), b"x").unwrap();
        let catalog = Catalog::in_memory().unwrap();
        let library = catalog.add_library(&root).unwrap();
        catalog.sync_library(&library).unwrap();

        assert_eq!(catalog.add_at_home(library.id, &[at_home("2026/away.RAF", "7"), at_home("here.RAF", "8")]).unwrap(), 1, "a row already here stays");
        let photos = catalog.photos(library.id, &Filter::default()).unwrap();
        assert_eq!(photos.len(), 2);
        let away = photos.iter().find(|photo| photo.path.ends_with("away.RAF")).unwrap().clone();
        assert_eq!((away.rating, away.flag, away.taken), (4, Flag::Picked, Some(1_600_000_000)));
        assert_eq!(catalog.at_home(library.id).unwrap(), HashMap::from([(away.id, ("7".to_string(), 3))]));
        catalog.set_rating(away.id, 2).unwrap();

        let thumb = home_thumb(&root, "7");
        std::fs::create_dir_all(thumb.parent().unwrap()).unwrap();
        image::RgbImage::new(4, 3).save_with_format(&thumb, image::ImageFormat::Jpeg).unwrap();
        assert_eq!(crate::thumbs::cached(&away.path, away.mtime, 640, None), Some(thumb.clone()));

        assert_eq!(catalog.sync_library(&library).unwrap(), 0);
        assert!(!catalog.known_files(&library).unwrap().contains_key(&away.path));
        assert_eq!(catalog.photos(library.id, &Filter::default()).unwrap().len(), 2);

        std::fs::create_dir_all(away.path.parent().unwrap()).unwrap();
        std::fs::write(&away.path, b"raw").unwrap();
        catalog.sync_library(&library).unwrap();
        assert!(catalog.at_home(library.id).unwrap().is_empty());
        assert!(!thumb.exists(), "Home's thumbnail is let go");
        assert_eq!(crate::thumbs::cached(&away.path, away.mtime, 640, None), None);
        let photos = catalog.photos(library.id, &Filter::default()).unwrap();
        assert_eq!(photos.iter().find(|photo| photo.id == away.id).map(|photo| photo.rating), Some(2));

        catalog.add_at_home(library.id, &[at_home("b.RAF", "9")]).unwrap();
        let b = catalog.photos(library.id, &Filter::default()).unwrap().into_iter().find(|photo| photo.path.ends_with("b.RAF")).unwrap();
        std::fs::write(&b.path, b"raw").unwrap();
        let mtime = std::fs::metadata(&b.path).unwrap().modified().unwrap().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs() as i64;
        catalog.arrived(b.id, mtime, &crate::exif::stamp(&b.path)).unwrap();
        assert!(catalog.at_home(library.id).unwrap().is_empty());
        let known = catalog.known_files(&library).unwrap();
        assert!(crate::catalog::scan(&root, &known).unchanged);
        drop(catalog);
        std::fs::remove_dir_all(&root).unwrap();
    }
}
