mod accounts;
mod activity;
mod auth;
mod by;
mod cards;
mod faces;
mod home;
mod jobs;
mod models;
mod pages;
mod photos;
mod send;
mod store;
mod timeline;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post, put};
use axum::{Json, Router};

pub struct Server {
    pub store: store::Store,
    pub home: home::Home,
    pub data: PathBuf,

    pub words: Mutex<HashMap<i64, numa_io::words::Index>>,
    pub jobs: jobs::Nudge,

    pub activity: activity::Activity,

    pub things: std::sync::OnceLock<Vec<Vec<f32>>>,

    pub artists: by::Artists,

    pub cards: cards::Cards,

    pub scanned: std::sync::atomic::AtomicBool,

    pub decodes: tokio::sync::Semaphore,
    pub renders: tokio::sync::Semaphore,
}

impl Server {

    pub fn shelve(&self, only: Option<i64>, catalog: &numa_io::catalog::Catalog) {
        self.store.seen(&self.home.shelve(only, catalog));
    }
}

#[derive(Debug)]
pub struct Fail(pub StatusCode, pub String);

impl Fail {
    pub fn internal(why: impl std::fmt::Display) -> Self {
        log::error!("{why}");
        Fail(StatusCode::INTERNAL_SERVER_ERROR, "Something went wrong on the server; its log says what.".into())
    }

    pub fn bad(why: impl Into<String>) -> Self {
        Fail(StatusCode::BAD_REQUEST, why.into())
    }
}

impl IntoResponse for Fail {
    fn into_response(self) -> Response {
        (self.0, Json(serde_json::json!({ "error": self.1 }))).into_response()
    }
}

fn path_from(var: &str, default: &str) -> PathBuf {
    std::env::var_os(var).map(PathBuf::from).unwrap_or_else(|| PathBuf::from(default))
}

fn main() -> Result<(), String> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();
    let photos = path_from("NUMA_HOME_PHOTOS", "/photos");
    let sent = path_from("NUMA_HOME_SENT", "/sent");
    let data = path_from("NUMA_HOME_DATA", "/data");
    let port: u16 = std::env::var("NUMA_HOME_PORT").ok().and_then(|port| port.parse().ok()).unwrap_or(8283);

    std::env::set_var("XDG_DATA_HOME", &data);
    std::env::set_var("XDG_CACHE_HOME", data.join("cache"));
    std::env::set_var("NUMA_MODELS", data.join("models"));

    if std::env::var_os("NUMA_MODEL_THREADS").is_none() {
        std::env::set_var("NUMA_MODEL_THREADS", jobs::half_the_cores().to_string());
    }
    for dir in [&photos, &sent, &data] {
        std::fs::create_dir_all(dir).map_err(|err| format!("{}: {err}", dir.display()))?;
    }

    let store = store::Store::open(&data.join("server.db"))?;
    let server = Arc::new(Server {
        activity: activity::Activity::new(&store),
        things: Default::default(),
        artists: Default::default(),
        store,
        home: home::Home::start(photos, sent)?,
        data,
        words: Mutex::default(),
        jobs: jobs::Nudge::default(),
        cards: cards::Cards::default(),
        scanned: Default::default(),
        decodes: tokio::sync::Semaphore::new(std::thread::available_parallelism().map_or(2, |n| n.get())),
        renders: tokio::sync::Semaphore::new(1),
    });
    jobs::start(server.clone());
    cards::watch(server.clone(), path_from("NUMA_HOME_CARDS", "/cards"));

    let runtime = tokio::runtime::Runtime::new().map_err(|err| err.to_string())?;
    runtime.block_on(async move {
        let listener = tokio::net::TcpListener::bind(("0.0.0.0", port)).await.map_err(|err| err.to_string())?;
        log::info!("Numa Home {} on port {port}", env!("CARGO_PKG_VERSION"));
        axum::serve(listener, router(server)).with_graceful_shutdown(stopped()).await.map_err(|err| err.to_string())
    })
}

async fn stopped() {
    let mut term = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()).expect("a SIGTERM handler");
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {}
        _ = term.recv() => {}
    }
}

fn router(server: Arc<Server>) -> Router {
    Router::new()
        .route("/api/v1/status", get(accounts::status))
        .route("/api/v1/setup", post(accounts::setup))
        .route("/api/v1/login", post(accounts::login))
        .route("/api/v1/logout", post(accounts::logout))
        .route("/api/v1/me", get(accounts::me))
        .route("/api/v1/users", get(accounts::users).post(accounts::add_user))
        .route("/api/v1/libraries", get(timeline::libraries))
        .route("/api/v1/timeline", get(timeline::months))
        .route("/api/v1/timeline/{month}", get(timeline::month))
        .route("/api/v1/people", get(timeline::people))
        .route("/api/v1/albums", get(timeline::albums))
        .route("/api/v1/activity", get(activity::status))
        .route("/api/v1/activity/pause", post(activity::pause))
        .route("/api/v1/activity/resume", post(activity::resume))
        .route("/api/v1/things", get(timeline::things))
        .route("/api/v1/by", get(by::counts))
        .route("/api/v1/cameras", get(by::cameras).put(by::link))
        .route("/api/v1/card", get(cards::status))
        .route("/api/v1/card/import", post(cards::start))
        .route("/api/v1/faces", get(faces::unnamed))
        .route("/api/v1/faces/{id}/portrait", get(faces::portrait))
        .route("/api/v1/faces/name", post(faces::name))
        .route("/api/v1/faces/ignore", post(faces::ignore))
        .route("/api/v1/search", get(timeline::search))
        .route("/api/v1/photos/{id}", get(photos::detail))
        .route("/api/v1/photos/{id}/thumb", get(photos::thumb))
        .route("/api/v1/photos/{id}/preview", get(photos::preview))
        .route("/api/v1/photos/{id}/original", get(photos::original))
        .route("/api/v1/photos/{id}/jpeg", get(photos::jpeg))
        .route("/api/v1/photos/{id}/edits", get(photos::edits_get).put(photos::edits_put).layer(axum::extract::DefaultBodyLimit::max(64 << 20)))
        .route("/api/v1/photos/{id}/marks", put(photos::marks))
        .route("/api/v1/libraries/{id}/photos", get(timeline::in_step))
        .route("/api/v1/send/{name}", put(send::send).layer(axum::extract::DefaultBodyLimit::disable()))
        .route("/api/{*rest}", axum::routing::any(|| async { Fail(StatusCode::NOT_FOUND, "There is no such address in the API.".into()) }))
        .merge(pages::routes())
        .with_state(server)
}
