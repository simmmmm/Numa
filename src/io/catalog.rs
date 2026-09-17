use std::cell::RefCell;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::UNIX_EPOCH;

use rusqlite::{params, Connection, OptionalExtension};

use crate::core::document::Document;
use crate::cull::people;
use crate::io::raw;

pub type Measured = (i64, crate::cull::Frame, Option<u32>, Option<f32>, Vec<([f32; people::LENGTH], Option<Vec<u8>>)>);

#[derive(Debug, Clone)]
pub struct StoredFace {

    pub id: i64,
    pub photo_id: i64,
    pub embedding: [f32; people::LENGTH],

    pub portrait: Option<Vec<u8>>,
}

fn embedding_bytes(embedding: &[f32; people::LENGTH]) -> Vec<u8> {
    embedding.iter().flat_map(|value| value.to_le_bytes()).collect()
}

fn embedding_from(bytes: &[u8]) -> Option<[f32; people::LENGTH]> {
    if bytes.len() != people::LENGTH * 4 {
        return None;
    }
    let mut out = [0.0f32; people::LENGTH];
    let (chunks, _) = bytes.as_chunks::<4>();
    for (slot, chunk) in out.iter_mut().zip(chunks) {
        *slot = f32::from_le_bytes(*chunk);
    }
    Some(out)
}

const LEGACY_SCHEMA: &str = r#"
PRAGMA foreign_keys = ON;
PRAGMA journal_mode = WAL;

CREATE TABLE IF NOT EXISTS libraries (
    id   INTEGER PRIMARY KEY,
    path TEXT NOT NULL UNIQUE,
    -- LIB-009: what to call it. Null means the folder's own name, which is
    -- right until two shoots are both called "100_FUJI".
    name TEXT
);

-- A path is unique within its library, not across all of them: two libraries
-- may legitimately overlap, and a global constraint made the nested one record
-- nothing at all and show itself as empty.
CREATE TABLE IF NOT EXISTS photos (
    id         INTEGER PRIMARY KEY,
    library_id INTEGER NOT NULL REFERENCES libraries(id) ON DELETE CASCADE,
    path       TEXT NOT NULL,
    mtime      INTEGER NOT NULL,
    rating     INTEGER NOT NULL DEFAULT 0,
    flag       INTEGER NOT NULL DEFAULT 0,
    edits      TEXT,
    UNIQUE(library_id, path)
);

CREATE INDEX IF NOT EXISTS photos_library ON photos(library_id);

-- Small things the application should remember between runs. A table rather
-- than a file beside one, so there is still exactly one thing to back up.
CREATE TABLE IF NOT EXISTS settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

-- CULL: what a pass of the measures found. A separate table on purpose — these
-- are suggestions, and they are never going to sit in the same row as the
-- rating the photographer typed. Dropping the table forgets every suggestion
-- and loses no work.
CREATE TABLE IF NOT EXISTS analysis (
    photo_id  INTEGER PRIMARY KEY REFERENCES photos(id) ON DELETE CASCADE,
    version   INTEGER NOT NULL,
    sharpness REAL    NOT NULL,
    blown     REAL    NOT NULL,
    hash      INTEGER NOT NULL,
    burst     INTEGER,
    best      INTEGER NOT NULL DEFAULT 0,
    -- CULL-003: null means the detector never ran, zero means it ran and found
    -- nobody. Those are different answers and the grid shows them differently.
    faces     INTEGER,
    face_sharpness REAL,
    -- CULL-004: a suggested rating, 0..5.
    suggested REAL
);

-- LIB-014: the names the photographer gave faces. Only what they typed is
-- kept — a face nobody named is not a row — and each name travels to other
-- photographs by likeness rather than being written onto them, so a name given
-- later reaches every photograph already taken.
CREATE TABLE IF NOT EXISTS people (
    id   INTEGER PRIMARY KEY,
    name TEXT NOT NULL UNIQUE COLLATE NOCASE
);
CREATE TABLE IF NOT EXISTS named_faces (
    id        INTEGER PRIMARY KEY,
    photo_id  INTEGER NOT NULL REFERENCES photos(id) ON DELETE CASCADE,
    person_id INTEGER NOT NULL REFERENCES people(id) ON DELETE CASCADE,
    -- SFace's 128 numbers, little-endian f32.
    embedding BLOB NOT NULL
);
CREATE INDEX IF NOT EXISTS named_faces_photo ON named_faces(photo_id);
-- LIB-014: and every face the measure pass found, named or not — what "every
-- photograph Anna is in" is worked out from. Derived, like `analysis`: dropping
-- it costs a pass of Analyse and no work.
CREATE TABLE IF NOT EXISTS faces (
    photo_id  INTEGER NOT NULL REFERENCES photos(id) ON DELETE CASCADE,
    embedding BLOB NOT NULL
);
CREATE INDEX IF NOT EXISTS faces_photo ON faces(photo_id);
"#;

fn migrate(conn: &Connection) -> Result<(), String> {
    let existing: String = conn
        .query_row(
            "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'photos'",
            [],
            |row| row.get(0),
        )
        .map_err(|err| err.to_string())?;

    conn.execute(
        "UPDATE photos SET edits = NULL WHERE edits LIKE \
         '%\"white_balance\":null,\"film_simulation\":null,\"operations\":[]}'",
        [],
    )
    .map_err(|err| format!("could not tidy stored edits: {err}"))?;

    let analysis_columns: Vec<String> = conn
        .prepare("SELECT * FROM analysis LIMIT 0")
        .map(|stmt| stmt.column_names().iter().map(|name| name.to_string()).collect())
        .unwrap_or_default();
    let expected = ["faces", "face_sharpness", "suggested"];

    let analysis_sql: String = conn
        .query_row("SELECT sql FROM sqlite_master WHERE name = 'analysis'", [], |row| row.get(0))
        .unwrap_or_default();
    if !analysis_columns.is_empty()
        && (!expected.iter().all(|want| analysis_columns.iter().any(|have| have == want))
            || analysis_sql.contains("photos_old"))
    {
        conn.execute_batch("DROP TABLE analysis;")
            .map_err(|err| format!("could not replace the analysis table: {err}"))?;
        conn.execute_batch(LEGACY_SCHEMA).map_err(|err| err.to_string())?;
    }

    let has_name: bool = conn
        .prepare("SELECT * FROM libraries LIMIT 0")
        .map(|stmt| stmt.column_names().iter().any(|column| *column == "name"))
        .unwrap_or(true);
    if !has_name {
        conn.execute("ALTER TABLE libraries ADD COLUMN name TEXT", [])
            .map_err(|err| format!("could not add the library name column: {err}"))?;
    }

    if !existing.contains("path       TEXT NOT NULL UNIQUE") {
        return Ok(());
    }

    conn.execute_batch(
        r#"
PRAGMA foreign_keys = OFF;
-- Since 3.26 a rename also rewrites the foreign keys that point at the table,
-- which would leave `analysis` referencing `photos_old` once it is dropped.
PRAGMA legacy_alter_table = ON;
BEGIN;
ALTER TABLE photos RENAME TO photos_old;
CREATE TABLE photos (
    id         INTEGER PRIMARY KEY,
    library_id INTEGER NOT NULL REFERENCES libraries(id) ON DELETE CASCADE,
    path       TEXT NOT NULL,
    mtime      INTEGER NOT NULL,
    rating     INTEGER NOT NULL DEFAULT 0,
    flag       INTEGER NOT NULL DEFAULT 0,
    edits      TEXT,
    UNIQUE(library_id, path)
);
INSERT INTO photos (id, library_id, path, mtime, rating, flag, edits)
    SELECT id, library_id, path, mtime, rating, flag, edits FROM photos_old;
DROP TABLE photos_old;
CREATE INDEX IF NOT EXISTS photos_library ON photos(library_id);
COMMIT;
PRAGMA legacy_alter_table = OFF;
PRAGMA foreign_keys = ON;
"#,
    )
    .map_err(|err| format!("could not migrate the catalog: {err}"))
}

#[derive(Debug, Clone)]
pub struct Library {
    pub id: i64,
    pub path: PathBuf,

    pub name: Option<String>,
}

impl Library {

    pub fn label(&self) -> String {
        if let Some(name) = self.name.as_ref().filter(|name| !name.trim().is_empty()) {
            return name.clone();
        }
        self.path
            .file_name()
            .map(|name| name.to_string_lossy().to_string())
            .unwrap_or_else(|| self.path.to_string_lossy().to_string())
    }

    pub fn export_dir(&self) -> PathBuf {
        self.path.join("edited")
    }
}

#[derive(Debug, Clone)]
pub struct Photo {
    pub id: i64,
    pub path: PathBuf,
    pub mtime: i64,
    pub rating: u8,
    pub flag: Flag,

    pub sharpness: Option<f32>,
    pub blown: Option<f32>,
    pub best_of_burst: bool,

    pub faces: Option<u32>,
    pub face_sharpness: Option<f32>,

    pub suggested: Option<f32>,

    pub edited: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Flag {
    Rejected,
    None,
    Picked,
}

impl Flag {
    fn to_i64(self) -> i64 {
        match self {
            Flag::Rejected => -1,
            Flag::None => 0,
            Flag::Picked => 1,
        }
    }

    fn from_i64(value: i64) -> Self {
        match value {
            i64::MIN..=-1 => Flag::Rejected,
            0 => Flag::None,
            _ => Flag::Picked,
        }
    }
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Filter {
    pub min_rating: u8,
    pub flag: Option<Flag>,
    pub sort: Sort,

    pub best_of_burst: bool,

    pub only_questionable: bool,

    pub person: Option<String>,

    pub file_type: FileType,

    pub album: Option<String>,

    pub all_libraries: bool,
}

impl Filter {

    pub fn spans_libraries(&self) -> bool {
        self.all_libraries || self.album.is_some() || self.person.is_some()
    }

    pub fn in_one_library(&mut self) {
        self.all_libraries = false;
        self.album = None;
        self.person = None;
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum FileType {
    #[default]
    Any,
    Raw,
    NotRaw,
}

impl FileType {
    fn admits(self, path: &Path) -> bool {
        match self {
            FileType::Any => true,
            FileType::Raw => crate::io::raw::is_raw(path),
            FileType::NotRaw => !crate::io::raw::is_raw(path),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum Sort {
    #[default]
    Captured,
    Name,
    Rating,

    Sharpness,

    Suggested,
}

impl Sort {
    fn order_by(self) -> &'static str {
        match self {

            Sort::Captured => "mtime ASC, path ASC",
            Sort::Name => "path ASC",
            Sort::Rating => "rating DESC, mtime ASC",
            Sort::Sharpness => "sharpness DESC NULLS LAST, mtime ASC",
            Sort::Suggested => "suggested DESC NULLS LAST, mtime ASC",
        }
    }

    fn compare(self, a: &Photo, b: &Photo) -> std::cmp::Ordering {

        let best = |a: Option<f32>, b: Option<f32>| match (a, b) {
            (Some(a), Some(b)) => b.total_cmp(&a),
            (a, b) => b.is_some().cmp(&a.is_some()),
        };
        let captured = (a.mtime, &a.path).cmp(&(b.mtime, &b.path));
        match self {
            Sort::Captured => captured,
            Sort::Name => a.path.cmp(&b.path),
            Sort::Rating => b.rating.cmp(&a.rating).then(a.mtime.cmp(&b.mtime)),
            Sort::Sharpness => best(a.sharpness, b.sharpness).then(a.mtime.cmp(&b.mtime)),
            Sort::Suggested => best(a.suggested, b.suggested).then(a.mtime.cmp(&b.mtime)),
        }
    }
}

const HOME_SCHEMA: &str = r#"
PRAGMA foreign_keys = ON;
PRAGMA journal_mode = WAL;

CREATE TABLE IF NOT EXISTS libraries (
    id   INTEGER PRIMARY KEY,
    path TEXT NOT NULL UNIQUE,
    -- LIB-009: what to call it. Null means the folder's own name, which is
    -- right until two shoots are both called "100_FUJI".
    name TEXT
);

-- Small things the application should remember between runs.
CREATE TABLE IF NOT EXISTS settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

-- LIB-008: albums, named here because an album may hold photographs from any
-- library and this is the one catalog that spans them. Which photographs are
-- in one is in each library's own catalog, under the album's key.
CREATE TABLE IF NOT EXISTS albums (
    key  TEXT PRIMARY KEY,
    name TEXT NOT NULL UNIQUE COLLATE NOCASE
);
"#;

const LIBRARY_SCHEMA: &str = r#"
PRAGMA foreign_keys = ON;
PRAGMA journal_mode = DELETE;

-- Paths are relative to the library's folder, so the folder can move — to
-- another drive, another mount point, another computer — and still be itself.
CREATE TABLE IF NOT EXISTS photos (
    id     INTEGER PRIMARY KEY,
    path   TEXT NOT NULL UNIQUE,
    mtime  INTEGER NOT NULL,
    rating INTEGER NOT NULL DEFAULT 0,
    flag   INTEGER NOT NULL DEFAULT 0,
    edits  TEXT
);

-- DOC-004: each photograph's history, so stepping back reaches past the moment
-- it was opened. The states as a JSON array of edit stacks, oldest first, and
-- which of them is on screen. A table of its own because the photographs'
-- rows are read for every grid, and this is read only when one is opened.
CREATE TABLE IF NOT EXISTS history (
    photo_id INTEGER PRIMARY KEY REFERENCES photos(id) ON DELETE CASCADE,
    position INTEGER NOT NULL,
    states   TEXT NOT NULL
);

-- DOC-005: named versions of one photograph's edit, each the same stack
-- `edits` holds. A name is taken once per photograph.
CREATE TABLE IF NOT EXISTS snapshots (
    photo_id INTEGER NOT NULL REFERENCES photos(id) ON DELETE CASCADE,
    name     TEXT NOT NULL,
    created  INTEGER NOT NULL,
    edits    TEXT NOT NULL,
    PRIMARY KEY (photo_id, name)
);

-- CULL: what a pass of the measures found. A separate table on purpose — these
-- are suggestions, and they are never going to sit in the same row as the
-- rating the photographer typed. Dropping the table forgets every suggestion
-- and loses no work.
CREATE TABLE IF NOT EXISTS analysis (
    photo_id  INTEGER PRIMARY KEY REFERENCES photos(id) ON DELETE CASCADE,
    version   INTEGER NOT NULL,
    sharpness REAL    NOT NULL,
    blown     REAL    NOT NULL,
    hash      INTEGER NOT NULL,
    burst     INTEGER,
    best      INTEGER NOT NULL DEFAULT 0,
    -- CULL-003: null means the detector never ran, zero means it ran and found
    -- nobody. Those are different answers and the grid shows them differently.
    faces     INTEGER,
    face_sharpness REAL,
    -- CULL-004: a suggested rating, 0..5.
    suggested REAL,
    -- CULL-005: the frame's tone, which a learned score reads alongside the rest.
    brightness REAL,
    contrast REAL,
    colourfulness REAL
);

-- LIB-014: the names the photographer gave faces. Only what they typed is
-- kept — a face nobody named is not a row — and each name travels to other
-- photographs by likeness rather than being written onto them. Per library,
-- and put together by name across every library that can be reached, so a
-- folder taken elsewhere still knows who is in it.
CREATE TABLE IF NOT EXISTS people (
    id   INTEGER PRIMARY KEY,
    name TEXT NOT NULL UNIQUE COLLATE NOCASE
);
CREATE TABLE IF NOT EXISTS named_faces (
    id        INTEGER PRIMARY KEY,
    photo_id  INTEGER NOT NULL REFERENCES photos(id) ON DELETE CASCADE,
    person_id INTEGER NOT NULL REFERENCES people(id) ON DELETE CASCADE,
    -- SFace's 128 numbers, little-endian f32.
    embedding BLOB NOT NULL
);
CREATE INDEX IF NOT EXISTS named_faces_photo ON named_faces(photo_id);
-- LIB-014: faces the photographer said are nobody to name — a stranger in the
-- background, a statue. Like a name, they travel by likeness: a face like one
-- of these is not asked about again.
CREATE TABLE IF NOT EXISTS ignored_faces (
    id        INTEGER PRIMARY KEY,
    photo_id  INTEGER NOT NULL REFERENCES photos(id) ON DELETE CASCADE,
    embedding BLOB NOT NULL
);
-- LIB-014: and every face the measure pass found, named or not. Derived, like
-- `analysis`: dropping it costs a pass of Analyse and no work.
CREATE TABLE IF NOT EXISTS faces (
    photo_id  INTEGER NOT NULL REFERENCES photos(id) ON DELETE CASCADE,
    embedding BLOB NOT NULL,
    -- The face as a small JPEG, for the People view.
    portrait  BLOB
);
CREATE INDEX IF NOT EXISTS faces_photo ON faces(photo_id);

-- LIB-008: which photographs here are in which album. Here rather than beside
-- the album's name at home, because nothing home could point with survives:
-- a library gets a new id when its folder is added back or from another
-- drive, its path changes with the drive, and a photograph's row number is
-- handed out again after a rescan forgets it. This row moves with the folder,
-- and a photograph gone from disk leaves its albums by the cascade. The album
-- is its key rather than its name, so renaming one while a drive is unplugged
-- does not lose that drive's photographs.
CREATE TABLE IF NOT EXISTS album_photos (
    album    TEXT NOT NULL,
    photo_id INTEGER NOT NULL REFERENCES photos(id) ON DELETE CASCADE,
    PRIMARY KEY (album, photo_id)
);
"#;

pub const LIBRARY_DIR: &str = ".numa";

const BACKUPS_KEPT: usize = 4;
const BACKUP_EVERY: std::time::Duration = std::time::Duration::from_secs(7 * 24 * 3600);

const LOCAL_BITS: u32 = 32;

fn global_id(library_id: i64, local: i64) -> i64 {
    (library_id << LOCAL_BITS) | local
}

fn split_id(id: i64) -> (i64, i64) {
    (id >> LOCAL_BITS, id & ((1 << LOCAL_BITS) - 1))
}

fn text(err: impl std::fmt::Display) -> String {
    err.to_string()
}

pub struct Catalog {
    home: Connection,

    home_path: Option<PathBuf>,

    opened: RefCell<HashMap<i64, Rc<OpenLibrary>>>,
}

struct OpenLibrary {
    conn: Connection,
    root: PathBuf,
}

impl OpenLibrary {
    fn absolute(&self, stored: &str) -> PathBuf {
        self.root.join(stored)
    }

    fn relative(&self, path: &Path) -> String {
        path.strip_prefix(&self.root).unwrap_or(path).to_string_lossy().into_owned()
    }
}

pub fn default_path() -> PathBuf {
    crate::io::data_dir().join("catalog.db")
}

impl Catalog {
    pub fn open(path: &Path) -> Result<Self, String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(text)?;
        }
        let home = Connection::open(path).map_err(text)?;
        Self::from_home(home, Some(path.to_path_buf()))
    }

    pub fn open_default() -> Result<Self, String> {
        Self::open(&default_path())
    }

    #[cfg(test)]
    fn in_memory() -> Result<Self, String> {
        Self::from_home(Connection::open_in_memory().map_err(text)?, None)
    }

    fn from_home(home: Connection, home_path: Option<PathBuf>) -> Result<Self, String> {

        let legacy = table_exists(&home, "photos");
        if legacy {
            home.execute_batch(LEGACY_SCHEMA).map_err(text)?;
            migrate(&home)?;
        }
        home.execute_batch(HOME_SCHEMA).map_err(text)?;

        let catalog = Self { home, home_path, opened: RefCell::new(HashMap::new()) };
        if legacy {
            catalog.split_legacy()?;
        }
        Ok(catalog)
    }

    fn library(&self, library_id: i64) -> Result<Rc<OpenLibrary>, String> {
        if let Some(open) = self.opened.borrow().get(&library_id) {
            return Ok(open.clone());
        }
        let root: String = self
            .home
            .query_row("SELECT path FROM libraries WHERE id = ?1", params![library_id], |row| row.get(0))
            .map_err(|_| format!("no library {library_id}"))?;
        let root = PathBuf::from(root);
        if !root.is_dir() {
            return Err(format!("{} is not there — is the drive connected?", root.display()));
        }

        let dir = root.join(LIBRARY_DIR);
        std::fs::create_dir_all(&dir)
            .map_err(|err| format!("cannot keep a catalog in {}: {err}", dir.display()))?;
        let conn = Connection::open(dir.join("catalog.db")).map_err(text)?;
        conn.execute_batch(LIBRARY_SCHEMA).map_err(text)?;

        let has_portrait = conn
            .prepare("SELECT * FROM faces LIMIT 0")
            .map(|statement| statement.column_names().contains(&"portrait"))
            .unwrap_or(true);
        if !has_portrait {
            conn.execute("ALTER TABLE faces ADD COLUMN portrait BLOB", []).map_err(text)?;
        }

        let has_tone = conn
            .prepare("SELECT * FROM analysis LIMIT 0")
            .map(|statement| statement.column_names().contains(&"colourfulness"))
            .unwrap_or(true);
        if !has_tone {
            conn.execute_batch(
                "ALTER TABLE analysis ADD COLUMN brightness REAL; \
                 ALTER TABLE analysis ADD COLUMN contrast REAL; \
                 ALTER TABLE analysis ADD COLUMN colourfulness REAL;",
            )
            .map_err(text)?;
        }
        back_up_weekly(&conn, &dir);

        let open = Rc::new(OpenLibrary { conn, root });
        self.opened.borrow_mut().insert(library_id, open.clone());
        Ok(open)
    }

    fn photo(&self, photo_id: i64) -> Result<(Rc<OpenLibrary>, i64), String> {
        let (library_id, local) = split_id(photo_id);
        Ok((self.library(library_id)?, local))
    }

    fn split_legacy(&self) -> Result<(), String> {
        if let Some(path) = &self.home_path {
            let copy = path.with_file_name("catalog-before-libraries.db");
            if !copy.exists() {
                self.home
                    .execute("VACUUM INTO ?1", params![copy.to_string_lossy()])
                    .map_err(|err| format!("could not copy the catalog before splitting it: {err}"))?;
            }
        }

        for library in self.libraries()? {
            let Ok(open) = self.library(library.id) else { continue };
            let already: i64 = open
                .conn
                .query_row("SELECT COUNT(*) FROM photos", [], |row| row.get(0))
                .map_err(text)?;
            if already > 0 {
                log::warn!(
                    "{} already has a catalog; the one from before is kept in catalog-before-libraries.db",
                    library.path.display()
                );
            } else {
                self.copy_legacy(library.id, &open)?;

                back_up_weekly(&open.conn, &library.path.join(LIBRARY_DIR));
            }
            self.home
                .execute("DELETE FROM photos WHERE library_id = ?1", params![library.id])
                .map_err(text)?;
        }

        let left: i64 = self
            .home
            .query_row("SELECT COUNT(*) FROM photos", [], |row| row.get(0))
            .map_err(text)?;
        if left == 0 {
            self.home
                .execute_batch(
                    "DROP TABLE IF EXISTS faces; DROP TABLE IF EXISTS named_faces; \
                     DROP TABLE IF EXISTS people; DROP TABLE IF EXISTS analysis; \
                     DROP TABLE IF EXISTS photos;",
                )
                .map_err(text)?;
        }
        Ok(())
    }

    fn copy_legacy(&self, library_id: i64, open: &OpenLibrary) -> Result<(), String> {
        let tx = open.conn.unchecked_transaction().map_err(text)?;
        let photos: Vec<(i64, String, i64, i64, i64, Option<String>)> = self
            .home
            .prepare("SELECT id, path, mtime, rating, flag, edits FROM photos WHERE library_id = ?1")
            .and_then(|mut statement| {
                statement
                    .query_map(params![library_id], |row| {
                        Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?, row.get(4)?, row.get(5)?))
                    })?
                    .collect()
            })
            .map_err(text)?;
        for (id, path, mtime, rating, flag, edits) in &photos {
            tx.execute(
                "INSERT INTO photos (id, path, mtime, rating, flag, edits) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                params![id, open.relative(Path::new(path)), mtime, rating, flag, edits],
            )
            .map_err(text)?;
        }

        let rows = |sql: &str| -> Result<Vec<Vec<rusqlite::types::Value>>, String> {
            let mut statement = self.home.prepare(sql).map_err(text)?;
            let width = statement.column_count();
            let rows = statement
                .query_map(params![library_id], |row| {
                    (0..width).map(|index| row.get::<_, rusqlite::types::Value>(index)).collect()
                })
                .map_err(text)?;
            rows.collect::<Result<_, _>>().map_err(text)
        };
        let copies = [
            (
                "SELECT a.photo_id, a.version, a.sharpness, a.blown, a.hash, a.burst, a.best, \
                        a.faces, a.face_sharpness, a.suggested \
                 FROM analysis a JOIN photos p ON p.id = a.photo_id WHERE p.library_id = ?1",
                "INSERT INTO analysis (photo_id, version, sharpness, blown, hash, burst, best, \
                                       faces, face_sharpness, suggested) \
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10)",
            ),
            (
                "SELECT DISTINCT pe.id, pe.name FROM people pe \
                 JOIN named_faces n ON n.person_id = pe.id \
                 JOIN photos p ON p.id = n.photo_id WHERE p.library_id = ?1",
                "INSERT INTO people (id, name) VALUES (?1, ?2)",
            ),
            (
                "SELECT n.id, n.photo_id, n.person_id, n.embedding FROM named_faces n \
                 JOIN photos p ON p.id = n.photo_id WHERE p.library_id = ?1",
                "INSERT INTO named_faces (id, photo_id, person_id, embedding) VALUES (?1, ?2, ?3, ?4)",
            ),
            (
                "SELECT f.photo_id, f.embedding FROM faces f \
                 JOIN photos p ON p.id = f.photo_id WHERE p.library_id = ?1",
                "INSERT INTO faces (photo_id, embedding) VALUES (?1, ?2)",
            ),
        ];
        for (read, write) in copies {

            let table = read.split("FROM ").nth(1).and_then(|rest| rest.split_whitespace().next());
            if table.is_some_and(|table| !table_exists(&self.home, table)) {
                continue;
            }
            for row in rows(read)? {
                tx.execute(write, rusqlite::params_from_iter(row)).map_err(text)?;
            }
        }
        tx.commit().map_err(text)
    }

    pub fn add_library(&self, path: &Path) -> Result<Library, String> {

        let dir = path.join(LIBRARY_DIR);
        std::fs::create_dir_all(&dir).map_err(|err| {
            format!(
                "Numa keeps ratings and edits in a hidden {LIBRARY_DIR} folder inside the folder it shows, \
                 and cannot create one in {}: {err}. Choose a folder you can write to, or copy the \
                 photographs into one.",
                path.display()
            )
        })?;
        let text_path = path.to_string_lossy();
        self.home
            .execute("INSERT OR IGNORE INTO libraries (path) VALUES (?1)", params![text_path])
            .map_err(text)?;

        let id = self
            .home
            .query_row("SELECT id FROM libraries WHERE path = ?1", params![text_path], |row| row.get(0))
            .map_err(text)?;

        Ok(Library { id, path: path.to_path_buf(), name: None })
    }

    pub fn libraries(&self) -> Result<Vec<Library>, String> {
        let mut stmt = self.home.prepare("SELECT id, path, name FROM libraries ORDER BY path").map_err(text)?;

        let rows = stmt
            .query_map([], |row| {
                Ok(Library {
                    id: row.get(0)?,
                    path: PathBuf::from(row.get::<_, String>(1)?),
                    name: row.get(2)?,
                })
            })
            .map_err(text)?;

        rows.collect::<Result<_, _>>().map_err(text)
    }

    pub fn unanalysed(&self, library_id: i64, version: i64) -> Result<Vec<Photo>, String> {
        let open = self.library(library_id)?;
        let mut stmt = open
            .conn
            .prepare(
                "SELECT p.id, p.path, p.mtime, p.rating, p.flag \
                 FROM photos p LEFT JOIN analysis a ON a.photo_id = p.id \
                 WHERE a.photo_id IS NULL OR a.version <> ?1 \
                 ORDER BY p.mtime ASC, p.path ASC",
            )
            .map_err(text)?;

        let rows = stmt
            .query_map(params![version], |row| {
                Ok(Photo {
                    id: global_id(library_id, row.get(0)?),
                    path: open.absolute(&row.get::<_, String>(1)?),
                    mtime: row.get(2)?,
                    rating: row.get::<_, i64>(3)?.clamp(0, 5) as u8,
                    flag: Flag::from_i64(row.get(4)?),
                    sharpness: None,
                    blown: None,
                    best_of_burst: false,
                    faces: None,
                    face_sharpness: None,
                    suggested: None,
                    edited: false,
                })
            })
            .map_err(text)?;

        rows.collect::<Result<_, _>>().map_err(text)
    }

    pub fn save_analysis_batch(&self, version: i64, measured: &[Measured]) -> Result<(), String> {
        let mut by_library: HashMap<i64, Vec<&Measured>> = HashMap::new();
        for row in measured {
            by_library.entry(split_id(row.0).0).or_default().push(row);
        }
        for (library_id, rows) in by_library {
            let open = self.library(library_id)?;
            let tx = open.conn.unchecked_transaction().map_err(text)?;
            for (photo_id, frame, faces, face_sharpness, embeddings) in rows {
                let local = split_id(*photo_id).1;
                write_analysis(&tx, local, version, frame, *faces, *face_sharpness)?;

                tx.execute("DELETE FROM faces WHERE photo_id = ?1", params![local]).map_err(text)?;
                for (embedding, portrait) in embeddings {
                    tx.execute(
                        "INSERT INTO faces (photo_id, embedding, portrait) VALUES (?1, ?2, ?3)",
                        params![local, embedding_bytes(embedding), portrait],
                    )
                    .map_err(text)?;
                }
            }
            tx.commit().map_err(text)?;
        }
        Ok(())
    }

    pub fn people(&self, library_id: i64) -> Result<Vec<(String, Vec<i64>)>, String> {
        let named = self.named_faces()?;
        if named.is_empty() {
            return Ok(Vec::new());
        }

        let known = self.known_faces()?;

        let open = self.library(library_id)?;
        let found: Vec<(i64, Vec<u8>)> = open
            .conn
            .prepare("SELECT photo_id, embedding FROM faces")
            .and_then(|mut statement| statement.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?.collect())
            .map_err(text)?;

        let mut everyone: Vec<(String, Vec<i64>)> = Vec::new();
        let mut add = |name: &str, photo: i64| {
            let index = match everyone.iter().position(|(have, _)| have.eq_ignore_ascii_case(name)) {
                Some(index) => index,
                None => {
                    everyone.push((name.to_string(), Vec::new()));
                    everyone.len() - 1
                }
            };
            if !everyone[index].1.contains(&photo) {
                everyone[index].1.push(photo);
            }
        };
        for (local, bytes) in &found {
            if let Some((name, _)) = embedding_from(bytes).and_then(|face| people::recognise(&face, &known)) {
                if !name.is_empty() {
                    add(name, global_id(library_id, *local));
                }
            }
        }

        for (photo, name, _) in &named {
            if split_id(*photo).0 == library_id {
                add(name, *photo);
            }
        }

        everyone.sort_by(|a, b| b.1.len().cmp(&a.1.len()).then_with(|| a.0.cmp(&b.0)));
        Ok(everyone)
    }

    pub fn faces(&self, library_id: i64) -> Result<Vec<StoredFace>, String> {
        let open = self.library(library_id)?;
        let mut statement = open
            .conn
            .prepare("SELECT rowid, photo_id, embedding, portrait FROM faces ORDER BY photo_id, rowid")
            .map_err(text)?;
        let rows = statement
            .query_map([], |row| {
                Ok((
                    row.get::<_, i64>(0)?,
                    row.get::<_, i64>(1)?,
                    row.get::<_, Vec<u8>>(2)?,
                    row.get::<_, Option<Vec<u8>>>(3)?,
                ))
            })
            .map_err(text)?;
        let mut faces = Vec::new();
        for row in rows {
            let (face, photo, bytes, portrait) = row.map_err(text)?;
            if let Some(embedding) = embedding_from(&bytes) {
                faces.push(StoredFace {
                    id: global_id(library_id, face),
                    photo_id: global_id(library_id, photo),
                    embedding,
                    portrait,
                });
            }
        }
        Ok(faces)
    }

    pub fn set_portrait(&self, face_id: i64, portrait: &[u8]) -> Result<(), String> {
        let (open, local) = self.photo(face_id)?;
        open.conn
            .execute("UPDATE faces SET portrait = ?1 WHERE rowid = ?2", params![portrait, local])
            .map_err(text)?;
        Ok(())
    }

    pub fn rename_person(&self, from: &str, to: &str) -> Result<(), String> {
        let to = to.trim();
        for library in self.libraries()? {
            let Ok(open) = self.library(library.id) else { continue };
            let tx = open.conn.unchecked_transaction().map_err(text)?;
            let Ok(old) = tx.query_row("SELECT id FROM people WHERE name = ?1", params![from], |row| {
                row.get::<_, i64>(0)
            }) else {
                continue;
            };
            if to.is_empty() {
                tx.execute("DELETE FROM people WHERE id = ?1", params![old]).map_err(text)?;
            } else {
                match tx.query_row("SELECT id FROM people WHERE name = ?1 AND id <> ?2", params![to, old], |row| {
                    row.get::<_, i64>(0)
                }) {
                    Ok(existing) => {
                        tx.execute("UPDATE named_faces SET person_id = ?1 WHERE person_id = ?2", params![existing, old])
                            .map_err(text)?;
                        tx.execute("DELETE FROM people WHERE id = ?1", params![old]).map_err(text)?;
                    }
                    Err(_) => {
                        tx.execute("UPDATE people SET name = ?1 WHERE id = ?2", params![to, old]).map_err(text)?;
                    }
                }
            }
            tx.commit().map_err(text)?;
        }
        Ok(())
    }

    pub fn save_analysis(
        &self,
        photo_id: i64,
        version: i64,
        frame: &crate::cull::Frame,
        faces: Option<u32>,
        face_sharpness: Option<f32>,
    ) -> Result<(), String> {
        let (open, local) = self.photo(photo_id)?;
        write_analysis(&open.conn, local, version, frame, faces, face_sharpness)
    }

    #[allow(clippy::type_complexity)]
    pub fn analysed(&self, library_id: i64) -> Result<Vec<(i64, crate::cull::Frame, Option<f32>)>, String> {
        let open = self.library(library_id)?;
        let mut stmt = open
            .conn
            .prepare(
                "SELECT a.photo_id, a.sharpness, a.blown, a.hash, a.face_sharpness, \
                        a.brightness, a.contrast, a.colourfulness \
                 FROM analysis a JOIN photos p ON p.id = a.photo_id \
                 ORDER BY p.mtime ASC, p.path ASC",
            )
            .map_err(text)?;

        let rows = stmt
            .query_map([], |row| {
                Ok((
                    global_id(library_id, row.get::<_, i64>(0)?),
                    crate::cull::Frame {
                        sharpness: row.get::<_, f64>(1)? as f32,
                        blown: row.get::<_, f64>(2)? as f32,
                        hash: row.get::<_, i64>(3)? as u64,
                        brightness: row.get::<_, Option<f64>>(5)?.unwrap_or_default() as f32,
                        contrast: row.get::<_, Option<f64>>(6)?.unwrap_or_default() as f32,
                        colourfulness: row.get::<_, Option<f64>>(7)?.unwrap_or_default() as f32,
                    },
                    row.get::<_, Option<f64>>(4)?.map(|value| value as f32),
                ))
            })
            .map_err(text)?;

        rows.collect::<Result<_, _>>().map_err(text)
    }

    pub fn save_bursts(&self, groups: &[(i64, usize, bool, f32)]) -> Result<(), String> {
        let mut by_library: HashMap<i64, Vec<&(i64, usize, bool, f32)>> = HashMap::new();
        for row in groups {
            by_library.entry(split_id(row.0).0).or_default().push(row);
        }
        for (library_id, rows) in by_library {
            let open = self.library(library_id)?;
            let tx = open.conn.unchecked_transaction().map_err(text)?;
            for (photo_id, burst, best, suggested) in rows {
                tx.execute(
                    "UPDATE analysis SET burst = ?1, best = ?2, suggested = ?3 WHERE photo_id = ?4",
                    params![*burst as i64, i64::from(*best), suggested, split_id(*photo_id).1],
                )
                .map_err(text)?;
            }
            tx.commit().map_err(text)?;
        }
        Ok(())
    }

    pub fn remove_photo(&self, photo_id: i64) -> Result<(), String> {
        let (open, local) = self.photo(photo_id)?;
        open.conn.execute("DELETE FROM photos WHERE id = ?1", params![local]).map_err(text)?;
        Ok(())
    }

    pub fn setting(&self, key: &str) -> Option<String> {
        self.home
            .query_row("SELECT value FROM settings WHERE key = ?1", params![key], |row| row.get(0))
            .ok()
    }

    pub fn remember<T: serde::Serialize>(&self, key: &str, value: &T) {
        let Ok(json) = serde_json::to_string(value) else { return };
        if let Err(err) = self.set_setting(key, &json) {
            log::warn!("could not remember {key}: {err}");
        }
    }

    pub fn recall<T: serde::de::DeserializeOwned>(&self, key: &str) -> Option<T> {
        serde_json::from_str(&self.setting(key)?).ok()
    }

    pub fn named_faces(&self) -> Result<Vec<(i64, String, [f32; people::LENGTH])>, String> {
        let mut named = Vec::new();
        for library in self.libraries()? {
            let Ok(open) = self.library(library.id) else { continue };
            let mut statement = open
                .conn
                .prepare(
                    "SELECT named_faces.photo_id, people.name, named_faces.embedding FROM named_faces \
                     JOIN people ON people.id = named_faces.person_id",
                )
                .map_err(text)?;
            let rows = statement
                .query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?, row.get::<_, Vec<u8>>(2)?)))
                .map_err(text)?;
            for row in rows {
                let (photo, name, bytes) = row.map_err(text)?;

                if let Some(embedding) = embedding_from(&bytes) {
                    named.push((global_id(library.id, photo), name, embedding));
                }
            }
        }
        Ok(named)
    }

    pub fn known_faces(&self) -> Result<Vec<(String, [f32; people::LENGTH])>, String> {
        let mut known: Vec<(String, [f32; people::LENGTH])> =
            self.named_faces()?.into_iter().map(|(_, name, embedding)| (name, embedding)).collect();
        for library in self.libraries()? {
            let Ok(open) = self.library(library.id) else { continue };
            let rows: Vec<Vec<u8>> = open
                .conn
                .prepare("SELECT embedding FROM ignored_faces")
                .and_then(|mut statement| statement.query_map([], |row| row.get(0))?.collect())
                .map_err(text)?;
            known.extend(rows.iter().filter_map(|bytes| embedding_from(bytes)).map(|e| (String::new(), e)));
        }
        Ok(known)
    }

    pub fn ignore_faces(&self, faces: &[(i64, [f32; people::LENGTH])]) -> Result<(), String> {
        for (photo_id, embedding) in faces {
            let (open, local) = self.photo(*photo_id)?;
            open.conn
                .execute(
                    "INSERT INTO ignored_faces (photo_id, embedding) VALUES (?1, ?2)",
                    params![local, embedding_bytes(embedding)],
                )
                .map_err(text)?;
        }
        Ok(())
    }

    pub fn ignored_count(&self, library_id: i64) -> Result<i64, String> {
        self.library(library_id)?
            .conn
            .query_row("SELECT COUNT(*) FROM ignored_faces", [], |row| row.get(0))
            .map_err(text)
    }

    pub fn unignore_all(&self, library_id: i64) -> Result<(), String> {
        self.library(library_id)?.conn.execute("DELETE FROM ignored_faces", []).map_err(text)?;
        Ok(())
    }

    pub fn name_face(&self, photo_id: i64, embedding: &[f32; people::LENGTH], name: &str) -> Result<(), String> {
        const SAME_FACE: f32 = 0.8;
        let (open, local) = self.photo(photo_id)?;
        let tx = open.conn.unchecked_transaction().map_err(text)?;

        let earlier: Vec<(i64, Vec<u8>)> = tx
            .prepare("SELECT id, embedding FROM named_faces WHERE photo_id = ?1")
            .and_then(|mut statement| {
                statement.query_map(params![local], |row| Ok((row.get(0)?, row.get(1)?)))?.collect()
            })
            .map_err(text)?;
        for (id, bytes) in earlier {
            if embedding_from(&bytes).is_some_and(|other| people::likeness(&other, embedding) >= SAME_FACE) {
                tx.execute("DELETE FROM named_faces WHERE id = ?1", params![id]).map_err(text)?;
            }
        }

        let name = name.trim();
        if !name.is_empty() {
            tx.execute("INSERT OR IGNORE INTO people (name) VALUES (?1)", params![name]).map_err(text)?;
            let person: i64 = tx
                .query_row("SELECT id FROM people WHERE name = ?1", params![name], |row| row.get(0))
                .map_err(text)?;
            tx.execute(
                "INSERT INTO named_faces (photo_id, person_id, embedding) VALUES (?1, ?2, ?3)",
                params![local, person, embedding_bytes(embedding)],
            )
            .map_err(text)?;
        }

        tx.execute("DELETE FROM people WHERE id NOT IN (SELECT person_id FROM named_faces)", [])
            .map_err(text)?;
        tx.commit().map_err(text)
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<(), String> {
        self.home
            .execute(
                "INSERT INTO settings (key, value) VALUES (?1, ?2) \
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                params![key, value],
            )
            .map_err(text)?;
        Ok(())
    }

    pub fn photo_count(&self, library_id: i64) -> Result<i64, String> {
        self.library(library_id)?
            .conn
            .query_row("SELECT COUNT(*) FROM photos", [], |row| row.get(0))
            .map_err(text)
    }

    pub fn set_library_name(&self, id: i64, name: &str) -> Result<(), String> {
        let trimmed = name.trim();
        self.home
            .execute(
                "UPDATE libraries SET name = ?1 WHERE id = ?2",
                params![(!trimmed.is_empty()).then_some(trimmed), id],
            )
            .map_err(text)?;
        Ok(())
    }

    pub fn rename_library_folder(&self, id: i64, name: &str) -> Result<PathBuf, String> {
        let name = name.trim();
        if name.is_empty() || name == "." || name == ".." || name.contains(std::path::MAIN_SEPARATOR) {
            return Err(format!("“{name}” cannot be a folder name"));
        }
        let from: String = self
            .home
            .query_row("SELECT path FROM libraries WHERE id = ?1", params![id], |row| row.get(0))
            .map_err(|_| format!("no library {id}"))?;
        let from = PathBuf::from(from);
        let to = from.with_file_name(name);
        if to == from {
            self.set_library_name(id, "")?;
            return Ok(to);
        }

        if to.exists() {
            return Err(format!("{} already exists", to.display()));
        }
        std::fs::rename(&from, &to).map_err(|err| format!("could not rename {}: {err}", from.display()))?;

        self.opened.borrow_mut().remove(&id);
        let updated = self.home.execute(
            "UPDATE libraries SET path = ?1, name = NULL WHERE id = ?2",
            params![to.to_string_lossy(), id],
        );
        if let Err(err) = updated {

            let _ = std::fs::rename(&to, &from);
            return Err(text(err));
        }
        Ok(to)
    }

    pub fn remove_library(&self, id: i64) -> Result<(), String> {
        self.opened.borrow_mut().remove(&id);
        self.home.execute("DELETE FROM libraries WHERE id = ?1", params![id]).map_err(text)?;
        Ok(())
    }

    pub fn sync_library(&self, library: &Library) -> Result<usize, String> {
        self.apply_scan(library, &scan(&library.path)).map(|changes| changes.added)
    }

    pub fn apply_scan(&self, library: &Library, found: &Scan) -> Result<Changes, String> {
        let open = self.library(library.id)?;

        let tx = open.conn.unchecked_transaction().map_err(text)?;
        let mut changes = Changes::default();
        let mut seen = std::collections::HashSet::new();
        let complete = found.complete;

        for (path, mtime) in &found.files {
            let (path, mtime) = (path, *mtime);
            let relative = open.relative(path);

            let inserted = tx
                .execute("INSERT OR IGNORE INTO photos (path, mtime) VALUES (?1, ?2)", params![relative, mtime])
                .map_err(text)?;

            if inserted == 1 {
                changes.added += 1;
            } else {
                changes.updated += tx
                    .execute(
                        "UPDATE photos SET mtime = ?1 WHERE path = ?2 AND mtime <> ?1",
                        params![mtime, relative],
                    )
                    .map_err(text)?;
            }

            seen.insert(relative);
        }

        if complete && !seen.is_empty() {
            let stale: Vec<i64> = {
                let mut stmt = tx.prepare("SELECT id, path FROM photos").map_err(text)?;
                let rows = stmt
                    .query_map([], |row| Ok((row.get::<_, i64>(0)?, row.get::<_, String>(1)?)))
                    .map_err(text)?;
                rows.flatten().filter(|(_, path)| !seen.contains(path)).map(|(id, _)| id).collect()
            };

            for id in stale {
                changes.removed += tx.execute("DELETE FROM photos WHERE id = ?1", params![id]).map_err(text)?;
            }
        }

        tx.commit().map_err(text)?;
        Ok(changes)
    }

    pub fn photos_everywhere(&self, filter: &Filter) -> Result<Vec<Photo>, String> {
        let mut photos = Vec::new();
        for library in self.libraries()? {
            match self.photos(library.id, filter) {
                Ok(found) => photos.extend(found),

                Err(err) => log::warn!("left {} out: {err}", library.label()),
            }
        }
        photos.sort_by(|a, b| filter.sort.compare(a, b));
        Ok(photos)
    }

    pub fn names(&self) -> Result<Vec<String>, String> {
        let mut names: Vec<String> = Vec::new();
        for (_, name, _) in self.named_faces()? {
            if !name.is_empty() && !names.iter().any(|have| have.eq_ignore_ascii_case(&name)) {
                names.push(name);
            }
        }
        names.sort_by_key(|name| name.to_lowercase());
        Ok(names)
    }

    pub fn albums(&self) -> Result<Vec<(String, String)>, String> {
        let mut statement =
            self.home.prepare("SELECT key, name FROM albums ORDER BY name COLLATE NOCASE").map_err(text)?;
        let rows = statement.query_map([], |row| Ok((row.get(0)?, row.get(1)?))).map_err(text)?;
        rows.collect::<Result<_, _>>().map_err(text)
    }

    pub fn create_album(&self, name: &str) -> Result<String, String> {
        let name = album_name(name)?;

        let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_err(text)?.as_nanos();
        let key = format!("{nanos:x}");
        self.home
            .execute("INSERT INTO albums (key, name) VALUES (?1, ?2)", params![key, name])
            .map_err(|err| album_taken(err, name))?;
        Ok(key)
    }

    pub fn rename_album(&self, key: &str, name: &str) -> Result<(), String> {
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
        self.home.execute("DELETE FROM albums WHERE key = ?1", params![key]).map_err(text)?;
        Ok(())
    }

    pub fn set_in_album(&self, key: &str, photo_ids: &[i64], inside: bool) -> Result<(), String> {
        let sql = if inside {
            "INSERT OR IGNORE INTO album_photos (album, photo_id) VALUES (?1, ?2)"
        } else {
            "DELETE FROM album_photos WHERE album = ?1 AND photo_id = ?2"
        };
        for &id in photo_ids {
            let (open, local) = self.photo(id)?;
            open.conn.execute(sql, params![key, local]).map_err(text)?;
        }
        Ok(())
    }

    pub fn photos(&self, library_id: i64, filter: &Filter) -> Result<Vec<Photo>, String> {
        let open = self.library(library_id)?;

        let best = "a.burst IS NOT NULL \
                    AND COUNT(*) OVER (PARTITION BY a.burst) > 1 \
                    AND a.sharpness = MAX(a.sharpness) OVER (PARTITION BY a.burst)";

        let mut sql = format!(
            "SELECT * FROM (\
               SELECT p.id, p.path, p.mtime, p.rating, p.flag, a.sharpness, a.blown, \
                      ({best}) AS best_of_burst, \
                      a.faces, a.face_sharpness, a.suggested, p.edits IS NOT NULL AS edited \
               FROM photos p LEFT JOIN analysis a ON a.photo_id = p.id \
               WHERE p.rating >= ?1 \
                 AND (?2 IS NULL OR p.id IN (SELECT photo_id FROM album_photos WHERE album = ?2))"
        );
        if let Some(flag) = filter.flag {
            sql.push_str(&format!(" AND p.flag = {}", flag.to_i64()));
        }
        if filter.only_questionable {
            sql.push_str(&format!(" AND (a.sharpness < {} OR a.blown > {})", crate::cull::SOFT, crate::cull::BLOWN));
        }

        if let Some(person) = &filter.person {

            let ids = self
                .people(library_id)?
                .into_iter()
                .find(|(name, _)| name.eq_ignore_ascii_case(person))
                .map(|(_, photos)| photos)
                .unwrap_or_default();
            let list = ids.iter().map(|id| split_id(*id).1.to_string()).collect::<Vec<_>>().join(",");
            sql.push_str(&format!(" AND p.id IN ({list})"));
        }

        sql.push(')');
        if filter.best_of_burst {
            sql.push_str(" WHERE best_of_burst");
        }
        sql.push_str(" ORDER BY ");
        sql.push_str(filter.sort.order_by());

        let mut stmt = open.conn.prepare(&sql).map_err(text)?;
        let rows = stmt
            .query_map(params![filter.min_rating, filter.album], |row| {
                Ok(Photo {
                    id: global_id(library_id, row.get(0)?),
                    path: open.absolute(&row.get::<_, String>(1)?),
                    mtime: row.get(2)?,
                    rating: row.get::<_, i64>(3)?.clamp(0, 5) as u8,
                    flag: Flag::from_i64(row.get(4)?),
                    sharpness: row.get(5)?,
                    blown: row.get(6)?,
                    best_of_burst: row.get::<_, Option<i64>>(7)?.unwrap_or(0) == 1,
                    faces: row.get::<_, Option<i64>>(8)?.map(|count| count as u32),
                    face_sharpness: row.get(9)?,
                    suggested: row.get(10)?,
                    edited: row.get(11)?,
                })
            })
            .map_err(text)?;

        rows.filter(|row| row.as_ref().map_or(true, |photo| filter.file_type.admits(&photo.path)))
            .collect::<Result<_, _>>()
            .map_err(text)
    }

    pub fn set_rating(&self, photo_id: i64, rating: u8) -> Result<(), String> {
        let (open, local) = self.photo(photo_id)?;
        open.conn
            .execute("UPDATE photos SET rating = ?1 WHERE id = ?2", params![rating.min(5), local])
            .map_err(text)?;
        Ok(())
    }

    pub fn set_flag(&self, photo_id: i64, flag: Flag) -> Result<(), String> {
        let (open, local) = self.photo(photo_id)?;
        open.conn
            .execute("UPDATE photos SET flag = ?1 WHERE id = ?2", params![flag.to_i64(), local])
            .map_err(text)?;
        Ok(())
    }

    pub fn load_edits(&self, photo_id: i64) -> Result<Option<Document>, String> {
        let (open, local) = self.photo(photo_id)?;
        let json: Option<String> = open
            .conn
            .query_row("SELECT edits FROM photos WHERE id = ?1", params![local], |row| row.get(0))
            .map_err(text)?;

        match json {
            Some(text) => serde_json::from_str(&text).map(Some).map_err(|err| err.to_string()),
            None => Ok(None),
        }
    }

    pub fn load_history(&self, photo_id: i64) -> Result<Option<(Vec<Document>, usize)>, String> {
        let (open, local) = self.photo(photo_id)?;
        let row: Option<(i64, String)> = open
            .conn
            .query_row("SELECT position, states FROM history WHERE photo_id = ?1", params![local], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })
            .optional()
            .map_err(text)?;
        Ok(row.and_then(|(position, states)| {
            let states: Vec<Document> = serde_json::from_str(&states).ok()?;
            let last = states.len().checked_sub(1)?;
            Some((states, (position.max(0) as usize).min(last)))
        }))
    }

    pub fn save_history(&self, photo_id: i64, states: &[Document], position: usize) -> Result<(), String> {
        let (open, local) = self.photo(photo_id)?;
        let json = serde_json::to_string(states).map_err(text)?;
        open.conn
            .execute(
                "INSERT INTO history (photo_id, position, states) VALUES (?1, ?2, ?3) \
                 ON CONFLICT(photo_id) DO UPDATE SET position = excluded.position, states = excluded.states",
                params![local, position as i64, json],
            )
            .map_err(text)?;
        Ok(())
    }

    pub fn snapshots(&self, photo_id: i64) -> Result<Vec<(String, i64)>, String> {
        let (open, local) = self.photo(photo_id)?;
        let mut statement = open
            .conn
            .prepare("SELECT name, created FROM snapshots WHERE photo_id = ?1 ORDER BY created, rowid")
            .map_err(text)?;
        let rows = statement.query_map(params![local], |row| Ok((row.get(0)?, row.get(1)?))).map_err(text)?;
        rows.collect::<Result<_, _>>().map_err(text)
    }

    pub fn save_snapshot(&self, photo_id: i64, name: &str, document: &Document) -> Result<(), String> {
        let name = snapshot_name(name)?;
        let json = serde_json::to_string(document).map_err(text)?;
        let created = std::time::SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs() as i64);
        let (open, local) = self.photo(photo_id)?;
        open.conn
            .execute(
                "INSERT INTO snapshots (photo_id, name, created, edits) VALUES (?1, ?2, ?3, ?4)",
                params![local, name, created, json],
            )
            .map_err(|err| taken(err, name))?;
        Ok(())
    }

    pub fn load_snapshot(&self, photo_id: i64, name: &str) -> Result<Document, String> {
        let (open, local) = self.photo(photo_id)?;
        let json: String = open
            .conn
            .query_row(
                "SELECT edits FROM snapshots WHERE photo_id = ?1 AND name = ?2",
                params![local, name],
                |row| row.get(0),
            )
            .map_err(text)?;
        serde_json::from_str(&json).map_err(text)
    }

    pub fn rename_snapshot(&self, photo_id: i64, from: &str, to: &str) -> Result<(), String> {
        let to = snapshot_name(to)?;
        let (open, local) = self.photo(photo_id)?;
        open.conn
            .execute("UPDATE snapshots SET name = ?1 WHERE photo_id = ?2 AND name = ?3", params![to, local, from])
            .map_err(|err| taken(err, to))?;
        Ok(())
    }

    pub fn delete_snapshot(&self, photo_id: i64, name: &str) -> Result<(), String> {
        let (open, local) = self.photo(photo_id)?;
        open.conn
            .execute("DELETE FROM snapshots WHERE photo_id = ?1 AND name = ?2", params![local, name])
            .map_err(text)?;
        Ok(())
    }

    pub fn save_edits(&self, photo_id: i64, document: &Document) -> Result<(), String> {
        let json = match document.is_untouched() {
            true => None,
            false => Some(serde_json::to_string(document).map_err(text)?),
        };
        let (open, local) = self.photo(photo_id)?;
        open.conn.execute("UPDATE photos SET edits = ?1 WHERE id = ?2", params![json, local]).map_err(text)?;
        Ok(())
    }
}

fn album_name(name: &str) -> Result<&str, String> {
    match name.trim() {
        "" => Err("An album needs a name".to_string()),
        name => Ok(name),
    }
}

fn album_taken(err: rusqlite::Error, name: &str) -> String {
    match err.sqlite_error_code() {
        Some(rusqlite::ErrorCode::ConstraintViolation) => format!("There is already an album called “{name}”"),
        _ => err.to_string(),
    }
}

fn snapshot_name(name: &str) -> Result<&str, String> {
    match name.trim() {
        "" => Err("A snapshot needs a name".to_string()),
        name => Ok(name),
    }
}

fn taken(err: rusqlite::Error, name: &str) -> String {
    match err.sqlite_error_code() {
        Some(rusqlite::ErrorCode::ConstraintViolation) => format!("There is already a snapshot called “{name}”"),
        _ => err.to_string(),
    }
}

fn write_analysis(
    conn: &Connection,
    local: i64,
    version: i64,
    frame: &crate::cull::Frame,
    faces: Option<u32>,
    face_sharpness: Option<f32>,
) -> Result<(), String> {
    conn.execute(
        "INSERT INTO analysis \
           (photo_id, version, sharpness, blown, hash, faces, face_sharpness, \
            brightness, contrast, colourfulness) \
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10) \
         ON CONFLICT(photo_id) DO UPDATE SET \
           version = ?2, sharpness = ?3, blown = ?4, hash = ?5, \
           faces = ?6, face_sharpness = ?7, burst = NULL, best = 0, suggested = NULL, \
           brightness = ?8, contrast = ?9, colourfulness = ?10",
        params![
            local,
            version,
            frame.sharpness,
            frame.blown,
            frame.hash as i64,
            faces.map(i64::from),
            face_sharpness,
            frame.brightness,
            frame.contrast,
            frame.colourfulness,
        ],
    )
    .map_err(text)?;
    Ok(())
}

fn table_exists(conn: &Connection, table: &str) -> bool {
    conn.query_row(
        "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = ?1",
        params![table],
        |_| Ok(()),
    )
    .is_ok()
}

fn back_up_weekly(conn: &Connection, dir: &Path) {

    if conn.query_row("SELECT 1 FROM photos LIMIT 1", [], |_| Ok(())).is_err() {
        return;
    }
    let backups = dir.join("backups");
    if let Err(err) = std::fs::create_dir_all(&backups) {
        log::warn!("{}: {err}", backups.display());
        return;
    }
    let mut existing: Vec<PathBuf> = std::fs::read_dir(&backups)
        .map(|entries| {
            entries
                .flatten()
                .map(|entry| entry.path())
                .filter(|path| path.extension().is_some_and(|ext| ext == "db"))
                .collect()
        })
        .unwrap_or_default();

    existing.sort();

    let now = std::time::SystemTime::now();
    let recent = existing.last().and_then(|newest| std::fs::metadata(newest).ok()?.modified().ok());
    if recent.is_some_and(|taken| now.duration_since(taken).is_ok_and(|age| age < BACKUP_EVERY)) {
        return;
    }

    let days = now.duration_since(UNIX_EPOCH).map(|since| since.as_secs() / 86_400).unwrap_or(0) as i64;
    let (year, month, day) = civil_date(days);
    let target = backups.join(format!("catalog-{year:04}-{month:02}-{day:02}.db"));
    if target.exists() {
        return;
    }
    if let Err(err) = conn.execute("VACUUM INTO ?1", params![target.to_string_lossy()]) {
        log::warn!("could not back up {}: {err}", dir.display());
        return;
    }
    existing.push(target);
    while existing.len() > BACKUPS_KEPT {
        let oldest = existing.remove(0);
        if let Err(err) = std::fs::remove_file(&oldest) {
            log::warn!("{}: {err}", oldest.display());
        }
    }
}

fn civil_date(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    (yoe + era * 400 + i64::from(month <= 2), month, day)
}

#[derive(Debug, Default)]
pub struct Scan {
    pub files: Vec<(PathBuf, i64)>,
    pub complete: bool,
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Changes {
    pub added: usize,
    pub updated: usize,
    pub removed: usize,
}

impl Changes {
    pub fn any(&self) -> bool {
        self.added + self.updated + self.removed > 0
    }
}

pub fn scan(root: &Path) -> Scan {
    let (paths, complete) = walk_images(root);
    Scan { files: paths.into_iter().map(|path| { let mtime = mtime_secs(&path); (path, mtime) }).collect(), complete }
}

fn mtime_secs(path: &Path) -> i64 {
    std::fs::metadata(path)
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|delta| delta.as_secs() as i64)
        .unwrap_or(0)
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Copied {
    pub photos: usize,

    pub existing: Vec<String>,
    pub failed: Vec<String>,
}

pub fn copy_into(dropped: &[PathBuf], folder: &Path) -> Copied {
    let mut copied = Copied::default();
    let mut pending: Vec<(PathBuf, PathBuf)> =
        dropped.iter().filter_map(|from| Some((from.clone(), folder.join(from.file_name()?)))).collect();

    while let Some((from, to)) = pending.pop() {
        let name = || to.strip_prefix(folder).unwrap_or(&to).display().to_string();
        if from.is_dir() {

            if folder.starts_with(&from) || from.file_name().is_some_and(|name| name == LIBRARY_DIR) {
                continue;
            }
            let Ok(entries) = std::fs::read_dir(&from) else {
                copied.failed.push(name());
                continue;
            };
            pending.extend(entries.flatten().map(|entry| (entry.path(), to.join(entry.file_name()))));
        } else if raw::is_supported(&from) {
            if to.exists() {
                if from != to {
                    copied.existing.push(name());
                }
                continue;
            }
            match copy_whole(&from, &to) {
                Ok(()) => copied.photos += 1,
                Err(err) => {
                    log::warn!("{}: {err}", from.display());
                    copied.failed.push(name());
                }
            }
        }
    }
    copied
}

fn copy_whole(from: &Path, to: &Path) -> std::io::Result<()> {
    if let Some(parent) = to.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut part = to.as_os_str().to_owned();
    part.push(".part");
    let part = PathBuf::from(part);
    let result = std::fs::copy(from, &part).and_then(|_| {
        let modified = std::fs::metadata(from)?.modified()?;
        std::fs::File::options().write(true).open(&part)?.set_modified(modified)?;
        std::fs::rename(&part, to)
    });
    if result.is_err() {
        let _ = std::fs::remove_file(&part);
    }
    result
}

fn walk_images(root: &Path) -> (Vec<PathBuf>, bool) {
    let mut found = Vec::new();
    let mut complete = true;
    let mut pending = vec![root.to_path_buf()];

    while let Some(dir) = pending.pop() {
        let entries = match std::fs::read_dir(&dir) {
            Ok(entries) => entries,
            Err(err) => {
                log::warn!("{}: {err}", dir.display());
                complete = false;
                continue;
            }
        };
        for entry in entries {
            let entry = match entry {
                Ok(entry) => entry,
                Err(err) => {
                    log::warn!("{}: {err}", dir.display());
                    complete = false;
                    continue;
                }
            };
            let path = entry.path();
            if path.is_dir() {

                if path.file_name().is_some_and(|name| name == "edited" || name == LIBRARY_DIR) {
                    continue;
                }
                pending.push(path);
            } else if raw::is_supported(&path) {
                found.push(path);
            }
        }
    }

    (found, complete)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::document::Basic;

    unsafe extern "C" {
        #[link_name = "geteuid"]
        fn libc_geteuid() -> u32;
    }

    #[test]
    fn a_read_only_folder_is_refused_and_not_listed() {
        if unsafe { libc_geteuid() } == 0 {
            eprintln!("skipped: root writes anywhere");
            return;
        }
        use std::os::unix::fs::PermissionsExt;
        let root = temp_dir("read-only");
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o555)).unwrap();

        let catalog = Catalog::in_memory().unwrap();
        let refused = catalog.add_library(&root);
        std::fs::set_permissions(&root, std::fs::Permissions::from_mode(0o755)).unwrap();

        assert!(refused.unwrap_err().contains("cannot create"));
        assert!(catalog.libraries().unwrap().is_empty());
        std::fs::remove_dir_all(&root).unwrap();
    }

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("numa-test-{}", name));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("edited")).unwrap();
        dir
    }

    #[test]
    fn a_mask_survives_the_catalog_with_everything_on_it() {
        use crate::core::mask::{Mask, RegionPoint, Shape, Stroke};

        let root = temp_dir("edits");
        std::fs::write(root.join("a.RAF"), b"x").unwrap();
        let catalog = Catalog::in_memory().unwrap();
        let library = catalog.add_library(&root).unwrap();
        catalog.sync_library(&library).unwrap();
        let photo = catalog.photos(library.id, &Filter::default()).unwrap().remove(0);

        let mut mask = Mask::new(Shape::Segment { classes: vec![2, 4] });
        mask.basic = Basic { exposure: -1.5, ..Default::default() };
        mask.inverted = true;
        mask.points.push(RegionPoint { at: [0.25, 0.75], subtract: true, enabled: true });
        let mut stroke = Stroke::new(0.08, 0.5, false);
        stroke.points = vec![[0.1, 0.1], [0.2, 0.3]];
        mask.strokes.push(stroke);
        mask.strokes.push(Stroke::lasso(true));

        let mut document = Document::new(photo.path.to_string_lossy().to_string());
        document.set_masks(vec![mask.clone()]);
        catalog.save_edits(photo.id, &document).unwrap();

        let loaded = catalog.load_edits(photo.id).unwrap().expect("the mask was not stored");
        let back = loaded.masks();
        assert_eq!(back.len(), 1);
        assert_eq!(back[0].shape, mask.shape, "the classes it was made of");
        assert_eq!(back[0].points, mask.points, "the clicks");
        assert_eq!(back[0].strokes, mask.strokes, "the brush and the lasso");
        assert_eq!(back[0].inverted, true);
        assert_eq!(back[0].basic.exposure, -1.5);

        assert!(back[0].map.0.is_none());
        assert!(back[0].is_pending(), "and the mask knows it is waiting for them");
    }

    #[test]
    fn a_name_given_a_face_is_kept_corrected_and_taken_back() {
        let root = temp_dir("people");
        for name in ["a.RAF", "b.RAF"] {
            std::fs::write(root.join(name), b"x").unwrap();
        }
        let catalog = Catalog::in_memory().unwrap();
        let library = catalog.add_library(&root).unwrap();
        catalog.sync_library(&library).unwrap();
        let photos = catalog.photos(library.id, &Filter::default()).unwrap();

        let face = |axis: usize, lean: f32| {
            let mut v = [0.0f32; people::LENGTH];
            v[axis] = 1.0;
            v[people::LENGTH - 1] = lean;
            let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
            v.map(|x| x / norm)
        };

        catalog.name_face(photos[0].id, &face(0, 0.0), "Anna").unwrap();
        catalog.name_face(photos[0].id, &face(1, 0.0), "Bram").unwrap();
        catalog.name_face(photos[1].id, &face(0, 0.2), "anna").unwrap();
        let named = catalog.named_faces().unwrap();
        assert_eq!(named.len(), 3);
        assert!(
            named.iter().filter(|(_, name, _)| name == "Anna").count() == 2,
            "one person whatever the case it was typed in: {:?}",
            named.iter().map(|(_, n, _)| n).collect::<Vec<_>>()
        );

        catalog.name_face(photos[0].id, &face(1, 0.1), "Bas").unwrap();
        let named = catalog.named_faces().unwrap();
        assert_eq!(named.len(), 3);
        assert!(named.iter().any(|(_, name, _)| name == "Bas"));
        assert!(!named.iter().any(|(_, name, _)| name == "Bram"), "and nobody is left called Bram");

        catalog.name_face(photos[1].id, &face(0, 0.2), "  ").unwrap();
        assert_eq!(catalog.named_faces().unwrap().len(), 2);
    }

    #[test]
    fn a_person_is_every_photograph_with_a_face_like_theirs() {
        let root = temp_dir("people-grid");
        for name in ["a.RAF", "b.RAF", "c.RAF"] {
            std::fs::write(root.join(name), b"x").unwrap();
        }
        let catalog = Catalog::in_memory().unwrap();
        let library = catalog.add_library(&root).unwrap();
        catalog.sync_library(&library).unwrap();
        let photos = catalog.photos(library.id, &Filter::default()).unwrap();
        let face = |axis: usize, lean: f32| {
            let mut v = [0.0f32; people::LENGTH];
            v[axis] = 1.0;
            v[people::LENGTH - 1] = lean;
            let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
            v.map(|x| x / norm)
        };
        let frame = crate::cull::Frame { sharpness: 1.0, blown: 0.0, hash: 0, ..Default::default() };

        catalog
            .save_analysis_batch(
                crate::cull::VERSION,
                &[
                    (photos[0].id, frame, Some(1), None, vec![(face(0, 0.0), None)]),
                    (photos[1].id, frame, Some(2), None, vec![(face(0, 0.4), None), (face(5, 0.0), Some(vec![1, 2, 3]))]),
                    (photos[2].id, frame, Some(1), None, vec![(face(5, 0.1), None)]),
                ],
            )
            .unwrap();
        assert!(catalog.people(library.id).unwrap().is_empty(), "nobody is named yet");

        catalog.name_face(photos[0].id, &face(0, 0.0), "Anna").unwrap();
        let everyone = catalog.people(library.id).unwrap();
        assert_eq!(everyone.len(), 1);
        let mut anna = everyone[0].1.clone();
        anna.sort();
        let mut expected = vec![photos[0].id, photos[1].id];
        expected.sort();
        assert_eq!(anna, expected);

        let faces = catalog.faces(library.id).unwrap();
        assert_eq!(faces.len(), 4);
        assert_eq!(faces.iter().filter(|face| face.portrait.is_some()).count(), 1);
        let bare = faces.iter().find(|face| face.portrait.is_none()).unwrap();
        catalog.set_portrait(bare.id, &[9]).unwrap();
        assert_eq!(catalog.faces(library.id).unwrap().iter().filter(|face| face.portrait.is_some()).count(), 2);

        catalog.rename_person("Anna", "Anne").unwrap();
        assert_eq!(catalog.people(library.id).unwrap()[0].0, "Anne");
        catalog.name_face(photos[2].id, &face(5, 0.1), "Bram").unwrap();
        catalog.rename_person("Bram", "anne").unwrap();
        let everyone = catalog.people(library.id).unwrap();
        assert_eq!(everyone.len(), 1, "one person now: {everyone:?}");
        catalog.rename_person("Anne", "").unwrap();
        assert!(catalog.people(library.id).unwrap().is_empty());

        catalog.ignore_faces(&[(photos[2].id, face(5, 0.1))]).unwrap();
        assert_eq!(catalog.ignored_count(library.id).unwrap(), 1);
        let known = catalog.known_faces().unwrap();
        assert_eq!(people::recognise(&face(5, 0.0), &known).map(|(n, _)| n), Some(""));
        catalog.unignore_all(library.id).unwrap();
        assert!(catalog.known_faces().unwrap().is_empty());
        catalog.name_face(photos[0].id, &face(0, 0.0), "Anna").unwrap();

        let filter = Filter { person: Some("anna".to_string()), ..Filter::default() };
        let shown: Vec<i64> = catalog.photos(library.id, &filter).unwrap().iter().map(|p| p.id).collect();
        assert_eq!(shown.len(), 2);
        assert!(!shown.contains(&photos[2].id));

        catalog
            .save_analysis_batch(crate::cull::VERSION, &[(photos[1].id, frame, Some(0), None, vec![])])
            .unwrap();
        assert_eq!(catalog.photos(library.id, &filter).unwrap().len(), 1);
    }

    #[test]
    fn analysis_filters_the_grid_without_touching_the_stars() {
        let root = temp_dir("cull");
        for name in ["a.RAF", "b.RAF", "c.RAF"] {
            std::fs::write(root.join(name), b"x").unwrap();
        }

        let catalog = Catalog::in_memory().unwrap();
        let library = catalog.add_library(&root).unwrap();
        catalog.sync_library(&library).unwrap();

        let all = catalog.photos(library.id, &Filter::default()).unwrap();
        assert_eq!(all.len(), 3);
        assert!(all.iter().all(|photo| photo.sharpness.is_none()), "nothing measured yet");

        assert_eq!(catalog.unanalysed(library.id, crate::cull::VERSION).unwrap().len(), 3);

        let frames = [
            crate::cull::Frame { sharpness: 0.9, blown: 0.0, hash: 0b0011, ..Default::default() },
            crate::cull::Frame { sharpness: 0.1, blown: 0.0, hash: 0b0001, ..Default::default() },
            crate::cull::Frame { sharpness: 0.7, blown: 0.5, hash: u64::MAX, ..Default::default() },
        ];
        for (photo, frame) in all.iter().zip(frames.iter()) {
            catalog.set_rating(photo.id, 3).unwrap();
            catalog.save_analysis(photo.id, crate::cull::VERSION, frame, None, None).unwrap();
        }
        assert!(catalog.unanalysed(library.id, crate::cull::VERSION).unwrap().is_empty());

        let analysed = catalog.analysed(library.id).unwrap();
        assert_eq!(analysed.len(), 3);
        assert_eq!(analysed[0].1.hash, 0b0011, "hashes must survive the round trip");

        let hashes: Vec<u64> = analysed.iter().map(|(_, frame, _)| frame.hash).collect();
        let groups = crate::cull::bursts(&hashes, crate::cull::BURST_TOLERANCE);
        let best = crate::cull::best_of_each(&frames, &groups);
        let rows: Vec<(i64, usize, bool, f32)> = analysed
            .iter()
            .enumerate()
            .map(|(index, (id, frame, face))| {
                let best = best.contains(&index);
                (*id, groups[index], best, crate::cull::suggestion(frame, *face, best))
            })
            .collect();
        catalog.save_bursts(&rows).unwrap();

        let kept = catalog
            .photos(library.id, &Filter { best_of_burst: true, ..Default::default() })
            .unwrap();
        assert_eq!(kept.len(), 1, "a lone frame is not the best of anything");
        assert!(kept[0].best_of_burst);
        assert!(kept[0].path.ends_with("a.RAF"), "the sharp one of the pair");

        let questionable = catalog
            .photos(library.id, &Filter { only_questionable: true, ..Default::default() })
            .unwrap();
        let names: Vec<String> = questionable
            .iter()
            .map(|photo| photo.path.file_name().unwrap().to_string_lossy().to_string())
            .collect();
        assert_eq!(names.len(), 2, "got {names:?}");
        assert!(names.contains(&"b.RAF".to_string()) && names.contains(&"c.RAF".to_string()));

        let sorted = catalog
            .photos(library.id, &Filter { sort: Sort::Sharpness, ..Default::default() })
            .unwrap();
        assert!(sorted[0].path.ends_with("a.RAF") && sorted[2].path.ends_with("b.RAF"));

        assert!(sorted.iter().all(|photo| photo.rating == 3), "a suggestion wrote a rating");

        assert_eq!(catalog.unanalysed(library.id, crate::cull::VERSION + 1).unwrap().len(), 3);
        assert_eq!(catalog.analysed(library.id).unwrap().len(), 3);
    }

    #[test]
    fn an_unreadable_subfolder_does_not_count_as_an_empty_one() {
        use std::os::unix::fs::PermissionsExt;

        if unsafe { libc_geteuid() } == 0 {
            eprintln!("skipped: root reads unreadable directories");
            return;
        }

        let root = temp_dir("unreadable");
        let inner = root.join("shoot");
        std::fs::create_dir_all(&inner).unwrap();
        std::fs::write(root.join("a.RAF"), b"x").unwrap();
        std::fs::write(inner.join("b.RAF"), b"x").unwrap();

        let catalog = Catalog::in_memory().unwrap();
        let library = catalog.add_library(&root).unwrap();
        assert_eq!(catalog.sync_library(&library).unwrap(), 2);

        let rated = catalog
            .photos(library.id, &Filter::default())
            .unwrap()
            .into_iter()
            .find(|photo| photo.path.ends_with("b.RAF"))
            .unwrap();
        catalog.set_rating(rated.id, 5).unwrap();

        std::fs::set_permissions(&inner, std::fs::Permissions::from_mode(0o000)).unwrap();
        let rescan = catalog.sync_library(&library);
        std::fs::set_permissions(&inner, std::fs::Permissions::from_mode(0o755)).unwrap();
        rescan.unwrap();

        let left = catalog.photos(library.id, &Filter::default()).unwrap();
        assert_eq!(left.len(), 2, "an unreadable folder is not a deleted one");
        assert_eq!(
            left.iter().find(|photo| photo.path.ends_with("b.RAF")).unwrap().rating,
            5,
            "the rating survived a walk that could not see the file"
        );

        std::fs::remove_file(inner.join("b.RAF")).unwrap();
        catalog.sync_library(&library).unwrap();
        assert_eq!(catalog.photos(library.id, &Filter::default()).unwrap().len(), 1);
    }

    #[test]
    fn best_of_burst_is_computed_and_cannot_go_stale() {
        let root = temp_dir("stale-burst");
        for name in ["a.RAF", "b.RAF", "c.RAF"] {
            std::fs::write(root.join(name), b"x").unwrap();
        }

        let catalog = Catalog::in_memory().unwrap();
        let library = catalog.add_library(&root).unwrap();
        catalog.sync_library(&library).unwrap();
        let all = catalog.photos(library.id, &Filter::default()).unwrap();

        let frames = [
            crate::cull::Frame { sharpness: 0.9, blown: 0.0, hash: 1, ..Default::default() },
            crate::cull::Frame { sharpness: 0.3, blown: 0.0, hash: 1, ..Default::default() },
            crate::cull::Frame { sharpness: 0.7, blown: 0.0, hash: 2, ..Default::default() },
        ];
        for (photo, frame) in all.iter().zip(frames.iter()) {
            catalog.save_analysis(photo.id, crate::cull::VERSION, frame, None, None).unwrap();
        }

        catalog
            .save_bursts(&[
                (all[0].id, 0, true, 3.0),
                (all[1].id, 0, false, 1.0),
                (all[2].id, 1, true, 2.0),
            ])
            .unwrap();

        let listed = catalog.photos(library.id, &Filter::default()).unwrap();
        let marked: Vec<&Photo> = listed.iter().filter(|photo| photo.best_of_burst).collect();
        assert_eq!(marked.len(), 1, "the stored flag leaked through");
        assert!(marked[0].path.ends_with("a.RAF"), "the sharper of the real pair");

        let filtered = catalog
            .photos(library.id, &Filter { best_of_burst: true, ..Default::default() })
            .unwrap();
        assert_eq!(filtered.len(), 1);
        assert!(filtered[0].path.ends_with("a.RAF"));
    }

    #[test]
    fn remembered_settings_come_back_and_rubbish_does_not() {
        let catalog = Catalog::in_memory().unwrap();

        let filter = Filter {
            min_rating: 3,
            flag: Some(Flag::Picked),
            sort: Sort::Sharpness,
            best_of_burst: true,
            only_questionable: false,
            person: None,
            file_type: FileType::Raw,
            album: Some("5f".to_string()),
            all_libraries: true,
        };
        catalog.remember("filter", &filter);
        let back: Filter = catalog.recall("filter").expect("it comes back");
        assert_eq!(back.min_rating, 3);
        assert_eq!(back.flag, Some(Flag::Picked));
        assert_eq!(back.sort, Sort::Sharpness);
        assert!(back.best_of_burst);
        assert_eq!(back.file_type, FileType::Raw);
        assert_eq!(back.album.as_deref(), Some("5f"));
        assert!(back.all_libraries);

        assert!(catalog.recall::<Filter>("never-written").is_none());

        catalog.set_setting("filter", "not json at all").unwrap();
        assert!(catalog.recall::<Filter>("filter").is_none());
    }

    #[test]
    fn settings_survive_and_missing_ones_are_not_an_error() {
        let catalog = Catalog::in_memory().unwrap();
        assert_eq!(catalog.setting("last-library"), None);

        catalog.set_setting("last-library", "7").unwrap();
        assert_eq!(catalog.setting("last-library").as_deref(), Some("7"));

        catalog.set_setting("last-library", "9").unwrap();
        assert_eq!(catalog.setting("last-library").as_deref(), Some("9"));
    }

    #[test]
    fn libraries_can_be_named_counted_and_removed() {
        let root = temp_dir("manage");
        std::fs::write(root.join("a.RAF"), b"x").unwrap();
        std::fs::write(root.join("b.RAF"), b"x").unwrap();

        let catalog = Catalog::in_memory().unwrap();
        let library = catalog.add_library(&root).unwrap();
        catalog.sync_library(&library).unwrap();
        assert_eq!(catalog.photo_count(library.id).unwrap(), 2);

        let stored = |catalog: &Catalog| catalog.libraries().unwrap().remove(0);
        assert_eq!(stored(&catalog).label(), "numa-test-manage");

        catalog.set_library_name(library.id, "  Japan 2026  ").unwrap();
        assert_eq!(stored(&catalog).label(), "Japan 2026", "a name is trimmed");

        catalog.set_library_name(library.id, "   ").unwrap();
        assert_eq!(stored(&catalog).label(), "numa-test-manage");

        let photos = catalog.photos(library.id, &Filter::default()).unwrap();
        catalog.set_rating(photos[0].id, 5).unwrap();
        catalog.remove_library(library.id).unwrap();
        assert!(catalog.libraries().unwrap().is_empty());
        assert!(root.join(LIBRARY_DIR).join("catalog.db").exists(), "the folder keeps its catalog");
        let back = catalog.add_library(&root).unwrap();
        assert_eq!(catalog.photo_count(back.id).unwrap(), 2);
    }

    #[test]
    fn an_original_catalog_upgrades_without_losing_anything() {
        const ORIGINAL: &str = r#"
CREATE TABLE libraries (
    id   INTEGER PRIMARY KEY,
    path TEXT NOT NULL UNIQUE
);
CREATE TABLE photos (
    id         INTEGER PRIMARY KEY,
    library_id INTEGER NOT NULL REFERENCES libraries(id) ON DELETE CASCADE,
    path       TEXT NOT NULL UNIQUE,
    mtime      INTEGER NOT NULL,
    rating     INTEGER NOT NULL DEFAULT 0,
    flag       INTEGER NOT NULL DEFAULT 0,
    edits      TEXT
);
INSERT INTO libraries (id, path) VALUES (1, '/photos');
INSERT INTO photos (id, library_id, path, mtime, rating, flag, edits)
    VALUES (7, 1, '/photos/a.RAF', 99, 4, 1, '{"kept":true}');
"#;

        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(ORIGINAL).unwrap();
        conn.execute_batch(LEGACY_SCHEMA).unwrap();
        migrate(&conn).unwrap();

        let (rating, flag, edits): (i64, i64, String) = conn
            .query_row("SELECT rating, flag, edits FROM photos WHERE id = 7", [], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })
            .unwrap();
        assert_eq!((rating, flag, edits.as_str()), (4, 1, r#"{"kept":true}"#), "the row came across whole");
        let name: Option<String> = conn.query_row("SELECT name FROM libraries", [], |row| row.get(0)).unwrap();
        assert_eq!(name, None, "an upgraded library has no name yet");

        conn.execute("INSERT INTO libraries (id, path) VALUES (2, '/photos/2026')", []).unwrap();
        conn.execute("INSERT INTO photos (library_id, path, mtime) VALUES (2, '/photos/a.RAF', 99)", [])
            .expect("the same path in a second library");

        migrate(&conn).unwrap();
        let count: i64 = conn.query_row("SELECT COUNT(*) FROM photos", [], |row| row.get(0)).unwrap();
        assert_eq!(count, 2);

        let analysis: String = conn
            .query_row("SELECT sql FROM sqlite_master WHERE name = 'analysis'", [], |row| row.get(0))
            .unwrap();
        assert!(!analysis.contains("photos_old"), "analysis still points at the old table: {analysis}");
        conn.execute(
            "INSERT INTO analysis (photo_id, version, sharpness, blown, hash) VALUES (7, 1, 0.0, 0.0, 0)",
            [],
        )
        .expect("an analysis row can be written after the migration");
    }

    #[test]
    fn a_single_catalog_is_split_into_the_libraries_folders() {
        let home = temp_dir("split-home");
        let here = temp_dir("split-here");
        std::fs::write(here.join("a.RAF"), b"x").unwrap();
        std::fs::create_dir_all(here.join("day2")).unwrap();
        std::fs::write(here.join("day2").join("b.RAF"), b"x").unwrap();
        let gone = std::env::temp_dir().join("numa-test-split-unplugged");
        let _ = std::fs::remove_dir_all(&gone);

        let path = home.join("catalog.db");
        {
            let conn = Connection::open(&path).unwrap();
            conn.execute_batch(LEGACY_SCHEMA).unwrap();
            conn.execute(
                "INSERT INTO libraries (id, path, name) VALUES (1, ?1, 'Japan'), (2, ?2, NULL)",
                params![here.to_string_lossy(), gone.to_string_lossy()],
            )
            .unwrap();
            let edits = serde_json::to_string(&{
                let mut doc = Document::new("a".into());
                doc.set_basic(Basic { exposure: 0.7, ..Default::default() });
                doc
            })
            .unwrap();
            conn.execute(
                "INSERT INTO photos (id, library_id, path, mtime, rating, flag, edits) VALUES \
                 (11, 1, ?1, 5, 4, 1, ?2), (12, 1, ?3, 6, 2, 0, NULL), (13, 2, ?4, 7, 5, 0, NULL)",
                params![
                    here.join("a.RAF").to_string_lossy(),
                    edits,
                    here.join("day2").join("b.RAF").to_string_lossy(),
                    gone.join("c.RAF").to_string_lossy()
                ],
            )
            .unwrap();
            conn.execute_batch(
                "INSERT INTO analysis (photo_id, version, sharpness, blown, hash, faces) VALUES (11, 2, 0.5, 0.0, 9, 1); \
                 INSERT INTO people (id, name) VALUES (3, 'Anna'); \
                 INSERT INTO named_faces (photo_id, person_id, embedding) VALUES (11, 3, zeroblob(512)); \
                 INSERT INTO faces (photo_id, embedding) VALUES (11, zeroblob(512));",
            )
            .unwrap();
        }

        let catalog = Catalog::open(&path).unwrap();
        assert!(home.join("catalog-before-libraries.db").exists(), "the whole file is copied first");
        assert!(here.join(LIBRARY_DIR).join("catalog.db").exists(), "the library's own catalog is in its folder");
        let backups: Vec<_> = std::fs::read_dir(here.join(LIBRARY_DIR).join("backups")).unwrap().flatten().collect();
        assert_eq!(backups.len(), 1, "and a first copy of it, taken after the rows arrived");
        assert!(!gone.exists(), "nothing is created where an unplugged drive would be");

        let libraries = catalog.libraries().unwrap();
        assert_eq!(libraries[0].label(), "Japan", "the list of libraries stays at home, names and all");
        let japan = libraries.iter().find(|library| library.id == 1).unwrap();
        let photos = catalog.photos(japan.id, &Filter::default()).unwrap();
        assert_eq!(photos.len(), 2);
        let a = photos.iter().find(|photo| photo.path.ends_with("a.RAF")).unwrap();
        assert_eq!(a.path, here.join("a.RAF"), "paths come back whole");
        assert_eq!((a.rating, a.flag, a.faces), (4, Flag::Picked, Some(1)));
        assert_eq!(catalog.load_edits(a.id).unwrap().unwrap().basic().exposure, 0.7);
        assert!(photos.iter().any(|photo| photo.path == here.join("day2").join("b.RAF")));
        assert_eq!(catalog.named_faces().unwrap().len(), 1, "the name came across");

        let stored: String = catalog.library(1).unwrap().conn
            .query_row("SELECT path FROM photos WHERE id = 12", [], |row| row.get(0))
            .unwrap();
        assert_eq!(stored, "day2/b.RAF");

        let waiting: i64 = catalog.home
            .query_row("SELECT COUNT(*) FROM photos WHERE library_id = 2", [], |row| row.get(0))
            .unwrap();
        assert_eq!(waiting, 1);
        drop(catalog);

        std::fs::create_dir_all(&gone).unwrap();
        let catalog = Catalog::open(&path).unwrap();
        assert_eq!(catalog.photos(2, &Filter::default()).unwrap()[0].rating, 5);
        assert!(!table_exists(&catalog.home, "photos"), "nothing left to split");

        drop(catalog);
        let catalog = Catalog::open(&path).unwrap();
        assert_eq!(catalog.photos(1, &Filter::default()).unwrap().len(), 2);
        std::fs::remove_dir_all(&gone).unwrap();
    }

    #[test]
    fn a_library_folder_keeps_its_work_when_it_moves_or_is_removed() {
        let root = temp_dir("moving");
        std::fs::write(root.join("a.RAF"), b"x").unwrap();
        let catalog = Catalog::in_memory().unwrap();
        let library = catalog.add_library(&root).unwrap();
        catalog.sync_library(&library).unwrap();
        let photo = catalog.photos(library.id, &Filter::default()).unwrap().remove(0);
        catalog.set_rating(photo.id, 3).unwrap();

        catalog.remove_library(library.id).unwrap();
        let moved = std::env::temp_dir().join("numa-test-moved");
        let _ = std::fs::remove_dir_all(&moved);
        std::fs::rename(&root, &moved).unwrap();

        let again = catalog.add_library(&moved).unwrap();
        catalog.sync_library(&again).unwrap();
        let photos = catalog.photos(again.id, &Filter::default()).unwrap();
        assert_eq!(photos.len(), 1);
        assert_eq!(photos[0].rating, 3, "the rating travelled with the folder");
        assert_eq!(photos[0].path, moved.join("a.RAF"));
        std::fs::remove_dir_all(&moved).unwrap();
    }

    #[test]
    fn renaming_a_library_renames_its_folder() {
        let root = temp_dir("misnamed");
        std::fs::write(root.join("a.RAF"), b"x").unwrap();
        let taken = std::env::temp_dir().join("numa-test-taken");
        std::fs::create_dir_all(&taken).unwrap();
        let catalog = Catalog::in_memory().unwrap();
        let library = catalog.add_library(&root).unwrap();
        catalog.sync_library(&library).unwrap();
        catalog.set_library_name(library.id, "Stand-in").unwrap();
        let photo = catalog.photos(library.id, &Filter::default()).unwrap().remove(0);
        catalog.set_rating(photo.id, 4).unwrap();

        assert!(catalog.rename_library_folder(library.id, "a/b").is_err(), "a name, not a move");
        assert!(catalog.rename_library_folder(library.id, "numa-test-taken").is_err(), "not over another folder");
        assert!(root.is_dir(), "a refused rename leaves the folder alone");

        let renamed = std::env::temp_dir().join("numa-test-renamed");
        let _ = std::fs::remove_dir_all(&renamed);
        assert_eq!(catalog.rename_library_folder(library.id, " numa-test-renamed ").unwrap(), renamed);
        assert!(!root.exists() && renamed.join(LIBRARY_DIR).join("catalog.db").exists());
        let library = catalog.libraries().unwrap().into_iter().find(|l| l.id == library.id).unwrap();
        assert_eq!((library.path.as_path(), library.name.as_deref()), (renamed.as_path(), None));
        let photos = catalog.photos(library.id, &Filter::default()).unwrap();
        assert_eq!((photos[0].path.clone(), photos[0].rating), (renamed.join("a.RAF"), 4));

        std::fs::remove_dir_all(&renamed).unwrap();
        std::fs::remove_dir_all(&taken).unwrap();
    }

    #[test]
    fn dropped_photographs_are_copied_in_and_nothing_is_overwritten() {
        let library = temp_dir("drop-library");
        let outside = temp_dir("drop-outside");
        let old = UNIX_EPOCH + std::time::Duration::from_secs(1_600_000_000);
        std::fs::write(outside.join("new.jpg"), b"new").unwrap();
        std::fs::File::options().write(true).open(outside.join("new.jpg")).unwrap().set_modified(old).unwrap();
        std::fs::write(outside.join("notes.txt"), b"x").unwrap();
        std::fs::write(outside.join("taken.jpg"), b"theirs").unwrap();
        std::fs::write(library.join("taken.jpg"), b"ours").unwrap();
        std::fs::create_dir_all(outside.join("shoot")).unwrap();
        std::fs::write(outside.join("shoot").join("a.RAF"), b"raw").unwrap();

        let dropped = ["new.jpg", "notes.txt", "taken.jpg", "shoot"].map(|name| outside.join(name));
        let mut copied = copy_into(&dropped, &library);
        copied.existing.sort();
        assert_eq!(copied, Copied { photos: 2, existing: vec!["taken.jpg".into()], failed: vec![] });

        assert_eq!(std::fs::read(library.join("taken.jpg")).unwrap(), b"ours", "never overwritten");
        assert_eq!(std::fs::read(library.join("shoot").join("a.RAF")).unwrap(), b"raw", "a folder comes as a folder");
        assert!(!library.join("notes.txt").exists(), "only photographs");
        assert_eq!(std::fs::metadata(library.join("new.jpg")).unwrap().modified().unwrap(), old, "its time kept");
        assert!(std::fs::read_dir(&library).unwrap().flatten().all(|e| !e.path().to_string_lossy().ends_with(".part")));

        let again = copy_into(&[library.join("new.jpg"), library.clone()], &library);
        assert_eq!(again, Copied::default());

        std::fs::remove_dir_all(&library).unwrap();
        std::fs::remove_dir_all(&outside).unwrap();
    }

    #[test]
    fn a_rescan_says_what_changed() {
        let root = temp_dir("rescan");
        std::fs::write(root.join("a.RAF"), b"x").unwrap();
        let catalog = Catalog::in_memory().unwrap();
        let library = catalog.add_library(&root).unwrap();
        assert_eq!(catalog.apply_scan(&library, &scan(&root)).unwrap(), Changes { added: 1, updated: 0, removed: 0 });
        assert!(!catalog.apply_scan(&library, &scan(&root)).unwrap().any(), "nothing new");

        std::fs::write(root.join("b.jpg"), b"x").unwrap();
        std::fs::remove_file(root.join("a.RAF")).unwrap();
        assert_eq!(catalog.apply_scan(&library, &scan(&root)).unwrap(), Changes { added: 1, updated: 0, removed: 1 });
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn a_history_survives_closing_the_photograph() {
        let root = temp_dir("history");
        std::fs::write(root.join("a.RAF"), b"x").unwrap();
        let catalog = Catalog::in_memory().unwrap();
        let library = catalog.add_library(&root).unwrap();
        catalog.sync_library(&library).unwrap();
        let photo = catalog.photos(library.id, &Filter::default()).unwrap().remove(0);
        assert!(catalog.load_history(photo.id).unwrap().is_none());

        let untouched = Document::new("a.RAF".into());
        let mut cropped = untouched.clone();
        cropped.set_crop([0.1, 0.1, 0.8, 0.8], 0.0);
        catalog.save_history(photo.id, &[untouched.clone(), cropped.clone()], 1).unwrap();
        catalog.save_history(photo.id, &[untouched, cropped], 0).unwrap();

        let (states, position) = catalog.load_history(photo.id).unwrap().unwrap();
        assert_eq!((states.len(), position), (2, 0));
        assert_eq!(states[1].crop().map(|(rect, _)| rect), Some([0.1, 0.1, 0.8, 0.8]));
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn snapshots_are_kept_per_photograph() {
        let root = temp_dir("snapshots");
        std::fs::write(root.join("a.RAF"), b"x").unwrap();
        let catalog = Catalog::in_memory().unwrap();
        let library = catalog.add_library(&root).unwrap();
        catalog.sync_library(&library).unwrap();
        let photo = catalog.photos(library.id, &Filter::default()).unwrap().remove(0);
        assert!(catalog.snapshots(photo.id).unwrap().is_empty());

        let mut cropped = Document::new("a.RAF".into());
        cropped.set_crop([0.1, 0.1, 0.8, 0.8], 0.0);
        catalog.save_snapshot(photo.id, " Tight ", &cropped).unwrap();
        catalog.save_snapshot(photo.id, "Wide", &Document::new("a.RAF".into())).unwrap();
        assert!(catalog.save_snapshot(photo.id, "Tight", &cropped).unwrap_err().contains("already"));
        assert!(catalog.save_snapshot(photo.id, "  ", &cropped).is_err());
        let names = |c: &Catalog| c.snapshots(photo.id).unwrap().into_iter().map(|(n, _)| n).collect::<Vec<_>>();
        assert_eq!(names(&catalog), ["Tight", "Wide"]);
        let loaded = catalog.load_snapshot(photo.id, "Tight").unwrap();
        assert_eq!(loaded.crop().map(|(rect, _)| rect), Some([0.1, 0.1, 0.8, 0.8]));

        assert!(catalog.rename_snapshot(photo.id, "Tight", "Wide").unwrap_err().contains("already"));
        catalog.rename_snapshot(photo.id, "Tight", "Crop").unwrap();
        catalog.delete_snapshot(photo.id, "Wide").unwrap();
        assert_eq!(names(&catalog), ["Crop"]);
        assert!(catalog.load_snapshot(photo.id, "Crop").is_ok());

        std::fs::write(root.join("b.RAF"), b"x").unwrap();
        std::fs::remove_file(root.join("a.RAF")).unwrap();
        catalog.apply_scan(&library, &scan(&root)).unwrap();
        let (open, _) = catalog.photo(photo.id).unwrap();
        let left: i64 = open.conn.query_row("SELECT COUNT(*) FROM snapshots", [], |row| row.get(0)).unwrap();
        assert_eq!(left, 0);
        std::fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn a_week_old_catalog_is_backed_up_and_four_are_kept() {
        let dir = temp_dir("backups");
        let conn = Connection::open(dir.join("catalog.db")).unwrap();
        conn.execute_batch(LIBRARY_SCHEMA).unwrap();

        back_up_weekly(&conn, &dir);
        assert!(
            std::fs::read_dir(dir.join("backups")).map_or(true, |mut entries| entries.next().is_none()),
            "an empty catalog is not worth a copy"
        );
        conn.execute("INSERT INTO photos (path, mtime) VALUES ('a.RAF', 1)", []).unwrap();
        back_up_weekly(&conn, &dir);
        let taken = || std::fs::read_dir(dir.join("backups")).unwrap().count();
        assert_eq!(taken(), 1, "the first opening takes one");
        back_up_weekly(&conn, &dir);
        assert_eq!(taken(), 1, "and the second, the same week, does not");

        for day in 1..=6 {
            let old = dir.join("backups").join(format!("catalog-2020-01-0{day}.db"));
            std::fs::write(&old, b"old").unwrap();
            let file = std::fs::File::options().write(true).open(&old).unwrap();
            file.set_modified(UNIX_EPOCH).unwrap();
        }
        std::fs::remove_file(
            std::fs::read_dir(dir.join("backups"))
                .unwrap()
                .flatten()
                .map(|entry| entry.path())
                .find(|path| !path.to_string_lossy().contains("2020"))
                .unwrap(),
        )
        .unwrap();
        back_up_weekly(&conn, &dir);
        assert_eq!(taken(), BACKUPS_KEPT);
        assert!(
            std::fs::read_dir(dir.join("backups"))
                .unwrap()
                .flatten()
                .any(|entry| !entry.path().to_string_lossy().contains("2020")),
            "the newest is today's"
        );
    }

    #[test]
    fn days_since_1970_are_the_right_dates() {
        assert_eq!(civil_date(0), (1970, 1, 1));
        assert_eq!(civil_date(19_782), (2024, 2, 29));
        assert_eq!(civil_date(20_712), (2026, 9, 16));
    }

    #[test]
    fn a_nested_library_records_its_own_photos() {
        let root = temp_dir("nested");
        let inner = root.join("2026");
        std::fs::create_dir_all(&inner).unwrap();
        std::fs::write(inner.join("a.RAF"), b"x").unwrap();

        let catalog = Catalog::in_memory().unwrap();
        let outer = catalog.add_library(&root).unwrap();
        let nested = catalog.add_library(&inner).unwrap();

        assert_eq!(catalog.sync_library(&outer).unwrap(), 1);
        assert_eq!(catalog.sync_library(&nested).unwrap(), 1, "the same file in two libraries");

        assert_eq!(catalog.photos(outer.id, &Filter::default()).unwrap().len(), 1);
        assert_eq!(catalog.photos(nested.id, &Filter::default()).unwrap().len(), 1);
    }

    #[test]
    fn a_rescan_forgets_deleted_photos_but_not_a_missing_drive() {
        let root = temp_dir("prune");
        std::fs::write(root.join("a.RAF"), b"x").unwrap();
        std::fs::write(root.join("b.RAF"), b"x").unwrap();

        let catalog = Catalog::in_memory().unwrap();
        let library = catalog.add_library(&root).unwrap();
        assert_eq!(catalog.sync_library(&library).unwrap(), 2);

        std::fs::remove_file(root.join("b.RAF")).unwrap();
        catalog.sync_library(&library).unwrap();
        let left = catalog.photos(library.id, &Filter::default()).unwrap();
        assert_eq!(left.len(), 1);
        assert!(left[0].path.ends_with("a.RAF"));

        std::fs::remove_file(root.join("a.RAF")).unwrap();
        catalog.sync_library(&library).unwrap();
        assert_eq!(
            catalog.photos(library.id, &Filter::default()).unwrap().len(),
            1,
            "an empty root must not empty the catalog"
        );

        std::fs::remove_dir_all(&root).unwrap();
        catalog.sync_library(&library).unwrap();
        assert_eq!(
            catalog.photos(library.id, &Filter::default()).unwrap().len(),
            1,
            "an unreadable library must not empty the catalog"
        );
    }

    #[test]
    fn rates_flags_and_filters() {
        let root = temp_dir("catalog");
        std::fs::write(root.join("a.RAF"), b"x").unwrap();
        std::fs::write(root.join("b.jpg"), b"x").unwrap();
        std::fs::write(root.join("notes.txt"), b"x").unwrap();

        std::fs::write(root.join("._a.RAF"), b"x").unwrap();
        std::fs::write(root.join("edited").join("c.jpg"), b"x").unwrap();

        let catalog = Catalog::in_memory().unwrap();
        let library = catalog.add_library(&root).unwrap();
        assert_eq!(catalog.add_library(&root).unwrap().id, library.id, "adding twice must not duplicate");

        assert_eq!(catalog.sync_library(&library).unwrap(), 2);
        assert_eq!(catalog.sync_library(&library).unwrap(), 0, "rescan must not re-add");

        let all = catalog.photos(library.id, &Filter::default()).unwrap();
        assert_eq!(all.len(), 2);

        let raf = all.iter().find(|p| p.path.ends_with("a.RAF")).unwrap();
        catalog.set_rating(raf.id, 9).unwrap();
        catalog.set_flag(raf.id, Flag::Picked).unwrap();

        let starred = catalog
            .photos(library.id, &Filter { min_rating: 5, ..Default::default() })
            .unwrap();
        assert_eq!(starred.len(), 1, "rating 9 must clamp to 5 and still match");
        assert_eq!(starred[0].rating, 5);
        assert_eq!(starred[0].flag, Flag::Picked);

        let picked = catalog
            .photos(library.id, &Filter { flag: Some(Flag::Picked), ..Default::default() })
            .unwrap();
        assert_eq!(picked.len(), 1);

        let only = |file_type| catalog.photos(library.id, &Filter { file_type, ..Default::default() }).unwrap();
        assert!(matches!(only(FileType::Raw).as_slice(), [photo] if photo.path.ends_with("a.RAF")));
        assert!(matches!(only(FileType::NotRaw).as_slice(), [photo] if photo.path.ends_with("b.jpg")));

        let mut doc = Document::new(raf.path.to_string_lossy().to_string());
        assert!(catalog.load_edits(raf.id).unwrap().is_none());

        catalog.save_edits(raf.id, &doc).unwrap();
        assert!(catalog.load_edits(raf.id).unwrap().is_none(), "opening is not editing");
        let opened = catalog.photos(library.id, &Filter::default()).unwrap();
        assert!(opened.iter().all(|photo| !photo.edited));

        doc.set_basic(Basic { exposure: 0.7, ..Default::default() });
        catalog.save_edits(raf.id, &doc).unwrap();
        assert_eq!(catalog.load_edits(raf.id).unwrap().unwrap().basic().exposure, 0.7);
        let marked = catalog.photos(library.id, &Filter::default()).unwrap();
        assert_eq!(marked.iter().filter(|photo| photo.edited).count(), 1);

        catalog.save_edits(raf.id, &Document::new(raf.path.to_string_lossy().to_string())).unwrap();
        let cleared = catalog.photos(library.id, &Filter::default()).unwrap();
        assert!(cleared.iter().all(|photo| !photo.edited));

        assert_eq!(library.export_dir(), root.join("edited"));

        catalog.remove_library(library.id).unwrap();
        assert!(catalog.photos(library.id, &Filter::default()).is_err(), "a removed library is not shown");

        std::fs::remove_dir_all(&root).unwrap();
    }

    fn two_libraries(catalog: &Catalog, name: &str) -> (Library, Library) {
        let first = temp_dir(&format!("{name}-1"));
        let second = temp_dir(&format!("{name}-2"));
        for file in ["a.RAF", "b.RAF"] {
            std::fs::write(first.join(file), b"x").unwrap();
        }
        for file in ["c.RAF", "d.JPG"] {
            std::fs::write(second.join(file), b"x").unwrap();
        }
        let first = catalog.add_library(&first).unwrap();
        let second = catalog.add_library(&second).unwrap();
        catalog.sync_library(&first).unwrap();
        catalog.sync_library(&second).unwrap();
        (first, second)
    }

    fn names_of(photos: &[Photo]) -> Vec<String> {
        photos.iter().map(|photo| photo.path.file_name().unwrap().to_string_lossy().into_owned()).collect()
    }

    #[test]
    fn an_album_is_made_filled_emptied_renamed_and_deleted() {
        let catalog = Catalog::in_memory().unwrap();
        let (first, second) = two_libraries(&catalog, "album");
        let a = catalog.photos(first.id, &Filter::default()).unwrap().remove(0);
        let c = catalog.photos(second.id, &Filter::default()).unwrap().remove(0);

        let key = catalog.create_album(" Trip ").unwrap();
        assert!(catalog.create_album("trip").unwrap_err().contains("already"), "one name, whatever its case");
        assert!(catalog.create_album("  ").is_err());
        assert_eq!(catalog.albums().unwrap(), [(key.clone(), "Trip".to_string())]);

        catalog.set_in_album(&key, &[a.id, c.id], true).unwrap();
        catalog.set_in_album(&key, &[a.id], true).unwrap();
        let album = Filter { album: Some(key.clone()), ..Filter::default() };
        assert_eq!(names_of(&catalog.photos_everywhere(&album).unwrap()), ["a.RAF", "c.RAF"]);
        assert_eq!(names_of(&catalog.photos(first.id, &album).unwrap()), ["a.RAF"]);

        catalog.set_rating(c.id, 4).unwrap();
        let rated = Filter { min_rating: 3, ..album.clone() };
        assert_eq!(names_of(&catalog.photos_everywhere(&rated).unwrap()), ["c.RAF"]);

        catalog.set_in_album(&key, &[a.id], false).unwrap();
        assert_eq!(names_of(&catalog.photos_everywhere(&album).unwrap()), ["c.RAF"]);

        let other = catalog.create_album("Other").unwrap();
        assert!(catalog.rename_album(&key, "other").is_err());
        catalog.rename_album(&key, "Summer").unwrap();
        assert_eq!(catalog.albums().unwrap()[1], (key.clone(), "Summer".to_string()));
        assert!(catalog.photos_everywhere(&Filter { album: Some(other), ..Filter::default() }).unwrap().is_empty());

        catalog.delete_album(&key).unwrap();
        assert_eq!(catalog.albums().unwrap().len(), 1);
        assert!(catalog.photos_everywhere(&album).unwrap().is_empty(), "and its rows went with it");
        assert_eq!(catalog.photos_everywhere(&Filter::default()).unwrap().len(), 4, "but not the photographs");
    }

    #[test]
    fn a_photograph_gone_from_disk_leaves_its_album() {
        let catalog = Catalog::in_memory().unwrap();
        let (first, _) = two_libraries(&catalog, "album-gone");
        let photos = catalog.photos(first.id, &Filter::default()).unwrap();
        let key = catalog.create_album("Gone").unwrap();
        catalog.set_in_album(&key, &[photos[0].id, photos[1].id], true).unwrap();

        std::fs::remove_file(&photos[0].path).unwrap();
        catalog.sync_library(&first).unwrap();
        let album = Filter { album: Some(key), ..Filter::default() };
        assert_eq!(names_of(&catalog.photos_everywhere(&album).unwrap()), ["b.RAF"]);
    }

    #[test]
    fn an_album_leaves_out_an_unplugged_library_and_follows_a_moved_one() {
        let home = temp_dir("album-home").join("catalog.db");
        let catalog = Catalog::open(&home).unwrap();
        let (first, second) = two_libraries(&catalog, "album-unplugged");
        let everything = catalog.photos_everywhere(&Filter::default()).unwrap();
        let key = catalog.create_album("Both").unwrap();
        catalog.set_in_album(&key, &everything.iter().map(|photo| photo.id).collect::<Vec<_>>(), true).unwrap();
        drop(catalog);

        let away = std::env::temp_dir().join("numa-test-album-unplugged-away");
        let _ = std::fs::remove_dir_all(&away);
        std::fs::rename(&second.path, &away).unwrap();
        let catalog = Catalog::open(&home).unwrap();
        let album = Filter { album: Some(key.clone()), ..Filter::default() };
        assert_eq!(names_of(&catalog.photos_everywhere(&album).unwrap()), ["a.RAF", "b.RAF"]);

        catalog.remove_library(second.id).unwrap();
        let moved = catalog.add_library(&away).unwrap();
        catalog.sync_library(&moved).unwrap();
        assert_eq!(names_of(&catalog.photos_everywhere(&album).unwrap()), ["a.RAF", "b.RAF", "c.RAF", "d.JPG"]);

        drop(catalog);
        for dir in [first.path, away, home.parent().unwrap().to_path_buf()] {
            std::fs::remove_dir_all(dir).unwrap();
        }
    }

    #[test]
    fn every_library_is_one_filtered_grid() {
        let catalog = Catalog::in_memory().unwrap();
        let (first, second) = two_libraries(&catalog, "everywhere");
        let all = Filter { all_libraries: true, ..Filter::default() };
        assert!(all.spans_libraries());
        let photos = catalog.photos_everywhere(&all).unwrap();
        assert_eq!(photos.len(), 4);
        let libraries: std::collections::HashSet<i64> = photos.iter().map(|photo| split_id(photo.id).0).collect();
        assert_eq!(libraries, [first.id, second.id].into());

        let b = photos.iter().find(|photo| photo.path.ends_with("b.RAF")).unwrap();
        let d = photos.iter().find(|photo| photo.path.ends_with("d.JPG")).unwrap();
        catalog.set_rating(b.id, 5).unwrap();
        catalog.set_rating(d.id, 5).unwrap();
        catalog.set_flag(d.id, Flag::Picked).unwrap();

        let starred = Filter { min_rating: 5, sort: Sort::Name, ..all.clone() };
        assert_eq!(names_of(&catalog.photos_everywhere(&starred).unwrap()), ["b.RAF", "d.JPG"]);
        let raws = Filter { file_type: FileType::Raw, ..starred.clone() };
        assert_eq!(names_of(&catalog.photos_everywhere(&raws).unwrap()), ["b.RAF"]);
        let picked = Filter { flag: Some(Flag::Picked), ..starred };
        assert_eq!(names_of(&catalog.photos_everywhere(&picked).unwrap()), ["d.JPG"]);
    }

    #[test]
    fn photographs_from_several_libraries_sort_as_one_query_would() {
        let photo = |id: i64, mtime: i64, rating: u8, sharpness: Option<f32>| Photo {
            id,
            path: PathBuf::from(format!("/{id}.raf")),
            mtime,
            rating,
            flag: Flag::None,
            sharpness,
            blown: None,
            best_of_burst: false,
            faces: None,
            face_sharpness: None,
            suggested: None,
            edited: false,
        };
        let photos = [photo(1, 30, 2, None), photo(2, 10, 5, Some(0.2)), photo(3, 20, 5, Some(0.9))];
        let order = |sort: Sort| {
            let mut sorted = photos.to_vec();
            sorted.sort_by(|a, b| sort.compare(a, b));
            sorted.iter().map(|photo| photo.id).collect::<Vec<_>>()
        };
        assert_eq!(order(Sort::Captured), [2, 3, 1]);
        assert_eq!(order(Sort::Rating), [2, 3, 1]);

        assert_eq!(order(Sort::Sharpness), [3, 2, 1]);
    }
}
