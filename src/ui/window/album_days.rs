use super::*;
use numa::io::album_page::DayLayout;
use numa::io::catalog::{AlbumChapter, AlbumPage, Photo, NO_CHAPTER};

pub(super) fn days(state: &App, page: &AlbumPage, chapter: Option<&AlbumChapter>, days: &[DayLayout], photos: &HashMap<i64, &Photo>, add: bool) -> gtk::Box {
    let all = gtk::Box::new(gtk::Orientation::Vertical, 0);
    for (at, day) in days.iter().enumerate() {

        if at > 0 {
            let rule = gtk::Separator::new(gtk::Orientation::Horizontal);
            rule.add_css_class("album-day-rule");
            all.append(&rule);
        }
        all.append(&one(state, page, chapter.map(|chapter| chapter.key.as_str()), day, photos));
    }
    if add {
        all.append(&add_row(state, page, chapter, days.last().map(|day| day.day)));
    }
    all
}

fn one(state: &App, page: &AlbumPage, chapter: Option<&str>, day: &DayLayout, photos: &HashMap<i64, &Photo>) -> gtk::Box {
    let block = gtk::Box::new(gtk::Orientation::Vertical, 8);
    let head = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let name = gtk::Label::new(Some(&numa::io::album_export::day_name(day.day)));
    name.add_css_class("album-day-name");
    let count = gtk::Label::new(Some(&match day.count {
        0 => "No photos".to_string(),
        1 => "1 photo".to_string(),
        n => format!("{} photos", places::grouped(n as i64)),
    }));
    count.add_css_class("album-day-count");
    count.add_css_class("numeric");
    count.set_valign(gtk::Align::Baseline);
    name.set_valign(gtk::Align::Baseline);
    head.append(&name);
    head.append(&count);
    block.append(&head);
    block.append(&report(state, page, chapter, day));
    if day.strip {
        block.append(&strip(state, &page.key, chapter.unwrap_or(NO_CHAPTER), &day.shown, photos));
    } else if !day.shown.is_empty() {
        block.append(&album_page::tiles(state, page, &day.shown, photos, chapter.unwrap_or(NO_CHAPTER), day.day, day.shown.len() + day.rest));
    }
    block
}

const STRIP_HEIGHT: i32 = 148;
const STRIP_SHAPE: (f32, f32) = (2.0 / 3.0, 16.0 / 9.0);

fn strip(state: &App, album: &str, chapter: &str, ids: &[i64], photos: &HashMap<i64, &Photo>) -> gtk::Overlay {
    let shots: Rc<Vec<Photo>> = Rc::new(ids.iter().filter_map(|id| photos.get(id).map(|photo| (*photo).clone())).collect());
    let factory = gtk::SignalListItemFactory::new();
    factory.connect_setup(|_, item| {
        let Some(item) = item.downcast_ref::<gtk::ListItem>() else { return };
        let picture = gtk::Picture::new();
        picture.set_content_fit(gtk::ContentFit::Cover);
        picture.set_can_shrink(true);
        let tile = gtk::Overlay::new();
        tile.set_child(Some(&picture));
        tile.set_overflow(gtk::Overflow::Hidden);
        tile.add_css_class("album-tile");
        let mark = gtk::Label::new(None);
        mark.add_css_class("card-badge");
        mark.set_halign(gtk::Align::Start);
        mark.set_valign(gtk::Align::End);
        mark.set_can_target(false);
        tile.add_overlay(&mark);
        item.set_child(Some(&tile));
    });
    factory.connect_bind(glib::clone!(
        #[strong] state,
        #[strong] shots,
        move |_, item| {
            let Some(item) = item.downcast_ref::<gtk::ListItem>() else { return };
            let (Some(tile), Some(photo)) = (item.child().and_downcast::<gtk::Overlay>(), shots.get(item.position() as usize)) else { return };
            let Some(picture) = tile.child().and_downcast::<gtk::Picture>() else { return };
            let name = photo.id.to_string();
            tile.set_widget_name(&name);
            let shape = photo.aspect.unwrap_or(1.5).clamp(STRIP_SHAPE.0, STRIP_SHAPE.1);
            tile.set_size_request((STRIP_HEIGHT as f32 * shape).round() as i32, STRIP_HEIGHT);
            picture.set_paintable(None::<&gtk::gdk::Paintable>);
            if let Some(mark) = picture.next_sibling().and_downcast::<gtk::Label>() {
                let (badge, _) = badges::card_badge(photo.rating, photo.flag, None, false);
                mark.set_visible(!badge.is_empty());
                mark.set_text(&badge);
            }
            tile.set_tooltip_text(photo.path.file_name().and_then(|name| name.to_str()));

            let (wanted, shown) = (tile.downgrade(), picture.downgrade());
            thumbnail::load_thumbnail_while(
                &photo.path,
                photo.mtime,
                GRID_THUMB_EDGE,
                state.catalog.edits_json(photo.id).ok().flatten(),
                None,
                move || wanted.upgrade().is_some_and(|tile| tile.widget_name() == name.as_str()),
                move |texture| {
                    if let Some(picture) = shown.upgrade() {
                        picture.set_paintable(Some(&texture));
                    }
                },
            );
        }
    ));
    let names: Vec<String> = shots.iter().map(|photo| photo.id.to_string()).collect();
    let model = gtk::StringList::new(&names.iter().map(String::as_str).collect::<Vec<_>>());
    let list = gtk::ListView::new(Some(gtk::NoSelection::new(Some(model))), Some(factory));
    list.set_orientation(gtk::Orientation::Horizontal);
    list.set_single_click_activate(true);
    list.add_css_class("album-strip");
    list.update_property(&[gtk::accessible::Property::Label("The day's photos")]);
    list.connect_activate(glib::clone!(
        #[strong] state,
        #[strong] shots,
        #[to_owned] album,
        #[to_owned] chapter,
        move |_, at| {
            if let Some(photo) = shots.get(at as usize) {
                album_page::open_grid(&state, &album, &chapter, None, Some(photo.id));
            }
        }
    ));
    list.add_controller(copy_key(state, &shots));
    let scroller = gtk::ScrolledWindow::new();
    scroller.set_policy(gtk::PolicyType::Automatic, gtk::PolicyType::Never);
    scroller.set_child(Some(&list));
    scroller.add_controller(page_wheel(state));
    let row = gtk::Overlay::new();
    row.set_child(Some(&scroller));
    chevrons(&row, &list, &scroller.hadjustment());
    row
}

fn page_wheel(state: &App) -> gtk::EventControllerScroll {
    let wheel = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::BOTH_AXES);
    wheel.set_propagation_phase(gtk::PropagationPhase::Capture);
    let page = state.libraries.album.root.vadjustment();
    wheel.connect_scroll(move |wheel, dx, dy| {
        if wheel.current_event_state().contains(gtk::gdk::ModifierType::SHIFT_MASK) || dy.abs() <= dx.abs() {
            return glib::Propagation::Proceed;
        }
        let step = match wheel.unit() {
            gtk::gdk::ScrollUnit::Wheel => dy * page.page_size().powf(2.0 / 3.0),
            _ => dy,
        };
        page.set_value((page.value() + step).min(page.upper() - page.page_size()).max(page.lower()));
        glib::Propagation::Stop
    });
    wheel
}

fn copy_key(state: &App, shots: &Rc<Vec<Photo>>) -> gtk::EventControllerKey {
    let keys = gtk::EventControllerKey::new();
    let (state, shots) = (state.clone(), shots.clone());
    keys.connect_key_pressed(move |keys, key, _, modifiers| {
        let both = gtk::gdk::ModifierType::CONTROL_MASK | gtk::gdk::ModifierType::SHIFT_MASK;
        if !modifiers.contains(both) || !matches!(key.to_unicode(), Some('c' | 'C')) {
            return glib::Propagation::Proceed;
        }
        let focused = keys.widget().and_then(|list| list.focus_child()).and_then(|item| item.first_child()).map(|tile| tile.widget_name());
        let Some(photo) = focused.and_then(|name| shots.iter().find(|photo| photo.id.to_string() == name.as_str())) else {
            return glib::Propagation::Proceed;
        };
        exporting::copy_photo(&state, photo_jobs(&state, [(photo.id, photo.path.clone())]).pop());
        glib::Propagation::Stop
    });
    keys
}

fn chevrons(row: &gtk::Overlay, list: &gtk::ListView, along: &gtk::Adjustment) {
    let make = |icon: &str, tip: &str, end: bool| {
        let button = gtk::Button::from_icon_name(icon);
        button.add_css_class("circular");
        button.add_css_class("osd");
        button.add_css_class("album-strip-chevron");
        button.set_tooltip_text(Some(tip));
        button.update_property(&[gtk::accessible::Property::Label(tip)]);
        button.set_halign(if end { gtk::Align::End } else { gtk::Align::Start });
        button.set_valign(gtk::Align::Center);
        button.set_margin_start(8);
        button.set_margin_end(8);
        button.set_visible(false);
        let (list, along) = (list.clone(), along.clone());
        button.connect_clicked(move |_| {
            let step = along.page_size() * 0.9 * if end { 1.0 } else { -1.0 };
            kit::glide(&list, &along, (along.value() + step).min(along.upper() - along.page_size()).max(along.lower()));
        });
        row.add_overlay(&button);
        button
    };
    let back = make("go-previous-symbolic", "Earlier photos", false);
    let on = make("go-next-symbolic", "Later photos", true);
    let hovering = Rc::new(Cell::new(false));
    let update = Rc::new(glib::clone!(
        #[strong] hovering,
        #[weak] back,
        #[weak] on,
        #[weak] along,
        move || {
            back.set_visible(hovering.get() && along.value() > along.lower() + 1.0);
            on.set_visible(hovering.get() && along.value() + along.page_size() < along.upper() - 1.0);
        }
    ));
    let motion = gtk::EventControllerMotion::new();
    for entering in [true, false] {
        let (hovering, update) = (hovering.clone(), update.clone());
        let set = move || {
            hovering.set(entering);
            update();
        };
        if entering { motion.connect_enter(move |_, _, _| set()) } else { motion.connect_leave(move |_| set()) };
    }
    row.add_controller(motion);
    let again = update.clone();
    along.connect_value_changed(move |_| again());
    along.connect_changed(move |_| update());
}

fn report(state: &App, page: &AlbumPage, chapter: Option<&str>, day: &DayLayout) -> gtk::Box {
    let (album, key, number, written) = (page.key.clone(), chapter.map(str::to_string), day.day, day.count == 0);
    let keep = {
        let (album, key) = (album.clone(), key.clone());
        Box::new(move |state: &App, words: &str| state.catalog.set_day_report(&album, key.as_deref(), number, words))
    };
    let left = {
        let key = key.clone();
        Box::new(move |state: &App, words: &str, changed: bool| {
            state.libraries.album.focus_day.replace(None);
            if written && words.trim().is_empty() {

                let (state, album, key) = (state.clone(), album.clone(), key.clone());
                glib::idle_add_local_once(move || match state.catalog.remove_day(&album, key.as_deref(), number) {
                    Ok(()) => album_chapter::changed(&state, &[]),
                    Err(err) => state.toast(&err),
                });
            } else if changed {
                album_round::after_a_round(state);
            }
        })
    };
    let name = format!("Report of {}", numa::io::album_export::day_name(number));

    let prompt = format!("Write about {}", numa::io::album_export::day_name(number).split(' ').next().unwrap_or_default());
    let field = text_field::field(state, &day.report, &prompt, &name, text_field::Kept { keep, left });

    let asked = state.libraries.album.focus_day.borrow().as_ref() == Some(&(key.clone(), number));
    if asked {
        let field = field.downgrade();
        glib::idle_add_local_once(move || {
            if let Some(field) = field.upgrade() {
                text_field::write(&field);
            }
        });
    }
    field
}

fn add_row(state: &App, page: &AlbumPage, chapter: Option<&AlbumChapter>, last: Option<i64>) -> gtk::Button {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    row.append(&gtk::Image::from_icon_name("list-add-symbolic"));
    row.append(&gtk::Label::new(Some("Add a Day…")));
    let button = gtk::Button::new();
    button.add_css_class("flat");
    button.add_css_class("album-add-day");
    button.set_halign(gtk::Align::Start);
    button.set_child(Some(&row));

    let span = (page.kind == numa::io::catalog::AlbumKind::Trip).then(|| album_words::span(page)).flatten();
    let asked = Asked { album: page.key.clone(), name: page.name.clone(), chapter: chapter.map(|chapter| chapter.key.clone()), own: chapter.and_then(|chapter| chapter.days), span };
    let state = state.clone();
    button.connect_clicked(move |_| ask_day(&state, &asked, last));
    button
}

struct Asked {
    album: String,
    name: String,
    chapter: Option<String>,
    own: Option<(i64, i64)>,
    span: Option<(i64, i64)>,
}

fn ask_day(state: &App, asked: &Asked, last: Option<i64>) {
    let span = asked.span;
    let today = glib::DateTime::now_local().map_or(0, |now| (now.to_unix() + now.utc_offset().as_seconds()).div_euclid(86_400));
    let first = last.map_or(asked.own.map_or(today, |(first, _)| first), |last| last + 1);
    let first = span.map_or(first, |(from, to)| first.clamp(from, to));
    let calendar = gtk::Calendar::new();
    if let Ok(date) = glib::DateTime::from_unix_utc(first * 86_400) {
        calendar.select_day(&date);
    }
    let ask = adw::AlertDialog::new(Some("Add a Day"), None);
    if let Some((first, last)) = span {
        ask.set_body(&format!("{}: {}", asked.name, album_words::days_span(first, last, true)));
    }
    ask.set_extra_child(Some(&calendar));
    ask.add_response("cancel", "Cancel");
    ask.add_response("add", "Add");
    ask.set_default_response(Some("add"));
    ask.set_close_response("cancel");
    let inside = move |day: i64| span.is_none_or(|(from, to)| (from..=to).contains(&day));
    calendar.connect_day_selected(glib::clone!(
        #[weak] ask,
        move |calendar| ask.set_response_enabled("add", inside(day_of(&calendar.date())))
    ));
    let (state, album, key) = (state.clone(), asked.album.clone(), asked.chapter.clone());
    let page = state.libraries.album.root.clone();

    let chosen: Rc<Cell<Option<i64>>> = Rc::default();
    let closed = Rc::new(Cell::new(false));
    let add = Rc::new(move |day: i64| match state.catalog.add_day(&album, key.as_deref(), day) {
        Ok(()) => {
            state.libraries.album.focus_day.replace(Some((key.clone(), day)));
            album_chapter::changed(&state, &[]);
        }
        Err(err) => state.toast(&err),
    });
    ask.connect_response(None, glib::clone!(
        #[strong] chosen,
        #[strong] closed,
        #[strong] add,
        move |_, response| {
            if response != "add" {
                return;
            }
            let day = day_of(&calendar.date());
            if closed.get() { add(day) } else { chosen.set(Some(day)) }
        }
    ));
    ask.connect_closed(move |_| {
        closed.set(true);
        if let Some(day) = chosen.take() {
            add(day);
        }
    });
    ask.present(Some(&page));
}

fn day_of(date: &glib::DateTime) -> i64 {
    glib::DateTime::from_utc(date.year(), date.month(), date.day_of_month(), 0, 0, 0.0).map_or(0, |utc| utc.to_unix().div_euclid(86_400))
}
