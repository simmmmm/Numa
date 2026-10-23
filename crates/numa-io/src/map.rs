use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::f64::consts::PI;
use std::io::Read;
use std::path::Path;

use rusqlite::{params, Connection, OpenFlags, OptionalExtension};

pub const WORLD: u8 = 6;
pub const DETAIL: u8 = 14;

const AROUND: f64 = 1.0;
const NEAR: f64 = 160.0 / 512.0;

const GAP: u64 = 256 << 10;
const CHUNK: u64 = 64 << 20;

pub const BUILDS: &str = "https://build-metadata.protomaps.dev/builds.json";
pub const BUILD: &str = "https://build.protomaps.com/";

pub type Reader<'a> = dyn FnMut(&[(u64, u64)]) -> Result<Vec<Vec<u8>>, String> + 'a;

pub fn tile_id(z: u8, x: u32, y: u32) -> u64 {
    let n = 1u64 << z;
    let (mut x, mut y, mut d) = (u64::from(x), u64::from(y), 0);
    let mut s = n / 2;
    while s > 0 {
        let (rx, ry) = (u64::from(x & s > 0), u64::from(y & s > 0));
        d += s * s * ((3 * rx) ^ ry);
        if ry == 0 {
            if rx == 1 {
                (x, y) = (n - 1 - x, n - 1 - y);
            }
            (x, y) = (y, x);
        }
        s /= 2;
    }
    first(z) + d
}

fn first(z: u8) -> u64 {
    ((1u64 << (2 * u32::from(z))) - 1) / 3
}

pub fn wanted(positions: &[(f32, f32)]) -> BTreeSet<u64> {
    let mut ids: BTreeSet<u64> = (0..first(WORLD + 1)).collect();

    let mut seen = HashSet::new();
    for &(lat, lon) in positions {
        if seen.insert(((lat * 1000.0) as i32, (lon * 1000.0) as i32)) {
            ids.extend(around(lat, lon));
        }
    }
    ids
}

fn around(lat: f32, lon: f32) -> Vec<u64> {
    let (lat, lon) = (f64::from(lat).clamp(-85.0, 85.0), f64::from(lon));
    let mut ids = Vec::new();
    for z in WORLD + 1..=DETAIL {
        let n = f64::from(1u32 << z);
        let (x, y) = ((lon + 180.0) / 360.0 * n, (1.0 - lat.to_radians().tan().asinh() / PI) / 2.0 * n);
        let reach = if z <= 12 { AROUND } else { NEAR };
        let span = |at: f64| ((at - reach).floor().max(0.0) as u32)..=((at + reach).floor().min(n - 1.0) as u32);
        for tx in span(x) {
            ids.extend(span(y).map(|ty| tile_id(z, tx, ty)));
        }
    }
    ids
}

#[derive(Clone, Copy, Default)]
struct Entry {
    id: u64,
    run: u64,
    offset: u64,
    length: u64,
}

fn varint(bytes: &[u8], at: &mut usize) -> Result<u64, String> {
    let mut value = 0;
    for shift in (0..64).step_by(7) {
        let byte = *bytes.get(*at).ok_or("a directory of the map ends early")?;
        *at += 1;
        value |= u64::from(byte & 0x7f) << shift;
        if byte < 0x80 {
            return Ok(value);
        }
    }
    Err("a directory of the map is not one".into())
}

fn directory(packed: &[u8]) -> Result<Vec<Entry>, String> {
    let mut bytes = Vec::new();
    flate2::read::GzDecoder::new(packed).read_to_end(&mut bytes).map_err(|err| err.to_string())?;
    let at = &mut 0;
    let n = varint(&bytes, at)? as usize;
    if n > bytes.len() {
        return Err("a directory of the map is not one".into());
    }
    let mut entries = vec![Entry::default(); n];
    let mut id = 0;
    for entry in entries.iter_mut() {
        id += varint(&bytes, at)?;
        entry.id = id;
    }
    for entry in entries.iter_mut() {
        entry.run = varint(&bytes, at)?;
    }
    for entry in entries.iter_mut() {
        entry.length = varint(&bytes, at)?;
    }
    for k in 0..n {
        let value = varint(&bytes, at)?;
        entries[k].offset = match (value, k.checked_sub(1)) {
            (0, Some(before)) => entries[before].offset + entries[before].length,
            _ => value.checked_sub(1).ok_or("a directory of the map is not one")?,
        };
    }
    Ok(entries)
}

enum Found {
    Tile(u64, u64),
    Leaf(u64, u64),
    Absent,
}

fn find(directory: &[Entry], id: u64) -> Found {
    let at = directory.partition_point(|entry| entry.id <= id);
    match at.checked_sub(1).map(|at| directory[at]) {
        Some(entry) if entry.run == 0 => Found::Leaf(entry.offset, entry.length),
        Some(entry) if id < entry.id + entry.run => Found::Tile(entry.offset, entry.length),
        _ => Found::Absent,
    }
}

struct Archive {
    leaves: u64,
    data: u64,
    root: Vec<Entry>,
}

impl Archive {
    fn parse(head: &[u8]) -> Result<Self, String> {
        if head.len() < 127 || &head[..7] != b"PMTiles" || head[7] != 3 {
            return Err("the map's build is not a PMTiles 3 file".into());
        }

        if head[97] != 2 || head[98] != 2 || head[99] != 1 {
            return Err("the map's build is not of gzipped vector tiles".into());
        }
        let word = |at: usize| u64::from_le_bytes(head[at..at + 8].try_into().expect("eight bytes"));
        let (at, length) = (word(8) as usize, word(16) as usize);
        let root = directory(head.get(at..at + length).ok_or("the map's root directory is not where it should be")?)?;
        Ok(Archive { leaves: word(40), data: word(56), root })
    }

    fn locate(&self, ids: &[u64], read: &mut Reader) -> Result<Vec<(u64, u64, u64)>, String> {
        let mut found = Vec::new();
        let mut todo = vec![(self.root.clone(), ids.to_vec())];
        for _ in 0..4 {
            let mut leaves: BTreeMap<(u64, u64), Vec<u64>> = BTreeMap::new();
            for (directory, ids) in &todo {
                for &id in ids {
                    match find(directory, id) {
                        Found::Tile(offset, length) => found.push((id, self.data + offset, length)),
                        Found::Leaf(offset, length) => leaves.entry((self.leaves + offset, length)).or_default().push(id),
                        Found::Absent => found.push((id, 0, 0)),
                    }
                }
            }
            if leaves.is_empty() {
                return Ok(found);
            }
            let bytes = read(&leaves.keys().copied().collect::<Vec<_>>())?;
            todo = bytes.iter().zip(leaves.into_values()).map(|(bytes, ids)| Ok((directory(bytes)?, ids))).collect::<Result<_, String>>()?;
        }
        Err("the map's directories go deeper than a PMTiles file's".into())
    }
}

pub fn spans(wanted: &[(u64, u64)]) -> Vec<(u64, u64)> {
    let mut sorted: Vec<(u64, u64)> = wanted.iter().copied().filter(|(_, length)| *length > 0).collect();
    sorted.sort_unstable();
    let mut spans: Vec<(u64, u64)> = Vec::new();
    for (offset, length) in sorted {
        match spans.last_mut() {
            Some((_, end)) if offset <= *end + GAP => *end = (*end).max(offset + length),
            _ => spans.push((offset, offset + length)),
        }
    }
    spans
}

pub fn fetch(db: &mut Connection, ids: &[u64], read: &mut Reader, started: impl FnOnce(u64)) -> Result<(usize, u64), String> {
    let archive = Archive::parse(&read(&[(0, 16384)])?.remove(0))?;
    let located = archive.locate(ids, read)?;
    let mut chunks: Vec<Vec<(u64, u64, u64)>> = vec![Vec::new()];
    let mut size = 0;
    for entry in located {
        if size >= CHUNK {
            chunks.push(Vec::new());
            size = 0;
        }
        size += entry.2;
        chunks.last_mut().expect("one at least").push(entry);
    }
    let offsets = |chunk: &[(u64, u64, u64)]| chunk.iter().map(|&(_, offset, length)| (offset, length)).collect::<Vec<_>>();
    let total: u64 = chunks.iter().map(|chunk| spans(&offsets(chunk)).iter().map(|(from, to)| to - from).sum::<u64>()).sum();
    started(total);
    let mut tiles = 0;
    for chunk in &chunks {
        let bytes = read(&offsets(chunk))?;
        let rows = db.transaction().map_err(|err| err.to_string())?;
        for (&(id, _, _), data) in chunk.iter().zip(&bytes) {
            if !data.is_empty() && !data.starts_with(&[0x1f, 0x8b]) {
                return Err("a tile of the map came back as something else".into());
            }
            rows.execute("INSERT OR REPLACE INTO tiles (id, data) VALUES (?1, ?2)", params![id as i64, data]).map_err(|err| err.to_string())?;
        }
        rows.commit().map_err(|err| err.to_string())?;
        tiles += chunk.len();
    }
    Ok((tiles, total))
}

pub fn open(path: &Path) -> Result<Connection, String> {
    let db = Connection::open(path).map_err(|err| err.to_string())?;
    db.execute_batch(
        "PRAGMA journal_mode = WAL;
         CREATE TABLE IF NOT EXISTS tiles (id INTEGER PRIMARY KEY, data BLOB NOT NULL);
         CREATE TABLE IF NOT EXISTS about (key TEXT PRIMARY KEY, value TEXT NOT NULL);",
    )
    .map_err(|err| err.to_string())?;
    Ok(db)
}

fn reading(path: &Path) -> Option<Connection> {
    Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX).ok()
}

pub fn about(db: &Connection, key: &str) -> Option<String> {
    db.query_row("SELECT value FROM about WHERE key = ?1", [key], |row| row.get(0)).optional().ok()?
}

pub fn have(db: &Connection) -> Result<HashSet<u64>, String> {
    let mut rows = db.prepare("SELECT id FROM tiles").map_err(|err| err.to_string())?;
    let ids = rows.query_map([], |row| row.get::<_, i64>(0)).map_err(|err| err.to_string())?;
    Ok(ids.filter_map(Result::ok).map(|id| id as u64).collect())
}

pub fn ready(path: &Path) -> bool {
    reading(path).and_then(|db| about(&db, "build")).is_some()
}

pub fn tile(path: &Path, z: u8, x: u32, y: u32) -> Option<Vec<u8>> {
    if z > DETAIL || x >= 1 << z || y >= 1 << z {
        return None;
    }
    let db = reading(path)?;
    db.query_row("SELECT data FROM tiles WHERE id = ?1", [tile_id(z, x, y) as i64], |row| row.get(0)).optional().ok()?
}

pub fn tile_or_above(path: &Path, z: u8, x: u32, y: u32) -> Option<Vec<u8>> {
    if z > DETAIL || x >= 1 << z || y >= 1 << z {
        return None;
    }
    let db = reading(path)?;
    let mut statement = db.prepare_cached("SELECT data FROM tiles WHERE id = ?1").ok()?;
    for up in 0..=z {
        let id = tile_id(z - up, x >> up, y >> up) as i64;
        let Some(packed) = statement.query_row([id], |row| row.get::<_, Vec<u8>>(0)).optional().ok()? else { continue };
        if packed.is_empty() {
            return Some(Vec::new());
        }
        let mut bytes = Vec::new();
        flate2::read::GzDecoder::new(packed.as_slice()).read_to_end(&mut bytes).ok()?;
        let part = |at: u32| at & ((1 << up) - 1);
        return match up {
            0 => Some(bytes),
            _ => cut(&bytes, up, part(x), part(y)),
        };
    }
    None
}

fn varint64(bytes: &[u8], at: &mut usize) -> Option<u64> {
    let mut value = 0;
    for shift in (0..64).step_by(7) {
        let byte = *bytes.get(*at)?;
        *at += 1;
        value |= u64::from(byte & 0x7f) << shift;
        if byte < 0x80 {
            return Some(value);
        }
    }
    None
}

fn put_varint(out: &mut Vec<u8>, mut value: u64) {
    while value >= 0x80 {
        out.push((value as u8) | 0x80);
        value >>= 7;
    }
    out.push(value as u8);
}

fn fields(bytes: &[u8]) -> Option<Vec<(u64, u64, &[u8], &[u8])>> {
    let mut found = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        let start = at;
        let key = varint64(bytes, &mut at)?;
        let (field, wire) = (key >> 3, key & 7);
        let inner = match wire {
            0 => {
                varint64(bytes, &mut at)?;
                &bytes[at..at]
            }
            1 => {
                at += 8;
                &bytes[at..at]
            }
            2 => {
                let length = varint64(bytes, &mut at)? as usize;
                let inner = bytes.get(at..at.checked_add(length)?)?;
                at += length;
                inner
            }
            5 => {
                at += 4;
                &bytes[at..at]
            }
            _ => return None,
        };
        found.push((field, wire, bytes.get(start..at)?, inner));
    }
    Some(found)
}

fn put_bytes(out: &mut Vec<u8>, field: u64, bytes: &[u8]) {
    put_varint(out, field << 3 | 2);
    put_varint(out, bytes.len() as u64);
    out.extend_from_slice(bytes);
}

pub fn cut(tile: &[u8], dz: u8, dx: u32, dy: u32) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    for (field, wire, raw, inner) in fields(tile)? {
        match (field, wire) {
            (3, 2) => put_bytes(&mut out, 3, &cut_layer(inner, dz, dx, dy)?),
            _ => out.extend_from_slice(raw),
        }
    }
    Some(out)
}

fn cut_layer(layer: &[u8], dz: u8, dx: u32, dy: u32) -> Option<Vec<u8>> {
    let parts = fields(layer)?;
    let mut extent = 4096u64;
    for (field, _, raw, _) in &parts {
        if *field == 5 {
            extent = varint64(raw, &mut 1)?;
        }
    }
    let scale = 1u64 << dz;
    if extent % scale != 0 {
        return None;
    }
    let part = extent / scale;
    let (ox, oy) = ((u64::from(dx) * part) as i64, (u64::from(dy) * part) as i64);
    let mut out = Vec::new();
    for (field, wire, raw, inner) in parts {
        match (field, wire) {
            (5, _) => {}
            (2, 2) => put_bytes(&mut out, 2, &cut_feature(inner, ox, oy)?),
            _ => out.extend_from_slice(raw),
        }
    }
    put_varint(&mut out, 5 << 3);
    put_varint(&mut out, part);
    Some(out)
}

fn cut_feature(feature: &[u8], ox: i64, oy: i64) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    for (field, wire, raw, inner) in fields(feature)? {
        if (field, wire) != (4, 2) {
            out.extend_from_slice(raw);
            continue;
        }
        let mut at = 0;
        let mut geometry = Vec::new();
        while at < inner.len() {
            geometry.push(varint64(inner, &mut at)?);
        }

        if geometry.first().is_some_and(|command| command & 7 == 1 && command >> 3 >= 1) && geometry.len() >= 3 {
            let unzag = |value: u64| ((value >> 1) as i64) ^ -((value & 1) as i64);
            let zag = |value: i64| ((value << 1) ^ (value >> 63)) as u64;
            geometry[1] = zag(unzag(geometry[1]) - ox);
            geometry[2] = zag(unzag(geometry[2]) - oy);
        }
        let mut packed = Vec::new();
        geometry.iter().for_each(|value| put_varint(&mut packed, *value));
        put_bytes(&mut out, 4, &packed);
    }
    Some(out)
}

pub fn newest_of(list: &[u8]) -> Result<(String, String), String> {
    let builds: Vec<serde_json::Value> = serde_json::from_slice(list).map_err(|err| err.to_string())?;
    let named = |key: &str| key.len() == 16 && key.ends_with(".pmtiles") && key[..8].bytes().all(|b| b.is_ascii_digit());
    let builds = builds.iter().filter_map(|build| Some((build["key"].as_str().filter(|key| named(key))?, build["version"].as_str()?)));
    let (key, version) = builds.max_by_key(|(key, _)| *key).ok_or("Protomaps lists no build")?;
    Ok((key.to_string(), version.to_string()))
}

#[cfg(not(target_os = "ios"))]
pub fn bring(dir: &Path, positions: &[(f32, f32)], started: impl FnOnce(u64, &Path)) -> Result<(usize, u64), String> {
    std::fs::create_dir_all(dir).map_err(|err| err.to_string())?;
    let mut db = open(&dir.join("tiles.db"))?;
    let have = have(&db)?;
    let mut missing: Vec<u64> = wanted(positions).into_iter().filter(|id| !have.contains(id)).collect();
    if missing.is_empty() {
        return Ok((0, 0));
    }
    let out = std::process::Command::new("curl").args(["--fail", "--location", "--silent", "--show-error", "--max-time", "60", BUILDS]).output().map_err(|err| err.to_string())?;
    if !out.status.success() {
        return Err(String::from_utf8_lossy(&out.stderr).trim().to_string());
    }
    let (build, version) = newest_of(&out.stdout)?;
    if about(&db, "version").is_some_and(|ours| ours.split('.').next() != version.split('.').next()) {
        db.execute_batch("DELETE FROM tiles; DELETE FROM about;").map_err(|err| err.to_string())?;
        missing = wanted(positions).into_iter().collect();
    }
    let part = dir.join("tiles.part");
    let _ = std::fs::remove_file(&part);
    let fetched = fetch(&mut db, &missing, &mut ranges(&format!("{BUILD}{build}"), &part), |total| started(total, &part));
    let _ = std::fs::remove_file(&part);
    let fetched = fetched?;
    for (key, value) in [("build", build.as_str()), ("version", version.as_str())] {
        db.execute("INSERT OR REPLACE INTO about (key, value) VALUES (?1, ?2)", params![key, value]).map_err(|err| err.to_string())?;
    }
    Ok(fetched)
}

#[cfg(not(target_os = "ios"))]
pub fn ranges<'a>(url: &'a str, part: &'a Path) -> impl FnMut(&[(u64, u64)]) -> Result<Vec<Vec<u8>>, String> + 'a {
    use std::io::{Seek, SeekFrom, Write};
    move |wanted| {
        let spans = spans(wanted);
        let start = std::fs::metadata(part).map_or(0, |meta| meta.len());
        if !spans.is_empty() {
            let mut config = String::new();
            for (from, to) in &spans {
                if !config.is_empty() {
                    config.push_str("next\n");
                }
                config += &format!("url = \"{url}\"\nrange = \"{from}-{}\"\nmax-filesize = {}\nfail\nlocation\nsilent\nshow-error\n", to - 1, to - from);
            }
            let out = std::fs::OpenOptions::new().create(true).append(true).open(part).map_err(|err| err.to_string())?;
            let mut curl = std::process::Command::new("curl")
                .args(["--config", "-"])
                .stdin(std::process::Stdio::piped())
                .stdout(out)
                .stderr(std::process::Stdio::piped())
                .spawn()
                .map_err(|err| format!("curl would not start: {err}"))?;
            curl.stdin.take().expect("piped").write_all(config.as_bytes()).map_err(|err| err.to_string())?;
            let done = curl.wait_with_output().map_err(|err| err.to_string())?;
            if !done.status.success() {
                return Err(String::from_utf8_lossy(&done.stderr).trim().to_string());
            }
        }
        let mut file = std::fs::File::open(part).map_err(|err| err.to_string())?;
        let mut before = vec![0];
        for (from, to) in &spans {
            before.push(before.last().copied().unwrap_or(0) + to - from);
        }
        if file.metadata().map_err(|err| err.to_string())?.len() != start + before.last().copied().unwrap_or(0) {
            return Err("the map's server sent other than was asked".into());
        }
        let mut read = Vec::new();
        for &(offset, length) in wanted {
            let mut bytes = vec![0; length as usize];
            if length > 0 {
                let at = spans.partition_point(|(from, _)| *from <= offset) - 1;
                file.seek(SeekFrom::Start(start + before[at] + offset - spans[at].0)).map_err(|err| err.to_string())?;
                file.read_exact(&mut bytes).map_err(|err| err.to_string())?;
            }
            read.push(bytes);
        }
        Ok(read)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn a_tile_is_numbered_along_the_hilbert_curve() {
        assert_eq!([tile_id(0, 0, 0), tile_id(1, 0, 0), tile_id(1, 0, 1), tile_id(1, 1, 1), tile_id(1, 1, 0), tile_id(2, 0, 0)], [0, 1, 2, 3, 4, 5]);

        assert_eq!(tile_id(12, 3423, 1763), 19_078_479);

        let lisbon = around(38.7223, -9.1393);
        assert!(lisbon.contains(&tile_id(14, 7776, 6277)), "{lisbon:?}");
        assert!(lisbon.len() < 80 && lisbon.iter().all(|&id| id >= first(7) && id < first(15)));
        let ids = wanted(&[(38.7223, -9.1393), (38.7224, -9.1394)]);
        assert_eq!(ids.len() as u64, first(7) + lisbon.len() as u64);
    }

    fn gzipped(bytes: &[u8]) -> Vec<u8> {
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        gz.write_all(bytes).unwrap();
        gz.finish().unwrap()
    }

    fn packed(entries: &[(u64, u64, u64, u64)]) -> Vec<u8> {
        let mut plain = Vec::new();
        let mut put = |mut value: u64| loop {
            let byte = (value & 0x7f) as u8;
            value >>= 7;
            plain.push(byte | if value > 0 { 0x80 } else { 0 });
            if value == 0 {
                break;
            }
        };
        put(entries.len() as u64);
        let mut last = 0;
        for &(id, ..) in entries {
            put(id - last);
            last = id;
        }
        entries.iter().for_each(|&(_, run, ..)| put(run));
        entries.iter().for_each(|&(_, _, _, length)| put(length));
        entries.iter().for_each(|&(_, _, offset, _)| put(offset + 1));
        gzipped(&plain)
    }

    #[test]
    fn a_part_of_a_tile_is_its_geometry_drawn_larger() {

        let mut geometry = Vec::new();
        let zag = |value: i64| ((value << 1) ^ (value >> 63)) as u64;
        for value in [1 << 3 | 1, zag(2148), zag(1074), 1 << 3 | 2, zag(10), zag(-5)] {
            put_varint(&mut geometry, value);
        }
        let mut feature = Vec::new();
        put_varint(&mut feature, 3 << 3);
        put_varint(&mut feature, 1);
        put_bytes(&mut feature, 4, &geometry);
        let mut layer = Vec::new();
        put_varint(&mut layer, 15 << 3);
        put_varint(&mut layer, 2);
        put_bytes(&mut layer, 1, b"places");
        put_bytes(&mut layer, 2, &feature);
        let mut tile = Vec::new();
        put_bytes(&mut tile, 3, &layer);

        let part = cut(&tile, 1, 1, 0).unwrap();
        let layers = fields(&part).unwrap();
        assert_eq!(layers.len(), 1);
        let inside = fields(layers[0].3).unwrap();
        let extent = inside.iter().find(|(field, ..)| *field == 5).map(|(_, _, raw, _)| varint64(raw, &mut 1).unwrap());
        assert_eq!(extent, Some(2048));
        assert!(inside.iter().any(|(field, _, _, inner)| *field == 1 && *inner == b"places"));
        let moved = inside.iter().find(|(field, ..)| *field == 2).map(|(.., inner)| fields(inner).unwrap()).unwrap();
        let geometry = moved.iter().find(|(field, ..)| *field == 4).unwrap().3;
        let mut at = 0;
        let values: Vec<u64> = std::iter::from_fn(|| (at < geometry.len()).then(|| varint64(geometry, &mut at).unwrap())).collect();
        assert_eq!(values, [1 << 3 | 1, zag(100), zag(1074), 1 << 3 | 2, zag(10), zag(-5)], "only the first point moves");
        assert!(cut(&tile, 13, 0, 0).is_none(), "an extent that does not divide is not cut");
    }

    #[test]
    fn a_build_is_read_by_its_ranges_into_the_map() {
        let dir = std::env::temp_dir().join(format!("numa-map-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();

        let (world, sea, land) = (gzipped(b"world"), gzipped(b"sea"), gzipped(b"land"));
        let data = [world.clone(), sea.clone(), land.clone()].concat();
        let leaf = packed(&[(1, 2, world.len() as u64, sea.len() as u64), (3, 1, (world.len() + sea.len()) as u64, land.len() as u64)]);
        let root = packed(&[(0, 1, 0, world.len() as u64), (1, 0, 0, leaf.len() as u64)]);
        let mut head = vec![0u8; 127];
        head[..7].copy_from_slice(b"PMTiles");
        head[7] = 3;
        let (leaves_at, data_at) = (16384u64, 16384 + leaf.len() as u64);
        for (at, value) in [(8, 127u64), (16, root.len() as u64), (40, leaves_at), (48, leaf.len() as u64), (56, data_at), (64, data.len() as u64)] {
            head[at..at + 8].copy_from_slice(&value.to_le_bytes());
        }
        (head[97], head[98], head[99]) = (2, 2, 1);
        let mut file = [head, root].concat();
        file.resize(16384, 0);
        file.extend([leaf, data].concat());
        let build = dir.join("build.pmtiles");
        std::fs::write(&build, &file).unwrap();

        let path = dir.join("tiles.db");
        let mut db = open(&path).unwrap();
        let (url, part) = (format!("file://{}", build.display()), dir.join("tiles.part"));
        let mut told = 0;
        let (tiles, _) = fetch(&mut db, &[0, 1, 2, 3, 4], &mut ranges(&url, &part), |total| told = total).unwrap();
        assert_eq!(tiles, 5);
        assert!(told > 0);
        assert_eq!(tile(&path, 0, 0, 0), Some(world));
        assert_eq!(tile(&path, 1, 0, 0), Some(sea.clone()));
        assert_eq!(tile(&path, 1, 0, 1), Some(sea));
        assert_eq!(tile(&path, 1, 1, 1), Some(land));
        assert_eq!(tile(&path, 1, 1, 0), Some(Vec::new()));
        assert_eq!(tile(&path, 2, 0, 0), None);
        assert_eq!(tile(&path, 1, 2, 0), None);
        assert_eq!(have(&db).unwrap().len(), 5);

        std::fs::write(&build, vec![0u8; 16384]).unwrap();
        assert!(fetch(&mut db, &[5], &mut ranges(&url, &part), |_| {}).is_err());
        assert_eq!(have(&db).unwrap().len(), 5);
        let list = br#"[{"key":"20261021.pmtiles","version":"4.15.2"},{"key":"20261022.pmtiles","version":"4.15.2"},{"key":"x.pmtiles","version":"9"}]"#;
        assert_eq!(newest_of(list).unwrap(), ("20261022.pmtiles".to_string(), "4.15.2".to_string()));
        let _ = std::fs::remove_dir_all(&dir);
    }
}
