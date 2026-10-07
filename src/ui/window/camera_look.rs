use super::*;

thread_local! {

    static CONTROLS: std::cell::OnceCell<(gtk::ToggleButton, gtk::Label, gtk::Box, gtk::Scale)> = const { std::cell::OnceCell::new() };

    static SAID: RefCell<HashMap<String, Result<String, String>>> = RefCell::new(HashMap::new());

    static FITTING: Cell<bool> = const { Cell::new(false) };
}

pub(super) fn build(state: &App) -> gtk::Box {
    let section = gtk::Box::new(gtk::Orientation::Vertical, 0);
    section.append(&section_header("From the Camera"));

    let tile = gtk::ToggleButton::new();
    tile.add_css_class("camera-look-tile");
    tile.set_margin_top(8);
    let words = gtk::Box::new(gtk::Orientation::Vertical, 2);
    let title = gtk::Label::new(Some("As Shot"));
    title.set_xalign(0.0);
    title.add_css_class("title");
    let line = gtk::Label::new(None);
    line.set_xalign(0.0);
    line.set_wrap(true);
    line.add_css_class("recipe");
    words.append(&title);
    words.append(&line);
    tile.set_child(Some(&words));
    tile.set_tooltip_text(Some("Follow this photo's own camera JPEG: its film simulation and recipe"));
    tile.connect_toggled(glib::clone!(
        #[strong] state,
        move |tile| {
            if !state.applying.get() {
                choose(&state, tile.is_active());
            }
        }
    ));
    section.append(&tile);

    let strength = strength_scale();
    let row = slider_row(state, "Strength", &strength, Readout::Positive(0));
    row.set_margin_top(4);
    strength.connect_value_changed(glib::clone!(
        #[strong] state,
        move |strength| {
            if state.applying.get() {
                return;
            }
            let changed = {
                let mut open = state.open.borrow_mut();
                let Some(look) = open.as_mut().and_then(|photo| photo.document.camera_look.as_mut()) else { return };
                let changed = look.strength != strength.value() as f32;
                look.strength = strength.value() as f32;
                changed
            };
            if changed {
                request_render(&state);
                schedule_history_push(&state);
            }
        }
    ));
    section.append(&row);

    CONTROLS.with(|controls| {
        let _ = controls.set((tile, line, row, strength));
    });
    write(state);
    section
}

fn open_path(state: &App) -> Option<String> {
    state.open.borrow().as_ref().and_then(|photo| match &photo.source {
        Source::Photo { path, .. } => Some(path.to_string_lossy().to_string()),
        Source::Bracket { .. } => None,
    })
}

fn said(path: &str) -> Result<String, String> {
    SAID.with_borrow_mut(|said| {
        said.entry(path.to_string()).or_insert_with(|| numa::io::camera_look::describe(Path::new(path))).clone()
    })
}

pub(super) fn write(state: &App) {
    let Some((tile, line, row, strength)) = CONTROLS.with(|controls| controls.get().cloned()) else { return };
    let look = state.open.borrow().as_ref().and_then(|photo| photo.document.camera_look.clone());
    let told = match open_path(state) {
        Some(path) => said(&path),
        None => Err("Not for a merged photo".to_string()),
    };
    let was = state.applying.replace(true);
    tile.set_active(look.is_some());
    strength.set_value(look.as_ref().map_or(100.0, |look| look.strength as f64));
    state.applying.set(was);

    tile.set_sensitive(told.is_ok() || look.is_some());
    row.set_visible(look.is_some());
    line.set_text(&match (&told, FITTING.get() && look.as_ref().is_some_and(|look| look.fit.is_none())) {
        (Ok(_), true) => "Reading the camera's JPEG…".to_string(),
        (Ok(text), false) | (Err(text), _) => text.clone(),
    });
    if told.is_ok() && look.is_some_and(|look| look.fit.is_none()) {
        fit_open(state);
    }
}

fn choose(state: &App, on: bool) {
    {
        let mut open = state.open.borrow_mut();
        let Some(photo) = open.as_mut() else { return };
        match on {
            true if photo.document.camera_look.is_none() => {
                photo.document.camera_look = Some(numa::core::camera_look::CameraLook { strength: 100.0, fit: None });
            }
            true => return,
            false => photo.document.camera_look = None,
        }
    }
    write(state);
    request_render(state);
    schedule_history_push(state);
}

fn fit_open(state: &App) {
    let Some(document) = state.open.borrow().as_ref().map(|photo| photo.document.clone()) else { return };
    if FITTING.replace(true) {
        return;
    }
    let path = document.source.path.clone();
    let state = state.clone();
    glib::spawn_future_local(async move {
        let found = gio::spawn_blocking(move || {
            let mut document = document;
            numa::io::camera_look::fill(&mut document).map(|()| document.camera_look.and_then(|look| look.fit))
        })
        .await
        .unwrap_or_else(|_| Err("The camera's JPEG could not be read".into()));
        FITTING.set(false);
        match found {
            Ok(Some(fit)) => {
                {
                    let mut open = state.open.borrow_mut();
                    let Some(photo) = open.as_mut().filter(|photo| photo.document.source.path == path) else { return };
                    let waiting = std::iter::once(&mut photo.document.camera_look)
                        .chain(photo.history.states.iter_mut().map(|step| &mut step.camera_look))
                        .chain(photo.before_preset.iter_mut().flat_map(|on| [&mut on.left.camera_look, &mut on.base.camera_look]));
                    for look in waiting.flatten().filter(|look| look.fit.is_none()) {
                        look.fit = Some(fit.clone());
                    }
                }
                request_render(&state);
            }
            Ok(None) => {}
            Err(why) => {
                state.toast(&why);
                SAID.with_borrow_mut(|said| said.insert(path, Err(why)));
            }
        }
        write(&state);
    });
}
