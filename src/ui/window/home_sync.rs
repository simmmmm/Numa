use super::home::{account, curl, forget_token, said, scratch, sha256, Running, NUMA_HOME};
use super::*;
use numa::io::home::sync::{self, Kept, Link, Local, Pace, Remote, Step};
use numa::io::home::{self as api, Account, Failure};
use std::collections::HashSet;

const EVERY: u32 = 180;

thread_local! {

    static ROUND: Cell<bool> = const { Cell::new(false) };

    static QUIET: RefCell<Option<Cancel>> = const { RefCell::new(None) };

    static TURN: Cell<usize> = const { Cell::new(0) };
}

const PACE: &str = "numa-home-pace";

fn linked(state: &App, library: i64) -> Option<Link> {
    serde_json::from_str(&state.catalog.library_setting(library, sync::LINK)?).ok()
}

pub(super) fn is_home(state: &App, library: i64) -> bool {
    state.catalog.library_setting(library, sync::LINK).is_some()
}

fn keep(state: &App, library: i64, link: &Link) {
    let saved = serde_json::to_string(link).map_err(|err| err.to_string()).and_then(|json| state.catalog.set_library_setting(library, sync::LINK, Some(&json)));
    if let Err(err) = saved {
        log::warn!("Numa Home: the link could not be kept: {err}");
    }
}

fn photographs(count: usize) -> String {
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
        #[strong] months,
        move || {
            let ticked: usize = months.borrow().iter().filter(|(_, check, _)| check.is_active()).map(|(month, ..)| month.count).sum();
            go.set_label(&match ticked {
                0 => "Tick the months to bring".to_string(),
                n => format!("Import {}", photographs(n)),
            });
            go.set_sensitive(ticked > 0);
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
                let photos = match sync::photos(&out) {
                    Ok(photos) => photos,
                    Err(failure) => return group.set_description(Some(&said(failure, &account))),
                };
                group.set_description(None);
                for month in sync::months(&photos) {
                    let row = adw::ActionRow::new();
                    row.set_title(&month.name);
                    row.set_subtitle(&format!("{} \u{b7} {}", photographs(month.count), glib::format_size(month.bytes)));
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
                *listed.borrow_mut() = photos;
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
                    let names: Vec<String> = list.iter().map(|library| format!("{} \u{b7} {}", library.name, photographs(library.count as usize))).collect();
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
            let all = listed.borrow();
            let photos = all.iter().filter(|photo| keys.contains(&sync::month_of(photo))).cloned().collect();

            let since = sync::next_since(0, &all);
            dialog.close();
            bring(&state, library, photos, since, None);
        }
    ));
    group
}

fn bring(state: &App, from: sync::Library, photos: Vec<Remote>, since: i64, open: Option<String>) {
    let account = account(state);
    let pictures = dirs::picture_dir().unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join("Pictures"));
    let root = pictures.join(NUMA_HOME).join(from.name.replace('/', "-"));
    let here = std::fs::create_dir_all(&root).map_err(|err| format!("{}: {err}", root.display())).and_then(|()| {
        let known = state.catalog.libraries()?.into_iter().find(|library| library.path == root);
        known.map_or_else(|| state.catalog.add_library(&root), Ok)
    });
    let library = match here {
        Ok(library) => library,
        Err(err) => return state.toast(&err),
    };
    let link = linked(state, library.id)
        .unwrap_or_else(|| Link { address: account.address.clone(), library: from.id.clone(), name: from.name.clone(), ..Link::default() });

    if link.library != from.id || api::base(&link.address) != api::base(&account.address) {
        return state.toast(&format!("{} is kept in step with another {NUMA_HOME} library", root.display()));
    }
    let since = since.max(link.since);
    keep(state, library.id, &link);

    if ROUND.with(|round| round.replace(true)) {
        return state.toast(&format!("{NUMA_HOME} is busy with this library; import again in a moment"));
    }
    state.libraries.filter.borrow_mut().in_one_library();
    reload_libraries(state);
    select_library(state, library.id);
    round(state, library, link, photos, since, How::Import, open);
}

pub(super) fn open_link(state: &App, uri: &str) {
    let Some((server, photo)) = api::open_link(uri) else {
        return state.toast("That Open in Numa link could not be read");
    };
    let account = account(state);
    if !account.signed_in() || api::base(&server) != api::base(&account.address) {
        return state.toast(&format!("Numa is not signed in to {NUMA_HOME} at {}", api::host(&server)));
    }
    let links: Vec<(Library, Link)> = state
        .catalog
        .libraries()
        .unwrap_or_default()
        .into_iter()
        .filter_map(|library| Some((library.clone(), linked(state, library.id)?)))
        .filter(|(_, link)| api::base(&link.address) == api::base(&server))
        .collect();
    let here = links.iter().find_map(|(library, link)| {
        let (path, _) = link.photos.iter().find(|(_, kept)| kept.id == photo)?;
        sync::inside(&library.path, path).filter(|file| file.exists()).map(|file| (library.clone(), file))
    });
    if let Some((library, file)) = here {
        return show_opened(state, library, &file, true);
    }
    let state = state.clone();
    glib::spawn_future_local(async move {
        let (from, theirs, everything) = match find(&account, &photo).await {
            Ok(found) => found,
            Err(failure) => return state.toast(&said(failure, &account)),
        };

        let since = match links.iter().any(|(_, link)| link.library == from.id) {
            true => 0,
            false => everything,
        };

        while ROUND.with(Cell::get) {
            glib::timeout_future(std::time::Duration::from_millis(300)).await;
        }
        let path = theirs.path.clone();
        bring(&state, from, vec![theirs], since, Some(path));
    });
}

async fn find(account: &Account, photo: &str) -> Result<(sync::Library, Remote, i64), Failure> {
    let ask = |argv| async move { curl(argv, account.header(), &Running::default()).await };
    let library = sync::library_of(&ask(sync::get_photo(&account.address, photo)).await)?;
    let missing = || Failure::Said(format!("{NUMA_HOME} did not list that photograph"));
    let from = sync::libraries(&ask(sync::list_libraries(&account.address)).await)?.into_iter().find(|from| from.id == library).ok_or_else(missing)?;
    let all = sync::photos(&ask(sync::list_photos(&account.address, &library, 0)).await)?;
    let since = sync::next_since(0, &all);
    let theirs = all.into_iter().find(|theirs| theirs.id == photo).ok_or_else(missing)?;
    Ok((from, theirs, since))
}

pub(super) fn keep_in_step(state: &App) {

    offer(state);
    glib::timeout_add_seconds_local(
        EVERY,
        glib::clone!(
            #[strong] state,
            move || {
                if !allowed(&state) {
                    QUIET.with(|quiet| quiet.borrow().as_ref().map(Cancel::stop));
                    return glib::ControlFlow::Continue;
                }
                let kept: Vec<Library> = state.catalog.libraries().unwrap_or_default().into_iter().filter(|library| is_home(&state, library.id)).collect();
                if !kept.is_empty() {
                    let at = TURN.with(|turn| turn.replace(turn.get() + 1)) % kept.len();
                    in_step(&state, kept[at].clone(), How::Quiet);
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

pub(super) fn opened(state: &App) {
    if let Some(library) = offer(state).filter(|_| allowed(state)) {
        in_step(state, library, How::Quiet);
    }
}

fn offer(state: &App) -> Option<Library> {
    let library = state.libraries.current.borrow().clone();
    let kept = library.filter(|library| is_home(state, library.id));
    if let Some(action) = state.stack.root().and_downcast::<adw::ApplicationWindow>().and_then(|window| window.lookup_action("home-sync")).and_downcast::<gio::SimpleAction>() {
        action.set_enabled(kept.is_some());
    }
    kept
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
}

pub(super) fn keep_at_home(state: &App, libraries: &[Library]) {
    let account = account(state);
    let new: Vec<&Library> = libraries.iter().filter(|library| !is_home(state, library.id)).collect();
    for library in &new {
        keep(state, library.id, &Link { address: account.address.clone(), name: library.label(), ..Link::default() });
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

#[derive(Clone, Copy, PartialEq)]
enum How {
    Import,
    ByHand,
    Quiet,
}

fn in_step(state: &App, library: Library, how: How) {
    let Some(mut link) = linked(state, library.id) else { return };
    let account = account(state);
    if !account.signed_in() || api::base(&link.address) != api::base(&account.address) {
        if how == How::ByHand {
            state.toast(&format!("Numa is not signed in to {NUMA_HOME} at {}", api::host(&link.address)));
        }
        return;
    }
    if ROUND.with(|round| round.replace(true)) {
        if how == How::ByHand {
            state.toast(&format!("{NUMA_HOME} is busy with a library; try again in a moment"));
        }
        return;
    }
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
        let out = curl(sync::list_photos(&account.address, &link.library, link.since), account.header(), &Running::default()).await;
        match sync::photos(&out) {
            Ok(remote) => {
                let since = sync::next_since(link.since, &remote);
                round(&state, library, link, remote, since, how, None);
            }
            Err(failure) => let_go(&state, failure, &account, (how == How::ByHand).then_some(&library)),
        }
    });
}

fn let_go(state: &App, failure: Failure, account: &Account, making: Option<&Library>) {
    ROUND.with(|round| round.set(false));
    match failure {
        Failure::SignIn(_) => {
            forget_token(state);
            state.toast("Sign in again in Preferences");
        }
        Failure::Said(sentence) if making.is_some_and(|library| linked(state, library.id).is_some_and(|link| link.library.is_empty())) => {
            let library = making.expect("checked above");
            if let Err(err) = state.catalog.set_library_setting(library.id, sync::LINK, None) {
                log::warn!("Numa Home: {err}");
            }
            reload_libraries(state);
            state.toast(&format!("{} is not kept at {NUMA_HOME}: {sentence}", library.label()));
        }
        failure if making.is_some() => state.toast(&said(failure, account)),
        failure => log::info!("Numa Home: {}", said(failure, account)),
    }
}

fn local_photos(state: &App, library: &Library) -> Vec<(i64, Local)> {

    state
        .catalog
        .photos(library.id, &Filter::default())
        .unwrap_or_default()
        .into_iter()
        .map(|photo| {
            let path = photo.path.strip_prefix(&library.path).unwrap_or(&photo.path).to_string_lossy().to_string();
            let edits = state.catalog.own_edits(photo.id).ok().flatten().and_then(|document| serde_json::to_string(&document).ok());
            (photo.id, Local { path, edits, rating: photo.rating, flag: sync::flag_name(photo.flag).to_string() })
        })
        .collect()
}

#[derive(Default)]
struct Done {

    came: usize,
    changed: usize,
    went: usize,
    failures: Vec<String>,

    cut: bool,
}

fn round(state: &App, library: Library, mut link: Link, remote: Vec<Remote>, since: i64, how: How, open: Option<String>) {
    let local: Vec<Local> = local_photos(state, &library).into_iter().map(|(_, photo)| photo).collect();
    let steps = sync::plan(&link, &remote, &local);
    let cancel = Cancel::default();
    let running = Running::default();
    let progress = (!steps.is_empty()).then(|| match how {
        How::Quiet => {
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
    let state = state.clone();
    glib::spawn_future_local(async move {
        let shown = progress.as_ref().map(|(_, text, bar)| (text.clone(), bar.clone()));
        let done = carry_out(&state, &library, &mut link, steps, shown, &cancel, &running).await;
        if how == How::Import || (!done.cut && done.failures.is_empty()) {
            link.since = since;
        }
        keep(&state, library.id, &link);
        ROUND.with(|round| round.set(false));
        QUIET.with(|quiet| quiet.replace(None));
        if let Some((toast, _, _)) = progress {
            toast.dismiss();
        }
        if done.came + done.changed > 0 && state.libraries.current.borrow().as_ref().is_some_and(|open| open.id == library.id) {
            reload_grid(&state);
        }
        for failure in &done.failures {
            log::warn!("Numa Home: {failure}");
        }
        let loud = how != How::Quiet;
        let said = match (&done, loud) {
            (Done { cut: true, came, went, .. }, true) if cancel.stopped() => Some(format!("Stopped after {}; those are in step", came + went)),
            (Done { failures, came, went, .. }, true) if !failures.is_empty() => {
                Some(format!("{} in step, {} failed: {}", came + went, failures.len(), failures[0]))
            }
            (Done { came, .. }, true) if how == How::Import => Some(format!("Brought {} from {NUMA_HOME}", photographs(*came))),
            (Done { came: 0, went: 0, changed: 0, .. }, true) => Some(format!("{} is in step with {NUMA_HOME}", library.label())),
            (Done { came, changed, went, .. }, true) => {
                let moved: Vec<String> = [(*came, "brought"), (*changed, "changed there"), (*went, "sent")]
                    .into_iter()
                    .filter(|(n, _)| *n > 0)
                    .map(|(n, said)| format!("{n} {said}"))
                    .collect();
                Some(format!("{} is in step with {NUMA_HOME}: {}", library.label(), moved.join(", ")))
            }
            (Done { came: 0, .. }, false) => None,
            (Done { came, .. }, false) => Some(format!("{} new from {NUMA_HOME} in {}", photographs(*came), library.label())),
        };
        log::info!("Numa Home: {} came down, {} changed there, {} went up", done.came, done.changed, done.went);
        if let Some(said) = said {
            state.toast(&said);
        }
        if let Some(file) = open.and_then(|path| sync::inside(&library.path, &path)).filter(|file| file.exists()) {
            show_opened(&state, library, &file, true);
        }
    });
}

fn quiet(state: &App, library: &Library) -> (adw::Toast, gtk::Label, Option<gtk::ProgressBar>) {
    let (toast, text) = quiet_toast(state, &format!("{} with {NUMA_HOME}", library.label()));
    (toast, text, None)
}

async fn carry_out(
    state: &App,
    library: &Library,
    link: &mut Link,
    steps: Vec<Step>,
    shown: Option<(gtk::Label, Option<gtk::ProgressBar>)>,
    cancel: &Cancel,
    running: &Running,
) -> Done {
    let account = account(state);
    let mut done = Done::default();
    let total = steps.len();
    let say = |at: usize, text: String| {
        if let Some((label, bar)) = &shown {
            label.set_text(&text);
            if let Some(bar) = bar {
                bar.set_fraction(at as f64 / total as f64);
            }
        }
    };
    let (downloads, rest): (Vec<Step>, Vec<Step>) = steps.into_iter().partition(|step| matches!(step, Step::Download(_)));
    let brought = downloads.len();
    let mut later = Vec::new();
    for (at, step) in downloads.into_iter().enumerate() {
        let Step::Download(theirs) = step else { continue };
        say(at, format!("Bringing {} of {brought} from {NUMA_HOME}", at + 1));
        match download(&account, &library.path, &theirs, running).await {
            Ok(()) => {
                done.came += 1;
                later.push(Step::Both { path: theirs.path.clone(), id: theirs.id.clone(), theirs: Some(theirs), down: sync::Parts::ALL, up: sync::Parts::default() });
            }
            Err(failure) => {
                if stop(state, failure, &account, cancel, &mut done) {
                    return done;
                }
            }
        }
    }
    if !later.is_empty() {
        if let Err(err) = sync_library_beside(state, library, "Reading what came from Numa Home…").await {
            done.failures.push(err);
        }
    }
    let here: HashMap<String, (i64, Local)> = local_photos(state, library).into_iter().map(|(id, photo)| (photo.path.clone(), (id, photo))).collect();
    let folder = scratch();
    let pulled = later.len();
    for (at, step) in later.into_iter().chain(rest).enumerate() {
        say(brought + at.saturating_sub(pulled), match &step {
            Step::Send(_) => format!("Sending {} of {total} to {NUMA_HOME}", brought + at.saturating_sub(pulled) + 1),
            _ => format!("Edits and marks with {NUMA_HOME}"),
        });
        let (path, ways) = match &step {
            Step::Both { path, down, up, .. } => (path, (down.any(), up.any())),
            Step::Send(path) => (path, (false, true)),
            Step::Download(_) => continue,
        };

        let Some((photo, ours)) = here.get(path) else { continue };
        let result = match &step {
            Step::Both { id, theirs, down, up, .. } => both(state, &account, (&library.path, &folder), (*photo, ours), id, theirs.as_ref(), (*down, *up), running).await,
            _ => send(state, &account, (&library.path, &folder), (*photo, ours), &link.library, running).await,
        };
        match result {
            Ok(kept) => {
                done.changed += ways.0 as usize;
                done.went += ways.1 as usize;
                link.photos.insert(path.clone(), kept);
            }
            Err(failure) => {
                if stop(state, failure, &account, cancel, &mut done) {
                    break;
                }
            }
        }
    }
    let _ = std::fs::remove_dir_all(&folder);
    done
}

fn stop(state: &App, failure: Failure, account: &Account, cancel: &Cancel, done: &mut Done) -> bool {
    if cancel.stopped() {
        done.cut = true;
        return true;
    }
    let last = !matches!(failure, Failure::Said(_));
    if let Failure::SignIn(_) = failure {
        forget_token(state);
    }
    done.failures.push(said(failure, account));
    done.cut |= last;
    last
}

async fn download(account: &Account, root: &Path, theirs: &Remote, running: &Running) -> Result<(), Failure> {
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

async fn both(
    state: &App,
    account: &Account,
    (root, folder): (&Path, &Path),
    (photo, ours): (i64, &Local),
    id: &str,
    theirs: Option<&Remote>,
    (down, up): (sync::Parts, sync::Parts),
    running: &Running,
) -> Result<Kept, Failure> {
    let mut now = ours.clone();
    let mut at = theirs.map_or(0, |theirs| theirs.updated);
    if let (true, Some(theirs)) = (down.edits, theirs) {
        let (stack, given) = sync::edits(&curl(sync::get_edits(&account.address, &theirs.id), account.header(), running).await)?;
        now.edits = put_edits(state, root.join(&ours.path), photo, stack)?;
        at = at.max(given);
    }
    if let (true, Some(theirs)) = (down.marks, theirs) {
        let (rating, flag) = (theirs.rating.min(5), sync::flag(&theirs.flag));
        state.catalog.set_ratings(&[photo], rating).map_err(Failure::Said)?;
        state.catalog.set_flags(&[photo], flag).map_err(Failure::Said)?;
        (now.rating, now.flag) = (rating, sync::flag_name(flag).to_string());
    }
    if up.edits {
        let stack = folder.join("edits.json");
        std::fs::write(&stack, ours.edits.as_deref().unwrap_or_default()).map_err(|err| Failure::Said(err.to_string()))?;
        at = at.max(sync::updated(&curl(api::edits(&account.address, id, &stack), account.header(), running).await)?);
    }
    if up.marks {
        at = at.max(sync::updated(&curl(sync::put_marks(&account.address, id, ours.rating, &ours.flag), account.header(), running).await)?);
    }
    Ok(now.kept(id, at))
}

fn put_edits(state: &App, file: PathBuf, photo: i64, stack: Option<String>) -> Result<Option<String>, Failure> {
    let file = file.to_string_lossy().to_string();
    let mut document = match &stack {
        Some(json) => serde_json::from_str::<Document>(json).map_err(|_| Failure::Said(format!("The edits of {file} could not be read")))?,
        None => Document::new(file.clone()),
    };

    document.source.path = file;
    state.catalog.save_own_edits(photo, &document).map_err(Failure::Said)?;
    Ok((!document.is_untouched()).then(|| serde_json::to_string(&document).unwrap_or_default()))
}

async fn send(state: &App, account: &Account, (root, folder): (&Path, &Path), (photo, ours): (i64, &Local), library: &str, running: &Running) -> Result<Kept, Failure> {
    let file = root.join(&ours.path);
    let dir = Path::new(&ours.path).parent().map(|dir| dir.to_string_lossy().to_string()).unwrap_or_default();
    let sha256 = sha256(&file).await?;
    let sent = api::sent(&curl(api::send_into(&account.address, &file, &sha256, library, &dir), account.header(), running).await)?;
    let up = sync::Parts { edits: ours.edits.is_some(), marks: ours.rating != 0 || ours.flag != "none" };
    both(state, account, (root, folder), (photo, ours), &sent.id, None, (sync::Parts::default(), up), running).await
}
