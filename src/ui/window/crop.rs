use super::*;

#[derive(Clone)]
pub(super) struct State {

    pub(super) area: gtk::DrawingArea,
    pub(super) rect: Rc<Cell<[f32; 4]>>,

    pub(super) ratio: Rc<Cell<Option<f32>>>,

    pub(super) landscape: Rc<Cell<bool>>,
    pub(super) ratios: Rc<RefCell<Vec<(gtk::ToggleButton, (f32, f32))>>>,

    pub(super) custom: Rc<Cell<(f32, f32)>>,

    pub(super) turned: Rc<Cell<Option<([f32; 4], [f32; 4])>>>,

    pub(super) fit_base: Rc<Cell<Option<([f32; 4], [f32; 4])>>>,
    pub(super) straighten: gtk::Scale,

    pub(super) perspective: Vec<gtk::Scale>,

    pub(super) guided: gtk::ToggleButton,
    pub(super) guide_lines: Rc<RefCell<Vec<numa::core::guided::Guide>>>,
    pub(super) controls: gtk::Box,

    pub(super) at_open: Rc<Cell<Option<([f32; 4], f32, f32)>>>,
}

impl State {
    pub(super) fn new() -> Self {
        Self {
            area: gtk::DrawingArea::new(),
            rect: Rc::new(Cell::new([0.0, 0.0, 1.0, 1.0])),
            ratio: Rc::new(Cell::new(None)),
            landscape: Rc::new(Cell::new(true)),
            ratios: Rc::new(RefCell::new(Vec::new())),
            custom: Rc::new(Cell::new((7.0, 5.0))),
            turned: Rc::new(Cell::new(None)),
            fit_base: Rc::new(Cell::new(None)),
            straighten: gtk::Scale::with_range(gtk::Orientation::Horizontal, -15.0, 15.0, 0.1),
            perspective: (0..3)
                .map(|_| gtk::Scale::with_range(gtk::Orientation::Horizontal, -100.0, 100.0, 1.0))
                .collect(),
            guided: gtk::ToggleButton::with_label("Guided"),
            guide_lines: Rc::new(RefCell::new(Vec::new())),
            controls: gtk::Box::new(gtk::Orientation::Vertical, 0),
            at_open: Rc::new(Cell::new(None)),
        }
    }
}

pub(super) fn build_crop_controls(state: &App) -> gtk::Box {
    let column = state.crop.controls.clone();
    column.set_orientation(gtk::Orientation::Vertical);
    column.set_spacing(6);
    column.set_visible(false);

    column.append(&section_header("Crop"));

    let (aspects, first) = aspect_buttons(state);
    column.append(&aspects);
    let free = first.clone();

    let turns = turn_buttons(state);
    column.append(&turns);

    let straighten = state.crop.straighten.clone();
    let row = slider_row(state, "Straighten", &straighten, Readout::Signed(1));
    row.add_css_class("lead");

    straighten.connect_value_changed(glib::clone!(
        #[strong] state,
        move |_| {
            if !state.applying.get() {
                commit_crop(&state);
            }
        }
    ));

    let level = gtk::Button::with_label("Auto");
    level.add_css_class("flat");
    level.add_css_class("caption");
    level.set_tooltip_text(Some("Level the photograph by its horizon and its verticals"));
    level.connect_clicked(glib::clone!(
        #[strong] state,
        move |_| auto_level(&state)
    ));
    if let Some(header) = row.first_child().and_downcast::<gtk::Box>() {
        header.insert_child_after(&level, header.first_child().as_ref());
    }
    column.append(&row);

    column.append(&section_header("Perspective"));
    for (index, (name, tooltip)) in [
        ("Vertical", "Verticals that converge — a camera pointed up or down"),
        ("Horizontal", "Horizontals that converge — a camera turned left or right"),
        ("Aspect", "What correcting either costs: the frame squashed along the axis it fixed"),
    ]
    .into_iter()
    .enumerate()
    {
        let scale = state.crop.perspective[index].clone();
        scale.set_tooltip_text(Some(tooltip));
        scale.connect_value_changed(glib::clone!(
            #[strong] state,
            move |_| {
                if !state.applying.get() {
                    read_perspective(&state);
                }
            }
        ));
        let row = slider_row(state, name, &scale, Readout::Signed(0));
        if index == 0 {
            row.add_css_class("lead");
        }
        column.append(&row);
    }

    let auto = gtk::Button::with_label("Auto");
    auto.set_tooltip_text(Some("Square up converging verticals; Horizontal is Guided's"));
    auto.set_margin_top(4);
    auto.connect_clicked(glib::clone!(
        #[strong] state,
        move |_| auto_perspective(&state)
    ));
    column.append(&guided_row(state, &auto));

    let finish = finish_buttons(state, &free);
    column.append(&finish);

    column
}

fn aspect_buttons(state: &App) -> (gtk::Box, Option<gtk::ToggleButton>) {
    let aspects = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    aspects.add_css_class("linked");
    let free = gtk::ToggleButton::with_label("Free");
    free.set_active(true);
    let choose = |button: &gtk::ToggleButton, ratio: Rc<dyn Fn(&App) -> Option<f32>>| {
        button.set_hexpand(true);
        button.connect_toggled(glib::clone!(
            #[strong] state,
            move |button| {
                if !button.is_active() || state.applying.get() {
                    return;
                }
                let ratio = ratio(&state);
                state.crop.ratio.set(ratio);
                if let Some(ratio) = ratio {
                    apply_aspect(&state, ratio);
                }
                commit_crop(&state);
            }
        ));
        aspects.append(button);
    };
    choose(&free, Rc::new(|_| None));
    press_again_to_turn(state, &free);
    for sides in [(1.0, 1.0), (5.0, 4.0), (3.0, 2.0), (16.0, 9.0)] {
        let button = gtk::ToggleButton::new();
        button.set_group(Some(&free));
        choose(&button, Rc::new(move |state| Some(ratio_of(sides, state.crop.landscape.get()))));
        press_again_to_turn(state, &button);
        state.crop.ratios.borrow_mut().push((button, sides));
    }
    let custom = gtk::ToggleButton::with_label("Custom");
    custom.set_group(Some(&free));
    custom.set_tooltip_text(Some("A ratio of your own"));
    choose(&custom, Rc::new(|state| Some(ratio_of(state.crop.custom.get(), state.crop.landscape.get()))));
    let popover = custom_popover(state, &custom);
    custom.connect_clicked(move |_| popover.popup());
    aspects.add_css_class("aspect-ratios");
    label_ratios(state);
    (aspects, Some(free))
}

fn press_again_to_turn(state: &App, button: &gtk::ToggleButton) {
    button.set_tooltip_text(Some("Press again for landscape or portrait"));
    let chosen = Rc::new(Cell::new(false));
    button.connect_toggled(glib::clone!(
        #[strong] chosen,
        move |button| {
            chosen.set(button.is_active());

            let chosen = chosen.clone();
            glib::idle_add_local_once(move || chosen.set(false));
        }
    ));
    button.connect_clicked(glib::clone!(
        #[strong] state,
        move |_| {
            if !chosen.replace(false) {
                swap_orientation(&state);
            }
        }
    ));
}

fn ratio_of((long, short): (f32, f32), landscape: bool) -> f32 {
    if landscape { long / short } else { short / long }
}

pub(super) fn label_ratios(state: &App) {
    let landscape = state.crop.landscape.get();
    for (button, (long, short)) in state.crop.ratios.borrow().iter() {
        let (first, second) = if landscape { (long, short) } else { (short, long) };
        button.set_label(&format!("{first}:{second}"));
    }
}

fn custom_popover(state: &App, custom: &gtk::ToggleButton) -> gtk::Popover {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    row.set_margin_top(6);
    row.set_margin_bottom(6);
    row.set_margin_start(6);
    row.set_margin_end(6);
    let sides: Vec<gtk::SpinButton> = (0..2).map(|_| gtk::SpinButton::with_range(1.0, 99.0, 1.0)).collect();
    row.append(&sides[0]);
    row.append(&gtk::Label::new(Some(":")));
    row.append(&sides[1]);
    let popover = gtk::Popover::new();
    popover.set_child(Some(&row));
    popover.set_parent(custom);

    popover.connect_show(glib::clone!(
        #[strong] state,
        #[strong] sides,
        move |_| {
            let (long, short) = state.crop.custom.get();
            let (wide, high) = if state.crop.landscape.get() { (long, short) } else { (short, long) };
            state.applying.set(true);
            sides[0].set_value(wide as f64);
            sides[1].set_value(high as f64);
            state.applying.set(false);
        }
    ));
    for side in &sides {
        side.connect_value_changed(glib::clone!(
            #[strong] state,
            #[strong] sides,
            #[weak] custom,
            move |_| {
                if state.applying.get() {
                    return;
                }

                let (wide, high) = (sides[0].value() as f32, sides[1].value() as f32);
                state.crop.custom.set((wide.max(high), wide.min(high)));
                if wide != high {
                    state.crop.landscape.set(wide > high);
                    label_ratios(&state);
                }
                if custom.is_active() {
                    state.crop.ratio.set(Some(wide / high));
                    apply_aspect(&state, wide / high);
                    commit_crop(&state);
                }
            }
        ));
    }
    popover
}

fn swap_orientation(state: &App) {
    state.crop.landscape.set(!state.crop.landscape.get());
    label_ratios(state);
    if let Some(ratio) = state.crop.ratio.get() {
        state.crop.ratio.set(Some(1.0 / ratio));
    }
    let now = state.crop.rect.get();
    let back = state.crop.turned.get().filter(|(turned, _)| *turned == now).map(|(_, before)| before);
    let next = back.unwrap_or_else(|| {
        let aspect = frame_aspect(state);
        let [x, y, w, h] = now;
        let (centre_x, centre_y) = (x + w / 2.0, y + h / 2.0);

        let (turned_w, turned_h) = (h / aspect, w * aspect);
        let scale = (1.0 / turned_w).min(1.0 / turned_h).min(1.0);
        let (turned_w, turned_h) = (turned_w * scale, turned_h * scale);
        [
            (centre_x - turned_w / 2.0).clamp(0.0, 1.0 - turned_w),
            (centre_y - turned_h / 2.0).clamp(0.0, 1.0 - turned_h),
            turned_w,
            turned_h,
        ]
    });
    state.crop.rect.set(next);
    state.crop.area.queue_draw();
    commit_crop(state);
    state.crop.turned.set(Some((state.crop.rect.get(), now)));
}

fn turn_buttons(state: &App) -> gtk::Box {
    let turns = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    turns.add_css_class("linked");

    {
        let degrees = 90.0f32;
        let button = gtk::Button::from_icon_name("object-rotate-right-symbolic");
        button.set_hexpand(true);
        button.set_tooltip_text(Some("Turn a quarter"));
        button.connect_clicked(glib::clone!(
            #[strong] state,
            move |_| {
                {
                    let mut open = state.open.borrow_mut();
                    let Some(photo) = open.as_mut() else { return };
                    let turned = photo.document.rotation() + degrees;
                    photo.document.set_rotation(turned);

                    let mut masks = photo.document.masks();
                    for mask in masks.iter_mut() {
                        mask.turn();
                    }
                    photo.document.set_masks(masks);
                    photo.view = None;
                }

                forget_model_frames(&state);

                state.crop.rect.set([0.0, 0.0, 1.0, 1.0]);
                state.crop.area.queue_draw();
                commit_crop(&state);

                state.crop.at_open.set(geometry_now(&state));
                refresh_masks(&state);
            }
        ));
        turns.append(&button);
    }

    for (icon, tooltip, vertical) in [
        ("object-flip-horizontal-symbolic", "Flip left to right", false),
        ("object-flip-vertical-symbolic", "Flip top to bottom", true),
    ] {
        let button = gtk::Button::from_icon_name(icon);
        button.set_hexpand(true);
        button.set_tooltip_text(Some(tooltip));
        button.connect_clicked(glib::clone!(
            #[strong] state,
            move |_| flip_frame(&state, vertical)
        ));
        turns.append(&button);
    }
    turns
}

fn finish_buttons(state: &App, free: &Option<gtk::ToggleButton>) -> gtk::Box {
    let finish = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    finish.set_margin_top(10);

    let reset = gtk::Button::with_label("Reset");
    reset.set_hexpand(true);
    reset.set_tooltip_text(Some("The whole frame again"));
    reset.connect_clicked(glib::clone!(
        #[strong] state,
        #[strong] free,
        move |_| {

            state.crop.ratio.set(None);
            if let Some(free) = &free {
                state.applying.set(true);
                free.set_active(true);
                state.applying.set(false);
            }
            state.crop.rect.set([0.0, 0.0, 1.0, 1.0]);
            state.crop.straighten.set_value(0.0);
            state.applying.set(true);
            for slider in &state.crop.perspective {
                slider.set_value(0.0);
            }
            state.applying.set(false);
            {
                let mut open = state.open.borrow_mut();
                if let Some(photo) = open.as_mut() {
                    photo.document.set_perspective(Perspective::default());
                    photo.view = None;
                }
            }
            state.crop.area.queue_draw();
            commit_crop(&state);
        }
    ));
    finish.append(&reset);

    let done = gtk::Button::with_label("Done");
    done.set_hexpand(true);
    done.add_css_class("suggested-action");
    done.set_tooltip_text(Some("Keep this crop and leave the tool"));
    done.connect_clicked(glib::clone!(
        #[strong] state,
        move |_| {
            commit_crop(&state);
            show_panel_tab(&state, "light");
        }
    ));
    finish.append(&done);
    finish
}
