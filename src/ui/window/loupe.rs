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
    loupe.add_css_class("numa-content");

    state.loupe.picture.set_vexpand(true);
    state.loupe.picture.set_hexpand(true);
    state.loupe.picture.set_can_shrink(true);

    state.loupe.picture.set_content_fit(gtk::ContentFit::Contain);
    loupe.append(&build_stage(state));

    let whole = gtk::Box::new(gtk::Orientation::Vertical, 0);
    whole.append(&loupe);
    whole.append(&tape::build(state));

    let reveal = state.loupe.reveal.clone();
    reveal.set_transition_type(gtk::RevealerTransitionType::Crossfade);
    reveal.set_transition_duration(160);
    reveal.set_child(Some(&whole));

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

    let shown = state.catalog.setting(NEIGHBOURS).as_deref() == Some("on");
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

    stage.add_overlay(&loupe_group::build(state));
    stage.add_overlay(&loupe_group::build_pill(state));

    let marker = &state.loupe.marker;
    marker.add_css_class("photo-mark");
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
    framed.add_css_class("photo-mark");
    framed.set_can_target(false);
    framed.set_draw_func(glib::clone!(
        #[strong] state,
        move |area, cr, _, _| draw_picked(&state, area, cr)
    ));
    stage.add_overlay(framed);

    stage.add_overlay(&review::build_pill(state));

    stage.connect_get_child_position(glib::clone!(
        #[strong] state,
        move |stage, child| place(&state, stage, child)
    ));
    stage
}

fn place(state: &App, stage: &gtk::Overlay, child: &gtk::Widget) -> Option<gtk::gdk::Rectangle> {
    let (width, height) = (stage.width(), stage.height());
    if child == state.loupe.tape.review.pill.upcast_ref::<gtk::Widget>()
        || child.has_css_class("loupe-group")
        || child.has_css_class("photo-pill")
    {
        return None;
    }
    if child == state.loupe.marker.upcast_ref::<gtk::Widget>() {
        let nothing = gtk::gdk::Rectangle::new(0, 0, 0, 0);
        let Some((x, y, side)) = af_box(state).filter(|_| !loupe_group::active(state)) else { return Some(nothing) };
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

    if loupe_group::active(state) {
        return;
    }
    let (width, height) = (area.width(), area.height());
    let picked = |at: Option<usize>| at.is_some_and(|at| flag_at(state, at) == Flag::Picked);
    let mut boxes = Vec::new();

    if picked(Some(at)) && !loupe_zoom::zoomed(state) {
        if let Some((x, y, w, h)) = loupe_zoom::on_stage(state, 0.0, 0.0) {
            boxes.push((x, y, w, h));
        }
    }
    if state.loupe.neighbours.get() && !loupe_zoom::zoomed(state) {
        for (previous, near) in [(true, beside(state, at, -1)), (false, beside(state, at, 1))] {

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
    let (bx, by, side) = af_box(state).filter(|_| !loupe_group::active(state))?;
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

pub(super) fn build_caption(state: &App) -> gtk::Box {
    let caption = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    state.loupe.caption.add_css_class("loupe-caption");
    state.loupe.caption.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
    state.loupe.caption.set_xalign(0.0);
    caption.append(&state.loupe.caption);
    caption.append(&state.loupe.zoom.waiting.spinner);
    state.loupe.burst.set_ellipsize(gtk::pango::EllipsizeMode::End);
    state.loupe.burst.set_visible(false);
    caption.append(&state.loupe.burst);
    caption
}

pub(super) fn build_loupe_bar(state: &App) -> gtk::Box {
    let bar = gtk::Box::new(gtk::Orientation::Horizontal, 6);
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

    bar.append(&build_target(state));

    let edit = gtk::Button::with_label("Edit");
    edit.add_css_class("flat");
    edit.set_margin_start(6);
    edit.set_tooltip_text(Some("Open this photograph in the editor (Enter)"));
    edit.connect_clicked(glib::clone!(
        #[strong] state,
        move |_| {

            if let Some(id) = selected_ids(&state).first().copied() {
                open_photo(&state, id);
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

pub(super) fn build_loupe_rating(state: &App) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    row.add_css_class("bar-rating");
    row.set_margin_start(6);
    let stars = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    stars.update_property(&[gtk::accessible::Property::Label("Rating")]);
    for (index, star) in state.loupe.stars.iter().enumerate() {
        let value = index as u8 + 1;
        star.set_icon_name("non-starred-symbolic");
        star.add_css_class("flat");
        star.set_tooltip_text(Some(&match value {
            1 => "1 Star (1), again to clear".to_string(),
            n => format!("{n} Stars ({n}), again to clear"),
        }));
        star.connect_clicked(glib::clone!(
            #[strong] state,
            move |_| rate_in_loupe(&state, value)
        ));
        stars.append(star);
    }
    row.append(&stars);
    for (button, icon, tip, flag) in [
        (&state.loupe.pick, "emoji-flags-symbolic", "Pick, again to clear \u{00b7} P or Up picks and moves on", Flag::Picked),
        (&state.loupe.reject, "window-close-symbolic", "Reject, again to clear \u{00b7} X or Down rejects and moves on", Flag::Rejected),
    ] {
        button.set_icon_name(icon);
        button.add_css_class("flat");
        button.set_tooltip_text(Some(tip));
        button.connect_clicked(glib::clone!(
            #[strong] state,
            move |_| flag_in_loupe(&state, flag)
        ));
        row.append(button);
    }
    state.loupe.pick.set_margin_start(6);
    row
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
    review::stop(state);
    let now = loupe_rating(state).0;
    apply_to_selection(state, Action::Rate(if now == value { 0 } else { value }));
    refresh_loupe_bar(state);
}

pub(super) fn flag_in_loupe(state: &App, flag: Flag) {
    review::stop(state);
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
    state.grid.cards.borrow().get(&id).map_or((0, Flag::None), |photo| (photo.rating, photo.flag))
}

fn flag_at(state: &App, at: usize) -> Flag {
    marks_at(state, at).1
}

pub(super) fn show_in_loupe(state: &App, id: i64) -> bool {
    let Some(at) = state.grid.lazy.borrow().iter().position(|card| card.id == id) else { return false };
    state.grid.wall.select_only(at);
    state.grid.wall.reveal(at);
    show_loupe(state, at);
    true
}

pub(super) fn open_loupe(state: &App) {
    let Some(at) = state.grid.wall.selected().first().copied() else {
        state.toast("Select a photo first");
        return;
    };
    show_loupe(state, at);
}

pub(super) fn show_loupe(state: &App, at: usize) {
    if at >= state.grid.lazy.borrow().len() {
        return;
    }

    let arriving = id_at(state, at);
    if !state.loupe.tape.quiet.get() {
        if state.loupe.visit.get().is_some_and(|(id, _, _)| Some(id) != arriving) {
            leave_visit(state);
        }
        if state.loupe.visit.get().is_none() {
            state.loupe.visit.set(arriving.map(|id| (id, std::time::Instant::now(), false)));
        }
    }

    let centre = loupe_zoom::centre(state);
    state.loupe.at.set(Some(at));
    if !state.loupe.reveal.reveals_child() {
        tape::rebuild(state);
    }
    state.loupe.reveal.set_visible(true);
    state.loupe.reveal.set_reveal_child(true);

    let near: Vec<i64> = wanted(state).into_iter().filter_map(|index| id_at(state, index)).collect();
    state.loupe.textures.borrow_mut().retain(|id, _| near.contains(id));

    paint(state);
    refresh_loupe_bar(state);

    let own = [Some(at), beside(state, at, 1), beside(state, at, 2), beside(state, at, -1)].into_iter().flatten();
    for index in own.chain(tape::held(state).into_iter().rev()) {
        hold(state, index);
    }
    for index in loupe_group::wanted(state) {
        hold(state, index);
    }
    loupe_zoom::stepped(state, centre);
    read_af(state, at);
    tape::follow(state);
}

fn wanted(state: &App) -> Vec<usize> {
    let Some(at) = state.loupe.at.get() else { return Vec::new() };

    let mut wanted: Vec<usize> = [beside(state, at, -1), Some(at), beside(state, at, 1), beside(state, at, 2)].into_iter().flatten().collect();
    wanted.extend(tape::held(state));
    wanted.extend(loupe_group::wanted(state));
    wanted
}

fn hold(state: &App, index: usize) {
    let Some((id, path, mtime)) = state.grid.lazy.borrow().get(index).map(|card| (card.id, card.path.clone(), card.mtime))
    else {
        return;
    };

    if state.loupe.textures.borrow().contains_key(&id) || !state.loupe.asked.borrow_mut().insert(id) {
        return;
    }

    let near = move |state: &App| wanted(state).contains(&index);
    thumbnail::load_thumbnail_first(
        &path,
        mtime,
        LOUPE_EDGE,
        glib::clone!(
            #[strong] state,
            move || {
                let near = near(&state);
                if !near {
                    state.loupe.asked.borrow_mut().remove(&id);
                }
                near
            }
        ),
        glib::clone!(
            #[strong] state,
            move |texture| {
                state.loupe.asked.borrow_mut().remove(&id);
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
        (&state.loupe.previous, beside(state, at, -1)),
        (&state.loupe.next, beside(state, at, 1)),
    ] {

        let full = (index == Some(at)).then(|| loupe_zoom::full_for(state, at)).flatten();
        let wanted = full.or_else(|| texture(index));
        let now = picture.paintable().and_then(|paintable| paintable.downcast::<gtk::gdk::Texture>().ok());
        if now != wanted {
            picture.set_paintable(wanted.as_ref());
        }
    }
    loupe_group::paint(state);

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

    use gtk::gdk::Key;
    let held_down = matches!(
        key,
        Key::Shift_L | Key::Shift_R | Key::Control_L | Key::Control_R | Key::Alt_L | Key::Alt_R | Key::Super_L | Key::Super_R
    );
    let reviewing = review::running(state);
    if !held_down {
        review::stop(state);
        tape::settle(state);
    }
    match key {
        Key::space | Key::Escape if reviewing => return glib::Propagation::Stop,
        Key::space if shift => {
            review::toggle(state);
            return glib::Propagation::Stop;
        }
        _ if !reviewing && tape::range_key(state, key, modifiers) => return glib::Propagation::Stop,
        _ => {}
    }
    match key {
        gtk::gdk::Key::Left if ctrl => step_burst(state, false),
        gtk::gdk::Key::Right if ctrl => step_burst(state, true),
        gtk::gdk::Key::Down if shift => reject_rest(state),
        gtk::gdk::Key::space | gtk::gdk::Key::Escape => close_loupe(state),
        gtk::gdk::Key::Left | gtk::gdk::Key::Page_Up => step_loupe(state, false),
        gtk::gdk::Key::Right | gtk::gdk::Key::Page_Down => step_loupe(state, true),
        gtk::gdk::Key::Return | gtk::gdk::Key::KP_Enter => {

            if let Some(id) = selected_ids(state).first().copied() {
                open_photo(state, id);
            }
            close_loupe(state);
        }

        gtk::gdk::Key::Up | gtk::gdk::Key::p | gtk::gdk::Key::P if !ctrl => cull(state, Flag::Picked),
        gtk::gdk::Key::Down | gtk::gdk::Key::x | gtk::gdk::Key::X if !ctrl => cull(state, Flag::Rejected),
        gtk::gdk::Key::z | gtk::gdk::Key::Z if !ctrl => loupe_group::toggle_one(state),
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
    tape::reset(state);
    loupe_group::reset(state);
    leave_visit(state);
    let at = state.loupe.at.take();
    let was_open = at.is_some();

    let looked_at = at.and_then(|at| id_at(state, at));
    state.loupe.textures.borrow_mut().clear();
    state.loupe.asked.borrow_mut().clear();
    loupe_zoom::reset(state);

    state.loupe.undo.borrow_mut().clear();

    state.loupe.reveal.set_reveal_child(false);

    let back_to_grid = state.stack.visible_child_name().as_deref() == Some("library");
    if was_open && back_to_grid && state.grid.stale.replace(false) {
        reload_grid(state);
    }
    state.loupe.scope.take();

    if was_open && rapid::is_on(state) {

        state.grid.wall.unselect_all();
        if back_to_grid {
            rapid::rebuild(state);
        }
        if let Some(id) = looked_at {
            rapid::keys_to(state, id);
        }
    }
}

fn cull(state: &App, flag: Flag) {
    let Some(at) = state.loupe.at.get() else { return };
    apply_to_selection(state, Action::Flag(flag));
    refresh_loupe_bar(state);

    if beside(state, at, 1).is_none() {
        state.toast(match state.loupe.scope.borrow().is_empty() {
            true => "Last photograph — Space goes back to the grid",
            false => "Last of the burst — Space when you have picked",
        });
        return;
    }

    let frame = state.loupe.picture.paintable().filter(|_| !loupe_zoom::zoomed(state) && !loupe_group::active(state));
    step_loupe(state, true);
    if let Some(frame) = frame {
        slide_away(state, frame, flag == Flag::Picked);
    }
}

pub(super) fn undo_mark(state: &App) {
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
    if let Some(next) = beside(state, at, if forward { 1 } else { -1 }) {
        go_to(state, arrive(state, at, next));
    }
}

fn arrive(state: &App, from: usize, to: usize) -> usize {
    if !state.loupe.scope.borrow().is_empty() {
        return to;
    }
    let run = burst_run(state, to);
    if run.len() < 2 || run.contains(&from) {
        return to;
    }
    best_in(state, run).unwrap_or(to)
}

pub(super) fn go_to(state: &App, index: usize) {
    state.grid.wall.select_only(index);
    state.grid.wall.reveal(index);
    show_loupe(state, index);
}

fn beside(state: &App, at: usize, by: isize) -> Option<usize> {
    let scope = state.loupe.scope.borrow();
    if scope.is_empty() {
        return at.checked_add_signed(by).filter(|index| *index < state.grid.lazy.borrow().len());
    }
    let here = scope.iter().position(|index| *index == at)?;
    scope.get(here.checked_add_signed(by)?).copied()
}

pub(super) fn cull_only(state: &App, ids: &[i64]) {
    let indices: Vec<usize> = {
        let lazy = state.grid.lazy.borrow();
        ids.iter().filter_map(|id| lazy.iter().position(|card| card.id == *id)).collect()
    };
    let Some(first) = indices.first().copied() else { return };

    let best = {
        let cards = state.grid.cards.borrow();
        indices.iter().copied().find(|index| id_at(state, *index).and_then(|id| cards.get(&id)).is_some_and(|photo| photo.best_of_burst))
    };
    *state.loupe.scope.borrow_mut() = indices;
    go_to(state, best.unwrap_or(first));
}

pub(super) fn id_at(state: &App, at: usize) -> Option<i64> {
    state.grid.lazy.borrow().get(at).map(|card| card.id)
}

fn burst_at(state: &App, at: usize) -> Option<(i64, i64)> {
    let id = id_at(state, at)?;
    let burst = state.grid.cards.borrow().get(&id)?.burst?;
    Some((numa::io::catalog::library_of(id), burst))
}

pub(super) fn burst_run(state: &App, at: usize) -> std::ops::Range<usize> {
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
        .find(|index| id_at(state, *index).and_then(|id| cards.get(&id)).is_some_and(|photo| photo.best_of_burst))
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

pub(super) fn leave_visit(state: &App) {
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

    let few = state.loupe.scope.borrow().clone();
    if !few.is_empty() {
        let unmarked: Vec<i64> = {
            let cards = state.grid.cards.borrow();
            few.iter().filter_map(|index| id_at(state, *index)).filter(|id| cards.get(id).is_some_and(|photo| photo.flag == Flag::None)).collect()
        };
        if !unmarked.is_empty() {
            apply_to_ids(state, &unmarked, Action::Flag(Flag::Rejected));
        }
        close_loupe(state);
        return;
    }
    let run = burst_run(state, at);
    if run.len() < 2 {
        cull(state, Flag::Rejected);
        return;
    }
    let unmarked: Vec<i64> = {
        let cards = state.grid.cards.borrow();
        run.filter_map(|index| id_at(state, index))
            .filter(|id| cards.get(id).is_some_and(|photo| photo.flag == Flag::None))
            .collect()
    };

    if !unmarked.is_empty() {
        apply_to_ids(state, &unmarked, Action::Flag(Flag::Rejected));
    }
    refresh_loupe_bar(state);
    let (frame, before) = (
        state.loupe.picture.paintable().filter(|_| !loupe_zoom::zoomed(state) && !loupe_group::active(state)),
        state.loupe.at.get(),
    );
    step_burst(state, true);
    if let (Some(frame), true) = (frame, state.loupe.at.get() != before) {
        slide_away(state, frame, false);
    }
}

fn refresh_burst(state: &App, at: usize) {
    let run = burst_run(state, at);
    state.loupe.burst.set_visible(run.len() > 1);
    if run.len() < 2 {
        return;
    }

    if loupe_group::active(state) {
        let kept = {
            let cards = state.grid.cards.borrow();
            run.clone().filter(|index| id_at(state, *index).and_then(|id| cards.get(&id)).is_some_and(|photo| photo.flag == Flag::Picked)).count()
        };
        state.loupe.burst.set_text(&format!("Burst of {} \u{b7} {kept} kept", run.len()));
        return;
    }
    let place = at - run.start + 1;
    let cards = state.grid.cards.borrow();
    let mut pips = Vec::new();
    let mut best = None;
    for (n, index) in run.clone().enumerate() {
        let Some(photo) = id_at(state, index).and_then(|id| cards.get(&id)) else { continue };
        let glyph = match photo.flag {
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
    state.loupe.burst.set_markup(&format!("[ {} ]   {place} of {}{why}", pips.join(" "), run.len()));
}

pub(super) fn echo_note(state: &App, at: usize) -> Option<String> {
    let cards = state.grid.cards.borrow();
    let photo = cards.get(&id_at(state, at)?)?;
    let other = cards.get(&photo.echo?)?;
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
    tape::refresh(state);
    let Some(id) = id_at(state, at) else { return };
    let name = {
        let cards = state.grid.cards.borrow();
        let Some(photo) = cards.get(&id) else { return };

        photo.path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default()
    };
    let (rating, flag) = loupe_rating(state);

    let sharpening = loupe_zoom::sharpening(state);
    loupe_zoom::wait_while(state, sharpening);
    match sharpening {
        true => state.loupe.caption.set_text(&format!("{name} \u{b7} developing at full size\u{2026}")),
        false => state.loupe.caption.set_text(&name),
    }
    loupe_group::refresh(state);
    refresh_burst(state, at);

    for (index, star) in state.loupe.stars.iter().enumerate() {
        let filled = index as u8 + 1 <= rating;
        star.set_icon_name(if filled { "starred-symbolic" } else { "non-starred-symbolic" });
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

    if let Some(row) = state.loupe.pick.parent() {
        match flag == Flag::Rejected {
            true => row.add_css_class("rejected"),
            false => row.remove_css_class("rejected"),
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

    pub(super) asked: Rc<RefCell<std::collections::HashSet<i64>>>,

    pub(super) tape: tape::State,

    pub(super) zoom: loupe_zoom::State,

    pub(super) group: loupe_group::State,

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

    pub(super) scope: Rc<RefCell<Vec<usize>>>,

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
            neighbours: Rc::new(Cell::new(false)),
            slide: Rc::new(Cell::new(0.0)),
            leaving_pick: Rc::new(Cell::new(false)),
            picked_frame: gtk::DrawingArea::new(),
            animation: Rc::new(RefCell::new(None)),
            textures: Rc::new(RefCell::new(std::collections::HashMap::new())),
            asked: Rc::default(),
            tape: tape::State::new(),
            zoom: loupe_zoom::State::new(),
            group: loupe_group::State::new(),
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
            scope: Rc::default(),
            undo: Rc::new(RefCell::new(Vec::new())),
        }
    }
}
