use super::*;

pub(super) fn section_row(title: &str, ends: &[&gtk::Widget]) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 2);
    let label = section_header(title);
    label.set_hexpand(true);
    row.append(&label);
    for end in ends {
        end.set_valign(gtk::Align::End);
        end.add_css_class("label-action");
        row.append(*end);
    }
    name_icon_buttons(row.upcast_ref());
    row
}

pub(super) fn more_menu(sections: &[&[(&str, &str)]]) -> gtk::MenuButton {
    let menu = gio::Menu::new();
    for items in sections {
        let section = gio::Menu::new();
        for (label, action) in *items {
            section.append(Some(label), Some(action));
        }
        menu.append_section(None, &section);
    }
    let button = gtk::MenuButton::new();
    button.set_icon_name("view-more-horizontal-symbolic");
    button.set_tooltip_text(Some("More"));
    button.add_css_class("flat");
    button.set_menu_model(Some(&menu));
    button
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

    pub(super) static PAINTERS: RefCell<HashMap<usize, Painter>> = RefCell::new(HashMap::new());

    pub(super) static ROWS: RefCell<Vec<(gtk::Scale, gtk::Label, Readout)>> =
        const { RefCell::new(Vec::new()) };

    pub(super) static REGISTERED: RefCell<Vec<gtk::Scale>> = const { RefCell::new(Vec::new()) };

    pub(super) static RESETS: RefCell<HashMap<usize, Rc<dyn Fn()>>> = RefCell::new(HashMap::new());
}

pub(super) struct Painter {
    rule: (String, String),
    fill: gtk::CssProvider,
    tick: gtk::CssProvider,
}

impl Painter {
    #[allow(deprecated)]
    fn on(scale: &gtk::Scale) -> Painter {
        let (fill, tick) = (gtk::CssProvider::new(), gtk::CssProvider::new());

        let priority = gtk::STYLE_PROVIDER_PRIORITY_APPLICATION + 1;
        scale.style_context().add_provider(&tick, priority);
        let mut child = scale.first_child();
        while let Some(widget) = child {
            if widget.css_name() == "trough" {
                widget.style_context().add_provider(&fill, priority);
            }
            child = widget.next_sibling();
        }
        Painter { rule: Default::default(), fill, tick }
    }
}

pub(super) fn set_neutral(scale: &gtk::Scale, value: f64) {
    NEUTRALS.with(|neutrals| neutrals.borrow_mut().insert(scale.as_ptr() as usize, value));
    repaint(scale);
}

pub(super) fn neutral_of(scale: &gtk::Scale) -> Option<f64> {
    NEUTRALS.with(|neutrals| neutrals.borrow().get(&(scale.as_ptr() as usize)).copied())
}

pub(super) fn fill_span(low: f64, high: f64, neutral: f64, value: f64) -> (f64, f64, bool) {
    let span = (high - low).max(f64::EPSILON);
    let at = |value: f64| ((value - low) / span * 100.0).clamp(0.0, 100.0);
    let (rest, now) = (at(neutral), at(value));
    let moved = (rest - now).abs() >= 0.2;
    match neutral >= high {
        true => (0.0, now, moved),
        false => (rest.min(now), rest.max(now), moved),
    }
}

fn rule_for(scale: &gtk::Scale, neutral: f64) -> (String, String) {
    let adjustment = scale.adjustment();
    let (low, high) = (adjustment.lower(), adjustment.upper());
    let (from, to, moved) = fill_span(low, high, neutral, scale.value());

    let fill = match (moved, neutral >= high) {
        (true, _) => "@numa_accent",
        (false, true) => "@numa_label",
        (false, false) => "transparent",
    };

    let mut rules = (String::new(), String::new());
    if !scale.has_css_class("mixer-track") && !scale.has_css_class("hue-slider") {
        rules.0 = format!(
        "trough {{ background-image: linear-gradient(to right, \
         transparent {from:.3}%, {fill} {from:.3}%, {fill} {to:.3}%, \
         transparent {to:.3}%); }}\n"
        );
    }
    if low < 0.0 && high > 0.0 {
        let zero = -low / (high - low) * 100.0;
        rules.1 = format!(
            "scale {{ background-image: linear-gradient(@numa_rest_tick, @numa_rest_tick); \
             background-size: 1px 8px; \
             background-position: {zero:.3}% center; background-repeat: no-repeat; }}\n"
        );
    }
    rules
}

pub(super) fn repaint(scale: &gtk::Scale) {
    let key = scale.as_ptr() as usize;
    let rule = rule_for(scale, neutral_of(scale).unwrap_or(0.0));
    PAINTERS.with(|painters| {
        let mut painters = painters.borrow_mut();
        let Some(painter) = painters.get_mut(&key) else { return };
        if painter.rule.0 != rule.0 {
            painter.fill.load_from_string(&rule.0);
        }
        if painter.rule.1 != rule.1 {
            painter.tick.load_from_string(&rule.1);
        }
        painter.rule = rule;
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

    value.set_halign(gtk::Align::End);
    value.add_css_class("slider-value");

    ROWS.with(|rows| rows.borrow_mut().push((scale.clone(), value.clone(), readout)));

    header.append(&title);
    header.append(&value);

    scale.update_relation(&[gtk::accessible::Relation::LabelledBy(&[title.upcast_ref()])]);

    scale.set_hexpand(true);

    scale.update_relation(&[gtk::accessible::Relation::LabelledBy(&[title.upcast_ref()])]);
    shift_moves_ten(scale, &scale.adjustment());

    scale.set_draw_value(false);

    scale.set_has_origin(false);

    PAINTERS.with(|painters| painters.borrow_mut().insert(scale.as_ptr() as usize, Painter::on(scale)));

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
    typed_value(&header, scale, &value, readout);

    connect_wheel(scale);

    hud::follow(scale, name, readout);

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

            glide(&scale, &scale.adjustment(), original);
        }
    );
    RESETS.with(|resets| resets.borrow_mut().insert(scale.as_ptr() as usize, Rc::new(reset.clone())));
    on_reset(scale, Some(value), reset);
}

fn on_reset(scale: &gtk::Scale, value: Option<&gtk::Label>, reset: impl Fn() + Clone + 'static) {
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
    scale.set_tooltip_text(Some("Right-click to reset"));

    let Some(value) = value else { return };
    let on_value = gtk::GestureClick::new();
    on_value.set_button(0);
    on_value.connect_pressed(move |gesture, presses, _, _| {
        let button = gesture.current_button();
        if button == gtk::gdk::BUTTON_SECONDARY || (button == gtk::gdk::BUTTON_PRIMARY && presses == 2) {
            reset();
        }
    });
    value.add_controller(on_value);
    value.set_tooltip_text(Some("Right-click to reset"));
}

fn typed_value(header: &gtk::Box, scale: &gtk::Scale, value: &gtk::Label, readout: Readout) {
    value.set_tooltip_text(Some("Click to type · right-click to reset"));
    value.set_cursor_from_name(Some("text"));
    let waiting: Rc<RefCell<Option<glib::SourceId>>> = Rc::default();
    let click = gtk::GestureClick::new();
    click.set_button(gtk::gdk::BUTTON_PRIMARY);
    click.connect_pressed(glib::clone!(
        #[weak] header,
        #[weak] scale,
        #[weak] value,
        move |_, presses, _, _| {
            if let Some(booked) = waiting.borrow_mut().take() {
                booked.remove();
            }
            if presses != 1 || !scale.is_sensitive() {
                return;
            }
            let time = gtk::Settings::default().map_or(400, |settings| settings.gtk_double_click_time());
            let booked = waiting.clone();
            let source = glib::timeout_add_local_once(
                std::time::Duration::from_millis(time.max(0) as u64),
                glib::clone!(
                    #[weak] header,
                    #[weak] scale,
                    #[weak] value,
                    move || {
                        booked.borrow_mut().take();
                        type_into(&header, &scale, &value, readout);
                    }
                ),
            );
            *waiting.borrow_mut() = Some(source);
        }
    ));
    value.add_controller(click);
}

fn type_into(header: &gtk::Box, scale: &gtk::Scale, value: &gtk::Label, readout: Readout) {
    let shown = value.text().to_string();
    let field = gtk::Text::new();
    gtk::prelude::EditableExt::set_text(&field, &shown);
    gtk::prelude::EditableExt::set_alignment(&field, 1.0);
    field.set_halign(gtk::Align::End);
    field.set_width_chars(6);
    field.set_max_width_chars(6);
    field.add_css_class("slider-value");
    field.add_css_class("typing");
    value.set_visible(false);
    header.append(&field);
    field.grab_focus();
    gtk::prelude::EditableExt::select_region(&field, 0, -1);

    let answered = Rc::new(Cell::new(false));
    let answer = Rc::new(glib::clone!(
        #[weak] header,
        #[weak] scale,
        #[weak] value,
        #[weak] field,
        move |take: bool| {
            if answered.replace(true) {
                return;
            }

            if take && field.text() != shown.as_str() {
                match readout.parse(&field.text()) {
                    Some(typed) => scale.set_value(typed),
                    None => value.error_bell(),
                }
            }
            value.set_visible(true);

            glib::idle_add_local_once(glib::clone!(
                #[weak] header,
                #[weak] field,
                move || header.remove(&field)
            ));
        }
    ));
    field.connect_activate(glib::clone!(
        #[strong] answer,
        #[weak] scale,
        move |_| {
            answer(true);
            scale.grab_focus();
        }
    ));
    let keys = gtk::EventControllerKey::new();

    keys.set_propagation_phase(gtk::PropagationPhase::Capture);
    keys.connect_key_pressed(glib::clone!(
        #[strong] answer,
        #[weak] scale,
        #[upgrade_or] glib::Propagation::Proceed,
        move |_, key, _, _| {
            if key != gtk::gdk::Key::Escape {
                return glib::Propagation::Proceed;
            }
            answer(false);
            scale.grab_focus();
            glib::Propagation::Stop
        }
    ));
    field.add_controller(keys);
    let focus = gtk::EventControllerFocus::new();
    focus.connect_leave(move |_| answer(true));
    field.add_controller(focus);
}

pub(super) fn strength_scale() -> gtk::Scale {
    let scale = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 100.0, 1.0);
    scale.set_value(100.0);
    set_neutral(&scale, 100.0);
    scale
}

pub(super) fn numa_slider(adjustment: &gtk::Adjustment, neutral: f64) -> gtk::Scale {
    let scale = gtk::Scale::new(gtk::Orientation::Horizontal, Some(adjustment));
    scale.add_css_class("numa-slider");
    scale.set_draw_value(false);
    scale.set_has_origin(false);
    PAINTERS.with(|painters| painters.borrow_mut().insert(scale.as_ptr() as usize, Painter::on(&scale)));
    set_neutral(&scale, neutral);
    let mark = move |scale: &gtk::Scale| {
        let moved = (scale.value() - neutral).abs() > scale.adjustment().step_increment() / 2.0;
        for widget in std::iter::once(scale.clone().upcast::<gtk::Widget>()).chain(scale.parent()) {
            match moved {
                true => widget.add_css_class("touched"),
                false => widget.remove_css_class("touched"),
            }
        }
    };
    scale.connect_value_changed(move |scale| {
        repaint(scale);
        mark(scale);
    });
    scale.connect_realize(move |scale| mark(scale));
    on_reset(&scale, None, glib::clone!(
        #[weak] scale,
        move || glide(&scale, &scale.adjustment(), neutral)
    ));
    scale
}

pub(super) fn connect_wheel(scale: &gtk::Scale) {
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
