use super::*;

pub(super) fn section_header(title: &str) -> gtk::Label {
    let label = gtk::Label::new(Some(&title.to_uppercase()));
    label.set_xalign(0.0);

    label.set_margin_top(18);
    label.set_margin_bottom(2);
    label.add_css_class("section-header");
    label
}

pub(super) fn shift_moves_ten(widget: &impl IsA<gtk::Widget>, adjustment: &gtk::Adjustment) {

    adjustment.set_page_increment(adjustment.step_increment() * 10.0);

    let keys = gtk::EventControllerKey::new();

    keys.set_propagation_phase(gtk::PropagationPhase::Capture);
    let adjustment = adjustment.clone();
    keys.connect_key_pressed(move |_, key, _, modifiers| {
        if !modifiers.contains(gtk::gdk::ModifierType::SHIFT_MASK) {
            return glib::Propagation::Proceed;
        }
        use gtk::gdk::Key;
        let by = match key {
            Key::Up | Key::Right | Key::KP_Up | Key::KP_Right => adjustment.page_increment(),
            Key::Down | Key::Left | Key::KP_Down | Key::KP_Left => -adjustment.page_increment(),
            _ => return glib::Propagation::Proceed,
        };
        let moved = (adjustment.value() + by).clamp(adjustment.lower(), adjustment.upper());
        adjustment.set_value(moved);
        glib::Propagation::Stop
    });
    widget.as_ref().add_controller(keys);
}

thread_local! {

    pub(super) static NEUTRALS: RefCell<HashMap<usize, f64>> = RefCell::new(HashMap::new());

    pub(super) static PAINTERS: RefCell<HashMap<usize, (String, String)>> =
        RefCell::new(HashMap::new());

    pub(super) static ROWS: RefCell<Vec<(gtk::Scale, gtk::Label, Readout)>> =
        const { RefCell::new(Vec::new()) };

    pub(super) static TRACKS: gtk::CssProvider = {
        let provider = gtk::CssProvider::new();
        if let Some(display) = gtk::gdk::Display::default() {
            gtk::style_context_add_provider_for_display(
                &display,
                &provider,
                gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
            );
        }
        provider
    };

    pub(super) static REGISTERED: RefCell<Vec<gtk::Scale>> = const { RefCell::new(Vec::new()) };

    static RELOAD_QUEUED: Cell<bool> = const { Cell::new(false) };
}

pub(super) fn set_neutral(scale: &gtk::Scale, value: f64) {
    NEUTRALS.with(|neutrals| neutrals.borrow_mut().insert(scale.as_ptr() as usize, value));
    repaint(scale);
}

pub(super) fn neutral_of(scale: &gtk::Scale) -> Option<f64> {
    NEUTRALS.with(|neutrals| neutrals.borrow().get(&(scale.as_ptr() as usize)).copied())
}

fn rule_for(scale: &gtk::Scale, class: &str, neutral: f64) -> String {
    let adjustment = scale.adjustment();
    let (low, high) = (adjustment.lower(), adjustment.upper());
    let span = (high - low).max(f64::EPSILON);
    let at = |value: f64| ((value - low) / span * 100.0).clamp(0.0, 100.0);
    let (rest, now) = (at(neutral), at(scale.value()));
    let (from, to) = (rest.min(now), rest.max(now));

    let fill = match (to - from).abs() < 0.2 {
        true => "transparent",
        false => "@accent_bg_color",
    };

    let mut rules = String::new();
    if !scale.has_css_class("mixer-track") && !scale.has_css_class("hue-slider") {
        rules = format!(
        "scale.{class} trough {{ background-image: linear-gradient(to right, \
         transparent {from:.3}%, {fill} {from:.3}%, {fill} {to:.3}%, \
         transparent {to:.3}%); }}\n"
        );
    }
    if low < 0.0 && high > 0.0 {
        let zero = at(0.0);
        rules += &format!(
            "scale.{class} {{ background-image: linear-gradient(rgba(255,255,255,0.32), \
             rgba(255,255,255,0.32)); background-size: 1px 8px; \
             background-position: {zero:.3}% center; background-repeat: no-repeat; }}\n"
        );
    }
    rules
}

pub(super) fn repaint(scale: &gtk::Scale) {
    let key = scale.as_ptr() as usize;
    let Some(class) = PAINTERS.with(|painters| painters.borrow().get(&key).map(|(class, _)| class.clone()))
    else {
        return;
    };
    let rule = rule_for(scale, &class, neutral_of(scale).unwrap_or(0.0));
    let changed = PAINTERS.with(|painters| match painters.borrow_mut().get_mut(&key) {
        Some(entry) if entry.1 != rule => {
            entry.1 = rule;
            true
        }
        _ => false,
    });
    if !changed || RELOAD_QUEUED.with(|queued| queued.replace(true)) {
        return;
    }

    glib::idle_add_local_full(glib::Priority::HIGH_IDLE, || {
        RELOAD_QUEUED.with(|queued| queued.set(false));
        let sheet = PAINTERS.with(|painters| {
            painters.borrow().values().map(|(_, rule)| rule.as_str()).collect::<String>()
        });
        TRACKS.with(|provider| provider.load_from_string(&sheet));
        glib::ControlFlow::Break
    });
}

pub(super) fn slider_row(state: &App, name: &str, scale: &gtk::Scale, readout: Readout) -> gtk::Box {
    REGISTERED.with(|registered| registered.borrow_mut().push(scale.clone()));
    let row = gtk::Box::new(gtk::Orientation::Vertical, 0);
    row.add_css_class("slider-row");

    let header = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let title = gtk::Label::new(Some(name));
    title.set_xalign(0.0);
    title.set_hexpand(true);
    title.add_css_class("slider-name");

    let value = gtk::Label::new(Some(&readout.format(scale.value())));
    value.set_xalign(1.0);
    value.set_width_chars(6);
    value.add_css_class("slider-value");

    ROWS.with(|rows| rows.borrow_mut().push((scale.clone(), value.clone(), readout)));

    header.append(&title);
    header.append(&value);

    scale.set_hexpand(true);
    shift_moves_ten(scale, &scale.adjustment());

    scale.set_draw_value(false);

    scale.set_has_origin(false);

    let class = REGISTERED.with(|registered| format!("track-{}", registered.borrow().len()));
    scale.add_css_class(&class);
    PAINTERS.with(|painters| {
        painters.borrow_mut().insert(scale.as_ptr() as usize, (class, String::new()))
    });

    repaint(scale);

    scale.connect_value_changed(glib::clone!(
        #[strong] state,
        #[weak] value,
        move |scale| {
            value.set_text(&readout.format(scale.value()));
            repaint(scale);
            adjustments_changed(&state);
        }
    ));

    connect_reset(state, scale, &value, readout);

    connect_wheel(scale);

    row.append(&header);
    row.append(scale);
    row
}

fn connect_reset(state: &App, scale: &gtk::Scale, value: &gtk::Label, readout: Readout) {
    let reset = glib::clone!(
        #[strong] state,
        #[weak] scale,
        move || {
            let original = match readout {
                Readout::Kelvin => state
                    .open
                    .borrow()
                    .as_ref()
                    .map_or(5500.0, |photo| photo.as_shot.temperature as f64),

                _ if neutral_of(&scale).is_some() => neutral_of(&scale).unwrap_or_default(),

                Readout::Signed(_) | Readout::Positive(_) | Readout::Degrees | Readout::OffsetKelvin => 0.0,

                Readout::Radius => 1.0,
                Readout::BrushSize => brush_travel(DEFAULT_BRUSH),
                Readout::Middle => 50.0,
            };
            scale.set_value(original);
        }
    );

    let right_click = gtk::GestureClick::new();
    right_click.set_button(gtk::gdk::BUTTON_SECONDARY);
    right_click.connect_pressed(glib::clone!(
        #[strong] reset,
        move |_, _, _, _| reset()
    ));
    scale.add_controller(right_click);

    let double_click = gtk::GestureClick::new();
    double_click.set_button(gtk::gdk::BUTTON_PRIMARY);

    double_click.set_propagation_phase(gtk::PropagationPhase::Capture);
    double_click.connect_pressed(glib::clone!(
        #[strong] reset,
        move |gesture, presses, _, _| {
            if presses == 2 {
                gesture.set_state(gtk::EventSequenceState::Claimed);
                reset();
            }
        }
    ));
    scale.add_controller(double_click);

    let on_value = gtk::GestureClick::new();
    on_value.set_button(0);
    on_value.connect_pressed(glib::clone!(
        #[strong] reset,
        move |gesture, presses, _, _| {
            let button = gesture.current_button();
            if button == gtk::gdk::BUTTON_SECONDARY || (button == gtk::gdk::BUTTON_PRIMARY && presses == 2) {
                reset();
            }
        }
    ));
    value.add_controller(on_value);
    value.set_tooltip_text(Some("Right-click to reset"));
    scale.set_tooltip_text(Some("Right-click to reset"));
}

fn connect_wheel(scale: &gtk::Scale) {
    let wheel = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::VERTICAL);
    wheel.set_propagation_phase(gtk::PropagationPhase::Capture);
    wheel.connect_scroll(move |controller, _, dy| {

        let Some(scroller) = controller
            .widget()
            .and_then(|scale| scale.ancestor(gtk::ScrolledWindow::static_type()))
            .and_downcast::<gtk::ScrolledWindow>()
        else {
            return glib::Propagation::Proceed;
        };
        let adjustment = scroller.vadjustment();
        let step = adjustment.step_increment().max(24.0);
            let target = (adjustment.value() + dy * step)
                .clamp(0.0, (adjustment.upper() - adjustment.page_size()).max(0.0));
        adjustment.set_value(target);
        glib::Propagation::Stop
    });
    scale.add_controller(wheel);
}

pub(super) fn check_rows() -> Vec<String> {
    ROWS.with(|rows| {
        rows.borrow()
            .iter()
            .filter_map(|(scale, label, readout)| {
                let wanted = readout.format(scale.value());
                let shown = label.text().to_string();
                let adjustment = scale.adjustment();
                let (low, high) = (adjustment.lower(), adjustment.upper());
                let inside = scale.value() >= low && scale.value() <= high;
                match (shown == wanted, inside) {
                    (true, true) => None,
                    (false, _) => Some(format!("reads {shown:?}, value formats to {wanted:?}")),
                    (_, false) => Some(format!("value {} outside {low}..{high}", scale.value())),
                }
            })
            .collect()
    })
}
