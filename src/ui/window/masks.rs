use super::*;

pub(super) const DEFAULT_BRUSH: f32 = 0.05;

pub(super) const BRUSH_SMALLEST: f32 = 0.0008;
pub(super) const BRUSH_LARGEST: f32 = 0.25;

pub(super) fn brush_size(travel: f64) -> f32 {
    let travel = travel.clamp(0.0, 1.0) as f32;
    BRUSH_SMALLEST + travel * travel * (BRUSH_LARGEST - BRUSH_SMALLEST)
}

pub(super) fn brush_travel(radius: f32) -> f64 {
    (((radius - BRUSH_SMALLEST) / (BRUSH_LARGEST - BRUSH_SMALLEST)).max(0.0).sqrt()) as f64
}

pub(super) fn leave_mask(state: &App) {
    let left = state.mask_overlay.selected_mask.get();
    select_mask(state, None);
    show_panel_tab(state, "masks");
    if let Some((_, tab)) = state.panel.tabs.borrow().iter().find(|(at, _)| *at == "masks") {
        tab.set_active(true);
    }
    if let Some(row) = left.and_then(|index| state.masks.mask_list.row_at_index(index as i32)) {
        row.grab_focus();
    }
}

#[derive(Clone)]
pub(super) struct State {

    pub(super) toolbar: Toolbar,

    pub(super) found_box: gtk::FlowBox,

    pub(super) strength: gtk::Scale,
    pub(super) feather: gtk::Scale,
    pub(super) edge: gtk::Scale,
    pub(super) mask_list: gtk::ListBox,

    pub(super) mask_parts: gtk::ListBox,
    pub(super) mask_parts_header: gtk::Label,

    pub(super) brush: Rc<Cell<MaskTool>>,
    pub(super) brush_radius: Rc<Cell<f32>>,

    pub(super) show_matte: Rc<Cell<bool>>,

    fine_frame: Rc<RefCell<Option<(String, Arc<image::RgbImage>)>>>,

    tracing: Rc<Cell<bool>>,
    trace_again: Rc<Cell<Option<usize>>>,

    searching: Rc<Cell<bool>>,

    pub(super) looking: Rc<Cell<bool>>,

    pub(super) brush_at: Rc<Cell<Option<(f32, f32)>>>,
    pub(super) mask_empty: gtk::Label,
}

impl State {
    pub(super) fn new() -> Self {
        Self {
            toolbar: Toolbar::new(),
            found_box: gtk::FlowBox::new(),
            strength: gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 100.0, 1.0),
            feather: gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 100.0, 1.0),
            edge: gtk::Scale::with_range(gtk::Orientation::Horizontal, -100.0, 100.0, 1.0),
            mask_list: gtk::ListBox::new(),
            mask_parts: gtk::ListBox::new(),
            mask_parts_header: gtk::Label::new(None),
            brush: Rc::new(Cell::new(MaskTool::Off)),
            brush_radius: Rc::new(Cell::new(DEFAULT_BRUSH)),
            show_matte: Rc::new(Cell::new(false)),
            fine_frame: Rc::new(RefCell::new(None)),
            tracing: Rc::new(Cell::new(false)),
            trace_again: Rc::new(Cell::new(None)),
            searching: Rc::new(Cell::new(false)),
            looking: Rc::new(Cell::new(false)),
            brush_at: Rc::new(Cell::new(None)),
            mask_empty: gtk::Label::new(None),
        }
    }
}

pub(super) fn build_masks(state: &App) -> gtk::Box {
    let column = gtk::Box::new(gtk::Orientation::Vertical, 8);

    let add = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    add.add_css_class("linked");
    add.add_css_class("mask-add");

    for (label, kind) in [
        ("Linear", MaskKind::Linear),
        ("Radial", MaskKind::Radial),
        ("Brush", MaskKind::Brush),
        ("Click", MaskKind::Click),
    ] {
        let button = gtk::Button::with_label(label);
        button.set_hexpand(true);

        if kind == MaskKind::Click && !segment::is_installed() && !sam::is_installed() {
            button.set_sensitive(false);
            button.set_tooltip_text(Some("Needs a model — see Preferences"));
        }
        button.connect_clicked(glib::clone!(
            #[strong] state,
            move |_| add_mask(&state, kind)
        ));
        add.append(&button);
    }
    column.append(&add);

    let found = state.masks.found_box.clone();
    found.set_selection_mode(gtk::SelectionMode::None);
    found.set_max_children_per_line(2);
    found.set_homogeneous(true);
    found.set_row_spacing(4);
    found.set_column_spacing(4);
    if segment::is_installed() {
        column.append(&section_header("In this photograph"));
        column.append(&found);
    }

    column.append(&section_header("Find by range"));
    let ranges = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    ranges.add_css_class("linked");
    for (label, kind, tooltip) in [
        ("Colour", MaskKind::ColourRange, "Every pixel of one colour, wherever it is"),
        ("Brightness", MaskKind::LuminanceRange, "Every pixel between two brightnesses"),
    ] {
        let button = gtk::Button::with_label(label);
        button.set_hexpand(true);
        button.set_tooltip_text(Some(tooltip));
        button.connect_clicked(glib::clone!(
            #[strong] state,
            move |_| add_mask(&state, kind)
        ));
        ranges.append(&button);
    }
    column.append(&ranges);

    ranges.set_tooltip_text(Some(
        "Then click the photograph to add what is under the cursor. \
         Shift-click, Shift-drag or Shift-lasso takes it away again.",
    ));
    let note = gtk::Label::new(Some(if segment::is_installed() {
        ""
    } else {
        "No model installed — Preferences (Ctrl+,) says which and where."
    }));
    note.set_visible(!segment::is_installed());
    note.set_xalign(0.0);
    note.set_wrap(true);
    note.add_css_class("profile-note");
    column.append(&note);

    state.masks.mask_list.set_selection_mode(gtk::SelectionMode::None);
    state.masks.mask_list.add_css_class("boxed-list");
    column.append(&state.masks.mask_list);

    let empty = state.masks.mask_empty.clone();

    empty.set_text("No masks yet.");
    empty.set_xalign(0.0);
    empty.set_wrap(true);
    empty.add_css_class("profile-note");
    column.append(&empty);

    column
}

pub(super) fn build_wash(alpha: &numa::core::mask::Stored, inverted: bool) -> Option<gtk::cairo::ImageSurface> {
    if alpha.width == 0 || alpha.height == 0 {
        return None;
    }
    let mut surface = gtk::cairo::ImageSurface::create(
        gtk::cairo::Format::A8,
        alpha.width as i32,
        alpha.height as i32,
    )
    .ok()?;
    paint_wash(&mut surface, alpha, inverted, (0, 0, alpha.width, alpha.height));
    Some(surface)
}

pub(super) fn name_selected_mask(state: &App) {
    if let Some(index) = state.mask_overlay.selected_mask.get() {
        let open = state.open.borrow();
        if let Some(photo) = open.as_ref() {
            let masks = photo.document.masks();
            let name = mask_label(&masks, index);
            let parts = masks.get(index).map(parts_of).unwrap_or(0);
            state.editor_page.mask_name_label.set_text(&name);
            state.editor_page.mask_where_label.set_text(&format!(
                "{} of {} \u{b7} {parts} part{}",
                index + 1,
                masks.len().max(1),
                if parts == 1 { "" } else { "s" }
            ));
            state.editor_page.mask_crumb_label.set_text(&name);
            state.editor_page.banner_label.set_markup(&format!(
                "These four tabs edit <b>{}</b>",
                glib::markup_escape_text(&name)
            ));
        }
    }
}

pub(super) fn set_panel_scope(state: &App) {
    let cropping = is_cropping(state);
    let masked = state.mask_overlay.selected_mask.get().is_some() && !cropping;

    for (widget, in_a_mask) in state.panel.scoped.borrow().iter() {
        widget.set_visible(masked == *in_a_mask);
    }

    state.light.curve_area.set_content_height(match masked {
        true => CURVE_EDGE - 56,
        false => CURVE_EDGE,
    });

    state.crop.controls.set_visible(cropping);

    state.editor_page.banner.set_visible(masked);
    state.masks.toolbar.bin.set_visible(masked);
    if !masked {
        state.masks.toolbar.drawing.set_visible(false);
    }
    state.filmstrip.scroller.set_visible(!masked);
    state.editor_page.strip_line.set_visible(!masked);
    match masked {
        true => state.editor_page.viewport.add_css_class("editing-mask"),
        false => state.editor_page.viewport.remove_css_class("editing-mask"),
    }

    for (name, tab) in state.panel.tabs.borrow().iter() {
        tab.set_visible(!masked || MASK_TABS.contains(name));
    }
    let showing = state.panel.stack.visible_child_name().unwrap_or_default();
    if masked && !MASK_TABS.contains(&showing.as_str()) {
        show_panel_tab(state, "light");
        if let Some((_, tab)) = state.panel.tabs.borrow().iter().find(|(at, _)| *at == "light") {
            tab.set_active(true);
        }
    }

    state.editor_page.mask_crumb.set_visible(masked);
    name_selected_mask(state);

    if selected_mask(state).is_some_and(|mask| is_gradient(&mask)) && state.masks.brush.get() != MaskTool::Off {
        state.masks.brush.set(MaskTool::Off);
    }
    if masked {
        refresh_mask_toolbar(state);
    }
    state.panel.tab_strip.set_visible(true);
}

pub(super) fn append_range_rows(state: &App, mask: &Mask, index: usize) -> bool {

    let ranged: &[(&str, &str, f64, f64, f64, f64, u8)] = match &mask.shape {
        Shape::ColourRange { hue, spread, saturation, .. } => &[
            ("Hue", "Which colour, in degrees round the wheel", 0.0, 359.0, 1.0, *hue as f64, 0),
            ("Spread", "How far either side of it still counts", 1.0, 180.0, 1.0, *spread as f64, 1),
            (
                "Minimum saturation",
                "Below this a pixel is grey, and grey has no colour to match",
                0.0,
                100.0,
                1.0,
                (*saturation * 100.0) as f64,
                2,
            ),
        ],
        Shape::LuminanceRange { low, high, softness, .. } => &[
            ("From", "The dark end of the range", 0.0, 100.0, 1.0, (*low * 100.0) as f64, 3),
            ("To", "The bright end", 0.0, 100.0, 1.0, (*high * 100.0) as f64, 4),
            (
                "Softness",
                "How gradually it lets go at both ends",
                0.0,
                100.0,
                1.0,
                (*softness * 100.0) as f64,
                5,
            ),
        ],
        _ => &[],
    };
    if !ranged.is_empty() {
        let chosen = matches!(
            mask.shape,
            Shape::ColourRange { picked: true, .. } | Shape::LuminanceRange { picked: true, .. }
        );
        if chosen {
            state.masks.mask_parts.append(&build_range_swatch(state, &mask.shape));
        }
        let hint = gtk::Label::new(Some(match chosen {
            true => "Click the photograph again to choose something else.",
            false => "Click the photograph to choose what this selects. \
                      Nothing is selected until you do.",
        }));
        hint.set_xalign(0.0);
        hint.set_wrap(true);
        hint.set_margin_start(14);
        hint.set_margin_end(14);
        hint.set_margin_bottom(4);
        hint.add_css_class("profile-note");
        state.masks.mask_parts.append(&hint);

        if !chosen {
            return false;
        }
    }
    for (title, subtitle, lo, hi, step, now, which) in ranged.iter().copied() {
        let row = adw::SpinRow::with_range(lo, hi, step);
        row.set_title_lines(1);
        row.set_title(title);
        row.set_tooltip_text(Some(subtitle));
        row.add_css_class("range-row");
        row.set_value(now);
        shift_moves_ten(&row, &row.adjustment());
        row.connect_value_notify(glib::clone!(
            #[strong] state,
            move |row| {
                if !state.applying.get() {
                    set_mask_range(&state, index, which, row.value() as f32);
                }
            }
        ));
        state.masks.mask_parts.append(&row);
    }
    true
}

pub(super) fn build_range_swatch(_state: &App, shape: &Shape) -> gtk::DrawingArea {
    let area = gtk::DrawingArea::new();
    area.set_content_height(34);
    area.set_margin_start(14);
    area.set_margin_end(14);

    area.set_margin_top(2);
    area.set_margin_bottom(4);

    let shape = shape.clone();
    area.set_draw_func(move |_, context, width, height| {
        let (width, height) = (width as f64, height as f64);
        let radius = 4.0;

        let gradient = gtk::cairo::LinearGradient::new(0.0, 0.0, width, 0.0);
        match &shape {
            Shape::ColourRange { .. } => {
                for step in 0..=12 {
                    let at = step as f64 / 12.0;
                    let (r, g, b) = hue_rgb(at * 360.0);
                    gradient.add_color_stop_rgb(at, r, g, b);
                }
            }
            _ => {
                gradient.add_color_stop_rgb(0.0, 0.0, 0.0, 0.0);
                gradient.add_color_stop_rgb(1.0, 1.0, 1.0, 1.0);
            }
        }
        rounded(context, 0.0, 0.0, width, height, radius);
        let _ = context.set_source(&gradient);
        let _ = context.fill();

        let (from, to) = match &shape {
            Shape::ColourRange { hue, spread, .. } => {
                let centre = hue.rem_euclid(360.0) / 360.0;
                let half = (spread / 360.0) as f64;
                (centre as f64 - half, centre as f64 + half)
            }
            Shape::LuminanceRange { low, high, .. } => (*low as f64, *high as f64),
            _ => (0.0, 1.0),
        };

        context.set_source_rgba(0.0, 0.0, 0.0, 0.62);

        for shift in [-1.0, 0.0, 1.0] {
            let (left, right) = ((from + shift).max(0.0), (to + shift).min(1.0));
            if right <= left {
                continue;
            }
            context.rectangle(0.0, 0.0, left * width, height);
            let _ = context.fill();
            context.rectangle(right * width, 0.0, width - right * width, height);
            let _ = context.fill();
        }

        let centre = match &shape {
            Shape::ColourRange { hue, .. } => (hue.rem_euclid(360.0) / 360.0) as f64,
            Shape::LuminanceRange { low, high, .. } => ((low + high) * 0.5) as f64,
            _ => 0.5,
        };
        let x = centre * width;
        context.set_line_width(2.0);
        context.set_source_rgba(1.0, 1.0, 1.0, 0.95);
        context.move_to(x, 0.0);
        context.line_to(x, height);
        let _ = context.stroke();
    });

    area
}

pub(super) fn ensure_embedding(state: &App) {
    if !sam::is_installed() {
        return;
    }
    let request = {
        let mut open = state.open.borrow_mut();
        let Some(photo) = open.as_mut() else { return };
        if photo.embedding.is_some() || photo.embedding_pending {
            return;
        }
        photo.embedding_pending = true;

        let mut geometry = Document::new(photo.document.source.path.clone());
        geometry.set_perspective(photo.document.perspective());
        if let Some((rect, angle)) = photo.document.crop() {
            geometry.set_crop(rect, angle);
        }
        geometry.set_rotation(photo.document.rotation());
        geometry.set_mirrored(photo.document.mirrored());
        (geometry, photo.working.clone())
    };

    let (geometry, working) = request;
    let generation = state.open_generation.get();
    let state = state.clone();
    glib::spawn_future_local(async move {
        let made = busy(&state, "Working out what can be clicked…", move || {
            let frame = render::apply_stack(&geometry, &*working, 1.0);
            sam::encode(&frame)
        })
        .await;

        if state.open_generation.get() != generation {
            return;
        }

        let waiting: Vec<usize> = {
            let mut open = state.open.borrow_mut();
            let Some(photo) = open.as_mut() else { return };
            photo.embedding_pending = false;
            photo.embedding = made.ok().flatten().map(Arc::new);
            photo
                .document
                .masks()
                .iter()
                .enumerate()
                .filter(|(_, mask)| !mask.points.is_empty())
                .map(|(index, _)| index)
                .collect()
        };

        if !waiting.is_empty() {
            for index in waiting {
                rebuild_mask_map(&state, index);
            }
            refresh_masks(&state);
            request_render(&state);
            show_coverage(&state);
        }
    });
}

pub(super) fn ensure_segmentation(state: &App) {
    let request = {
        let mut open = state.open.borrow_mut();
        let Some(photo) = open.as_mut() else { return };
        if photo.segmentation.is_some() || photo.segmenting {
            return;
        }
        photo.segmenting = true;

        let mut geometry = Document::new(photo.document.source.path.clone());
        geometry.set_perspective(photo.document.perspective());
        if let Some((rect, angle)) = photo.document.crop() {
            geometry.set_crop(rect, angle);
        }
        geometry.set_rotation(photo.document.rotation());
        geometry.set_mirrored(photo.document.mirrored());
        (geometry, photo.working.clone())
    };

    let (geometry, working) = request;

    refresh_found(state);

    let generation = state.open_generation.get();
    let state = state.clone();
    glib::spawn_future_local(async move {
        let found = busy(&state, "Finding what is in the photograph…", move || {
            let frame = render::apply_stack(&geometry, &*working, 1.0);
            segment::of(&frame)
        })
        .await;

        if state.open_generation.get() != generation {
            return;
        }

        if let Some(photo) = state.open.borrow_mut().as_mut() {
            photo.segmenting = false;
            photo.segmentation = found.ok().flatten().map(Arc::new);

            let mut masks = photo.document.masks();
            for mask in masks.iter_mut().filter(|mask| mask.wants_pixels()) {
                mask.map = Pixels(None);
            }
            photo.document.set_masks(masks);
        }

        fill_segment_masks(&state);
        refresh_found(&state);
        refresh_masks(&state);
        request_render(&state);
        state.mask_overlay.area.queue_draw();

        let Some(found) = state.open.borrow().as_ref().and_then(|photo| photo.segmentation.clone())
        else {
            return;
        };
        if !numa::render::classify::is_installed() {
            return;
        }
        let asked = Arc::downgrade(&found);
        let guess = gtk::gio::spawn_blocking(move || numa::render::classify::animal(&found)).await;
        if state.open_generation.get() != generation {
            return;
        }
        let Ok(Some(guess)) = guess else { return };
        if let Some(photo) = state.open.borrow_mut().as_mut() {
            photo.animal = Some((asked, guess));
        }
        refresh_found(&state);
    });
}

pub(super) fn settle_edges(state: &App) -> bool {
    let (width, height) = super::mask_raster_size(state);
    let mut open = state.open.borrow_mut();
    let Some(photo) = open.as_mut() else { return false };
    let mut shaped = false;
    for index in 0..photo.document.masks().len() {
        let Some(mask) = photo.document.mask_mut(index) else { continue };
        if mask.edge_is_draft(width, height) {
            shaped |= mask.reshape_edge(width, height);
        }
    }
    if shaped {
        photo.view = None;
    }
    shaped
}

pub(super) fn draw_coverage(
    state: &App,
    context: &gtk::cairo::Context,
    content: (f64, f64, f64, f64),
    visible: bool,
    opacity: f32,
) {
    let (left, top, width, height) = content;
    let opacity = opacity.clamp(0.0, 1.0) as f64;
    let matte = state.masks.show_matte.get();
    if matte {
        context.set_source_rgb(0.0, 0.0, 0.0);
        context.rectangle(left, top, width, height);
        let _ = context.fill();
    }
    let size = state.mask_overlay.wash_size.get();
    let wash = state.mask_overlay.wash.borrow();
    let Some(surface) = wash.as_ref().filter(|_| visible && (matte || wash_shown(state))) else {
        return;
    };
    if size.0 == 0 || size.1 == 0 {
        return;
    }

    let _ = context.save();
    context.translate(left, top);
    context.scale(width / size.0 as f64, height / size.1 as f64);

    match matte {
        true => context.set_source_rgba(1.0, 1.0, 1.0, opacity),

        false => context.set_source_rgba(0.95, 0.3, 0.3, 0.4 * opacity),
    }
    let _ = context.mask_surface(surface, 0.0, 0.0);
    let _ = context.restore();
}

pub(super) fn refine_selected_mask(state: &App) {
    if let Some(index) = state.mask_overlay.selected_mask.get() {
        refine_mask_edge(state, index);
    }
}

pub(super) fn refine_mask_edge(state: &App, index: usize) {
    if state.masks.searching.get() {
        return;
    }
    let (width, height) = mask_raster_size(state);
    let request = {
        let mut open = state.open.borrow_mut();
        let Some(photo) = open.as_mut() else { return };
        let segmentation = photo.segmentation.clone();
        let embedding = photo.embedding.clone();
        let Some(mask) = photo.document.mask_mut(index) else { return };

        let feathered = !mask.matte && mask.feather != 0.0;
        if feathered {
            mask.feather = 0.0;
        }
        mask.matte = true;
        mask.matte_edge = mask.shift;
        (mask.clone(), segmentation, embedding, feathered)
    };
    schedule_history_push(state);

    let (mut mask, segmentation, embedding, feathered) = request;

    if feathered {
        let (width, height) = mask_raster_size(state);
        if let Some(photo) = state.open.borrow_mut().as_mut() {
            if let Some(mask) = photo.document.mask_mut(index) {
                mask.reshape_edge(width, height);
            }
            photo.view = None;
        }
        refresh_masks(state);
        request_render(state);
    }
    let generation = state.open_generation.get();
    let state = state.clone();

    state.masks.searching.set(true);
    glib::spawn_future_local(async move {
        let worked = mask.clone();
        let resolved = busy_until(&state, "Tracing the edge…", std::time::Duration::ZERO, move || {

            render::resolve_mask(
                &mut mask,
                segmentation.as_deref(),
                embedding.as_deref(),
                None,
                width,
                height,
            );
            mask
        })
        .await;
        state.masks.searching.set(false);

        if state.open_generation.get() != generation {
            return;
        }
        let Ok(resolved) = resolved else { return };
        let Some(unshaped) = resolved.unshaped.0.clone() else { return };
        {
            let mut open = state.open.borrow_mut();
            let Some(photo) = open.as_mut() else { return };
            let Some(mask) = photo.document.mask_mut(index) else { return };

            let mut asked = mask.clone();
            asked.feather = worked.feather;
            asked.shift = worked.shift;
            if asked != worked {
                return;
            }
            mask.unshaped = Pixels(Some(unshaped));

            mask.matted = resolved.matted;
            mask.reshape_edge(width, height);
            photo.view = None;
        }
        refresh_outline(&state);
        request_render(&state);
        show_coverage(&state);
        refresh_mask_toolbar(&state);
    });
}

pub(super) fn trace_hair(state: &App, index: usize) {
    if state.masks.tracing.get() {
        state.masks.trace_again.set(Some(index));
        return;
    }
    let (width, height) = mask_raster_size(state);
    let request = {
        let open = state.open.borrow();
        let Some(photo) = open.as_ref() else { return };
        let Some(mask) = photo.document.masks().get(index).cloned() else { return };
        if !mask.matted || mask.unshaped.0.is_none() {
            return;
        }
        let document = &photo.document;
        let key = format!(
            "{} {:?} {:?} {} {}",
            state.open_generation.get(),
            document.crop(),
            document.perspective(),
            document.rotation(),
            document.mirrored()
        );
        let kept = state.masks.fine_frame.borrow().as_ref().filter(|(at, _)| *at == key).map(|(_, frame)| frame.clone());
        (mask, photo.source.clone(), document.clone(), kept, key)
    };
    let (mut mask, source, document, kept, key) = request;

    mask.fine = true;
    let asked = mask.unshaped.0.clone();
    let generation = state.open_generation.get();
    state.masks.tracing.set(true);
    let state = state.clone();
    glib::spawn_future_local(async move {
        let traced = busy_until(&state, "Tracing the hair…", std::time::Duration::ZERO, move || {
            let frame = match kept {
                Some(frame) => frame,
                None => {
                    let full = source.full_resolution().ok()?;
                    Arc::new(render::fine_frame(&document, &full, &render_inputs(&document)))
                }
            };
            render::refine_finely(&mut mask, &frame).then_some((mask.unshaped, frame))
        })
        .await;
        state.masks.tracing.set(false);
        if let Some(again) = state.masks.trace_again.take() {
            trace_hair(&state, again);
        }
        if state.open_generation.get() != generation {
            return;
        }
        let Ok(Some((unshaped, frame))) = traced else { return };
        *state.masks.fine_frame.borrow_mut() = Some((key, frame));
        {
            let mut open = state.open.borrow_mut();
            let Some(photo) = open.as_mut() else { return };
            let Some(mask) = photo.document.mask_mut(index) else { return };

            let same = match (&mask.unshaped.0, &asked) {
                (Some(now), Some(then)) => Arc::ptr_eq(now, then),
                _ => false,
            };
            if !same {
                return;
            }
            mask.unshaped = unshaped;
            mask.fine = true;
            mask.reshape_edge(width, height);
            photo.view = None;
        }
        refresh_outline(&state);
        request_render(&state);
        show_coverage(&state);
        refresh_mask_toolbar(&state);
    });
}
