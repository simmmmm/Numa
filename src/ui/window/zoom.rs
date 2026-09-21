use super::*;

pub(super) fn install_canvas_clicks(state: &App) {

    let pipette_click = gtk::GestureClick::new();
    pipette_click.connect_released(glib::clone!(
        #[strong] state,
        move |_, _, x, y| {
            let Some((u, v)) = canvas_point(&state, x, y) else { return };
            if state.colour.picking_white.get() {
                pick_white(&state, u, v);
            } else if state.colour.picking_band.get() {
                pick_band(&state, u, v);
            } else if state.colour.picking_point.get() {
                pick_point(&state, u, v);
            }
        }
    ));
    state.canvas.add_controller(pipette_click);

    let to_one_to_one = gtk::GestureClick::new();
    to_one_to_one.set_button(gtk::gdk::BUTTON_PRIMARY);
    to_one_to_one.connect_pressed(glib::clone!(
        #[strong] state,
        move |_, presses, x, y| {
            if presses != 2 || armed_pipette(&state) {
                return;
            }
            toggle_one_to_one(&state, x, y);
        }
    ));
    state.canvas.add_controller(to_one_to_one);
}

pub(super) fn zoom_keeping(state: &App, zoom: f64, focus: [f64; 2]) {
    set_zoom(state, zoom);

    if state.render.recentring.replace(true) {
        return;
    }

    let state = state.clone();
    glib::idle_add_local_once(move || {
        state.render.recentring.set(false);
        let after = effective_zoom(&state) / device_scale(&state);
        for (adjustment, centre) in [
            (state.zooming.scroller.hadjustment(), focus[0]),
            (state.zooming.scroller.vadjustment(), focus[1]),
        ] {
            adjustment.set_value(centre * after - adjustment.page_size() / 2.0);
        }
    });
}

thread_local! {

    static BEFORE_ONE_TO_ONE: Cell<f64> = const { Cell::new(FIT_ZOOM) };
}

fn armed_pipette(state: &App) -> bool {
    state.colour.picking_band.get()
        || state.colour.picking_white.get()
        || state.colour.picking_point.get()
}

pub(super) fn toggle_one_to_one(state: &App, x: f64, y: f64) {
    if state.open.borrow().is_none() {
        return;
    }
    let zoom = state.zooming.level.get();
    if zoom != FIT_ZOOM && (zoom - 1.0).abs() < 0.01 {
        let back = BEFORE_ONE_TO_ONE.with(|was| was.get());
        match back {
            FIT_ZOOM => set_zoom(state, FIT_ZOOM),
            back => zoom_about_centre(state, back),
        }
        return;
    }

    BEFORE_ONE_TO_ONE.with(|was| was.set(zoom));

    let Some((u, v)) = canvas_point(state, x, y) else { return };
    let (_, _, width, height) =
        content_rect(state, state.canvas.width() as f64, state.canvas.height() as f64);
    let per_pixel = (effective_zoom(state) / device_scale(state)).max(1e-6);
    let focus = [u as f64 * width / per_pixel, v as f64 * height / per_pixel];
    zoom_keeping(state, 1.0, focus);
}
