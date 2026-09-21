use super::*;

#[derive(Clone)]
pub(super) struct State {

    pub(super) curve_area: gtk::DrawingArea,

    pub(super) curve_channel: Rc<Cell<usize>>,
}

impl State {
    pub(super) fn new() -> Self {
        Self {
            curve_area: gtk::DrawingArea::new(),
            curve_channel: Rc::new(Cell::new(0)),
        }
    }
}

pub(super) fn build_light(
    state: &App,
    all: &[(&'static str, &gtk::Scale, Readout); SLIDER_COUNT],
    global_only: &dyn Fn(&gtk::Widget),
) -> gtk::Box {
    let light = page_column();

    light.add_css_class("quiet");

    light.append(&section_header("Tone"));

    let auto_tone_button = gtk::Button::with_label("Auto");
    auto_tone_button.set_tooltip_text(Some(
        "Set exposure and the black and white points from this photograph",
    ));
    auto_tone_button.set_margin_bottom(6);
    auto_tone_button.connect_clicked(glib::clone!(
        #[strong] state,

        move |_| busy_sync(&state, "Looking at the photograph…", auto_tone)
    ));

    auto_tone_button.set_halign(gtk::Align::End);
    auto_tone_button.add_css_class("panel-action");
    global_only(auto_tone_button.as_ref());
    light.append(&auto_tone_button);

    for (name, scale, readout) in &all[2..8] {
        let row = slider_row(state, name, scale, *readout);

        if *name == "Exposure" {
            row.add_css_class("lead");
        }
        light.append(&row);
    }

    let curve_header = section_header("Tone curve");
    let curve = build_tone_curve(state, global_only);
    light.append(&curve_header);
    light.append(&curve);
    light
}

pub(super) const CURVE_EDGE: i32 = PANEL_WIDTH - RAIL_WIDTH - 28;

pub(super) fn build_tone_curve(state: &App, global_only: &dyn Fn(&gtk::Widget)) -> gtk::Box {
    let column = gtk::Box::new(gtk::Orientation::Vertical, 4);
    column.set_margin_bottom(6);

    let area = state.light.curve_area.clone();
    area.set_content_height(CURVE_EDGE);
    area.set_hexpand(true);
    area.add_css_class("tone-curve");

    area.set_draw_func(glib::clone!(
        #[strong] state,
        move |_, context, width, height| {

            let open = state.open.borrow();
            let curves = open.as_ref().map(|photo| match state.mask_overlay.selected_mask.get() {
                Some(index) => {
                    let mask = photo.document.masks().get(index).map(|mask| mask.curve.clone());
                    [mask.unwrap_or_else(Curve::identity), Curve::identity(), Curve::identity(), Curve::identity()]
                }
                None => photo.document.curves(),
            });
            draw_curve(
                context,
                width as f64,
                height as f64,
                curves.as_ref(),
                match state.mask_overlay.selected_mask.get() {
                    Some(_) => 0,
                    None => state.light.curve_channel.get(),
                },
                state.info.histogram.borrow().as_ref(),
            );
        }
    ));

    let held: Rc<Cell<Option<usize>>> = Rc::new(Cell::new(None));

    const GRAB: f32 = 0.04;

    let drag = gtk::GestureDrag::new();
    drag.connect_drag_begin(glib::clone!(
        #[strong] state,
        #[strong] held,
        move |_, x, y| {
            let Some(at) = curve_point(&state, x, y) else { return };
            let index = with_curve(&state, |curve| curve.place(at, GRAB));
            held.set(index);
            state.light.curve_area.queue_draw();
        }
    ));
    drag.connect_drag_update(glib::clone!(
        #[strong] state,
        #[strong] held,
        move |gesture, dx, dy| {
            let Some(index) = held.get() else { return };
            let Some((sx, sy)) = gesture.start_point() else { return };
            let Some(at) = curve_point(&state, sx + dx, sy + dy) else { return };

            with_curve(&state, |curve| curve.move_point(index, at));
            state.light.curve_area.queue_draw();
        }
    ));
    drag.connect_drag_end(glib::clone!(
        #[strong] held,
        move |_, _, _| held.set(None)
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
            state.light.curve_area.queue_draw();
        }
    ));
    area.add_controller(remove);

    let channels = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    channels.add_css_class("linked");
    let mut first: Option<gtk::ToggleButton> = None;
    for (index, (label, tooltip)) in
        [("RGB", "All channels"), ("R", "Red"), ("G", "Green"), ("B", "Blue")].into_iter().enumerate()
    {
        let button = gtk::ToggleButton::with_label(label);
        button.set_hexpand(true);
        button.set_tooltip_text(Some(tooltip));
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
                    state.light.curve_area.queue_draw();
                }
            }
        ));
        channels.append(&button);
    }

    let hint = gtk::Label::new(Some("Drag to shape · right-click a point to remove"));
    hint.set_xalign(0.0);
    hint.add_css_class("profile-note");

    global_only(channels.as_ref());
    column.append(&channels);
    column.append(&area);
    column.append(&hint);
    column
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

fn with_curve<T>(state: &App, edit: impl FnOnce(&mut Curve) -> T) -> Option<T> {
    let out = {
        let mut open = state.open.borrow_mut();
        let photo = open.as_mut()?;

        if let Some(index) = state.mask_overlay.selected_mask.get() {
            let mut masks = photo.document.masks();
            let mask = masks.get_mut(index)?;
            let out = edit(&mut mask.curve);
            photo.document.set_masks(masks);
            photo.view = None;
            out
        } else {
            let mut curves = photo.document.curves();
            let out = edit(&mut curves[state.light.curve_channel.get()]);
            photo.document.set_curves(curves);
            out
        }
    };

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

    for [x, y] in curve.points() {
        let (px, py) = at(*x as f64, *y as f64);
        context.arc(px, py, HANDLE, 0.0, std::f64::consts::TAU);
        context.set_source_rgb(r, g, b);
        let _ = context.fill();
    }
}
