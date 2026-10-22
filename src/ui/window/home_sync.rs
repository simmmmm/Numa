use super::home::{account, curl, forget_token, said, scratch, sha256, sign_in_first, signed_out, Running, NUMA_HOME};
use super::places::photos;
use super::*;
use numa::io::home::sync::{self, Kept, Link, Local, Pace, Remote, Step};
use numa::io::home::{self as api, Account, Failure};
use std::collections::HashSet;
use std::time::Duration;

const EVERY: u32 = 180;

thread_local! {

    static ROUND: Cell<bool> = const { Cell::new(false) };

    static QUIET: RefCell<Option<Cancel>> = const { RefCell::new(None) };

    static TURN: Cell<usize> = const { Cell::new(0) };
}

const PACE: &str = "numa-home-pace";

pub(super) fn linked(state: &App, library: i64) -> Option<Link> {
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
    let pictures = dirs::picture_dir().unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join("Pictures"));
    let root = pictures.join(NUMA_HOME).join(from.name.replace('/', "-"));
    std::fs::create_dir_all(&root).map_err(|err| format!("{}: {err}", root.display()))?;
    let known = state.catalog.libraries()?.into_iter().find(|library| library.path == root);
    let library = known.map_or_else(|| state.catalog.add_library(&root), Ok)?;
    let link = linked(state, library.id)
        .unwrap_or_else(|| Link { address: account.address.clone(), library: from.id.clone(), name: from.name.clone(), ..Link::default() });

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
            if state.stack.visible_child_name().as_deref() == Some("folders") {
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
    if let Some(library) = offer(state) {
        let how = if allowed(state) { How::Quiet } else { How::Look };
        in_step(state, library, how);
    }
}

fn offer(state: &App) -> Option<Library> {
    let library = state.libraries.current.borrow().clone();
    let kept = library.filter(|library| is_home(state, library.id));
    let window = state.stack.root().and_downcast::<adw::ApplicationWindow>();
    let action = |name: &str| window.as_ref().and_then(|window| window.lookup_action(name)).and_downcast::<gio::SimpleAction>();
    if let Some(action) = action("home-sync") {
        action.set_enabled(kept.is_some());
    }

    if let Some(action) = action("home-keep") {
        action.set_enabled(kept.is_some());
        action.set_state(&kept.as_ref().is_some_and(|library| keeps_here(state, library.id)).to_variant());
    }
    kept
}

pub(super) fn keeps_here(state: &App, library: i64) -> bool {
    linked(state, library).is_some_and(|link| link.keep_here)
}

pub(super) fn keep_here(state: &App, library: &Library, on: bool) {
    let Some(link) = linked(state, library.id) else { return };
    if link.keep_here == on {
        return;
    }
    keep(state, library.id, &Link { keep_here: on, ..link });
    offer(state);
    if state.stack.visible_child_name().as_deref() == Some("folders") {
        show_folders(state);
    }
    if !on || state.catalog.at_home(library.id).map_or(true, |at_home| at_home.is_empty()) {
        return;
    }
    match allowed(state) {
        true => in_step(state, library.clone(), How::Quiet),
        false => state.toast(&format!("{} comes to this computer {}", library.label(), when(pace(state)))),
    }
}

pub(super) fn kept_here_line(state: &App, library: i64) -> Option<String> {
    if !keeps_here(state, library) {
        return None;
    }
    Some(match state.catalog.at_home(library).map_or(0, |at_home| at_home.len()) {
        0 => "Kept on this computer".to_string(),
        left => format!("Kept on this computer \u{b7} {} to bring", places::grouped(left as i64)),
    })
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

    let keep_action = gio::SimpleAction::new_stateful("home-keep", None, &false.to_variant());
    keep_action.set_enabled(false);
    keep_action.connect_change_state(glib::clone!(
        #[strong] state,
        move |action, value| {
            let Some(on) = value.and_then(|value| value.get::<bool>()) else { return };
            action.set_state(&on.to_variant());
            if let Some(library) = state.libraries.current.borrow().clone() {
                keep_here(&state, &library, on);
            }
        }
    ));
    window.add_action(&keep_action);
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
    Look,
    Glance,
}

impl How {
    fn looks(self) -> bool {
        matches!(self, How::Look | How::Glance)
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
        let out = curl(sync::list_photos(&account.address, &link.library, sync::listed_after(&link)), account.header(), &Running::default()).await;
        match sync::photos(&out) {
            Ok(remote) => {
                let since = sync::next_since(link.since, &remote);
                round(&state, library, link, remote, since, how, (Vec::new(), None));
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
            state.toast(&signed_out());
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

            let edits = state.catalog.own_edits_json(photo.id).ok().flatten();
            (photo.id, Local { path, edits, rating: photo.rating, flag: sync::flag_name(photo.flag).to_string() })
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
    failures: Vec<String>,

    cut: bool,
}

fn round(state: &App, library: Library, mut link: Link, remote: Vec<Remote>, since: i64, how: How, then: (Vec<String>, Option<String>)) {
    let local: Vec<Local> = local_photos(state, &library).into_iter().map(|(_, photo)| photo).collect();
    let mut steps = sync::plan(&link, &remote, &local);

    let whole = !how.looks() || steps.iter().all(|step| matches!(step, Step::Arrive(_)));
    steps.retain(|step| !how.looks() || matches!(step, Step::Arrive(_)));

    let to_send = steps.iter().filter(|step| matches!(step, Step::Send(_)) || matches!(step, Step::Both { up, .. } if up.any())).count();

    let keep_here = link.keep_here && !how.looks();
    let to_bring = if keep_here { state.catalog.at_home(library.id).map_or(0, |at_home| at_home.len()) } else { 0 };
    let cancel = Cancel::default();
    let running = Running::default();
    let progress = (!steps.is_empty() || to_bring > 0).then(|| match how {
        How::Quiet | How::Look | How::Glance => {
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
        let mut done = carry_out(&state, &library, &mut link, steps, shown, how == How::Glance, &cancel, &running).await;
        if keep_here && !done.cut {
            let say = |at: usize, of: usize| {
                if let Some((_, text, bar)) = &progress {
                    text.set_text(&format!("Bringing {} of {of} from {NUMA_HOME}", at + 1));
                    if let Some(bar) = bar {
                        bar.set_fraction(at as f64 / of.max(1) as f64);
                    }
                }
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
        if how == How::Import || (whole && finished) {
            link.since = since;
        }
        keep(&state, library.id, &link);
        ROUND.with(|round| round.set(false));
        QUIET.with(|quiet| quiet.replace(None));
        if let Some((toast, _, _)) = progress {
            toast.dismiss();
        }
        if done.came + done.changed + done.pictures + done.brought > 0 && state.libraries.current.borrow().as_ref().is_some_and(|open| open.id == library.id) {
            reload_grid(&state);
        }
        for failure in &done.failures {
            log::warn!("Numa Home: {failure}");
        }
        log::info!(
            "Numa Home: {} new there, {} changed there, {} went up, {} thumbnails, {} brought",
            done.came,
            done.changed,
            done.went,
            done.pictures,
            done.brought
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
    let to_go = match to_send.saturating_sub(done.went) {
        0 => String::new(),
        left => format!("; {left} to send home"),
    };
    match (done, loud) {
        (Done { cut: true, came, went, .. }, true) if cancel.stopped() => Some(format!("Stopped after {}{to_go}", came + went)),
        (Done { failures, came, went, .. }, true) if !failures.is_empty() => {
            Some(format!("{} kept at {NUMA_HOME}, {} failed: {}{to_go}", came + went, failures.len(), failures[0]))
        }

        _ if how == How::Import => None,
        (Done { came: 0, went: 0, changed: 0, brought: 0, .. }, true) => Some(format!("Kept at {NUMA_HOME} \u{b7} just now")),
        (Done { came, changed, went, brought, .. }, true) => {
            let moved: Vec<String> = [(*came, "new"), (*changed, "changed there"), (*went, "sent home"), (*brought, "brought")]
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

#[allow(clippy::too_many_arguments)]
async fn carry_out(
    state: &App,
    library: &Library,
    link: &mut Link,
    steps: Vec<Step>,
    shown: Option<(gtk::Label, Option<gtk::ProgressBar>)>,
    covers_only: bool,
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
                bar.set_fraction(at as f64 / total.max(1) as f64);
            }
        }
    };
    let (arrivals, rest): (Vec<Step>, Vec<Step>) = steps.into_iter().partition(|step| matches!(step, Step::Arrive(_)));
    let arrived: Vec<Remote> = arrivals.into_iter().filter_map(|step| if let Step::Arrive(theirs) = step { Some(theirs) } else { None }).collect();
    let later = arrive(state, library, link, &arrived, &mut done);
    let here: HashMap<String, (i64, Local)> = local_photos(state, library).into_iter().map(|(id, photo)| (photo.path.clone(), (id, photo))).collect();
    let folder = scratch();
    let (brought, pulled) = (arrived.len(), later.len());
    let mut moved: HashSet<String> = HashSet::new();
    for (at, step) in later.into_iter().chain(rest).enumerate() {
        say(brought + at.saturating_sub(pulled), match &step {
            Step::Send(_) => format!("Sending {} of {total} home", brought + at.saturating_sub(pulled) + 1),
            _ => format!("Edits and marks with {NUMA_HOME}"),
        });
        let (path, ways, edits) = match &step {
            Step::Both { path, down, up, .. } => (path, (down.any(), up.any()), down.edits || up.edits),
            Step::Send(path) => (path, (false, true), true),
            Step::Arrive(_) => continue,
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
                if edits {
                    moved.insert(path.clone());
                }
            }
            Err(failure) => {
                if stop(state, failure, &account, cancel, &mut done) {
                    break;
                }
            }
        }
    }
    let _ = std::fs::remove_dir_all(&folder);
    if !done.cut {
        let saying = |at: usize, of: usize| say(total, format!("{} of {} photos from {NUMA_HOME}", places::grouped(at as i64), places::grouped(of as i64)));
        match pictures(state, &account, library, (&here, covers_only), &moved, running, saying).await {
            Ok(came) => done.pictures = came,
            Err(failure) => {
                stop(state, failure, &account, cancel, &mut done);
            }
        }
    }
    done
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
    let edits = sync::Parts { edits: true, marks: false };
    arrived
        .iter()
        .filter_map(|theirs| {
            let flag = sync::flag_name(sync::flag(&theirs.flag)).to_string();
            let kept = Kept { id: theirs.id.clone(), edits: sync::digest(None), rating: theirs.rating.min(5), flag, updated: theirs.updated };
            link.photos.insert(theirs.path.clone(), kept);
            let down = Step::Both { path: theirs.path.clone(), id: theirs.id.clone(), theirs: Some(theirs.clone()), down: edits, up: sync::Parts::default() };
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
    state.catalog.own_edits_json(photo).map_err(Failure::Said)
}

async fn send(state: &App, account: &Account, (root, folder): (&Path, &Path), (photo, ours): (i64, &Local), library: &str, running: &Running) -> Result<Kept, Failure> {
    let file = root.join(&ours.path);
    let dir = Path::new(&ours.path).parent().map(|dir| dir.to_string_lossy().to_string()).unwrap_or_default();
    let sha256 = sha256(&file).await?;
    let sent = api::sent(&curl(api::send_into(&account.address, &file, &sha256, library, &dir), account.header(), running).await)?;
    let up = sync::Parts { edits: ours.edits.is_some(), marks: ours.rating != 0 || ours.flag != "none" };
    both(state, account, (root, folder), (photo, ours), &sent.id, None, (sync::Parts::default(), up), running).await
}
