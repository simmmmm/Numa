use rusqlite::Connection;

pub(super) const LEGACY_SCHEMA: &str = r#"
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

pub(super) fn migrate(conn: &Connection) -> Result<(), String> {
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

pub(super) const HOME_SCHEMA: &str = r#"
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

pub(super) const LIBRARY_SCHEMA: &str = r#"
PRAGMA foreign_keys = ON;
PRAGMA journal_mode = DELETE;

-- Paths are relative to the library's folder, so the folder can move — to
-- another drive, another mount point, another computer — and still be itself.
-- IO-005: `taken` is the EXIF DateTimeOriginal, and `mtime` the file's own
-- date. Both are kept because either can be the only one there is: a scan
-- always has the file's date, and a photograph without EXIF — a scan of a
-- negative, an export that dropped its tags — has no other.
CREATE TABLE IF NOT EXISTS photos (
    id     INTEGER PRIMARY KEY,
    path   TEXT NOT NULL UNIQUE,
    mtime  INTEGER NOT NULL,
    taken  INTEGER,
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
    colourfulness REAL,
    -- FT-029 C8: the second hash, and the photograph of the same scene later
    -- or earlier that it found (this catalog's row), if there is one.
    shape INTEGER,
    echo INTEGER,
    -- FT-029 C6: the exposure time in seconds and the 35 mm focal length.
    exposure REAL,
    focal35 REAL,
    -- FT-029 C2: clipping and deep shadow on the raw's own values.
    raw_clipped REAL,
    raw_dark REAL,
    -- FT-029 C4: 1 when both eyes of the largest face read as closed, 0 open.
    eyes_closed INTEGER
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
-- Every photograph deleted looks here for its rows, for the cascade.
CREATE INDEX IF NOT EXISTS ignored_faces_photo ON ignored_faces(photo_id);
-- LIB-014: and every face the measure pass found, named or not. Derived, like
-- `analysis`: dropping it costs a pass of Analyse and no work.
CREATE TABLE IF NOT EXISTS faces (
    photo_id  INTEGER NOT NULL REFERENCES photos(id) ON DELETE CASCADE,
    embedding BLOB NOT NULL,
    -- The face as a small JPEG, for the People view.
    portrait  BLOB
);
CREATE INDEX IF NOT EXISTS faces_photo ON faces(photo_id);

-- FT-029 C10: what the photographer did in the loupe, in order — every mark,
-- every mark taken back, and every frame looked at and passed without one, with
-- how long it was looked at. What a score learned from this photographer's own
-- taste needs and the stars alone do not say: "seen, and not this one".
CREATE TABLE IF NOT EXISTS decisions (
    photo_id INTEGER NOT NULL REFERENCES photos(id) ON DELETE CASCADE,
    at       INTEGER NOT NULL,
    action   TEXT NOT NULL,
    dwell_ms INTEGER
);

-- FT-029 C9: what the photographer set for this shoot, which travels with it —
-- how many picks the cull is aiming for.
CREATE TABLE IF NOT EXISTS settings (
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
);

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
-- The key leads with the album, so asking by photograph — a photograph's
-- albums, the cascade when one is deleted — needs its own.
CREATE INDEX IF NOT EXISTS album_photos_photo ON album_photos(photo_id);
"#;
