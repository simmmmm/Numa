use super::*;
use numa::io::made::{self, Made};

const STEP_MS: u64 = 1000;

const FADE_MS: u32 = 300;

struct Card {
    root: gtk::Box,
    rows: gtk::Box,
    scroller: gtk::ScrolledWindow,
    back: gtk::Button,
    play: gtk::Button,
    forward: gtk::Button,
    caption: gtk::Label,

    fader: gtk::Picture,
}

struct Playing {
    made: Made,

    at: usize,
    timer: Option<glib::SourceId>,
}

thread_local! {
    static CARD: RefCell<Option<Card>> = const { RefCell::new(None) };
    static PLAYING: RefCell<Option<Playing>> = const { RefCell::new(None) };

    static ARMED: Cell<u64> = const { Cell::new(0) };
}

pub(super) fn shown() -> Option<EditState> {
    PLAYING.with(|playing| {
        let playing = playing.borrow();
        let playing = playing.as_ref()?;
        Some(match playing.at.checked_sub(1) {
            Some(step) => playing.made.steps[step].state.clone(),
            None => playing.made.start.clone(),
        })
    })
}

pub(super) fn build(state: &App) {
    let root = gtk::Box::new(gtk::Orientation::Vertical, 0);
    root.add_css_class("photo-card");
    root.set_halign(gtk::Align::Start);
    root.set_valign(gtk::Align::Start);
    root.set_margin_top(14);
    root.set_margin_start(14);
    root.set_size_request(380, -1);
    root.set_visible(false);

    let head = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    let title = section_header("How It Was Made");
    title.set_margin_top(0);
    title.set_hexpand(true);
    let exit = gtk::Button::from_icon_name("window-close-symbolic");
    exit.add_css_class("flat");
    exit.add_css_class("circular");
    exit.set_tooltip_text(Some("Close"));
    exit.connect_clicked(glib::clone!(#[strong] state, move |_| close(&state)));
    head.append(&title);
    head.append(&exit);
    root.append(&head);

    let rows = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let scroller = gtk::ScrolledWindow::new();
    scroller.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    scroller.set_propagate_natural_height(true);
    scroller.set_max_content_height(300);
    scroller.set_child(Some(&rows));
    root.append(&scroller);

    let foot = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    foot.set_margin_top(8);
    let back = gtk::Button::from_icon_name("go-previous-symbolic");
    back.set_tooltip_text(Some("Step Back"));
    let play = gtk::Button::with_label("Pause");
    let forward = gtk::Button::from_icon_name("go-next-symbolic");
    forward.set_tooltip_text(Some("Step Forward"));
    let caption = gtk::Label::new(None);
    caption.add_css_class("made-caption");
    caption.set_hexpand(true);
    caption.set_xalign(1.0);
    caption.set_ellipsize(gtk::pango::EllipsizeMode::End);
    for widget in [back.upcast_ref::<gtk::Widget>(), play.upcast_ref(), forward.upcast_ref(), caption.upcast_ref()] {
        foot.append(widget);
    }
    root.append(&foot);

    back.connect_clicked(glib::clone!(#[strong] state, move |_| step(&state, -1)));
    forward.connect_clicked(glib::clone!(#[strong] state, move |_| step(&state, 1)));
    play.connect_clicked(glib::clone!(#[strong] state, move |_| toggle_play(&state)));

    let fader = gtk::Picture::new();
    fader.set_can_shrink(true);
    fader.set_can_target(false);
    fader.set_visible(false);
    CARD.with(|card| *card.borrow_mut() = Some(Card { root, rows, scroller, back, play, forward, caption, fader }));
}

pub(super) fn card() -> gtk::Box {
    CARD.with(|card| card.borrow().as_ref().map(|card| card.root.clone())).unwrap_or_default()
}

pub(super) fn fader() -> gtk::Picture {
    CARD.with(|card| card.borrow().as_ref().map(|card| card.fader.clone())).unwrap_or_default()
}

pub(super) fn toggle(state: &App) {
    if PLAYING.with(|playing| playing.borrow().is_some()) {
        close(state);
    } else {
        open(state);
    }
}

fn open(state: &App) {
    let built = {
        let open = state.open.borrow();
        let Some(photo) = open.as_ref() else { return };
        let mut states = photo.history.states[..=photo.history.position].to_vec();

        let now = EditState::of(&photo.document);
        if states.last() != Some(&now) {
            states.push(now);
        }
        let applied = match &photo.source {
            Source::Photo { id, .. } => state.catalog.looks_applied(*id),
            Source::Bracket { .. } => Default::default(),
        };
        made::how_it_was_made(&states, &applied)
    };
    if built.steps.is_empty() {
        state.toast("Nothing has been done to this photograph yet");
        return;
    }
    CARD.with(|card| {
        let card = card.borrow();
        let Some(card) = card.as_ref() else { return };
        while let Some(row) = card.rows.first_child() {
            card.rows.remove(&row);
        }
        for step in &built.steps {
            card.rows.append(&row_of(step));
        }
        card.root.set_visible(true);
    });
    PLAYING.with(|playing| *playing.borrow_mut() = Some(Playing { made: built, at: 0, timer: None }));
    show(state, 0, false);
    start(state);
}

fn row_of(step: &made::MadeStep) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    row.add_css_class("made-row");
    let dot = gtk::Box::new(gtk::Orientation::Vertical, 0);
    dot.add_css_class("made-dot");
    dot.set_valign(gtk::Align::Start);
    let tick = gtk::Image::from_icon_name("object-select-symbolic");
    tick.set_pixel_size(12);
    tick.set_vexpand(true);
    dot.append(&tick);

    let text = gtk::Box::new(gtk::Orientation::Vertical, 0);
    text.set_hexpand(true);
    let line = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    let what = gtk::Label::new(Some(&step.what));
    what.add_css_class("made-what");
    what.set_xalign(0.0);
    what.set_wrap(true);
    line.append(&what);
    if !step.tab.is_empty() {
        let tab = gtk::Label::new(Some(&format!("· {}", step.tab)));
        tab.add_css_class("made-tab");
        line.append(&tab);
    }
    text.append(&line);
    if let Some(why) = &step.why {
        let why = gtk::Label::new(Some(why));
        why.add_css_class("made-why");
        why.set_xalign(0.0);
        why.set_wrap(true);
        text.append(&why);
    }
    row.append(&dot);
    row.append(&text);
    row
}

fn start(state: &App) {
    stop_timer();
    let timer = glib::timeout_add_local(
        std::time::Duration::from_millis(STEP_MS),
        glib::clone!(
            #[strong] state,
            move || {
                let next = PLAYING.with(|playing| playing.borrow().as_ref().map(|p| (p.at + 1, p.made.steps.len())));
                match next {
                    Some((at, len)) if at <= len => {
                        show(&state, at, true);
                        if at < len {
                            return glib::ControlFlow::Continue;
                        }

                        PLAYING.with(|playing| playing.borrow_mut().as_mut().map(|p| p.timer = None));
                        refresh_controls();
                        glib::ControlFlow::Break
                    }
                    _ => glib::ControlFlow::Break,
                }
            }
        ),
    );
    PLAYING.with(|playing| playing.borrow_mut().as_mut().map(|p| p.timer = Some(timer)));
    refresh_controls();
}

fn stop_timer() {
    if let Some(timer) = PLAYING.with(|playing| playing.borrow_mut().as_mut().and_then(|p| p.timer.take())) {
        timer.remove();
    }
}

fn toggle_play(state: &App) {
    let (running, at, len) = PLAYING.with(|playing| {
        let playing = playing.borrow();
        playing.as_ref().map_or((false, 0, 0), |p| (p.timer.is_some(), p.at, p.made.steps.len()))
    });
    if running {
        stop_timer();
        refresh_controls();
    } else {

        if at >= len {
            show(state, 0, true);
        }
        start(state);
    }
}

fn step(state: &App, by: isize) {
    stop_timer();
    let landing = PLAYING.with(|playing| playing.borrow().as_ref().map(|p| p.at.saturating_add_signed(by).min(p.made.steps.len())));
    if let Some(at) = landing {
        show(state, at, true);
    }
    refresh_controls();
}

fn show(state: &App, at: usize, fade: bool) {
    let changed = PLAYING.with(|playing| playing.borrow_mut().as_mut().map(|p| std::mem::replace(&mut p.at, at) != at));
    if changed.is_none() {
        return;
    }
    CARD.with(|card| {
        let card = card.borrow();
        let Some(card) = card.as_ref() else { return };

        if fade && changed == Some(true) && gtk::Settings::default().is_some_and(|settings| settings.is_gtk_enable_animations()) {
            if let Some(frame) = state.canvas.paintable() {
                card.fader.set_content_fit(state.canvas.content_fit());
                card.fader.set_paintable(Some(&frame.current_image()));
                card.fader.set_opacity(1.0);
                card.fader.set_visible(true);
                let armed = ARMED.get().wrapping_add(1).max(1);
                ARMED.set(armed);

                glib::timeout_add_local_once(std::time::Duration::from_millis(1500), glib::clone!(
                    #[weak(rename_to = fader)] card.fader,
                    move || if ARMED.get() == armed {
                        ARMED.set(0);
                        fader.set_visible(false);
                    }
                ));
            }
        }
        let mut row = card.rows.first_child();
        let mut index = 0;
        while let Some(widget) = row {
            if index < at { widget.add_css_class("shown") } else { widget.remove_css_class("shown") }
            if index + 1 == at {
                scroll_to(&card.scroller, &card.rows, &widget);
            }
            row = widget.next_sibling();
            index += 1;
        }
        let caption = PLAYING.with(|playing| {
            let playing = playing.borrow();
            let playing = playing.as_ref()?;
            let look = at.checked_sub(1).and_then(|step| playing.made.steps[step].look);
            Some(match (look, at) {
                (Some(look), _) => playing.made.looks[look].line(),
                (None, 0) => "As shot".to_string(),
                (None, at) => format!("{at} of {}", playing.made.steps.len()),
            })
        });
        card.caption.set_text(&caption.unwrap_or_default());
    });
    refresh_controls();
    request_render(state);
}

fn scroll_to(scroller: &gtk::ScrolledWindow, rows: &gtk::Box, row: &gtk::Widget) {
    let Some(bounds) = row.compute_bounds(rows) else { return };
    let adjustment = scroller.vadjustment();
    let (top, bottom) = (bounds.y() as f64, (bounds.y() + bounds.height()) as f64);
    if bottom > adjustment.value() + adjustment.page_size() {
        adjustment.set_value((bottom - adjustment.page_size()).max(0.0));
    } else if top < adjustment.value() {
        adjustment.set_value(top);
    }
}

fn refresh_controls() {
    let (running, at, len) = PLAYING.with(|playing| {
        let playing = playing.borrow();
        playing.as_ref().map_or((false, 0, 0), |p| (p.timer.is_some(), p.at, p.made.steps.len()))
    });
    CARD.with(|card| {
        let card = card.borrow();
        let Some(card) = card.as_ref() else { return };
        card.play.set_label(if running { "Pause" } else { "Play" });
        card.back.set_sensitive(at > 0);
        card.forward.set_sensitive(at < len);
    });
}

pub(super) fn presented() {
    let armed = ARMED.replace(0);
    if armed == 0 {
        return;
    }
    CARD.with(|card| {
        let card = card.borrow();
        let Some(card) = card.as_ref() else { return };
        let fader = card.fader.clone();
        let target = adw::CallbackAnimationTarget::new(glib::clone!(#[weak] fader, move |value| fader.set_opacity(value)));
        let fade = adw::TimedAnimation::new(&card.fader, 1.0, 0.0, FADE_MS, target);
        fade.set_easing(adw::Easing::EaseOutCubic);
        fade.connect_done(glib::clone!(#[weak] fader, move |_| fader.set_visible(false)));
        fade.play();
    });
}

pub(super) fn close(state: &App) {
    if reset() {
        request_render(state);
    }
}

pub(super) fn reset() -> bool {
    stop_timer();
    let was = PLAYING.with(|playing| playing.borrow_mut().take().is_some());
    ARMED.set(0);
    CARD.with(|card| {
        if let Some(card) = card.borrow().as_ref() {
            card.root.set_visible(false);
            card.fader.set_visible(false);
        }
    });
    was
}
