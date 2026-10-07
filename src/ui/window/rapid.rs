use super::*;
use numa::io::workflows::{self as kinds, Kind};

const SIZES: (f32, f32) = (160.0, 8.0);
const SIZES_KEY: &str = "rapid-sizes";

pub(super) struct Tile {
    pub(super) id: i64,

    pub(super) members: Vec<i64>,

    pub(super) stacked: usize,
}

impl Tile {
    pub(super) fn is_stack(&self) -> bool {
        self.stacked > 1
    }
}

pub(super) struct Moment {
    pub(super) key: i64,
    pub(super) ids: Vec<i64>,
    pub(super) times: Vec<i64>,
    pub(super) tiles: Vec<Tile>,

    pub(super) label: Option<String>,
}

impl Moment {
    pub(super) fn start(&self) -> i64 {
        self.times[0]
    }

    pub(super) fn end(&self) -> i64 {
        self.times[self.times.len() - 1]
    }

    pub(super) fn face(&self) -> i64 {
        self.tiles.get(self.tiles.len() / 2).map_or(self.ids[0], |tile| tile.id)
    }
}

#[derive(Clone)]
pub(super) struct State {

    pub(super) views: gtk::Stack,
    view: gtk::Box,
    pub(super) list: gtk::Box,
    pub(super) scroller: gtk::ScrolledWindow,

    pub(super) chapter_bar: gtk::Box,
    pub(super) chapter_marks: Rc<RefCell<Vec<(gtk::Widget, gtk::ToggleButton, String)>>>,
    pub(super) sticky: gtk::MenuButton,
    pub(super) following: Rc<Cell<bool>>,

    by_time: gtk::ToggleButton,
    by_likeness: gtk::ToggleButton,
    pub(super) panel: gtk::Box,

    pub(super) chosen: Rc<RefCell<Option<(usize, gtk::Box)>>>,

    pub(super) filled: Rc<Cell<Option<usize>>>,
    pub(super) note: gtk::MenuButton,

    first_look: gtk::ToggleButton,

    clipping: gtk::ToggleButton,
    deliver: gtk::Button,
    pub(super) moments: Rc<RefCell<Vec<Moment>>>,

    tiles: Rc<RefCell<Vec<Vec<Vec<gtk::Overlay>>>>>,

    asked: Rc<RefCell<std::collections::HashSet<i64>>>,
    sweeping: Rc<Cell<u64>>,

    counts: Rc<RefCell<Vec<gtk::Label>>>,

    pub(super) at: Rc<Cell<(usize, usize)>>,

    pub(super) as_shot: Rc<RefCell<HashMap<i64, WhiteBalance>>>,
    pub(super) ahead: rapid_numa::Ahead,
    pub(super) pages: rapid_pages::Pages,
}

impl State {
    pub(super) fn new() -> Self {
        let list = gtk::Box::new(gtk::Orientation::Vertical, 22);
        list.add_css_class("rapid-list");
        let panel = gtk::Box::new(gtk::Orientation::Vertical, 6);
        panel.add_css_class("rapid-panel");

        let view = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        view.add_css_class("linked");
        for (label, target, tip) in [("Grid", "grid", "Every photograph, in rows"), ("Moments", "rapid", "The shoot by moment, for a quick pass")] {
            let toggle = gtk::ToggleButton::with_label(label);
            toggle.set_action_name(Some("win.library-view"));
            toggle.set_action_target_value(Some(&target.to_variant()));
            toggle.set_tooltip_text(Some(tip));
            toggle.set_focus_on_click(false);
            view.append(&toggle);
        }
        let first_look = gtk::ToggleButton::with_label("First Look");
        first_look.set_tooltip_text(Some("One frame from each moment: is its look right, and is it one for the teaser"));
        let clipping = gtk::ToggleButton::with_label("Clipping");
        clipping.set_tooltip_text(Some("Where the frames clip, as they are set now: red where the highlights are blown, blue where the shadows are black (J)"));
        let deliver = gtk::Button::with_label("Deliver…");

        let chapter_bar = gtk::Box::new(gtk::Orientation::Vertical, 2);
        let sticky = gtk::MenuButton::new();
        sticky.add_css_class("flat");
        sticky.set_visible(false);
        sticky.set_tooltip_text(Some("The chapter the list is in — and the others, to go to"));
        let chapters = gtk::Popover::new();
        chapters.set_child(Some(&chapter_bar));
        sticky.set_popover(Some(&chapters));
        let by_time = gtk::ToggleButton::with_label("Time");
        by_time.set_tooltip_text(Some("Moments: what was shot together in time"));
        let by_likeness = gtk::ToggleButton::with_label("Likeness");
        by_likeness.set_tooltip_text(Some("Setups: the same scene together, even hours apart"));
        by_likeness.set_group(Some(&by_time));
        Self {
            views: gtk::Stack::new(),
            view,
            list,
            scroller: gtk::ScrolledWindow::new(),
            chapter_bar,
            chapter_marks: Rc::default(),
            sticky,
            following: Rc::default(),
            by_time,
            by_likeness,
            panel,
            chosen: Rc::default(),
            filled: Rc::default(),
            note: gtk::MenuButton::new(),
            first_look,
            clipping,
            deliver,
            moments: Rc::default(),
            tiles: Rc::default(),
            asked: Rc::default(),
            sweeping: Rc::default(),
            counts: Rc::default(),
            at: Rc::default(),
            as_shot: Rc::default(),
            ahead: rapid_numa::Ahead::default(),
            pages: rapid_pages::Pages::new(),
        }
    }
}

pub(super) fn view_button(state: &App) -> gtk::Box {
    of(state).view.clone()
}

pub(super) fn deliver_button(state: &App) -> gtk::Button {
    of(state).deliver.clone()
}

pub(super) fn view_options(state: &App) -> gtk::Box {
    let rapid = of(state);
    let column = gtk::Box::new(gtk::Orientation::Vertical, 12);
    for (name, parts) in [
        ("Moments By", [rapid.by_time.upcast_ref::<gtk::Widget>(), rapid.by_likeness.upcast_ref()]),
        ("Show", [rapid.first_look.upcast_ref(), rapid.clipping.upcast_ref()]),
    ] {
        let row = gtk::Box::new(gtk::Orientation::Vertical, 6);
        row.append(&gtk::Label::builder().label(name).xalign(0.0).margin_start(12).margin_end(12).build());
        let chips = chip_row();
        chips.set_margin_start(12);
        for part in parts {
            chips.append(part);
        }
        row.append(&chips);
        column.append(&row);
    }

    let kind = gtk::Box::new(gtk::Orientation::Vertical, 6);
    kind.append(&gtk::Label::builder().label("Kind of Shoot").xalign(0.0).margin_start(12).margin_end(12).build());
    let slot = gtk::Box::new(gtk::Orientation::Vertical, 0);
    slot.set_margin_start(12);
    slot.set_margin_end(12);
    kind.append(&slot);
    column.append(&kind);
    column.connect_map(glib::clone!(
        #[strong] state,
        #[weak] slot,
        move |_| {
            while let Some(child) = slot.first_child() {
                slot.remove(&child);
            }
            slot.append(&rapid_panel::kind_choice(&state));
        }
    ));
    column
}

pub(super) fn of(state: &App) -> &State {
    &state.libraries.rapid
}

pub(super) fn is_on(state: &App) -> bool {
    of(state).views.visible_child_name().as_deref() == Some("rapid")
}

pub(super) fn first_look(state: &App) -> bool {
    of(state).first_look.is_active()
}

pub(super) fn build(state: &App, grid: &gtk::ScrolledWindow) -> gtk::Stack {
    let rapid = of(state);

    kit::primary(&rapid.deliver);
    rapid.by_likeness.connect_toggled(glib::clone!(
        #[strong] state,
        move |likeness| {
            if of(&state).following.get() {
                return;
            }
            let Some(library) = library_of_view(&state) else { return };
            let _ = state.catalog.set_library_setting(library, "group-by", Some(if likeness.is_active() { "likeness" } else { "time" }));
            rebuild(&state);
        }
    ));
    rapid.first_look.connect_toggled(glib::clone!(
        #[strong] state,
        move |_| {
            set_deliver_label(&state);
            fill_list(&state);
        }
    ));
    set_deliver_label(state);
    rapid.clipping.set_active(state.catalog.recall::<bool>("rapid-clipping").unwrap_or(false));
    rapid.clipping.connect_toggled(glib::clone!(
        #[strong] state,
        move |clipping| {
            state.catalog.remember("rapid-clipping", &clipping.is_active());
            fill_list(&state);
            if of(&state).views.visible_child_name().as_deref() == Some("check") {
                rapid_pages::show_check(&state);
            }
        }
    ));
    rapid.deliver.connect_clicked(glib::clone!(
        #[strong] state,
        move |_| match first_look(&state) {
            true => rapid_pages::export_teaser(&state),
            false => rapid_pages::deliver(&state),
        }
    ));

    let scroller = &rapid.scroller;
    scroller.set_hscrollbar_policy(gtk::PolicyType::Never);
    scroller.set_child(Some(&rapid.list));
    scroller.vadjustment().connect_value_changed(glib::clone!(
        #[strong] state,
        move |_| {
            rapid_chapters::follow(&state);
            sweep_soon(&state);
        }
    ));

    scroller.vadjustment().connect_changed(glib::clone!(
        #[strong] state,
        move |_| sweep_soon(&state)
    ));

    let over = gtk::Overlay::new();
    over.set_vexpand(true);
    over.set_child(Some(scroller));
    let bar = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    bar.add_css_class("rapid-sticky");
    bar.set_valign(gtk::Align::Start);
    bar.append(&rapid.sticky);
    rapid.sticky.bind_property("visible", &bar, "visible").sync_create().build();
    over.add_overlay(&bar);
    let left = gtk::Box::new(gtk::Orientation::Vertical, 0);
    left.set_hexpand(true);
    left.append(&over);
    let side = gtk::ScrolledWindow::new();
    side.set_hscrollbar_policy(gtk::PolicyType::Never);
    side.set_size_request(340, -1);

    side.set_hexpand(false);
    side.set_child(Some(&rapid.panel));
    side.add_css_class("rapid-side");
    let page = gtk::Box::new(gtk::Orientation::Horizontal, 0);

    page.add_css_class("numa-content");
    page.append(&left);
    page.append(&side);
    rapid.views.add_named(grid, Some("grid"));
    rapid.views.add_named(&page, Some("rapid"));
    rapid_pages::build(state, &rapid.views);
    rapid_panel::build_note(state);
    rapid.views.clone()
}

fn set_deliver_label(state: &App) {
    let rapid = of(state);
    let (label, tip) = match first_look(state) {
        true => ("Share Teaser…", "The frames marked for the teaser, written out to send tonight"),
        false => ("Deliver…", "A last look through the picks, then the teaser and the gallery written out"),
    };
    rapid.deliver.set_label(label);
    rapid.deliver.set_tooltip_text(Some(tip));
}

pub(super) fn refresh(state: &App) {
    if is_on(state) {
        rebuild(state);
    }
}

fn set_view(state: &App, on: bool) {
    let rapid = of(state);
    rapid.views.set_visible_child_name(if on { "rapid" } else { "grid" });
    if let Some(show) = state.libraries.show_filter.borrow().as_ref() {
        show();
    }
    state.catalog.remember("library-view", &on);
    if on {
        rebuild(state);
    } else if state.grid.stale.replace(false) {
        reload_grid(state);
    }
}

pub(super) fn library_of_view(state: &App) -> Option<i64> {
    if state.libraries.filter.borrow().spans_libraries() {
        return None;
    }
    state.libraries.current.borrow().as_ref().map(|library| library.id)
}

pub(super) fn kind_of_view(state: &App) -> Option<Kind> {
    library_of_view(state).and_then(|library| state.catalog.kind(library))
}

pub(super) fn kept_ids(state: &App, key: &str) -> Vec<i64> {
    library_of_view(state).and_then(|library| state.catalog.library_setting(library, key)).and_then(|json| serde_json::from_str(&json).ok()).unwrap_or_default()
}

pub(super) fn keep_ids(state: &App, key: &str, ids: &[i64]) {
    let Some(library) = library_of_view(state) else { return };
    let json = serde_json::to_string(ids).unwrap_or_default();
    if let Err(err) = state.catalog.set_library_setting(library, key, Some(&json)) {
        state.toast(&err);
    }
}

fn when(photo: &Photo) -> i64 {
    photo.taken.unwrap_or(photo.mtime)
}

fn starts(state: &App, photos: &[Photo]) -> Vec<usize> {
    let times: Vec<i64> = photos.iter().map(when).collect();
    let at = |id: i64| photos.iter().position(|photo| photo.id == id);
    let mut starts: Vec<usize> = kinds::moments(&times).into_iter().map(|run| run.start).collect();
    starts.extend(kept_ids(state, "moment-cuts").into_iter().filter_map(at));
    let joins: Vec<usize> = kept_ids(state, "moment-joins").into_iter().filter_map(at).collect();
    starts.retain(|start| *start == 0 || !joins.contains(start));
    starts.sort_unstable();
    starts.dedup();
    starts
}

fn looks_of(state: &App) -> HashMap<i64, (u64, u64)> {
    let Some(library) = library_of_view(state) else { return HashMap::new() };
    state.catalog.analysed(library).unwrap_or_default().into_iter().filter(|(_, frame, _)| !frame.is_blank()).map(|(id, frame, _)| (id, (frame.hash, frame.shape))).collect()
}

fn tiles_of(frames: &[Photo], looks: &HashMap<i64, (u64, u64)>) -> Vec<Tile> {

    let stacks = kinds::camera_bursts(
        &frames.iter().map(|photo| photo.taken).collect::<Vec<_>>(),
        &frames.iter().map(|photo| looks.get(&photo.id).map(|look| look.0)).collect::<Vec<_>>(),
    );
    let mut runs: Vec<Vec<&Photo>> = Vec::new();
    for (photo, stack) in frames.iter().zip(stacks) {
        match runs.get_mut(stack) {
            Some(run) => run.push(photo),
            None => runs.push(vec![photo]),
        }
    }

    let alone = |id: i64| Tile { id, members: vec![id], stacked: 1 };
    runs.into_iter()
        .flat_map(|run| {
            let members: Vec<i64> = run.iter().map(|photo| photo.id).collect();
            let rest: Vec<&Photo> = run.iter().copied().filter(|photo| photo.flag != Flag::Picked).collect();
            let top = rest.iter().max_by(|a, b| a.sharpness.unwrap_or(0.0).total_cmp(&b.sharpness.unwrap_or(0.0))).map(|photo| photo.id);
            let mut tiles = Vec::new();
            let mut stacked = false;
            for photo in &run {
                if photo.flag == Flag::Picked || rest.len() == 1 {
                    tiles.push(alone(photo.id));
                } else if !stacked {
                    stacked = true;
                    tiles.push(Tile { id: top.unwrap_or(photo.id), members: members.clone(), stacked: rest.len() });
                }
            }
            tiles
        })
        .collect()
}

pub(super) fn rebuild(state: &App) {
    let rapid = of(state);
    let mut photos: Vec<Photo> = {
        let cards = state.grid.cards.borrow();
        state.grid.order.borrow().iter().filter_map(|id| cards.get(id).cloned()).collect()
    };
    photos.sort_by_key(when);
    let starts = starts(state, &photos);
    let runs: Vec<&[Photo]> = starts
        .iter()
        .enumerate()
        .map(|(at, start)| &photos[*start..starts.get(at + 1).copied().unwrap_or(photos.len())])
        .filter(|frames| !frames.is_empty())
        .collect();
    let likeness = by_likeness(state);
    let looks = looks_of(state);
    rapid.following.set(true);
    rapid.by_likeness.set_active(likeness);
    rapid.by_time.set_active(!likeness);
    rapid.following.set(false);
    let moment = |frames: Vec<&Photo>, label: Option<String>| Moment {
        key: frames[0].id,
        ids: frames.iter().map(|photo| photo.id).collect(),
        times: frames.iter().map(|photo| when(photo)).collect(),
        tiles: tiles_of(&frames.iter().map(|photo| (*photo).clone()).collect::<Vec<_>>(), &looks),
        label,
    };
    let moments: Vec<Moment> = match likeness {
        false => runs.iter().map(|frames| moment(frames.iter().collect(), None)).collect(),

        true => {
            let of_photo: HashMap<i64, usize> = runs.iter().enumerate().flat_map(|(at, frames)| frames.iter().map(move |photo| (photo.id, at))).collect();
            let links: Vec<(usize, usize)> = photos.iter().filter_map(|photo| Some((*of_photo.get(&photo.id)?, *of_photo.get(&photo.echo?)?))).collect();
            kinds::alike(runs.len(), &links)
                .into_iter()
                .enumerate()
                .map(|(at, group)| {
                    let times: Vec<String> = group.iter().map(|run| clock(when(&runs[*run][0]))).collect();
                    let mut frames: Vec<&Photo> = group.iter().flat_map(|run| runs[*run].iter()).collect();
                    frames.sort_by_key(|photo| when(photo));
                    moment(frames, Some(format!("Setup {} · {}", at + 1, said_list(&times))))
                })
                .collect()
        }
    };
    *rapid.moments.borrow_mut() = moments;
    fill_list(state);
}

fn said_list(items: &[String]) -> String {
    match items {
        [] => String::new(),
        [one] => one.clone(),
        [rest @ .., last] => format!("{} and {last}", rest.join(", ")),
    }
}

pub(super) fn by_likeness(state: &App) -> bool {
    match library_of_view(state).and_then(|library| state.catalog.library_setting(library, "group-by")).as_deref() {
        Some("likeness") => true,
        Some(_) => false,
        None => Kind::by_likeness(kind_of_view(state)),
    }
}

pub(super) fn sizes(state: &App) -> (f32, f32) {
    state.catalog.recall(SIZES_KEY).unwrap_or(SIZES)
}

pub(super) fn set_sizes(state: &App, sizes: (f32, f32)) {
    state.catalog.remember(SIZES_KEY, &sizes);
    fill_list(state);
}

pub(super) fn fill_list(state: &App) {
    let rapid = of(state);
    while let Some(child) = rapid.list.first_child() {
        rapid.list.remove(&child);
    }
    rapid.asked.borrow_mut().clear();
    let seen = kept_ids(state, "moments-seen");
    let (mut all_tiles, mut counts) = (Vec::new(), Vec::new());

    let asking = library_of_view(state).is_some() && kind_of_view(state).is_none();
    rapid.deliver.set_sensitive(!asking);
    if asking {
        rapid.list.append(&rapid_panel::kind_question(state));
    } else if first_look(state) {
        let teaser = kept_ids(state, "teaser");
        let about = gtk::Label::new(Some(&format!(
            "One frame from each moment, as it will go out. Where one is off, set its moment on the right. P puts a frame in the teaser — {} so far.",
            teaser.len()
        )));
        about.add_css_class("dim-label");
        about.set_wrap(true);
        about.set_xalign(0.0);
        rapid.list.append(&about);
        let (height, gap) = sizes(state);
        let rows = rapid_rows::Rows::new(height * 1.5, gap);
        rows.add_css_class("photo-rows");
        for (m, moment) in rapid.moments.borrow().iter().enumerate() {
            let id = moment.face();
            let tile = tile(state, id, None, None);
            if teaser.contains(&id) {
                tile.add_overlay(&pill("Teaser", gtk::Align::Start, gtk::Align::Start));
            }
            clicks(state, &tile, m, 0, id);

            let label = pill(&clock(moment.start()), gtk::Align::End, gtk::Align::Start);
            tile.add_overlay(&label);
            rows.append(&tile, aspect_of(state, id));
            all_tiles.push(vec![vec![tile]]);
            counts.push(label);
        }
        rapid.list.append(&rows);
    } else {

        let chapters = match by_likeness(state) {
            true => Vec::new(),
            false => rapid_chapters::of_moments(state),
        };
        let mut headings = Vec::new();
        let keys = rapid_layers::key_ids(state);
        for (m, moment) in rapid.moments.borrow().iter().enumerate() {
            let chapter = chapters.get(m).cloned().flatten();
            if let Some(offer) = rapid_chapters::offer(state, m, chapter.as_ref().map(|(name, _)| name.as_str())).filter(|_| !chapter.as_ref().is_some_and(|(_, starts)| *starts)) {
                rapid.list.append(&offer);
            }
            if let Some((name, true)) = &chapter {
                let (count, about) = chapter_about(state, &chapters, m);
                let heading = rapid_chapters::heading(state, m, name, &about);
                rapid.list.append(&heading);
                headings.push((name.clone(), heading.upcast::<gtk::Widget>(), count));
            }
            let starts = chapter.as_ref().is_some_and(|(_, starts)| *starts);
            let (section, tiles, count) = section(state, m, moment, seen.contains(&moment.key), (!by_likeness(state)).then_some(starts), &keys);
            rapid.list.append(&section);
            all_tiles.push(tiles);
            counts.push(count);
        }
        let state_ = state.clone();
        glib::idle_add_local_once(move || rapid_chapters::fill_bar(&state_, &headings));
    }
    if asking || first_look(state) {
        rapid_chapters::fill_bar(state, &[]);
    }
    let count = all_tiles.len();
    *rapid.tiles.borrow_mut() = all_tiles;
    *rapid.counts.borrow_mut() = counts;

    let kept = chosen(state).filter(|m| !asking && !first_look(state) && *m < count);
    if kept.is_none() {
        rapid.chosen.take();
    }
    show_panel(state, kept);
    let (m, t) = rapid.at.get();
    focus(state, m.min(count.saturating_sub(1)), t);
    sweep_soon(state);
}

pub(super) fn sweep_soon(state: &App) {
    let rapid = of(state);
    let generation = rapid.sweeping.get().wrapping_add(1);
    rapid.sweeping.set(generation);
    let state = state.clone();
    glib::timeout_add_local_once(std::time::Duration::from_millis(60), move || {
        if of(&state).sweeping.get() == generation {
            sweep(&state);
        }
    });
}

fn sweep(state: &App) {
    let rapid = of(state);
    let adjustment = rapid.scroller.vadjustment();
    let (top, page) = (adjustment.value(), adjustment.page_size().max(1.0));
    let near = (top - page)..=(top + 2.0 * page);
    let kept = (top - 3.0 * page)..=(top + 4.0 * page);
    let edge = grid_edge(state);
    let tiles = rapid.tiles.borrow();
    for tile in tiles.iter().flatten().flatten() {
        let Some(at) = tile.compute_point(&rapid.list, &gtk::graphene::Point::zero()) else { continue };
        let (from, to) = (at.y() as f64, at.y() as f64 + tile.height() as f64);
        let within = |band: &std::ops::RangeInclusive<f64>| to >= *band.start() && from <= *band.end();
        for picture in pictures_of(tile.upcast_ref()) {
            let Ok(id) = picture.widget_name().parse::<i64>() else { continue };
            if within(&near) {
                let photo = rapid.asked.borrow_mut().insert(id).then(|| state.grid.cards.borrow().get(&id).cloned()).flatten();
                if let Some(photo) = photo {
                    load(state, &photo, &picture, edge);
                }
            } else if !within(&kept) && rapid.asked.borrow_mut().remove(&id) {
                picture.set_paintable(gtk::gdk::Paintable::NONE);
            }
        }
    }
}

fn choose(state: &App, m: usize, section: &gtk::Box) {
    let rapid = of(state);
    let was = rapid.chosen.borrow_mut().take();
    if let Some((_, old)) = &was {
        old.remove_css_class("chosen");
    }
    if was.is_some_and(|(at, _)| at == m) {
        show_panel(state, None);
        return;
    }
    section.add_css_class("chosen");
    rapid.chosen.replace(Some((m, section.clone())));
    show_panel(state, Some(m));

    if of(state).at.get().0 != m {
        focus(state, m, 0);
    }
}

pub(super) fn let_go(state: &App) {
    if let Some((_, section)) = of(state).chosen.take() {
        section.remove_css_class("chosen");
    }
    show_panel(state, None);
}

pub(super) fn chosen(state: &App) -> Option<usize> {
    of(state).chosen.borrow().as_ref().map(|(m, _)| *m)
}

fn show_panel(state: &App, m: Option<usize>) {
    let rapid = of(state);
    if let Some(side) = rapid.panel.ancestor(gtk::ScrolledWindow::static_type()) {
        side.set_visible(m.is_some());
    }
    match m {
        Some(m) => rapid_panel::fill(state, m),
        None => rapid_panel::clear(state),
    }
}

fn chapter_about(state: &App, chapters: &[Option<(String, bool)>], m: usize) -> (usize, String) {
    let moments = of(state).moments.borrow();
    let end = (m + 1..moments.len()).find(|at| chapters.get(*at).cloned().flatten().is_some_and(|(_, starts)| starts)).unwrap_or(moments.len());
    let ids: Vec<i64> = moments[m..end].iter().flat_map(|moment| moment.ids.iter().copied()).collect();
    let moments_said = match end - m {
        1 => "1 moment".to_string(),
        n => format!("{n} moments"),
    };
    (ids.len(), format!("{moments_said} · {} · {} picked", photos(ids.len()), picked(state, &ids).len()))
}

pub(super) fn clock(at: i64) -> String {
    glib::DateTime::from_unix_local(at).ok().and_then(|time| time.format("%H:%M").ok()).map(|text| text.to_string()).unwrap_or_default()
}

pub(super) fn photos(count: usize) -> String {
    match count {
        1 => "1 photo".to_string(),
        n => format!("{} photos", places::grouped(n as i64)),
    }
}

pub(super) fn picked(state: &App, ids: &[i64]) -> Vec<i64> {
    let cards = state.grid.cards.borrow();
    ids.iter().copied().filter(|id| cards.get(id).is_some_and(|photo| photo.flag == Flag::Picked)).collect()
}

pub(super) fn counted(state: &App, moment: &Moment) -> String {
    format!("{} · {} picked", photos(moment.ids.len()), picked(state, &moment.ids).len())
}

pub(super) fn aspect_of(state: &App, id: i64) -> f32 {
    state.grid.cards.borrow().get(&id).and_then(|photo| photo.aspect).unwrap_or(1.5)
}

fn clicks(state: &App, tile: &gtk::Overlay, m: usize, t: usize, id: i64) {
    let click = gtk::GestureClick::new();
    click.connect_pressed(glib::clone!(
        #[strong] state,
        move |_, presses, _, _| {
            focus(&state, m, t);
            if presses == 2 {
                open_photo(&state, id);
            }
        }
    ));
    tile.add_controller(click);
    let menu = gtk::GestureClick::new();
    menu.set_button(gtk::gdk::BUTTON_SECONDARY);
    menu.connect_pressed(glib::clone!(
        #[strong] state,
        #[weak] tile,
        move |_, _, x, y| {
            focus(&state, m, t);
            photo_menu(&state, &tile, m, id, (x, y));
        }
    ));
    tile.add_controller(menu);
}

fn photo_menu(state: &App, tile: &gtk::Overlay, m: usize, id: i64, (x, y): (f64, f64)) {
    let apart = rapid_layers::read(state, m).is_some_and(|(_, layers)| layers.apart.contains(&id));
    let actions = gio::SimpleActionGroup::new();
    let matching = gio::SimpleAction::new("match", None);
    matching.connect_activate(glib::clone!(
        #[strong] state,
        move |_, _| rapid_layers::match_to(&state, m)
    ));
    let detach = gio::SimpleAction::new("detach", None);
    detach.connect_activate(glib::clone!(
        #[strong] state,
        move |_, _| rapid_layers::detach_or_join(&state, m, id, !apart)
    ));
    actions.add_action(&matching);
    actions.add_action(&detach);
    tile.insert_action_group("photo", Some(&actions));
    let model = gio::Menu::new();
    model.append(Some("Match the Moment to This Photo"), Some("photo.match"));
    model.append(Some(if apart { "Join the Moment" } else { "Detach from Moment" }), Some("photo.detach"));
    let popover = gtk::PopoverMenu::from_model(Some(&model));
    popover.set_parent(tile);
    popover.set_has_arrow(false);
    popover.set_pointing_to(Some(&gtk::gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
    popover.connect_closed(|popover| {
        let popover = popover.clone();
        glib::idle_add_local_once(move || popover.unparent());
    });
    popover.popup();
}

fn section(state: &App, m: usize, moment: &Moment, seen: bool, chapter: Option<bool>, keys: &[i64]) -> (gtk::Box, Vec<Vec<gtk::Overlay>>, gtk::Label) {
    let column = gtk::Box::new(gtk::Orientation::Vertical, 8);
    column.add_css_class("rapid-moment");
    if let Some((_, section)) = of(state).chosen.borrow_mut().as_mut().filter(|(at, _)| *at == m) {
        column.add_css_class("chosen");
        *section = column.clone();
    }
    let head = gtk::Box::new(gtk::Orientation::Horizontal, 10);

    let handle = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    handle.add_css_class("rapid-handle");
    handle.set_cursor_from_name(Some("pointer"));
    handle.set_tooltip_text(Some("This moment's light and look, for all of its photographs — in the panel"));
    let name = moment.label.clone().unwrap_or_else(|| format!("{} – {}", clock(moment.start()), clock(moment.end())));
    let time = gtk::Label::new(Some(&name));
    time.add_css_class("heading");
    let about = gtk::Label::new(Some(&counted(state, moment)));
    about.add_css_class("dim-label");
    handle.append(&time);
    handle.append(&about);
    if seen {
        let mark = gtk::Label::new(Some("Seen"));
        mark.add_css_class("dim-label");
        mark.add_css_class("caption");
        handle.append(&mark);
    }
    let click = gtk::GestureClick::new();
    click.connect_released(glib::clone!(
        #[strong] state,
        #[weak] column,
        move |_, _, _, _| choose(&state, m, &column)
    ));
    handle.add_controller(click);
    head.append(&handle);
    if let Some(starts) = chapter {
        let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        spacer.set_hexpand(true);
        head.append(&spacer);
        head.append(&rapid_chapters::start_button(state, m, starts));
    }
    column.append(&head);
    let (height, gap) = sizes(state);
    let rows = rapid_rows::Rows::new(height, gap);
    rows.add_css_class("photo-rows");
    let mut tiles = Vec::new();
    for (t, entry) in moment.tiles.iter().enumerate() {
        let tile = tile(state, entry.id, entry.is_stack().then_some(entry.stacked), None);

        let key = keys.contains(&entry.id);
        if key && !entry.is_stack() {
            tile.add_overlay(&pill("Key Frame", gtk::Align::Start, gtk::Align::Start));
        }
        if entry.is_stack() {
            stack(state, &tile, entry, m, t, key);
        } else {
            clicks(state, &tile, m, t, entry.id);
        }
        rows.append(&tile, aspect_of(state, entry.id));
        tiles.push(vec![tile]);
    }
    column.append(&rows);
    (column, tiles, about)
}

fn stack(state: &App, tile: &gtk::Overlay, entry: &Tile, m: usize, t: usize, key: bool) {
    tile.add_css_class("rapid-stack");

    if key {
        tile.add_overlay(&pill("Key Frame", gtk::Align::Start, gtk::Align::Start));
    }
    tile.set_tooltip_text(Some(&format!("{} of the same picture — a click shows them side by side; P picks the frames Numa would keep", entry.members.len())));

    let members = entry.members.clone();
    let click = gtk::GestureClick::new();
    click.connect_pressed(glib::clone!(
        #[strong] state,
        move |_, _, _, _| {
            focus(&state, m, t);
            loupe::cull_only(&state, &members);
        }
    ));
    tile.add_controller(click);
}

pub(super) fn keep_count(state: &App) -> usize {
    library_of_view(state).and_then(|library| state.catalog.library_setting(library, "keep-per-burst")).and_then(|count| count.parse().ok()).unwrap_or_else(|| Kind::keep(kind_of_view(state)))
}

pub(super) fn numa_keeps(state: &App, members: &[i64]) -> Vec<i64> {
    let scores: Vec<Option<f32>> = {
        let cards = state.grid.cards.borrow();
        members
            .iter()
            .map(|id| {
                let photo = cards.get(id)?;
                let out = photo.eyes_closed == Some(true) || photo.brightness.is_some_and(|brightness| brightness < 0.02);
                (!out).then(|| photo.sharpness.unwrap_or(0.5) + photo.suggested.unwrap_or(2.5) / 5.0)
            })
            .collect()
    };

    let keep = keep_count(state).min(members.len().saturating_sub(1).max(1));
    kinds::keep_few(&scores, keep).into_iter().map(|at| members[at]).collect()
}

fn pictures_of(widget: &gtk::Widget) -> Vec<gtk::Picture> {
    let mut found = Vec::new();
    let mut child = widget.first_child();
    while let Some(next) = child {
        match next.downcast_ref::<gtk::Picture>() {
            Some(picture) => found.push(picture.clone()),
            None => found.extend(pictures_of(&next)),
        }
        child = next.next_sibling();
    }
    found
}

pub(super) fn tile(state: &App, id: i64, burst: Option<usize>, size: Option<(i32, i32)>) -> gtk::Overlay {
    let card = make_card(state);
    if let Some((width, height)) = size {
        card.set_size_request(width, height);
    }
    if let Some(photo) = state.grid.cards.borrow().get(&id).cloned() {
        mark_photo_card(state, &card, &quiet(state, &photo), burst);
        if let Some(picture) = card_picture(&card) {
            picture.set_widget_name(&id.to_string());

            if size.is_some() {
                load(state, &photo, &picture, grid_edge(state));
            }
        }
    }
    card.downcast().expect("a card is an overlay")
}

fn quiet(state: &App, photo: &Photo) -> Photo {
    let mut quiet = photo.clone();
    quiet.suggested = None;

    let untouched = |before: Document| serde_json::to_string(&before).ok() == serde_json::to_string(&Document::new(before.source.path.clone())).ok();
    let only_moment = || {
        state.catalog.own_edits(photo.id).ok().flatten().is_some_and(|mut own| {
            own.moment = None;
            own.is_untouched()
        })
    };
    if quiet.edited && (only_moment() || state.catalog.numa_did(photo.id).is_some() && state.catalog.before_numa(photo.id).is_some_and(untouched)) {
        quiet.edited = false;
    }
    quiet
}

pub(super) fn pill(text: &str, x: gtk::Align, y: gtk::Align) -> gtk::Label {
    let label = gtk::Label::new(Some(text));
    label.add_css_class("card-badge");
    label.set_halign(x);
    label.set_valign(y);
    label.set_can_target(false);

    label.set_ellipsize(gtk::pango::EllipsizeMode::End);
    label
}

pub(super) fn load(state: &App, photo: &Photo, picture: &gtk::Picture, edge: u32) {
    let edits = state.catalog.edits_json(photo.id).ok().flatten();
    let alive = picture.downgrade();
    let target = picture.downgrade();
    let clipping = of(state).clipping.is_active();
    thumbnail::load_thumbnail_while(&photo.path, photo.mtime, edge, edits, None, move || alive.upgrade().is_some(), move |texture| {
        if let Some(picture) = target.upgrade() {
            match clipping {
                true => picture.set_paintable(Some(&clipped(&texture))),
                false => picture.set_paintable(Some(&texture)),
            }
            reshape(&picture, texture.width() as f32 / texture.height().max(1) as f32);
        }
    });
}

fn reshape(picture: &gtk::Picture, aspect: f32) {
    let mut widget: gtk::Widget = picture.clone().upcast();
    while let Some(parent) = widget.parent() {
        if let Some(rows) = parent.downcast_ref::<rapid_rows::Rows>() {
            rows.set_aspect(&widget, aspect);
            return;
        }
        widget = parent;
    }
}

fn clipped(texture: &gtk::gdk::Texture) -> gtk::gdk::Texture {
    let mut downloader = gtk::gdk::TextureDownloader::new(texture);
    downloader.set_format(gtk::gdk::MemoryFormat::R8g8b8a8);
    let (bytes, stride) = downloader.download_bytes();
    let (width, height) = (texture.width() as usize, texture.height() as usize);
    let mut pixels = bytes.to_vec();
    for row in pixels.chunks_mut(stride).take(height) {
        for pixel in row[..width * 4].chunks_exact_mut(4) {
            let top = pixel[0].max(pixel[1]).max(pixel[2]);
            if top >= 254 {
                pixel[..3].copy_from_slice(&[255, 64, 64]);
            } else if top <= 1 {
                pixel[..3].copy_from_slice(&[40, 88, 255]);
            }
        }
    }
    gtk::gdk::MemoryTexture::new(width as i32, height as i32, gtk::gdk::MemoryFormat::R8g8b8a8, &glib::Bytes::from_owned(pixels), stride).upcast()
}

pub(super) fn focus(state: &App, m: usize, t: usize) {
    let rapid = of(state);
    let (old_m, old_t) = rapid.at.get();
    {
        let tiles = rapid.tiles.borrow();

        for tile in tiles.get(old_m).and_then(|row| row.get(old_t)).into_iter().flatten() {
            tile.unset_state_flags(gtk::StateFlags::SELECTED);
        }
        let Some(row) = tiles.get(m) else { return };
        let t = t.min(row.len().saturating_sub(1));
        rapid.at.set((m, t));
        for tile in row.get(t).into_iter().flatten() {
            tile.set_state_flags(gtk::StateFlags::SELECTED, false);
        }
        reveal(state);
    }
    if old_m != m {

        if let Some(key) = rapid.moments.borrow().get(old_m).map(|moment| moment.key) {
            let mut seen = kept_ids(state, "moments-seen");
            if !seen.contains(&key) {
                seen.push(key);
                keep_ids(state, "moments-seen", &seen);
            }
        }
    }
}

fn reveal(state: &App) {
    let state = state.clone();
    glib::timeout_add_local_once(std::time::Duration::from_millis(60), move || {
        let rapid = of(&state);
        let (m, t) = rapid.at.get();
        let Some(tile) = rapid.tiles.borrow().get(m).and_then(|row| row.get(t)).and_then(|frames| frames.first().cloned()) else { return };
        let Some(at) = tile.compute_point(&rapid.list, &gtk::graphene::Point::zero()) else { return };
        let adjustment = rapid.scroller.vadjustment();
        let (top, bottom) = (at.y() as f64, at.y() as f64 + tile.height() as f64);
        if top < adjustment.value() + 48.0 || bottom > adjustment.value() + adjustment.page_size() {
            glide(&rapid.scroller, &adjustment, (top - 120.0).max(0.0));
        }
    });
}

pub(super) fn keys_to(state: &App, id: i64) {
    let at = of(state).moments.borrow().iter().enumerate().find_map(|(m, moment)| moment.tiles.iter().position(|tile| tile.members.contains(&id)).map(|t| (m, t)));
    if let Some((m, t)) = at {
        focus(state, m, t);
    }
}

pub(super) fn focused(state: &App) -> Option<i64> {
    let rapid = of(state);
    let (m, t) = rapid.at.get();
    let moments = rapid.moments.borrow();
    let moment = moments.get(m)?;
    match first_look(state) {
        true => Some(moment.face()),
        false => moment.tiles.get(t).map(|tile| tile.id),
    }
}

pub(super) fn own_of(state: &App, id: i64) -> Option<Document> {
    let path = state.grid.cards.borrow().get(&id).map(|photo| photo.path.to_string_lossy().to_string())?;
    Some(state.catalog.own_edits(id).ok().flatten().unwrap_or_else(|| Document::new(path)))
}

pub(super) fn repaint_moment(state: &App, m: usize) {
    let rapid = of(state);
    let Some(ids) = rapid.moments.borrow().get(m).map(|moment| moment.ids.clone()) else { return };
    for id in &ids {
        if let Some(photo) = state.grid.cards.borrow_mut().get_mut(id) {
            photo.edited = true;
        }
        refresh_thumbnail(state, *id, true);
    }
    state.grid.stale.set(true);
    let tiles = rapid.tiles.borrow();
    let Some(row) = tiles.get(m) else { return };
    for picture in row.iter().flatten().flat_map(|tile| pictures_of(tile.upcast_ref())) {
        let photo = picture.widget_name().parse::<i64>().ok().and_then(|id| state.grid.cards.borrow().get(&id).cloned());
        if let Some(photo) = photo {
            rapid.asked.borrow_mut().insert(photo.id);
            load(state, &photo, &picture, grid_edge(state));
        }
    }
}

pub(super) fn set_flags(state: &App, ids: &[i64], flag: Flag) {
    for id in ids {
        if let Err(err) = state.catalog.set_flag(*id, flag) {
            state.toast(&err);
            return;
        }
        if let Some(photo) = state.grid.cards.borrow_mut().get_mut(id) {
            photo.flag = flag;
        }
    }
    state.grid.stale.set(true);
}

fn toggle_teaser(state: &App, id: i64) {
    let mut teaser = kept_ids(state, "teaser");
    match teaser.iter().position(|kept| *kept == id) {
        Some(at) => {
            teaser.remove(at);
        }
        None => teaser.push(id),
    }
    keep_ids(state, "teaser", &teaser);
    let (m, _) = of(state).at.get();
    fill_list(state);
    focus(state, m + 1, 0);
}

fn mark(state: &App, flag: Option<Flag>, rating: Option<u8>) {
    let Some(id) = focused(state) else { return };
    if first_look(state) && flag == Some(Flag::Picked) {
        toggle_teaser(state, id);
        return;
    }
    let rapid = of(state);
    let (m, t) = rapid.at.get();

    let stack = rapid.moments.borrow().get(m).and_then(|moment| moment.tiles.get(t)).filter(|tile| tile.is_stack()).map(|tile| tile.members.clone());
    if let (Some(Flag::Picked), Some(members)) = (flag, &stack) {
        set_flags(state, &numa_keeps(state, members), Flag::Picked);
        rebuild(state);
        step(state, 1);
        return;
    }
    match (flag, rating) {
        (Some(flag), _) => set_flags(state, &[id], flag),
        (_, Some(rating)) => {
            if let Err(err) = state.catalog.set_rating(id, rating) {
                state.toast(&err);
                return;
            }
            if let Some(photo) = state.grid.cards.borrow_mut().get_mut(&id) {
                photo.rating = rating;
            }
            state.grid.stale.set(true);
        }
        _ => return,
    }

    let photo = state.grid.cards.borrow().get(&id).cloned();
    if flag.is_some() && photo.as_ref().is_some_and(|photo| photo.burst.is_some()) {
        rebuild(state);
        step(state, 1);
        return;
    }

    let stacked = rapid.moments.borrow().get(m).and_then(|moment| moment.tiles.get(t)).filter(|tile| tile.is_stack()).map(|tile| tile.stacked);
    for tile in rapid.tiles.borrow().get(m).and_then(|row| row.get(t)).into_iter().flatten() {
        if let Some(photo) = &photo {
            mark_photo_card(state, tile.upcast_ref(), &quiet(state, photo), stacked);
        }
    }
    if flag.is_none() {
        return;
    }
    if !first_look(state) {
        if let (Some(label), Some(moment)) = (rapid.counts.borrow().get(m), rapid.moments.borrow().get(m)) {
            label.set_text(&counted(state, moment));
        }
    }
    if chosen(state) == Some(m) {
        rapid_panel::fill(state, m);
    }
    step(state, 1);
}

pub(super) fn step(state: &App, by: isize) {
    let rapid = of(state);
    let (m, t) = rapid.at.get();
    let sizes: Vec<usize> = rapid.tiles.borrow().iter().map(Vec::len).collect();
    let (mut m, mut t) = (m as isize, t as isize + by);
    while m >= 0 && (m as usize) < sizes.len() && (t < 0 || t >= sizes[m as usize] as isize) {
        if t < 0 {
            m -= 1;
            t = if m >= 0 { sizes[m as usize] as isize - 1 } else { 0 };
        } else {
            t -= sizes[m as usize] as isize;
            m += 1;
        }
    }
    if m >= 0 && (m as usize) < sizes.len() {
        focus(state, m as usize, t.max(0) as usize);
    }
}

pub(super) fn next_moment(state: &App) {
    let rapid = of(state);
    let (m, _) = rapid.at.get();
    let seen = kept_ids(state, "moments-seen");
    let keys: Vec<i64> = rapid.moments.borrow().iter().map(|moment| moment.key).collect();
    match (m + 1..keys.len()).chain(0..m).find(|at| !seen.contains(&keys[*at])) {
        Some(at) => focus(state, at, 0),
        None => state.toast("Every moment seen — Deliver… when it is ready"),
    }
}

fn cut(state: &App, split: bool) {
    let rapid = of(state);
    let (m, t) = rapid.at.get();
    let Some((key, first)) = rapid.moments.borrow().get(m).map(|moment| (moment.key, moment.tiles.get(t).map(|tile| tile.members[0]))) else { return };
    let (mut cuts, mut joins) = (kept_ids(state, "moment-cuts"), kept_ids(state, "moment-joins"));
    if split {
        let Some(id) = first.filter(|id| *id != key) else {
            state.toast("The moment starts here already");
            return;
        };
        cuts.push(id);
        joins.retain(|join| *join != id);
        rapid.at.set((m + 1, 0));
    } else {
        if m == 0 {
            state.toast("There is no moment before this one");
            return;
        }
        joins.push(key);
        cuts.retain(|cut| *cut != key);
        rapid.at.set((m - 1, 0));
    }
    keep_ids(state, "moment-cuts", &cuts);
    keep_ids(state, "moment-joins", &joins);
    rebuild(state);
}

fn typing(state: &App) -> bool {
    state.stack.root().and_then(|root| root.focus()).is_some_and(|focus| focus.is::<gtk::Editable>() || focus.is::<gtk::Text>())
}

fn key(state: &App, key: gtk::gdk::Key, modifiers: gtk::gdk::ModifierType) -> glib::Propagation {
    use gtk::gdk::Key;
    let on_library = state.stack.visible_child_name().as_deref() == Some("library");

    let modified = !modifiers.difference(gtk::gdk::ModifierType::LOCK_MASK).is_empty();
    if !on_library || state.loupe.reveal.reveals_child() || modified || typing(state) {
        return glib::Propagation::Proceed;
    }
    let page = of(state).views.visible_child_name();
    if matches!(key, Key::j | Key::J) && matches!(page.as_deref(), Some("rapid" | "check")) {
        let clipping = &of(state).clipping;
        clipping.set_active(!clipping.is_active());
        return glib::Propagation::Stop;
    }
    match page.as_deref() {
        Some("check") => return rapid_pages::check_key(state, key),
        Some("rapid") => {}
        _ => return glib::Propagation::Proceed,
    }
    match key {
        Key::Right => step(state, 1),
        Key::Left => step(state, -1),
        Key::Down | Key::Up => {
            let (m, _) = of(state).at.get();
            let to = if key == Key::Down { m + 1 } else { m.saturating_sub(1) };
            focus(state, to, 0);
        }
        Key::Page_Down => next_moment(state),
        Key::p | Key::P => mark(state, Some(Flag::Picked), None),
        Key::x | Key::X => mark(state, Some(Flag::Rejected), None),
        Key::u | Key::U => mark(state, Some(Flag::None), None),
        Key::s | Key::S if !first_look(state) => cut(state, true),
        Key::m | Key::M if !first_look(state) => cut(state, false),
        Key::n | Key::N => rapid_panel::note(state),
        Key::e | Key::E => {
            if let Some(id) = focused(state) {
                open_photo(state, id);
            }
        }
        Key::Return | Key::KP_Enter => {
            let rapid = of(state);
            let (m, t) = rapid.at.get();
            let burst = !first_look(state) && rapid.moments.borrow().get(m).and_then(|moment| moment.tiles.get(t)).is_some_and(Tile::is_stack);
            let members = rapid.moments.borrow().get(m).and_then(|moment| moment.tiles.get(t)).map(|tile| tile.members.clone());
            match (burst, members, focused(state)) {
                (true, Some(members), _) => loupe::cull_only(state, &members),
                (false, _, Some(id)) => open_photo(state, id),
                _ => {}
            }
        }

        Key::space => {
            let rapid = of(state);
            let (m, t) = rapid.at.get();
            let stack = rapid.moments.borrow().get(m).and_then(|moment| moment.tiles.get(t)).filter(|tile| tile.is_stack() && !first_look(state)).map(|tile| tile.members.clone());
            match (stack, focused(state)) {
                (Some(members), _) => loupe::cull_only(state, &members),
                (None, Some(id)) => {
                    loupe::show_in_loupe(state, id);
                }
                _ => {}
            }
        }
        Key::Escape if chosen(state).is_some() => let_go(state),
        _ => match key.to_unicode().and_then(|c| c.to_digit(10)).filter(|d| *d <= 5) {
            Some(stars) => mark(state, None, Some(stars as u8)),
            None => return glib::Propagation::Proceed,
        },
    }
    glib::Propagation::Stop
}

pub(super) fn install(state: &App, window: &adw::ApplicationWindow) {
    let shown = state.catalog.recall::<bool>("library-view").unwrap_or(false);
    let view = gio::SimpleAction::new_stateful("library-view", Some(glib::VariantTy::STRING), &(if shown { "rapid" } else { "grid" }).to_variant());
    view.connect_activate(glib::clone!(
        #[strong] state,
        move |action, target| {
            let Some(target) = target.and_then(|target| target.get::<String>()) else { return };
            action.set_state(&target.to_variant());
            set_view(&state, target == "rapid");
        }
    ));
    window.add_action(&view);
    if shown {
        of(state).views.set_visible_child_name("rapid");
    }
    rapid_numa::install(state, window);

    let keys = gtk::EventControllerKey::new();
    keys.set_propagation_phase(gtk::PropagationPhase::Capture);
    keys.connect_key_pressed(glib::clone!(
        #[strong] state,
        move |_, pressed, _, modifiers| key(&state, pressed, modifiers)
    ));
    window.add_controller(keys);

    state.stack.connect_visible_child_name_notify(glib::clone!(
        #[strong] state,
        move |stack| {
            if stack.visible_child_name().as_deref() != Some("library") {
                return;
            }
            match of(&state).views.visible_child_name().as_deref() {
                Some("rapid") => rebuild(&state),
                Some("check") => rapid_pages::show_check(&state),
                _ => {}
            }
        }
    ));
}
