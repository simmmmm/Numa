use super::*;

#[derive(Clone)]
pub(super) struct State {

    pub(super) area: gtk::DrawingArea,
    pub(super) sliders: Vec<gtk::Scale>,

    pub(super) tool: Rc<Cell<Option<RetouchTool>>>,
    pub(super) heal: gtk::ToggleButton,
    pub(super) clone_tool: gtk::ToggleButton,
    pub(super) pet_eye: gtk::ToggleButton,
    pub(super) remove: gtk::ToggleButton,
    pub(super) on: Rc<Cell<bool>>,
    pub(super) selected_spot: Rc<Cell<Option<usize>>>,
    pub(super) list: gtk::ListBox,

    pub(super) face_sliders: Vec<gtk::Scale>,
    pub(super) face_section: gtk::Box,
    pub(super) face_note: gtk::Label,
}

impl State {
    pub(super) fn new() -> Self {
        Self {
            area: gtk::DrawingArea::new(),
            sliders: vec![

                gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.5, 20.0, 0.1),
                gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 100.0, 1.0),
                gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 100.0, 1.0),
            ],
            tool: Rc::new(Cell::new(None)),
            heal: gtk::ToggleButton::new(),
            clone_tool: gtk::ToggleButton::new(),
            pet_eye: gtk::ToggleButton::new(),
            remove: gtk::ToggleButton::new(),
            on: Rc::new(Cell::new(false)),
            selected_spot: Rc::new(Cell::new(None)),
            list: gtk::ListBox::new(),
            face_sliders: (0..5)
                .map(|_| gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 100.0, 1.0))
                .collect(),
            face_section: gtk::Box::new(gtk::Orientation::Vertical, 0),
            face_note: gtk::Label::new(None),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum RetouchTool {
    Heal,
    Clone,
    Remove,
    PetEye,
}

impl RetouchTool {

    pub(super) fn spot(self) -> (numa::core::retouch::Kind, bool) {
        use numa::core::retouch::Kind;
        match self {
            RetouchTool::Heal => (Kind::Patch, true),
            RetouchTool::Clone => (Kind::Patch, false),
            RetouchTool::PetEye => (Kind::PetEye, false),
            RetouchTool::Remove => (Kind::Remove, false),
        }
    }
}

fn tool_buttons(state: &App) -> gtk::Box {

    let tools = gtk::Box::new(gtk::Orientation::Vertical, 6);
    tools.set_margin_bottom(8);
    let pairs = [gtk::Box::new(gtk::Orientation::Horizontal, 0), gtk::Box::new(gtk::Orientation::Horizontal, 0)];
    for pair in &pairs {
        pair.add_css_class("linked");
        pair.set_homogeneous(true);
        tools.append(pair);
    }

    let buttons = [state.retouch.heal.clone(), state.retouch.clone_tool.clone(), state.retouch.remove.clone(), state.retouch.pet_eye.clone()];
    for (at, (button, label, tooltip, tool)) in [
        (&state.retouch.heal, "Heal", "The source's texture, this place's brightness", RetouchTool::Heal),
        (&state.retouch.clone_tool, "Clone", "The source exactly as it is", RetouchTool::Clone),
        (&state.retouch.remove, "Remove", "Take out what is under the circle and fill it from around it", RetouchTool::Remove),
        (&state.retouch.pet_eye, "Pet eye", "Put the glow in an animal's eye out: a circle over the pupil", RetouchTool::PetEye),
    ]
    .into_iter()
    .enumerate()
    {
        pairs[at / 2].append(button);
        button.set_label(label);
        button.set_hexpand(true);
        button.set_tooltip_text(Some(tooltip));
        let others: Vec<gtk::ToggleButton> = buttons.iter().filter(|other| *other != button).cloned().collect();
        button.connect_toggled(glib::clone!(
            #[strong] state,
            move |button| {
                if state.applying.get() {
                    return;
                }
                state.applying.set(true);
                if button.is_active() {
                    others.iter().for_each(|other| other.set_active(false));
                }
                state.applying.set(false);

                if button.is_active() && tool == RetouchTool::Remove && !numa::render::remove::is_installed() {
                    state.toast("Remove needs LaMa — Preferences downloads it");
                }
                state.retouch.tool.set(button.is_active().then_some(tool));

                if button.is_active() && matches!(tool, RetouchTool::Heal | RetouchTool::Clone) {
                    if let Some(index) = state.retouch.selected_spot.get() {
                        let mut spots = current_spots(&state);
                        if let Some(spot) = spots.get_mut(index).filter(|spot| spot.kind == numa::core::retouch::Kind::Patch) {
                            spot.heal = tool == RetouchTool::Heal;
                            write_spots(&state, spots, true);
                            refresh_retouch(&state);
                        }
                    }
                }
            }
        ));
    }
    tools
}

pub(super) fn build_retouch(state: &App) -> gtk::Box {
    let column = gtk::Box::new(gtk::Orientation::Vertical, 0);

    column.append(&tool_buttons(state));

    for (index, value) in [(0usize, 3.0), (1, 50.0), (2, 100.0)] {
        state.retouch.sliders[index].set_value(value);
        set_neutral(&state.retouch.sliders[index], value);
    }

    let names = [
        ("Size", Readout::Positive(1)),
        ("Feather", Readout::Positive(0)),
        ("Strength", Readout::Positive(0)),
    ];
    for (index, (name, readout)) in names.into_iter().enumerate() {
        let scale = &state.retouch.sliders[index];
        scale.connect_value_changed(glib::clone!(
            #[strong] state,
            move |scale| {
                if state.applying.get() {
                    return;
                }

                let Some(at) = state.retouch.selected_spot.get() else { return };
                let mut spots = current_spots(&state);
                let Some(spot) = spots.get_mut(at) else { return };
                let value = scale.value() as f32;
                match index {
                    0 => spot.radius = value / 100.0,
                    1 => spot.feather = value / 100.0,
                    _ => spot.opacity = value / 100.0,
                }
                write_spots(&state, spots, true);
            }
        ));
        let row = slider_row(state, name, scale, readout);
        if index == 0 {
            row.add_css_class("lead");
        }
        column.append(&row);
    }

    let finders = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    finders.set_halign(gtk::Align::End);
    finders.set_margin_top(6);
    let people = gtk::Button::with_label("Remove people");
    people.set_tooltip_text(Some("Take out everyone but the subject — the passers-by behind"));
    people.connect_clicked(glib::clone!(
        #[strong] state,
        move |_| remove_people(&state)
    ));
    let dust = gtk::Button::with_label("Find dust");
    dust.set_tooltip_text(Some("Heal the specks a dirty sensor leaves in skies and walls"));
    dust.connect_clicked(glib::clone!(
        #[strong] state,
        move |_| find_dust(&state)
    ));
    for button in [&people, &dust] {
        button.add_css_class("panel-action");
        finders.append(button);
    }
    column.append(&finders);

    let note = gtk::Label::new(Some(
        "Click the blemish, then drag to where the replacement should come from.",
    ));
    note.set_xalign(0.0);
    note.set_wrap(true);
    note.set_margin_top(4);
    note.set_margin_bottom(8);
    note.add_css_class("profile-note");
    column.append(&note);

    state.retouch.list.set_selection_mode(gtk::SelectionMode::None);
    state.retouch.list.add_css_class("boxed-list");
    column.append(&state.retouch.list);

    column
}

fn find_dust(state: &App) {
    let Some(frame) = mask_frame(state) else { return };
    let (width, height) = (frame.width() as f32, frame.height() as f32);
    let long_edge = width.max(height);
    let mut spots = current_spots(state);
    let found: Vec<_> = numa::render::dust::find(&frame)
        .into_iter()
        .filter(|speck| {
            spots.iter().all(|spot| {
                let gap = ((speck.at[0] - spot.at[0]) * width).hypot((speck.at[1] - spot.at[1]) * height);
                gap > (speck.radius + spot.radius) * long_edge
            })
        })
        .collect();
    if found.is_empty() {
        state.toast("No dust found — it only shows where the picture is smooth");
        return;
    }
    let count = found.len();
    spots.extend(found);
    write_spots(state, spots, true);
    refresh_retouch(state);
    state.retouch.area.queue_draw();
    state.toast(&match count {
        1 => "One speck of dust healed".to_string(),
        many => format!("{many} specks of dust healed"),
    });
}

fn remove_people(state: &App) {
    if !numa::render::remove::is_installed() || !numa::render::distractions::is_installed() {
        state.toast("Removing people needs LaMa and YOLOX — Preferences downloads them");
        return;
    }
    let Some(frame) = mask_frame(state) else { return };
    let people = numa::render::distractions::people(&frame).unwrap_or_default();
    if people.is_empty() {
        state.toast("Nobody to remove — only the subject, or nobody at all");
        return;
    }
    let count = people.len();
    let mut spots = current_spots(state);
    spots.extend(people);
    write_spots(state, spots, true);
    refresh_retouch(state);
    state.retouch.area.queue_draw();
    state.toast(&match count {
        1 => "One person removed".to_string(),
        many => format!("{many} people removed"),
    });
}

pub(super) fn build_face(state: &App) -> gtk::Box {
    let column = state.retouch.face_section.clone();
    column.set_margin_top(14);
    column.append(&section_header("Face"));

    let note = state.retouch.face_note.clone();
    note.set_xalign(0.0);
    note.set_wrap(true);
    note.set_margin_bottom(6);
    note.add_css_class("profile-note");
    column.append(&note);

    let names = [
        ("Spots", "Take out what is small, dark and temporary — a pimple, not a freckle"),
        ("Skin", "Flatten what is blotchy and keep what is texture"),
        ("Evenness", "The same for colour alone, which is where blotchiness lives"),
        ("Red eye", "Only where the red is out of proportion — a brown iris is not red eye"),
        ("Teeth", "Only where a mouth has something bright and yellow in it"),
    ];
    for (index, (name, tooltip)) in names.into_iter().enumerate() {
        let scale = &state.retouch.face_sliders[index];
        scale.set_tooltip_text(Some(tooltip));
        scale.connect_value_changed(glib::clone!(
            #[strong] state,
            move |_| {
                if !state.applying.get() {
                    read_face(&state);
                }
            }
        ));
        let row = slider_row(state, name, scale, Readout::Positive(0));
        if index == 0 {
            row.add_css_class("lead");
        }
        column.append(&row);
    }

    column.set_visible(false);
    column
}
