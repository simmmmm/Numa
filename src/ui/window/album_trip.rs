use super::*;
use numa::io::catalog::{AlbumPage, ChapterPlace};

const MAP: i32 = 360;

const AROUND: f64 = 32.0;
const DOT: f64 = 4.0;

const MARK: f64 = 9.0;
const LINE: f64 = 2.0;
const DASH: [f64; 2] = [1.0, 6.0];

const UNDER: f64 = 4.0;

const PRINT: f32 = 32.0;

const PEEK: i32 = 8;

const FOUND: usize = 8;

pub(super) fn places_offered(state: &App, page: &AlbumPage) -> gtk::Box {
    let block = gtk::Box::new(gtk::Orientation::Vertical, 8);
    block.append(&kit::section_header("Where these were taken"));
    let list = gtk::ListBox::new();
    list.set_selection_mode(gtk::SelectionMode::None);
    list.add_css_class("boxed-list");
    for offered in &page.layout.places {
        let row = adw::ActionRow::new();
        row.set_title(&glib::markup_escape_text(&offered.town.name));
        row.set_subtitle(&album_words::place_line(offered.first_day, offered.last_day, offered.ids.len()));
        list.append(&row);
    }
    block.append(&list);
    let n = page.layout.places.len();
    let said = gtk::Label::new(Some(&if n == 1 { "1 place offered".to_string() } else { format!("{n} places offered") }));
    said.add_css_class("heading");
    said.set_xalign(0.0);
    let why = gtk::Label::new(Some("Each becomes a chapter with its photographs"));
    why.add_css_class("caption");
    why.add_css_class("dim-label");
    why.set_xalign(0.0);
    let words = gtk::Box::new(gtk::Orientation::Vertical, 0);
    words.set_hexpand(true);
    words.append(&said);
    words.append(&why);
    let chips = kit::chip_row();
    chips.set_valign(gtk::Align::Center);
    let add = gtk::Button::with_label(&format!("Add {n}"));
    let (state, key) = (state.clone(), page.key.clone());
    add.connect_clicked(move |_| match state.catalog.take_places(&key) {
        Ok(made) => {
            state.toast(&if made.len() == 1 { "1 place added".to_string() } else { format!("{} places added", made.len()) });
            let ids: Vec<i64> = state.catalog.album_page(&key, &numa::io::catalog::PageAsked::default()).map(|page| page.photos.iter().map(|photo| photo.id).collect()).unwrap_or_default();
            album_chapter::changed(&state, &ids);
        }
        Err(err) => state.toast(&err),
    });
    chips.append(&add);
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    row.append(&words);
    row.append(&chips);
    block.append(&row);
    block
}

pub(super) fn fill_side(state: &App, page: &AlbumPage, today: i64) {
    let side = &state.libraries.album.side;
    let here = album_words::here_now(page, today);
    let photos: HashMap<i64, &numa::io::catalog::Photo> = page.photos.iter().map(|photo| (photo.id, photo)).collect();

    let covers = |shown: &[i64]| -> Vec<(PathBuf, i64, Option<String>)> {
        shown.iter().filter_map(|id| photos.get(id)).take(3).map(|photo| (photo.path.clone(), photo.mtime, state.catalog.edits_json(photo.id).ok().flatten())).collect()
    };
    let stops: Vec<Stop> = page
        .chapters
        .iter()
        .zip(&page.layout.chapters)
        .enumerate()
        .filter_map(|(at, (chapter, laid))| {
            chapter.place.as_ref().map(|place| Stop { name: chapter.name.clone(), at: (place.latitude, place.longitude), count: laid.count, number: Some(at + 1), here: here == Some(at), key: chapter.key.clone(), covers: covers(&laid.shown) })
        })
        .chain(page.layout.places.iter().map(|offered| Stop { name: offered.town.name.clone(), at: (offered.town.latitude, offered.town.longitude), count: 0, number: None, here: false, key: String::new(), covers: Vec::new() }))
        .collect();
    let route = page.chapters.iter().filter(|chapter| chapter.place.is_some()).count();
    if !stops.is_empty() {
        side.append(&route_map(state, stops, route));
    }
    if !page.chapters.is_empty() {
        side.append(&kit::section_header("The Route"));
        let list = gtk::Box::new(gtk::Orientation::Vertical, 0);
        for (at, (chapter, laid)) in page.chapters.iter().zip(&page.layout.chapters).enumerate() {
            let ahead = laid.count == 0 && chapter.place.is_some();
            let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
            row.append(&album_page::mark(at + 1, ahead, false));
            let name = gtk::Label::new(Some(&chapter.name));
            if !ahead {
                name.add_css_class("heading");
            }
            name.set_xalign(0.0);
            name.set_hexpand(true);
            name.set_ellipsize(gtk::pango::EllipsizeMode::End);
            row.append(&name);
            let said = gtk::Label::new(Some(&album_words::route_line(chapter, laid, here == Some(at))));
            said.add_css_class("caption");
            said.add_css_class("dim-label");
            said.add_css_class("numeric");
            row.append(&said);
            let button = gtk::Button::new();
            button.add_css_class("flat");
            button.add_css_class("album-stop");
            if here == Some(at) {
                button.add_css_class("here");
            }
            button.set_child(Some(&row));
            let (state, key) = (state.clone(), chapter.key.clone());
            button.connect_clicked(move |_| album_chapter::scroll_to(&state, &key));
            list.append(&button);
        }
        side.append(&list);
    }
    side.append(&place_field(state, &page.key));
}

struct Stop {
    name: String,
    at: (f64, f64),
    count: usize,
    number: Option<usize>,
    here: bool,

    key: String,

    covers: Vec<(PathBuf, i64, Option<String>)>,
}

impl Stop {

    fn stacked(&self) -> bool {
        self.number.is_some() && !self.covers.is_empty()
    }
}

fn stack_pin(state: &App, stop: &Stop) -> gtk::Button {
    let fan = places::fan(PRINT);
    places::show_covers(&fan, &stop.covers, PRINT);
    fan.set_margin_start(PEEK);
    fan.set_margin_top(PEEK);
    let over = gtk::Overlay::new();
    over.set_child(Some(&fan));
    let mark = album_page::mark(stop.number.unwrap_or_default(), false, stop.here);
    mark.set_halign(gtk::Align::Start);
    over.add_overlay(&mark);
    let pin = gtk::Button::new();
    pin.add_css_class("flat");
    pin.add_css_class("map-pin");
    pin.set_child(Some(&over));
    pin.set_tooltip_text(Some(&format!("{}, {}", stop.name, places::photos(stop.count))));
    let (state, key) = (state.clone(), stop.key.clone());
    pin.connect_clicked(move |_| album_chapter::scroll_to(&state, &key));
    pin
}

fn name_at(mark: (f64, f64), half: (f64, f64), size: (f64, f64), room: (f64, f64), taken: &[[f64; 4]]) -> (f64, f64) {
    let (x, y) = mark;
    let (wide, high) = size;
    let (across, down) = (half.0 + UNDER, half.1 + UNDER);
    let tries = [(x - wide / 2.0, y + down), (x - wide / 2.0, y - down - high), (x + across, y - high / 2.0), (x - across - wide, y - high / 2.0)];
    let fits = |at: &(f64, f64)| at.0 >= 0.0 && at.1 >= 0.0 && at.0 + wide <= room.0 && at.1 + high <= room.1 && !taken.iter().any(|other| at.0 < other[2] && other[0] < at.0 + wide && at.1 < other[3] && other[1] < at.1 + high);
    tries.iter().copied().find(fits).unwrap_or((tries[0].0.clamp(0.0, (room.0 - wide).max(0.0)), tries[0].1))
}

fn mercator(latitude: f64, longitude: f64) -> (f64, f64) {
    let y = (std::f64::consts::FRAC_PI_4 + latitude.clamp(-85.0, 85.0).to_radians() / 2.0).tan().ln();
    (longitude.to_radians(), -y)
}

fn route_map(state: &App, stops: Vec<Stop>, route: usize) -> gtk::Frame {
    let area = gtk::DrawingArea::new();
    area.set_content_height(MAP);
    area.set_hexpand(true);
    area.add_css_class("album-route");
    area.update_property(&[gtk::accessible::Property::Label("Map of the trip: where it has been and where it goes")]);

    let map = match numa::io::map::ready(&map_view::file()) {
        true => map_view::new_map().map(|(map, _)| map),
        false => None,
    };
    let lats = || stops.iter().map(|stop| stop.at.0);
    let lons = || stops.iter().map(|stop| stop.at.1);
    let bounds = [lons().fold(f64::MAX, f64::min), lats().fold(f64::MAX, f64::min), lons().fold(f64::MIN, f64::max), lats().fold(f64::MIN, f64::max)];
    let under = map.clone();

    let pins = gtk::Fixed::new();
    let stacks: Vec<(usize, gtk::Button, (f64, f64))> = stops
        .iter()
        .enumerate()
        .filter(|(_, stop)| stop.stacked())
        .map(|(at, stop)| {
            let pin = stack_pin(state, stop);
            let size = (f64::from(pin.measure(gtk::Orientation::Horizontal, -1).1), f64::from(pin.measure(gtk::Orientation::Vertical, -1).1));
            pins.put(&pin, 0.0, 0.0);
            (at, pin, size)
        })
        .collect();
    let placed: Rc<RefCell<Vec<(f64, f64)>>> = Rc::default();
    area.set_draw_func(move |area, cr, width, height| {
        let (width, height) = (f64::from(width), f64::from(height));
        let own = |at: (f64, f64)| -> (f64, f64) {
            let ((west, north), (east, south)) = (mercator(bounds[3], bounds[0]), mercator(bounds[1], bounds[2]));
            let scale = ((width - 2.0 * AROUND) / (east - west).max(1e-9)).min((height - 2.0 * AROUND) / (south - north).max(1e-9));
            let (x, y) = mercator(at.0, at.1);
            (width / 2.0 + (x - (west + east) / 2.0) * scale, height / 2.0 + (y - (north + south) / 2.0) * scale)
        };
        let point = |at: (f64, f64)| under.as_ref().and_then(|map| map_view::pixel(map, at.0, at.1)).unwrap_or_else(|| own(at));
        let ink = area.color();
        let mut small = area.pango_context().font_description().unwrap_or_default();
        small.set_size(9 * gtk::pango::SCALE);
        small.set_weight(gtk::pango::Weight::Bold);
        let quiet = gtk::gdk::RGBA::new(ink.red(), ink.green(), ink.blue(), ink.alpha() / 2.0);
        cr.set_line_width(LINE);
        cr.set_line_cap(gtk::cairo::LineCap::Round);
        for pair in stops[..route].windows(2) {
            let done = pair[0].count > 0 && pair[1].count > 0;
            cr.set_source_color(if done { &ink } else { &quiet });
            cr.set_dash(if done { &[] } else { &DASH }, 0.0);
            let (from, to) = (point(pair[0].at), point(pair[1].at));
            cr.move_to(from.0, from.1);
            cr.line_to(to.0, to.1);
            let _ = cr.stroke();
        }
        cr.set_dash(&[], 0.0);

        let at: Vec<(f64, f64)> = stops.iter().map(|stop| point(stop.at)).collect();
        let mut taken: Vec<[f64; 4]> = at.iter().map(|(x, y)| [x - MARK, y - MARK, x + MARK, y + MARK]).collect();

        let corners: Vec<(f64, f64)> = stacks.iter().map(|(stop, _, (wide, high))| (at[*stop].0 - wide / 2.0, at[*stop].1 - high / 2.0)).collect();
        for ((stop, _, (wide, high)), (left, top)) in stacks.iter().zip(&corners) {
            taken[*stop] = [*left, *top, left + wide, top + high];
        }
        if *placed.borrow() != corners {
            placed.replace(corners.clone());

            let moving: Vec<(gtk::Button, (f64, f64))> = stacks.iter().zip(&corners).map(|((_, pin, _), corner)| (pin.clone(), *corner)).collect();
            glib::idle_add_local_once(move || {
                for (pin, (left, top)) in moving {
                    if let Some(pins) = pin.parent().and_downcast::<gtk::Fixed>() {
                        pins.move_(&pin, left, top);
                    }
                }
            });
        }
        for (stop, (x, y)) in stops.iter().zip(&at) {

            cr.new_path();
            let been = stop.count > 0;
            cr.set_source_color(if been { &ink } else { &quiet });

            let mark = !stop.stacked();
            if mark && stop.here {

                cr.arc(*x, *y, MARK + 4.0, 0.0, std::f64::consts::TAU);
                let _ = cr.stroke();
            }
            if mark {
                cr.arc(*x, *y, if stop.number.is_some() { MARK } else { DOT }, 0.0, std::f64::consts::TAU);
                let _ = if been { cr.fill() } else { cr.stroke() };
            }
            if let Some(number) = stop.number.filter(|_| mark) {
                let figure = area.create_pango_layout(Some(&number.to_string()));
                figure.set_font_description(Some(&small));
                let (wide, high) = figure.pixel_size();
                cr.move_to(x - f64::from(wide) / 2.0, y - f64::from(high) / 2.0);

                if been {
                    cr.set_operator(gtk::cairo::Operator::DestOut);
                }
                pangocairo::functions::show_layout(cr, &figure);
                cr.set_operator(gtk::cairo::Operator::Over);
            }
            let said = if been { format!("{} · {}", stop.name, places::grouped(stop.count as i64)) } else { stop.name.clone() };
            let layout = area.create_pango_layout(Some(&said));
            let (wide, high) = layout.pixel_size();
            let (wide, high) = (f64::from(wide), f64::from(high));
            let half = stacks.iter().find(|(one, ..)| std::ptr::eq(&stops[*one], stop)).map_or((MARK, MARK), |(_, _, (w, h))| (w / 2.0, h / 2.0));
            let (left, top) = name_at((*x, *y), half, (wide, high), (width, height), &taken);
            taken.push([left, top, left + wide, top + high]);
            cr.set_source_color(if been { &ink } else { &quiet });
            cr.move_to(left, top);
            pangocairo::functions::show_layout(cr, &layout);
        }
    });
    let over = gtk::Overlay::new();
    match &map {
        Some(map) => {
            map.set_can_target(false);
            map.set_size_request(-1, MAP);
            over.set_child(Some(map));
            over.add_overlay(&area);

            let fitted = map.clone();
            area.connect_resize(move |drawn, _, _| {

                let (fitted, drawn) = (fitted.clone(), drawn.clone());
                glib::idle_add_local_once(move || {
                    map_view::fit(&fitted, bounds, AROUND, 9.0);
                    drawn.queue_draw();
                });
            });
            if let Some(viewport) = map.viewport() {
                let drawn = area.clone();
                viewport.connect_zoom_level_notify(move |_| drawn.queue_draw());
            }
        }
        None => over.set_child(Some(&area)),
    }
    over.add_overlay(&pins);
    let frame = gtk::Frame::new(None);
    frame.add_css_class("album-map");
    frame.set_child(Some(&over));
    frame
}

fn place_field(state: &App, album: &str) -> gtk::Box {
    let entry = gtk::Entry::new();
    entry.set_placeholder_text(Some("Add a Place…"));
    entry.set_max_length(numa::io::catalog::CHAPTER_NAME_LIMIT as i32);
    entry.add_css_class("album-add");
    let found = gtk::ListBox::new();
    found.set_selection_mode(gtk::SelectionMode::None);
    found.add_css_class("boxed-list");
    found.set_visible(false);
    let add = {
        let (state, album) = (state.clone(), album.to_string());
        Rc::new(move |name: &str, place: Option<ChapterPlace>| match state.catalog.add_chapter(&album, name, place, None) {
            Ok(_) => album_chapter::changed(&state, &[]),
            Err(err) => state.toast(&err),
        })
    };
    let place_of = |town: &numa::io::places::Place| ChapterPlace { name: town.name.clone(), latitude: numa::io::album_page::short(town.latitude), longitude: numa::io::album_page::short(town.longitude) };
    entry.connect_changed(glib::clone!(
        #[weak] found,
        #[strong] add,
        move |entry| {
            while let Some(row) = found.first_child() {
                found.remove(&row);
            }
            let towns = numa::io::places::find(&entry.text(), FOUND);
            found.set_visible(!towns.is_empty());
            for town in towns {
                let row = adw::ActionRow::new();
                row.set_title(&glib::markup_escape_text(&town.name));
                let within: Vec<&str> = [town.region.as_str(), town.country.as_str()].into_iter().filter(|part| !part.is_empty() && *part != town.name).collect();
                row.set_subtitle(&glib::markup_escape_text(&within.join(", ")));
                row.set_activatable(true);
                let (add, place) = (add.clone(), place_of(town));
                row.connect_activated(move |_| add(&place.name.clone(), Some(place.clone())));
                found.append(&row);
            }
        }
    ));
    entry.connect_activate(move |entry| {
        let name = entry.text().trim().to_string();
        if name.is_empty() {
            return;
        }
        let same = numa::io::places::find(&name, FOUND).into_iter().find(|town| town.name.eq_ignore_ascii_case(&name)).map(place_of);
        add(&name, same);
    });
    let field = gtk::Box::new(gtk::Orientation::Vertical, 8);
    field.append(&entry);
    field.append(&found);
    field
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_name_stands_clear_of_what_is_placed() {
        let room = (320.0, 360.0);

        assert_eq!(name_at((100.0, 100.0), (MARK, MARK), (40.0, 16.0), room, &[]), (80.0, 100.0 + MARK + UNDER));

        let under = [70.0, 105.0, 130.0, 130.0];
        assert_eq!(name_at((100.0, 100.0), (MARK, MARK), (40.0, 16.0), room, &[under]), (80.0, 100.0 - MARK - UNDER - 16.0));

        assert_eq!(name_at((100.0, 350.0), (MARK, MARK), (40.0, 16.0), room, &[]).1, 350.0 - MARK - UNDER - 16.0);
    }
}
