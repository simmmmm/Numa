use super::*;

const MARK: f64 = 5.0;
const PITCH: f64 = 7.0;

const WIDEST: f64 = 30.0;

const LABEL_ROOM: f64 = 88.0;

const EDGE: f64 = 16.0;
const HEIGHT: i32 = 60;

const FOOT: f64 = 8.0;

const SHORT: f64 = 16.0;
const STAR: f64 = 2.0;
const TALL: f64 = 30.0;

const REST: f64 = 0.32;
const PICKED: f64 = 0.85;
const REJECTED: f64 = 0.12;
const TIME: f64 = 0.55;

const DRAG_SLOP: f64 = 4.0;

#[derive(Clone)]
struct Shot {
    card: usize,
    twins: Vec<usize>,
}

#[derive(Clone)]
pub(super) struct State {
    band: gtk::Box,
    area: gtk::DrawingArea,

    position: gtk::Label,

    title: gtk::Label,
    acts: gtk::Box,
    reject: gtk::Button,
    pick: gtk::Button,

    shots: Rc<RefCell<Vec<Shot>>>,
    pub(super) times: Rc<RefCell<Vec<i64>>>,

    place: Rc<RefCell<Vec<usize>>>,
    layout: Rc<RefCell<cull::tape::Layout>>,

    scroll: gtk::Adjustment,

    range: Rc<Cell<Option<(usize, usize)>>>,

    pressed: Rc<Cell<Option<(usize, bool)>>>,

    home: Rc<Cell<Option<usize>>>,

    unsettled: Rc<Cell<bool>>,
    pub(super) quiet: Rc<Cell<bool>>,
    pub(super) review: review::State,
}

impl State {
    pub(super) fn new() -> Self {
        Self {
            band: gtk::Box::new(gtk::Orientation::Vertical, 4),
            area: gtk::DrawingArea::new(),
            position: gtk::Label::new(None),
            title: gtk::Label::new(None),
            acts: gtk::Box::new(gtk::Orientation::Horizontal, 8),
            reject: gtk::Button::new(),
            pick: gtk::Button::new(),
            shots: Rc::default(),
            times: Rc::default(),
            place: Rc::default(),
            layout: Rc::default(),
            scroll: gtk::Adjustment::new(0.0, 0.0, 0.0, 1.0, 1.0, 0.0),
            range: Rc::default(),
            pressed: Rc::default(),
            home: Rc::default(),
            unsettled: Rc::default(),
            quiet: Rc::default(),
            review: review::State::new(),
        }
    }
}

pub(super) fn build(state: &App) -> gtk::Box {
    let tape = &state.loupe.tape;
    let band = tape.band.clone();
    band.add_css_class("tape-band");
    band.add_css_class("numa-content");

    let caption = build_caption(state);
    tape.position.add_css_class("loupe-caption");
    tape.position.add_css_class("numeric");
    caption.insert_child_after(&tape.position, Some(&state.loupe.caption));
    tape.title.add_css_class("tape-title");
    tape.acts.append(&tape.title);
    for (button, flag, tip) in [
        (&tape.reject, Flag::Rejected, "Reject the chosen frames (X)"),
        (&tape.pick, Flag::Picked, "Pick the chosen frames (P)"),
    ] {
        button.set_tooltip_text(Some(tip));
        button.connect_clicked(glib::clone!(
            #[strong] state,
            move |_| act_on_range(&state, Action::Flag(flag))
        ));
        tape.acts.append(button);
    }
    tape.acts.append(&rate_menu(state));
    tape.acts.set_visible(false);
    let end = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    end.append(&build_loupe_bar(state));
    end.append(&review::build_button(state));
    let line = gtk::CenterBox::new();
    line.set_start_widget(Some(&caption));
    line.set_center_widget(Some(&tape.acts));
    line.set_end_widget(Some(&end));
    band.append(&line);

    let area = &tape.area;
    area.add_css_class("tape");
    area.update_property(&[gtk::accessible::Property::Label("The shoot, by the time each frame was shot")]);
    area.set_content_height(HEIGHT);
    area.set_hexpand(true);
    area.set_draw_func(glib::clone!(
        #[strong] state,
        move |area, cr, width, height| draw(&state, area, cr, width, height)
    ));
    area.connect_resize(glib::clone!(
        #[strong] state,
        move |_, width, _| {

            state.loupe.tape.scroll.set_page_size(width as f64);
            follow(&state);
        }
    ));
    tape.scroll.connect_value_changed(glib::clone!(
        #[weak] area,
        move |_| area.queue_draw()
    ));
    install_gestures(state, area);
    band.append(area);
    cullbar::keep_focus(band.upcast_ref());
    band
}

fn rate_menu(state: &App) -> gtk::MenuButton {
    let rate = gio::SimpleAction::new("rate", Some(glib::VariantTy::INT32));
    rate.connect_activate(glib::clone!(
        #[strong] state,
        move |_, value| {
            if let Some(stars) = value.and_then(|value| value.get::<i32>()) {
                act_on_range(&state, Action::Rate(stars.clamp(0, 5) as u8));
            }
        }
    ));
    let group = gio::SimpleActionGroup::new();
    group.add_action(&rate);
    state.loupe.tape.band.insert_action_group("tape", Some(&group));
    let button = cullbar::rate_button("tape.rate");
    button.set_tooltip_text(Some("Rate the chosen frames (0–5)"));
    button
}

fn install_gestures(state: &App, area: &gtk::DrawingArea) {
    let motion = gtk::EventControllerMotion::new();
    motion.connect_motion(glib::clone!(
        #[strong] state,
        move |_, x, _| skim(&state, x)
    ));
    motion.connect_leave(glib::clone!(
        #[strong] state,
        move |_| end_skim(&state)
    ));
    area.add_controller(motion);

    let drag = gtk::GestureDrag::new();
    drag.connect_drag_begin(glib::clone!(
        #[strong] state,
        move |gesture, x, _| {
            review::stop(&state);
            let tape = &state.loupe.tape;
            let Some(shot) = shot_at(&state, x) else { return };
            let shift = gesture.current_event_state().contains(gtk::gdk::ModifierType::SHIFT_MASK);

            let home = tape.home.get().and_then(|card| tape.place.borrow().get(card).copied());
            let anchor = match (shift, tape.range.get(), home.or_else(|| current(&state))) {
                (true, Some((anchor, _)), _) | (true, None, Some(anchor)) => anchor,
                _ => shot,
            };
            tape.pressed.set(Some((anchor, shift)));
            if shift {
                tape.range.set(Some((anchor, shot)));
                refresh(&state);
            }
        }
    ));
    drag.connect_drag_update(glib::clone!(
        #[strong] state,
        move |gesture, dx, _| {
            let tape = &state.loupe.tape;
            let (Some((anchor, shift)), Some((x, _))) = (tape.pressed.get(), gesture.start_point()) else { return };
            if dx.abs() < DRAG_SLOP && !shift {
                return;
            }
            if let Some(shot) = shot_at(&state, x + dx) {
                tape.range.set(Some((anchor, shot)));
                refresh(&state);
            }
        }
    ));
    drag.connect_drag_end(glib::clone!(
        #[strong] state,
        move |gesture, dx, _| {
            let tape = &state.loupe.tape;
            let (Some((_, shift)), Some((x, _))) = (tape.pressed.take(), gesture.start_point()) else { return };
            if shift || dx.abs() >= DRAG_SLOP {
                return;
            }

            tape.range.set(None);
            if let Some(card) = shot_at(&state, x).and_then(|shot| card_of(&state, shot)) {
                tape.home.set(None);
                tape.unsettled.set(false);
                go_to(&state, card);
            }
        }
    ));
    area.add_controller(drag);

    let wheel = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::BOTH_AXES);
    wheel.connect_scroll(glib::clone!(
        #[strong] state,
        move |_, dx, dy| {
            let scroll = &state.loupe.tape.scroll;
            scroll.set_value(scroll.value() + (dx + dy) * PITCH * 6.0);
            glib::Propagation::Stop
        }
    ));
    area.add_controller(wheel);
}

fn clock(photo: &Photo) -> i64 {
    photo.taken.unwrap_or_else(|| {
        let offset = glib::DateTime::from_unix_local(photo.mtime).map_or(0, |time| time.utc_offset().as_seconds());
        photo.mtime + offset
    })
}

pub(super) fn rebuild(state: &App) {
    let tape = &state.loupe.tape;
    let (shots, times, place) = {
        let lazy = state.grid.lazy.borrow();
        let cards = state.grid.cards.borrow();
        let photos: Vec<Option<&Photo>> = lazy.iter().map(|card| cards.get(&card.id)).collect();

        let scope = state.loupe.scope.borrow();
        let shown = |at: &usize| scope.is_empty() || scope.contains(at);
        let index: HashMap<i64, usize> = lazy.iter().enumerate().filter(|(at, _)| shown(at)).map(|(at, card)| (card.id, at)).collect();
        let twins = numa::io::analysis::twins(photos.iter().flatten().map(|photo| (photo.id, photo.path.as_path(), photo.taken)), raw::is_raw);

        let raw_of = |photo: Option<&Photo>| photo.and_then(|photo| twins.get(&photo.id)).and_then(|raw| index.get(raw)).copied();

        let mut order: Vec<(i64, bool, usize)> = photos
            .iter()
            .enumerate()
            .filter(|(at, _)| shown(at))
            .map(|(at, photo)| (photo.map_or(0, clock), raw_of(*photo).is_some(), at))
            .collect();
        order.sort_unstable();
        let mut shots: Vec<Shot> = Vec::with_capacity(order.len());
        let mut times = Vec::with_capacity(order.len());
        let mut place = vec![0; lazy.len()];
        for (time, _, card) in order {
            if let Some(raw) = raw_of(photos[card]) {
                let shot = place[raw];
                shots[shot].twins.push(card);
                place[card] = shot;
                continue;
            }
            place[card] = shots.len();
            shots.push(Shot { card, twins: Vec::new() });
            times.push(time);
        }
        (shots, times, place)
    };
    let layout = cull::tape::layout(&times, PITCH, WIDEST, LABEL_ROOM);
    tape.scroll.set_upper(layout.width + 2.0 * EDGE);
    *tape.layout.borrow_mut() = layout;
    *tape.shots.borrow_mut() = shots;
    *tape.times.borrow_mut() = times;
    *tape.place.borrow_mut() = place;
    tape.range.set(None);
    tape.home.set(None);
}

pub(super) fn reset(state: &App) {
    let tape = &state.loupe.tape;
    review::halt(state);
    tape.range.set(None);
    tape.home.set(None);
    tape.unsettled.set(false);
    tape.pressed.set(None);
}

pub(super) fn current(state: &App) -> Option<usize> {
    let at = state.loupe.at.get()?;
    state.loupe.tape.place.borrow().get(at).copied()
}

pub(super) fn card_of(state: &App, shot: usize) -> Option<usize> {
    state.loupe.tape.shots.borrow().get(shot).map(|shot| shot.card)
}

pub(super) fn shot_count(state: &App) -> usize {
    state.loupe.tape.shots.borrow().len()
}

pub(super) fn range(state: &App) -> Option<(usize, usize)> {
    state.loupe.tape.range.get().map(|(a, b)| (a.min(b), a.max(b)))
}

fn shot_at(state: &App, x: f64) -> Option<usize> {
    let tape = &state.loupe.tape;
    let layout = tape.layout.borrow();
    let along = x + tape.scroll.value() - EDGE - MARK / 2.0;
    let after = layout.x.partition_point(|left| *left < along);
    let before = after.checked_sub(1);
    [before, Some(after).filter(|&after| after < layout.x.len())]
        .into_iter()
        .flatten()
        .min_by(|a, b| (layout.x[*a] - along).abs().total_cmp(&(layout.x[*b] - along).abs()))
}

pub(super) fn show_quietly(state: &App, card: usize) {
    let tape = &state.loupe.tape;
    tape.quiet.set(true);
    state.grid.wall.select_only(card);
    show_loupe(state, card);
    tape.quiet.set(false);
    tape.unsettled.set(true);
}

pub(super) fn settle(state: &App) {
    let tape = &state.loupe.tape;
    tape.home.set(None);
    if tape.unsettled.replace(false) {
        if let Some(at) = state.loupe.at.get() {
            go_to(state, at);
        }
    }
}

fn skim(state: &App, x: f64) {
    let tape = &state.loupe.tape;
    if review::running(state) || loupe_zoom::zoomed(state) {
        return;
    }
    let Some(card) = shot_at(state, x).and_then(|shot| card_of(state, shot)) else { return };
    if state.loupe.at.get() == Some(card) {
        return;
    }
    if tape.home.get().is_none() {
        tape.home.set(state.loupe.at.get());
    }
    show_quietly(state, card);
}

fn end_skim(state: &App) {
    let tape = &state.loupe.tape;
    let Some(home) = tape.home.take() else { return };
    if state.loupe.at.get() != Some(home) {
        show_quietly(state, home);
    }
    tape.unsettled.set(false);
}

pub(super) fn held(state: &App) -> Vec<usize> {
    let mut held: Vec<usize> = state.loupe.tape.home.get().into_iter().collect();
    held.extend(review::ahead(state));
    held
}

pub(super) fn range_key(state: &App, key: gtk::gdk::Key, modifiers: gtk::gdk::ModifierType) -> bool {
    if range(state).is_none()
        || modifiers.intersects(gtk::gdk::ModifierType::CONTROL_MASK | gtk::gdk::ModifierType::ALT_MASK | gtk::gdk::ModifierType::SUPER_MASK)
    {
        return false;
    }
    if key == gtk::gdk::Key::Escape {
        state.loupe.tape.range.set(None);
        refresh(state);
        return true;
    }
    let action = match key.to_unicode() {
        Some(digit @ '0'..='5') => Action::Rate(digit as u8 - b'0'),
        Some('p' | 'P') => Action::Flag(Flag::Picked),
        Some('x' | 'X') => Action::Flag(Flag::Rejected),
        Some('u' | 'U') => Action::Flag(Flag::None),
        _ => return false,
    };
    let ids = range_ids(state);
    act_on_range(state, toggled(state, &ids, action));
    true
}

fn range_ids(state: &App) -> Vec<i64> {
    let Some((first, last)) = range(state) else { return Vec::new() };
    let shots = state.loupe.tape.shots.borrow();
    shots[first..=last.min(shots.len().saturating_sub(1))]
        .iter()
        .flat_map(|shot| std::iter::once(shot.card).chain(shot.twins.iter().copied()))
        .filter_map(|card| id_at(state, card))
        .collect()
}

fn act_on_range(state: &App, action: Action) {
    let Some((first, last)) = range(state) else { return };
    let ids = range_ids(state);
    if ids.is_empty() {
        return;
    }
    apply_to_ids(state, &ids, action);
    refresh_loupe_bar(state);
    let count = last - first + 1;
    let photographs = match count {
        1 => "1 photograph".to_string(),
        n => format!("{} photographs", places::grouped(n as i64)),
    };
    let done = match action {
        Action::Flag(Flag::Picked) => "picked".to_string(),
        Action::Flag(Flag::Rejected) => "rejected".to_string(),
        Action::Flag(Flag::None) => "unflagged".to_string(),
        Action::Rate(0) => "unrated".to_string(),
        Action::Rate(1) => "rated 1 star".to_string(),
        Action::Rate(stars) => format!("rated {stars} stars"),
    };
    let toast = adw::Toast::new(&format!("{photographs} {done}"));
    toast.set_button_label(Some("Undo"));
    let depth = state.loupe.undo.borrow().len();
    toast.connect_button_clicked(glib::clone!(
        #[strong] state,
        move |_| {

            if state.loupe.at.get().is_some() && state.loupe.undo.borrow().len() == depth {
                undo_mark(&state);
            }
        }
    ));
    state.toasts.add_toast(toast);
}

pub(super) fn refresh(state: &App) {
    let tape = &state.loupe.tape;
    tape.area.queue_draw();
    let total = shot_count(state);
    match range(state) {
        Some((first, last)) => {
            let count = places::grouped((last - first + 1) as i64);
            let (from, to) = (places::grouped(first as i64 + 1), places::grouped(last as i64 + 1));
            tape.title.set_text(&match first == last {
                true => format!("Frame {from} \u{b7} 1 photograph"),
                false => format!("Frames {from}\u{2013}{to} \u{b7} {count} photographs"),
            });
            tape.title.set_tooltip_text(Some("Escape lets them go"));
            tape.reject.set_label(&format!("Reject {count}"));
            tape.pick.set_label(&format!("Pick {count}"));
            tape.acts.set_visible(true);
        }
        None => tape.acts.set_visible(false),
    }
    let here = current(state).map_or(0, |shot| shot + 1);
    tape.position.set_text(&format!("Frame {} of {}", places::grouped(here as i64), places::grouped(total as i64)));
    tape.position.set_tooltip_text(Some("Drag on the tape to choose frames, Shift+click to choose up to a frame"));
}

pub(super) fn follow(state: &App) {
    let tape = &state.loupe.tape;
    let Some(shot) = current(state).filter(|_| tape.home.get().is_none()) else { return };
    let Some(left) = tape.layout.borrow().x.get(shot).copied() else { return };
    let (x, value, page) = (left + EDGE, tape.scroll.value(), tape.scroll.page_size());
    if page <= 0.0 || (x > value + page * 0.15 && x < value + page * 0.85) {
        return;
    }
    let to = (x - page / 2.0).clamp(0.0, (tape.scroll.upper() - page).max(0.0));
    glide(&tape.area, &tape.scroll, to);
}

fn time_text(time: i64, day_before: Option<i64>) -> String {
    let Ok(at) = glib::DateTime::from_unix_utc(time) else { return String::new() };
    let format = match day_before == Some(time.div_euclid(86_400)) {
        true => "%H:%M",
        false => "%e %b %H:%M",
    };
    at.format(format).map(|text| text.trim().to_string()).unwrap_or_default()
}

fn draw(state: &App, area: &gtk::DrawingArea, cr: &gtk::cairo::Context, width: i32, height: i32) {
    let tape = &state.loupe.tape;
    let ink = area.color();
    let paint = |alpha: f64| cr.set_source_rgba(ink.red() as f64, ink.green() as f64, ink.blue() as f64, alpha * ink.alpha() as f64);
    let layout = tape.layout.borrow();
    let shots = tape.shots.borrow();
    let times = tape.times.borrow();
    let left = tape.scroll.value() - EDGE;
    let (width, foot) = (width as f64, height as f64 - FOOT);

    paint(TIME);
    let mut day = None;
    for &index in &layout.labels {
        let x = layout.x[index] - left;
        let text = time_text(times[index], day);
        day = Some(times[index].div_euclid(86_400));
        if x > width {
            break;
        }
        if x < -LABEL_ROOM {
            continue;
        }
        let words = area.create_pango_layout(Some(&text));
        cr.move_to(x, 0.0);
        pangocairo::functions::show_layout(cr, &words);
    }

    let now = current(state);
    let home = tape.home.get().and_then(|card| tape.place.borrow().get(card).copied());
    let lazy = state.grid.lazy.borrow();
    let cards = state.grid.cards.borrow();
    let first = layout.x.partition_point(|x| *x < left - MARK);
    for (index, shot) in shots.iter().enumerate().skip(first) {

        let x = (layout.x[index] - left).round();
        if x > width {
            break;
        }
        let marks = lazy.get(shot.card).and_then(|card| cards.get(&card.id)).map_or((0, Flag::None), |photo| (photo.rating, photo.flag));
        let (tall, alpha) = match (Some(index) == now, Some(index) == home, marks) {
            (true, _, _) => (TALL, 1.0),

            (_, true, _) => (TALL, REST),
            (_, _, (rating, flag)) => (
                SHORT + STAR * rating as f64,
                match flag {
                    Flag::Picked => PICKED,
                    Flag::Rejected => REJECTED,
                    Flag::None => REST,
                },
            ),
        };
        paint(alpha);
        cr.rectangle(x, foot - tall, MARK, tall);
        let _ = cr.fill();
    }

    if let Some((first, last)) = range(state).filter(|(_, last)| *last < layout.x.len()) {
        let (x0, x1) = ((layout.x[first] - left).round() - 4.0, (layout.x[last] - left).round() + MARK + 4.0);
        let (y0, y1) = (foot - TALL - 4.0, foot + 4.0);
        rounded(cr, x0 + 1.0, y0 + 1.0, x1 - x0 - 2.0, y1 - y0 - 2.0, 6.0);
        paint(1.0);
        cr.set_line_width(2.0);
        let _ = cr.stroke();
    }
}

fn rounded(cr: &gtk::cairo::Context, x: f64, y: f64, w: f64, h: f64, r: f64) {
    use std::f64::consts::{FRAC_PI_2, PI};
    cr.new_sub_path();
    cr.arc(x + w - r, y + r, r, -FRAC_PI_2, 0.0);
    cr.arc(x + w - r, y + h - r, r, 0.0, FRAC_PI_2);
    cr.arc(x + r, y + h - r, r, FRAC_PI_2, PI);
    cr.arc(x + r, y + r, r, PI, 3.0 * FRAC_PI_2);
    cr.close_path();
}
