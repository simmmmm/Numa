use super::home::{account, curl, Running, NUMA_HOME};
use super::*;
use numa::io::home::same::{self, Held, Origin, Print};
use numa::io::home::sync::{self, Kept};
use numa::io::home::Failure;
use std::collections::BTreeMap;

const KEPT_APART: &str = "same-photographs-kept-apart";

#[derive(Clone)]
enum Offer {

    Libraries { here: Library, home: Library, tied: BTreeMap<String, Kept>, shared: usize, of: usize },

    Albums { keep: (String, String), other: (String, String), shared: usize, of: usize },
}

impl Offer {

    fn id(&self) -> String {
        match self {
            Offer::Libraries { here, home, .. } => format!("l{}:{}", here.path.display(), home.path.display()),

            Offer::Albums { keep, other, .. } => format!("a{}:{}", keep.0.as_str().min(&other.0), keep.0.as_str().max(&other.0)),
        }
    }

    fn line(&self) -> String {
        match self {
            Offer::Libraries { here, home, .. } => format!("\u{201c}{}\u{201d} on this computer has the same photographs as \u{201c}{}\u{201d} at {NUMA_HOME}", here.label(), home.label()),
            Offer::Albums { keep, other, .. } => format!("The albums \u{201c}{}\u{201d} and \u{201c}{}\u{201d} have the same photographs", keep.1, other.1),
        }
    }
}

thread_local!(static OFFERS: RefCell<Vec<Offer>> = const { RefCell::new(Vec::new()) });

pub(super) fn origin_of(state: &App, library: i64) -> Origin {
    match home_sync::way_of(state, library) {
        Some(sync::Way::Down) => Origin::Home,
        Some(sync::Way::Up) => Origin::Both,
        None => Origin::Here,
    }
}

pub(super) fn banners(state: &App, column: &gtk::Box, libraries: bool) {
    let offers = OFFERS.with(|offers| offers.borrow().clone());
    for offer in offers.into_iter().filter(|offer| matches!(offer, Offer::Libraries { .. }) == libraries) {
        let banner = adw::Banner::new(&offer.line());
        banner.set_use_markup(false);
        banner.set_button_label(Some("Merge\u{2026}"));
        banner.set_revealed(true);
        let state = state.clone();
        banner.connect_button_clicked(move |_| ask(&state, offer.clone()));
        column.append(&banner);
    }
}

pub(super) fn look_later(state: &App) {
    let state = state.clone();
    glib::spawn_future_local(async move {
        glib::timeout_future_seconds(45).await;
        match look(&state).await {
            Ok(offers) => {
                OFFERS.with(|shown| shown.replace(offers));
                show_again(&state);
            }

            Err(failure) => log::info!("Numa Home: same photographs: {}", super::home::said(failure, &account(&state))),
        }
    });
}

fn show_again(state: &App) {
    match state.stack.visible_child_name().as_deref() {
        Some("libraries") => show_folders(state),
        Some("albums") => show_albums(state),
        _ => {}
    }
}

type AtHome = HashMap<i64, (String, u64)>;

fn prints(state: &App, library: &Library, at_home: &AtHome, filter: &Filter) -> Vec<Print> {
    let photos = state.catalog.photos(library.id, filter).unwrap_or_default();
    let print = |photo: numa::io::catalog::Photo| {
        let path = photo.path.strip_prefix(&library.path).unwrap_or(&photo.path).to_string_lossy().to_string();
        match at_home.get(&photo.id) {
            Some((id, size)) => Print { path, size: *size, held: Held::Home(id.clone()) },
            None => Print { path, size: 0, held: Held::File(photo.path) },
        }
    };
    photos.into_iter().map(print).collect()
}

async fn weighed(sets: Vec<Vec<Print>>) -> Vec<Vec<Print>> {
    let copy = sets.clone();
    let weigh = move || {
        let mut sets = copy;
        for print in sets.iter_mut().flatten() {
            if let Held::File(path) = &print.held {
                print.size = std::fs::metadata(path).map_or(0, |meta| meta.len());
            }
        }
        sets
    };
    gio::spawn_blocking(weigh).await.unwrap_or(sets)
}

pub(super) async fn same_bytes(state: &App, a: &[Print], b: &[Print], pairs: &[(usize, usize)]) -> Result<bool, Failure> {
    let sample = same::sample(pairs);
    let held: Vec<(Held, Held)> = sample.iter().map(|(ours, theirs)| (a[*ours].held.clone(), b[*theirs].held.clone())).collect();
    let all = || held.iter().flat_map(|(ours, theirs)| [ours, theirs]);
    let ids: Vec<String> = all().filter_map(|held| if let Held::Home(id) = held { Some(id.clone()) } else { None }).collect();
    let files: Vec<PathBuf> = all().filter_map(|held| if let Held::File(path) = held { Some(path.clone()) } else { None }).collect();
    let mut there = HashMap::new();
    let account = account(state);
    for ids in ids.chunks(same::SAMPLE) {
        there.extend(sync::photo_hashes(&curl(sync::hash_photos(&account.address, ids), account.header(), &Running::default()).await)?);
    }
    let here: HashMap<PathBuf, String> = gio::spawn_blocking(move || files.into_iter().filter_map(|path| Some((path.clone(), same::hash_here(&path)?))).collect()).await.unwrap_or_default();
    let hash = |held: &Held| match held {
        Held::File(path) => here.get(path).cloned(),
        Held::Home(id) => there.get(id).cloned(),
    };
    let agree = held.iter().filter(|(ours, theirs)| ours == theirs || hash(ours).is_some_and(|hashed| Some(hashed) == hash(theirs)));
    Ok(same::holds(agree.count(), held.len()))
}

async fn look(state: &App) -> Result<Vec<Offer>, Failure> {
    if !account(state).signed_in() {
        return Ok(Vec::new());
    }
    let apart: Vec<String> = state.catalog.recall(KEPT_APART).unwrap_or_default();
    let libraries = state.catalog.libraries().unwrap_or_default();
    let at_home: Vec<AtHome> = libraries.iter().map(|library| state.catalog.at_home(library.id).unwrap_or_default()).collect();
    let rows_of = |library: &Library, filter: &Filter| {
        let at = libraries.iter().position(|it| it.id == library.id);
        prints(state, library, at.map_or(&AtHome::new(), |at| &at_home[at]), filter)
    };
    let mut offers = Vec::new();

    let connected = |library: &&Library| !state.catalog.is_offline(library.id);
    let here: Vec<&Library> = libraries.iter().filter(connected).filter(|library| home_sync::way_of(state, library.id).is_none()).collect();
    let home: Vec<&Library> = libraries.iter().filter(|library| home_sync::is_home(state, library.id) && home_sync::is_cache_only(library) && !home_sync::is_synced(state, library.id)).collect();
    if !here.is_empty() && !home.is_empty() {
        let read = |list: &[&Library]| list.iter().map(|library| rows_of(library, &Filter::default())).collect::<Vec<_>>();
        let ours = weighed(read(&here)).await;
        let theirs = read(&home);
        for (library, a) in here.iter().zip(&ours) {
            for (other, b) in home.iter().zip(&theirs) {
                let pairs = same::matched(a, b);
                if same::alike(&pairs, a.len(), b.len()) && same_bytes(state, a, b, &pairs).await? {
                    let offer = Offer::Libraries { here: (*library).clone(), home: (*other).clone(), tied: same::seeded(a, b, &pairs), shared: pairs.len(), of: a.len().max(b.len()) };
                    offers.push(offer);
                }
            }
        }
    }

    let albums = state.catalog.albums().unwrap_or_default();
    let sets: Vec<Vec<Print>> = albums
        .iter()
        .map(|(key, _)| {
            let filter = Filter { album: Some(key.clone()), ..Filter::default() };
            libraries.iter().flat_map(|library| rows_of(library, &filter)).collect()
        })
        .collect();
    let touches_home: Vec<bool> = albums
        .iter()
        .map(|(key, _)| libraries.iter().any(|library| origin_of(state, library.id) != Origin::Here && state.catalog.glance(library.id, Some(key), None).is_ok_and(|glance| glance.photos > 0)))
        .collect();
    let sets = weighed(sets).await;
    for (at, (keep, a)) in albums.iter().zip(&sets).enumerate() {
        for (other_at, (other, b)) in albums.iter().zip(&sets).enumerate().skip(at + 1) {
            if !touches_home[at] && !touches_home[other_at] {
                continue;
            }
            let pairs = same::matched(a, b);
            if same::alike(&pairs, a.len(), b.len()) && same_bytes(state, a, b, &pairs).await? {
                let (keep, other) = if a.len() >= b.len() { (keep, other) } else { (other, keep) };
                offers.push(Offer::Albums { keep: keep.clone(), other: other.clone(), shared: pairs.len(), of: a.len().max(b.len()) });
            }
        }
    }
    offers.retain(|offer| !apart.contains(&offer.id()));
    Ok(offers)
}

fn ask(state: &App, offer: Offer) {
    let Some(window) = state.stack.root().and_downcast::<adw::ApplicationWindow>() else { return };
    let (title, body) = match &offer {
        Offer::Libraries { here, home, shared, of, .. } => (
            format!("Merge the Two \u{201c}{}\u{201d}?", home.label()),
            format!(
                "{} of {} are the same on this computer ({}) and at {NUMA_HOME}. The two become one, kept in step like a synced library: what only one of them has goes to the other, and edits, marks and albums of both are kept. No photograph is removed from either side.",
                places::grouped(*shared as i64),
                home_sync::photographs(*of),
                here.path.display(),
            ),
        ),
        Offer::Albums { keep, other, shared, of } => (
            format!("Merge \u{201c}{}\u{201d} into \u{201c}{}\u{201d}?", other.1, keep.1),
            format!(
                "{} of {} are the same in both. \u{201c}{}\u{201d} stays and holds the photographs of both; \u{201c}{}\u{201d} goes, here and, at the next sync, at {NUMA_HOME}. No photograph is removed.",
                places::grouped(*shared as i64),
                home_sync::photographs(*of),
                keep.1,
                other.1,
            ),
        ),
    };
    let dialog = adw::AlertDialog::new(Some(&title), Some(&body));
    dialog.add_response("later", "Not Now");
    dialog.add_response("apart", "Keep Both");
    dialog.add_response("merge", "Merge");
    dialog.set_default_response(Some("later"));
    dialog.set_close_response("later");
    let state = state.clone();
    dialog.connect_response(None, move |_, response| match response {
        "merge" => merge(&state, &offer),
        "apart" => {
            let mut apart: Vec<String> = state.catalog.recall(KEPT_APART).unwrap_or_default();
            apart.push(offer.id());
            state.catalog.remember(KEPT_APART, &apart);
            settled(&state, &offer);
        }
        _ => {}
    });
    dialog.present(Some(&window));
}

fn settled(state: &App, offer: &Offer) {
    let id = offer.id();
    OFFERS.with(|offers| offers.borrow_mut().retain(|it| it.id() != id));
    show_again(state);
}

fn merge(state: &App, offer: &Offer) {
    match offer {
        Offer::Albums { keep, other, .. } => match state.catalog.merge_albums(&keep.0, &other.0) {
            Ok(()) => {
                refresh_places(state);
                settled(state, offer);
                reload_grid(state);
            }
            Err(err) => state.toast(&err),
        },
        Offer::Libraries { here, home, tied, .. } => {
            let (state, offer, here, home, tied) = (state.clone(), offer.clone(), here.clone(), home.clone(), tied.clone());
            glib::spawn_future_local(async move {
                if let Err(err) = home_pair::merge_with_home(&state, &here, &home, tied).await {
                    return state.toast(&err);
                }
                settled(&state, &offer);
            });
        }
    }
}
