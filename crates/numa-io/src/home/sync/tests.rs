use super::*;

fn remote(id: &str, path: &str, updated: i64) -> Remote {
    Remote { id: id.into(), path: path.into(), size: 10, mtime: 0, taken: None, rating: 0, flag: "none".into(), edited: false, updated, albums: None, caption: None, deleted: None }
}

fn local(path: &str, edits: Option<&str>, rating: u8) -> Local {
    Local { path: path.into(), edits: edits.map(String::from), rating, flag: "none".into(), ..Local::default() }
}

fn none() -> BTreeMap<String, i64> {
    BTreeMap::new()
}

fn plan(link: &Link, remote: &[Remote], local: &[Local]) -> Vec<Step> {
    super::plan(link, remote, local, &none())
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

const NONE: Parts = Parts { edits: false, marks: false, albums: false, caption: false };
const EDITS: Parts = Parts { edits: true, ..NONE };
const MARKS: Parts = Parts { marks: true, ..NONE };
const BOTH: Parts = Parts { edits: true, marks: true, ..NONE };

fn both(path: &str, id: &str, theirs: Option<Remote>, down: Parts, up: Parts) -> Step {
    Step::Both { path: path.into(), id: id.into(), theirs, down, up, albums: None, caption: None }
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
fn a_stack_as_stored_agrees_with_the_link_from_before() {
    let stored = r#"{"source":{"path":"a.RAF"},"operations":[{"type":"Basic","exposure":1.0}]}"#;
    let again = serde_json::to_string(&serde_json::from_str::<numa_core::document::Document>(stored).unwrap()).unwrap();
    assert_ne!(stored, again, "an older stack's text is not what this build writes");
    let kept = link(&[("1", &local("a.raf", Some(&again), 0), 50)]);
    assert_eq!(plan(&kept, &[], &[local("a.raf", Some(stored), 0)]), vec![]);
    let changed = stored.replace("1.0", "2.0");
    assert_eq!(plan(&kept, &[], &[local("a.raf", Some(&changed), 0)]), vec![both("a.raf", "1", None, NONE, EDITS)]);
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
    assert_eq!(plan(&kept, &[theirs.clone()], &[edited]), vec![both("a.raf", "1", Some(theirs), BOTH, EDITS)]);
}

#[test]
fn a_photograph_removed_here_but_not_trashed_stays_removed() {
    let a = local("a.raf", None, 0);
    let kept = link(&[("1", &a, 50), ("2", &local("gone.raf", None, 0), 50)]);
    let edited = local("a.raf", Some("{}"), 0);
    let theirs = remote("1", "a.raf", 120);
    let steps = plan(&kept, &[theirs.clone(), remote("2", "gone.raf", 120)], &[edited]);
    assert_eq!(steps, vec![both("a.raf", "1", Some(theirs), EDITS, EDITS)]);

    assert_eq!(plan(&kept, &[], &[a]), vec![]);
}

#[test]
fn a_file_here_before_the_link_takes_the_side_with_something_on_it() {
    let theirs = remote("9", "a.raf", 10);
    assert_eq!(
        plan(&Link::default(), &[theirs.clone()], &[local("a.raf", None, 0)]),
        vec![both("a.raf", "9", Some(theirs.clone()), Parts::ALL, NONE)]
    );
    assert_eq!(plan(&Link::default(), &[theirs.clone()], &[local("a.raf", None, 3)]), vec![both("a.raf", "9", Some(theirs), EDITS, Parts::ALL)]);
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
        put_caption("nas", "3", "The harbour at six."),
    ];
    for argv in &all {
        assert!(argv.windows(2).any(|pair| pair[0] == "--header" && pair[1] == "@-"), "{argv:?}");
    }
    assert_eq!(all[1].last().unwrap(), "http://nas:8283/api/v1/libraries/1/photos?since=5");
    assert_eq!(all[5].last().unwrap(), "http://nas:8283/api/v1/send/a.raf?library=1&dir=2026%2F03%20Rome");
    assert_eq!(all[6].last().unwrap(), "http://nas:8283/api/v1/photos/3/caption");
    assert!(all[6].iter().any(|word| word == "{\"caption\":\"The harbour at six.\"}"));
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
    assert_eq!(listed_after(&Link { everything: true, rules: RULES, ..kept.clone() }), 500);

    assert_eq!(listed_after(&Link { everything: true, ..kept }), 0);

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
    assert!(text.contains(&"--parallel".to_string()));

    let came = thumbnails_came("404 /lib/.numa/home/8.jpg.part\n200 /Numa Home/Aad & Wijn/.numa/home/7.jpg.part\n").unwrap();
    assert_eq!(came, HashSet::from([PathBuf::from("/Numa Home/Aad & Wijn/.numa/home/7.jpg.part")]));
    assert_eq!(thumbnails_came("000\n000\n"), Err(Failure::Unreachable));
    assert_eq!(thumbnails_came(""), Err(Failure::Unreachable));
    assert!(matches!(thumbnails_came("401\n401\n"), Err(Failure::SignIn(_))));
}

fn gone(id: &str, here: bool) -> Gone {
    Gone { id: id.into(), here, restored: false }
}

fn deleted(id: &str, path: &str, at: i64) -> Remote {
    Remote { deleted: Some(at), ..remote(id, path, at) }
}

#[test]
fn deletes_go_to_the_other_sides_trash() {
    let a = local("a.raf", None, 0);
    let kept = link(&[("1", &a, 50)]);
    assert_eq!(plan(&kept, &[deleted("1", "a.raf", 120)], &[a.clone()]), vec![Step::Trash { path: "a.raf".into(), id: "1".into() }]);

    assert_eq!(plan(&kept, &[deleted("1", "a.raf", 120)], &[local("a.raf", None, 4)]), vec![Step::Restore("a.raf".into())]);

    assert_eq!(plan(&Link::default(), &[deleted("5", "b.raf", 120)], &[]), vec![]);
    assert_eq!(plan(&kept, &[deleted("9", "a.raf", 120)], &[a.clone()]), vec![]);

    assert_eq!(plan(&kept, &[deleted("", "a.raf", 120)], &[a]), vec![Step::Trash { path: "a.raf".into(), id: "1".into() }]);

    let trashed = BTreeMap::from([("a.raf".to_string(), 130)]);
    let delete = vec![Step::Delete { path: "a.raf".into(), id: "1".into() }];
    assert_eq!(super::plan(&kept, &[], &[], &trashed), delete);
    assert_eq!(super::plan(&kept, &[remote("1", "a.raf", 50)], &[], &trashed), delete);

    let changed = Remote { rating: 5, ..remote("1", "a.raf", 120) };
    assert_eq!(super::plan(&kept, &[changed.clone()], &[], &trashed), vec![Step::PutBack(changed)]);
}

#[test]
fn a_photograph_put_back_comes_back_on_the_other_side() {

    let after_home = Link { gone: BTreeMap::from([("a.raf".to_string(), gone("1", false))]), ..Link::default() };
    let back = remote("7", "a.raf", 200);
    assert_eq!(plan(&after_home, &[back.clone()], &[]), vec![Step::PutBack(back.clone())]);

    assert_eq!(plan(&after_home, &[], &[local("a.raf", None, 0)]), vec![Step::Restore("a.raf".into())]);

    let after_here = Link { gone: BTreeMap::from([("a.raf".to_string(), gone("1", true))]), ..Link::default() };
    assert_eq!(plan(&after_here, &[], &[local("a.raf", None, 0)]), vec![Step::Restore("a.raf".into())]);

    let asked = Link { gone: BTreeMap::from([("a.raf".to_string(), Gone { restored: true, ..gone("1", true) })]), ..Link::default() };
    assert_eq!(plan(&asked, &[], &[local("a.raf", None, 0)]), vec![]);

    let steps = plan(&after_here, &[back.clone()], &[local("a.raf", None, 3)]);
    assert_eq!(steps, vec![Step::Both { path: "a.raf".into(), id: "7".into(), theirs: Some(back), down: EDITS, up: Parts::ALL, albums: None, caption: None }]);
}

#[test]
fn a_merged_album_goes_up_and_does_not_come_back() {
    let names = |list: &[&str]| list.iter().map(|name| name.to_string()).collect::<Vec<String>>();
    let agreed = names(&["Holiday", "Holiday at Home"]);
    assert_eq!(albums(Some(&agreed), &names(&["Holiday"]), &agreed), names(&["Holiday"]));
    let agreed = names(&["Holiday at Home"]);
    assert_eq!(albums(Some(&agreed), &names(&["Holiday"]), &agreed), names(&["Holiday"]));
}

#[test]
fn a_round_pauses_at_a_tenth_free_or_20_gb_whichever_is_more() {
    const GB: u64 = 1_000_000_000;
    assert!(short_of_room(19 * GB, 100 * GB));
    assert!(!short_of_room(20 * GB, 100 * GB));
    assert!(short_of_room(99 * GB, 1000 * GB));
    assert!(!short_of_room(100 * GB, 1000 * GB));
}

#[test]
fn an_old_folder_kept_on_purpose_is_not_offered() {
    let (old, synced) = (PathBuf::from("/p/Numa Home/Holiday"), PathBuf::from("/p/Numa Home/Family"));
    assert_eq!(old_folders(vec![old.clone(), synced.clone()], &[synced]), [old]);
}

#[test]
fn an_answer_of_hashes_is_by_id_and_says_only_what_home_could_read() {
    let out = "[{\"id\":7,\"hash\":\"ab\"}]\n200";
    assert_eq!(photo_hashes(out).unwrap(), HashMap::from([("7".to_string(), "ab".to_string())]));
    let cmd = hash_photos("http://home:8080", &["7".to_string()]).iter().map(|word| word.to_string_lossy().to_string()).collect::<Vec<_>>().join(" ");
    assert!(cmd.contains("/api/v1/photos/hashes") && cmd.contains(r#"{"ids":[7]}"#), "{cmd}");
}

#[test]
fn albums_go_both_ways_by_name() {
    let names = |list: &[&str]| list.iter().map(|name| name.to_string()).collect::<Vec<String>>();
    let agreed = names(&["Rome", "Family"]);

    assert_eq!(albums(Some(&agreed), &names(&["Rome", "Family", "Best"]), &names(&["Rome"])), names(&["Best", "Rome"]));
    assert_eq!(albums(None, &names(&["rome"]), &names(&["Rome", "Coast"])), names(&["Coast", "rome"]));
    assert!(same_albums(&names(&["Rome", "best"]), &names(&["Best", "rome"])));

    let a = Local { albums: names(&["Rome"]), ..local("a.raf", None, 0) };
    let kept = Link { photos: BTreeMap::from([("a.raf".to_string(), Kept { albums: Some(names(&["Rome"])), ..a.kept("1", 50) })]), ..Link::default() };

    let best = Local { albums: names(&["Rome", "Best"]), ..a.clone() };
    let up = Parts { albums: true, ..NONE };
    assert_eq!(plan(&kept, &[], &[best]), vec![Step::Both { path: "a.raf".into(), id: "1".into(), theirs: None, down: NONE, up, albums: Some(names(&["Best", "Rome"])), caption: None }]);

    let theirs = Remote { albums: Some(vec![]), ..remote("1", "a.raf", 120) };
    let down = Parts { edits: true, albums: true, ..NONE };
    assert_eq!(plan(&kept, &[theirs.clone()], &[a.clone()]), vec![Step::Both { path: "a.raf".into(), id: "1".into(), theirs: Some(theirs), down, up: NONE, albums: Some(vec![]), caption: None }]);

    assert_eq!(plan(&kept, &[remote("1", "a.raf", 50)], &[a.clone()]), vec![]);

    let before = link(&[("1", &a, 50)]);
    let said = Remote { albums: Some(names(&["Rome"])), ..remote("1", "a.raf", 50) };
    assert_eq!(plan(&before, &[said.clone()], &[a]), vec![Step::Both { path: "a.raf".into(), id: "1".into(), theirs: Some(said), down: NONE, up: NONE, albums: Some(names(&["Rome"])), caption: None }]);
}

#[test]
fn a_conflict_goes_to_the_later_and_keeps_the_other() {
    let stack = |exposure: f32| {
        let mut document = numa_core::document::Document::new("/home/a/a.raf".into());
        let mut basic = document.basic();
        basic.tone.exposure = exposure;
        document.set_basic(basic);
        serde_json::to_string(&document).unwrap()
    };
    let agreed = Local { edits: Some(stack(1.0)), ..local("a.raf", None, 0) };
    let kept = agreed.kept("1", 100);
    let ours = Local { edits: Some(stack(2.0)), changed: 150, ..agreed.clone() };

    let home = |exposure: f32| stack(exposure).replace("/home/a/a.raf", "/photos/2026/a.raf");
    assert_eq!(settle(&kept, &ours, Some(&home(1.0)), 120), Settled::Up, "Home's unchanged");
    assert_eq!(settle(&kept, &ours, Some(&home(2.0)), 160), Settled::Same, "the same on both");
    assert_eq!(settle(&kept, &agreed, Some(&home(3.0)), 160), Settled::Down, "unchanged here");
    assert_eq!(settle(&kept, &ours, Some(&home(3.0)), 140), Settled::UpKeepingTheirs, "here later");
    assert_eq!(settle(&kept, &ours, Some(&home(3.0)), 160), Settled::DownKeepingOurs, "Home later");

    assert_eq!(settle(&kept, &Local { changed: 0, ..ours.clone() }, Some(&home(3.0)), 160), Settled::UpKeepingTheirs);

    let before = Kept { canon: None, ..kept };
    assert_eq!(settle(&before, &ours, Some(&home(1.0)), 120), Settled::UpKeepingTheirs);
    assert_eq!(aside(true, "24 Oct 10:12", ""), "From Numa Home, 24 Oct 10:12");
    assert_eq!(aside(false, "", "Tijmen's MacBook"), "From Tijmen's MacBook");
    assert_eq!(device_label("Tijmen", "MacBook"), "Tijmen's MacBook");
    assert_eq!(device_label("Tijmen", "Tijmen's iPhone"), "Tijmen's iPhone");
    assert_eq!(canon(None), canon(Some(&serde_json::to_string(&numa_core::document::Document::new("x".into())).unwrap())));
}

#[test]
fn marks_given_on_both_sides_go_to_the_later() {
    let a = local("a.raf", None, 0);
    let kept = link(&[("1", &a, 100)]);
    let theirs = Remote { rating: 5, ..remote("1", "a.raf", 160) };
    let early = Local { rating: 2, changed: 150, ..a.clone() };
    assert_eq!(plan(&kept, &[theirs.clone()], &[early]), vec![both("a.raf", "1", Some(theirs.clone()), BOTH, NONE)]);
    let late = Local { rating: 2, changed: 170, ..a };
    assert_eq!(plan(&kept, &[theirs.clone()], &[late]), vec![both("a.raf", "1", Some(theirs), EDITS, MARKS)]);
}

#[test]
fn the_way_is_kept_with_the_link() {
    let up = Link { way: Some(Way::Up), ..Link::default() };
    assert_eq!(way(&up, true), Way::Up);
    assert_eq!(way(&Link::default(), true), Way::Down);
    assert_eq!(way(&Link::default(), false), Way::Up);
    assert!(synced(&up, Way::Up), "a library of this device's has its originals here");
    assert!(!synced(&Link::default(), Way::Down));
    assert!(synced(&Link { keep_here: true, ..Link::default() }, Way::Down), "Keep on This Computer is Sync on This Device");
    let json = serde_json::to_string(&up).unwrap();
    assert!(json.contains("\"way\":\"up\""));
    let read: Link = serde_json::from_str(r#"{"address":"nas","library":"2","name":"Garden","since":9,"photos":{"a.raf":{"id":"3","edits":"","rating":0,"flag":"none","updated":1}}}"#).unwrap();
    assert_eq!((read.way, read.rules, read.photos["a.raf"].canon.clone()), (None, 0, None));

    assert_eq!(restored("{\"restored\":1,\"photos\":[{\"id\":12,\"path\":\"2026/a.raf\"}]}\n200"), Ok(vec![("12".into(), "2026/a.raf".into())]));
    assert_eq!(photos("[{\"id\":null,\"path\":\"a.raf\",\"deleted\":5,\"updated\":5}]\n200").unwrap()[0], Remote { size: 0, ..deleted("", "a.raf", 5) });
    assert_eq!(albums_at_home("[{\"key\":\"k1\",\"name\":\"Rome\",\"count\":3}]\n200"), Ok(vec![("k1".into(), "Rome".into())]));
    assert!(list_changes("nas", "1", 5, true, 1).last().unwrap().to_string_lossy().ends_with("/libraries/1/photos?since=5&gone=1&rules=1&synced=1"));
    assert!(delete_photos("nas", &["3".into()]).iter().any(|arg| arg == "{\"ids\":[3]}"));
}

#[test]
fn a_pairing_looks_at_homes_edits_before_sending_ours() {
    let ours = local("a.raf", Some("{\"exposure\":1}"), 0);
    let theirs = Remote { edited: true, ..remote("1", "a.raf", 120) };
    let steps = plan(&Link::default(), &[theirs.clone()], &[ours]);
    assert_eq!(steps, vec![both("a.raf", "1", Some(theirs.clone()), EDITS, Parts::ALL)]);

    let steps = plan(&Link::default(), &[theirs.clone()], &[local("a.raf", None, 3)]);
    assert_eq!(steps, vec![both("a.raf", "1", Some(theirs), EDITS, Parts::ALL)]);
}

#[test]
fn a_pairing_never_writes_over_homes_edits_unkept() {
    let stack = |exposure: f32| {
        let mut document = numa_core::document::Document::new("/home/a/a.raf".into());
        let mut basic = document.basic();
        basic.tone.exposure = exposure;
        document.set_basic(basic);
        serde_json::to_string(&document).unwrap()
    };
    let ours = Local { edits: Some(stack(2.0)), changed: 150, ..local("a.raf", None, 0) };
    let home = |exposure: f32| stack(exposure).replace("/home/a/a.raf", "/photos/a.raf");
    assert_eq!(settle_new(&ours, Some(&home(2.0)), 160), Settled::Same);
    assert_eq!(settle_new(&ours, Some(&home(3.0)), 140), Settled::UpKeepingTheirs, "here later: Home's kept");
    assert_eq!(settle_new(&ours, Some(&home(3.0)), 160), Settled::DownKeepingOurs, "Home later: ours kept");
    assert_eq!(settle_new(&ours, None, 160), Settled::Up, "Home has none: nothing to keep");

    assert_eq!(settle_new(&local("a.raf", None, 3), Some(&home(3.0)), 160), Settled::Down);
}

#[test]
fn a_round_killed_part_way_resumes_from_what_it_kept() {
    let (a, b) = (local("a.raf", Some("{\"exposure\":1}"), 0), local("b.raf", Some("{\"exposure\":2}"), 0));
    let theirs = |id: &str, path: &str| Remote { edited: true, ..remote(id, path, 120) };
    let (home_a, home_b) = (theirs("1", "a.raf"), theirs("2", "b.raf"));

    let saved = Link { photos: BTreeMap::from([("a.raf".to_string(), a.kept("1", 120))]), ..Link::default() };
    let steps = plan(&saved, &[home_a, home_b.clone()], &[a, b]);
    assert_eq!(steps, vec![both("b.raf", "2", Some(home_b), EDITS, Parts::ALL)], "a.raf is settled; b.raf is looked at");
    assert!(keep_now(KEEP_EVERY) && !keep_now(1) && !keep_now(0));
}

fn captioned(path: &str, caption: &str, changed: i64) -> Local {
    Local { caption: caption.into(), caption_changed: changed, ..local(path, None, 0) }
}

fn agreed_on(ours: &Local, caption: &str) -> Link {
    Link { photos: BTreeMap::from([(ours.path.clone(), Kept { caption: Some(caption.into()), ..ours.kept("1", 50) })]), ..Link::default() }
}

fn worded(theirs: Option<Remote>, down: Parts, up: Parts, caption: &str) -> Vec<Step> {
    vec![Step::Both { path: "a.raf".into(), id: "1".into(), theirs, down, up, albums: None, caption: Some(caption.into()) }]
}

const WORDS: Parts = Parts { caption: true, ..NONE };

#[test]
fn a_caption_goes_the_way_it_changed() {
    let a = captioned("a.raf", "The harbour at six.", 40);
    let kept = agreed_on(&a, "The harbour at six.");
    assert_eq!(plan(&kept, &[], &[a.clone()]), vec![]);

    let longer = captioned("a.raf", "The harbour at six, before the boats.", 90);
    assert_eq!(plan(&kept, &[], &[longer]), worded(None, NONE, WORDS, "The harbour at six, before the boats."));
    assert_eq!(plan(&kept, &[], &[captioned("a.raf", "", 0)]), worded(None, NONE, WORDS, ""));

    let down = Parts { edits: true, caption: true, ..NONE };
    let theirs = Remote { caption: Some("The harbour.".into()), ..remote("1", "a.raf", 120) };
    assert_eq!(plan(&kept, &[theirs.clone()], &[a.clone()]), worded(Some(theirs), down, NONE, "The harbour."));
    let theirs = Remote { caption: Some(String::new()), ..remote("1", "a.raf", 120) };
    assert_eq!(plan(&kept, &[theirs.clone()], &[a.clone()]), worded(Some(theirs), down, NONE, ""));

    let theirs = Remote { caption: Some(" The harbour at six.\r\n".into()), ..remote("1", "a.raf", 50) };
    assert_eq!(plan(&kept, &[theirs], &[a]), vec![]);
}

#[test]
fn words_written_on_both_sides_are_both_kept() {
    let agreed = Some("The harbour.");
    assert_eq!(caption(agreed, "Six in the morning.", "Before the boats left.", true), "Before the boats left.\n\nSix in the morning.");
    assert_eq!(caption(agreed, "Six in the morning.", "Before the boats left.", false), "Six in the morning.\n\nBefore the boats left.");

    assert_eq!(caption(agreed, "The harbour at six.", "The harbour at six. Before the boats left.", false), "The harbour at six. Before the boats left.");

    assert_eq!(caption(agreed, "", "Before the boats left.", false), "Before the boats left.");
    assert_eq!(caption(agreed, "Six in the morning.", "", true), "Six in the morning.");

    assert_eq!(caption(None, "Six in the morning.", "", true), "Six in the morning.");
    assert_eq!(caption(None, "", "", false), "");
    assert_eq!(caption(None, "Six.", "Boats.", false), "Six.\n\nBoats.");

    let (long, other) = ("a".repeat(crate::catalog::CAPTION_LIMIT - 3), "The other side's words.");
    assert_eq!(caption(agreed, &long, other, true), other);
    assert_eq!(caption(agreed, &long, other, false), long);

    let ours = captioned("a.raf", "Six in the morning.", 150);
    let kept = agreed_on(&ours, "The harbour.");
    let theirs = Remote { caption: Some("Before the boats left.".into()), ..remote("1", "a.raf", 160) };
    let down = Parts { edits: true, caption: true, ..NONE };
    assert_eq!(plan(&kept, &[theirs.clone()], &[ours.clone()]), worded(Some(theirs.clone()), down, WORDS, "Before the boats left.\n\nSix in the morning."));
    let later = Local { caption_changed: 170, ..ours.clone() };
    assert_eq!(plan(&kept, &[theirs.clone()], &[later]), worded(Some(theirs.clone()), down, WORDS, "Six in the morning.\n\nBefore the boats left."));

    let undated = Local { caption_changed: 0, ..ours };
    assert_eq!(plan(&kept, &[theirs.clone()], &[undated]), worded(Some(theirs), down, WORDS, "Six in the morning.\n\nBefore the boats left."));
}

#[test]
fn a_caption_waits_for_a_home_that_knows_captions() {
    let ours = captioned("a.raf", "The harbour at six.", 90);
    let before = Link { everything: true, rules: ALBUMS, ..link(&[("1", &ours, 50)]) };
    assert_eq!(plan(&before, &[], &[ours.clone()]), vec![]);
    assert_eq!(plan(&before, &[remote("1", "a.raf", 50)], &[ours.clone()]), vec![]);
    assert_eq!(rules_after(&before, &[remote("1", "a.raf", 50)]), ALBUMS);

    assert_eq!(listed_after(&before), 100);

    let said = Remote { caption: Some(String::new()), ..remote("1", "a.raf", 50) };
    assert_eq!(plan(&before, &[said.clone()], &[ours.clone()]), worded(Some(said.clone()), NONE, WORDS, "The harbour at six."));
    assert_eq!(rules_after(&before, &[said.clone()]), RULES);

    assert_eq!(rules_after(&Link { rules: RULES, ..before.clone() }, &[]), RULES);
    assert_eq!(rules_after(&Link::default(), &[deleted("1", "a.raf", 60)]), ALBUMS);

    let plain = local("a.raf", None, 0);
    let unsaid = Link { everything: true, rules: ALBUMS, ..link(&[("1", &plain, 50)]) };
    assert_eq!(plan(&unsaid, &[said.clone()], &[plain]), worded(Some(said), NONE, NONE, ""));

    assert_eq!(caption_put("{\"error\":\"Not found\"}\n404"), Ok(None));
    assert_eq!(caption_put("\n405"), Ok(None));
    assert_eq!(caption_put("{\"updated\":77}\n200"), Ok(Some(77)));
    assert_eq!(caption_put("\n401"), Err(Failure::SignIn("Sign in again".into())));
    assert_eq!(caption_put("{\"error\":\"A caption is at most 2000 characters.\"}\n400"), Err(Failure::Said("A caption is at most 2000 characters.".into())));
}

#[test]
fn a_pairing_keeps_the_words_of_both_sides() {
    let ours = captioned("a.raf", "The harbour at six.", 90);
    let theirs = Remote { edited: true, rating: 4, caption: Some(String::new()), ..remote("1", "a.raf", 120) };
    let steps = plan(&Link::default(), &[theirs.clone()], &[ours]);
    assert_eq!(steps, worded(Some(theirs), BOTH, WORDS, "The harbour at six."));

    let theirs = Remote { caption: Some("Before the boats left.".into()), ..remote("1", "a.raf", 120) };
    let rated = local("a.raf", None, 3);
    let down = Parts { edits: true, caption: true, ..NONE };
    assert_eq!(plan(&Link::default(), &[theirs.clone()], &[rated]), worded(Some(theirs), down, BOTH, "Before the boats left."));
}

#[test]
fn words_here_win_over_a_delete_and_an_arrival_is_kept_with_its_words() {
    let a = captioned("a.raf", "The harbour.", 40);
    let kept = agreed_on(&a, "The harbour.");
    assert_eq!(plan(&kept, &[deleted("1", "a.raf", 120)], &[a]), vec![Step::Trash { path: "a.raf".into(), id: "1".into() }]);
    let written = captioned("a.raf", "The harbour at six.", 110);
    assert_eq!(plan(&kept, &[deleted("1", "a.raf", 120)], &[written]), vec![Step::Restore("a.raf".into())]);

    let arrives = Remote { caption: Some(" Before the boats left.\n".into()), ..remote("7", "b.raf", 5) };
    assert_eq!(arrives.kept().caption.as_deref(), Some("Before the boats left."));
    assert_eq!(arrives.at_home().caption, " Before the boats left.\n");
    assert_eq!((remote("7", "b.raf", 5).kept().caption, remote("7", "b.raf", 5).at_home().caption), (None, String::new()));

    let before: Link = serde_json::from_str(r#"{"address":"nas","library":"2","name":"Garden","since":9,"photos":{"a.raf":{"id":"1","edits":"","rating":0,"flag":"none","updated":5}}}"#).unwrap();
    assert_eq!(before.photos["a.raf"].caption, None);
}
