use super::rapid::{self, of};
use super::*;
use numa::io::workflows::{read_note, Control, Kind};

pub(super) fn clear(state: &App) {
    let panel = &of(state).panel;
    while let Some(child) = panel.first_child() {
        panel.remove(&child);
    }
}

fn label(text: &str, class: &str) -> gtk::Label {
    let label = gtk::Label::new(Some(text));
    label.add_css_class(class);
    label.set_xalign(0.0);
    label.set_wrap(true);
    label
}

pub(super) fn kind_choice(state: &App) -> gtk::DropDown {
    let library = rapid::library_of_view(state);
    let kind = rapid::kind_of_view(state);
    let names: Vec<&str> = std::iter::once("Kind of Shoot…").chain(Kind::ALL.iter().map(|kind| kind.name())).collect();
    let choose = gtk::DropDown::from_strings(&names);
    choose.set_selected(kind.and_then(|kind| Kind::ALL.iter().position(|k| *k == kind)).map_or(0, |at| at as u32 + 1));
    choose.set_sensitive(library.is_some());
    choose.set_tooltip_text(Some("What this library is a shoot of: it decides what the moments are adjusted by, and what Numa offers to do"));
    choose.connect_selected_notify(glib::clone!(
        #[strong] state,
        move |choose| {
            let Some(library) = rapid::library_of_view(&state) else { return };
            let kind = (choose.selected() as usize).checked_sub(1).and_then(|at| Kind::ALL.get(at).copied());
            if let Err(err) = state.catalog.set_kind(library, kind) {
                state.toast(&err);
            }

            let state = state.clone();
            glib::idle_add_local_once(move || rapid::rebuild(&state));
        }
    ));
    choose
}

pub(super) fn kind_question(state: &App) -> adw::StatusPage {
    let kinds = gtk::FlowBox::new();
    kinds.set_selection_mode(gtk::SelectionMode::None);
    kinds.set_homogeneous(true);
    kinds.set_max_children_per_line(4);
    kinds.set_column_spacing(12);
    kinds.set_row_spacing(12);
    for kind in Kind::ALL {
        let button = gtk::Button::with_label(kind.name());
        button.add_css_class("pill");
        button.connect_clicked(glib::clone!(
            #[strong] state,
            move |_| {
                let Some(library) = rapid::library_of_view(&state) else { return };
                if let Err(err) = state.catalog.set_kind(library, Some(kind)) {
                    state.toast(&err);
                }

                let state = state.clone();
                glib::idle_add_local_once(move || rapid::rebuild(&state));
            }
        ));
        kinds.append(&button);
    }
    let page = adw::StatusPage::new();
    page.set_title("What Did You Shoot?");
    page.set_description(Some("Numa sets the moments to it: what each one is adjusted by, how many frames of a burst it keeps, and what it offers to do."));

    let clamp = adw::Clamp::new();
    clamp.set_maximum_size(720);
    clamp.set_child(Some(&kinds));
    page.set_child(Some(&clamp));
    page.set_vexpand(true);
    page
}

pub(super) fn fill(state: &App, m: usize) {
    let rapid = of(state);

    let scrolled = rapid.panel.ancestor(gtk::ScrolledWindow::static_type()).and_downcast::<gtk::ScrolledWindow>().map(|side| side.vadjustment());
    if let Some(adjustment) = scrolled.filter(|_| rapid.filled.replace(Some(m)) == Some(m)) {
        let at = adjustment.value();
        glib::idle_add_local_once(move || adjustment.set_value(at));
    }
    clear(state);
    let Some((times, picked, size)) = rapid.moments.borrow().get(m).map(|moment| {
        (format!("{} – {}", rapid::clock(moment.start()), rapid::clock(moment.end())), rapid::picked(state, &moment.ids).len(), moment.ids.len())
    }) else {
        return;
    };
    let panel = &rapid.panel;

    let chapter = (!rapid::by_likeness(state)).then(|| rapid_chapters::of_moments(state).get(m).cloned().flatten()).flatten();
    let name = chapter.as_ref().map_or(times.clone(), |(chapter, _)| chapter.clone());
    let title = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let words = section_header(&format!("This moment · {name}"));
    words.set_hexpand(true);
    words.set_ellipsize(gtk::pango::EllipsizeMode::End);
    let count = label(&rapid::photos(size), "rapid-count");
    count.set_wrap(false);
    count.set_valign(gtk::Align::End);
    count.set_margin_bottom(2);
    title.append(&words);
    title.append(&count);
    let close = gtk::Button::from_icon_name("window-close-symbolic");
    close.add_css_class("flat");
    close.set_valign(gtk::Align::Center);
    close.set_tooltip_text(Some("Done with this moment (Esc)"));
    close.connect_clicked(glib::clone!(
        #[strong] state,
        move |_| rapid::let_go(&state)
    ));
    title.append(&close);
    panel.append(&title);
    let about = match chapter {
        Some(_) => format!("{times} · {picked} picked"),
        None => format!("{picked} picked"),
    };
    panel.append(&label(&about, "dim-label"));
    panel.append(&rapid_layers::section(state, m));

    let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    buttons.set_margin_top(10);
    let reset = gtk::Button::with_label("Reset");
    reset.set_tooltip_text(Some("The moment's sliders, white balance and key frames back to where they rest — its look and each photo's own changes stay; one slider alone with a double click on it"));
    reset.connect_clicked(glib::clone!(
        #[strong] state,
        move |_| reset_moment(&state, m)
    ));
    let style = gtk::Button::with_label("Your Style…");
    style.set_tooltip_text(Some("What Numa has learned from you, and when"));
    style.set_action_name(Some("win.rapid-style"));
    buttons.append(&reset);

    if let Some(parent) = rapid.note.parent().and_downcast::<gtk::Box>() {
        parent.remove(&rapid.note);
    }
    buttons.append(&rapid.note);
    buttons.append(&style);
    panel.append(&buttons);

    panel.append(&rapid_numa::did_section(state, m));
    if rapid.moments.borrow().get(m).is_some_and(|moment| moment.tiles.iter().any(|tile| tile.is_stack())) {
        panel.append(&bursts_section(state, m));
    }

}

fn bursts_section(state: &App, m: usize) -> gtk::Box {
    let column = gtk::Box::new(gtk::Orientation::Vertical, 6);
    column.append(&section_header("Stacks"));
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let words = label("Numa keeps from each", "slider-name");
    words.set_hexpand(true);
    row.append(&words);
    let chips = chip_row();
    let now = rapid::keep_count(state);
    let mut first: Option<gtk::ToggleButton> = None;
    for count in 1..=3usize {
        let chip = gtk::ToggleButton::with_label(&count.to_string());
        chip.set_group(first.as_ref());
        chip.set_active(count == now);
        chip.connect_toggled(glib::clone!(
            #[strong] state,
            move |chip| {
                if !chip.is_active() {
                    return;
                }
                let Some(library) = rapid::library_of_view(&state) else { return };
                let _ = state.catalog.set_library_setting(library, "keep-per-burst", Some(&count.to_string()));
                let state = state.clone();
                glib::idle_add_local_once(move || {
                    rapid::fill_list(&state);
                    fill(&state, m);
                });
            }
        ));
        first.get_or_insert(chip.clone());
        chips.append(&chip);
    }
    row.append(&chips);
    column.append(&row);
    column.append(&label("The sharpest frames with open eyes, spread over the stack. P on a stack picks them; a click shows them first.", "dim-label"));
    column
}

pub(super) fn amount(control: Control, delta: f32) -> String {
    let value = match control {
        Control::Warmth => Readout::OffsetKelvin.format(delta as f64),
        Control::Exposure => Readout::Signed(2).format(delta as f64),
        _ => Readout::Signed(0).format(delta as f64),
    };
    format!("{} {value}", control.name())
}

pub(super) fn as_shot(state: &App, key: i64) -> WhiteBalance {
    of(state).as_shot.borrow().get(&key).copied().unwrap_or(WhiteBalance { temperature: 5500.0, tint: 0.0 })
}

fn reset_moment(state: &App, m: usize) {
    rapid_layers::change(state, m, |layers| {
        layers.tone = [0.0; 6];
        (layers.vibrance, layers.noise, layers.balance, layers.matched) = (0.0, 0.0, None, None);
        layers.keys.clear();
        for frame in layers.frames.values_mut() {
            frame.balance = None;
        }
    });
    fill(state, m);
}

pub(super) fn note(state: &App) {
    of(state).note.popup();
}

pub(super) fn build_note(state: &App) {
    let entry = gtk::Entry::new();
    entry.set_placeholder_text(Some("What is off? Too warm, te donker, the sky is blown…"));
    entry.set_width_chars(34);
    let reads = label("", "dim-label");
    reads.set_max_width_chars(40);
    let photo = gtk::ToggleButton::with_label("This Photo");
    photo.set_tooltip_text(Some("Only the photograph with the keys"));
    let moment = gtk::ToggleButton::with_label("This Moment");
    let style = gtk::ToggleButton::with_label("My Style");
    style.set_tooltip_text(Some("This moment, and Numa does it on every moment of this kind from now on"));
    moment.set_group(Some(&photo));
    style.set_group(Some(&photo));
    moment.set_active(true);
    let scope = chip_row();
    scope.set_homogeneous(true);
    for button in [&photo, &moment, &style] {
        scope.append(button);
    }
    let apply = gtk::Button::with_label("Apply");
    apply.set_sensitive(false);
    let column = gtk::Box::new(gtk::Orientation::Vertical, 10);
    for part in [entry.upcast_ref::<gtk::Widget>(), reads.upcast_ref(), scope.upcast_ref(), apply.upcast_ref()] {
        column.append(part);
    }
    let popover = gtk::Popover::new();
    popover.add_css_class("numa-content");
    popover.set_child(Some(&column));
    let button = &of(state).note;
    button.set_label("Note…");
    button.set_tooltip_text(Some("Say what is off in your own words (N)"));
    button.set_popover(Some(&popover));

    entry.connect_changed(glib::clone!(
        #[weak] reads,
        #[weak] apply,
        move |entry| {
            let text = entry.text();
            let read = read_note(&text);
            apply.set_sensitive(!read.is_empty());
            reads.set_text(&match (text.trim().is_empty(), read.is_empty()) {
                (true, _) => String::new(),
                (false, true) => "Numa does not know these words yet. It knows warm, cool, light, dark, contrast, highlights, shadows, colour and noise.".to_string(),
                (false, false) => format!("Numa reads it as {}", read.iter().map(|(control, delta)| amount(*control, *delta)).collect::<Vec<_>>().join(" · ")),
            });
        }
    ));
    let go = glib::clone!(
        #[strong] state,
        #[weak] entry,
        #[weak] popover,
        #[weak] photo,
        #[weak] style,
        move || {
            let text = entry.text().to_string();
            let read = read_note(&text);
            if read.is_empty() {
                return;
            }
            let scope = if photo.is_active() { Scope::Photo } else if style.is_active() { Scope::Style } else { Scope::Moment };
            if apply_note(&state, &text, &read, scope) {
                entry.set_text("");
                popover.popdown();
            }
        }
    );
    let go_ = go.clone();
    apply.connect_clicked(move |_| go_());
    entry.connect_activate(move |_| go());
}

#[derive(Clone, Copy, PartialEq)]
enum Scope {
    Photo,
    Moment,
    Style,
}

fn apply_note(state: &App, text: &str, read: &[(Control, f32)], scope: Scope) -> bool {
    let rapid = of(state);
    let (m, _) = rapid.at.get();
    let Some((key, count)) = rapid.moments.borrow().get(m).map(|moment| (moment.key, moment.ids.len())) else { return false };
    if scope == Scope::Style && !rapid_numa::learn_note(state, m, text, read) {
        return false;
    }
    let shot = as_shot(state, key);
    match scope {

        Scope::Photo => {
            let Some(id) = rapid::focused(state) else { return false };

            rapid_layers::claim(state, m);
            let Some(mut own) = rapid::own_of(state, id) else { return false };
            for (control, delta) in read {
                let (lower, upper, _) = control.range();
                let value = (control.get(&own, shot) + delta).clamp(lower, upper);
                control.set(&mut own, value, shot);
            }
            if let Err(err) = state.catalog.save_own_edits(id, &own) {
                state.toast(&err);
                return false;
            }
            rapid::repaint_moment(state, m);
        }
        Scope::Moment | Scope::Style => rapid_layers::note_on_moment(state, m, read, shot),
    }
    fill(state, m);
    let said = rapid_layers::said(read);
    state.toast(&match scope {
        Scope::Photo => format!("{said} on this photograph"),
        Scope::Moment => format!("{said} on {}", rapid::photos(count)),
        Scope::Style => format!("{said} on this moment, and on every one like it from now on — Your Style… takes it back"),
    });
    true
}
