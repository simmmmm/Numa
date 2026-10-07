use super::*;
use numa::io::import::{self, Place};
use numa::tether::{Event, Session, Waiting};

const SAME_EDIT: &str = "tether-same-edit";

thread_local! {
    static TETHER: RefCell<Option<Tether>> = const { RefCell::new(None) };
    static BANNER: adw::Banner = adw::Banner::new("");
    static GENERATION: Cell<u64> = const { Cell::new(0) };

    static PILL: gtk::MenuButton = gtk::MenuButton::builder()
        .child(&adw::ButtonContent::builder().icon_name("camera-photo-symbolic").build())
        .tooltip_text("Shoot tethered or import")
        .valign(gtk::Align::Center)
        .visible(false)
        .build();

    static FOUND: RefCell<Option<String>> = const { RefCell::new(None) };
    static LOOKING: Cell<bool> = const { Cell::new(false) };
    static AGAIN: Cell<bool> = const { Cell::new(false) };

    static CABLES: RefCell<Vec<gio::FileMonitor>> = const { RefCell::new(Vec::new()) };
}

struct Tether {
    state: App,

    generation: u64,
    _session: Session,
    library: Library,
    camera: Option<String>,
    waiting: Option<Waiting>,
    received: usize,

    last: Option<i64>,
    same_edit: bool,

    queue: Vec<PathBuf>,
    taking: bool,
}

pub(super) fn tethering() -> bool {
    TETHER.with(|tether| tether.borrow().is_some())
}

pub(super) fn tethering_into(library: i64) -> bool {
    TETHER.with(|tether| tether.borrow().as_ref().is_some_and(|tether| tether.library.id == library))
}

pub(super) fn tether_banner() -> adw::Banner {
    BANNER.with(Clone::clone)
}

pub(super) fn camera_pill() -> gtk::MenuButton {
    PILL.with(Clone::clone)
}

pub(super) fn install_tethering(state: &App, window: &adw::ApplicationWindow) {
    let start = gio::SimpleAction::new("tether", None);

    start.set_enabled(false);
    glib::timeout_add_seconds_local_once(2, glib::clone!(
        #[strong] state,
        move || {
            glib::spawn_future_local(async move {
                let available = gio::spawn_blocking(numa::tether::available).await.unwrap_or(false);
                enable_actions(&state);
                if available {
                    watch_cables();
                }
            });
        }
    ));
    start.connect_activate(glib::clone!(
        #[strong] state,
        #[weak] window,
        move |_, _| tether_dialog(&state, &window)
    ));
    window.add_action(&start);
    let import = gio::SimpleAction::new("import-photos", None);
    import.connect_activate(glib::clone!(
        #[strong] state,
        #[weak] window,
        move |_, _| import_from_camera(&state, &window)
    ));
    window.add_action(&import);
    let offers = gio::Menu::new();
    offers.append(Some("Shoot Tethered…"), Some("win.tether"));
    offers.append(Some("Import…"), Some("win.import-photos"));
    camera_pill().set_menu_model(Some(&offers));
    let stop = gio::SimpleAction::new("tether-stop", None);
    stop.set_enabled(false);
    stop.connect_activate(glib::clone!(
        #[strong] state,
        move |_, _| stop_tethering(&state)
    ));
    window.add_action(&stop);
    let banner = tether_banner();
    banner.set_button_label(Some("Stop"));
    banner.connect_button_clicked(glib::clone!(
        #[strong] state,
        move |_| stop_tethering(&state)
    ));
}

fn enable_actions(state: &App) {
    let Some(window) = state.stack.root().and_downcast::<adw::ApplicationWindow>() else { return };
    let running = tethering();
    for (name, enabled) in [("tether", !running && numa::tether::available()), ("tether-stop", running)] {
        if let Some(action) = window.lookup_action(name).and_downcast::<gio::SimpleAction>() {
            action.set_enabled(enabled);
        }
    }
    light_pill();
}

fn import_from_camera(state: &App, window: &adw::ApplicationWindow) {
    let camera = gio::VolumeMonitor::get()
        .volumes()
        .into_iter()
        .find(|volume| matches!(volume.activation_root().and_then(|root| root.uri_scheme()).as_deref(), Some("gphoto2" | "mtp")));
    let Some(volume) = camera else { return import_dialog(state, window, None) };
    if let Some(mount) = volume.get_mount() {
        return import_dialog(state, window, mount.root().path());
    }
    volume.clone().mount(gio::MountMountFlags::NONE, None::<&gio::MountOperation>, gio::Cancellable::NONE, glib::clone!(
        #[strong] state,
        #[weak] window,
        move |done| {
            if let Err(err) = &done {
                log::warn!("import: the camera did not mount: {err}");
            }
            import_dialog(&state, &window, volume.get_mount().and_then(|mount| mount.root().path()));
        }
    ));
}

fn watch_cables() {
    look_for_camera();
    let Ok(buses) = std::fs::read_dir("/dev/bus/usb") else { return };
    let watches: Vec<gio::FileMonitor> = buses
        .flatten()
        .filter_map(|bus| gio::File::for_path(bus.path()).monitor_directory(gio::FileMonitorFlags::NONE, gio::Cancellable::NONE).ok())
        .collect();
    for watch in &watches {
        watch.connect_changed(|_, _, _, event| {
            if matches!(event, gio::FileMonitorEvent::Created | gio::FileMonitorEvent::Deleted) {
                look_for_camera();
            }
        });
    }
    CABLES.with(|kept| *kept.borrow_mut() = watches);
}

fn look_for_camera() {
    if LOOKING.with(|looking| looking.replace(true)) {
        AGAIN.with(|again| again.set(true));
        return;
    }
    glib::spawn_future_local(async {
        let found = gio::spawn_blocking(numa::tether::cameras).await.unwrap_or_default();
        FOUND.with(|kept| *kept.borrow_mut() = found.into_iter().next());
        LOOKING.with(|looking| looking.set(false));
        light_pill();
        if AGAIN.with(|again| again.replace(false)) {
            look_for_camera();
        }
    });
}

fn light_pill() {
    let found = FOUND.with(|found| found.borrow().clone());
    PILL.with(|pill| {
        pill.set_can_target(!tethering());
        if let (Some(name), Some(content)) = (&found, pill.child().and_downcast::<adw::ButtonContent>()) {
            content.set_label(name);
        }
        pill.set_visible(found.is_some());
    });
}

pub(super) fn tether_dialog(state: &App, window: &adw::ApplicationWindow) {
    let dialog = adw::Dialog::new();
    dialog.set_title("Shoot Tethered");
    dialog.set_content_width(480);
    let view = adw::ToolbarView::new();
    view.add_top_bar(&adw::HeaderBar::new());
    let page = adw::PreferencesPage::new();
    view.set_content(Some(&page));
    let go = primary_button("Start");
    go.add_css_class("pill");
    go.set_halign(gtk::Align::Center);
    go.set_margin_top(12);
    go.set_margin_bottom(12);
    view.add_bottom_bar(&go);
    dialog.set_child(Some(&view));

    let camera_group = adw::PreferencesGroup::new();
    camera_group.set_title("Camera");
    camera_group.set_description(Some(
        "Connect it with its cable and switch it on. A Fujifilm wants USB Tether Shooting in its \
         connection settings, a Sony PC Remote. Each photograph taken comes straight in.",
    ));
    let camera = adw::ActionRow::new();
    let again = gtk::Button::with_label("Look Again");
    again.add_css_class("flat");
    again.set_valign(gtk::Align::Center);
    camera.add_suffix(&again);
    camera_group.add(&camera);
    page.add(&camera_group);
    let look = glib::clone!(
        #[weak] camera,
        #[weak] again,
        move || {
            camera.set_title("Looking for a camera…");
            again.set_visible(false);
            glib::spawn_future_local(async move {
                let found = gio::spawn_blocking(numa::tether::cameras).await.unwrap_or_default();
                match found.first() {
                    Some(name) => set_row_title(&camera, name),
                    None => camera.set_title("No camera found yet"),
                }
                again.set_visible(found.is_empty());
            });
        }
    );
    again.connect_clicked(glib::clone!(
        #[strong] look,
        move |_| look()
    ));
    look();

    let now = glib::DateTime::now_utc().map_or(0, |now| now.to_unix());
    let today = import::Shoot { first: now, last: now, photos: Vec::new() };
    let place = match import::suggest(&today, &import_spans(state), import_offset()) {

        Place::New(path) => match state.libraries.all.borrow().iter().find(|library| library.path == path) {
            Some(library) => Place::Library(library.clone()),
            None => Place::New(path),
        },
        place => place,
    };
    let place = Rc::new(RefCell::new(place));
    let into_group = adw::PreferencesGroup::new();
    into_group.set_title("Into");
    let into = adw::ActionRow::new();
    describe_place(&into, &place.borrow());
    let change = gtk::Button::with_label("Change…");
    change.add_css_class("flat");
    change.set_valign(gtk::Align::Center);
    change.connect_clicked(glib::clone!(
        #[strong] state,
        #[strong] place,
        #[weak] dialog,
        #[weak] into,
        move |_| {
            let chooser = gtk::FileDialog::new();
            chooser.set_title("Shoot into this folder");
            let (state, place) = (state.clone(), place.clone());
            glib::spawn_future_local(async move {
                let window = dialog.root().and_downcast::<gtk::Window>();
                let Some(path) = chooser.select_folder_future(window.as_ref()).await.ok().and_then(|file| file.path()) else { return };
                let library = state.libraries.all.borrow().iter().find(|library| library.path == path).cloned();
                *place.borrow_mut() = library.map_or(Place::New(path), Place::Library);
                describe_place(&into, &place.borrow());
            });
        }
    ));
    into.add_suffix(&change);
    into_group.add(&into);
    page.add(&into_group);

    let same_group = adw::PreferencesGroup::new();
    let same = adw::SwitchRow::new();
    same.set_title("Same Edit as the Last");
    same.set_subtitle("Each new photograph takes the edit of the one before it, crop included");
    same.set_active(state.catalog.setting(SAME_EDIT).as_deref() == Some("1"));
    same_group.add(&same);
    page.add(&same_group);

    go.connect_clicked(glib::clone!(
        #[strong] state,
        #[strong] place,
        #[weak] dialog,
        #[weak] same,
        move |_| {
            dialog.close();
            let _ = state.catalog.set_setting(SAME_EDIT, if same.is_active() { "1" } else { "0" });
            let place = place.borrow().clone();
            start_tethering(&state, place, same.is_active());
        }
    ));
    dialog.present(Some(window));
}

fn describe_place(row: &adw::ActionRow, place: &Place) {
    match place {
        Place::Library(library) => {
            set_row_title(row, &library.label());
            row.set_subtitle("This library");
        }
        Place::New(path) => {
            set_row_title(row, &path.file_name().unwrap_or_default().to_string_lossy());
            set_row_subtitle(row, &format!("A new library in {}", path.parent().unwrap_or(path).display()));
        }
    }
}

fn start_tethering(state: &App, place: Place, same_edit: bool) {
    let library = match place {
        Place::Library(library) => library,
        Place::New(path) => match std::fs::create_dir_all(&path).map_err(|err| err.to_string()).and_then(|_| state.catalog.add_library(&path)) {
            Ok(library) => library,
            Err(err) => {
                state.toast(&format!("Could not make {}: {err}", path.display()));
                return;
            }
        },
    };
    state.libraries.filter.borrow_mut().in_one_library();
    reload_libraries(state);
    select_library(state, library.id);
    if state.stack.visible_child_name().as_deref() != Some("editor") {
        state.stack.set_visible_child_name("library");

        let wall = state.grid.wall.clone();
        glib::idle_add_local_once(move || {
            wall.grab_focus();
        });
    }

    let generation = GENERATION.with(|counter| {
        counter.set(counter.get() + 1);
        counter.get()
    });
    let told = move |event: Event| glib::MainContext::default().invoke(move || heard(generation, event));
    let Some(session) = numa::tether::start(library.path.clone(), told) else {
        state.toast("Tethering needs libgphoto2, which is not installed");
        return;
    };
    release_camera();
    TETHER.with(|tether| {
        *tether.borrow_mut() = Some(Tether {
            state: state.clone(),
            generation,
            _session: session,
            library,
            camera: None,
            waiting: None,
            received: 0,
            last: None,
            same_edit,
            queue: Vec::new(),
            taking: false,
        })
    });
    say();
    BANNER.with(|banner| banner.set_revealed(true));
    enable_actions(state);
}

fn stop_tethering(state: &App) {

    let Some(stopped) = TETHER.with(|tether| tether.borrow_mut().take()) else { return };
    BANNER.with(|banner| banner.set_revealed(false));
    enable_actions(state);

    rescan_in_background(state, true);
    state.toast(&match stopped.received {
        0 => "Tethering stopped".to_string(),
        1 => "Tethering stopped · 1 photograph".to_string(),
        count => format!("Tethering stopped · {} photographs", places::grouped(count as i64)),
    });
}

fn release_camera() {
    for mount in gio::VolumeMonitor::get().mounts() {
        if mount.root().uri_scheme().as_deref() == Some("gphoto2") {
            mount.unmount_with_operation(gio::MountUnmountFlags::NONE, None::<&gio::MountOperation>, gio::Cancellable::NONE, |done| {
                if let Err(err) = done {
                    log::warn!("tethering: the camera's mount stayed: {err}");
                }
            });
        }
    }
}

fn heard(generation: u64, event: Event) {
    let Some(state) = TETHER.with(|tether| tether.borrow().as_ref().filter(|tether| tether.generation == generation).map(|tether| tether.state.clone())) else {
        return;
    };
    let with = |change: &mut dyn FnMut(&mut Tether)| TETHER.with(|tether| tether.borrow_mut().as_mut().map(|tether| change(tether)));
    match event {
        Event::Connected(name) => {
            let mut back = false;
            with(&mut |tether| {
                back = tether.waiting == Some(Waiting::Lost);
                tether.camera = Some(name.clone());
                tether.waiting = None;
            });
            if back {
                state.toast(&format!("{name} is back"));
            }
        }
        Event::Waiting(why) => {
            let (mut camera, mut before) = (None, None);
            with(&mut |tether| {
                before = tether.waiting.replace(why);
                camera = tether.camera.clone();
            });
            match why {
                Waiting::Held => release_camera(),

                Waiting::Lost if before == Some(Waiting::NotShooting) => {}

                Waiting::Lost => state.toast(&format!("{} is not answering — Numa waits for it", camera.as_deref().unwrap_or("The camera"))),
                Waiting::NotShooting => state.toast(&format!("{} is set to read its card — {}", camera.as_deref().unwrap_or("The camera"), switch_to(camera.as_deref()))),
                Waiting::NoCamera => {}
            }
        }
        Event::Arrived(path) => {
            let mut start = false;
            with(&mut |tether| {
                tether.received += 1;
                tether.queue.push(path.clone());
                start = !std::mem::replace(&mut tether.taking, true);
            });
            if start {
                take_in(&state);
            }
        }
        Event::Failed(name, why) => state.toast(&format!("{name} could not be fetched from the camera: {why}")),
    }
    say();
}

fn switch_to(camera: Option<&str>) -> &'static str {
    match camera.unwrap_or_default() {
        name if name.starts_with("Fujifilm") => "choose USB Tether Shooting in its connection settings",
        name if name.starts_with("Sony") => "choose PC Remote in its USB connection settings",
        _ => "set it to tethering in its connection settings",
    }
}

fn say() {
    let line = TETHER.with(|tether| {
        let tether = tether.borrow();
        let tether = tether.as_ref()?;
        let camera = tether.camera.as_deref().unwrap_or("The camera");
        Some(match (tether.waiting, &tether.camera) {
            (Some(Waiting::Held), _) => format!("{camera} is open in another program — Numa is asking for it"),
            (Some(Waiting::Lost), _) => format!("{camera} is not answering — switched off or unplugged"),
            (Some(Waiting::NotShooting), _) => format!("{camera} is set to read its card — {}", switch_to(tether.camera.as_deref())),
            (Some(Waiting::NoCamera), _) | (None, None) => "Waiting for a camera — connect it with its cable and switch it on".to_string(),
            (None, Some(name)) => match tether.received {
                0 => format!("Tethered to {name} · into {}", tether.library.label()),
                1 => format!("Tethered to {name} · 1 photograph into {}", tether.library.label()),
                count => format!("Tethered to {name} · {} photographs into {}", places::grouped(count as i64), tether.library.label()),
            },
        })
    });
    if let Some(line) = line {
        BANNER.with(|banner| banner.set_title(&glib::markup_escape_text(&line)));
    }
}

fn take_in(state: &App) {
    if state.libraries.scanning.get() {
        glib::timeout_add_local_once(std::time::Duration::from_millis(250), glib::clone!(
            #[strong] state,
            move || take_in(&state)
        ));
        return;
    }
    let Some((library, paths)) = TETHER.with(|tether| tether.borrow_mut().as_mut().map(|tether| (tether.library.clone(), std::mem::take(&mut tether.queue)))) else {
        return;
    };
    state.libraries.scanning.set(true);
    let known = state.catalog.known_files(&library).unwrap_or_default();
    let state = state.clone();
    glib::spawn_future_local(async move {
        let root = library.path.clone();
        let found = gio::spawn_blocking(move || numa::io::catalog::scan(&root, &known)).await;
        state.libraries.scanning.set(false);
        match found.map(|found| state.catalog.apply_scan(&library, &found)) {
            Ok(Ok(_)) => {
                show(&state, &library, &paths);
                read_arrivals(&state, &library, &paths);
            }
            Ok(Err(err)) => log::warn!("tethering: {}: {err}", library.path.display()),
            Err(_) => {}
        }
        let more = TETHER.with(|tether| {
            tether.borrow_mut().as_mut().is_some_and(|tether| {
                tether.taking = !tether.queue.is_empty();
                tether.taking
            })
        });
        if more {
            take_in(&state);
        }
    });
}

fn show(state: &App, library: &Library, paths: &[PathBuf]) {
    let Some((last, same_edit)) = TETHER.with(|tether| tether.borrow().as_ref().map(|tether| (tether.last, tether.same_edit))) else { return };
    let photos = state.catalog.photos(library.id, &Filter::default()).unwrap_or_default();
    let new: Vec<&Photo> = paths.iter().filter_map(|path| photos.iter().find(|photo| photo.path == *path)).collect();

    let Some(newest) = new.iter().rev().find(|photo| raw::is_raw(&photo.path)).or(new.last()).map(|photo| photo.id) else { return };

    let editing = state.stack.visible_child_name().as_deref() == Some("editor");

    let (open, merge) = match state.open.borrow().as_ref().map(|photo| &photo.source) {
        Some(Source::Photo { id, .. }) => (Some(*id), false),
        Some(Source::Bracket { .. }) => (None, true),
        None => (None, false),
    };

    if editing {
        save_open_edits(state);
    }
    if let Some(source) = last.filter(|_| same_edit).and_then(|last| state.catalog.load_edits(last).ok().flatten()) {
        let parts = EditParts { geometry: true, ..EditParts::default() };
        for photo in &new {
            let mut document = Document::new(photo.path.to_string_lossy().to_string());
            document.copy_from(&source, parts);
            if let Err(err) = state.catalog.save_edits(photo.id, &document) {
                log::warn!("tethering: {}: {err}", photo.path.display());
            }
        }
    }
    TETHER.with(|tether| {
        if let Some(tether) = tether.borrow_mut().as_mut() {
            tether.last = Some(newest);
        }
    });

    if state.libraries.current.borrow().as_ref().map(|open| open.id) != Some(library.id) {
        return;
    }

    let follow = |on: Option<i64>| last.is_none() || on.is_none() || on == last;
    let in_loupe = state.loupe.at.get().and_then(|at| id_at(state, at));
    let selected = selected_ids(state);
    let adjustment = state.grid.scroller.vadjustment();
    let was = adjustment.value();

    reload_grid(state);
    let select = |id: i64| {
        let at = state.grid.order.borrow().iter().position(|card| *card == id);
        if let Some(at) = at {
            state.grid.wall.select_only(at);
        }
        at
    };
    if editing {
        build_filmstrip(state);
        match (merge, open) {
            (false, open) if follow(open) => open_photo(state, newest),
            (_, Some(open)) => mark_filmstrip(state, open),
            _ => {}
        }
    } else if let Some(looking) = in_loupe {
        show_in_loupe(state, if follow(Some(looking)) { newest } else { looking });
    } else if last.is_none() {

        show_in_loupe(state, newest);
    } else if selected.len() <= 1 && follow(selected.first().copied()) {
        if let Some(at) = select(newest) {
            state.grid.wall.reveal(at);
        }
    } else {
        if let [one] = selected[..] {
            select(one);
        }
        glib::timeout_add_local_once(std::time::Duration::from_millis(80), move || adjustment.set_value(was));
    }
}

fn read_arrivals(state: &App, library: &Library, paths: &[PathBuf]) {
    let photos = state.catalog.photos(library.id, &Filter::default()).unwrap_or_default();
    let work: Vec<(i64, PathBuf)> = paths.iter().filter_map(|path| photos.iter().find(|photo| photo.path == *path)).map(|photo| (photo.id, photo.path.clone())).collect();
    if work.is_empty() {
        return;
    }
    let (state, library) = (state.clone(), library.clone());
    glib::spawn_future_local(async move {
        let measured = gio::spawn_blocking(move || {
            use rayon::prelude::*;
            work.par_iter().filter_map(|(id, path)| measure_photo(path).ok().map(|(frame, faces, face, embeddings)| (*id, frame, faces, face, embeddings))).collect::<Vec<_>>()
        })
        .await
        .unwrap_or_default();
        if let Err(err) = state.catalog.save_analysis_batch(cull::VERSION, &measured) {
            log::warn!("tethering: the analysis was not kept: {err}");
            return;
        }
        words::read_imported(&state, &[library.id]);

        let fresh = state.catalog.photos(library.id, &Filter::default()).unwrap_or_default();
        for (id, ..) in &measured {
            let Some(photo) = fresh.iter().find(|photo| photo.id == *id) else { continue };
            state.grid.cards.borrow_mut().insert(*id, photo.clone());
            let at = state.grid.order.borrow().iter().position(|card| card == id);
            if let Some(at) = at {
                state.grid.wall.rebind(at);
            }
        }
        refresh_loupe_bar(&state);
    });
}
