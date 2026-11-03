use serde_json::{json, Value};

use crate::grants::{Grants, Level};
use crate::share::tests::http::{account, ask, fixture, get, raw};

fn jpeg(seed: u8) -> Vec<u8> {
    let picture = image::RgbImage::from_fn(96, 64, |x, y| image::Rgb([(x as u8).wrapping_mul(seed), (y as u8).wrapping_add(seed), seed]));
    let mut bytes = Vec::new();
    picture.write_to(&mut std::io::Cursor::new(&mut bytes), image::ImageFormat::Jpeg).unwrap();
    bytes
}

fn send_home(who: &str, name: &str, seed: u8) -> i64 {
    let sent = raw("PUT", &format!("/api/v1/send/{name}"), Some(who), &jpeg(seed));
    assert!(sent.status == 201 || sent.status == 200, "{}", String::from_utf8_lossy(&sent.body));
    sent.json()["id"].as_i64().unwrap()
}

fn status(path: &str, who: &str) -> u16 {
    ask("GET", path, Some(who), "", "").status
}

fn albums(who: &str) -> Vec<Value> {
    ask("GET", "/api/v1/albums", Some(who), "", "").json().as_array().unwrap().clone()
}

fn into_album(who: &str, body: Value) -> crate::share::tests::http::Answer {
    ask("POST", "/api/v1/photos/album", Some(who), &body.to_string(), "")
}

#[test]
fn people_in_an_album_see_it_and_nothing_else() {
    let (olive, mia, zed) = (account("Olive", Level::None), account("Mia", Level::None), account("Zed", Level::None));
    let (party, private) = (send_home(&olive, "party.jpg", 11), send_home(&olive, "private.jpg", 12));
    let made = into_album(&olive, json!({ "ids": [party], "name": "Party", "inside": true }));
    assert_eq!(made.status, 200, "{}", String::from_utf8_lossy(&made.body));
    let key = made.json()["album"].as_str().unwrap().to_string();
    assert_eq!(fixture().server.store.album_owner(&key).as_deref(), Some("Olive"), "whoever makes it owns it");

    assert!(albums(&mia).is_empty(), "not shared yet");
    assert_eq!(status(&format!("/api/v1/photos/{party}/thumb"), &mia), 404);

    let people = format!("/api/v1/albums/{key}/people");
    let share = |who: &str, mia: &str| ask("PUT", &people, Some(who), &json!({ "people": { "Mia": mia } }).to_string(), "").status;
    assert_eq!(share(&mia, "add"), 404, "a stranger to the album does not share it");
    assert_eq!(share(&olive, "view"), 200);

    let seen = albums(&mia);
    assert_eq!((seen.len(), seen[0]["count"].as_i64(), seen[0]["owner"].as_str(), seen[0]["add"].as_bool()), (1, Some(1), Some("Olive"), Some(false)));
    assert_eq!(status(&format!("/api/v1/photos/{party}/thumb"), &mia), 200, "in the album");
    assert_eq!(status(&format!("/api/v1/photos/{party}/original"), &mia), 404, "looking is not the original");
    assert_eq!(status(&format!("/api/v1/photos/{private}/thumb"), &mia), 404, "Sent Home stays hers, but for what is in the album");
    assert!(albums(&zed).is_empty() && status(&format!("/api/v1/photos/{party}/thumb"), &zed) == 404, "nor anyone else");

    let theirs = send_home(&mia, "mine.jpg", 13);
    let other = send_home(&mia, "other.jpg", 14);
    assert_eq!(into_album(&mia, json!({ "ids": [theirs], "album": key, "inside": true })).status, 403);
    assert_eq!(share(&olive, "add"), 200);
    assert_eq!(into_album(&mia, json!({ "ids": [theirs], "album": key, "inside": true })).status, 200);
    assert_eq!(into_album(&mia, json!({ "ids": [party], "album": key, "inside": false })).status, 404, "not hers to take out");
    assert_eq!(status(&format!("/api/v1/photos/{theirs}/thumb"), &olive), 200, "the owner sees what was added");
    assert_eq!(status(&format!("/api/v1/photos/{other}/thumb"), &olive), 404, "and nothing else of Mia's");
    assert_eq!(status(&format!("/api/v1/photos/{theirs}/original"), &olive), 404, "not her original to change either");
    assert_eq!(status(&format!("/api/v1/photos/{party}/edit"), &mia), 404);
    assert_eq!(ask("PUT", &format!("/api/v1/albums/{key}"), Some(&mia), r#"{"name":"Mine"}"#, "").status, 403, "only the owner renames");
    assert_eq!(ask("DELETE", &format!("/api/v1/albums/{key}"), Some(&mia), "", "").status, 403);
    assert_eq!(ask("PUT", &format!("/api/v1/albums/{key}"), Some(&olive), r#"{"name":"Party 2026"}"#, "").status, 200, "the owner renames, though not every photograph is hers");
    assert_eq!(ask("GET", &people, Some(&mia), "", "").status, 404);
    assert_eq!(ask("GET", &people, None, "", "").status, 401);

    assert_eq!(ask("PUT", &people, Some(&olive), r#"{"people":{}}"#, "").status, 200);
    assert_eq!(status(&format!("/api/v1/photos/{party}/thumb"), &mia), 404);
    assert_eq!(share(&olive, "everything"), 400, "only look or add");
    assert_eq!(ask("PUT", &people, Some(&olive), r#"{"people":{"Nobody At All":"view"}}"#, "").status, 400);
}

#[test]
fn a_plain_album_grant_still_leaves_sent_home_alone() {
    let (nell, guest) = (account("Nell", Level::None), account("Grant Guest", Level::None));
    let photo = send_home(&nell, "nell.jpg", 21);
    let key = into_album(&nell, json!({ "ids": [photo], "name": "Nell's", "inside": true })).json()["album"].as_str().unwrap().to_string();
    let server = &fixture().server;
    server.store.set_grants("Grant Guest", &Grants { access: Level::None, albums: [(key.clone(), Level::View)].into(), ..Grants::default() }).unwrap();
    assert_eq!(status(&format!("/api/v1/photos/{photo}/thumb"), &guest), 404);
}

fn collect(who: &str, body: Value) -> (i64, String) {
    let made = ask("POST", "/api/v1/collects", Some(who), &body.to_string(), "");
    assert_eq!(made.status, 200, "{}", String::from_utf8_lossy(&made.body));
    let made = made.json();
    (made["id"].as_i64().unwrap(), made["link"].as_str().unwrap().strip_prefix("/c/").unwrap().to_string())
}

fn put(token: &str, name: &str, bytes: &[u8]) -> u16 {
    raw("PUT", &format!("/api/v1/c/{token}/files/{name}?by=Mo"), None, bytes).status
}

#[test]
fn a_guest_sends_within_the_links_limits() {
    let fixture = fixture();
    let (_, token) = collect(&fixture.sam, json!({ "title": "Limits", "by": "Anna and Tom", "count": 2, "days": 1 }));
    assert_eq!(get(&format!("/c/{token}")).status, 200, "the page");
    let said = get(&format!("/api/v1/c/{token}"));
    assert_eq!((said.json()["title"].as_str(), said.json()["by"].as_str(), said.json()["left"].as_i64()), (Some("Limits"), Some("Anna and Tom"), Some(2)));
    let plain = collect(&fixture.sam, json!({ "title": "Plain" })).1;
    assert_eq!(get(&format!("/api/v1/c/{plain}")).json()["by"], "Sam", "the account's name when none is said");
    assert!(!get(&format!("/api/v1/c/{plain}")).json().to_string().contains("owner"), "no field of the owner's");
    assert_eq!(said.header("x-robots-tag").is_some_and(|tag| tag.contains("noindex")), true);

    assert_eq!(put(&token, "one.jpg", &jpeg(31)), 201);
    assert_eq!(put(&token, "one-again.jpg", &jpeg(31)), 200, "the same bytes are kept once");
    assert_eq!(put(&token, "page.jpg", b"<html><script>alert(1)</script></html>"), 415, "not what its name says");
    assert_eq!(put(&token, "tool.exe", &jpeg(32)), 415, "not a kind of photograph");
    assert_eq!(put(&token, "empty.jpg", b""), 415);
    assert_eq!(put(&token, "..%2F..%2Fetc.jpg", &jpeg(33)), 201, "a path in a name is only its last part");
    assert_eq!(put(&token, "three.jpg", &jpeg(34)), 413, "past the count");
    assert_eq!(get(&format!("/api/v1/c/{token}")).json()["left"], 0);

    let (id, small) = collect(&fixture.sam, json!({ "title": "Small", "count": 1 }));
    fixture.server.store.with(|conn| conn.execute("UPDATE shares SET max_bytes = 100 WHERE id = ?1", [id])).unwrap();
    assert_eq!(put(&small, "big.jpg", &jpeg(35)), 413, "more than the link's bytes");
}

#[test]
fn a_collecting_link_opens_nothing_but_itself() {
    let fixture = fixture();
    let (id, token) = collect(&fixture.sam, json!({ "title": "Closed" }));
    assert_eq!(put(&token, "a.jpg", &jpeg(41)), 201);
    for path in ["/api/v1/collects".to_string(), format!("/api/v1/collects/{id}/files"), format!("/api/v1/collects/{id}/files/1/thumb"), "/api/v1/albums".into(), "/api/v1/timeline".into(), "/api/v1/shares".into()] {
        assert_eq!(ask("GET", &path, Some(&token), "", "").status, 401, "{path}, the link as a bearer");
        assert_eq!(get(&path).status, 401, "{path}, nobody");
    }
    for path in [format!("/api/v1/collects/{id}/accept"), format!("/api/v1/collects/{id}/reject")] {
        assert_eq!(ask("POST", &path, None, r#"{"files":[1]}"#, "").status, 401, "{path}");
    }
    assert_eq!(get(&format!("/api/v1/s/{token}")).status, 404, "a collecting link is no gallery");
    let (_, gallery) = crate::share::tests::http::gallery_by(&fixture.sam, json!({ "ids": &fixture.ids[..1] }));
    assert_eq!(get(&format!("/api/v1/c/{gallery}")).status, 404, "nor a gallery's link a collection");
    assert_eq!(put(&gallery, "a.jpg", &jpeg(42)), 404);
    assert_eq!(put("0000", "a.jpg", &jpeg(42)), 404);

    let stranger = account("Collect Stranger", Level::Edit);
    assert_eq!(ask("POST", &format!("/api/v1/shares/{id}/revoke"), Some(&stranger), "", "").status, 404, "not theirs to revoke");
    assert_eq!(ask("GET", &format!("/api/v1/collects/{id}/files"), Some(&stranger), "", "").status, 404);
    assert_eq!(ask("POST", &format!("/api/v1/collects/{id}/accept"), Some(&stranger), "{}", "").status, 404);
    assert_eq!(ask("POST", &format!("/api/v1/shares/{id}/revoke"), Some(&fixture.sam), "", "").status, 204);
    assert_eq!(get(&format!("/api/v1/c/{token}")).status, 404);
    assert_eq!(put(&token, "b.jpg", &jpeg(43)), 404);

    let (ended, later) = collect(&fixture.sam, json!({ "title": "Ended" }));
    fixture.server.store.with(|conn| conn.execute("UPDATE shares SET expires = ?2 WHERE id = ?1", [ended, 1])).unwrap();
    assert_eq!(put(&later, "b.jpg", &jpeg(44)), 404, "an ended link");
}

#[test]
fn accepting_and_rejecting() {
    let fixture = fixture();
    let owner = account("Collector", Level::None);
    let wanted = send_home(&owner, "wanted.jpg", 50);
    let key = into_album(&owner, json!({ "ids": [wanted], "name": "Barbecue", "inside": true })).json()["album"].as_str().unwrap().to_string();
    let (id, token) = collect(&owner, json!({ "title": "Anna & Tom's barbecue", "album": key }));
    assert_eq!((put(&token, "a.jpg", &jpeg(51)), put(&token, "b.jpg", &jpeg(52)), put(&token, "c.jpg", &jpeg(53))), (201, 201, 201));

    let before = fixture.server.home.sent_folder("Collector");
    assert!(!before.join("Guests").exists(), "nothing is home until accepted");
    let waiting = ask("GET", &format!("/api/v1/collects/{id}/files"), Some(&owner), "", "").json();
    let waiting = waiting.as_array().unwrap();
    assert_eq!((waiting.len(), waiting[0]["guest"].as_str()), (3, Some("Mo")));
    let ids: Vec<i64> = waiting.iter().map(|file| file["id"].as_i64().unwrap()).collect();
    let thumb = ask("GET", &format!("/api/v1/collects/{id}/files/{}/thumb", ids[0]), Some(&owner), "", "");
    assert!(thumb.status == 200 && thumb.body.starts_with(&[0xff, 0xd8]), "a small picture for the owner");
    let summary = ask("GET", "/api/v1/collects", Some(&owner), "", "").json();
    assert_eq!((summary[0]["waiting"].as_i64(), summary[0]["accepted"].as_i64()), (Some(3), Some(0)));

    let rejected = ask("POST", &format!("/api/v1/collects/{id}/reject"), Some(&owner), &json!({ "files": [ids[2]] }).to_string(), "").json();
    assert_eq!(rejected["rejected"], 1);
    assert_eq!(ask("POST", &format!("/api/v1/collects/{id}/reject"), Some(&owner), "{}", "").status, 400, "reject names its photographs");
    let accepted = ask("POST", &format!("/api/v1/collects/{id}/accept"), Some(&owner), "{}", "").json();
    assert_eq!((accepted["accepted"].as_i64(), accepted["duplicates"].as_i64()), (Some(2), Some(0)));
    let folder = before.join("Guests").join("Anna _ Tom_s barbecue");
    assert!(folder.join("a.jpg").exists() && folder.join("b.jpg").exists() && !folder.join("c.jpg").exists());
    let staged = before.join(".numa/guests").join(id.to_string());
    assert_eq!(std::fs::read_dir(&staged).map_or(0, Iterator::count), 0, "nothing left waiting");
    let in_album = albums(&owner);
    assert_eq!(in_album[0]["count"], 3, "the two accepted joined the one that was there");
    assert_eq!(ask("POST", &format!("/api/v1/collects/{id}/accept"), Some(&owner), "{}", "").json()["accepted"], 0, "once only");
    let summary = ask("GET", "/api/v1/collects", Some(&owner), "", "").json();
    assert_eq!((summary[0]["waiting"].as_i64(), summary[0]["accepted"].as_i64()), (Some(0), Some(2)));
}

#[test]
fn an_album_without_an_owner_is_claimed_by_sharing() {
    let (alex, kit, jan) = (account("Alex", Level::Edit), account("Kit", Level::None), account("Jan", Level::None));
    let photo = send_home(&alex, "alex.jpg", 61);
    let key = into_album(&alex, json!({ "ids": [photo], "name": "Old" , "inside": true })).json()["album"].as_str().unwrap().to_string();
    fixture().server.store.forget_album(&key);
    let people = format!("/api/v1/albums/{key}/people");
    assert_eq!(ask("GET", &people, Some(&kit), "", "").status, 404, "not household");
    assert_eq!(ask("GET", &people, Some(&alex), "", "").status, 200);
    assert_eq!(ask("PUT", &people, Some(&kit), r#"{"people":{"Jan":"view"}}"#, "").status, 404);
    assert_eq!(ask("PUT", &people, Some(&alex), r#"{"people":{"Jan":"view"}}"#, "").status, 200);
    assert_eq!(fixture().server.store.album_owner(&key).as_deref(), Some("Alex"));
    assert_eq!(status(&format!("/api/v1/photos/{photo}/thumb"), &jan), 200, "Alex's own Sent Home photograph, in the album she now owns");
}

#[test]
fn a_new_link_for_a_collection_collects() {
    let fixture = fixture();
    let (id, old) = collect(&fixture.sam, json!({ "title": "Again" }));
    let new = ask("POST", &format!("/api/v1/shares/{id}/link"), Some(&fixture.sam), "", "").json();
    let new = new["link"].as_str().unwrap().strip_prefix("/c/").expect("a collecting link").to_string();
    assert_eq!(put(&old, "a.jpg", &jpeg(71)), 404);
    assert_eq!(put(&new, "a.jpg", &jpeg(71)), 201);
}

#[test]
fn a_shared_album_follows_its_owners_rights() {
    let fixture = fixture();
    let (owner, member) = (account("Rights Owner", Level::None), account("Rights Member", Level::None));
    fixture.server.store.set_grants("Rights Owner", &Grants { access: Level::Edit, ..Grants::default() }).unwrap();
    let archive = fixture.ids[0];
    let key = into_album(&owner, json!({ "ids": [archive], "name": "Archive", "inside": true })).json()["album"].as_str().unwrap().to_string();
    assert_eq!(ask("PUT", &format!("/api/v1/albums/{key}/people"), Some(&owner), r#"{"people":{"Rights Member":"view"}}"#, "").status, 200);
    assert_eq!(status(&format!("/api/v1/photos/{archive}/thumb"), &member), 200, "the owner may edit it");
    fixture.server.store.set_grants("Rights Owner", &Grants { access: Level::None, ..Grants::default() }).unwrap();
    assert_eq!(status(&format!("/api/v1/photos/{archive}/thumb"), &member), 404, "no longer the owner's to share");
    assert_eq!(status(&format!("/api/v1/photos/{archive}/thumb"), &owner), 404);
}

#[test]
fn a_guest_cannot_send_a_pixel_bomb() {
    let fixture = fixture();
    let (_, token) = collect(&fixture.sam, json!({ "title": "Bombs" }));
    let answer = raw("PUT", &format!("/api/v1/c/{token}/files/bomb.png"), None, &PIXEL_BOMB);
    assert_eq!(answer.status, 413, "{}", String::from_utf8_lossy(&answer.body));
    assert!(String::from_utf8_lossy(&answer.body).contains("more pixels"), "{}", String::from_utf8_lossy(&answer.body));
    assert_eq!(put(&token, "fine.png", &png_of_size(64, 48)), 201, "an ordinary one still goes in");
}

const PIXEL_BOMB: [u8; 47] = [
        0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52, 0x00, 0x00, 0xea, 0x60, 0x00, 0x00, 0xea, 0x60, 0x01, 0x00, 0x00, 0x00, 0x00, 0xa8, 0xa9, 0x48, 0xef, 0x00, 0x00, 0x00, 0x02, 0x49, 0x44, 0x41, 0x54, 0x78, 0x9c, 0x62, 0xa4, 0x91, 0x2b,
    ];

fn png_of_size(width: u32, height: u32) -> Vec<u8> {
    let mut bytes = Vec::new();
    image::RgbImage::from_pixel(width, height, image::Rgb([7, 7, 7])).write_to(&mut std::io::Cursor::new(&mut bytes), image::ImageFormat::Png).unwrap();
    bytes
}

#[test]
fn an_admin_cannot_share_an_album_into_their_own_view() {
    let fixture = fixture();
    let owner = account("Diary Owner", Level::None);
    let diary = send_home(&owner, "diary.jpg", 81);
    let key = into_album(&owner, json!({ "ids": [diary], "name": "Diary", "inside": true })).json()["album"].as_str().unwrap().to_string();
    let people = format!("/api/v1/albums/{key}/people");
    assert_eq!(status(&format!("/api/v1/photos/{diary}/thumb"), &fixture.sam), 404, "not shared with anyone");
    assert_eq!(ask("GET", &people, Some(&fixture.sam), "", "").status, 404);
    assert_eq!(ask("PUT", &people, Some(&fixture.sam), &json!({ "people": { "Sam": "view" } }).to_string(), "").status, 404);
    assert_eq!(status(&format!("/api/v1/photos/{diary}/thumb"), &fixture.sam), 404, "still not");
    assert_eq!(status(&format!("/api/v1/photos/{diary}/jpeg"), &fixture.sam), 404);
    assert_eq!(ask("PUT", &people, Some(&owner), &json!({ "people": { "Sam": "view" } }).to_string(), "").status, 200, "the owner may");
    assert_eq!(status(&format!("/api/v1/photos/{diary}/thumb"), &fixture.sam), 200);
}

#[test]
fn only_the_owner_changes_an_album_whoever_has_its_photographs() {
    let fixture = fixture();
    let (owner, adder, household) = (account("Album Owner", Level::Edit), account("Album Adder", Level::None), account("Album Household", Level::Edit));
    let mine = send_home(&owner, "mine.jpg", 91);
    let key = into_album(&owner, json!({ "ids": [mine], "name": "Owned", "inside": true })).json()["album"].as_str().unwrap().to_string();
    assert_eq!(ask("PUT", &format!("/api/v1/albums/{key}/people"), Some(&owner), r#"{"people":{"Album Adder":"add"}}"#, "").status, 200);
    let theirs = send_home(&adder, "theirs.jpg", 92);
    assert_eq!(into_album(&adder, json!({ "ids": [theirs], "album": key, "inside": true })).status, 200);
    assert_eq!(into_album(&owner, json!({ "ids": [mine], "album": key, "inside": false })).status, 200, "the owner's own photograph leaves it");
    let album = format!("/api/v1/albums/{key}");
    assert_eq!(ask("PUT", &album, Some(&adder), r#"{"name":"Mine now"}"#, "").status, 403, "all that is left is the adder's");
    assert_eq!(ask("DELETE", &album, Some(&adder), "", "").status, 403);

    let archive = fixture.ids[1];
    assert_eq!(into_album(&owner, json!({ "ids": [archive], "album": key, "inside": true })).status, 200);
    assert_eq!(ask("PUT", &album, Some(&household), r#"{"name":"Taken"}"#, "").status, 403);
    assert_eq!(ask("DELETE", &album, Some(&household), "", "").status, 403);
    assert!(fixture.server.store.album_owner(&key).is_some() && fixture.server.store.album_people(&key).len() == 1, "the sharing is intact");

    assert_eq!(ask("PUT", &album, Some(&owner), r#"{"name":"Owned again"}"#, "").status, 200);
    assert_eq!(ask("DELETE", &album, Some(&owner), "", "").status, 200);
    assert!(fixture.server.store.album_owner(&key).is_none(), "and takes its sharing with it");
}

#[test]
fn photographs_with_one_name_are_all_kept() {
    let fixture = fixture();
    let owner = account("Alike Owner", Level::None);
    let (id, token) = collect(&owner, json!({ "title": "Alike", "count": 30 }));
    for seed in 1..=25u8 {
        assert_eq!(put(&token, "IMG_0001.jpg", &jpeg(seed)), 201);
    }
    let accepted = ask("POST", &format!("/api/v1/collects/{id}/accept"), Some(&owner), "{}", "").json();
    assert_eq!((accepted["accepted"].as_i64(), accepted["duplicates"].as_i64()), (Some(25), Some(0)));
    let folder = fixture.server.home.sent_folder("Alike Owner").join("Guests").join("Alike");
    assert_eq!(std::fs::read_dir(&folder).unwrap().count(), 25, "all of them, none written over");
    assert!(folder.join("IMG_0001.jpg").exists() && folder.join("IMG_0001 (25).jpg").exists());
}
