use rusqlite::params;

use numa_core::document::Document;

use super::{library_of, text, Catalog, Flag};
use crate::layers::Layers;

impl Catalog {

    pub fn kind(&self, library_id: i64) -> Option<crate::workflows::Kind> {
        crate::workflows::Kind::from_name(&self.library_setting(library_id, "kind")?)
    }

    pub fn set_kind(&self, library_id: i64, kind: Option<crate::workflows::Kind>) -> Result<(), String> {
        self.set_library_setting(library_id, "kind", kind.map(|kind| kind.name()))
    }

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

    pub fn layers(&self, library_id: i64, record: i64) -> Option<Layers> {
        serde_json::from_str(&self.library_setting(library_id, &format!("moment-layers/{record}"))?).ok()
    }

    pub fn keep_layers(&self, library_id: i64, record: i64, layers: &Layers) -> Result<(), String> {
        self.set_library_setting(library_id, &format!("moment-layers/{record}"), Some(&serde_json::to_string(layers).map_err(text)?))
    }

    pub fn all_layers(&self, library_id: i64) -> Vec<(i64, Layers)> {
        let Ok(open) = self.library(library_id) else { return Vec::new() };
        let Ok(mut query) = open.conn.prepare("SELECT key, value FROM settings WHERE key LIKE 'moment-layers/%'") else { return Vec::new() };
        query
            .query_map([], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))
            .map(|rows| rows.flatten().filter_map(|(key, json)| Some((key.strip_prefix("moment-layers/")?.parse().ok()?, serde_json::from_str(&json).ok()?))).collect())
            .unwrap_or_default()
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

    pub fn keep_numa_did(&self, photo_id: i64, did: &crate::workflows::Did) -> Result<(), String> {
        let open = self.library(library_of(photo_id))?;
        open.connected()?;
        open.conn
            .execute(
                "INSERT INTO settings (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                params![format!("numa-did/{photo_id}"), serde_json::to_string(did).map_err(text)?],
            )
            .map_err(text)?;
        Ok(())
    }

    pub fn numa_did(&self, photo_id: i64) -> Option<crate::workflows::Did> {
        let open = self.library(library_of(photo_id)).ok()?;
        let json: String = open.conn.query_row("SELECT value FROM settings WHERE key = ?1", params![format!("numa-did/{photo_id}")], |row| row.get(0)).ok()?;
        serde_json::from_str(&json).ok()
    }

    pub fn before_numa(&self, photo_id: i64) -> Option<Document> {
        let open = self.library(library_of(photo_id)).ok()?;
        let json: String = open.conn.query_row("SELECT value FROM settings WHERE key = ?1", params![format!("numa-before/{photo_id}")], |row| row.get(0)).ok()?;
        serde_json::from_str(&json).ok()
    }

    pub fn keep_before_numa(&self, photo_id: i64, document: &Document) -> Result<(), String> {
        let open = self.library(library_of(photo_id))?;
        open.connected()?;

        open.conn
            .execute(
                "INSERT OR IGNORE INTO settings (key, value) VALUES (?1, ?2)",
                params![format!("numa-before/{photo_id}"), serde_json::to_string(document).map_err(text)?],
            )
            .map_err(text)?;
        Ok(())
    }

    pub fn keep_numa_flags(&self, photo_ids: &[i64], flag: Flag) -> Result<(), String> {
        for id in photo_ids {
            let open = self.library(library_of(*id))?;
            open.connected()?;
            open.conn
                .execute(
                    "INSERT OR IGNORE INTO settings (key, value) VALUES (?1, ?2)",
                    params![format!("numa-flag/{id}"), serde_json::to_string(&flag).map_err(text)?],
                )
                .map_err(text)?;
        }
        Ok(())
    }

    pub fn numas_work(&self, library_id: i64) -> (Vec<(i64, Document)>, Vec<(i64, Flag)>) {
        let Ok(open) = self.library(library_id) else { return Default::default() };
        let rows = |prefix: &str| -> Vec<(i64, String)> {
            let Ok(mut query) = open.conn.prepare("SELECT key, value FROM settings WHERE key LIKE ?1") else { return Vec::new() };
            query
                .query_map(params![format!("{prefix}%")], |row| Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?)))
                .map(|rows| rows.flatten().filter_map(|(key, value)| Some((key.strip_prefix(prefix)?.parse().ok()?, value))).collect())
                .unwrap_or_default()
        };
        let before = rows("numa-before/").into_iter().filter_map(|(id, json)| Some((id, serde_json::from_str(&json).ok()?))).collect();
        let flags = rows("numa-flag/").into_iter().filter_map(|(id, json)| Some((id, serde_json::from_str(&json).ok()?))).collect();
        (before, flags)
    }

    pub fn forget_numa(&self, photo_ids: &[i64]) -> Result<(), String> {
        for id in photo_ids {
            let open = self.library(library_of(*id))?;
            open.connected()?;
            open.conn
                .execute(
                    "DELETE FROM settings WHERE key IN (?1, ?2, ?3)",
                    params![format!("numa-before/{id}"), format!("numa-flag/{id}"), format!("numa-did/{id}")],
                )
                .map_err(text)?;
        }
        Ok(())
    }
}
