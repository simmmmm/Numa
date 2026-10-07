use super::rapid::{self, of};
use super::*;
use numa::io::workflows::Kind;

const TURN: i64 = 20 * 60;

const LIGHT_TURN: f32 = 1500.0;

pub(super) fn chapters(state: &App) -> Vec<(i64, String)> {
    rapid::library_of_view(state).and_then(|library| state.catalog.library_setting(library, "chapters")).and_then(|json| serde_json::from_str(&json).ok()).unwrap_or_default()
}

fn keep(state: &App, chapters: &[(i64, String)]) {
    let Some(library) = rapid::library_of_view(state) else { return };
    let json = serde_json::to_string(chapters).unwrap_or_default();
    if let Err(err) = state.catalog.set_library_setting(library, "chapters", Some(&json)) {
        state.toast(&err);
    }
}

pub(super) fn of_moments(state: &App) -> Vec<Option<(String, bool)>> {
    let list = chapters(state);
    let mut current: Option<String> = None;
    of(state)
        .moments
        .borrow()
        .iter()
        .map(|moment| match list.iter().find(|(id, _)| moment.ids.contains(id)) {
            Some((_, name)) => {
                current = Some(name.clone());
                Some((name.clone(), true))
            }
            None => current.clone().map(|name| (name, false)),
        })
        .collect()
}

fn start(state: &App, m: usize, name: Option<&str>) {
    let Some((key, ids)) = of(state).moments.borrow().get(m).map(|moment| (moment.key, moment.ids.clone())) else { return };
    let mut list = chapters(state);
    list.retain(|(id, _)| !ids.contains(id));
    if let Some(name) = name.map(str::trim).filter(|name| !name.is_empty()) {
        list.push((key, name.to_string()));
    }
    keep(state, &list);
    rapid::fill_list(state);
}

pub(super) fn heading(state: &App, m: usize, name: &str, about: &str) -> gtk::Box {
    let entry = gtk::Entry::new();
    entry.set_text(name);
    entry.set_placeholder_text(Some("No chapter"));
    let popover = gtk::Popover::new();
    popover.set_child(Some(&entry));
    let title = gtk::MenuButton::new();
    title.set_label(name);
    title.add_css_class("flat");
    title.add_css_class("rapid-chapter-name");
    title.set_tooltip_text(Some("Rename the chapter; empty, and the moments go to the one before"));
    title.set_popover(Some(&popover));
    entry.connect_activate(glib::clone!(
        #[strong] state,
        #[weak] popover,
        move |entry| {
            popover.popdown();
            let name = entry.text().to_string();
            let state = state.clone();
            glib::idle_add_local_once(move || start(&state, m, Some(&name).filter(|name| !name.trim().is_empty()).map(|name| name.as_str())));
        }
    ));
    let line = gtk::Label::new(Some(about));
    line.add_css_class("dim-label");
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    row.add_css_class("rapid-chapter");
    row.append(&title);
    row.append(&line);
    row
}

pub(super) fn start_button(state: &App, m: usize, starts: bool) -> gtk::MenuButton {
    let column = gtk::Box::new(gtk::Orientation::Vertical, 2);
    let popover = gtk::Popover::new();
    for name in Kind::chapters(rapid::kind_of_view(state)) {
        let choice = gtk::Button::with_label(name);
        choice.add_css_class("flat");
        if let Some(label) = choice.child().and_downcast::<gtk::Label>() {
            label.set_xalign(0.0);
        }
        choice.connect_clicked(glib::clone!(
            #[strong] state,
            #[weak] popover,
            move |_| {
                popover.popdown();
                let state = state.clone();
                glib::idle_add_local_once(move || start(&state, m, Some(name)));
            }
        ));
        column.append(&choice);
    }
    let own = gtk::Entry::new();
    own.set_placeholder_text(Some("New Chapter…"));
    own.connect_activate(glib::clone!(
        #[strong] state,
        #[weak] popover,
        move |own| {
            popover.popdown();
            let name = own.text().to_string();
            let state = state.clone();
            glib::idle_add_local_once(move || start(&state, m, Some(&name)));
        }
    ));
    column.append(&own);
    if starts {
        let none = gtk::Button::with_label("No Chapter Here");
        none.add_css_class("flat");
        none.connect_clicked(glib::clone!(
            #[strong] state,
            #[weak] popover,
            move |_| {
                popover.popdown();
                let state = state.clone();
                glib::idle_add_local_once(move || start(&state, m, None));
            }
        ));
        column.append(&none);
    }
    popover.set_child(Some(&column));
    let button = gtk::MenuButton::new();
    button.set_label(if starts { "Chapter…" } else { "Start Chapter Here…" });
    button.add_css_class("flat");
    button.add_css_class("rapid-start");
    button.set_tooltip_text(Some("A chapter from this moment on, until the next one starts"));
    button.set_popover(Some(&popover));
    button
}

pub(super) fn offer(state: &App, m: usize, current: Option<&str>) -> Option<gtk::Box> {
    let kind = rapid::kind_of_view(state);
    if !Kind::suggests_chapters(kind) {
        return None;
    }

    let first = m == 0 && chapters(state).is_empty();
    if m == 0 && !first || m > 0 && current.is_none() {
        return None;
    }
    let (key, pause, light) = {
        let moments = of(state).moments.borrow();
        let here = moments.get(m)?;
        let before = m.checked_sub(1).and_then(|before| moments.get(before));

        let shot = of(state).as_shot.borrow();
        let light = before.and_then(|before| Some((shot.get(&before.key)?.temperature, shot.get(&here.key)?.temperature)));
        (here.key, before.map_or(0, |before| here.start() - before.end()), light.filter(|(from, to)| (to - from).abs() >= LIGHT_TURN))
    };
    if !first && pause < TURN && light.is_none() || rapid::kept_ids(state, "chapter-not-here").contains(&key) {
        return None;
    }
    let names = Kind::chapters(kind);
    let used: Vec<String> = chapters(state).into_iter().map(|(_, name)| name).collect();
    let after = current.and_then(|current| names.iter().position(|name| *name == current)).map_or(0, |at| at + 1);
    let name = names.iter().skip(after).find(|name| !used.iter().any(|used| used == *name)).copied()?;
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    row.add_css_class("rapid-offer");
    let words = gtk::Label::new(Some(&match first {
        true => "Tell the day in chapters? The first starts here".to_string(),
        false => match light {
            Some((from, to)) => format!("New chapter here? The light went from {} K to {} K", places::grouped(from.round() as i64), places::grouped(to.round() as i64)),
            None => format!("New chapter here? A {}-minute pause", pause / 60),
        },
    }));
    words.set_hexpand(true);
    words.set_xalign(0.0);
    let take = gtk::Button::with_label(&format!("Start {name}"));
    take.connect_clicked(glib::clone!(
        #[strong] state,
        move |_| {
            let state = state.clone();
            glib::idle_add_local_once(move || start(&state, m, Some(name)));
        }
    ));
    let no = gtk::Button::with_label("Not Here");
    no.add_css_class("flat");
    no.connect_clicked(glib::clone!(
        #[strong] state,
        move |_| {
            let mut said = rapid::kept_ids(&state, "chapter-not-here");
            said.push(key);
            rapid::keep_ids(&state, "chapter-not-here", &said);
            let state = state.clone();
            glib::idle_add_local_once(move || rapid::fill_list(&state));
        }
    ));
    row.append(&words);
    row.append(&take);
    row.append(&no);
    Some(row)
}

pub(super) fn fill_bar(state: &App, headings: &[(String, gtk::Widget, usize)]) {
    let rapid = of(state);
    while let Some(child) = rapid.chapter_bar.first_child() {
        rapid.chapter_bar.remove(&child);
    }
    let mut marks = Vec::new();
    for (name, heading, count) in headings {
        let chip = gtk::ToggleButton::with_label(&format!("{name} · {}", rapid::photos(*count)));
        chip.add_css_class("flat");
        chip.connect_clicked(glib::clone!(
            #[strong] state,
            #[weak] heading,
            move |chip| {
                if of(&state).following.get() {
                    return;
                }
                let rapid = of(&state);
                if let Some(at) = heading.compute_point(&rapid.list, &gtk::graphene::Point::zero()) {
                    let adjustment = rapid.scroller.vadjustment();
                    glide(&rapid.scroller, &adjustment, (at.y() as f64).min(adjustment.upper() - adjustment.page_size()));
                }
                chip.set_active(true);
                if let Some(menu) = chip.ancestor(gtk::Popover::static_type()).and_downcast::<gtk::Popover>() {
                    menu.popdown();
                }
            }
        ));
        rapid.chapter_bar.append(&chip);
        marks.push((heading.clone(), chip, name.clone()));
    }
    *rapid.chapter_marks.borrow_mut() = marks;
    follow(state);
}

pub(super) fn follow(state: &App) {
    let rapid = of(state);
    let top = rapid.scroller.vadjustment().value() as f32;
    let marks = rapid.chapter_marks.borrow();
    let ys: Vec<f32> = marks.iter().map(|(heading, _, _)| heading.compute_point(&rapid.list, &gtk::graphene::Point::zero()).map_or(f32::MAX, |at| at.y())).collect();
    let current = ys.iter().rposition(|y| *y <= top + 4.0);
    rapid.following.set(true);
    for (at, (_, chip, _)) in marks.iter().enumerate() {
        chip.set_active(Some(at) == current);
    }
    rapid.following.set(false);
    match current.filter(|at| ys[*at] < top - 8.0) {
        Some(at) => {
            rapid.sticky.set_label(&marks[at].2);
            rapid.sticky.set_visible(true);
        }
        None => rapid.sticky.set_visible(false),
    }
}
