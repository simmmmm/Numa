use super::*;

const HEIGHT: f32 = 64.0;

pub(super) fn build_filmstrip(state: &App) {
    let timed = std::time::Instant::now();

    let (lazy, aspects): (Vec<LazyThumb>, Vec<f32>) = {
        let grid = state.grid.lazy.borrow();
        grid.iter()
            .enumerate()
            .map(|(index, card)| {
                let aspect = state.grid.wall.aspect(index).unwrap_or(justified::UNKNOWN_ASPECT).clamp(0.5, 2.5);
                (LazyThumb::new(card.id, card.path.clone(), card.mtime, card.edited, GRID_THUMB_EDGE), aspect)
            })
            .unzip()
    };
    let ids: Vec<String> = lazy.iter().map(|card| card.id.to_string()).collect();
    *state.filmstrip.lazy.borrow_mut() = lazy;
    *state.filmstrip.aspects.borrow_mut() = aspects;
    let model = &state.filmstrip.model;
    let ids: Vec<&str> = ids.iter().map(String::as_str).collect();
    model.splice(0, model.n_items(), &ids);
    if timing() {
        eprintln!("x-data: build_filmstrip {} frames in {:?}", ids.len(), timed.elapsed());
    }
}

pub(super) fn install_filmstrip(state: &App) {
    let factory = gtk::SignalListItemFactory::new();
    factory.connect_setup(glib::clone!(
        #[strong] state,
        move |_, item| {
            let Some(item) = item.downcast_ref::<gtk::ListItem>() else { return };
            item.set_activatable(false);
            item.set_selectable(false);

            item.set_focusable(false);
            item.set_child(Some(&make_frame(&state)));
        }
    ));
    factory.connect_bind(glib::clone!(
        #[strong] state,
        move |_, item| {
            let Some(item) = item.downcast_ref::<gtk::ListItem>() else { return };
            let (Some(frame), index) = (item.child().and_downcast::<gtk::Button>(), item.position() as usize) else {
                return;
            };
            bind_frame(&state, &frame, index);
            state.filmstrip.live.borrow_mut().insert(index, frame);

            schedule_thumbnails(&state);
        }
    ));
    factory.connect_unbind(glib::clone!(
        #[strong] state,
        move |_, item| {
            let Some(item) = item.downcast_ref::<gtk::ListItem>() else { return };
            let Some(frame) = item.child().and_downcast::<gtk::Button>() else { return };
            let mut live = state.filmstrip.live.borrow_mut();
            live.retain(|_, bound| *bound != frame);
        }
    ));
    let strip = &state.filmstrip.strip;
    strip.set_factory(Some(&factory));
    strip.set_model(Some(&gtk::NoSelection::new(Some(state.filmstrip.model.clone()))));
    strip.set_orientation(gtk::Orientation::Horizontal);
    strip.add_css_class("filmstrip-frames");
}

fn make_frame(state: &App) -> gtk::Button {
    let picture = gtk::Picture::new();
    picture.set_content_fit(gtk::ContentFit::Cover);
    picture.set_overflow(gtk::Overflow::Hidden);

    let stacked = gtk::Overlay::new();
    stacked.set_child(Some(&picture));
    let mark = gtk::Image::from_icon_name("document-edit-symbolic");
    mark.set_pixel_size(11);
    mark.add_css_class("edited-mark");
    mark.set_halign(gtk::Align::Start);
    mark.set_valign(gtk::Align::Start);
    stacked.add_overlay(&mark);
    let badge = gtk::Label::new(None);
    badge.add_css_class("strip-badge");
    badge.set_halign(gtk::Align::End);
    badge.set_valign(gtk::Align::Start);
    stacked.add_overlay(&badge);

    let frame = gtk::Button::new();
    frame.set_child(Some(&stacked));
    frame.add_css_class("filmstrip-frame");
    frame.connect_clicked(glib::clone!(
        #[strong] state,
        move |frame| {
            if let Ok(id) = frame.widget_name().parse::<i64>() {
                open_photo(&state, id);
            }
        }
    ));
    frame
}

fn bind_frame(state: &App, frame: &gtk::Button, index: usize) {
    let Some(stacked) = frame.child().and_downcast::<gtk::Overlay>() else { return };
    let Some(picture) = stacked.child().and_downcast::<gtk::Picture>() else { return };
    let Some(mark) = picture.next_sibling() else { return };
    let Some(badge) = mark.next_sibling().and_downcast::<gtk::Label>() else { return };
    let lazy = state.filmstrip.lazy.borrow();
    let Some(thumb) = lazy.get(index) else { return };
    let aspect = state.filmstrip.aspects.borrow().get(index).copied().unwrap_or(justified::UNKNOWN_ASPECT);
    picture.set_size_request((HEIGHT * aspect).round() as i32, HEIGHT as i32);
    picture.set_paintable(thumb.texture.as_ref());

    let cards = state.grid.cards.borrow();
    let marks = cards.get(&thumb.id).map_or((0, Flag::None, thumb.edited), |photo| (photo.rating, photo.flag, photo.edited));
    let (rating, flag, edited) = marks;
    mark.set_visible(edited);
    badge.set_text(&strip_badge_text(rating, flag));
    badge.set_visible(!badge.text().is_empty());
    match flag == Flag::Rejected {
        true => badge.add_css_class("rejected"),
        false => badge.remove_css_class("rejected"),
    }

    frame.set_tooltip_text(thumb.path.file_name().and_then(|name| name.to_str()));
    frame.set_widget_name(&thumb.id.to_string());
    match state.filmstrip.current.get() == Some(thumb.id) {
        true => frame.add_css_class("current"),
        false => frame.remove_css_class("current"),
    }
}

pub(super) fn rebind_frame(state: &App, index: usize) {
    let frame = state.filmstrip.live.borrow().get(&index).cloned();
    if let Some(frame) = frame {
        bind_frame(state, &frame, index);
    }
}

pub(super) fn rebind_frames(state: &App) {
    let live: Vec<(usize, gtk::Button)> =
        state.filmstrip.live.borrow().iter().map(|(index, frame)| (*index, frame.clone())).collect();
    for (index, frame) in live {
        bind_frame(state, &frame, index);
    }
}

pub(super) fn mark_filmstrip(state: &App, current: i64) {
    state.filmstrip.current.set(Some(current));
    rebind_frames(state);
    let Some(index) = state.filmstrip.lazy.borrow().iter().position(|card| card.id == current) else { return };

    let state = state.clone();
    let frames_waited = Cell::new(0);
    state.filmstrip.strip.clone().add_tick_callback(move |strip, _| {

        if state.filmstrip.current.get() != Some(current) {
            return glib::ControlFlow::Break;
        }
        frames_waited.set(frames_waited.get() + 1);
        let frame = state.filmstrip.live.borrow().get(&index).cloned().filter(|frame| frame.width() > 0);
        let Some(frame) = frame else {
            strip.scroll_to(index as u32, gtk::ListScrollFlags::NONE, None);

            return match frames_waited.get() < 30 {
                true => glib::ControlFlow::Continue,
                false => glib::ControlFlow::Break,
            };
        };
        let Some(bounds) = frame.compute_bounds(strip) else { return glib::ControlFlow::Break };
        let adjustment = state.filmstrip.scroller.hadjustment();
        let centre = adjustment.value() + bounds.x() as f64 + bounds.width() as f64 / 2.0;
        let reach = adjustment.upper() - adjustment.page_size();

        adjustment.set_value((centre - adjustment.page_size() / 2.0).clamp(0.0, reach.max(0.0)));
        glib::ControlFlow::Break
    });
}

#[derive(Clone)]
pub(super) struct State {

    pub(super) strip: gtk::ListView,

    pub(super) model: gtk::StringList,
    pub(super) scroller: gtk::ScrolledWindow,
    pub(super) lazy: Rc<RefCell<Vec<LazyThumb>>>,

    pub(super) aspects: Rc<RefCell<Vec<f32>>>,

    pub(super) live: Rc<RefCell<std::collections::BTreeMap<usize, gtk::Button>>>,

    pub(super) current: Rc<Cell<Option<i64>>>,
}

impl State {
    pub(super) fn new() -> Self {
        Self {
            strip: gtk::ListView::new(None::<gtk::NoSelection>, None::<gtk::ListItemFactory>),
            model: gtk::StringList::new(&[]),
            scroller: gtk::ScrolledWindow::new(),
            lazy: Rc::new(RefCell::new(Vec::new())),
            aspects: Rc::default(),
            live: Rc::default(),
            current: Rc::default(),
        }
    }
}
