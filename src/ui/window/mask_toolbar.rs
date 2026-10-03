use super::*;

#[derive(Clone)]
pub(super) struct Toolbar {

    pub(super) bin: adw::BreakpointBin,

    chip: gtk::MenuButton,
    chip_thumb: gtk::DrawingArea,
    chip_masks: gtk::Box,

    tools: Rc<RefCell<Vec<(Tool, gtk::ToggleButton)>>>,

    tools_menu: gtk::MenuButton,
    tool_items: Rc<RefCell<Vec<(Tool, gtk::Button)>>>,

    add: gtk::ToggleButton,
    take: gtk::ToggleButton,

    adding_words: [gtk::Label; 2],
    pub(super) subtract: Rc<Cell<bool>>,

    pub(super) drawing: gtk::Box,
    sliders: gtk::Box,
    size: gtk::Adjustment,
    soft: gtk::Adjustment,

    pub(super) softness: Rc<Cell<f32>>,
    points: gtk::CheckButton,

    refine: adw::SplitButton,
    refine_again: gio::Menu,

    eye: gtk::ToggleButton,
}

#[derive(Clone, Copy, PartialEq)]
pub(super) enum Tool {

    Look,
    Brush,
    Lasso,
    Click,
    Linear,
    Radial,
}

const TOOLS: [(Tool, &str, &str); 6] = [
    (Tool::Look, "Look", "Nothing in hand: zoom and pan the photograph"),
    (Tool::Brush, "Brush", "Paint the mask on"),
    (Tool::Lasso, "Lasso", "Draw round what the mask should cover"),
    (Tool::Click, "Click", "Click something in the photograph to add it"),
    (Tool::Linear, "Linear", "A gradient across the photograph"),
    (Tool::Radial, "Radial", "An ellipse, soft at its edge"),
];

impl Toolbar {
    pub(super) fn new() -> Self {
        Self {
            bin: adw::BreakpointBin::new(),
            chip: gtk::MenuButton::new(),
            chip_thumb: gtk::DrawingArea::new(),
            chip_masks: gtk::Box::new(gtk::Orientation::Vertical, 0),
            tools: Rc::new(RefCell::new(Vec::new())),
            tools_menu: gtk::MenuButton::new(),
            tool_items: Rc::new(RefCell::new(Vec::new())),
            add: gtk::ToggleButton::new(),
            take: gtk::ToggleButton::new(),
            adding_words: [gtk::Label::new(Some("Add")), gtk::Label::new(Some("Subtract"))],
            subtract: Rc::new(Cell::new(false)),
            drawing: gtk::Box::new(gtk::Orientation::Horizontal, 12),
            sliders: gtk::Box::new(gtk::Orientation::Horizontal, 12),
            size: gtk::Adjustment::new(brush_travel(DEFAULT_BRUSH), 0.0, 1.0, 0.005, 0.05, 0.0),
            soft: gtk::Adjustment::new((BRUSH_FEATHER * 100.0) as f64, 0.0, 100.0, 1.0, 10.0, 0.0),
            softness: Rc::new(Cell::new(BRUSH_FEATHER)),
            points: gtk::CheckButton::with_label("Points"),
            refine: adw::SplitButton::new(),
            refine_again: gio::Menu::new(),
            eye: gtk::ToggleButton::new(),
        }
    }
}

pub(super) fn build_mask_toolbar(state: &App) -> adw::BreakpointBin {
    let bar = &state.masks.toolbar;
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);

    row.set_margin_start(12);
    row.set_margin_end(12);

    row.append(&build_chip(state));
    let divider = separator();
    row.append(&divider);

    let hand = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    for part in [build_tools(state).upcast::<gtk::Widget>(), build_tools_menu(state).upcast(), build_drawing(state).upcast()] {
        part.set_valign(gtk::Align::Center);
        hand.append(&part);
    }
    row.append(&hand);

    let filler = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    filler.set_hexpand(true);
    row.append(&filler);

    bar.refine.set_label("Refine");
    bar.refine.set_tooltip_text(Some(
        "Search for the real edge, from where Edge puts it — slower, and worth it on a subject",
    ));
    let looks = gio::Menu::new();
    looks.append(Some("Edge"), Some("win.refine-edge"));
    looks.append(Some("Hair, Fur and Feathers"), Some("win.refine-hair"));
    looks.append_section(None, &bar.refine_again);
    bar.refine.set_menu_model(Some(&looks));
    bar.refine.connect_clicked(glib::clone!(
        #[strong] state,
        move |_| refine_selected_mask(&state)
    ));
    let again = gio::SimpleAction::new("search-again", None);
    again.connect_activate(glib::clone!(
        #[strong] state,
        move |_, _| refine_selected_mask(&state)
    ));
    let actions = gio::SimpleActionGroup::new();
    actions.add_action(&again);
    bar.refine.insert_action_group("mask", Some(&actions));
    row.append(&bar.refine);

    let eye = bar.eye.clone();
    eye.set_tooltip_text(Some("Show the wash — what the mask covers, in red"));
    eye.connect_toggled(glib::clone!(
        #[strong] state,
        move |eye| {
            eye.set_icon_name(if eye.is_active() { "view-reveal-symbolic" } else { "view-conceal-symbolic" });
            if eye.is_active() != state.mask_overlay.show_coverage.get() {
                state.mask_overlay.show_coverage.set(eye.is_active());
                state.mask_overlay.wash_resting.set(false);
                state.mask_overlay.area.queue_draw();
            }
        }
    ));
    eye.set_active(state.mask_overlay.show_coverage.get());
    eye.set_icon_name(if eye.is_active() { "view-reveal-symbolic" } else { "view-conceal-symbolic" });
    let show = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    show.add_css_class("linked");
    show.append(&eye);
    let more = gtk::MenuButton::new();
    more.set_tooltip_text(Some("Outline, Points and Matte"));
    let inside = gtk::Box::new(gtk::Orientation::Vertical, 6);
    inside.set_margin_top(6);
    inside.set_margin_bottom(6);
    inside.set_margin_start(6);
    inside.set_margin_end(6);
    inside.append(&popover_heading("SHOW"));
    inside.append(&build_show(state));
    let shown = gtk::Popover::new();
    shown.set_child(Some(&inside));
    more.set_popover(Some(&shown));
    more.set_direction(gtk::ArrowType::Down);
    show.append(&more);
    row.append(&show);

    let done = primary_button("Done");
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
            menu.set_direction(gtk::ArrowType::Down);
        }
        child = widget.next_sibling();
    }
    bar.refine.set_direction(gtk::ArrowType::Down);
    bar.chip_thumb.set_valign(gtk::Align::Center);
    name_icon_buttons(row.upcast_ref());

    let lines = gtk::Box::new(gtk::Orientation::Vertical, 0);
    lines.append(&row);
    let second = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    second.set_margin_start(12);
    second.set_margin_end(12);
    second.set_visible(false);
    lines.append(&second);

    let bin = bar.bin.clone();
    bin.add_css_class("mask-toolbar");
    bin.set_child(Some(&lines));
    narrowing(&bin, [&row, &second, &hand], &divider, state);
    bin.set_visible(false);
    bin
}

fn narrowing(bin: &adw::BreakpointBin, [row, second, hand]: [&gtk::Box; 3], divider: &gtk::Separator, state: &App) {
    let bar = &state.masks.toolbar;

    bin.set_width_request(540);
    bin.set_height_request(56);
    let (hidden, shown) = (false.to_value(), true.to_value());
    let (closer, edge, short) = (4i32.to_value(), 6i32.to_value(), 72i32.to_value());
    let tight = ["chip-row", "tight"].to_value();
    let Some(tools) = bar.tools.borrow().first().and_then(|(_, button)| button.parent()) else { return };
    let words: Vec<gtk::Widget> = bar.adding_words.iter().map(|word| word.clone().upcast()).collect();
    let scales = scales_in(bar.sliders.upcast_ref());
    let set = |what: &[&gtk::Widget], property: &'static str, value: &glib::Value| {
        what.iter().map(|widget| ((*widget).clone(), property, value.clone())).collect::<Vec<_>>()
    };
    let compact = [set(&[&tools], "css-classes", &tight), set(&words.iter().collect::<Vec<_>>(), "visible", &hidden)].concat();
    let two = set(&[divider.upcast_ref()], "visible", &hidden);
    let menu = [set(&[&tools], "visible", &hidden), set(&[bar.tools_menu.upcast_ref()], "visible", &shown)].concat();
    let close = [
        set(&[row.upcast_ref(), second.upcast_ref()], "margin-start", &edge),
        set(&[row.upcast_ref(), second.upcast_ref()], "margin-end", &edge),
        set(&[row.upcast_ref()], "spacing", &closer),
        set(&scales.iter().collect::<Vec<_>>(), "width-request", &short),
    ]
    .concat();
    let steps: [(f64, Vec<(gtk::Widget, &str, glib::Value)>); 3] = [
        (1160.0, [compact.clone(), close.clone()].concat()),
        (1000.0, [compact.clone(), close.clone(), menu.clone()].concat()),
        (700.0, [compact, close, menu, two].concat()),
    ];
    let mut split = Vec::new();

    let floors = [1000.0, 700.0, 0.0];
    for (index, (width, setters)) in steps.into_iter().enumerate() {
        let below = adw::BreakpointCondition::new_length(adw::BreakpointConditionLengthType::MaxWidth, width, adw::LengthUnit::Px);
        let condition = match floors[index] > 0.0 {
            true => adw::BreakpointCondition::new_and(
                below,
                adw::BreakpointCondition::new_length(adw::BreakpointConditionLengthType::MinWidth, floors[index] + 1.0, adw::LengthUnit::Px),
            ),
            false => below,
        };
        let step = adw::Breakpoint::new(condition);
        for (widget, property, value) in &setters {
            step.add_setter(widget, property, Some(value));
        }
        if index == 2 {
            split.push(step.clone());
        }
        bin.add_breakpoint(step);
    }

    second.set_margin_bottom(10);
    row.set_height_request(54);
    let (hand, row, second, divider) = (hand.clone(), row.clone(), second.clone(), divider.clone());
    bin.connect_current_breakpoint_notify(move |bin| {
        let below = bin.current_breakpoint().is_some_and(|step| split.contains(&step));
        second.set_visible(below);
        let into: &gtk::Box = if below { &second } else { &row };
        if hand.parent().as_ref() == Some(into.upcast_ref()) {
            return;
        }
        if let Some(from) = hand.parent().and_downcast::<gtk::Box>() {
            from.remove(&hand);
        }
        match below {
            true => second.append(&hand),
            false => row.insert_child_after(&hand, Some(&divider)),
        }
    });
}

fn scales_in(root: &gtk::Widget) -> Vec<gtk::Widget> {
    let mut found = Vec::new();
    let mut pending = vec![root.clone()];
    while let Some(widget) = pending.pop() {
        if widget.is::<gtk::Scale>() {
            found.push(widget.clone());
        }
        let mut child = widget.first_child();
        while let Some(current) = child {
            child = current.next_sibling();
            pending.push(current);
        }
    }
    found
}

fn separator() -> gtk::Separator {
    let line = gtk::Separator::new(gtk::Orientation::Vertical);
    line.set_margin_top(14);
    line.set_margin_bottom(14);
    line
}

fn popover_heading(text: &str) -> gtk::Label {
    let heading = gtk::Label::new(Some(text));
    as_popover_heading(&heading);
    heading
}

fn as_popover_heading(heading: &gtk::Label) {
    heading.set_xalign(0.0);
    heading.add_css_class("section-header");
    heading.set_margin_top(4);
    heading.set_margin_bottom(4);
    heading.set_margin_start(10);
    heading.set_margin_end(10);
}

fn popover_item(label: &str, popover: &gtk::Popover) -> gtk::Button {
    let button = gtk::Button::with_label(label);
    button.add_css_class("flat");
    button.add_css_class("popover-item");
    if let Some(label) = button.child().and_downcast::<gtk::Label>() {
        label.set_xalign(0.0);
    }
    button.connect_clicked(glib::clone!(
        #[weak] popover,
        move |_| popover.popdown()
    ));
    button
}

fn build_chip(state: &App) -> gtk::MenuButton {
    let bar = &state.masks.toolbar;
    let chip = bar.chip.clone();
    chip.add_css_class("mask-chip");
    chip.set_tooltip_text(Some("This mask: choose another, or change what it is made of"));

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
    let words = gtk::Box::new(gtk::Orientation::Vertical, 0);
    words.set_valign(gtk::Align::Center);
    let editing = gtk::Label::new(Some("EDITING MASK"));
    editing.add_css_class("section-header");
    editing.set_xalign(0.0);
    words.append(&editing);
    let name = state.editor_page.mask_name_label.clone();
    name.add_css_class("mask-name");

    name.set_ellipsize(gtk::pango::EllipsizeMode::End);
    name.set_width_chars(4);
    name.set_max_width_chars(20);
    name.set_xalign(0.0);
    words.append(&name);
    inside.append(&words);
    inside.append(&gtk::Image::from_icon_name("pan-down-symbolic"));
    chip.set_child(Some(&inside));

    let popover = gtk::Popover::new();
    let column = gtk::Box::new(gtk::Orientation::Vertical, 4);
    column.set_margin_top(6);
    column.set_margin_bottom(6);

    column.set_width_request(320);
    column.append(&popover_heading("MASKS"));
    column.append(&bar.chip_masks);

    let new = popover_item("New Mask", &popover);
    let words = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    words.append(&gtk::Image::from_icon_name("list-add-symbolic"));
    words.append(&gtk::Label::new(Some("New Mask")));
    new.set_child(Some(&words));
    new.connect_clicked(glib::clone!(
        #[strong] state,
        move |_| leave_mask(&state)
    ));
    column.append(&new);

    let header = state.masks.mask_parts_header.clone();
    header.set_text("THIS MASK");
    as_popover_heading(&header);
    column.append(&header);
    state.masks.mask_parts.set_selection_mode(gtk::SelectionMode::None);
    state.masks.mask_parts.add_css_class("boxed-list");
    column.append(&state.masks.mask_parts);
    column.append(&build_shape(state));

    column.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
    for (label, action) in [("Invert", "win.invert-mask"), ("Duplicate", "win.duplicate-mask"), ("Delete Mask", "win.delete-mask")] {
        let item = popover_item(label, &popover);
        item.set_action_name(Some(action));
        column.append(&item);
    }

    let scroller = gtk::ScrolledWindow::new();
    scroller.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    scroller.set_propagate_natural_height(true);
    scroller.set_max_content_height(640);
    scroller.set_child(Some(&column));
    popover.set_child(Some(&scroller));
    chip.set_popover(Some(&popover));
    chip
}

fn draw_chip_thumb(state: &App, context: &gtk::cairo::Context, width: i32, height: i32) {
    context.set_source_rgba(0.08, 0.08, 0.08, 1.0);
    context.rectangle(0.0, 0.0, width as f64, height as f64);
    let _ = context.fill();
    let Some(mask) = selected_mask(state) else { return };
    let Some(alpha) = mask.map.0.as_deref() else { return };
    let (aw, ah) = (alpha.width.max(1), alpha.height.max(1));
    context.set_source_rgba(0.92, 0.92, 0.92, 1.0);
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

fn build_tools(state: &App) -> gtk::Box {
    let bar = &state.masks.toolbar;
    let group = chip_row();
    let mut first: Option<gtk::ToggleButton> = None;
    for (tool, label, tooltip) in TOOLS {
        let button = gtk::ToggleButton::with_label(label);
        button.set_tooltip_text(Some(tooltip));
        match &first {
            None => first = Some(button.clone()),
            Some(first) => button.set_group(Some(first)),
        }
        button.connect_toggled(glib::clone!(
            #[strong] state,
            move |button| {

                if button.is_active() && tool != current_tool(&state) {
                    pick_tool(&state, tool);
                }
            }
        ));
        group.append(&button);
        bar.tools.borrow_mut().push((tool, button));
    }
    group
}

fn build_tools_menu(state: &App) -> gtk::MenuButton {
    let bar = &state.masks.toolbar;
    let menu = bar.tools_menu.clone();
    menu.set_tooltip_text(Some("What is in hand"));
    menu.set_direction(gtk::ArrowType::Down);
    let popover = gtk::Popover::new();
    let column = gtk::Box::new(gtk::Orientation::Vertical, 2);
    for (tool, label, tooltip) in TOOLS {
        let item = gtk::Button::with_label(label);
        item.add_css_class("flat");
        item.set_tooltip_text(Some(tooltip));
        if let Some(text) = item.child().and_downcast::<gtk::Label>() {
            text.set_xalign(0.0);
        }
        item.connect_clicked(glib::clone!(
            #[strong] state,
            #[weak] popover,
            move |_| {
                popover.popdown();
                if tool != current_tool(&state) {
                    pick_tool(&state, tool);
                }
            }
        ));
        column.append(&item);
        bar.tool_items.borrow_mut().push((tool, item));
    }
    popover.set_child(Some(&column));
    menu.set_popover(Some(&popover));
    menu.set_label("Look");
    menu.set_visible(false);
    menu
}

fn current_tool(state: &App) -> Tool {
    if state.masks.looking.get() {
        return Tool::Look;
    }
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

pub(super) fn is_empty_painted(mask: &Mask) -> bool {
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
        Tool::Look => state.masks.brush.set(MaskTool::Off),
    }

    let looking = tool == Tool::Look || (tool == current_tool(state) && state.masks.looking.get());
    state.masks.looking.set(looking);
    state.mask_overlay.area.set_can_target(!looking);
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
    let adding = chip_row();
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

fn build_drawing(state: &App) -> gtk::Box {
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
    let adding = build_adding(state);
    adding.set_valign(gtk::Align::Center);
    drawing.append(&adding);
    for (name, adjustment, readout, rest) in [
        ("Size", &bar.size, Readout::BrushSize, brush_travel(DEFAULT_BRUSH)),
        ("Softness", &bar.soft, Readout::Positive(0), (BRUSH_FEATHER * 100.0) as f64),
    ] {
        bar.sliders.append(&inline_slider(name, adjustment, readout, rest, 110));
    }
    drawing.append(&bar.sliders);
    drawing.set_visible(false);
    drawing
}

fn inline_slider(name: &str, adjustment: &gtk::Adjustment, readout: Readout, rest: f64, width: i32) -> gtk::Box {
    let column = gtk::Box::new(gtk::Orientation::Vertical, 0);
    column.add_css_class("mask-inline");
    let top = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    let label = gtk::Label::new(Some(name));
    label.set_hexpand(true);
    label.set_xalign(0.0);

    let scale = numa_slider(adjustment, rest);
    hud::follow(&scale, name, readout);
    scale.set_width_request(width);
    if name == "Softness" {
        scale.set_tooltip_text(Some("How a stroke's edge falls off — the brush's own, not the mask's Feather"));
    }
    let value = gtk::Label::new(Some(&readout.format(adjustment.value())));
    value.add_css_class("numeric");
    value.set_xalign(1.0);
    adjustment.connect_value_changed(glib::clone!(
        #[weak] value,
        move |adjustment| value.set_text(&readout.format(adjustment.value()))
    ));
    top.append(&label);
    top.append(&value);
    column.append(&top);
    column.append(&scale);
    column
}

fn build_show(state: &App) -> gtk::Box {
    let bar = &state.masks.toolbar;
    let column = gtk::Box::new(gtk::Orientation::Vertical, 2);
    column.set_margin_start(8);
    for (label, on, which) in [
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
                state.mask_overlay.wash_resting.set(false);

                if which == 1 && check.is_active() {
                    start_ants(&state);
                }
                state.mask_overlay.area.queue_draw();
                refresh_mask_toolbar(&state);
            }
        ));
        column.append(&check);
    }
    column
}

fn build_shape(state: &App) -> gtk::Box {
    let column = gtk::Box::new(gtk::Orientation::Vertical, 0);

    column.add_css_class("adjustment");
    column.add_css_class("quiet");
    column.add_css_class("mask-shape");
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
            }
        ));
    }
    column
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
    state.mask_overlay.area.set_can_target(tool != Tool::Look);
    let gradient = is_gradient(mask);
    let empty = is_empty_painted(mask);
    for (which, button) in bar.tools.borrow().iter() {
        let (sensitive, why) = match which {
            Tool::Linear | Tool::Radial if !(gradient || empty) => {
                (false, "A gradient is a mask of its own — add a new mask and pick it here")
            }
            Tool::Brush | Tool::Lasso | Tool::Click if gradient => {
                (false, "A gradient is one shape, moved by its handles")
            }
            _ => (true, ""),
        };
        button.set_sensitive(sensitive);
        if !sensitive {
            button.set_tooltip_text(Some(why));
        } else if let Some((_, _, tip)) = TOOLS.iter().find(|(tool, _, _)| tool == which) {
            button.set_tooltip_text(Some(tip));
        }
        if *which == tool {
            button.set_active(true);
        }
    }

    for (which, item) in bar.tool_items.borrow().iter() {
        if let Some((_, button)) = bar.tools.borrow().iter().find(|(tool, _)| tool == which) {
            item.set_sensitive(button.is_sensitive());
            item.set_tooltip_text(button.tooltip_text().as_deref());
        }
    }
    if let Some((_, label, _)) = TOOLS.iter().find(|(which, _, _)| *which == tool) {
        bar.tools_menu.set_label(label);
    }

    bar.drawing.set_visible(bar.bin.is_visible() && matches!(tool, Tool::Brush | Tool::Lasso | Tool::Click));
    bar.sliders.set_visible(matches!(tool, Tool::Brush | Tool::Lasso));
    bar.points.set_visible(!gradient);
    if bar.eye.is_active() != state.mask_overlay.show_coverage.get() {
        bar.eye.set_active(state.mask_overlay.show_coverage.get());
    }

    let subject = match &mask.shape {
        Shape::Segment { classes } => classes.iter().any(|class| segment::MATTEABLE.contains(class)),
        Shape::Subject | Shape::Painted => true,
        _ => false,
    };
    let can_refine = subject && numa::render::matte::is_installed() && segment::is_installed();
    bar.refine.set_visible(can_refine);
    bar.refine_again.remove_all();
    if can_refine && mask.matte {
        bar.refine_again.append(Some("Search Again"), Some("mask.search-again"));
    }
}
