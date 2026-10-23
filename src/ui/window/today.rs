use super::*;

const NEWS: i64 = 14 * 86_400;

const YEARS: usize = 6;

const ARRIVALS: usize = 3;
const STRIP: usize = 6;

const STRIP_HEIGHT: i32 = 96;

const TILE: f32 = 132.0;

const FRESH: std::time::Duration = std::time::Duration::from_secs(10 * 60);

thread_local! {

    static FILLED: Cell<u64> = const { Cell::new(0) };

    static FACES: Cell<Option<(std::time::Instant, usize)>> = const { Cell::new(None) };
}

pub(super) fn show(state: &App) {
    fill(state);
    state.stack.set_visible_child_name("today");
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
}

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
    let now = glib::DateTime::now_local().ok();

    let date = match (std::env::var("NUMA_DAY").ok(), &now) {
        (Some(day), Some(now)) if day.len() == 5 => format!("{}-{day}", now.year()),
        (_, Some(now)) => format!("{}-{:02}-{:02}", now.year(), now.month(), now.day_of_month()),
        (_, None) => return,
    };
    column.append(&head(&date));

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
                .map(|photo| Shot { id: photo.id, when: photo.taken.unwrap_or(photo.mtime), path: photo.path, mtime: photo.mtime, rating: photo.rating, flag: photo.flag })
                .collect();

            shots.sort_by_key(|shot| std::cmp::Reverse(shot.when));
            (library, shots)
        })
        .collect();
    let mut shown = false;
    let years = numa::io::import::on_this_day(shelves.iter().flat_map(|(_, shots)| shots.iter().map(|shot| (shot.id, shot.when, shot.rating, shot.flag))), &date, YEARS);
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
        #[weak] column,
        #[strong] left,
        #[strong] shown,
        move || {
            left.set(left.get() - 1);
            if left.get() == 0 && !shown.get() && FILLED.get() == filled {

                let empty = adw::StatusPage::new();
                empty.set_description(Some("Photographs show here as they come in."));
                column.append(&empty);
            }
        }
    ));
    just_arrived(state, shelves, arrived, filled, shown.clone(), settle.clone());
    waiting_for_you(state, waiting, filled, shown, settle);
}

fn head(date: &str) -> gtk::Box {
    const DAYS: [&str; 7] = ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"];
    const MONTHS: [&str; 12] =
        ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
    let mut parts = date.split('-').map(|part| part.parse::<i32>().unwrap_or(1));
    let (year, month, day) = (parts.next().unwrap_or(2026), parts.next().unwrap_or(1), parts.next().unwrap_or(1));
    let said = glib::DateTime::from_local(year, month, day, 12, 0, 0.0)
        .map(|at| format!("{} {day} {}", DAYS[(at.day_of_week() as usize).clamp(1, 7) - 1], MONTHS[(month as usize).clamp(1, 12) - 1]))
        .unwrap_or_default();
    let head = gtk::Box::new(gtk::Orientation::Vertical, 4);
    let title = gtk::Label::new(Some("Today"));
    title.set_xalign(0.0);
    title.add_css_class("title-2");
    let caption = gtk::Label::new(Some(&said));
    caption.set_xalign(0.0);
    caption.add_css_class("caption");
    caption.add_css_class("dim-label");
    head.append(&title);
    head.append(&caption);
    head
}

fn part(label: &str, aside: Option<&str>) -> gtk::Box {
    let part = gtk::Box::new(gtk::Orientation::Vertical, 12);
    part.set_margin_top(24);
    let line = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let title = section_header(label);
    title.set_margin_top(0);
    line.append(&title);
    if let Some(aside) = aside {
        let caption = gtk::Label::new(Some(aside));
        caption.add_css_class("caption");
        caption.add_css_class("dim-label");
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

fn waiting_for_you(state: &App, into: gtk::Box, filled: u64, shown: Rc<Cell<bool>>, settle: Rc<dyn Fn()>) {
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
        let waits: Vec<gtk::Box> = [
            (sets, "set to choose from", "sets to choose from", "Tidy Up", "Choose", true),
            (faces, "face to name", "faces to name", "People \u{b7} not named yet", "Name", false),
        ]
        .into_iter()
        .filter(|(count, ..)| *count > 0)
        .map(|(count, one, more, caption, label, tidy)| {
            let go: Rc<dyn Fn(&App)> = match tidy {
                true => Rc::new(tidy::show),
                false => Rc::new(show_people),
            };
            wait(&state, count, if count == 1 { one } else { more }, caption, label, go)
        })
        .collect();
        if !waits.is_empty() {
            shown.set(true);
            let part = part("Waiting for You", None);
            waits.iter().for_each(|line| part.append(line));
            into.append(&part);
        }
        settle();
    });
}

fn wait(state: &App, count: usize, what: &str, caption: &str, label: &str, go: Rc<dyn Fn(&App)>) -> gtk::Box {
    let line = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    let words = gtk::Box::new(gtk::Orientation::Vertical, 0);
    words.set_hexpand(true);
    let name = gtk::Label::new(Some(&format!("{} {what}", places::grouped(count as i64))));
    name.set_xalign(0.0);
    name.add_css_class("heading");
    let under = gtk::Label::new(Some(caption));
    under.set_xalign(0.0);
    under.add_css_class("caption");
    under.add_css_class("dim-label");
    words.append(&name);
    words.append(&under);
    let button = gtk::Button::with_label(label);
    button.set_valign(gtk::Align::Center);
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
