use gtk::glib;
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use std::cell::{Cell, RefCell};

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct Rows {

        pub(super) aspects: RefCell<Vec<f32>>,
        pub(super) row_height: Cell<f32>,

        pub(super) gap: Cell<f32>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for Rows {
        const NAME: &'static str = "NumaRapidRows";
        type Type = super::Rows;
        type ParentType = gtk::Widget;
    }

    impl ObjectImpl for Rows {
        fn dispose(&self) {
            while let Some(child) = self.obj().first_child() {
                child.unparent();
            }
        }
    }

    impl WidgetImpl for Rows {
        fn request_mode(&self) -> gtk::SizeRequestMode {
            gtk::SizeRequestMode::HeightForWidth
        }

        fn measure(&self, orientation: gtk::Orientation, for_size: i32) -> (i32, i32, i32, i32) {
            let row = self.row_height.get();
            match orientation {
                gtk::Orientation::Horizontal => (row as i32, (row * 6.0) as i32, -1, -1),
                _ => {
                    let width = if for_size < 0 { row * 6.0 } else { for_size as f32 };
                    let height = rows(&self.aspects.borrow(), width, row, self.gap.get()).1.ceil() as i32;
                    (height, height, -1, -1)
                }
            }
        }

        fn size_allocate(&self, width: i32, _height: i32, _baseline: i32) {
            let (rects, _) = rows(&self.aspects.borrow(), width as f32, self.row_height.get(), self.gap.get());
            let mut child = self.obj().first_child();
            for [x, y, w, h] in rects {
                let Some(widget) = child else { break };

                let (left, top) = (x.round() as i32, y.round() as i32);
                widget.size_allocate(&gtk::Allocation::new(left, top, ((x + w).round() as i32 - left).max(1), ((y + h).round() as i32 - top).max(1)), -1);
                child = widget.next_sibling();
            }
        }
    }
}

glib::wrapper! {
    pub struct Rows(ObjectSubclass<imp::Rows>)
        @extends gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget;
}

impl Rows {
    pub fn new(row_height: f32, gap: f32) -> Self {
        let rows: Self = glib::Object::new();
        rows.imp().row_height.set(row_height);
        rows.imp().gap.set(gap);
        rows
    }

    pub fn append(&self, child: &impl IsA<gtk::Widget>, aspect: f32) {
        child.set_parent(self);
        self.imp().aspects.borrow_mut().push(aspect.clamp(0.2, 5.0));
        self.queue_resize();
    }

    pub fn set_aspect(&self, child: &impl IsA<gtk::Widget>, aspect: f32) {
        let mut at = 0;
        let mut next = self.first_child();
        while let Some(widget) = next {
            if widget == *child.upcast_ref() {
                let mut aspects = self.imp().aspects.borrow_mut();
                if let Some(old) = aspects.get_mut(at).filter(|old| (**old - aspect).abs() > 0.01) {
                    *old = aspect.clamp(0.2, 5.0);
                    drop(aspects);
                    self.queue_resize();
                }
                return;
            }
            at += 1;
            next = widget.next_sibling();
        }
    }
}

fn rows(aspects: &[f32], width: f32, row: f32, gap: f32) -> (Vec<[f32; 4]>, f32) {
    let height_for = |start: usize, end: usize, sum: f32| (width - gap * (end - start - 1) as f32).max(1.0) / sum;
    let mut rects = vec![[0.0; 4]; aspects.len()];
    let (mut y, mut start) = (0.0, 0);
    while start < aspects.len() {
        let (mut end, mut sum) = (start, 0.0);
        let height = loop {
            sum += aspects[end];
            end += 1;
            let height = height_for(start, end, sum);
            if height <= row {
                if end - start > 1 {
                    let shorter = height_for(start, end - 1, sum - aspects[end - 1]);
                    if shorter - row < row - height {
                        end -= 1;
                        break shorter;
                    }
                }
                break height;
            }
            if end == aspects.len() {
                break row;
            }
        };
        let mut x = 0.0;
        for index in start..end {
            rects[index] = [x, y, aspects[index] * height, height];
            x += aspects[index] * height + gap;
        }
        y += height + gap;
        start = end;
    }
    (rects, (y - gap).max(0.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_fill_the_width_and_keep_each_shape() {

        let aspects = [1.5, 0.667, 1.5, 1.5];
        let (rects, height) = rows(&aspects, 800.0, 160.0, 10.0);
        let first_row: Vec<&[f32; 4]> = rects.iter().filter(|rect| rect[1] == 0.0).collect();
        let right = first_row.last().map(|rect| rect[0] + rect[2]).unwrap();
        assert!((right - 800.0).abs() < 0.5, "a full row reaches the edge: {right}");
        for (rect, aspect) in rects.iter().zip(aspects) {
            assert!((rect[2] / rect[3] - aspect).abs() < 1e-3, "each at its own shape");
        }
        assert!((rects[3][0] - (rects[2][0] + rects[2][2]) - 10.0).abs() < 1e-3, "a gap between two");
        assert!(height > 0.0);

        let (alone, _) = rows(&[1.5], 800.0, 160.0, 10.0);
        assert_eq!(alone[0][3], 160.0);
    }
}
