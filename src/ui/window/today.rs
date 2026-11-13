use super::*;

const NEWS: i64 = 14 * 86_400;

const YEARS: usize = 6;

const ARRIVALS: usize = 3;
const STRIP: usize = 6;

const STRIP_HEIGHT: i32 = 80;

const TILE: f32 = 132.0;

const FRESH: std::time::Duration = std::time::Duration::from_secs(10 * 60);

thread_local! {

    static FILLED: Cell<u64> = const { Cell::new(0) };

    static FACES: Cell<Option<(std::time::Instant, usize)>> = const { Cell::new(None) };

    static OPENED_HERE: Cell<bool> = const { Cell::new(false) };

    static HERO_TURN: Cell<(u64, u32)> = const { Cell::new((0, 0)) };
}

const BEST: usize = 12;

const FAN_TILE: f32 = 56.0;

const HERO_HEIGHT: i32 = 600;

const SIDE_WIDTH: i32 = 260;

const HERO_EDGE: u32 = 3072;

const CLEAR_OF_EDGE: i32 = 1;

const FANS: usize = 6;

pub(super) fn page(state: &App) -> gtk::Overlay {
    let column = &state.libraries.today;
    let scroller = gtk::ScrolledWindow::new();
    scroller.set_hscrollbar_policy(gtk::PolicyType::Never);
    scroller.set_vexpand(true);
    scroller.set_margin_top(CLEAR_OF_EDGE);
    column.set_margin_bottom(32);
    scroller.set_child(Some(column));

    scroller.set_overflow(gtk::Overflow::Visible);
    if let Some(viewport) = scroller.child() {
        viewport.set_overflow(gtk::Overflow::Visible);
    }
    let page = gtk::Overlay::new();
    page.set_child(Some(&scroller));
    run_under_side(state, &page);

    let fade = gtk::Box::new(gtk::Orientation::Vertical, 0);
    fade.add_css_class("today-fade");
    fade.set_valign(gtk::Align::Start);
    fade.set_can_target(false);
    page.add_overlay(&fade);
    page
}

fn run_under_side(state: &App, page: &gtk::Overlay) {
    let split = &state.libraries.side.split;
    let sheet = gtk::CssProvider::new();

    sheet.load_from_string(&format!(".under-side .hero-under, .under-side .today-fade {{ margin-left: -{SIDE_WIDTH}px; min-width: {SIDE_WIDTH}px; }}"));
    gtk::style_context_add_provider_for_display(&page.display(), &sheet, gtk::STYLE_PROVIDER_PRIORITY_APPLICATION);
    let follow = glib::clone!(
        #[weak] page,
        move |split: &adw::OverlaySplitView| match split.is_collapsed() {
            true => page.remove_css_class("under-side"),
            false => page.add_css_class("under-side"),
        }
    );
    follow(split);
    split.connect_collapsed_notify(follow);

    page.connect_realize(|page| {
        let mut above = page.parent();
        while let Some(widget) = above.filter(|widget| !widget.is::<adw::OverlaySplitView>()) {
            widget.set_overflow(gtk::Overflow::Visible);
            above = widget.parent();
        }
    });
}

pub(super) fn show(state: &App) {
    let started = std::time::Instant::now();
    fill(state);
    state.stack.set_visible_child_name("today");
    log::info!("Today: {} ms on the main thread", started.elapsed().as_millis());
}

pub(super) fn faces_changed() {
    FACES.set(None);
}

struct Shot {
    id: i64,
    path: PathBuf,
    mtime: i64,
    when: i64,
    rating: u8,
    flag: Flag,
    edited: bool,
}

struct Waits {
    count: usize,
    one: &'static str,
    more: &'static str,
    caption: String,
    label: &'static str,
    go: Rc<dyn Fn(&App)>,

    covers: Vec<(PathBuf, i64, Option<String>)>,
}

const MOST_WAITING: usize = 3;

fn fill(state: &App) {
    let column = &state.libraries.today;
    clear(column);
    let filled = FILLED.get() + 1;
    FILLED.set(filled);
    if state.libraries.all.borrow().is_empty() {
        let empty = adw::StatusPage::new();
        no_libraries_yet(state, &empty);
        column.append(&empty);
        return;
    }
    let Some(date) = day() else { return };

    let seed = numa::io::import::day_seed(&date);

    let hero = gtk::Box::new(gtk::Orientation::Vertical, 0);
    fill_hero(state, &hero, seed);
    column.append(&hero);

    let parts = gtk::Box::new(gtk::Orientation::Vertical, 0);
    parts.set_margin_start(24);
    parts.set_margin_end(24);
    column.append(&parts);
    let column = &parts;

    let here: Vec<Library> = state.libraries.places.borrow().iter().filter_map(|place| match place {
        Place::Library(library) if !home_sync::is_home(state, library.id) && !state.catalog.is_offline(library.id) => Some(library.clone()),
        _ => None,
    }).collect();
    let shelves: Vec<(Library, Vec<Shot>)> = here
        .into_iter()
        .map(|library| {
            let mut shots: Vec<Shot> = state
                .catalog
                .photos(library.id, &Filter::default())
                .unwrap_or_default()
                .into_iter()
                .map(|photo| Shot { id: photo.id, when: photo.taken.unwrap_or(photo.mtime), path: photo.path, mtime: photo.mtime, rating: photo.rating, flag: photo.flag, edited: photo.edited })
                .collect();

            shots.sort_by_key(|shot| std::cmp::Reverse(shot.when));
            (library, shots)
        })
        .collect();
    let extras = waiting_here(state, &shelves);
    let mut shown = false;
    let mut years = numa::io::import::on_this_day(shelves.iter().flat_map(|(_, shots)| shots.iter().map(|shot| (shot.id, shot.when, shot.rating, shot.flag))), &date, YEARS);

    if let Some(mine) = mine(state) {
        years = years
            .into_iter()
            .filter_map(|(year, ids)| {
                let ids: Vec<i64> = ids.into_iter().filter(|id| mine.keeps(*id)).collect();
                (!ids.is_empty()).then_some((year, ids))
            })
            .collect();
    }
    if !years.is_empty() {
        column.append(&on_this_day(state, &shelves, &years, &date));
        shown = true;
    }

    let arrived = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let waiting = gtk::Box::new(gtk::Orientation::Vertical, 0);
    column.append(&arrived);
    column.append(&waiting);
    let left = Rc::new(Cell::new(2));
    let shown = Rc::new(Cell::new(shown));
    let settle = Rc::new(glib::clone!(
        #[strong] state,
        #[weak] column,
        #[strong] left,
        #[strong] shown,
        move || {
            left.set(left.get() - 1);
            if left.get() == 0 && !shown.get() && FILLED.get() == filled {

                let empty = adw::StatusPage::new();
                empty.set_title("All Caught Up");
                empty.set_description(Some("Photographs show here as they come in."));
                let import = gtk::Button::with_label("Import…");
                import.add_css_class("pill");
                import.add_css_class("raised-control");
                import.set_halign(gtk::Align::Center);
                import.connect_clicked(glib::clone!(
                    #[strong] state,
                    move |_| {
                        if let Some(window) = state.stack.root().and_downcast::<adw::ApplicationWindow>() {
                            import_dialog(&state, &window, None);
                        }
                    }
                ));
                empty.set_child(Some(&import));
                column.append(&empty);
            }
        }
    ));
    just_arrived(state, shelves, arrived, filled, shown.clone(), settle.clone());
    waiting_for_you(state, waiting, extras, filled, shown, settle);
    if let Some(libraries) = your_libraries(state, seed) {
        column.append(&libraries);
    }
}

fn fill_hero(state: &App, hero: &gtk::Box, seed: u64) {
    while let Some(child) = hero.first_child() {
        hero.remove(&child);
    }
    let shelves = places::shelves(state);
    let mine = mine(state);
    let mut pool: Vec<(PathBuf, i64, Option<String>, i64, String)> = Vec::new();
    for place in state.libraries.places.borrow().iter() {
        let Place::Library(library) = place else { continue };
        let Some(glance) = shelves.get(&library.id) else { continue };
        for (cover, id) in glance.covers.iter().zip(&glance.ids).filter(|(_, id)| mine.as_ref().is_none_or(|mine| mine.keeps(**id))).take(BEST) {
            pool.push((cover.0.clone(), cover.1, cover.2.clone(), *id, library.label()));
        }
    }
    if pool.is_empty() {
        return;
    }
    let turn = {
        let (day, turn) = HERO_TURN.get();
        if day == seed { turn } else { 0 }
    };
    let (path, mtime, edits, id, whence) = pool[numa::io::import::day_pick(seed, 0, turn, pool.len())].clone();
    let picture = hero_picture(state, id, &path, mtime, edits, &whence);

    let over = gtk::Overlay::new();
    picture.add_css_class("hero-under");

    picture.set_margin_bottom(4);
    over.set_child(Some(&picture));
    let scrim = gtk::Box::new(gtk::Orientation::Vertical, 0);
    scrim.add_css_class("hero-scrim");
    scrim.add_css_class("hero-under");
    scrim.set_can_target(false);
    over.add_overlay(&scrim);
    let words = gtk::Box::new(gtk::Orientation::Vertical, 8);
    words.add_css_class("hero-words");
    words.set_halign(gtk::Align::Start);
    words.set_valign(gtk::Align::End);
    words.set_margin_start(24);
    words.set_margin_end(24);
    words.set_margin_bottom(48);
    words.set_can_target(false);
    let what = gtk::Label::new(Some("PHOTOGRAPH OF THE DAY"));
    what.set_xalign(0.0);
    what.add_css_class("caption");
    let from = gtk::Label::new(Some(&whence));
    from.set_xalign(0.0);
    from.add_css_class("hero-title");
    from.set_ellipsize(gtk::pango::EllipsizeMode::End);
    words.append(&what);
    words.append(&from);

    let taken = state.catalog.taken_of(id).map(|taken| {
        let day = numa::io::import::day(taken, 0);
        let mut parts = day.split('-').map(|part| part.parse::<usize>().unwrap_or(1));
        let (year, month, date) = (parts.next().unwrap_or(1), parts.next().unwrap_or(1), parts.next().unwrap_or(1));
        format!("{date} {} {year}", super::grid::MONTH_NAMES[month.clamp(1, 12) - 1])
    });
    if let Some(taken) = taken {
        let when = gtk::Label::new(Some(&taken));
        when.set_xalign(0.0);
        when.add_css_class("hero-when");
        words.append(&when);
    }
    over.add_overlay(&words);

    let actions = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    actions.add_css_class("hero-actions");
    actions.set_halign(gtk::Align::End);
    actions.set_valign(gtk::Align::End);
    actions.set_margin_end(24);
    actions.set_margin_bottom(48);

    let open = primary_button("Open");
    open.connect_clicked(glib::clone!(
        #[strong] state,
        move |_| edit(&state, id)
    ));
    let next = gtk::Button::new();
    next.set_child(Some(&adw::ButtonContent::builder().icon_name("media-playlist-shuffle-symbolic").label("Shuffle").build()));
    next.add_css_class("flat");
    next.connect_clicked(glib::clone!(
        #[strong] state,
        #[weak] hero,
        move |_| {
            HERO_TURN.set((seed, turn + 1));
            fill_hero(&state, &hero, seed);
        }
    ));
    for button in [open.upcast_ref::<gtk::Widget>(), next.upcast_ref()] {
        actions.append(button);
    }
    over.add_overlay(&actions);

    hero.append(&over);
}

fn hero_picture(state: &App, id: i64, path: &Path, mtime: i64, edits: Option<String>, whence: &str) -> gtk::Picture {

    let face = state.catalog.faces_in(id).is_some_and(|faces| faces > 0);
    let paintable = HeroPaintable::new(face);

    let picture = gtk::Picture::for_paintable(&paintable);
    picture.set_content_fit(gtk::ContentFit::Fill);
    picture.set_size_request(-1, HERO_HEIGHT);
    picture.set_hexpand(true);
    picture.set_overflow(gtk::Overflow::Hidden);
    picture.update_property(&[gtk::accessible::Property::Label(&format!("Photograph of the day, {whence}"))]);

    let (near, shown) = (picture.downgrade(), paintable.downgrade());
    let at_home = home_fetch::preview(
        state,
        id,
        move |_| near.upgrade().is_some(),
        move |_, texture| {
            if let Some(paintable) = shown.upgrade() {
                paintable.set_texture(texture);
            }
        },
    );
    if !at_home {
        let (wanted, shown) = (picture.downgrade(), paintable.downgrade());
        crate::ui::thumbnail::load_thumbnail_while(
            path,
            mtime,
            HERO_EDGE,
            edits,
            None,
            move || wanted.upgrade().is_some(),
            move |texture| {
                if let Some(paintable) = shown.upgrade() {
                    paintable.set_texture(texture);
                }
            },
        );
    }
    picture
}

fn band_top(photo: (f64, f64), band: (f64, f64), face: bool) -> f64 {
    if photo.0 <= 0.0 || photo.1 <= 0.0 {
        return 0.0;
    }
    let scale = (band.0 / photo.0).max(band.1 / photo.1);

    let spare = ((photo.1 * scale).ceil() - band.1).max(0.0);
    match face {
        true => 0.0,
        false => (spare / 2.0).floor(),
    }
}

mod hero {
    use super::*;
    use gtk::gdk;
    use gdk::subclass::prelude::PaintableImpl;
    use gtk::subclass::prelude::*;

    #[derive(Default)]
    pub(super) struct HeroPaintable {
        pub(super) texture: RefCell<Option<gdk::Texture>>,
        pub(super) face: Cell<bool>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for HeroPaintable {
        const NAME: &'static str = "NumaHeroPaintable";
        type Type = super::HeroPaintable;
        type Interfaces = (gdk::Paintable,);
    }

    impl ObjectImpl for HeroPaintable {}

    impl PaintableImpl for HeroPaintable {

        fn intrinsic_width(&self) -> i32 {
            self.texture.borrow().as_ref().map_or(0, |texture| texture.width())
        }

        fn intrinsic_height(&self) -> i32 {
            self.texture.borrow().as_ref().map_or(0, |texture| texture.height())
        }

        fn snapshot(&self, snapshot: &gdk::Snapshot, width: f64, height: f64) {
            let texture = self.texture.borrow();
            let (Some(texture), Some(snapshot)) = (texture.as_ref(), snapshot.downcast_ref::<gtk::Snapshot>()) else { return };
            let photo = (texture.width() as f64, texture.height() as f64);
            let scale = (width / photo.0).max(height / photo.1);
            let (drawn_width, drawn_height) = ((photo.0 * scale).ceil(), (photo.1 * scale).ceil());
            let top = band_top(photo, (width, height), self.face.get());
            snapshot.push_clip(&gtk::graphene::Rect::new(0.0, 0.0, width as f32, height as f32));
            snapshot.save();
            snapshot.translate(&gtk::graphene::Point::new(((width - drawn_width) / 2.0).trunc() as f32, -top as f32));

            texture.snapshot(snapshot, drawn_width, drawn_height);
            snapshot.restore();
            snapshot.pop();
        }
    }
}

glib::wrapper! {
    struct HeroPaintable(ObjectSubclass<hero::HeroPaintable>) @implements gtk::gdk::Paintable;
}

impl HeroPaintable {
    fn new(face: bool) -> Self {
        use gtk::subclass::prelude::*;
        let paintable: Self = glib::Object::new();
        paintable.imp().face.set(face);
        paintable
    }

    fn set_texture(&self, texture: gtk::gdk::Texture) {
        use gtk::subclass::prelude::*;
        self.imp().texture.replace(Some(texture));
        self.invalidate_size();
        self.invalidate_contents();
    }
}

fn edit(state: &App, id: i64) {
    open_these(state, &[id], 0);

    let then = move |state: &App| {
        OPENED_HERE.set(true);
        open_photo(state, id);
        close_loupe(state);
    };
    if !home_fetch::fetch_first(state, id, then) {
        OPENED_HERE.set(true);
        open_photo(state, id);
        close_loupe(state);
    }
}

pub(super) fn back_from_editor(state: &App) -> bool {
    if !OPENED_HERE.replace(false) {
        return false;
    }
    show(state);
    true
}

fn your_libraries(state: &App, seed: u64) -> Option<gtk::Box> {
    let shelves = places::shelves(state);
    let part = part("Your Libraries", None);
    let flow = gtk::FlowBox::new();
    flow.set_selection_mode(gtk::SelectionMode::None);
    flow.set_homogeneous(true);
    flow.set_column_spacing(12);
    flow.set_row_spacing(12);
    flow.set_max_children_per_line(12);
    let mut any = false;
    for place in state.libraries.places.borrow().iter() {
        let Place::Library(library) = place else { continue };
        if any && flow.observe_children().n_items() as usize >= FANS {
            break;
        }
        let glance = shelves.get(&library.id);
        let (covers, caption) = match glance {
            Some(glance) => {
                let best = glance.covers.len().min(BEST);
                let start = numa::io::import::day_pick(seed, library.id, 0, best);
                let covers: Vec<_> = (0..best.min(places::FAN_SIZE)).map(|at| glance.covers[(start + at) % best].clone()).collect();
                (covers, places::photos(glance.photos as usize))
            }
            None => (Vec::new(), "Not connected".to_string()),
        };
        let stack = stack(&library.label(), &caption, &covers);
        if glance.is_none() {
            stack.set_opacity(0.55);
        }
        let library = library.clone();
        stack.connect_clicked(glib::clone!(
            #[strong] state,
            move |_| show_place(&state, Place::Library(library.clone()))
        ));
        flow.append(&stack);
        any = true;
    }
    if !any {
        return None;
    }
    part.append(&flow);
    Some(part)
}

fn waiting_here(state: &App, shelves: &[(Library, Vec<Shot>)]) -> Vec<Waits> {
    let mut found = Vec::new();
    let covers_of = |ids: &[i64]| -> Vec<(PathBuf, i64, Option<String>)> {
        ids.iter()
            .filter_map(|id| shelves.iter().flat_map(|(_, shots)| shots).find(|shot| shot.id == *id))
            .take(places::FAN_SIZE)
            .map(|shot| (shot.path.clone(), shot.mtime, state.catalog.edits_json(shot.id).ok().flatten()))
            .collect()
    };
    let most = |wanted: &dyn Fn(&Shot) -> bool| -> Option<(&Library, Vec<i64>)> {
        shelves
            .iter()
            .map(|(library, shots)| (library, shots.iter().filter(|shot| wanted(shot)).map(|shot| shot.id).collect::<Vec<i64>>()))
            .filter(|(_, ids)| !ids.is_empty())
            .max_by_key(|(_, ids)| ids.len())
    };
    if let Some((library, ids)) = most(&|shot| shot.flag == Flag::Picked && !shot.edited) {
        found.push(Waits {
            count: ids.len(),
            one: "pick not edited yet",
            more: "picks not edited yet",
            caption: library.label(),
            label: "Edit",
            covers: covers_of(&ids),
            go: Rc::new(move |state: &App| open_these(state, &ids, 0)),
        });
    }
    if let Some((library, ids)) = most(&|shot| shot.rating == 0 && shot.flag == Flag::None) {
        let library = library.clone();
        found.push(Waits {
            count: ids.len(),
            one: "photograph not rated yet",
            more: "photographs not rated yet",
            caption: library.label(),
            label: "Rate",
            covers: covers_of(&ids),
            go: Rc::new(move |state: &App| show_place(state, Place::Library(library.clone()))),
        });
    }

    if home::account(state).signed_in() {
        let apart_libraries: Vec<&(Library, Vec<Shot>)> = shelves.iter().filter(|(library, _)| !home_sync::is_linked(state, library.id)).collect();
        let apart = apart_libraries.len();
        if apart > 0 {
            let first: Vec<i64> = apart_libraries[0].1.iter().take(places::FAN_SIZE).map(|shot| shot.id).collect();
            found.push(Waits {
                count: apart,
                one: "library not at Numa Home",
                more: "libraries not at Numa Home",
                caption: "Libraries".to_string(),
                label: "Sync",
                covers: covers_of(&first),
                go: Rc::new(show_folders),
            });
        }
    }
    found
}

fn mine(state: &App) -> Option<numa::io::import::Mine> {
    use numa::io::import::{MINE_ON, MY_CAMERAS, ME};
    if !state.catalog.recall::<bool>(MINE_ON).unwrap_or(false) {
        return None;
    }
    let cameras = state.catalog.recall::<Vec<String>>(MY_CAMERAS).unwrap_or_default();
    let me = state.catalog.recall::<String>(ME).filter(|name| !name.is_empty());
    Some(state.catalog.mine(&cameras, me.as_deref())).filter(|mine| mine.is_set())
}

pub(super) fn install(state: &App, window: &adw::ApplicationWindow) {
    use numa::io::import::MINE_ON;
    let mine = gio::SimpleAction::new_stateful("today-mine", None, &state.catalog.recall::<bool>(MINE_ON).unwrap_or(false).to_variant());
    mine.connect_change_state(glib::clone!(
        #[strong] state,
        move |action, wanted| {
            let Some(on) = wanted.and_then(|wanted| wanted.get::<bool>()) else { return };
            action.set_state(&on.to_variant());
            state.catalog.remember(MINE_ON, &on);

            if on && unset(&state) {
                whose(&state);
            } else {
                fill(&state);
            }
        }
    ));
    window.add_action(&mine);
    let choose = gio::SimpleAction::new("today-whose", None);
    choose.connect_activate(glib::clone!(
        #[strong] state,
        move |_, _| whose(&state)
    ));
    window.add_action(&choose);
}

fn unset(state: &App) -> bool {
    use numa::io::import::{MY_CAMERAS, ME};
    state.catalog.recall::<Vec<String>>(MY_CAMERAS).unwrap_or_default().is_empty()
        && state.catalog.recall::<String>(ME).unwrap_or_default().is_empty()
}

fn whose(state: &App) {
    use numa::io::import::{MINE_ON, MY_CAMERAS, ME};
    let Some(window) = state.stack.root().and_downcast::<adw::ApplicationWindow>() else { return };
    let list = gtk::Box::new(gtk::Orientation::Vertical, 4);
    list.set_margin_top(8);
    list.set_margin_bottom(24);
    list.set_margin_start(24);
    list.set_margin_end(24);
    if state.catalog.recall::<bool>(MINE_ON).unwrap_or(false) && unset(state) {
        let say = gtk::Label::new(Some("Choose your cameras, or who you are, to see only your own."));
        say.set_xalign(0.0);
        say.set_wrap(true);
        say.add_css_class("dim-label");
        say.set_margin_bottom(8);
        list.append(&say);
    }
    let cameras: Vec<(String, i64)> = state.catalog.all_cameras();
    let keys: Vec<String> = cameras.iter().map(|(key, _)| key.clone()).collect();
    let mine_now: std::collections::HashSet<String> = state.catalog.recall::<Vec<String>>(MY_CAMERAS).unwrap_or_default().into_iter().collect();
    let chosen = Rc::new(RefCell::new(mine_now.clone()));
    let head = gtk::Label::new(Some("Taken with"));
    head.set_xalign(0.0);
    head.add_css_class("heading");
    list.append(&head);
    if cameras.is_empty() {
        let none = gtk::Label::new(Some("No camera is known yet: Numa reads them as it scans."));
        none.set_xalign(0.0);
        none.add_css_class("dim-label");
        list.append(&none);
    }
    for (key, count) in &cameras {
        let name = numa::io::clocks::camera_name(key, &keys);
        let check = gtk::CheckButton::with_label(&format!("{name} \u{b7} {}", places::photos(*count as usize)));
        check.set_active(mine_now.contains(key));
        check.connect_toggled(glib::clone!(
            #[strong] state,
            #[strong] chosen,
            #[strong] key,
            move |check| {
                match check.is_active() {
                    true => chosen.borrow_mut().insert(key.clone()),
                    false => chosen.borrow_mut().remove(&key),
                };
                let mut all: Vec<String> = chosen.borrow().iter().cloned().collect();
                all.sort();
                state.catalog.remember(MY_CAMERAS, &all);
                fill(&state);
            }
        ));
        list.append(&check);
    }
    let who = gtk::Label::new(Some("I am"));
    who.set_xalign(0.0);
    who.add_css_class("heading");
    who.set_margin_top(8);
    list.append(&who);
    let names = state.catalog.names().unwrap_or_default();
    let mut model: Vec<&str> = vec!["Nobody"];
    model.extend(names.iter().map(String::as_str));
    let drop = gtk::DropDown::from_strings(&model);
    let me = state.catalog.recall::<String>(ME).unwrap_or_default();
    drop.set_selected(names.iter().position(|name| name.eq_ignore_ascii_case(&me)).map_or(0, |at| at as u32 + 1));
    drop.connect_selected_notify(glib::clone!(
        #[strong] state,
        #[strong] names,
        move |drop| {
            let name = match drop.selected() {
                0 => String::new(),
                at => names.get(at as usize - 1).cloned().unwrap_or_default(),
            };
            if state.catalog.recall::<String>(ME).unwrap_or_default() != name {
                state.catalog.remember(ME, &name);
                fill(&state);
            }
        }
    ));
    list.append(&drop);
    if names.is_empty() {
        let hint = gtk::Label::new(Some("Name yourself in People first, to be found in photographs."));
        hint.set_xalign(0.0);
        hint.add_css_class("dim-label");
        list.append(&hint);
    }
    let view = adw::ToolbarView::new();
    view.add_top_bar(&adw::HeaderBar::new());
    view.set_content(Some(&list));
    let dialog = adw::Dialog::new();
    dialog.set_title("Whose Photographs");
    dialog.set_content_width(360);
    dialog.set_child(Some(&view));
    dialog.present(Some(&window));
}

pub(super) fn day() -> Option<String> {
    let now = glib::DateTime::now_local().ok()?;
    Some(match std::env::var("NUMA_DAY").ok() {
        Some(day) if day.len() == 5 => format!("{}-{day}", now.year()),
        _ => format!("{}-{:02}-{:02}", now.year(), now.month(), now.day_of_month()),
    })
}

pub(super) fn said(date: &str) -> String {
    const DAYS: [&str; 7] = ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"];
    const MONTHS: [&str; 12] =
        ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
    let mut parts = date.split('-').map(|part| part.parse::<i32>().unwrap_or(1));
    let (year, month, day) = (parts.next().unwrap_or(2026), parts.next().unwrap_or(1), parts.next().unwrap_or(1));
    glib::DateTime::from_local(year, month, day, 12, 0, 0.0)
        .map(|at| format!("{} {day} {}", DAYS[(at.day_of_week() as usize).clamp(1, 7) - 1], MONTHS[(month as usize).clamp(1, 12) - 1]))
        .unwrap_or_default()
}

fn part(label: &str, aside: Option<&str>) -> gtk::Box {

    let part = gtk::Box::new(gtk::Orientation::Vertical, 16);
    part.set_margin_top(48);
    let line = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let title = gtk::Label::new(Some(label));
    title.set_xalign(0.0);
    title.set_hexpand(true);
    title.add_css_class("title-4");
    line.append(&title);
    if let Some(aside) = aside {
        let caption = gtk::Label::new(Some(aside));
        caption.add_css_class("caption");
        caption.add_css_class("dim-label");
        caption.set_valign(gtk::Align::Baseline);
        line.append(&caption);
    }
    part.append(&line);
    part
}

fn on_this_day(state: &App, shelves: &[(Library, Vec<Shot>)], years: &[(i64, Vec<i64>)], date: &str) -> gtk::Box {
    let this_year: i64 = date[..4].parse().unwrap_or(0);
    let part = part("On This Day", Some("Picks and 3 stars or more first"));
    let flow = gtk::FlowBox::new();
    flow.set_selection_mode(gtk::SelectionMode::None);
    flow.set_homogeneous(true);
    flow.set_column_spacing(12);
    flow.set_row_spacing(12);
    flow.set_max_children_per_line(12);
    let find = |id: i64| shelves.iter().flat_map(|(_, shots)| shots).find(|shot| shot.id == id);
    for (year, ids) in years {
        let covers: Vec<(PathBuf, i64, Option<String>)> = ids
            .iter()
            .filter_map(|id| find(*id))
            .take(places::FAN_SIZE)
            .map(|shot| (shot.path.clone(), shot.mtime, state.catalog.edits_json(shot.id).ok().flatten()))
            .collect();
        let mut from: Vec<i64> = ids.iter().map(|id| numa::io::catalog::library_of(*id)).collect();
        from.sort_unstable();
        from.dedup();

        let whence = match from[..] {
            [only] => shelves.iter().find(|(library, _)| library.id == only).map(|(library, _)| library.label()).unwrap_or_default(),
            _ => format!("{} libraries", from.len()),
        };
        let ago = this_year - year;
        let name = match ago {
            1 => "1 Year Ago".to_string(),
            n => format!("{n} Years Ago"),
        };
        let stack = stack(&name, &format!("{whence} \u{b7} {}", places::photos(ids.len())), &covers);
        let ids = ids.clone();
        stack.connect_clicked(glib::clone!(
            #[strong] state,
            move |_| open_these(&state, &ids, 0)
        ));
        flow.append(&stack);
    }
    part.append(&flow);
    part
}

pub(super) fn stack(name: &str, caption: &str, covers: &[(PathBuf, i64, Option<String>)]) -> gtk::Button {
    let fan = places::fan(TILE);
    places::show_covers(&fan, covers, TILE);
    fan.set_halign(gtk::Align::Center);
    let title = gtk::Label::new(Some(name));
    title.add_css_class("heading");
    let under = gtk::Label::new(Some(caption));
    under.add_css_class("caption");
    under.add_css_class("dim-label");
    under.set_wrap(true);
    under.set_justify(gtk::Justification::Center);
    under.set_max_width_chars(30);
    let inside = gtk::Box::new(gtk::Orientation::Vertical, 4);
    inside.set_margin_top(12);
    inside.set_margin_bottom(12);
    inside.append(&fan);
    inside.append(&title);
    inside.append(&under);
    let button = gtk::Button::new();
    button.add_css_class("flat");
    button.set_child(Some(&inside));
    button
}

fn just_arrived(state: &App, shelves: Vec<(Library, Vec<Shot>)>, into: gtk::Box, filled: u64, shown: Rc<Cell<bool>>, settle: Rc<dyn Fn()>) {
    let since = glib::real_time() / 1_000_000 - NEWS;
    let now = since + NEWS;
    let looked: Vec<(i64, Vec<(i64, PathBuf, i64)>)> = shelves
        .into_iter()
        .map(|(library, shots)| (library.id, shots.into_iter().map(|shot| (shot.id, shot.path, shot.mtime)).collect()))
        .collect();
    let state = state.clone();
    glib::spawn_future_local(async move {
        let came = gio::spawn_blocking(move || {
            let mut came: Vec<(i64, i64, Vec<(i64, PathBuf, i64)>)> = Vec::new();
            for (library, shots) in looked {
                let mut latest = 0;
                let new: Vec<(i64, PathBuf, i64)> = shots
                    .into_iter()
                    .filter(|(_, path, _)| {
                        let born = std::fs::metadata(path).and_then(|meta| meta.created()).ok();
                        let born = born.and_then(|at| at.duration_since(std::time::UNIX_EPOCH).ok()).map_or(0, |at| at.as_secs() as i64);
                        latest = latest.max(born);
                        born >= since
                    })
                    .collect();
                if !new.is_empty() {
                    came.push((latest, library, new));
                }
            }
            came.sort_by_key(|(at, ..)| std::cmp::Reverse(*at));
            came.truncate(ARRIVALS);
            came
        })
        .await
        .unwrap_or_default();
        if FILLED.get() != filled {
            return;
        }
        if !came.is_empty() {
            shown.set(true);
            let part = part("Just Arrived", None);
            for (at, library, shots) in came {
                let name = state.libraries.all.borrow().iter().find(|one| one.id == library).map(Library::label).unwrap_or_default();
                part.append(&arrival(&state, library, &name, &shots, now - at));
            }
            into.append(&part);
        }
        settle();
    });
}

fn arrival(state: &App, library: i64, name: &str, shots: &[(i64, PathBuf, i64)], ago: i64) -> gtk::Box {
    let days = ago / 86_400;
    let came = match days {
        0 => "today".to_string(),
        1 => "yesterday".to_string(),
        n => format!("{n} days"),
    };
    let line = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let title = gtk::Label::new(Some(name));
    title.add_css_class("heading");
    let caption = gtk::Label::new(Some(&format!("{} \u{b7} {came}", places::photos(shots.len()))));
    caption.add_css_class("caption");
    caption.add_css_class("dim-label");
    caption.set_hexpand(true);
    caption.set_xalign(0.0);
    let open = gtk::Button::with_label("Open");
    open.connect_clicked(glib::clone!(
        #[strong] state,
        move |_| {
            let found = state.libraries.all.borrow().iter().find(|one| one.id == library).cloned();
            if let Some(found) = found {
                show_place(&state, Place::Library(found));
            }
        }
    ));
    line.append(&title);
    line.append(&caption);
    line.append(&open);
    let strip = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    let ids: Vec<i64> = shots.iter().take(STRIP).map(|(id, ..)| *id).collect();
    for (at, (id, path, mtime)) in shots.iter().take(STRIP).enumerate() {
        let picture = gtk::Picture::new();
        picture.set_content_fit(gtk::ContentFit::Cover);
        picture.set_size_request(STRIP_HEIGHT * 3 / 2, STRIP_HEIGHT);
        picture.add_css_class("print");
        picture.set_overflow(gtk::Overflow::Hidden);
        let edits = state.catalog.edits_json(*id).ok().flatten();
        let wanted = picture.downgrade();
        let shown = picture.downgrade();
        crate::ui::thumbnail::load_thumbnail_while(
            path,
            *mtime,
            GRID_THUMB_EDGE,
            edits,
            None,

            move || wanted.upgrade().is_some(),
            move |texture| {
                if let Some(picture) = shown.upgrade() {
                    picture.set_paintable(Some(&texture));
                }
            },
        );
        let button = gtk::Button::new();
        button.add_css_class("flat");
        button.add_css_class("strip-photo");
        button.set_child(Some(&picture));
        button.set_tooltip_text(Some(&format!("{name}, photograph {} of {}", at + 1, ids.len())));
        let ids = ids.clone();
        button.connect_clicked(glib::clone!(
            #[strong] state,
            move |_| open_these(&state, &ids, at)
        ));
        strip.append(&button);
    }
    let arrival = gtk::Box::new(gtk::Orientation::Vertical, 8);
    arrival.set_margin_bottom(12);
    arrival.append(&line);
    arrival.append(&strip);
    arrival
}

fn waiting_for_you(state: &App, into: gtk::Box, extras: Vec<Waits>, filled: u64, shown: Rc<Cell<bool>>, settle: Rc<dyn Fn()>) {
    let state = state.clone();
    glib::spawn_future_local(async move {
        let sets = tidy::waiting(&state).await;
        let faces = match FACES.get().filter(|(at, _)| at.elapsed() < FRESH) {
            Some((_, count)) => count,
            None => {

                let named = state.catalog.named_faces().unwrap_or_default();
                let known = state.catalog.known_from(&named).unwrap_or_default();
                let each: Vec<Vec<numa::io::catalog::StoredFace>> =
                    state.libraries.all.borrow().iter().filter_map(|library| state.catalog.faces(library.id).ok()).collect();
                let count = gio::spawn_blocking(move || {
                    each.iter()
                        .map(|faces| {
                            let names: Vec<Option<String>> = faces.iter().map(|face| cull::people::recognise(&face.embedding, &known).map(|(name, _)| name.to_string())).collect();
                            numa::io::analysis::unnamed_groups(faces, &names).0.len()
                        })
                        .sum::<usize>()
                })
                .await
                .unwrap_or(0);
                FACES.set(Some((std::time::Instant::now(), count)));
                count
            }
        };
        if FILLED.get() != filled {
            return;
        }

        let mut all: Vec<Waits> = Vec::new();
        all.push(Waits { count: faces, one: "face to name", more: "faces to name", caption: "People \u{b7} not named yet".to_string(), label: "Name", go: Rc::new(show_people), covers: Vec::new() });
        let (mut picks_rate, mut rest): (Vec<Waits>, Vec<Waits>) = extras.into_iter().partition(|wait| wait.label != "Sync");
        all.append(&mut picks_rate);
        all.push(Waits { count: sets, one: "set to choose from", more: "sets to choose from", caption: "Tidy Up".to_string(), label: "Choose", go: Rc::new(tidy::show), covers: Vec::new() });
        all.append(&mut rest);
        let waits: Vec<gtk::Box> = all
            .into_iter()
            .filter(|wait| wait.count > 0)
            .take(MOST_WAITING)
            .map(|wait| wait_line(&state, &wait))
            .collect();
        if !waits.is_empty() {
            shown.set(true);
            let part = part("Waiting for You", None);

            let row = gtk::Box::new(gtk::Orientation::Horizontal, 16);
            row.set_homogeneous(true);
            waits.iter().for_each(|card| row.append(card));
            part.append(&row);
            into.append(&part);
        }
        settle();
    });
}

fn wait_line(state: &App, wait: &Waits) -> gtk::Box {
    let line = gtk::Box::new(gtk::Orientation::Horizontal, 16);
    line.add_css_class("today-card");
    if !wait.covers.is_empty() {
        let fan = places::fan(FAN_TILE);
        places::show_covers(&fan, &wait.covers, FAN_TILE);
        fan.set_valign(gtk::Align::Center);
        line.append(&fan);
    }
    let words = gtk::Box::new(gtk::Orientation::Vertical, 0);
    words.set_hexpand(true);
    let what = if wait.count == 1 { wait.one } else { wait.more };
    let name = gtk::Label::new(Some(&format!("{} {what}", places::grouped(wait.count as i64))));
    name.set_xalign(0.0);
    name.set_wrap(true);
    name.set_max_width_chars(16);
    name.add_css_class("heading");
    let under = gtk::Label::new(Some(&wait.caption));
    under.set_xalign(0.0);
    under.set_ellipsize(gtk::pango::EllipsizeMode::End);
    under.add_css_class("caption");
    under.add_css_class("dim-label");
    words.append(&name);
    words.append(&under);
    let button = gtk::Button::with_label(wait.label);
    button.add_css_class("raised-control");
    button.set_valign(gtk::Align::Center);
    let go = wait.go.clone();
    button.connect_clicked(glib::clone!(
        #[strong] state,
        move |_| go(&state)
    ));
    line.append(&words);
    line.append(&button);
    line
}

pub(super) fn open_these(state: &App, ids: &[i64], at: usize) {
    let mut from: Vec<i64> = ids.iter().map(|id| numa::io::catalog::library_of(*id)).collect();
    from.dedup();
    let library = match from[..] {
        [only] => state.libraries.all.borrow().iter().find(|one| one.id == only).cloned(),
        _ => None,
    };
    show_place(state, library.map_or(Place::Everywhere, Place::Library));
    let indices: Vec<usize> = {
        let lazy = state.grid.lazy.borrow();
        ids.iter().filter_map(|id| lazy.iter().position(|card| card.id == *id)).collect()
    };
    let Some(&first) = indices.get(at).or(indices.first()) else {
        state.toast("Those photographs are not in the grid's filter");
        return;
    };
    state.grid.wall.select_these(&indices, first);
    state.grid.wall.reveal(first);
    if indices.len() > 1 {
        state.loupe.few.borrow_mut().selection = indices;
    }
    show_loupe(state, first);
}

#[cfg(test)]
mod tests {
    use super::band_top;

    const BAND: (f64, f64) = (1530.0, 600.0);

    #[test]
    fn a_portrait_with_a_face_is_shown_from_its_top() {

        assert_eq!(band_top((2000.0, 3000.0), BAND, true), 0.0);
        assert_eq!(band_top((2000.0, 3000.0), BAND, false), 847.0);
    }

    #[test]
    fn without_a_face_the_middle_as_cover_has_it() {

        assert_eq!(band_top((6000.0, 4000.0), BAND, false), 210.0);
        assert_eq!(band_top((6000.0, 4000.0), BAND, true), 0.0);
    }

    #[test]
    fn a_panorama_fills_the_height_and_loses_its_sides() {

        assert_eq!(band_top((8000.0, 2000.0), BAND, false), 0.0);
        assert_eq!(band_top((8000.0, 2000.0), BAND, true), 0.0);
    }

    #[test]
    fn nothing_yet_is_no_offset() {
        assert_eq!(band_top((0.0, 0.0), BAND, true), 0.0);
    }
}
