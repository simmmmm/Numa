use super::*;
use numa::io::import::{self, Found, Place, Shoot};
use numa::io::stand_in::{self, StandIn};
use std::collections::HashMap;

thread_local! {

    static MONITOR: RefCell<Option<gio::VolumeMonitor>> = const { RefCell::new(None) };

    static OFFERED: RefCell<Option<PathBuf>> = const { RefCell::new(None) };
}

pub(super) fn watch_cards(state: &App) {
    let monitor = gio::VolumeMonitor::get();
    monitor.connect_mount_added(glib::clone!(
        #[strong] state,
        move |_, mount| {

            rescan_in_background(&state, false);
            if let Some(root) = card_root(mount) {
                offer(&state, &mount.name(), root);
            }
        }
    ));

    monitor.connect_mount_removed(glib::clone!(
        #[strong] state,
        move |_, _| {
            follow_drive(&state);
            if OFFERED.with(|offered| offered.borrow().as_ref().is_some_and(|root| !root.exists())) {
                state.grid.card_banner.set_revealed(false);
            }
        }
    ));
    state.grid.card_banner.set_button_label(Some("Import…"));
    state.grid.card_banner.connect_button_clicked(glib::clone!(
        #[strong] state,
        move |banner| {
            banner.set_revealed(false);
            let root = OFFERED.with(|offered| offered.borrow_mut().take());
            if let Some(window) = state.stack.root().and_downcast::<adw::ApplicationWindow>() {
                import_dialog(&state, &window, root);
            }
        }
    ));
    MONITOR.with(|kept| *kept.borrow_mut() = Some(monitor));
}

fn offer(state: &App, name: &str, root: PathBuf) {

    if tethering() {
        return;
    }
    let (state, name) = (state.clone(), name.to_string());
    glib::spawn_future_local(async move {
        let card = read_card(&state, root.clone()).await;
        let new = card.already.iter().filter(|there| there.is_none()).count();
        if new == 0 {
            return;
        }
        let photographs = if new == 1 { "photograph" } else { "photographs" };
        state.grid.card_banner.set_title(&format!("{name} · {} new {photographs}", places::grouped(new as i64)));
        OFFERED.with(|offered| *offered.borrow_mut() = Some(root));
        state.grid.card_banner.set_revealed(true);
    });
}

fn card_root(mount: &gio::Mount) -> Option<PathBuf> {
    let root = mount.root().path()?;
    let has = |dir: &Path| dir.join("DCIM").is_dir();
    let camera = matches!(mount.root().uri_scheme().as_deref(), Some("gphoto2" | "mtp"));
    let slots = || std::fs::read_dir(&root).ok().is_some_and(|mut entries| entries.any(|entry| entry.is_ok_and(|entry| has(&entry.path()))));

    (camera || has(&root) || slots()).then_some(root)
}

fn cards() -> Vec<(String, PathBuf)> {
    gio::VolumeMonitor::get().mounts().iter().filter_map(|mount| Some((mount.name().to_string(), card_root(mount)?))).collect()
}

pub(super) struct Card {
    pub(super) found: Vec<Found>,

    pub(super) already: Vec<Option<PathBuf>>,
    pub(super) shoots: Vec<Shoot>,

    pub(super) stand_ins: Vec<StandIn>,
}

const RAWS_ONLY: &str = "import_raws_only";

fn raws_row(state: &App) -> adw::SwitchRow {
    let raws = adw::SwitchRow::new();
    raws.set_title("Only the Raws");
    raws.set_subtitle("A JPEG or HEIF beside its raw stays on the card");
    raws.set_active(state.catalog.recall::<bool>(RAWS_ONLY).unwrap_or(false));
    raws
}

fn read_again_for_raws<F: Fn(PathBuf) + 'static>(raws: &adw::SwitchRow, state: &App, read: &Rc<RefCell<Option<(Card, Rc<RefCell<Vec<Place>>>, PathBuf)>>>, choose: &Rc<F>) {
    raws.connect_active_notify(glib::clone!(
        #[strong] state,
        #[strong] read,
        #[strong] choose,
        move |row| {
            state.catalog.remember(RAWS_ONLY, &row.is_active());
            let path = read.borrow().as_ref().map(|(_, _, path)| path.clone());
            if let Some(path) = path {
                choose(path);
            }
        }
    ));
}

pub(super) fn import_dialog(state: &App, window: &adw::ApplicationWindow, from: Option<PathBuf>) {
    let dialog = adw::Dialog::new();
    dialog.set_title("Import");
    dialog.set_content_width(560);
    dialog.set_content_height(712);
    let view = adw::ToolbarView::new();
    view.add_top_bar(&adw::HeaderBar::new());
    let page = adw::PreferencesPage::new();
    view.set_content(Some(&page));

    let go = import_button();
    view.add_bottom_bar(&go);
    dialog.set_child(Some(&view));

    let (sources, source) = sources_row(from);
    let from_group = adw::PreferencesGroup::new();
    from_group.set_title("From");
    from_group.add(&source);
    let raws = raws_row(state);
    from_group.add(&raws);

    if let Some(tether) = tether_row(state, &dialog, window) {
        from_group.add(&tether);
    }
    page.add(&from_group);

    let body = adw::PreferencesGroup::new();
    page.add(&body);
    let copies = Copies::new(state);
    page.add(&copies.group);
    let rename = adw::EntryRow::new();
    rename.set_title("Rename — {date} {time} {n} {name}; empty keeps the camera's names");
    let names_group = adw::PreferencesGroup::new();
    names_group.set_title("Names");
    names_group.add(&rename);
    page.add(&names_group);

    let sources = Rc::new(sources);
    let read: Rc<RefCell<Option<(Card, Rc<RefCell<Vec<Place>>>, PathBuf)>>> = Rc::new(RefCell::new(None));
    let marks: import_look::Marks = Rc::default();

    let jpegs_on = Rc::new(Cell::new(true));
    let placed: Rc<RefCell<HashMap<PathBuf, (f64, f64)>>> = Rc::default();
    let shown: Rc<RefCell<Vec<adw::PreferencesGroup>>> = Rc::new(RefCell::new(vec![body]));
    let count = import_count(&read, &marks, &go);
    let choose = glib::clone!(
        #[strong] state,
        #[strong] read,
        #[strong] shown,
        #[strong] page,
        #[strong] marks,
        #[strong] count,
        #[strong] copies,
        #[strong] names_group,
        #[strong] jpegs_on,
        #[strong] placed,
        #[weak] go,
        #[weak] dialog,
        move |path: PathBuf| {
            go.set_sensitive(false);
            marks.borrow_mut().clear();
            jpegs_on.set(true);
            placed.borrow_mut().clear();
            let fresh = adw::PreferencesGroup::new();
            fresh.set_title("Reading the card…");

            fresh.set_header_suffix(Some(&spinner("Reading the card")));
            for group in shown.borrow().iter() {
                page.remove(group);
            }
            add_above(&page, &[&fresh], &[&copies.group, &names_group]);
            *shown.borrow_mut() = vec![fresh.clone()];
            let (state, read, shown, page, marks, count) = (state.clone(), read.clone(), shown.clone(), page.clone(), marks.clone(), count.clone());
            let (copies, names_group, jpegs_on, placed) = (copies.clone(), names_group.clone(), jpegs_on.clone(), placed.clone());
            glib::spawn_future_local(async move {
                let card = read_card(&state, path.clone()).await;
                fresh.set_header_suffix(gtk::Widget::NONE);
                let places = fill(&state, &dialog, &fresh, &card);
                copies.set_bytes(import_look::new_frames(&card).iter().map(|at| card.found[*at].size).sum());

                if !import_look::new_frames(&card).is_empty() {
                    let look = import_look::look_group(&dialog, &card, &marks, count.clone());
                    page.remove(&fresh);
                    add_above(&page, &[&look, &fresh], &[&copies.group, &names_group]);
                    shown.borrow_mut().insert(0, look);
                }

                let later = later_groups(&card, &jpegs_on, placed);
                add_above(&page, &later.iter().collect::<Vec<_>>(), &[&copies.group, &names_group]);
                shown.borrow_mut().extend(later);
                *read.borrow_mut() = Some((card, places, path));
                count();
            });
        }
    );
    let choose = Rc::new(choose);
    read_again_for_raws(&raws, state, &read, &choose);
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
        #[strong] marks,
        #[strong] sources,
        #[strong] copies,
        #[strong] jpegs_on,
        #[strong] placed,
        #[weak] source,
        #[weak] dialog,
        #[weak] rename,
        move |_| {
            let Some(read) = read.borrow_mut().take() else { return };
            dialog.close();
            let name = sources.get(source.selected() as usize).map(|(name, _)| name.clone());
            let later = (jpegs_on.get(), placed.borrow().clone());
            begin(&state, read, &marks, &copies, &rename.text(), name, later);
        }
    ));

    dialog.present(Some(window));
    match sources.first() {
        Some((_, path)) => choose(path.clone()),
        None => source.set_selected(0),
    }
}

fn sources_row(from: Option<PathBuf>) -> (Vec<(String, PathBuf)>, adw::ComboRow) {
    let mut sources = cards();
    if let Some(from) = from {
        if !sources.iter().any(|(_, path)| *path == from) {
            sources.insert(0, (from.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default(), from));
        }
    }
    let names: Vec<String> = sources.iter().map(|(name, _)| name.clone()).chain(["Another folder…".to_string()]).collect();
    let source = adw::ComboRow::new();
    source.set_title("Card or camera");
    source.set_model(Some(&gtk::StringList::new(&names.iter().map(String::as_str).collect::<Vec<_>>())));
    (sources, source)
}

fn add_above(page: &adw::PreferencesPage, groups: &[&adw::PreferencesGroup], after: &[&adw::PreferencesGroup]) {
    for group in after {
        page.remove(*group);
    }
    for group in groups.iter().chain(after) {
        page.add(*group);
    }
}

fn later_groups(card: &Card, jpegs_on: &Rc<Cell<bool>>, placed: Rc<RefCell<HashMap<PathBuf, (f64, f64)>>>) -> Vec<adw::PreferencesGroup> {
    let frames: Vec<PathBuf> = import_look::new_frames(card).iter().map(|at| card.found[*at].path.clone()).collect();
    let mut groups = Vec::new();
    if !card.stand_ins.is_empty() {
        groups.push(import_jpegs::group(&card.stand_ins, jpegs_on));
    }
    if !frames.is_empty() {
        groups.push(tracks::import_group(frames, placed));
    }
    groups
}

fn import_button() -> gtk::Button {
    let go = primary_button("Import");
    go.add_css_class("pill");
    go.set_halign(gtk::Align::Center);
    go.set_margin_top(12);
    go.set_margin_bottom(12);
    go.set_sensitive(false);
    go
}

fn import_count(read: &Rc<RefCell<Option<(Card, Rc<RefCell<Vec<Place>>>, PathBuf)>>>, marks: &import_look::Marks, go: &gtk::Button) -> Rc<dyn Fn()> {
    Rc::new(glib::clone!(
        #[strong] read,
        #[strong] marks,
        #[weak] go,
        move || {
            let new = read.borrow().as_ref().map_or(0, |(card, _, _)| import_look::new_frames(card).len());
            let coming = new.saturating_sub(import_look::left_on_card(&marks));
            go.set_label(&match (new, coming) {
                (0, _) => "Nothing new to import".to_string(),
                (_, 0) => "Everything left on the card".to_string(),
                (_, coming) => format!("Import {}", places::grouped(coming as i64)),
            });
            go.set_sensitive(coming > 0);
        }
    ))
}

fn begin(
    state: &App,
    (card, places, root): (Card, Rc<RefCell<Vec<Place>>>, PathBuf),
    marks: &import_look::Marks,
    copies: &Copies,
    pattern: &str,
    name: Option<String>,
    (jpegs, positions): (bool, HashMap<PathBuf, (f64, f64)>),
) {
    let places = places.borrow().clone();
    let marks: HashMap<PathBuf, (u8, Flag)> = marks.borrow().iter().map(|(at, mark)| (card.found[*at].path.clone(), *mark)).collect();
    let second = copies.second();
    let items = import_done::plan(&card, &places, pattern, &marks, second.as_deref());
    let run = import_done::Run {
        card_name: name.unwrap_or_else(|| root.file_name().unwrap_or_default().to_string_lossy().into_owned()),
        card_root: root,
        older: card.found.iter().zip(&card.already).filter_map(|(photo, there)| Some((photo.path.clone(), there.clone()?, photo.size))).collect(),
        second,
        check: copies.check(),
        marks: Rc::new(marks),
        places,
        stand_ins: Rc::new(match jpegs {
            true => card.stand_ins.iter().map(|stand_in| (card.found[stand_in.raw].path.clone(), stand_in.jpeg.clone())).collect(),
            false => HashMap::new(),
        }),
        positions: Rc::new(positions),
    };
    import_done::run_checked(state, items, run);
}

#[derive(Clone)]
struct Copies {
    group: adw::PreferencesGroup,
    state: App,
    rows: Rc<RefCell<Vec<adw::PreferencesRow>>>,

    bytes: Rc<Cell<u64>>,
}

impl Copies {
    fn new(state: &App) -> Self {
        let group = adw::PreferencesGroup::new();
        group.set_title("Copies");
        let copies = Self { group, state: state.clone(), rows: Rc::default(), bytes: Rc::default() };
        copies.build();
        copies
    }

    fn second(&self) -> Option<PathBuf> {
        let on = self.state.catalog.setting(import_done::SECOND_ON).as_deref() != Some("no");
        self.state.catalog.setting(import_done::SECOND_COPY).map(PathBuf::from).filter(|root| on && root.is_dir())
    }

    fn check(&self) -> bool {
        self.state.catalog.setting(import_done::CHECK).as_deref() != Some("no")
    }

    fn set_bytes(&self, bytes: u64) {
        self.bytes.set(bytes);
        self.build();
    }

    fn build(&self) {
        for row in self.rows.borrow_mut().drain(..) {
            self.group.remove(&row);
        }
        let state = &self.state;
        let place = state.catalog.setting(import_done::SECOND_COPY).map(PathBuf::from);
        let mut rows: Vec<adw::PreferencesRow> = Vec::new();
        match &place {
            None => {
                let add = adw::ActionRow::new();
                add.set_title("Add a Second Copy…");
                add.set_subtitle("A folder on another drive, so the card can be formatted");
                add.add_prefix(&gtk::Image::from_icon_name("list-add-symbolic"));
                add.set_activatable(true);
                add.connect_activated(glib::clone!(
                    #[strong(rename_to = copies)] self,
                    move |row| copies.choose(row)
                ));
                rows.push(add.upcast());
            }
            Some(root) => {
                let row = adw::ActionRow::new();
                row.set_title("Second Copy");
                let there = root.is_dir();
                let said = import_done::place_label(root);
                set_row_subtitle(&row, &if there { said } else { format!("Not connected \u{b7} {said}") });
                let change = gtk::Button::with_label("Change…");
                change.set_valign(gtk::Align::Center);
                change.add_css_class("flat");
                change.connect_clicked(glib::clone!(
                    #[strong(rename_to = copies)] self,
                    move |button| copies.choose(button)
                ));
                row.add_suffix(&change);
                let switch = gtk::Switch::new();
                switch.set_valign(gtk::Align::Center);
                switch.set_active(there && state.catalog.setting(import_done::SECOND_ON).as_deref() != Some("no"));
                switch.set_sensitive(there);
                switch.connect_active_notify(glib::clone!(
                    #[strong(rename_to = copies)] self,
                    move |switch| {
                        let _ = copies.state.catalog.set_setting(import_done::SECOND_ON, if switch.is_active() { "yes" } else { "no" });
                        copies.build();
                    }
                ));
                row.add_suffix(&switch);
                row.set_activatable_widget(Some(&switch));
                rows.push(row.upcast());
            }
        }
        let check = adw::SwitchRow::new();
        check.set_title(if self.second().is_some() { "Check Both Copies" } else { "Check the Copy" });
        let copies = if self.second().is_some() { 2 } else { 1 };

        let seconds = (self.bytes.get() * copies) as f64 / 1e9;
        let cost = match seconds {
            s if s < 1.0 => String::new(),
            s if s < 90.0 => format!(" \u{b7} about {} s more", s.round() as u64),
            s => format!(" \u{b7} about {} min more", (s / 60.0).round() as u64),
        };
        check.set_subtitle(&format!("Every file read back and compared{cost}"));
        check.set_active(self.check());
        check.connect_active_notify(glib::clone!(
            #[strong] state,
            move |row| {
                let _ = state.catalog.set_setting(import_done::CHECK, if row.is_active() { "yes" } else { "no" });
            }
        ));
        rows.push(check.upcast());
        for row in &rows {
            self.group.add(row);
        }
        *self.rows.borrow_mut() = rows;
    }

    fn choose(&self, from: &impl IsA<gtk::Widget>) {
        let chooser = gtk::FileDialog::new();
        chooser.set_title("Keep a second copy of every import here");
        let window = from.root().and_downcast::<gtk::Window>();
        let copies = self.clone();
        glib::spawn_future_local(async move {
            let Some(path) = chooser.select_folder_future(window.as_ref()).await.ok().and_then(|file| file.path()) else { return };
            let _ = copies.state.catalog.set_setting(import_done::SECOND_COPY, &path.to_string_lossy());
            let _ = copies.state.catalog.set_setting(import_done::SECOND_ON, "yes");

            let _ = copies.state.catalog.set_setting(import_done::CHECK, "yes");
            copies.build();
        });
    }
}

async fn read_card(state: &App, path: PathBuf) -> Card {
    let mut known = Vec::new();
    for library in state.libraries.all.borrow().iter() {
        known.extend(state.catalog.paths(library.id).unwrap_or_default());
    }
    let raws_only = state.catalog.recall::<bool>(RAWS_ONLY).unwrap_or(false);
    let read = gio::spawn_blocking(move || {
        let found = import::scan(&path);
        let found = if raws_only { import::raws_only(found) } else { found };
        let already = import::already(&found, &import::by_name(known.iter().cloned()));

        let raws: Vec<(usize, PathBuf)> =
            found.iter().enumerate().filter(|(at, photo)| already[*at].is_none() && raw::is_raw(&photo.path)).map(|(at, photo)| (at, photo.path.clone())).collect();
        let stand_ins = stand_in::find(&raws, &known);
        (found, already, stand_ins)
    })
    .await;
    let (found, already, stand_ins) = read.unwrap_or_default();
    let new: Vec<usize> = (0..found.len()).filter(|at| already[*at].is_none()).collect();
    let shoots = import::shoots(&found, &new);
    Card { found, already, shoots, stand_ins }
}

fn tether_row(state: &App, dialog: &adw::Dialog, window: &adw::ApplicationWindow) -> Option<adw::ActionRow> {
    if !numa::tether::available() {
        return None;
    }
    let tether = adw::ActionRow::new();
    tether.set_title("Shoot Tethered…");
    tether.set_subtitle("Each photograph comes in as it is taken");
    tether.set_activatable(true);
    tether.add_suffix(&gtk::Image::from_icon_name("go-next-symbolic"));
    tether.connect_activated(glib::clone!(
        #[strong] state,
        #[weak] dialog,
        #[weak] window,
        move |_| {
            dialog.close();
            tether_dialog(&state, &window);
        }
    ));
    Some(tether)
}

pub(super) fn import_offset() -> i64 {
    glib::DateTime::now_local().map_or(0, |now| now.utc_offset().as_seconds())
}

pub(super) fn days(first: i64, last: i64) -> String {
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

pub(super) fn import_spans(state: &App) -> Vec<import::Span> {
    state
        .libraries
        .all
        .borrow()
        .iter()
        .map(|library| {
            let glance = state.catalog.glance(library.id, None, None).ok();
            (library.clone(), glance.as_ref().and_then(|g| g.first), glance.as_ref().and_then(|g| g.last))
        })
        .collect()
}

fn fill(state: &App, dialog: &adw::Dialog, group: &adw::PreferencesGroup, card: &Card) -> Rc<RefCell<Vec<Place>>> {
    let libraries = state.libraries.all.borrow().clone();
    let spans = import_spans(state);
    let places = Rc::new(RefCell::new(card.shoots.iter().map(|shoot| import::suggest(shoot, &spans, import_offset())).collect::<Vec<_>>()));

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
