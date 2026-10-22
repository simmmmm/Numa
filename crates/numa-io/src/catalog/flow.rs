use rusqlite::params;

use numa_core::document::Document;

use super::{library_of, text, Catalog};
use crate::layers::Layers;

impl Catalog {

    pub fn library_setting(&self, library_id: i64, key: &str) -> Option<String> {
        let open = self.library(library_id).ok()?;
        open.conn.query_row("SELECT value FROM settings WHERE key = ?1", params![key], |row| row.get(0)).ok()
    }

    pub fn set_library_setting(&self, library_id: i64, key: &str, value: Option<&str>) -> Result<(), String> {
        let open = self.library(library_id)?;
        open.connected()?;
        match value {
            Some(value) => open.conn.execute(
                "INSERT INTO settings (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                params![key, value],
            ),
            None => open.conn.execute("DELETE FROM settings WHERE key = ?1", params![key]),
        }
        .map_err(text)?;
        Ok(())
    }

    pub(crate) fn layers(&self, library_id: i64, record: i64) -> Option<Layers> {
        serde_json::from_str(&self.library_setting(library_id, &format!("moment-layers/{record}"))?).ok()
    }

    pub(super) fn composed(&self, photo_id: i64, own: Document) -> Document {
        match own.moment.and_then(|record| self.layers(library_of(photo_id), record)) {
            Some(layers) => layers.compose(photo_id, &own),
            None => own,
        }
    }

    pub(super) fn composed_json(&self, photo_id: i64, json: Option<String>) -> Option<String> {
        let json = json?;
        if !json.contains("\"moment\"") {
            return Some(json);
        }
        let Ok(own) = serde_json::from_str::<Document>(&json) else { return Some(json) };
        Some(serde_json::to_string(&self.composed(photo_id, own)).unwrap_or(json))
    }
}
