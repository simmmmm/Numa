use super::*;
use numa::io::import::{self, Found, Place, Shoot};
use std::collections::HashMap;
use std::ffi::OsString;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

thread_local! {

    static MONITOR: RefCell<Option<gio::VolumeMonitor>> = const { RefCell::new(None) };
}

pub(super) fn watch_cards(state: &App) {
    let monitor = gio::VolumeMonitor::get();
    monitor.connect_mount_added(glib::clone!(
        #[strong] state,
        move |_, mount| {
            if let Some(root) = card_root(mount) {
                offer(&state, &mount.name(), root);
            }
        }
    ));
    MONITOR.with(|kept| *kept.borrow_mut() = Some(monitor));
}

fn offer(state: &App, name: &str, root: PathBuf) {
    let toast = adw::Toast::new(&format!("{name} connected"));
    toast.set_button_label(Some("Import…"));
    toast.set_timeout(0);
    toast.connect_button_clicked(glib::clone!(
        #[strong] state,
        move |_| {
            if let Some(window) = state.stack.root().and_downcast::<adw::ApplicationWindow>() {
                import_dialog(&state, &window, Some(root.clone()));
            }
        }
    ));
    state.toasts.add_toast(toast);
}

fn card_root(mount: &gio::Mount) -> Option<PathBuf> {
    let root = mount.root().path()?;
    let has = |dir: &Path| dir.join("DCIM").is_dir();
    let camera = matches!(mount.root().uri_scheme().as_deref(), Some("gphoto2" | "mtp"));
    let slots = || std::fs::read_dir(&root).ok().is_some_and(|mut entries| entries.any(|entry| entry.is_ok_and(|entry| has(&entry.path()))));
    (has(&root) || camera || slots()).then_some(root)
}

fn cards() -> Vec<(String, PathBuf)> {
    gio::VolumeMonitor::get().mounts().iter().filter_map(|mount| Some((mount.name().to_string(), card_root(mount)?))).collect()
}

struct Card {
    found: Vec<Found>,

    already: Vec<Option<PathBuf>>,
    shoots: Vec<Shoot>,
}

pub(super) fn import_dialog(state: &App, window: &adw::ApplicationWindow, from: Option<PathBuf>) {
    let dialog = adw::Dialog::new();
    dialog.set_title("Import");
    dialog.set_content_width(560);
    dialog.set_content_height(640);
    let view = adw::ToolbarView::new();
    view.add_top_bar(&adw::HeaderBar::new());
    let page = adw::PreferencesPage::new();
    view.set_content(Some(&page));

    let go = gtk::Button::with_label("Import");
    go.add_css_class("suggested-action");
    go.add_css_class("pill");
    go.set_halign(gtk::Align::Center);
    go.set_margin_top(12);
    go.set_margin_bottom(12);
    go.set_sensitive(false);
    view.add_bottom_bar(&go);
    dialog.set_child(Some(&view));

    let from_group = adw::PreferencesGroup::new();
    from_group.set_title("From");
    let mut sources = cards();
    if let Some(from) = &from {
        if !sources.iter().any(|(_, path)| path == from) {
            sources.insert(0, (from.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default(), from.clone()));
        }
    }
    let names: Vec<String> = sources.iter().map(|(name, _)| name.clone()).chain(["Another folder…".to_string()]).collect();
    let source = adw::ComboRow::new();
    source.set_title("Card or camera");
    source.set_model(Some(&gtk::StringList::new(&names.iter().map(String::as_str).collect::<Vec<_>>())));
    from_group.add(&source);
    page.add(&from_group);

    let body = adw::PreferencesGroup::new();
    page.add(&body);
    let rename = adw::EntryRow::new();
    rename.set_title("Rename — {date} {time} {n} {name}; empty keeps the camera's names");
    let names_group = adw::PreferencesGroup::new();
    names_group.set_title("Names");
    names_group.add(&rename);
    page.add(&names_group);

    let sources = Rc::new(sources);
    let read: Rc<RefCell<Option<(Card, Rc<RefCell<Vec<Place>>>)>>> = Rc::new(RefCell::new(None));
    let body = Rc::new(RefCell::new(body));
    let choose = glib::clone!(
        #[strong] state,
        #[strong] read,
        #[strong] body,
        #[strong] page,
        #[strong] names_group,
        #[weak] go,
        #[weak] dialog,
        move |path: PathBuf| {
            go.set_sensitive(false);
            let fresh = adw::PreferencesGroup::new();
            fresh.set_title("Reading the card…");

            page.remove(&*body.borrow());
            page.remove(&names_group);
            page.add(&fresh);
            page.add(&names_group);
            *body.borrow_mut() = fresh.clone();
            let state = state.clone();
            let read = read.clone();
            glib::spawn_future_local(async move {
                let card = read_card(&state, path).await;
                let places = fill(&state, &dialog, &fresh, &card);
                go.set_label(&match card.already.iter().filter(|there| there.is_none()).count() {
                    0 => "Nothing new to import".to_string(),
                    count => format!("Import {count}"),
                });
                go.set_sensitive(card.already.iter().any(Option::is_none));
                *read.borrow_mut() = Some((card, places));
            });
        }
    );
    let choose = Rc::new(choose);
    source.connect_selected_notify(glib::clone!(
        #[strong] sources,
        #[strong] choose,
        #[weak] window,
        move |row| match sources.get(row.selected() as usize) {
            Some((_, path)) => choose(path.clone()),
            None => {
                let chooser = gtk::FileDialog::new();
                chooser.set_title("Import from this folder");
                let choose = choose.clone();
                glib::spawn_future_local(async move {
                    if let Some(path) = chooser.select_folder_future(Some(&window)).await.ok().and_then(|file| file.path()) {
                        choose(path);
                    }
                });
            }
        }
    ));

    go.connect_clicked(glib::clone!(
        #[strong] state,
        #[strong] read,
        #[weak] dialog,
        #[weak] rename,
        move |_| {
            let Some((card, places)) = read.borrow_mut().take() else { return };
            dialog.close();
            let places = places.borrow().clone();
            run_import(&state, card, places, rename.text().to_string());
        }
    ));

    dialog.present(Some(window));
    match sources.first() {
        Some((_, path)) => choose(path.clone()),
        None => source.set_selected(0),
    }
}

async fn read_card(state: &App, path: PathBuf) -> Card {
    let mut known = Vec::new();
    for library in state.libraries.all.borrow().iter() {
        known.extend(state.catalog.paths(library.id).unwrap_or_default());
    }
    let read = gio::spawn_blocking(move || {
        let found = import::scan(&path);
        let already = import::already(&found, &import::by_name(known));
        (found, already)
    })
    .await;
    let (found, already) = read.unwrap_or_default();
    let new: Vec<usize> = (0..found.len()).filter(|at| already[*at].is_none()).collect();
    let shoots = import::shoots(&found, &new);
    Card { found, already, shoots }
}

fn offset() -> i64 {
    glib::DateTime::now_local().map_or(0, |now| now.utc_offset().as_seconds())
}

fn days(first: i64, last: i64) -> String {
    let date = |when: i64| glib::DateTime::from_unix_local(when).ok();
    let (Some(from), Some(to)) = (date(first), date(last)) else { return String::new() };
    let month = |at: &glib::DateTime| ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"][(at.month() as usize).clamp(1, 12) - 1];
    let whole = |at: &glib::DateTime| format!("{} {} {}", at.day_of_month(), month(at), at.year());
    match (from.year() == to.year(), from.month() == to.month(), from.day_of_month() == to.day_of_month()) {
        (true, true, true) => whole(&from),
        (true, true, false) => format!("{}\u{2009}\u{2013}\u{2009}{}", from.day_of_month(), whole(&to)),
        (true, false, _) => format!("{} {}\u{2009}\u{2013}\u{2009}{}", from.day_of_month(), month(&from), whole(&to)),
        _ => format!("{}\u{2009}\u{2013}\u{2009}{}", whole(&from), whole(&to)),
    }
}

fn fill(state: &App, dialog: &adw::Dialog, group: &adw::PreferencesGroup, card: &Card) -> Rc<RefCell<Vec<Place>>> {
    let libraries = state.libraries.all.borrow().clone();
    let spans: Vec<import::Span> = libraries
        .iter()
        .map(|library| {
            let glance = state.catalog.glance(library.id, None, None).ok();
            (library.clone(), glance.as_ref().and_then(|g| g.first), glance.as_ref().and_then(|g| g.last))
        })
        .collect();
    let places = Rc::new(RefCell::new(card.shoots.iter().map(|shoot| import::suggest(shoot, &spans, offset())).collect::<Vec<_>>()));

    group.set_title(&match card.found.len() {
        0 => "No photographs on it".to_string(),
        count => format!("{count} photographs"),
    });
    let old = card.already.iter().filter(|there| there.is_some()).count();
    if old > 0 {
        let held: std::collections::BTreeSet<String> = card
            .already
            .iter()
            .flatten()
            .filter_map(|path| libraries.iter().find(|library| path.starts_with(&library.path)).map(|library| library.label()))
            .collect();
        let row = adw::ActionRow::new();
        row.set_title(&format!("{old} already in {}", held.into_iter().collect::<Vec<_>>().join(", ")));
        row.set_subtitle("Left on the card and not copied again");
        row.add_prefix(&gtk::Image::from_icon_name("object-select-symbolic"));
        group.add(&row);
    }
    for (at, shoot) in card.shoots.iter().enumerate() {
        let title = format!("{} \u{b7} {} photographs", days(shoot.first, shoot.last), shoot.photos.len());
        let row: adw::PreferencesRow = match &places.borrow()[at] {
            Place::Library(library) => {
                let row = adw::ActionRow::new();
                row.set_title(&title);
                row.set_subtitle(&format!("Into {}, which has photographs from these days", library.label()));
                row.upcast()
            }
            Place::New(path) => {
                let row = adw::EntryRow::new();
                let parent = path.parent().map(|parent| parent.display().to_string()).unwrap_or_default();
                row.set_title(&format!("{title} — a new library in {parent}"));
                row.set_text(&path.file_name().unwrap_or_default().to_string_lossy());
                row.connect_changed(glib::clone!(
                    #[strong] places,
                    move |row| {
                        if let Place::New(path) = &mut places.borrow_mut()[at] {
                            let name = row.text().replace('/', "-");
                            if !name.trim().is_empty() {
                                path.set_file_name(name.trim());
                            }
                        }
                    }
                ));
                row.upcast()
            }
        };
        let change = gtk::Button::with_label("Change…");
        change.set_valign(gtk::Align::Center);
        change.add_css_class("flat");
        change.connect_clicked(glib::clone!(
            #[strong] places,
            #[strong] libraries,
            #[weak] dialog,
            #[weak] row,
            move |_| {
                let chooser = gtk::FileDialog::new();
                chooser.set_title("Import these into this folder");
                let places = places.clone();
                let libraries = libraries.clone();
                glib::spawn_future_local(async move {
                    let window = dialog.root().and_downcast::<gtk::Window>();
                    let Some(path) = chooser.select_folder_future(window.as_ref()).await.ok().and_then(|file| file.path()) else { return };
                    let place = match libraries.iter().find(|library| library.path == path) {
                        Some(library) => Place::Library(library.clone()),
                        None => Place::New(path.clone()),
                    };
                    let said = match &place {
                        Place::Library(library) => format!("Into {}", library.label()),
                        Place::New(path) => format!("Into {}, as a new library", path.display()),
                    };
                    places.borrow_mut()[at] = place;
                    match row.downcast_ref::<adw::ActionRow>() {
                        Some(action) => action.set_subtitle(&said),
                        None => row.set_title(&said),
                    }
                });
            }
        ));
        match row.downcast_ref::<adw::ActionRow>() {
            Some(action) => action.add_suffix(&change),
            None => row.downcast_ref::<adw::EntryRow>().expect("a row of one of two kinds").add_suffix(&change),
        }
        group.add(&row);
    }
    places
}

fn run_import(state: &App, card: Card, places: Vec<Place>, pattern: String) {
    let total: usize = card.shoots.iter().map(|shoot| shoot.photos.len()).sum();
    let progress = adw::Toast::new(&format!("Importing {total} photographs…"));
    progress.set_timeout(0);
    progress.set_button_label(Some("Stop"));
    let stop = Arc::new(AtomicBool::new(false));
    let count = Arc::new(AtomicUsize::new(0));
    progress.connect_button_clicked(glib::clone!(
        #[strong] stop,
        move |_| stop.store(true, Ordering::Relaxed)
    ));
    state.toasts.add_toast(progress.clone());
    glib::timeout_add_local(std::time::Duration::from_millis(250), glib::clone!(
        #[strong] count,
        #[weak] progress,
        #[upgrade_or] glib::ControlFlow::Break,
        move || {
            progress.set_title(&format!("Importing {} of {total}…", count.load(Ordering::Relaxed)));
            glib::ControlFlow::Continue
        }
    ));

    let state = state.clone();
    let offset = offset();
    glib::spawn_future_local(async move {
        let jobs: Vec<(Vec<(Found, OsString)>, PathBuf)> = card
            .shoots
            .iter()
            .zip(&places)
            .map(|(shoot, place)| {
                let folder = match place {
                    Place::Library(library) => library.path.clone(),
                    Place::New(path) => path.clone(),
                };

                let mut stems: HashMap<OsString, usize> = HashMap::new();
                let photos = shoot
                    .photos
                    .iter()
                    .map(|&at| {
                        let photo = card.found[at].clone();
                        let next = stems.len() + 1;
                        let n = *stems.entry(photo.path.file_stem().unwrap_or_default().to_owned()).or_insert(next);
                        let name = import::named(&pattern, &photo, n, offset);
                        (photo, name)
                    })
                    .collect();
                (photos, folder)
            })
            .collect();
        let copied = gio::spawn_blocking(move || {
            let mut copied = numa::io::catalog::Copied::default();
            let mut before = 0;
            for (photos, folder) in &jobs {
                let listed: Vec<(&Found, OsString)> = photos.iter().map(|(photo, name)| (photo, name.clone())).collect();
                let _ = std::fs::create_dir_all(folder);
                let one = import::copy(&listed, folder, || stop.load(Ordering::Relaxed), |done| count.store(before + done, Ordering::Relaxed));
                before += photos.len();
                copied.photos += one.photos;
                copied.existing.extend(one.existing);
                copied.failed.extend(one.failed);
            }
            copied
        })
        .await
        .unwrap_or_default();
        progress.dismiss();
        finish(&state, &places, copied);
    });
}

fn finish(state: &App, places: &[Place], copied: numa::io::catalog::Copied) {
    let mut opened = None;
    let mut into: Vec<String> = Vec::new();
    for place in places {
        let library = match place {
            Place::Library(library) => Ok(library.clone()),
            Place::New(path) if path.is_dir() => state.catalog.add_library(path),
            Place::New(_) => continue,
        };
        match library.and_then(|library| state.catalog.sync_library(&library).map(|_| library)) {
            Ok(library) => {
                into.push(library.label());
                opened.get_or_insert(library.id);
            }
            Err(err) => state.toast(&format!("Copied, but {err}")),
        }
    }
    state.libraries.filter.borrow_mut().in_one_library();
    reload_libraries(state);
    if let Some(id) = opened {
        select_library(state, id);
        state.stack.set_visible_child_name("library");
    }
    into.dedup();
    let mut said = match copied.photos {
        1 => "Imported 1 photograph".to_string(),
        count => format!("Imported {count} photographs"),
    };
    if !into.is_empty() {
        said.push_str(&format!(" into {}", into.join(" and ")));
    }
    if !copied.existing.is_empty() {
        said.push_str(&format!(" · {} were already there", copied.existing.len()));
    }
    if !copied.failed.is_empty() {
        said.push_str(&format!(" · {} could not be copied", copied.failed.len()));
    }
    state.toast(&said);
}
