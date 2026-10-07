use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use rusqlite::{params, Connection};

pub(super) const BACKUPS_KEPT: usize = 4;
const BACKUP_EVERY: std::time::Duration = std::time::Duration::from_secs(7 * 24 * 3600);

pub(super) fn back_up_weekly(conn: &Connection, dir: &Path) {

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

pub(super) fn civil_date(days: i64) -> (i64, u32, u32) {
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
