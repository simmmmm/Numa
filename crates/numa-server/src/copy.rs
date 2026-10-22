use std::ffi::CString;
use std::fs::File;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use axum::extract::State;
use axum::http::StatusCode;
use rusqlite::{Connection, OpenFlags};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::activity::Kind;
use crate::auth::Signed;
use crate::night::plural;
use crate::store::now;
use crate::{Fail, Server};

const DAY: i64 = 86_400;

const KEPT: usize = 14;

const NIGHTS: i64 = 2;

const LAST: &str = "copy.last";

const OK: &str = "copy.ok";

const SEEN: &str = "copy.seen";

const PENDING: &str = "copy.pending";

const DATABASES_AT: &str = "backup.at";
const DATABASES_PROBLEM: &str = "backup.problem";

pub fn root() -> PathBuf {
    std::env::var_os("NUMA_HOME_COPY").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("/copy"))
}

fn there(root: &Path) -> (bool, bool) {
    let exists = root.is_dir();

    let writable = exists && CString::new(root.as_os_str().as_bytes()).is_ok_and(|path| unsafe { libc::access(path.as_ptr(), libc::W_OK) } == 0);
    (exists, writable)
}

#[derive(Serialize, Deserialize, Default)]
struct Last {
    finished: i64,

    files: u64,
    bytes: u64,

    copied: u64,
    complete: bool,
    problem: Option<String>,
}

#[derive(Default)]
pub struct Said {
    pub done: Vec<String>,
    pub needs: Vec<String>,

    pub finished: bool,
}

fn amount(bytes: u64) -> String {
    match bytes >= 1_000_000_000 {
        true => crate::safe::size(bytes),
        false => crate::night::megabytes(bytes),
    }
}

pub fn run(server: &Arc<Server>, databases: bool, stop: &mut dyn FnMut() -> bool) -> Said {
    let activity = &server.activity;
    activity.plan(Kind::Copy, 0, None);
    activity.start(Kind::Copy);
    let mut said = Said { finished: true, ..Said::default() };
    if databases {
        activity.stage(Kind::Copy, "Copying the database and catalogs");
        match back_up(server, &crate::night::local().1) {
            Ok(catalogs) => {
                let into = server.data.join("backups");
                said.done.push(format!("The database and {} copied to {}; the newest {KEPT} are kept", plural(catalogs, "catalog", "catalogs"), into.display()));
                let _ = server.store.set_setting(DATABASES_AT, &now().to_string());
                let _ = server.store.set_setting(DATABASES_PROBLEM, "");
            }
            Err(problem) => {
                said.needs.push(format!("The copy of the database failed, so the last good copy stays the newest: {problem}"));
                let _ = server.store.set_setting(DATABASES_PROBLEM, &problem);
            }
        }
    }
    let root = root();
    let (exists, writable) = there(&root);
    if exists && server.store.setting(SEEN).is_none() {
        let _ = server.store.set_setting(SEEN, &now().to_string());
    }
    if exists && !writable {
        let problem = format!("{} is there, but Numa Home may not write to it", root.display());
        said.needs.push(format!("No second copy: {problem}"));
        save(server, &Last { finished: now(), complete: false, problem: Some(problem), ..last(server) });
    }
    if exists && writable {
        activity.stage(Kind::Copy, "Copying new and changed photographs to the second disk");
        let mut sources = vec![(server.home.photos.clone(), root.join("photos")), (server.home.sent.clone(), root.join("sent"))];
        sources.extend(newest(&server.data.join("backups")).map(|newest| (newest, root.join("databases"))));
        let room = crate::safe::space(&root).map(|(_, free)| free);
        let mirrored = mirror(&sources, &root, room, stop, &mut |total| activity.total(Kind::Copy, total), &mut || activity.add(Kind::Copy, 1));
        let complete = !mirrored.stopped && mirrored.problems.is_empty();
        let problem = match mirrored.problems.len() {
            0 => None,
            1 => Some(mirrored.problems[0].clone()),
            n => Some(format!("{} could not be copied; the first: {}", plural(n, "file", "files"), mirrored.problems[0])),
        };
        save(server, &Last { finished: now(), files: mirrored.files, bytes: mirrored.bytes, copied: mirrored.copied, complete, problem: problem.clone() });
        if complete {
            let _ = server.store.set_setting(OK, &now().to_string());
        }
        said.done.push(format!(
            "Second copy: {} copied to {}; it holds {}, {}{}",
            plural(mirrored.copied as usize, "new or changed file", "new or changed files"),
            root.display(),
            plural(mirrored.files as usize, "file", "files"),
            amount(mirrored.bytes),
            if mirrored.stopped { ", the rest next" } else { "" },
        ));
        said.needs.extend(problem.map(|problem| format!("Not everything is in the second copy: {problem}")));
        said.finished = !mirrored.stopped;
    }
    let _ = server.store.set_setting(PENDING, if said.finished { "" } else { "rest" });
    activity.finish(Kind::Copy);
    said
}

pub fn by_day(server: &Arc<Server>) -> bool {
    let databases = match server.store.setting(PENDING).as_deref() {
        Some("all") => true,
        Some("rest") => false,
        _ => return true,
    };
    run(server, databases, &mut || crate::jobs::stop(server)).finished
}

pub async fn now_please(State(server): State<Arc<Server>>, Signed(me): Signed) -> Result<StatusCode, Fail> {
    if !me.admin {
        return Err(Fail(StatusCode::FORBIDDEN, "Only an admin starts a copy.".into()));
    }
    server.store.set_setting(PENDING, "all").map_err(Fail::internal)?;
    server.jobs.nudge();
    Ok(StatusCode::ACCEPTED)
}

fn last(server: &Server) -> Last {
    server.store.setting(LAST).and_then(|text| serde_json::from_str(&text).ok()).unwrap_or_default()
}

fn save(server: &Server, last: &Last) {
    let _ = server.store.set_setting(LAST, &serde_json::to_string(last).unwrap_or_default());
}

fn stamp(server: &Server, key: &str) -> Option<i64> {
    server.store.setting(key)?.parse().ok()
}

pub fn status(server: &Server) -> Value {
    let root = root();
    let (exists, writable) = there(&root);
    let disk = exists.then(|| crate::safe::space(&root)).flatten();
    let report = last(server);
    let problem = server.store.setting(DATABASES_PROBLEM).filter(|problem| !problem.is_empty());
    json!({
        "where": root.display().to_string(), "exists": exists, "writable": writable,
        "total": disk.map(|(total, _)| total), "free": disk.map(|(_, free)| free),
        "last": stamp(server, OK), "finished": (report.finished > 0).then_some(report.finished),
        "files": report.files, "bytes": report.bytes, "copied": report.copied, "complete": report.complete, "problem": report.problem,
        "running": server.activity.running(Kind::Copy),
        "databases": { "count": dated(&server.data.join("backups")).len(), "newest": stamp(server, DATABASES_AT), "problem": problem },
    })
}

pub fn attention(server: &Server, now: i64) -> Vec<String> {
    let mut lines = Vec::new();
    if let Some(problem) = server.store.setting(DATABASES_PROBLEM).filter(|problem| !problem.is_empty()) {
        lines.push(format!("The last copy of the database failed: {problem}"));
    }
    let since = stamp(server, OK).max(stamp(server, SEEN));
    if let Some(since) = since.filter(|since| root().is_dir() && now - since > NIGHTS * DAY) {
        let why = last(server).problem.map(|problem| format!(": {problem}")).unwrap_or_default();
        lines.push(format!("The second copy has not been made for {} nights{why}", (now - since) / DAY));
    }
    lines
}

fn back_up(server: &Arc<Server>, date: &str) -> Result<usize, String> {
    let dir = server.data.join("backups");
    let partial = dir.join(format!(".{date}.partial"));
    let _ = std::fs::remove_dir_all(&partial);
    let copied = copy_databases(server, &partial);
    if let Err(problem) = copied {
        let _ = std::fs::remove_dir_all(&partial);
        return Err(problem);
    }
    let done = dir.join(date);
    let _ = std::fs::remove_dir_all(&done);
    std::fs::rename(&partial, &done).map_err(|err| format!("{}: {err}", done.display()))?;
    for old in dated(&dir).iter().rev().skip(KEPT) {
        let _ = std::fs::remove_dir_all(old);
    }
    copied
}

fn copy_databases(server: &Arc<Server>, into: &Path) -> Result<usize, String> {
    let failed = |path: &Path, err: &dyn std::fmt::Display| format!("{}: {err}", path.display());
    std::fs::create_dir_all(into).map_err(|err| failed(into, &err))?;
    let kept = into.join("server.db");
    server.store.back_up(&kept).map_err(|err| failed(&kept, &err))?;
    check(&kept)?;
    let own = numa_core::paths::data_dir().join("catalog.db");
    let mut catalogs = vec![(own.clone(), into.join(own.strip_prefix(&server.data).unwrap_or(Path::new("numa/catalog.db"))))];
    for library in server.home.all_libraries() {
        let place = match (library.path.strip_prefix(&server.home.photos), library.path.strip_prefix(&server.home.sent)) {
            (Ok(inside), _) => Path::new("photos").join(inside),
            (_, Ok(inside)) => Path::new("sent").join(inside),
            _ => continue,
        };
        catalogs.push((library.path.join(numa_io::catalog::LIBRARY_DIR).join("catalog.db"), into.join(place).join("catalog.db")));
    }

    server
        .home
        .blocking(move |_| {
            let mut copied = 0;
            for (from, to) in catalogs.iter().filter(|(from, _)| from.is_file()) {
                std::fs::create_dir_all(to.parent().unwrap_or(Path::new("."))).map_err(|err| failed(to, &err))?;
                let source = Connection::open_with_flags(from, OpenFlags::SQLITE_OPEN_READ_ONLY).map_err(|err| failed(from, &err))?;
                sqlite(&source, to).map_err(|err| failed(from, &err))?;
                check(to)?;
                copied += 1;
            }
            Ok(copied)
        })
        .unwrap_or_else(|| Err("the catalogs could not be reached".into()))
}

pub fn sqlite(source: &Connection, to: &Path) -> rusqlite::Result<()> {
    let mut copy = Connection::open(to)?;
    rusqlite::backup::Backup::new(source, &mut copy)?.run_to_completion(1024, std::time::Duration::from_millis(10), None)?;
    copy.pragma_update(None, "journal_mode", "DELETE")
}

fn check(path: &Path) -> Result<(), String> {
    let said: String = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)
        .and_then(|conn| conn.query_row("PRAGMA integrity_check", [], |row| row.get(0)))
        .map_err(|err| format!("{}: {err}", path.display()))?;
    match said.as_str() {
        "ok" => Ok(()),
        _ => Err(format!("{} is damaged: {said}", path.display())),
    }
}

fn dated(dir: &Path) -> Vec<PathBuf> {
    let date = |name: &str| name.len() == 10 && name.bytes().enumerate().all(|(at, byte)| if at == 4 || at == 7 { byte == b'-' } else { byte.is_ascii_digit() });
    let mut found: Vec<PathBuf> = std::fs::read_dir(dir)
        .map(|entries| entries.flatten().map(|entry| entry.path()).filter(|path| path.is_dir() && path.file_name().and_then(|name| name.to_str()).is_some_and(date)).collect())
        .unwrap_or_default();
    found.sort();
    found
}

fn newest(dir: &Path) -> Option<PathBuf> {
    dated(dir).pop()
}

#[derive(Default, Debug)]
struct Mirrored {
    files: u64,
    bytes: u64,
    copied: u64,
    problems: Vec<String>,
    stopped: bool,
}

struct One {
    from: PathBuf,
    to: PathBuf,
    size: u64,
    modified: SystemTime,
}

fn seconds(time: SystemTime) -> i64 {
    time.duration_since(UNIX_EPOCH).map_or(0, |since| since.as_secs() as i64)
}

fn differs(size: u64, modified: SystemTime, held: Option<(u64, SystemTime)>) -> bool {
    held.is_none_or(|(len, at)| len != size || (seconds(at) - seconds(modified)).abs() > 2)
}

fn mirror(
    sources: &[(PathBuf, PathBuf)],
    root: &Path,
    room: Option<u64>,
    stop: &mut dyn FnMut() -> bool,
    planned: &mut dyn FnMut(usize),
    one: &mut dyn FnMut(),
) -> Mirrored {
    let mut out = Mirrored::default();
    let mut todo: Vec<One> = Vec::new();
    for (from, to) in sources.iter().filter(|(from, _)| from.is_dir()) {
        let mut folders = vec![(from.clone(), to.clone())];
        while let Some((folder, there)) = folders.pop() {
            let entries = match std::fs::read_dir(&folder) {
                Ok(entries) => entries,
                Err(err) => {
                    out.problems.push(format!("{}: {err}", folder.display()));
                    continue;
                }
            };
            for entry in entries.flatten() {
                let (name, path) = (entry.file_name(), entry.path());
                let Ok(kind) = entry.file_type() else { continue };
                if name.as_bytes().starts_with(b".") || path == root {
                    continue;
                }
                if kind.is_dir() {
                    folders.push((path, there.join(&name)));
                    continue;
                }
                let Some((size, modified)) = kind.is_file().then(|| entry.metadata().ok()).flatten().and_then(|meta| Some((meta.len(), meta.modified().ok()?))) else {
                    continue;
                };
                out.files += 1;
                out.bytes += size;
                let to = there.join(&name);
                let held = std::fs::metadata(&to).ok().and_then(|meta| Some((meta.len(), meta.modified().ok()?)));
                if differs(size, modified, held) {
                    todo.push(One { from: path, to, size, modified });
                }
            }
        }
    }
    let needed: u64 = todo.iter().map(|one| one.size).sum();

    if let Some(free) = room.filter(|free| needed > *free) {
        out.problems.push(format!("there is not room on the second disk: {} to copy, {} free", amount(needed), amount(free)));
        out.files -= todo.len() as u64;
        out.bytes -= needed;
        return out;
    }
    planned(todo.len());

    out.files -= todo.len() as u64;
    out.bytes -= needed;
    for file in todo {
        if stop() {
            out.stopped = true;
            break;
        }
        match copy_one(&file) {
            Ok(()) => {
                out.copied += 1;
                out.files += 1;
                out.bytes += file.size;
            }
            Err(err) => out.problems.push(format!("{}: {err}", file.from.display())),
        }
        one();
    }
    out
}

fn copy_one(one: &One) -> std::io::Result<()> {
    let folder = one.to.parent().ok_or_else(|| std::io::Error::other("no folder"))?;
    std::fs::create_dir_all(folder)?;
    let name = one.to.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
    let part = folder.join(format!(".{name}.numa-copy"));
    let copied = (|| {
        let mut writer = File::create(&part)?;
        std::io::copy(&mut File::open(&one.from)?, &mut writer)?;
        writer.sync_all()?;
        writer.set_modified(one.modified)?;
        let arrived = std::fs::metadata(&part)?.len();
        if arrived != one.size {
            return Err(std::io::Error::other(format!("{arrived} of {} bytes arrived; it may have changed while it was copied", one.size)));
        }
        std::fs::rename(&part, &one.to)
    })();
    if copied.is_err() {
        let _ = std::fs::remove_file(&part);
    }
    copied
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn the_mirror_only_adds() {
        let base = std::env::temp_dir().join(format!("numa-home-copy-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let (home, copy) = (base.join("photos"), base.join("copy"));
        let write = |path: &Path, text: &str| {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, text).unwrap();
        };
        write(&home.join("2026/Japan/a.raf"), "aaaa");
        write(&home.join("2026/Japan/b.raf"), "bbbbbb");
        write(&home.join("Garden/c.jpg"), "c");
        write(&home.join("Garden/.numa/catalog.db"), "hidden");
        std::os::unix::fs::symlink(home.join("Garden/c.jpg"), home.join("Garden/link.jpg")).unwrap();
        write(&copy.join("photos/Garden/gone.jpg"), "deleted at home, kept here");
        let sources = [(home.clone(), copy.join("photos"))];
        let (mut planned, mut done) = (0, 0);
        let run = |stop: &mut dyn FnMut() -> bool, planned: &mut usize, done: &mut usize| {
            mirror(&sources, &copy, None, stop, &mut |n| *planned = n, &mut || *done += 1)
        };

        let mut after_one = 0;
        let first = run(&mut || { after_one += 1; after_one > 1 }, &mut planned, &mut done);
        assert!(first.stopped && first.copied == 1 && planned == 3, "{first:?}");
        let second = run(&mut || false, &mut planned, &mut done);
        assert_eq!((second.copied, planned, second.files, second.bytes), (2, 2, 3, 11), "resumed, not begun again: {second:?}");
        assert!(second.problems.is_empty() && !second.stopped);
        assert_eq!(std::fs::read_to_string(copy.join("photos/2026/Japan/b.raf")).unwrap(), "bbbbbb");
        assert!(copy.join("photos/Garden/gone.jpg").exists(), "nothing is deleted in the copy");
        assert!(!copy.join("photos/Garden/.numa").exists() && !copy.join("photos/Garden/link.jpg").exists());
        let source_time = std::fs::metadata(home.join("Garden/c.jpg")).unwrap().modified().unwrap();
        assert_eq!(seconds(std::fs::metadata(copy.join("photos/Garden/c.jpg")).unwrap().modified().unwrap()), seconds(source_time), "the date comes along");

        assert_eq!(run(&mut || false, &mut planned, &mut done).copied, 0, "the same files are not copied again");
        write(&home.join("Garden/c.jpg"), "changed");
        let third = run(&mut || false, &mut planned, &mut done);
        assert_eq!(third.copied, 1);
        assert_eq!(std::fs::read_to_string(copy.join("photos/Garden/c.jpg")).unwrap(), "changed");

        write(&home.join("Garden/d.jpg"), "dddd");
        let full = mirror(&sources, &copy, Some(3), &mut || false, &mut |_| {}, &mut || {});
        assert!(full.copied == 0 && full.problems[0].contains("not room"), "{full:?}");
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn what_differs() {
        let at = UNIX_EPOCH + Duration::from_secs(1_000_000);
        assert!(differs(10, at, None));
        assert!(differs(10, at, Some((11, at))));
        assert!(!differs(10, at, Some((10, at + Duration::from_secs(2)))), "FAT's two seconds");
        assert!(differs(10, at, Some((10, at + Duration::from_secs(3)))));
    }

    #[test]
    fn dated_folders_only() {
        let base = std::env::temp_dir().join(format!("numa-home-dated-{}", std::process::id()));
        for name in ["2026-10-01", "2026-10-03", "2026-10-02", ".2026-10-04.partial", "notes"] {
            std::fs::create_dir_all(base.join(name)).unwrap();
        }
        let names: Vec<String> = dated(&base).iter().map(|path| path.file_name().unwrap().to_string_lossy().into_owned()).collect();
        assert_eq!(names, ["2026-10-01", "2026-10-02", "2026-10-03"]);
        assert_eq!(newest(&base).unwrap().file_name().unwrap(), "2026-10-03");
        let _ = std::fs::remove_dir_all(&base);
    }
}
