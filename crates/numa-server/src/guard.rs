use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

use axum::extract::{ConnectInfo, Request, State};
use axum::http::{header, HeaderMap, HeaderName, HeaderValue, Method, StatusCode};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use crate::{auth, Fail, Server};

const POLICY: &str = "default-src 'self'; img-src 'self' blob: data:; object-src 'none'; base-uri 'none'; form-action 'self'; frame-ancestors 'none'";

pub async fn guarded(State(server): State<Arc<Server>>, request: Request, next: Next) -> Response {
    let peer = request.extensions().get::<ConnectInfo<SocketAddr>>().map(|info| info.0);
    let https = peer.is_some_and(|peer| auth::client(peer, request.headers()).1);
    let refused = match changes(request.method()) && !same_origin(request.headers()) {
        true => Some(Fail(StatusCode::FORBIDDEN, "That request came from another site.".into()).into_response()),
        false => paced(&server, &request),
    };
    let mut response = match refused {
        Some(refused) => refused,
        None => next.run(request).await,
    };
    tell(response.headers_mut(), https);
    response
}

fn tell(headers: &mut HeaderMap, https: bool) {
    headers.insert(header::CONTENT_SECURITY_POLICY, HeaderValue::from_static(POLICY));
    headers.insert(header::X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    headers.insert(header::REFERRER_POLICY, HeaderValue::from_static("same-origin"));

    headers.entry(HeaderName::from_static("x-robots-tag")).or_insert(HeaderValue::from_static("noindex, noai, noimageai"));
    headers.insert("cross-origin-resource-policy", HeaderValue::from_static("same-origin"));

    if https {
        headers.insert(header::STRICT_TRANSPORT_SECURITY, HeaderValue::from_static("max-age=31536000"));
    }
}

fn changes(method: &Method) -> bool {
    !matches!(*method, Method::GET | Method::HEAD | Method::OPTIONS)
}

fn same_origin(headers: &HeaderMap) -> bool {
    let Some(origin) = headers.get(header::ORIGIN).and_then(|value| value.to_str().ok()) else { return true };
    let Some((_, asked)) = origin.split_once("://") else { return false };
    let here = |name: &str| headers.get(name).and_then(|value| value.to_str().ok()).is_some_and(|host| host.eq_ignore_ascii_case(asked));
    here("host") || here("x-forwarded-host")
}

#[derive(Default)]
pub struct Pace(Mutex<HashMap<String, (f64, f64)>>);

const BUCKET: f64 = 3000.0;
const PER_SECOND: f64 = 100.0;

impl Pace {

    fn take(&self, key: &str, cost: f64, now: f64) -> Result<(), u64> {
        let mut buckets = self.0.lock().unwrap_or_else(|poisoned| poisoned.into_inner());

        if buckets.len() > 10_000 {
            buckets.retain(|_, (left, at)| *left + (now - *at) * PER_SECOND < BUCKET);
        }
        let (left, at) = buckets.entry(key.to_string()).or_insert((BUCKET, now));
        *left = (*left + (now - *at) * PER_SECOND).min(BUCKET);
        *at = now;
        if *left <= 0.0 {
            return Err((-*left / PER_SECOND).ceil().max(1.0) as u64);
        }
        *left -= cost;
        Ok(())
    }
}

fn cost(path: &str, query: Option<&str>) -> Option<f64> {
    if path == "/api/v1/photos/zip" {
        let ids = query.and_then(|query| query.split('&').find_map(|pair| pair.strip_prefix("ids="))).unwrap_or("");
        return Some(25.0 * ids.split([',', '%']).filter(|id| !id.is_empty()).count().max(1) as f64);
    }
    let rest = path.strip_prefix("/api/v1/photos/")?;
    match rest.rsplit_once('/')?.1 {
        "thumb" => Some(1.0),
        "preview" | "jpeg" => Some(4.0),
        "original" => Some(25.0),
        _ => None,
    }
}

fn paced(server: &Server, request: &Request) -> Option<Response> {
    let cost = cost(request.uri().path(), request.uri().query())?;
    let session = auth::digest(&auth::presented(request.headers())?);
    static START: OnceLock<Instant> = OnceLock::new();
    let now = START.get_or_init(Instant::now).elapsed().as_secs_f64();
    let wait = server.pace.take(&session, cost, now).err()?;
    let mut refused = Fail(StatusCode::TOO_MANY_REQUESTS, format!("Too many photographs asked for at once. {}", auth::try_again(wait as i64))).into_response();
    refused.headers_mut().insert(header::RETRY_AFTER, HeaderValue::from(wait));
    Some(refused)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_pictures_keep_a_pace() {
        let pace = Pace::default();
        for _ in 0..3000 {
            assert!(pace.take("a", 1.0, 0.0).is_ok());
        }
        assert!(pace.take("a", 1.0, 0.0).is_err(), "the bucket is empty");
        assert!(pace.take("b", 1.0, 0.0).is_ok(), "another session has its own");
        assert!(pace.take("a", 1.0, 0.5).is_ok(), "half a second fills 50");
        assert!(pace.take("c", 25.0 * 500.0, 0.0).is_ok(), "a big zip goes once");
        assert_eq!(pace.take("c", 1.0, 1.0), Err(94), "and waits till the debt is paid");
        assert_eq!(cost("/api/v1/photos/12/thumb", Some("edge=512")), Some(1.0));
        assert_eq!(cost("/api/v1/photos/12/original", None), Some(25.0));
        assert_eq!(cost("/api/v1/photos/12/jpeg", Some("edge=2048")), Some(4.0));
        assert_eq!(cost("/api/v1/photos/zip", Some("ids=1,2,3&as=jpeg")), Some(75.0));
        assert_eq!(cost("/api/v1/photos/12", None), None);
        assert_eq!(cost("/api/v1/timeline", None), None);
    }

    #[test]
    fn what_every_answer_says() {
        let mut headers = HeaderMap::new();
        tell(&mut headers, false);
        assert!(!headers.contains_key(header::STRICT_TRANSPORT_SECURITY), "plain http must not pin https");
        assert_eq!(headers["x-robots-tag"], "noindex, noai, noimageai");
        assert_eq!(headers["cross-origin-resource-policy"], "same-origin");
        tell(&mut headers, true);
        assert_eq!(headers[header::STRICT_TRANSPORT_SECURITY], "max-age=31536000");
        let mut headers = HeaderMap::new();
        headers.insert("x-robots-tag", HeaderValue::from_static("noindex, noimageindex"));
        tell(&mut headers, false);
        assert_eq!(headers["x-robots-tag"], "noindex, noimageindex");
    }

    #[test]
    fn origin_must_be_here() {
        let headers = |pairs: &[(&'static str, &str)]| {
            let mut map = HeaderMap::new();
            pairs.iter().for_each(|(name, value)| drop(map.insert(*name, value.parse().unwrap())));
            map
        };
        assert!(same_origin(&headers(&[("host", "nas.local:8283"), ("origin", "http://nas.local:8283")])));
        assert!(same_origin(&headers(&[("host", "nas.local:8283")])), "an app sends none");
        assert!(!same_origin(&headers(&[("host", "nas.local:8283"), ("origin", "http://nas.local:5000")])), "another service on the NAS");
        assert!(!same_origin(&headers(&[("host", "nas.local:8283"), ("origin", "null")])));
        assert!(same_origin(&headers(&[("host", "127.0.0.1:8283"), ("x-forwarded-host", "photos.example.org"), ("origin", "https://photos.example.org")])));
        assert!(changes(&Method::POST) && changes(&Method::DELETE) && !changes(&Method::GET));
    }
}
