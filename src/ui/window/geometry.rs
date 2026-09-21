use super::*;

pub(super) fn leave_crop(state: &App) {
    if is_cropping(state) {
        show_panel_tab(state, "light");
    }
}

pub(super) fn is_cropping(state: &App) -> bool {
    state.panel.stack.visible_child_name().as_deref() == Some("crop")
}

pub(super) fn toggle_crop(state: &App, active: bool) {
    if active {
        state.crop.at_open.set(geometry_now(state));
        state.crop.fit_base.set(None);
        let (rect, _) = tool_rect(state);
        state.crop.rect.set(rect);

        let lies_flat = match state.crop.ratio.get() {
            Some(ratio) => ratio >= 1.0,
            None => rect[2] * frame_aspect(state) >= rect[3],
        };
        state.crop.landscape.set(lies_flat);
        label_ratios(state);

        set_zoom(state, FIT_ZOOM);
        state.crop.area.set_visible(true);
        state.crop.area.queue_draw();
    } else {
        state.crop.area.set_visible(false);

        state.crop.guided.set_active(false);

        if geometry_now(state) != state.crop.at_open.get() {
            remake_masks(state);
        }
    }

    set_panel_scope(state);
    commit_crop(state);
}

pub(super) fn geometry_now(state: &App) -> Option<([f32; 4], f32, f32)> {
    let open = state.open.borrow();
    let photo = open.as_ref()?;
    let (rect, angle) = photo.document.crop().unwrap_or(([0.0, 0.0, 1.0, 1.0], 0.0));
    Some((rect, angle, photo.document.rotation()))
}

pub(super) fn remake_masks(state: &App) {

    {
        let mut open = state.open.borrow_mut();
        let Some(photo) = open.as_mut() else { return };
        photo.segmentation = None;
        photo.mask_frame = None;
        photo.segmenting = false;
    }

    {
        let mut open = state.open.borrow_mut();
        let Some(photo) = open.as_mut() else { return };
        let mut masks = photo.document.masks();
        if !masks.iter().any(Mask::wants_pixels) {
            return;
        }
        for mask in masks.iter_mut() {
            mask.map = Pixels(None);
        }
        photo.document.set_masks(masks);
        photo.view = None;
    }

    fill_segment_masks(state);
    if state
        .open
        .borrow()
        .as_ref()
        .is_some_and(|photo| photo.document.masks().iter().any(Mask::is_pending))
    {
        ensure_segmentation(state);
    }

    ensure_faces(state);
    request_render(state);
    state.mask_overlay.area.queue_draw();
}

pub(super) fn auto_perspective(state: &App) {
    let measured = {
        let open = state.open.borrow();
        let Some(photo) = open.as_ref() else { return };

        let mut framing = Document::new(photo.document.source.path.clone());
        framing.set_rotation(photo.document.rotation());
        framing.set_mirrored(photo.document.mirrored());
        if let Some((rect, angle)) = photo.document.crop() {
            framing.set_crop(rect, angle);
        }
        let framed = render::geometry_only(&framing, &photo.working);

        let small = framed.downscaled(AUTO_EDGE).unwrap_or(framed);
        let luma = numa::core::plane::Plane::new(
            small.width as usize,
            small.height as usize,
            small
                .data
                .chunks_exact(3)
                .map(|pixel| 0.2126 * pixel[0] + 0.7152 * pixel[1] + 0.0722 * pixel[2])
                .collect(),
        );
        let (mut perspective, angle) = render::auto::perspective(&luma);

        if let Some(([_, _, width, height], _)) = photo.document.crop() {
            perspective.vertical /= height;
            perspective.horizontal /= width;
        }
        (perspective, angle)
    };

    let (perspective, angle) = measured;
    if perspective.is_identity() && angle == 0.0 {
        state.toast("No straight lines to square up to");
        return;
    }

    state.applying.set(true);
    for (slider, value) in state.crop.perspective.iter().zip([
        perspective.vertical,
        perspective.horizontal,
        perspective.aspect,
    ]) {
        slider.set_value(value as f64);
    }

    if angle != 0.0 {
        let straightened = (state.crop.straighten.value() + angle as f64).clamp(-15.0, 15.0);
        state.crop.straighten.set_value(straightened);
    }
    state.applying.set(false);

    apply_perspective(state, perspective);
    state.toast("Squared up — every slider is yours to change");
}

pub(super) const AUTO_EDGE: u32 = 900;

pub(super) fn auto_tone(state: &App) {

    let subject = subject_alpha(state);

    let measured = {
        let open = state.open.borrow();
        let Some(photo) = open.as_ref() else { return };
        render::auto::tone(&photo.working, subject.as_deref())
    };

    let (basic, added) = {
        let mut open = state.open.borrow_mut();
        let Some(photo) = open.as_mut() else { return };
        let added = measured.apply(&mut photo.document);
        photo.view = None;
        (photo.document.basic(), added)
    };

    match added {

        Some(index) => {
            fill_segment_masks(state);
            ensure_segmentation(state);
            refresh_masks(state);
            select_mask(state, Some(index));
        }

        None => {
            state.applying.set(true);
            state.sliders.write(basic);
            state.mask_overlay.sliders_hold.set(None);
            state.applying.set(false);
            refresh_slider_marks(state);
        }
    }

    adjustments_changed(state);
    request_render(state);
    schedule_history_push(state);
    state.toast(match added {
        Some(_) => "The subject has its own exposure — every slider is yours to change",
        None => "Exposure and the endpoints set — every slider is yours to change",
    });
}

pub(super) fn read_perspective(state: &App) {
    apply_perspective(
        state,
        Perspective {
            vertical: state.crop.perspective[0].value() as f32,
            horizontal: state.crop.perspective[1].value() as f32,
            aspect: state.crop.perspective[2].value() as f32,
        },
    );
}

pub(super) fn flip_frame(state: &App, vertical: bool) {
    {
        let mut open = state.open.borrow_mut();
        let Some(photo) = open.as_mut() else { return };
        let document = &mut photo.document;
        let quarter = (document.rotation() / 90.0).round() as i32 % 2 != 0;

        if vertical != quarter {
            document.set_rotation(document.rotation() + 180.0);
        }
        document.set_mirrored(!document.mirrored());

        let mut perspective = document.perspective();
        if vertical {
            perspective.vertical = -perspective.vertical;
        } else {
            perspective.horizontal = -perspective.horizontal;
        }
        document.set_perspective(perspective);
        photo.view = None;
    }
    let [x, y, w, h] = state.crop.rect.get();
    state.crop.rect.set(if vertical { [x, 1.0 - y - h, w, h] } else { [1.0 - x - w, y, w, h] });
    state.crop.straighten.set_value(-state.crop.straighten.value());
    write_perspective(state);
    state.crop.area.queue_draw();
    commit_crop(state);
}

pub(super) fn apply_perspective(state: &App, perspective: Perspective) {
    {
        let mut open = state.open.borrow_mut();
        let Some(photo) = open.as_mut() else { return };
        photo.document.set_perspective(perspective);

        photo.view = None;
    }
    state.crop.area.queue_draw();
    commit_crop(state);
}

pub(super) fn write_perspective(state: &App) {
    let perspective = state
        .open
        .borrow()
        .as_ref()
        .map(|photo| photo.document.perspective())
        .unwrap_or_default();

    state.applying.set(true);
    for (slider, value) in state.crop.perspective.iter().zip([
        perspective.vertical,
        perspective.horizontal,
        perspective.aspect,
    ]) {
        slider.set_value(value as f64);
    }
    state.applying.set(false);
}

pub(super) fn commit_crop(state: &App) {
    let angle = state.crop.straighten.value() as f32;
    {
        let mut open = state.open.borrow_mut();
        let Some(photo) = open.as_mut() else { return };

        let (width, height) = oriented_pixels(photo);
        let shown = state.crop.rect.get();
        let left = match state.crop.fit_base.get() {
            Some((base, fitted)) if fitted == shown => base,
            _ => shown,
        };
        let wanted = crop_of_view(photo, left, angle);
        let kept = numa::core::image::crop_inside(wanted, angle, photo.document.perspective(), width, height);
        let now = if kept == wanted { left } else { view_of_crop(photo, kept, angle) };
        state.crop.fit_base.set((now != left).then_some((left, now)));
        if now != shown {
            state.crop.rect.set(now);
            state.crop.area.queue_draw();
        }
        photo.document.set_crop(kept, angle);
    }

    request_render(state);
    schedule_history_push(state);

    if needs_full_resolution(state, state.zooming.level.get()) {
        ensure_full_resolution(state);
    }
}

pub(super) fn tool_rect(state: &App) -> ([f32; 4], f32) {
    let open = state.open.borrow();
    let Some(photo) = open.as_ref() else { return ([0.0, 0.0, 1.0, 1.0], 0.0) };
    let (rect, angle) = photo.document.crop().unwrap_or(([0.0, 0.0, 1.0, 1.0], 0.0));
    (view_of_crop(photo, rect, angle), angle)
}

pub(super) fn view_of_crop(photo: &OpenPhoto, rect: [f32; 4], angle: f32) -> [f32; 4] {
    let (width, height) = oriented_pixels(photo);
    numa::core::image::crop_in_view(width, height, rect, angle, photo.document.perspective())
}

fn crop_of_view(photo: &OpenPhoto, view: [f32; 4], angle: f32) -> [f32; 4] {
    let (width, height) = oriented_pixels(photo);
    numa::core::image::crop_from_view(width, height, view, angle, photo.document.perspective())
}

pub(super) fn keep_on_photograph(state: &App, from: [f32; 4], to: [f32; 4]) -> [f32; 4] {
    let open = state.open.borrow();
    let Some(photo) = open.as_ref() else { return to };
    let angle = state.crop.straighten.value() as f32;
    let (width, height) = oriented_pixels(photo);
    let perspective = photo.document.perspective();
    let fits = |view: [f32; 4]| {
        numa::core::image::crop_fits(crop_of_view(photo, view, angle), angle, perspective, width, height)
    };

    if fits(to) || !fits(from) {
        return to;
    }
    let towards = |from: [f32; 4], to: [f32; 4]| -> [f32; 4] {
        let between = |t: f32| std::array::from_fn(|i| from[i] + (to[i] - from[i]) * t);
        let (mut good, mut bad) = (0.0f32, 1.0f32);
        for _ in 0..12 {
            let middle = (good + bad) / 2.0;
            if fits(between(middle)) {
                good = middle;
            } else {
                bad = middle;
            }
        }
        between(good)
    };

    if state.crop.ratio.get().is_some() {
        return towards(from, to);
    }
    let across = towards(from, [to[0], from[1], to[2], from[3]]);
    let down = towards(across, [across[0], to[1], across[2], to[3]]);
    towards(down, [to[0], down[1], to[2], down[3]])
}
