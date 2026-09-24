use super::*;

pub(super) fn build_face_names_overlay(state: &App) -> gtk::DrawingArea {
    let area = state.overlays.face_names_area.clone();
    area.set_can_target(false);
    area.set_visible(false);
    state.canvas.connect_paintable_notify(glib::clone!(
        #[weak] area,
        move |_| area.queue_draw()
    ));
    area.set_draw_func(glib::clone!(
        #[strong] state,
        move |_, context, width, height| {
            let (x, y, w, h) = content_rect(&state, width as f64, height as f64);
            context.select_font_face(
                "Sans",
                gtk::cairo::FontSlant::Normal,
                gtk::cairo::FontWeight::Normal,
            );
            context.set_font_size(13.0);
            for (at, name) in state.overlays.face_names.borrow().iter() {
                let (face_x, face_y) = (x + w * at[0] as f64, y + h * at[1] as f64);
                let (face_w, face_h) = (w * at[2] as f64, h * at[3] as f64);

                context.set_source_rgba(1.0, 1.0, 1.0, 0.7);
                context.set_line_width(1.0);
                context.rectangle(face_x.round() + 0.5, face_y.round() + 0.5, face_w.round(), face_h.round());
                let _ = context.stroke();

                let Ok(extents) = context.text_extents(name) else { continue };
                let pad = 5.0;
                let (label_w, label_h) = (extents.width() + pad * 2.0, 20.0);

                let label_x = (face_x + face_w / 2.0 - label_w / 2.0).clamp(x, x + w - label_w);
                let label_y = (face_y + face_h + 4.0).min(y + h - label_h);
                context.set_source_rgba(0.0, 0.0, 0.0, 0.55);
                context.rectangle(label_x, label_y, label_w, label_h);
                let _ = context.fill();
                context.set_source_rgba(1.0, 1.0, 1.0, 0.95);
                context.move_to(label_x + pad, label_y + label_h - 6.0);
                let _ = context.show_text(name);
            }
        }
    ));
    area
}

pub(super) fn refresh_face_names(state: &App) {
    if !state.overlays.show_face_names.get() {
        return;
    }
    let open = state.open.borrow();
    let Some(photo) = open.as_ref() else { return };
    let Source::Photo { id, .. } = photo.source else { return };

    let named = state.catalog.named_faces().unwrap_or_else(|err| {
        log::warn!("could not read the named faces: {err}");
        Vec::new()
    });
    let elsewhere: Vec<(String, [f32; cull::people::LENGTH])> =
        named.iter().map(|(_, name, embedding)| (name.clone(), *embedding)).collect();

    let names = photo
        .people
        .iter()
        .filter_map(|seen| {
            let here = named
                .iter()
                .filter(|(photo_id, _, _)| *photo_id == id)
                .map(|(_, name, embedding)| {
                    (name, cull::people::likeness(embedding, &seen.embedding))
                })
                .filter(|(_, alike)| *alike >= 0.8)
                .max_by(|a, b| a.1.total_cmp(&b.1))
                .map(|(name, _)| name.clone());
            let name = match here {
                Some(name) => name,
                None => format!("{}?", cull::people::recognise(&seen.embedding, &elsewhere)?.0),
            };

            (!name.trim_end_matches('?').is_empty()).then_some((seen.at, name))
        })
        .collect();

    *state.overlays.face_names.borrow_mut() = names;
    state.overlays.face_names_area.queue_draw();
}

pub(super) fn show_face_names(state: &App, on: bool) {
    state.overlays.show_face_names.set(on);
    state.overlays.face_names_area.set_visible(on);
    if on {
        refresh_face_names(state);
    } else {
        state.overlays.face_names.borrow_mut().clear();
    }
    state.overlays.face_names_area.queue_draw();
}

pub(super) fn build_guides_overlay(state: &App) -> gtk::DrawingArea {
    let area = state.overlays.guides_area.clone();
    area.set_can_target(false);
    area.set_visible(false);

    state.canvas.connect_paintable_notify(glib::clone!(
        #[weak] area,
        move |_| area.queue_draw()
    ));
    area.set_draw_func(glib::clone!(
        #[strong] state,
        move |_, context, width, height| {
            let (x, y, w, h) = content_rect(&state, width as f64, height as f64);
            let divisions = match state.overlays.guides.get() {
                1 => 3,
                2 => 8,
                _ => return,
            };

            for (colour, line) in [((0.0, 0.0, 0.0, 0.45), 2.0), ((1.0, 1.0, 1.0, 0.7), 1.0)] {
                context.set_source_rgba(colour.0, colour.1, colour.2, colour.3);
                context.set_line_width(line);
                for step in 1..divisions {
                    let fraction = step as f64 / divisions as f64;
                    context.move_to((x + w * fraction).round() + 0.5, y);
                    context.line_to((x + w * fraction).round() + 0.5, y + h);
                    context.move_to(x, (y + h * fraction).round() + 0.5);
                    context.line_to(x + w, (y + h * fraction).round() + 0.5);
                }
                let _ = context.stroke();
            }
        }
    ));
    area
}

pub(super) fn cycle_guides(state: &App) {
    let next = (state.overlays.guides.get() + 1) % 3;
    state.overlays.guides.set(next);
    state.overlays.guides_area.set_visible(next != 0);
    state.overlays.guides_area.queue_draw();
    state.overlays.guides_button.set_tooltip_text(Some(match next {
        1 => "Guides: thirds (G)",
        2 => "Guides: grid (G)",
        _ => "Guides: none (G)",
    }));
}

pub(super) fn build_crop_overlay(state: &App) -> gtk::DrawingArea {
    let area = state.crop.area.clone();
    area.set_visible(false);
    area.set_can_target(true);

    area.set_draw_func(glib::clone!(
        #[strong] state,
        move |_, context, width, height| {
            let content = content_rect(&state, width as f64, height as f64);
            draw_crop(context, width as f64, height as f64, content, state.crop.rect.get());
            draw_guide_lines(&state, context);
        }
    ));

    let grabbed: Rc<Cell<Option<(usize, [f32; 4], f64, f64)>>> = Rc::new(Cell::new(None));

    let guide_grab: GuideGrab = Rc::new(Cell::new(None));

    let drag = gtk::GestureDrag::new();
    drag.connect_drag_begin(glib::clone!(
        #[strong] state,
        #[strong] grabbed,
        #[strong] guide_grab,
        move |gesture, x, y| {
            if state.crop.guided.is_active() {
                gesture.set_state(gtk::EventSequenceState::Claimed);
                guide_drag_begin(&state, &guide_grab, x, y);
                return;
            }
            let (ox, oy, width, height) = content_rect(
                &state,
                state.crop.area.width() as f64,
                state.crop.area.height() as f64,
            );
            if width <= 0.0 || height <= 0.0 {
                return;
            }

            gesture.set_state(gtk::EventSequenceState::Claimed);
            grabbed.set(Some((
                nearest_handle(state.crop.rect.get(), (x - ox) / width, (y - oy) / height),
                state.crop.rect.get(),
                x,
                y,
            )));
        }
    ));
    drag.connect_drag_update(glib::clone!(
        #[strong] state,
        #[strong] grabbed,
        #[strong] guide_grab,
        move |_, dx, dy| {
            if guide_grab.get().is_some() {
                guide_drag_update(&state, &guide_grab, dx, dy);
                return;
            }
            let Some((handle, start, _, _)) = grabbed.get() else { return };
            let (_, _, width, height) = content_rect(
                &state,
                state.crop.area.width() as f64,
                state.crop.area.height() as f64,
            );
            if width <= 0.0 || height <= 0.0 {
                return;
            }

            let mut moved = move_handle(
                start,
                handle,
                (dx / width) as f32,
                (dy / height) as f32,
            );

            if let Some(ratio) = state.crop.ratio.get() {
                if handle < 4 {
                    moved = hold_aspect(moved, handle, ratio / frame_aspect(&state));
                }
            }
            state.crop.rect.set(keep_on_photograph(&state, state.crop.rect.get(), moved));
            state.crop.area.queue_draw();
        }
    ));
    drag.connect_drag_end(glib::clone!(
        #[strong] state,
        #[strong] grabbed,
        #[strong] guide_grab,
        move |_, _, _| {
            if state.crop.guided.is_active() {
                guide_drag_end(&state, &guide_grab);
                return;
            }
            grabbed.set(None);

            commit_crop(&state);
        }
    ));
    area.add_controller(drag);

    area
}

pub(super) type GuideGrab = Rc<Cell<Option<(usize, usize, f64, f64)>>>;

pub(super) const MOST_GUIDES: usize = 4;

pub(super) const GUIDE_REACH: f64 = 12.0;

pub(super) fn guided_row(state: &App, auto: &gtk::Button) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    row.set_margin_top(4);
    auto.set_margin_top(0);
    auto.set_hexpand(true);
    row.append(auto);

    let guided = state.crop.guided.clone();
    guided.set_hexpand(true);
    guided.set_tooltip_text(Some("Draw up to four lines along what should be upright or level"));
    let clear = gtk::Button::with_label("Clear");
    clear.set_hexpand(true);
    clear.set_tooltip_text(Some("Remove the guides"));
    clear.set_sensitive(false);
    guided.connect_toggled(glib::clone!(
        #[strong] state,
        #[weak] clear,
        move |button| {
            clear.set_sensitive(button.is_active());
            if !button.is_active() {
                state.crop.guide_lines.borrow_mut().clear();
            }
            state.crop.area.queue_draw();
        }
    ));
    clear.connect_clicked(glib::clone!(
        #[strong] state,
        move |_| {
            state.crop.guide_lines.borrow_mut().clear();
            state.crop.area.queue_draw();
        }
    ));
    row.append(&guided);
    row.append(&clear);
    row
}

pub(super) fn guide_frame(state: &App) -> (f32, f32, f32, Perspective) {
    let (width, height) = frame_pixels(state);
    let perspective = state.open.borrow().as_ref().map(|photo| photo.document.perspective()).unwrap_or_default();
    (width, height, state.crop.straighten.value() as f32, perspective)
}

pub(super) fn guide_to_widget(state: &App, point: [f32; 2]) -> Option<(f64, f64)> {
    let (ox, oy, shown_width, shown_height) =
        content_rect(state, state.crop.area.width() as f64, state.crop.area.height() as f64);
    let (width, height, angle, perspective) = guide_frame(state);
    let [x, y] = numa::core::guided::to_frame(point, width, height, angle, perspective)?;
    Some((ox + x as f64 * shown_width, oy + y as f64 * shown_height))
}

pub(super) fn widget_to_guide(state: &App, x: f64, y: f64) -> Option<[f32; 2]> {
    let (ox, oy, shown_width, shown_height) =
        content_rect(state, state.crop.area.width() as f64, state.crop.area.height() as f64);
    if shown_width <= 0.0 || shown_height <= 0.0 {
        return None;
    }
    let fraction = [((x - ox) / shown_width).clamp(0.0, 1.0) as f32, ((y - oy) / shown_height).clamp(0.0, 1.0) as f32];
    let (width, height, angle, perspective) = guide_frame(state);
    Some(numa::core::guided::to_source(fraction, width, height, angle, perspective))
}

pub(super) fn guide_drag_begin(state: &App, grab: &GuideGrab, x: f64, y: f64) {
    let ends: Vec<_> = state
        .crop
        .guide_lines
        .borrow()
        .iter()
        .enumerate()
        .flat_map(|(line, ends)| [(line, 0, ends[0]), (line, 1, ends[1])])
        .collect();
    let nearest = ends
        .into_iter()
        .filter_map(|(line, end, point)| {
            let (px, py) = guide_to_widget(state, point)?;
            Some((line, end, (px - x).hypot(py - y)))
        })
        .filter(|(_, _, distance)| *distance < GUIDE_REACH)
        .min_by(|a, b| a.2.total_cmp(&b.2));
    if let Some((line, end, _)) = nearest {
        grab.set(Some((line, end, x, y)));
        return;
    }

    if state.crop.guide_lines.borrow().len() >= MOST_GUIDES {
        state.toast("Four guides at most — move one, or Clear");
        return;
    }
    let Some(point) = widget_to_guide(state, x, y) else { return };
    let mut lines = state.crop.guide_lines.borrow_mut();
    lines.push([point, point]);
    grab.set(Some((lines.len() - 1, 1, x, y)));
}

pub(super) fn guide_drag_update(state: &App, grab: &GuideGrab, dx: f64, dy: f64) {
    let Some((line, end, x, y)) = grab.get() else { return };
    let Some(point) = widget_to_guide(state, x + dx, y + dy) else { return };
    if let Some(ends) = state.crop.guide_lines.borrow_mut().get_mut(line) {
        ends[end] = point;
    }
    state.crop.area.queue_draw();
}

pub(super) fn guide_drag_end(state: &App, grab: &GuideGrab) {
    if grab.take().is_none() {
        return;
    }
    let drawn = state.crop.guide_lines.borrow().clone();
    let lines: Vec<_> = drawn
        .into_iter()
        .filter(|[a, b]| match (guide_to_widget(state, *a), guide_to_widget(state, *b)) {
            (Some(a), Some(b)) => (a.0 - b.0).hypot(a.1 - b.1) >= GUIDE_REACH,
            _ => false,
        })
        .collect();
    *state.crop.guide_lines.borrow_mut() = lines.clone();
    state.crop.area.queue_draw();
    if lines.is_empty() {
        return;
    }

    let (width, height, angle, perspective) = guide_frame(state);
    let (perspective, angle) = numa::core::guided::solve(&lines, width, height, angle, perspective);
    state.applying.set(true);
    state.crop.straighten.set_value(angle as f64);
    state.applying.set(false);
    apply_perspective(state, perspective);
    write_perspective(state);
}

pub(super) fn draw_guide_lines(state: &App, context: &gtk::cairo::Context) {
    if !state.crop.guided.is_active() {
        return;
    }
    let lines = state.crop.guide_lines.borrow().clone();
    let ends: Vec<_> = lines
        .iter()
        .filter_map(|[a, b]| Some((guide_to_widget(state, *a)?, guide_to_widget(state, *b)?)))
        .collect();
    for (colour, line, radius) in [((0.0, 0.0, 0.0, 0.6), 3.0, 5.5), ((1.0, 1.0, 1.0, 0.95), 1.5, 4.0)] {
        context.set_source_rgba(colour.0, colour.1, colour.2, colour.3);
        context.set_line_width(line);
        for ((ax, ay), (bx, by)) in &ends {
            context.move_to(*ax, *ay);
            context.line_to(*bx, *by);
        }
        let _ = context.stroke();
        for ((ax, ay), (bx, by)) in &ends {
            for (x, y) in [(ax, ay), (bx, by)] {
                context.new_sub_path();
                context.arc(*x, *y, radius, 0.0, std::f64::consts::TAU);
            }
        }
        let _ = context.fill();
    }
}

pub(super) fn content_rect(state: &App, width: f64, height: f64) -> (f64, f64, f64, f64) {
    let Some(paintable) = state.canvas.paintable() else {
        return (0.0, 0.0, width, height);
    };
    let (image_width, image_height) = (
        paintable.intrinsic_width() as f64,
        paintable.intrinsic_height() as f64,
    );
    if image_width <= 0.0 || image_height <= 0.0 || width <= 0.0 || height <= 0.0 {
        return (0.0, 0.0, width, height);
    }

    let scale = (width / image_width).min(height / image_height);
    let shown_width = image_width * scale;
    let shown_height = image_height * scale;

    (
        (width - shown_width) / 2.0,
        (height - shown_height) / 2.0,
        shown_width,
        shown_height,
    )
}

pub(super) fn nearest_handle(rect: [f32; 4], x: f64, y: f64) -> usize {
    let [rx, ry, rw, rh] = rect.map(f64::from);
    let corners = [
        (rx, ry),
        (rx + rw, ry),
        (rx, ry + rh),
        (rx + rw, ry + rh),
    ];

    const GRAB: f64 = 0.05;

    corners
        .iter()
        .enumerate()
        .map(|(index, (cx, cy))| (index, (x - cx).hypot(y - cy)))
        .filter(|(_, distance)| *distance < GRAB)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map_or(4, |(index, _)| index)
}

pub(super) fn move_handle(rect: [f32; 4], handle: usize, dx: f32, dy: f32) -> [f32; 4] {
    const MIN: f32 = 0.05;
    let [x, y, w, h] = rect;

    let (mut left, mut top, mut right, mut bottom) = (x, y, x + w, y + h);

    match handle {
        0 => { left += dx; top += dy; }
        1 => { right += dx; top += dy; }
        2 => { left += dx; bottom += dy; }
        3 => { right += dx; bottom += dy; }
        _ => {

            let dx = dx.clamp(-left, 1.0 - right);
            let dy = dy.clamp(-top, 1.0 - bottom);
            return [left + dx, top + dy, w, h];
        }
    }

    left = left.clamp(0.0, 1.0);
    top = top.clamp(0.0, 1.0);
    right = right.clamp(0.0, 1.0);
    bottom = bottom.clamp(0.0, 1.0);

    if right - left < MIN {
        if handle == 0 || handle == 2 { left = right - MIN; } else { right = left + MIN; }
    }
    if bottom - top < MIN {
        if handle == 0 || handle == 1 { top = bottom - MIN; } else { bottom = top + MIN; }
    }

    [
        left.clamp(0.0, 1.0 - MIN),
        top.clamp(0.0, 1.0 - MIN),
        (right - left).clamp(MIN, 1.0),
        (bottom - top).clamp(MIN, 1.0),
    ]
}

pub(super) fn draw_crop(
    context: &gtk::cairo::Context,
    width: f64,
    height: f64,
    content: (f64, f64, f64, f64),
    rect: [f32; 4],
) {
    let (ox, oy, content_width, content_height) = content;
    let [x, y, w, h] = rect.map(f64::from);
    let left = ox + x * content_width;
    let top = oy + y * content_height;
    let right = ox + (x + w) * content_width;
    let bottom = oy + (y + h) * content_height;

    context.set_source_rgba(0.0, 0.0, 0.0, 0.55);
    context.rectangle(0.0, 0.0, width, height);
    context.rectangle(left, top, right - left, bottom - top);
    context.set_fill_rule(gtk::cairo::FillRule::EvenOdd);
    let _ = context.fill();
    context.set_fill_rule(gtk::cairo::FillRule::Winding);

    context.set_source_rgba(1.0, 1.0, 1.0, 0.25);
    context.set_line_width(1.0);
    for third in 1..3 {
        let fraction = third as f64 / 3.0;
        context.move_to(left + (right - left) * fraction, top);
        context.line_to(left + (right - left) * fraction, bottom);
        context.move_to(left, top + (bottom - top) * fraction);
        context.line_to(right, top + (bottom - top) * fraction);
    }
    let _ = context.stroke();

    context.set_source_rgba(1.0, 1.0, 1.0, 0.9);
    context.set_line_width(1.5);
    context.rectangle(left, top, right - left, bottom - top);
    let _ = context.stroke();

    for (cx, cy) in [(left, top), (right, top), (left, bottom), (right, bottom)] {
        context.rectangle(cx - 6.0, cy - 6.0, 12.0, 12.0);
    }
    let _ = context.fill();
}

pub(super) fn show_baseline(state: &App) {
    let texture = {
        let mut open = state.open.borrow_mut();
        let Some(photo) = open.as_mut() else { return };

        if photo.baseline.is_none() {
            let original = Document::new(photo.document.source.path.clone());
            let working = render::to_working_space(&original, &*photo.proxy, &Default::default());
            let proxy_scale = photo.proxy.width.max(photo.proxy.height) as f32
                / photo.full_size.0.max(photo.full_size.1).max(1) as f32;
            photo.baseline = Some(

                crate::ui::pixel_paintable::PixelPaintable::new(texture_from(render::apply_stack(
                    &original,
                    &working,
                    proxy_scale,
                )))
                .upcast(),
            );
        }
        photo.baseline.clone()
    };

    if let Some(paintable) = texture {
        state.canvas.set_paintable(Some(&paintable));
        apply_zoom(state);
    }
}

#[derive(Clone)]
pub(super) struct State {

    pub(super) face_names_area: gtk::DrawingArea,
    pub(super) show_face_names: Rc<Cell<bool>>,
    pub(super) face_names: Rc<RefCell<Vec<([f32; 4], String)>>>,

    pub(super) guides: Rc<Cell<u8>>,
    pub(super) guides_area: gtk::DrawingArea,
    pub(super) guides_button: gtk::Button,
}

impl State {
    pub(super) fn new() -> Self {
        Self {
            face_names_area: gtk::DrawingArea::new(),
            show_face_names: Rc::new(Cell::new(false)),
            face_names: Rc::new(RefCell::new(Vec::new())),
            guides: Rc::new(Cell::new(0)),
            guides_area: gtk::DrawingArea::new(),
            guides_button: gtk::Button::new(),
        }
    }
}
