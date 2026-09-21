use super::*;

pub(super) fn begin_open(state: &App) -> u64 {
    state.render.tile.set(None);
    *state.render.failed_key.borrow_mut() = None;
    state.render.full_resolution_stale.set(false);
    let generation = state.open_generation.get().wrapping_add(1);
    state.open_generation.set(generation);
    generation
}

pub(super) fn rendered_document(state: &App, photo: &OpenPhoto) -> Document {
    let mut document = photo.document.clone();

    if state.colour.point_show.is_active() {
        let mut points = document.point_colours();
        points.highlight = Some(state.colour.point_selected.get());
        document.set_point_colours(points);
    }
    if is_cropping(state) {

        let (rect, angle) = document.crop().unwrap_or(([0.0, 0.0, 1.0, 1.0], 0.0));
        document.set_crop([0.0, 0.0, 1.0, 1.0], angle);

        let (width, height) = oriented_pixels(photo);
        let made = match state.crop.at_open.get() {
            Some((made, made_angle, turned)) if turned == document.rotation() => Some((made, made_angle)),
            Some(_) => None,
            None => Some((rect, angle)),
        };
        match made {
            Some((made, made_angle)) => {
                document.masks_map = Some(numa::core::image::view_to_crop(
                    width,
                    height,
                    made,
                    made_angle,
                    angle,
                    document.perspective(),
                ))
            }

            None => document.set_masks(Vec::new()),
        }
    }
    document
}

pub(super) fn visible_rect(state: &App) -> Option<[f32; 4]> {
    let bounds = state.canvas.compute_bounds(&state.zooming.scroller)?;
    let (shown_width, shown_height) = (bounds.width() as f64, bounds.height() as f64);
    if shown_width <= 0.0 || shown_height <= 0.0 {
        return None;
    }

    let left = (-bounds.x() as f64).clamp(0.0, shown_width);
    let top = (-bounds.y() as f64).clamp(0.0, shown_height);
    let right =
        (state.zooming.scroller.width() as f64 - bounds.x() as f64).clamp(left, shown_width);
    let bottom =
        (state.zooming.scroller.height() as f64 - bounds.y() as f64).clamp(top, shown_height);

    Some([
        (left / shown_width) as f32,
        (top / shown_height) as f32,
        ((right - left) / shown_width) as f32,
        ((bottom - top) / shown_height) as f32,
    ])
}

pub(super) fn tile_for(state: &App) -> Option<[f32; 4]> {

    const MARGIN: f32 = 0.5;

    let [x, y, width, height] = visible_rect(state)?;
    if width >= 0.9 && height >= 0.9 {
        return None;
    }

    let grow = |start: f32, length: f32| {
        let low = (start - length * MARGIN).max(0.0);
        let high = (start + length * (1.0 + MARGIN)).min(1.0);
        (low, high - low)
    };
    let (left, tile_width) = grow(x, width);
    let (top, tile_height) = grow(y, height);

    (tile_width > 0.0 && tile_height > 0.0).then_some([left, top, tile_width, tile_height])
}

pub(super) fn displayed_size(state: &App) -> Option<(u32, u32)> {
    let open = state.open.borrow();
    let photo = open.as_ref()?;
    let document = rendered_document(state, photo);

    let (mut width, mut height) = photo.full_size;
    if matches!(document.rotation() as i32, 90 | 270) {
        std::mem::swap(&mut width, &mut height);
    }
    if let Some(([_, _, crop_width, crop_height], _)) = document.crop() {

        width = ((width as f32 * crop_width).round() as u32).max(1);
        height = ((height as f32 * crop_height).round() as u32).max(1);
    }
    Some((width, height))
}

pub(super) fn open_in_editor(state: &App, child: &impl IsA<gtk::Widget>) {
    let Ok(id) = child.widget_name().parse::<i64>() else { return };
    open_photo(state, id);
}

pub(super) fn step_photo(state: &App, forward: bool) {
    let current = state.open.borrow().as_ref().and_then(|photo| match &photo.source {
        Source::Photo { id, .. } => Some(*id),
        Source::Bracket { .. } => None,
    });
    let Some(current) = current else { return };

    let next = {
        let order = state.grid.order.borrow();
        let Some(at) = order.iter().position(|id| *id == current) else { return };
        let step = if forward { at + 1 } else { at.checked_sub(1).unwrap_or(usize::MAX) };
        order.get(step).copied()
    };

    match next {
        Some(id) => open_photo(state, id),

        None => state.toast(if forward { "Last photo" } else { "First photo" }),
    }
}

pub(super) fn write_panel_for(state: &App, rating: u8, flag: Flag) {
    write_rating_button(state, rating, flag);
    write_mixer(state);
    write_point_colours(state);
    write_grading(state);
    write_perspective(state);
    refresh_retouch(state);
    refresh_face(state);
    refresh_found(state);
}

pub(super) fn open_photo(state: &App, id: i64) {
    let Some((photo, _)) = state.grid.cards.borrow().get(&id).cloned() else { return };

    save_open_edits(state);

    clear_editor_for(state, id, &photo);

    let (document, edits_unreadable) = stored_document(state, &photo);

    let generation = begin_open(state);
    let state = state.clone();
    let path = photo.path.clone();
    let ai_denoised = (document.ai_denoise > 0.0, document.ai_sharpen > 0.0);
    let edge = proxy_edge(&state);
    glib::spawn_future_local(async move {
        let decoded = busy(&state, "Opening…", move || decode_for_open(path, edge, ai_denoised)).await;

        if state.open_generation.get() != generation {
            return;
        }

        let (proxy, full_size, summary, lens_corrected) = match decoded {
            Ok(Ok(proxy)) => proxy,
            Ok(Err(err)) => {
                state.toast(&format!("Could not open: {err}"));
                close_editor(&state);
                return;
            }
            Err(_) => {
                state.toast("Decoding was cancelled");
                close_editor(&state);
                return;
            }
        };

        refresh_profile_picker(&state);

        let basic = document.basic();

        let as_shot = proxy
            .profile
            .map(|profile| profile.as_shot_white_balance())
            .unwrap_or(WhiteBalance { temperature: 5500.0, tint: 0.0 });
        let document_balance = document.white_balance;
        let balance = document_balance.unwrap_or(as_shot);

        let adjustable = proxy.profile.is_some();
        state.sliders.balance.temperature.set_sensitive(adjustable);
        state.sliders.balance.tint.set_sensitive(adjustable);

        let working_key = colour_key(&document);
        let inputs = render_inputs(&document);
        let working = render::to_working_space(&document, &proxy, &inputs);

        *state.open.borrow_mut() = Some(OpenPhoto {
            source: Source::Photo { id: photo.id, path: photo.path.clone() },
            inputs,
            summary,
            lens_corrected,
            segmentation: None,
            mask_frame: None,
            embedding: None,
            embedding_pending: false,
            faces_pending: false,
            people: Vec::new(),
            segmenting: false,
            animal: None,
            draft: None,
            full_size,
            proxy,
            working,
            working_key,
            full_working: None,
            full_working_key: None,
            view: None,
            behind: None,
            baseline: None,
            before_preset: None,
            history: resumed_history(&state, &photo, &document),
            document,
            as_shot,
            edits_unreadable,
        });

        write_opened_sliders(&state, basic, balance);
        write_rest_of_panel(&state);
    });
}

fn clear_editor_for(state: &App, id: i64, photo: &Photo) {

    let entering = state.open.borrow().is_none();
    state.canvas.set_paintable(gtk::gdk::Paintable::NONE);
    state.editor_page.before.set_active(false);

    state.overlays.face_names.borrow_mut().clear();
    leave_crop(state);
    state.render.loading_full.set(false);
    *state.open.borrow_mut() = None;
    startup::show_editor(state);
    state.zooming.level.set(FIT_ZOOM);
    state.zooming.label.set_text("…");

    if entering {
        build_filmstrip(state);
    }
    mark_filmstrip(state, id);
    write_panel_for(state, photo.rating, photo.flag);

    state.mask_overlay.selected_mask.set(None);
}

fn stored_document(state: &App, photo: &Photo) -> (Document, bool) {

    let loaded = state.catalog.load_edits(photo.id);

    let edits_unreadable = loaded.is_err();
    if let Err(err) = &loaded {
        log::warn!("{}: could not read the stored edits: {err}", photo.path.display());
        state.toast("This photograph's stored edits could not be read — opening without them");
    }
    let document = loaded
        .ok()
        .flatten()
        .unwrap_or_else(|| Document::new(photo.path.to_string_lossy().to_string()));
    (document, edits_unreadable)
}

fn decode_for_open(
    path: PathBuf,
    edge: u32,
    (ai_denoised, ai_sharpened): (bool, bool),
) -> Result<(LinearImage, (u32, u32), Option<raw::Summary>, bool), String> {
    let linear = raw::decode_for_editing(&path)?;

    let (full_size, proxy) =
        ((linear.width, linear.height), linear.downscaled(edge).unwrap_or(linear));

    if ai_denoised {
        if let Some(stored) = numa::io::denoised::load(&path) {
            render::ai_denoise::warm(&path, &stored, proxy.width, proxy.height);
        }
    }

    if ai_sharpened {
        if let Some(stored) = numa::io::denoised::load_sharpened(&path, ai_denoised) {
            render::ai_denoise::warm(&path, &stored, proxy.width, proxy.height);
        }
    }

    let corrected =
        raw::lens_profile(&path).is_some_and(|profile| profile.corrects_anything());
    Ok::<_, String>((proxy, full_size, raw::summary(&path), corrected))
}

fn resumed_history(state: &App, photo: &Photo, document: &Document) -> History {
    History::resumed(
        state
            .catalog
            .load_history(photo.id)
            .ok()
            .flatten()
            .map(|(states, position)| (states.iter().map(EditState::of).collect(), position)),
        EditState::of(document),
    )
}

fn write_opened_sliders(state: &App, basic: Basic, balance: WhiteBalance) {
    state.applying.set(true);
    state.sliders.write(basic);
    state.mask_overlay.sliders_hold.set(None);
    state.sliders.write_white_balance(balance);
    refresh_slider_marks(state);
    let (rect, angle) = tool_rect(state);
    state.crop.rect.set(rect);
    state.crop.straighten.set_value(angle as f64);
    state.applying.set(false);
    state.light.curve_area.queue_draw();
}

pub(super) fn write_rest_of_panel(state: &App) {

    if state.panel.stack.visible_child_name().as_deref() == Some("presets") {
        fill_presets_page(state);
    }
    write_mixer(state);
    write_point_colours(state);
    write_grading(state);
    write_perspective(state);
    write_lens(state);
    ai_denoise::write(state);
    refresh_retouch(state);
    refresh_face(state);
    refresh_found(state);
    refresh_masks(state);
    select_mask(state, None);
    refresh_crumbs(state);

    fill_segment_masks(state);

    let showing = state.panel.stack.visible_child_name();
    let showing = showing.as_deref().unwrap_or_default();
    if showing == "masks" || state.open.borrow().as_ref().is_some_and(pending_masks) {
        ensure_segmentation(state);
    }
    if showing == "retouch" {
        ensure_faces(state);
    }

    adjustments_changed(state);
    refresh_info(state);

    reference::follow_photo(state);
}

pub(super) fn close_editor(state: &App) {
    save_open_edits(state);

    clear_reference(state);
    if let Some(photo) = state.open.borrow().as_ref() {

        if matches!(photo.source, Source::Bracket { .. }) {
            state.toast("Merged image discarded — export it to keep it");
        }
    }
    begin_open(state);
    *state.open.borrow_mut() = None;
    state.stack.set_visible_child_name("library");
    if state.grid.stale.replace(false) {
        reload_grid(state);
    }
}
