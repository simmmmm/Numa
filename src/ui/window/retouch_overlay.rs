use super::*;

fn draw_spots(state: &App, context: &gtk::cairo::Context, width: i32, height: i32) {
    let spots = match state.open.borrow().as_ref() {
        Some(photo) => photo.document.retouch().spots,
        None => return,
    };
    let (left, top, shown_width, shown_height) =
        content_rect(&state, width as f64, height as f64);
    if shown_width <= 0.0 {
        return;
    }
    let long_edge = shown_width.max(shown_height);
    let selected = state.retouch.selected_spot.get();

    for (index, spot) in spots.iter().enumerate() {
        let radius = spot.radius as f64 * long_edge;
        let to = (
            left + spot.at[0] as f64 * shown_width,
            top + spot.at[1] as f64 * shown_height,
        );
        let from = (
            left + spot.from[0] as f64 * shown_width,
            top + spot.from[1] as f64 * shown_height,
        );
        let current = Some(index) == selected;
        let alpha = if current { 1.0 } else { 0.55 };

        let patch = spot.kind == numa::core::retouch::Kind::Patch;
        let rings: &[((f64, f64), bool)] = if patch { &[(to, false), (from, true)] } else { &[(to, false)] };
        if !patch {
            for (centre, dashed) in rings {
                context.set_dash(if *dashed { &[5.0, 4.0] } else { &[] }, 0.0);
                context.arc(centre.0, centre.1, radius, 0.0, std::f64::consts::TAU);
                context.set_source_rgba(0.0, 0.0, 0.0, 0.6 * alpha);
                context.set_line_width(3.0);
                let _ = context.stroke_preserve();
                context.set_source_rgba(1.0, 1.0, 1.0, 0.95 * alpha);
                context.set_line_width(1.5);
                let _ = context.stroke();
            }
            continue;
        }

        context.set_source_rgba(0.0, 0.0, 0.0, 0.5 * alpha);
        context.set_line_width(3.0);
        context.move_to(from.0, from.1);
        context.line_to(to.0, to.1);
        let _ = context.stroke();
        context.set_source_rgba(1.0, 1.0, 1.0, 0.9 * alpha);
        context.set_line_width(1.0);
        context.move_to(from.0, from.1);
        context.line_to(to.0, to.1);
        let _ = context.stroke();

        for (centre, dashed) in [(to, false), (from, true)] {
            context.set_dash(if dashed { &[5.0, 4.0] } else { &[] }, 0.0);
            context.arc(centre.0, centre.1, radius, 0.0, std::f64::consts::TAU);
            context.set_source_rgba(0.0, 0.0, 0.0, 0.6 * alpha);
            context.set_line_width(3.0);
            let _ = context.stroke_preserve();
            context.set_source_rgba(1.0, 1.0, 1.0, 0.95 * alpha);
            context.set_line_width(1.5);
            let _ = context.stroke();
        }
        context.set_dash(&[], 0.0);
    }
}

pub(super) fn build_retouch_overlay(state: &App) -> gtk::DrawingArea {
    let area = state.retouch.area.clone();
    area.set_visible(false);
    area.set_can_target(true);

    let grabbed: Rc<RefCell<Option<(usize, bool, Spot, f32, f32)>>> = Rc::new(RefCell::new(None));

    area.set_draw_func(glib::clone!(
        #[strong] state,
        move |_, context, width, height| draw_spots(&state, context, width, height)
    ));

    let drag = gtk::GestureDrag::new();
    drag.connect_drag_begin(glib::clone!(
        #[strong] state,
        #[strong] grabbed,
        move |gesture, x, y| {
            let Some((u, v)) = retouch_point(&state, x, y) else { return };

            if let Some((index, source, spot)) = spot_at(&state, u, v) {

                gesture.set_state(gtk::EventSequenceState::Claimed);
                state.retouch.selected_spot.set(Some(index));
                *grabbed.borrow_mut() = Some((index, source, spot, u, v));
                refresh_retouch(&state);
                state.retouch.area.queue_draw();
                return;
            }

            if state.retouch.tool.get().is_none() {
                return;
            }
            let Some(index) = place_spot(&state, u, v) else { return };
            gesture.set_state(gtk::EventSequenceState::Claimed);
            let spot = current_spots(&state).get(index).copied().unwrap_or_default();

            let source = spot.kind == numa::core::retouch::Kind::Patch;
            *grabbed.borrow_mut() = Some((index, source, spot, u, v));
        }
    ));
    drag.connect_drag_update(glib::clone!(
        #[strong] state,
        #[strong] grabbed,
        move |_, dx, dy| {
            let held = *grabbed.borrow();
            let Some((index, source, original, _, _)) = held else { return };
            let Some((width, height)) = frame_size(&state) else { return };
            let moved = [(dx / width) as f32, (dy / height) as f32];

            let mut spots = current_spots(&state);
            let Some(spot) = spots.get_mut(index) else { return };
            let anchor = if source { original.from } else { original.at };
            let moved_to = [
                (anchor[0] + moved[0]).clamp(0.0, 1.0),
                (anchor[1] + moved[1]).clamp(0.0, 1.0),
            ];
            if source {
                spot.from = moved_to;
            } else {

                spot.at = moved_to;
                spot.from = [
                    (original.from[0] + moved[0]).clamp(0.0, 1.0),
                    (original.from[1] + moved[1]).clamp(0.0, 1.0),
                ];
            }
            write_spots(&state, spots, false);
        }
    ));
    drag.connect_drag_end(glib::clone!(
        #[strong] state,
        #[strong] grabbed,
        move |_, _, _| {

            let released = grabbed.borrow_mut().take();
            if released.is_some() {
                let spots = current_spots(&state);
                write_spots(&state, spots, true);
                refresh_retouch(&state);
            }
        }
    ));
    area.add_controller(drag);

    area
}

pub(super) fn retouch_point(state: &App, x: f64, y: f64) -> Option<(f32, f32)> {
    let (left, top, width, height) = content_rect(
        state,
        state.retouch.area.width() as f64,
        state.retouch.area.height() as f64,
    );
    (width > 0.0 && height > 0.0)
        .then(|| (((x - left) / width) as f32, ((y - top) / height) as f32))
}

pub(super) fn frame_size(state: &App) -> Option<(f64, f64)> {
    let (_, _, width, height) = content_rect(
        state,
        state.retouch.area.width() as f64,
        state.retouch.area.height() as f64,
    );
    (width > 0.0 && height > 0.0).then_some((width, height))
}

pub(super) fn current_spots(state: &App) -> Vec<Spot> {
    state
        .open
        .borrow()
        .as_ref()
        .map(|photo| photo.document.retouch().spots)
        .unwrap_or_default()
}

pub(super) fn spot_at(state: &App, u: f32, v: f32) -> Option<(usize, bool, Spot)> {
    let (width, height) = frame_size(state)?;
    let long_edge = width.max(height);
    let spots = current_spots(state);

    for (index, spot) in spots.iter().enumerate().rev() {
        let radius = spot.radius as f64 * long_edge;
        let reach = radius.max(8.0);
        let ends: &[(bool, [f32; 2])] = match spot.kind {
            numa::core::retouch::Kind::Patch => &[(true, spot.from), (false, spot.at)],
            _ => &[(false, spot.at)],
        };
        for &(source, centre) in ends {
            let dx = (u - centre[0]) as f64 * width;
            let dy = (v - centre[1]) as f64 * height;
            if dx.hypot(dy) <= reach {
                return Some((index, source, *spot));
            }
        }
    }
    None
}

pub(super) fn place_spot(state: &App, u: f32, v: f32) -> Option<usize> {
    let radius = state.retouch.sliders[0].value() as f32 / 100.0;
    let feather = state.retouch.sliders[1].value() as f32 / 100.0;
    let opacity = state.retouch.sliders[2].value() as f32 / 100.0;

    let (width, height) = frame_size(state)?;
    let long_edge = width.max(height);
    let step = radius * 3.0 * (long_edge / width) as f32;
    let from_x = if u + step < 0.98 { u + step } else { u - step };

    let (kind, heal) = state.retouch.tool.get()?.spot();

    let from = match kind {
        numa::core::retouch::Kind::Patch => [from_x.clamp(0.0, 1.0), v],
        _ => [u, v],
    };

    let feather = match kind {
        numa::core::retouch::Kind::Remove => feather.min(0.2),
        _ => feather,
    };
    let spot = Spot { at: [u, v], from, radius, feather, opacity, heal, kind };

    let mut spots = current_spots(state);
    spots.push(spot);
    let index = spots.len() - 1;
    state.retouch.selected_spot.set(Some(index));
    write_spots(state, spots, true);
    refresh_retouch(state);
    refresh_face(state);
    refresh_found(state);
    Some(index)
}

pub(super) fn write_spots(state: &App, spots: Vec<Spot>, settle: bool) {
    {
        let mut open = state.open.borrow_mut();
        let Some(photo) = open.as_mut() else { return };
        photo.document.set_retouch(Retouch { spots });
    }
    request_render(state);
    state.retouch.area.queue_draw();
    if settle {
        schedule_history_push(state);
    }
}

pub(super) fn remove_spot(state: &App, index: usize) {
    let mut spots = current_spots(state);
    if index >= spots.len() {
        return;
    }
    spots.remove(index);
    state.retouch.selected_spot.set(None);
    write_spots(state, spots, true);
    refresh_retouch(state);
    refresh_face(state);
    refresh_found(state);
}

pub(super) fn write_face(state: &App) {
    let beautify = state
        .open
        .borrow()
        .as_ref()
        .map(|photo| photo.document.beautify())
        .unwrap_or_default();

    state.applying.set(true);
    for (index, value) in
        [beautify.spots, beautify.skin, beautify.evenness, beautify.red_eye, beautify.teeth]
            .into_iter()
            .enumerate()
    {
        state.retouch.face_sliders[index].set_value(value as f64);
    }
    state.applying.set(false);
}

pub(super) fn read_face(state: &App) {
    {
        let mut open = state.open.borrow_mut();
        let Some(photo) = open.as_mut() else { return };
        let mut beautify = photo.document.beautify();
        beautify.spots = state.retouch.face_sliders[0].value() as f32;
        beautify.skin = state.retouch.face_sliders[1].value() as f32;
        beautify.evenness = state.retouch.face_sliders[2].value() as f32;
        beautify.red_eye = state.retouch.face_sliders[3].value() as f32;
        beautify.teeth = state.retouch.face_sliders[4].value() as f32;
        photo.document.set_beautify(beautify);
    }

    request_render(state);
    schedule_history_push(state);
}

pub(super) fn refresh_face(state: &App) {
    let found = state
        .open
        .borrow()
        .as_ref()
        .map(|photo| photo.document.faces.len())
        .unwrap_or(0);

    state.retouch.face_section.set_visible(found > 0);
    state.retouch.face_note.set_text(&match found {
        0 => String::new(),
        1 => "One face found.".to_string(),
        many => format!("{many} faces found — these apply to all of them."),
    });
    write_face(state);
}

pub(super) fn ensure_faces(state: &App) {
    if !cull::faces::is_installed() {
        return;
    }
    let request = {
        let mut open = state.open.borrow_mut();
        let Some(photo) = open.as_mut() else { return };
        if !photo.document.faces.is_empty() || photo.faces_pending {
            return;
        }

        let mut geometry = Document::new(photo.document.source.path.clone());
        geometry.set_perspective(photo.document.perspective());
        if let Some((rect, angle)) = photo.document.crop() {
            geometry.set_crop(rect, angle);
        }
        geometry.set_rotation(photo.document.rotation());
        geometry.set_mirrored(photo.document.mirrored());
        photo.faces_pending = true;
        (geometry, photo.working.clone())
    };

    let (geometry, working) = request;
    let generation = state.open_generation.get();
    let state = state.clone();
    glib::spawn_future_local(async move {
        let found = busy(&state, "Looking for faces…", move || {
            let frame = render::apply_stack(&geometry, &working, 1.0);
            let (width, height) = (frame.width() as f32, frame.height() as f32);
            cull::faces::detect(&frame).map(|faces| {
                let portraits = faces
                    .iter()
                    .map(|face| Portrait {
                        at: [
                            face.x / width,
                            face.y / height,
                            face.width / width,
                            face.height / height,
                        ],
                        points: face.landmarks.map(|(x, y)| [x / width, y / height]),
                    })
                    .collect::<Vec<_>>();

                let people = faces
                    .iter()
                    .filter_map(|face| {
                        Some(SeenFace {
                            at: [
                                face.x / width,
                                face.y / height,
                                face.width / width,
                                face.height / height,
                            ],
                            embedding: cull::people::embed(&frame, face)?,
                            portrait: cull::people::align(&frame, face)?,
                        })
                    })
                    .collect::<Vec<_>>();
                (portraits, people)
            })
        })
        .await;

        if state.open_generation.get() != generation {
            return;
        }

        let any = {
            let mut open = state.open.borrow_mut();
            let Some(photo) = open.as_mut() else { return };
            photo.faces_pending = false;
            let (portraits, people) = found.ok().flatten().unwrap_or_default();
            photo.document.faces = portraits;
            photo.people = people;
            !photo.document.faces.is_empty()
        };

        refresh_face(&state);
        refresh_found(&state);
        refresh_info(&state);

        refresh_face_names(&state);
        if any {
            request_render(&state);
        }
    });
}

pub(super) fn refresh_retouch(state: &App) {
    let spots = current_spots(state);
    let selected = state.retouch.selected_spot.get().filter(|index| *index < spots.len());
    state.retouch.selected_spot.set(selected);

    while let Some(row) = state.retouch.list.first_child() {
        state.retouch.list.remove(&row);
    }

    for (index, spot) in spots.iter().enumerate() {
        let row = adw::ActionRow::new();
        row.set_title(&format!(
            "{} {}",
            match (spot.kind, spot.heal) {
                (numa::core::retouch::Kind::PetEye, _) => "Pet eye",
                (numa::core::retouch::Kind::Remove, _) => "Removed",
                (_, true) => "Healed",
                (_, false) => "Cloned",
            },
            index + 1
        ));
        row.set_subtitle(&format!("{:.1} % across", spot.radius * 100.0));
        row.add_css_class("mask-part");
        row.set_activatable(true);
        if Some(index) == selected {
            row.add_css_class("current-mask");
        }
        row.connect_activated(glib::clone!(
            #[strong] state,
            move |_| {
                state.retouch.selected_spot.set(Some(index));
                refresh_retouch(&state);
        refresh_face(&state);
        refresh_found(&state);
                state.retouch.area.queue_draw();
            }
        ));

        let remove = gtk::Button::from_icon_name("window-close-symbolic");
        remove.set_valign(gtk::Align::Center);
        remove.add_css_class("flat");
        remove.set_tooltip_text(Some("Put this back the way it was"));
        remove.connect_clicked(glib::clone!(
            #[strong] state,
            move |_| remove_spot(&state, index)
        ));
        row.add_suffix(&remove);
        state.retouch.list.append(&row);
    }

    state.retouch.list.set_visible(!spots.is_empty());

    if let Some(spot) = selected.and_then(|index| spots.get(index)) {
        state.applying.set(true);
        state.retouch.sliders[0].set_value(spot.radius as f64 * 100.0);
        state.retouch.sliders[1].set_value(spot.feather as f64 * 100.0);
        state.retouch.sliders[2].set_value(spot.opacity as f64 * 100.0);

        let _ = spot.heal;
        state.applying.set(false);
    }
}

pub(super) fn toggle_retouch(state: &App, on: bool) {
    state.retouch.on.set(on);
    state.retouch.area.set_visible(on);
    if !on {
        state.retouch.selected_spot.set(None);
    }

    state.retouch.tool.set(None);
    state.applying.set(true);
    state.retouch.heal.set_active(false);
    state.retouch.clone_tool.set_active(false);
    state.retouch.pet_eye.set_active(false);
    state.retouch.remove.set_active(false);
    state.applying.set(false);
    refresh_retouch(state);
    refresh_face(state);
    refresh_found(state);
    state.retouch.area.queue_draw();
}
