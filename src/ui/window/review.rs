use super::*;

const AHEAD: usize = 12;

const WAIT_MS: u32 = 15;

#[derive(Clone)]
pub(super) struct State {

    pub(super) pill: gtk::Revealer,
    words: gtk::Label,
    button: gtk::Button,
    running: Rc<Cell<bool>>,
    timer: Rc<RefCell<Option<glib::SourceId>>>,

    end: Rc<Cell<usize>>,

    began: Rc<Cell<Option<std::time::Instant>>>,
    shown: Rc<Cell<u32>>,
    waited: Rc<Cell<u32>>,
}

impl State {
    pub(super) fn new() -> Self {
        Self {
            pill: gtk::Revealer::new(),
            words: gtk::Label::new(None),
            button: gtk::Button::new(),
            running: Rc::default(),
            timer: Rc::default(),
            end: Rc::default(),
            began: Rc::default(),
            shown: Rc::default(),
            waited: Rc::default(),
        }
    }
}

pub(super) fn build_button(state: &App) -> gtk::Button {
    let review = &state.loupe.tape.review;
    let button = review.button.clone();
    label(&button, false);
    button.set_tooltip_text(Some("Run through the shoot at full size, about six a second — any key stops (Shift+Space)"));
    button.connect_clicked(glib::clone!(
        #[strong] state,
        move |_| toggle(&state)
    ));
    button
}

fn label(button: &gtk::Button, running: bool) {
    let content = adw::ButtonContent::new();
    content.set_label(if running { "Stop" } else { "Review" });
    content.set_icon_name(if running { "media-playback-stop-symbolic" } else { "media-playback-start-symbolic" });
    button.set_child(Some(&content));
}

pub(super) fn build_pill(state: &App) -> gtk::Revealer {
    let review = &state.loupe.tape.review;
    review.words.add_css_class("photo-pill");
    let pill = review.pill.clone();
    pill.set_child(Some(&review.words));
    pill.set_transition_type(gtk::RevealerTransitionType::Crossfade);
    pill.set_halign(gtk::Align::Center);
    pill.set_valign(gtk::Align::Start);
    pill.set_margin_top(12);
    pill.set_can_target(false);
    pill
}

pub(super) fn running(state: &App) -> bool {
    state.loupe.tape.review.running.get()
}

pub(super) fn toggle(state: &App) {
    match running(state) {
        true => stop(state),
        false => start(state),
    }
}

fn start(state: &App) {
    let review = &state.loupe.tape.review;
    let count = tape::shot_count(state);
    let Some(here) = tape::current(state).filter(|_| count > 0) else { return };
    let (from, end) = match tape::range(state) {
        Some(range) => range,

        None if here + 1 >= count => (0, count - 1),
        None => (here, count - 1),
    };

    tape::settle(state);
    leave_visit(state);

    if loupe_zoom::zoomed(state) {
        loupe_zoom::reset(state);
    }
    review.running.set(true);
    review.end.set(end);
    review.began.set(Some(std::time::Instant::now()));
    review.shown.set(0);
    review.waited.set(0);
    label(&review.button, true);

    if let Some(card) = tape::card_of(state, from) {
        tape::show_quietly(state, card);
    }
    say(state, from);
    review.pill.set_transition_duration(200);
    review.pill.set_reveal_child(true);
    next_in(state, pace(state, from));
}

pub(super) fn stop(state: &App) {
    if !running(state) {
        return;
    }
    halt(state);
    tape::settle(state);
}

pub(super) fn halt(state: &App) {
    let review = &state.loupe.tape.review;
    if !review.running.replace(false) {
        return;
    }
    if let Some(timer) = review.timer.take() {
        timer.remove();
    }
    review.pill.set_transition_duration(160);
    review.pill.set_reveal_child(false);
    label(&review.button, false);
    if let (true, Some(began)) = (timing(), review.began.take()) {
        let seconds = began.elapsed().as_secs_f64();
        let shown = review.shown.get();
        eprintln!(
            "x-data: review {shown} frames in {seconds:.2} s = {:.1} a second, {} ms waiting for decodes",
            shown as f64 / seconds.max(0.001),
            review.waited.get()
        );
    }
}

fn pace(state: &App, shot: usize) -> u32 {
    cull::tape::pace_ms(&state.loupe.tape.times.borrow(), shot)
}

fn say(state: &App, shot: usize) {
    let rate = (1000.0 / pace(state, shot) as f64).round();
    state.loupe.tape.review.words.set_text(&format!("Reviewing \u{b7} {rate} a second"));
}

fn next_in(state: &App, ms: u32) {
    let timer = glib::timeout_add_local_once(
        std::time::Duration::from_millis(ms as u64),
        glib::clone!(
            #[strong] state,
            move || {
                state.loupe.tape.review.timer.take();
                step(&state);
            }
        ),
    );
    *state.loupe.tape.review.timer.borrow_mut() = Some(timer);
}

fn step(state: &App) {
    let review = &state.loupe.tape.review;
    let Some(here) = tape::current(state).filter(|_| running(state)) else { return };
    let next = here + 1;
    let Some(card) = tape::card_of(state, next).filter(|_| here < review.end.get()) else {
        stop(state);
        return;
    };
    let ready = id_at(state, card).is_some_and(|id| state.loupe.textures.borrow().contains_key(&id));
    if !ready {
        review.waited.set(review.waited.get() + WAIT_MS);
        next_in(state, WAIT_MS);
        return;
    }
    tape::show_quietly(state, card);
    review.shown.set(review.shown.get() + 1);
    say(state, next);
    next_in(state, pace(state, next));
}

pub(super) fn ahead(state: &App) -> Vec<usize> {
    let Some(here) = tape::current(state).filter(|_| running(state)) else { return Vec::new() };
    let end = state.loupe.tape.review.end.get();
    (here + 1..=end.min(here + AHEAD)).filter_map(|shot| tape::card_of(state, shot)).collect()
}
