use super::*;

#[derive(Clone)]
pub(super) struct Toolbar {

    pub(super) bin: adw::BreakpointBin,

    chip: gtk::MenuButton,
    chip_thumb: gtk::DrawingArea,
    chip_masks: gtk::Box,
    tool: gtk::MenuButton,
    tool_icon: gtk::Image,
    tool_label: gtk::Label,
    tools: Rc<RefCell<Vec<(Tool, gtk::Button)>>>,

    adding: gtk::Box,
    add: gtk::ToggleButton,
    take: gtk::ToggleButton,

    adding_words: [gtk::Label; 2],
    pub(super) subtract: Rc<Cell<bool>>,

    pub(super) drawing: gtk::Box,
    sliders: gtk::Box,
    size: gtk::Adjustment,
    soft: gtk::Adjustment,

    pub(super) softness: Rc<Cell<f32>>,
    show_value: gtk::Label,
    points: gtk::CheckButton,
    shape_value: gtk::Label,
    more: gtk::Box,
}

#[derive(Clone, Copy, PartialEq)]
pub(super) enum Tool {
    Brush,
    Lasso,
    Click,
    Linear,
    Radial,
}

const TOOLS: [(Tool, &str, &str); 5] = [
    (Tool::Brush, "Brush", "document-edit-symbolic"),
    (Tool::Lasso, "Lasso", "edit-select-symbolic"),
    (Tool::Click, "Click", "input-mouse-symbolic"),
    (Tool::Linear, "Linear", "view-dual-symbolic"),
    (Tool::Radial, "Radial", "radio-symbolic"),
];

impl Toolbar {
    pub(super) fn new() -> Self {
        Self {
            bin: adw::BreakpointBin::new(),
            chip: gtk::MenuButton::new(),
            chip_thumb: gtk::DrawingArea::new(),
            chip_masks: gtk::Box::new(gtk::Orientation::Vertical, 0),
            tool: gtk::MenuButton::new(),
            tool_icon: gtk::Image::new(),
            tool_label: gtk::Label::new(None),
            tools: Rc::new(RefCell::new(Vec::new())),
            adding: gtk::Box::new(gtk::Orientation::Horizontal, 2),
            add: gtk::ToggleButton::new(),
            take: gtk::ToggleButton::new(),
            adding_words: [gtk::Label::new(Some("Add")), gtk::Label::new(Some("Subtract"))],
            subtract: Rc::new(Cell::new(false)),
            drawing: gtk::Box::new(gtk::Orientation::Horizontal, 16),
            sliders: gtk::Box::new(gtk::Orientation::Horizontal, 16),
            size: gtk::Adjustment::new(brush_travel(DEFAULT_BRUSH), 0.0, 1.0, 0.005, 0.05, 0.0),
            soft: gtk::Adjustment::new((BRUSH_FEATHER * 100.0) as f64, 0.0, 100.0, 1.0, 10.0, 0.0),
            softness: Rc::new(Cell::new(BRUSH_FEATHER)),
            show_value: gtk::Label::new(None),
            points: gtk::CheckButton::with_label("Points"),
            shape_value: gtk::Label::new(None),
            more: gtk::Box::new(gtk::Orientation::Vertical, 0),
        }
    }
}

pub(super) fn build_mask_toolbar(state: &App) -> adw::BreakpointBin {
    let bar = &state.masks.toolbar;
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    row.add_css_class("mask-toolbar");

    let editing = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    editing.add_css_class("mask-editing");
    editing.append(&gtk::Image::from_icon_name("find-location-symbolic"));
    let editing_label = gtk::Label::new(Some("EDITING MASK"));
    editing.append(&editing_label);
    row.append(&editing);

    row.append(&build_chip(state));
    row.append(&separator());
    row.append(&build_tool_button(state));
    row.append(&separator());
    row.append(&build_show_button(state));
    row.append(&build_shape_button(state));
    row.append(&separator());

    let more = gtk::MenuButton::new();
    more.set_icon_name("view-more-symbolic");
    more.set_tooltip_text(Some("This mask"));
    let popover = gtk::Popover::new();
    popover.set_child(Some(&bar.more));
    more.set_popover(Some(&popover));
    row.append(&more);

    let filler = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    filler.set_hexpand(true);
    row.append(&filler);
    let done = gtk::Button::with_label("Done");
    done.add_css_class("suggested-action");
    done.set_tooltip_text(Some("Leave the mask — Esc does the same"));
    done.connect_clicked(glib::clone!(
        #[strong] state,
        move |_| leave_mask(&state)
    ));
    row.append(&done);

    let mut child = row.first_child();
    while let Some(widget) = child {
        widget.set_valign(gtk::Align::Center);
        if let Some(menu) = widget.downcast_ref::<gtk::MenuButton>() {
            menu.set_direction(gtk::ArrowType::Up);
        }
        child = widget.next_sibling();
    }
    bar.chip_thumb.set_valign(gtk::Align::Center);

    let bin = bar.bin.clone();
    bin.set_child(Some(&row));
    narrowing(&bin, &editing_label, state);
    bin.set_visible(false);
    bin
}

fn narrowing(bin: &adw::BreakpointBin, editing: &gtk::Label, state: &App) {
    let bar = &state.masks.toolbar;

    bin.set_width_request(680);
    bin.set_height_request(56);
    let hidden = false.to_value();
    let steps: [(f64, Vec<(gtk::Widget, &glib::Value)>); 2] = [
        (
            975.0,
            vec![
                (editing.clone().upcast(), &hidden),
                (bar.show_value.clone().upcast(), &hidden),
                (bar.shape_value.clone().upcast(), &hidden),
            ],
        ),
        (
            750.0,
            vec![
                (bar.adding_words[0].clone().upcast(), &hidden),
                (bar.adding_words[1].clone().upcast(), &hidden),
                (state.editor_page.mask_where_label.clone().upcast(), &hidden),
                (bar.tool_label.clone().upcast(), &hidden),
            ],
        ),
    ];
    let mut setters: Vec<(gtk::Widget, &glib::Value)> = Vec::new();
    for (width, more) in steps {
        setters.extend(more);
        let step = adw::Breakpoint::new(adw::BreakpointCondition::new_length(
            adw::BreakpointConditionLengthType::MaxWidth,
            width,
            adw::LengthUnit::Px,
        ));
        for (widget, value) in &setters {
            step.add_setter(widget, "visible", Some(value));
        }
        bin.add_breakpoint(step);
    }
}

fn separator() -> gtk::Separator {
    let line = gtk::Separator::new(gtk::Orientation::Vertical);
    line.set_margin_top(12);
    line.set_margin_bottom(12);
    line
}

fn popover_heading(text: &str) -> gtk::Label {
    let heading = gtk::Label::new(Some(text));
    heading.set_xalign(0.0);
    heading.add_css_class("popover-heading");
    heading
}

fn build_chip(state: &App) -> gtk::MenuButton {
    let bar = &state.masks.toolbar;
    let chip = bar.chip.clone();
    chip.add_css_class("mask-chip");
    chip.set_tooltip_text(Some("Choose another mask, or change what this one is made of"));

    let inside = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let thumb = bar.chip_thumb.clone();
    thumb.set_content_width(26);
    thumb.set_content_height(26);
    thumb.add_css_class("mask-thumb");
    thumb.set_draw_func(glib::clone!(
        #[strong] state,
        move |_, context, width, height| draw_chip_thumb(&state, context, width, height)
    ));
    inside.append(&thumb);
    let name = state.editor_page.mask_name_label.clone();
    name.add_css_class("mask-name");
    name.set_ellipsize(gtk::pango::EllipsizeMode::End);

    name.set_width_chars(5);
    name.set_max_width_chars(16);
    inside.append(&name);
    let count = state.editor_page.mask_where_label.clone();
    count.add_css_class("mask-where");
    count.set_ellipsize(gtk::pango::EllipsizeMode::End);
    inside.append(&count);
    inside.append(&gtk::Image::from_icon_name("pan-up-symbolic"));
    chip.set_child(Some(&inside));

    let column = gtk::Box::new(gtk::Orientation::Vertical, 6);
    column.set_margin_top(6);
    column.set_margin_bottom(6);

    column.set_width_request(340);
    column.append(&popover_heading("MASKS"));
    column.append(&bar.chip_masks);
    let header = state.masks.mask_parts_header.clone();
    header.set_text("THIS MASK");
    header.set_xalign(0.0);
    header.add_css_class("popover-heading");
    column.append(&header);
    state.masks.mask_parts.set_selection_mode(gtk::SelectionMode::None);
    state.masks.mask_parts.add_css_class("boxed-list");
    column.append(&state.masks.mask_parts);

    let scroller = gtk::ScrolledWindow::new();
    scroller.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    scroller.set_propagate_natural_height(true);
    scroller.set_max_content_height(520);
    scroller.set_child(Some(&column));
    let popover = gtk::Popover::new();
    popover.set_child(Some(&scroller));
    chip.set_popover(Some(&popover));
    chip
}

fn draw_chip_thumb(state: &App, context: &gtk::cairo::Context, width: i32, height: i32) {
    context.set_source_rgba(0.14, 0.16, 0.19, 1.0);
    context.rectangle(0.0, 0.0, width as f64, height as f64);
    let _ = context.fill();
    let Some(mask) = selected_mask(state) else { return };
    let Some(alpha) = mask.map.0.as_deref() else { return };
    let (aw, ah) = (alpha.width.max(1), alpha.height.max(1));
    context.set_source_rgba(0.384, 0.627, 0.918, 1.0);
    for y in 0..height {
        for x in 0..width {
            let ax = (x as usize * aw / width.max(1) as usize).min(aw - 1);
            let ay = (y as usize * ah / height.max(1) as usize).min(ah - 1);
            if alpha.at(ay * aw + ax) > 0.5 {
                context.rectangle(x as f64, y as f64, 1.0, 1.0);
            }
        }
    }
    let _ = context.fill();
}

fn build_tool_button(state: &App) -> gtk::MenuButton {
    let bar = &state.masks.toolbar;
    let button = bar.tool.clone();
    button.add_css_class("mask-select");

    button.add_css_class("tool-in-hand");
    let inside = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    inside.append(&bar.tool_icon);
    inside.append(&bar.tool_label);
    inside.append(&gtk::Image::from_icon_name("pan-up-symbolic"));
    button.set_child(Some(&inside));
    button.set_tooltip_text(Some("What a drag on the photograph does"));

    let tools = gtk::Box::new(gtk::Orientation::Horizontal, 2);
    let popover = gtk::Popover::new();
    for (tool, label, icon) in TOOLS {
        let item = gtk::Button::new();
        item.add_css_class("flat");
        item.add_css_class("tool-item");
        let content = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        content.append(&gtk::Image::from_icon_name(icon));
        content.append(&gtk::Label::new(Some(label)));
        item.set_child(Some(&content));
        item.connect_clicked(glib::clone!(
            #[strong] state,
            #[weak] popover,
            move |_| {
                popover.popdown();
                pick_tool(&state, tool);
            }
        ));
        tools.append(&item);
        bar.tools.borrow_mut().push((tool, item));
    }
    popover.set_child(Some(&tools));
    button.set_popover(Some(&popover));
    button
}

fn current_tool(state: &App) -> Tool {
    match state.masks.brush.get() {
        MaskTool::Brush => Tool::Brush,
        MaskTool::Lasso => Tool::Lasso,
        MaskTool::Off => match selected_mask(state).map(|mask| mask.shape) {
            Some(Shape::Linear { .. }) => Tool::Linear,
            Some(Shape::Radial { .. }) => Tool::Radial,
            _ => Tool::Click,
        },
    }
}

fn is_empty_painted(mask: &Mask) -> bool {
    matches!(mask.shape, Shape::Painted) && mask.points.is_empty() && mask.strokes.is_empty()
}

pub(super) fn pick_tool(state: &App, tool: Tool) {
    match tool {
        Tool::Brush => state.masks.brush.set(MaskTool::Brush),
        Tool::Lasso => state.masks.brush.set(MaskTool::Lasso),
        Tool::Click => {
            state.masks.brush.set(MaskTool::Off);
            ensure_segmentation(state);
            ensure_embedding(state);
        }
        Tool::Linear | Tool::Radial => {
            state.masks.brush.set(MaskTool::Off);
            if let Some(index) = state.mask_overlay.selected_mask.get() {
                make_gradient(state, index, tool);
            }
        }
    }
    refresh_mask_toolbar(state);
    state.mask_overlay.area.queue_draw();
}

fn make_gradient(state: &App, index: usize, tool: Tool) {
    {
        let mut open = state.open.borrow_mut();
        let Some(photo) = open.as_mut() else { return };
        let Some(mask) = photo.document.mask_mut(index) else { return };
        if !(is_empty_painted(mask) || is_gradient(mask)) {
            return;
        }
        let already = matches!(
            (&mask.shape, tool),
            (Shape::Linear { .. }, Tool::Linear) | (Shape::Radial { .. }, Tool::Radial)
        );
        if already {
            return;
        }
        mask.shape = match tool {
            Tool::Linear => Shape::linear(),
            _ => Shape::radial(),
        };

        mask.map = Pixels(None);
        mask.unshaped = Pixels(None);
        photo.view = None;
    }
    select_mask(state, Some(index));
    request_render(state);
    schedule_history_push(state);
}

fn build_adding(state: &App) -> gtk::Box {
    let bar = &state.masks.toolbar;
    let adding = bar.adding.clone();

    adding.add_css_class("mask-adding");
    for (button, icon, words) in [
        (&bar.add, "list-add-symbolic", &bar.adding_words[0]),
        (&bar.take, "list-remove-symbolic", &bar.adding_words[1]),
    ] {
        let inside = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        inside.append(&gtk::Image::from_icon_name(icon));
        inside.append(words);
        button.set_child(Some(&inside));
    }
    bar.add.set_tooltip_text(Some("Add to the mask — hold Shift to take away"));
    bar.take.set_tooltip_text(Some("Take away from the mask — hold Shift to add"));
    bar.take.set_group(Some(&bar.add));
    bar.add.set_active(true);
    bar.take.connect_toggled(glib::clone!(
        #[strong] state,
        move |button| state.masks.toolbar.subtract.set(button.is_active())
    ));
    adding.append(&bar.add);
    adding.append(&bar.take);
    adding
}

pub(super) fn build_drawing(state: &App) -> gtk::Box {
    let bar = &state.masks.toolbar;

    bar.size.connect_value_changed(glib::clone!(
        #[strong] state,
        move |adjustment| {
            state.masks.brush_radius.set(brush_size(adjustment.value()));
            state.mask_overlay.area.queue_draw();
        }
    ));
    bar.soft.connect_value_changed(glib::clone!(
        #[strong] state,
        move |adjustment| state.masks.toolbar.softness.set(adjustment.value() as f32 / 100.0)
    ));

    let drawing = bar.drawing.clone();
    drawing.add_css_class("osd");
    drawing.add_css_class("mask-drawing");
    drawing.set_halign(gtk::Align::Center);
    drawing.set_valign(gtk::Align::End);
    drawing.set_margin_bottom(10);
    drawing.append(&build_adding(state));
    for (name, adjustment, readout) in [("Size", &bar.size, Readout::BrushSize), ("Softness", &bar.soft, Readout::Positive(0))] {
        bar.sliders.append(&inline_slider(name, adjustment, readout, 140));
    }

    bar.sliders.set_margin_end(8);
    drawing.append(&bar.sliders);
    drawing.set_visible(false);
    drawing
}

fn inline_slider(name: &str, adjustment: &gtk::Adjustment, readout: Readout, width: i32) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    row.add_css_class("mask-inline");
    let label = gtk::Label::new(Some(name));
    label.add_css_class("dim-label");
    let scale = gtk::Scale::new(gtk::Orientation::Horizontal, Some(adjustment));
    scale.set_width_request(width);
    scale.set_draw_value(false);
    scale.set_valign(gtk::Align::Center);
    if name == "Softness" {
        scale.set_tooltip_text(Some("How a stroke's edge falls off — the brush's own, not the mask's Feather"));
    }
    let value = gtk::Label::new(Some(&readout.format(adjustment.value())));
    value.add_css_class("numeric");
    value.set_width_chars(5);
    value.set_xalign(1.0);
    adjustment.connect_value_changed(glib::clone!(
        #[weak] value,
        move |adjustment| value.set_text(&readout.format(adjustment.value()))
    ));
    row.append(&label);
    row.append(&scale);
    row.append(&value);
    row
}

fn build_show_button(state: &App) -> gtk::MenuButton {
    let bar = &state.masks.toolbar;
    let button = gtk::MenuButton::new();
    button.add_css_class("mask-select");
    let inside = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let title = gtk::Label::new(Some("Show"));
    title.add_css_class("heading");
    inside.append(&title);
    bar.show_value.add_css_class("dim-label");

    bar.show_value.set_ellipsize(gtk::pango::EllipsizeMode::End);
    bar.show_value.set_max_width_chars(22);
    inside.append(&bar.show_value);
    inside.append(&gtk::Image::from_icon_name("pan-up-symbolic"));
    button.set_child(Some(&inside));

    let column = gtk::Box::new(gtk::Orientation::Vertical, 2);
    column.set_margin_top(6);
    column.set_margin_bottom(6);
    column.append(&popover_heading("SHOW \u{2014} ANY OF THESE"));
    for (label, on, which) in [
        ("Wash", state.mask_overlay.show_coverage.clone(), 0),
        ("Outline", state.mask_overlay.show_ants.clone(), 1),
        ("Points", state.mask_overlay.show_dots.clone(), 2),
        ("Matte", state.masks.show_matte.clone(), 3),
    ] {
        let check = if which == 2 { bar.points.clone() } else { gtk::CheckButton::with_label(label) };
        check.set_active(on.get());
        check.connect_toggled(glib::clone!(
            #[strong] state,
            #[strong] on,
            move |check| {
                on.set(check.is_active());

                if which == 1 && check.is_active() {
                    start_ants(&state);
                }
                state.mask_overlay.area.queue_draw();
                refresh_mask_toolbar(&state);
            }
        ));
        column.append(&check);
    }
    let popover = gtk::Popover::new();
    popover.set_child(Some(&column));
    button.set_popover(Some(&popover));
    button
}

fn build_shape_button(state: &App) -> gtk::MenuButton {
    let bar = &state.masks.toolbar;
    let button = gtk::MenuButton::new();
    button.add_css_class("mask-select");
    let inside = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let title = gtk::Label::new(Some("Shape"));
    title.add_css_class("heading");
    inside.append(&title);
    bar.shape_value.add_css_class("dim-label");
    inside.append(&bar.shape_value);
    inside.append(&gtk::Image::from_icon_name("pan-up-symbolic"));
    button.set_child(Some(&inside));

    let column = gtk::Box::new(gtk::Orientation::Vertical, 0);
    column.set_width_request(290);
    column.append(&popover_heading("SHAPE"));
    for (name, scale, readout, rest) in [
        ("Strength", &state.masks.strength, Readout::Positive(0), 100.0),
        ("Feather", &state.masks.feather, Readout::Positive(0), 12.0),
        ("Edge", &state.masks.edge, Readout::Signed(0), 0.0),
    ] {
        set_neutral(scale, rest);
        column.append(&slider_row(state, name, scale, readout));
    }
    state.masks.strength.connect_value_changed(glib::clone!(
        #[strong] state,
        move |scale| {
            if let (false, Some(index)) = (state.applying.get(), state.mask_overlay.selected_mask.get()) {
                set_mask_opacity(&state, index, scale.value() as f32 / 100.0);
            }
        }
    ));
    for (which, scale) in [(0u8, &state.masks.feather), (1, &state.masks.edge)] {
        scale.connect_value_changed(glib::clone!(
            #[strong] state,
            move |scale| {
                if let (false, Some(index)) = (state.applying.get(), state.mask_overlay.selected_mask.get()) {
                    set_mask_edge(&state, index, which, scale.value() as f32);
                }
                state.masks.toolbar.shape_value.set_text(&format!("F {:.0}", state.masks.feather.value()));
            }
        ));
    }
    let popover = gtk::Popover::new();
    popover.set_child(Some(&column));
    button.set_popover(Some(&popover));
    button
}

fn fill_mask_menu(state: &App, mask: &Mask, index: usize) {
    let more = &state.masks.toolbar.more;
    while let Some(child) = more.first_child() {
        more.remove(&child);
    }
    more.set_margin_top(6);
    more.set_margin_bottom(6);
    more.append(&popover_heading("THIS MASK"));
    let popover = more.ancestor(gtk::Popover::static_type()).and_downcast::<gtk::Popover>();
    let item = |label: &str| {
        let button = gtk::Button::with_label(label);
        button.add_css_class("flat");
        button.add_css_class("popover-item");
        if let Some(label) = button.child().and_downcast::<gtk::Label>() {
            label.set_xalign(0.0);
        }
        if let Some(popover) = &popover {
            button.connect_clicked(glib::clone!(
                #[weak] popover,
                move |_| popover.popdown()
            ));
        }
        button
    };

    let invert = item("Invert");
    invert.set_action_name(Some("win.invert-mask"));
    more.append(&invert);

    let subject = match &mask.shape {
        Shape::Segment { classes } => classes.iter().any(|class| segment::MATTEABLE.contains(class)),
        Shape::Painted => true,
        _ => false,
    };
    if subject && numa::render::matte::is_installed() && segment::is_installed() {
        let refine = item(if mask.matte { "Search again" } else { "Refine edge" });
        refine.set_tooltip_text(Some(
            "Search for the real edge, from where Edge puts it — slower, and worth it on a subject",
        ));
        refine.connect_clicked(glib::clone!(
            #[strong] state,
            move |button| refine_mask_edge(&state, index, button)
        ));
        more.append(&refine);
    }
    let duplicate = item("Duplicate");
    duplicate.set_action_name(Some("win.duplicate-mask"));
    more.append(&duplicate);
    let line = gtk::Separator::new(gtk::Orientation::Horizontal);
    line.set_margin_top(4);
    line.set_margin_bottom(4);
    more.append(&line);
    let delete = item("Delete");
    delete.set_action_name(Some("win.delete-mask"));
    delete.add_css_class("destructive-item");
    more.append(&delete);
}

pub(super) fn refresh_mask_toolbar(state: &App) {
    let bar = &state.masks.toolbar;
    let masks = state.open.borrow().as_ref().map(|photo| photo.document.masks()).unwrap_or_default();
    let selected = state.mask_overlay.selected_mask.get().filter(|index| *index < masks.len());

    while let Some(child) = bar.chip_masks.first_child() {
        bar.chip_masks.remove(&child);
    }
    for index in 0..masks.len() {
        let button = gtk::Button::with_label(&mask_label(&masks, index));
        button.add_css_class("flat");
        button.add_css_class("popover-item");
        if Some(index) == selected {
            button.add_css_class("current-mask");
        }
        if let Some(label) = button.child().and_downcast::<gtk::Label>() {
            label.set_xalign(0.0);
            label.set_ellipsize(gtk::pango::EllipsizeMode::End);
        }
        button.connect_clicked(glib::clone!(
            #[strong] state,
            move |_| {
                if let Some(popover) = state.masks.toolbar.chip.popover() {
                    popover.popdown();
                }
                select_mask(&state, Some(index));
            }
        ));
        bar.chip_masks.append(&button);
    }
    bar.chip_thumb.queue_draw();

    let Some(index) = selected else { return };
    let mask = &masks[index];

    let tool = current_tool(state);
    if let Some((_, label, icon)) = TOOLS.iter().find(|(which, _, _)| *which == tool) {
        bar.tool_label.set_text(label);
        bar.tool_icon.set_icon_name(Some(icon));
    }
    let gradient = is_gradient(mask);
    let empty = is_empty_painted(mask);
    for (which, item) in bar.tools.borrow().iter() {
        let (sensitive, why) = match which {
            Tool::Linear | Tool::Radial if !(gradient || empty) => {
                (false, "A gradient is a mask of its own — add a new mask and pick it here")
            }
            Tool::Brush | Tool::Lasso | Tool::Click if gradient => {
                (false, "A gradient is one shape, moved by its handles")
            }
            _ => (true, ""),
        };
        item.set_sensitive(sensitive);
        item.set_tooltip_text((!sensitive).then_some(why));
        match *which == tool {
            true => item.add_css_class("current-tool"),
            false => item.remove_css_class("current-tool"),
        }
    }

    bar.drawing.set_visible(bar.bin.is_visible() && matches!(tool, Tool::Brush | Tool::Lasso | Tool::Click));
    bar.sliders.set_visible(matches!(tool, Tool::Brush | Tool::Lasso));
    bar.points.set_visible(!gradient);

    let on: Vec<&str> = [
        ("Wash", state.mask_overlay.show_coverage.get()),
        ("Outline", state.mask_overlay.show_ants.get()),
        ("Points", state.mask_overlay.show_dots.get() && !gradient),
        ("Matte", state.masks.show_matte.get()),
    ]
    .into_iter()
    .filter_map(|(name, on)| on.then_some(name))
    .collect();
    bar.show_value.set_text(&match on.is_empty() {
        true => "Nothing".to_string(),
        false => on.join(", "),
    });
    bar.shape_value.set_text(&format!("F {:.0}", mask.feather));

    fill_mask_menu(state, mask, index);
}

pub(super) fn parts_of(mask: &Mask) -> usize {
    let classes = match &mask.shape {
        Shape::Segment { classes } => classes.len(),
        Shape::Painted => 0,
        _ => 1,
    };
    classes + mask.points.len() + mask.strokes.len()
}
