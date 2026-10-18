use super::*;

pub(super) fn section_header(title: &str) -> gtk::Label {
    let label = gtk::Label::new(Some(&title.to_uppercase()));
    label.set_xalign(0.0);

    label.set_margin_top(18);
    label.set_margin_bottom(2);
    label.add_css_class("section-header");
    label
}

pub(super) fn named_row(name: &str, end: &impl IsA<gtk::Widget>) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    row.set_margin_top(6);
    row.set_margin_bottom(6);
    let title = gtk::Label::new(Some(name));
    title.set_xalign(0.0);
    title.set_hexpand(!end.hexpands());
    title.add_css_class("slider-name");
    end.set_valign(gtk::Align::Center);
    row.append(&title);
    row.append(end);
    row
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

pub(super) fn dialog_header(dialog: &adw::Dialog, primary: &impl IsA<gtk::Widget>) -> adw::HeaderBar {
    let header = adw::HeaderBar::new();
    header.set_show_start_title_buttons(false);
    header.set_show_end_title_buttons(false);
    let cancel = gtk::Button::with_label("Cancel");
    cancel.connect_clicked(glib::clone!(
        #[weak] dialog,
        move |_| {
            dialog.close();
        }
    ));
    header.pack_start(&cancel);
    header.pack_end(primary);
    header
}

pub(super) fn whole_on_hover(label: &gtk::Label) {
    label.set_has_tooltip(true);
    label.connect_query_tooltip(|label, _, _, _, tip| {
        let cut = label.layout().is_ellipsized();
        if cut {
            tip.set_text(Some(&label.text()));
        }
        cut
    });
}

pub(super) fn keep_whole(label: &gtk::Label, most: i32) {
    label.set_size_request(-1, -1);
    let (_, whole, _, _) = label.measure(gtk::Orientation::Horizontal, -1);
    label.set_size_request(whole.min(most), -1);
}

pub(super) const LIBRARY_NAME_LEAST: i32 = 72;

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
