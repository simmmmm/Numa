use super::*;

pub(super) const LOUPE_EDGE: u32 = 1920;

pub(super) fn build_loupe(state: &App) -> gtk::Revealer {
    let loupe = state.loupe.root.clone();
    loupe.add_css_class("loupe");

    state.loupe.picture.set_vexpand(true);
    state.loupe.picture.set_hexpand(true);
    state.loupe.picture.set_can_shrink(true);

    state.loupe.picture.set_content_fit(gtk::ContentFit::Contain);
    loupe.append(&state.loupe.picture);

    state.loupe.caption.add_css_class("loupe-caption");
    state.loupe.caption.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
    state.loupe.caption.set_halign(gtk::Align::Center);
    loupe.append(&state.loupe.caption);
    loupe.append(&build_loupe_bar(state));

    let click = gtk::GestureClick::new();
    click.connect_released(glib::clone!(
        #[strong] state,
        move |_, _, _, _| close_loupe(&state)
    ));
    state.loupe.picture.add_controller(click);

    let reveal = state.loupe.reveal.clone();
    reveal.set_transition_type(gtk::RevealerTransitionType::Crossfade);
    reveal.set_transition_duration(160);
    reveal.set_child(Some(&loupe));

    reveal.set_visible(false);
    reveal.connect_child_revealed_notify(glib::clone!(
        #[strong] state,
        move |reveal| {
            if !reveal.reveals_child() {
                reveal.set_visible(false);
                state.loupe.picture.set_paintable(gtk::gdk::Paintable::NONE);
            }
        }
    ));
    reveal
}

pub(super) fn build_loupe_bar(state: &App) -> gtk::Box {
    let bar = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    bar.set_halign(gtk::Align::Center);
    bar.set_margin_bottom(6);
    bar.add_css_class("loupe-bar");

    let step = |state: &App, icon: &str, hint: &str, forward: bool| {
        let button = gtk::Button::from_icon_name(icon);
        button.add_css_class("flat");
        button.set_tooltip_text(Some(hint));
        button.connect_clicked(glib::clone!(
            #[strong] state,
            move |_| step_loupe(&state, forward)
        ));
        button
    };
    bar.append(&step(state, "go-previous-symbolic", "Previous photograph (Left)", false));

    let stars = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    stars.add_css_class("linked");
    stars.set_margin_start(6);
    stars.set_margin_end(6);
    for (index, star) in state.loupe.stars.iter().enumerate() {
        let value = index as u8 + 1;
        star.set_label("\u{2606}");
        star.add_css_class("flat");
        star.add_css_class("loupe-star");
        star.set_tooltip_text(Some(&format!("{value} \u{2605} ({value}) \u{2014} again to clear")));
        star.connect_clicked(glib::clone!(
            #[strong] state,
            move |_| rate_in_loupe(&state, value)
        ));
        stars.append(star);
    }
    bar.append(&stars);

    state.loupe.pick.set_label("\u{2691}");
    state.loupe.pick.add_css_class("flat");
    state.loupe.pick.set_tooltip_text(Some("Pick (P) \u{2014} again to clear"));
    state.loupe.pick.connect_clicked(glib::clone!(
        #[strong] state,
        move |_| flag_in_loupe(&state, Flag::Picked)
    ));
    bar.append(&state.loupe.pick);

    state.loupe.reject.set_label("\u{2715}");
    state.loupe.reject.add_css_class("flat");
    state.loupe.reject.set_tooltip_text(Some("Reject (X) \u{2014} again to clear"));
    state.loupe.reject.connect_clicked(glib::clone!(
        #[strong] state,
        move |_| flag_in_loupe(&state, Flag::Rejected)
    ));
    bar.append(&state.loupe.reject);

    let edit = gtk::Button::with_label("Edit");
    edit.add_css_class("flat");
    edit.set_margin_start(6);
    edit.set_tooltip_text(Some("Open this photograph in the editor (Enter)"));
    edit.connect_clicked(glib::clone!(
        #[strong] state,
        move |_| {
            let card = selected_cards(&state).first().cloned();
            close_loupe(&state);
            if let Some(card) = card {
                open_in_editor(&state, &card);
            }
        }
    ));
    bar.append(&edit);

    let close = gtk::Button::from_icon_name("view-grid-symbolic");
    close.add_css_class("flat");
    close.set_tooltip_text(Some("Back to the grid (Space or Escape)"));
    close.connect_clicked(glib::clone!(
        #[strong] state,
        move |_| close_loupe(&state)
    ));
    bar.append(&close);

    bar.append(&step(state, "go-next-symbolic", "Next photograph (Right)", true));
    bar
}

pub(super) fn rate_in_loupe(state: &App, value: u8) {
    let now = loupe_rating(state).0;
    apply_to_selection(state, Action::Rate(if now == value { 0 } else { value }));
    refresh_loupe_bar(state);
}

pub(super) fn flag_in_loupe(state: &App, flag: Flag) {
    let now = loupe_rating(state).1;
    apply_to_selection(state, Action::Flag(if now == flag { Flag::None } else { flag }));
    refresh_loupe_bar(state);
}

pub(super) fn loupe_rating(state: &App) -> (u8, Flag) {
    let Some(at) = state.loupe.at.get() else { return (0, Flag::None) };
    let id = state
        .grid.lazy
        .borrow()
        .get(at)
        .and_then(|card| card.widget.widget_name().parse::<i64>().ok());
    let Some(id) = id else { return (0, Flag::None) };
    let cards = state.grid.cards.borrow();
    let Some((_, badge)) = cards.get(&id) else { return (0, Flag::None) };
    let text = badge.text();
    (rating_from_badge(&text), flag_from_badge(&text))
}

pub(super) fn show_in_loupe(state: &App, id: i64) -> bool {
    let card = state
        .grid.lazy
        .borrow()
        .iter()
        .position(|card| card.widget.widget_name() == id.to_string())
        .map(|at| (at, state.grid.lazy.borrow()[at].widget.clone()));
    let Some((at, card)) = card else { return false };
    state.grid.wall.select_only(&card);
    state.grid.wall.reveal(&card);
    show_loupe(state, at);
    true
}

pub(super) fn open_loupe(state: &App) {
    let selected = selected_cards(state);
    let Some(first) = selected.first() else {
        state.toast("Select a photo first");
        return;
    };
    let at = state.grid.lazy.borrow().iter().position(|card| card.widget == *first);
    if let Some(at) = at {
        show_loupe(state, at);
    }
}

pub(super) fn show_loupe(state: &App, at: usize) {
    let Some((path, mtime)) = state
        .grid.lazy
        .borrow()
        .get(at)
        .map(|card| (card.path.clone(), card.mtime))
    else {
        return;
    };

    state.loupe.at.set(Some(at));
    state.loupe.reveal.set_visible(true);
    state.loupe.reveal.set_reveal_child(true);

    state.loupe.picture.set_paintable(gtk::gdk::Paintable::NONE);
    refresh_loupe_bar(state);

    let loupe = state.loupe.picture.clone();
    let at_open = state.loupe.at.clone();

    thumbnail::load_thumbnail(&path, mtime, LOUPE_EDGE, None, move |texture| {

        if at_open.get() == Some(at) {
            loupe.set_paintable(Some(&texture));
        }
    });
}

pub(super) fn loupe_key(state: &App, key: gtk::gdk::Key) -> glib::Propagation {
    if state.loupe.at.get().is_none() {
        return glib::Propagation::Proceed;
    }
    match key {
        gtk::gdk::Key::space | gtk::gdk::Key::Escape => close_loupe(state),
        gtk::gdk::Key::Left | gtk::gdk::Key::Page_Up => step_loupe(state, false),
        gtk::gdk::Key::Right | gtk::gdk::Key::Page_Down => step_loupe(state, true),
        gtk::gdk::Key::Return | gtk::gdk::Key::KP_Enter => {
            let card = selected_cards(state).first().cloned();
            close_loupe(state);
            if let Some(card) = card {
                open_in_editor(state, &card);
            }
        }
        _ => return glib::Propagation::Proceed,
    }
    glib::Propagation::Stop
}

pub(super) fn close_loupe(state: &App) {
    state.loupe.at.set(None);

    state.loupe.reveal.set_reveal_child(false);
}

pub(super) fn step_loupe(state: &App, forward: bool) {
    let Some(at) = state.loupe.at.get() else { return };
    let count = state.grid.lazy.borrow().len();
    let next = match forward {
        true => (at + 1).min(count.saturating_sub(1)),
        false => at.saturating_sub(1),
    };
    if next == at {
        return;
    }

    let card = state.grid.lazy.borrow().get(next).map(|card| card.widget.clone());
    if let Some(card) = card {
        state.grid.wall.select_only(&card);
        state.grid.wall.reveal(&card);
    }
    show_loupe(state, next);
}

pub(super) fn refresh_loupe_bar(state: &App) {
    let Some(at) = state.loupe.at.get() else { return };
    let id = state.grid.lazy.borrow().get(at).and_then(|card| card.widget.widget_name().parse::<i64>().ok());
    let Some(id) = id else { return };
    let name = {
        let cards = state.grid.cards.borrow();
        let Some((photo, _)) = cards.get(&id) else { return };
        photo.path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default()
    };
    let (rating, flag) = loupe_rating(state);

    state.loupe.caption.set_text(&name);

    for (index, star) in state.loupe.stars.iter().enumerate() {
        let filled = index as u8 + 1 <= rating;
        star.set_label(if filled { "\u{2605}" } else { "\u{2606}" });
        match filled {
            true => star.add_css_class("rated"),
            false => star.remove_css_class("rated"),
        }
    }
    for (button, on, class) in [
        (&state.loupe.pick, flag == Flag::Picked, "rated"),
        (&state.loupe.reject, flag == Flag::Rejected, "rejected"),
    ] {
        match on {
            true => button.add_css_class(class),
            false => button.remove_css_class(class),
        }
    }
}

#[derive(Clone)]
pub(super) struct State {

    pub(super) root: gtk::Box,

    pub(super) reveal: gtk::Revealer,
    pub(super) picture: gtk::Picture,
    pub(super) caption: gtk::Label,

    pub(super) stars: Rc<Vec<gtk::Button>>,
    pub(super) pick: gtk::Button,
    pub(super) reject: gtk::Button,

    pub(super) at: Rc<Cell<Option<usize>>>,
}

impl State {
    pub(super) fn new() -> Self {
        Self {
            root: gtk::Box::new(gtk::Orientation::Vertical, 6),
            reveal: gtk::Revealer::new(),
            picture: gtk::Picture::new(),
            caption: gtk::Label::new(None),
            stars: Rc::new((1..=5).map(|_| gtk::Button::new()).collect()),
            pick: gtk::Button::new(),
            reject: gtk::Button::new(),
            at: Rc::new(Cell::new(None)),
        }
    }
}
