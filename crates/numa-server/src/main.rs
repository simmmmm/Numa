mod accounts;
mod activity;
mod album;
mod auth;
mod by;
mod cards;
mod collect;
mod copy;
mod edit;
mod faces;
mod grants;
mod guard;
mod home;
mod jobs;
mod manage;
mod map;
mod mark;
mod models;
mod night;
mod pages;
mod safe;
mod photos;
mod places;
mod proof;
mod reach;
mod selection;
mod serve;
mod send;
mod share;
mod shared;
#[cfg(test)]
mod shared_http;
mod start;
mod store;
mod sync;
mod tidy;
mod timeline;
mod today;
mod trip_link;
mod album_sync;
mod video;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::routing::{delete, get, post, put};
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

    pub map: map::Map,

    pub cards: cards::Cards,

    pub scanned: std::sync::atomic::AtomicBool,

    pub decodes: tokio::sync::Semaphore,
    pub renders: tokio::sync::Semaphore,

    pub tidy: tidy::Tidy,

    pub editing: edit::Editing,

    pub limit: auth::Limit,

    pub pace: guard::Pace,

    pub setup_code: String,
}

impl Server {

    pub fn new(store: store::Store, photos: PathBuf, sent: PathBuf, data: PathBuf) -> Result<Arc<Self>, String> {
        Ok(Arc::new(Server {
            activity: activity::Activity::new(&store),
            things: Default::default(),
            artists: Default::default(),
            map: Default::default(),
            store,
            home: home::Home::start(photos, sent)?,
            data,
            words: Mutex::default(),
            jobs: jobs::Nudge::default(),
            cards: cards::Cards::default(),
            scanned: Default::default(),
            decodes: tokio::sync::Semaphore::new(std::thread::available_parallelism().map_or(2, |n| n.get())),
            renders: tokio::sync::Semaphore::new(1),
            tidy: tidy::Tidy::default(),
            editing: Default::default(),
            limit: Default::default(),
            pace: Default::default(),
            setup_code: auth::setup_code(),
        }))
    }

    pub fn shelve(&self, only: Option<i64>, catalog: &numa_io::catalog::Catalog) {
        let ids = self.home.shelve(only, catalog);
        map::kept(self, catalog, &ids);
        self.store.seen(&ids);
    }
}

#[derive(Debug)]
pub struct Fail(pub StatusCode, pub String);

impl Fail {
    pub fn internal(why: impl std::fmt::Display) -> Self {
        log::error!("{why}");
        Fail(StatusCode::INTERNAL_SERVER_ERROR, "Something went wrong on the server; its log says what.".into())
    }

    pub fn unreadable(why: impl std::fmt::Display) -> Self {
        log::warn!("unreadable: {why}");
        Fail(StatusCode::UNPROCESSABLE_ENTITY, "Numa could not read this photograph.".into())
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

    let arguments: Vec<String> = std::env::args().skip(1).collect();
    if let [command, name] = arguments.as_slice() {
        if command == "reset-password" {
            let store = store::Store::open(&data.join("server.db"))?;
            let password = accounts::reset_password(&store, name)?;
            println!("{name} has this password until the next sign-in, which asks for a new one, on Numa Home's page in a browser:\n{password}");
            return Ok(());
        }
    }
    let port: u16 = std::env::var("NUMA_HOME_PORT").ok().and_then(|port| port.parse().ok()).unwrap_or(8283);

    std::env::set_var("XDG_DATA_HOME", &data);
    std::env::set_var("XDG_CACHE_HOME", data.join("cache"));
    std::env::set_var("NUMA_MODELS", data.join("models"));

    if std::env::var_os("NUMA_MODEL_THREADS").is_none() {
        std::env::set_var("NUMA_MODEL_THREADS", jobs::half_the_cores().to_string());
    }

    extern "C" {
        fn tzset();
    }

    unsafe { tzset() };
    for dir in [&photos, &sent, &data] {
        std::fs::create_dir_all(dir).map_err(|err| format!("{}: {err}", dir.display()))?;
    }

    let store = store::Store::open(&data.join("server.db"))?;
    share::tables(&store)?;
    shared::tables(&store)?;
    let server = Server::new(store, photos, sent, data)?;
    if !server.store.has_accounts() {
        log::warn!("Numa Home is not set up yet. Set it up from your home network, or from elsewhere with this code: {}", server.setup_code);
    }
    jobs::start(server.clone());
    cards::watch(server.clone(), path_from("NUMA_HOME_CARDS", "/cards"));

    let idle = server.clone();
    let cores = std::thread::available_parallelism().map_or(2, |n| n.get());
    std::thread::spawn(move || loop {
        std::thread::sleep(std::time::Duration::from_secs(30));
        if idle.renders.available_permits() == 1 && idle.decodes.available_permits() == cores {
            numa_render::forget_kept();

            #[cfg(target_env = "gnu")]
            unsafe {
                libc::malloc_trim(0)
            };
        }
    });

    let runtime = tokio::runtime::Runtime::new().map_err(|err| err.to_string())?;
    runtime.block_on(async move {
        let listener = tokio::net::TcpListener::bind(("0.0.0.0", port)).await.map_err(|err| err.to_string())?;
        log::info!("Numa Home {} on port {port}", env!("CARGO_PKG_VERSION"));
        serve::serve(listener, router(server), serve::HEAD_TIME, serve::CONNECTIONS, stopped()).await;
        Ok(())
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
        .route("/api/v1/start", put(start::moved))
        .route("/api/v1/start/libraries", get(start::libraries))
        .route("/api/v1/start/count", get(start::count))
        .route("/api/v1/start/thumb", get(start::thumb))
        .route("/api/v1/start/qr", get(start::qr))
        .route("/api/v1/login", post(accounts::login))
        .route("/api/v1/logout", post(accounts::logout))
        .route("/api/v1/me", get(accounts::me).put(accounts::set_start))
        .route("/api/v1/me/password", put(accounts::change_password))
        .route("/api/v1/sessions", get(accounts::sessions))
        .route("/api/v1/sessions/others", post(accounts::end_others))
        .route("/api/v1/sessions/{id}", delete(accounts::end_session))
        .route("/api/v1/users", get(accounts::users).post(accounts::add_user))
        .route("/api/v1/users/{name}", put(accounts::set_user))
        .route("/api/v1/users/{name}/password", put(accounts::give_password))
        .route("/api/v1/users/{name}/grants", get(accounts::grants).put(accounts::set_grants))
        .route("/api/v1/libraries", get(timeline::libraries).post(manage::add))
        .route("/api/v1/libraries/folders", get(manage::folders))
        .route("/api/v1/libraries/{id}", put(manage::rename))
        .route("/api/v1/libraries/remove", post(manage::remove_libraries))
        .route("/api/v1/libraries/delete", post(manage::delete_libraries))
        .route("/api/v1/libraries/combine", post(manage::combine))
        .route("/api/v1/trash/libraries", get(manage::dropped))
        .route("/api/v1/trash/libraries/restore", post(manage::undrop))
        .route("/api/v1/trash", get(manage::list))
        .route("/api/v1/trash/{id}/thumb", get(manage::thumb))
        .route("/api/v1/trash/restore", post(manage::restore))
        .route("/api/v1/photos/remove", post(selection::remove))
        .route("/api/v1/photos/delete", post(selection::delete))
        .route("/api/v1/photos/keep-jpeg", get(selection::keeping).post(selection::keep_jpeg))
        .route("/api/v1/photos/move", post(selection::moved))
        .route("/api/v1/photos/album", post(selection::album))
        .route("/api/v1/photos/hashes", post(sync::hashes))
        .route("/api/v1/photos/marks", post(selection::marks))
        .route("/api/v1/photos/by", post(selection::by))
        .route("/api/v1/photos/zip", get(selection::zip))
        .route("/api/v1/today", get(today::today))
        .route("/api/v1/timeline", get(timeline::months))
        .route("/api/v1/timeline/{month}", get(timeline::month))
        .route("/api/v1/people", get(timeline::people))
        .route("/api/v1/albums", get(timeline::albums))
        .route("/api/v1/albums/{key}", put(timeline::rename_album).delete(timeline::delete_album))
        .route("/api/v1/albums/{key}/page", get(album::page))
        .route("/api/v1/albums/{key}/kind", put(album::kind))
        .route("/api/v1/albums/{key}/chapters", post(album::add_chapter))
        .route("/api/v1/albums/{key}/places", post(album::take_places))
        .route("/api/v1/chapters/{key}", put(album::change_chapter).delete(album::remove_chapter))
        .route("/api/v1/chapters/{key}/photos", post(album::put_photos))
        .route("/api/v1/places/find", get(album::find_places))

        .route("/api/v1/background", get(activity::status))
        .route("/api/v1/safe", get(safe::show))
        .route("/api/v1/reach", get(reach::show).put(reach::change))
        .route("/api/v1/reach/ping", get(reach::ping))
        .route("/api/v1/reach/signins", get(reach::sign_ins))
        .route("/api/v1/copy", post(copy::now_please))
        .route("/api/v1/background/pause", post(activity::pause))
        .route("/api/v1/background/resume", post(activity::resume))
        .route("/api/v1/rights", get(photos::rights).put(photos::rights_put))
        .route("/api/v1/tidy", get(tidy::list))
        .route("/api/v1/tidy/keep", post(tidy::keep))
        .route("/api/v1/filters", get(timeline::filters))
        .route("/api/v1/cameras", get(by::cameras).put(by::link))
        .route("/api/v1/card", get(cards::status))
        .route("/api/v1/card/import", post(cards::start))
        .route("/api/v1/card/photos", get(cards::photos))
        .route("/api/v1/card/thumb/{n}", get(cards::thumb))
        .route("/api/v1/card/names", get(cards::names))
        .route("/api/v1/libraries/{id}/names", get(places::library))
        .route("/api/v1/places", get(map::places))
        .route("/api/v1/map/bring", post(map::again))
        .route("/api/v1/map/{z}/{x}/{y}", get(map::tile))
        .route("/api/v1/faces", get(faces::unnamed))
        .route("/api/v1/faces/{id}/portrait", get(faces::portrait))
        .route("/api/v1/faces/name", post(faces::name))
        .route("/api/v1/faces/ignore", post(faces::ignore))
        .route("/api/v1/search", get(timeline::search))
        .route("/api/v1/photos/{id}", get(photos::detail))
        .route("/api/v1/photos/{id}/thumb", get(photos::thumb))
        .route("/api/v1/photos/{id}/preview", get(photos::preview))
        .route("/api/v1/photos/{id}/sharpness", get(photos::sharpness_now))
        .route("/api/v1/photos/{id}/original", get(photos::original))
        .route("/api/v1/photos/{id}/video", get(video::play))
        .route("/api/v1/photos/{id}/live", get(video::live))
        .route("/api/v1/photos/{id}/jpeg", get(photos::jpeg))
        .route("/api/v1/photos/{id}/edit", get(edit::get).post(edit::post))
        .route("/api/v1/photos/{id}/frame", get(edit::frame))
        .route("/api/v1/photos/{id}/edits", get(photos::edits_get).put(photos::edits_put).layer(axum::extract::DefaultBodyLimit::max(64 << 20)))
        .route("/api/v1/photos/{id}/marks", put(photos::marks))
        .route("/api/v1/photos/{id}/caption", put(photos::caption))
        .route("/api/v1/libraries/{id}/photos", get(timeline::in_step))
        .route("/api/v1/libraries/{id}/restore", post(manage::restore_paths))
        .route("/api/v1/libraries/{id}/sync", delete(sync::stop))
        .route("/api/v1/sync", get(sync::devices))
        .route("/api/v1/send/{name}", put(send::send).layer(axum::extract::DefaultBodyLimit::disable()))
        .route("/api/{*rest}", axum::routing::any(|| async { Fail(StatusCode::NOT_FOUND, "There is no such address in the API.".into()) }))
        .merge(pages::routes())
        .merge(share::routes())
        .merge(shared::routes())
        .merge(collect::routes())
        .merge(trip_link::routes())
        .merge(album_sync::routes())
        .layer(axum::middleware::from_fn_with_state(server.clone(), guard::guarded))
        .with_state(server)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_unreadable_photograph_does_not_tell_its_path() {
        let fail = Fail::unreadable("/nas/photos/2026/Secret trip/IMG_1.jpg: a picture of 65535x65535 is too large to open");
        assert_eq!(fail.0, StatusCode::UNPROCESSABLE_ENTITY);
        assert!(!fail.1.contains("/nas") && !fail.1.contains("Secret"), "{}", fail.1);
    }
}
