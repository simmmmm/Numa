use super::*;

impl Catalog {

    pub fn take_place(&self, jpeg_id: i64, raw_id: i64, edit: Option<&Document>) -> Result<(), String> {
        let (from, jpeg) = self.photo(jpeg_id)?;
        let (to, raw) = self.photo(raw_id)?;
        from.connected()?;
        to.connected()?;
        let (rating, flag): (i64, i64) =
            from.conn.query_row("SELECT rating, flag FROM photos WHERE id = ?1", [jpeg], |row| Ok((row.get(0)?, row.get(1)?))).map_err(text)?;
        let albums: Vec<String> = from
            .conn
            .prepare("SELECT album FROM album_photos WHERE photo_id = ?1")
            .and_then(|mut statement| statement.query_map([jpeg], |row| row.get(0))?.collect())
            .map_err(text)?;
        let names: Vec<(String, Vec<u8>)> = from
            .conn
            .prepare("SELECT p.name, n.embedding FROM named_faces n JOIN people p ON p.id = n.person_id WHERE n.photo_id = ?1")
            .and_then(|mut statement| statement.query_map([jpeg], |row| Ok((row.get(0)?, row.get(1)?)))?.collect())
            .map_err(text)?;
        let position: Option<(f64, f64)> = from
            .conn
            .query_row("SELECT latitude, longitude FROM positions WHERE photo_id = ?1", [jpeg], |row| Ok((row.get(0)?, row.get(1)?)))
            .optional()
            .map_err(text)?;

        let tx = to.conn.unchecked_transaction().map_err(text)?;
        tx.execute("UPDATE photos SET rating = ?1 WHERE id = ?2 AND rating = 0", params![rating, raw]).map_err(text)?;
        tx.execute("UPDATE photos SET flag = ?1 WHERE id = ?2 AND flag = 0", params![flag, raw]).map_err(text)?;
        if let Some(edit) = edit.filter(|edit| !edit.is_untouched()) {
            let json = serde_json::to_string(edit).map_err(text)?;
            tx.execute("UPDATE photos SET edits = ?1 WHERE id = ?2 AND edits IS NULL", params![json, raw]).map_err(text)?;
        }
        for album in &albums {
            tx.execute("INSERT OR IGNORE INTO album_photos (album, photo_id) VALUES (?1, ?2)", params![album, raw]).map_err(text)?;
        }
        for (name, embedding) in &names {
            tx.execute("INSERT OR IGNORE INTO people (name) VALUES (?1)", params![name]).map_err(text)?;
            let person: i64 = tx.query_row("SELECT id FROM people WHERE name = ?1", params![name], |row| row.get(0)).map_err(text)?;
            tx.execute("INSERT INTO named_faces (photo_id, person_id, embedding) VALUES (?1, ?2, ?3)", params![raw, person, embedding])
                .map_err(text)?;
        }
        if let Some((latitude, longitude)) = position {
            tx.execute("INSERT OR IGNORE INTO positions VALUES (?1, ?2, ?3)", params![raw, latitude, longitude]).map_err(text)?;
        }
        tx.commit().map_err(text)?;

        let tx = from.conn.unchecked_transaction().map_err(text)?;
        tx.execute("UPDATE photos SET rating = 0, flag = 0 WHERE id = ?1", [jpeg]).map_err(text)?;
        tx.execute("DELETE FROM album_photos WHERE photo_id = ?1", [jpeg]).map_err(text)?;
        tx.commit().map_err(text)
    }

    pub fn positions(&self, photo_ids: &[i64]) -> Result<HashMap<i64, (f64, f64)>, String> {
        let mut found = HashMap::new();
        for &id in photo_ids {
            let (open, local) = self.photo(id)?;
            let position = open
                .conn
                .prepare_cached("SELECT latitude, longitude FROM positions WHERE photo_id = ?1")
                .and_then(|mut statement| statement.query_row([local], |row| Ok((row.get(0)?, row.get(1)?))).optional())
                .map_err(text)?;
            if let Some(position) = position {
                found.insert(id, position);
            }
        }
        Ok(found)
    }

    pub fn set_positions(&self, placed: &[(i64, f64, f64)]) -> Result<(), String> {
        self.each_library(placed, |tx, (local, latitude, longitude)| {
            tx.execute("INSERT OR REPLACE INTO positions VALUES (?1, ?2, ?3)", params![local, latitude, longitude]).map(|_| ())
        })
    }

    pub fn clear_positions(&self, photo_ids: &[i64]) -> Result<(), String> {
        let rows: Vec<(i64, f64, f64)> = photo_ids.iter().map(|&id| (id, 0.0, 0.0)).collect();
        self.each_library(&rows, |tx, (local, _, _)| tx.execute("DELETE FROM positions WHERE photo_id = ?1", [local]).map(|_| ()))
    }

    fn each_library(&self, rows: &[(i64, f64, f64)], write: impl Fn(&rusqlite::Transaction, (i64, f64, f64)) -> rusqlite::Result<()>) -> Result<(), String> {
        let mut by_library: HashMap<i64, Vec<(i64, f64, f64)>> = HashMap::new();
        for &(id, latitude, longitude) in rows {
            let (library, local) = split_id(id);
            by_library.entry(library).or_default().push((local, latitude, longitude));
        }
        for (library, rows) in by_library {
            let open = self.library(library)?;
            open.connected()?;
            let tx = open.conn.unchecked_transaction().map_err(text)?;
            for row in rows {
                write(&tx, row).map_err(text)?;
            }
            tx.commit().map_err(text)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn catalog_with(names: &[&str]) -> (Catalog, Library, PathBuf) {
        let dir = std::env::temp_dir().join(format!("numa-after-camera-{}-{}", std::process::id(), names.len()));
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
    fn a_raw_takes_the_place_of_its_jpeg() {
        let (catalog, library, dir) = catalog_with(&["DSCF0001.JPG", "DSCF0001.RAF"]);
        let (jpeg, raw) = (id_of(&catalog, &library, "DSCF0001.JPG"), id_of(&catalog, &library, "DSCF0001.RAF"));
        catalog.set_rating(jpeg, 4).unwrap();
        catalog.set_flag(jpeg, Flag::Picked).unwrap();
        let album = catalog.create_album("Tonight").unwrap();
        catalog.set_in_album(&album, &[jpeg], true).unwrap();
        catalog.set_positions(&[(jpeg, 35.0, 135.0)]).unwrap();
        let mut tone = Document::new("DSCF0001.JPG".into());
        tone.set_crop([0.1, 0.1, 0.5, 0.5], 0.0);
        catalog.save_edits(jpeg, &tone).unwrap();
        let mut edit = Document::new("DSCF0001.RAF".into());
        edit.set_crop([0.1, 0.1, 0.5, 0.5], 0.0);

        catalog.take_place(jpeg, raw, Some(&edit)).unwrap();

        let photos = catalog.photos(library.id, &Filter::default()).unwrap();
        assert_eq!(photos.len(), 2, "the JPEG stays in view");
        let of = |name: &str| photos.iter().find(|photo| photo.path.ends_with(name)).unwrap();
        assert_eq!((of("DSCF0001.RAF").rating, of("DSCF0001.RAF").flag, of("DSCF0001.RAF").edited), (4, Flag::Picked, true));
        assert_eq!((of("DSCF0001.JPG").rating, of("DSCF0001.JPG").flag, of("DSCF0001.JPG").edited), (0, Flag::None, true));
        assert_eq!(catalog.load_edits(raw).unwrap().and_then(|edit| edit.crop()), edit.crop());
        assert_eq!(catalog.positions(&[raw, jpeg]).unwrap().len(), 2);
        let in_album = catalog.photos(library.id, &Filter { album: Some(album), ..Filter::default() }).unwrap();
        assert_eq!(in_album.iter().map(|photo| photo.id).collect::<Vec<_>>(), vec![raw]);
        let raws = catalog.photos(library.id, &Filter { file_type: FileType::Raw, ..Filter::default() }).unwrap();
        assert_eq!(raws.len(), 1, "RAW Only shows the pair as one");
        catalog.sync_library(&library).unwrap();
        assert_eq!(catalog.photos(library.id, &Filter::default()).unwrap().len(), 2);

        catalog.clear_positions(&[raw]).unwrap();
        assert!(!catalog.positions(&[raw]).unwrap().contains_key(&raw));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_raws_own_marks_win() {
        let (catalog, library, dir) = catalog_with(&["A.JPG", "A.RAF", "B.TXT"]);
        let (jpeg, raw) = (id_of(&catalog, &library, "A.JPG"), id_of(&catalog, &library, "A.RAF"));
        catalog.set_rating(jpeg, 2).unwrap();
        catalog.set_rating(raw, 5).unwrap();
        catalog.take_place(jpeg, raw, None).unwrap();
        let photos = catalog.photos(library.id, &Filter { file_type: FileType::Raw, ..Filter::default() }).unwrap();
        assert_eq!((photos.len(), photos[0].rating, photos[0].edited), (1, 5, false));
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
