use super::*;

#[derive(Clone)]
struct Shot {
    card: usize,
}

#[derive(Clone)]
pub(super) struct State {
    strip: gtk::Box,

    pub(super) slot: gtk::Box,

    position: gtk::Label,

    shots: Rc<RefCell<Vec<Shot>>>,
    pub(super) times: Rc<RefCell<Vec<i64>>>,

    place: Rc<RefCell<Vec<usize>>>,

    unsettled: Rc<Cell<bool>>,
    pub(super) quiet: Rc<Cell<bool>>,
    pub(super) review: review::State,
}

impl State {
    pub(super) fn new() -> Self {
        Self {
            strip: gtk::Box::new(gtk::Orientation::Vertical, 4),
            slot: gtk::Box::new(gtk::Orientation::Vertical, 0),
            position: gtk::Label::new(None),
            shots: Rc::default(),
            times: Rc::default(),
            place: Rc::default(),
            unsettled: Rc::default(),
            quiet: Rc::default(),
            review: review::State::new(),
        }
    }
}

pub(super) fn build(state: &App) -> gtk::Box {
    let band = &state.loupe.band;
    let strip = band.strip.clone();
    strip.add_css_class("loupe-band");
    strip.add_css_class("numa-content");

    let caption = build_caption(state);
    band.position.add_css_class("loupe-caption");
    band.position.add_css_class("numeric");
    caption.insert_child_after(&band.position, Some(&state.loupe.caption));
    let end = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    end.append(&build_loupe_bar(state));
    end.append(&review::build_button(state));
    let line = gtk::CenterBox::new();
    line.set_start_widget(Some(&caption));
    line.set_end_widget(Some(&end));
    strip.append(&line);

    strip.append(&band.slot);
    cullbar::keep_focus(strip.upcast_ref());
    strip
}

fn clock(photo: &Photo) -> i64 {
    photo.taken.unwrap_or_else(|| {
        let offset = glib::DateTime::from_unix_local(photo.mtime).map_or(0, |time| time.utc_offset().as_seconds());
        photo.mtime + offset
    })
}

pub(super) fn rebuild(state: &App) {
    let band = &state.loupe.band;
    let (shots, times, place) = {
        let lazy = state.grid.lazy.borrow();
        let cards = state.grid.cards.borrow();
        let photos: Vec<Option<&Photo>> = lazy.iter().map(|card| cards.get(&card.id)).collect();

        let few: std::collections::HashSet<usize> = state.loupe.few.borrow().selection.iter().copied().collect();
        let shown = |at: &usize| few.is_empty() || few.contains(at);
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
                place[card] = shot;
                continue;
            }
            place[card] = shots.len();
            shots.push(Shot { card });
            times.push(time);
        }
        (shots, times, place)
    };
    *band.shots.borrow_mut() = shots;
    *band.times.borrow_mut() = times;
    *band.place.borrow_mut() = place;
}

pub(super) fn reset(state: &App) {
    review::halt(state);
    state.loupe.band.unsettled.set(false);
}

pub(super) fn current(state: &App) -> Option<usize> {
    let at = state.loupe.at.get()?;
    state.loupe.band.place.borrow().get(at).copied()
}

pub(super) fn card_of(state: &App, shot: usize) -> Option<usize> {
    state.loupe.band.shots.borrow().get(shot).map(|shot| shot.card)
}

pub(super) fn shot_count(state: &App) -> usize {
    state.loupe.band.shots.borrow().len()
}

pub(super) fn show_quietly(state: &App, card: usize) {
    let band = &state.loupe.band;
    band.quiet.set(true);
    state.grid.wall.select_only(card);
    show_loupe(state, card);
    band.quiet.set(false);
    band.unsettled.set(true);
}

pub(super) fn settle(state: &App) {
    if state.loupe.band.unsettled.replace(false) {
        if let Some(at) = state.loupe.at.get() {
            go_to(state, at);
        }
    }
}

pub(super) fn refresh(state: &App) {
    let total = shot_count(state);
    let here = current(state).map_or(0, |shot| shot + 1);
    state.loupe.band.position.set_text(&format!("Frame {} of {}", places::grouped(here as i64), places::grouped(total as i64)));
}
