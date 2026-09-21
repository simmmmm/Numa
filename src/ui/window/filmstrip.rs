use super::*;

pub(super) fn build_filmstrip(state: &App) {
    while let Some(child) = state.filmstrip.strip.first_child() {
        state.filmstrip.strip.remove(&child);
    }
    state.filmstrip.badges.borrow_mut().clear();
    state.filmstrip.lazy.borrow_mut().clear();

    const HEIGHT: f32 = 64.0;
    let cards = state.grid.cards.borrow();
    for id in state.grid.order.borrow().iter() {
        let Some((photo, _)) = cards.get(id) else { continue };

        let aspect = numa::io::thumbs::cached_size(&photo.path, photo.mtime, GRID_THUMB_EDGE)
            .map_or(1.5, |(width, height)| width as f32 / height.max(1) as f32)
            .clamp(0.5, 2.5);
        let picture = gtk::Picture::new();
        picture.set_content_fit(gtk::ContentFit::Cover);
        picture.set_size_request((HEIGHT * aspect).round() as i32, HEIGHT as i32);
        picture.set_overflow(gtk::Overflow::Hidden);

        let stacked = gtk::Overlay::new();
        stacked.set_child(Some(&picture));
        if photo.edited {
            let mark = gtk::Image::from_icon_name("document-edit-symbolic");
            mark.set_pixel_size(11);
            mark.add_css_class("edited-mark");
            mark.set_halign(gtk::Align::Start);
            mark.set_valign(gtk::Align::Start);
            stacked.add_overlay(&mark);
        }

        let badge = gtk::Label::new(Some(&strip_badge_text(photo.rating, photo.flag)));
        badge.add_css_class("strip-badge");
        badge.set_halign(gtk::Align::End);
        badge.set_valign(gtk::Align::Start);
        badge.set_visible(!badge.text().is_empty());
        if photo.flag == Flag::Rejected {
            badge.add_css_class("rejected");
        }
        stacked.add_overlay(&badge);
        state.filmstrip.badges.borrow_mut().insert(*id, badge);

        let frame = gtk::Button::new();
        frame.set_child(Some(&stacked));
        frame.add_css_class("filmstrip-frame");
        frame.set_tooltip_text(photo.path.file_name().and_then(|name| name.to_str()));
        frame.set_widget_name(&id.to_string());
        frame.connect_clicked(glib::clone!(
            #[strong] state,
            #[strong] id,
            move |_| open_photo(&state, id)
        ));

        state.filmstrip.lazy.borrow_mut().push(LazyThumb {
            id: *id,
            path: photo.path.clone(),
            mtime: photo.mtime,
            edited: photo.edited,
            edge: GRID_THUMB_EDGE,
            asked: 0,
            picture,
            widget: frame.clone().upcast(),
            wanted: false,
        });
        state.filmstrip.strip.append(&frame);
    }

    let state = state.clone();
    state.filmstrip.strip.clone().add_tick_callback(move |_, _| {
        sweep_thumbnails(&state);
        glib::ControlFlow::Break
    });
}

pub(super) fn mark_filmstrip(state: &App, current: i64) {
    let mut found = None;
    let mut child = state.filmstrip.strip.first_child();
    while let Some(widget) = child {
        child = widget.next_sibling();
        if widget.widget_name() == current.to_string() {
            widget.add_css_class("current");
            found = Some(widget);
        } else {
            widget.remove_css_class("current");
        }
    }

    let Some(widget) = found else { return };
    let state = state.clone();
    let frames_waited = Cell::new(0);
    state.filmstrip.strip.clone().add_tick_callback(move |_, _| {

        if !widget.has_css_class("current") {
            return glib::ControlFlow::Break;
        }

        let adjustment = state.filmstrip.scroller.hadjustment();
        let bounds = widget.compute_bounds(&state.filmstrip.strip);

        let measured = widget.width() > 0 && adjustment.upper() > 0.0;

        frames_waited.set(frames_waited.get() + 1);
        if !measured {

            return if frames_waited.get() < 30 {
                glib::ControlFlow::Continue
            } else {
                glib::ControlFlow::Break
            };
        }

        let bounds = bounds.expect("measured");
        let centre = bounds.x() as f64 + bounds.width() as f64 / 2.0;
        let reach = adjustment.upper() - adjustment.page_size();

        adjustment.set_value((centre - adjustment.page_size() / 2.0).clamp(0.0, reach.max(0.0)));
        glib::ControlFlow::Break
    });
}

#[derive(Clone)]
pub(super) struct State {
    pub(super) strip: gtk::Box,
    pub(super) scroller: gtk::ScrolledWindow,

    pub(super) badges: Rc<RefCell<HashMap<i64, gtk::Label>>>,
    pub(super) lazy: Rc<RefCell<Vec<LazyThumb>>>,
}

impl State {
    pub(super) fn new() -> Self {
        Self {
            strip: gtk::Box::new(gtk::Orientation::Horizontal, 6),
            scroller: gtk::ScrolledWindow::new(),
            badges: Rc::new(RefCell::new(HashMap::new())),
            lazy: Rc::new(RefCell::new(Vec::new())),
        }
    }
}
