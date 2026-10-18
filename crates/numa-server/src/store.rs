use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;

use rusqlite::{params, Connection, OptionalExtension};

pub struct Store(Mutex<Connection>);

#[derive(Clone, Debug, serde::Serialize)]
pub struct Account {
    pub name: String,
    pub admin: bool,

    pub manage: bool,
}

impl Store {
    pub fn open(path: &Path) -> Result<Self, String> {
        let conn = Connection::open(path).map_err(|err| err.to_string())?;

        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             CREATE TABLE IF NOT EXISTS accounts (
                 name TEXT PRIMARY KEY COLLATE NOCASE, hash TEXT NOT NULL, admin INTEGER NOT NULL);
             CREATE TABLE IF NOT EXISTS sessions (
                 token TEXT PRIMARY KEY, name TEXT NOT NULL COLLATE NOCASE, device TEXT, made INTEGER NOT NULL);
             CREATE TABLE IF NOT EXISTS sent (
                 name TEXT NOT NULL COLLATE NOCASE, sha256 TEXT NOT NULL, library INTEGER NOT NULL, path TEXT NOT NULL,
                 PRIMARY KEY (name, sha256, library));
             CREATE TABLE IF NOT EXISTS updated (photo INTEGER PRIMARY KEY, at INTEGER NOT NULL);
             CREATE TABLE IF NOT EXISTS settings (key TEXT PRIMARY KEY, value TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS cameras (camera TEXT PRIMARY KEY, account TEXT NOT NULL COLLATE NOCASE);
             CREATE TABLE IF NOT EXISTS removed (library INTEGER NOT NULL, path TEXT NOT NULL, PRIMARY KEY (library, path));
             CREATE TABLE IF NOT EXISTS trash (
                 id INTEGER PRIMARY KEY, library INTEGER NOT NULL, path TEXT NOT NULL, kept TEXT NOT NULL, at INTEGER NOT NULL);
             CREATE TABLE IF NOT EXISTS authors (
                 library INTEGER NOT NULL, path TEXT NOT NULL, account TEXT NOT NULL COLLATE NOCASE, PRIMARY KEY (library, path));
             CREATE TABLE IF NOT EXISTS settled (key TEXT PRIMARY KEY);
             CREATE TABLE IF NOT EXISTS hashes (path TEXT PRIMARY KEY, mtime INTEGER NOT NULL, size INTEGER NOT NULL, hash TEXT NOT NULL);
             CREATE TABLE IF NOT EXISTS left_out (folder TEXT PRIMARY KEY);
             CREATE TABLE IF NOT EXISTS dropped (id INTEGER PRIMARY KEY, folder TEXT NOT NULL, at INTEGER NOT NULL);
             CREATE TABLE IF NOT EXISTS arrived (photo INTEGER PRIMARY KEY, at INTEGER NOT NULL);",
        )
        .map_err(|err| err.to_string())?;

        let _ = conn.execute("ALTER TABLE accounts ADD COLUMN manage INTEGER NOT NULL DEFAULT 0", []);

        let _ = conn.execute("ALTER TABLE sessions ADD COLUMN sent INTEGER", []);
        Ok(Self(Mutex::new(conn)))
    }

    fn with<R>(&self, work: impl FnOnce(&Connection) -> rusqlite::Result<R>) -> Result<R, String> {
        let conn = self.0.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        work(&conn).map_err(|err| err.to_string())
    }

    pub fn set_manage(&self, name: &str, manage: bool) -> Result<bool, String> {
        self.with(|conn| conn.execute("UPDATE accounts SET manage = ?2 WHERE name = ?1", params![name, manage])).map(|changed| changed == 1)
    }

    pub fn has_accounts(&self) -> bool {
        self.with(|conn| conn.query_row("SELECT EXISTS (SELECT 1 FROM accounts)", [], |row| row.get(0))).unwrap_or(true)
    }

    pub fn accounts(&self) -> Result<Vec<Account>, String> {
        self.with(|conn| {
            let mut statement = conn.prepare("SELECT name, admin, manage FROM accounts ORDER BY name COLLATE NOCASE")?;
            let rows = statement.query_map([], |row| Ok(Account { name: row.get(0)?, admin: row.get(1)?, manage: row.get(2)? }))?;
            rows.collect()
        })
    }

    pub fn add_account(&self, name: &str, hash: &str, admin: bool, only_if_first: bool) -> Result<bool, String> {
        let sql = match only_if_first {
            true => "INSERT INTO accounts (name, hash, admin) SELECT ?1, ?2, ?3 WHERE NOT EXISTS (SELECT 1 FROM accounts)",
            false => "INSERT OR IGNORE INTO accounts (name, hash, admin) VALUES (?1, ?2, ?3)",
        };
        self.with(|conn| conn.execute(sql, params![name, hash, admin])).map(|changed| changed == 1)
    }

    pub fn account(&self, name: &str) -> Option<(String, String, bool)> {
        self.with(|conn| {
            conn.query_row("SELECT name, hash, admin FROM accounts WHERE name = ?1", [name], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
                .optional()
        })
        .ok()
        .flatten()
    }

    pub fn add_session(&self, digest: &str, name: &str, device: Option<&str>) -> Result<(), String> {
        let now = now();
        self.with(|conn| conn.execute("INSERT INTO sessions (token, name, device, made) VALUES (?1, ?2, ?3, ?4)", params![digest, name, device, now]))
            .map(drop)
    }

    pub fn session(&self, digest: &str) -> Option<Account> {
        self.with(|conn| {
            conn.query_row(
                "SELECT a.name, a.admin, a.manage FROM sessions s JOIN accounts a ON a.name = s.name WHERE s.token = ?1",
                [digest],
                |row| Ok(Account { name: row.get(0)?, admin: row.get(1)?, manage: row.get(2)? }),
            )
            .optional()
        })
        .ok()
        .flatten()
    }

    pub fn note_arrival(&self, digest: &str) {
        let _ = self.with(|conn| conn.execute("UPDATE sessions SET sent = ?2 WHERE token = ?1", params![digest, now()]));
    }

    pub fn session_device(&self, digest: &str) -> Option<String> {
        self.with(|conn| conn.query_row("SELECT COALESCE(device, '') FROM sessions WHERE token = ?1", [digest], |row| row.get(0)).optional())
            .ok()
            .flatten()
    }

    pub fn devices(&self) -> Vec<(String, String, i64)> {
        self.with(|conn| {
            let sql = "SELECT name, COALESCE(device, ''), MAX(sent) FROM sessions WHERE sent IS NOT NULL GROUP BY name COLLATE NOCASE, device ORDER BY 3 DESC";
            let mut statement = conn.prepare(sql)?;
            let rows = statement.query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?;
            rows.collect()
        })
        .unwrap_or_default()
    }

    pub fn end_session(&self, digest: &str) {
        let _ = self.with(|conn| conn.execute("DELETE FROM sessions WHERE token = ?1", [digest]));
    }

    pub fn sent(&self, name: &str, sha256: &str, library: i64) -> Option<String> {
        let sql = "SELECT path FROM sent WHERE name = ?1 AND sha256 = ?2 AND library = ?3";
        self.with(|conn| conn.query_row(sql, params![name, sha256, library], |row| row.get(0)).optional()).ok().flatten()
    }

    pub fn note_sent(&self, name: &str, sha256: &str, library: i64, path: &str) -> Result<(), String> {
        let sql = "INSERT OR REPLACE INTO sent (name, sha256, library, path) VALUES (?1, ?2, ?3, ?4)";
        self.with(|conn| conn.execute(sql, params![name, sha256, library, path])).map(drop)
    }

    pub fn seen(&self, ids: &[i64]) {
        let now = now();
        let _ = self.with(|conn| {
            let tx = conn.unchecked_transaction()?;
            {
                let mut insert = tx.prepare_cached("INSERT OR IGNORE INTO updated (photo, at) VALUES (?1, ?2)")?;
                let mut arrived = tx.prepare_cached("INSERT OR REPLACE INTO arrived (photo, at) VALUES (?1, ?2)")?;
                for id in ids {
                    if insert.execute(params![id, now])? == 1 {
                        arrived.execute(params![id, now])?;
                    }
                }
            }
            tx.execute("DELETE FROM arrived WHERE at < ?1", [now - crate::today::NEWS])?;
            tx.commit()
        });
    }

    pub fn arrived(&self, since: i64) -> HashMap<i64, i64> {
        self.with(|conn| {
            let mut statement = conn.prepare("SELECT photo, at FROM arrived WHERE at >= ?1")?;
            let rows = statement.query_map([since], |row| Ok((row.get(0)?, row.get(1)?)))?;
            rows.collect()
        })
        .unwrap_or_default()
    }

    pub fn touch(&self, id: i64) -> i64 {
        let now = now();
        let _ = self.with(|conn| conn.execute("INSERT OR REPLACE INTO updated (photo, at) VALUES (?1, ?2)", params![id, now]));
        now
    }

    pub fn updated(&self) -> std::collections::HashMap<i64, i64> {
        self.with(|conn| {
            let mut statement = conn.prepare("SELECT photo, at FROM updated")?;
            let rows = statement.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
            rows.collect()
        })
        .unwrap_or_default()
    }
}

impl Store {

    pub fn setting(&self, key: &str) -> Option<String> {
        self.with(|conn| conn.query_row("SELECT value FROM settings WHERE key = ?1", [key], |row| row.get(0)).optional()).ok().flatten()
    }

    pub fn cameras(&self) -> HashMap<String, String> {
        self.with(|conn| {
            let mut statement = conn.prepare("SELECT camera, account FROM cameras")?;
            let rows = statement.query_map([], |row| Ok((row.get(0)?, row.get(1)?)))?;
            rows.collect()
        })
        .unwrap_or_default()
    }

    pub fn link_camera(&self, camera: &str, account: Option<&str>) -> Result<(), String> {
        self.with(|conn| match account {
            Some(account) => conn.execute("INSERT OR REPLACE INTO cameras (camera, account) VALUES (?1, ?2)", params![camera, account]),
            None => conn.execute("DELETE FROM cameras WHERE camera = ?1", [camera]),
        })
        .map(drop)
    }

    pub fn sendings(&self) -> Vec<(String, i64, String)> {
        self.with(|conn| {
            let mut statement = conn.prepare("SELECT name, library, path FROM sent")?;
            let rows = statement.query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?;
            rows.collect()
        })
        .unwrap_or_default()
    }

    pub fn settings_under(&self, prefix: &str) -> Vec<(String, String)> {
        self.with(|conn| {
            let mut statement = conn.prepare("SELECT substr(key, length(?1) + 1), value FROM settings WHERE substr(key, 1, length(?1)) = ?1")?;
            let rows = statement.query_map([prefix], |row| Ok((row.get(0)?, row.get(1)?)))?;
            rows.collect()
        })
        .unwrap_or_default()
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<(), String> {
        self.with(|conn| conn.execute("INSERT OR REPLACE INTO settings (key, value) VALUES (?1, ?2)", params![key, value])).map(drop)
    }
}

pub struct Trashed {
    pub id: i64,
    pub library: i64,
    pub path: String,
    pub kept: String,
    pub at: i64,
}

impl Store {
    pub fn removed(&self, library: i64) -> std::collections::HashSet<String> {
        self.with(|conn| {
            let mut statement = conn.prepare("SELECT path FROM removed WHERE library = ?1")?;
            let rows = statement.query_map([library], |row| row.get(0))?;
            rows.collect()
        })
        .unwrap_or_default()
    }

    pub fn remove(&self, library: i64, path: &str) -> Result<(), String> {
        self.with(|conn| conn.execute("INSERT OR IGNORE INTO removed (library, path) VALUES (?1, ?2)", params![library, path])).map(drop)
    }

    pub fn trash(&self, library: i64, path: &str, kept: &str) -> Result<i64, String> {
        let now = now();
        self.with(|conn| {
            conn.execute("INSERT INTO trash (library, path, kept, at) VALUES (?1, ?2, ?3, ?4)", params![library, path, kept, now])?;
            Ok(conn.last_insert_rowid())
        })
    }

    pub fn trashed(&self) -> Vec<Trashed> {
        self.with(|conn| {
            let mut statement = conn.prepare("SELECT id, library, path, kept, at FROM trash ORDER BY at DESC, id DESC")?;
            let rows = statement.query_map([], |row| Ok(Trashed { id: row.get(0)?, library: row.get(1)?, path: row.get(2)?, kept: row.get(3)?, at: row.get(4)? }))?;
            rows.collect()
        })
        .unwrap_or_default()
    }

    pub fn untrash(&self, id: i64) {
        let _ = self.with(|conn| conn.execute("DELETE FROM trash WHERE id = ?1", [id]));
    }

    pub fn left_out(&self) -> std::collections::HashSet<String> {
        self.with(|conn| {
            let mut statement = conn.prepare("SELECT folder FROM left_out")?;
            let rows = statement.query_map([], |row| row.get(0))?;
            rows.collect()
        })
        .unwrap_or_default()
    }

    pub fn leave_out(&self, folder: &str, out: bool) -> Result<(), String> {
        let sql = if out { "INSERT OR IGNORE INTO left_out (folder) VALUES (?1)" } else { "DELETE FROM left_out WHERE folder = ?1" };
        self.with(|conn| conn.execute(sql, [folder])).map(drop)
    }

    pub fn drop_library(&self, folder: &str) -> Result<i64, String> {
        let now = now();
        self.with(|conn| {
            conn.execute("INSERT INTO dropped (folder, at) VALUES (?1, ?2)", params![folder, now])?;
            Ok(conn.last_insert_rowid())
        })
    }

    pub fn dropped(&self) -> Vec<(i64, String, i64)> {
        self.with(|conn| {
            let mut statement = conn.prepare("SELECT id, folder, at FROM dropped ORDER BY at DESC, id DESC")?;
            let rows = statement.query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))?;
            rows.collect()
        })
        .unwrap_or_default()
    }

    pub fn undrop(&self, id: i64) {
        let _ = self.with(|conn| conn.execute("DELETE FROM dropped WHERE id = ?1", [id]));
    }

    pub fn authors(&self) -> HashMap<(i64, String), String> {
        self.with(|conn| {
            let mut statement = conn.prepare("SELECT library, path, account FROM authors")?;
            let rows = statement.query_map([], |row| Ok(((row.get(0)?, row.get(1)?), row.get(2)?)))?;
            rows.collect()
        })
        .unwrap_or_default()
    }

    pub fn set_author(&self, library: i64, path: &str, account: Option<&str>) -> Result<(), String> {
        self.with(|conn| match account {
            Some(account) => conn.execute("INSERT OR REPLACE INTO authors (library, path, account) VALUES (?1, ?2, ?3)", params![library, path, account]),
            None => conn.execute("DELETE FROM authors WHERE library = ?1 AND path = ?2", params![library, path]),
        })
        .map(drop)
    }
}

impl Store {
    pub fn settled(&self) -> std::collections::HashSet<String> {
        self.with(|conn| {
            let mut statement = conn.prepare("SELECT key FROM settled")?;
            let rows = statement.query_map([], |row| row.get(0))?;
            rows.collect()
        })
        .unwrap_or_default()
    }

    pub fn settle(&self, keys: &[String]) {
        let _ = self.with(|conn| {
            let tx = conn.unchecked_transaction()?;
            for key in keys {
                tx.execute("INSERT OR IGNORE INTO settled (key) VALUES (?1)", [key])?;
            }
            tx.commit()
        });
    }

    pub fn hashes(&self) -> HashMap<String, (i64, u64, String)> {
        self.with(|conn| {
            let mut statement = conn.prepare("SELECT path, mtime, size, hash FROM hashes")?;
            let rows = statement.query_map([], |row| Ok((row.get(0)?, (row.get(1)?, row.get::<_, i64>(2)? as u64, row.get(3)?))))?;
            rows.collect()
        })
        .unwrap_or_default()
    }

    pub fn note_hash(&self, path: &str, mtime: i64, size: u64, hash: &str) {
        let sql = "INSERT OR REPLACE INTO hashes (path, mtime, size, hash) VALUES (?1, ?2, ?3, ?4)";
        let _ = self.with(|conn| conn.execute(sql, params![path, mtime, size as i64, hash]));
    }

    pub fn vacuum(&self) -> Result<(), String> {
        self.with(|conn| conn.execute_batch("VACUUM"))
    }
}

pub fn now() -> i64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |since| since.as_secs() as i64)
}
