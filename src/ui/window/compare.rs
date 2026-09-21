use super::*;

const MOST: f64 = 8.0;

pub(super) fn build_compare(state: &App) -> gtk::Revealer {
    let compare = &state.compare;
    let root = gtk::Box::new(gtk::Orientation::Vertical, 6);
    root.add_css_class("loupe");
    compare.row.set_homogeneous(true);
    compare.row.set_vexpand(true);
    root.append(&compare.row);

    let keys = gtk::Label::new(Some(
        "The keys act on the photograph under the pointer: 0–5, P, X, U \u{b7} Enter edits it \u{b7} Escape or C closes",
    ));
    keys.add_css_class("loupe-caption");
    keys.set_ellipsize(gtk::pango::EllipsizeMode::End);
    root.append(&keys);

    let reveal = compare.reveal.clone();
    reveal.set_transition_type(gtk::RevealerTransitionType::Crossfade);
    reveal.set_transition_duration(160);
    reveal.set_child(Some(&root));
    reveal.set_visible(false);
    reveal.connect_child_revealed_notify(glib::clone!(
        #[strong] state,
        move |reveal| {
            if !reveal.reveals_child() {
                reveal.set_visible(false);
                let row = &state.compare.row;
                while let Some(child) = row.first_child() {
                    row.remove(&child);
                }
                state.compare.panes.borrow_mut().clear();
            }
        }
    ));
    reveal
}

pub(super) fn open_compare(state: &App) {
    let chosen: Vec<(i64, PathBuf, i64)> = {
        let lazy = state.grid.lazy.borrow();
        selected_cards(state)
            .iter()
            .filter_map(|card| lazy.iter().find(|thumb| thumb.widget == *card))
            .map(|thumb| (thumb.id, thumb.path.clone(), thumb.mtime))
            .collect()
    };
    if !(2..=4).contains(&chosen.len()) {
        state.toast("Select two to four photographs to compare them");
        return;
    }
    compare_these(state, chosen);
}

pub(super) fn compare_these(state: &App, chosen: Vec<(i64, PathBuf, i64)>) {
    let compare = &state.compare;
    while let Some(child) = compare.row.first_child() {
        compare.row.remove(&child);
    }
    compare.zoom.set(1.0);
    compare.centre.set((0.5, 0.5));
    compare.under.set(0);
    compare.open.set(true);

    let mut panes = Vec::new();
    for (index, (id, path, mtime)) in chosen.into_iter().enumerate() {
        let (pane, column) = build_pane(state, index, id);
        compare.row.append(&column);
        let view = pane.view.clone();
        let open = compare.open.clone();
        thumbnail::load_thumbnail_while(&path, mtime, LOUPE_EDGE, None, move || open.get(), move |texture| {
            view.set_texture(texture)
        });
        panes.push(pane);
    }
    *compare.panes.borrow_mut() = panes;
    refresh_compare(state);
    compare.reveal.set_visible(true);
    compare.reveal.set_reveal_child(true);
}

pub(super) fn close_compare(state: &App) {
    state.compare.open.set(false);

    state.compare.reveal.set_reveal_child(false);
}

fn build_pane(state: &App, index: usize, id: i64) -> (Pane, gtk::Box) {
    let column = gtk::Box::new(gtk::Orientation::Vertical, 4);
    let view = view::View::default();
    let picture = gtk::Picture::for_paintable(&view);
    picture.set_content_fit(gtk::ContentFit::Fill);
    picture.set_can_shrink(true);
    picture.set_hexpand(true);
    picture.set_vexpand(true);
    let caption = gtk::Label::new(None);
    caption.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
    column.append(&picture);
    column.append(&caption);

    let motion = gtk::EventControllerMotion::new();
    motion.connect_enter(glib::clone!(
        #[strong] state,
        move |_, _, _| {
            state.compare.under.set(index);
            refresh_compare(&state);
        }
    ));
    picture.add_controller(motion);

    let wheel = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::VERTICAL);
    let pointer: Rc<Cell<(f64, f64)>> = Rc::new(Cell::new((0.0, 0.0)));
    let follow = gtk::EventControllerMotion::new();
    follow.connect_motion(glib::clone!(
        #[strong] pointer,
        move |_, x, y| pointer.set((x, y))
    ));
    picture.add_controller(follow);
    wheel.connect_scroll(glib::clone!(
        #[strong] state,
        #[weak] picture,
        #[upgrade_or] glib::Propagation::Proceed,
        move |_, _, dy| {
            let zoom = (state.compare.zoom.get() * 1.2f64.powf(-dy)).clamp(1.0, MOST);
            zoom_about(&state, &picture, index, zoom, pointer.get());
            glib::Propagation::Stop
        }
    ));
    picture.add_controller(wheel);

    let drag = gtk::GestureDrag::new();
    let from: Rc<Cell<(f64, f64)>> = Rc::new(Cell::new((0.5, 0.5)));
    drag.connect_drag_begin(glib::clone!(
        #[strong] state,
        #[strong] from,
        move |_, _, _| from.set(state.compare.centre.get())
    ));
    drag.connect_drag_update(glib::clone!(
        #[strong] state,
        #[weak] picture,
        move |_, dx, dy| {
            let Some(scale) = pixels_per_frame(&state, &picture, index) else { return };
            let (x, y) = from.get();
            set_centre(&state, &picture, index, (x - dx / scale.0, y - dy / scale.1));
        }
    ));
    picture.add_controller(drag);

    let double = gtk::GestureClick::new();
    double.connect_pressed(glib::clone!(
        #[strong] state,
        #[weak] picture,
        move |_, presses, x, y| {
            if presses != 2 {
                return;
            }
            let zoom = match state.compare.zoom.get() > 1.0 {
                true => 1.0,
                false => one_to_one(&state, &picture, index).clamp(2.0, MOST),
            };
            zoom_about(&state, &picture, index, zoom, (x, y));
        }
    ));
    picture.add_controller(double);

    (Pane { id, view, caption }, column)
}

fn pixels_per_frame(state: &App, picture: &gtk::Picture, index: usize) -> Option<(f64, f64)> {
    let (tw, th) = state.compare.panes.borrow().get(index)?.view.size()?;
    let (w, h) = (picture.width() as f64, picture.height() as f64);
    let scale = (w / tw).min(h / th) * state.compare.zoom.get();
    Some((tw * scale, th * scale))
}

fn one_to_one(state: &App, picture: &gtk::Picture, index: usize) -> f64 {
    let Some((tw, th)) = state.compare.panes.borrow().get(index).and_then(|pane| pane.view.size()) else { return 2.0 };
    let fit = (picture.width() as f64 / tw).min(picture.height() as f64 / th);
    1.0 / fit.max(f64::EPSILON)
}

fn zoom_about(state: &App, picture: &gtk::Picture, index: usize, zoom: f64, at: (f64, f64)) {
    let Some(before) = pixels_per_frame(state, picture, index) else { return };
    let (w, h) = (picture.width() as f64, picture.height() as f64);
    let (cx, cy) = state.compare.centre.get();
    let point = (cx + (at.0 - w / 2.0) / before.0, cy + (at.1 - h / 2.0) / before.1);
    state.compare.zoom.set(zoom);
    let Some(after) = pixels_per_frame(state, picture, index) else { return };
    set_centre(state, picture, index, (point.0 - (at.0 - w / 2.0) / after.0, point.1 - (at.1 - h / 2.0) / after.1));
}

fn set_centre(state: &App, picture: &gtk::Picture, index: usize, centre: (f64, f64)) {
    let Some(span) = pixels_per_frame(state, picture, index) else { return };
    let (w, h) = (picture.width() as f64, picture.height() as f64);
    let keep = |at: f64, room: f64, span: f64| match span > room {
        true => at.clamp(room / 2.0 / span, 1.0 - room / 2.0 / span),
        false => 0.5,
    };
    state.compare.centre.set((keep(centre.0, w, span.0), keep(centre.1, h, span.1)));
    refresh_compare(state);
}

fn refresh_compare(state: &App) {
    let compare = &state.compare;
    let cards = state.grid.cards.borrow();
    for (index, pane) in compare.panes.borrow().iter().enumerate() {
        if let Some((photo, badge)) = cards.get(&pane.id) {
            let name = photo.path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
            pane.caption.set_text(&format!("{name}   {}", badge.text()));
        }
        let under = index == compare.under.get();
        for (class, on) in [("heading", under), ("dim-label", !under)] {
            match on {
                true => pane.caption.add_css_class(class),
                false => pane.caption.remove_css_class(class),
            }
        }
        pane.view.set_view(compare.zoom.get(), compare.centre.get());
    }
}

pub(super) fn compare_key(state: &App, key: gtk::gdk::Key) -> glib::Propagation {
    use gtk::gdk::Key;
    if !state.compare.open.get() {
        return glib::Propagation::Proceed;
    }
    let under = state.compare.panes.borrow().get(state.compare.under.get()).map(|pane| pane.id);
    let action = match key.to_unicode() {
        Some(digit @ '0'..='5') => Some(Action::Rate(digit as u8 - b'0')),
        Some('p' | 'P') => Some(Action::Flag(Flag::Picked)),
        Some('x' | 'X') => Some(Action::Flag(Flag::Rejected)),
        Some('u' | 'U') => Some(Action::Flag(Flag::None)),
        _ => None,
    };
    if let (Some(action), Some(id)) = (action, under) {
        apply_to_ids(state, &[id], action);
        refresh_compare(state);
        return glib::Propagation::Stop;
    }
    match key {
        Key::Escape | Key::c | Key::C => close_compare(state),
        Key::Return | Key::KP_Enter => {
            let card = state.grid.lazy.borrow().iter().find(|thumb| Some(thumb.id) == under).map(|thumb| thumb.widget.clone());
            close_compare(state);
            if let Some(card) = card {
                open_in_editor(state, &card);
            }
        }
        Key::Left | Key::Right | Key::Up | Key::Down | Key::Page_Up | Key::Page_Down | Key::space => {}
        _ => return glib::Propagation::Proceed,
    }
    glib::Propagation::Stop
}

#[derive(Clone)]
struct Pane {
    id: i64,
    view: view::View,
    caption: gtk::Label,
}

#[derive(Clone)]
pub(super) struct State {
    reveal: gtk::Revealer,
    row: gtk::Box,
    panes: Rc<RefCell<Vec<Pane>>>,

    zoom: Rc<Cell<f64>>,
    centre: Rc<Cell<(f64, f64)>>,

    under: Rc<Cell<usize>>,
    open: Rc<Cell<bool>>,
}

impl State {
    pub(super) fn new() -> Self {
        Self {
            reveal: gtk::Revealer::new(),
            row: gtk::Box::new(gtk::Orientation::Horizontal, 6),
            panes: Rc::new(RefCell::new(Vec::new())),
            zoom: Rc::new(Cell::new(1.0)),
            centre: Rc::new(Cell::new((0.5, 0.5))),
            under: Rc::new(Cell::new(0)),
            open: Rc::new(Cell::new(false)),
        }
    }
}

mod view {
    use gtk::subclass::prelude::*;
    use gtk::{gdk, glib, graphene, gsk, prelude::*};

    mod imp {
        use super::*;
        use gdk::subclass::prelude::PaintableImpl;
        use std::cell::{Cell, RefCell};

        #[derive(Default)]
        pub struct View {
            pub texture: RefCell<Option<gdk::Texture>>,
            pub zoom: Cell<f64>,
            pub centre: Cell<(f64, f64)>,
        }

        #[glib::object_subclass]
        impl ObjectSubclass for View {
            const NAME: &'static str = "NumaCompareView";
            type Type = super::View;
            type Interfaces = (gdk::Paintable,);
        }

        impl ObjectImpl for View {}

        impl PaintableImpl for View {
            fn snapshot(&self, snapshot: &gdk::Snapshot, width: f64, height: f64) {
                let texture = self.texture.borrow();
                let Some(texture) = texture.as_ref() else { return };
                let Some(snapshot) = snapshot.downcast_ref::<gtk::Snapshot>() else { return };
                let (tw, th) = (texture.width() as f64, texture.height() as f64);
                let scale = (width / tw).min(height / th) * self.zoom.get().max(1.0);
                let (cx, cy) = self.centre.get();
                let placed = graphene::Rect::new(
                    (width / 2.0 - cx * tw * scale) as f32,
                    (height / 2.0 - cy * th * scale) as f32,
                    (tw * scale) as f32,
                    (th * scale) as f32,
                );

                let filter = match scale > 1.01 {
                    true => gsk::ScalingFilter::Nearest,
                    false => gsk::ScalingFilter::Linear,
                };
                snapshot.push_clip(&graphene::Rect::new(0.0, 0.0, width as f32, height as f32));
                snapshot.append_scaled_texture(texture, filter, &placed);
                snapshot.pop();
            }
        }
    }

    glib::wrapper! {
        pub struct View(ObjectSubclass<imp::View>) @implements gdk::Paintable;
    }

    impl Default for View {
        fn default() -> Self {
            let view: Self = glib::Object::new();
            view.set_view(1.0, (0.5, 0.5));
            view
        }
    }

    impl View {
        pub fn set_texture(&self, texture: gdk::Texture) {
            self.imp().texture.replace(Some(texture));
            self.invalidate_contents();
        }

        pub fn set_view(&self, zoom: f64, centre: (f64, f64)) {
            self.imp().zoom.set(zoom);
            self.imp().centre.set(centre);
            self.invalidate_contents();
        }

        pub fn size(&self) -> Option<(f64, f64)> {
            let texture = self.imp().texture.borrow();
            texture.as_ref().map(|texture| (texture.width() as f64, texture.height() as f64))
        }
    }
}
