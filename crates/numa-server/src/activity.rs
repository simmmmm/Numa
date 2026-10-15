use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::time::{Duration, Instant};

use axum::extract::State;
use axum::http::StatusCode;
use axum::Json;
use serde_json::{json, Value};

use crate::auth::Signed;
use crate::{Fail, Server};

const PAUSED: &str = "paused";

#[derive(Clone, Copy)]
pub enum Kind {
    Scan,
    Thumbnails,
    Analyse,
    Words,
    Things,
}

const TEXTS: [(&str, &str, &str, &str); 5] = [
    ("scan", "Looking for Photographs", "Looking for new and changed photographs", "Every library looked through"),
    ("thumbnails", "Thumbnails", "Making thumbnails, edits applied", "Every photograph has its thumbnails"),
    ("analyse", "Faces, Sharpness and Bursts", "Finding faces and measuring sharpness", "Every photograph analysed"),
    ("words", "Search in Words", "Reading what is in each photograph", "Every photograph read for words"),
    ("things", "Things Numa Saw", "Seeing which everyday things each photograph has", "Every photograph looked at"),
];

#[derive(Clone, Copy, PartialEq, Default)]
enum Is {
    #[default]
    Idle,
    Waiting,
    Running,

    Waits(&'static str),
}

#[derive(Default)]
struct Job {
    is: Is,

    stage: Option<&'static str>,
    library: Option<i64>,
    done: usize,
    total: usize,
    started: Option<Instant>,
    last: Option<Instant>,
}

pub struct Download {
    pub name: String,
    pub total: u64,
    pub parts: Vec<PathBuf>,
    pub state: &'static str,
}

#[derive(Default)]
pub struct Activity {
    paused: AtomicBool,
    jobs: Mutex<[Job; 5]>,
    downloads: Mutex<Vec<Download>>,
}

impl Activity {
    pub fn new(store: &crate::store::Store) -> Self {
        let activity = Self::default();
        activity.paused.store(store.setting(PAUSED).as_deref() == Some("1"), Ordering::Relaxed);
        activity
    }

    pub fn paused(&self) -> bool {
        self.paused.load(Ordering::Relaxed)
    }

    pub fn hold(&self) {
        while self.paused() {
            std::thread::sleep(Duration::from_secs(1));
        }
    }

    fn job<R>(&self, kind: Kind, change: impl FnOnce(&mut Job) -> R) -> R {
        change(&mut self.jobs.lock().unwrap_or_else(|poisoned| poisoned.into_inner())[kind as usize])
    }

    pub fn plan(&self, kind: Kind, total: usize, waits: Option<&'static str>) {
        self.job(kind, |job| {
            job.is = match (waits, total) {
                (Some(why), _) => Is::Waits(why),
                (None, 0) => Is::Idle,
                (None, _) => Is::Waiting,
            };
            (job.total, job.done, job.library, job.stage) = (total, 0, None, None);
        })
    }

    pub fn start(&self, kind: Kind) {
        self.job(kind, |job| {
            if job.is != Is::Idle || matches!(kind, Kind::Scan) {
                (job.is, job.started) = (Is::Running, Some(Instant::now()));
            }
        })
    }

    pub fn at(&self, kind: Kind, library: i64, stage: Option<&'static str>) {
        self.job(kind, |job| (job.library, job.stage) = (Some(library), stage))
    }

    pub fn add(&self, kind: Kind, done: usize) {
        self.job(kind, |job| job.done += done)
    }

    pub fn total(&self, kind: Kind, total: usize) {
        self.job(kind, |job| job.total = total)
    }

    pub fn finish(&self, kind: Kind) {
        self.job(kind, |job| *job = Job { last: Some(Instant::now()), ..Job::default() })
    }

    pub fn downloads(&self) -> MutexGuard<'_, Vec<Download>> {
        self.downloads.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

fn left(job: &Job) -> Option<u64> {
    let gone = job.started?.elapsed();
    (job.done > 0 && gone >= Duration::from_secs(3) && job.total > job.done)
        .then(|| (gone.as_secs_f64() / job.done as f64 * (job.total - job.done) as f64).round() as u64)
}

pub async fn status(State(server): State<Arc<Server>>, Signed(me): Signed) -> Json<Value> {

    let seen = server.home.libraries(&me.name);
    let paused = server.activity.paused();
    let jobs = server.activity.jobs.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let rows: Vec<Value> = jobs
        .iter()
        .zip(TEXTS)
        .map(|(job, (key, name, running, idle))| {
            let (state, doing) = match job.is {
                Is::Idle => ("idle", idle.to_string()),
                Is::Waits(why) => ("waits", why.to_string()),
                _ if paused => ("paused", "Paused".to_string()),
                Is::Waiting => ("waiting", "Waits its turn".to_string()),
                Is::Running => ("running", job.stage.unwrap_or(running).to_string()),
            };
            let library = job.library.and_then(|id| seen.iter().find(|library| library.id == id)).map(|library| library.label());
            json!({
                "job": key, "name": name, "state": state, "doing": doing, "library": library,
                "done": job.done, "total": job.total,
                "left": if state == "running" { left(job) } else { None },
                "ago": job.last.map(|last| last.elapsed().as_secs()),
            })
        })
        .collect();
    drop(jobs);
    let downloads: Vec<Value> = server
        .activity
        .downloads()
        .iter()
        .map(|download| {
            let grown = download.parts.iter().filter_map(|part| std::fs::metadata(part).ok()).map(|meta| meta.len()).max().unwrap_or(0);
            let done = if download.state == "done" { download.total } else { grown.min(download.total) };
            json!({ "name": download.name, "state": download.state, "done": done, "total": download.total })
        })
        .collect();
    let busy = rows.iter().any(|row| matches!(row["state"].as_str(), Some("running" | "waiting")))
        || downloads.iter().any(|row| matches!(row["state"].as_str(), Some("fetching" | "waiting")));
    Json(json!({ "paused": paused, "busy": busy, "jobs": rows, "downloads": downloads }))
}

async fn set(server: Arc<Server>, me: crate::store::Account, paused: bool) -> Result<Json<Value>, Fail> {
    if !me.admin {
        return Err(Fail(StatusCode::FORBIDDEN, "Only an admin pauses the background work.".into()));
    }
    server.store.set_setting(PAUSED, if paused { "1" } else { "0" }).map_err(Fail::internal)?;
    server.activity.paused.store(paused, Ordering::Relaxed);
    if !paused {
        server.jobs.nudge();
    }
    Ok(Json(json!({ "paused": paused })))
}

pub async fn pause(State(server): State<Arc<Server>>, Signed(me): Signed) -> Result<Json<Value>, Fail> {
    set(server, me, true).await
}

pub async fn resume(State(server): State<Arc<Server>>, Signed(me): Signed) -> Result<Json<Value>, Fail> {
    set(server, me, false).await
}
