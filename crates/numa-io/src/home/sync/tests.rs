use super::*;

fn remote(id: &str, path: &str, updated: i64) -> Remote {
    Remote { id: id.into(), path: path.into(), size: 10, mtime: 0, taken: None, rating: 0, flag: "none".into(), updated }
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
    assert_eq!(steps, vec![Step::Download(remote("7", "2026/b.raf", 5)), Step::Send("2026/a.raf".into())]);

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
