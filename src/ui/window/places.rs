use super::*;
use gtk::graphene;
use gtk::gsk;
use numa::io::catalog::Glance;

pub(super) const TILE: f32 = 36.0;

const FAN: [(f32, f32); 3] = [(-8.0, -7.0), (6.0, 4.5), (0.0, 0.0)];

pub(super) const FAN_SIZE: usize = FAN.len();

const MARGIN: f32 = 10.0;

fn card_edge(tile: f32) -> u32 {
    tile as u32 * 2
}

const GROUP: char = '\u{2009}';

thread_local! {

    static GLANCES: RefCell<(i64, HashMap<String, Rc<Row>>)> = RefCell::new((0, HashMap::new()));

    static SHELVES: RefCell<(i64, Rc<HashMap<i64, Glance>>)> = RefCell::new((0, Rc::new(HashMap::new())));

    static TEXTURES: RefCell<HashMap<(PathBuf, u32), gtk::gdk::Texture>> = RefCell::new(HashMap::new());
}

#[derive(Default)]
pub(super) struct Row {
    pub(super) subtitle: String,

    pub(super) count: i64,
    pub(super) first: Option<i64>,
    pub(super) last: Option<i64>,

    pub(super) covers: Vec<(PathBuf, i64, Option<String>)>,
}

fn stamp(state: &App) -> i64 {
    let mut newest = 0;
    for library in state.libraries.all.borrow().iter() {
        let db = library.path.join(".numa").join("catalog.db");
        for path in [db.clone(), db.with_extension("db-wal")] {
            if let Ok(when) = std::fs::metadata(&path).and_then(|meta| meta.modified()) {

                let at = when.duration_since(std::time::UNIX_EPOCH).map(|since| since.as_nanos() as i64);
                newest = newest.max(at.unwrap_or(0));
            }
        }
    }
    newest
}

pub(super) fn key(place: &Place) -> String {
    match place {
        Place::Everywhere => "*".to_string(),
        Place::Library(library) => format!("l{}", library.id),
        Place::Album(album) => format!("a{album}"),
        Place::Person(name) => format!("p{}", name.to_lowercase()),
    }
}

fn shelves(state: &App) -> Rc<HashMap<i64, Glance>> {
    let now = stamp(state);

    if let Some(had) = SHELVES.with(|shelves| (shelves.borrow().0 == now).then(|| shelves.borrow().1.clone())) {
        return had;
    }
    let started = std::time::Instant::now();
    let read: Rc<HashMap<i64, Glance>> = state
        .libraries.all
        .borrow()
        .iter()

        .filter_map(|library| Some((library.id, state.catalog.glance(library.id, None, None).ok()?)))
        .collect::<HashMap<_, _>>()
        .into();
    log::debug!("places read {} libraries in {:.1} ms", read.len(), started.elapsed().as_secs_f64() * 1000.0);
    SHELVES.with(|shelves| *shelves.borrow_mut() = (now, read.clone()));
    read
}

pub(super) fn in_order(state: &App, shelf: &mut [&Library]) {
    let shelves = shelves(state);
    shelf.sort_by_key(|library| order_key(shelves.get(&library.id).and_then(|glance| glance.last)));
}

pub(super) fn shelves_in_order(state: &App, grouped: &mut [(String, Vec<&Library>)]) {
    grouped.iter_mut().for_each(|(_, shelf)| in_order(state, shelf));
    let shelves = shelves(state);
    grouped.sort_by_key(|(_, shelf)| order_key(shelf.iter().filter_map(|library| shelves.get(&library.id)?.last).max()));
}

pub(super) fn name_of(state: &App, place: &Place) -> String {
    match place {
        Place::Everywhere => "Photos".to_string(),
        Place::Library(library) => library.label(),

        Place::Album(key) => state
            .catalog
            .albums()
            .ok()
            .and_then(|albums| albums.into_iter().find(|(album, _)| album == key).map(|(_, name)| name))
            .unwrap_or_else(|| key.clone()),
        Place::Person(person) => person.clone(),
    }
}

fn order_key(when: Option<i64>) -> (bool, std::cmp::Reverse<Option<i64>>) {
    (when.is_none(), std::cmp::Reverse(when))
}

pub(super) fn row(state: &App, place: &Place) -> Rc<Row> {
    read_all(state);
    GLANCES.with(|glances| glances.borrow().1.get(&key(place)).cloned()).unwrap_or_default()
}

fn read_all(state: &App) {
    let now = stamp(state);
    if GLANCES.with(|glances| glances.borrow().0 == now) {
        return;
    }
    let started = std::time::Instant::now();
    let libraries = state.libraries.all.borrow().clone();
    let places = state.libraries.places.borrow().clone();

    let each = shelves(state);

    let mut everyone: Vec<(String, Vec<i64>)> = Vec::new();
    if places.iter().any(|place| matches!(place, Place::Person(_))) {
        for library in &libraries {
            for (name, photos) in state.catalog.people(library.id).unwrap_or_default() {
                match everyone.iter_mut().find(|(had, _)| had.eq_ignore_ascii_case(&name)) {
                    Some((_, had)) => had.extend(photos),
                    None => everyone.push((name, photos)),
                }
            }
        }
    }

    let mut rows = HashMap::new();
    for place in &places {

        let glances: Vec<Glance> = match place {
            Place::Library(library) => each.get(&library.id).cloned().into_iter().collect(),

            Place::Everywhere => libraries.iter().filter(|it| !home_sync::is_home(state, it.id)).filter_map(|it| each.get(&it.id).cloned()).collect(),
            Place::Album(album) => {
                libraries.iter().filter_map(|it| state.catalog.glance(it.id, Some(album), None).ok()).collect()
            }
            Place::Person(name) => {
                let theirs = everyone.iter().find(|(had, _)| had.eq_ignore_ascii_case(name));
                let theirs = theirs.map(|(_, photos)| photos.as_slice()).unwrap_or_default();
                libraries
                    .iter()

                    .filter(|library| theirs.iter().any(|id| numa::io::catalog::library_of(*id) == library.id))
                    .filter_map(|it| state.catalog.glance(it.id, None, Some(theirs)).ok())
                    .collect()
            }
        };

        let row = match place {
            Place::Library(library) if glances.is_empty() && !library.path.exists() => {
                Row { subtitle: "Not connected".to_string(), ..Row::default() }
            }

            Place::Library(library) if !library.path.exists() => {
                let row = assemble(&glances);
                Row { subtitle: format!("Not connected · {}", row.subtitle), ..row }
            }
            _ => assemble(&glances),
        };

        let row = match place {
            Place::Library(library) if home_sync::is_home(state, library.id) => {
                Row { subtitle: format!("{} \u{b7} {}", home::NUMA_HOME, row.subtitle), ..row }
            }
            _ => row,
        };
        rows.insert(key(place), Rc::new(row));
    }

    log::debug!("places read {} rows in {:.1} ms", rows.len(), started.elapsed().as_secs_f64() * 1000.0);
    GLANCES.with(|glances| *glances.borrow_mut() = (now, rows));
}

fn assemble(glances: &[Glance]) -> Row {
    let photos: i64 = glances.iter().map(|glance| glance.photos).sum();
    let first = glances.iter().filter_map(|glance| glance.first).min();
    let last = glances.iter().filter_map(|glance| glance.last).max();

    let mut covers = Vec::new();
    for round in 0..numa::io::catalog::COVERS {
        for glance in glances {
            if covers.len() == FAN.len() {
                break;
            }
            let Some(photo) = glance.covers.get(round) else { continue };
            if numa::io::thumbs::any_cached(&photo.0, photo.1, photo.2.as_deref()).is_some() {
                covers.push(photo.clone());
            }
        }
    }
    Row { subtitle: subtitle(photos, first, last), count: photos, first, last, covers }
}

fn subtitle(photos: i64, first: Option<i64>, last: Option<i64>) -> String {
    let count = match photos {
        0 => "No photos".to_string(),
        1 => "1 photo".to_string(),
        photos => format!("{} photos", grouped(photos)),
    };
    match period(first, last) {
        Some(period) => format!("{count} \u{b7} {period}"),
        None => count,
    }
}

pub(super) fn photos(count: usize) -> String {
    match count {
        1 => "1 photo".to_string(),
        n => format!("{} photos", grouped(n as i64)),
    }
}

pub(super) fn grouped(number: i64) -> String {
    let digits = number.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (at, digit) in digits.chars().enumerate() {
        if at > 0 && (digits.len() - at) % 3 == 0 {
            out.push(GROUP);
        }
        out.push(digit);
    }
    out
}

const MONTHS: [&str; 12] =
    ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

pub(super) fn period(first: Option<i64>, last: Option<i64>) -> Option<String> {
    let month = |when: i64| {
        let at = glib::DateTime::from_unix_local(when).ok()?;
        Some(format!("{} {}", MONTHS[(at.month() as usize).clamp(1, 12) - 1], at.year()))
    };
    let (from, to) = (month(first?)?, month(last?)?);

    Some(if from == to { from } else { format!("{from}\u{2009}\u{2013}\u{2009}{to}") })
}

pub(super) fn texture(photo: &(PathBuf, i64, Option<String>), edge: u32) -> Option<gtk::gdk::Texture> {
    let (path, mtime, edits) = photo;
    let key = (path.clone(), edge);
    if let Some(known) = TEXTURES.with(|textures| textures.borrow().get(&key).cloned()) {
        return Some(known);
    }
    let edits = edits.as_deref();

    let file = numa::io::thumbs::cached(path, *mtime, edge, edits).or_else(|| numa::io::thumbs::any_cached(path, *mtime, edits))?;
    let whole = image::open(&file).ok()?;
    let side = whole.width().min(whole.height());
    let square = whole
        .crop_imm((whole.width() - side) / 2, (whole.height() - side) / 2, side, side)
        .resize_exact(edge, edge, image::imageops::FilterType::Triangle)
        .to_rgb8();
    if side > edge {
        numa::io::thumbs::store(path, *mtime, edge, edits, &square);
    }
    let made = texture_from(&square);
    TEXTURES.with(|textures| textures.borrow_mut().insert(key, made.clone()));
    Some(made)
}

fn lean(index: usize, tile: f32) -> gsk::Transform {
    let (angle, slide) = FAN[index];
    let (scale, middle) = (tile / TILE, tile / 2.0);
    let margin = MARGIN * scale;
    gsk::Transform::new()
        .translate(&graphene::Point::new(margin + slide * scale, margin / 2.0))
        .translate(&graphene::Point::new(middle, middle))
        .rotate(angle)
        .translate(&graphene::Point::new(-middle, -middle))
}

pub(super) fn fan(tile: f32) -> gtk::Fixed {
    let margin = MARGIN * tile / TILE;
    let fan = gtk::Fixed::new();

    fan.set_size_request((tile + margin * 2.0) as i32, (tile + margin) as i32);
    fan.set_valign(gtk::Align::Center);

    fan.set_overflow(gtk::Overflow::Visible);

    if tile >= 100.0 {
        fan.add_css_class("large-stack");
    }

    for index in 0..FAN.len() {

        let picture = gtk::Image::new();
        picture.set_pixel_size(tile as i32);

        let frame = gtk::Frame::new(None);
        frame.add_css_class("print");
        frame.add_css_class(["print-back", "print-mid", "print-front"][index]);
        frame.set_overflow(gtk::Overflow::Hidden);
        frame.set_child(Some(&picture));
        fan.put(&frame, 0.0, 0.0);
        fan.set_child_transform(&frame, Some(&lean(index, tile)));
    }
    fan
}

pub(super) fn show_covers(fan: &gtk::Fixed, covers: &[(PathBuf, i64, Option<String>)], tile: f32) {

    let mut tiles: Vec<gtk::Widget> = Vec::new();
    let mut child = fan.first_child();
    while let Some(widget) = child {
        child = widget.next_sibling();
        tiles.push(widget);
    }
    tiles.reverse();

    for (frame, cover) in tiles.iter().zip(covers.iter().map(Some).chain(std::iter::repeat(None))) {
        let picture = frame.downcast_ref::<gtk::Frame>().and_then(|frame| frame.child());
        if let Some(picture) = picture.and_downcast::<gtk::Image>() {
            picture.set_paintable(cover.and_then(|cover| texture(cover, card_edge(tile))).as_ref());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{grouped, order_key, period, subtitle};

    #[test]
    fn a_shelf_runs_newest_first_and_ends_with_what_has_no_date() {

        let mut shelf = [Some(1657960247), None, Some(1652022408), Some(1663358713)];
        shelf.sort_by_key(|when| order_key(*when));
        assert_eq!(shelf, [Some(1663358713), Some(1657960247), Some(1652022408), None]);
    }

    #[test]
    fn thousands_are_grouped_the_way_the_panel_writes_them() {
        assert_eq!(grouped(7), "7");
        assert_eq!(grouped(999), "999");

        assert_eq!(grouped(1147), "1\u{2009}147");
        assert_eq!(grouped(11748), "11\u{2009}748");
        assert_eq!(grouped(1234567), "1\u{2009}234\u{2009}567");
    }

    #[test]
    fn a_shoot_inside_one_month_says_one_month() {

        let one = period(Some(1715500800), Some(1716105600)).unwrap();
        assert_eq!(one, "May 2024");

        let two = period(Some(1715500800), Some(1747036800)).unwrap();
        assert!(two.contains('\u{2013}'), "{two}");
    }

    #[test]
    fn a_place_with_no_dates_says_only_how_many() {
        assert_eq!(subtitle(0, None, None), "No photos");
        assert_eq!(subtitle(1, None, None), "1 photo");
        assert_eq!(subtitle(1147, None, None), "1\u{2009}147 photos");
    }
}
