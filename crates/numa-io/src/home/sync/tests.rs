use super::*;

fn remote(id: &str, path: &str, updated: i64) -> Remote {
    Remote { id: id.into(), path: path.into(), size: 10, mtime: 0, taken: None, rating: 0, flag: "none".into(), edited: false, updated }
}

fn local(path: &str, edits: Option<&str>, rating: u8) -> Local {
    Local { path: path.into(), edits: edits.map(String::from), rating, flag: "none".into() }
}

fn link(kept: &[(&str, &Local, i64)]) -> Link {
    let photos = kept.iter().map(|(id, photo, updated)| (photo.path.clone(), photo.kept(id, *updated))).collect();
    Link { since: 100, photos, ..Link::default() }
}

#[test]
fn new_on_the_server_comes_down_and_new_here_goes_up() {
    let ours = [local("2026/a.raf", None, 0)];
    let steps = plan(&Link::default(), &[remote("7", "2026/b.raf", 5)], &ours);
    assert_eq!(steps, vec![Step::Arrive(remote("7", "2026/b.raf", 5)), Step::Send("2026/a.raf".into())]);

    let sent = link(&[("7", &local("2026/b.RAF", None, 0), 5)]);
    assert_eq!(plan(&sent, &[remote("7", "2026/b.raf", 5)], &[local("2026/b.RAF", None, 0)]), vec![]);
}

const NONE: Parts = Parts { edits: false, marks: false };
const EDITS: Parts = Parts { edits: true, marks: false };
const MARKS: Parts = Parts { edits: false, marks: true };

fn both(path: &str, id: &str, theirs: Option<Remote>, down: Parts, up: Parts) -> Step {
    Step::Both { path: path.into(), id: id.into(), theirs, down, up }
}

#[test]
fn a_change_on_one_side_goes_to_the_other() {
    let a = local("a.raf", Some("{\"exposure\":1}"), 2);
    let b = local("b.raf", None, 0);
    let kept = link(&[("1", &a, 50), ("2", &b, 50)]);

    let a_now = Local { rating: 4, ..a.clone() };
    let steps = plan(&kept, &[remote("2", "b.raf", 120)], &[a_now, b.clone()]);
    assert_eq!(steps, vec![both("b.raf", "2", Some(remote("2", "b.raf", 120)), EDITS, NONE), both("a.raf", "1", None, NONE, MARKS)]);

    assert_eq!(plan(&kept, &[remote("1", "a.raf", 50)], &[a, b]), vec![]);
}

#[test]
fn a_rating_here_and_an_edit_there_both_survive() {
    let a = local("a.raf", None, 0);
    let kept = link(&[("1", &a, 50)]);
    let rated = local("a.raf", None, 3);
    let theirs = Remote { rating: 0, ..remote("1", "a.raf", 120) };
    assert_eq!(plan(&kept, &[theirs.clone()], &[rated]), vec![both("a.raf", "1", Some(theirs), EDITS, MARKS)]);

    let edited = local("a.raf", Some("{}"), 0);
    let theirs = Remote { rating: 5, flag: "pick".into(), ..remote("1", "a.raf", 120) };
    assert_eq!(plan(&kept, &[theirs.clone()], &[edited]), vec![both("a.raf", "1", Some(theirs), MARKS, EDITS)]);
}

#[test]
fn both_changed_this_side_wins_and_deletes_never_travel() {
    let a = local("a.raf", None, 0);
    let kept = link(&[("1", &a, 50), ("2", &local("gone.raf", None, 0), 50)]);
    let edited = local("a.raf", Some("{}"), 0);
    let theirs = remote("1", "a.raf", 120);
    let steps = plan(&kept, &[theirs.clone(), remote("2", "gone.raf", 120)], &[edited]);
    assert_eq!(steps, vec![both("a.raf", "1", Some(theirs), NONE, EDITS)]);

    assert_eq!(plan(&kept, &[], &[a]), vec![]);
}

#[test]
fn a_file_here_before_the_link_takes_the_side_with_something_on_it() {
    let theirs = remote("9", "a.raf", 10);
    assert_eq!(
        plan(&Link::default(), &[theirs.clone()], &[local("a.raf", None, 0)]),
        vec![both("a.raf", "9", Some(theirs.clone()), Parts::ALL, NONE)]
    );
    assert_eq!(plan(&Link::default(), &[theirs.clone()], &[local("a.raf", None, 3)]), vec![both("a.raf", "9", Some(theirs), NONE, Parts::ALL)]);
}

#[test]
fn a_path_from_the_server_stays_inside_the_library() {
    let root = Path::new("/home/x/Pictures/Numa Home/Family");
    assert_eq!(inside(root, "2026/03/a.raf"), Some(root.join("2026/03/a.raf")));
    assert_eq!(inside(root, "../../.bashrc"), None);
    assert_eq!(inside(root, "/etc/passwd"), None);
    assert_eq!(inside(root, "2026/./a.raf"), Some(root.join("2026/a.raf")));
    assert_eq!(inside(root, ""), None);
}

#[test]
fn months_newest_first_with_counts_and_sizes() {
    let march = 1_772_323_200;
    let photos = [
        Remote { taken: Some(march), size: 100, ..remote("1", "a", 0) },
        Remote { taken: Some(march + 86_400), size: 50, ..remote("2", "b", 0) },
        Remote { mtime: 1_767_225_600, size: 7, ..remote("3", "c", 0) },
    ];
    assert_eq!(
        months(&photos),
        vec![
            Month { key: "2026-03".into(), name: "March 2026".into(), count: 2, bytes: 150 },
            Month { key: "2026-01".into(), name: "January 2026".into(), count: 1, bytes: 7 },
        ]
    );
    assert_eq!(next_since(100, &photos), 100);
}

#[test]
fn the_answers_are_read() {
    let listed = photos("[{\"id\":3,\"path\":\"2026/a.raf\",\"size\":9,\"mtime\":1,\"taken\":null,\"rating\":2,\"flag\":\"pick\",\"updated\":44}]\n200").unwrap();
    assert_eq!(listed[0].id, "3");
    assert_eq!((listed[0].rating, listed[0].flag.as_str(), listed[0].updated), (2, "pick", 44));
    assert!(!listed[0].edited, "a server from before says nothing of edits");
    assert!(photos("[{\"id\":3,\"path\":\"a.raf\",\"edited\":true,\"updated\":1}]\n200").unwrap()[0].edited);
    assert_eq!(libraries("[{\"id\":1,\"name\":\"Family\",\"count\":12,\"mine\":false}]\n200").unwrap()[0].name, "Family");
    assert_eq!(edits("{\"edits\":null,\"updated\":5}\n200"), Ok((None, 5)));
    assert_eq!(edits("{\"edits\":{\"exposure\":1},\"updated\":6}\n200"), Ok((Some("{\"exposure\":1}".into()), 6)));
    assert_eq!(edits("{\"edits\":\"{\\\"a\\\":1}\",\"updated\":6}\n200"), Ok((Some("{\"a\":1}".into()), 6)));
    assert_eq!(updated("{\"updated\":77}\n200"), Ok(77));
    assert_eq!(updated("\n401"), Err(Failure::SignIn("Sign in again".into())));
    assert_eq!(flag(flag_name(crate::catalog::Flag::Rejected)), crate::catalog::Flag::Rejected);
    assert_eq!(digest(None), "");
    assert_ne!(digest(Some("{}")), digest(Some("{ }")));
}

#[test]
fn the_command_lines_keep_the_token_off_them() {
    let all = [
        list_libraries("nas"),
        list_photos("nas", "1", 5),
        original("nas", "3", Path::new("/tmp/a.part")),
        get_edits("nas", "3"),
        put_marks("nas", "3", 4, "pick"),
        super::super::send_into("nas", Path::new("/tmp/a.raf"), "ab", "1", "2026/03 Rome"),
    ];
    for argv in &all {
        assert!(argv.windows(2).any(|pair| pair[0] == "--header" && pair[1] == "@-"), "{argv:?}");
    }
    assert_eq!(all[1].last().unwrap(), "http://nas:8283/api/v1/libraries/1/photos?since=5");
    assert_eq!(all[5].last().unwrap(), "http://nas:8283/api/v1/send/a.raf?library=1&dir=2026%2F03%20Rome");
}

#[test]
fn the_night_and_the_hotspot_hold_a_round_back() {
    let night = Pace::default();
    assert!(night.allows(1, false) && night.allows(5, false));
    assert!(!night.allows(0, false) && !night.allows(6, false) && !night.allows(14, false));
    assert!(!night.allows(3, true));
    let always = Pace { night: false, unmetered: false };
    assert!(always.allows(14, true));
    assert!(!Pace { night: false, unmetered: true }.allows(14, true));
}

#[test]
fn a_library_from_before_lists_everything_once_and_nothing_comes_twice() {
    let brought = local("2026-03/a.raf", None, 3);
    let removed = local("2026-03/b.raf", None, 0);
    let kept = Link { since: 500, ..link(&[("1", &brought, 400), ("2", &removed, 400)]) };
    assert_eq!(listed_after(&kept), 0);
    let all = [remote("1", "2026-03/a.raf", 400), remote("2", "2026-03/b.raf", 400), remote("3", "2026-01/c.raf", 200)];
    assert_eq!(plan(&kept, &all, &[brought.clone()]), vec![Step::Arrive(remote("3", "2026-01/c.raf", 200))]);
    assert_eq!(listed_after(&Link { everything: true, ..kept }), 500);

    let before: Link = serde_json::from_str(r#"{"address":"nas","library":"2","name":"Garden","since":9,"photos":{}}"#).unwrap();
    assert!(!before.everything && !before.keep_here);

    let at_home = local("2026-01/c.raf", None, 0);
    let now = link(&[("3", &at_home, 200)]);
    let starred = local("2026-01/c.raf", None, 5);
    assert_eq!(plan(&now, &[], &[starred]), vec![both("2026-01/c.raf", "3", None, NONE, MARKS)]);
}

#[test]
fn home_thumbnails_come_in_one_curl_and_each_is_read() {
    let to = |name: &str| PathBuf::from(format!("/lib/.numa/home/{name}.jpg.part"));
    let argv = thumbnails("nas", &[("7".into(), to("7")), ("8".into(), to("8"))]);
    let text: Vec<String> = argv.iter().map(|arg| arg.to_string_lossy().to_string()).collect();
    assert_eq!(text.iter().filter(|arg| *arg == "--output").count(), 2);
    assert!(text.ends_with(&["--output".into(), "/lib/.numa/home/8.jpg.part".into(), "http://nas:8283/api/v1/photos/8/thumb?edge=256".into()]));
    assert!(text.windows(2).any(|pair| pair == ["--header", "@-"]) && text.contains(&"--fail".to_string()));
    let preview: Vec<String> = preview("nas", "8", Path::new("/c/p.jpg")).iter().map(|arg| arg.to_string_lossy().to_string()).collect();
    assert_eq!(preview.last().unwrap(), "http://nas:8283/api/v1/photos/8/preview");
    assert!(preview.windows(2).any(|pair| pair == ["--output", "/c/p.jpg"]) && preview.contains(&"--fail".to_string()));
    assert_eq!(thumbnails_came("200\n404\n"), Ok(vec![true, false]));
    assert_eq!(thumbnails_came("000\n000\n"), Err(Failure::Unreachable));
    assert_eq!(thumbnails_came(""), Err(Failure::Unreachable));
    assert!(matches!(thumbnails_came("401\n401\n"), Err(Failure::SignIn(_))));
}
