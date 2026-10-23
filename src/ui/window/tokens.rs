use super::*;

const SECTIONS: [&str; 6] = ["Things", "Places", "People", "Cameras", "Dates", "Libraries"];

const ROWS: usize = 3;

const TALL: i32 = 480;

#[derive(Clone)]
pub(super) enum Token {

    Thing(usize),

    Place(String),
    Person(String),
    Camera(String),

    Date(String),
    Library(Place),
}

impl Token {
    fn section(&self) -> usize {
        match self {
            Token::Thing(_) => 0,
            Token::Place(_) => 1,
            Token::Person(_) => 2,
            Token::Camera(_) => 3,
            Token::Date(_) => 4,
            Token::Library(_) => 5,
        }
    }
}

pub(super) struct Suggestion {
    pub(super) token: Token,

    pub(super) name: String,

    pub(super) aside: String,
    pub(super) count: usize,
}

#[derive(Clone)]
pub(super) struct Suggestions {
    popover: gtk::Popover,
    list: gtk::ListBox,
    scroller: gtk::ScrolledWindow,

    rows: Rc<RefCell<Vec<(gtk::ListBoxRow, Rc<dyn Fn()>)>>>,

    every: Rc<RefCell<Option<Rc<Vec<Suggestion>>>>>,
}

impl Suggestions {
    pub(super) fn new() -> Self {
        let list = gtk::ListBox::new();
        list.add_css_class("navigation-sidebar");
        list.set_focusable(false);
        let scroller = gtk::ScrolledWindow::new();
        scroller.set_hscrollbar_policy(gtk::PolicyType::Never);
        scroller.set_propagate_natural_height(true);
        scroller.set_max_content_height(TALL);
        scroller.set_child(Some(&list));

        let popover = gtk::Popover::new();
        popover.set_autohide(false);
        popover.set_has_arrow(false);
        popover.set_position(gtk::PositionType::Bottom);
        popover.set_focusable(false);

        popover.add_css_class("numa-content");
        popover.add_css_class("filter-menu");
        popover.set_child(Some(&scroller));
        Self { popover, list, scroller, rows: Rc::default(), every: Rc::default() }
    }
}

pub(super) fn install(state: &App, entry: &gtk::SearchEntry) {
    let suggestions = &state.libraries.words.suggestions;
    suggestions.popover.set_parent(entry);
    suggestions.list.connect_row_activated(glib::clone!(
        #[strong] state,
        move |_, row| run(&state, row.upcast_ref())
    ));
    let keys = gtk::EventControllerKey::new();
    keys.set_propagation_phase(gtk::PropagationPhase::Capture);
    keys.connect_key_pressed(glib::clone!(
        #[strong] state,
        #[weak] entry,
        #[upgrade_or] glib::Propagation::Proceed,
        move |_, key, _, _| pressed(&state, &entry, key)
    ));
    entry.add_controller(keys);
}

fn run(state: &App, row: &gtk::Widget) {
    let deed = state.libraries.words.suggestions.rows.borrow().iter().find(|(known, _)| known.upcast_ref::<gtk::Widget>() == row).map(|(_, deed)| deed.clone());
    if let Some(deed) = deed {
        deed();
    }
}

pub(super) fn close_on_clicks(state: &App, window: &adw::ApplicationWindow) {
    let click = gtk::GestureClick::new();
    click.set_button(0);
    click.set_propagation_phase(gtk::PropagationPhase::Capture);
    click.connect_pressed(glib::clone!(
        #[strong] state,
        move |click, _, x, y| {
            let words = &state.libraries.words;
            let popover = &words.suggestions.popover;
            let event = click.current_event();
            let surface = event.as_ref().and_then(|event| event.surface());
            if surface.is_some() && surface == popover.surface() {
                click.set_state(gtk::EventSequenceState::Claimed);
                let (dx, dy) = popover.surface_transform();
                let at = event.and_then(|event| event.position());
                let hit = at.and_then(|(x, y)| popover.pick(x - dx, y - dy, gtk::PickFlags::DEFAULT));
                if let Some(row) = hit.and_then(|hit| hit.ancestor(gtk::ListBoxRow::static_type())) {
                    run(&state, &row);
                }
                return;
            }
            let hit = click.widget().and_then(|window| window.pick(x, y, gtk::PickFlags::DEFAULT));
            if !hit.is_some_and(|hit| hit == *words.entry.upcast_ref::<gtk::Widget>() || hit.is_ancestor(&words.entry)) {
                hide(&state);
            }
        }
    ));
    window.add_controller(click);
}

fn pressed(state: &App, entry: &gtk::SearchEntry, key: gtk::gdk::Key) -> glib::Propagation {
    use gtk::gdk::Key;
    let suggestions = &state.libraries.words.suggestions;
    let open = suggestions.popover.is_visible();
    match key {
        Key::Down | Key::Up if open => step(suggestions, key == Key::Down),
        Key::Escape if open => hide(state),
        Key::Return | Key::KP_Enter | Key::ISO_Enter if open => {
            match suggestions.list.selected_row() {
                Some(row) => run(state, row.upcast_ref()),
                None => return glib::Propagation::Proceed,
            }
        }
        Key::BackSpace if entry.text().is_empty() => filter::drop_last_token(state),
        _ => return glib::Propagation::Proceed,
    }
    glib::Propagation::Stop
}

fn step(suggestions: &Suggestions, down: bool) {
    let rows = suggestions.rows.borrow();
    let now = suggestions.list.selected_row().and_then(|chosen| rows.iter().position(|(row, _)| *row == chosen));
    let next = match (now, down) {
        (None, true) => Some(0),
        (None, false) | (Some(0), false) => None,
        (Some(at), true) => Some((at + 1).min(rows.len() - 1)),
        (Some(at), false) => Some(at - 1),
    };
    match next.and_then(|at| rows.get(at)) {
        Some((row, _)) => {
            suggestions.list.select_row(Some(row));

            if let Some(point) = row.compute_point(&suggestions.list, &gtk::graphene::Point::zero()) {
                suggestions.scroller.vadjustment().clamp_page(f64::from(point.y()), f64::from(point.y()) + f64::from(row.height()));
            }
        }
        None => suggestions.list.unselect_all(),
    }
}

pub(super) fn hide(state: &App) {
    let suggestions = &state.libraries.words.suggestions;
    suggestions.popover.popdown();
    suggestions.every.take();
}

pub(super) fn recount(state: &App) {
    let suggestions = &state.libraries.words.suggestions;
    if suggestions.every.take().is_some() && suggestions.popover.is_visible() {
        let state = state.clone();
        glib::idle_add_local_once(move || again(&state));
    }
}

fn again(state: &App) {
    let words = &state.libraries.words;
    words.suggestions.every.take();
    if words.suggestions.popover.is_visible() {
        show(state, words.entry.text().trim());
    }
}

fn every(state: &App) -> Rc<Vec<Suggestion>> {
    let suggestions = &state.libraries.words.suggestions;
    if let Some(every) = suggestions.every.borrow().clone() {
        return every;
    }
    let later = state.clone();
    let every = Rc::new(filter_menu::suggestions(state, Rc::new(move || again(&later))));
    suggestions.every.replace(Some(every.clone()));
    every
}

pub(super) fn show(state: &App, typed: &str) {
    let words = &state.libraries.words;
    let suggestions = &words.suggestions;
    let every = every(state);
    let mut sections: [Vec<(&Suggestion, Vec<(usize, usize)>)>; 6] = Default::default();
    for suggestion in every.iter() {
        if let Some(spans) = matched(suggestion, typed) {
            sections[suggestion.token.section()].push((suggestion, spans));
        }
    }
    let list = &suggestions.list;
    list.remove_all();
    let mut rows: Vec<(gtk::ListBoxRow, Rc<dyn Fn()>)> = Vec::new();
    for (at, mut found) in sections.into_iter().enumerate().filter(|(_, found)| !found.is_empty()) {
        found.sort_by(|a, b| b.0.count.cmp(&a.0.count).then_with(|| a.0.name.cmp(&b.0.name)));
        list.append(&label_row(SECTIONS[at]));
        for (suggestion, spans) in found.into_iter().take(ROWS) {
            let markup = format!("{}{}", emphasised(&suggestion.name, &spans), glib::markup_escape_text(&suggestion.aside));
            let row = row(&markup, &places::grouped(suggestion.count as i64));
            if matches!(suggestion.token, Token::Thing(_)) {
                row.add_css_class("saw-choice");
            }
            list.append(&row);
            let token = suggestion.token.clone();
            rows.push((row, Rc::new(glib::clone!(
                #[strong] state,
                move || choose(&state, token.clone())
            ))));
        }
    }
    list.append(&label_row("Words"));
    let looking = row(&format!("Photos that look like \u{201c}<b>{}</b>\u{201d}", glib::markup_escape_text(typed)), "Enter");
    list.append(&looking);
    rows.push((looking, Rc::new(glib::clone!(
        #[strong] state,
        move || {
            hide(&state);
            words::search_now(&state);
        }
    ))));
    suggestions.rows.replace(rows);
    list.set_size_request(words.entry.width(), -1);
    suggestions.popover.popup();

    suggestions.popover.present();
}

fn label_row(title: &str) -> gtk::ListBoxRow {
    let head = section_header(title);
    head.set_margin_top(8);
    let row = gtk::ListBoxRow::new();
    row.set_child(Some(&head));
    row.set_activatable(false);
    row.set_selectable(false);
    row.set_focusable(false);
    row
}

fn row(markup: &str, end: &str) -> gtk::ListBoxRow {
    let name = gtk::Label::new(None);
    name.set_markup(markup);
    name.set_xalign(0.0);
    name.set_hexpand(true);
    name.set_ellipsize(gtk::pango::EllipsizeMode::End);
    let aside = gtk::Label::new(Some(end));
    aside.add_css_class("caption");
    aside.add_css_class("dim-label");
    aside.add_css_class("numeric");
    let inside = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    inside.append(&name);
    inside.append(&aside);
    let row = gtk::ListBoxRow::new();
    row.set_child(Some(&inside));
    row.set_focusable(false);
    row
}

fn choose(state: &App, token: Token) {
    let words = &state.libraries.words;
    hide(state);
    words.entry.set_text("");
    words.entry.grab_focus();
    let narrow = &state.libraries.narrow;
    match token {
        Token::Thing(at) => words::set_thing(state, Some(at)),
        Token::Place(name) => narrow.borrow_mut().place = Some(name),
        Token::Person(name) => narrow.borrow_mut().person = Some(name),
        Token::Camera(name) => narrow.borrow_mut().camera = Some(name),
        Token::Date(key) => narrow.borrow_mut().date = Some(key),

        Token::Library(place) => {
            let kept = narrow.borrow().clone();
            show_place(state, place);
            if kept == filter_menu::Narrow::default() {
                return;
            }
            narrow.replace(kept);
        }
    }
    reload_grid(state);
}

fn matched(suggestion: &Suggestion, typed: &str) -> Option<Vec<(usize, usize)>> {
    let spans = spans(&suggestion.name, typed);
    match &suggestion.token {
        Token::Date(key) => asked_date(typed).is_some_and(|asked| asked.finds(key)).then(|| spans.unwrap_or_default()),
        _ => spans,
    }
}

fn spans(name: &str, typed: &str) -> Option<Vec<(usize, usize)>> {
    let chars: Vec<char> = name.chars().collect();
    let plain: Vec<String> = chars.iter().map(|letter| numa::io::words::folded(&letter.to_string())).collect();
    let starts: Vec<usize> = (0..chars.len()).filter(|&at| chars[at].is_alphanumeric() && (at == 0 || !chars[at - 1].is_alphanumeric())).collect();
    let mut spans = Vec::new();
    for word in typed.split_whitespace() {
        let want = numa::io::words::folded(word);
        let found = starts.iter().find_map(|&at| {
            let (mut have, mut end) = (String::new(), at);
            while have.len() < want.len() && end < chars.len() {
                have.push_str(&plain[end]);
                end += 1;
            }
            (have == want).then_some((at, end))
        })?;
        spans.push(found);
    }
    (!spans.is_empty()).then_some(spans)
}

fn emphasised(name: &str, spans: &[(usize, usize)]) -> String {
    let bold = |at: usize| spans.iter().any(|(from, to)| (*from..*to).contains(&at));
    let mut markup = String::new();
    for (at, letter) in name.chars().enumerate() {
        if bold(at) && (at == 0 || !bold(at - 1)) {
            markup.push_str("<b>");
        }
        markup.push_str(&glib::markup_escape_text(&letter.to_string()));
        if bold(at) && !bold(at + 1) {
            markup.push_str("</b>");
        }
    }
    markup
}

#[derive(Debug, PartialEq)]
enum Asked {
    Year(String),
    Exactly(String),
}

impl Asked {
    fn finds(&self, key: &str) -> bool {
        match self {
            Asked::Year(start) => key.len() == 4 && key.starts_with(start.as_str()),
            Asked::Exactly(asked) => key == asked,
        }
    }
}

fn asked_date(typed: &str) -> Option<Asked> {
    let typed = numa::io::words::folded(typed.trim());
    let number = |part: &str| part.parse::<u32>().ok().filter(|_| part.chars().all(|digit| digit.is_ascii_digit()));
    let parts: Vec<&str> = typed.split('-').collect();
    match parts.as_slice() {
        [year] if year.len() <= 4 && number(year).is_some() => Some(Asked::Year(year.to_string())),
        [year, month] if year.len() == 4 && number(year).is_some() => Some(Asked::Exactly(format!("{year}-{:02}", number(month)?))),
        [year, month, day] if year.len() == 4 && number(year).is_some() => {
            Some(Asked::Exactly(format!("{year}-{:02}-{:02}", number(month)?, number(day)?)))
        }
        _ => {
            let [name, year] = typed.split_whitespace().collect::<Vec<_>>()[..] else { return None };
            let month = MONTH_NAMES.iter().position(|month| name.len() >= 3 && numa::io::words::folded(month).starts_with(name))? + 1;
            (year.len() == 4 && number(year).is_some()).then(|| Asked::Exactly(format!("{year}-{month:02}")))
        }
    }
}

pub(super) fn date_label(key: &str) -> String {
    let mut parts = key.split('-');
    let year = parts.next().unwrap_or_default();
    let month = parts.next().and_then(|month| month.parse::<usize>().ok()).map(|month| MONTH_NAMES[month.clamp(1, 12) - 1]);
    match (month, parts.next().and_then(|day| day.parse::<u32>().ok())) {
        (Some(month), Some(day)) => format!("{day} {month} {year}"),
        (Some(month), None) => format!("{month} {year}"),
        _ => year.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_is_found_by_the_start_of_any_of_its_words() {
        assert_eq!(spans("Beach", "bea"), Some(vec![(0, 3)]));
        assert_eq!(spans("Beach grass", "gra"), Some(vec![(6, 9)]));
        assert_eq!(spans("São Paulo", "sao"), Some(vec![(0, 3)]));
        assert_eq!(spans("Saint-Tropez", "trop"), Some(vec![(6, 10)]));
        assert_eq!(spans("Beach", "each"), None);
        assert_eq!(spans("Beach", ""), None);
        assert_eq!(emphasised("São Paulo", &[(0, 3)]), "<b>São</b> Paulo");
        assert_eq!(emphasised("A & B", &[(4, 5)]), "A &amp; <b>B</b>");
    }

    #[test]
    fn a_date_is_a_year_a_month_or_a_day() {
        assert_eq!(asked_date("20"), Some(Asked::Year("20".into())));
        assert_eq!(asked_date("2025-3"), Some(Asked::Exactly("2025-03".into())));
        assert_eq!(asked_date("March 2025"), Some(Asked::Exactly("2025-03".into())));
        assert_eq!(asked_date("mar 2025"), Some(Asked::Exactly("2025-03".into())));
        assert_eq!(asked_date("2025-03-14"), Some(Asked::Exactly("2025-03-14".into())));
        assert_eq!(asked_date("ma 2025"), None);
        assert_eq!(asked_date("beach"), None);
        assert!(Asked::Year("202".into()).finds("2025") && !Asked::Year("2025".into()).finds("2025-03"));
        assert_eq!(date_label("2025"), "2025");
        assert_eq!(date_label("2025-03"), "March 2025");
        assert_eq!(date_label("2025-03-14"), "14 March 2025");
    }
}
