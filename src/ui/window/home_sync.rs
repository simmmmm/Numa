use super::home::{account, curl, forget_token, said, scratch, sha256, sign_in_first, signed_out, Running, NUMA_HOME};
use super::places::photos;
use super::*;
use numa::io::home::sync::{self, Gone, Kept, Link, Local, Pace, Remote, Settled, Step, Way};
use numa::io::home::{self as api, Account, Failure};
use std::collections::{BTreeMap, HashSet};
use std::time::Duration;

const EVERY: u32 = 180;

thread_local! {

    static ROUND: Cell<bool> = const { Cell::new(false) };

    static QUIET: RefCell<Option<Cancel>> = const { RefCell::new(None) };

    static TURN: Cell<usize> = const { Cell::new(0) };

    static SHOWN: RefCell<HashMap<i64, Shown>> = RefCell::default();
    static CAPTIONS: RefCell<Vec<(i64, glib::WeakRef<gtk::Label>, String)>> = const { RefCell::new(Vec::new()) };

    static WAYS: RefCell<HashMap<i64, Option<(Way, bool)>>> = RefCell::default();
}

#[derive(Clone, Copy, Default)]
struct Shown {
    syncing: Option<(usize, usize)>,
    to_send: usize,
    waits: bool,
}

impl Shown {
    fn said(self) -> String {
        let n = |n: usize| places::grouped(n as i64);
        match self {
            Shown { syncing: Some((done, total)), .. } => format!("Syncing \u{b7} {} of {}", n(done.min(total)), n(total)),
            Shown { waits: true, .. } => format!("Waits for {NUMA_HOME}"),
            Shown { to_send: 0, .. } => "Synced".to_string(),
            Shown { to_send, .. } => format!("{} to send", n(to_send)),
        }
    }
}

pub(super) fn sync_said(state: &App, library: i64) -> Option<String> {
    is_synced(state, library).then(|| SHOWN.with(|shown| shown.borrow().get(&library).copied().unwrap_or_default()).said())
}

pub(super) fn follow_caption(library: i64, label: &gtk::Label, before: &str) {
    CAPTIONS.with(|captions| {
        let mut captions = captions.borrow_mut();
        captions.retain(|(_, label, _)| label.upgrade().is_some());
        captions.push((library, label.downgrade(), before.to_string()));
    });
}

fn say_state(_state: &App, library: i64, change: impl FnOnce(&mut Shown)) {
    let now = SHOWN.with(|shown| {
        let mut shown = shown.borrow_mut();
        let entry = shown.entry(library).or_default();
        change(entry);
        *entry
    });
    CAPTIONS.with(|captions| {
        for (_, label, before) in captions.borrow().iter().filter(|(id, ..)| *id == library) {
            if let Some(label) = label.upgrade() {
                label.set_text(&format!("{before}{}", now.said()));
            }
        }
    });
}

const PACE: &str = "numa-home-pace";

pub(super) fn linked(state: &App, library: i64) -> Option<Link> {
    serde_json::from_str(&state.catalog.library_setting(library, sync::LINK)?).ok()
}

pub(super) fn is_linked(state: &App, library: i64) -> bool {
    state.catalog.library_setting(library, sync::LINK).is_some()
}

pub(super) fn is_home(state: &App, library: i64) -> bool {
    way_of(state, library) == Some(Way::Down)
}

fn home_root() -> PathBuf {
    let pictures = dirs::picture_dir().unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join("Pictures"));
    pictures.join(NUMA_HOME)
}

fn way_synced(state: &App, library: i64) -> Option<(Way, bool)> {
    if let Some(known) = WAYS.with(|ways| ways.borrow().get(&library).copied()) {
        return known;
    }
    let known = linked(state, library).map(|link| {
        let path = state.libraries.all.borrow().iter().find(|it| it.id == library).map(|it| it.path.clone());
        let path = path.or_else(|| state.catalog.libraries().ok()?.into_iter().find(|it| it.id == library).map(|it| it.path));
        let way = sync::way(&link, path.is_some_and(|path| path.starts_with(home_root())));
        (way, sync::synced(&link, way))
    });
    WAYS.with(|ways| ways.borrow_mut().insert(library, known));
    known
}

pub(super) fn way_of(state: &App, library: i64) -> Option<Way> {
    way_synced(state, library).map(|(way, _)| way)
}

pub(super) fn is_synced(state: &App, library: i64) -> bool {
    way_synced(state, library).is_some_and(|(_, synced)| synced)
}

pub(super) fn unlink(state: &App, library: i64) {
    for key in [sync::LINK, sync::TRASHED] {
        if let Err(err) = state.catalog.set_library_setting(library, key, None) {
            log::warn!("Numa Home: {err}");
        }
    }
    WAYS.with(|ways| ways.borrow_mut().remove(&library));
    SHOWN.with(|shown| shown.borrow_mut().remove(&library));
}

pub(super) fn keep(state: &App, library: i64, link: &Link) {
    WAYS.with(|ways| ways.borrow_mut().remove(&library));
    let saved = serde_json::to_string(link).map_err(|err| err.to_string()).and_then(|json| state.catalog.set_library_setting(library, sync::LINK, Some(&json)));
    if let Err(err) = saved {
        log::warn!("Numa Home: the link could not be kept: {err}");
    }
}

pub(super) fn photographs(count: usize) -> String {
    match count {
        1 => "1 photograph".to_string(),
        n => format!("{} photographs", places::grouped(n as i64)),
    }
}

pub(super) fn import_group(state: &App, dialog: &adw::Dialog, go: &gtk::Button) -> adw::PreferencesGroup {
    let account = account(state);
    let group = adw::PreferencesGroup::new();
    group.set_title(NUMA_HOME);
    group.set_header_suffix(Some(&spinner("Asking Numa Home")));
    let library_row = adw::ComboRow::new();
    library_row.set_title("Library");
    group.add(&library_row);
    let libraries: Rc<RefCell<Vec<sync::Library>>> = Rc::default();
    let listed: Rc<RefCell<Vec<Remote>>> = Rc::default();
    let months: Rc<RefCell<Vec<(sync::Month, gtk::CheckButton, adw::ActionRow)>>> = Rc::default();

    let count = Rc::new(glib::clone!(
        #[weak] go,
        #[weak] group,
        #[strong] months,
        move || {
            let months = months.borrow();
            let ticked: usize = months.iter().filter(|(_, check, _)| check.is_active()).map(|(month, ..)| month.count).sum();
            go.set_label(&import_label(ticked));
            go.set_sensitive(ticked > 0);

            group.set_description(Some("Tick the months to bring").filter(|_| ticked == 0 && !months.is_empty()));
        }
    ));
    count();

    let turn: Rc<Cell<u32>> = Rc::default();
    let show = Rc::new(glib::clone!(
        #[strong] turn,
        #[strong] account,
        #[strong] libraries,
        #[strong] listed,
        #[strong] months,
        #[strong] count,
        #[weak] group,
        move |at: usize| {
            let Some(library) = libraries.borrow().get(at).cloned() else { return };
            let mine = turn.get() + 1;
            turn.set(mine);
            group.set_header_suffix(Some(&spinner("Asking Numa Home")));
            let (account, listed, months, count, turn) = (account.clone(), listed.clone(), months.clone(), count.clone(), turn.clone());
            glib::spawn_future_local(async move {
                let out = curl(sync::list_photos(&account.address, &library.id, 0), account.header(), &Running::default()).await;
                if turn.get() != mine {
                    return;
                }
                group.set_header_suffix(gtk::Widget::NONE);
                for (_, _, row) in months.borrow_mut().drain(..) {
                    group.remove(&row);
                }
                listed.borrow_mut().clear();
                count();
                let everything = match sync::photos(&out) {
                    Ok(listed) => listed,
                    Err(failure) => return group.set_description(Some(&said(failure, &account))),
                };
                group.set_description(None);
                for month in sync::months(&everything) {
                    let row = adw::ActionRow::new();
                    row.set_title(&month.name);
                    row.set_subtitle(&format!("{} \u{b7} {}", photos(month.count), glib::format_size(month.bytes)));
                    let check = gtk::CheckButton::new();
                    row.add_prefix(&check);
                    row.set_activatable_widget(Some(&check));
                    check.connect_toggled(glib::clone!(
                        #[strong] count,
                        move |_| count()
                    ));
                    group.add(&row);
                    months.borrow_mut().push((month, check, row));
                }
                *listed.borrow_mut() = everything;
                count();
            });
        }
    ));
    library_row.connect_selected_notify(glib::clone!(
        #[strong] show,
        move |row| show(row.selected() as usize)
    ));

    glib::spawn_future_local(glib::clone!(
        #[strong] account,
        #[strong] libraries,
        #[weak] group,
        #[weak] library_row,
        async move {
            let out = curl(sync::list_libraries(&account.address), account.header(), &Running::default()).await;
            group.set_header_suffix(gtk::Widget::NONE);
            match sync::libraries(&out) {
                Ok(list) if list.is_empty() => group.set_description(Some("No libraries on Numa Home yet")),
                Ok(list) => {
                    let names: Vec<String> = list.iter().map(|library| format!("{} \u{b7} {}", library.name, photos(library.count as usize))).collect();
                    *libraries.borrow_mut() = list;
                    library_row.set_model(Some(&gtk::StringList::new(&names.iter().map(String::as_str).collect::<Vec<_>>())));
                    show(library_row.selected() as usize);
                }
                Err(failure) => group.set_description(Some(&said(failure, &account))),
            }
        }
    ));

    go.connect_clicked(glib::clone!(
        #[strong] state,
        #[weak] dialog,
        #[weak] group,
        #[weak] library_row,
        move |_| {

            if group.parent().is_none() {
                return;
            }
            let Some(library) = libraries.borrow().get(library_row.selected() as usize).cloned() else { return };
            let keys: HashSet<String> = months.borrow().iter().filter(|(_, check, _)| check.is_active()).map(|(month, ..)| month.key.clone()).collect();
            let all = listed.borrow().clone();

            let ticked = all.iter().filter(|photo| keys.contains(&sync::month_of(photo))).map(|photo| photo.path.clone()).collect();
            let since = sync::next_since(0, &all);
            dialog.close();
            bring(&state, library, all, since, (ticked, None));
        }
    ));
    group
}

fn home_folder(state: &App, from: &sync::Library, account: &Account) -> Result<Library, String> {
    let root = home_root().join(from.name.replace('/', "-"));
    std::fs::create_dir_all(&root).map_err(|err| format!("{}: {err}", root.display()))?;
    let known = state.catalog.libraries()?.into_iter().find(|library| library.path == root);
    let library = known.map_or_else(|| state.catalog.add_library(&root), Ok)?;
    let link = linked(state, library.id).unwrap_or_else(|| Link {
        address: account.address.clone(),
        library: from.id.clone(),
        name: from.name.clone(),
        way: Some(Way::Down),
        ..Link::default()
    });

    if link.library != from.id || !api::same_server(&link.address, &account.address) {
        return Err(format!("{} is kept at {NUMA_HOME} as another library", root.display()));
    }
    keep(state, library.id, &link);
    Ok(library)
}

fn bring(state: &App, from: sync::Library, photos: Vec<Remote>, since: i64, then: (Vec<String>, Option<String>)) {
    let account = account(state);
    let library = match home_folder(state, &from, &account) {
        Ok(library) => library,
        Err(err) => return state.toast(&err),
    };
    let Some(link) = linked(state, library.id) else { return };
    let since = since.max(link.since);

    if ROUND.with(|round| round.replace(true)) {
        return state.toast(&format!("{NUMA_HOME} is busy with this library; import again in a moment"));
    }
    state.libraries.filter.borrow_mut().in_one_library();
    reload_libraries(state);
    select_library(state, library.id);
    round(state, library, link, photos, since, How::Import, then);
}

pub(super) fn adopt(state: &App, catch_up: bool) {
    let account = account(state);
    if !account.signed_in() {
        return;
    }
    let state = state.clone();
    glib::spawn_future_local(async move {
        let listed = match sync::libraries(&curl(sync::list_libraries(&account.address), account.header(), &Running::default()).await) {
            Ok(listed) => listed,
            Err(failure) => return log::info!("Numa Home: {}", said(failure, &account)),
        };
        let links: Vec<Link> = state
            .catalog
            .libraries()
            .unwrap_or_default()
            .iter()
            .filter_map(|library| linked(&state, library.id))
            .filter(|link| api::same_server(&link.address, &account.address))
            .collect();

        let new = listed.into_iter().filter(|from| !links.iter().any(|link| link.library == from.id || (link.library.is_empty() && link.name == from.name)));
        let made: Vec<Library> = new.filter_map(|from| home_folder(&state, &from, &account).map_err(|err| log::info!("Numa Home: {err}")).ok()).collect();
        if !made.is_empty() {
            reload_libraries(&state);
        }
        let unfinished = |library: &Library| {
            linked(&state, library.id).is_some_and(|link| api::same_server(&link.address, &account.address) && !link.library.is_empty() && !link.everything)
        };
        let made = match catch_up {
            true => state.catalog.libraries().unwrap_or_default().into_iter().filter(unfinished).collect(),
            false => made,
        };
        if made.is_empty() {
            return;
        }

        for how in [How::Glance, How::Look] {
            for library in &made {
                round_over().await;
                in_step(&state, library.clone(), how);
                round_over().await;
            }
            if state.stack.visible_child_name().as_deref() == Some("libraries") {
                show_folders(&state);
            }
        }
    });
}

async fn round_over() {
    while ROUND.with(Cell::get) {
        glib::timeout_future(Duration::from_millis(300)).await;
    }
}

pub(super) fn open_link(state: &App, uri: &str) {
    let Some((server, photo)) = api::open_link(uri) else {
        return state.toast("That Open in Numa link could not be read");
    };
    let account = account(state);
    if !account.signed_in() || !api::same_server(&server, &account.address) {
        return state.toast(&sign_in_first(&account, &api::host(&server)));
    }
    let links: Vec<(Library, Link)> = state
        .catalog
        .libraries()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|library| Some((library.clone(), linked(state, library.id)?)))
        .filter(|(_, link)| api::same_server(&link.address, &server))
        .collect();
    let here = links.iter().find_map(|(library, link)| {
        let (path, _) = link.photos.iter().find(|(_, kept)| kept.id == photo)?;
        Some((library.clone(), path.clone()))
    });

    if let Some((library, path)) = here {
        return match sync::inside(&library.path, &path).filter(|file| file.exists()) {
            Some(file) => show_opened(state, library, &file, true),
            None => home_fetch::fetch_paths(state, library, vec![path.clone()], Some(path)),
        };
    }
    let state = state.clone();
    glib::spawn_future_local(async move {
        let (from, path, all) = match find(&account, &photo).await {
            Ok(found) => found,
            Err(failure) => return state.toast(&said(failure, &account)),
        };
        let since = sync::next_since(0, &all);

        round_over().await;
        bring(&state, from, all, since, (vec![path.clone()], Some(path)));
    });
}

async fn find(account: &Account, photo: &str) -> Result<(sync::Library, String, Vec<Remote>), Failure> {
    let ask = |argv| async move { curl(argv, account.header(), &Running::default()).await };
    let library = sync::library_of(&ask(sync::get_photo(&account.address, photo)).await)?;
    let missing = || Failure::Said(format!("{NUMA_HOME} did not list that photograph"));
    let from = sync::libraries(&ask(sync::list_libraries(&account.address)).await)?.into_iter().find(|from| from.id == library).ok_or_else(missing)?;
    let all = sync::photos(&ask(sync::list_photos(&account.address, &library, 0)).await)?;
    let path = all.iter().find(|theirs| theirs.id == photo).ok_or_else(missing)?.path.clone();
    Ok((from, path, all))
}

pub(super) fn keep_in_step(state: &App) {

    offer(state);
    adopt(state, true);
    glib::timeout_add_seconds_local(
        EVERY,
        glib::clone!(
            #[strong] state,
            move || {

                adopt(&state, false);
                let how = match allowed(&state) {
                    true => How::Quiet,
                    false => {
                        QUIET.with(|quiet| quiet.borrow().as_ref().map(Cancel::stop));
                        How::Light
                    }
                };
                let kept: Vec<Library> = state.catalog.libraries().unwrap_or_default().into_iter().filter(|library| is_linked(&state, library.id)).collect();
                if !kept.is_empty() {
                    let at = TURN.with(|turn| turn.replace(turn.get() + 1)) % kept.len();
                    in_step(&state, kept[at].clone(), how);
                }
                glib::ControlFlow::Continue
            }
        ),
    );
}

pub(super) fn pace(state: &App) -> Pace {
    state.catalog.recall(PACE).unwrap_or_default()
}

pub(super) fn set_pace(state: &App, pace: Pace) {
    state.catalog.remember(PACE, &pace);
}

fn allowed(state: &App) -> bool {
    let hour = glib::DateTime::now_local().map_or(12, |now| now.hour());
    pace(state).allows(hour, gio::NetworkMonitor::default().is_network_metered())
}

fn paced(state: &App) -> How {
    match allowed(state) {
        true => How::Quiet,
        false => How::Light,
    }
}

pub(super) fn opened(state: &App) {
    if let Some(library) = offer(state) {
        in_step(state, library, paced(state));
    }
}

pub(super) fn edited(state: &App, library: i64) {
    let library = state.libraries.all.borrow().iter().find(|it| it.id == library).cloned();
    if let Some(library) = library.filter(|library| is_linked(state, library.id)) {
        if !ROUND.with(Cell::get) {
            in_step(state, library, paced(state));
        }
    }
}

pub(super) fn offer(state: &App) -> Option<Library> {
    let library = state.libraries.current.borrow().clone();
    let window = state.stack.root().and_downcast::<adw::ApplicationWindow>();
    let action = |name: &str| window.as_ref().and_then(|window| window.lookup_action(name)).and_downcast::<gio::SimpleAction>();
    let commands = library.as_ref().map(|library| home_pair::commands(state, library)).unwrap_or_default();
    for (name, on) in [("home-sync-here", commands.here), ("home-sync-home", commands.home), ("home-stop-sync", commands.stop)] {
        if let Some(action) = action(name) {
            action.set_enabled(on);
        }
    }
    let kept = library.filter(|library| is_linked(state, library.id));
    if let Some(action) = action("home-sync") {
        action.set_enabled(kept.is_some());
    }
    kept
}

pub(super) fn keep_here(state: &App, library: &Library, on: bool) {
    let Some(link) = linked(state, library.id) else { return };
    if link.keep_here != on {
        keep(state, library.id, &Link { keep_here: on, ..link });
    }
}

pub(super) fn sync_now(state: &App, library: Library) {
    let state = state.clone();
    glib::spawn_future_local(async move {
        round_over().await;
        in_step(&state, library, How::ByHand);
    });
}

pub(super) fn install(state: &App, window: &adw::ApplicationWindow) {
    let sync_now = gio::SimpleAction::new("home-sync", None);
    sync_now.set_enabled(false);
    sync_now.connect_activate(glib::clone!(
        #[strong] state,
        move |_, _| {
            if let Some(library) = state.libraries.current.borrow().clone() {
                in_step(&state, library, How::ByHand);
            }
        }
    ));
    window.add_action(&sync_now);
    for name in ["home-sync-here", "home-sync-home", "home-stop-sync"] {
        let action = gio::SimpleAction::new(name, None);
        action.set_enabled(false);
        action.connect_activate(glib::clone!(
            #[strong] state,
            move |action, _| {
                let Some(library) = state.libraries.current.borrow().clone() else { return };
                match action.name().as_str() {
                    "home-stop-sync" => home_pair::stop_syncing(&state, vec![library]),
                    _ => home_pair::start_syncing(&state, vec![library]),
                }
            }
        ));
        window.add_action(&action);
    }
}

pub(super) fn keep_at_home(state: &App, libraries: &[Library]) {
    let account = account(state);
    let new: Vec<&Library> = libraries.iter().filter(|library| !is_linked(state, library.id)).collect();
    for library in &new {
        keep(state, library.id, &Link { address: account.address.clone(), name: library.label(), way: Some(Way::Up), ..Link::default() });
    }
    let Some(first) = new.first() else { return };
    match allowed(state) {
        true => in_step(state, (*first).clone(), How::Quiet),
        false => state.toast(&format!("{} goes to {NUMA_HOME} {}", first.label(), when(pace(state)))),
    }
}

fn when(pace: Pace) -> &'static str {
    match pace.night {
        true => "tonight",
        false => "when this computer is off a metered connection",
    }
}

fn trashed_here(state: &App, library: i64) -> BTreeMap<String, i64> {
    state.catalog.library_setting(library, sync::TRASHED).and_then(|json| serde_json::from_str(&json).ok()).unwrap_or_default()
}

fn keep_trashed(state: &App, library: i64, trashed: &BTreeMap<String, i64>) {
    let json = (!trashed.is_empty()).then(|| serde_json::to_string(trashed).unwrap_or_default());
    if let Err(err) = state.catalog.set_library_setting(library, sync::TRASHED, json.as_deref()) {
        log::warn!("Numa Home: {err}");
    }
}

pub(super) fn note_trashed(state: &App, photos: &[(i64, PathBuf)]) {
    let now = glib::real_time() / 1_000_000;
    let libraries = state.libraries.all.borrow().clone();
    for library in libraries.iter().filter(|library| is_linked(state, library.id)) {
        let mut trashed = trashed_here(state, library.id);
        let before = trashed.len();
        for (_, file) in photos.iter().filter(|(id, _)| numa::io::catalog::library_of(*id) == library.id) {
            if let Ok(path) = file.strip_prefix(&library.path) {
                trashed.insert(path.to_string_lossy().into_owned(), now);
            }
        }
        if trashed.len() != before {
            keep_trashed(state, library.id, &trashed);
        }
    }
}

fn forget_trashed(state: &App, library: i64, paths: &[String]) {
    let mut trashed = trashed_here(state, library);
    let before = trashed.len();
    for path in paths {
        trashed.remove(path);
    }
    if trashed.len() != before {
        keep_trashed(state, library, &trashed);
    }
}

#[derive(Clone, Copy, PartialEq)]
enum How {
    Import,
    ByHand,
    Quiet,
    Light,
    Look,
    Glance,
}

impl How {
    fn looks(self) -> bool {
        matches!(self, How::Look | How::Glance)
    }

    fn holds(self, step: &Step) -> bool {
        match self {
            How::Look | How::Glance => !matches!(step, Step::Arrive(_)),
            How::Light => matches!(step, Step::Send(_)),
            _ => false,
        }
    }
}

fn in_step(state: &App, library: Library, how: How) {
    let Some(mut link) = linked(state, library.id) else { return };
    let account = account(state);
    if !account.signed_in() || !api::same_server(&link.address, &account.address) {
        if how == How::ByHand {
            state.toast(&sign_in_first(&account, &api::host(&link.address)));
        }
        return;
    }
    if ROUND.with(|round| round.replace(true)) {
        if how == How::ByHand {
            state.toast(&format!("{NUMA_HOME} is busy with a library; try again in a moment"));
        }
        return;
    }

    link.way = link.way.or_else(|| way_of(state, library.id));
    let synced = is_synced(state, library.id);
    let state = state.clone();
    glib::spawn_future_local(async move {
        if link.library.is_empty() {
            match sync::made_library(&curl(sync::new_library(&account.address, &link.name), account.header(), &Running::default()).await) {
                Ok(made) => {
                    link.library = made.id;
                    keep(&state, library.id, &link);
                }
                Err(failure) => return let_go(&state, failure, &account, Some(&library)),
            }
        }
        let asked = sync::list_changes(&account.address, &link.library, sync::listed_after(&link), synced && !how.looks());
        match sync::photos(&curl(asked, account.header(), &Running::default()).await) {
            Ok(remote) => {
                let since = sync::next_since(link.since, &remote);
                round(&state, library, link, remote, since, how, (Vec::new(), None));
            }
            Err(failure) => {
                if !matches!(failure, Failure::Said(_)) {
                    say_state(&state, library.id, |shown| shown.waits = true);
                }
                let_go(&state, failure, &account, (how == How::ByHand).then_some(&library))
            }
        }
    });
}

fn let_go(state: &App, failure: Failure, account: &Account, making: Option<&Library>) {
    ROUND.with(|round| round.set(false));
    match failure {
        Failure::SignIn(_) => {
            forget_token(state);
            state.toast(&signed_out());
        }
        Failure::Said(sentence) if making.is_some_and(|library| linked(state, library.id).is_some_and(|link| link.library.is_empty())) => {
            let library = making.expect("checked above");
            unlink(state, library.id);
            reload_libraries(state);
            state.toast(&format!("{} is not kept at {NUMA_HOME}: {sentence}", library.label()));
        }
        failure if making.is_some() => state.toast(&said(failure, account)),
        failure => log::info!("Numa Home: {}", said(failure, account)),
    }
}

fn local_photos(state: &App, library: &Library) -> Vec<(i64, Local)> {

    let photos = state.catalog.photos(library.id, &Filter::default()).unwrap_or_default();
    let ids: Vec<i64> = photos.iter().map(|photo| photo.id).collect();
    let mut albums = state.catalog.albums_of(&ids).unwrap_or_default();
    let changed = state.catalog.changed_times(library.id).unwrap_or_default();
    photos
        .into_iter()
        .map(|photo| {
            let path = photo.path.strip_prefix(&library.path).unwrap_or(&photo.path).to_string_lossy().to_string();

            let edits = state.catalog.own_edits_json(photo.id).ok().flatten();
            let local = Local {
                path,
                edits,
                rating: photo.rating,
                flag: sync::flag_name(photo.flag).to_string(),
                albums: albums.remove(&photo.id).unwrap_or_default(),
                changed: changed.get(&photo.id).copied().unwrap_or(0),
            };
            (photo.id, local)
        })
        .collect()
}

#[derive(Default)]
struct Done {

    came: usize,
    changed: usize,
    went: usize,

    pictures: usize,
    brought: usize,

    trashed: usize,
    deleted: usize,
    failures: Vec<String>,

    cut: bool,

    waits: bool,

    hold: bool,
}

fn goes_up(step: &Step) -> bool {
    matches!(step, Step::Send(_) | Step::Delete { .. } | Step::Restore(_)) || matches!(step, Step::Both { up, .. } if up.any())
}

fn round(state: &App, library: Library, mut link: Link, remote: Vec<Remote>, since: i64, how: How, then: (Vec<String>, Option<String>)) {
    let local: Vec<Local> = local_photos(state, &library).into_iter().map(|(_, photo)| photo).collect();
    let mut steps = sync::plan(&link, &remote, &local, &trashed_here(state, library.id));

    let whole = !how.looks() || !steps.iter().any(|step| how.holds(step));
    let held = steps.iter().filter(|step| how.holds(step) && goes_up(step)).count();
    steps.retain(|step| !how.holds(step));

    let to_send = steps.iter().filter(|step| goes_up(step)).count();

    let keep_here = is_synced(state, library.id) && !how.looks() && how != How::Light;
    let to_bring = if keep_here { state.catalog.at_home(library.id).map_or(0, |at_home| at_home.len()) } else { 0 };
    let cancel = Cancel::default();
    let running = Running::default();
    let progress = (!steps.is_empty() || to_bring > 0).then(|| match how {
        How::Quiet | How::Light | How::Look | How::Glance => {
            QUIET.with(|quiet| quiet.replace(Some(cancel.clone())));
            quiet(state, &library)
        }
        _ => {
            let (toast, text, bar) = progress_toast(state, &cancel);
            toast.connect_button_clicked(glib::clone!(
                #[strong] running,
                move |_| {
                    if let Some(process) = running.borrow().as_ref() {
                        process.force_exit();
                    }
                }
            ));
            (toast, text, Some(bar))
        }
    });
    if progress.is_some() {
        say_state(state, library.id, |shown| shown.syncing = Some((0, steps.len().max(to_bring))));
    }
    let state = state.clone();
    glib::spawn_future_local(async move {
        let shown = progress.as_ref().map(|(_, text, bar)| (text.clone(), bar.clone()));
        let mut done = carry_out(&state, &library, &mut link, steps, (shown, how), &cancel, &running).await;
        if keep_here && !done.cut {
            let say = |at: usize, of: usize| {
                if let Some((_, text, bar)) = &progress {
                    text.set_text(&format!("Bringing {} of {of} from {NUMA_HOME}", at + 1));
                    if let Some(bar) = bar {
                        bar.set_fraction(at as f64 / of.max(1) as f64);
                    }
                }
                say_state(&state, library.id, |shown| shown.syncing = Some((at, of)));
            };
            match home_fetch::bring_all(&state, &library, &cancel, say).await {
                Ok(brought) => done.brought = brought,
                Err(failure) => {
                    stop(&state, failure, &account(&state), &cancel, &mut done);
                }
            }
        }
        let finished = !done.cut && done.failures.is_empty();

        link.everything |= finished && how != How::Glance;
        if how == How::Import || (whole && finished && !done.hold) {
            link.since = since;
            if how != How::Import {
                link.rules = sync::RULES;
            }
        }

        if let Some(stored) = linked(&state, library.id) {
            (link.keep_here, link.way) = (stored.keep_here, stored.way.or(link.way));
        }
        keep(&state, library.id, &link);
        ROUND.with(|round| round.set(false));
        QUIET.with(|quiet| quiet.replace(None));
        let left = held + to_send.saturating_sub(done.went + done.deleted);
        say_state(&state, library.id, |shown| *shown = Shown { syncing: None, to_send: if finished { held } else { left }, waits: done.waits });
        if let Some((toast, _, _)) = progress {
            toast.dismiss();
        }
        let moved = done.came + done.changed + done.pictures + done.brought + done.trashed;
        if moved > 0 && state.libraries.current.borrow().as_ref().is_some_and(|open| open.id == library.id) {
            reload_grid(&state);
        }
        for failure in &done.failures {
            log::warn!("Numa Home: {failure}");
        }
        log::info!(
            "Numa Home: {} new there, {} changed there, {} went up, {} thumbnails, {} brought, {} to the trash here, {} to Home's trash",
            done.came,
            done.changed,
            done.went,
            done.pictures,
            done.brought,
            done.trashed,
            done.deleted
        );
        if let Some(said) = round_said(&done, how, &cancel, to_send, &library) {
            state.toast(&said);
        }
        let (fetch, open) = then;
        if !fetch.is_empty() || open.is_some() {
            home_fetch::fetch_paths(&state, library, fetch, open);
        }
    });
}

fn round_said(done: &Done, how: How, cancel: &Cancel, to_send: usize, library: &Library) -> Option<String> {
    let loud = matches!(how, How::Import | How::ByHand);
    let to_go = match to_send.saturating_sub(done.went + done.deleted) {
        0 => String::new(),
        left => format!("; {left} to send home"),
    };
    match (done, loud) {
        (Done { cut: true, came, went, .. }, true) if cancel.stopped() => Some(format!("Stopped after {}{to_go}", came + went)),
        (Done { failures, came, went, .. }, true) if !failures.is_empty() => {
            Some(format!("{} kept at {NUMA_HOME}, {} failed: {}{to_go}", came + went, failures.len(), failures[0]))
        }

        _ if how == How::Import => None,
        (Done { came: 0, went: 0, changed: 0, brought: 0, trashed: 0, deleted: 0, .. }, true) => Some(format!("Kept at {NUMA_HOME} \u{b7} just now")),
        (Done { came, changed, went, brought, trashed, deleted, .. }, true) => {
            let moved: Vec<String> = [
                (*came, "new"),
                (*changed, "changed there"),
                (*went, "sent home"),
                (*brought, "brought"),
                (*trashed, "to the trash here"),
                (*deleted, "to Numa Home's trash"),
            ]
            .into_iter()
            .filter(|(n, _)| *n > 0)
            .map(|(n, said)| format!("{n} {said}"))
            .collect();
            Some(format!("Kept at {NUMA_HOME} \u{b7} just now \u{b7} {}", moved.join(" \u{b7} ")))
        }

        (Done { came: 0, .. }, false) => None,
        _ if how.looks() => None,
        (Done { came, .. }, false) => Some(format!("{} new from {NUMA_HOME} in {}", photographs(*came), library.label())),
    }
}

fn quiet(state: &App, library: &Library) -> (adw::Toast, gtk::Label, Option<gtk::ProgressBar>) {
    let (toast, text) = quiet_toast(state, &format!("{} with {NUMA_HOME}", library.label()));
    (toast, text, None)
}

struct Saying<'a> {
    state: &'a App,
    library: i64,
    shown: Option<(gtk::Label, Option<gtk::ProgressBar>)>,
    total: usize,
}

impl Saying<'_> {
    fn say(&self, at: usize, text: String) {
        if let Some((label, bar)) = &self.shown {
            label.set_text(&text);
            if let Some(bar) = bar {
                bar.set_fraction(at as f64 / self.total.max(1) as f64);
            }
        }
        say_state(self.state, self.library, |shown| shown.syncing = Some((at, self.total)));
    }
}

async fn carry_out(
    state: &App,
    library: &Library,
    link: &mut Link,
    steps: Vec<Step>,
    (shown, how): (Option<(gtk::Label, Option<gtk::ProgressBar>)>, How),
    cancel: &Cancel,
    running: &Running,
) -> Done {
    let account = account(state);
    let mut done = Done::default();
    let saying = Saying { state, library: library.id, shown, total: steps.len() };
    let (first, rest): (Vec<Step>, Vec<Step>) = steps.into_iter().partition(|step| matches!(step, Step::Arrive(_) | Step::PutBack(_)));
    let back: Vec<Remote> = first.iter().filter_map(|step| if let Step::PutBack(theirs) = step { Some(theirs.clone()) } else { None }).collect();
    let came_back = from_trash(library, &back).await;
    let arrived: Vec<Remote> = first.into_iter().filter_map(|step| if let Step::Arrive(theirs) | Step::PutBack(theirs) = step { Some(theirs) } else { None }).collect();
    let later = arrive(state, library, link, &arrived, &mut done);
    forget_trashed(state, library.id, &back.iter().map(|theirs| theirs.path.clone()).collect::<Vec<_>>());
    let here: HashMap<String, (i64, Local)> = local_photos(state, library).into_iter().map(|(id, photo)| (photo.path.clone(), (id, photo))).collect();
    settle_arrivals(state, library, &arrived, &came_back, &here).await;
    let (deletes, rest): (Vec<Step>, Vec<Step>) = rest.into_iter().partition(|step| matches!(step, Step::Delete { .. }));
    if let Err(failure) = delete_there(state, &account, library, link, deletes, &mut done, running).await {
        stop(state, failure, &account, cancel, &mut done);
    }
    let folder = scratch();
    let albums_there = RefCell::new(None);
    let (brought, pulled) = (arrived.len(), later.len());
    let mut moved: HashSet<String> = HashSet::new();
    let ctx = Ctx { state, account: &account, library, folder: &folder, albums_there: &albums_there, running, light: how == How::Light };
    for (at, step) in later.into_iter().chain(rest).enumerate() {
        if done.cut {
            break;
        }
        let at = brought + done.deleted + at.saturating_sub(pulled);
        saying.say(at, match &step {
            Step::Send(_) => format!("Sending {} of {} home", at + 1, saying.total),
            _ => format!("Edits and marks with {NUMA_HOME}"),
        });
        let result = one(&ctx, link, &here, step, &mut done, &mut moved).await;
        if let Err(failure) = result {
            if stop(state, failure, &account, cancel, &mut done) {
                break;
            }
        }
    }
    let _ = std::fs::remove_dir_all(&folder);
    if !done.cut {
        let saying = |at: usize, of: usize| saying.say(saying.total, format!("{} of {} photos from {NUMA_HOME}", places::grouped(at as i64), places::grouped(of as i64)));
        match pictures(state, &account, library, (&here, how == How::Glance), &moved, running, saying).await {
            Ok(came) => done.pictures = came,
            Err(failure) => {
                stop(state, failure, &account, cancel, &mut done);
            }
        }
    }
    done
}

struct Ctx<'a> {
    state: &'a App,
    account: &'a Account,
    library: &'a Library,

    folder: &'a Path,

    albums_there: &'a RefCell<Option<Vec<(String, String)>>>,
    running: &'a Running,

    light: bool,
}

async fn one(ctx: &Ctx<'_>, link: &mut Link, here: &HashMap<String, (i64, Local)>, step: Step, done: &mut Done, moved: &mut HashSet<String>) -> Result<(), Failure> {
    match step {
        Step::Trash { path, id } => {
            trash_here(ctx.state, ctx.library, here.get(&path).map(|(photo, _)| *photo), &path).await?;
            link.photos.remove(&path);
            link.gone.insert(path, Gone { id, here: false, restored: false });
            done.trashed += 1;
        }
        Step::Restore(path) => {
            let back = sync::restored(&curl(sync::restore(&ctx.account.address, &link.library, std::slice::from_ref(&path)), ctx.account.header(), ctx.running).await);
            match back {
                Ok(back) if back.iter().any(|(_, at)| *at == path) => {
                    link.gone.entry(path).or_default().restored = true;
                    done.hold = true;
                }

                Ok(_) | Err(Failure::Said(_)) if !ctx.light => {
                    link.gone.remove(&path);
                    let Some((photo, ours)) = here.get(&path) else { return Ok(()) };
                    let kept = send(ctx, (*photo, ours), &link.library).await?;
                    done.went += 1;
                    moved.insert(path.clone());
                    link.photos.insert(path, kept);
                }
                Ok(_) | Err(Failure::Said(_)) => {}
                Err(failure) => return Err(failure),
            }
        }
        Step::Both { path, id, theirs, down, up, albums } => {
            let Some((photo, ours)) = here.get(&path) else { return Ok(()) };
            let kept = link.photos.get(&path).cloned();
            let theirs_albums = theirs.as_ref().and_then(|theirs| theirs.albums.clone()).or_else(|| kept.as_ref().and_then(|kept| kept.albums.clone())).unwrap_or_default();
            let sides = Sides { theirs: theirs.as_ref(), kept: kept.as_ref(), albums: albums.as_deref(), theirs_albums: &theirs_albums };
            let now = both(ctx, (*photo, ours), &id, sides, (down, up)).await?;
            done.changed += (down.any() && now.updated > 0) as usize;
            done.went += up.any() as usize;
            if now.edits != kept.as_ref().map(|kept| kept.edits.clone()).unwrap_or_default() || down.edits {
                moved.insert(path.clone());
            }
            link.gone.remove(&path);
            link.photos.insert(path, now);
        }
        Step::Send(path) => {
            let Some((photo, ours)) = here.get(&path) else { return Ok(()) };
            let kept = send(ctx, (*photo, ours), &link.library).await?;
            done.went += 1;
            moved.insert(path.clone());
            link.photos.insert(path, kept);
        }
        Step::Arrive(_) | Step::PutBack(_) | Step::Delete { .. } => {}
    }
    Ok(())
}

async fn trash_here(state: &App, library: &Library, photo: Option<i64>, path: &str) -> Result<(), Failure> {
    let Some(photo) = photo else { return Ok(()) };
    let at_home = state.catalog.at_home(library.id).unwrap_or_default().contains_key(&photo);
    if let (false, Some(file)) = (at_home, sync::inside(&library.path, path).filter(|file| file.exists())) {
        let shown = file.display().to_string();
        let trashed = gio::spawn_blocking(move || gio::File::for_path(&file).trash(gio::Cancellable::NONE).map_err(|err| err.to_string())).await;
        trashed.unwrap_or_else(|_| Err("the trash did not answer".into())).map_err(|err| Failure::Said(format!("{shown}: {err}")))?;
    }
    state.catalog.remove_photo(photo).map_err(Failure::Said)
}

async fn delete_there(state: &App, account: &Account, library: &Library, link: &mut Link, deletes: Vec<Step>, done: &mut Done, running: &Running) -> Result<(), Failure> {
    let gone: Vec<(String, String)> = deletes.into_iter().filter_map(|step| if let Step::Delete { path, id } = step { Some((path, id)) } else { None }).collect();
    if gone.is_empty() {
        return Ok(());
    }
    let ids: Vec<String> = gone.iter().map(|(_, id)| id.clone()).collect();
    let answer = api::taken(&curl(sync::delete_photos(&account.address, &ids), account.header(), running).await);
    let refused = match answer {
        Ok(()) => None,
        Err(Failure::Said(sentence)) => Some(sentence),
        Err(failure) => return Err(failure),
    };
    for (path, id) in &gone {
        link.photos.remove(path);
        if refused.is_none() {
            link.gone.insert(path.clone(), Gone { id: id.clone(), here: true, restored: false });
        }
    }
    match refused {
        None => done.deleted += gone.len(),
        Some(sentence) => done.failures.push(format!("{} stay at {NUMA_HOME}: {sentence}", photographs(gone.len()))),
    }
    forget_trashed(state, library.id, &gone.into_iter().map(|(path, _)| path).collect::<Vec<_>>());
    Ok(())
}

async fn from_trash(library: &Library, back: &[Remote]) -> HashSet<String> {
    let wanted: HashMap<PathBuf, String> = back.iter().filter_map(|theirs| Some((sync::inside(&library.path, &theirs.path)?, theirs.path.clone()))).collect();
    if wanted.is_empty() {
        return HashSet::new();
    }
    let found = gio::spawn_blocking(move || {
        let trash = gio::File::for_uri("trash:///");
        let mut came = HashSet::new();
        let Ok(items) = trash.enumerate_children("standard::name,trash::orig-path", gio::FileQueryInfoFlags::NONE, gio::Cancellable::NONE) else { return came };
        for info in items.flatten() {
            let Some(orig) = info.attribute_byte_string("trash::orig-path").map(|orig| PathBuf::from(orig.as_str())) else { continue };
            let Some(path) = wanted.get(&orig).filter(|path| !came.contains(*path) && !orig.exists()) else { continue };
            let moved = trash.child(info.name()).move_(&gio::File::for_path(&orig), gio::FileCopyFlags::NONE, gio::Cancellable::NONE, None);
            match moved {
                Ok(()) => drop(came.insert(path.clone())),
                Err(err) => log::info!("Numa Home: {}: {err}", orig.display()),
            }
        }
        came
    });
    found.await.unwrap_or_default()
}

async fn settle_arrivals(state: &App, library: &Library, arrived: &[Remote], came_back: &HashSet<String>, here: &HashMap<String, (i64, Local)>) {
    for theirs in arrived {
        let Some((photo, _)) = here.get(&theirs.path) else { continue };
        if let Some(albums) = theirs.albums.as_deref().filter(|albums| !albums.is_empty()) {
            if let Err(failure) = albums_here(state, *photo, &[], albums) {
                log::warn!("Numa Home: {}", said(failure, &account(state)));
            }
        }
        let Some(file) = sync::inside(&library.path, &theirs.path).filter(|_| came_back.contains(&theirs.path)) else { continue };
        let read = gio::spawn_blocking(move || {
            let mtime = std::fs::metadata(&file).and_then(|meta| meta.modified()).ok();
            let mtime = mtime.and_then(|at| at.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |at| at.as_secs() as i64);
            (mtime, numa::io::exif::stamp(&file))
        });
        let (mtime, stamp) = read.await.unwrap_or_default();
        if let Err(err) = state.catalog.arrived(*photo, mtime, &stamp) {
            log::warn!("Numa Home: {err}");
        }
        state.grid.at_home.borrow_mut().remove(photo);
    }
}

fn arrive(state: &App, library: &Library, link: &mut Link, arrived: &[Remote], done: &mut Done) -> Vec<Step> {
    if arrived.is_empty() {
        return Vec::new();
    }
    if let Err(err) = state.catalog.add_at_home(library.id, &arrived.iter().map(Remote::at_home).collect::<Vec<_>>()) {
        done.failures.push(err);
        return Vec::new();
    }
    done.came += arrived.len();
    let edits = sync::Parts { edits: true, ..sync::Parts::default() };
    arrived
        .iter()
        .filter_map(|theirs| {
            link.photos.insert(theirs.path.clone(), theirs.kept());
            link.gone.remove(&theirs.path);
            let down = Step::Both {
                path: theirs.path.clone(),
                id: theirs.id.clone(),
                theirs: Some(theirs.clone()),
                down: edits,
                up: sync::Parts::default(),
                albums: theirs.albums.clone(),
            };
            theirs.edited.then_some(down)
        })
        .collect()
}

async fn pictures(
    state: &App,
    account: &Account,
    library: &Library,
    (here, covers_only): (&HashMap<String, (i64, Local)>, bool),
    moved: &HashSet<String>,
    running: &Running,
    say: impl Fn(usize, usize),
) -> Result<usize, Failure> {
    let at_home = state.catalog.at_home(library.id).unwrap_or_default();
    let covers: HashSet<PathBuf> = match covers_only {
        true => state.catalog.glance(library.id, None, None).map(|glance| glance.covers.into_iter().take(places::FAN_SIZE).map(|(path, ..)| path).collect()).unwrap_or_default(),
        false => HashSet::new(),
    };
    let ids: Vec<String> = here
        .values()
        .filter(|(_, ours)| !covers_only || covers.contains(&library.path.join(&ours.path)))
        .filter_map(|(id, ours)| {
            let (server, _) = at_home.get(id)?;
            (moved.contains(&ours.path) || !numa::io::catalog::home_thumb(&library.path, server).exists()).then(|| server.clone())
        })
        .collect();
    if ids.is_empty() {
        return Ok(0);
    }
    home_fetch::thumbnails(account, &library.path, ids, running, say).await
}

fn stop(state: &App, failure: Failure, account: &Account, cancel: &Cancel, done: &mut Done) -> bool {
    if cancel.stopped() {
        done.cut = true;
        return true;
    }
    let last = !matches!(failure, Failure::Said(_));
    done.waits |= last;
    if let Failure::SignIn(_) = failure {
        forget_token(state);
    }
    done.failures.push(said(failure, account));
    done.cut |= last;
    last
}

pub(super) async fn download(account: &Account, root: &Path, theirs: &Remote, running: &Running) -> Result<(), Failure> {
    let to = sync::inside(root, &theirs.path).ok_or_else(|| Failure::Said(format!("{} is not a path inside the library", theirs.path)))?;
    if to.exists() {
        return Ok(());
    }
    let unwritten = |err: std::io::Error| Failure::Said(format!("{}: {err}", to.display()));
    std::fs::create_dir_all(to.parent().unwrap_or(root)).map_err(unwritten)?;
    let mut part = to.clone().into_os_string();
    part.push(".part");
    let part = PathBuf::from(part);
    let answer = api::taken(&curl(sync::original(&account.address, &theirs.id, &part), account.header(), running).await);
    let size = std::fs::metadata(&part).map_or(0, |meta| meta.len());
    if let Err(failure) = answer.and_then(|()| match theirs.size == 0 || size == theirs.size {
        true => Ok(()),
        false => Err(Failure::Said(format!("{} arrived short", theirs.path))),
    }) {
        let _ = std::fs::remove_file(&part);
        return Err(failure);
    }
    std::fs::rename(&part, &to).map_err(unwritten)?;
    let at = std::time::UNIX_EPOCH + std::time::Duration::from_secs(theirs.mtime.max(0) as u64);
    if let Err(err) = std::fs::File::options().write(true).open(&to).and_then(|file| file.set_modified(at)) {
        log::info!("Numa Home: {}: {err}", to.display());
    }
    Ok(())
}

struct Sides<'a> {
    theirs: Option<&'a Remote>,
    kept: Option<&'a Kept>,
    albums: Option<&'a [String]>,
    theirs_albums: &'a [String],
}

async fn both(ctx: &Ctx<'_>, (photo, ours): (i64, &Local), id: &str, sides: Sides<'_>, (down, up): (sync::Parts, sync::Parts)) -> Result<Kept, Failure> {
    let (state, account, running) = (ctx.state, ctx.account, ctx.running);
    let file = ctx.library.path.join(&ours.path);
    let mut now = ours.clone();
    let mut at = sides.theirs.map_or(0, |theirs| theirs.updated);
    let mut up_edits = up.edits;
    if let (true, Some(theirs)) = (down.edits, sides.theirs) {
        let (stack, given) = sync::edits(&curl(sync::get_edits(&account.address, &theirs.id), account.header(), running).await)?;
        at = at.max(given);
        let settled = match sides.kept {
            Some(kept) => sync::settle(kept, ours, stack.as_deref(), at),
            None => Settled::Down,
        };
        match settled {
            Settled::Same => up_edits = false,
            Settled::Down | Settled::DownKeepingOurs => {
                now.edits = put_edits(state, file.clone(), photo, stack)?;
                if settled == Settled::DownKeepingOurs {
                    let device = sync::device_label(&account.name, &glib::host_name());
                    keep_aside(state, photo, &file, ours.edits.as_deref(), &sync::aside(false, "", &device))?;
                }
                up_edits = false;
            }
            Settled::Up => up_edits = true,
            Settled::UpKeepingTheirs => {
                keep_aside(state, photo, &file, stack.as_deref(), &sync::aside(true, &moment(at), ""))?;
                up_edits = true;
            }
        }
    }
    if let (true, Some(theirs)) = (down.marks, sides.theirs) {
        let (rating, flag) = (theirs.rating.min(5), sync::flag(&theirs.flag));
        state.catalog.set_ratings(&[photo], rating).map_err(Failure::Said)?;
        state.catalog.set_flags(&[photo], flag).map_err(Failure::Said)?;
        (now.rating, now.flag) = (rating, sync::flag_name(flag).to_string());
    }
    if let (true, Some(albums)) = (down.albums, sides.albums) {
        albums_here(state, photo, &ours.albums, albums)?;
        now.albums = albums.to_vec();
    }
    if up_edits {
        let stack = ctx.folder.join("edits.json");
        std::fs::write(&stack, ours.edits.as_deref().unwrap_or_default()).map_err(|err| Failure::Said(err.to_string()))?;
        at = at.max(sync::updated(&curl(api::edits(&account.address, id, &stack), account.header(), running).await)?);
    }
    if up.marks {
        at = at.max(sync::updated(&curl(sync::put_marks(&account.address, id, ours.rating, &ours.flag), account.header(), running).await)?);
    }
    if let (true, Some(albums)) = (up.albums, sides.albums) {
        at = at.max(albums_there(ctx, id, sides.theirs_albums, albums).await?);
    }
    let mut kept = now.kept(id, at);
    kept.albums = sides.albums.map(<[String]>::to_vec).or_else(|| sides.kept.and_then(|kept| kept.albums.clone()));
    Ok(kept)
}

fn moment(at: i64) -> String {
    const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    match glib::DateTime::from_unix_local(at) {
        Ok(when) => format!("{} {} {:02}:{:02}", when.day_of_month(), MONTHS[(when.month() as usize).clamp(1, 12) - 1], when.hour(), when.minute()),
        Err(_) => String::new(),
    }
}

fn keep_aside(state: &App, photo: i64, file: &Path, stack: Option<&str>, name: &str) -> Result<(), Failure> {
    let file = file.to_string_lossy().to_string();
    let mut other = match stack {
        Some(json) => serde_json::from_str::<Document>(json).map_err(|_| Failure::Said(format!("The edits of {file} could not be read")))?,
        None => Document::new(file.clone()),
    };
    other.source.path = file;
    other.moment = None;
    let kept = state.catalog.keep_aside(photo, name, &other).map_err(Failure::Said)?;
    log::info!("Numa Home: the other edit kept as “{kept}”");
    Ok(())
}

fn albums_here(state: &App, photo: i64, from: &[String], to: &[String]) -> Result<(), Failure> {
    let mut known = state.catalog.albums().map_err(Failure::Said)?;
    let key = |known: &[(String, String)], name: &str| known.iter().find(|(_, it)| it.eq_ignore_ascii_case(name)).map(|(key, _)| key.clone());
    let has = |names: &[String], name: &str| names.iter().any(|it| it.eq_ignore_ascii_case(name));
    for name in to.iter().filter(|name| !has(from, name)) {
        let key = match key(&known, name) {
            Some(key) => key,
            None => {
                let made = state.catalog.create_album(name).map_err(Failure::Said)?;
                known.push((made.clone(), name.clone()));
                made
            }
        };
        state.catalog.set_in_album(&key, &[photo], true).map_err(Failure::Said)?;
    }
    for name in from.iter().filter(|name| !has(to, name)) {
        if let Some(key) = key(&known, name) {
            state.catalog.set_in_album(&key, &[photo], false).map_err(Failure::Said)?;
        }
    }
    Ok(())
}

async fn albums_there(ctx: &Ctx<'_>, id: &str, from: &[String], to: &[String]) -> Result<i64, Failure> {
    let (account, running) = (ctx.account, ctx.running);
    if ctx.albums_there.borrow().is_none() {
        let listed = sync::albums_at_home(&curl(sync::list_albums(&account.address), account.header(), running).await)?;
        ctx.albums_there.replace(Some(listed));
    }
    let key = |name: &str| ctx.albums_there.borrow().iter().flatten().find(|(_, it)| it.eq_ignore_ascii_case(name)).map(|(key, _)| key.clone());
    let has = |names: &[String], name: &str| names.iter().any(|it| it.eq_ignore_ascii_case(name));
    let ids = [id.to_string()];
    let mut at = 0;
    for name in to.iter().filter(|name| !has(from, name)) {
        let known = key(name);
        let out = curl(sync::put_in_album(&account.address, &ids, known.as_deref().ok_or(name.as_str()), true), account.header(), running).await;
        let made = sync::album_key(&out)?;
        if known.is_none() {
            ctx.albums_there.borrow_mut().get_or_insert_with(Vec::new).push((made, name.clone()));
        }
        at = at.max(sync::updated(&out)?);
    }
    for name in from.iter().filter(|name| !has(to, name)) {
        if let Some(known) = key(name) {
            at = at.max(sync::updated(&curl(sync::put_in_album(&account.address, &ids, Ok(&known), false), account.header(), running).await)?);
        }
    }
    Ok(at)
}

fn put_edits(state: &App, file: PathBuf, photo: i64, stack: Option<String>) -> Result<Option<String>, Failure> {
    let file = file.to_string_lossy().to_string();
    let mut document = match &stack {
        Some(json) => serde_json::from_str::<Document>(json).map_err(|_| Failure::Said(format!("The edits of {file} could not be read")))?,
        None => Document::new(file.clone()),
    };

    document.source.path = file;
    state.catalog.save_own_edits(photo, &document).map_err(Failure::Said)?;
    state.catalog.own_edits_json(photo).map_err(Failure::Said)
}

async fn send(ctx: &Ctx<'_>, (photo, ours): (i64, &Local), library: &str) -> Result<Kept, Failure> {
    let (account, running) = (ctx.account, ctx.running);
    let file = ctx.library.path.join(&ours.path);
    let dir = Path::new(&ours.path).parent().map(|dir| dir.to_string_lossy().to_string()).unwrap_or_default();
    let sha256 = sha256(&file).await?;
    let sent = api::sent(&curl(api::send_into(&account.address, &file, &sha256, library, &dir), account.header(), running).await)?;
    let up = sync::Parts { edits: ours.edits.is_some(), marks: ours.rating != 0 || ours.flag != "none", albums: !ours.albums.is_empty() };
    let sides = Sides { theirs: None, kept: None, albums: Some(&ours.albums), theirs_albums: &[] };
    both(ctx, (photo, ours), &sent.id, sides, (sync::Parts::default(), up)).await
}
