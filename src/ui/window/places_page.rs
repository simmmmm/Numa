use super::*;
use libshumate::prelude::*;
use numa::io::places::Place as Town;

const LIST_WIDTH: i32 = 300;

const PIN: f32 = 28.0;

const COUNTRY_OPENS: f64 = 4.0;
const REGION_OPENS: f64 = 6.0;
const APART: f64 = 64.0;

const PADDING: f64 = 80.0;
const FIT_MOST: f64 = 10.0;
const TOWN_MOST: f64 = 12.0;
const AREA_MOST: f64 = 9.0;

#[derive(Clone, Copy, PartialEq)]
enum Level {
    Country,
    Region,
    Town,
}

#[derive(Clone)]
struct Node {
    name: String,
    level: Level,
    key: String,
    count: usize,
    ids: Vec<i64>,
    latitude: f64,
    longitude: f64,
    area: [f64; 4],
    kids: Vec<Node>,
}

impl Node {
    fn new(name: &str, level: Level, parent: &str) -> Self {
        let key = if parent.is_empty() { name.to_string() } else { format!("{parent}/{name}") };
        Node { name: name.to_string(), level, key, count: 0, ids: Vec::new(), latitude: 0.0, longitude: 0.0, area: [180.0, 90.0, -180.0, -90.0], kids: Vec::new() }
    }

    fn take(&mut self, id: i64, latitude: f64, longitude: f64) {
        self.count += 1;
        if self.ids.len() < 3 {
            self.ids.push(id);
        }
        self.area = [self.area[0].min(longitude), self.area[1].min(latitude), self.area[2].max(longitude), self.area[3].max(latitude)];
    }

    fn find(&self, key: &str) -> Option<&Node> {
        if self.key == key {
            return Some(self);
        }
        self.kids.iter().find_map(|kid| kid.find(key))
    }
}

fn child<'a>(kids: &'a mut Vec<Node>, name: &str, level: Level, parent: &str) -> &'a mut Node {
    let at = match kids.iter().position(|kid| kid.name == name) {
        Some(at) => at,
        None => {
            kids.push(Node::new(name, level, parent));
            kids.len() - 1
        }
    };
    &mut kids[at]
}

fn tree(points: &[(i64, f64, f64, &Town)]) -> Vec<Node> {
    let mut countries: Vec<Node> = Vec::new();
    for &(id, latitude, longitude, town) in points {
        let land = child(&mut countries, &town.country, Level::Country, "");
        land.take(id, latitude, longitude);
        let area = match town.region.is_empty() {
            true => land,
            false => {
                let key = land.key.clone();
                let area = child(&mut land.kids, &town.region, Level::Region, &key);
                area.take(id, latitude, longitude);
                area
            }
        };
        let key = area.key.clone();
        let here = child(&mut area.kids, &town.name, Level::Town, &key);
        here.take(id, latitude, longitude);
        here.latitude += latitude;
        here.longitude += longitude;
    }
    fn finish(node: &mut Node) {
        node.kids.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.name.cmp(&b.name)));
        node.kids.iter_mut().for_each(finish);
        for kid in node.kids.iter_mut() {
            if kid.level == Level::Region && kid.kids.len() == 1 {
                *kid = kid.kids.remove(0);
            }
        }
        match node.level {
            Level::Town => (node.latitude, node.longitude) = (node.latitude / node.count as f64, node.longitude / node.count as f64),
            _ => (node.latitude, node.longitude) = node.kids.first().map_or((0.0, 0.0), |kid| (kid.latitude, kid.longitude)),
        }
    }
    countries.iter_mut().for_each(finish);
    countries.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.name.cmp(&b.name)));
    countries
}

fn shown_at(tree: &[Node], zoom: f64) -> Vec<Vec<&Node>> {
    let at = |node: &Node| map_view::world_pixel(node.latitude, node.longitude, zoom);
    let apart = |kids: &[Node]| {
        kids.iter().enumerate().all(|(i, a)| kids[i + 1..].iter().all(|b| {
            let ((ax, ay), (bx, by)) = (at(a), at(b));
            (ax - bx).hypot(ay - by) >= APART
        }))
    };
    let mut shown: Vec<&Node> = Vec::new();
    fn visit<'a>(node: &'a Node, zoom: f64, apart: &dyn Fn(&[Node]) -> bool, shown: &mut Vec<&'a Node>) {
        let opens = match node.level {
            Level::Country => COUNTRY_OPENS,
            Level::Region => REGION_OPENS,
            Level::Town => f64::INFINITY,
        };
        match !node.kids.is_empty() && zoom >= opens && apart(&node.kids) {
            true => node.kids.iter().for_each(|kid| visit(kid, zoom, apart, shown)),
            false => shown.push(node),
        }
    }
    tree.iter().for_each(|node| visit(node, zoom, &apart, &mut shown));
    shown.sort_by(|a, b| b.count.cmp(&a.count));
    let mut stacks: Vec<((f64, f64), Vec<&Node>)> = Vec::new();
    for node in shown {
        let (x, y) = at(node);
        match stacks.iter_mut().find(|((sx, sy), _)| (sx - x).hypot(sy - y) < APART) {
            Some((_, places)) => places.push(node),
            None => stacks.push(((x, y), vec![node])),
        }
    }
    stacks.into_iter().map(|(_, places)| places).collect()
}

#[derive(Clone, Default, PartialEq)]
struct Chips {
    picks: bool,
    stars: bool,
    year: Option<String>,
}

struct Coming {
    line: gtk::Box,
    said: gtk::Label,
    spinner: gtk::Spinner,
    bar: gtk::ProgressBar,
}

impl Coming {
    fn new(words_at: gtk::Align) -> Self {
        let spinner = spinner("Bringing the map");
        let said = gtk::Label::new(None);
        said.add_css_class("numeric");
        let words = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        words.append(&spinner);
        words.append(&said);
        words.set_halign(words_at);
        let bar = progress_on("map-coming-bar");
        let line = gtk::Box::new(gtk::Orientation::Vertical, 8);

        line.add_css_class("numa-content");
        line.append(&words);
        line.append(&bar);
        Self { line, said, spinner, bar }
    }

    fn show(&self, lead: &str, progress: Option<(u64, u64)>) {
        let megabytes = |bytes: u64| (bytes + 500_000) / 1_000_000;
        self.said.set_text(&match progress {
            Some((done, total)) => format!("{lead} {} of {} MB", megabytes(done), megabytes(total)),
            None => lead.to_string(),
        });
        self.spinner.set_visible(progress.is_none());
        self.bar.set_visible(progress.is_some());
        self.bar.set_fraction(progress.map_or(0.0, |(done, total)| done as f64 / total as f64));
    }
}

struct Page {
    list: gtk::ListBox,
    countries: gtk::Label,
    none: gtk::Label,
    stage: gtk::Stack,
    overlay: gtk::Overlay,
    note: adw::StatusPage,

    world: Coming,
    again: gtk::Button,
    streets: Coming,
    quick: Vec<gtk::ToggleButton>,
    year: gtk::MenuButton,
    pop: gtk::Popover,
    map: RefCell<Option<(libshumate::Map, libshumate::MarkerLayer)>>,

    tree: RefCell<Vec<Node>>,
    photos: RefCell<HashMap<i64, Photo>>,
    years: RefCell<Vec<(String, usize)>>,
    chips: RefCell<Chips>,

    open: RefCell<std::collections::HashSet<String>>,
    chosen: RefCell<Option<String>>,
    markers: RefCell<Vec<(String, libshumate::Marker, gtk::Button)>>,
    rows: RefCell<Vec<(gtk::ListBoxRow, String)>>,
    fitted: Cell<bool>,

    at: Cell<Option<(f64, f64)>>,
    placing: Cell<bool>,

    towns: Cell<Option<usize>>,
}

thread_local! {
    static PAGE: RefCell<Option<Rc<Page>>> = const { RefCell::new(None) };
}

fn current() -> Option<Rc<Page>> {
    PAGE.with(|page| page.borrow().clone())
}

pub(super) fn count() -> Option<usize> {
    current().and_then(|page| page.towns.get())
}

pub(super) fn count_at_start(state: &App) {
    let Some(page) = current() else { return };
    let all = photographs(state);
    let again = glib::clone!(
        #[strong] state,
        move || count_at_start(&state)
    );
    if !spots::read(state, &all, again) {
        tally(state, &page, &all);
    }
}

fn tally(state: &App, page: &Page, all: &[Photo]) {
    let towns: std::collections::HashSet<(&str, &str)> = all.iter().filter_map(spots::town_of).map(|town| (town.name.as_str(), town.country.as_str())).collect();
    if page.towns.replace(Some(towns.len())) != Some(towns.len()) {
        sidebar::fill(state);
    }
}

pub(super) fn build(state: &App) -> gtk::Box {
    let title = gtk::Label::new(Some("Places"));
    title.add_css_class("title-2");
    title.set_xalign(0.0);
    title.set_hexpand(true);
    let countries = gtk::Label::new(None);
    countries.add_css_class("caption");
    countries.add_css_class("dim-label");
    let head = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    head.add_css_class("place-list-head");
    head.append(&title);
    head.append(&countries);
    let list = gtk::ListBox::new();
    list.add_css_class("navigation-sidebar");
    let scroller = gtk::ScrolledWindow::new();
    scroller.set_hscrollbar_policy(gtk::PolicyType::Never);
    scroller.set_vexpand(true);
    scroller.set_child(Some(&list));
    let foot = gtk::Box::new(gtk::Orientation::Vertical, 4);
    foot.add_css_class("place-list-foot");
    let none = gtk::Label::new(None);
    none.set_xalign(0.0);
    none.set_wrap(true);
    none.add_css_class("caption");
    none.add_css_class("dim-label");
    none.set_visible(false);
    foot.append(&none);
    let column = gtk::Box::new(gtk::Orientation::Vertical, 0);
    column.set_size_request(LIST_WIDTH, -1);

    column.set_hexpand(false);
    column.append(&head);
    column.append(&scroller);
    column.append(&foot);

    let (note, world, again) = note(state);
    let streets = Coming::new(gtk::Align::Start);
    streets.said.add_css_class("caption");
    streets.line.add_css_class("map-coming");
    streets.line.set_halign(gtk::Align::Start);
    streets.line.set_valign(gtk::Align::End);
    streets.line.set_visible(false);
    let stage = gtk::Stack::new();
    stage.set_hexpand(true);
    stage.set_vexpand(true);
    stage.add_named(&note, Some("note"));
    let overlay = gtk::Overlay::new();
    overlay.set_hexpand(true);
    overlay.set_child(Some(&stage));
    let quick: Vec<gtk::ToggleButton> = ["All", "Picks", "★ 3+"].iter().map(|label| gtk::ToggleButton::with_label(label)).collect();
    let year = gtk::MenuButton::new();
    year.set_label("All Years");
    let chips = chip_row();
    quick.iter().for_each(|chip| chips.append(chip));
    let line = gtk::Separator::new(gtk::Orientation::Vertical);
    line.set_margin_start(4);
    line.set_margin_end(4);
    chips.append(&line);
    chips.append(&year);
    chips.add_css_class("map-chips");
    chips.set_halign(gtk::Align::Start);
    chips.set_valign(gtk::Align::Start);
    overlay.add_overlay(&chips);
    overlay.add_overlay(&streets.line);
    let zoom = zoom_buttons();
    let credit = gtk::Label::new(Some(map_view::CREDIT));
    credit.add_css_class("caption");
    credit.add_css_class("map-credit");
    let pop = gtk::Popover::new();
    pop.set_parent(&overlay);
    pop.add_css_class("map-pop");
    let page = Rc::new(Page {
        list,
        countries,
        none,
        stage,
        overlay: overlay.clone(),
        note,
        world,
        again,
        streets,
        quick,
        year,
        pop,
        map: RefCell::default(),
        tree: RefCell::default(),
        photos: RefCell::default(),
        years: RefCell::default(),
        chips: RefCell::default(),
        open: RefCell::default(),
        chosen: RefCell::default(),
        markers: RefCell::default(),
        rows: RefCell::default(),
        fitted: Cell::new(false),
        at: Cell::new(None),
        placing: Cell::new(false),
        towns: Cell::new(None),
    });
    overlay.add_overlay(&zoom);
    credit.set_halign(gtk::Align::End);
    credit.set_valign(gtk::Align::End);
    overlay.add_overlay(&credit);
    for (at, chip) in page.quick.iter().enumerate() {
        chip.set_focus_on_click(false);
        chip.connect_toggled(glib::clone!(
            #[strong] state,
            move |chip| {
                let Some(page) = current() else { return };
                if state.applying.get() {
                    return;
                }
                {
                    let mut chips = page.chips.borrow_mut();
                    match at {
                        0 => (chips.picks, chips.stars) = (false, false),
                        1 => chips.picks = chip.is_active(),
                        _ => chips.stars = chip.is_active(),
                    }
                }
                fill(&state);
            }
        ));
    }
    page.list.connect_row_activated(glib::clone!(
        #[strong] state,
        move |_, row| {
            let Some(page) = current() else { return };
            let key = page.rows.borrow().iter().find(|(one, _)| one == row).map(|(_, key)| key.clone());
            if let Some(key) = key {
                choose(&state, &page, &key, true);
            }
        }
    ));
    let box_ = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    box_.append(&column);
    box_.append(&gtk::Separator::new(gtk::Orientation::Vertical));
    box_.append(&overlay);
    PAGE.with(|slot| slot.replace(Some(page)));
    box_
}

fn note(state: &App) -> (adw::StatusPage, Coming, gtk::Button) {
    let note = adw::StatusPage::new();
    note.set_title("Places");
    let world = Coming::new(gtk::Align::Center);
    let again = primary_button("Try Again");
    again.add_css_class("pill");
    again.set_halign(gtk::Align::Center);
    again.connect_clicked(glib::clone!(
        #[strong] state,
        move |_| {
            if let Some(page) = current() {
                bring(&state);
                show_map(&state, &page);
            }
        }
    ));
    let under = gtk::Box::new(gtk::Orientation::Vertical, 0);
    under.append(&world.line);
    under.append(&again);

    let clamp = adw::Clamp::new();
    clamp.set_maximum_size(LIST_WIDTH);
    clamp.set_child(Some(&under));
    note.set_child(Some(&clamp));
    (note, world, again)
}

fn zoom_buttons() -> gtk::Box {
    let zoom = gtk::Box::new(gtk::Orientation::Vertical, 8);
    zoom.set_halign(gtk::Align::End);
    zoom.set_valign(gtk::Align::End);
    zoom.add_css_class("map-zoom");
    for (icon, tip, inward) in [("list-add-symbolic", "Zoom In", true), ("list-remove-symbolic", "Zoom Out", false)] {
        let button = gtk::Button::from_icon_name(icon);
        button.add_css_class("osd");
        button.add_css_class("circular");
        button.set_tooltip_text(Some(tip));
        button.connect_clicked(move |_| {
            let Some(page) = current() else { return };
            let map = page.map.borrow().as_ref().map(|(map, _)| map.clone());
            match (map, inward) {
                (Some(map), true) => map.zoom_in(),
                (Some(map), false) => map.zoom_out(),
                (None, _) => {}
            }
        });
        zoom.append(&button);
    }
    zoom
}

pub(super) fn show(state: &App) {
    state.stack.set_visible_child_name("places");
    fill(state);
}

pub(super) fn show_at(state: &App, latitude: f64, longitude: f64) {
    if let Some(page) = current() {
        page.at.set(Some((latitude, longitude)));
    }
    show(state);
}

fn photographs(state: &App) -> Vec<Photo> {
    let every = Filter { all_libraries: true, reversed: true, ..Filter::default() };
    let mut photos = state.catalog.photos_everywhere(&every).unwrap_or_default();
    photos.retain(|photo| !state.catalog.is_offline(numa::io::catalog::library_of(photo.id)));
    photos
}

fn year_of(photo: &Photo) -> String {
    numa::io::import::day(photo.taken.unwrap_or(photo.mtime), 0)[..4].to_string()
}

fn liked(photo: &Photo) -> bool {
    photo.flag == Flag::Picked || photo.rating >= 3
}

fn fill(state: &App) {
    let Some(page) = current() else { return };
    let all = photographs(state);
    let looking = spots::read(state, &all, glib::clone!(
        #[strong] state,
        move || {
            if state.stack.visible_child_name().as_deref() == Some("places") {
                fill(&state);
            }

            bring(&state);
        }
    ));
    let chips = page.chips.borrow().clone();
    let shown: Vec<&Photo> = all
        .iter()
        .filter(|photo| (!chips.picks || photo.flag == Flag::Picked) && (!chips.stars || photo.rating >= 3))
        .filter(|photo| chips.year.as_ref().is_none_or(|year| year_of(photo) == *year))
        .collect();
    let mut years: HashMap<String, usize> = HashMap::new();
    all.iter().for_each(|photo| *years.entry(year_of(photo)).or_default() += 1);
    let mut years: Vec<(String, usize)> = years.into_iter().collect();
    years.sort_by(|a, b| b.0.cmp(&a.0));
    *page.years.borrow_mut() = years;
    let mut none = 0;
    let mut points = Vec::new();
    for photo in shown.iter().filter(|photo| liked(photo)).chain(shown.iter().filter(|photo| !liked(photo))) {
        match spots::spot_of(photo) {
            Some(Some(spots::Spot { at, town: Some(town) })) => points.push((photo.id, f64::from(at.0), f64::from(at.1), town)),
            Some(_) => none += 1,
            None => {}
        }
    }
    let tree = tree(&points);
    if !looking {
        tally(state, &page, &all);
    }
    page.countries.set_text(&match tree.len() {
        0 => String::new(),
        1 => "1 country".to_string(),
        n => format!("{} countries", places::grouped(n as i64)),
    });
    page.none.set_text(&match (looking, none) {
        (true, _) => "Looking where these were taken…".to_string(),
        (false, 1) => "1 photo has no place.".to_string(),
        (false, n) => format!("{} photos have no place.", places::grouped(n as i64)),
    });
    page.none.set_visible(looking || none > 0);
    *page.photos.borrow_mut() = shown.into_iter().map(|photo| (photo.id, photo.clone())).collect();
    *page.tree.borrow_mut() = tree;
    if page.chosen.borrow().as_ref().is_some_and(|key| !page.tree.borrow().iter().any(|node| node.find(key).is_some())) {
        page.chosen.replace(None);
        page.pop.popdown();
    }
    show_chips(state, &page);
    fill_list(state, &page);
    show_map(state, &page);
}

fn show_chips(state: &App, page: &Rc<Page>) {
    let chips = page.chips.borrow().clone();
    let applying = state.applying.replace(true);
    for (chip, on) in page.quick.iter().zip([!chips.picks && !chips.stars, chips.picks, chips.stars]) {
        chip.set_active(on);
    }
    state.applying.set(applying);
    page.year.set_label(chips.year.as_deref().unwrap_or("All Years"));
    let list = gtk::ListBox::new();
    list.add_css_class("navigation-sidebar");
    list.set_selection_mode(gtk::SelectionMode::None);
    let choices: Vec<(Option<String>, String, Option<usize>)> = std::iter::once((None, "All Years".to_string(), None))
        .chain(page.years.borrow().iter().map(|(year, n)| (Some(year.clone()), year.clone(), Some(*n))))
        .collect();
    for (_, label, n) in &choices {
        let line = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        let name = gtk::Label::new(Some(label));
        name.set_xalign(0.0);
        name.set_hexpand(true);
        line.append(&name);
        if let Some(n) = n {
            let count = gtk::Label::new(Some(&places::grouped(*n as i64)));
            count.add_css_class("caption");
            count.add_css_class("dim-label");
            line.append(&count);
        }
        list.append(&line);
    }
    let popover = gtk::Popover::new();
    popover.set_child(Some(&list));
    list.connect_row_activated(glib::clone!(
        #[strong] state,
        #[weak] popover,
        move |_, row| {
            let Some(page) = current() else { return };
            let Some((year, ..)) = usize::try_from(row.index()).ok().and_then(|at| choices.get(at)) else { return };
            page.chips.borrow_mut().year = year.clone();
            popover.popdown();
            fill(&state);
        }
    ));
    page.year.set_popover(Some(&popover));
}

fn fill_list(state: &App, page: &Rc<Page>) {
    page.list.remove_all();
    let mut rows = Vec::new();
    let tree = page.tree.borrow();
    if tree.is_empty() && !page.none.is_visible() {
        let row = gtk::ListBoxRow::new();
        row.set_selectable(false);
        row.set_activatable(false);
        let none = gtk::Label::new(Some("No photograph here says where it was taken."));
        none.set_wrap(true);
        none.set_xalign(0.0);
        none.add_css_class("dim-label");
        row.set_child(Some(&none));
        page.list.append(&row);
    }
    fn add(state: &App, page: &Rc<Page>, node: &Node, depth: i32, rows: &mut Vec<(gtk::ListBoxRow, String)>) {
        let name = gtk::Label::new(Some(&node.name));
        name.set_xalign(0.0);
        name.set_hexpand(true);
        name.set_ellipsize(gtk::pango::EllipsizeMode::End);
        let count = gtk::Label::new(Some(&places::grouped(node.count as i64)));
        count.add_css_class("caption");
        count.add_css_class("dim-label");
        count.add_css_class("numeric");
        let line = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        line.set_margin_start(depth * 16);
        line.append(&name);
        line.append(&count);
        let open = page.open.borrow().contains(&node.key);
        if !node.kids.is_empty() {
            let fold = gtk::Button::from_icon_name(if open { "pan-down-symbolic" } else { "pan-end-symbolic" });
            fold.add_css_class("flat");
            fold.add_css_class("circular");
            fold.add_css_class("side-fold");
            fold.set_valign(gtk::Align::Center);
            fold.set_tooltip_text(Some(&format!("Places in {}", node.name)));
            let key = node.key.clone();
            fold.connect_clicked(glib::clone!(
                #[strong] state,
                move |_| {
                    let Some(page) = current() else { return };
                    {
                        let mut open = page.open.borrow_mut();
                        if !open.remove(&key) {
                            open.insert(key.clone());
                        }
                    }
                    fill_list(&state, &page);
                }
            ));
            line.append(&fold);
        }
        let row = gtk::ListBoxRow::new();
        row.set_child(Some(&line));
        page.list.append(&row);
        rows.push((row, node.key.clone()));
        if open {
            node.kids.iter().for_each(|kid| add(state, page, kid, depth + 1, rows));
        }
    }
    tree.iter().for_each(|node| add(state, page, node, 0, &mut rows));
    let chosen = page.chosen.borrow().clone();
    let row = rows.iter().find(|(_, key)| Some(key) == chosen.as_ref()).map(|(row, _)| row.clone());
    page.list.select_row(row.as_ref());
    *page.rows.borrow_mut() = rows;
}

fn show_map(state: &App, page: &Rc<Page>) {
    let file = map_view::file();
    if !numa::io::map::ready(&file) {

        let failed = map_view::failed();
        if failed.is_none() {
            bring(state);
        }
        page.note.set_description(Some(failed.as_deref().unwrap_or("The world once, about 45 MB; it stays on this computer.")));
        page.world.line.set_visible(failed.is_none());
        page.again.set_visible(failed.is_some());
        show_coming(page);
        page.stage.set_visible_child_name("note");
        return;
    }
    if page.map.borrow().is_none() {
        let Some((map, markers)) = map_view::new_map() else {
            page.note.set_description(Some("This computer's libshumate cannot draw the map."));
            page.world.line.set_visible(false);
            page.again.set_visible(false);
            page.stage.set_visible_child_name("note");
            return;
        };
        map.add_css_class("places-map");
        if let Some(viewport) = map.viewport() {
            viewport.connect_zoom_level_notify(glib::clone!(
                #[strong] state,
                move |_| place_soon(&state)
            ));
        }
        page.stage.add_named(&map, Some("map"));
        page.map.replace(Some((map, markers)));
    }
    page.stage.set_visible_child_name("map");
    if let (Some((latitude, longitude)), Some((map, _))) = (page.at.take(), page.map.borrow().as_ref()) {
        if let Some(viewport) = map.viewport() {
            viewport.set_zoom_level(TOWN_MOST);
        }
        map.center_on(latitude, longitude);
        page.fitted.set(true);
    }
    if !page.fitted.get() {

        let state = state.clone();
        glib::idle_add_local_once(move || {
            let Some(page) = current() else { return };
            let Some((map, _)) = page.map.borrow().clone() else { return };
            if map.width() == 0 {
                return;
            }
            if let Some(area) = everywhere(&page.tree.borrow()) {
                map_view::fit(&map, area, PADDING, FIT_MOST);
                page.fitted.set(true);
            }
            place(&state, &page);
        });
    }
    place(state, page);
}

fn everywhere(tree: &[Node]) -> Option<[f64; 4]> {
    let first = tree.first()?;
    let all = tree.iter().fold([180.0f64, 90.0, -180.0, -90.0], |all, node| {
        [all[0].min(node.area[0]), all[1].min(node.area[1]), all[2].max(node.area[2]), all[3].max(node.area[3])]
    });
    Some(if all[2] - all[0] > 200.0 { first.area } else { all })
}

fn bring(state: &App) {
    if map_view::bringing() {
        return;
    }
    map_view::bring(glib::clone!(
        #[strong] state,
        move |came| {
            let Some(page) = current() else { return };
            show_coming(&page);
            if let (true, Some((map, _))) = (came, page.map.borrow().as_ref()) {
                map_view::refresh(map);
            }
            if state.stack.visible_child_name().as_deref() == Some("places") && page.map.borrow().is_none() {
                show_map(&state, &page);
            }
        }
    ));
    let state = state.clone();
    glib::timeout_add_local(std::time::Duration::from_millis(500), move || {
        let Some(page) = current() else { return glib::ControlFlow::Break };

        if page.map.borrow().is_none() && numa::io::map::ready(&map_view::file()) && state.stack.visible_child_name().as_deref() == Some("places") {
            show_map(&state, &page);
        }
        show_coming(&page);
        match map_view::bringing() {
            true => glib::ControlFlow::Continue,
            false => glib::ControlFlow::Break,
        }
    });
}

fn show_coming(page: &Page) {
    let progress = map_view::progress();
    match page.map.borrow().is_none() {
        true => page.world.show("Bringing the map…", progress),
        false => page.streets.show("Bringing the streets around new places…", progress),
    }
    page.streets.line.set_visible(page.map.borrow().is_some() && map_view::bringing());
}

fn place_soon(state: &App) {
    let Some(page) = current() else { return };

    page.pop.popdown();
    if page.placing.replace(true) {
        return;
    }
    let state = state.clone();
    glib::idle_add_local_once(move || {
        if let Some(page) = current() {
            page.placing.set(false);
            place(&state, &page);
        }
    });
}

fn place(state: &App, page: &Rc<Page>) {
    let Some((map, layer)) = page.map.borrow().clone() else { return };
    let Some(viewport) = map.viewport() else { return };
    let tree = page.tree.borrow();
    let shown = shown_at(&tree, viewport.zoom_level());
    let keyed: Vec<(String, Vec<&Node>)> = shown.into_iter().map(|places| (places.iter().map(|node| node.key.as_str()).collect::<Vec<_>>().join("|"), places)).collect();
    let mut markers = page.markers.borrow_mut();
    markers.retain(|(key, marker, _)| {
        let stays = keyed.iter().any(|(wanted, _)| wanted == key);
        if !stays {
            layer.remove_marker(marker);
        }
        stays
    });
    let chosen = page.chosen.borrow().clone();
    for (key, places) in keyed {
        if markers.iter().any(|(had, ..)| *had == key) {
            continue;
        }
        let node = places[0];
        let count: usize = places.iter().map(|one| one.count).sum();
        let photos = page.photos.borrow();
        let covers: Vec<(PathBuf, i64, Option<String>)> = node
            .ids
            .iter()
            .filter_map(|id| photos.get(id))
            .map(|photo| (photo.path.clone(), photo.mtime, state.catalog.edits_json(photo.id).ok().flatten()))
            .collect();
        let fan = places::fan(PIN);
        places::show_covers(&fan, &covers, PIN);
        let badge = gtk::Label::new(Some(&places::grouped(count as i64)));
        badge.add_css_class("map-count");
        badge.set_halign(gtk::Align::End);
        badge.set_valign(gtk::Align::Start);
        let stack = gtk::Overlay::new();
        stack.set_child(Some(&fan));
        stack.add_overlay(&badge);
        let pin = gtk::Button::new();
        pin.add_css_class("flat");
        pin.add_css_class("map-pin");
        pin.set_child(Some(&stack));
        pin.set_tooltip_text(Some(&format!("{}, {}", places.iter().map(|one| one.name.as_str()).collect::<Vec<_>>().join(", "), places::photos(count))));
        if chosen.as_deref() == Some(key.as_str()) {
            pin.add_css_class("chosen");
        }
        let (keys, area) = (
            places.iter().map(|one| one.key.clone()).collect::<Vec<_>>(),
            places.iter().fold([180.0f64, 90.0, -180.0, -90.0], |all, one| {
                [all[0].min(one.area[0]), all[1].min(one.area[1]), all[2].max(one.area[2]), all[3].max(one.area[3])]
            }),
        );
        pin.connect_clicked(glib::clone!(
            #[strong] state,
            move |_| {
                let Some(page) = current() else { return };
                match keys.as_slice() {
                    [one] => choose(&state, &page, one, false),

                    _ => {
                        page.pop.popdown();
                        if let Some((map, _)) = page.map.borrow().as_ref() {
                            let zoom = map.viewport().map_or(1.0, |viewport| viewport.zoom_level());
                            map_view::fit(map, area, PADDING * 1.5, (zoom + 2.0).max(REGION_OPENS));
                        }
                    }
                }
            }
        ));
        let marker = libshumate::Marker::new();
        marker.set_child(Some(&pin));
        marker.set_location(node.latitude, node.longitude);
        layer.add_marker(&marker);
        markers.push((key, marker, pin));
    }
}

fn choose(state: &App, page: &Rc<Page>, key: &str, fly: bool) {
    let node = page.tree.borrow().iter().find_map(|node| node.find(key)).cloned();
    let Some(node) = node else { return };
    page.chosen.replace(Some(key.to_string()));

    {
        let mut open = page.open.borrow_mut();
        let parts: Vec<&str> = key.split('/').collect();
        for depth in 1..parts.len() {
            open.insert(parts[..depth].join("/"));
        }
        if fly {
            open.insert(key.to_string());
        }
    }
    fill_list(state, page);
    for (had, _, pin) in page.markers.borrow().iter() {
        match had == key {
            true => pin.add_css_class("chosen"),
            false => pin.remove_css_class("chosen"),
        }
    }
    let Some((map, _)) = page.map.borrow().clone() else { return };
    if fly {
        map_view::fit(&map, node.area, PADDING * 1.5, if node.level == Level::Town { TOWN_MOST } else { AREA_MOST });
    }
    let state = state.clone();

    glib::idle_add_local_once(move || {
        let Some(page) = current() else { return };
        pop(&state, &page, &map, &node);
    });
}

fn pop(state: &App, page: &Rc<Page>, map: &libshumate::Map, node: &Node) {
    let photos = page.photos.borrow();
    let theirs: Vec<&Photo> = photos.values().filter(|photo| spots::town_of(photo).is_some_and(|town| in_place(town, node))).collect();
    let first = theirs.iter().map(|photo| photo.taken.unwrap_or(photo.mtime)).min();
    let last = theirs.iter().map(|photo| photo.taken.unwrap_or(photo.mtime)).max();
    let said = match places::period(first, last) {
        Some(period) => format!("{} \u{b7} {period}", places::photos(node.count)),
        None => places::photos(node.count),
    };
    let name = gtk::Label::new(Some(&node.name));
    name.add_css_class("heading");
    let caption = gtk::Label::new(Some(&said));
    caption.add_css_class("caption");
    caption.add_css_class("dim-label");
    let head = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    head.append(&name);
    head.append(&caption);
    let strip = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    for id in &node.ids {
        let Some(photo) = photos.get(id) else { continue };
        let picture = gtk::Picture::new();
        picture.set_content_fit(gtk::ContentFit::Cover);
        picture.set_size_request(STRIP_WIDTH, STRIP_HEIGHT);
        picture.add_css_class("print");
        picture.set_overflow(gtk::Overflow::Hidden);
        let (wanted, shown) = (picture.downgrade(), picture.downgrade());
        crate::ui::thumbnail::load_thumbnail_while(
            &photo.path,
            photo.mtime,
            GRID_THUMB_EDGE,
            state.catalog.edits_json(photo.id).ok().flatten(),
            None,
            move || wanted.upgrade().is_some(),
            move |texture| {
                if let Some(picture) = shown.upgrade() {
                    picture.set_paintable(Some(&texture));
                }
            },
        );

        let clamp = adw::Clamp::new();
        clamp.set_maximum_size(STRIP_WIDTH);
        clamp.set_tightening_threshold(STRIP_WIDTH);
        clamp.set_child(Some(&picture));
        strip.append(&clamp);
    }
    let show = primary_button(&format!("Show {}", places::grouped(node.count as i64)));
    show.set_halign(gtk::Align::Start);
    let name_of = node.name.clone();
    show.connect_clicked(glib::clone!(
        #[strong] state,
        move |_| {
            let Some(page) = current() else { return };
            page.pop.popdown();
            let chips = page.chips.borrow().clone();
            show_place(&state, Place::Everywhere);
            {
                let mut filter = state.libraries.filter.borrow_mut();
                if chips.picks {
                    filter.flag = Some(Flag::Picked);
                }
                if chips.stars {
                    filter.min_rating = 3;
                }
            }
            {
                let mut narrow = state.libraries.narrow.borrow_mut();
                narrow.place = Some(name_of.clone());
                narrow.date = chips.year.clone();
            }
            reload_grid(&state);
        }
    ));
    let card = gtk::Box::new(gtk::Orientation::Vertical, 12);
    card.append(&head);
    card.append(&strip);
    card.append(&show);
    page.pop.set_child(Some(&card));
    let (x, y) = map_view::pixel(map, node.latitude, node.longitude).unwrap_or_default();
    let at = map.compute_point(&page.overlay, &gtk::graphene::Point::new(x as f32, y as f32)).map_or((x, y), |point| (f64::from(point.x()), f64::from(point.y())));
    page.pop.set_pointing_to(Some(&gtk::gdk::Rectangle::new(at.0 as i32 - 16, at.1 as i32 - 16, 32, 32)));
    page.pop.popup();
}

const STRIP_WIDTH: i32 = 88;
const STRIP_HEIGHT: i32 = 64;

fn in_place(town: &Town, node: &Node) -> bool {
    match node.level {
        Level::Country => town.country == node.name,
        Level::Region => town.region == node.name && node.key.starts_with(&format!("{}/", town.country)),
        Level::Town => town.name == node.name && node.key.starts_with(&format!("{}/", town.country)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn town(name: &str, region: &str, country: &str) -> Town {
        Town { name: name.into(), latitude: 0.0, longitude: 0.0, region: region.into(), country: country.into(), people: 0 }
    }

    #[test]
    fn places_are_countries_then_regions_then_towns() {
        let (lisbon, porto, braga, oslo) =
            (town("Lisbon", "Lisbon", "Portugal"), town("Porto", "Norte", "Portugal"), town("Braga", "Norte", "Portugal"), town("Oslo", "Oslo", "Norway"));
        let points = [
            (1, 38.7, -9.1, &lisbon),
            (2, 41.1, -8.6, &porto),
            (3, 41.5, -8.4, &braga),
            (4, 41.1, -8.6, &porto),
            (5, 59.9, 10.7, &oslo),
        ];
        let places = tree(&points);
        assert_eq!(places.iter().map(|node| (node.name.as_str(), node.count)).collect::<Vec<_>>(), [("Portugal", 4), ("Norway", 1)]);
        let portugal = &places[0];

        assert_eq!(portugal.kids.iter().map(|node| (node.name.as_str(), node.count)).collect::<Vec<_>>(), [("Norte", 3), ("Lisbon", 1)]);
        assert!(portugal.kids[1].level == Level::Town && portugal.kids[1].key == "Portugal/Lisbon/Lisbon");
        assert_eq!(portugal.ids, [1, 2, 3], "the first three");
        assert_eq!(portugal.area, [-9.1, 38.7, -8.4, 41.5]);

        assert_eq!((portugal.latitude, portugal.longitude), (41.1, -8.6));

        assert_eq!(shown_at(&places, 1.0).iter().map(|stack| stack.len()).collect::<Vec<_>>(), [1, 1]);
        let near = shown_at(&places, 9.0);
        assert!(near.iter().flatten().any(|node| node.name == "Braga") && near.iter().flatten().any(|node| node.name == "Lisbon"));
    }
}
