use super::*;

use numa::io::words::{self as seen, Index, THINGS};

const WANTED: &str = "words-wanted";

const THINGS_HIDDEN: &str = "things-numa-saw-hidden";

const THINGS_SHOWN: usize = 30;

#[derive(Clone)]
pub(super) struct State {

    query: Rc<RefCell<Option<(String, Option<Vec<f32>>)>>>,

    who: Rc<RefCell<Option<std::collections::HashSet<i64>>>>,

    thing: Rc<Cell<Option<usize>>>,

    indexes: Rc<RefCell<HashMap<i64, Index>>>,

    things: Rc<RefCell<Option<Rc<Vec<Vec<f32>>>>>>,

    reading: Rc<RefCell<Option<i64>>>,
    checked: Rc<RefCell<std::collections::HashSet<i64>>>,

    stopped: Rc<Cell<bool>>,

    asked: Rc<Cell<bool>>,

    making: Rc<Cell<bool>>,
    pub(super) bar: gtk::SearchBar,
    entry: gtk::SearchEntry,
}

impl State {
    pub(super) fn new() -> Self {
        Self {
            query: Rc::default(),
            who: Rc::default(),
            thing: Rc::default(),
            indexes: Rc::default(),
            things: Rc::default(),
            reading: Rc::default(),
            checked: Rc::default(),
            stopped: Rc::default(),
            asked: Rc::default(),
            making: Rc::default(),
            bar: gtk::SearchBar::new(),
            entry: gtk::SearchEntry::new(),
        }
    }
}

pub(super) fn build_search_bar(state: &App) -> gtk::SearchBar {
    let words = &state.libraries.words;
    let entry = &words.entry;
    entry.set_placeholder_text(Some("Search by what is in the photographs"));
    entry.set_hexpand(true);
    entry.set_width_chars(36);
    let clamp = adw::Clamp::new();
    clamp.set_maximum_size(520);
    clamp.set_child(Some(entry));
    words.bar.set_child(Some(&clamp));
    words.bar.connect_entry(entry);
    words.bar.set_show_close_button(true);
    entry.connect_search_changed(glib::clone!(
        #[strong] state,
        move |entry| search(&state, entry.text().trim())
    ));

    entry.connect_activate(glib::clone!(
        #[strong] state,
        move |_| {
            if !seen::installed() {
                offer(&state);
            }
        }
    ));

    words.bar.connect_search_mode_enabled_notify(glib::clone!(
        #[strong] state,
        move |bar| match bar.is_search_mode() {

            true if !seen::installed() && !state.libraries.words.asked.get() => {
                glib::idle_add_local_once(glib::clone!(
                    #[strong] state,
                    move || offer(&state)
                ));
            }
            true => {}
            false => stop_words(&state),
        }
    ));
    words.bar.clone()
}

pub(super) fn install_search_action(state: &App, window: &adw::ApplicationWindow) {
    let action = gio::SimpleAction::new("search-words", None);
    action.connect_activate(glib::clone!(
        #[strong] state,
        move |_, _| {
            if state.stack.visible_child_name().as_deref() != Some("library") || state.grid.welcome.is_visible() {
                return;
            }
            let bar = &state.libraries.words.bar;
            bar.set_search_mode(true);
            state.libraries.words.entry.grab_focus();
        }
    ));
    window.add_action(&action);
    if let Some(app) = window.application() {
        app.set_accels_for_action("win.search-words", &["<primary>f"]);
    }
}

pub(super) fn search_button(state: &App) -> gtk::ToggleButton {
    let button = gtk::ToggleButton::new();
    button.set_icon_name("system-search-symbolic");
    button.set_tooltip_text(Some("Search by what is in the photographs (Ctrl+F)"));
    button.bind_property("active", &state.libraries.words.bar, "search-mode-enabled").bidirectional().sync_create().build();
    button
}

fn search(state: &App, text: &str) {
    let words = &state.libraries.words;
    if text.is_empty() {
        stop_words(state);
        return;
    }

    let (who, rest) = people_in(state, text);
    if !rest.is_empty() && !seen::installed() {
        return;
    }
    words.stopped.set(false);
    words.who.replace(who);
    if rest.is_empty() {

        words.query.replace(Some((text.to_string(), Some(Vec::new()))));
        reload_grid(state);
        return;
    }
    state.catalog.remember(WANTED, &true);
    words.query.replace(Some((text.to_string(), None)));
    let (state, text) = (state.clone(), text.to_string());
    glib::spawn_future_local(async move {
        let asked = rest;
        let numbers = gio::spawn_blocking(move || seen::embed_text(&asked)).await;
        let numbers = match numbers {
            Ok(Ok(numbers)) => numbers,
            Ok(Err(err)) => {
                log::warn!("search by words: {err}");
                state.toast("Search by words could not start its model");
                return;
            }
            Err(_) => return,
        };

        {
            let mut query = state.libraries.words.query.borrow_mut();
            match query.as_mut() {
                Some((now, kept)) if *now == text => *kept = Some(numbers),
                _ => return,
            }
        }
        reload_grid(&state);
    });
}

pub(super) fn forget_query(state: &App) {
    let words = &state.libraries.words;
    words.query.take();
    words.who.take();
    if !words.entry.text().is_empty() {
        words.entry.set_text("");
    }
    words.bar.set_search_mode(false);
}

fn stop_words(state: &App) {
    state.libraries.words.who.take();
    if state.libraries.words.query.take().is_some() {
        reload_grid(state);
    }
}

pub(super) fn forget_thing(state: &App) {
    state.libraries.words.thing.set(None);
}

pub(super) fn chips(state: &App) -> (Option<String>, Option<&'static str>) {
    let words = &state.libraries.words;
    let query = words.query.borrow().as_ref().filter(|(_, numbers)| numbers.is_some()).map(|(text, _)| format!("Words: {text}"));
    (query, words.thing.get().map(|at| THINGS[at]))
}

fn offer(state: &App) {
    let words = &state.libraries.words;
    if words.asked.replace(true) {
        state.toast("Search by words needs its download — Preferences has it under Add-ons");
        return;
    }
    downloads::ask_for(
        state,
        seen::MODEL,
        "Search by Words?",
        "Numa looks at your photographs on this computer to find what is in them, in English or Dutch. \
         Nothing is sent anywhere. It needs a download once, then reads each library in the background.",
        |state| {

            state.catalog.remember(WANTED, &true);
            keep_reading(state);
            let text = state.libraries.words.entry.text();
            search(state, text.trim());
        },
    );
}

pub(super) fn empty_text(state: &App) -> Option<&'static str> {
    let words = &state.libraries.words;

    let model = words.query.borrow().as_ref().is_some_and(|(_, numbers)| numbers.as_ref().is_none_or(|numbers| !numbers.is_empty()));
    let narrowing = model || words.thing.get().is_some();
    (narrowing && words.reading.borrow().is_some()).then_some("Numa is still reading these photographs for words.")
}

fn shown(state: &App) -> Vec<Library> {
    match state.libraries.filter.borrow().spans_libraries() {
        true => state.libraries.all.borrow().clone(),
        false => state.libraries.current.borrow().iter().cloned().collect(),
    }
}

fn index_of<'a>(state: &App, indexes: &'a mut HashMap<i64, Index>, library: i64) -> Option<&'a mut Index> {
    if !indexes.contains_key(&library) {
        let root = state.libraries.all.borrow().iter().find(|known| known.id == library)?.path.clone();
        indexes.insert(library, Index::open(&root));
    }
    indexes.get_mut(&library)
}

pub(super) fn narrow(state: &App, photos: &mut Vec<Photo>) {
    let words = &state.libraries.words;
    let query = words.query.borrow().as_ref().and_then(|(_, numbers)| numbers.clone());
    let thing = words.thing.get();
    let things = words.things.borrow().clone();
    if query.is_none() && thing.is_none() {
        return;
    }
    let mut indexes = words.indexes.borrow_mut();
    let libraries: std::collections::HashSet<i64> = photos.iter().map(|photo| numa::io::catalog::library_of(photo.id)).collect();
    for library in libraries {
        let Some(index) = index_of(state, &mut indexes, library) else { continue };
        if let (Some(_), Some(things)) = (thing, &things) {
            index.see(things);
        }
    }
    let numbers_of = |photo: &Photo| indexes.get(&numa::io::catalog::library_of(photo.id))?.get(&photo.path, photo.mtime);
    if let Some(thing) = thing {
        let has = |photo: &Photo| {
            let index = indexes.get(&numa::io::catalog::library_of(photo.id));
            index.and_then(|index| index.saw(&photo.path, photo.mtime)).is_some_and(|seen| seen.contains(&thing))
        };
        photos.retain(has);
    }
    if let (Some(_), Some(who)) = (&query, words.who.borrow().as_ref()) {
        photos.retain(|photo| who.contains(&photo.id));
    }
    if let Some(query) = query.filter(|numbers| !numbers.is_empty()) {
        let scored: Vec<(Photo, f32)> = std::mem::take(photos)
            .into_iter()
            .filter_map(|photo| {
                let score = seen::cosine(numbers_of(&photo)?, &query);
                Some((photo, score))
            })
            .collect();
        *photos = seen::ranked(scored).into_iter().map(|(photo, _)| photo).collect();
    }
}

const JOINING: [&str; 8] = ["and", "en", "with", "met", "&", "+", "und", "et"];

fn people_in(state: &App, text: &str) -> (Option<std::collections::HashSet<i64>>, String) {
    let named = state.catalog.named_faces().unwrap_or_default();
    let mut names: Vec<String> = Vec::new();
    for (_, name, _) in &named {
        if !name.is_empty() && !names.iter().any(|have| have.eq_ignore_ascii_case(name)) {
            names.push(name.clone());
        }
    }
    let (groups, rest) = split_names(text, &names);
    if groups.is_empty() {
        return (None, text.to_string());
    }
    let known = state.catalog.known_from(&named).unwrap_or_default();
    let mut sets = vec![std::collections::HashSet::new(); groups.len()];
    for library in shown(state) {
        for (name, ids) in state.catalog.people_among(library.id, &named, &known).unwrap_or_default() {
            for (group, set) in groups.iter().zip(sets.iter_mut()) {
                if group.iter().any(|one| one.eq_ignore_ascii_case(&name)) {
                    set.extend(ids.iter().copied());
                }
            }
        }
    }
    let everyone = sets.into_iter().reduce(|a, b| a.intersection(&b).copied().collect()).unwrap_or_default();
    (Some(everyone), rest)
}

fn split_names(text: &str, names: &[String]) -> (Vec<Vec<String>>, String) {
    let mut left: Vec<&str> = text.split_whitespace().collect();
    let mut groups = Vec::new();

    let mut whole: Vec<&String> = names.iter().filter(|name| name.split_whitespace().count() > 1).collect();
    whole.sort_by_key(|name| std::cmp::Reverse(name.split_whitespace().count()));
    for name in whole {
        let parts: Vec<String> = name.split_whitespace().map(folded).collect();
        if let Some(at) = left.windows(parts.len()).position(|run| run.iter().map(|word| folded(word)).eq(parts.iter().cloned())) {
            left.drain(at..at + parts.len());
            groups.push(vec![name.clone()]);
        }
    }
    let mut rest = Vec::new();
    for word in left {
        let plain = folded(word);
        let first = |name: &&String| name.split_whitespace().next().map(folded).as_deref() == Some(plain.as_str());
        let who: Vec<String> = names.iter().filter(first).cloned().collect();
        if !who.is_empty() {
            groups.push(who);
        } else if !JOINING.contains(&plain.as_str()) {
            rest.push(word);
        }
    }
    (groups, rest.join(" "))
}

fn folded(text: &str) -> String {
    const FROM: &str = concat!("àáâãäåāăą", "çćč", "ď", "èéêëēėęě", "ìíîïīį", "ł", "ñńň", "òóôõöøōő", "ř", "śšş", "ť", "ùúûüūůűų", "ýÿ", "źżž");
    const TO: &str = concat!("aaaaaaaaa", "ccc", "d", "eeeeeeee", "iiiiii", "l", "nnn", "oooooooo", "r", "sss", "t", "uuuuuuuu", "yy", "zzz");
    text.to_lowercase().chars().map(|c| FROM.chars().position(|from| from == c).and_then(|at| TO.chars().nth(at)).unwrap_or(c)).collect()
}

pub(super) fn keep_reading(state: &App) {
    let words = &state.libraries.words;
    if words.stopped.get() || !seen::installed() || numa::core::power::frugal() || !state.catalog.recall::<bool>(WANTED).unwrap_or(false) {
        return;
    }

    if words.reading.borrow().is_some() {
        return;
    }
    let checked = words.checked.borrow().clone();
    let Some(library) = shown(state).into_iter().find(|library| !checked.contains(&library.id) && !state.catalog.is_offline(library.id)) else {
        return;
    };
    let Ok(photos) = state.catalog.photos(library.id, &Filter::default()) else { return };
    let pending: Vec<(PathBuf, i64)> = {
        let mut indexes = words.indexes.borrow_mut();
        let Some(index) = index_of(state, &mut indexes, library.id) else { return };
        photos.into_iter().filter(|photo| index.get(&photo.path, photo.mtime).is_none()).map(|photo| (photo.path, photo.mtime)).collect()
    };
    if pending.is_empty() {
        words.checked.borrow_mut().insert(library.id);
        keep_reading(state);
        return;
    }
    let cancel = Cancel::default();
    words.reading.replace(Some(library.id));
    glib::spawn_future_local(read_library(state.clone(), library, pending, cancel));
}

pub(super) fn read_imported(state: &App, libraries: &[i64]) {
    if !seen::installed() {
        return;
    }
    let words = &state.libraries.words;
    state.catalog.remember(WANTED, &true);
    words.stopped.set(false);
    words.checked.borrow_mut().retain(|id| !libraries.contains(id));
    keep_reading(state);
}

fn searching(state: &App) -> bool {
    let words = &state.libraries.words;
    words.query.borrow().as_ref().is_some_and(|(_, numbers)| numbers.is_some()) || words.thing.get().is_some()
}

async fn read_library(state: App, library: Library, pending: Vec<(PathBuf, i64)>, cancel: Cancel) {

    const CHUNK: usize = 32;
    let total = pending.len();

    let mut progress = None;
    let mut done = 0;
    let mut failed = None;
    for chunk in pending.chunks(CHUNK) {
        let moved = !shown(&state).iter().any(|now| now.id == library.id);
        if cancel.stopped() || moved || numa::core::power::frugal() {
            break;
        }
        let work = chunk.to_vec();
        let read = gio::spawn_blocking(move || {
            numa::core::power::quietly(|| {
                use rayon::prelude::*;
                let images: Vec<(PathBuf, i64, image::RgbImage)> = work
                    .into_par_iter()
                    .filter_map(|(path, mtime)| {
                        let image = numa::io::thumbs::load(&path, mtime, GRID_THUMB_EDGE, None);
                        image.map_err(|err| log::info!("words: {}: {err}", path.display())).ok().map(|image| (path, mtime, image))
                    })
                    .collect();

                images
                    .into_iter()
                    .map(|(path, mtime, image)| Ok((path, mtime, seen::embed_images(&[image])?.remove(0))))
                    .collect::<Result<Vec<_>, String>>()
            })
        })
        .await;
        match read {
            Ok(Ok(read)) => {
                let mut indexes = state.libraries.words.indexes.borrow_mut();
                if let Some(Err(err)) = index_of(&state, &mut indexes, library.id).map(|index| index.add(read)) {
                    failed = Some(err.to_string());
                    break;
                }
            }
            Ok(Err(err)) => {
                failed = Some(err);
                break;
            }
            Err(_) => break,
        }
        done += chunk.len();
        if progress.is_none() && total >= 2 * CHUNK && searching(&state) {
            progress = Some(progress_toast(&state, &cancel));
        }
        if let Some((_, text, bar)) = &progress {
            let fraction = done as f64 / total as f64;
            text.set_text(&format!(
                "Reading {} for search by words — {} of {} · {:.0} %",
                library.label(),
                places::grouped(done as i64),
                places::grouped(total as i64),
                fraction * 100.0
            ));
            bar.set_fraction(fraction);
        }
    }
    if let Some((toast, _, _)) = progress {
        toast.dismiss();
    }
    let words = &state.libraries.words;
    words.reading.replace(None);

    words.stopped.set(cancel.stopped());
    if let Some(err) = failed {
        log::warn!("reading {} for words: {err}", library.path.display());
    } else if done == total {
        words.checked.borrow_mut().insert(library.id);
    }
    if searching(&state) && done > 0 {
        reload_grid(&state);
    }

    keep_reading(&state);
}

pub(super) fn things_section(state: &App, popover: &gtk::Popover) -> gtk::Box {
    let section = gtk::Box::new(gtk::Orientation::Vertical, 6);
    let heading = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    heading.set_margin_top(6);
    let label = gtk::Label::new(Some("THINGS NUMA SAW"));
    label.add_css_class("section-header");
    label.set_xalign(0.0);
    label.set_hexpand(true);
    label.set_tooltip_text(Some("What Numa found in the photographs on this computer — never written into your keywords or exported"));
    let hide = gtk::Button::with_label("Hide");
    hide.add_css_class("flat");
    hide.add_css_class("caption");
    heading.append(&label);
    heading.append(&hide);
    let chips = gtk::FlowBox::new();
    chips.add_css_class("saw-chips");
    chips.set_selection_mode(gtk::SelectionMode::None);
    chips.set_column_spacing(4);
    chips.set_row_spacing(4);

    chips.set_max_children_per_line(4);
    section.append(&heading);
    section.append(&chips);
    section.set_visible(false);

    let fill = Rc::new(glib::clone!(
        #[strong] state,
        #[weak] section,
        #[weak] chips,
        #[weak] hide,
        move || fill_things(&state, &section, &chips, &hide)
    ));
    popover.connect_show(glib::clone!(
        #[strong] fill,
        move |_| fill()
    ));
    hide.connect_clicked(glib::clone!(
        #[strong] state,
        move |_| {
            let hidden = !state.catalog.recall::<bool>(THINGS_HIDDEN).unwrap_or(false);
            state.catalog.remember(THINGS_HIDDEN, &hidden);
            if hidden && state.libraries.words.thing.take().is_some() {
                reload_grid(&state);
            }
            fill();
        }
    ));
    section
}

fn fill_things(state: &App, section: &gtk::Box, chips: &gtk::FlowBox, hide: &gtk::Button) {
    chips.remove_all();
    let words = &state.libraries.words;
    let Some(library) = state.libraries.current.borrow().clone().filter(|_| seen::installed()) else {
        section.set_visible(false);
        return;
    };
    let spans = state.libraries.filter.borrow().spans_libraries();
    let mut indexes = words.indexes.borrow_mut();
    let empty = index_of(state, &mut indexes, library.id).is_none_or(|index| index.is_empty());
    section.set_visible(!empty);
    let hidden = state.catalog.recall::<bool>(THINGS_HIDDEN).unwrap_or(false);
    hide.set_label(if hidden { "Show" } else { "Hide" });
    chips.set_visible(!hidden);
    if empty || hidden {
        return;
    }
    let Some(things) = words.things.borrow().clone() else {
        drop(indexes);
        make_things(state, section, chips, hide);
        return;
    };
    let mut counts = vec![0usize; THINGS.len()];
    for (_, index) in indexes.iter_mut().filter(|(id, _)| spans || **id == library.id) {
        index.see(&things);
        for seen in index.seen() {
            for thing in seen {
                counts[*thing] += 1;
            }
        }
    }
    let mut found: Vec<(usize, usize)> = counts.into_iter().enumerate().filter(|(_, count)| *count > 0).collect();
    found.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    for (thing, count) in found.into_iter().take(THINGS_SHOWN) {
        let chip = gtk::ToggleButton::with_label(THINGS[thing]);
        chip.set_tooltip_text(Some(&format!("{} {}", places::grouped(count as i64), if count == 1 { "photo" } else { "photos" })));
        chip.set_focus_on_click(false);
        chip.set_active(words.thing.get() == Some(thing));
        chip.connect_toggled(glib::clone!(
            #[strong] state,
            move |chip| {
                let words = &state.libraries.words;
                match chip.is_active() {
                    true => words.thing.set(Some(thing)),
                    false if words.thing.get() == Some(thing) => words.thing.set(None),
                    false => return,
                }
                reload_grid(&state);
            }
        ));
        chips.append(&chip);
    }
}

fn make_things(state: &App, section: &gtk::Box, chips: &gtk::FlowBox, hide: &gtk::Button) {
    if state.libraries.words.making.replace(true) {
        return;
    }
    let (state, section, chips, hide) = (state.clone(), section.clone(), chips.clone(), hide.clone());
    glib::spawn_future_local(async move {
        let made = gio::spawn_blocking(seen::things).await;
        state.libraries.words.making.set(false);
        match made {
            Ok(Ok(things)) => {
                state.libraries.words.things.replace(Some(Rc::new(things)));
                fill_things(&state, &section, &chips, &hide);
            }
            Ok(Err(err)) => log::warn!("things numa saw: {err}"),
            Err(_) => {}
        }
    });
}

#[cfg(test)]
mod tests {
    use super::{folded, split_names};

    #[test]
    fn a_name_is_found_among_the_words_and_the_rest_is_left_for_the_model() {
        let names: Vec<String> = ["Frédérique", "Tijmen de Vries", "Frederik"].map(String::from).to_vec();
        assert_eq!(folded("Frédérique ŁÓDŹ"), "frederique lodz");
        let (groups, rest) = split_names("frederique op het strand", &names);
        assert_eq!(groups, vec![vec!["Frédérique".to_string()]]);
        assert_eq!(rest, "op het strand");
        let (groups, rest) = split_names("Tijmen de Vries en frédérique", &names);
        assert_eq!(groups, vec![vec!["Tijmen de Vries".to_string()], vec!["Frédérique".to_string()]]);
        assert_eq!(rest, "");

        let (groups, _) = split_names("tijmen", &names);
        assert_eq!(groups, vec![vec!["Tijmen de Vries".to_string()]]);
        assert!(split_names("bride with bouquet", &names).0.is_empty());
    }
}
