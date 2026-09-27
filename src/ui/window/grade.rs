use super::*;

pub(super) fn build_grade(state: &App, global_only: &dyn Fn(&gtk::Widget)) -> gtk::Box {
    let grade = page_column();
    grade.add_css_class("quiet");

    let header = section_header("Colour grading");
    grade.append(&header);

    let _ = global_only;
    let grading = build_grading(state);
    grade.append(&grading);
    grade
}

#[derive(Clone)]
pub(super) struct State {

    pub(super) grading_sliders: Vec<gtk::Scale>,
    pub(super) grading_shape: Vec<gtk::Scale>,
    pub(super) grading_range: Rc<Cell<usize>>,
}

impl State {
    pub(super) fn new() -> Self {
        Self {
            grading_sliders: vec![
            gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 360.0, 1.0),
            gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 100.0, 1.0),
            gtk::Scale::with_range(gtk::Orientation::Horizontal, -100.0, 100.0, 1.0),
            ],
            grading_shape: vec![
            gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 100.0, 1.0),
            gtk::Scale::with_range(gtk::Orientation::Horizontal, -100.0, 100.0, 1.0),
            ],

            grading_range: Rc::new(Cell::new(3)),
        }
    }
}

pub(super) fn build_grading(state: &App) -> gtk::Box {
    let column = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let (wheel, caption) = build_grading_wheel(state);

    let ranges = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    ranges.add_css_class("linked");
    ranges.add_css_class("aspect-ratios");
    ranges.set_margin_bottom(6);
    let mut first: Option<gtk::ToggleButton> = None;
    for (index, name) in ["Shadow", "Mid", "High", "All"].iter().enumerate() {
        let tab = gtk::ToggleButton::with_label(name);
        tab.set_hexpand(true);
        match &first {
            None => first = Some(tab.clone()),
            Some(first) => tab.set_group(Some(first)),
        }
        tab.set_active(index == state.grade.grading_range.get());
        tab.connect_toggled(glib::clone!(
            #[strong] state,
            #[weak] wheel,
            #[weak] caption,
            move |tab| {
                if !tab.is_active() {
                    return;
                }
                state.grade.grading_range.set(index);

                write_grading(&state);

                wheel.queue_draw();
                caption.set_label(&wheel_caption(&state));
        refresh_retouch(&state);
        refresh_face(&state);
        refresh_found(&state);
            }
        ));
        ranges.append(&tab);
    }
    column.append(&ranges);
    column.append(&wheel);
    column.append(&caption);

    let names = [("Hue", Readout::Degrees), ("Saturation", Readout::Positive(0)), ("Luminance", Readout::Signed(0))];
    for (index, (name, readout)) in names.into_iter().enumerate() {
        let scale = &state.grade.grading_sliders[index];
        if index == 0 {

            scale.add_css_class("hue-slider");
        }
        scale.connect_value_changed(glib::clone!(
            #[strong] state,
            #[weak] wheel,
            #[weak] caption,
            move |_| {
                if !state.applying.get() {
                    read_grading(&state);
                }
                tint_grading_hue(&state);
                if index < 2 {
                    wheel.queue_draw();
                    caption.set_label(&wheel_caption(&state));
                }
            }
        ));
        column.append(&slider_row(state, name, scale, readout));
    }

    let shaping = [
        ("Blending", Readout::Positive(0)),
        ("Balance", Readout::Signed(0)),
    ];
    set_neutral(&state.grade.grading_shape[0], Grading::default().blending as f64);
    for (index, (name, readout)) in shaping.into_iter().enumerate() {
        let scale = &state.grade.grading_shape[index];
        scale.connect_value_changed(glib::clone!(
            #[strong] state,
            move |_| {
                if !state.applying.get() {
                    read_grading(&state);
                }
            }
        ));
        column.append(&slider_row(state, name, scale, readout));
    }

    column
}

const WHEEL_RADIUS: f64 = 90.0;
const WHEEL_DOT: f64 = 22.0;

fn build_grading_wheel(state: &App) -> (gtk::DrawingArea, gtk::Label) {
    let side = (WHEEL_RADIUS + WHEEL_DOT / 2.0 + 1.0).ceil() as i32 * 2;
    let wheel = gtk::DrawingArea::new();
    wheel.set_content_width(side);
    wheel.set_content_height(side);
    wheel.set_halign(gtk::Align::Center);
    wheel.set_draw_func(glib::clone!(
        #[strong] state,
        move |_, context, width, height| {
            let (cx, cy) = (width as f64 / 2.0, height as f64 / 2.0);

            for degree in 0..360 {
                let (r, g, b) = gtk::hsv_to_rgb(degree as f32 / 360.0, 0.7, 0.8);
                context.set_source_rgb(r as f64, g as f64, b as f64);
                context.move_to(cx, cy);

                let from = (degree as f64 - 0.6).to_radians();
                context.arc(cx, cy, WHEEL_RADIUS, from, (degree as f64 + 1.0).to_radians());
                context.close_path();
                let _ = context.fill();
            }
            let fade = gtk::cairo::RadialGradient::new(cx, cy, 0.0, cx, cy, WHEEL_RADIUS);
            fade.add_color_stop_rgba(0.0, 0.45, 0.45, 0.45, 1.0);
            fade.add_color_stop_rgba(1.0, 0.45, 0.45, 0.45, 0.0);
            let _ = context.set_source(&fade);
            context.arc(cx, cy, WHEEL_RADIUS, 0.0, std::f64::consts::TAU);
            let _ = context.fill_preserve();
            context.set_source_rgba(1.0, 1.0, 1.0, 0.25);
            context.set_line_width(1.0);
            let _ = context.stroke();

            let (hue, saturation) = wheel_value(&state);
            let (dx, dy) = wheel_offset(hue, saturation);
            let (x, y) = (cx + dx, cy + dy);
            if saturation < 0.5 {
                context.set_source_rgb(0.45, 0.45, 0.45);
            } else {
                let (r, g, b) = gtk::hsv_to_rgb(hue as f32 / 360.0, 0.7, 0.8);
                context.set_source_rgb(r as f64, g as f64, b as f64);
            }
            context.arc(x, y, WHEEL_DOT / 2.0 - 1.0, 0.0, std::f64::consts::TAU);
            let _ = context.fill_preserve();
            context.set_source_rgb(1.0, 1.0, 1.0);
            context.set_line_width(2.0);
            let _ = context.stroke();
        }
    ));

    let start = Rc::new(Cell::new((0.0, 0.0)));
    let drag = gtk::GestureDrag::new();
    drag.connect_drag_begin(glib::clone!(
        #[strong] state,
        #[strong] start,
        move |_, _, _| {
            let (hue, saturation) = wheel_value(&state);
            start.set(wheel_offset(hue, saturation));
        }
    ));
    drag.connect_drag_update(glib::clone!(
        #[strong] state,
        #[strong] start,
        move |_, dx, dy| {
            let (x, y) = (start.get().0 + dx, start.get().1 + dy);
            let distance = x.hypot(y).min(WHEEL_RADIUS);

            if distance > 0.5 {
                state.grade.grading_sliders[0].set_value(y.atan2(x).to_degrees().rem_euclid(360.0));
            }
            state.grade.grading_sliders[1].set_value(distance / WHEEL_RADIUS * 100.0);
        }
    ));
    wheel.add_controller(drag);

    let click = gtk::GestureClick::new();
    click.connect_pressed(glib::clone!(
        #[strong] state,
        #[strong] start,
        move |_, presses, _, _| {
            if presses == 2 {

                start.set((0.0, 0.0));
                state.grade.grading_sliders[0].set_value(0.0);
                state.grade.grading_sliders[1].set_value(0.0);
            }
        }
    ));
    wheel.add_controller(click);

    let caption = gtk::Label::new(Some(&wheel_caption(state)));
    caption.add_css_class("caption");
    caption.add_css_class("dim-label");
    caption.set_margin_top(4);
    caption.set_margin_bottom(6);
    (wheel, caption)
}

fn wheel_value(state: &App) -> (f64, f64) {
    (state.grade.grading_sliders[0].value(), state.grade.grading_sliders[1].value())
}

fn wheel_offset(hue: f64, saturation: f64) -> (f64, f64) {
    let distance = saturation / 100.0 * WHEEL_RADIUS;
    let angle = hue.to_radians();
    (distance * angle.cos(), distance * angle.sin())
}

fn wheel_caption(state: &App) -> String {
    let name = ["Shadows", "Midtones", "Highlights", "Global"][state.grade.grading_range.get().min(3)];
    let (hue, saturation) = wheel_value(state);
    format!("{name} · {}° · {}", hue.round(), saturation.round())
}
