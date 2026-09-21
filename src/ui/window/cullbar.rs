use super::*;

const KEYS: &str = "0–5 rate · P pick · X reject";

pub(super) fn build(state: &App) -> gtk::Widget {
    let bar = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    bar.add_css_class("toolbar");
    bar.add_css_class("osd");
    bar.append(&gtk::Label::new(Some(KEYS)));

    let floating = gtk::Revealer::new();
    floating.set_child(Some(&bar));
    floating.set_transition_type(gtk::RevealerTransitionType::SlideUp);
    floating.set_halign(gtk::Align::Center);
    floating.set_valign(gtk::Align::End);

    floating.set_margin_bottom(18);

    floating.set_can_target(false);

    let update = Rc::new(glib::clone!(
        #[weak] floating,
        #[strong] state,
        move || {
            let looking = state.loupe.reveal.reveals_child();
            floating.set_reveal_child(!looking && !state.grid.wall.selected().is_empty());
        }
    ));
    state.grid.wall.connect_selection_changed(glib::clone!(
        #[strong] update,
        move |_| update()
    ));
    state.loupe.reveal.connect_reveal_child_notify(move |_| update());
    floating.upcast()
}
