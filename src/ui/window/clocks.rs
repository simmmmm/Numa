use super::*;
use numa::io::catalog::{Camera, Clock};
use numa::io::clocks::{self, Offer};

pub(super) fn install_clock_action(state: &App, window: &adw::ApplicationWindow) {
    let action = gio::SimpleAction::new("clocks", None);
    action.connect_activate(glib::clone!(
        #[strong] state,
        #[weak] window,
        move |_, _| clocks_dialog(&state, &window)
    ));
    window.add_action(&action);
}

fn names(cameras: &[Camera]) -> HashMap<String, String> {
    let keys: Vec<String> = cameras.iter().map(|camera| camera.key.clone()).collect();
    keys.iter().map(|key| (key.clone(), clocks::camera_name(key, &keys))).collect()
}

fn change_clock(state: &App, toasts: &adw::ToastOverlay, change: (i64, &str, Option<Clock>), said: &str, after: Rc<dyn Fn()>) {
    let (library_id, camera, clock) = change;
    let before = state
        .catalog
        .cameras(library_id)
        .ok()
        .and_then(|cameras| cameras.into_iter().find(|known| known.key == camera))
        .and_then(|known| known.clock);
    if let Err(err) = state.catalog.set_clock(library_id, camera, clock) {
        state.toast(&format!("Could not set the clock: {err}"));
        return;
    }
    clocks_moved(state, library_id);
    after();
    let toast = adw::Toast::new(&glib::markup_escape_text(said));
    toast.set_button_label(Some("Undo"));
    let camera = camera.to_string();
    toast.connect_button_clicked(glib::clone!(
        #[strong] state,
        move |_| match state.catalog.set_clock(library_id, &camera, before) {
            Ok(()) => {
                clocks_moved(&state, library_id);
                after();
            }
            Err(err) => state.toast(&format!("Could not set the clock back: {err}")),
        }
    ));
    toasts.add_toast(toast);
}

fn clocks_moved(state: &App, library_id: i64) {
    if let Err(err) = regroup_bursts(&state.catalog, library_id) {
        log::warn!("could not group the bursts again after a clock moved: {err}");
    }
    reload_grid(state);
    offer_clocks(state);
}

pub(super) fn offer_clocks(state: &App) {
    let revealer = &state.grid.clock_offer;
    let library = state.libraries.current.borrow().clone();
    let one = library
        .filter(|library| !state.libraries.filter.borrow().spans_libraries() && !state.catalog.is_offline(library.id))
        .and_then(|library| {
            let offers = clocks::offers(&state.catalog, library.id)
                .map_err(|err| log::warn!("could not look for camera clocks: {err}"))
                .ok()?;
            Some((library, offers.into_iter().next()?))
        });
    let Some((library, offer)) = one else {
        revealer.set_reveal_child(false);
        return;
    };
    revealer.set_child(Some(&offer_banner(state, &library, &offer)));
    revealer.set_reveal_child(true);
}

fn offer_banner(state: &App, library: &Library, offer: &Offer) -> gtk::Box {
    let names = names(&state.catalog.cameras(library.id).unwrap_or_default());
    let name = |key: &str| names.get(key).cloned().unwrap_or_else(|| key.to_string());
    let banner = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    banner.add_css_class("clock-offer");
    let (theirs, ours) = offer.pair;
    for id in if theirs == ours { vec![ours] } else { vec![theirs, ours] } {
        banner.append(&offer_frame(state, id));
    }

    let (title, body) = match &offer.reference {
        Some(reference) => (
            "Two cameras saw the same moment",
            format!("The {}'s clock {} of the {}.", name(&offer.camera), clocks::runs(offer.seconds), name(reference)),
        ),
        None => ("A camera photographed Numa's clock", format!("The {}'s clock {}.", name(&offer.camera), clocks::runs(offer.seconds))),
    };
    let words = gtk::Box::new(gtk::Orientation::Vertical, 2);
    words.set_hexpand(true);
    words.set_valign(gtk::Align::Center);
    let heading = gtk::Label::new(Some(title));
    heading.add_css_class("heading");
    heading.set_xalign(0.0);
    let line = gtk::Label::new(Some(&body));
    line.add_css_class("dim-label");
    line.set_xalign(0.0);
    line.set_wrap(true);
    words.append(&heading);
    words.append(&line);
    banner.append(&words);

    let line_up = gtk::Button::with_label("Line Up");
    let not_now = gtk::Button::with_label("Not Now");
    not_now.add_css_class("flat");
    let (library_id, camera, seconds) = (library.id, offer.camera.clone(), offer.seconds);
    let said = format!("Lined up the {}'s clock", name(&offer.camera));
    line_up.connect_clicked(glib::clone!(
        #[strong] state,
        #[strong] camera,
        move |_| {
            let change = (library_id, camera.as_str(), Some(Clock { seconds, applied: true }));
            change_clock(&state, &state.toasts, change, &said, Rc::new(|| ()));
        }
    ));
    not_now.connect_clicked(glib::clone!(
        #[strong] state,
        move |_| {
            if let Err(err) = state.catalog.decline_clock(library_id, &camera, seconds) {
                log::warn!("could not remember Not Now: {err}");
            }
            offer_clocks(&state);
        }
    ));
    for button in [&line_up, &not_now] {
        button.set_valign(gtk::Align::Center);
        banner.append(button);
    }
    banner
}

fn offer_frame(state: &App, id: i64) -> adw::Clamp {
    let picture = gtk::Picture::new();
    picture.set_size_request(96, 64);
    picture.set_content_fit(gtk::ContentFit::Cover);
    picture.set_overflow(gtk::Overflow::Hidden);
    picture.add_css_class("clock-offer-frame");
    let (tall, wide) = (adw::Clamp::new(), adw::Clamp::new());
    tall.set_orientation(gtk::Orientation::Vertical);
    for (clamp, most) in [(&tall, 64), (&wide, 96)] {
        clamp.set_maximum_size(most);
        clamp.set_tightening_threshold(most);
    }
    tall.set_child(Some(&picture));
    wide.set_child(Some(&tall));
    wide.set_valign(gtk::Align::Center);
    if let Some((path, mtime)) = state.catalog.file_of(id) {
        let (wanted, shown) = (picture.downgrade(), picture.downgrade());
        thumbnail::load_thumbnail_first(
            &path,
            mtime,
            numa::io::thumbs::LIBRARY_EDGE,
            move || wanted.upgrade().is_some(),
            move |texture| {
                if let Some(picture) = shown.upgrade() {
                    picture.set_paintable(Some(&texture));
                }
            },
        );
    }
    wide
}

fn clocks_dialog(state: &App, window: &adw::ApplicationWindow) {
    let Some(library) = state.libraries.current.borrow().clone() else {
        state.toast("No library selected");
        return;
    };
    if state.catalog.is_offline(library.id) {
        state.toast(&format!("{} is not connected — its clocks wait until it is back", library.label()));
        return;
    }
    let dialog = adw::Dialog::new();
    dialog.set_title("Camera Clocks");
    dialog.set_content_width(480);
    let holder = adw::Bin::new();
    let toasts = adw::ToastOverlay::new();
    toasts.set_child(Some(&holder));
    let bar = adw::ToolbarView::new();
    bar.add_top_bar(&adw::HeaderBar::new());
    bar.set_content(Some(&toasts));
    dialog.set_child(Some(&bar));
    fill_clocks(state, &library, &holder, &toasts);
    dialog.present(Some(window));
}

fn fill_clocks(state: &App, library: &Library, holder: &adw::Bin, toasts: &adw::ToastOverlay) {
    let cameras = state.catalog.cameras(library.id).unwrap_or_default();
    if cameras.is_empty() {
        let empty = adw::StatusPage::new();
        empty.set_icon_name(Some("camera-photo-symbolic"));
        empty.set_title("No Cameras Yet");
        empty.set_description(Some("Numa reads which camera took each photograph when it scans the library."));
        holder.set_child(Some(&empty));
        return;
    }
    let names = names(&cameras);

    let refill: Rc<dyn Fn()> = Rc::new(glib::clone!(
        #[strong] state,
        #[strong] library,
        #[weak] holder,
        #[weak] toasts,
        move || {
            let (state, library) = (state.clone(), library.clone());
            glib::idle_add_local_once(move || fill_clocks(&state, &library, &holder, &toasts));
        }
    ));

    let group = adw::PreferencesGroup::new();
    group.set_title("Cameras");

    let reference = cameras.iter().find(|camera| camera.clock.is_none_or(|clock| !clock.applied)).map(|camera| camera.key.clone());

    let mut found: HashMap<String, i64> = HashMap::new();
    for offer in clocks::offers(&state.catalog, library.id).unwrap_or_default() {
        found.entry(offer.camera).or_insert(offer.seconds);
    }
    for camera in &cameras {
        let row = adw::ActionRow::new();
        row.set_title(&names[&camera.key]);
        let photos = format!("{} {}", places::grouped(camera.photos), if camera.photos == 1 { "photo" } else { "photos" });
        let clock = match camera.clock {
            Some(clock) => clocks::off(clock.seconds),
            None if found.contains_key(&camera.key) => format!("{}, not lined up yet", clocks::off(found[&camera.key])),
            None if cameras.len() > 1 && reference.as_ref() == Some(&camera.key) => "its clock is the reference".to_string(),
            None => "on time".to_string(),
        };
        row.set_subtitle(&format!("{photos} · {clock}"));
        if let Some(clock) = camera.clock {
            let switch = gtk::Switch::new();
            switch.set_valign(gtk::Align::Center);
            switch.set_active(clock.applied);
            switch.set_tooltip_text(Some("Apply the Offset"));
            let (key, name) = (camera.key.clone(), names[&camera.key].clone());
            switch.connect_active_notify(glib::clone!(
                #[strong] state,
                #[strong] refill,
                #[strong] library,
                #[weak] toasts,
                move |switch| {
                    let applied = switch.is_active();
                    let said = match applied {
                        true => format!("Lined up the {name}'s clock"),
                        false => format!("The {name} is back on its own clock"),
                    };
                    change_clock(&state, &toasts, (library.id, &key, Some(Clock { applied, ..clock })), &said, refill.clone());
                }
            ));
            row.add_suffix(&switch);
            row.set_activatable_widget(Some(&switch));
        }
        group.add(&row);
    }

    let more = adw::PreferencesGroup::new();
    more.set_description(Some(
        "Numa finds a clock that is off from one moment two cameras both saw. Or photograph Numa's clock with each camera before the day starts.",
    ));
    let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let photo = gtk::Button::with_label("Clock Photo…");
    let by_hand = gtk::Button::with_label("Set by Hand…");
    by_hand.add_css_class("flat");
    photo.connect_clicked(|button| clock_photo(button));
    by_hand.connect_clicked(glib::clone!(
        #[strong] state,
        #[strong] library,
        #[strong] refill,
        #[weak] toasts,
        move |button| set_by_hand(&state, (&library, &cameras, &names), button, &toasts, refill.clone())
    ));
    buttons.append(&photo);
    buttons.append(&by_hand);
    more.add(&buttons);

    let page = adw::PreferencesPage::new();
    page.add(&group);
    page.add(&more);
    holder.set_child(Some(&page));
}

fn set_by_hand(
    state: &App,
    (library, cameras, names): (&Library, &[Camera], &HashMap<String, String>),
    parent: &gtk::Button,
    toasts: &adw::ToastOverlay,
    refill: Rc<dyn Fn()>,
) {
    let alert = adw::AlertDialog::new(
        Some("Set a Clock by Hand"),
        Some("How far the camera's clock is off from the right time — or from the camera the others follow."),
    );
    let titles: Vec<&str> = cameras.iter().map(|camera| names[&camera.key].as_str()).collect();
    let camera_row = adw::ComboRow::new();
    camera_row.set_title("Camera");
    camera_row.set_model(Some(&gtk::StringList::new(&titles)));
    let direction = adw::ComboRow::new();
    direction.set_title("Its Clock Runs");
    direction.set_model(Some(&gtk::StringList::new(&["Ahead", "Behind"])));
    let minutes = adw::SpinRow::with_range(0.0, 1440.0, 1.0);
    minutes.set_title("Minutes");
    let seconds = adw::SpinRow::with_range(0.0, 59.0, 1.0);
    seconds.set_title("Seconds");

    let clocks: Vec<Option<Clock>> = cameras.iter().map(|camera| camera.clock).collect();
    let show = glib::clone!(
        #[weak] direction,
        #[weak] minutes,
        #[weak] seconds,
        move |index: u32| {
            let off = clocks.get(index as usize).copied().flatten().map_or(0, |clock| clock.seconds);
            direction.set_selected(u32::from(off >= 0));
            minutes.set_value((off.abs() / 60) as f64);
            seconds.set_value((off.abs() % 60) as f64);
        }
    );

    let first = cameras.iter().position(|camera| camera.clock.is_some()).unwrap_or(usize::from(cameras.len() > 1));
    camera_row.set_selected(first as u32);
    show(first as u32);
    camera_row.connect_selected_notify(move |row| show(row.selected()));

    let list = gtk::ListBox::new();
    list.add_css_class("boxed-list");
    list.set_selection_mode(gtk::SelectionMode::None);
    for row in [camera_row.upcast_ref::<gtk::Widget>(), direction.upcast_ref(), minutes.upcast_ref(), seconds.upcast_ref()] {
        list.append(row);
    }
    alert.set_extra_child(Some(&list));
    alert.add_responses(&[("cancel", "Cancel"), ("set", "Set")]);
    alert.set_response_appearance("set", adw::ResponseAppearance::Suggested);
    let keys: Vec<(String, String)> = cameras.iter().map(|camera| (camera.key.clone(), names[&camera.key].clone())).collect();
    let library_id = library.id;
    alert.connect_response(
        Some("set"),
        glib::clone!(
            #[strong] state,
            #[weak] toasts,
            move |_, _| {
                let Some((key, name)) = keys.get(camera_row.selected() as usize) else { return };
                let off = minutes.value() as i64 * 60 + seconds.value() as i64;
                let off = if direction.selected() == 0 { -off } else { off };
                let clock = (off != 0).then_some(Clock { seconds: off, applied: true });
                change_clock(&state, &toasts, (library_id, key, clock), &format!("Set the {name}'s clock"), refill.clone());
            }
        ),
    );
    alert.present(Some(parent));
}

fn clock_photo(parent: &gtk::Button) {
    let dialog = adw::Dialog::new();
    dialog.set_title("Numa's Clock");
    dialog.set_content_width(520);
    let column = gtk::Box::new(gtk::Orientation::Vertical, 16);
    column.set_margin_top(8);
    column.set_margin_bottom(24);
    column.set_margin_start(24);
    column.set_margin_end(24);
    let time = gtk::Label::new(None);
    time.add_css_class("title-1");
    time.add_css_class("numeric");
    let code = gtk::Picture::new();
    code.set_size_request(400, 400);
    code.set_content_fit(gtk::ContentFit::Contain);
    let hint = gtk::Label::new(Some(
        "Photograph this screen with each camera, the code filling most of the frame. When Numa analyses those photographs it reads the time in the code and offers to line each camera up.",
    ));
    hint.set_wrap(true);
    hint.set_justify(gtk::Justification::Center);
    hint.add_css_class("dim-label");
    column.append(&time);
    column.append(&code);
    column.append(&hint);
    let bar = adw::ToolbarView::new();
    bar.add_top_bar(&adw::HeaderBar::new());
    bar.set_content(Some(&column));
    dialog.set_child(Some(&bar));

    let tick = {
        let (time, code) = (time.downgrade(), code.downgrade());
        move || {
            let (Some(time), Some(code)) = (time.upgrade(), code.upgrade()) else { return glib::ControlFlow::Break };
            let now = local_millis();
            let second = now.div_euclid(1000).rem_euclid(86_400);
            time.set_text(&format!("{:02}:{:02}:{:02}", second / 3600, second / 60 % 60, second % 60));
            code.set_paintable(code_texture(&clocks::payload(now)).as_ref());
            glib::ControlFlow::Continue
        }
    };
    tick();
    glib::timeout_add_local(std::time::Duration::from_millis(100), tick);
    dialog.present(Some(parent));
}

fn local_millis() -> i64 {
    let Ok(now) = glib::DateTime::now_local() else { return 0 };
    (now.to_unix() + now.utc_offset().as_seconds()) * 1000 + i64::from(now.microsecond()) / 1000
}

fn code_texture(text: &str) -> Option<gtk::gdk::Texture> {
    const SCALE: usize = 16;
    const QUIET: usize = 4;
    let (width, modules) = clocks::code(text)?;
    let side = (width + 2 * QUIET) * SCALE;
    let mut grey = vec![u8::MAX; side * side];
    for (index, value) in grey.iter_mut().enumerate() {
        let (x, y) = ((index % side / SCALE).wrapping_sub(QUIET), (index / side / SCALE).wrapping_sub(QUIET));
        if x < width && y < width && modules[y * width + x] {
            *value = 0;
        }
    }
    let bytes = glib::Bytes::from_owned(grey);
    Some(gtk::gdk::MemoryTexture::new(side as i32, side as i32, gtk::gdk::MemoryFormat::G8, &bytes, side).upcast())
}
