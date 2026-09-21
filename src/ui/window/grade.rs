use super::*;

pub(super) fn build_grade(state: &App, global_only: &dyn Fn(&gtk::Widget)) -> gtk::Box {
    let grade = page_column();
    grade.add_css_class("quiet");

    let header = section_header("Colour grading");
    global_only(header.as_ref());
    grade.append(&header);
    let grading = build_grading(state);
    global_only(grading.as_ref());
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
            move |tab| {
                if !tab.is_active() {
                    return;
                }
                state.grade.grading_range.set(index);

                write_grading(&state);
        refresh_retouch(&state);
        refresh_face(&state);
        refresh_found(&state);
            }
        ));
        ranges.append(&tab);
    }
    column.append(&ranges);

    let names = [("Hue", Readout::Degrees), ("Saturation", Readout::Positive(0)), ("Luminance", Readout::Signed(0))];
    for (index, (name, readout)) in names.into_iter().enumerate() {
        let scale = &state.grade.grading_sliders[index];
        if index == 0 {

            scale.add_css_class("hue-slider");
        }
        scale.connect_value_changed(glib::clone!(
            #[strong] state,
            move |_| {
                if !state.applying.get() {
                    read_grading(&state);
                }
                tint_grading_hue(&state);
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
