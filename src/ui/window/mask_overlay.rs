use super::*;

pub(super) fn build_mask_overlay(state: &App) -> gtk::DrawingArea {
    let area = state.mask_overlay.area.clone();
    area.set_visible(false);
    area.set_can_target(true);

    let grabbed: Rc<RefCell<Option<(Handle, Shape, f32, f32)>>> = Rc::new(RefCell::new(None));

    let painting: Rc<RefCell<Option<Stroke>>> = Rc::new(RefCell::new(None));

    connect_overlay_draw(state, &area, &painting);

    let moved: Rc<Cell<bool>> = Rc::new(Cell::new(false));
    let drag = gtk::GestureDrag::new();
    connect_overlay_drag_begin(state, &drag, &moved, &grabbed, &painting);
    connect_overlay_drag_update(state, &drag, &moved, &grabbed, &painting);
    connect_overlay_drag_end(state, &drag, &grabbed, &painting);
    area.add_controller(drag);

    let motion = gtk::EventControllerMotion::new();
    motion.connect_motion(glib::clone!(
        #[strong] state,
        move |_, x, y| {
            if state.masks.brush.get() == MaskTool::Off {
                return;
            }
            state.masks.brush_at.set(Some((x as f32, y as f32)));
            state.mask_overlay.area.queue_draw();
        }
    ));
    motion.connect_leave(glib::clone!(
        #[strong] state,
        move |_| {
            state.masks.brush_at.set(None);
            state.mask_overlay.area.queue_draw();
        }
    ));
    area.add_controller(motion);

    let click = overlay_point_click(state, &moved);
    area.add_controller(click);

    area
}

fn connect_overlay_draw(
    state: &App,
    area: &gtk::DrawingArea,
    painting: &Rc<RefCell<Option<Stroke>>>,
) {
    area.set_draw_func(glib::clone!(
        #[strong] state,
        #[strong] painting,
        move |_, context, width, height| {
            let content = content_rect(&state, width as f64, height as f64);
            let Some(mask) = selected_mask(&state) else {

                if state.mask_overlay.previewing.get() && state.mask_overlay.show_ants.get() {
                    draw_ants(context, content, &state.mask_overlay.outline.borrow(), state.mask_overlay.ants_phase.get());
                }
                return;
            };

            masks::draw_coverage(&state, context, content, mask.visible);

            if state.mask_overlay.show_ants.get() {
                draw_ants(
                    context,
                    content,
                    &state.mask_overlay.outline.borrow(),
                    state.mask_overlay.ants_phase.get(),
                );
            }

            draw_mask(context, content, &mask.shape);

            if state.mask_overlay.show_dots.get() {
                draw_dots(context, content, &state.mask_overlay.dot_cache.borrow());
            }

            match (state.masks.brush.get(), painting.borrow().as_ref()) {

                (MaskTool::Lasso, Some(stroke)) if stroke.fill => {
                    draw_lasso(context, content, &stroke.points, stroke.erase)
                }
                (MaskTool::Brush, painting) => {

                    if let Some(stroke) = painting.filter(|_| !state.mask_overlay.show_coverage.get()) {
                        draw_stroke(context, content, stroke);
                    }
                    draw_brush(context, content, &state)
                }
                _ => {}
            }
        }
    ));
}

fn connect_overlay_drag_begin(
    state: &App,
    drag: &gtk::GestureDrag,
    moved: &Rc<Cell<bool>>,
    grabbed: &Rc<RefCell<Option<(Handle, Shape, f32, f32)>>>,
    painting: &Rc<RefCell<Option<Stroke>>>,
) {
    drag.connect_drag_begin(glib::clone!(
        #[strong] state,
        #[strong] moved,
        #[strong] grabbed,
        #[strong] painting,
        move |gesture, x, y| {
            moved.set(false);
            let Some(mask) = selected_mask(&state) else { return };
            let Some((u, v)) = mask_point(&state, x, y) else { return };

            let claim = |gesture: &gtk::GestureDrag| {
                gesture.set_state(gtk::EventSequenceState::Claimed);
            };

            let tool = state.masks.brush.get();
            if tool == MaskTool::Off {

                if !matches!(mask.shape, Shape::Linear { .. } | Shape::Radial { .. }) {
                    return;
                }
                let handle = nearest_mask_handle(&mask.shape, u, v);
                *grabbed.borrow_mut() = Some((handle, mask.shape.clone(), u, v));
                claim(gesture);
                return;
            }
            claim(gesture);
            let away = taking_away(&state, gesture.current_event_state());

            if mask.map.0.is_none() {
                if let Some(index) = state.mask_overlay.selected_mask.get() {
                    rebuild_mask_map(&state, index);
                }
            }

            let mut stroke = match tool {

                MaskTool::Lasso => Stroke::soft_lasso(
                    state.masks.brush_radius.get(),
                    state.masks.toolbar.softness.get(),
                    away,
                ),

                _ => Stroke::new(state.masks.brush_radius.get(), state.masks.toolbar.softness.get(), away),
            };
            stroke.points.push([u, v]);
            if !stroke.fill {
                paint_segment(&state, [u, v], [u, v], &stroke);
            }
            *painting.borrow_mut() = Some(stroke);
        }
    ));
}

fn connect_overlay_drag_update(
    state: &App,
    drag: &gtk::GestureDrag,
    moved: &Rc<Cell<bool>>,
    grabbed: &Rc<RefCell<Option<(Handle, Shape, f32, f32)>>>,
    painting: &Rc<RefCell<Option<Stroke>>>,
) {
    drag.connect_drag_update(glib::clone!(
        #[strong] state,
        #[strong] moved,
        #[strong] grabbed,
        #[strong] painting,
        move |gesture, dx, dy| {
            moved.set(true);
            let Some((start_x, start_y)) = gesture.start_point() else { return };
            let Some((u, v)) = mask_point(&state, start_x + dx, start_y + dy) else { return };

            if let Some(stroke) = painting.borrow_mut().as_mut() {
                let last = *stroke.points.last().unwrap_or(&[u, v]);

                stroke.points.push([u, v]);
                if stroke.fill {

                    state.mask_overlay.area.queue_draw();
                } else {
                    paint_segment(&state, last, [u, v], stroke);
                }
                return;
            }

            let Some((handle, original, from_u, from_v)) = grabbed.borrow().clone() else { return };
            let moved = move_mask_handle(original, handle, [u - from_u, v - from_v], [u, v]);
            update_mask_shape(&state, moved);
        }
    ));
}

fn connect_overlay_drag_end(
    state: &App,
    drag: &gtk::GestureDrag,
    grabbed: &Rc<RefCell<Option<(Handle, Shape, f32, f32)>>>,
    painting: &Rc<RefCell<Option<Stroke>>>,
) {
    drag.connect_drag_end(glib::clone!(
        #[strong] grabbed,
        #[strong] state,
        #[strong] painting,
        move |_, _, _| {
            let dragged = grabbed.borrow_mut().take().is_some();

            let finished = painting.borrow_mut().take();

            if let Some(stroke) = finished {
                if let Some(index) = state.mask_overlay.selected_mask.get() {
                    let filled = stroke.fill;
                    {
                        let mut open = state.open.borrow_mut();
                        if let Some(photo) = open.as_mut() {
                            if let Some(mask) = photo.document.mask_mut(index) {
                                mask.strokes.push(stroke);
                            }
                        }
                    }

                    if filled {
                        rebuild_mask_map(&state, index);
                    } else {

                        refresh_outline(&state);
                    }
                    request_render(&state);
                    show_coverage(&state);
                }
                refresh_masks(&state);
            }

            if dragged {
                refresh_outline(&state);
            }
            schedule_history_push(&state);
        }
    ));
}

fn overlay_point_click(state: &App, moved: &Rc<Cell<bool>>) -> gtk::GestureClick {
    let click = gtk::GestureClick::new();
    click.connect_released(glib::clone!(
        #[strong] state,
        #[strong] moved,
        move |gesture, _, x, y| {

            if state.masks.brush.get() != MaskTool::Off || moved.get() {
                return;
            }
            let Some(mask) = selected_mask(&state) else { return };
            let Some((u, v)) = mask_point(&state, x, y) else { return };

            if is_gradient(&mask) {
                return;
            }

            if matches!(mask.shape, Shape::ColourRange { .. } | Shape::LuminanceRange { .. }) {
                pick_range(&state, u, v);
                return;
            }

            if state.mask_overlay.show_dots.get() {
                let content = content_rect(
                    &state,
                    state.mask_overlay.area.width() as f64,
                    state.mask_overlay.area.height() as f64,
                );
                let (left, top, width, height) = content;
                let index = state.mask_overlay.selected_mask.get().unwrap_or(0);
                for (at, _, part) in mask_dots(&state, &mask) {
                    let dx = left + at[0] as f64 * width - x;
                    let dy = top + at[1] as f64 * height - y;
                    if dx.hypot(dy) <= DOT_REACH {
                        toggle_mask_part(&state, index, part);
                        return;
                    }
                }
            }

            if (0.0..=1.0).contains(&u) && (0.0..=1.0).contains(&v) {
                point_at(&state, u, v, taking_away(&state, gesture.current_event_state()));
            }
        }
    ));
    click
}

pub(super) fn is_gradient(mask: &Mask) -> bool {
    matches!(mask.shape, Shape::Linear { .. } | Shape::Radial { .. })
        && mask.strokes.is_empty()
        && mask.points.is_empty()
}

pub(super) fn mask_name(mask: &Mask) -> String {

    if let Some(name) = mask.name.as_ref().filter(|name| !name.trim().is_empty()) {
        return name.trim().to_string();
    }
    match &mask.shape {
        Shape::Linear { .. } => "Linear".to_string(),
        Shape::Radial { .. } => "Radial".to_string(),
        Shape::Segment { classes } => segment::name_for(classes),

        Shape::Painted if mask.strokes.is_empty() && !mask.points.is_empty() => "Click".to_string(),
        Shape::Painted => "Brush".to_string(),
        Shape::ColourRange { .. } => "Colour range".to_string(),
        Shape::LuminanceRange { .. } => "Luminance range".to_string(),
    }
}

pub(super) fn mask_label(masks: &[Mask], index: usize) -> String {
    let name = mask_name(&masks[index]);
    let same: Vec<usize> =
        (0..masks.len()).filter(|other| mask_name(&masks[*other]) == name).collect();
    match same.len() > 1 {
        true => {
            format!("{name} {}", same.iter().position(|other| *other == index).unwrap_or(0) + 1)
        }
        false => name,
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Handle {
    Start,
    End,
    Centre,
    EdgeX,
    EdgeY,
    Whole,
}

pub(super) const MASK_GRAB: f32 = 0.05;

pub(super) fn nearest_mask_handle(shape: &Shape, u: f32, v: f32) -> Handle {
    let near = |point: [f32; 2]| {
        let (dx, dy) = (u - point[0], v - point[1]);
        (dx * dx + dy * dy).sqrt() <= MASK_GRAB
    };

    match shape {
        Shape::Linear { from, to } => {
            if near(*from) {
                Handle::Start
            } else if near(*to) {
                Handle::End
            } else {
                Handle::Whole
            }
        }

        Shape::Segment { .. }
        | Shape::Painted
        | Shape::ColourRange { .. }
        | Shape::LuminanceRange { .. } => Handle::Whole,
        Shape::Radial { centre, radius, .. } => {
            if near(*centre) {
                Handle::Centre
            } else if near([centre[0] + radius[0], centre[1]]) {
                Handle::EdgeX
            } else if near([centre[0], centre[1] + radius[1]]) {
                Handle::EdgeY
            } else {
                Handle::Whole
            }
        }
    }
}

pub(super) fn move_mask_handle(shape: Shape, handle: Handle, shift: [f32; 2], at: [f32; 2]) -> Shape {
    let clamp = |point: [f32; 2]| [point[0].clamp(-0.5, 1.5), point[1].clamp(-0.5, 1.5)];

    match (shape, handle) {

        (shape @ (Shape::ColourRange { .. } | Shape::LuminanceRange { .. }), _) => shape,

        (Shape::Linear { to, .. }, Handle::Start) => Shape::Linear { from: clamp(at), to },
        (Shape::Linear { from, .. }, Handle::End) => Shape::Linear { from, to: clamp(at) },
        (Shape::Linear { from, to }, _) => Shape::Linear {
            from: clamp([from[0] + shift[0], from[1] + shift[1]]),
            to: clamp([to[0] + shift[0], to[1] + shift[1]]),
        },

        (Shape::Radial { radius, feather, .. }, Handle::Centre) => {
            Shape::Radial { centre: clamp(at), radius, feather }
        }
        (Shape::Radial { centre, radius, feather }, Handle::EdgeX) => Shape::Radial {
            centre,

            radius: [(at[0] - centre[0]).abs().max(0.01), radius[1]],
            feather,
        },
        (Shape::Radial { centre, radius, feather }, Handle::EdgeY) => Shape::Radial {
            centre,
            radius: [radius[0], (at[1] - centre[1]).abs().max(0.01)],
            feather,
        },
        (Shape::Radial { centre, radius, feather }, _) => Shape::Radial {
            centre: clamp([centre[0] + shift[0], centre[1] + shift[1]]),
            radius,
            feather,
        },

        (shape @ (Shape::Segment { .. } | Shape::Painted), _) => shape,
    }
}

pub(super) fn selected_mask(state: &App) -> Option<Mask> {
    let index = state.mask_overlay.selected_mask.get()?;
    let open = state.open.borrow();
    open.as_ref()?.document.masks().get(index).cloned()
}

pub(super) fn canvas_point(state: &App, x: f64, y: f64) -> Option<(f32, f32)> {
    let (left, top, width, height) =
        content_rect(state, state.canvas.width() as f64, state.canvas.height() as f64);
    (width > 0.0 && height > 0.0)
        .then(|| (((x - left) / width) as f32, ((y - top) / height) as f32))
}

pub(super) fn mask_point(state: &App, x: f64, y: f64) -> Option<(f32, f32)> {
    let (left, top, width, height) = content_rect(
        state,
        state.mask_overlay.area.width() as f64,
        state.mask_overlay.area.height() as f64,
    );
    (width > 0.0 && height > 0.0)
        .then(|| (((x - left) / width) as f32, ((y - top) / height) as f32))
}

pub(super) fn update_mask_shape(state: &App, shape: Shape) {
    let Some(index) = state.mask_overlay.selected_mask.get() else { return };
    let painted = {
        let mut open = state.open.borrow_mut();
        let Some(photo) = open.as_mut() else { return };
        let Some(mask) = photo.document.mask_mut(index) else { return };
        mask.shape = shape;
        photo.view = None;
        mask.wants_pixels()
    };

    if painted {
        rebuild_mask_map(state, index);
    }

    state.mask_overlay.area.queue_draw();
    request_render(state);
}

pub(super) fn mask_dots(state: &App, mask: &Mask) -> Vec<([f32; 2], bool, MaskPart)> {
    let mut dots = Vec::new();

    if let Shape::Segment { classes } = &mask.shape {
        let found = state.open.borrow().as_ref().and_then(|photo| photo.segmentation.clone());
        if let Some(found) = found {
            for class in classes {
                if let Some(at) = found.centre_of(*class) {
                    dots.push((at, !mask.muted.contains(class), MaskPart::Class(*class)));
                }
            }
        }
    }

    for (index, point) in mask.points.iter().enumerate() {
        dots.push((point.at, point.enabled, MaskPart::Point(index)));
    }

    dots
}

pub(super) const DOT_REACH: f64 = 13.0;

pub(super) fn draw_dots(
    context: &gtk::cairo::Context,
    content: (f64, f64, f64, f64),
    dots: &[([f32; 2], bool, MaskPart)],
) {
    let (left, top, width, height) = content;
    for (at, on, _) in dots {
        let (x, y) = (left + at[0] as f64 * width, top + at[1] as f64 * height);

        context.arc(x, y, 7.0, 0.0, std::f64::consts::TAU);
        context.set_source_rgba(0.0, 0.0, 0.0, 0.55);
        context.set_line_width(3.5);
        let _ = context.stroke();

        context.arc(x, y, 7.0, 0.0, std::f64::consts::TAU);
        if *on {
            context.set_source_rgba(1.0, 1.0, 1.0, 0.95);
        } else {
            context.set_source_rgba(1.0, 1.0, 1.0, 0.45);
        }
        context.set_line_width(1.6);
        let _ = context.stroke();

        if *on {

            context.arc(x, y, 3.0, 0.0, std::f64::consts::TAU);
            context.set_source_rgba(1.0, 1.0, 1.0, 0.95);
            let _ = context.fill();
        }
    }
}

pub(super) fn draw_ants(
    context: &gtk::cairo::Context,
    content: (f64, f64, f64, f64),
    paths: &[Vec<[f32; 2]>],
    phase: f64,
) {
    if paths.is_empty() {
        return;
    }
    let (left, top, width, height) = content;
    const PERIOD: f64 = 12.0;

    for (colour, offset) in [((0.0, 0.0, 0.0, 0.85), 0.0), ((1.0, 1.0, 1.0, 0.95), PERIOD / 2.0)] {
        context.set_source_rgba(colour.0, colour.1, colour.2, colour.3);
        context.set_line_width(1.5);
        context.set_dash(&[PERIOD / 2.0, PERIOD / 2.0], phase + offset);
        for path in paths {
            let mut points = path.iter();
            let Some(first) = points.next() else { continue };
            context.move_to(left + first[0] as f64 * width, top + first[1] as f64 * height);
            for at in points {
                context.line_to(left + at[0] as f64 * width, top + at[1] as f64 * height);
            }
            let _ = context.stroke();
        }
    }
    context.set_dash(&[], 0.0);
}

pub(super) const OUTLINE_EDGE: usize = 640;

pub(super) fn gradient_alpha(state: &App, mask: &Mask) -> Option<std::sync::Arc<numa::core::mask::Stored>> {
    if mask.map.0.is_some() || !matches!(mask.shape, Shape::Linear { .. } | Shape::Radial { .. }) {
        return None;
    }
    let (width, height) = mask_raster_size(state);
    if width == 0 || height == 0 {
        return None;
    }

    use rayon::prelude::*;
    let data = (0..width * height)
        .into_par_iter()
        .map(|index| {
            let u = ((index % width) as f32 + 0.5) / width as f32;
            let v = ((index / width) as f32 + 0.5) / height as f32;
            mask.shape.weight(u, v)
        })
        .collect();
    Some(std::sync::Arc::new(numa::core::mask::Stored::new(&Alpha::new(width, height, data))))
}

pub(super) fn refresh_outline(state: &App) {
    let selected = selected_mask(state);
    let made = selected.as_ref().and_then(|mask| gradient_alpha(state, mask));
    let traced = selected
        .as_ref()
        .and_then(|mask| {
            let visible = mask.visible;
            mask.map.0.as_ref().or(made.as_ref()).filter(|_| visible).map(|alpha| {
                let mut paths = numa::core::mask::outline(alpha, OUTLINE_EDGE);

                if paths.len() > 400 {
                    paths.truncate(400);
                }
                paths
            })
        })
        .unwrap_or_default();

    *state.mask_overlay.outline.borrow_mut() = traced;

    let wash = selected.as_ref().and_then(|mask| {
        let alpha = mask.map.0.as_ref().or(made.as_ref())?;
        state.mask_overlay.wash_size.set((alpha.width, alpha.height));
        build_wash(alpha, mask.inverted, mask.opacity)
    });
    *state.mask_overlay.wash.borrow_mut() = wash;
    *state.mask_overlay.dot_cache.borrow_mut() = match &selected {
        Some(mask) => mask_dots(state, mask),
        None => Vec::new(),
    };

    start_ants(state);
    state.mask_overlay.area.queue_draw();
}

pub(super) fn start_ants(state: &App) {
    if state.mask_overlay.ants_running.get() || state.mask_overlay.outline.borrow().is_empty() {
        return;
    }
    state.mask_overlay.ants_running.set(true);

    let state = state.clone();
    state.mask_overlay.area.clone().add_tick_callback(move |_, clock| {
        if state.mask_overlay.outline.borrow().is_empty() || !state.mask_overlay.area.is_visible() {
            state.mask_overlay.ants_running.set(false);
            return glib::ControlFlow::Break;
        }

        let seconds = clock.frame_time() as f64 / 1_000_000.0;
        state.mask_overlay.ants_phase.set(-(seconds * 8.0) % 12.0);
        state.mask_overlay.area.queue_draw();
        glib::ControlFlow::Continue
    });
}

pub(super) fn draw_brush(context: &gtk::cairo::Context, content: (f64, f64, f64, f64), state: &App) {
    let Some((x, y)) = state.masks.brush_at.get() else { return };
    let (_, _, width, height) = content;
    let radius = state.masks.brush_radius.get() as f64 * width.max(height);

    context.arc(x as f64, y as f64, radius.max(1.0), 0.0, std::f64::consts::TAU);

    context.set_line_width(3.0);
    context.set_source_rgba(0.0, 0.0, 0.0, 0.5);
    let _ = context.stroke_preserve();
    context.set_line_width(1.2);
    context.set_source_rgba(1.0, 1.0, 1.0, 0.95);
    let _ = context.stroke();
}

pub(super) fn draw_lasso(
    context: &gtk::cairo::Context,
    content: (f64, f64, f64, f64),
    points: &[[f32; 2]],
    taking_away: bool,
) {
    let (left, top, width, height) = content;
    let Some(first) = points.first() else { return };

    context.move_to(left + first[0] as f64 * width, top + first[1] as f64 * height);
    for point in &points[1..] {
        context.line_to(left + point[0] as f64 * width, top + point[1] as f64 * height);
    }
    context.close_path();

    context.set_line_width(3.0);
    context.set_source_rgba(0.0, 0.0, 0.0, 0.5);
    let _ = context.stroke_preserve();
    context.set_line_width(1.2);
    if taking_away {
        context.set_source_rgba(1.0, 0.45, 0.4, 0.95);
    } else {
        context.set_source_rgba(1.0, 1.0, 1.0, 0.95);
    }
    let _ = context.stroke();
}

pub(super) fn draw_stroke(context: &gtk::cairo::Context, content: (f64, f64, f64, f64), stroke: &Stroke) {
    let (left, top, width, height) = content;
    let long = width.max(height);
    context.set_line_width((stroke.radius as f64 * 2.0 * long).max(1.0));
    context.set_line_cap(gtk::cairo::LineCap::Round);
    context.set_line_join(gtk::cairo::LineJoin::Round);

    if stroke.erase {
        context.set_source_rgba(1.0, 1.0, 1.0, 0.35);
    } else {
        context.set_source_rgba(0.95, 0.3, 0.3, 0.4);
    }

    let mut points = stroke.points.iter();
    let Some(first) = points.next() else { return };
    context.move_to(left + first[0] as f64 * width, top + first[1] as f64 * height);
    for at in points {
        context.line_to(left + at[0] as f64 * width, top + at[1] as f64 * height);
    }

    if stroke.points.len() == 1 {
        context.line_to(left + first[0] as f64 * width + 0.01, top + first[1] as f64 * height);
    }
    let _ = context.stroke();
}

pub(super) fn paint_wash(
    surface: &mut gtk::cairo::ImageSurface,
    alpha: &numa::core::mask::Stored,
    inverted: bool,
    opacity: f32,
    bounds: (usize, usize, usize, usize),
) {
    let stride = surface.stride() as usize;
    let Ok(mut pixels) = surface.data() else { return };
    let opacity = opacity.clamp(0.0, 1.0);
    let (left, top, right, bottom) =
        (bounds.0, bounds.1, bounds.2.min(alpha.width), bounds.3.min(alpha.height));
    if top >= bottom {
        return;
    }
    use rayon::prelude::*;
    pixels[top * stride..bottom * stride].par_chunks_mut(stride).enumerate().for_each(|(row, line)| {
        let y = top + row;
        for x in left..right {
            let value = alpha.at(y * alpha.width + x);
            let value = if inverted { 1.0 - value } else { value };

            let value = value * opacity;
            line[x] = (value * 255.0).clamp(0.0, 255.0) as u8;
        }
    });
}

pub(super) fn draw_mask(context: &gtk::cairo::Context, content: (f64, f64, f64, f64), shape: &Shape) {
    let (left, top, width, height) = content;
    let at = |point: [f32; 2]| (left + point[0] as f64 * width, top + point[1] as f64 * height);

    context.set_line_width(1.5);
    context.set_source_rgba(1.0, 1.0, 1.0, 0.85);

    let handles: Vec<(f64, f64)> = match shape {

        Shape::Segment { .. }
        | Shape::Painted
        | Shape::ColourRange { .. }
        | Shape::LuminanceRange { .. } => Vec::new(),
        Shape::Linear { from, to } => {
            let (x0, y0) = at(*from);
            let (x1, y1) = at(*to);

            context.move_to(x0, y0);
            context.line_to(x1, y1);
            let _ = context.stroke();

            let (dx, dy) = (x1 - x0, y1 - y0);
            let length = (dx * dx + dy * dy).sqrt().max(1.0);
            let (across_x, across_y) = (-dy / length * 400.0, dx / length * 400.0);
            context.set_source_rgba(1.0, 1.0, 1.0, 0.35);
            for (x, y) in [(x0, y0), (x1, y1)] {
                context.move_to(x - across_x, y - across_y);
                context.line_to(x + across_x, y + across_y);
            }
            let _ = context.stroke();

            vec![(x0, y0), (x1, y1)]
        }
        Shape::Radial { centre, radius, feather } => {
            let (cx, cy) = at(*centre);
            let (rx, ry) = (*radius as [f32; 2]).map(f64::from).into();
            let (rx, ry) = (rx * width, ry * height);

            let ellipse = |context: &gtk::cairo::Context, scale: f64| {
                let _ = context.save();
                context.translate(cx, cy);
                context.scale((rx * scale).max(0.1), (ry * scale).max(0.1));
                context.arc(0.0, 0.0, 1.0, 0.0, std::f64::consts::TAU);
                let _ = context.restore();
                let _ = context.stroke();
            };

            ellipse(context, 1.0);

            context.set_source_rgba(1.0, 1.0, 1.0, 0.35);
            ellipse(context, (1.0 - *feather as f64).max(0.05));

            vec![(cx, cy), (cx + rx, cy), (cx, cy + ry)]
        }
    };

    context.set_source_rgb(1.0, 1.0, 1.0);
    for (x, y) in handles {
        context.arc(x, y, 5.0, 0.0, std::f64::consts::TAU);
        let _ = context.fill();
    }
}

#[derive(Clone)]
pub(super) struct State {

    pub(super) area: gtk::DrawingArea,

    pub(super) outline: Rc<RefCell<Vec<Vec<[f32; 2]>>>>,

    pub(super) wash: Rc<RefCell<Option<gtk::cairo::ImageSurface>>>,
    pub(super) wash_size: Rc<Cell<(usize, usize)>>,
    pub(super) dot_cache: Rc<RefCell<Vec<([f32; 2], bool, MaskPart)>>>,
    pub(super) ants_phase: Rc<Cell<f64>>,
    pub(super) ants_running: Rc<Cell<bool>>,
    pub(super) show_ants: Rc<Cell<bool>>,

    pub(super) show_dots: Rc<Cell<bool>>,

    pub(super) show_coverage: Rc<Cell<bool>>,

    pub(super) brush_owner: Rc<Cell<Option<usize>>>,

    pub(super) previewing: Rc<Cell<bool>>,

    pub(super) selected_mask: Rc<Cell<Option<usize>>>,

    pub(super) sliders_hold: Rc<Cell<Option<usize>>>,
}

impl State {
    pub(super) fn new() -> Self {
        Self {
            area: gtk::DrawingArea::new(),
            outline: Rc::new(RefCell::new(Vec::new())),
            wash: Rc::new(RefCell::new(None)),
            wash_size: Rc::new(Cell::new((0, 0))),
            dot_cache: Rc::new(RefCell::new(Vec::new())),
            ants_phase: Rc::new(Cell::new(0.0)),
            ants_running: Rc::new(Cell::new(false)),
            show_ants: Rc::new(Cell::new(true)),
            show_dots: Rc::new(Cell::new(true)),
            show_coverage: Rc::new(Cell::new(true)),
            brush_owner: Rc::new(Cell::new(None)),
            previewing: Rc::new(Cell::new(false)),
            selected_mask: Rc::new(Cell::new(None)),
            sliders_hold: Rc::new(Cell::new(None)),
        }
    }
}
