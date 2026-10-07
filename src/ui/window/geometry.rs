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

        let was = state.crop.at_open.get();
        if geometry_now(state) != was {
            carry_masks(state, was);
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

pub(super) fn forget_model_frames(state: &App) {
    let mut open = state.open.borrow_mut();
    let Some(photo) = open.as_mut() else { return };
    photo.segmentation = None;
    photo.chips = None;
    photo.mask_frame = None;
    photo.segmenting = false;
    photo.embedding = None;
    photo.embedding_pending = false;
}

pub(super) fn carry_masks(state: &App, was: Option<([f32; 4], f32, f32)>) {
    let (Some((old_rect, old_angle, old_rotation)), Some((rect, angle, rotation))) = (was, geometry_now(state))
    else {
        return remake_masks(state);
    };
    if old_rotation != rotation {
        return remake_masks(state);
    }

    forget_model_frames(state);

    let (width, height) = mask_raster_size(state);
    let (pixels_wide, pixels_high) = frame_pixels(state);
    {
        let mut open = state.open.borrow_mut();
        let Some(photo) = open.as_mut() else { return };
        let map = numa::core::image::between_frames(
            pixels_wide,
            pixels_high,
            (rect, angle),
            (old_rect, old_angle),
            photo.document.perspective(),
        );
        let mut masks = photo.document.masks();
        if masks.is_empty() {
            return;
        }
        for mask in masks.iter_mut() {
            mask.remap(map, width, height);
        }
        photo.document.set_masks(masks);

        photo.document.masks_map = None;
        photo.view = None;
    }
    refresh_masks(state);
    request_render(state);
}

pub(super) fn remake_masks(state: &App) {

    forget_model_frames(state);

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
    let (pending, clicked) = match state.open.borrow().as_ref() {
        Some(photo) => {
            let masks = photo.document.masks();
            (masks.iter().any(Mask::is_pending), masks.iter().any(|mask| !mask.points.is_empty()))
        }
        None => (false, false),
    };
    if pending {
        ensure_segmentation(state);
    }

    if clicked {
        ensure_embedding(state);
    }

    ensure_faces(state);
    request_render(state);
    state.mask_overlay.area.queue_draw();
}

type Framing = (([f32; 4], f32, f32), Perspective, bool);

fn framing_at(state: &App) -> Option<Framing> {
    let (perspective, mirrored) = {
        let open = state.open.borrow();
        let document = &open.as_ref()?.document;
        (document.perspective(), document.mirrored())
    };
    Some((geometry_now(state)?, perspective, mirrored))
}

fn still_framed(state: &App, generation: u64, was: Option<Framing>) -> bool {
    state.open_generation.get() == generation && is_cropping(state) && framing_at(state) == was
}

pub(super) fn auto_perspective(state: &App) {
    let was = framing_at(state);
    let generation = state.open_generation.get();
    let state = state.clone();
    glib::spawn_future_local(async move {
        let scene = scene_now(&state, &state.crop.upright_waiting).await;
        if !still_framed(&state, generation, was) {
            return;
        }
        let Some((document, scene)) = state.open.borrow().as_ref().map(|photo| photo.document.clone()).zip(scene) else { return };

        if !document.masks().is_empty() || !document.retouch().is_identity() {
            state.toast("Verticals not corrected — your masks or spots would move");
            return;
        }
        let Some(lean) = scene.upright.filter(|lean| lean.sigma <= 0.5 && lean.vertical().abs() >= 2.0) else {
            state.toast("No converging verticals clear enough to square up");
            return;
        };
        let perspective = Perspective { vertical: lean.vertical(), ..document.perspective() };
        if (perspective.vertical as f64 - state.crop.perspective[0].value()).abs() < 1.0 {
            state.toast("The verticals are already upright");
            return;
        }
        state.applying.set(true);
        state.crop.perspective[0].set_value(perspective.vertical as f64);
        state.applying.set(false);
        apply_perspective(&state, perspective);
        state.toast("Verticals squared up by the buildings — the slider is yours to change");
    });
}

pub(super) fn auto_level(state: &App) {
    let was = framing_at(state);
    let generation = state.open_generation.get();
    let state = state.clone();
    glib::spawn_future_local(async move {
        let scene = scene_now(&state, &state.crop.level_waiting).await;
        if !still_framed(&state, generation, was) {
            return;
        }
        let Some((document, scene)) = state.open.borrow().as_ref().map(|photo| photo.document.clone()).zip(scene) else {
            state.toast("Not levelled: a merged photograph");
            return;
        };
        let current = state.crop.straighten.value() as f32;
        use render::auto::evidence::{decide, Verdict};
        let (angle, by) = match decide(&scene.level, &scene.frame(document.perspective(), current)) {
            Verdict::Level { angle, by } | Verdict::Offer { angle, by } => (angle, by),
            Verdict::Refused(refusal) => {
                state.toast(&render::auto::plan::Level::Leave(render::auto::plan::Leave::Refused(refusal)).says());
                return;
            }
        };
        state.crop.straighten.set_value(angle as f64);
        state.toast(&format!("Levelled {angle:+.1}° by {} — the slider is yours to change", by.name()));
    });
}

pub(super) fn auto_tone_then(
    state: &App,
    then: impl FnOnce(&render::auto::Applied, Option<Arc<numa::core::mask::Stored>>) + 'static,
) {

    let frame = mask_frame_job(state);
    let framing = state.open.borrow().as_ref().map(mask_framing).unwrap_or_default();
    let (width, height) = mask_raster_size(state);

    let (working, found, document, compensation) = {
        let open = state.open.borrow();
        let Some(photo) = open.as_ref() else { return };
        let compensation = photo.summary.as_ref().and_then(|summary| summary.exposure_bias);
        (photo.working.clone(), photo.segmentation.clone(), photo.document.clone(), compensation)
    };
    let path = state.open.borrow().as_ref().and_then(|photo| match &photo.source {
        Source::Photo { path, .. } => Some(path.clone()),
        Source::Bracket { .. } => None,
    });
    let generation = state.open_generation.get();
    let state = state.clone();
    glib::spawn_future_local(async move {

        let measured = busy_in(&state.light.auto_waiting, move || {
            let (frame, made) = match frame {
                Some(Ok(frame)) => (Some(frame), false),
                Some(Err(job)) => (Some(job.make()), true),
                None => (None, false),
            };

            let subject = frame.as_ref().and_then(|frame| {
                let found = match found {
                    Some(found) => Some(found),
                    None => segment::of(frame).map(Arc::new),
                }?;
                let stored = render::auto::subject_mask(&found, frame, width, height).and_then(|mask| mask.map.0);

                let faces = render::auto::faces_in(frame);
                let faces_share: f32 = faces.iter().map(|[_, _, w, h]| w * h).sum();
                let lights = found.alpha(&render::auto::LIGHTS);
                let lit = stored.as_ref().and_then(|stored| {
                    let animal = found.coarse(&[126]).data.iter().any(|share| *share > 0.5);

                    let focus = path
                        .as_deref()
                        .and_then(raw::af_point)
                        .filter(|point| !point.zone && document.rotation() == 0.0 && !document.mirrored())
                        .map(|point| {
                            let (pw, ph) = (working.width as f32, working.height as f32);
                            let (rect, angle) = document.crop().unwrap_or(([0.0, 0.0, 1.0, 1.0], 0.0));
                            numa::core::image::into_crop(pw, ph, rect, angle, document.perspective(), [point.x, point.y])
                        });
                    render::auto::lit_part(&stored.to_alpha(), &faces, animal, focus)
                });
                Some((found, stored, lit, lights, faces_share))
            });
            let (found, stored, lit, lights, faces_share) = match subject {
                Some((found, stored, lit, lights, share)) => (Some(found), stored, lit, Some(lights), share),
                None => (None, None, None, None, 0.0),
            };

            let intent = render::auto::Intent::of(
                compensation,
                path.as_deref().and_then(raw::dynamic_range_mode),
                path.as_deref().and_then(raw::colour_setting),
            );
            let kept = frame.filter(|_| made);

            let shown = render::auto::framed(&document, &working);
            let told = render::auto::Told {
                subject: lit.as_ref().map(|lit| &lit.alpha),
                lights: lights.as_ref(),
                faces: faces_share,
                intent,
            };
            let of = lit.as_ref().map(|lit| match lit.faces {
                0 => "the animal",
                1 => "the face",
                _ => "the faces",
            });
            (found, render::auto::tone_told(&shown, &document, &told), kept, stored, of)
        })
        .await;

        if state.open_generation.get() != generation {
            return;
        }
        let Ok((found, measured, made, subject, of)) = measured else { return };
        if let Some(frame) = made {
            keep_mask_frame(&state, &frame, &framing);
        }

        if let (Some(found), Some(photo)) = (found, state.open.borrow_mut().as_mut()) {
            if mask_framing(photo) == framing {
                photo.segmentation = Some(found);
            }
        }
        let mut applied = apply_auto_tone(&state, &measured);
        applied.of = of;
        then(&applied, subject);
    });
}

fn apply_auto_tone(state: &App, measured: &render::auto::Auto) -> render::auto::Applied {
    let selected = state.mask_overlay.selected_mask.get();
    let (basic, applied, reselect) = {
        let mut open = state.open.borrow_mut();
        let Some(photo) = open.as_mut() else { return Default::default() };
        let id = selected.and_then(|index| photo.document.masks().get(index).map(|mask| mask.id));
        let applied = measured.apply(&mut photo.document);
        let reselect = id.map(|id| photo.document.masks().iter().position(|mask| mask.id == id));
        photo.view = None;
        (photo.document.basic(), applied, reselect)
    };

    refresh_masks(state);

    match reselect {

        Some(Some(index)) if Some(index) != selected => {
            state.mask_overlay.selected_mask.set(Some(index));
            state.mask_overlay.sliders_hold.set(Some(index));
            if state.mask_overlay.brush_owner.get() == selected {
                state.mask_overlay.brush_owner.set(Some(index));
            }
            refresh_masks(state);
        }
        Some(None) => select_mask(state, None),
        Some(_) => {}

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
    applied
}

pub(super) fn auto_tone_toast(applied: &render::auto::Applied) -> String {
    let kept = match applied.kept.as_slice() {
        [] => None,
        [one] => Some(format!("{one} left as it was")),
        [first @ .., last] => Some(format!("{} and {last} left as they were", first.join(", "))),
    };
    if let Some(bias) = applied.held_low.filter(|_| !applied.changed) {
        return format!("{bias:+.1} EV dialled in — kept low-key");
    }
    let of = applied.of.unwrap_or("the subject");
    let done = match (applied.subject, applied.changed) {

        (Some(stops), _) if stops > 0.0 => format!("Exposure set for {of} ({stops:+.1}); the brightest parts may clip"),
        (Some(stops), _) => format!("Exposure set for {of} ({stops:+.1}); the deepest shadows may close"),
        (None, _) if applied.silhouette => "Silhouette kept".to_string(),
        (None, true) => "Exposure and the endpoints set".to_string(),
        (None, false) if kept.is_none() => return "Exposure and the endpoints already right".into(),
        (None, false) => "Nothing changed".to_string(),
    };
    format!("{done} — {}", kept.unwrap_or_else(|| "every slider is yours to change".into()))
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
