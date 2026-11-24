use std::io::Read;

use serde_json::Value;

use crate::grants::Level;
use crate::share::tests::http::{account, ask, fixture};

const GZIP: &str = "Accept-Encoding: gzip, br\r\n";

fn unpacked(bytes: &[u8]) -> Vec<u8> {
    let mut plain = Vec::new();
    flate2::read::GzDecoder::new(bytes).read_to_end(&mut plain).unwrap();
    plain
}

fn row(who: &str, id: i64) -> Value {
    let months = ask("GET", "/api/v1/timeline", Some(who), "", "").json();
    let months: Vec<String> = months.as_array().unwrap().iter().map(|month| month["month"].as_str().unwrap().to_string()).collect();
    let rows = months.iter().flat_map(|month| ask("GET", &format!("/api/v1/timeline/{month}"), Some(who), "", "").json().as_array().unwrap().clone());
    rows.into_iter().find(|row| row["id"] == id).expect("the photograph is on the timeline")
}

#[test]
fn a_kept_list_follows_a_mark() {
    let fixture = fixture();
    let who = account("Speed Marker", Level::Edit);
    let id = fixture.ids[1];
    assert_eq!(row(&who, id)["rating"], 0);
    assert_eq!(row(&who, id)["rating"], 0, "asked again, from what was kept");
    let marked = ask("PUT", &format!("/api/v1/photos/{id}/marks"), Some(&who), r#"{"rating":4,"flag":"pick"}"#, "");
    assert_eq!(marked.status, 200, "{}", String::from_utf8_lossy(&marked.body));
    let after = row(&who, id);
    assert_eq!((after["rating"].as_i64(), after["flag"].as_str()), (Some(4), Some("pick")));

    assert_eq!(row(&fixture.sam, id)["rating"], 4);
    assert_eq!(ask("PUT", &format!("/api/v1/photos/{id}/marks"), Some(&who), r#"{"rating":0,"flag":"none"}"#, "").status, 200);
    assert_eq!(row(&who, id)["rating"], 0);
}

#[test]
fn a_kept_session_ends_at_once() {
    let fixture = fixture();
    let who = account("Speed Leaver", Level::View);
    let thumb = format!("/api/v1/photos/{}/thumb", fixture.ids[0]);
    assert_eq!(ask("GET", &thumb, Some(&who), "", "").status, 200);
    assert_eq!(ask("GET", &thumb, Some(&who), "", "").status, 200);
    assert_eq!(fixture.server.store.end_sessions("Speed Leaver", None), 1);
    assert_eq!(ask("GET", &thumb, Some(&who), "", "").status, 401);
}

#[test]
fn the_pages_come_packed_and_once() {
    let whole = ask("GET", "/app.js", None, "", "");
    assert_eq!((whole.status, whole.header("content-encoding")), (200, None));
    assert_eq!(whole.body, include_bytes!("web/app.js"));
    assert!(whole.header("server-timing").is_some_and(|said| said.starts_with("total;dur=")));
    let packed = ask("GET", "/app.js", None, "", GZIP);
    assert_eq!(packed.header("content-encoding"), Some("gzip"));
    assert!(packed.body.len() * 2 < whole.body.len(), "{} of {}", packed.body.len(), whole.body.len());
    assert_eq!(unpacked(&packed.body), whole.body);
    let tag = packed.header("etag").unwrap().to_string();
    assert_eq!(whole.header("etag"), Some(tag.as_str()));
    let again = ask("GET", "/app.js", None, "", &format!("{GZIP}If-None-Match: {tag}\r\n"));
    assert_eq!((again.status, again.body.len()), (304, 0));

    assert_eq!(ask("GET", "/icon-180.png", None, "", GZIP).header("content-encoding"), None);
}

#[test]
fn a_long_list_comes_packed() {
    let fixture = fixture();
    let short = ask("GET", "/api/v1/photos/0", Some(&fixture.sam), "", GZIP);
    assert_eq!((short.status, short.header("content-encoding")), (404, None));
    let months = ask("GET", "/api/v1/timeline", Some(&fixture.sam), "", "").json();
    let month = months[0]["month"].as_str().unwrap();
    let plain = ask("GET", &format!("/api/v1/timeline/{month}"), Some(&fixture.sam), "", "");
    let packed = ask("GET", &format!("/api/v1/timeline/{month}"), Some(&fixture.sam), "", GZIP);
    match plain.body.len() >= 1024 {
        true => {
            assert_eq!(packed.header("content-encoding"), Some("gzip"));
            assert_eq!(serde_json::from_slice::<Value>(&unpacked(&packed.body)).unwrap(), plain.json());
        }
        false => assert_eq!(packed.body, plain.body),
    }
}
