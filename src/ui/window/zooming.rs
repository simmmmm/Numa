use super::*;

pub(super) fn set_zoom(state: &App, zoom: f64) {
    let was_full = needs_full_resolution(state, state.zooming.level.get());

    state.render.tile.set(None);
    state.zooming.level.set(zoom);
    apply_zoom(state);

    let wants_full = needs_full_resolution(state, zoom);
    if wants_full {

        let generation = state.render.zoom_generation.get().wrapping_add(1);
        state.render.zoom_generation.set(generation);

        let state = state.clone();
        glib::timeout_add_local_once(std::time::Duration::from_millis(250), move || {
            let still_wanted = needs_full_resolution(&state, state.zooming.level.get());
            if state.render.zoom_generation.get() == generation && still_wanted {
                ensure_full_resolution(&state);
            }
        });
    } else if was_full {

        if let Some(photo) = state.open.borrow_mut().as_mut() {
            photo.full_working = None;
            photo.full_working_key = None;
            photo.view = None;
        }
        request_render(state);
        refresh_info(state);
    }
}

pub(super) fn ensure_full_resolution(state: &App) {
    let request = {
        let open = state.open.borrow();
        let Some(photo) = open.as_ref() else { return };

        let key = colour_key(&photo.document);
        if photo.full_working_key.as_ref() == Some(&key) {

            state.render.full_resolution_stale.set(false);
            return;
        }

        if state.render.failed_key.borrow().as_ref() == Some(&key) {
            return;
        }
        if state.render.loading_full.get() {

            state.render.full_resolution_stale.set(true);
            return;
        }

        (photo.source.clone(), photo.document.clone(), key)
    };

    let (source, document, key) = request;
    state.render.loading_full.set(true);
    state.render.full_resolution_stale.set(false);
    state.zooming.label.set_text("…");

    let generation = state.open_generation.get();
    let state = state.clone();
    glib::spawn_future_local(async move {
        let decoded = busy(&state, "Decoding the original…", move || {
            let full = source.full_resolution()?;
            let inputs = render_inputs(&document);

            Ok::<_, String>(render::to_working_space(&document, full, &inputs))
        })
        .await;

        state.render.loading_full.set(false);

        if state.open_generation.get() != generation {
            return;
        }

        if !needs_full_resolution(&state, state.zooming.level.get()) {
            state.render.full_resolution_stale.set(false);
            return;
        }

        match decoded {
            Ok(Ok(working)) => {
                if let Some(photo) = state.open.borrow_mut().as_mut() {
                    photo.full_working = Some(working);
                    photo.full_working_key = Some(key);
                }
                request_render(&state);
                refresh_info(&state);
            }
            Ok(Err(err)) => {
                *state.render.failed_key.borrow_mut() = Some(key);
                state.toast(&format!("Could not load full resolution: {err}"));
            }
            Err(_) => {}
        }

        apply_zoom(&state);

        if state.render.full_resolution_stale.get() {
            ensure_full_resolution(&state);
        }
    });
}

pub(super) fn fit_scale(state: &App) -> f64 {
    let Some((width, height)) = displayed_size(state) else {
        return 1.0;
    };
    let (available_x, available_y) = (
        state.zooming.scroller.width() as f64,
        state.zooming.scroller.height() as f64,
    );

    if width == 0 || height == 0 || available_x <= 0.0 || available_y <= 0.0 {
        return 1.0;
    }

    (available_x / width as f64).min(available_y / height as f64) * device_scale(state)
}

pub(super) fn device_scale(state: &App) -> f64 {
    state.canvas.scale_factor().max(1) as f64
}

pub(super) fn scaled_zoom(state: &App, factor: f64) -> f64 {
    let floor = fit_scale(state);
    let wanted = effective_zoom(state).max(1e-6) * factor;

    if wanted <= floor * 1.01 {
        FIT_ZOOM
    } else {
        wanted.min(MAX_ZOOM)
    }
}

pub(super) fn effective_zoom(state: &App) -> f64 {
    if state.zooming.level.get() == FIT_ZOOM {
        fit_scale(state)
    } else {
        state.zooming.level.get()
    }
}

pub(super) fn zoom_about_centre(state: &App, zoom: f64) {
    let horizontal = state.zooming.scroller.hadjustment();
    let vertical = state.zooming.scroller.vadjustment();

    let before = (effective_zoom(state) / device_scale(state)).max(1e-6);

    let focus = [&horizontal, &vertical].map(|adjustment| {
        (adjustment.value() + adjustment.page_size() / 2.0) / before
    });

    zoom_keeping(state, zoom, focus);
}

pub(super) fn apply_zoom(state: &App) {
    let zoom = state.zooming.level.get();

    if zoom == FIT_ZOOM {
        state.canvas.set_size_request(-1, -1);
        state.canvas.set_halign(gtk::Align::Fill);
        state.canvas.set_valign(gtk::Align::Fill);
        write_zoom_label(state);
        queue_reference(state);
        return;
    }

    let Some((full_width, full_height)) = displayed_size(state) else {
        return;
    };
    let scale = device_scale(state);
    let width = (full_width as f64 * zoom / scale).round() as i32;
    let height = (full_height as f64 * zoom / scale).round() as i32;

    state.canvas.set_size_request(width.max(1), height.max(1));
    state.canvas.set_halign(gtk::Align::Center);
    state.canvas.set_valign(gtk::Align::Center);

    write_zoom_label(state);

    queue_reference(state);
}

pub(super) fn write_zoom_label(state: &App) {

    let note = camera_note(state);
    let zoom = state.zooming.level.get();
    if zoom == FIT_ZOOM {
        let fit = format!("Fit {:.0}%", effective_zoom(state) * 100.0);
        state.zooming.label.set_text(&(fit + &note));
        return;
    }
    let full_ready = state.render.rendered_from_full.get();
    let said = match (needs_full_resolution(state, zoom), full_ready) {
        (true, false) if state.render.loading_full.get() => "…".to_string(),
        (true, false) => format!("{:.0}% soft", zoom * 100.0),
        _ => format!("{:.0}%", zoom * 100.0),
    };
    state.zooming.label.set_text(&(said + &note));
}

#[derive(Clone)]
pub(super) struct State {
    pub(super) level: Rc<Cell<f64>>,
    pub(super) label: gtk::Label,

    pub(super) scroller: gtk::ScrolledWindow,
}

impl State {
    pub(super) fn new() -> Self {
        Self {
            level: Rc::new(Cell::new(FIT_ZOOM)),
            label: gtk::Label::new(Some("Fit")),
            scroller: gtk::ScrolledWindow::new(),
        }
    }
}
