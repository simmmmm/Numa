use rusqlite::params;

use super::{text, Catalog};

impl Catalog {

    pub fn found(&self, photo_id: i64, asked: &str) -> Option<crate::masks::Chips> {
        let (open, local) = self.photo(photo_id).ok()?;
        let (chips, grid): (String, Vec<u8>) = open
            .conn
            .query_row("SELECT chips, grid FROM found WHERE photo_id = ?1 AND asked = ?2", params![local, asked], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })
            .ok()?;
        let mut chips: crate::masks::Chips = serde_json::from_str(&chips).ok()?;
        chips.grid = grid_from(&grid)?;
        Some(chips)
    }

    pub fn save_found(&self, photo_id: i64, asked: &str, chips: &crate::masks::Chips) -> Result<(), String> {
        let (open, local) = self.photo(photo_id)?;
        open.connected()?;
        open.conn
            .execute(
                "INSERT INTO found (photo_id, asked, chips, grid) VALUES (?1, ?2, ?3, ?4) \
                 ON CONFLICT(photo_id) DO UPDATE SET asked = excluded.asked, chips = excluded.chips, grid = excluded.grid",
                params![local, asked, serde_json::to_string(chips).map_err(text)?, grid_blob(&chips.grid)?],
            )
            .map_err(text)?;
        Ok(())
    }

    pub fn scene(&self, photo_id: i64, asked: &str) -> Option<numa_render::auto::scene::Scene> {
        let (open, local) = self.photo(photo_id).ok()?;
        let (scene, grid): (String, Vec<u8>) = open
            .conn
            .query_row("SELECT scene, grid FROM auto_read WHERE photo_id = ?1 AND asked = ?2", params![local, asked], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })
            .ok()?;
        let mut scene: numa_render::auto::scene::Scene = serde_json::from_str(&scene).ok()?;
        scene.classes = grid_from(&grid)?;
        Some(scene)
    }

    pub fn save_scene(&self, photo_id: i64, scene: &numa_render::auto::scene::Scene) -> Result<(), String> {
        let (open, local) = self.photo(photo_id)?;
        open.connected()?;
        open.conn
            .execute(
                "INSERT INTO auto_read (photo_id, asked, scene, grid) VALUES (?1, ?2, ?3, ?4) \
                 ON CONFLICT(photo_id) DO UPDATE SET asked = excluded.asked, scene = excluded.scene, grid = excluded.grid",
                params![local, scene.asked, serde_json::to_string(scene).map_err(text)?, grid_blob(&scene.classes)?],
            )
            .map_err(text)?;
        Ok(())
    }
}

fn grid_blob((width, height, cells): &(usize, usize, Vec<u8>)) -> Result<Vec<u8>, String> {
    let mut grid = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::default());
    std::io::Write::write_all(&mut grid, cells).map_err(text)?;
    let mut blob = [(*width as u32).to_le_bytes(), (*height as u32).to_le_bytes()].concat();
    blob.extend(grid.finish().map_err(text)?);
    Ok(blob)
}

fn grid_from(blob: &[u8]) -> Option<(usize, usize, Vec<u8>)> {
    let (size, cells) = blob.split_at_checked(8)?;
    let (width, height) = (u32::from_le_bytes(size[..4].try_into().ok()?), u32::from_le_bytes(size[4..].try_into().ok()?));
    let mut raw = Vec::new();
    std::io::Read::read_to_end(&mut flate2::read::DeflateDecoder::new(cells), &mut raw).ok()?;
    (raw.len() == width as usize * height as usize).then_some((width as usize, height as usize, raw))
}
