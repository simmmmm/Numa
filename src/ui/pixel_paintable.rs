use gtk::gdk;
use gtk::glib;
use gtk::prelude::*;
use gtk::subclass::prelude::*;

#[derive(Debug, Clone, Copy)]
pub struct Placement {
    pub frame: (f64, f64),
    pub tile: (f64, f64, f64, f64),
}

mod imp {
    use super::*;
    use gdk::subclass::prelude::PaintableImpl;
    use std::cell::RefCell;

    #[derive(Default)]
    pub struct PixelPaintable {
        pub texture: RefCell<Option<gdk::Texture>>,

        pub base: RefCell<Option<gdk::Texture>>,
        pub placement: std::cell::Cell<Option<Placement>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for PixelPaintable {
        const NAME: &'static str = "NumaPixelPaintable";
        type Type = super::PixelPaintable;
        type Interfaces = (gdk::Paintable,);
    }

    impl ObjectImpl for PixelPaintable {}

    impl PaintableImpl for PixelPaintable {
        fn intrinsic_width(&self) -> i32 {
            match self.placement.get() {
                Some(placement) => placement.frame.0 as i32,
                None => self.texture.borrow().as_ref().map_or(0, |t| t.width()),
            }
        }

        fn intrinsic_height(&self) -> i32 {
            match self.placement.get() {
                Some(placement) => placement.frame.1 as i32,
                None => self.texture.borrow().as_ref().map_or(0, |t| t.height()),
            }
        }

        fn snapshot(&self, snapshot: &gdk::Snapshot, width: f64, height: f64) {
            let borrowed = self.texture.borrow();
            let Some(texture) = borrowed.as_ref() else { return };

            let Some(snapshot) = snapshot.downcast_ref::<gtk::Snapshot>() else {
                return;
            };

            if let Some(base) = self.base.borrow().as_ref() {
                if self.placement.get().is_some() {
                    snapshot.append_scaled_texture(
                        base,
                        gtk::gsk::ScalingFilter::Linear,
                        &gtk::graphene::Rect::new(0.0, 0.0, width as f32, height as f32),
                    );
                }
            }

            let (target, covered) = match self.placement.get() {
                Some(Placement { frame, tile }) if frame.0 > 0.0 && frame.1 > 0.0 => {
                    let (scale_x, scale_y) = (width / frame.0, height / frame.1);
                    (
                        gtk::graphene::Rect::new(
                            (tile.0 * scale_x) as f32,
                            (tile.1 * scale_y) as f32,
                            (tile.2 * scale_x) as f32,
                            (tile.3 * scale_y) as f32,
                        ),
                        tile.2,
                    )
                }
                _ => (
                    gtk::graphene::Rect::new(0.0, 0.0, width as f32, height as f32),
                    texture.width() as f64,
                ),
            };

            let magnifying = target.width() as f64 > covered * 1.01;
            let filter = if magnifying {
                gtk::gsk::ScalingFilter::Nearest
            } else {
                gtk::gsk::ScalingFilter::Linear
            };

            snapshot.append_scaled_texture(texture, filter, &target);
        }
    }
}

glib::wrapper! {
    pub struct PixelPaintable(ObjectSubclass<imp::PixelPaintable>) @implements gdk::Paintable;
}

impl Default for PixelPaintable {
    fn default() -> Self {
        glib::Object::new()
    }
}

impl PixelPaintable {
    pub fn new(texture: gdk::Texture) -> Self {
        let paintable = Self::default();
        paintable.set_texture(Some(texture));
        paintable
    }

    pub fn with_placement(
        texture: gdk::Texture,
        placement: Placement,
        base: Option<gdk::Texture>,
    ) -> Self {
        let paintable = Self::default();
        paintable.imp().placement.set(Some(placement));
        *paintable.imp().base.borrow_mut() = base;
        paintable.set_texture(Some(texture));
        paintable
    }

    pub fn set_texture(&self, texture: Option<gdk::Texture>) {
        let changed = {
            let imp = self.imp();

            let size = |imp: &imp::PixelPaintable| match imp.placement.get() {
                Some(placement) => Some((placement.frame.0 as i32, placement.frame.1 as i32)),
                None => imp.texture.borrow().as_ref().map(|t| (t.width(), t.height())),
            };
            let previous = size(imp);
            *imp.texture.borrow_mut() = texture;
            previous != size(imp)
        };

        if changed {
            self.invalidate_size();
        }
        self.invalidate_contents();
    }
}
