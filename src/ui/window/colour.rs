use super::*;

#[derive(Clone)]
pub(super) struct State {

    pub(super) profile_picker: gtk::DropDown,

    pub(super) picking_band: Rc<Cell<bool>>,
    pub(super) band_pipette: gtk::ToggleButton,

    pub(super) white_pipette: gtk::ToggleButton,
    pub(super) picking_white: Rc<Cell<bool>>,

    pub(super) mixer_swatches: Rc<RefCell<Vec<gtk::ToggleButton>>>,

    pub(super) picking_point: Rc<Cell<bool>>,
    pub(super) point_pipette: gtk::ToggleButton,
    pub(super) point_swatches: gtk::Box,
    pub(super) point_selected: Rc<Cell<usize>>,
    pub(super) point_sliders: Vec<gtk::Scale>,
    pub(super) point_controls: gtk::Box,
    pub(super) point_show: gtk::ToggleButton,
    pub(super) profile_label: gtk::Label,
    pub(super) mixer_sliders: Vec<gtk::Scale>,
    pub(super) mixer_band: Rc<Cell<usize>>,

    pub(super) monochrome: gtk::Switch,

    pub(super) mask_temperature: gtk::Scale,
    pub(super) mask_tint: gtk::Scale,

    pub(super) mask_hue: gtk::Scale,
    pub(super) mask_colour_strength: gtk::Scale,
}

impl State {
    pub(super) fn new() -> Self {
        Self {
            profile_picker: gtk::DropDown::from_strings(&[]),
            picking_band: Rc::new(Cell::new(false)),
            band_pipette: gtk::ToggleButton::new(),
            white_pipette: gtk::ToggleButton::new(),
            picking_white: Rc::new(Cell::new(false)),
            mixer_swatches: Rc::new(RefCell::new(Vec::new())),
            picking_point: Rc::new(Cell::new(false)),
            point_pipette: gtk::ToggleButton::new(),
            point_swatches: gtk::Box::new(gtk::Orientation::Horizontal, 6),
            point_selected: Rc::new(Cell::new(0)),
            point_sliders: [(-100.0, 100.0), (-100.0, 100.0), (-100.0, 100.0), (0.0, 100.0)]
            .into_iter()
            .map(|(low, high)| gtk::Scale::with_range(gtk::Orientation::Horizontal, low, high, 1.0))
            .collect(),
            point_controls: gtk::Box::new(gtk::Orientation::Vertical, 0),
            point_show: gtk::ToggleButton::new(),
            profile_label: gtk::Label::new(None),
            mixer_sliders: (0..3)
            .map(|_| gtk::Scale::with_range(gtk::Orientation::Horizontal, -100.0, 100.0, 1.0))
            .collect(),
            mixer_band: Rc::new(Cell::new(0)),
            monochrome: gtk::Switch::new(),

            mask_temperature: gtk::Scale::with_range(gtk::Orientation::Horizontal, -2000.0, 2000.0, 10.0),
            mask_hue: gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 360.0, 1.0),
            mask_colour_strength: gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 100.0, 1.0),
            mask_tint: gtk::Scale::with_range(
                gtk::Orientation::Horizontal,
                -color::MAX_TINT as f64,
                color::MAX_TINT as f64,
                1.0,
            ),
        }
    }
}

pub(super) fn build_mixer(state: &App) -> gtk::Box {
    let column = gtk::Box::new(gtk::Orientation::Vertical, 0);

    let colours = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    colours.add_css_class("mixer-bands");
    colours.set_margin_bottom(12);
    colours.set_halign(gtk::Align::Start);
    let mut first: Option<gtk::ToggleButton> = None;
    for (band, (name, _)) in BANDS.iter().enumerate() {
        let swatch = gtk::ToggleButton::new();

        swatch.set_halign(gtk::Align::Center);
        swatch.set_valign(gtk::Align::Center);
        swatch.set_tooltip_text(Some(name));
        swatch.add_css_class(&format!("band-{band}"));
        match &first {
            None => {
                swatch.set_active(true);
                first = Some(swatch.clone());
            }
            Some(first) => swatch.set_group(Some(first)),
        }
        swatch.connect_toggled(glib::clone!(
            #[strong] state,
            move |swatch| {
                if !swatch.is_active() || state.applying.get() {
                    return;
                }
                state.colour.mixer_band.set(band);

                tint_mixer(&state);
                write_mixer(&state);
            }
        ));
        state.colour.mixer_swatches.borrow_mut().push(swatch.clone());
        colours.append(&swatch);
    }

    let pipette = state.colour.point_pipette.clone();

    pipette.add_css_class("band-pipette");
    colours.set_valign(gtk::Align::Center);
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    row.append(&colours);
    row.append(&pipette);
    row.set_margin_bottom(12);
    colours.set_margin_bottom(0);

    column.append(&row);

    for (channel, name) in ["Hue", "Saturation", "Luminance"].into_iter().enumerate() {
        let scale = &state.colour.mixer_sliders[channel];
        scale.add_css_class("mixer-track");
        scale.add_css_class(match channel {
            0 => "track-hue",
            1 => "track-saturation",
            _ => "track-luminance",
        });
        scale.connect_value_changed(glib::clone!(
            #[strong] state,
            move |_| {
                if !state.applying.get() {
                    read_mixer(&state);
                }
            }
        ));
        column.append(&slider_row(state, name, scale, Readout::Signed(0)));
    }

    tint_mixer(state);
    column
}

fn build_monochrome(state: &App) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    row.set_margin_top(6);
    row.set_margin_bottom(6);
    let title = gtk::Label::new(Some("Black & white"));
    title.set_xalign(0.0);
    title.set_hexpand(true);
    title.add_css_class("slider-name");
    let switch = state.colour.monochrome.clone();
    switch.set_valign(gtk::Align::Center);
    switch.update_property(&[gtk::accessible::Property::Label("Black & white")]);
    row.set_tooltip_text(Some("The mixer sets how light each colour's grey is"));
    row.append(&title);
    row.append(&switch);
    switch.connect_active_notify(glib::clone!(
        #[strong] state,
        move |switch| {
            if state.applying.get() {
                return;
            }
            {
                let mut open = state.open.borrow_mut();
                let Some(photo) = open.as_mut() else { return };
                let mixer = Mixer { monochrome: switch.is_active(), ..mixer_in(&state, &photo.document) };
                set_mixer_in(&state, &mut photo.document, mixer);
                photo.view = None;
            }
            write_mixer(&state);
            request_render(&state);
            schedule_history_push(&state);
        }
    ));
    row
}

pub(super) fn build_point_colours(state: &App) -> gtk::Box {
    let column = gtk::Box::new(gtk::Orientation::Vertical, 0);

    let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    row.set_margin_bottom(12);
    let pipette = state.colour.point_pipette.clone();
    pipette.set_icon_name("color-select-symbolic");
    pipette.set_tooltip_text(Some("Pick a colour from the photograph to adjust"));
    pipette.add_css_class("flat");
    pipette.set_valign(gtk::Align::Center);
    pipette.connect_toggled(glib::clone!(
        #[strong] state,
        move |button| {
            if state.applying.get() {
                return;
            }
            state.colour.picking_point.set(button.is_active());
            if button.is_active() {
                disarm_pipettes(&state, "point");
                state.toast("Click the photograph to pick a colour");
            }
            arm_band_pipette(&state);
        }
    ));
    state.colour.point_swatches.set_valign(gtk::Align::Center);
    row.append(&state.colour.point_swatches);

    column.append(&row);

    let controls = state.colour.point_controls.clone();
    controls.add_css_class("point-section");
    let names = [
        ("Hue", Readout::Signed(0)),
        ("Saturation", Readout::Signed(0)),
        ("Luminance", Readout::Signed(0)),
        ("Range", Readout::Middle),
    ];
    for (index, (name, readout)) in names.into_iter().enumerate() {
        let scale = &state.colour.point_sliders[index];

        if index == 0 {
            scale.add_css_class("mixer-track");
            scale.add_css_class("point-hue");
        }
        scale.connect_value_changed(glib::clone!(
            #[strong] state,
            move |_| {
                if !state.applying.get() {
                    read_point_colours(&state);
                }
            }
        ));
        controls.append(&slider_row(state, name, scale, readout));
    }

    let actions = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    let show = state.colour.point_show.clone();
    show.set_label("Show affected area");
    show.set_tooltip_text(Some("Grey out everything this colour does not reach"));
    show.connect_toggled(glib::clone!(
        #[strong] state,
        move |_| request_render(&state)
    ));
    let remove = gtk::Button::from_icon_name("user-trash-symbolic");
    remove.add_css_class("flat");
    remove.set_tooltip_text(Some("Remove this colour"));
    remove.connect_clicked(glib::clone!(
        #[strong] state,
        move |_| {
            {
                let mut open = state.open.borrow_mut();
                let Some(photo) = open.as_mut() else { return };
                let mut points = points_in(&state, &photo.document);
                let selected = state.colour.point_selected.get();
                if selected < points.points.len() {
                    points.points.remove(selected);
                }
                state.colour.point_selected.set(selected.saturating_sub(1));
                set_points_in(&state, &mut photo.document, points);
            }
            write_point_colours(&state);
            request_render(&state);
            schedule_history_push(&state);
        }
    ));
    actions.append(&show);
    actions.append(&remove);
    controls.append(&actions);
    column.append(&controls);

    write_point_colours(state);
    column
}

pub(super) fn build_colour(
    state: &App,
    all: &[(&'static str, &gtk::Scale, Readout); SLIDER_COUNT],
    global_only: &dyn Fn(&gtk::Widget),
    mask_only: &dyn Fn(&gtk::Widget),
) -> gtk::Box {
    let colour = page_column();

    colour.add_css_class("quiet");
    let balance_header = section_header("White balance");
    global_only(balance_header.as_ref());
    colour.append(&balance_header);
    for (name, scale, readout) in &all[..2] {

        let row = slider_row(state, name, scale, *readout);
        if *name == "Temperature" {
            row.add_css_class("lead");
        }
        global_only(row.as_ref());
        colour.append(&row);
    }
    let white = white_pipette(state);
    global_only(white.as_ref());
    colour.append(&white);

    let profile_header = section_header("Camera profile");
    global_only(profile_header.as_ref());
    colour.append(&profile_header);
    let profile = build_profile_picker(state);
    global_only(profile.as_ref());
    colour.append(&profile);

    let mask_header = section_header("White balance");
    mask_only(mask_header.as_ref());
    colour.append(&mask_header);
    for (name, scale, readout, tooltip) in [
        (
            "Temperature",
            &state.colour.mask_temperature,
            Readout::OffsetKelvin,
            "Warmer or cooler than the rest of the photograph",
        ),
        (
            "Tint",
            &state.colour.mask_tint,
            Readout::Signed(0),
            "Greener or more magenta than the rest of the photograph",
        ),
    ] {
        let row = slider_row(state, name, scale, readout);
        row.set_tooltip_text(Some(tooltip));
        if name == "Temperature" {
            row.add_css_class("lead");
        }

        set_neutral(scale, 0.0);
        mask_only(row.as_ref());
        colour.append(&row);
    }

    colour.append(&section_header("Colour"));
    state.colour.mask_hue.add_css_class("hue-slider");
    for (name, scale, readout) in [
        ("Hue", &state.colour.mask_hue, Readout::Degrees),
        ("Strength", &state.colour.mask_colour_strength, Readout::Positive(0)),
    ] {
        let row = slider_row(state, name, scale, readout);
        set_neutral(scale, 0.0);
        mask_only(row.as_ref());
        colour.append(&row);
    }

    let monochrome = build_monochrome(state);
    colour.append(&monochrome);
    for (name, scale, readout) in &all[9..11] {
        let row = slider_row(state, name, scale, *readout);
        if *name == "Vibrance" {
            row.add_css_class("lead");
        }
        colour.append(&row);
    }

    let mixer_header = section_header("Mixer");
    let mixer = build_mixer(state);
    let point = build_point_colours(state);
    colour.append(&mixer_header);
    colour.append(&mixer);
    colour.append(&point);

    colour
}

fn white_pipette(state: &App) -> gtk::ToggleButton {
    let white = state.colour.white_pipette.clone();
    let inside = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let pipette = gtk::Image::from_icon_name("color-select-symbolic");
    pipette.set_pixel_size(16);
    inside.append(&pipette);
    inside.append(&gtk::Label::new(Some("Pick a neutral")));
    white.set_child(Some(&inside));
    white.add_css_class("panel-action");
    white.set_tooltip_text(Some("Click something in the photograph that should be grey or white"));

    white.set_halign(gtk::Align::End);
    white.set_margin_top(6);
    white.connect_toggled(glib::clone!(
        #[strong] state,
        move |button| {
            if state.applying.get() {
                return;
            }
            state.colour.picking_white.set(button.is_active());
            if button.is_active() {
                disarm_pipettes(&state, "white");
            }
            arm_band_pipette(&state);
        }
    ));
    white
}

pub(super) fn build_profile_picker(state: &App) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Vertical, 4);
    row.set_margin_bottom(6);

    state.colour.profile_picker.set_hexpand(true);
    narrow_dropdown(&state.colour.profile_picker);
    state.colour.profile_picker.connect_selected_notify(glib::clone!(
        #[strong] state,
        move |picker| {
            if state.applying.get() {
                return;
            }
            let Some(chosen) = profile_choices(&state).get(picker.selected() as usize).cloned()
            else {
                return;
            };

            {
                let mut open = state.open.borrow_mut();
                let Some(photo) = open.as_mut() else { return };
                if photo.document.colour_profile == chosen {
                    return;
                }
                photo.document.colour_profile = chosen;

                photo.inputs = render_inputs(&photo.document);
                photo.working = Arc::new(render::to_working_space(&photo.document, &*photo.proxy, &photo.inputs));
                photo.full_working = None;
                photo.full_working_key = None;

                photo.draft = None;
                photo.view = None;
            }

            request_render(&state);
            schedule_history_push(&state);
        }
    ));

    row.append(&state.colour.profile_picker);

    state.colour.profile_label.set_xalign(0.0);
    state.colour.profile_label.set_wrap(true);
    state.colour.profile_label.add_css_class("profile-note");
    row.append(&state.colour.profile_label);

    row
}

fn profile_choices(state: &App) -> Vec<Option<String>> {
    let mut choices = vec![None, Some(render::NO_COLOUR_PROFILE.to_string())];
    choices.extend(camera_profiles(state).into_iter().map(|(name, _)| Some(name)));
    choices
}

fn source_label(source: dcp::Source) -> &'static str {
    match source {
        dcp::Source::Numa => "Numa",
        dcp::Source::RawTherapee => "RawTherapee",
        dcp::Source::Yours => "yours",
    }
}

fn camera_profiles(state: &App) -> Vec<(String, dcp::Source)> {
    let open = state.open.borrow();
    let Some(summary) = open.as_ref().and_then(|photo| photo.summary.as_ref()) else {
        return Vec::new();
    };
    dcp::names_for_camera(&summary.make, &summary.model)
}

pub(super) fn refresh_profile_picker(state: &App) {
    let (chosen, automatic) = {
        let open = state.open.borrow();
        let Some(photo) = open.as_ref() else { return };
        (
            photo.document.colour_profile.clone(),
            photo.proxy.rendering.as_ref().map(|profile| profile.name.clone()),
        )
    };

    let profiles = camera_profiles(state);
    let source_of = |name: &str| profiles.iter().find(|(n, _)| n == name).map(|(_, source)| *source);
    let mut labels = vec![
        match &automatic {
            Some(name) => match source_of(name) {
                Some(source) => format!("Automatic — {name} · {}", source_label(source)),
                None => format!("Automatic — {name}"),
            },
            None => "Automatic — the camera's matrix".to_string(),
        },
        "Camera matrix only".to_string(),
    ];
    labels.extend(profiles.iter().map(|(name, source)| format!("{name} · {}", source_label(*source))));
    let model = gtk::StringList::new(&labels.iter().map(String::as_str).collect::<Vec<_>>());

    state.applying.set(true);
    state.colour.profile_picker.set_model(Some(&model));
    let index = profile_choices(state)
        .iter()
        .position(|choice| *choice == chosen)
        .unwrap_or(0);
    state.colour.profile_picker.set_selected(index as u32);
    state.applying.set(false);

    let showing = chosen.clone().or(automatic);
    let from = match showing.as_deref() {
        None => "From the colour matrix in the raw file.".to_string(),
        Some(name) if name == render::NO_COLOUR_PROFILE => "From the colour matrix in the raw file.".to_string(),
        Some(name) => match source_of(name) {
            Some(dcp::Source::Numa) => "Numa's own profile, fitted from public-domain raws.".to_string(),
            Some(dcp::Source::RawTherapee) => "A RawTherapee profile (GPL-3.0).".to_string(),
            Some(dcp::Source::Yours) | None => "One of your own profiles.".to_string(),
        },
    };
    let more = dcp::profiles_dir().map(|dir| format!(" More: .dcp files in {}", dir.display())).unwrap_or_default();
    state.colour.profile_label.set_text(&format!("{from}{more}"));
}
