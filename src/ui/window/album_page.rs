use super::*;
use numa::io::catalog::{AlbumKind, AlbumPage, PageAsked, Photo, NO_CHAPTER};

const TILE: i32 = 148;

const WIDE: i32 = 320;

const COLUMN: i32 = 880;
const SIDE: i32 = 320;
const GAP: i32 = 32;

const COVER: i32 = 280;
const COVER_EDGE: u32 = 2048;

const RAIL: i32 = 40;

#[derive(Clone)]
pub(super) struct State {
    pub(super) root: gtk::ScrolledWindow,

    head: gtk::Box,

    pub(super) name: Rc<RefCell<String>>,
    pub(super) column: gtk::Box,
    pub(super) side: gtk::Box,
    pub(super) key: Rc<RefCell<Option<String>>>,

    turn: Rc<Cell<u32>>,

    looking: Rc<Cell<Option<f64>>>,

    title: Rc<RefCell<glib::WeakRef<gtk::Label>>>,
}

impl State {
    pub(super) fn new() -> Self {
        State {
            root: gtk::ScrolledWindow::new(),
            head: gtk::Box::new(gtk::Orientation::Vertical, 0),
            name: Rc::default(),
            column: gtk::Box::new(gtk::Orientation::Vertical, 24),
            side: gtk::Box::new(gtk::Orientation::Vertical, 16),
            key: Rc::default(),
            turn: Rc::default(),
            looking: Rc::default(),
            title: Rc::default(),
        }
    }
}

pub(super) fn build(state: &App) -> gtk::ScrolledWindow {
    let page = &state.libraries.album;
    let body = gtk::Box::new(gtk::Orientation::Horizontal, GAP);
    page.column.set_hexpand(true);
    page.column.set_size_request(SIDE, -1);
    page.side.set_size_request(SIDE, -1);
    page.side.set_valign(gtk::Align::Start);
    body.append(&page.column);
    body.append(&page.side);
    let all = gtk::Box::new(gtk::Orientation::Vertical, 24);
    all.append(&page.head);
    all.append(&body);
    all.set_margin_top(24);
    all.set_margin_bottom(32);
    all.set_margin_start(24);
    all.set_margin_end(24);
    let clamp = adw::Clamp::new();
    clamp.set_maximum_size(COLUMN + SIDE + GAP);
    clamp.set_child(Some(&all));
    page.root.set_hscrollbar_policy(gtk::PolicyType::Never);
    page.root.set_vexpand(true);
    page.root.set_child(Some(&clamp));
    page.root.clone()
}

fn today() -> i64 {
    glib::DateTime::now_local().map_or(0, |now| (now.to_unix() + now.utc_offset().as_seconds()).div_euclid(86_400))
}

fn fill_head(state: &App, page: &AlbumPage, today: i64) {
    let head = &state.libraries.album.head;
    folders::clear(head);
    state.libraries.album.name.replace(page.name.clone());
    let words = gtk::Box::new(gtk::Orientation::Vertical, 4);
    words.set_hexpand(true);
    words.append(&kit::section_header(&album_words::over_name(page)));
    let title = gtk::Label::new(Some(&page.name));
    title.add_css_class("title-1");
    title.set_xalign(0.0);
    title.set_ellipsize(gtk::pango::EllipsizeMode::End);
    state.libraries.album.title.replace(title.downgrade());
    words.append(&title);
    let figures = gtk::Box::new(gtk::Orientation::Horizontal, 32);
    figures.set_margin_top(12);
    for figure in album_words::figures(page, today) {
        let one = gtk::Box::new(gtk::Orientation::Vertical, 0);
        one.append(&kit::section_header(figure.label));
        let line = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        let value = gtk::Label::new(Some(&figure.value));
        value.add_css_class("title-2");
        value.add_css_class("numeric");
        line.append(&value);
        if !figure.of.is_empty() {
            let of = gtk::Label::new(Some(&figure.of));
            of.add_css_class("dim-label");
            of.add_css_class("numeric");
            of.set_valign(gtk::Align::Baseline);
            line.append(&of);
        }
        one.append(&line);
        figures.append(&one);
    }
    words.append(&figures);

    if page.kind == AlbumKind::Trip && page.chapters.len() > 1 {
        let bar = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        bar.set_homogeneous(true);
        bar.set_margin_top(12);
        bar.set_halign(gtk::Align::Start);
        bar.set_size_request((page.chapters.len() as i32 * 88).min(520), -1);
        for laid in &page.layout.chapters {
            let part = gtk::Box::new(gtk::Orientation::Horizontal, 0);
            part.add_css_class("album-part");
            if laid.count > 0 {
                part.add_css_class("been");
            }
            bar.append(&part);
        }
        let filled = page.layout.chapters.iter().filter(|laid| laid.count > 0).count();
        bar.update_property(&[gtk::accessible::Property::Label(&format!("{filled} of {} places", page.chapters.len()))]);
        words.append(&bar);
    }
    let actions = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    actions.set_valign(gtk::Align::Start);
    if page.kind == AlbumKind::Trip {

        let share = gtk::Button::with_label("Share…");
        share.add_css_class("numa-primary");
        share.set_action_name(Some("album.share"));
        actions.append(&share);
    }
    actions.append(&album_chapter::album_menu(state));

    let by_id: HashMap<i64, &Photo> = page.photos.iter().map(|photo| (photo.id, photo)).collect();
    let shown: Vec<&Photo> = page.layout.chapters.iter().flat_map(|laid| &laid.shown).chain(page.layout.days.iter().flat_map(|day| &day.shown)).filter_map(|id| by_id.get(id).copied()).collect();
    let cover = shown.iter().find(|photo| photo.flag == numa::io::catalog::Flag::Picked).or_else(|| shown.first()).copied();
    let Some(cover) = cover else {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        row.append(&words);
        row.append(&actions);
        head.append(&row);
        return;
    };
    let picture = gtk::Picture::new();
    picture.set_content_fit(gtk::ContentFit::Cover);
    picture.set_can_shrink(true);
    picture.set_size_request(-1, COVER);
    picture.update_property(&[gtk::accessible::Property::Label(&format!("Cover of {}", page.name))]);
    let (wanted, shown) = (picture.downgrade(), picture.clone());
    thumbnail::load_thumbnail_while(
        &cover.path,
        cover.mtime,
        COVER_EDGE,
        state.catalog.edits_json(cover.id).ok().flatten(),
        None,
        move || wanted.upgrade().is_some(),
        move |texture| shown.set_paintable(Some(&texture)),
    );
    let over = gtk::Overlay::new();
    over.add_css_class("album-cover");
    over.set_overflow(gtk::Overflow::Hidden);
    over.set_child(Some(&picture));
    let scrim = gtk::Box::new(gtk::Orientation::Vertical, 0);
    scrim.add_css_class("album-scrim");
    scrim.set_can_target(false);
    over.add_overlay(&scrim);
    words.add_css_class("hero-words");

    let say = gtk::Box::new(gtk::Orientation::Vertical, 0);
    say.add_css_class("album-say");
    say.set_valign(gtk::Align::End);
    say.append(&words);
    over.add_overlay(&say);
    actions.set_halign(gtk::Align::End);
    actions.set_margin_top(12);
    actions.set_margin_end(12);
    over.add_overlay(&actions);
    head.append(&over);
}

pub(super) fn follow_name(state: &App, bar: &adw::WindowTitle) {
    let say = {
        let (page, stack, bar) = (state.libraries.album.clone(), state.stack.clone(), bar.downgrade());
        move || {
            let Some(bar) = bar.upgrade() else { return };
            if stack.visible_child_name().as_deref() != Some("album") {
                return;
            }

            let gone = page.title.borrow().upgrade().zip(page.root.child()).and_then(|(title, viewport)| title.compute_bounds(&viewport)).is_some_and(|bounds| bounds.height() > 0.0 && bounds.y() + bounds.height() <= 0.0);
            let name = if gone { page.name.borrow().clone() } else { String::new() };
            bar.set_title(&name);
        }
    };

    let later = move || glib::idle_add_local_once(say.clone());
    let again = later.clone();
    state.libraries.album.root.vadjustment().connect_value_changed(move |_| drop(again()));
    state.stack.connect_visible_child_name_notify(move |_| drop(later()));
}

pub(super) fn has_page(state: &App, key: &str) -> bool {
    state.catalog.album_page(key, &PageAsked::default()).is_ok_and(|page| !page.grid)
}

pub(super) fn show(state: &App, key: &str) {
    let page = &state.libraries.album;
    if page.key.replace(Some(key.to_string())).as_deref() != Some(key) {
        page.root.vadjustment().set_value(0.0);
    }
    refill(state);
    state.stack.set_visible_child_name("album");
}

pub(super) fn refill(state: &App) {
    let Some(key) = state.libraries.album.key.borrow().clone() else { return };
    let turn = state.libraries.album.turn.get().wrapping_add(1);
    state.libraries.album.turn.set(turn);
    let read = |outside: bool| state.catalog.album_page(&key, &PageAsked { outside, ..PageAsked::default() });
    match read(false) {
        Ok(page) => {
            let places = page.chapters.iter().any(|chapter| chapter.place.is_some());
            paint(state, &page);
            if places {
                let state = state.clone();
                glib::idle_add_local_once(move || {
                    if state.libraries.album.turn.get() == turn {
                        if let Ok(page) = read_outside(&state) {
                            paint(&state, &page);
                        }
                    }
                });
            }
            read_positions(state, &page);
        }
        Err(err) => state.toast(&err),
    }
}

fn read_outside(state: &App) -> Result<AlbumPage, String> {
    let key = state.libraries.album.key.borrow().clone().ok_or("no album")?;
    state.catalog.album_page(&key, &PageAsked { outside: true, ..PageAsked::default() })
}

fn read_positions(state: &App, page: &AlbumPage) {
    if page.unread.is_empty() {
        return;
    }
    let unread: std::collections::HashSet<i64> = page.unread.iter().copied().collect();
    let batch: Vec<Photo> = page.photos.iter().filter(|photo| unread.contains(&photo.id)).cloned().collect();
    if batch.is_empty() {
        return;
    }
    let (state, turn) = (state.clone(), state.libraries.album.turn.get());
    glib::spawn_future_local(async move {
        let read = gio::spawn_blocking(move || numa::io::catalog::read_where(&batch)).await.unwrap_or_default();
        if state.catalog.keep_where(&read).is_ok() && state.libraries.album.turn.get() == turn {
            refill(&state);
        }
    });
}

fn paint(state: &App, page: &AlbumPage) {
    let view = &state.libraries.album;
    let today = today();
    fill_head(state, page, today);
    folders::clear(&view.column);
    folders::clear(&view.side);
    let photos: HashMap<i64, &Photo> = page.photos.iter().map(|photo| (photo.id, photo)).collect();
    let trip = page.kind == AlbumKind::Trip;
    if trip && !page.layout.places.is_empty() {
        view.column.append(&album_trip::places_offered(state, page));
    }

    if !page.chapters.is_empty() {
        let here = if trip { album_words::here_now(page, today) } else { None };
        let line = gtk::Box::new(gtk::Orientation::Vertical, 0);
        let last = page.chapters.len() - 1;
        for (at, (chapter, laid)) in page.chapters.iter().zip(&page.layout.chapters).enumerate() {
            let ahead = trip && laid.count == 0 && chapter.place.is_some();
            let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
            row.set_widget_name(&chapter.key);
            row.append(&rail(Some(at + 1), ahead, here == Some(at), at == 0));
            let block = album_chapter::block(state, page, chapter, laid, &photos, ahead, here == Some(at));
            block.set_hexpand(true);
            block.set_margin_bottom(if at == last { 0 } else { 16 });
            row.append(&block);
            line.append(&row);
            if at < last {
                let next_ahead = trip && page.layout.chapters[at + 1].count == 0 && page.chapters[at + 1].place.is_some();
                let between = gtk::Box::new(gtk::Orientation::Horizontal, 8);
                between.append(&rail(None, next_ahead, false, false));
                match album_words::leg(chapter, &page.chapters[at + 1]).filter(|_| trip) {
                    Some(far) => {
                        let said = gtk::Label::new(Some(&far));
                        said.add_css_class("caption");
                        said.add_css_class("dim-label");
                        said.add_css_class("numeric");
                        said.set_margin_top(8);
                        said.set_margin_bottom(8);
                        between.append(&said);
                    }
                    None => between.set_size_request(-1, 8),
                }
                line.append(&between);
            }
        }
        view.column.append(&line);
    }
    for day in &page.layout.days {
        view.column.append(&day_block(state, page, day, &photos));
    }
    let starting = page.layout.count == 0 && page.chapters.is_empty();
    if starting {
        view.column.append(&album_chapter::start(page));
    }
    if trip {
        album_trip::fill_side(state, page, today);
    } else {

        if !starting {
            view.column.append(&album_chapter::add_field(state, page));
        }
        album_chapter::fill_side(state, page);
    }
    view.side.set_visible(view.side.first_child().is_some());
}

fn rail(number: Option<usize>, ahead: bool, here: bool, first: bool) -> gtk::Overlay {
    let over = gtk::Overlay::new();
    over.set_size_request(RAIL, -1);
    let line = gtk::Box::new(gtk::Orientation::Vertical, 0);
    line.add_css_class("album-rail");
    if ahead {
        line.add_css_class("ahead");
    }
    line.set_halign(gtk::Align::Center);
    line.set_vexpand(true);

    line.set_margin_top(if first { 12 } else { 0 });
    over.set_child(Some(&line));
    if let Some(number) = number {
        over.add_overlay(&mark(number, ahead, here));
    }
    over
}

pub(super) fn mark(number: usize, ahead: bool, here: bool) -> gtk::Label {
    let mark = gtk::Label::new(Some(&number.to_string()));
    mark.add_css_class("album-mark");
    mark.add_css_class("numeric");
    if ahead {
        mark.add_css_class("ahead");
    }
    if here {
        mark.add_css_class("here");
    }
    mark.set_halign(gtk::Align::Center);
    mark.set_valign(gtk::Align::Start);
    mark
}

fn day_block(state: &App, page: &AlbumPage, day: &numa::io::album_page::DayLayout, photos: &HashMap<i64, &Photo>) -> gtk::Box {
    let block = gtk::Box::new(gtk::Orientation::Vertical, 8);
    let mut head = album_words::day_name(day.day);
    if !day.towns.is_empty() {
        head = format!("{head} · {}", day.towns.join(", "));
    }
    block.append(&kit::section_header(&head));
    block.append(&tiles(state, page, &day.shown, photos, NO_CHAPTER, day.count));
    block
}

fn more_tile(state: &App, album: &str, chapter: &str, rest: usize) -> gtk::Button {
    let count = gtk::Label::new(Some(&format!("+{}", places::grouped(rest as i64))));
    count.add_css_class("heading");
    count.add_css_class("numeric");
    let said = gtk::Label::new(Some("Show All"));
    said.add_css_class("caption");
    said.add_css_class("dim-label");
    let words = gtk::Box::new(gtk::Orientation::Vertical, 4);
    words.set_valign(gtk::Align::Center);
    words.append(&count);
    words.append(&said);
    let button = gtk::Button::new();
    button.add_css_class("album-more");
    button.set_size_request(-1, TILE);
    button.set_child(Some(&words));
    let (state, album, chapter) = (state.clone(), album.to_string(), chapter.to_string());
    button.connect_clicked(move |_| open_grid(&state, &album, &chapter, None));
    button
}

pub(super) fn open_grid(state: &App, album: &str, chapter: &str, at: Option<i64>) {
    let scrolled = Some(state.libraries.album.root.vadjustment().value());
    {
        let mut filter = state.libraries.filter.borrow_mut();
        filter.album = Some(album.to_string());
        filter.chapter = Some(chapter.to_string());
        filter.all_libraries = false;
        filter.person = None;
    }
    reload_grid(state);
    library_page::show_head(state);
    state.stack.set_visible_child_name("library");
    sidebar::mark(state);
    if let Some(id) = at {
        let state = state.clone();
        glib::idle_add_local_once(move || {

            state.loupe.group.plain.set(true);
            let shown = loupe::show_in_loupe(&state, id);
            state.loupe.group.plain.set(shown);
            state.libraries.album.looking.set(scrolled.filter(|_| shown));
        });
    }
}

pub(super) fn back_from_loupe(state: &App, to_grid: bool) {
    let Some(scrolled) = state.libraries.album.looking.take() else { return };
    let ours = {
        let filter = state.libraries.filter.borrow();
        filter.chapter.is_some() && filter.album == *state.libraries.album.key.borrow()
    };
    if !to_grid || !ours {
        return;
    }
    state.stack.set_visible_child_name("album");
    state.libraries.album.root.vadjustment().set_value(scrolled);
}

pub(super) fn is_looking(state: &App) -> bool {
    state.libraries.album.looking.get().is_some()
}

pub(super) fn tiles(state: &App, page: &AlbumPage, shown: &[i64], photos: &HashMap<i64, &Photo>, chapter: &str, count: usize) -> gtk::Box {
    let all = gtk::Box::new(gtk::Orientation::Vertical, 8);
    let mut row: Option<gtk::FlowBox> = None;
    for photo in shown.iter().filter_map(|id| photos.get(id).copied()) {
        match page.captions.get(&photo.id) {
            Some(caption) => {
                row = None;
                all.append(&tile(state, &page.key, chapter, photo, None));
                let words = gtk::Label::new(Some(caption));
                words.set_wrap(true);
                words.set_xalign(0.0);
                words.set_selectable(true);
                all.append(&words);
            }
            None => {
                let flow = row.get_or_insert_with(|| {
                    let flow = gtk::FlowBox::new();
                    flow.set_selection_mode(gtk::SelectionMode::None);

                    flow.set_homogeneous(true);
                    flow.set_min_children_per_line(3);
                    flow.set_max_children_per_line(4);
                    flow.set_row_spacing(4);
                    flow.set_column_spacing(4);
                    all.append(&flow);
                    flow
                });
                flow.insert(&tile(state, &page.key, chapter, photo, Some(TILE)), -1);
            }
        }
    }

    if count > shown.len() {
        let more = more_tile(state, &page.key, chapter, count - shown.len());
        match row {
            Some(flow) => flow.insert(&more, -1),
            None => {
                more.set_halign(gtk::Align::Start);
                more.set_size_request(TILE * 3 / 2, TILE);
                all.append(&more);
            }
        }
    }
    all
}

fn tile(state: &App, album: &str, chapter: &str, photo: &Photo, width: Option<i32>) -> gtk::Button {
    let picture = gtk::Picture::new();
    picture.set_content_fit(gtk::ContentFit::Cover);
    picture.set_can_shrink(true);
    picture.set_size_request(-1, width.unwrap_or(WIDE));
    let over = gtk::Overlay::new();
    over.set_child(Some(&picture));
    over.set_overflow(gtk::Overflow::Hidden);
    over.add_css_class("album-tile");
    let (badge, _) = badges::card_badge(photo.rating, photo.flag, None, false);
    if !badge.is_empty() {
        let mark = gtk::Label::new(Some(&badge));
        mark.add_css_class("card-badge");
        mark.set_halign(gtk::Align::Start);
        mark.set_valign(gtk::Align::End);
        mark.set_can_target(false);
        over.add_overlay(&mark);
    }
    let wanted = picture.downgrade();
    let shown = picture.clone();
    thumbnail::load_thumbnail_while(
        &photo.path,
        photo.mtime,
        GRID_THUMB_EDGE,
        state.catalog.edits_json(photo.id).ok().flatten(),
        None,
        move || wanted.upgrade().is_some(),
        move |texture| shown.set_paintable(Some(&texture)),
    );
    let button = gtk::Button::new();
    button.add_css_class("flat");
    button.add_css_class("album-photo");
    button.set_child(Some(&over));
    button.set_tooltip_text(photo.path.file_name().and_then(|name| name.to_str()));

    let keys = gtk::EventControllerKey::new();
    let (copying, id_of_tile, path) = (state.clone(), photo.id, photo.path.clone());
    keys.connect_key_pressed(move |_, key, _, modifiers| {
        let both = gtk::gdk::ModifierType::CONTROL_MASK | gtk::gdk::ModifierType::SHIFT_MASK;
        if !modifiers.contains(both) || !matches!(key.to_unicode(), Some('c' | 'C')) {
            return glib::Propagation::Proceed;
        }
        exporting::copy_photo(&copying, photo_jobs(&copying, [(id_of_tile, path.clone())]).pop());
        glib::Propagation::Stop
    });
    button.add_controller(keys);
    let (state, album, chapter, id) = (state.clone(), album.to_string(), chapter.to_string(), photo.id);
    button.connect_clicked(move |_| open_grid(&state, &album, &chapter, Some(id)));
    button
}
