use super::*;

pub(super) const LOUPE_EDGE: u32 = 1920;

const NEIGHBOUR_OPACITY: f64 = 0.35;

const NEIGHBOUR_MIN: i32 = 24;

const NEIGHBOUR_GAP: i32 = 12;

const SLIDE_MS: u32 = 180;

const PICK_MS: u32 = 340;

const HOP: f64 = 0.35;
const HOP_HEIGHT: f64 = 0.04;

const NEIGHBOURS: &str = "loupe_neighbours";

const AF_POINT: f64 = 0.06;
const AF_ZONE: f64 = 0.2;

pub(super) fn build_loupe(state: &App) -> gtk::Revealer {
    let loupe = state.loupe.root.clone();
    loupe.add_css_class("loupe");

    state.loupe.picture.set_vexpand(true);
    state.loupe.picture.set_hexpand(true);
    state.loupe.picture.set_can_shrink(true);

    state.loupe.picture.set_content_fit(gtk::ContentFit::Contain);
    loupe.append(&build_stage(state));

    state.loupe.caption.add_css_class("loupe-caption");
    state.loupe.caption.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
    state.loupe.caption.set_halign(gtk::Align::Center);
    loupe.append(&state.loupe.caption);
    state.loupe.burst.set_halign(gtk::Align::Center);
    state.loupe.burst.set_visible(false);
    loupe.append(&state.loupe.burst);
    loupe.append(&build_loupe_bar(state));

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
                for picture in [&state.loupe.picture, &state.loupe.previous, &state.loupe.next] {
                    picture.set_paintable(gtk::gdk::Paintable::NONE);
                }
            }
        }
    ));
    reveal
}

fn build_stage(state: &App) -> gtk::Overlay {
    let stage = state.loupe.stage.clone();
    stage.set_vexpand(true);
    stage.set_hexpand(true);

    stage.set_overflow(gtk::Overflow::Hidden);
    stage.set_child(Some(&loupe_zoom::build(state)));

    let shown = state.catalog.setting(NEIGHBOURS).as_deref() != Some("off");
    state.loupe.neighbours.set(shown);
    for (side, forward) in [(&state.loupe.previous, false), (&state.loupe.next, true)] {
        side.set_content_fit(gtk::ContentFit::Cover);
        side.set_can_shrink(true);
        side.set_opacity(NEIGHBOUR_OPACITY);
        side.set_visible(shown);

        let click = gtk::GestureClick::new();
        click.connect_released(glib::clone!(
            #[strong] state,
            move |_, _, _, _| step_loupe(&state, forward)
        ));
        side.add_controller(click);
        stage.add_overlay(side);
    }

    let marker = &state.loupe.marker;
    marker.add_css_class("accent");
    marker.set_can_target(false);
    marker.set_draw_func(|area, cr, width, height| {
        let (width, height) = (width as f64, height as f64);
        let accent = area.color();
        cr.rectangle(1.5, 1.5, width - 3.0, height - 3.0);
        cr.set_source_rgba(0.0, 0.0, 0.0, 0.45);
        cr.set_line_width(3.0);
        let _ = cr.stroke_preserve();
        cr.set_source_rgba(accent.red() as f64, accent.green() as f64, accent.blue() as f64, 1.0);
        cr.set_line_width(1.5);
        let _ = cr.stroke();
    });
    stage.add_overlay(marker);

    let leaving = &state.loupe.leaving;
    leaving.set_content_fit(gtk::ContentFit::Contain);
    leaving.set_can_shrink(true);
    leaving.set_can_target(false);
    leaving.set_visible(false);
    stage.add_overlay(leaving);

    let framed = &state.loupe.picked_frame;
    framed.add_css_class("accent");
    framed.set_can_target(false);
    framed.set_draw_func(glib::clone!(
        #[strong] state,
        move |area, cr, _, _| draw_picked(&state, area, cr)
    ));
    stage.add_overlay(framed);

    stage.connect_get_child_position(glib::clone!(
        #[strong] state,
        move |stage, child| place(&state, stage, child)
    ));
    stage
}

fn place(state: &App, stage: &gtk::Overlay, child: &gtk::Widget) -> Option<gtk::gdk::Rectangle> {
    let (width, height) = (stage.width(), stage.height());
    if child == state.loupe.marker.upcast_ref::<gtk::Widget>() {
        let nothing = gtk::gdk::Rectangle::new(0, 0, 0, 0);
        let Some((x, y, side)) = af_box(state).filter(|_| state.loupe.neighbours.get()) else { return Some(nothing) };
        let half = side / 2.0;
        return Some(gtk::gdk::Rectangle::new((x - half).round() as i32, (y - half).round() as i32, side.round() as i32, side.round() as i32));
    }
    if child == state.loupe.picked_frame.upcast_ref::<gtk::Widget>() {
        return Some(gtk::gdk::Rectangle::new(0, 0, width, height));
    }
    if child == state.loupe.leaving.upcast_ref::<gtk::Widget>() && state.loupe.leaving_pick.get() {
        let (x, y, w, h) = picked_leaving(state, width, height);
        return Some(gtk::gdk::Rectangle::new(x.round() as i32, y.round() as i32, w.round() as i32, h.round() as i32));
    }
    if child != state.loupe.leaving.upcast_ref::<gtk::Widget>() && loupe_zoom::zoomed(state) {

        return Some(gtk::gdk::Rectangle::new(0, 0, 0, 0));
    }
    if child == state.loupe.leaving.upcast_ref::<gtk::Widget>() {
        let offset = (state.loupe.slide.get() * height as f64).round() as i32;
        return Some(gtk::gdk::Rectangle::new(0, offset, width, height));
    }
    let previous = child == state.loupe.previous.upcast_ref::<gtk::Widget>();
    Some(side_rect(state, width, height, previous).unwrap_or(gtk::gdk::Rectangle::new(0, 0, 0, 0)))
}

fn side_rect(state: &App, width: i32, height: i32, previous: bool) -> Option<gtk::gdk::Rectangle> {
    let aspect = state
        .loupe
        .picture
        .paintable()
        .map(|paintable| paintable.intrinsic_aspect_ratio())
        .filter(|aspect| *aspect > 0.0);
    let shown = aspect.map_or(width, |aspect| ((height as f64 * aspect).round() as i32).min(width));
    let side = (width - shown) / 2 - NEIGHBOUR_GAP;
    if side < NEIGHBOUR_MIN {
        return None;
    }
    let x = if previous { 0 } else { width - side };
    Some(gtk::gdk::Rectangle::new(x, 0, side, height))
}

fn picked_leaving(state: &App, width: i32, height: i32) -> (f64, f64, f64, f64) {
    let t = state.loupe.slide.get();
    let (w, h) = (width as f64, height as f64);
    if t < HOP {
        let lift = (std::f64::consts::PI * t / HOP).sin() * HOP_HEIGHT * h;
        return (0.0, -lift, w, h);
    }
    let s = (t - HOP) / (1.0 - HOP);

    let s = s * s * (3.0 - 2.0 * s);
    let to = match side_rect(state, width, height, true).filter(|_| state.loupe.neighbours.get()) {
        Some(slot) => (slot.x() as f64, slot.y() as f64, slot.width() as f64, slot.height() as f64),
        None => (-w, 0.0, w, h),
    };
    (to.0 * s, to.1 * s, w + (to.2 - w) * s, h + (to.3 - h) * s)
}

fn contained(aspect: f64, (x, y, w, h): (f64, f64, f64, f64)) -> (f64, f64, f64, f64) {
    if aspect <= 0.0 || w <= 0.0 || h <= 0.0 {
        return (x, y, w, h);
    }
    let (fw, fh) = if w / h > aspect { (h * aspect, h) } else { (w, w / aspect) };
    (x + (w - fw) / 2.0, y + (h - fh) / 2.0, fw, fh)
}

fn draw_picked(state: &App, area: &gtk::DrawingArea, cr: &gtk::cairo::Context) {
    let Some(at) = state.loupe.at.get() else { return };
    let (width, height) = (area.width(), area.height());
    let picked = |at: Option<usize>| at.is_some_and(|at| flag_at(state, at) == Flag::Picked);
    let mut boxes = Vec::new();

    if picked(Some(at)) && !loupe_zoom::zoomed(state) {
        if let Some((x, y, w, h)) = loupe_zoom::on_stage(state, 0.0, 0.0) {
            boxes.push((x, y, w, h));
        }
    }
    if state.loupe.neighbours.get() && !loupe_zoom::zoomed(state) {
        for (previous, near) in [(true, at.checked_sub(1)), (false, Some(at + 1))] {

            let arriving = previous && state.loupe.leaving_pick.get() && state.loupe.leaving.is_visible();
            if picked(near) && !arriving {
                if let Some(slot) = side_rect(state, width, height, previous) {
                    boxes.push((slot.x() as f64, slot.y() as f64, slot.width() as f64, slot.height() as f64));
                }
            }
        }
    }
    if state.loupe.leaving_pick.get() && state.loupe.leaving.is_visible() {
        let aspect = state.loupe.leaving.paintable().map_or(0.0, |frame| frame.intrinsic_aspect_ratio());
        boxes.push(contained(aspect, picked_leaving(state, width, height)));
    }

    let accent = area.color();
    for (x, y, w, h) in boxes {
        cr.rectangle(x + 2.0, y + 2.0, w - 4.0, h - 4.0);
        cr.set_source_rgba(0.0, 0.0, 0.0, 0.45);
        cr.set_line_width(6.0);
        let _ = cr.stroke_preserve();
        cr.set_source_rgba(accent.red() as f64, accent.green() as f64, accent.blue() as f64, 1.0);
        cr.set_line_width(3.0);
        let _ = cr.stroke();
    }
}

pub(super) fn af_here(state: &App) -> Option<raw::AfPoint> {
    let id = id_at(state, state.loupe.at.get()?)?;
    *state.loupe.af.borrow().get(&id)?
}

fn af_box(state: &App) -> Option<(f64, f64, f64)> {
    let point = af_here(state)?;
    let (x, y, width, height) = loupe_zoom::on_stage(state, point.x as f64, point.y as f64)?;
    let side = width.max(height) * if point.zone { AF_ZONE } else { AF_POINT };
    Some((x, y, side))
}

pub(super) fn af_under(state: &App, x: f64, y: f64) -> Option<raw::AfPoint> {
    let (bx, by, side) = af_box(state).filter(|_| state.loupe.neighbours.get())?;
    ((x - bx).abs() <= side / 2.0 && (y - by).abs() <= side / 2.0).then(|| af_here(state)).flatten()
}

fn read_af(state: &App, at: usize) {
    let Some(id) = id_at(state, at) else { return };
    if state.loupe.af.borrow().contains_key(&id) {
        state.loupe.stage.queue_allocate();
        return;
    }
    let Some(path) = state.grid.lazy.borrow().get(at).map(|card| card.path.clone()) else { return };
    let state = state.clone();
    glib::spawn_future_local(async move {
        let point = gio::spawn_blocking(move || raw::af_point(&path)).await.ok().flatten();
        state.loupe.af.borrow_mut().insert(id, point);
        state.loupe.stage.queue_allocate();
    });
}

fn toggle_neighbours(state: &App) {
    let shown = !state.loupe.neighbours.get();
    state.loupe.neighbours.set(shown);
    state.loupe.previous.set_visible(shown);
    state.loupe.next.set_visible(shown);
    state.loupe.stage.queue_allocate();
    let _ = state.catalog.set_setting(NEIGHBOURS, if shown { "on" } else { "off" });
}

fn slide_away(state: &App, frame: gtk::gdk::Paintable, up: bool) {

    if let Some(running) = state.loupe.animation.borrow_mut().take() {
        running.skip();
    }
    let leaving = state.loupe.leaving.clone();
    leaving.set_paintable(Some(&frame));
    leaving.set_opacity(1.0);
    leaving.set_visible(true);

    state.loupe.leaving_pick.set(up);

    let target = adw::CallbackAnimationTarget::new(glib::clone!(
        #[strong] state,
        move |value| {
            state.loupe.slide.set(value);
            let opacity = match state.loupe.leaving_pick.get() {

                true => 1.0 - (1.0 - NEIGHBOUR_OPACITY) * ((value - HOP) / (1.0 - HOP)).clamp(0.0, 1.0),
                false => 1.0 - value.abs(),
            };
            state.loupe.leaving.set_opacity(opacity);
            state.loupe.stage.queue_allocate();
            state.loupe.picked_frame.queue_draw();
        }
    ));
    let duration = if up { PICK_MS } else { SLIDE_MS };
    let animation = adw::TimedAnimation::new(&state.loupe.stage, 0.0, 1.0, duration, target);
    animation.set_easing(if up { adw::Easing::Linear } else { adw::Easing::EaseInCubic });
    animation.connect_done(glib::clone!(
        #[strong] state,
        move |_| {
            state.loupe.leaving.set_visible(false);
            state.loupe.leaving.set_paintable(gtk::gdk::Paintable::NONE);
            state.loupe.slide.set(0.0);
            state.loupe.leaving_pick.set(false);
            state.loupe.stage.queue_allocate();
            state.loupe.picked_frame.queue_draw();
        }
    ));
    animation.play();
    *state.loupe.animation.borrow_mut() = Some(animation);
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
    state.loupe.pick.set_tooltip_text(Some("Pick (P) \u{2014} again to clear \u{00b7} Up picks and moves on"));
    state.loupe.pick.connect_clicked(glib::clone!(
        #[strong] state,
        move |_| flag_in_loupe(&state, Flag::Picked)
    ));
    bar.append(&state.loupe.pick);

    state.loupe.reject.set_label("\u{2715}");
    state.loupe.reject.add_css_class("flat");
    state.loupe.reject.set_tooltip_text(Some("Reject (X) \u{2014} again to clear \u{00b7} Down rejects and moves on"));
    state.loupe.reject.connect_clicked(glib::clone!(
        #[strong] state,
        move |_| flag_in_loupe(&state, Flag::Rejected)
    ));
    bar.append(&state.loupe.reject);
    bar.append(&build_target(state));

    let edit = gtk::Button::with_label("Edit");
    edit.add_css_class("flat");
    edit.set_margin_start(6);
    edit.set_tooltip_text(Some("Open this photograph in the editor (Enter)"));
    edit.connect_clicked(glib::clone!(
        #[strong] state,
        move |_| {

            if let Some(card) = selected_cards(&state).first() {
                open_in_editor(&state, card);
            }
            close_loupe(&state);
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

fn build_target(state: &App) -> gtk::MenuButton {
    let spin = gtk::SpinButton::with_range(0.0, 9999.0, 1.0);
    spin.set_numeric(true);
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    row.append(&gtk::Label::new(Some("Aim for")));
    row.append(&spin);
    row.append(&gtk::Label::new(Some("picks")));
    let hint = gtk::Label::new(Some("For this library \u{2014} 0 for no target"));
    hint.add_css_class("dim-label");
    hint.add_css_class("caption");
    let content = gtk::Box::new(gtk::Orientation::Vertical, 6);
    content.append(&row);
    content.append(&hint);
    let popover = gtk::Popover::new();
    popover.set_child(Some(&content));

    popover.connect_show(glib::clone!(
        #[strong] state,
        #[strong] spin,
        move |_| {
            let target = loupe_library(&state).and_then(|library| state.catalog.cull_target(library));
            spin.set_value(target.unwrap_or(0) as f64);
        }
    ));
    spin.connect_value_changed(glib::clone!(
        #[strong] state,
        move |spin| {
            let Some(library) = loupe_library(&state) else { return };
            let target = spin.value() as u32;
            if let Err(err) = state.catalog.set_cull_target(library, (target > 0).then_some(target)) {
                state.toast(&format!("Could not save: {err}"));
            }
            refresh_loupe_bar(&state);
        }
    ));

    let button = state.loupe.target.clone();
    button.add_css_class("flat");
    button.set_popover(Some(&popover));
    button.set_tooltip_text(Some("Picked in this library \u{2014} press to set how many you are aiming for"));
    button
}

fn loupe_library(state: &App) -> Option<i64> {
    let id = id_at(state, state.loupe.at.get()?)?;
    Some(numa::io::catalog::library_of(id))
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
    marks_at(state, at)
}

fn marks_at(state: &App, at: usize) -> (u8, Flag) {
    let Some(id) = id_at(state, at) else { return (0, Flag::None) };
    let cards = state.grid.cards.borrow();
    let Some((_, badge)) = cards.get(&id) else { return (0, Flag::None) };
    let text = badge.text();
    (rating_from_badge(&text), flag_from_badge(&text))
}

fn flag_at(state: &App, at: usize) -> Flag {
    marks_at(state, at).1
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
    if at >= state.grid.lazy.borrow().len() {
        return;
    }

    let arriving = id_at(state, at);
    if state.loupe.visit.get().is_some_and(|(id, _, _)| Some(id) != arriving) {
        leave_visit(state);
    }
    if state.loupe.visit.get().is_none() {
        state.loupe.visit.set(arriving.map(|id| (id, std::time::Instant::now(), false)));
    }

    let centre = loupe_zoom::centre(state);
    state.loupe.at.set(Some(at));
    state.loupe.reveal.set_visible(true);
    state.loupe.reveal.set_reveal_child(true);

    let near: Vec<i64> = [at.checked_sub(1), Some(at), Some(at + 1), Some(at + 2)]
        .into_iter()
        .flatten()
        .filter_map(|index| id_at(state, index))
        .collect();
    state.loupe.textures.borrow_mut().retain(|id, _| near.contains(id));

    paint(state);
    refresh_loupe_bar(state);

    for index in [Some(at), Some(at + 1), Some(at + 2), at.checked_sub(1)].into_iter().flatten() {
        hold(state, index);
    }
    loupe_zoom::stepped(state, centre);
    read_af(state, at);
}

fn hold(state: &App, index: usize) {
    let Some((id, path, mtime)) = state.grid.lazy.borrow().get(index).and_then(|card| {
        Some((card.widget.widget_name().parse::<i64>().ok()?, card.path.clone(), card.mtime))
    }) else {
        return;
    };
    if state.loupe.textures.borrow().contains_key(&id) {
        return;
    }

    let near = move |state: &App| state.loupe.at.get().is_some_and(|at| index + 1 >= at && index <= at + 2);
    thumbnail::load_thumbnail_first(
        &path,
        mtime,
        LOUPE_EDGE,
        glib::clone!(
            #[strong] state,
            move || near(&state)
        ),
        glib::clone!(
            #[strong] state,
            move |texture| {
                if near(&state) {
                    state.loupe.textures.borrow_mut().insert(id, texture);
                    paint(&state);
                }
            }
        ),
    );
}

pub(super) fn paint(state: &App) {
    let Some(at) = state.loupe.at.get() else { return };
    let textures = state.loupe.textures.borrow();
    let texture = |index: Option<usize>| index.and_then(|index| id_at(state, index)).and_then(|id| textures.get(&id).cloned());
    for (picture, index) in [
        (&state.loupe.picture, Some(at)),
        (&state.loupe.previous, at.checked_sub(1)),
        (&state.loupe.next, Some(at + 1)),
    ] {

        let full = (index == Some(at)).then(|| loupe_zoom::full_for(state, at)).flatten();
        let wanted = full.or_else(|| texture(index));
        let now = picture.paintable().and_then(|paintable| paintable.downcast::<gtk::gdk::Texture>().ok());
        if now != wanted {
            picture.set_paintable(wanted.as_ref());
        }
    }

    state.loupe.stage.queue_allocate();
}

pub(super) fn loupe_key(state: &App, key: gtk::gdk::Key, modifiers: gtk::gdk::ModifierType) -> glib::Propagation {
    if state.loupe.at.get().is_none() {
        return glib::Propagation::Proceed;
    }

    let typing = state
        .loupe
        .root
        .root()
        .and_then(|root| root.focus())
        .is_some_and(|focus| focus.is::<gtk::Editable>() || focus.is::<gtk::Text>());
    if typing {
        return glib::Propagation::Proceed;
    }
    let ctrl = modifiers.contains(gtk::gdk::ModifierType::CONTROL_MASK);
    let shift = modifiers.contains(gtk::gdk::ModifierType::SHIFT_MASK);
    match key {
        gtk::gdk::Key::Left if ctrl => step_burst(state, false),
        gtk::gdk::Key::Right if ctrl => step_burst(state, true),
        gtk::gdk::Key::Down if shift => reject_rest(state),
        gtk::gdk::Key::space | gtk::gdk::Key::Escape => close_loupe(state),
        gtk::gdk::Key::Left | gtk::gdk::Key::Page_Up => step_loupe(state, false),
        gtk::gdk::Key::Right | gtk::gdk::Key::Page_Down => step_loupe(state, true),
        gtk::gdk::Key::Return | gtk::gdk::Key::KP_Enter => {

            if let Some(card) = selected_cards(state).first() {
                open_in_editor(state, card);
            }
            close_loupe(state);
        }
        gtk::gdk::Key::Up => cull(state, Flag::Picked),
        gtk::gdk::Key::Down => cull(state, Flag::Rejected),
        gtk::gdk::Key::BackSpace => undo_mark(state),
        gtk::gdk::Key::f | gtk::gdk::Key::F => toggle_neighbours(state),

        gtk::gdk::Key::Home
        | gtk::gdk::Key::End
        | gtk::gdk::Key::a
        | gtk::gdk::Key::A
        | gtk::gdk::Key::Delete
        | gtk::gdk::Key::KP_Delete => {}
        _ => return glib::Propagation::Proceed,
    }
    glib::Propagation::Stop
}

pub(super) fn close_loupe(state: &App) {
    leave_visit(state);
    let was_open = state.loupe.at.take().is_some();
    state.loupe.textures.borrow_mut().clear();
    loupe_zoom::reset(state);

    state.loupe.undo.borrow_mut().clear();

    state.loupe.reveal.set_reveal_child(false);

    let back_to_grid = state.stack.visible_child_name().as_deref() == Some("library");
    if was_open && back_to_grid && state.grid.stale.replace(false) {
        reload_grid(state);
    }
}

fn cull(state: &App, flag: Flag) {
    let Some(at) = state.loupe.at.get() else { return };
    apply_to_selection(state, Action::Flag(flag));
    refresh_loupe_bar(state);

    if at + 1 >= state.grid.lazy.borrow().len() {
        state.toast("Last photograph — Space goes back to the grid");
        return;
    }

    let frame = state.loupe.picture.paintable().filter(|_| !loupe_zoom::zoomed(state));
    step_loupe(state, true);
    if let Some(frame) = frame {
        slide_away(state, frame, flag == Flag::Picked);
    }
}

fn undo_mark(state: &App) {
    let Some(before) = state.loupe.undo.borrow_mut().pop() else {
        state.toast("Nothing to undo");
        return;
    };
    let depth = state.loupe.undo.borrow().len();

    state.loupe.restoring.set(true);
    for &(id, rating, flag) in &before {
        apply_to_ids(state, &[id], Action::Rate(rating));
        apply_to_ids(state, &[id], Action::Flag(flag));
        let _ = state.catalog.log_decision(id, "undo", None);
    }
    state.loupe.restoring.set(false);

    state.loupe.undo.borrow_mut().truncate(depth);
    if let Some(&(id, _, _)) = before.first() {
        show_in_loupe(state, id);
    }
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
    go_to(state, next);
}

fn go_to(state: &App, index: usize) {
    let card = state.grid.lazy.borrow().get(index).map(|card| card.widget.clone());
    if let Some(card) = card {
        state.grid.wall.select_only(&card);
        state.grid.wall.reveal(&card);
    }
    show_loupe(state, index);
}

pub(super) fn id_at(state: &App, at: usize) -> Option<i64> {
    state.grid.lazy.borrow().get(at).and_then(|card| card.widget.widget_name().parse().ok())
}

fn burst_at(state: &App, at: usize) -> Option<(i64, i64)> {
    let id = id_at(state, at)?;
    let burst = state.grid.cards.borrow().get(&id)?.0.burst?;
    Some((numa::io::catalog::library_of(id), burst))
}

fn burst_run(state: &App, at: usize) -> std::ops::Range<usize> {
    let Some(burst) = burst_at(state, at) else { return at..at + 1 };
    let count = state.grid.lazy.borrow().len();
    let same = |index: &usize| burst_at(state, *index) == Some(burst);
    let start = (0..at).rev().take_while(same).last().unwrap_or(at);
    let end = (at + 1..count).take_while(same).last().map_or(at + 1, |last| last + 1);
    start..end
}

fn best_in(state: &App, run: std::ops::Range<usize>) -> Option<usize> {
    let cards = state.grid.cards.borrow();
    run.into_iter()
        .find(|index| id_at(state, *index).and_then(|id| cards.get(&id)).is_some_and(|(photo, _)| photo.best_of_burst))
}

pub(super) fn note_mark(state: &App, ids: &[i64], action: Action) {
    if state.loupe.at.get().is_none() || state.loupe.restoring.get() {
        return;
    }
    let what = match action {
        Action::Rate(stars) => format!("rate {stars}"),
        Action::Flag(Flag::Picked) => "pick".to_string(),
        Action::Flag(Flag::Rejected) => "reject".to_string(),
        Action::Flag(Flag::None) => "clear".to_string(),
    };
    for id in ids {
        let _ = state.catalog.log_decision(*id, &what, None);
    }
    if let Some((id, since, _)) = state.loupe.visit.get().filter(|(id, _, _)| ids.contains(id)) {
        state.loupe.visit.set(Some((id, since, true)));
    }
}

fn leave_visit(state: &App) {
    if let Some((id, since, false)) = state.loupe.visit.take() {
        let _ = state.catalog.log_decision(id, "passed", Some(since.elapsed().as_millis() as i64));
    }
}

fn step_burst(state: &App, forward: bool) {
    let Some(at) = state.loupe.at.get() else { return };
    let here = burst_run(state, at);
    let next = match forward {
        true => Some(here.end).filter(|next| *next < state.grid.lazy.borrow().len()),
        false => here.start.checked_sub(1),
    };
    let Some(next) = next else {
        state.toast(match forward {
            true => "Last burst — Space goes back to the grid",
            false => "This is the first burst",
        });
        return;
    };
    let there = burst_run(state, next);
    go_to(state, best_in(state, there.clone()).unwrap_or(there.start));
}

fn reject_rest(state: &App) {
    let Some(at) = state.loupe.at.get() else { return };
    let run = burst_run(state, at);
    if run.len() < 2 {
        cull(state, Flag::Rejected);
        return;
    }
    let unmarked: Vec<i64> = {
        let cards = state.grid.cards.borrow();
        run.filter_map(|index| id_at(state, index))
            .filter(|id| cards.get(id).is_some_and(|(_, badge)| flag_from_badge(&badge.text()) == Flag::None))
            .collect()
    };

    if !unmarked.is_empty() {
        apply_to_ids(state, &unmarked, Action::Flag(Flag::Rejected));
    }
    refresh_loupe_bar(state);
    let (frame, before) =
        (state.loupe.picture.paintable().filter(|_| !loupe_zoom::zoomed(state)), state.loupe.at.get());
    step_burst(state, true);
    if let (Some(frame), true) = (frame, state.loupe.at.get() != before) {
        slide_away(state, frame, false);
    }
}

fn refresh_burst(state: &App, at: usize) {
    let run = burst_run(state, at);
    let echo = echo_note(state, at);
    state.loupe.burst.set_visible(run.len() > 1 || echo.is_some());
    if run.len() < 2 {
        state.loupe.burst.set_text(echo.as_deref().unwrap_or_default());
        return;
    }
    let place = at - run.start + 1;
    let cards = state.grid.cards.borrow();
    let mut pips = Vec::new();
    let mut best = None;
    for (n, index) in run.clone().enumerate() {
        let Some((photo, badge)) = id_at(state, index).and_then(|id| cards.get(&id)) else { continue };
        let glyph = match flag_from_badge(&badge.text()) {
            Flag::Picked => "\u{2691}",
            Flag::Rejected => "\u{2715}",
            Flag::None => "\u{25cb}",
        };
        if photo.best_of_burst {
            best = Some((n + 1, photo.face_sharpness.is_some()));
        }
        pips.push(match index == at {
            true => format!("<span size='x-large' weight='bold'>{glyph}</span>"),
            false => glyph.to_string(),
        });
    }
    let why = match best {
        Some((n, face)) if n == place => {
            format!(" \u{b7} best of the burst: {}", if face { "sharpest face" } else { "sharpest frame" })
        }
        Some((n, _)) => format!(" \u{b7} best is {n}"),
        None => String::new(),
    };
    let echo = echo.map_or(String::new(), |echo| format!(" \u{b7} {}", glib::markup_escape_text(&echo)));
    state.loupe.burst.set_markup(&format!("[ {} ]   {place} of {}{why}{echo}", pips.join(" "), run.len()));
}

fn echo_note(state: &App, at: usize) -> Option<String> {
    let cards = state.grid.cards.borrow();
    let (photo, _) = cards.get(&id_at(state, at)?)?;
    let (other, _) = cards.get(&photo.echo?)?;
    let when = |photo: &Photo| photo.taken.unwrap_or(photo.mtime);
    let apart = when(other) - when(photo);
    let minutes = apart.unsigned_abs() / 60;
    let span = match minutes {
        0..60 => format!("{minutes} min"),
        60..2880 => format!("{} h", minutes / 60),
        _ => format!("{} days", minutes / 1440),
    };
    let name = other.path.file_name()?.to_string_lossy().into_owned();
    Some(format!("same scene as {name}, {span} {}", if apart >= 0 { "later" } else { "earlier" }))
}

pub(super) fn refresh_loupe_bar(state: &App) {
    state.loupe.picked_frame.queue_draw();
    let Some(at) = state.loupe.at.get() else { return };
    let id = state.grid.lazy.borrow().get(at).and_then(|card| card.widget.widget_name().parse::<i64>().ok());
    let Some(id) = id else { return };
    let name = {
        let cards = state.grid.cards.borrow();
        let Some((photo, _)) = cards.get(&id) else { return };
        photo.path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default()
    };
    let (rating, flag) = loupe_rating(state);

    match loupe_zoom::sharpening(state) {
        true => state.loupe.caption.set_text(&format!("{name} \u{b7} developing at full size\u{2026}")),
        false => state.loupe.caption.set_text(&name),
    }
    refresh_burst(state, at);

    for (index, star) in state.loupe.stars.iter().enumerate() {
        let filled = index as u8 + 1 <= rating;
        star.set_label(if filled { "\u{2605}" } else { "\u{2606}" });
        match filled {
            true => star.add_css_class("rated"),
            false => star.remove_css_class("rated"),
        }
    }

    if let Some(library) = loupe_library(state) {
        let picks = state.catalog.picks(library).unwrap_or(0);
        let target = state.catalog.cull_target(library);
        let label = match target {
            Some(target) => format!("\u{2691} {picks} / {target}"),
            None => format!("\u{2691} {picks}"),
        };
        state.loupe.target.set_label(&label);
        match target.is_some_and(|target| picks > target as i64) {
            true => state.loupe.target.add_css_class("warning"),
            false => state.loupe.target.remove_css_class("warning"),
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

    pub(super) stage: gtk::Overlay,
    pub(super) picture: gtk::Picture,
    pub(super) previous: gtk::Picture,
    pub(super) next: gtk::Picture,
    pub(super) leaving: gtk::Picture,

    pub(super) neighbours: Rc<Cell<bool>>,

    pub(super) slide: Rc<Cell<f64>>,

    pub(super) leaving_pick: Rc<Cell<bool>>,

    pub(super) picked_frame: gtk::DrawingArea,
    pub(super) animation: Rc<RefCell<Option<adw::TimedAnimation>>>,

    pub(super) textures: Rc<RefCell<std::collections::HashMap<i64, gtk::gdk::Texture>>>,

    pub(super) zoom: loupe_zoom::State,

    pub(super) marker: gtk::DrawingArea,
    pub(super) af: Rc<RefCell<std::collections::HashMap<i64, Option<raw::AfPoint>>>>,
    pub(super) caption: gtk::Label,

    pub(super) burst: gtk::Label,

    pub(super) stars: Rc<Vec<gtk::Button>>,
    pub(super) pick: gtk::Button,
    pub(super) reject: gtk::Button,

    pub(super) target: gtk::MenuButton,

    pub(super) visit: Rc<Cell<Option<(i64, std::time::Instant, bool)>>>,
    pub(super) restoring: Rc<Cell<bool>>,

    pub(super) at: Rc<Cell<Option<usize>>>,

    pub(super) undo: Rc<RefCell<Vec<Vec<(i64, u8, Flag)>>>>,
}

impl State {
    pub(super) fn new() -> Self {
        Self {
            root: gtk::Box::new(gtk::Orientation::Vertical, 6),
            reveal: gtk::Revealer::new(),
            stage: gtk::Overlay::new(),
            picture: gtk::Picture::new(),
            previous: gtk::Picture::new(),
            next: gtk::Picture::new(),
            leaving: gtk::Picture::new(),
            neighbours: Rc::new(Cell::new(true)),
            slide: Rc::new(Cell::new(0.0)),
            leaving_pick: Rc::new(Cell::new(false)),
            picked_frame: gtk::DrawingArea::new(),
            animation: Rc::new(RefCell::new(None)),
            textures: Rc::new(RefCell::new(std::collections::HashMap::new())),
            zoom: loupe_zoom::State::new(),
            marker: gtk::DrawingArea::new(),
            af: Rc::new(RefCell::new(std::collections::HashMap::new())),
            caption: gtk::Label::new(None),
            burst: gtk::Label::new(None),
            stars: Rc::new((1..=5).map(|_| gtk::Button::new()).collect()),
            pick: gtk::Button::new(),
            reject: gtk::Button::new(),
            target: gtk::MenuButton::new(),
            visit: Rc::new(Cell::new(None)),
            restoring: Rc::new(Cell::new(false)),
            at: Rc::new(Cell::new(None)),
            undo: Rc::new(RefCell::new(Vec::new())),
        }
    }
}
