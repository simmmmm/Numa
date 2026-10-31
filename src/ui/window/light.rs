use super::*;

#[derive(Clone)]
pub(super) struct State {

    pub(super) curve_area: gtk::DrawingArea,

    pub(super) curve_channel: Rc<Cell<usize>>,

    pub(super) curve_chosen: Rc<Cell<Option<usize>>>,
    pub(super) curve_foot: gtk::Stack,
    pub(super) curve_in: gtk::Entry,
    pub(super) curve_out: gtk::Entry,
    pub(super) curve_remove: gtk::Button,

    pub(super) curve_held: Rc<RefCell<Option<(CurveScope, Curve)>>>,

    pub(super) auto: gtk::Button,
    pub(super) auto_waiting: Waiting,

    pub(super) plan: Plan,
}

pub(super) type CurveScope = (u64, Option<usize>, usize);

impl State {
    pub(super) fn new() -> Self {
        let auto = gtk::Button::new();
        Self {
            curve_area: gtk::DrawingArea::new(),
            curve_channel: Rc::new(Cell::new(0)),
            curve_chosen: Rc::new(Cell::new(None)),
            curve_foot: gtk::Stack::new(),
            curve_in: gtk::Entry::new(),
            curve_out: gtk::Entry::new(),
            curve_remove: gtk::Button::with_label("Remove"),
            curve_held: Rc::new(RefCell::new(None)),
            auto_waiting: Waiting::new("Looking at the photograph", &auto),
            auto,
            plan: Plan::new(),
        }
    }
}

pub(super) fn build_light(
    state: &App,
    all: &[(&'static str, &gtk::Scale, Readout); SLIDER_COUNT],
    global_only: &dyn Fn(&gtk::Widget),
) -> gtk::Box {
    let light = page_column();

    let auto_tone_button = state.light.auto.clone();
    word_or_spinner(&auto_tone_button, "Auto", &state.light.auto_waiting);
    auto_tone_button.set_tooltip_text(Some(
        "Level the photograph and set its light from what is in it (A)",
    ));
    auto_tone_button.connect_clicked(glib::clone!(
        #[strong] state,

        move |_| auto_press(&state)
    ));

    global_only(auto_tone_button.as_ref());

    light.append(&section_row("Tone", &[auto_tone_button.upcast_ref()]));

    for (name, scale, readout) in &all[2..8] {

        light.append(&slider_row(state, name, scale, *readout));
    }

    light.append(&build_tone_curve(state, global_only));
    light
}

pub(super) const CURVE_EDGE: i32 = PANEL_WIDTH - RAIL_WIDTH - 40;

pub(super) fn build_tone_curve(state: &App, global_only: &dyn Fn(&gtk::Widget)) -> gtk::Box {
    let column = gtk::Box::new(gtk::Orientation::Vertical, 4);
    column.set_margin_bottom(6);

    let area = state.light.curve_area.clone();
    area.set_content_height(CURVE_EDGE);
    area.set_hexpand(true);
    area.add_css_class("tone-curve");

    area.set_tooltip_text(Some(CURVE_KEYS));

    area.set_focusable(true);

    area.set_draw_func(glib::clone!(
        #[strong] state,
        move |_, context, width, height| {

            let open = state.open.borrow();
            let curves = open.as_ref().map(|photo| match state.mask_overlay.selected_mask.get() {
                Some(index) => match photo.document.masks().get(index) {
                    Some(mask) => {
                        let [red, green, blue] = mask.channel_curves.clone();
                        [mask.curve.clone(), red, green, blue]
                    }
                    None => [Curve::identity(), Curve::identity(), Curve::identity(), Curve::identity()],
                },
                None => photo.document.curves(),
            });
            let curves = curves.map(|mut curves| {
                if let Some(held) = held_curve(&state) {
                    curves[state.light.curve_channel.get().min(3)] = held;
                }
                curves
            });
            draw_curve(
                context,
                width as f64,
                height as f64,
                curves.as_ref(),
                state.light.curve_channel.get(),
                state.light.curve_chosen.get(),
                state.info.histogram.borrow().as_ref(),
            );

            let moved = curves.map(|all| all.map(|curve| !curve.is_identity())).unwrap_or([false; 4]);
            glib::idle_add_local_once(move || show_curve_moved(moved));
            let state = state.clone();
            glib::idle_add_local_once(move || show_curve_point(&state));
        }
    ));

    curve_gestures(state, &area);

    let _ = global_only;

    let (line, channels) = curve_line(state);
    column.append(&line);
    column.append(&channels);
    column.append(&area);
    column.append(&curve_footer(state));
    column
}

fn curve_gestures(state: &App, area: &gtk::DrawingArea) {

    let held: Rc<Cell<Option<(usize, [f32; 2], [f32; 2])>>> = Rc::new(Cell::new(None));

    const GRAB: f32 = 0.04;

    let drag = gtk::GestureDrag::new();
    drag.connect_drag_begin(glib::clone!(
        #[strong] state,
        #[strong] held,
        move |_, x, y| {
            let Some(press) = curve_point(&state, x, y) else { return };
            let grabbed = with_curve(&state, |curve| {
                let near = curve
                    .points()
                    .iter()
                    .enumerate()
                    .min_by(|(_, a), (_, b)| (a[0] - press[0]).abs().total_cmp(&(b[0] - press[0]).abs()))
                    .filter(|(_, point)| (point[0] - press[0]).abs() <= GRAB)
                    .map(|(index, point)| (index, *point));
                near.unwrap_or_else(|| {
                    let on_line = [press[0], curve.value_at(press[0])];
                    (curve.place(on_line, 0.0), on_line)
                })
            });
            held.set(grabbed.map(|(index, point)| (index, point, press)));
            state.light.curve_chosen.set(grabbed.map(|(index, _)| index));
            state.light.curve_area.grab_focus();
            state.light.curve_area.queue_draw();
        }
    ));
    drag.connect_drag_update(glib::clone!(
        #[strong] state,
        #[strong] held,
        move |gesture, dx, dy| {
            let Some((index, from, press)) = held.get() else { return };
            let Some((sx, sy)) = gesture.start_point() else { return };
            let Some(now) = curve_point(&state, sx + dx, sy + dy) else { return };
            let to = [from[0] + now[0] - press[0], from[1] + now[1] - press[1]];
            with_curve(&state, |curve| curve.move_point(index, to));
            state.light.curve_area.queue_draw();
        }
    ));
    drag.connect_drag_end(glib::clone!(
        #[strong] state,
        #[strong] held,
        move |gesture, dx, dy| {

            if let (Some((index, _, _)), Some((sx, sy))) = (held.get(), gesture.start_point()) {
                let area = &state.light.curve_area;
                let (x, y) = (sx + dx, sy + dy);
                let out = x < -OUT || y < -OUT || x > area.width() as f64 + OUT || y > area.height() as f64 + OUT;
                if out {
                    with_curve(&state, |curve| curve.remove(index));
                    state.light.curve_chosen.set(None);
                    area.queue_draw();
                }
            }
            held.set(None);
        }
    ));
    area.add_controller(drag);

    let remove = gtk::GestureClick::new();
    remove.set_button(gtk::gdk::BUTTON_SECONDARY);
    remove.connect_pressed(glib::clone!(
        #[strong] state,
        move |_, _, x, y| {
            let Some([at, _]) = curve_point(&state, x, y) else { return };

            let found = with_curve(&state, |curve| {
                let nearest = curve
                    .points()
                    .iter()
                    .enumerate()
                    .min_by(|(_, a), (_, b)| (a[0] - at).abs().total_cmp(&(b[0] - at).abs()))?;
                ((nearest.1[0] - at).abs() <= GRAB).then_some(nearest.0)
            })
            .flatten();

            let Some(index) = found else { return };
            with_curve(&state, |curve| curve.remove(index));
            state.light.curve_chosen.set(None);
            state.light.curve_area.queue_draw();
        }
    ));
    area.add_controller(remove);

    let twice = gtk::GestureClick::new();
    twice.set_button(gtk::gdk::BUTTON_PRIMARY);
    twice.connect_pressed(glib::clone!(
        #[strong] state,
        move |_, presses, _, _| {
            if presses == 2 {
                with_curve(&state, |curve| *curve = Curve::identity());
                state.light.curve_chosen.set(None);
                state.light.curve_area.queue_draw();
            }
        }
    ));
    area.add_controller(twice);
    area.add_controller(curve_keys(state));
}

const CURVE_HINT: &str = "Drag to shape · right-click a point to remove";
const CURVE_KEYS: &str = "Drag to shape · right-click a point to remove\n← → choose a point · ↑ ↓ its Out · Alt+← → its In · Shift for 10 · Return adds one · Delete removes it";

fn curve_footer(state: &App) -> gtk::Widget {
    let hint = gtk::Label::new(Some(CURVE_HINT));
    hint.set_xalign(0.0);
    hint.set_ellipsize(gtk::pango::EllipsizeMode::End);
    hint.add_css_class("caption");
    hint.add_css_class("curve-hint");
    let values = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    for (index, (word, entry)) in [("In", &state.light.curve_in), ("→ Out", &state.light.curve_out)].into_iter().enumerate() {
        let label = gtk::Label::new(Some(word));
        label.add_css_class("curve-hint");
        values.append(&label);
        entry.set_width_chars(3);
        entry.set_max_width_chars(3);
        gtk::prelude::EntryExt::set_alignment(entry, 1.0);
        entry.set_input_purpose(gtk::InputPurpose::Digits);
        entry.add_css_class("curve-value");
        entry.update_property(&[gtk::accessible::Property::Label(if index == 0 { "In" } else { "Out" })]);
        entry.connect_activate(glib::clone!(
            #[strong] state,
            move |entry| {
                let Some(chosen) = state.light.curve_chosen.get() else { return };
                let Ok(typed) = entry.text().trim().parse::<f32>() else { return };
                let value = (typed / 255.0).clamp(0.0, 1.0);
                with_curve(&state, |curve| {
                    let Some(&[x, y]) = curve.points().get(chosen) else { return };
                    curve.move_point(chosen, if index == 0 { [value, y] } else { [x, value] });
                });
                state.light.curve_area.queue_draw();
                state.light.curve_area.grab_focus();
            }
        ));
        values.append(entry);
    }
    let remove = state.light.curve_remove.clone();
    remove.add_css_class("flat");
    remove.add_css_class("row-accessory");
    remove.set_tooltip_text(Some("Remove Point"));
    remove.connect_clicked(glib::clone!(
        #[strong] state,
        move |_| remove_chosen_point(&state)
    ));

    let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    spacer.set_hexpand(true);
    values.append(&spacer);
    values.append(&remove);
    let foot = state.light.curve_foot.clone();
    foot.add_css_class("curve-foot");
    foot.add_named(&hint, Some("hint"));
    foot.add_named(&values, Some("point"));
    show_curve_point(state);
    foot.clone().upcast()
}

fn show_curve_point(state: &App) {
    let light = &state.light;
    let point = light.curve_chosen.get().and_then(|chosen| {
        let points = shown_curve(state)?.points().to_vec();
        let ends = chosen == 0 || chosen + 1 == points.len();
        points.get(chosen).map(|point| (*point, !ends))
    });
    light.curve_foot.set_visible_child_name(if point.is_some() { "point" } else { "hint" });
    let removable = point.is_some_and(|(_, removable)| removable);
    light.curve_remove.set_opacity(if removable { 1.0 } else { 0.0 });
    light.curve_remove.set_sensitive(removable);
    light.curve_remove.set_can_target(removable);
    if let Some(([x, y], _)) = point {
        for (entry, value) in [(&light.curve_in, x), (&light.curve_out, y)] {
            if !entry.has_focus() {
                entry.set_text(&format!("{:.0}", value * 255.0));
            }
        }
    }
}

fn shown_curve(state: &App) -> Option<Curve> {
    if let Some(held) = held_curve(state) {
        return Some(held);
    }
    let open = state.open.borrow();
    let photo = open.as_ref()?;
    let channel = state.light.curve_channel.get();
    Some(match state.mask_overlay.selected_mask.get() {
        Some(index) => {
            let mask = photo.document.masks().get(index)?.clone();
            match channel {
                0 => mask.curve,
                channel => mask.channel_curves[(channel - 1).min(2)].clone(),
            }
        }
        None => photo.document.curves()[channel.min(3)].clone(),
    })
}

fn remove_chosen_point(state: &App) {
    let Some(chosen) = state.light.curve_chosen.get() else { return };
    with_curve(state, |curve| curve.remove(chosen));
    state.light.curve_chosen.set(None);
    state.light.curve_area.queue_draw();
}

fn curve_keys(state: &App) -> gtk::EventControllerKey {
    use gtk::gdk::Key;
    let keys = gtk::EventControllerKey::new();
    keys.connect_key_pressed(glib::clone!(
        #[strong] state,
        move |_, key, _, modifiers| {
            let Some(points) = shown_curve(&state).map(|curve| curve.points().to_vec()) else {
                return glib::Propagation::Proceed;
            };
            let chosen = state.light.curve_chosen.get().filter(|chosen| *chosen < points.len());
            let step = if modifiers.contains(gtk::gdk::ModifierType::SHIFT_MASK) { 10.0 } else { 1.0 } / 255.0;
            let alt = modifiers.contains(gtk::gdk::ModifierType::ALT_MASK);
            let nudge = |state: &App, chosen: usize, by: [f32; 2]| {
                let [x, y] = points[chosen];
                with_curve(state, |curve| curve.move_point(chosen, [x + by[0], y + by[1]]));
            };
            match (key, chosen) {
                (Key::Left | Key::Right, Some(chosen)) if alt => {
                    nudge(&state, chosen, [if key == Key::Left { -step } else { step }, 0.0]);
                }
                (Key::Left, _) => state.light.curve_chosen.set(Some(chosen.map_or(0, |chosen| chosen.saturating_sub(1)))),
                (Key::Right, _) => state.light.curve_chosen.set(Some(chosen.map_or(0, |chosen| (chosen + 1).min(points.len() - 1)))),
                (Key::Up | Key::Down, Some(chosen)) => nudge(&state, chosen, [0.0, if key == Key::Down { -step } else { step }]),
                (Key::Return | Key::KP_Enter, _) => {
                    let from = chosen.unwrap_or(0).min(points.len().saturating_sub(2));
                    let x = (points[from][0] + points[from + 1][0]) / 2.0;
                    let added = with_curve(&state, |curve| curve.place([x, curve.value_at(x)], 0.0));
                    state.light.curve_chosen.set(added);
                }
                (Key::Delete | Key::BackSpace, Some(_)) => remove_chosen_point(&state),
                _ => return glib::Propagation::Proceed,
            }
            state.light.curve_area.queue_draw();
            glib::Propagation::Stop
        }
    ));
    keys
}

const OUT: f64 = 12.0;

thread_local! {

    static CURVE_MARKS: RefCell<Option<(gtk::Button, [gtk::Box; 4])>> = const { RefCell::new(None) };
}

fn show_curve_moved(moved: [bool; 4]) {
    CURVE_MARKS.with(|marks| {
        if let Some((reset, dots)) = marks.borrow().as_ref() {
            reset.set_visible(moved.iter().any(|&m| m));
            for (dot, moved) in dots.iter().zip(moved) {
                dot.set_visible(moved);
            }
        }
    });
}

fn curve_line(state: &App) -> (gtk::Box, gtk::Box) {
    let channels = chip_row();
    channels.add_css_class("four");
    channels.set_homogeneous(true);
    let mut first: Option<gtk::ToggleButton> = None;
    let mut dots = Vec::new();
    for (index, (label, tooltip)) in
        [("RGB", "All channels"), ("R", "Red"), ("G", "Green"), ("B", "Blue")].into_iter().enumerate()
    {
        let button = gtk::ToggleButton::new();

        let face = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        face.set_halign(gtk::Align::Center);
        face.append(&gtk::Label::new(Some(label)));
        let dot = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        dot.add_css_class("touched-dot");
        dot.set_valign(gtk::Align::Center);
        dot.set_visible(false);
        face.append(&dot);
        dots.push(dot);
        button.set_child(Some(&face));
        button.set_tooltip_text(Some(tooltip));
        button.update_property(&[gtk::accessible::Property::Label(label)]);
        button.set_active(index == 0);
        if let Some(first) = &first {
            button.set_group(Some(first));
        } else {
            first = Some(button.clone());
        }
        button.connect_toggled(glib::clone!(
            #[strong] state,
            move |button| {
                if button.is_active() {
                    state.light.curve_channel.set(index);
                    state.light.curve_chosen.set(None);
                    state.light.curve_area.queue_draw();
                }
            }
        ));
        channels.append(&button);
    }

    let title = section_header("Tone curve");
    let line = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    line.append(&title);

    let reset = gtk::Button::from_icon_name("edit-undo-symbolic");
    reset.add_css_class("flat");
    reset.add_css_class("panel-accessory");
    reset.set_tooltip_text(Some("Reset Curve"));
    reset.update_property(&[gtk::accessible::Property::Label("Reset Curve")]);
    reset.set_valign(gtk::Align::Center);
    reset.set_visible(false);
    reset.connect_clicked(glib::clone!(
        #[strong] state,
        move |_| {
            let channel = state.light.curve_channel.get();
            for each in 0..4 {
                state.light.curve_channel.set(each);
                with_curve(&state, |curve| *curve = Curve::identity());
            }
            state.light.curve_channel.set(channel);
            state.light.curve_area.queue_draw();
        }
    ));
    let slot = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    slot.add_css_class("reset-slot");
    slot.set_size_request(24, 24);
    slot.append(&reset);
    line.append(&slot);
    let dots: [gtk::Box; 4] = dots.try_into().unwrap_or_else(|_| unreachable!("four channels"));
    CURVE_MARKS.with(|marks| *marks.borrow_mut() = Some((reset, dots)));
    (line, channels)
}

pub(super) fn draw_tone_distribution(
    context: &gtk::cairo::Context,
    width: f64,
    height: f64,
    histogram: Option<&render::histogram::Histogram>,
) {
    let Some(histogram) = histogram else { return };

    let scale = histogram.scale() as f64;
    let step = width / render::histogram::BINS as f64;

    context.set_source_rgba(1.0, 1.0, 1.0, 0.13);
    context.move_to(0.0, height);
    for bin in 0..render::histogram::BINS {

        let count = histogram.channels.iter().map(|channel| channel[bin]).max().unwrap_or(0);
        let value = (count as f64 / scale).min(1.0);
        context.line_to(bin as f64 * step, height - value * height);
    }
    context.line_to(width, height);
    context.close_path();
    let _ = context.fill();
}

pub(super) const HANDLE: f64 = 4.0;

pub(super) fn plot_rect(width: f64, height: f64) -> (f64, f64, f64, f64) {
    let inset = (HANDLE + 1.0).min(width / 4.0).min(height / 4.0);
    (inset, inset, (width - inset * 2.0).max(1.0), (height - inset * 2.0).max(1.0))
}

pub(super) fn curve_point(state: &App, x: f64, y: f64) -> Option<[f32; 2]> {
    let (width, height) = (state.light.curve_area.width() as f64, state.light.curve_area.height() as f64);
    if width <= 0.0 || height <= 0.0 {
        return None;
    }

    let (left, top, plot_width, plot_height) = plot_rect(width, height);
    Some([
        ((x - left) / plot_width).clamp(0.0, 1.0) as f32,
        (1.0 - (y - top) / plot_height).clamp(0.0, 1.0) as f32,
    ])
}

fn curve_scope(state: &App) -> CurveScope {
    (state.open_generation.get(), state.mask_overlay.selected_mask.get(), state.light.curve_channel.get())
}

fn held_curve(state: &App) -> Option<Curve> {
    let scope = curve_scope(state);
    state.light.curve_held.borrow().as_ref().filter(|(at, _)| *at == scope).map(|(_, curve)| curve.clone())
}

fn with_curve<T>(state: &App, edit: impl FnOnce(&mut Curve) -> T) -> Option<T> {

    let (scope, held) = (curve_scope(state), held_curve(state));
    let mut keep = None;
    let apply = |curve: &mut Curve| {
        if let Some(held) = held.clone().filter(|_| curve.is_identity()) {
            *curve = held;
        }
        let out = edit(curve);
        keep = (curve.is_identity() && curve.points().len() > 2).then(|| curve.clone());
        out
    };
    let out = {
        let mut open = state.open.borrow_mut();
        let photo = open.as_mut()?;

        if let Some(index) = state.mask_overlay.selected_mask.get() {
            let mut masks = photo.document.masks();
            let mask = masks.get_mut(index)?;
            let out = match state.light.curve_channel.get() {
                0 => apply(&mut mask.curve),
                channel => apply(&mut mask.channel_curves[(channel - 1).min(2)]),
            };
            photo.document.set_masks(masks);
            out
        } else {
            let mut curves = photo.document.curves();
            let out = apply(&mut curves[state.light.curve_channel.get()]);
            photo.document.set_curves(curves);
            out
        }
    };
    *state.light.curve_held.borrow_mut() = keep.map(|curve| (scope, curve));

    request_render(state);
    schedule_history_push(state);
    Some(out)
}

pub(super) fn draw_curve(
    context: &gtk::cairo::Context,
    width: f64,
    height: f64,
    curves: Option<&[Curve; 4]>,
    channel: usize,
    chosen: Option<usize>,
    histogram: Option<&render::histogram::Histogram>,
) {

    let _ = context.save();
    draw_tone_distribution(context, width, height, histogram);
    let _ = context.restore();

    let (left, top, plot_width, plot_height) = plot_rect(width, height);
    let at = |x: f64, y: f64| (left + x * plot_width, top + (1.0 - y) * plot_height);

    context.set_line_width(1.0);
    context.set_source_rgba(1.0, 1.0, 1.0, 0.10);
    for step in 1..4 {
        let fraction = step as f64 / 4.0;
        let (vertical, _) = at(fraction, 0.0);
        let (_, horizontal) = at(0.0, fraction);
        context.move_to(vertical, top);
        context.line_to(vertical, top + plot_height);
        context.move_to(left, horizontal);
        context.line_to(left + plot_width, horizontal);
    }
    let _ = context.stroke();

    context.set_dash(&[3.0, 3.0], 0.0);
    context.set_source_rgba(1.0, 1.0, 1.0, 0.18);
    let (x0, y0) = at(0.0, 0.0);
    let (x1, y1) = at(1.0, 1.0);
    context.move_to(x0, y0);
    context.line_to(x1, y1);
    let _ = context.stroke();
    context.set_dash(&[], 0.0);

    let Some(curves) = curves else { return };
    let colours = [(0.95, 0.95, 0.95), (0.95, 0.42, 0.40), (0.45, 0.85, 0.45), (0.45, 0.62, 0.98)];

    const SAMPLES: usize = 128;
    let trace = |curve: &Curve| {
        for step in 0..=SAMPLES {
            let x = step as f64 / SAMPLES as f64;
            let (px, py) = at(x, curve.value_at(x as f32) as f64);
            context.line_to(px, py);
        }
        let _ = context.stroke();
    };

    context.set_line_width(1.2);
    for (index, other) in curves.iter().enumerate() {
        if index != channel && !other.is_identity() {
            let (r, g, b) = colours[index];
            context.set_source_rgba(r, g, b, 0.35);
            trace(other);
        }
    }

    let curve = &curves[channel.min(3)];
    let (r, g, b) = colours[channel.min(3)];
    context.set_line_width(1.8);
    context.set_source_rgb(r, g, b);
    trace(curve);

    for (index, [x, y]) in curve.points().iter().enumerate() {
        let (px, py) = at(*x as f64, *y as f64);
        context.set_source_rgb(r, g, b);
        if Some(index) == chosen {
            context.arc(px, py, 6.0, 0.0, std::f64::consts::TAU);
            let _ = context.fill();
        } else {

            context.arc(px, py, HANDLE - 0.5, 0.0, std::f64::consts::TAU);
            context.set_operator(gtk::cairo::Operator::Clear);
            let _ = context.fill_preserve();
            context.set_operator(gtk::cairo::Operator::Over);
            context.set_line_width(1.0);
            let _ = context.stroke();
        }
    }
}
