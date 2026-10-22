use super::*;

const WIDE: i32 = 72;
const TRACK: f64 = 6.0;
const TRACK_END: f64 = 14.0;
const EDGE: f64 = 16.0;

const THUMB: f64 = 18.0;
const THUMB_END: f64 = 8.0;

const YEARS_END: f64 = 32.0;

const FLOOR: f64 = 0.006;
const TINTS: usize = 8;

pub(super) struct Month {
    pub(super) name: String,
    pub(super) count: usize,
    pub(super) libraries: Vec<(i64, usize)>,
}

#[derive(Clone)]
pub(super) struct Scrubber {
    pub(super) area: gtk::DrawingArea,
    pub(super) pill: gtk::Box,
    count: gtk::Label,
    name: gtk::Label,
    library: gtk::Label,
    months: Rc<RefCell<Vec<Month>>>,

    spans: Rc<RefCell<Vec<(f64, f64)>>>,

    tints: Rc<RefCell<HashMap<i64, usize>>>,

    pointer: Rc<Cell<Option<f64>>>,
    dragging: Rc<Cell<bool>>,
}

impl Scrubber {
    pub(super) fn new() -> Self {
        let area = gtk::DrawingArea::new();
        area.set_content_width(WIDE);
        area.set_focusable(true);
        area.set_visible(false);
        area.set_cursor_from_name(Some("ns-resize"));

        area.add_css_class("section-header");
        area.update_property(&[gtk::accessible::Property::Label("Timeline")]);
        let pill = gtk::Box::new(gtk::Orientation::Vertical, 0);
        pill.add_css_class("value-hud");
        pill.set_halign(gtk::Align::End);
        pill.set_valign(gtk::Align::Start);
        pill.set_margin_end(WIDE + 8);
        pill.set_can_target(false);
        pill.set_visible(false);
        let (count, name, library) = (gtk::Label::new(None), gtk::Label::new(None), gtk::Label::new(None));
        count.add_css_class("hud-name");
        name.add_css_class("hud-value");
        library.add_css_class("hud-name");
        for label in [&count, &name, &library] {
            label.set_xalign(1.0);
            pill.append(label);
        }
        Self {
            area,
            pill,
            count,
            name,
            library,
            months: Rc::default(),
            spans: Rc::default(),
            tints: Rc::default(),
            pointer: Rc::default(),
            dragging: Rc::default(),
        }
    }

    pub(super) fn show(&self, scroller: &gtk::ScrolledWindow, months: Vec<Month>, mixed: bool) {
        let total = months.iter().map(|month| month.count).sum::<usize>().max(1) as f64;
        let shares: Vec<f64> = months.iter().map(|month| (month.count as f64 / total).max(FLOOR)).collect();
        let sum: f64 = shares.iter().sum();
        let mut at = 0.0;
        *self.spans.borrow_mut() = shares
            .iter()
            .map(|share| {
                let span = (at, at + share / sum);
                at = span.1;
                span
            })
            .collect();
        let mut tints = HashMap::new();
        if mixed {
            for (library, _) in months.iter().flat_map(|month| &month.libraries) {
                let next = tints.len() % TINTS;
                tints.entry(*library).or_insert(next);
            }
        }
        *self.tints.borrow_mut() = tints;
        let shown = months.len() > 1;
        *self.months.borrow_mut() = months;
        self.area.set_visible(shown);
        self.pill.set_visible(false);

        scroller.set_vscrollbar_policy(if shown { gtk::PolicyType::External } else { gtk::PolicyType::Automatic });
        self.area.queue_draw();
    }

    fn fraction(&self, y: f64) -> f64 {
        ((y - EDGE) / (self.area.height() as f64 - 2.0 * EDGE).max(1.0)).clamp(0.0, 0.99999)
    }

    fn month_at(&self, fraction: f64) -> usize {
        let spans = self.spans.borrow();
        spans.iter().position(|(_, bottom)| fraction < *bottom).unwrap_or(spans.len().saturating_sub(1))
    }

    fn jump(&self, wall: &Justified, fraction: f64) {
        let at = self.month_at(fraction);
        let Some((top, bottom)) = self.spans.borrow().get(at).copied() else { return };
        wall.show_section(at, ((fraction - top) / (bottom - top).max(1e-9)) as f32);
    }

    fn say(&self, state: &App, fraction: f64) {
        let at = self.month_at(fraction);
        let months = self.months.borrow();
        let Some(month) = months.get(at) else { return };
        let Some((top, bottom)) = self.spans.borrow().get(at).copied() else { return };
        self.count.set_text(&format!("{} {}", places::grouped(month.count as i64), if month.count == 1 { "PHOTO" } else { "PHOTOS" }));
        self.name.set_text(&month.name);
        let mut into = (fraction - top) / (bottom - top).max(1e-9) * month.count as f64;
        let under = month.libraries.iter().find(|(_, n)| {
            into -= *n as f64;
            into < 0.0
        });
        let tinted = under.and_then(|(library, _)| Some((*library, *self.tints.borrow().get(library)?)));
        match tinted {
            Some((library, tint)) => {
                let named = state.libraries.all.borrow().iter().find(|one| one.id == library).map(Library::label).unwrap_or_default();
                let tint = colour(&self.area, &format!("numa_library_tint_{tint}"));
                let hex = |part: f32| (part * 255.0).round() as u8;
                let dot = format!("#{:02x}{:02x}{:02x}", hex(tint.red()), hex(tint.green()), hex(tint.blue()));
                self.library.set_markup(&format!("<span foreground=\"{dot}\">●</span> {}", glib::markup_escape_text(&named)));
                self.library.set_visible(true);
            }
            None => self.library.set_visible(false),
        }
        let tall = self.area.height() as f64 - 2.0 * EDGE;
        let high = self.pill.measure(gtk::Orientation::Vertical, -1).1 as f64;
        self.pill.set_margin_top((EDGE + fraction * tall - high / 2.0).max(0.0) as i32);
        self.pill.set_visible(true);
    }

    fn draw(&self, wall: &Justified, cr: &gtk::cairo::Context, width: f64, height: f64) {
        let (spans, months) = (self.spans.borrow(), self.months.borrow());
        if spans.is_empty() {
            return;
        }
        let area = &self.area;
        let tall = (height - 2.0 * EDGE).max(1.0);
        let paint = |rgba: &gtk::gdk::RGBA, alpha: f64| {
            cr.set_source_rgba(rgba.red() as f64, rgba.green() as f64, rgba.blue() as f64, rgba.alpha() as f64 * alpha)
        };
        let grey = colour(area, "numa_label");
        let palette: Vec<gtk::gdk::RGBA> = (0..TINTS).map(|tint| colour(area, &format!("numa_library_tint_{tint}"))).collect();
        let tints = self.tints.borrow();

        let x = width - TRACK_END - TRACK;
        for ((top, bottom), month) in spans.iter().zip(months.iter()) {
            let (from, to) = (EDGE + top * tall, EDGE + bottom * tall);
            let long = if to - from > 2.0 { to - from - 1.0 } else { to - from };
            let mut y = from;
            for (library, n) in &month.libraries {
                let part = long * *n as f64 / month.count.max(1) as f64;
                paint(tints.get(library).map_or(&grey, |tint| &palette[*tint]), 0.85);
                cr.rectangle(x, y, TRACK, part);
                let _ = cr.fill();
                y += part;
            }
        }

        let spaced = gtk::pango::AttrList::new();
        spaced.insert(gtk::pango::AttrInt::new_letter_spacing((1.1 * gtk::pango::SCALE as f64) as i32));
        spaced.insert(gtk::pango::AttrFontFeatures::new("tnum"));
        paint(&grey, 1.0);
        let (mut last, mut shown) = (f64::NEG_INFINITY, "");
        for ((top, _), month) in spans.iter().zip(months.iter()) {
            let year = month.name.rsplit(' ').next().unwrap_or_default();
            let y = EDGE + top * tall;
            if year == shown || y - last < 18.0 {
                continue;
            }
            let words = area.create_pango_layout(Some(year));
            words.set_attributes(Some(&spaced));
            cr.move_to(width - YEARS_END - words.pixel_size().0 as f64, y - 2.0);
            pangocairo::functions::show_layout(cr, &words);
            (last, shown) = (y, year);
        }

        let thumb_x = width - THUMB_END - THUMB;
        let pointer = self.pointer.get();
        if let Some(at) = pointer.filter(|_| !self.dragging.get()) {
            paint(&colour(area, "numa_name"), 1.0);
            cr.rectangle(thumb_x, EDGE + at * tall, THUMB, 1.0);
            let _ = cr.fill();
        }
        let here = match (self.dragging.get(), pointer) {
            (true, Some(at)) => Some(at),
            _ => wall.section_view().and_then(|(at, into)| spans.get(at).map(|(top, bottom)| top + into as f64 * (bottom - top))),
        };
        if let Some(at) = here {
            let y = EDGE + at * tall - 2.0;
            paint(&colour(area, "window_bg_color"), 1.0);
            cr.rectangle(thumb_x - 2.0, y - 2.0, THUMB + 4.0, 8.0);
            let _ = cr.fill();
            paint(&colour(area, "numa_accent"), 1.0);
            cr.rectangle(thumb_x, y, THUMB, 4.0);
            let _ = cr.fill();
        }
    }
}

#[allow(deprecated)]
fn colour(area: &gtk::DrawingArea, name: &str) -> gtk::gdk::RGBA {
    area.style_context().lookup_color(name).unwrap_or_else(|| area.color())
}

pub(super) fn install(state: &App) {
    let scrub = state.grid.scrubber.clone();
    scrub.area.set_draw_func(glib::clone!(
        #[strong] state,
        move |_, cr, width, height| state.grid.scrubber.draw(&state.grid.wall, cr, width as f64, height as f64)
    ));

    let redraw = glib::clone!(
        #[weak(rename_to = area)] scrub.area,
        move |_: &gtk::Adjustment| {
            if area.is_visible() {
                area.queue_draw();
            }
        }
    );
    let adjustment = state.grid.scroller.vadjustment();
    adjustment.connect_value_changed(redraw.clone());
    adjustment.connect_changed(redraw);

    let drag = gtk::GestureDrag::new();
    drag.connect_drag_begin(glib::clone!(
        #[strong] state,
        move |_, _, y| {
            let scrub = &state.grid.scrubber;

            scrub.dragging.set(true);
            follow(&state, y, true);
        }
    ));
    drag.connect_drag_update(glib::clone!(
        #[strong] state,
        move |drag, _, dy| {
            if let Some((_, y)) = drag.start_point() {
                follow(&state, y + dy, true);
            }
        }
    ));
    let done = glib::clone!(
        #[strong] state,
        move || {
            let scrub = &state.grid.scrubber;
            scrub.dragging.set(false);
            scrub.pill.set_visible(false);
            scrub.area.queue_draw();
        }
    );
    let ended = done.clone();
    drag.connect_drag_end(move |_, _, _| ended());
    drag.connect_cancel(move |_, _| done());
    scrub.area.add_controller(drag);

    let motion = gtk::EventControllerMotion::new();
    motion.connect_motion(glib::clone!(
        #[strong] state,
        move |_, _, y| follow(&state, y, false)
    ));
    motion.connect_leave(glib::clone!(
        #[strong] state,
        move |_| {
            let scrub = &state.grid.scrubber;
            if !scrub.dragging.get() {
                scrub.pointer.set(None);
                scrub.pill.set_visible(false);
                scrub.area.queue_draw();
            }
        }
    ));
    scrub.area.add_controller(motion);

    let keys = gtk::EventControllerKey::new();
    keys.connect_key_pressed(glib::clone!(
        #[strong] state,
        move |_, key, _, _| {
            use gtk::gdk::Key;
            let by: isize = match key {
                Key::Up | Key::Page_Up => -1,
                Key::Down | Key::Page_Down => 1,
                _ => return glib::Propagation::Proceed,
            };
            let wall = &state.grid.wall;
            let count = state.grid.scrubber.months.borrow().len();
            if let Some((at, _)) = wall.section_view() {
                wall.show_section((at as isize + by).clamp(0, count as isize - 1) as usize, 0.0);
            }
            glib::Propagation::Stop
        }
    ));
    scrub.area.add_controller(keys);
}

fn follow(state: &App, y: f64, dragging: bool) {
    let scrub = &state.grid.scrubber;
    if scrub.spans.borrow().is_empty() {
        return;
    }
    let fraction = scrub.fraction(y);
    scrub.pointer.set(Some(fraction));
    if dragging {
        scrub.jump(&state.grid.wall, fraction);
    }
    scrub.say(state, fraction);
    scrub.area.queue_draw();
}
