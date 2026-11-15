use super::*;
use numa::io::whats_new;

const SEEN: &str = "whats-new-seen";

pub(super) fn consider(state: &App, window: &adw::ApplicationWindow, first_start: bool) {
    if super::wizard::beside() {
        return;
    }
    let seen = match state.catalog.setting(SEEN) {
        Some(seen) => Some(seen),

        None if !first_start => Some(whats_new::EARLIER.to_string()),
        None => None,
    };
    let version = env!("CARGO_PKG_VERSION");
    let start = whats_new::at_start(seen.as_deref(), version);
    let Some(lines) = start.show else {
        if let Some(store) = start.store {
            let _ = state.catalog.set_setting(SEEN, &store);
        }
        return;
    };
    glib::idle_add_local_once(glib::clone!(
        #[strong] state,
        #[weak] window,
        move || {
            if window.visible_dialog().is_some() {
                return;
            }
            present(&window, version, &lines);
            let _ = state.catalog.set_setting(SEEN, version);
        }
    ));
}

fn present(window: &adw::ApplicationWindow, version: &'static str, lines: &[String]) {
    let dialog = adw::Dialog::new();
    let title = format!("New in Numa {version}");
    dialog.set_title(&title);
    dialog.set_content_width(440);
    let column = gtk::Box::new(gtk::Orientation::Vertical, 16);
    column.set_margin_top(24);
    column.set_margin_bottom(24);
    column.set_margin_start(24);
    column.set_margin_end(24);
    let heading = gtk::Label::new(Some(&title));
    heading.add_css_class("title-2");
    heading.set_xalign(0.0);
    heading.set_wrap(true);
    column.append(&heading);
    let list = gtk::Box::new(gtk::Orientation::Vertical, 8);
    for line in lines {
        let row = gtk::Label::new(Some(&format!("•  {line}")));
        row.set_xalign(0.0);
        row.set_wrap(true);
        row.set_max_width_chars(48);
        list.append(&row);
    }
    column.append(&list);
    let more = gtk::Button::with_label("Read More");
    let go = primary_button("Continue");
    let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    buttons.set_halign(gtk::Align::End);
    buttons.set_margin_top(4);
    buttons.append(&more);
    buttons.append(&go);
    column.append(&buttons);
    dialog.set_child(Some(&column));

    more.connect_clicked(glib::clone!(
        #[weak] dialog,
        #[weak] window,
        move |_| {
            let page = format!("{}#{version}", whats_new::READ_MORE);
            gtk::UriLauncher::new(&page).launch(Some(&window), gio::Cancellable::NONE, |_| {});
            dialog.close();
        }
    ));
    go.connect_clicked(glib::clone!(
        #[weak] dialog,
        move |_| {
            dialog.close();
        }
    ));
    dialog.set_default_widget(Some(&go));
    dialog.set_focus(Some(&go));
    dialog.present(Some(window));
}
