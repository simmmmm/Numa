use super::*;

pub(super) const OFFSET: &str =
    "IFNULL((SELECT seconds FROM clocks WHERE clocks.camera = ?4 AND applied), 0)";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Clock {
    pub seconds: i64,
    pub applied: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Camera {

    pub key: String,
    pub photos: i64,
    pub clock: Option<Clock>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PhotoClock {
    pub camera: String,

    pub own: Option<i64>,
    pub taken: Option<i64>,

    pub slate: Option<i64>,
}

pub(super) fn migrate(conn: &Connection) -> Result<(), String> {
    let has_camera = conn
        .prepare("SELECT * FROM photos LIMIT 0")
        .map(|statement| statement.column_names().contains(&"camera"))
        .unwrap_or(true);
    if !has_camera {
        conn.execute_batch(
            "BEGIN; \
             ALTER TABLE photos ADD COLUMN camera TEXT; \
             ALTER TABLE photos ADD COLUMN camera_time INTEGER; \
             UPDATE photos SET camera_time = taken; \
             COMMIT;",
        )
        .map_err(text)?;
    }
    Ok(())
}

impl Catalog {

    pub fn cameras(&self, library_id: i64) -> Result<Vec<Camera>, String> {
        let open = self.library(library_id)?;
        open.connected()?;
        let mut stmt = open
            .conn
            .prepare(
                "SELECT p.camera, COUNT(*), c.seconds, c.applied \
                 FROM photos p LEFT JOIN clocks c ON c.camera = p.camera \
                 WHERE p.camera <> '' GROUP BY p.camera ORDER BY COUNT(*) DESC, p.camera",
            )
            .map_err(text)?;
        let rows = stmt
            .query_map([], |row| {
                let clock = match row.get::<_, Option<i64>>(2)? {
                    Some(seconds) => Some(Clock { seconds, applied: row.get::<_, i64>(3)? != 0 }),
                    None => None,
                };
                Ok(Camera { key: row.get(0)?, photos: row.get(1)?, clock })
            })
            .map_err(text)?;
        rows.collect::<Result<_, _>>().map_err(text)
    }

    pub fn set_clock(&self, library_id: i64, camera: &str, clock: Option<Clock>) -> Result<(), String> {
        let open = self.library(library_id)?;
        open.connected()?;
        let tx = open.conn.unchecked_transaction().map_err(text)?;
        match clock {
            Some(clock) => tx.execute(
                "INSERT OR REPLACE INTO clocks (camera, seconds, applied) VALUES (?1, ?2, ?3)",
                params![camera, clock.seconds, clock.applied],
            ),
            None => tx.execute("DELETE FROM clocks WHERE camera = ?1", params![camera]),
        }
        .map_err(text)?;
        let shift = clock.filter(|clock| clock.applied).map_or(0, |clock| clock.seconds);
        tx.execute("UPDATE photos SET taken = camera_time + ?1 WHERE camera = ?2", params![shift, camera])
            .map_err(text)?;
        tx.commit().map_err(text)
    }

    pub fn clock_of(&self, photo_id: i64) -> Option<PhotoClock> {
        let (open, local) = self.photo(photo_id).ok()?;
        open.connected().ok()?;
        open.conn
            .query_row(
                "SELECT p.camera, p.camera_time, p.taken, s.shown \
                 FROM photos p LEFT JOIN slates s ON s.photo_id = p.id WHERE p.id = ?1",
                params![local],
                |row| {
                    Ok(PhotoClock {
                        camera: row.get::<_, Option<String>>(0)?.unwrap_or_default(),
                        own: row.get(1)?,
                        taken: row.get(2)?,
                        slate: row.get(3)?,
                    })
                },
            )
            .ok()
    }

    pub fn bodies(&self, library_id: i64) -> Result<HashMap<i64, String>, String> {
        let open = self.library(library_id)?;
        open.connected()?;
        let mut stmt = open.conn.prepare("SELECT id, COALESCE(camera, '') FROM photos").map_err(text)?;
        let rows = stmt
            .query_map([], |row| Ok((global_id(library_id, row.get(0)?), row.get::<_, String>(1)?)))
            .map_err(text)?;
        rows.collect::<Result<_, _>>().map_err(text)
    }

    pub fn file_of(&self, photo_id: i64) -> Option<(PathBuf, i64)> {
        let (open, local) = self.photo(photo_id).ok()?;
        open.conn
            .query_row("SELECT path, mtime FROM photos WHERE id = ?1", params![local], |row| {
                Ok((open.absolute(&row.get::<_, String>(0)?), row.get(1)?))
            })
            .ok()
    }

    pub fn slates(&self, library_id: i64) -> Result<Vec<(i64, String, i64, i64)>, String> {
        let open = self.library(library_id)?;
        open.connected()?;
        let mut stmt = open
            .conn
            .prepare(
                "SELECT s.photo_id, p.camera, p.camera_time, s.shown FROM slates s JOIN photos p ON p.id = s.photo_id \
                 WHERE p.camera <> '' AND p.camera_time IS NOT NULL ORDER BY p.camera_time",
            )
            .map_err(text)?;
        let rows = stmt
            .query_map([], |row| Ok((global_id(library_id, row.get(0)?), row.get(1)?, row.get(2)?, row.get(3)?)))
            .map_err(text)?;
        rows.collect::<Result<_, _>>().map_err(text)
    }

    pub fn moments(&self, library_id: i64) -> Result<Vec<crate::clocks::Moment>, String> {
        let open = self.library(library_id)?;
        open.connected()?;
        let mut stmt = open
            .conn
            .prepare(&format!(
                "SELECT p.id, p.camera, p.taken, p.camera_time, a.hash, COALESCE(a.shape, 0) \
                 FROM photos p JOIN analysis a ON a.photo_id = p.id \
                 WHERE p.camera <> '' AND p.camera_time IS NOT NULL AND p.taken IS NOT NULL \
                   AND COALESCE(a.contrast, 1) >= {} AND p.id NOT IN (SELECT photo_id FROM slates) \
                 ORDER BY p.taken",
                numa_cull::BLANK
            ))
            .map_err(text)?;
        let rows = stmt
            .query_map([], |row| {
                Ok(crate::clocks::Moment {
                    id: global_id(library_id, row.get(0)?),
                    camera: row.get(1)?,
                    taken: row.get(2)?,
                    own: row.get(3)?,
                    hash: row.get::<_, i64>(4)? as u64,
                    shape: row.get::<_, i64>(5)? as u64,
                })
            })
            .map_err(text)?;
        rows.collect::<Result<_, _>>().map_err(text)
    }

    pub fn declined_clocks(&self, library_id: i64) -> HashMap<String, i64> {
        let Ok(open) = self.library(library_id) else { return HashMap::new() };
        let Ok(mut stmt) = open.conn.prepare("SELECT key, value FROM settings WHERE key LIKE 'clock_declined:%'") else {
            return HashMap::new();
        };
        let rows = stmt.query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)));
        rows.map(|rows| {
            rows.flatten()
                .filter_map(|(key, value)| Some((key.strip_prefix("clock_declined:")?.to_string(), value.parse().ok()?)))
                .collect()
        })
        .unwrap_or_default()
    }

    pub fn decline_clock(&self, library_id: i64, camera: &str, seconds: i64) -> Result<(), String> {
        let open = self.library(library_id)?;
        open.connected()?;
        open.conn
            .execute(
                "INSERT INTO settings (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                params![format!("clock_declined:{camera}"), seconds.to_string()],
            )
            .map_err(text)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exif::Stamp;

    #[test]
    fn a_clock_lined_up_moves_its_frames_and_the_ones_still_to_come() {
        let root = std::env::temp_dir().join(format!("numa-test-clocks-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        for name in ["a.jpg", "b.jpg", "c.jpg"] {
            std::fs::write(root.join(name), b"x").unwrap();
        }
        let catalog = Catalog::in_memory().unwrap();
        let library = catalog.add_library(&root).unwrap();
        let (fuji, sony) = ("FUJIFILM|X-T5|1", "SONY|ILCE-7M4|");
        let stamp = |taken: i64, camera: &str| Stamp { taken: Some(taken), camera: camera.into() };
        let files = |names: &[(&str, Stamp)]| Scan {
            files: names.iter().map(|(name, stamp)| (root.join(name), 1, stamp.clone())).collect(),
            complete: false,
            ..Scan::default()
        };
        catalog.apply_scan(&library, &files(&[("a.jpg", stamp(1000, fuji)), ("b.jpg", stamp(1060 + 192, sony))])).unwrap();
        let order = |catalog: &Catalog| -> Vec<String> {
            let photos = catalog.photos(library.id, &Filter::default()).unwrap();
            photos.iter().map(|photo| photo.path.file_name().unwrap().to_string_lossy().into_owned()).collect()
        };
        catalog.apply_scan(&library, &files(&[("c.jpg", stamp(1030, fuji))])).unwrap();
        assert_eq!(order(&catalog), ["a.jpg", "c.jpg", "b.jpg"]);

        catalog.set_clock(library.id, sony, Some(Clock { seconds: -192, applied: true })).unwrap();
        assert_eq!(order(&catalog), ["a.jpg", "c.jpg", "b.jpg"], "b at 1060: still after c at 1030");
        catalog.set_clock(library.id, fuji, Some(Clock { seconds: 60, applied: true })).unwrap();
        assert_eq!(order(&catalog), ["a.jpg", "b.jpg", "c.jpg"], "the X-T5 a minute on: c at 1090");

        let cameras = catalog.cameras(library.id).unwrap();
        assert_eq!(cameras[0].key, fuji, "most photographs first");
        assert_eq!(cameras[1].clock, Some(Clock { seconds: -192, applied: true }));

        catalog.set_clock(library.id, fuji, Some(Clock { seconds: 60, applied: false })).unwrap();
        assert_eq!(order(&catalog), ["a.jpg", "c.jpg", "b.jpg"]);
        let b = catalog.photos(library.id, &Filter::default()).unwrap().into_iter().find(|photo| photo.path.ends_with("b.jpg")).unwrap();
        let clock = catalog.clock_of(b.id).unwrap();
        assert_eq!((clock.own, clock.taken, clock.slate), (Some(1252), Some(1060), None));

        std::fs::write(root.join("d.jpg"), b"x").unwrap();
        catalog.apply_scan(&library, &files(&[("d.jpg", stamp(1100 + 192, sony))])).unwrap();
        let d = catalog.photos(library.id, &Filter::default()).unwrap().into_iter().find(|photo| photo.path.ends_with("d.jpg")).unwrap();
        assert_eq!(d.taken, Some(1100));
        catalog.apply_scan(&library, &files(&[("d.jpg", stamp(1100 + 192, sony))])).unwrap();
        assert_eq!(catalog.clock_of(d.id).unwrap().taken, Some(1100));
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn a_library_from_before_keeps_its_dates() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE photos (id INTEGER PRIMARY KEY, path TEXT, mtime INTEGER, taken INTEGER); \
             INSERT INTO photos VALUES (1, 'a.jpg', 5, 1000);",
        )
        .unwrap();
        migrate(&conn).unwrap();
        migrate(&conn).unwrap();
        let row: (Option<String>, Option<i64>) =
            conn.query_row("SELECT camera, camera_time FROM photos", [], |row| Ok((row.get(0)?, row.get(1)?))).unwrap();
        assert_eq!(row, (None, Some(1000)));
    }
}
