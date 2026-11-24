use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use axum::extract::Path;
use axum::http::{header, HeaderMap, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use axum::Router;

use crate::Server;

fn file(asked: &HeaderMap, kind: &'static str, body: &'static [u8]) -> Response {
    let (tag, packed) = kept_packed(body);
    let said = [(header::CONTENT_TYPE, kind.to_string()), (header::CACHE_CONTROL, "no-cache".to_string()), (header::ETAG, tag.clone()), (header::VARY, "accept-encoding".to_string())];
    if asked.get(header::IF_NONE_MATCH).is_some_and(|had| had.as_bytes() == tag.as_bytes()) {
        return (StatusCode::NOT_MODIFIED, said).into_response();
    }

    match takes_gzip(asked) && !kind.starts_with("image/png") {
        true => (said, [(header::CONTENT_ENCODING, "gzip")], packed.to_vec()).into_response(),
        false => (said, body).into_response(),
    }
}

fn kept_packed(body: &'static [u8]) -> (String, Arc<Vec<u8>>) {
    static KEPT: Mutex<Option<HashMap<usize, (String, Arc<Vec<u8>>)>>> = Mutex::new(None);
    let mut kept = KEPT.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    kept.get_or_insert_with(HashMap::new)
        .entry(body.as_ptr() as usize)
        .or_insert_with(|| {
            let hash = body.iter().fold(0xcbf2_9ce4_8422_2325u64, |hash, byte| (hash ^ *byte as u64).wrapping_mul(0x100_0000_01b3));
            (format!("\"{hash:x}-{}\"", body.len()), Arc::new(gzip(body)))
        })
        .clone()
}

pub fn takes_gzip(asked: &HeaderMap) -> bool {
    asked.get_all(header::ACCEPT_ENCODING).iter().filter_map(|value| value.to_str().ok()).flat_map(|value| value.split(',')).any(|coding| {
        let mut parts = coding.split(';').map(str::trim);
        parts.next().is_some_and(|name| name.eq_ignore_ascii_case("gzip")) && !parts.any(|weight| matches!(weight, "q=0" | "q=0.0" | "q=0.00" | "q=0.000"))
    })
}

pub fn gzip(bytes: &[u8]) -> Vec<u8> {
    use std::io::Write;
    let mut packer = flate2::write::GzEncoder::new(Vec::with_capacity(bytes.len() / 4), flate2::Compression::new(5));

    let _ = packer.write_all(bytes);
    packer.finish().unwrap_or_default()
}

fn kept(kind: &'static str, body: &'static [u8]) -> impl IntoResponse {
    ([(header::CONTENT_TYPE, kind), (header::CACHE_CONTROL, "public, max-age=31536000, immutable")], body)
}

pub const FONTS: [(&str, &[u8]); 7] = [
    ("0-255.pbf", include_bytes!("../../../data/map/fonts/0-255.pbf")),
    ("256-511.pbf", include_bytes!("../../../data/map/fonts/256-511.pbf")),
    ("512-767.pbf", include_bytes!("../../../data/map/fonts/512-767.pbf")),
    ("768-1023.pbf", include_bytes!("../../../data/map/fonts/768-1023.pbf")),
    ("1024-1279.pbf", include_bytes!("../../../data/map/fonts/1024-1279.pbf")),
    ("7680-7935.pbf", include_bytes!("../../../data/map/fonts/7680-7935.pbf")),
    ("8192-8447.pbf", include_bytes!("../../../data/map/fonts/8192-8447.pbf")),
];

async fn glyphs(Path((_, range)): Path<(String, String)>) -> impl IntoResponse {
    kept("application/x-protobuf", FONTS.iter().find(|(name, _)| *name == range).map_or(&[], |(_, glyphs)| glyphs))
}

pub fn routes() -> Router<Arc<Server>> {
    Router::new()
        .route("/", get(|asked: HeaderMap| async move { file(&asked, "text/html; charset=utf-8", include_bytes!("web/index.html")) }))

        .route("/robots.txt", get(|asked: HeaderMap| async move { file(&asked, "text/plain; charset=utf-8", b"User-agent: *\nDisallow: /\n") }))
        .route("/app.js", get(|asked: HeaderMap| async move { file(&asked, "text/javascript; charset=utf-8", include_bytes!("web/app.js")) }))
        .route("/strings.js", get(|asked: HeaderMap| async move { file(&asked, "text/javascript; charset=utf-8", include_bytes!("web/strings.js")) }))
        .route("/scrubber.js", get(|asked: HeaderMap| async move { file(&asked, "text/javascript; charset=utf-8", include_bytes!("web/scrubber.js")) }))
        .route("/activity.js", get(|asked: HeaderMap| async move { file(&asked, "text/javascript; charset=utf-8", include_bytes!("web/activity.js")) }))
        .route("/import.js", get(|asked: HeaderMap| async move { file(&asked, "text/javascript; charset=utf-8", include_bytes!("web/import.js")) }))
        .route("/today.js", get(|asked: HeaderMap| async move { file(&asked, "text/javascript; charset=utf-8", include_bytes!("web/today.js")) }))
        .route("/start.js", get(|asked: HeaderMap| async move { file(&asked, "text/javascript; charset=utf-8", include_bytes!("web/start.js")) }))
        .route("/reach.js", get(|asked: HeaderMap| async move { file(&asked, "text/javascript; charset=utf-8", include_bytes!("web/reach.js")) }))
        .route("/tidy.js", get(|asked: HeaderMap| async move { file(&asked, "text/javascript; charset=utf-8", include_bytes!("web/tidy.js")) }))
        .route("/manage.js", get(|asked: HeaderMap| async move { file(&asked, "text/javascript; charset=utf-8", include_bytes!("web/manage.js")) }))
        .route("/tokens.css", get(|asked: HeaderMap| async move { file(&asked, "text/css; charset=utf-8", include_bytes!("web/tokens.css")) }))
        .route("/app.css", get(|asked: HeaderMap| async move { file(&asked, "text/css; charset=utf-8", include_bytes!("web/app.css")) }))
        .route("/icon.svg", get(|asked: HeaderMap| async move { file(&asked, "image/svg+xml", include_bytes!("web/icon.svg")) }))
        .route("/icon-180.png", get(|asked: HeaderMap| async move { file(&asked, "image/png", include_bytes!("web/icon-180.png")) }))
        .route("/icon-512.png", get(|asked: HeaderMap| async move { file(&asked, "image/png", include_bytes!("web/icon-512.png")) }))
        .route("/manifest.webmanifest", get(|asked: HeaderMap| async move { file(&asked, "application/manifest+json", include_bytes!("web/manifest.webmanifest")) }))
        .route("/mark.png", get(|asked: HeaderMap| async move { file(&asked, "image/png", include_bytes!("../../../data/branding/mark.png")) }))

        .route("/mapstyle.js", get(|asked: HeaderMap| async move { file(&asked, "text/javascript; charset=utf-8", include_bytes!("web/mapstyle.js")) }))
        .route("/map.js", get(|asked: HeaderMap| async move { file(&asked, "text/javascript; charset=utf-8", include_bytes!("web/map.js")) }))
        .route("/search.js", get(|asked: HeaderMap| async move { file(&asked, "text/javascript; charset=utf-8", include_bytes!("web/search.js")) }))
        .route("/album.js", get(|asked: HeaderMap| async move { file(&asked, "text/javascript; charset=utf-8", include_bytes!("web/album.js")) }))
        .route("/whats-new.js", get(|asked: HeaderMap| async move { file(&asked, "text/javascript; charset=utf-8", include_bytes!("web/whats-new.js")) }))
        .route("/map/6.13.0/maplibre-gl.mjs", get(|| async { kept("text/javascript; charset=utf-8", include_bytes!("../../../data/map/maplibre-gl.mjs")) }))
        .route("/map/6.13.0/maplibre-gl-worker.mjs", get(|| async { kept("text/javascript; charset=utf-8", include_bytes!("../../../data/map/maplibre-gl-worker.mjs")) }))
        .route("/map/6.13.0/maplibre-gl.css", get(|| async { kept("text/css; charset=utf-8", include_bytes!("../../../data/map/maplibre-gl.css")) }))
        .route("/map/fonts/{stack}/{range}", get(glyphs))

        .route("/licences", get(|asked: HeaderMap| async move {
            file(
                &asked,
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
