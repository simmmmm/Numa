use super::*;

struct Hud {
    root: gtk::Box,
    name: gtk::Label,
    value: gtk::Label,
    leaving: Option<glib::SourceId>,
}

thread_local! {
    static HUD: RefCell<Option<Hud>> = const { RefCell::new(None) };
}

pub(super) fn build() -> gtk::Box {
    let root = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    root.add_css_class("value-hud");
    root.set_halign(gtk::Align::Center);
    root.set_valign(gtk::Align::Start);
    root.set_margin_top(34);
    root.set_can_target(false);
    root.set_visible(false);
    let name = gtk::Label::new(None);
    name.add_css_class("hud-name");
    let value = gtk::Label::new(None);
    value.add_css_class("hud-value");
    root.append(&name);
    root.append(&value);
    HUD.with(|hud| hud.replace(Some(Hud { root: root.clone(), name, value, leaving: None })));
    root
}

pub(super) fn show(name: &str, value: &str) {
    HUD.with(|hud| {
        let mut hud = hud.borrow_mut();
        let Some(hud) = hud.as_mut() else { return };
        hud.name.set_text(&name.to_uppercase());
        hud.value.set_text(value);
        hud.root.set_visible(true);
        if let Some(timer) = hud.leaving.take() {
            timer.remove();
        }
        hud.leaving = Some(glib::timeout_add_local_once(std::time::Duration::from_millis(900), || {
            HUD.with(|hud| {
                if let Some(hud) = hud.borrow_mut().as_mut() {
                    hud.leaving = None;
                    hud.root.set_visible(false);
                }
            });
        }));
    });
}

pub(super) fn follow(scale: &gtk::Scale, name: &str, readout: Readout) {
    let marked = Rc::new(Cell::new(false));
    let mark = {
        let marked = marked.clone();
        move || {
            marked.set(true);
            let marked = marked.clone();
            glib::idle_add_local_once(move || marked.set(false));
        }
    };

    let keys = gtk::EventControllerKey::new();
    keys.set_propagation_phase(gtk::PropagationPhase::Capture);
    keys.connect_key_pressed(glib::clone!(
        #[strong] mark,
        move |_, key, _, modifiers| {
            use gtk::gdk::{Key, ModifierType};

            let chord = modifiers.intersects(ModifierType::CONTROL_MASK | ModifierType::ALT_MASK | ModifierType::SUPER_MASK);
            let moves = matches!(
                key,
                Key::Up | Key::Down | Key::Left | Key::Right | Key::KP_Up | Key::KP_Down | Key::KP_Left | Key::KP_Right
                    | Key::Page_Up | Key::Page_Down | Key::KP_Page_Up | Key::KP_Page_Down
                    | Key::Home | Key::End | Key::KP_Home | Key::KP_End
            );
            if moves && !chord {
                mark();
            }
            glib::Propagation::Proceed
        }
    ));
    scale.add_controller(keys);

    let wheel = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::BOTH_AXES);
    wheel.set_propagation_phase(gtk::PropagationPhase::Capture);
    wheel.connect_scroll(move |_, _, _| {
        mark();
        glib::Propagation::Proceed
    });
    scale.add_controller(wheel);

    let name = name.to_owned();
    scale.connect_value_changed(move |scale| {
        if marked.get() {
            show(&name, &readout.format(scale.value()));
        }
    });
}
