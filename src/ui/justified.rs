use gtk::gdk;
use gtk::glib;
use gtk::graphene;
use gtk::prelude::*;
use gtk::subclass::prelude::*;

pub const ROW_HEIGHT: f32 = 160.0;

pub const SPACING: f32 = 8.0;

const CHROME: f32 = 12.0;

pub const UNKNOWN_ASPECT: f32 = 1.5;

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

mod imp {
    use super::*;
    use glib::subclass::Signal;
    use std::cell::{Cell, RefCell};
    use std::sync::OnceLock;

    #[derive(Default)]
    pub struct Justified {
        pub(super) cards: RefCell<Vec<(gtk::Widget, f32)>>,
        pub(super) selected: RefCell<Vec<bool>>,
        pub(super) layout: RefCell<Layout>,

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
                    Signal::builder("card-activated")
                        .param_types([gtk::Widget::static_type()])
                        .build(),
                ]
            })
        }

        fn constructed(&self) {
            self.parent_constructed();
            self.row_height.set(ROW_HEIGHT);
            self.spacing.set(SPACING);
            let obj = self.obj();
            obj.set_focusable(true);
            obj.install_input();
        }

        fn dispose(&self) {
            self.cards.take();

            while let Some(child) = self.obj().first_child() {
                child.unparent();
            }
        }
    }

    impl WidgetImpl for Justified {
        fn request_mode(&self) -> gtk::SizeRequestMode {
            gtk::SizeRequestMode::HeightForWidth
        }

        fn measure(&self, orientation: gtk::Orientation, for_size: i32) -> (i32, i32, i32, i32) {
            match orientation {
                gtk::Orientation::Horizontal => {
                    let row = self.row_height.get() as i32;
                    (row * 2, row * 6, -1, -1)
                }
                _ => {
                    let width = if for_size > 0 { for_size } else { self.row_height.get() as i32 * 6 };
                    let height = self.obj().height_for(width).ceil() as i32;
                    (height, height, -1, -1)
                }
            }
        }

        fn size_allocate(&self, width: i32, _height: i32, _baseline: i32) {
            let obj = self.obj();
            obj.layout_for(width);
            let layout = self.layout.borrow();
            for ((card, _), rect) in self.cards.borrow().iter().zip(&layout.rects) {
                card.size_allocate(
                    &gtk::Allocation::new(
                        rect.x.round() as i32,
                        rect.y.round() as i32,
                        (rect.width.round() as i32).max(1),
                        (rect.height.round() as i32).max(1),
                    ),
                    -1,
                );
            }
        }

        fn snapshot(&self, snapshot: &gtk::Snapshot) {
            self.parent_snapshot(snapshot);
            let Some((x0, y0, x1, y1)) = self.band.get() else { return };
            let rect = graphene::Rect::new(x0.min(x1), y0.min(y1), (x1 - x0).abs(), (y1 - y0).abs());
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
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Default for Justified {
    fn default() -> Self {
        glib::Object::new()
    }
}

impl Justified {

    pub fn append(&self, card: &impl IsA<gtk::Widget>, aspect: f32) {
        card.set_parent(self);
        self.imp().cards.borrow_mut().push((card.clone().upcast(), aspect));
        self.imp().selected.borrow_mut().push(false);
        self.forget_layout();
    }

    pub fn remove_all(&self) {
        let imp = self.imp();
        let had_selection = imp.selected.borrow().contains(&true);
        for (card, _) in imp.cards.take() {
            card.unparent();
        }
        imp.selected.borrow_mut().clear();
        imp.anchor.set(None);
        imp.cursor.set(None);
        self.forget_layout();
        if had_selection {
            self.emit_by_name::<()>("selection-changed", &[]);
        }
    }

    pub fn set_aspect(&self, card: &impl IsA<gtk::Widget>, aspect: f32) {
        let imp = self.imp();
        let card = card.as_ref();
        {
            let mut cards = imp.cards.borrow_mut();
            let Some(entry) = cards.iter_mut().find(|(widget, _)| widget == card) else { return };

            if (entry.1 - aspect).abs() < 0.01 * aspect {
                return;
            }
            entry.1 = aspect;
        }
        self.relayout_holding_view();
    }

    fn relayout_holding_view(&self) {
        let imp = self.imp();

        let width = self.width();
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
                adjustment.set_value(adjustment.value() + (now - was) as f64);
            }
        }
    }

    pub fn selected(&self) -> Vec<gtk::Widget> {
        let imp = self.imp();
        let selected = imp.selected.borrow();
        imp.cards
            .borrow()
            .iter()
            .zip(selected.iter())
            .filter(|(_, on)| **on)
            .map(|((card, _), _)| card.clone())
            .collect()
    }

    pub fn card_at(&self, x: f64, y: f64) -> Option<gtk::Widget> {
        let index = self.index_at(x as f32, y as f32)?;
        Some(self.imp().cards.borrow()[index].0.clone())
    }

    pub fn is_selected(&self, card: &impl IsA<gtk::Widget>) -> bool {
        self.index_of(card.as_ref()).is_some_and(|index| self.imp().selected.borrow()[index])
    }

    pub fn select_only(&self, card: &impl IsA<gtk::Widget>) {
        if let Some(index) = self.index_of(card.as_ref()) {
            self.select_only_index(index);
        }
    }

    pub fn unselect_all(&self) {
        let count = self.imp().selected.borrow().len();
        self.set_selection(vec![false; count]);
    }

    pub fn connect_selection_changed<F: Fn(&Self) + 'static>(&self, f: F) {
        self.connect_closure(
            "selection-changed",
            false,
            glib::closure_local!(move |this: &Self| f(this)),
        );
    }

    pub fn connect_card_activated<F: Fn(&Self, &gtk::Widget) + 'static>(&self, f: F) {
        self.connect_closure(
            "card-activated",
            false,
            glib::closure_local!(move |this: &Self, card: &gtk::Widget| f(this, card)),
        );
    }

    fn forget_layout(&self) {
        self.imp().layout.borrow_mut().width = -1;
        self.queue_resize();
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

    fn height_for(&self, width: i32) -> f32 {
        let imp = self.imp();
        if imp.layout.borrow().width == width {
            return imp.layout.borrow().height;
        }
        self.justify(width).height
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
        let cards = imp.cards.borrow();

        let captions: Vec<f32> = cards
            .iter()
            .map(|(card, _)| card.measure(gtk::Orientation::Vertical, -1).0 as f32)
            .collect();
        let aspects: Vec<f32> = cards.iter().map(|(_, aspect)| aspect.max(0.1)).collect();
        justify(&aspects, &captions, width as f32, imp.row_height.get(), imp.spacing.get())
    }

    fn index_of(&self, card: &gtk::Widget) -> Option<usize> {
        self.imp().cards.borrow().iter().position(|(widget, _)| widget == card)
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
        for ((card, _), on) in imp.cards.borrow().iter().zip(&selection) {
            if *on {
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
        let viewport = self.parent()?;
        let scroller = self.ancestor(gtk::ScrolledWindow::static_type())?;
        let adjustment = scroller.downcast::<gtk::ScrolledWindow>().ok()?.vadjustment();
        let top = viewport.compute_point(self, &graphene::Point::new(0.0, 0.0))?.y();
        Some((adjustment, top, top + viewport.height() as f32))
    }

    fn scroll_to(&self, index: usize) {
        let Some((adjustment, top, bottom)) = self.view() else { return };
        let Some(rect) = self.imp().layout.borrow().rects.get(index).copied() else { return };

        if rect.y < top {
            adjustment.set_value(adjustment.value() - (top - rect.y + self.margin_top() as f32) as f64);
        } else if rect.y + rect.height > bottom {
            let past = rect.y + rect.height - bottom + self.margin_bottom() as f32;
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
                let Some(index) = this.index_at(x as f32, y as f32) else {
                    if !ctrl && !shift {
                        this.unselect_all();
                    }
                    return;
                };

                if presses == 2 {
                    let card = this.imp().cards.borrow()[index].0.clone();
                    this.emit_by_name::<()>("card-activated", &[&card]);
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
                if this.imp().band.get().is_none() && dx.hypot(dy) < 6.0 {
                    return;
                }
                let band = (x as f32, y as f32, (x + dx) as f32, (y + dy) as f32);
                this.imp().band.set(Some(band));
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
        if selection.len() == imp.cards.borrow().len() {
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
        let count = imp.cards.borrow().len();
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
                    let card = imp.cards.borrow()[index].0.clone();
                    self.emit_by_name::<()>("card-activated", &[&card]);
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

fn justify(aspects: &[f32], captions: &[f32], width: f32, row_height: f32, spacing: f32) -> Layout {
    let height_for = |sum: f32, count: usize| {
        (width - spacing * (count as f32 - 1.0) - CHROME * count as f32).max(1.0) / sum
    };

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
        let mut tallest: f32 = 0.0;
        for index in start..end {
            let card_width = aspects[index] * height + CHROME;
            let card_height = height + captions[index];
            layout.rects[index] = Rect { x, y, width: card_width, height: card_height };
            x += card_width + spacing;
            tallest = tallest.max(card_height);
        }
        layout.rows.push(start);
        y += tallest + spacing;
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
        let captions = [40.0; 9];
        let width = 1000.0;
        let layout = justify(&aspects, &captions, width, ROW_HEIGHT, SPACING);

        assert_eq!(layout.rows[0], 0);
        for (row, first) in layout.rows.iter().enumerate() {
            let end = layout.rows.get(row + 1).copied().unwrap_or(aspects.len());
            let last = layout.rects[end - 1];

            if row + 1 < layout.rows.len() {
                assert!((last.x + last.width - width).abs() < 0.5, "row {row} ends at {}", last.x + last.width);
            }
            for index in *first..end {
                let rect = layout.rects[index];
                let picture = (rect.width - CHROME) / (rect.height - captions[index]);
                assert!((picture - aspects[index]).abs() < 1e-3);
                assert_eq!(rect.y, layout.rects[*first].y);
            }
        }

        assert!(layout.rects.windows(2).all(|pair| pair[1].y >= pair[0].y));
    }

    #[test]
    fn a_panorama_never_runs_past_the_window() {
        let layout = justify(&[1.5, 12.0, 1.5], &[0.0; 3], 800.0, ROW_HEIGHT, SPACING);
        assert!(layout.rects.iter().all(|rect| rect.x + rect.width <= 800.5));

        assert!(layout.rects.iter().all(|rect| rect.height < 2.0 * ROW_HEIGHT));
    }

    #[test]
    fn a_bigger_size_puts_fewer_photographs_in_a_taller_row_and_the_space_is_kept() {
        let aspects = [1.5; 30];
        let small = justify(&aspects, &[0.0; 30], 1200.0, 120.0, 4.0);
        let large = justify(&aspects, &[0.0; 30], 1200.0, 360.0, 24.0);
        assert!(large.rows.len() > small.rows.len());
        assert!(large.rects[0].height > small.rects[0].height);
        let (first, second) = (large.rects[0], large.rects[1]);
        assert!((second.x - (first.x + first.width) - 24.0).abs() < 1e-3);
    }
}
