use super::*;

#[derive(Clone)]
enum Target {
    Page(&'static str),
    Place(Place),
    NewAlbum,
    Import,
}

const FOLDS: &str = "side-open";

const PEEK_AFTER: std::time::Duration = std::time::Duration::from_millis(350);
const PEEK_TILE: f32 = 100.0;

thread_local! {

    static PEEKING: RefCell<Option<glib::SourceId>> = const { RefCell::new(None) };
}

enum Look {

    Tint(usize),

    House,

    Face,

    Dot,
}

#[derive(Clone)]
pub(super) struct State {
    pub(super) split: adw::OverlaySplitView,

    pub(super) toggle: gtk::ToggleButton,
    list: gtk::ListBox,
    rows: Rc<RefCell<Vec<(gtk::ListBoxRow, Target)>>>,
    pending: Rc<Cell<bool>>,

    peek: gtk::Popover,
}

impl State {
    pub(super) fn new() -> Self {
        let list = gtk::ListBox::new();
        list.add_css_class("navigation-sidebar");
        let toggle = gtk::ToggleButton::new();
        toggle.set_icon_name("sidebar-show-symbolic");
        toggle.set_tooltip_text(Some("Places"));
        toggle.set_visible(false);
        let peek = gtk::Popover::new();
        peek.set_autohide(false);
        peek.set_can_target(false);
        peek.set_position(gtk::PositionType::Right);
        peek.add_css_class("side-peek");
        Self { split: adw::OverlaySplitView::new(), toggle, list, rows: Rc::default(), pending: Rc::default(), peek }
    }
}

pub(super) fn build(state: &App, window: &adw::ApplicationWindow, content: &impl IsA<gtk::Widget>) -> adw::OverlaySplitView {
    let side = &state.libraries.side;
    let scroller = gtk::ScrolledWindow::new();
    scroller.set_hscrollbar_policy(gtk::PolicyType::Never);
    scroller.set_vexpand(true);
    scroller.set_child(Some(&side.list));

    side.peek.set_parent(&scroller);
    let bar = adw::HeaderBar::new();
    bar.pack_start(&mark_button(state));
    bar.set_title_widget(Some(&gtk::Box::new(gtk::Orientation::Horizontal, 0)));
    let view = adw::ToolbarView::new();
    view.add_top_bar(&bar);
    view.set_content(Some(&scroller));
    side.split.set_sidebar(Some(&view));
    side.split.set_content(Some(content));
    side.split.set_max_sidebar_width(260.0);
    side.list.connect_row_activated(glib::clone!(
        #[strong] state,
        #[weak] window,
        move |_, row| go_row(&state, &window, row)
    ));

    let breakpoint = adw::Breakpoint::new(narrower_than(0));
    breakpoint.add_setter(&side.split, "collapsed", Some(&true.to_value()));
    breakpoint.add_setter(&side.split, "show-sidebar", Some(&false.to_value()));
    window.add_breakpoint(breakpoint.clone());
    let fit = Rc::new(glib::clone!(
        #[weak] window,
        #[weak(rename_to = split)] side.split,
        #[strong(rename_to = content)] content.clone().upcast::<gtk::Widget>(),
        move || fit(&window, &split, &content, &breakpoint)
    ));
    glib::idle_add_local_once(glib::clone!(
        #[strong] fit,
        move || fit()
    ));
    state.stack.connect_visible_child_name_notify(move |_| {
        let fit = fit.clone();
        glib::idle_add_local_once(move || fit());
    });
    side.split.bind_property("show-sidebar", &side.toggle, "active").bidirectional().sync_create().build();
    side.split.connect_collapsed_notify(glib::clone!(
        #[strong] state,
        move |_| follow(&state)
    ));
    state.stack.connect_visible_child_name_notify(glib::clone!(
        #[strong] state,
        move |_| {
            follow(&state);
            mark(&state);
        }
    ));
    state.loupe.reveal.connect_reveal_child_notify(glib::clone!(
        #[strong] state,
        move |_| follow(&state)
    ));
    side.split.clone()
}

fn fit(window: &adw::ApplicationWindow, split: &adw::OverlaySplitView, content: &gtk::Widget, breakpoint: &adw::Breakpoint) {
    let (least, ..) = content.measure(gtk::Orientation::Horizontal, -1);
    let (wide, high) = window.size_request();
    if least <= wide {
        return;
    }
    let (tall, ..) = content.measure(gtk::Orientation::Vertical, -1);
    window.set_size_request(least, tall.max(high));
    breakpoint.set_condition(Some(&narrower_than(least + split.min_sidebar_width().ceil() as i32)));
}

fn narrower_than(width: i32) -> adw::BreakpointCondition {
    adw::BreakpointCondition::new_length(adw::BreakpointConditionLengthType::MaxWidth, width as f64, adw::LengthUnit::Px)
}

fn follow(state: &App) {
    let side = &state.libraries.side;
    let away = state.stack.visible_child_name().as_deref() == Some("editor") || state.loupe.reveal.reveals_child();
    side.toggle.set_visible(side.split.is_collapsed() && !away);
    if away {
        side.split.set_show_sidebar(false);
    } else if !side.split.is_collapsed() {
        side.split.set_show_sidebar(true);
    }
}

fn mark_button(state: &App) -> gtk::Button {
    let mark = gtk::Image::from_paintable(mark_texture().as_ref());
    mark.set_pixel_size(50);
    adw::StyleManager::default().connect_dark_notify(glib::clone!(
        #[weak] mark,
        move |_| mark.set_paintable(mark_texture().as_ref())
    ));
    let button = gtk::Button::new();
    button.set_child(Some(&mark));
    button.add_css_class("flat");
    button.set_tooltip_text(Some(if starts_on_photos(state) { "Photos" } else { "Today" }));
    button.connect_clicked(glib::clone!(
        #[strong] state,
        move |button| {
            button.set_tooltip_text(Some(if starts_on_photos(&state) { "Photos" } else { "Today" }));
            start_page(&state);
        }
    ));
    button
}

pub(super) const START_ON_PHOTOS: &str = "start-on-photos";

pub(super) fn starts_on_photos(state: &App) -> bool {
    state.catalog.recall::<bool>(START_ON_PHOTOS).unwrap_or(false)
}

pub(super) fn start_page(state: &App) {
    match starts_on_photos(state) {
        true => show_place(state, Place::Everywhere),
        false => today::show(state),
    }
}

pub(super) fn fill(state: &App) {
    if state.libraries.side.pending.replace(true) {
        return;
    }
    let state = state.clone();
    glib::idle_add_local_once(move || {
        state.libraries.side.pending.set(false);
        fill_now(&state);
        library_page::show_head(&state);
    });
}

fn fill_now(state: &App) {
    let side = &state.libraries.side;
    unpeek(state);
    side.list.remove_all();
    let open = folds(state);
    let places = state.libraries.places.borrow().clone();
    let mut rows = Vec::new();
    let mut add = |row: gtk::ListBoxRow, target: Target| {
        side.list.append(&row);
        rows.push((row, target));
    };
    add(place_row("Today"), Target::Page("today"));
    add(place_row("Photos"), Target::Place(Place::Everywhere));
    for (label, page, key) in [("Libraries", "libraries", "library"), ("People", "people", "person"), ("Albums", "albums", "album")] {
        let is_open = open.iter().any(|one| one == key);
        add(group_row(state, label, key, is_open), Target::Page(page));
        if !is_open {
            continue;
        }
        let mut tint = 0;
        for place in &places {
            let look = match (place, key) {
                (Place::Library(library), "library") if home_sync::is_home(state, library.id) => Look::House,
                (Place::Library(_), "library") => {
                    tint += 1;
                    Look::Tint((tint - 1) % TINTS)
                }
                (Place::Person(_), "person") => Look::Face,
                (Place::Album(_), "album") => Look::Dot,
                _ => continue,
            };
            let row = entry_row(state, place, look);
            peek_on(state, &row, place);
            add(row, Target::Place(place.clone()));
        }
        if key == "album" {
            add(new_album_row(), Target::NewAlbum);
        }
    }

    add(counted_row("Places", places_page::count()), Target::Page("places"));
    let archive = gtk::ListBoxRow::new();
    archive.set_selectable(false);
    archive.set_activatable(false);
    archive.set_child(Some(&section_header("Archive")));
    side.list.append(&archive);
    add(place_row("Import…"), Target::Import);

    add(place_row("Tidy Up"), Target::Page("tidy"));
    *side.rows.borrow_mut() = rows;
    mark(state);
}

pub(super) const TINTS: usize = 8;

pub(super) fn tints(state: &App) -> HashMap<i64, usize> {
    state
        .libraries
        .places
        .borrow()
        .iter()
        .filter_map(|place| match place {
            Place::Library(library) if !home_sync::is_home(state, library.id) => Some(library.id),
            _ => None,
        })
        .enumerate()
        .map(|(at, id)| (id, at % TINTS))
        .collect()
}

fn folds(state: &App) -> Vec<String> {
    state.catalog.recall::<Vec<String>>(FOLDS).unwrap_or_default()
}

fn place_row(label: &str) -> gtk::ListBoxRow {
    let name = gtk::Label::new(Some(label));
    name.set_xalign(0.0);
    let row = gtk::ListBoxRow::new();
    row.set_child(Some(&name));
    row
}

fn counted_row(label: &str, count: Option<usize>) -> gtk::ListBoxRow {
    let name = gtk::Label::new(Some(label));
    name.set_xalign(0.0);
    name.set_hexpand(true);
    let line = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    line.append(&name);
    if let Some(count) = count {
        let count = gtk::Label::new(Some(&places::grouped(count as i64)));
        count.add_css_class("caption");
        count.add_css_class("dim-label");
        count.add_css_class("numeric");
        line.append(&count);
    }
    let row = gtk::ListBoxRow::new();
    row.set_child(Some(&line));
    row
}

fn group_row(state: &App, label: &str, key: &'static str, open: bool) -> gtk::ListBoxRow {
    let name = gtk::Label::new(Some(label));
    name.set_xalign(0.0);
    name.set_hexpand(true);
    let fold = gtk::Button::from_icon_name(if open { "pan-down-symbolic" } else { "pan-end-symbolic" });
    fold.add_css_class("flat");
    fold.add_css_class("circular");
    fold.add_css_class("side-fold");
    fold.set_valign(gtk::Align::Center);
    fold.set_tooltip_text(Some(&format!("Every one of {}", label.to_lowercase())));
    fold.connect_clicked(glib::clone!(
        #[strong] state,
        move |_| {
            let mut open = folds(&state);
            match open.iter().position(|one| one == key) {
                Some(at) => {
                    open.remove(at);
                }
                None => open.push(key.to_string()),
            }
            state.catalog.remember(FOLDS, &open);
            fill_now(&state);
        }
    ));
    let line = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    line.append(&name);
    line.append(&fold);
    let row = gtk::ListBoxRow::new();
    row.set_child(Some(&line));
    row
}

fn entry_row(state: &App, place: &Place, look: Look) -> gtk::ListBoxRow {
    let read = places::row(state, place);
    let line = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    line.set_margin_start(12);
    match look {
        Look::Tint(_) | Look::Dot => {
            let dot = gtk::Box::new(gtk::Orientation::Horizontal, 0);
            dot.add_css_class("library-tint");
            if let Look::Tint(tint) = look {
                dot.add_css_class(&format!("tint-{tint}"));
            }
            dot.set_valign(gtk::Align::Center);
            line.append(&dot);
        }
        Look::House => {
            let house = gtk::Image::from_icon_name("go-home-symbolic");
            house.set_pixel_size(12);
            house.add_css_class("dim-label");
            house.set_tooltip_text(Some(home::NUMA_HOME));
            line.append(&house);
        }
        Look::Face => {
            let face = gtk::Image::new();
            face.set_pixel_size(16);
            face.add_css_class("side-face");
            face.set_overflow(gtk::Overflow::Hidden);
            face.set_paintable(read.covers.first().and_then(|cover| places::texture(cover, 32)).as_ref());
            line.append(&face);
        }
    }
    let name = gtk::Label::new(Some(&places::name_of(state, place)));
    name.set_xalign(0.0);
    name.set_hexpand(true);
    name.set_ellipsize(gtk::pango::EllipsizeMode::End);
    whole_on_hover(&name);
    let count = gtk::Label::new(Some(&places::grouped(read.count)));
    count.add_css_class("caption");
    count.add_css_class("dim-label");
    count.add_css_class("numeric");
    line.append(&name);
    line.append(&count);

    let row = gtk::ListBoxRow::new();
    row.set_child(Some(&line));
    row
}

fn new_album_row() -> gtk::ListBoxRow {
    let plus = gtk::Image::from_icon_name("list-add-symbolic");
    plus.set_pixel_size(12);
    let name = gtk::Label::new(Some("New Album…"));
    name.set_xalign(0.0);
    let line = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    line.set_margin_start(12);
    line.append(&plus);
    line.append(&name);
    let row = gtk::ListBoxRow::new();
    row.set_child(Some(&line));
    row
}

fn key_of(target: &Target) -> String {
    match target {
        Target::Page(page) => format!("page:{page}"),
        Target::Place(place) => places::key(place),
        Target::NewAlbum | Target::Import => String::new(),
    }
}

pub(super) fn mark(state: &App) {
    let side = &state.libraries.side;
    let (wanted, owner) = match state.stack.visible_child_name().as_deref() {
        Some("library") => {
            let place = current_place(state);
            let owner = match place {
                Place::Everywhere => "page:photos",
                Place::Library(_) => "page:libraries",
                Place::Album(_) => "page:albums",
                Place::Person(_) => "page:people",
            };
            (places::key(&place), owner.to_string())
        }
        Some("editor") | None => return,
        Some(page) => (format!("page:{page}"), String::new()),
    };
    let rows = side.rows.borrow();
    let row = rows
        .iter()
        .find(|(_, target)| key_of(target) == wanted)
        .or_else(|| rows.iter().find(|(_, target)| key_of(target) == owner))
        .map(|(row, _)| row);
    side.list.select_row(row);
}

fn peek_on(state: &App, row: &gtk::ListBoxRow, place: &Place) {
    let motion = gtk::EventControllerMotion::new();
    motion.connect_enter(glib::clone!(
        #[strong] state,
        #[weak] row,
        #[strong] place,
        move |_, _, _| {
            unpeek(&state);
            let later = glib::timeout_add_local_once(PEEK_AFTER, glib::clone!(
                #[strong] state,
                #[weak] row,
                #[strong] place,
                move || {
                    PEEKING.take();
                    peek(&state, &row, &place);
                }
            ));
            PEEKING.replace(Some(later));
        }
    ));
    motion.connect_leave(glib::clone!(
        #[strong] state,
        move |_| unpeek(&state)
    ));
    row.add_controller(motion);
}

fn unpeek(state: &App) {
    if let Some(later) = PEEKING.take() {
        later.remove();
    }
    state.libraries.side.peek.popdown();
}

fn peek(state: &App, row: &gtk::ListBoxRow, place: &Place) {
    let side = &state.libraries.side;
    let Some(bounds) = side.peek.parent().and_then(|parent| row.compute_bounds(&parent)) else { return };
    let read = places::row(state, place);
    let fan = places::fan(PEEK_TILE);
    places::show_covers(&fan, &read.covers, PEEK_TILE);
    fan.set_halign(gtk::Align::Center);
    let name = gtk::Label::new(Some(&places::name_of(state, place)));
    name.add_css_class("heading");
    name.set_ellipsize(gtk::pango::EllipsizeMode::End);
    name.set_max_width_chars(22);
    let caption = gtk::Label::new(Some(&read.subtitle));
    caption.add_css_class("caption");
    caption.add_css_class("dim-label");
    let card = gtk::Box::new(gtk::Orientation::Vertical, 4);
    card.append(&fan);
    card.append(&name);
    card.append(&caption);
    side.peek.set_child(Some(&card));
    let at = gtk::gdk::Rectangle::new(bounds.x() as i32, bounds.y() as i32, bounds.width() as i32, bounds.height() as i32);
    side.peek.set_pointing_to(Some(&at));
    side.peek.popup();
}

fn go_row(state: &App, window: &adw::ApplicationWindow, row: &gtk::ListBoxRow) {
    unpeek(state);
    let Some(target) = state.libraries.side.rows.borrow().iter().find(|(one, _)| one == row).map(|(_, target)| target.clone()) else {
        return;
    };
    match target {
        Target::Page("today") => today::show(state),
        Target::Page("libraries") => show_folders(state),
        Target::Page("albums") => show_albums(state),
        Target::Page("people") => show_people(state),
        Target::Page("tidy") => tidy::show(state),
        Target::Page("places") => places_page::show(state),
        Target::Page(_) => {}
        Target::Place(place) => show_place(state, place),
        Target::NewAlbum => {
            mark(state);
            new_album(state, window);
        }
        Target::Import => {
            mark(state);
            import_dialog(state, window, None);
        }
    }
    let side = &state.libraries.side;
    if side.split.is_collapsed() {
        side.split.set_show_sidebar(false);
    }
}
