use std::io::{Read, SeekFrom};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::{Arc, OnceLock};
use std::time::{Duration, Instant};

use axum::body::Body;
use axum::extract::{Path as Route, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use image::RgbImage;
use numa_io::thumbs;
use numa_io::video::{content_type, is_video};
use tokio::io::{AsyncReadExt, AsyncSeekExt};

use crate::auth::Signed;
use crate::grants::Level;
use crate::{Fail, Server};

const LIMIT: Duration = Duration::from_secs(30);

const CHUNK: u64 = 8 << 20;

pub fn ffmpeg() -> Option<&'static Path> {
    static FOUND: OnceLock<Option<PathBuf>> = OnceLock::new();
    FOUND
        .get_or_init(|| std::env::var_os("PATH").and_then(|paths| std::env::split_paths(&paths).map(|dir| dir.join("ffmpeg")).find(|tool| tool.is_file())))
        .as_deref()
}

pub fn range(asked: Option<&str>, length: u64) -> Result<Option<(u64, u64)>, ()> {
    let Some(asked) = asked else { return Ok(None) };
    let Some(spec) = asked.trim().strip_prefix("bytes=") else { return Ok(None) };
    if spec.contains(',') {
        return Ok(None);
    }
    let Some((from, to)) = spec.split_once('-') else { return Ok(None) };
    let (from, to) = (from.trim(), to.trim());
    let last = length.saturating_sub(1);
    let (first, end) = match (from.is_empty(), to.is_empty()) {
        (true, true) => return Ok(None),

        (true, false) => {
            let n: u64 = to.parse().map_err(|_| ())?;
            if n == 0 {
                return Err(());
            }
            (length.saturating_sub(n), last)
        }
        (false, true) => (from.parse().map_err(|_| ())?, last),
        (false, false) => (from.parse().map_err(|_| ())?, to.parse::<u64>().map_err(|_| ())?.min(last)),
    };
    match first <= end && first < length {
        true => Ok(Some((first, end))),
        false => Err(()),
    }
}

fn etag(meta: &std::fs::Metadata) -> String {
    let nanos = meta.modified().ok().and_then(|at| at.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |since| since.as_nanos());
    format!("\"{:x}-{nanos:x}\"", meta.len())
}

fn range_still_holds(if_range: Option<&str>, tag: &str) -> bool {
    if_range.map_or(true, |asked| asked.trim() == tag)
}

async fn stream(path: &Path, headers: &HeaderMap) -> Result<Response, Fail> {
    let mut file = tokio::fs::File::open(path).await.map_err(|_| Fail(StatusCode::NOT_FOUND, "The file is no longer there.".into()))?;
    let length = file.metadata().await.map_err(Fail::internal)?.len();
    let tag = etag(&file.metadata().await.map_err(Fail::internal)?);
    let asked = headers.get(header::RANGE).and_then(|value| value.to_str().ok());
    let asked = asked.filter(|_| range_still_holds(headers.get(header::IF_RANGE).and_then(|value| value.to_str().ok()), &tag));
    let kind = HeaderValue::from_static(content_type(path));
    let range = match range(asked, length) {
        Ok(range) => range,
        Err(()) => {
            let outside = [(header::CONTENT_RANGE, format!("bytes */{length}"))];
            return Ok((StatusCode::RANGE_NOT_SATISFIABLE, outside).into_response());
        }
    };
    let (status, first, count) = match range {
        None => (StatusCode::OK, 0, length),
        Some((first, last)) => (StatusCode::PARTIAL_CONTENT, first, (last - first + 1).min(CHUNK)),
    };
    file.seek(SeekFrom::Start(first)).await.map_err(Fail::internal)?;
    let mut response = Body::from_stream(tokio_util::io::ReaderStream::new(file.take(count))).into_response();
    *response.status_mut() = status;
    let out = response.headers_mut();
    out.insert(header::CONTENT_TYPE, kind);
    if let Ok(tag) = HeaderValue::from_str(&tag) {
        out.insert(header::ETAG, tag);
    }
    out.insert(header::ACCEPT_RANGES, HeaderValue::from_static("bytes"));
    out.insert(header::CONTENT_LENGTH, HeaderValue::from(count));
    out.insert(header::CACHE_CONTROL, HeaderValue::from_static("private, no-cache"));
    if status == StatusCode::PARTIAL_CONTENT {
        let value = format!("bytes {first}-{}/{length}", first + count - 1);
        out.insert(header::CONTENT_RANGE, HeaderValue::from_str(&value).map_err(Fail::internal)?);
    }
    Ok(response)
}

pub async fn play(State(server): State<Arc<Server>>, Signed(me): Signed, Route(id): Route<i64>, headers: HeaderMap) -> Result<Response, Fail> {
    let photo = server.home.photo(&me, id, Level::View)?;
    if !is_video(&photo.path) {
        return Err(Fail(StatusCode::NOT_FOUND, "That is not a video.".into()));
    }
    stream(&photo.path, &headers).await
}

pub async fn live(State(server): State<Arc<Server>>, Signed(me): Signed, Route(id): Route<i64>, headers: HeaderMap) -> Result<Response, Fail> {
    server.home.photo(&me, id, Level::View)?;
    let hold = server.home.hold_of(id).and_then(|hold| server.home.any_photo(hold));
    match hold {
        Some(hold) => stream(&hold.path, &headers).await,
        None => Err(Fail(StatusCode::NOT_FOUND, "This photograph does not move.".into())),
    }
}

pub fn poster(path: &Path, mtime: i64, edge: u32) -> Result<RgbImage, String> {
    if let Some(cached) = thumbs::cached(path, mtime, edge, None).and_then(|file| image::open(file).ok()) {
        return Ok(cached.into_rgb8());
    }
    let tool = ffmpeg().ok_or("There is no poster frame: ffmpeg is not installed")?;
    let key = (path.to_path_buf(), mtime, edge);
    if failed_lately(&key) {
        return Err("No poster frame: ffmpeg failed on this file a short while ago".into());
    }
    let image = frame(tool, path, edge).inspect_err(|_| failed(key))?;
    thumbs::store(path, mtime, edge, None, &image);
    Ok(image)
}

type Attempt = (PathBuf, i64, u32);

const RETRY: Duration = Duration::from_secs(600);

fn failures() -> std::sync::MutexGuard<'static, std::collections::HashMap<Attempt, Instant>> {
    static KEPT: OnceLock<std::sync::Mutex<std::collections::HashMap<Attempt, Instant>>> = OnceLock::new();
    KEPT.get_or_init(Default::default).lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn failed_lately(key: &Attempt) -> bool {
    failures().get(key).is_some_and(|at| at.elapsed() < RETRY)
}

fn failed(key: Attempt) {
    let mut kept = failures();

    if kept.len() >= 4096 {
        kept.retain(|_, at| at.elapsed() < RETRY);
    }
    if kept.len() < 4096 {
        kept.insert(key, Instant::now());
    }
}

pub fn placeholder(path: &Path, edge: u32) -> RgbImage {
    let (width, height) = numa_io::video::probe(path).and_then(|film| film.size).filter(|(w, h)| *w > 0 && *h > 0).unwrap_or((16, 9));
    let scale = edge as f32 / width.max(height) as f32;
    let (width, height) = (((width as f32 * scale).round() as u32).max(2), ((height as f32 * scale).round() as u32).max(2));
    let (cx, cy, r) = (width as f32 / 2.0, height as f32 / 2.0, width.min(height) as f32 * 0.1);
    RgbImage::from_fn(width, height, |x, y| {
        let (dx, dy) = (x as f32 + 0.5 - cx, y as f32 + 0.5 - cy);

        let inside = dx >= -r / 2.0 && dx <= r && dy.abs() <= (r - dx) * (2.0 / 3.0);
        match inside {
            true => image::Rgb([235, 235, 237]),
            false => image::Rgb([44, 44, 48]),
        }
    })
}

fn demuxer(path: &Path) -> &'static str {
    match path.extension().map(|ext| ext.to_string_lossy().to_lowercase()).as_deref() {
        Some("webm" | "mkv") => "matroska",
        Some("avi") => "avi",
        Some("mts") => "mpegts",
        _ => "mov",
    }
}

fn frame(tool: &Path, path: &Path, edge: u32) -> Result<RgbImage, String> {
    let fit = format!("scale='if(gt(iw,ih),min({edge},iw),-2)':'if(gt(iw,ih),-2,min({edge},ih))'");
    for second in ["1", "0"] {
        let mut child = Command::new(tool)
            .args(["-nostdin", "-loglevel", "error", "-protocol_whitelist", "file", "-threads", "1", "-f", demuxer(path), "-ss", second, "-i"])
            .arg(path)
            .args(["-frames:v", "1", "-fs", "8388608", "-vf", &fit, "-f", "image2pipe", "-c:v", "mjpeg", "-q:v", "3", "-"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .map_err(|err| err.to_string())?;
        let mut out = child.stdout.take().ok_or("no output")?;
        let reader = std::thread::spawn(move || {
            let mut bytes = Vec::new();
            let _ = out.read_to_end(&mut bytes);
            bytes
        });
        let started = Instant::now();
        while child.try_wait().map_err(|err| err.to_string())?.is_none() {
            if started.elapsed() > LIMIT {
                let _ = child.kill();
                let _ = child.wait();
                return Err("ffmpeg took too long".into());
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let bytes = reader.join().unwrap_or_default();
        if let Ok(image) = image::load_from_memory(&bytes) {
            return Ok(image.into_rgb8());
        }
    }
    Err("ffmpeg made no frame of it".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_range_is_read_the_way_browsers_ask() {
        assert_eq!(range(None, 100), Ok(None));
        assert_eq!(range(Some("bytes=0-"), 100), Ok(Some((0, 99))));
        assert_eq!(range(Some("bytes=10-19"), 100), Ok(Some((10, 19))));
        assert_eq!(range(Some("bytes=90-500"), 100), Ok(Some((90, 99))));
        assert_eq!(range(Some("bytes=-10"), 100), Ok(Some((90, 99))));
        assert_eq!(range(Some("bytes=-500"), 100), Ok(Some((0, 99))));

        assert_eq!(range(Some("bytes=100-"), 100), Err(()));
        assert_eq!(range(Some("bytes=20-10"), 100), Err(()));
        assert_eq!(range(Some("bytes=-0"), 100), Err(()));
        assert_eq!(range(Some("bytes=a-b"), 100), Err(()));
        assert_eq!(range(Some("bytes=0-"), 0), Err(()));

        assert_eq!(range(Some("items=0-5"), 100), Ok(None));
        assert_eq!(range(Some("bytes=0-5,10-15"), 100), Ok(None));
        assert_eq!(range(Some("bytes=-"), 100), Ok(None));
    }

    #[test]
    fn if_range_only_keeps_a_range_for_the_same_file() {
        assert!(range_still_holds(None, "\"a\""));
        assert!(range_still_holds(Some("\"a\""), "\"a\""));
        assert!(!range_still_holds(Some("\"b\""), "\"a\""));
        assert!(!range_still_holds(Some("Wed, 21 Oct 2015 07:28:00 GMT"), "\"a\""));
    }

    #[test]
    fn a_failed_poster_is_not_tried_again_at_once() {
        let key = (PathBuf::from("/nowhere/broken.mp4"), 7, 320);
        assert!(!failed_lately(&key));
        failed(key.clone());
        assert!(failed_lately(&key));
        assert!(!failed_lately(&(key.0.clone(), 8, 320)), "a changed file is tried");
    }

    #[test]
    fn the_demuxer_follows_the_extension() {
        assert_eq!(demuxer(Path::new("a.MKV")), "matroska");
        assert_eq!(demuxer(Path::new("a.mp4")), "mov");
        assert_eq!(demuxer(Path::new("a.mts")), "mpegts");
    }

    #[test]
    fn the_placeholder_keeps_the_longest_side_and_draws_a_triangle() {
        let tile = placeholder(Path::new("/nowhere/none.mp4"), 320);
        assert_eq!(tile.dimensions(), (320, 180));
        assert_eq!(tile.get_pixel(160, 90).0, [235, 235, 237]);
        assert_eq!(tile.get_pixel(2, 2).0, [44, 44, 48]);
    }
}
