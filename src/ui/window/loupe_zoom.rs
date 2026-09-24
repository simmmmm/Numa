use super::*;

const MOST: f64 = 4.0;

const DRAG_SLOP: f64 = 4.0;

#[derive(Clone)]
pub(super) struct State {
    pub(super) scroller: gtk::ScrolledWindow,

    level: Rc<Cell<f64>>,

    before: Rc<Cell<f64>>,

    long_edge: Rc<Cell<Option<u32>>>,

    full: Rc<RefCell<HashMap<i64, gtk::gdk::Texture>>>,

    busy: Rc<Cell<Option<i64>>>,

    pending: Rc<Cell<Option<((f64, f64), (f64, f64))>>>,
}

impl State {
    pub(super) fn new() -> Self {
        Self {
            scroller: gtk::ScrolledWindow::new(),
            level: Rc::new(Cell::new(0.0)),
            before: Rc::new(Cell::new(0.0)),
            long_edge: Rc::new(Cell::new(None)),
            full: Rc::new(RefCell::new(HashMap::new())),
            busy: Rc::new(Cell::new(None)),
            pending: Rc::new(Cell::new(None)),
        }
    }
}

pub(super) fn build(state: &App) -> gtk::ScrolledWindow {
    let scroller = state.loupe.zoom.scroller.clone();
    scroller.set_policy(gtk::PolicyType::External, gtk::PolicyType::External);
    scroller.set_child(Some(&state.loupe.picture));

    let double = gtk::GestureClick::new();
    double.set_button(gtk::gdk::BUTTON_PRIMARY);
    double.connect_pressed(glib::clone!(
        #[strong] state,
        move |_, presses, x, y| {
            if presses == 2 {
                one_to_one(&state, x, y);
            }
        }
    ));
    scroller.add_controller(double);

    let wheel = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::VERTICAL);
    wheel.connect_scroll(glib::clone!(
        #[strong] state,
        move |_, _, dy| {
            let now = match zoomed(&state) {
                true => state.loupe.zoom.level.get(),
                false => fit_level(&state),
            };
            let (x, y) = middle(&state);
            set_level(&state, now * ZOOM_PER_NOTCH.powf(-dy), x, y);
            glib::Propagation::Stop
        }
    ));
    scroller.add_controller(wheel);

    for adjustment in [scroller.hadjustment(), scroller.vadjustment()] {

        adjustment.connect_value_changed(glib::clone!(
            #[strong] state,
            move |_| state.loupe.stage.queue_allocate()
        ));
        adjustment.connect_changed(glib::clone!(
            #[strong] state,
            move |_| {
                let state = state.clone();
                glib::idle_add_local_once(move || settle(&state));
            }
        ));
    }

    let drag = gtk::GestureDrag::new();
    let anchor: Rc<Cell<Option<(f64, f64)>>> = Rc::new(Cell::new(None));
    drag.connect_drag_begin(glib::clone!(
        #[strong] anchor,
        move |_, _, _| anchor.set(None)
    ));
    drag.connect_drag_update(glib::clone!(
        #[strong] state,
        #[strong] anchor,
        move |_, dx, dy| {
            let scroller = &state.loupe.zoom.scroller;
            let (x, y) = match anchor.get() {
                Some(anchor) => anchor,
                None if dx.hypot(dy) < DRAG_SLOP => return,
                None => {

                    state.loupe.zoom.pending.set(None);
                    let from = (scroller.hadjustment().value() + dx, scroller.vadjustment().value() + dy);
                    anchor.set(Some(from));
                    from
                }
            };
            scroller.hadjustment().set_value(x - dx);
            scroller.vadjustment().set_value(y - dy);
        }
    ));
    scroller.add_controller(drag);
    scroller
}

pub(super) fn zoomed(state: &App) -> bool {
    state.loupe.zoom.level.get() > 0.0
}

pub(super) fn full_for(state: &App, at: usize) -> Option<gtk::gdk::Texture> {
    let id = zoomed(state).then(|| id_at(state, at)).flatten()?;
    state.loupe.zoom.full.borrow().get(&id).cloned()
}

pub(super) fn sharpening(state: &App) -> bool {
    zoomed(state) && state.loupe.at.get().is_some_and(|at| full_for(state, at).is_none())
}

fn one_to_one(state: &App, x: f64, y: f64) {
    let now = state.loupe.zoom.level.get();
    if zoomed(state) && (now - 1.0).abs() < 0.01 {
        let (x, y) = middle(state);
        set_level(state, state.loupe.zoom.before.get(), x, y);
        return;
    }
    state.loupe.zoom.before.set(now);

    if let Some(point) = af_under(state, x, y) {
        let (mx, my) = middle(state);
        set_level_to(state, 1.0, Some((point.x as f64, point.y as f64)), mx, my);
        return;
    }
    set_level(state, 1.0, x, y);
}

fn middle(state: &App) -> (f64, f64) {
    let scroller = &state.loupe.zoom.scroller;
    (scroller.width() as f64 / 2.0, scroller.height() as f64 / 2.0)
}

fn fit_level(state: &App) -> f64 {
    let Some((width, height)) = size(state) else { return 1.0 };
    let scale = state.loupe.zoom.scroller.scale_factor() as f64;
    let scroller = &state.loupe.zoom.scroller;
    (scroller.width() as f64 * scale / width).min(scroller.height() as f64 * scale / height)
}

fn size(state: &App) -> Option<(f64, f64)> {
    let paintable = state.loupe.picture.paintable()?;
    let (width, height) = (paintable.intrinsic_width() as f64, paintable.intrinsic_height() as f64);
    if width <= 0.0 || height <= 0.0 {
        return None;
    }
    let at = state.loupe.at.get()?;
    if full_for(state, at).is_some() {
        return Some((width, height));
    }
    let long = state.loupe.zoom.long_edge.get().or_else(|| {
        let path = state.grid.lazy.borrow().get(at)?.path.clone();
        let (w, h) = raw::summary(&path)?.sensor;
        Some(w.max(h))
    });
    let factor = long.map_or(1.0, |long| long as f64 / width.max(height));
    Some((width * factor, height * factor))
}

fn drawn(state: &App) -> Option<(f64, f64, f64, f64)> {
    if zoomed(state) {
        let (width, height) = size(state)?;
        let per = state.loupe.zoom.level.get() / state.loupe.picture.scale_factor() as f64;
        let (w, h) = ((width * per).round(), (height * per).round());
        let scroller = &state.loupe.zoom.scroller;
        let (page_w, page_h) = (scroller.width() as f64, scroller.height() as f64);
        return Some((((page_w - w) / 2.0).max(0.0), ((page_h - h) / 2.0).max(0.0), w, h));
    }
    let picture = &state.loupe.picture;
    let aspect = picture.paintable()?.intrinsic_aspect_ratio();
    let (width, height) = (picture.width() as f64, picture.height() as f64);
    if aspect <= 0.0 || width <= 0.0 || height <= 0.0 {
        return None;
    }
    let (w, h) = match width / height > aspect {
        true => (height * aspect, height),
        false => (width, width / aspect),
    };
    Some(((width - w) / 2.0, (height - h) / 2.0, w, h))
}

pub(super) fn on_stage(state: &App, u: f64, v: f64) -> Option<(f64, f64, f64, f64)> {
    let (left, top, width, height) = drawn(state)?;
    let scroller = &state.loupe.zoom.scroller;
    let x = left + u * width - scroller.hadjustment().value();
    let y = top + v * height - scroller.vadjustment().value();
    Some((x, y, width, height))
}

fn fraction_at(state: &App, x: f64, y: f64) -> Option<(f64, f64)> {
    let (left, top, width, height) = drawn(state)?;
    let scroller = &state.loupe.zoom.scroller;
    let u = (scroller.hadjustment().value() + x - left) / width;
    let v = (scroller.vadjustment().value() + y - top) / height;
    Some((u.clamp(0.0, 1.0), v.clamp(0.0, 1.0)))
}

fn set_level(state: &App, level: f64, x: f64, y: f64) {
    set_level_to(state, level, fraction_at(state, x, y), x, y);
}

fn set_level_to(state: &App, level: f64, point: Option<(f64, f64)>, x: f64, y: f64) {
    let fit = fit_level(state);
    let level = level.min(MOST);
    state.loupe.zoom.level.set(if level <= fit * 1.001 { 0.0 } else { level });
    if !zoomed(state) {

        state.loupe.zoom.full.borrow_mut().clear();
        state.loupe.zoom.pending.set(None);
    }
    apply(state);
    if let Some(point) = point {
        keep(state, point, x, y);
    }
    develop_next(state);
    refresh_loupe_bar(state);
}

fn apply(state: &App) {

    paint(state);
    let picture = &state.loupe.picture;
    match (zoomed(state), size(state)) {
        (true, Some((width, height))) => {
            let per = state.loupe.zoom.level.get() / picture.scale_factor() as f64;
            picture.set_size_request((width * per).round() as i32, (height * per).round() as i32);
        }
        _ => picture.set_size_request(-1, -1),
    }
}

fn keep(state: &App, point: (f64, f64), x: f64, y: f64) {
    state.loupe.zoom.pending.set(Some((point, (x, y))));
    settle(state);
}

fn settle(state: &App) {
    let Some(((u, v), (x, y))) = state.loupe.zoom.pending.get() else { return };
    let Some((left, top, width, height)) = drawn(state) else { return };
    let scroller = &state.loupe.zoom.scroller;
    scroller.hadjustment().set_value(left + u * width - x);
    scroller.vadjustment().set_value(top + v * height - y);
}

pub(super) fn centre(state: &App) -> Option<(f64, f64)> {
    let (x, y) = middle(state);
    zoomed(state).then(|| fraction_at(state, x, y)).flatten()
}

pub(super) fn stepped(state: &App, centre: Option<(f64, f64)>) {
    if !zoomed(state) {
        return;
    }
    let keep_ids: Vec<i64> = state.loupe.at.get().map_or(Vec::new(), |at| {
        [Some(at), Some(at + 1)].into_iter().flatten().filter_map(|index| id_at(state, index)).collect()
    });
    state.loupe.zoom.full.borrow_mut().retain(|id, _| keep_ids.contains(id));
    apply(state);
    if let Some(point) = centre {
        let (x, y) = middle(state);
        keep(state, point, x, y);
    }
    develop_next(state);
}

pub(super) fn reset(state: &App) {
    state.loupe.zoom.level.set(0.0);
    state.loupe.zoom.pending.set(None);
    state.loupe.zoom.full.borrow_mut().clear();
    state.loupe.picture.set_size_request(-1, -1);
}

fn develop_next(state: &App) {
    if !zoomed(state) || state.loupe.zoom.busy.get().is_some() {
        return;
    }
    let Some(at) = state.loupe.at.get() else { return };
    let wanted = [at, at + 1].into_iter().find_map(|index| {
        let card = state.grid.lazy.borrow().get(index).map(|card| card.path.clone())?;
        let id = id_at(state, index)?;
        (!state.loupe.zoom.full.borrow().contains_key(&id)).then_some((id, card))
    });
    let Some((id, path)) = wanted else { return };

    state.loupe.zoom.busy.set(Some(id));
    let state = state.clone();
    glib::spawn_future_local(async move {
        let developed = gio::spawn_blocking(move || raw::as_shot(&path)).await;
        state.loupe.zoom.busy.set(None);
        match developed {
            Ok(Ok(image)) => {

                let near = state.loupe.at.get().is_some_and(|at| {
                    [at, at + 1].into_iter().any(|index| id_at(&state, index) == Some(id))
                });
                if zoomed(&state) && near {
                    state.loupe.zoom.long_edge.set(Some(image.width().max(image.height())));
                    let centre = centre(&state);
                    state.loupe.zoom.full.borrow_mut().insert(id, crate::ui::display::texture(image));

                    apply(&state);
                    if let Some(point) = centre {
                        let (x, y) = middle(&state);
                        keep(&state, point, x, y);
                    }
                    refresh_loupe_bar(&state);
                }
            }
            Ok(Err(err)) => state.toast(&format!("Could not develop it at full size: {err}")),
            Err(_) => {}
        }
        develop_next(&state);
    });
}
