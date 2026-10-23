use super::*;
use numa::io::places::Place as Town;

const WIDTH: i32 = 320;

const TALL: i32 = 480;

#[derive(Clone, Default, PartialEq)]
pub(super) struct Narrow {
    pub(super) person: Option<String>,

    pub(super) place: Option<String>,
    pub(super) camera: Option<String>,

    pub(super) date: Option<String>,
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Section {
    Library,
    People,
    Places,
    Cameras,
    Dates,
    Flag,
    Rating,
    Things,
    Analyse,
    Type,
    Folder,
}

const HOME: [(Section, &str); 8] = [
    (Section::Library, "Library"),
    (Section::People, "People"),
    (Section::Places, "Places"),
    (Section::Cameras, "Cameras"),
    (Section::Dates, "Dates"),
    (Section::Flag, "Flag"),
    (Section::Rating, "Rating"),
    (Section::Things, "Things Numa Saw"),
];

const OWN: [(Section, &str); 3] = [(Section::Analyse, "Analyse"), (Section::Type, "File Type"), (Section::Folder, "Folder")];

const FLAGS: [(&str, Flag); 3] = [("Picked", Flag::Picked), ("Rejected", Flag::Rejected), ("No Flag", Flag::None)];

pub(super) fn rated(stars: u8) -> String {
    match stars {
        5 => "★ 5".to_string(),
        n => format!("★ {n}+"),
    }
}

struct Seen {
    photos: Vec<Photo>,
    people: HashMap<i64, Vec<String>>,
    cameras: HashMap<i64, String>,
    saw: Option<HashMap<i64, Vec<usize>>>,

    root: Option<PathBuf>,
}

fn pipeline(state: &App, filter: &Filter) -> Vec<Photo> {
    let Some(library) = state.libraries.current.borrow().clone() else { return Vec::new() };
    let photos = match filter.spans_libraries() {
        true => state.catalog.photos_everywhere(filter),
        false => state.catalog.photos(library.id, filter),
    };
    let mut photos = photos.unwrap_or_default();
    words::narrow_but_thing(state, &mut photos);
    photos
}

fn people_of(state: &App, photos: &[Photo]) -> HashMap<i64, Vec<String>> {
    let libraries: std::collections::BTreeSet<i64> = photos.iter().map(|photo| numa::io::catalog::library_of(photo.id)).collect();
    let mut named: HashMap<i64, Vec<String>> = HashMap::new();
    for library in libraries {
        for (name, ids) in state.catalog.people(library).unwrap_or_default() {
            ids.into_iter().for_each(|id| named.entry(id).or_default().push(name.clone()));
        }
    }
    named
}

fn cameras_of(state: &App, photos: &[Photo]) -> HashMap<i64, String> {
    let libraries: std::collections::BTreeSet<i64> = photos.iter().map(|photo| numa::io::catalog::library_of(photo.id)).collect();
    libraries
        .into_iter()
        .filter_map(|library| state.catalog.bodies(library).ok())
        .flatten()
        .filter(|(_, key)| !key.is_empty())
        .map(|(id, key)| (id, numa::io::clocks::camera_name(&key, &[])))
        .collect()
}

fn day_of(photo: &Photo) -> String {
    numa::io::import::day(photo.taken.unwrap_or(photo.mtime), 0)
}

fn dated(photo: &Photo, date: &str) -> bool {
    day_of(photo).starts_with(date)
}

fn keeps(state: &App, seen: &Seen, photo: &Photo, own: Section) -> bool {
    let filter = state.libraries.filter.borrow();
    let narrow = state.libraries.narrow.borrow();
    let folder = state.libraries.folder.borrow();
    let same = |a: &str, b: &str| a.eq_ignore_ascii_case(b);
    let tests: [(Section, bool); 9] = [
        (Section::Rating, photo.rating >= filter.min_rating),
        (Section::Flag, filter.flag.is_none_or(|flag| photo.flag == flag)),
        (Section::Type, filter.file_type.admits(&photo.path)),
        (Section::Folder, folder.as_ref().zip(seen.root.as_ref()).is_none_or(|(folder, root)| photo.path.starts_with(root.join(folder)))),
        (Section::People, narrow.person.as_ref().is_none_or(|person| seen.people.get(&photo.id).is_some_and(|names| names.iter().any(|name| same(name, person))))),
        (Section::Places, narrow.place.as_ref().is_none_or(|place| spots::town_of(photo).is_some_and(|town| spots::named(town, place)))),
        (Section::Cameras, narrow.camera.as_ref().is_none_or(|camera| seen.cameras.get(&photo.id).is_some_and(|name| same(name, camera)))),
        (Section::Dates, narrow.date.as_ref().is_none_or(|date| dated(photo, date))),
        (Section::Things, words::thing(state).is_none_or(|thing| seen.saw.as_ref().is_some_and(|saw| saw.get(&photo.id).is_some_and(|things| things.contains(&thing))))),
    ];
    tests.iter().all(|(section, passes)| *section == own || *passes)
}

pub(super) fn narrow(state: &App, photos: &mut Vec<Photo>) {
    let narrow = state.libraries.narrow.borrow().clone();
    if narrow == Narrow::default() {
        return;
    }
    let people = narrow.person.as_ref().map(|_| people_of(state, photos)).unwrap_or_default();
    let cameras = narrow.camera.as_ref().map(|_| cameras_of(state, photos)).unwrap_or_default();
    let same = |a: &str, b: &str| a.eq_ignore_ascii_case(b);
    photos.retain(|photo| {
        narrow.person.as_ref().is_none_or(|person| people.get(&photo.id).is_some_and(|names| names.iter().any(|name| same(name, person))))
            && narrow.place.as_ref().is_none_or(|place| spots::town_of(photo).is_some_and(|town| spots::named(town, place)))
            && narrow.camera.as_ref().is_none_or(|camera| cameras.get(&photo.id).is_some_and(|name| same(name, camera)))
            && narrow.date.as_ref().is_none_or(|date| dated(photo, date))
    });
}

pub(super) fn suggestions(state: &App, ready: Rc<dyn Fn()>) -> Vec<tokens::Suggestion> {
    use tokens::{Suggestion, Token};
    let seen = {
        let ready = ready.clone();
        seen_now(state, move || ready())
    };
    spots::read(state, &seen.photos, move || ready());
    let within = |section: Section| -> Vec<&Photo> { seen.photos.iter().filter(|photo| keeps(state, &seen, photo, section)).collect() };
    let suggest = |token: Token, name: &str, aside: String, count: usize| Suggestion { token, name: name.to_string(), aside, count };
    let mut all = Vec::new();
    if let Some(saw) = &seen.saw {
        let photos = within(Section::Things);
        for (thing, n) in tally(photos.iter().flat_map(|photo| saw.get(&photo.id).into_iter().flatten().map(|at| numa::io::words::THINGS[*at]))) {
            if let Some(at) = numa::io::words::THINGS.iter().position(|known| *known == thing) {
                all.push(suggest(Token::Thing(at), thing, String::new(), n));
            }
        }
    }

    let mut places: HashMap<&str, (usize, &str)> = HashMap::new();
    for town in within(Section::Places).into_iter().filter_map(spots::town_of) {
        let mut names = vec![town.name.as_str(), town.region.as_str()];
        names.retain(|name| !name.is_empty() && *name != town.country);
        names.dedup();
        for name in names {
            places.entry(name).or_insert((0, town.country.as_str())).0 += 1;
        }
        places.entry(town.country.as_str()).or_insert((0, "")).0 += 1;
    }
    for (name, (n, country)) in places {
        let aside = if country.is_empty() { String::new() } else { format!(", {country}") };
        all.push(suggest(Token::Place(name.to_string()), name, aside, n));
    }
    for (name, n) in tally(within(Section::People).iter().flat_map(|photo| seen.people.get(&photo.id).into_iter().flatten().map(String::as_str))) {
        all.push(suggest(Token::Person(name.to_string()), name, String::new(), n));
    }
    for (camera, n) in tally(within(Section::Cameras).iter().filter_map(|photo| seen.cameras.get(&photo.id).map(String::as_str))) {
        all.push(suggest(Token::Camera(camera.to_string()), camera, String::new(), n));
    }

    let days: Vec<String> = within(Section::Dates).into_iter().map(day_of).collect();
    for (date, n) in tally(days.iter().flat_map(|day| [&day[..4], &day[..7], day.as_str()])) {
        all.push(suggest(Token::Date(date.to_string()), &tokens::date_label(date), String::new(), n));
    }
    all.extend(library_suggestions(state));
    all
}

fn library_suggestions(state: &App) -> Vec<tokens::Suggestion> {
    let narrowing = *state.libraries.narrow.borrow() != Narrow::default() || words::searching(state);
    let mut counts: HashMap<i64, usize> = HashMap::new();
    if narrowing {
        let mut photos = state.catalog.photos_everywhere(&Filter { all_libraries: true, ..Filter::default() }).unwrap_or_default();
        narrow(state, &mut photos);
        words::narrow(state, &mut photos);
        photos.iter().for_each(|photo| *counts.entry(numa::io::catalog::library_of(photo.id)).or_default() += 1);
    }
    let libraries = state.libraries.places.borrow().clone();
    libraries
        .into_iter()
        .filter_map(|place| {
            let Place::Library(library) = &place else { return None };
            let count = match narrowing {
                true => counts.get(&library.id).copied().unwrap_or(0),
                false => places::row(state, &place).count as usize,
            };
            let name = places::name_of(state, &place);
            (count > 0).then(|| tokens::Suggestion { token: tokens::Token::Library(place), name, aside: String::new(), count })
        })
        .collect()
}

#[derive(Clone)]
struct Menu {
    state: App,
    popover: gtk::Popover,
    pages: gtk::Stack,
    seen: Rc<RefCell<Option<Rc<Seen>>>>,

    open: Rc<Cell<Option<(Section, &'static str)>>>,
}

pub(super) fn build(state: &App) -> gtk::MenuButton {
    let popover = gtk::Popover::new();
    popover.add_css_class("numa-content");
    popover.add_css_class("filter-menu");

    popover.set_halign(gtk::Align::Start);
    let pages = gtk::Stack::new();
    pages.set_hhomogeneous(true);
    pages.set_vhomogeneous(false);
    pages.set_interpolate_size(true);
    pages.set_size_request(WIDTH, -1);
    popover.set_child(Some(&pages));
    let menu = Menu { state: state.clone(), popover: popover.clone(), pages, seen: Rc::default(), open: Rc::default() };
    popover.connect_show(move |_| {
        menu.seen.replace(None);
        top(&menu);
    });
    let button = gtk::MenuButton::new();
    button.set_label("Filter");
    button.set_tooltip_text(Some("Narrow by library, person, place, camera, date, flag, rating or what Numa saw"));
    button.set_popover(Some(&popover));
    button
}

fn seen(menu: &Menu) -> Rc<Seen> {
    if let Some(seen) = menu.seen.borrow().clone() {
        return seen;
    }
    let later = menu.clone();
    let seen = Rc::new(seen_now(&menu.state, move || again(&later)));
    menu.seen.replace(Some(seen.clone()));
    seen
}

fn seen_now(state: &App, ready: impl Fn() + 'static) -> Seen {
    let filter = Filter { min_rating: 0, flag: None, file_type: FileType::Any, ..state.libraries.filter.borrow().clone() };
    let photos = pipeline(state, &filter);
    let saw = words::seen_in(state, &photos, ready);
    let root = (!filter.spans_libraries()).then(|| state.libraries.current.borrow().as_ref().map(|library| library.path.clone())).flatten();
    Seen { people: people_of(state, &photos), cameras: cameras_of(state, &photos), saw, root, photos }
}

fn again(menu: &Menu) {
    menu.seen.replace(None);
    if let (true, Some((section, label))) = (menu.popover.is_visible(), menu.open.get()) {
        choices(menu, section, label, gtk::StackTransitionType::None);
    }
}

fn show_page(menu: &Menu, name: &str, page: &gtk::Widget, transition: gtk::StackTransitionType) {
    if let Some(old) = menu.pages.child_by_name(name) {
        menu.pages.remove(&old);
    }
    menu.pages.add_named(page, Some(name));
    menu.pages.set_visible_child_full(name, transition);
}

fn line(label: &str, aside: &str, tick: Option<bool>, more: bool, indent: bool) -> gtk::ListBoxRow {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    if let Some(chosen) = tick {
        let mark = gtk::Image::from_icon_name("object-select-symbolic");
        mark.set_opacity(if chosen { 1.0 } else { 0.0 });
        row.append(&mark);
    }
    let name = gtk::Label::new(Some(label));
    name.set_xalign(0.0);
    name.set_hexpand(true);
    name.set_ellipsize(gtk::pango::EllipsizeMode::End);
    if indent {
        name.set_margin_start(16);
        name.add_css_class("dim-label");
    }
    row.append(&name);
    if !aside.is_empty() {
        let end = gtk::Label::new(Some(aside));
        end.add_css_class("caption");
        end.add_css_class("dim-label");
        end.add_css_class("numeric");
        end.set_ellipsize(gtk::pango::EllipsizeMode::End);
        end.set_max_width_chars(16);
        row.append(&end);
    }
    if more {
        row.append(&gtk::Image::from_icon_name("go-next-symbolic"));
    }
    let line = gtk::ListBoxRow::new();
    line.set_child(Some(&row));
    line
}

fn list(rows: Vec<(gtk::ListBoxRow, Rc<dyn Fn()>)>) -> gtk::ListBox {
    let list = gtk::ListBox::new();
    list.add_css_class("navigation-sidebar");
    list.set_selection_mode(gtk::SelectionMode::None);
    let deeds: Vec<Rc<dyn Fn()>> = rows.iter().map(|(_, deed)| deed.clone()).collect();
    for (row, _) in rows {
        list.append(&row);
    }
    list.connect_row_activated(move |_, row| {
        if let Some(deed) = usize::try_from(row.index()).ok().and_then(|at| deeds.get(at)) {
            deed();
        }
    });
    list
}

fn said(state: &App, section: Section) -> String {
    let filter = state.libraries.filter.borrow();
    let narrow = state.libraries.narrow.borrow();
    match section {
        Section::Library => match current_place(state) {
            Place::Library(library) => library.label(),
            _ => String::new(),
        },
        Section::People => narrow.person.clone().unwrap_or_default(),
        Section::Places => narrow.place.clone().unwrap_or_default(),
        Section::Cameras => narrow.camera.clone().unwrap_or_default(),
        Section::Dates => narrow.date.as_deref().map(tokens::date_label).unwrap_or_default(),
        Section::Flag => FLAGS.iter().find(|(_, flag)| filter.flag == Some(*flag)).map(|(label, _)| label.to_string()).unwrap_or_default(),
        Section::Rating if filter.min_rating > 0 => rated(filter.min_rating),
        Section::Rating => String::new(),
        Section::Things => words::thing(state).map(|thing| numa::io::words::THINGS[thing].to_string()).unwrap_or_default(),
        Section::Analyse => match (filter.only_questionable, filter.best_of_burst) {
            (true, true) => "Both".to_string(),
            (true, false) => "Questionable".to_string(),
            (false, true) => "Best of Each Burst".to_string(),
            (false, false) => String::new(),
        },
        Section::Type => match filter.file_type {
            FileType::Any => String::new(),
            FileType::Raw => "RAW Only".to_string(),
            FileType::NotRaw => "JPEG and Others".to_string(),
        },
        Section::Folder => state.libraries.folder.borrow().as_ref().map(|folder| folder.display().to_string()).unwrap_or_default(),
    }
}

fn top(menu: &Menu) {
    menu.open.set(None);
    let state = &menu.state;
    let folders = !state.libraries.filter.borrow().spans_libraries() && !state.libraries.folders.borrow().is_empty();
    let things = numa::io::words::installed();
    let open = |section: Section, label: &'static str| -> (gtk::ListBoxRow, Rc<dyn Fn()>) {
        let menu = menu.clone();
        (line(label, &said(state, section), None, true, false), Rc::new(move || choices(&menu, section, label, gtk::StackTransitionType::SlideLeft)))
    };
    let home: Vec<_> = HOME.iter().filter(|(section, _)| *section != Section::Things || things).map(|(section, label)| open(*section, label)).collect();
    let own: Vec<_> = OWN.iter().filter(|(section, _)| *section != Section::Folder || folders).map(|(section, label)| open(*section, label)).collect();
    let page = gtk::Box::new(gtk::Orientation::Vertical, 0);
    page.append(&list(home));
    page.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
    page.append(&list(own));
    show_page(menu, "top", page.upcast_ref(), gtk::StackTransitionType::SlideRight);
}

fn tally<'a>(values: impl Iterator<Item = &'a str>) -> Vec<(&'a str, usize)> {
    let mut counts: HashMap<&str, usize> = HashMap::new();
    values.for_each(|value| *counts.entry(value).or_default() += 1);
    let mut rows: Vec<(&str, usize)> = counts.into_iter().collect();
    rows.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(b.0)));
    rows
}

fn note(text: &str) -> gtk::Label {
    let note = gtk::Label::new(Some(text));
    note.set_xalign(0.0);
    note.set_wrap(true);
    note.set_max_width_chars(36);
    note.add_css_class("dim-label");
    note.set_margin_start(12);
    note.set_margin_end(12);
    note.set_margin_top(8);
    note.set_margin_bottom(8);
    note
}

type Choice = (gtk::ListBoxRow, Rc<dyn Fn()>);

fn count(n: usize) -> String {
    places::grouped(n as i64)
}

fn narrowing(menu: &Menu, set: impl Fn(&mut Narrow) + 'static) -> Rc<dyn Fn()> {
    let (state, popover) = (menu.state.clone(), menu.popover.clone());
    Rc::new(move || {
        set(&mut state.libraries.narrow.borrow_mut());
        popover.popdown();
        reload_grid(&state);
    })
}

fn filtering(menu: &Menu, set: impl Fn(&mut Filter) + 'static) -> Rc<dyn Fn()> {
    let (state, popover) = (menu.state.clone(), menu.popover.clone());
    Rc::new(move || {
        set(&mut state.libraries.filter.borrow_mut());
        popover.popdown();
        reload_grid(&state);
    })
}

fn choices(menu: &Menu, section: Section, label: &'static str, transition: gtk::StackTransitionType) {
    menu.open.set(Some((section, label)));
    let seen = seen(menu);
    let within: Vec<&Photo> = seen.photos.iter().filter(|photo| keeps(&menu.state, &seen, photo, section)).collect();
    let (rows, mut notes) = options(menu, section, &seen, &within);
    if rows.is_empty() && notes.is_empty() {
        notes.push("Nothing here to choose from.".to_string());
    }
    let page = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let back = {
        let menu = menu.clone();
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        row.append(&gtk::Image::from_icon_name("go-previous-symbolic"));
        row.append(&gtk::Label::new(Some("Filter")));
        let back = gtk::ListBoxRow::new();
        back.set_child(Some(&row));
        list(vec![(back, Rc::new(move || top(&menu)) as Rc<dyn Fn()>)])
    };
    page.append(&back);
    let head = section_header(label);
    head.set_margin_top(4);
    head.set_margin_start(12);
    page.append(&head);
    let scroller = gtk::ScrolledWindow::new();
    scroller.set_hscrollbar_policy(gtk::PolicyType::Never);
    scroller.set_propagate_natural_height(true);
    scroller.set_max_content_height(TALL);
    let inside = gtk::Box::new(gtk::Orientation::Vertical, 0);
    if !rows.is_empty() {
        inside.append(&list(rows));
    }
    for text in notes {
        inside.append(&note(&text));
    }
    scroller.set_child(Some(&inside));
    page.append(&scroller);
    show_page(menu, "choices", page.upcast_ref(), transition);
}

fn options(menu: &Menu, section: Section, seen: &Seen, within: &[&Photo]) -> (Vec<Choice>, Vec<String>) {
    let state = &menu.state;
    let narrow = state.libraries.narrow.borrow().clone();
    let filter = state.libraries.filter.borrow().clone();
    let same = |a: Option<&str>, b: &str| a.is_some_and(|a| a.eq_ignore_ascii_case(b));
    let mut rows: Vec<Choice> = Vec::new();
    match section {
        Section::Library => rows = library_choices(menu),
        Section::Places => return place_choices(menu, seen, within, narrow.place.as_deref()),
        Section::Things => return thing_choices(menu, seen, within),
        Section::Analyse => rows = analyse_choices(menu, seen, &filter),
        Section::Folder => rows = folder_choices(menu, seen, within),
        Section::People => {
            for (name, n) in tally(within.iter().flat_map(|photo| seen.people.get(&photo.id).into_iter().flatten().map(String::as_str))) {
                let (name, on) = (name.to_string(), same(narrow.person.as_deref(), name));
                rows.push((line(&name, &count(n), Some(on), false, false), narrowing(menu, move |narrow| narrow.person = (!on).then(|| name.clone()))));
            }
        }
        Section::Cameras => {
            for (camera, n) in tally(within.iter().filter_map(|photo| seen.cameras.get(&photo.id).map(String::as_str))) {
                let (camera, on) = (camera.to_string(), same(narrow.camera.as_deref(), camera));
                rows.push((line(&camera, &count(n), Some(on), false, false), narrowing(menu, move |narrow| narrow.camera = (!on).then(|| camera.clone()))));
            }
        }
        Section::Dates => {
            let years: Vec<String> = within.iter().map(|photo| day_of(photo)[..4].to_string()).collect();
            let mut years = tally(years.iter().map(String::as_str));
            years.sort_by(|a, b| b.0.cmp(a.0));
            for (year, n) in years {
                let (year, on) = (year.to_string(), narrow.date.as_deref() == Some(year));
                rows.push((line(&year, &count(n), Some(on), false, false), narrowing(menu, move |narrow| narrow.date = (!on).then(|| year.clone()))));
            }
        }
        Section::Flag => {
            for (label, flag) in FLAGS {
                let on = filter.flag == Some(flag);
                let n = within.iter().filter(|photo| photo.flag == flag).count();
                rows.push((line(label, &count(n), Some(on), false, false), filtering(menu, move |filter| filter.flag = (!on).then_some(flag))));
            }
        }
        Section::Rating => {
            for stars in 1..=5u8 {
                let on = filter.min_rating == stars;
                let n = within.iter().filter(|photo| photo.rating >= stars).count();
                rows.push((line(&rated(stars), &count(n), Some(on), false, false), filtering(menu, move |filter| filter.min_rating = if on { 0 } else { stars })));
            }
        }
        Section::Type => {
            for (label, kind) in [("RAW Only", FileType::Raw), ("JPEG and Others", FileType::NotRaw)] {
                let on = filter.file_type == kind;
                let n = within.iter().filter(|photo| kind.admits(&photo.path)).count();
                rows.push((line(label, &count(n), Some(on), false, false), filtering(menu, move |filter| filter.file_type = if on { FileType::Any } else { kind })));
            }
        }
    }
    (rows, Vec::new())
}

fn library_choices(menu: &Menu) -> Vec<Choice> {
    let state = &menu.state;
    let here = current_place(state);
    let going = |place: Place| -> Rc<dyn Fn()> {
        let (state, popover) = (state.clone(), menu.popover.clone());
        Rc::new(move || {
            popover.popdown();
            show_place(&state, place.clone());
        })
    };
    let mut rows = vec![(
        line("All Libraries", &count(places::row(state, &Place::Everywhere).count as usize), Some(matches!(here, Place::Everywhere)), false, false),
        going(Place::Everywhere),
    )];
    for place in state.libraries.places.borrow().iter().filter(|place| matches!(place, Place::Library(_))) {
        let chosen = matches!((&here, place), (Place::Library(a), Place::Library(b)) if a.id == b.id);
        let read = places::row(state, place);
        rows.push((line(&places::name_of(state, place), &count(read.count as usize), Some(chosen), false, false), going(place.clone())));
    }
    rows
}

fn place_choices(menu: &Menu, seen: &Seen, within: &[&Photo], chosen: Option<&str>) -> (Vec<Choice>, Vec<String>) {
    let looking = {
        let asked = menu.clone();
        spots::read(&menu.state, &seen.photos, move || again(&asked))
    };
    let towns: Vec<&'static Town> = within.iter().filter_map(|photo| spots::town_of(photo)).collect();
    let mut rows = Vec::new();
    let place = |name: &str, n: usize, indent: bool| -> Choice {
        let (name, on) = (name.to_string(), chosen.is_some_and(|chosen| chosen.eq_ignore_ascii_case(name)));
        (line(&name, &count(n), Some(on), false, indent), narrowing(menu, move |narrow| narrow.place = (!on).then(|| name.clone())))
    };
    for (country, n) in tally(towns.iter().map(|town| town.country.as_str())) {
        rows.push(place(country, n, false));
        for (town, n) in tally(towns.iter().filter(|town| town.country == country).map(|town| town.name.as_str())) {
            rows.push(place(town, n, true));
        }
    }
    let notes = match (looking, rows.is_empty()) {
        (true, _) => vec!["Looking where these were taken…".to_string()],
        (false, true) => vec!["No photograph here says where it was taken.".to_string()],
        (false, false) => Vec::new(),
    };
    (rows, notes)
}

fn thing_choices(menu: &Menu, seen: &Seen, within: &[&Photo]) -> (Vec<Choice>, Vec<String>) {
    let Some(saw) = &seen.saw else {
        return (Vec::new(), vec!["Numa has not read these photographs for what is in them yet.".to_string()]);
    };
    let chosen = words::thing(&menu.state);
    let names: Vec<&str> = within.iter().flat_map(|photo| saw.get(&photo.id).into_iter().flatten().map(|at| numa::io::words::THINGS[*at])).collect();
    let mut rows = Vec::new();
    for (thing, n) in tally(names.into_iter()) {
        let Some(at) = numa::io::words::THINGS.iter().position(|known| *known == thing) else { continue };
        let on = chosen == Some(at);
        let (state, popover) = (menu.state.clone(), menu.popover.clone());
        let deed: Rc<dyn Fn()> = Rc::new(move || {
            words::set_thing(&state, (!on).then_some(at));
            popover.popdown();
            reload_grid(&state);
        });
        let row = line(thing, &count(n), Some(on), false, false);
        row.add_css_class("saw-choice");
        rows.push((row, deed));
    }
    (rows, Vec::new())
}

fn analyse_choices(menu: &Menu, seen: &Seen, filter: &Filter) -> Vec<Choice> {
    let mut rows = Vec::new();
    for (label, questionable) in [("Only Questionable", true), ("Only Best of Each Burst", false)] {
        let on = if questionable { filter.only_questionable } else { filter.best_of_burst };
        let pass = Filter {
            min_rating: 0,
            flag: None,
            file_type: FileType::Any,
            only_questionable: questionable || filter.only_questionable,
            best_of_burst: !questionable || filter.best_of_burst,
            ..filter.clone()
        };
        let n = pipeline(&menu.state, &pass).iter().filter(|photo| keeps(&menu.state, seen, photo, Section::Analyse)).count();
        let deed = filtering(menu, move |filter| match questionable {
            true => filter.only_questionable = !on,
            false => filter.best_of_burst = !on,
        });
        rows.push((line(label, &count(n), Some(on), false, false), deed));
    }
    rows
}

fn folder_choices(menu: &Menu, seen: &Seen, within: &[&Photo]) -> Vec<Choice> {
    let Some(root) = &seen.root else { return Vec::new() };
    let state = &menu.state;
    let chosen = state.libraries.folder.borrow().clone();
    let mut rows = Vec::new();
    for folder in state.libraries.folders.borrow().iter() {
        let n = within.iter().filter(|photo| photo.path.starts_with(root.join(folder))).count();
        let name = folder.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
        let on = chosen.as_ref() == Some(folder);
        let (state, popover, folder) = (state.clone(), menu.popover.clone(), folder.clone());
        let deep = folder.components().count() > 1;
        let deed: Rc<dyn Fn()> = Rc::new(move || {
            state.libraries.folder.replace((!on).then(|| folder.clone()));
            popover.popdown();
            reload_grid(&state);
        });
        rows.push((line(&name, &count(n), Some(on), false, deep), deed));
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_rating_reads_as_home_says_it() {
        assert_eq!(rated(3), "★ 3+");
        assert_eq!(rated(5), "★ 5");
    }

    #[test]
    fn a_tally_is_most_first_then_by_name() {
        let names = ["Kyoto", "Tokyo", "Kyoto", "Osaka", "Tokyo", "Kyoto"];
        assert_eq!(tally(names.into_iter()), [("Kyoto", 3), ("Tokyo", 2), ("Osaka", 1)]);
    }
}
