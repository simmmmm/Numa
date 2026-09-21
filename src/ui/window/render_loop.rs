use super::*;

pub(super) fn adjustments_changed(state: &App) {

    if state.applying.get() {
        return;
    }

    if state.mask_overlay.show_coverage.replace(false) {
        state.mask_overlay.area.queue_draw();
    }

    if let Some(photo) = state.open.borrow_mut().as_mut() {
        photo.before_preset = None;
    }

    sync_document(state);
    request_render(state);
    schedule_history_push(state);
    refresh_slider_marks(state);
}

pub(super) fn sync_document(state: &App) {
    let mut open = state.open.borrow_mut();
    let Some(photo) = open.as_mut() else { return };

    if let Some(index) = state.mask_overlay.selected_mask.get() {

        if state.mask_overlay.sliders_hold.get() != Some(index) {
            log::warn!("the sliders hold {:?} while mask {index} is selected: not written", state.mask_overlay.sliders_hold.get());
            drop(open);
            let state = state.clone();
            glib::idle_add_local_once(move || select_mask(&state, Some(index)));
            return;
        }
        let mut masks = photo.document.masks();
        if let Some(mask) = masks.get_mut(index) {

            mask.basic = state.sliders.read();
            mask.basic.balance.temperature = state.colour.mask_temperature.value() as f32;
            mask.basic.balance.tint = -(state.colour.mask_tint.value() as f32);
            photo.document.set_masks(masks);
            photo.view = None;
            return;
        }
        state.mask_overlay.selected_mask.set(None);
    }

    if let Some(held) = state.mask_overlay.sliders_hold.get() {
        log::warn!("the sliders hold mask {held} while the photograph is selected: not written");
        drop(open);
        let state = state.clone();
        glib::idle_add_local_once(move || select_mask(&state, None));
        return;
    }

    photo.document.set_basic(state.sliders.read());

    let balance = state.sliders.white_balance();
    photo.document.white_balance = (balance != photo.as_shot).then_some(balance);
}

pub(super) fn request_render(state: &App) {

    state.render.drafting.set(true);
    settle_render(state);

    if state.render.render_pending.replace(true) {
        return;
    }

    let state = state.clone();
    state.canvas.clone().add_tick_callback(move |_, _| {
        state.render.render_pending.set(false);
        render_current(&state);
        glib::ControlFlow::Break
    });
}

pub(super) fn colour_key(document: &Document) -> ColourKey {
    (document.white_balance, document.colour_profile.clone(), document.ai_denoise, document.ai_sharpen)
}

pub(super) type ColourKey = (Option<WhiteBalance>, Option<String>, f32, f32);

pub(super) fn proxy_runs_out_at(photo: &OpenPhoto) -> f64 {
    let full = photo.full_size.0.max(photo.full_size.1) as f64;
    let proxy = photo.proxy.width.max(photo.proxy.height) as f64;
    if full <= 0.0 || proxy <= 0.0 {
        return 1.0;
    }
    proxy / full
}

pub(super) fn covers(outer: [f32; 4], inner: [f32; 4]) -> bool {
    inner[0] >= outer[0] - 1e-4
        && inner[1] >= outer[1] - 1e-4
        && inner[0] + inner[2] <= outer[0] + outer[2] + 1e-4
        && inner[1] + inner[3] <= outer[1] + outer[3] + 1e-4
}

pub(super) fn render_current(state: &App) {

    let zoom = state.zooming.level.get();
    let Some((frame_width, frame_height)) = displayed_size(state) else { return };
    let visible = visible_rect(state).unwrap_or([0.0, 0.0, 1.0, 1.0]);
    let wanted = tile_for(state);

    let mut open = state.open.borrow_mut();
    let Some(photo) = open.as_mut() else { return };

    let key = colour_stage(state, photo);

    let document = rendered_document(state, photo);

    let wants_full = zoom > proxy_runs_out_at(photo);
    let have_full = photo.full_working.is_some() && photo.full_working_key.as_ref() == Some(&key);

    let region = region_to_render(&document, wanted, frame_width, frame_height);

    let needed = (region[2] as f64 * frame_width as f64 * zoom)
        .max(region[3] as f64 * frame_height as f64 * zoom)
        .ceil()
        .clamp(1.0, u32::MAX as f64) as u32;

    let geometry = geometry_of_document(&document);
    let reusable = photo.view.as_ref().is_some_and(|view| {
        view.key == key
            && view.geometry == geometry
            && view.edge >= needed
            && covers(view.rect, visible)
    });

    if wants_full && have_full && !reusable {
        cut_view_tile(photo, &document, &key, geometry, region, needed);
    }

    let usable_view = wants_full
        && have_full
        && photo.view.as_ref().is_some_and(|view| {
            view.key == key && view.geometry == geometry && covers(view.rect, visible)
        });

    let proxy_scale = photo.proxy.width.max(photo.proxy.height) as f32
        / photo.full_size.0.max(photo.full_size.1).max(1) as f32;

    let (rendered, placement, whole_frame) = match (usable_view, &photo.view) {
        (true, Some(view)) => render_view_tile(&document, view, frame_width, frame_height),

        (_, _) if wants_full && have_full => {
            let full = photo.full_working.as_ref().expect("checked by have_full");
            (render::apply_stack(&document, full, 1.0), None, true)
        }

        _ => (render_proxy(state, photo, &document, proxy_scale), None, true),
    };

    state.render.rendered_from_full.set(wants_full && have_full);
    state.render.tile.set((!whole_frame).then_some(
        photo.view.as_ref().map_or([0.0, 0.0, 1.0, 1.0], |view| view.rect),
    ));

    let (backdrop, histogram) = whole_frame_behind(state, photo, &document, whole_frame, proxy_scale, &rendered, &key);

    state.render.rendered_size.set(rendered.dimensions());
    drop(open);

    present(state, rendered, placement, backdrop, histogram, wants_full, have_full);
}

fn region_to_render(
    document: &Document,
    wanted: Option<[f32; 4]>,
    frame_width: u32,
    frame_height: u32,
) -> [f32; 4] {

    render::tiles_cleanly(document)
        .then_some(wanted)
        .flatten()

        .filter(|rect| {
            render::spots_within(document, *rect, [frame_width as f32, frame_height as f32])
        })
        .unwrap_or([0.0, 0.0, 1.0, 1.0])
}

fn cut_view_tile(
    photo: &mut OpenPhoto,
    document: &Document,
    key: &ColourKey,
    geometry: Geometry,
    region: [f32; 4],
    needed: u32,
) {
    if let Some(within) = render::tile_in_source(document, region) {
        let full = photo.full_working.as_ref().expect("checked by have_full");
        let cut = full.cropped(within, 0.0, Default::default());
        let image = cut.downscaled(needed).unwrap_or(cut);
        photo.view = Some(ViewTile {
            rect: region,
            key: key.clone(),
            geometry,
            edge: needed,
            image,
        });
    } else {

        photo.view = None;
    }
}

fn render_view_tile(
    document: &Document,
    view: &ViewTile,
    frame_width: u32,
    frame_height: u32,
) -> (image::RgbImage, Option<crate::ui::pixel_paintable::Placement>, bool) {
    let covered = (view.rect[2] * frame_width as f32).max(1.0);
    let detail_scale = view.image.width as f32 / covered;

    let rendered = render::apply_pixels(document, &view.image, detail_scale, view.rect);
    let placement = crate::ui::pixel_paintable::Placement {
        frame: (frame_width as f64, frame_height as f64),
        tile: (
            (view.rect[0] * frame_width as f32) as f64,
            (view.rect[1] * frame_height as f32) as f64,
            (view.rect[2] * frame_width as f32) as f64,
            (view.rect[3] * frame_height as f32) as f64,
        ),
    };
    let whole = view.rect == [0.0, 0.0, 1.0, 1.0];
    (rendered, Some(placement), whole)
}

fn render_proxy(
    state: &App,
    photo: &mut OpenPhoto,
    document: &Document,
    proxy_scale: f32,
) -> image::RgbImage {

    if state.render.drafting.get() && photo.draft.is_none() {
        let half = photo.working.width.max(photo.working.height) / 2;
        photo.draft = photo.working.downscaled(half as u32);
    }
    let draft = state.render.drafting.get().then(|| photo.draft.as_ref()).flatten();
    match draft {
        Some(small) => {
            let scale = proxy_scale * small.width as f32
                / photo.working.width.max(1) as f32;
            render::apply_stack(document, small, scale)
        }
        None => render::apply_stack(document, &photo.working, proxy_scale),
    }
}

fn present(
    state: &App,
    mut rendered: image::RgbImage,
    placement: Option<crate::ui::pixel_paintable::Placement>,
    backdrop: Option<image::RgbImage>,
    histogram: render::histogram::Histogram,
    wants_full: bool,
    have_full: bool,
) {

    write_zoom_label(state);

    if wants_full && !have_full {
        ensure_full_resolution(state);
    }

    state.info.shadow_clip.set_visible(histogram.is_shadow_clipped());
    state.info.highlight_clip.set_visible(histogram.is_highlight_clipped());
    *state.info.histogram.borrow_mut() = Some(histogram);
    state.info.histogram_area.queue_draw();

    if state.editor_page.before.is_active() {
        return;
    }

    render::histogram::mark_clipping(
        &mut rendered,
        render::histogram::ClippingOverlay {
            shadows: state.info.shadow_clip.is_active(),
            highlights: state.info.highlight_clip.is_active(),
        },
    );

    show(state, rendered, placement, backdrop);

    refresh_info(state);
}

pub(super) fn schedule_history_push(state: &App) {

    schedule_save(state);

    let generation = state.render.history_generation.get().wrapping_add(1);
    state.render.history_generation.set(generation);

    let state = state.clone();
    glib::timeout_add_local_once(std::time::Duration::from_millis(400), move || {
        if state.render.history_generation.get() != generation {
            return;
        }
        if let Some(photo) = state.open.borrow_mut().as_mut() {
            let snapshot = EditState::of(&photo.document);
            photo.history.push(snapshot);
        }
    });
}

pub(super) fn step_history(state: &App, redo: bool) {
    let stepped = state.open.borrow_mut().as_mut().and_then(|photo| {

        let snapshot = EditState::of(&photo.document);
        photo.history.push(snapshot);
        let stepped = if redo { photo.history.redo() } else { photo.history.undo() };

        stepped.map(|state| (state, photo.as_shot))
    });

    let Some((edit, as_shot)) = stepped else {
        state.toast(if redo { "Nothing to redo" } else { "Nothing to undo" });
        return;
    };

    apply_history(state, edit, as_shot);
}

pub(super) fn jump_history(state: &App, position: usize) {
    let stepped = state.open.borrow_mut().as_mut().and_then(|photo| {

        let snapshot = EditState::of(&photo.document);
        photo.history.push(snapshot);
        photo.history.go_to(position).map(|state| (state, photo.as_shot))
    });

    let Some((edit, as_shot)) = stepped else { return };
    apply_history(state, edit, as_shot);
}

pub(super) fn apply_history(state: &App, edit: EditState, as_shot: WhiteBalance) {
    state.applying.set(true);
    state.sliders.write(edit.basic);
    state.mask_overlay.sliders_hold.set(None);
    state.sliders.write_white_balance(edit.white_balance.unwrap_or(as_shot));
    refresh_slider_marks(state);

    let angle = edit.crop.map_or(0.0, |(_, angle)| angle);
    state.crop.straighten.set_value(angle as f64);
    state.applying.set(false);

    {
        let mut open = state.open.borrow_mut();
        if let Some(photo) = open.as_mut() {
            let space_moved = photo.document.working_space != edit.working_space;
            edit.restore(&mut photo.document);
            if space_moved {
                photo.inputs = render_inputs(&photo.document);
                photo.working = render::to_working_space(&photo.document, &photo.proxy, &photo.inputs);
                photo.full_working = None;
                photo.full_working_key = None;
                photo.draft = None;
            }
            photo.view = None;
        }
    }
    state.crop.rect.set(tool_rect(state).0);
    state.crop.fit_base.set(None);
    write_mixer(state);
    write_point_colours(state);
    write_grading(state);
    write_perspective(state);
    ai_denoise::write(state);
    refresh_retouch(state);
    refresh_face(state);
    refresh_found(state);
    refresh_masks(state);
    select_mask(state, None);
    refresh_profile_picker(state);
    state.crop.area.queue_draw();
    state.light.curve_area.queue_draw();

    sync_document(state);
    request_render(state);

    refresh_history(state);
}

pub(super) fn copy_image(state: &App) {
    let job = state.open.borrow().as_ref().map(|photo| {
        let document = photo.document.clone();
        let working = photo.working.clone();
        let scale = working.width.max(working.height) as f32 / photo.full_size.0.max(photo.full_size.1).max(1) as f32;
        (document, working, scale)
    });
    let Some((document, working, scale)) = job else { return };
    let state = state.clone();
    glib::spawn_future_local(async move {
        let Ok(image) = busy(&state, "Copying the picture…", move || {
            let document = render::with_masks_resolved(&document, &working);
            render::apply_stack(&document, &working, scale)
        })
        .await
        else {
            return;
        };
        if let Some(display) = gtk::gdk::Display::default() {

            display.clipboard().set_texture(&crate::ui::display::unconverted(image));
            state.toast("Picture copied");
        }
    });
}

pub(super) fn texture_from(image: image::RgbImage) -> gtk::gdk::Texture {
    crate::ui::display::texture(image)
}

pub(super) fn show(
    state: &App,
    image: image::RgbImage,
    placement: Option<crate::ui::pixel_paintable::Placement>,
    backdrop: Option<image::RgbImage>,
) {
    let texture = texture_from(image);
    let paintable = match placement {
        Some(placement) => crate::ui::pixel_paintable::PixelPaintable::with_placement(
            texture,
            placement,
            backdrop.map(texture_from),
        ),
        None => crate::ui::pixel_paintable::PixelPaintable::new(texture),
    };
    state.canvas.set_paintable(Some(&paintable));
    apply_zoom(state);
}

#[derive(Clone)]
pub(super) struct State {

    pub(super) rendered_size: Rc<Cell<(u32, u32)>>,

    pub(super) full_resolution_stale: Rc<Cell<bool>>,

    pub(super) failed_key: Rc<RefCell<Option<ColourKey>>>,

    pub(super) loading_full: Rc<Cell<bool>>,

    pub(super) tile: Rc<Cell<Option<[f32; 4]>>>,

    pub(super) rendered_from_full: Rc<Cell<bool>>,

    pub(super) recentring: Rc<Cell<bool>>,

    pub(super) zoom_generation: Rc<Cell<u64>>,

    pub(super) history_generation: Rc<Cell<u64>>,

    pub(super) save_generation: Rc<Cell<u64>>,

    pub(super) render_pending: Rc<Cell<bool>>,

    pub(super) drafting: Rc<Cell<bool>>,
    pub(super) settle_generation: Rc<Cell<u64>>,
}

impl State {
    pub(super) fn new() -> Self {
        Self {
            rendered_size: Rc::new(Cell::new((0, 0))),
            full_resolution_stale: Rc::new(Cell::new(false)),
            failed_key: Rc::new(RefCell::new(None)),
            loading_full: Rc::new(Cell::new(false)),
            tile: Rc::new(Cell::new(None)),
            rendered_from_full: Rc::new(Cell::new(false)),
            recentring: Rc::new(Cell::new(false)),
            zoom_generation: Rc::new(Cell::new(0)),
            history_generation: Rc::new(Cell::new(0)),
            save_generation: Rc::new(Cell::new(0)),
            render_pending: Rc::new(Cell::new(false)),
            drafting: Rc::new(Cell::new(false)),
            settle_generation: Rc::new(Cell::new(0)),
        }
    }
}
