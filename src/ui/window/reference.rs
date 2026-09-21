use super::*;
use crate::ui::pixel_paintable::{PixelPaintable, Placement};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Held {
    Nothing,

    Frame,

    Camera,
}

#[derive(Clone)]
pub(super) struct State {
    pub(super) button: gtk::ToggleButton,

    pub(super) camera_button: gtk::ToggleButton,
    pub(super) pane: gtk::Box,
    pub(super) picture: gtk::Picture,
    pub(super) caption: gtk::Label,
    pub(super) held: Rc<Cell<Held>>,

    pub(super) texture: Rc<RefCell<Option<gtk::gdk::Texture>>>,

    pub(super) switching: Rc<Cell<bool>>,

    pub(super) pending: Rc<Cell<bool>>,

    pub(super) generation: Rc<Cell<u64>>,
}

impl State {
    pub(super) fn new() -> Self {
        Self {
            button: gtk::ToggleButton::new(),
            camera_button: gtk::ToggleButton::new(),
            pane: gtk::Box::new(gtk::Orientation::Vertical, 6),
            picture: gtk::Picture::new(),
            caption: gtk::Label::new(None),
            held: Rc::new(Cell::new(Held::Nothing)),
            texture: Rc::new(RefCell::new(None)),
            switching: Rc::new(Cell::new(false)),
            pending: Rc::new(Cell::new(false)),
            generation: Rc::new(Cell::new(0)),
        }
    }
}

pub(super) fn build_reference_pane(state: &App) -> gtk::Box {
    let pane = state.reference.pane.clone();
    pane.set_visible(false);
    pane.add_css_class("reference-pane");
    pane.set_hexpand(true);

    state.reference.picture.set_vexpand(true);
    state.reference.picture.set_hexpand(true);
    state.reference.picture.set_can_shrink(true);
    state.reference.picture.set_content_fit(gtk::ContentFit::Contain);
    pane.append(&state.reference.picture);

    state.reference.caption.add_css_class("reference-caption");
    state.reference.caption.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
    state.reference.caption.set_margin_bottom(6);
    state.reference.caption.set_halign(gtk::Align::Center);
    pane.append(&state.reference.caption);

    for adjustment in [state.zooming.scroller.hadjustment(), state.zooming.scroller.vadjustment()] {
        adjustment.connect_value_changed(glib::clone!(
            #[strong] state,
            move |_| queue_reference(&state)
        ));
    }

    pane
}

pub(super) fn build_reference_picker(state: &App) -> gtk::Box {
    let sources = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    sources.add_css_class("linked");
    sources.set_margin_end(8);

    let reference = state.reference.button.clone();
    reference.set_label("Reference");
    reference.set_tooltip_text(Some("Keep this frame beside the next ones, to match them to it"));
    sources.append(&reference);

    let camera = state.reference.camera_button.clone();
    camera.set_label("Camera");
    camera.set_tooltip_text(Some(
        "The camera's own rendering of this frame beside yours, at the same zoom",
    ));
    sources.append(&camera);

    for (button, show) in [(reference, set_reference as fn(&App)), (camera, show_camera)] {
        button.connect_toggled(glib::clone!(
            #[strong] state,
            move |button| match (state.reference.switching.get(), button.is_active()) {
                (true, _) => (),
                (false, true) => show(&state),
                (false, false) => clear_reference(&state),
            }
        ));
    }

    sources
}

pub(super) fn set_reference(state: &App) {

    let rendered = {
        let mut open = state.open.borrow_mut();
        open.as_mut().map(|photo| {
            let name = match &photo.source {
                Source::Photo { path, .. } => {
                    path.file_name().map(|name| name.to_string_lossy().into_owned())
                }
                Source::Bracket { paths } => Some(format!("Merge of {} frames", paths.len())),
            };

            let key = colour_key(&photo.document);
            if key != photo.working_key {
                photo.inputs = render_inputs(&photo.document);
                photo.working =
                    render::to_working_space(&photo.document, &photo.proxy, &photo.inputs);
                photo.working_key = key;
                photo.draft = None;
            }
            let scale = photo.proxy.width.max(photo.proxy.height) as f32
                / photo.full_size.0.max(photo.full_size.1).max(1) as f32;
            let document = render::with_masks_resolved(&photo.document, &photo.working);
            (render::apply_stack(&document, &photo.working, scale), name)
        })
    };
    let Some((frame, name)) = rendered else {
        only_active(state, None);
        return;
    };

    state.reference.held.set(Held::Frame);
    only_active(state, Some(&state.reference.button));
    write_zoom_label(state);

    *state.reference.texture.borrow_mut() = Some(texture_from(frame));
    queue_reference(state);
    state.reference.caption.set_text(&match name {
        Some(name) => format!("Reference \u{00b7} {name}"),
        None => "Reference".to_string(),
    });
    state.reference.pane.set_visible(true);
    state.toast("This frame is the reference \u{2014} step to another to compare");
}

pub(super) fn clear_reference(state: &App) {
    state.reference.held.set(Held::Nothing);
    state.reference.pane.set_visible(false);
    state.reference.picture.set_paintable(gtk::gdk::Paintable::NONE);
    *state.reference.texture.borrow_mut() = None;

    only_active(state, None);
    write_zoom_label(state);
}

fn only_active(state: &App, wanted: Option<&gtk::ToggleButton>) {
    state.reference.switching.set(true);
    for button in [&state.reference.button, &state.reference.camera_button] {
        button.set_active(wanted.is_some_and(|it| it == button));
    }
    state.reference.switching.set(false);
}

pub(super) fn show_camera(state: &App) {
    let Some(path) = raw_path(state) else {
        state.toast("A merge has no camera rendering of its own");
        only_active(state, None);
        return;
    };

    state.reference.held.set(Held::Camera);
    only_active(state, Some(&state.reference.camera_button));
    state.reference.pane.set_visible(true);
    state.reference.caption.set_text(&format!(
        "Camera \u{00b7} {}",
        path.file_name().map_or_else(String::new, |name| name.to_string_lossy().into_owned())
    ));

    if state.reference.texture.borrow().is_some() {
        queue_reference(state);
        return;
    }

    state.reference.picture.set_paintable(gtk::gdk::Paintable::NONE);
    let generation = state.reference.generation.get();
    glib::spawn_future_local(glib::clone!(
        #[strong] state,
        async move {
            let found = gio::spawn_blocking(move || raw::embedded_preview(&path)).await;
            if state.reference.generation.get() != generation {
                return;
            }
            match found {
                Ok(Ok(Some(image))) => {
                    *state.reference.texture.borrow_mut() = Some(texture_from(image.into_rgb8()));
                    queue_reference(&state);
                    write_zoom_label(&state);
                }
                _ => {
                    state.toast("This file carries no camera rendering");
                    clear_reference(&state);
                }
            }
        }
    ));
}

fn raw_path(state: &App) -> Option<PathBuf> {
    match &state.open.borrow().as_ref()?.source {
        Source::Photo { path, .. } => Some(path.clone()),
        Source::Bracket { .. } => None,
    }
}

pub(super) fn queue_reference(state: &App) {
    if state.reference.held.get() == Held::Nothing || state.reference.pending.replace(true) {
        return;
    }
    glib::idle_add_local_once(glib::clone!(
        #[strong] state,
        move || {
            state.reference.pending.set(false);
            place_reference(&state);
        }
    ));
}

pub(super) fn follow_photo(state: &App) {
    state.reference.generation.set(state.reference.generation.get().wrapping_add(1));
    match state.reference.held.get() {
        Held::Camera => {
            *state.reference.texture.borrow_mut() = None;
            show_camera(state);
        }
        Held::Frame => queue_reference(state),
        Held::Nothing => (),
    }
}

fn place_reference(state: &App) {
    let Some(texture) = state.reference.texture.borrow().clone() else { return };

    if state.zooming.level.get() == FIT_ZOOM {
        state.reference.picture.set_paintable(Some(&PixelPaintable::new(texture)));
        return;
    }

    let placement = visible_rect(state).and_then(|visible| {
        let pane =
            (state.reference.picture.width() as f64, state.reference.picture.height() as f64);
        let image = (texture.width() as f64, texture.height() as f64);
        let scale = effective_zoom(state) / device_scale(state);
        match state.reference.held.get() {

            Held::Camera => {
                let open = state.open.borrow();
                let photo = open.as_ref()?;
                let frame = (photo.full_size.0 as f64, photo.full_size.1 as f64);
                let source = render::tile_in_source(&photo.document, visible)?;
                pane_window(pane, image, frame, scale, source)
            }

            Held::Frame => {
                let frame = displayed_size(state)?;
                pane_window(pane, image, (frame.0 as f64, frame.1 as f64), scale, visible)
            }
            Held::Nothing => None,
        }
    });

    let paintable = match placement {
        Some(placement) => PixelPaintable::with_placement(texture, placement, None),
        None => PixelPaintable::new(texture),
    };
    state.reference.picture.set_paintable(Some(&paintable));
}

fn pane_window(
    pane: (f64, f64),
    image: (f64, f64),
    frame: (f64, f64),
    scale: f64,
    region: [f32; 4],
) -> Option<Placement> {
    let sane = [pane.0, pane.1, image.0, image.1, frame.0, frame.1, scale];
    if sane.iter().any(|value| *value <= 0.0) {
        return None;
    }

    let per_pixel = scale * frame.0 / image.0;
    let window = (pane.0 / per_pixel, pane.1 / per_pixel);
    let centre = (
        (region[0] + region[2] / 2.0) as f64 * image.0,
        (region[1] + region[3] / 2.0) as f64 * image.1,
    );

    Some(Placement {
        frame: window,
        tile: (window.0 / 2.0 - centre.0, window.1 / 2.0 - centre.1, image.0, image.1),
    })
}

pub(super) fn camera_note(state: &App) -> String {
    if state.reference.held.get() != Held::Camera {
        return String::new();
    }
    let borrowed = state.reference.texture.borrow();
    let Some(camera) = borrowed.as_ref() else { return String::new() };
    let open = state.open.borrow();
    let Some(photo) = open.as_ref() else { return String::new() };

    let (long, sensor) = (
        camera.width().max(camera.height()) as f64,
        photo.full_size.0.max(photo.full_size.1).max(1) as f64,
    );
    match long >= sensor * 0.99 {
        true => " \u{00b7} camera".to_string(),
        false => format!(" \u{00b7} camera preview {long:.0} px, \u{00d7}{:.2}", sensor / long),
    }
}

#[cfg(test)]
mod tests {
    use super::pane_window;

    #[test]
    fn the_camera_frame_meets_the_canvas() {

        let pane = (470.0, 900.0);
        let camera = (2944.0, 4416.0);
        let sensor = (5152.0, 7728.0);
        let placement = pane_window(pane, camera, sensor, 1.0, [0.5, 0.25, 0.5, 0.25]).unwrap();

        let drawn = pane.0 / placement.frame.0;
        assert!((drawn - sensor.0 / camera.0).abs() < 1e-9, "{drawn}");

        assert!((drawn * camera.0 / sensor.0 - 1.0).abs() < 1e-9);

        let (centre_x, centre_y) = (
            (placement.tile.0 + 0.75 * camera.0) * drawn,
            (placement.tile.1 + 0.375 * camera.1) * drawn,
        );
        assert!((centre_x - pane.0 / 2.0).abs() < 1e-9, "{centre_x}");
        assert!((centre_y - pane.1 / 2.0).abs() < 1e-9, "{centre_y}");
    }

    #[test]
    fn a_reference_photograph_shows_the_same_fraction_of_its_own_frame() {
        let pane = (470.0, 900.0);
        let region = [0.4, 0.1, 0.2, 0.3];
        let scale = 1.0;

        for (image, frame) in
            [((1600.0, 2400.0), (5152.0, 7728.0)), ((2400.0, 1600.0), (6000.0, 4000.0))]
        {
            let placement = pane_window(pane, image, frame, scale, region).unwrap();
            let drawn = pane.0 / placement.frame.0;

            assert!((image.0 * drawn - frame.0 * scale).abs() < 1e-6, "{image:?}");

            let middle = (placement.tile.0 + (region[0] + region[2] / 2.0) as f64 * image.0) * drawn;
            assert!((middle - pane.0 / 2.0).abs() < 1e-9, "{middle}");
        }
    }

    #[test]
    fn nothing_to_place_before_the_pane_has_a_size() {
        let full = [0.0, 0.0, 1.0, 1.0];
        assert!(pane_window((0.0, 900.0), (2944.0, 4416.0), (5152.0, 7728.0), 1.0, full).is_none());
        assert!(pane_window((470.0, 900.0), (2944.0, 4416.0), (5152.0, 7728.0), 0.0, full).is_none());
    }
}
