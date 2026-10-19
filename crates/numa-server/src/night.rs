use std::collections::{HashMap, HashSet};
use std::io::Read;
use std::path::PathBuf;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

use crate::activity::Kind;
use crate::store::now;
use crate::Server;

const DONE: &str = "night.done";
const REPORT: &str = "night.report";

const CACHE_GB: u64 = 8;

const HEAD: u64 = 1 << 20;

const LISTED: usize = 50;

#[derive(Serialize, Deserialize, Default)]
pub struct Report {

    pub date: String,
    pub started: i64,
    pub finished: Option<i64>,
    pub done: Vec<String>,
    pub needs: Vec<Need>,

    pub stopped: Option<String>,
}

#[derive(Serialize, Deserialize)]
pub struct Need {
    pub text: String,

    pub link: String,
}

pub fn local() -> (u32, String) {

    let tm = unsafe {
        let mut tm: libc::tm = std::mem::zeroed();
        let at = libc::time(std::ptr::null_mut());
        libc::localtime_r(&at, &mut tm);
        tm
    };
    (tm.tm_hour as u32, format!("{:04}-{:02}-{:02}", tm.tm_year + 1900, tm.tm_mon + 1, tm.tm_mday))
}

fn window() -> (u32, u32) {
    let asked = std::env::var("NUMA_HOME_NIGHT").ok();
    let parsed = asked.as_deref().and_then(|text| text.split_once('-')).and_then(|(from, to)| Some((from.trim().parse().ok()?, to.trim().parse().ok()?)));
    parsed.unwrap_or((1, 6))
}

fn in_window(hour: u32, (from, to): (u32, u32)) -> bool {
    match from <= to {
        true => (from..to).contains(&hour),
        false => hour >= from || hour < to,
    }
}

pub fn is_night() -> bool {
    in_window(local().0, window())
}

pub fn due(server: &Server) -> bool {
    is_night() && server.store.setting(DONE) != Some(local().1)
}

pub fn last(server: &Server) -> Option<Report> {
    serde_json::from_str(&server.store.setting(REPORT)?).ok()
}

fn stop(server: &Server, report: &mut Report) -> bool {
    if server.activity.paused() {
        report.stopped = Some("Paused; the rest when the work is resumed.".into());
    } else if !is_night() {
        report.stopped = Some(format!("Morning came at {:02}:00; the rest is done the next night.", window().1));
    }
    report.stopped.is_some()
}

fn save(server: &Server, report: &Report) {
    let _ = server.store.set_setting(REPORT, &serde_json::to_string(report).unwrap_or_default());
}

fn plural(n: usize, one: &str, more: &str) -> String {
    format!("{n} {}", if n == 1 { one } else { more })
}

fn megabytes(bytes: u64) -> String {
    format!("{} MB", (bytes as f64 / 1e6).round())
}

type Step = fn(&Arc<Server>, &mut Report, &mut HashSet<i64>) -> bool;

pub fn run(server: &Arc<Server>, failed: &mut HashSet<i64>) {
    let (_, date) = local();
    let mut report = Report { date: date.clone(), started: now(), ..Report::default() };
    save(server, &report);
    server.activity.plan(Kind::Night, 0, None);
    server.activity.start(Kind::Night);
    let steps: [(&'static str, Step); 7] = [
        ("Trying again what failed", retry),
        ("Reading every library again in full", rescan),
        ("Checking that every file still reads", check),
        ("Emptying the trash's oldest", trash),
        ("Preparing Tidy Up", tidy),
        ("Compacting the catalogs", vacuum),
        ("Pruning the thumbnail cache", prune),
    ];
    for (stage, step) in steps {
        if stop(server, &mut report) {
            break;
        }
        server.activity.stage(Kind::Night, stage);
        let started = std::time::Instant::now();
        let finished = step(server, &mut report, failed);
        log::info!("night: {stage} in {:.1} s", started.elapsed().as_secs_f32());
        save(server, &report);
        if !finished {
            stop(server, &mut report);
            break;
        }
    }
    report.finished = Some(now());
    save(server, &report);

    if !server.activity.paused() {
        let _ = server.store.set_setting(DONE, &date);
    }
    server.activity.finish(Kind::Night);
}

fn retry(server: &Arc<Server>, report: &mut Report, failed: &mut HashSet<i64>) -> bool {
    if !failed.is_empty() {
        report.done.push(format!("Tried again: {} whose thumbnails or analysis failed by day", plural(failed.len(), "photograph", "photographs")));
        failed.clear();
    }
    let missing = server.activity.downloads().iter().filter(|download| download.state == "failed").count();
    if missing > 0 {
        crate::models::fetch(server);
        numa_infer::release_all();
        let still: Vec<String> = server.activity.downloads().iter().filter(|download| download.state == "failed").map(|download| download.name.clone()).collect();
        report.done.push(format!("Tried the models' downloads again: {} of {missing} came", missing - still.len().min(missing)));
        for name in still {
            report.needs.push(Need { text: format!("{name} could not be downloaded; is the NAS on the internet?"), link: "#/activity".into() });
        }
    }
    true
}

fn rescan(server: &Arc<Server>, report: &mut Report, _: &mut HashSet<i64>) -> bool {
    let changed = crate::jobs::scan(server, true);
    let libraries = server.home.all_libraries();

    let (photos, sent) = (server.home.photos.clone(), server.home.sent.clone());
    let known = server.home.blocking(|catalog| catalog.libraries().unwrap_or_default()).unwrap_or_default();
    for library in known.iter().filter(|library| (library.path.starts_with(&photos) || library.path.parent() == Some(sent.as_path())) && !library.path.is_dir()) {
        report.needs.push(Need {
            text: format!("The folder of {} is missing ({}); put it or its disk back and its photographs return as they were", library.label(), library.path.display()),
            link: "#/libraries".into(),
        });
    }
    report.done.push(format!("Every library read again in full: {}, {}", plural(libraries.len(), "library", "libraries"), plural(changed, "change", "changes")));
    true
}

fn check(server: &Arc<Server>, report: &mut Report, _: &mut HashSet<i64>) -> bool {
    let (mut read, mut receipted, mut problems) = (0usize, 0usize, Vec::new());
    for library in server.home.all_libraries().into_iter().filter(|library| library.path.is_dir()) {
        let receipts: HashMap<PathBuf, u128> = numa_io::receipt::all(&library.path)
            .iter()
            .flat_map(|receipt| numa_io::receipt::read(receipt, &library.path))
            .map(|(hash, file)| (file, hash))
            .collect();
        for photo in server.home.files(library.id) {
            if read % 64 == 0 && stop(server, report) {
                report.done.push(format!("Checked {read} files before stopping"));
                return false;
            }
            read += 1;
            let place = format!("{}/{}", library.label(), crate::manage::within(&library, &photo.path));
            let link = format!("#/?library={}", library.id);
            let problem = match receipts.get(&photo.path) {
                Some(hash) => {
                    receipted += 1;
                    match numa_io::offload::hash_file(&photo.path) {
                        Ok(now) if now == *hash => None,
                        Ok(_) => Some(format!("{place} is not what was imported: its receipt's hash differs")),
                        Err(err) => Some(format!("{place} could not be read: {err}")),
                    }
                }
                None => head(&photo.path).err().map(|err| format!("{place} could not be read: {err}")),
            };
            problems.extend(problem.map(|text| Need { text, link }));
        }
    }
    report.done.push(format!("Every file read: {read}, {receipted} of them whole against their receipts; {} unreadable or changed", problems.len()));
    let more = problems.len().saturating_sub(LISTED);
    report.needs.extend(problems.into_iter().take(LISTED));
    if more > 0 {
        report.needs.push(Need { text: format!("And {more} more files that could not be read"), link: "#/libraries".into() });
    }
    true
}

fn head(path: &std::path::Path) -> std::io::Result<()> {
    let mut file = std::fs::File::open(path)?;
    let mut buffer = Vec::new();
    match (&mut file).take(HEAD).read_to_end(&mut buffer)? {
        0 => Err(std::io::Error::other("the file is empty")),
        _ => Ok(()),
    }
}

fn trash(server: &Arc<Server>, report: &mut Report, _: &mut HashSet<i64>) -> bool {
    let emptied = crate::manage::empty_old(server);
    if emptied > 0 {
        report.done.push(format!("Emptied the trash of {} past their 30 days", plural(emptied, "photograph", "photographs")));
    }
    true
}

fn tidy(server: &Arc<Server>, report: &mut Report, _: &mut HashSet<i64>) -> bool {

    for library in server.home.all_libraries() {
        let home = server.clone();
        server.home.blocking(move |catalog| {
            let _ = numa_io::analysis::regroup_bursts(catalog, library.id);
            home.shelve(Some(library.id), catalog);
        });
    }

    let counts = crate::tidy::prepare(server);
    let count = |kind| counts.get(kind).copied().unwrap_or(0);
    let total = count("copies") + count("bursts") + count("unusable");
    let line = match total {
        0 => "Tidy Up: nothing to choose".to_string(),
        _ => format!(
            "Tidy Up: {}, {} and {} to choose from",
            plural(count("copies"), "set of exact copies", "sets of exact copies"),
            plural(count("bursts"), "run of the same frame", "runs of the same frame"),
            plural(count("unusable"), "card of unusable frames", "cards of unusable frames"),
        ),
    };
    report.done.push(line);
    if total > 0 {
        report.needs.push(Need { text: format!("{} in Tidy Up {} for a choice", plural(total, "set", "sets"), if total == 1 { "waits" } else { "wait" }), link: "#/tidy?everyone=1".into() });
    }
    true
}

fn vacuum(server: &Arc<Server>, report: &mut Report, _: &mut HashSet<i64>) -> bool {
    let mut files: Vec<PathBuf> = server.home.all_libraries().iter().map(|library| library.path.join(numa_io::catalog::LIBRARY_DIR).join("catalog.db")).collect();
    files.push(numa_core::paths::data_dir().join("catalog.db"));
    let size = |files: &[PathBuf]| files.iter().filter_map(|file| std::fs::metadata(file).ok()).map(|meta| meta.len()).sum::<u64>();
    let before = size(&files) + std::fs::metadata(server.data.join("server.db")).map_or(0, |meta| meta.len());
    if let Err(err) = server.store.vacuum() {
        log::warn!("night: server.db: {err}");
    }

    let compacting = files.clone();
    server.home.blocking(move |_| {
        for file in compacting.iter().filter(|file| file.exists()) {
            if let Err(err) = rusqlite::Connection::open(file).and_then(|conn| conn.execute_batch("VACUUM")) {
                log::warn!("night: {}: {err}", file.display());
            }
        }
    });
    let after = size(&files) + std::fs::metadata(server.data.join("server.db")).map_or(0, |meta| meta.len());
    report.done.push(format!("{} catalogs compacted, {} freed", files.len() + 1, megabytes(before.saturating_sub(after))));
    true
}

fn prune(_: &Arc<Server>, report: &mut Report, _: &mut HashSet<i64>) -> bool {
    let gigabytes = std::env::var("NUMA_HOME_CACHE_GB").ok().and_then(|text| text.parse().ok()).unwrap_or(CACHE_GB);
    let freed = numa_io::thumbs::prune(gigabytes << 30);
    report.done.push(match freed {
        0 => format!("The thumbnail cache is within its {gigabytes} GB"),
        _ => format!("The thumbnail cache pruned to its {gigabytes} GB, {} freed", megabytes(freed)),
    });
    true
}

#[cfg(test)]
mod tests {
    #[test]
    fn the_window_may_cross_midnight() {
        assert!(super::in_window(1, (1, 6)) && super::in_window(5, (1, 6)) && !super::in_window(6, (1, 6)) && !super::in_window(0, (1, 6)));
        assert!(super::in_window(23, (22, 4)) && super::in_window(3, (22, 4)) && !super::in_window(12, (22, 4)));
        assert!((0..24).all(|hour| super::in_window(hour, (0, 24))));
    }
}
