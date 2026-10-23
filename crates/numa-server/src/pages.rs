use std::sync::Arc;

use axum::extract::Path;
use axum::http::header;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;

use crate::Server;

fn file(kind: &'static str, body: &'static [u8]) -> impl IntoResponse {
    ([(header::CONTENT_TYPE, kind), (header::CACHE_CONTROL, "no-cache")], body)
}

fn kept(kind: &'static str, body: &'static [u8]) -> impl IntoResponse {
    ([(header::CONTENT_TYPE, kind), (header::CACHE_CONTROL, "public, max-age=31536000, immutable")], body)
}

async fn glyphs(Path((_, range)): Path<(String, String)>) -> impl IntoResponse {
    const FONTS: [(&str, &[u8]); 7] = [
        ("0-255.pbf", include_bytes!("../../../data/map/fonts/0-255.pbf")),
        ("256-511.pbf", include_bytes!("../../../data/map/fonts/256-511.pbf")),
        ("512-767.pbf", include_bytes!("../../../data/map/fonts/512-767.pbf")),
        ("768-1023.pbf", include_bytes!("../../../data/map/fonts/768-1023.pbf")),
        ("1024-1279.pbf", include_bytes!("../../../data/map/fonts/1024-1279.pbf")),
        ("7680-7935.pbf", include_bytes!("../../../data/map/fonts/7680-7935.pbf")),
        ("8192-8447.pbf", include_bytes!("../../../data/map/fonts/8192-8447.pbf")),
    ];
    kept("application/x-protobuf", FONTS.iter().find(|(name, _)| *name == range).map_or(&[], |(_, glyphs)| glyphs))
}

pub fn routes() -> Router<Arc<Server>> {
    Router::new()
        .route("/", get(|| async { file("text/html; charset=utf-8", include_bytes!("web/index.html")) }))
        .route("/app.js", get(|| async { file("text/javascript; charset=utf-8", include_bytes!("web/app.js")) }))
        .route("/strings.js", get(|| async { file("text/javascript; charset=utf-8", include_bytes!("web/strings.js")) }))
        .route("/scrubber.js", get(|| async { file("text/javascript; charset=utf-8", include_bytes!("web/scrubber.js")) }))
        .route("/activity.js", get(|| async { file("text/javascript; charset=utf-8", include_bytes!("web/activity.js")) }))
        .route("/import.js", get(|| async { file("text/javascript; charset=utf-8", include_bytes!("web/import.js")) }))
        .route("/today.js", get(|| async { file("text/javascript; charset=utf-8", include_bytes!("web/today.js")) }))
        .route("/start.js", get(|| async { file("text/javascript; charset=utf-8", include_bytes!("web/start.js")) }))
        .route("/tidy.js", get(|| async { file("text/javascript; charset=utf-8", include_bytes!("web/tidy.js")) }))
        .route("/manage.js", get(|| async { file("text/javascript; charset=utf-8", include_bytes!("web/manage.js")) }))
        .route("/tokens.css", get(|| async { file("text/css; charset=utf-8", include_bytes!("web/tokens.css")) }))
        .route("/app.css", get(|| async { file("text/css; charset=utf-8", include_bytes!("web/app.css")) }))
        .route("/icon.svg", get(|| async { file("image/svg+xml", include_bytes!("web/icon.svg")) }))
        .route("/icon-180.png", get(|| async { file("image/png", include_bytes!("web/icon-180.png")) }))
        .route("/icon-512.png", get(|| async { file("image/png", include_bytes!("web/icon-512.png")) }))
        .route("/manifest.webmanifest", get(|| async { file("application/manifest+json", include_bytes!("web/manifest.webmanifest")) }))
        .route("/mark.png", get(|| async { file("image/png", include_bytes!("../../../data/branding/mark.png")) }))

        .route("/map.js", get(|| async { file("text/javascript; charset=utf-8", include_bytes!("web/map.js")) }))
        .route("/map/6.13.0/maplibre-gl.mjs", get(|| async { kept("text/javascript; charset=utf-8", include_bytes!("../../../data/map/maplibre-gl.mjs")) }))
        .route("/map/6.13.0/maplibre-gl-worker.mjs", get(|| async { kept("text/javascript; charset=utf-8", include_bytes!("../../../data/map/maplibre-gl-worker.mjs")) }))
        .route("/map/6.13.0/maplibre-gl.css", get(|| async { kept("text/css; charset=utf-8", include_bytes!("../../../data/map/maplibre-gl.css")) }))
        .route("/map/fonts/{stack}/{range}", get(glyphs))

        .route("/licences", get(|| async {
            file(
                "text/plain; charset=utf-8",
                concat!(
                    include_str!("../../../data/THIRD_PARTY_LICENSES-server.txt"), "\n\n",
                    include_str!("../../../data/places/NOTICE.txt"), "\n\n",
                    include_str!("../../../data/design/LUCIDE-LICENSE.txt"), "\n\n",
                    include_str!("../../../data/map/NOTICE.txt"), "\n\n",
                    "ONNX Runtime 1.28.0 (Microsoft)\n\n",
                    include_str!("../../../data/onnxruntime/LICENSE"), "\n\n",
                    include_str!("../../../data/onnxruntime/ThirdPartyNotices.txt"),
                )
                .as_bytes(),
            )
        }))
}
