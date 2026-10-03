use gtk::gdk;
use gtk::glib;
use gtk::graphene;
use gtk::prelude::*;
use gtk::subclass::prelude::*;

pub const ROW_HEIGHT: f32 = 160.0;

pub const SPACING: f32 = 8.0;

pub const UNKNOWN_ASPECT: f32 = 1.5;

const PAD: f32 = 12.0;

#[derive(Clone, Copy, Default)]
struct Rect {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

#[derive(Default)]
struct Layout {
    width: i32,
    height: f32,
    rects: Vec<Rect>,

    rows: Vec<usize>,
}

type Make = Box<dyn Fn() -> gtk::Widget>;
type Bind = Box<dyn Fn(&gtk::Widget, usize)>;

mod imp {
    use super::*;
    use glib::subclass::Signal;
    use std::cell::{Cell, RefCell};
    use std::sync::OnceLock;

    #[derive(Default)]
    pub struct Justified {

        pub(super) aspects: RefCell<Vec<f32>>,
        pub(super) selected: RefCell<Vec<bool>>,
        pub(super) layout: RefCell<Layout>,

        pub(super) live: RefCell<Vec<(usize, gtk::Widget)>>,
        pub(super) spare: RefCell<Vec<gtk::Widget>>,
        pub(super) make: RefCell<Option<Make>>,
        pub(super) bind: RefCell<Option<Bind>>,
        pub(super) hadjustment: RefCell<Option<gtk::Adjustment>>,
        pub(super) vadjustment: RefCell<Option<gtk::Adjustment>>,
        pub(super) scrolled: RefCell<Option<glib::SignalHandlerId>>,

        pub(super) anchor: Cell<Option<usize>>,
        pub(super) cursor: Cell<Option<usize>>,

        pub(super) band: Cell<Option<(f32, f32, f32, f32)>>,

        pub(super) band_base: RefCell<Vec<bool>>,
        pub(super) band_scrolling: Cell<bool>,

        pub(super) drag: glib::WeakRef<gtk::GestureDrag>,
        pub(super) row_height: Cell<f32>,
        pub(super) spacing: Cell<f32>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Justified {
        const NAME: &'static str = "NumaJustified";
        type Type = super::Justified;
        type ParentType = gtk::Widget;
        type Interfaces = (gtk::Scrollable,);

        fn class_init(klass: &mut Self::Class) {
            klass.set_accessible_role(gtk::AccessibleRole::Grid);
        }
    }

    impl ObjectImpl for Justified {
        fn signals() -> &'static [Signal] {
            static SIGNALS: OnceLock<Vec<Signal>> = OnceLock::new();
            SIGNALS.get_or_init(|| {
                vec![
                    Signal::builder("selection-changed").build(),
                    Signal::builder("card-activated").param_types([u64::static_type()]).build(),
                ]
            })
        }

        fn properties() -> &'static [glib::ParamSpec] {
            static PROPERTIES: OnceLock<Vec<glib::ParamSpec>> = OnceLock::new();
            PROPERTIES.get_or_init(|| {
                ["hadjustment", "vadjustment", "hscroll-policy", "vscroll-policy"]
                    .into_iter()
                    .map(glib::ParamSpecOverride::for_interface::<gtk::Scrollable>)
                    .collect()
            })
        }

        fn set_property(&self, _id: usize, value: &glib::Value, pspec: &glib::ParamSpec) {
            match pspec.name() {
                "hadjustment" => {
                    let adjustment = value.get::<Option<gtk::Adjustment>>().ok().flatten();
                    *self.hadjustment.borrow_mut() = Some(adjustment.unwrap_or_default());
                }
                "vadjustment" => {
                    let adjustment = value.get::<Option<gtk::Adjustment>>().ok().flatten().unwrap_or_default();
                    if let (Some(old), Some(handler)) = (self.vadjustment.take(), self.scrolled.take()) {
                        old.disconnect(handler);
                    }
                    let handler = adjustment.connect_value_changed(glib::clone!(
                        #[weak(rename_to = this)] self.obj(),
                        move |_| this.queue_allocate()
                    ));
                    *self.scrolled.borrow_mut() = Some(handler);
                    *self.vadjustment.borrow_mut() = Some(adjustment);
                    self.obj().queue_allocate();
                }

                _ => {}
            }
        }

        fn property(&self, _id: usize, pspec: &glib::ParamSpec) -> glib::Value {
            match pspec.name() {
                "hadjustment" => self.hadjustment.borrow().to_value(),
                "vadjustment" => self.vadjustment.borrow().to_value(),
                _ => gtk::ScrollablePolicy::Minimum.to_value(),
            }
        }

        fn constructed(&self) {
            self.parent_constructed();
            self.row_height.set(ROW_HEIGHT);
            self.spacing.set(SPACING);
            let obj = self.obj();
            obj.set_focusable(true);
            obj.set_overflow(gtk::Overflow::Hidden);
            obj.install_input();
        }

        fn dispose(&self) {
            self.live.take();
            self.spare.take();

            while let Some(child) = self.obj().first_child() {
                child.unparent();
            }
        }
    }

    impl ScrollableImpl for Justified {}

    impl WidgetImpl for Justified {
        fn request_mode(&self) -> gtk::SizeRequestMode {
            gtk::SizeRequestMode::ConstantSize
        }

        fn measure(&self, orientation: gtk::Orientation, _for_size: i32) -> (i32, i32, i32, i32) {

            let row = self.row_height.get() as i32;
            match orientation {
                gtk::Orientation::Horizontal => (row * 2, row * 6, -1, -1),
                _ => (row, row * 4, -1, -1),
            }
        }

        fn size_allocate(&self, width: i32, height: i32, _baseline: i32) {
            let obj = self.obj();
            obj.layout_for(obj.inner_width(width));
            let content = self.layout.borrow().height as f64 + 2.0 * PAD as f64;
            let (width, height) = (width as f64, height as f64);
            if let Some(adjustment) = self.vadjustment.borrow().clone() {
                let upper = content.max(height);
                let value = adjustment.value().clamp(0.0, upper - height);
                adjustment.configure(value, 0.0, upper, height * 0.1, height * 0.9, height);
            }
            if let Some(adjustment) = self.hadjustment.borrow().clone() {
                adjustment.configure(0.0, 0.0, width, width * 0.1, width * 0.9, width);
            }
            obj.place_cards(height as f32);
        }

        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            self.parent_snapshot(snapshot);
            let Some((x0, y0, x1, y1)) = self.band.get() else { return };
            let offset = self.obj().offset();
            let rect = graphene::Rect::new(
                x0.min(x1) + PAD,
                y0.min(y1) + PAD - offset,
                (x1 - x0).abs(),
                (y1 - y0).abs(),
            );
            let mut colour = self.obj().color();
            colour.set_alpha(0.12);
            snapshot.append_color(&colour, &rect);
            colour.set_alpha(0.5);
            snapshot.append_border(
                &gtk::gsk::RoundedRect::from_rect(rect, 0.0),
                &[1.0; 4],
                &[colour; 4],
            );
        }
    }
}

glib::wrapper! {
    pub struct Justified(ObjectSubclass<imp::Justified>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Scrollable;
}

impl Default for Justified {
    fn default() -> Self {
        glib::Object::new()
    }
}

const OVERSCAN: f32 = 0.5;

impl Justified {

    pub fn set_factory(&self, make: impl Fn() -> gtk::Widget + 'static, bind: impl Fn(&gtk::Widget, usize) + 'static) {
        *self.imp().make.borrow_mut() = Some(Box::new(make));
        *self.imp().bind.borrow_mut() = Some(Box::new(bind));
    }

    pub fn fill(&self, aspects: Vec<f32>) {
        let imp = self.imp();
        let had_selection = imp.selected.borrow().contains(&true);
        let count = aspects.len();
        *imp.aspects.borrow_mut() = aspects;
        *imp.selected.borrow_mut() = vec![false; count];
        imp.anchor.set(None);
        imp.cursor.set(None);

        let live = imp.live.take();
        for (_, card) in &live {
            card.unset_state_flags(gtk::StateFlags::SELECTED);
            card.set_child_visible(false);
        }
        imp.spare.borrow_mut().extend(live.into_iter().map(|(_, card)| card));

        self.forget_layout();
        if had_selection {
            self.emit_by_name::<()>("selection-changed", &[]);
        }
    }

    pub fn remove_all(&self) {
        self.fill(Vec::new());
    }

    pub fn len(&self) -> usize {
        self.imp().aspects.borrow().len()
    }

    pub fn aspect(&self, index: usize) -> Option<f32> {
        self.imp().aspects.borrow().get(index).copied()
    }

    pub fn card(&self, index: usize) -> Option<gtk::Widget> {
        self.imp().live.borrow().iter().find(|(at, _)| *at == index).map(|(_, card)| card.clone())
    }

    pub fn rebind(&self, index: usize) {
        if let Some(card) = self.card(index) {
            self.bind_card(&card, index);
        }
    }

    pub fn rebind_all(&self) {
        let live = self.imp().live.borrow().clone();
        for (index, card) in live {
            self.bind_card(&card, index);
        }
    }

    pub fn set_aspect(&self, index: usize, aspect: f32) {
        {
            let mut aspects = self.imp().aspects.borrow_mut();
            let Some(entry) = aspects.get_mut(index) else { return };

            if (*entry - aspect).abs() < 0.01 * aspect {
                return;
            }
            *entry = aspect;
        }
        self.relayout_holding_view();
    }

    fn relayout_holding_view(&self) {
        let imp = self.imp();

        let width = self.inner_width(self.width());
        let anchor = self.view().filter(|_| imp.layout.borrow().width == width).and_then(
            |(adjustment, top, _)| {
                let layout = imp.layout.borrow();
                let first = layout.rects.iter().position(|rect| rect.y + rect.height >= top)?;
                Some((adjustment, first, layout.rects[first].y))
            },
        );

        self.forget_layout();
        if let Some((adjustment, first, was)) = anchor {
            self.layout_for(width);
            let now = imp.layout.borrow().rects[first].y;
            if now != was {

                let content = imp.layout.borrow().height as f64 + 2.0 * PAD as f64;
                adjustment.set_upper(adjustment.upper().max(content));
                adjustment.set_value(adjustment.value() + (now - was) as f64);
            }
        }
    }

    pub fn selected(&self) -> Vec<usize> {
        let selected = self.imp().selected.borrow();
        selected.iter().enumerate().filter(|(_, on)| **on).map(|(index, _)| index).collect()
    }

    pub fn index_at_point(&self, x: f64, y: f64) -> Option<usize> {
        let (x, y) = self.to_layout(x, y);
        self.index_at(x, y)
    }

    pub fn is_selected(&self, index: usize) -> bool {
        self.imp().selected.borrow().get(index).copied().unwrap_or(false)
    }

    pub fn select_only(&self, index: usize) {
        if index < self.len() {
            self.select_only_index(index);
        }
    }

    pub fn reveal(&self, index: usize) {
        self.scroll_to(index);
    }

    pub fn unselect_all(&self) {
        let count = self.imp().selected.borrow().len();
        self.set_selection(vec![false; count]);
    }

    pub fn visible(&self) -> Option<(usize, usize)> {
        let (width, height) = (self.width(), self.height());
        if width <= 0 || height <= 0 || self.len() == 0 {
            return None;
        }
        self.layout_for(self.inner_width(width));
        let top = self.offset() - PAD;
        let range = self.between(top, top + height as f32);
        (!range.is_empty()).then(|| (range.start, range.end - 1))
    }

    pub fn connect_selection_changed<F: Fn(&Self) + 'static>(&self, f: F) {
        self.connect_closure(
            "selection-changed",
            false,
            glib::closure_local!(move |this: &Self| f(this)),
        );
    }

    pub fn connect_card_activated<F: Fn(&Self, usize) + 'static>(&self, f: F) {
        self.connect_closure(
            "card-activated",
            false,
            glib::closure_local!(move |this: &Self, index: u64| f(this, index as usize)),
        );
    }

    fn activate_card(&self, index: usize) {
        self.emit_by_name::<()>("card-activated", &[&(index as u64)]);
    }

    fn forget_layout(&self) {
        self.imp().layout.borrow_mut().width = -1;
        self.queue_allocate();
    }

    pub fn row_height(&self) -> f32 {
        self.imp().row_height.get()
    }

    pub fn set_sizes(&self, row_height: f32, spacing: f32) {
        let imp = self.imp();
        if imp.row_height.get() == row_height && imp.spacing.get() == spacing {
            return;
        }
        imp.row_height.set(row_height);
        imp.spacing.set(spacing);

        self.relayout_holding_view();
    }

    fn inner_width(&self, width: i32) -> i32 {
        (width - 2 * PAD as i32).max(1)
    }

    fn offset(&self) -> f32 {
        self.imp().vadjustment.borrow().as_ref().map_or(0.0, |adjustment| adjustment.value() as f32)
    }

    fn to_layout(&self, x: f64, y: f64) -> (f32, f32) {
        (x as f32 - PAD, y as f32 + self.offset() - PAD)
    }

    fn layout_for(&self, width: i32) {
        let imp = self.imp();
        if imp.layout.borrow().width != width {
            let layout = self.justify(width);
            *imp.layout.borrow_mut() = Layout { width, ..layout };
        }
    }

    fn justify(&self, width: i32) -> Layout {
        let imp = self.imp();
        let aspects: Vec<f32> = imp.aspects.borrow().iter().map(|aspect| aspect.max(0.1)).collect();
        justify(&aspects, width as f32, imp.row_height.get(), imp.spacing.get())
    }

    fn make_card(&self) -> Option<gtk::Widget> {
        let card = self.imp().make.borrow().as_ref()?();
        card.set_parent(self);
        card.set_child_visible(false);
        Some(card)
    }

    fn bind_card(&self, card: &gtk::Widget, index: usize) {
        if let Some(bind) = self.imp().bind.borrow().as_ref() {
            bind(card, index);
        }
        match self.is_selected(index) {
            true => card.set_state_flags(gtk::StateFlags::SELECTED, false),
            false => card.unset_state_flags(gtk::StateFlags::SELECTED),
        }
    }

    fn between(&self, top: f32, bottom: f32) -> std::ops::Range<usize> {
        let layout = self.imp().layout.borrow();
        let (rows, count) = (&layout.rows, layout.rects.len());

        let first = rows.partition_point(|start| layout.rects[*start].y <= top).saturating_sub(1);
        let last = rows.partition_point(|start| layout.rects[*start].y <= bottom);
        let start = rows.get(first).copied().unwrap_or(count);
        let end = rows.get(last).copied().unwrap_or(count);
        start..end.max(start)
    }

    fn place_cards(&self, height: f32) {
        let imp = self.imp();
        let offset = self.offset();
        let reach = height * OVERSCAN;
        let top = offset - PAD;
        let wanted = self.between(top - reach, top + height + reach);

        let (kept, gone): (Vec<_>, Vec<_>) = imp.live.take().into_iter().partition(|(index, _)| wanted.contains(index));
        for (_, card) in gone {
            card.set_child_visible(false);
            imp.spare.borrow_mut().push(card);
        }
        let mut bound = vec![false; wanted.len()];
        for (index, _) in &kept {
            bound[index - wanted.start] = true;
        }
        *imp.live.borrow_mut() = kept;
        for index in wanted.clone().filter(|index| !bound[index - wanted.start]) {
            let card = imp.spare.borrow_mut().pop().or_else(|| self.make_card());
            let Some(card) = card else { break };
            self.bind_card(&card, index);
            card.set_child_visible(true);
            imp.live.borrow_mut().push((index, card));
        }

        let layout = imp.layout.borrow();
        for (index, card) in imp.live.borrow().iter() {
            let Some(rect) = layout.rects.get(*index) else { continue };

            card.measure(gtk::Orientation::Horizontal, -1);
            card.size_allocate(
                &gtk::Allocation::new(
                    (rect.x + PAD).round() as i32,
                    (rect.y + PAD - offset).round() as i32,
                    (rect.width.round() as i32).max(1),
                    (rect.height.round() as i32).max(1),
                ),
                -1,
            );
        }
    }

    fn index_at(&self, x: f32, y: f32) -> Option<usize> {
        let layout = self.imp().layout.borrow();
        let row = layout.rows.partition_point(|first| layout.rects[*first].y <= y).checked_sub(1)?;
        let end = layout.rows.get(row + 1).copied().unwrap_or(layout.rects.len());
        (layout.rows[row]..end).find(|index| {
            let rect = layout.rects[*index];
            (rect.x..=rect.x + rect.width).contains(&x) && (rect.y..=rect.y + rect.height).contains(&y)
        })
    }

    fn set_selection(&self, selection: Vec<bool>) {
        let imp = self.imp();
        if *imp.selected.borrow() == selection {
            return;
        }
        for (index, card) in imp.live.borrow().iter() {
            if selection.get(*index).copied().unwrap_or(false) {
                card.set_state_flags(gtk::StateFlags::SELECTED, false);
            } else {
                card.unset_state_flags(gtk::StateFlags::SELECTED);
            }
        }
        *imp.selected.borrow_mut() = selection;
        self.emit_by_name::<()>("selection-changed", &[]);
    }

    fn select_only_index(&self, index: usize) {
        let mut selection = vec![false; self.imp().selected.borrow().len()];
        selection[index] = true;
        self.set_selection(selection);
        self.imp().anchor.set(Some(index));
        self.imp().cursor.set(Some(index));
    }

    fn select_run(&self, index: usize, keep: bool) {
        let imp = self.imp();
        let anchor = imp.anchor.get().unwrap_or(index);
        let mut selection = if keep {
            imp.selected.borrow().clone()
        } else {
            vec![false; imp.selected.borrow().len()]
        };
        for on in &mut selection[anchor.min(index)..=anchor.max(index)] {
            *on = true;
        }
        self.set_selection(selection);
        imp.anchor.set(Some(anchor));
        imp.cursor.set(Some(index));
    }

    fn view(&self) -> Option<(gtk::Adjustment, f32, f32)> {
        let adjustment = self.imp().vadjustment.borrow().clone()?;
        let top = adjustment.value() as f32 - PAD;
        Some((adjustment, top, top + self.height() as f32))
    }

    fn scroll_to(&self, index: usize) {
        let Some((adjustment, top, bottom)) = self.view() else { return };
        self.layout_for(self.inner_width(self.width()));
        let Some(rect) = self.imp().layout.borrow().rects.get(index).copied() else { return };

        if rect.y < top {
            adjustment.set_value(adjustment.value() - (top - rect.y + PAD) as f64);
        } else if rect.y + rect.height > bottom {
            let past = rect.y + rect.height - bottom + PAD;
            let content = self.imp().layout.borrow().height as f64 + 2.0 * PAD as f64;
            adjustment.set_upper(adjustment.upper().max(content));
            adjustment.set_value(adjustment.value() + past as f64);
        }
    }

    fn install_input(&self) {
        let click = gtk::GestureClick::new();
        click.set_button(gdk::BUTTON_PRIMARY);
        click.connect_pressed(glib::clone!(
            #[weak(rename_to = this)] self,
            move |gesture, presses, x, y| {
                this.grab_focus();
                let modifiers = gesture.current_event_state();
                let (ctrl, shift) = (
                    modifiers.contains(gdk::ModifierType::CONTROL_MASK),
                    modifiers.contains(gdk::ModifierType::SHIFT_MASK),
                );
                let Some(index) = this.index_at_point(x, y) else {
                    if !ctrl && !shift {
                        this.unselect_all();
                    }
                    return;
                };

                if presses == 2 {
                    this.activate_card(index);
                } else if shift {
                    this.select_run(index, ctrl);
                } else if ctrl {
                    let mut selection = this.imp().selected.borrow().clone();
                    selection[index] = !selection[index];
                    this.set_selection(selection);
                    this.imp().anchor.set(Some(index));
                    this.imp().cursor.set(Some(index));
                } else {
                    this.select_only_index(index);
                }
            }
        ));
        self.add_controller(click);

        let drag = gtk::GestureDrag::new();
        drag.set_button(gdk::BUTTON_PRIMARY);
        drag.connect_drag_begin(glib::clone!(
            #[weak(rename_to = this)] self,
            move |gesture, _, _| {
                let keep = gesture.current_event_state().contains(gdk::ModifierType::CONTROL_MASK);
                let imp = this.imp();
                *imp.band_base.borrow_mut() = if keep {
                    imp.selected.borrow().clone()
                } else {
                    vec![false; imp.selected.borrow().len()]
                };
            }
        ));
        drag.connect_drag_update(glib::clone!(
            #[weak(rename_to = this)] self,
            move |gesture, dx, dy| {
                let Some((x, y)) = gesture.start_point() else { return };
                let imp = this.imp();
                let band = match imp.band.get() {
                    None if dx.hypot(dy) < 6.0 => return,
                    None => {
                        let (x0, y0) = this.to_layout(x, y);
                        let (x1, y1) = this.to_layout(x + dx, y + dy);
                        (x0, y0, x1, y1)
                    }
                    Some((x0, y0, _, _)) => {
                        let (x1, y1) = this.to_layout(x + dx, y + dy);
                        (x0, y0, x1, y1)
                    }
                };
                imp.band.set(Some(band));
                this.select_band();
                this.autoscroll();
            }
        ));
        drag.connect_drag_end(glib::clone!(
            #[weak(rename_to = this)] self,
            move |_, _, _| this.end_band()
        ));

        drag.connect_cancel(glib::clone!(
            #[weak(rename_to = this)] self,
            move |_, _| this.end_band()
        ));
        self.imp().drag.set(Some(&drag));
        self.add_controller(drag);

        let keys = gtk::EventControllerKey::new();
        keys.connect_key_pressed(glib::clone!(
            #[weak(rename_to = this)] self,
            #[upgrade_or] glib::Propagation::Proceed,
            move |_, key, _, modifiers| this.key(key, modifiers)
        ));
        self.add_controller(keys);
    }

    fn end_band(&self) {
        if self.imp().band.take().is_some() {
            self.queue_draw();
        }
    }

    fn button_held(&self) -> bool {
        self.imp().drag.upgrade().is_some_and(|drag| drag.is_active())
    }

    fn select_band(&self) {
        let imp = self.imp();
        let Some((x0, y0, x1, y1)) = imp.band.get() else { return };
        let (left, right, top, bottom) = (x0.min(x1), x0.max(x1), y0.min(y1), y0.max(y1));
        let selection: Vec<bool> = {
            let layout = imp.layout.borrow();
            let base = imp.band_base.borrow();
            layout
                .rects
                .iter()
                .zip(base.iter())
                .map(|(rect, kept)| {
                    *kept
                        || (rect.x < right
                            && rect.x + rect.width > left
                            && rect.y < bottom
                            && rect.y + rect.height > top)
                })
                .collect()
        };
        if selection.len() == self.len() {
            self.set_selection(selection);
        }
        self.queue_draw();
    }

    fn autoscroll(&self) {
        let imp = self.imp();
        if imp.band_scrolling.replace(true) {
            return;
        }
        self.add_tick_callback(|this, _| {
            let imp = this.imp();

            if !this.button_held() {
                this.end_band();
            }
            let (Some((x0, y0, x1, y1)), Some((adjustment, top, bottom))) = (imp.band.get(), this.view())
            else {
                imp.band_scrolling.set(false);
                return glib::ControlFlow::Break;
            };
            let past = if y1 < top {
                y1 - top
            } else if y1 > bottom {
                y1 - bottom
            } else {
                imp.band_scrolling.set(false);
                return glib::ControlFlow::Break;
            };
            let before = adjustment.value();
            adjustment.set_value(before + (past as f64 / 4.0).clamp(-40.0, 40.0));
            let moved = (adjustment.value() - before) as f32;

            imp.band.set(Some((x0, y0, x1, y1 + moved)));
            this.select_band();
            glib::ControlFlow::Continue
        });
    }

    fn key(&self, key: gdk::Key, modifiers: gdk::ModifierType) -> glib::Propagation {
        let imp = self.imp();
        let count = self.len();
        if count == 0 {
            return glib::Propagation::Proceed;
        }
        let (ctrl, shift) = (
            modifiers.contains(gdk::ModifierType::CONTROL_MASK),
            modifiers.contains(gdk::ModifierType::SHIFT_MASK),
        );

        if ctrl && matches!(key, gdk::Key::a | gdk::Key::A) {
            if shift {
                self.unselect_all();
            } else {
                self.set_selection(vec![true; count]);
            }
            return glib::Propagation::Stop;
        }

        let cursor = imp.cursor.get();
        let target = match key {
            gdk::Key::Return | gdk::Key::KP_Enter | gdk::Key::ISO_Enter => {
                if let Some(index) = cursor {
                    self.activate_card(index);
                }
                return glib::Propagation::Stop;
            }
            gdk::Key::Left => cursor.map_or(0, |index| index.saturating_sub(1)),
            gdk::Key::Right => cursor.map_or(0, |index| (index + 1).min(count - 1)),
            gdk::Key::Home => 0,
            gdk::Key::End => count - 1,
            gdk::Key::Up => cursor.map_or(0, |index| self.across_rows(index, -1.0)),
            gdk::Key::Down => cursor.map_or(0, |index| self.across_rows(index, 1.0)),
            gdk::Key::Page_Up | gdk::Key::Page_Down => {
                let page = self.view().map_or(self.imp().row_height.get(), |(_, top, bottom)| bottom - top);
                let page = if key == gdk::Key::Page_Up { -page } else { page };
                cursor.map_or(0, |index| self.across_rows(index, page))
            }
            _ => return glib::Propagation::Proceed,
        };

        if shift {
            self.select_run(target, ctrl);
        } else {
            self.select_only_index(target);
        }
        self.scroll_to(target);
        glib::Propagation::Stop
    }

    fn across_rows(&self, index: usize, by: f32) -> usize {
        let layout = self.imp().layout.borrow();
        let Some(rect) = layout.rects.get(index) else { return index };
        let row = layout.rows.partition_point(|first| *first <= index) - 1;
        let reached = layout.rows.partition_point(|first| layout.rects[*first].y <= rect.y + by);
        let other = if by > 0.0 {
            reached.saturating_sub(1).max(row + 1).min(layout.rows.len() - 1)
        } else {
            reached.saturating_sub(1).min(row.saturating_sub(1))
        };
        let end = layout.rows.get(other + 1).copied().unwrap_or(layout.rects.len());
        let middle = rect.x + rect.width / 2.0;
        (layout.rows[other]..end)
            .min_by(|a, b| {
                let distance = |i: usize| (layout.rects[i].x + layout.rects[i].width / 2.0 - middle).abs();
                distance(*a).total_cmp(&distance(*b))
            })
            .unwrap_or(index)
    }
}

fn justify(aspects: &[f32], width: f32, row_height: f32, spacing: f32) -> Layout {
    let height_for = |sum: f32, count: usize| (width - spacing * (count as f32 - 1.0)).max(1.0) / sum;

    let mut layout = Layout { rects: vec![Rect::default(); aspects.len()], ..Layout::default() };
    let mut y = 0.0;
    let mut start = 0;
    while start < aspects.len() {
        let mut end = start;
        let mut sum = 0.0;
        let height = loop {
            sum += aspects[end];
            end += 1;
            let height = height_for(sum, end - start);
            if height <= row_height {
                if end - start > 1 {
                    let shorter = height_for(sum - aspects[end - 1], end - 1 - start);
                    if shorter - row_height < row_height - height {
                        end -= 1;
                        break shorter;
                    }
                }
                break height;
            }
            if end == aspects.len() {
                break row_height;
            }
        };

        let mut x = 0.0;
        for index in start..end {
            let card_width = aspects[index] * height;
            layout.rects[index] = Rect { x, y, width: card_width, height };
            x += card_width + spacing;
        }
        layout.rows.push(start);
        y += height + spacing;
        start = end;
    }
    layout.height = (y - spacing).max(0.0);
    layout
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_fill_the_width_keep_the_order_and_the_shapes() {
        let aspects = [1.5, 0.667, 1.5, 1.5, 3.0, 1.5, 0.667, 1.0, 1.5];
        let width = 1000.0;
        let layout = justify(&aspects, width, ROW_HEIGHT, SPACING);

        assert_eq!(layout.rows[0], 0);
        for (row, first) in layout.rows.iter().enumerate() {
            let end = layout.rows.get(row + 1).copied().unwrap_or(aspects.len());
            let last = layout.rects[end - 1];

            if row + 1 < layout.rows.len() {
                assert!((last.x + last.width - width).abs() < 0.5, "row {row} ends at {}", last.x + last.width);
            }
            for index in *first..end {
                let rect = layout.rects[index];
                let picture = rect.width / rect.height;
                assert!((picture - aspects[index]).abs() < 1e-3);
                assert_eq!(rect.y, layout.rects[*first].y);
            }
        }

        assert!(layout.rects.windows(2).all(|pair| pair[1].y >= pair[0].y));
    }

    #[test]
    fn a_panorama_never_runs_past_the_window() {
        let layout = justify(&[1.5, 12.0, 1.5], 800.0, ROW_HEIGHT, SPACING);
        assert!(layout.rects.iter().all(|rect| rect.x + rect.width <= 800.5));

        assert!(layout.rects.iter().all(|rect| rect.height < 2.0 * ROW_HEIGHT));
    }

    #[test]
    fn a_bigger_size_puts_fewer_photographs_in_a_taller_row_and_the_space_is_kept() {
        let aspects = [1.5; 30];
        let small = justify(&aspects, 1200.0, 120.0, 4.0);
        let large = justify(&aspects, 1200.0, 360.0, 24.0);
        assert!(large.rows.len() > small.rows.len());
        assert!(large.rects[0].height > small.rects[0].height);
        let (first, second) = (large.rects[0], large.rects[1]);
        assert!((second.x - (first.x + first.width) - 24.0).abs() < 1e-3);
    }
}
