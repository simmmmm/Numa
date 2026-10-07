use super::*;

const DAILY: &str = "tonight_daily";
const MADE: &str = "tonight_made";

const ROOT: &str = "tonight_root";
const SIZE: &str = "tonight_size";
const SIZES: [(&str, Option<u32>); 4] = [
    ("Long edge 1080", Some(1080)),
    ("Long edge 2048", Some(2048)),
    ("Long edge 3840", Some(3840)),
    ("Full size", None),
];
const DEFAULT_SIZE: usize = 1;

const SHOWN: usize = 6;

fn day_of(photo: &Photo) -> i64 {
    photo.taken.unwrap_or(photo.mtime).div_euclid(86_400)
}

fn today() -> i64 {
    let now = glib::DateTime::now_local().ok();
    now.map_or(0, |now| (now.to_unix() + now.utc_offset().as_seconds()).div_euclid(86_400))
}

fn day_said(day: i64) -> String {
    let date = glib::DateTime::from_unix_utc(day * 86_400).ok();
    let this_year = glib::DateTime::now_local().ok().map(|now| now.year());
    date.and_then(|date| date.format(if Some(date.year()) == this_year { "%-d %b" } else { "%-d %b %Y" }).ok())
        .map(|said| said.to_string())
        .unwrap_or_default()
}

struct Pack {
    photos: Vec<Photo>,
    library: Library,
    day: i64,

    chosen: bool,
}

fn picks_of(state: &App, library: &Library, day: Option<i64>) -> Option<Pack> {
    let picks = state.catalog.photos(library.id, &Filter { flag: Some(Flag::Picked), ..Filter::default() }).ok()?;
    let day = day.or_else(|| picks.iter().map(day_of).max())?;
    let photos: Vec<Photo> = picks.into_iter().filter(|photo| day_of(photo) == day).collect();
    (!photos.is_empty()).then(|| Pack { photos, library: library.clone(), day, chosen: false })
}

fn pack_now(state: &App) -> Option<Pack> {
    let library = state.libraries.current.borrow().clone()?;
    let selected = selected_ids(state);
    if selected.is_empty() {
        return picks_of(state, &library, None);
    }
    let cards = state.grid.cards.borrow();
    let photos: Vec<Photo> = selected.iter().filter_map(|id| cards.get(id).cloned()).collect();
    Some(Pack { photos, library, day: today(), chosen: true })
}

fn folder_for(state: &App, library: &Library, day: i64) -> PathBuf {
    let root = state
        .catalog
        .setting(ROOT)
        .map(PathBuf::from)
        .or_else(|| dirs::picture_dir().map(|pictures| pictures.join("Tonight")))
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join("Pictures/Tonight"));
    root.join(format!("{} \u{b7} {}", library.label(), day_said(day)).replace('/', "-"))
}

fn said_path(path: &Path) -> String {
    match dirs::home_dir().and_then(|home| path.strip_prefix(home).ok().map(Path::to_path_buf)) {
        Some(inside) => format!("~/{}", inside.display()),
        None => path.display().to_string(),
    }
}

fn pack_settings(state: &App, edge: Option<u32>, folder: PathBuf) -> export::ExportSettings {
    let mine = state.export.settings.borrow();
    export::ExportSettings {
        template: "{stem}".into(),
        size: edge.map_or(export::Size::Full, export::Size::LongEdge),
        folder: Some(folder),
        creator: mine.creator.clone(),
        copyright: mine.copyright.clone(),
        keywords: mine.keywords,
        strip_location: mine.strip_location,
        ..export::ExportSettings::default()
    }
}

fn chosen_size(state: &App) -> usize {
    state.catalog.setting(SIZE).and_then(|size| size.parse().ok()).filter(|size| *size < SIZES.len()).unwrap_or(DEFAULT_SIZE)
}

fn make(state: &App, photos: &[Photo], settings: export::ExportSettings, by_itself: bool, then: impl FnOnce(&App) + 'static) {
    let Some(folder) = settings.folder.clone() else { return };
    let jobs = photo_jobs(state, photos.iter().map(|photo| (photo.id, photo.path.clone())));
    exporting::run_export_then(state, jobs, settings, folder.clone(), move |state, written, failures, _| {
        let mut said = format!("{} in Tonight", places::grouped(written as i64));
        if !failures.is_empty() {
            said.push_str(&format!(", {} failed", failures.len()));
        }
        let toast = adw::Toast::new(&glib::markup_escape_text(&said));
        if by_itself {
            toast.set_timeout(0);
        }
        if written > 0 {
            toast.set_button_label(Some("Open Folder"));
            let window = state.stack.root().and_downcast::<gtk::Window>();
            toast.connect_button_clicked(move |_| {
                gtk::FileLauncher::new(Some(&gio::File::for_path(&folder))).launch(window.as_ref(), None::<&gio::Cancellable>, |result| {
                    if let Err(err) = result {
                        log::warn!("could not open the Tonight folder: {err}");
                    }
                });
            });
        }
        state.toasts.add_toast(toast);
        then(state);
    });
}

pub(super) fn install_tonight_action(state: &App, window: &adw::ApplicationWindow) {
    let action = gio::SimpleAction::new("tonight", None);
    action.connect_activate(glib::clone!(
        #[strong] state,
        #[weak] window,
        move |_, _| tonight_dialog(&state, &window)
    ));
    window.add_action(&action);
}

fn tonight_dialog(state: &App, window: &adw::ApplicationWindow) {
    let Some(pack) = pack_now(state) else {
        state.toast("No picks yet \u{2014} pick the ones for tonight, or select them");
        return;
    };
    let dialog = adw::Dialog::new();
    dialog.set_title("Tonight");
    dialog.set_content_width(520);
    let view = adw::ToolbarView::new();
    view.add_top_bar(&adw::HeaderBar::new());
    let page = adw::PreferencesPage::new();
    view.set_content(Some(&page));

    let top = adw::PreferencesGroup::new();
    let strip = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    strip.add_css_class("card-strip");
    let pictures: Vec<gtk::Picture> = pack.photos.iter().take(SHOWN).map(|_| frame()).collect();
    for picture in &pictures {
        strip.append(picture);
    }
    let count = places::grouped(pack.photos.len() as i64);
    let (picks, their) = if pack.photos.len() == 1 { ("pick", "its") } else { ("picks", "their") };
    let what = match (pack.chosen, pack.day == today()) {
        (true, _) => format!("{count} selected, with {their} look."),
        (false, true) => format!("{count} {picks} from today, with {their} look."),
        (false, false) => format!("{count} {picks} from {}, with {their} look.", day_said(pack.day)),
    };
    let line = gtk::Label::new(Some(&what));
    line.add_css_class("dim-label");
    line.set_xalign(0.0);
    line.set_wrap(true);
    let column = gtk::Box::new(gtk::Orientation::Vertical, 12);
    column.append(&strip);
    column.append(&line);
    top.add(&column);
    page.add(&top);
    fill_frames(state, pack.photos.iter().take(SHOWN).cloned().collect(), pictures);

    let rows = adw::PreferencesGroup::new();
    let size = adw::ComboRow::new();
    size.set_title("Size");
    size.set_model(Some(&gtk::StringList::new(&SIZES.map(|(name, _)| name))));
    size.set_selected(chosen_size(state) as u32);
    rows.add(&size);
    let folder = Rc::new(RefCell::new(folder_for(state, &pack.library, pack.day)));
    let into = adw::ActionRow::new();
    into.set_title("Into");
    set_row_subtitle(&into, &said_path(&folder.borrow()));
    let change = gtk::Button::with_label("Change…");
    change.set_valign(gtk::Align::Center);
    change.add_css_class("flat");
    change.connect_clicked(glib::clone!(
        #[strong] state,
        #[strong] folder,
        #[weak] into,
        move |button| {
            let chooser = gtk::FileDialog::new();
            chooser.set_title("Make tonight's pack in this folder");
            let window = button.root().and_downcast::<gtk::Window>();
            let (state, folder) = (state.clone(), folder.clone());
            glib::spawn_future_local(async move {
                let Some(path) = chooser.select_folder_future(window.as_ref()).await.ok().and_then(|file| file.path()) else { return };

                if let Some(parent) = path.parent() {
                    let _ = state.catalog.set_setting(ROOT, &parent.to_string_lossy());
                }
                set_row_subtitle(&into, &said_path(&path));
                *folder.borrow_mut() = path;
            });
        }
    ));
    into.add_suffix(&change);
    rows.add(&into);
    let daily = adw::SwitchRow::new();
    daily.set_title("Every Day of the Trip");
    daily.set_subtitle("A folder per evening, by itself");
    let key = format!("{DAILY}:{}", pack.library.id);
    daily.set_active(state.catalog.setting(&key).as_deref() == Some("yes"));
    daily.connect_active_notify(glib::clone!(
        #[strong] state,
        move |row| {
            let _ = state.catalog.set_setting(&key, if row.is_active() { "yes" } else { "no" });
        }
    ));
    rows.add(&daily);
    page.add(&rows);

    let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    buttons.set_halign(gtk::Align::Center);
    buttons.set_margin_top(12);
    buttons.set_margin_bottom(16);
    let cancel = gtk::Button::with_label("Cancel");
    cancel.add_css_class("pill");
    cancel.connect_clicked(glib::clone!(
        #[weak] dialog,
        move |_| {
            dialog.close();
        }
    ));
    let go = primary_button(&format!("Make {count}"));
    go.add_css_class("pill");
    let pack = Rc::new(pack);
    go.connect_clicked(glib::clone!(
        #[strong] state,
        #[strong] pack,
        #[strong] folder,
        #[weak] size,
        #[weak] dialog,
        move |_| {
            let chosen = (size.selected() as usize).min(SIZES.len() - 1);
            let _ = state.catalog.set_setting(SIZE, &chosen.to_string());
            dialog.close();
            if !pack.chosen {
                remember_made(&state, pack.library.id, pack.day);
            }
            make(&state, &pack.photos, pack_settings(&state, SIZES[chosen].1, folder.borrow().clone()), false, |_| {});
        }
    ));
    buttons.append(&cancel);
    buttons.append(&go);
    view.add_bottom_bar(&buttons);
    dialog.set_child(Some(&view));
    dialog.present(Some(window));
}

fn frame() -> gtk::Picture {
    let picture = gtk::Picture::new();
    picture.set_content_fit(gtk::ContentFit::Cover);
    picture.set_size_request(74, 50);
    picture.set_overflow(gtk::Overflow::Hidden);
    picture
}

fn fill_frames(state: &App, photos: Vec<Photo>, pictures: Vec<gtk::Picture>) {
    let edits: Vec<Option<String>> = photos.iter().map(|photo| state.catalog.edits_json(photo.id).ok().flatten()).collect();
    glib::spawn_future_local(async move {
        for ((photo, edits), picture) in photos.into_iter().zip(edits).zip(pictures) {
            let image = gio::spawn_blocking(move || {
                let edits = edits.as_deref();
                numa::io::thumbs::load_cached(&photo.path, photo.mtime, 160, edits).or_else(|| numa::io::thumbs::load(&photo.path, photo.mtime, 160, edits).ok())
            })
            .await
            .ok()
            .flatten();
            if picture.root().is_none() {
                return;
            }
            if let Some(image) = image {
                picture.set_paintable(Some(&texture_from(&image)));
            }
        }
    });
}

fn made(state: &App, library: i64) -> Vec<i64> {
    let said = state.catalog.setting(&format!("{MADE}:{library}")).unwrap_or_default();
    said.split_whitespace().filter_map(|day| day.parse().ok()).collect()
}

fn remember_made(state: &App, library: i64, day: i64) {
    let mut days = made(state, library);
    days.retain(|known| *known != day);
    days.push(day);
    let kept: Vec<String> = days.iter().rev().take(60).rev().map(i64::to_string).collect();
    let _ = state.catalog.set_setting(&format!("{MADE}:{library}"), &kept.join(" "));
}

pub(super) fn after_import(state: &App, report: &numa::io::offload::Report) {
    let libraries = state.libraries.all.borrow().clone();
    let mut packs = Vec::new();
    for library in libraries.iter().filter(|library| state.catalog.setting(&format!("{DAILY}:{}", library.id)).as_deref() == Some("yes")) {
        let landed: std::collections::HashSet<&Path> = report.landed.iter().map(|landed| landed.to.as_path()).filter(|to| to.starts_with(&library.path)).collect();
        if landed.is_empty() {
            continue;
        }
        let photos = state.catalog.photos(library.id, &Filter::default()).unwrap_or_default();
        let mut days: Vec<i64> = photos.iter().filter(|photo| landed.contains(photo.path.as_path())).map(day_of).collect();
        days.sort_unstable();
        days.dedup();
        let done = made(state, library.id);
        for day in days.into_iter().filter(|day| !done.contains(day)) {
            if let Some(pack) = picks_of(state, library, Some(day)) {
                remember_made(state, library.id, day);
                packs.push(pack);
            }
        }
    }
    make_each(state, packs);
}

fn make_each(state: &App, mut packs: Vec<Pack>) {
    if packs.is_empty() {
        return;
    }
    let pack = packs.remove(0);
    log::info!("tonight: {} picks of {} made by themselves", pack.photos.len(), day_said(pack.day));
    let settings = pack_settings(state, SIZES[chosen_size(state)].1, folder_for(state, &pack.library, pack.day));
    make(state, &pack.photos, settings, true, move |state| make_each(state, packs));
}
