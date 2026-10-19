use super::home::{account, curl, forget_token, said, sign_in_first, Running, NUMA_HOME};
use super::*;
use numa::io::catalog::{home_preview, home_thumb};
use numa::io::home::sync::{self, Remote};
use numa::io::home::{self as api, Account, Failure};

thread_local! {

    static FETCHING: RefCell<HashMap<i64, f64>> = RefCell::default();

    static LOUPE_PILL: RefCell<Option<(gtk::Box, gtk::Button, gtk::ProgressBar)>> = const { RefCell::new(None) };
}

pub(super) fn progress(id: i64) -> Option<f64> {
    FETCHING.with(|fetching| fetching.borrow().get(&id).copied())
}

pub(super) fn card_bar() -> gtk::ProgressBar {
    let bar = progress_on("card-fetch");
    bar.set_valign(gtk::Align::Center);
    bar
}

pub(super) fn loupe_pill(state: &App) -> gtk::Box {
    let pill = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    pill.add_css_class("photo-pill");
    pill.set_halign(gtk::Align::Center);
    pill.set_valign(gtk::Align::Start);
    pill.set_margin_top(12);
    pill.set_visible(false);
    pill.append(&gtk::Label::new(Some(&format!("Preview from {NUMA_HOME}"))));
    let get = gtk::Button::with_label("Get Original");
    get.set_tooltip_text(Some("The file the camera wrote, to this computer"));
    get.connect_clicked(glib::clone!(
        #[strong] state,
        move |_| {
            if let Some(id) = state.loupe.at.get().and_then(|at| id_at(&state, at)) {
                fetch_first(&state, id, |_| {});
            }
        }
    ));
    let bar = progress_on("photo-fetch");
    bar.set_valign(gtk::Align::Center);
    pill.append(&get);
    pill.append(&bar);
    LOUPE_PILL.with(|kept| kept.replace(Some((pill.clone(), get, bar))));
    pill
}

pub(super) fn paint_loupe(state: &App) {
    let id = state.loupe.at.get().and_then(|at| id_at(state, at));
    let at_home = id.is_some_and(|id| state.grid.at_home.borrow().contains_key(&id));
    let coming = id.and_then(progress);
    let review = state.loupe.tape.review.pill.reveals_child();
    LOUPE_PILL.with(|pill| {
        if let Some((pill, get, bar)) = pill.borrow().as_ref() {
            pill.set_visible(at_home && !review);
            get.set_visible(coming.is_none());
            bar.set_visible(coming.is_some());
            bar.set_fraction(coming.unwrap_or(0.0));
        }
    });
}

pub(super) fn preview(state: &App, id: i64, near: impl Fn(&App) -> bool + 'static, apply: impl Fn(&App, gtk::gdk::Texture) + 'static) -> bool {
    let Some((server, _)) = state.grid.at_home.borrow().get(&id).cloned() else { return false };
    let library = state.libraries.all.borrow().iter().find(|library| library.id == numa::io::catalog::library_of(id)).cloned();
    let Some(library) = library else { return false };
    let file = home_preview(&library.path, &server);
    let account = account(state);
    let link = home_sync::linked(state, library.id);
    let reachable = account.signed_in() && link.is_some_and(|link| api::same_server(&link.address, &account.address));
    let thumb = home_thumb(&library.path, &server);
    let state = state.clone();
    glib::spawn_future_local(async move {
        let decoded = |file: PathBuf| gio::spawn_blocking(move || image::open(file).ok().map(|image| image.into_rgb8()));
        if !file.exists() {
            if let Ok(Some(image)) = decoded(thumb).await {
                apply(&state, crate::ui::display::texture(&image));
            }
            if !reachable || !near(&state) {
                return;
            }
            let mut part = file.clone().into_os_string();
            part.push(".part");
            let part = PathBuf::from(part);
            let came = api::taken(&curl(sync::preview(&account.address, &server, &part), account.header(), &Running::default()).await);
            if came.is_err() || std::fs::rename(&part, &file).is_err() {
                let _ = std::fs::remove_file(&part);
                return;
            }
        }
        if let Ok(Some(image)) = decoded(file).await {
            if near(&state) {
                apply(&state, crate::ui::display::texture(&image));
            }
        }
    });
    true
}

fn show(state: &App, id: i64) {
    let at = state.grid.lazy.borrow().iter().position(|card| card.id == id);
    if let Some(at) = at {
        state.grid.wall.rebind(at);
    }
    paint_loupe(state);
}

pub(super) fn fetch_first(state: &App, id: i64, then: impl FnOnce(&App) + 'static) -> bool {
    let Some((server, size)) = state.grid.at_home.borrow().get(&id).cloned() else { return false };
    if progress(id).is_some() {
        return true;
    }
    let photo = state.grid.cards.borrow().get(&id).cloned();
    let library = state.libraries.all.borrow().iter().find(|library| library.id == numa::io::catalog::library_of(id)).cloned();
    let (Some(photo), Some(library)) = (photo, library) else { return false };
    let Some(link) = home_sync::linked(state, library.id) else { return false };
    let account = account(state);
    if !account.signed_in() || !api::same_server(&link.address, &account.address) {
        state.toast(&sign_in_first(&account, &api::host(&link.address)));
        return true;
    }
    let theirs = remote(&library, &photo.path, photo.mtime, server, size);
    let state = state.clone();
    glib::spawn_future_local(async move {
        match fetch_one(&state, &account, &library, id, &theirs).await {
            Ok(()) => then(&state),
            Err(failure) => failed(&state, failure, &account),
        }
    });
    true
}

fn remote(library: &Library, path: &Path, mtime: i64, id: String, size: u64) -> Remote {
    let path = path.strip_prefix(&library.path).unwrap_or(path).to_string_lossy().into_owned();
    Remote { id, path, size, mtime, taken: None, rating: 0, flag: "none".into(), edited: false, updated: 0 }
}

fn failed(state: &App, failure: Failure, account: &Account) {
    if let Failure::SignIn(_) = failure {
        forget_token(state);
    }
    state.toast(&said(failure, account));
}

async fn fetch_one(state: &App, account: &Account, library: &Library, id: i64, theirs: &Remote) -> Result<(), Failure> {
    FETCHING.with(|fetching| fetching.borrow_mut().insert(id, 0.0));
    show(state, id);
    let part = sync::inside(&library.path, &theirs.path).map(|to| {
        let mut part = to.into_os_string();
        part.push(".part");
        PathBuf::from(part)
    });
    let size = theirs.size.max(1) as f64;
    glib::timeout_add_local(
        std::time::Duration::from_millis(250),
        glib::clone!(
            #[strong] state,
            move || {
                let come = part.as_ref().and_then(|part| std::fs::metadata(part).ok()).map_or(0.0, |meta| meta.len() as f64 / size);
                let going = FETCHING.with(|fetching| fetching.borrow_mut().get_mut(&id).map(|at| *at = come.min(1.0)).is_some());
                if !going {
                    return glib::ControlFlow::Break;
                }
                show(&state, id);
                glib::ControlFlow::Continue
            }
        ),
    );
    let mut fetched = home_sync::download(account, &library.path, theirs, &Running::default()).await;
    if let (Ok(()), Some(file)) = (&fetched, sync::inside(&library.path, &theirs.path)) {

        let read = gio::spawn_blocking(move || {
            let mtime = std::fs::metadata(&file).and_then(|meta| meta.modified()).ok();
            let mtime = mtime.and_then(|at| at.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |at| at.as_secs() as i64);
            (mtime, numa::io::exif::stamp(&file))
        });
        let (mtime, stamp) = read.await.unwrap_or_default();
        fetched = state.catalog.arrived(id, mtime, &stamp).map_err(Failure::Said);
    }
    FETCHING.with(|fetching| fetching.borrow_mut().remove(&id));
    if fetched.is_ok() {
        state.grid.at_home.borrow_mut().remove(&id);

        reload_frame(state, id);
    }
    show(state, id);
    fetched
}

fn at_home_of(state: &App, library: &Library, wanted: impl Fn(&str) -> bool) -> Vec<(i64, Remote)> {
    let at_home = state.catalog.at_home(library.id).unwrap_or_default();
    let photos = state.catalog.photos(library.id, &Filter::default()).unwrap_or_default();
    photos
        .iter()
        .filter_map(|photo| {
            let (server, size) = at_home.get(&photo.id)?.clone();
            let theirs = remote(library, &photo.path, photo.mtime, server, size);
            wanted(&theirs.path).then_some((photo.id, theirs))
        })
        .collect()
}

pub(super) async fn bring_all(state: &App, library: &Library, cancel: &Cancel, say: impl Fn(usize, usize)) -> Result<usize, Failure> {
    let account = account(state);
    let wanted = at_home_of(state, library, |_| true);
    let mut came = 0;
    for (at, (id, theirs)) in wanted.iter().enumerate() {
        if cancel.stopped() {
            break;
        }
        say(at, wanted.len());
        match fetch_one(state, &account, library, *id, theirs).await {
            Ok(()) => came += 1,
            Err(Failure::Said(sentence)) => log::warn!("Numa Home: {sentence}"),
            Err(failure) => return Err(failure),
        }
    }
    Ok(came)
}

pub(super) fn fetch_paths(state: &App, library: Library, paths: Vec<String>, open: Option<String>) {
    let account = account(state);
    let paths: std::collections::HashSet<String> = paths.into_iter().collect();
    let wanted = at_home_of(state, &library, |path| paths.contains(path));
    let state = state.clone();
    glib::spawn_future_local(async move {
        let cancel = Cancel::default();

        let toast = (wanted.len() > 1).then(|| progress_toast(&state, &cancel));
        let (mut brought, mut trouble) = (0, None);
        for (at, (id, theirs)) in wanted.iter().enumerate() {
            if cancel.stopped() {
                break;
            }
            if let Some((_, text, bar)) = &toast {
                text.set_text(&format!("Bringing {} of {} from {NUMA_HOME}", at + 1, wanted.len()));
                bar.set_fraction(at as f64 / wanted.len() as f64);
            }
            match fetch_one(&state, &account, &library, *id, theirs).await {
                Ok(()) => brought += 1,

                Err(Failure::Said(sentence)) => {
                    log::warn!("Numa Home: {sentence}");
                    trouble.get_or_insert(sentence);
                }
                Err(failure) => {
                    if let Failure::SignIn(_) = failure {
                        forget_token(&state);
                    }
                    trouble = Some(said(failure, &account));
                    break;
                }
            }
        }
        if let Some((toast, _, _)) = toast {
            toast.dismiss();
            state.toast(&match trouble {
                None => format!("Brought {} from {NUMA_HOME}", home_sync::photographs(brought)),
                Some(trouble) => format!("Brought {brought} of {} from {NUMA_HOME}: {trouble}", home_sync::photographs(wanted.len())),
            });
        } else if let Some(trouble) = trouble {
            return state.toast(&trouble);
        }
        if let Some(file) = open.and_then(|path| sync::inside(&library.path, &path)).filter(|file| file.exists()) {
            show_opened(&state, library, &file, true);
        } else if state.libraries.current.borrow().as_ref().is_some_and(|open| open.id == library.id) {
            reload_grid(&state);
        }
    });
}

pub(super) async fn thumbnails(account: &Account, root: &Path, ids: Vec<String>, running: &Running, say: impl Fn(usize, usize)) -> Result<usize, Failure> {
    const BATCH: usize = 100;
    if let Some(folder) = home_thumb(root, "-").parent() {
        std::fs::create_dir_all(folder).map_err(|err| Failure::Said(format!("{}: {err}", folder.display())))?;
    }
    let mut came = 0;
    for (at, batch) in ids.chunks(BATCH).enumerate() {
        say(at * BATCH, ids.len());
        let wanted: Vec<(String, PathBuf, PathBuf)> = batch
            .iter()
            .map(|id| {
                let to = home_thumb(root, id);
                let mut part = to.clone().into_os_string();
                part.push(".part");
                (id.clone(), PathBuf::from(part), to)
            })
            .collect();
        let argv = sync::thumbnails(&account.address, &wanted.iter().map(|(id, part, _)| (id.clone(), part.clone())).collect::<Vec<_>>());
        let answered = sync::thumbnails_came(&curl(argv, account.header(), running).await);
        let arrived = answered.as_deref().unwrap_or_default();
        for (at, (_, part, to)) in wanted.iter().enumerate() {
            match arrived.get(at).copied().unwrap_or(false) && std::fs::rename(part, to).is_ok() {

                true => {
                    came += 1;
                    let _ = std::fs::remove_file(home_preview(root, &batch[at]));
                }
                false => drop(std::fs::remove_file(part)),
            }
        }
        answered?;
    }
    Ok(came)
}
