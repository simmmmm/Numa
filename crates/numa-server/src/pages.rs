use std::sync::Arc;

use axum::http::header;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;

use crate::Server;

fn file(kind: &'static str, body: &'static [u8]) -> impl IntoResponse {
    ([(header::CONTENT_TYPE, kind), (header::CACHE_CONTROL, "no-cache")], body)
}

pub fn routes() -> Router<Arc<Server>> {
    Router::new()
        .route("/", get(|| async { file("text/html; charset=utf-8", include_bytes!("web/index.html")) }))
        .route("/app.js", get(|| async { file("text/javascript; charset=utf-8", include_bytes!("web/app.js")) }))
        .route("/scrubber.js", get(|| async { file("text/javascript; charset=utf-8", include_bytes!("web/scrubber.js")) }))
        .route("/activity.js", get(|| async { file("text/javascript; charset=utf-8", include_bytes!("web/activity.js")) }))
        .route("/import.js", get(|| async { file("text/javascript; charset=utf-8", include_bytes!("web/import.js")) }))
        .route("/tidy.js", get(|| async { file("text/javascript; charset=utf-8", include_bytes!("web/tidy.js")) }))
        .route("/manage.js", get(|| async { file("text/javascript; charset=utf-8", include_bytes!("web/manage.js")) }))
        .route("/app.css", get(|| async { file("text/css; charset=utf-8", include_bytes!("web/app.css")) }))
        .route("/icon.svg", get(|| async { file("image/svg+xml", include_bytes!("web/icon.svg")) }))
        .route("/icon-180.png", get(|| async { file("image/png", include_bytes!("web/icon-180.png")) }))
        .route("/icon-512.png", get(|| async { file("image/png", include_bytes!("web/icon-512.png")) }))
        .route("/manifest.webmanifest", get(|| async { file("application/manifest+json", include_bytes!("web/manifest.webmanifest")) }))
        .route("/mark.png", get(|| async { file("image/png", include_bytes!("../../../data/branding/mark.png")) }))
}
