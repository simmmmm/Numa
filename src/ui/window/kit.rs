use super::*;

pub(super) fn section_header(title: &str) -> gtk::Label {
    let label = gtk::Label::new(Some(&title.to_uppercase()));
    label.set_xalign(0.0);

    label.set_margin_top(18);
    label.set_margin_bottom(2);
    label.add_css_class("section-header");
    label
}

pub(super) fn primary_button(label: &str) -> gtk::Button {
    let button = gtk::Button::with_label(label);
    primary(&button);
    button
}

pub(super) fn primary(button: &impl IsA<gtk::Widget>) {
    button.add_css_class("numa-primary");
    button.add_css_class("opaque");
}

pub(super) fn chip_row() -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    row.add_css_class("chip-row");
    row
}

thread_local! {

    static GLIDES: RefCell<HashMap<usize, adw::TimedAnimation>> = RefCell::new(HashMap::new());
}

pub(super) fn glide(widget: &impl IsA<gtk::Widget>, adjustment: &gtk::Adjustment, to: f64) {
    let key = adjustment.as_ptr() as usize;
    let target = adw::PropertyAnimationTarget::new(adjustment, "value");
    let animation = adw::TimedAnimation::new(widget, adjustment.value(), to, 200, target);
    animation.set_easing(adw::Easing::EaseOutCubic);
    animation.connect_done(move |_| {
        GLIDES.with(|glides| glides.borrow_mut().remove(&key));
    });
    if let Some(earlier) = GLIDES.with(|glides| glides.borrow_mut().insert(key, animation.clone())) {
        earlier.pause();
    }
    animation.play();
}
