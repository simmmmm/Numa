use super::*;

pub(super) fn build_editor_page(state: &App) -> gtk::Box {
    let page = state.editor_page.page.clone();

    page.append(&build_editor_bar(state));
    page.append(&key_hint(
        state,
        "hint-editor-keys",
        "Hold Space to compare with the original · double-click a slider to reset it · I shows the camera's details",
    ));

    let body = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    body.set_hexpand(true);
    body.set_vexpand(true);

    let scroller = state.zooming.scroller.clone();
    scroller.set_hexpand(true);
    scroller.set_vexpand(true);
    scroller.add_css_class("canvas-area");

    install_canvas_clicks(state);

    let overlay = build_canvas_overlay(state);
    scroller.set_child(Some(&overlay));

    install_canvas_navigation(state, &scroller);

    append_panel_split(state, &body, &scroller);

    page.append(&body);

    let strip = build_editor_filmstrip(state);

    page.append(&state.editor_page.strip_line);
    page.append(&strip);
    page
}

fn build_canvas_overlay(state: &App) -> gtk::Overlay {

    let overlay = gtk::Overlay::new();
    overlay.set_child(Some(&state.canvas));
    overlay.add_overlay(&build_guides_overlay(state));
    overlay.add_overlay(&build_face_names_overlay(state));
    overlay.add_overlay(&build_crop_overlay(state));
    overlay.add_overlay(&build_mask_overlay(state));
    overlay.add_overlay(&build_retouch_overlay(state));
    overlay.add_controller(overlay_dot_remove(state));

    overlay
}

fn install_canvas_navigation(state: &App, scroller: &gtk::ScrolledWindow) {

    for adjustment in [scroller.hadjustment(), scroller.vadjustment()] {
        adjustment.connect_value_changed(glib::clone!(
            #[strong] state,
            move |_| {
                let Some(tile) = state.render.tile.get() else { return };
                let Some(visible) = visible_rect(&state) else { return };
                let inside = visible[0] >= tile[0]
                    && visible[1] >= tile[1]
                    && visible[0] + visible[2] <= tile[0] + tile[2]
                    && visible[1] + visible[3] <= tile[1] + tile[3];
                if !inside {
                    request_render(&state);
                }
            }
        ));
    }

    let scroll = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::VERTICAL);
    scroll.connect_scroll(glib::clone!(
        #[strong] state,
        move |_, _, dy| {

            let factor = ZOOM_PER_NOTCH.powf(-dy);
            zoom_about_centre(&state, scaled_zoom(&state, factor));
            glib::Propagation::Stop
        }
    ));
    scroller.add_controller(scroll);

    let drag = gtk::GestureDrag::new();
    let anchor = Rc::new(Cell::new((0.0f64, 0.0f64)));
    drag.connect_drag_begin(glib::clone!(
        #[strong] state,
        #[strong] anchor,
        move |_, _, _| {
            anchor.set((
                state.zooming.scroller.hadjustment().value(),
                state.zooming.scroller.vadjustment().value(),
            ));
        }
    ));
    drag.connect_drag_update(glib::clone!(
        #[strong] state,
        #[strong] anchor,
        move |_, dx, dy| {
            let (x, y) = anchor.get();

            state.zooming.scroller.hadjustment().set_value(x - dx);
            state.zooming.scroller.vadjustment().set_value(y - dy);
        }
    ));
    scroller.add_controller(drag);

    for adjustment in [scroller.hadjustment(), scroller.vadjustment()] {
        adjustment.connect_page_size_notify(glib::clone!(
            #[strong] state,
            move |_| {
                if state.zooming.level.get() == FIT_ZOOM {
                    apply_zoom(&state);
                }
            }
        ));
    }
}

fn append_panel_split(state: &App, body: &gtk::Box, scroller: &gtk::ScrolledWindow) {

    let beside = gtk::Box::new(gtk::Orientation::Horizontal, 0);

    beside.set_homogeneous(true);
    beside.append(&build_reference_pane(state));
    beside.append(scroller);

    let canvas_column = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let above = state.editor_page.viewport.clone();
    above.add_css_class("canvas-viewport");
    above.set_child(Some(&beside));
    above.add_overlay(&build_drawing(state));
    above.set_vexpand(true);
    canvas_column.append(&above);
    canvas_column.append(&build_mask_toolbar(state));

    let panel = build_adjustment_panel(state);
    let split = gtk::Paned::new(gtk::Orientation::Horizontal);
    split.set_start_child(Some(&canvas_column));
    split.set_end_child(Some(&panel));
    split.set_resize_start_child(true);
    split.set_resize_end_child(false);

    split.set_shrink_end_child(true);
    split.set_position(-1);

    body.append(&split);
    split.set_hexpand(true);

    let panel_split = split.clone();
    split.add_tick_callback(move |split, _| {
        let width = split.width();
        if width > PANEL_WIDTH * 2 {
            panel_split.set_position(width - panel_floor(&panel));
            return glib::ControlFlow::Break;
        }
        glib::ControlFlow::Continue
    });
}

fn build_editor_filmstrip(state: &App) -> gtk::ScrolledWindow {

    let strip = state.filmstrip.scroller.clone();
    strip.set_policy(gtk::PolicyType::Automatic, gtk::PolicyType::Never);
    strip.set_child(Some(&state.filmstrip.strip));
    strip.add_css_class("filmstrip");
    state.filmstrip.strip.set_margin_top(6);
    state.filmstrip.strip.set_margin_bottom(6);
    state.filmstrip.strip.set_margin_start(8);
    state.filmstrip.strip.set_margin_end(8);

    let wheel = gtk::EventControllerScroll::new(
        gtk::EventControllerScrollFlags::BOTH_AXES | gtk::EventControllerScrollFlags::DISCRETE,
    );
    wheel.connect_scroll(glib::clone!(
        #[strong] state,
        move |_, dx, dy| {
            let adjustment = state.filmstrip.scroller.hadjustment();

            let step = if dx.abs() > dy.abs() { dx } else { dy };
            let reach = adjustment.upper() - adjustment.page_size();
            adjustment.set_value((adjustment.value() + step * FILMSTRIP_STEP).clamp(0.0, reach.max(0.0)));
            glib::Propagation::Stop
        }
    ));
    strip.add_controller(wheel);

    let adjustment = strip.hadjustment();
    adjustment.connect_value_changed(glib::clone!(
        #[strong] state,
        move |_| schedule_thumbnails(&state)
    ));
    adjustment.connect_changed(glib::clone!(
        #[strong] state,
        move |_| schedule_thumbnails(&state)
    ));
    strip
}

pub(super) fn build_crumbs(state: &App) -> gtk::Box {
    let crumbs = gtk::Box::new(gtk::Orientation::Horizontal, 2);
    crumbs.add_css_class("breadcrumbs");
    crumbs.set_halign(gtk::Align::Center);

    for (crumb, tooltip) in [
        (&state.editor_page.library_crumb, "Back to the library"),
        (&state.editor_page.photo_crumb, "The whole photograph"),
    ] {
        crumb.add_css_class("flat");
        crumb.set_tooltip_text(Some(tooltip));

        crumb.set_can_shrink(true);
    }

    state.editor_page.library_crumb.connect_clicked(glib::clone!(
        #[strong] state,
        move |_| close_editor(&state)
    ));

    state.editor_page.photo_crumb.connect_clicked(glib::clone!(
        #[strong] state,
        move |_| select_mask(&state, None)
    ));

    crumbs.append(&libraries_crumb(state));
    crumbs.append(&crumb_arrow());
    crumbs.append(&state.editor_page.library_crumb);
    crumbs.append(&crumb_arrow());
    crumbs.append(&state.editor_page.photo_crumb);

    let mask = &state.editor_page.mask_crumb;
    mask.append(&crumb_arrow());
    state.editor_page.mask_crumb_label.add_css_class("mask-crumb");
    state.editor_page.mask_crumb_label.set_ellipsize(gtk::pango::EllipsizeMode::End);
    state.editor_page.mask_crumb_label.set_max_width_chars(24);
    mask.append(&state.editor_page.mask_crumb_label);
    mask.set_visible(false);
    crumbs.append(mask);
    crumbs
}

pub(super) fn show_coverage(state: &App) {
    state.mask_overlay.wash_resting.set(false);
    state.mask_overlay.area.queue_draw();
}

pub(super) fn build_mask_banner(state: &App) -> gtk::Box {
    let banner = state.editor_page.banner.clone();
    banner.add_css_class("mask-scope");
    banner.set_margin_start(14);
    banner.set_margin_end(14);
    banner.set_margin_top(8);
    banner.append(&gtk::Image::from_icon_name("find-location-symbolic"));
    let words = state.editor_page.banner_label.clone();
    words.set_use_markup(true);
    words.set_xalign(0.0);
    words.set_hexpand(true);
    words.set_ellipsize(gtk::pango::EllipsizeMode::End);
    banner.append(&words);

    let eye = state.editor_page.banner_eye.clone();
    eye.add_css_class("flat");
    eye.connect_toggled(glib::clone!(
        #[strong] state,
        move |button| {
            if state.applying.get() {
                return;
            }
            if let Some(index) = state.mask_overlay.selected_mask.get() {
                set_mask_visible(&state, index, button.is_active());
            }
        }
    ));
    banner.append(&eye);
    banner.set_visible(false);
    banner
}

pub(super) fn crumb_arrow() -> gtk::Image {
    let arrow = gtk::Image::from_icon_name("pan-end-symbolic");
    arrow.add_css_class("crumb-arrow");
    arrow.set_margin_start(2);
    arrow.set_margin_end(2);
    arrow
}

pub(super) fn refresh_crumbs(state: &App) {

    let holding = match state.open.borrow().as_ref().map(|photo| &photo.source) {
        Some(Source::Photo { id, .. }) => {
            let library_id = numa::io::catalog::library_of(*id);
            state.libraries.all.borrow().iter().find(|library| library.id == library_id).map(Library::label)
        }

        _ => None,
    };
    state.editor_page.library_crumb.set_label(
        &holding
            .or_else(|| state.libraries.current.borrow().as_ref().map(Library::label))
            .unwrap_or_else(|| "Library".to_string()),
    );

    let name = match state.open.borrow().as_ref().map(|photo| &photo.source) {
        Some(Source::Photo { path, .. }) => {
            path.file_name().map(|name| name.to_string_lossy().into_owned())
        }
        Some(Source::Bracket { paths }) => Some(format!("Merge of {} frames", paths.len())),
        None => None,
    };
    state.editor_page.photo_crumb.set_label(name.as_deref().unwrap_or("\u{2014}"));
}

pub(super) fn build_editor_bar(state: &App) -> gtk::Box {
    let bar = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    bar.add_css_class("toolbar-row");

    let copy = gtk::Button::from_icon_name("edit-copy-symbolic");
    copy.set_tooltip_text(Some("Copy this photo's settings (Ctrl+C)"));
    copy.connect_clicked(glib::clone!(
        #[strong] state,
        move |_| copy_settings(&state)
    ));
    bar.append(&copy);

    let popover = build_info_popover(state);

    let guides = state.overlays.guides_button.clone();
    guides.set_icon_name("view-grid-symbolic");
    guides.set_tooltip_text(Some("Guides: none (G)"));
    guides.connect_clicked(glib::clone!(
        #[strong] state,
        move |_| cycle_guides(&state)
    ));
    bar.append(&guides);

    let info = state.info.button.clone();
    info.set_icon_name("dialog-information-symbolic");
    info.set_tooltip_text(Some("What the camera recorded (I)"));
    info.set_popover(Some(&popover));
    bar.append(&info);

    let ratings = build_rating_menu();
    state.editor_page.rating_button.set_menu_model(Some(&ratings));
    state.editor_page.rating_button.set_tooltip_text(Some("Rating — or press 0 to 5, P, X"));
    state.editor_page.rating_button.add_css_class("rating-button");
    bar.append(&state.editor_page.rating_button);

    let zoom_group = build_zoom_group(state);
    bar.append(&zoom_group);

    let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    spacer.set_hexpand(true);
    bar.append(&spacer);

    let history_group = build_history_group(state);
    bar.append(&history_group);

    state.editor_page.before.set_label("Before");
    state.editor_page.before.set_tooltip_text(Some("Show the frame as shot (hold Space)"));
    state.editor_page.before.set_margin_end(8);
    state.editor_page.before.connect_toggled(glib::clone!(
        #[strong] state,
        move |button| {
            if button.is_active() {
                show_baseline(&state);
            } else {
                render_current(&state);
            }
        }
    ));
    bar.append(&state.editor_page.before);

    bar.append(&build_reference_picker(state));

    let (group, export) = export_buttons(state, |state| export_now(state));
    state.export.button.replace(Some(export));
    refresh_export_button(state);
    bar.append(&group);

    bar
}

fn build_info_popover(state: &App) -> gtk::Popover {

    let facts = page_column();
    facts.append(&section_header("This photograph"));
    facts.append(&build_info(state));
    let scroller = gtk::ScrolledWindow::new();
    scroller.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    scroller.set_propagate_natural_height(true);
    scroller.set_max_content_height(600);

    facts.set_size_request(380, -1);
    scroller.set_child(Some(&facts));
    let popover = gtk::Popover::new();
    popover.set_child(Some(&scroller));
    popover
}

fn build_rating_menu() -> gio::Menu {
    let ratings = gio::Menu::new();
    let stars = gio::Menu::new();
    for value in (0..=5i32).rev() {
        let label = match value {
            0 => "No rating".to_string(),
            n => format!("{} {}", "\u{2605}".repeat(n as usize), n),
        };
        let item = gio::MenuItem::new(Some(&label), None);
        item.set_action_and_target_value(Some("win.photo-rate"), Some(&value.to_variant()));
        stars.append_item(&item);
    }
    ratings.append_section(None, &stars);

    let flags = gio::Menu::new();
    for (label, which) in [("Pick", "pick"), ("Reject", "reject"), ("Clear flag", "none")] {
        let item = gio::MenuItem::new(Some(label), None);
        item.set_action_and_target_value(Some("win.photo-flag"), Some(&which.to_variant()));
        flags.append_item(&item);
    }
    ratings.append_section(None, &flags);
    ratings
}

fn build_zoom_group(state: &App) -> gtk::Box {

    let zoom_group = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    zoom_group.add_css_class("linked");
    zoom_group.set_margin_start(8);

    let out = gtk::Button::from_icon_name("zoom-out-symbolic");
    out.set_tooltip_text(Some("Zoom out"));
    out.connect_clicked(glib::clone!(
        #[strong] state,
        move |_| zoom_about_centre(&state, scaled_zoom(&state, 1.0 / ZOOM_PER_NOTCH))
    ));
    zoom_group.append(&out);

    let levels = gio::Menu::new();
    let fit = gio::Menu::new();
    fit.append(Some("Fit to window"), Some("win.zoom::fit"));
    levels.append_section(None, &fit);
    let steps = gio::Menu::new();
    for percent in [50u32, 100, 200, 400] {
        steps.append(Some(&format!("{percent} %")), Some(&format!("win.zoom::{percent}")));
    }
    levels.append_section(None, &steps);

    let readout = gtk::MenuButton::new();
    readout.set_child(Some(&state.zooming.label));
    readout.set_menu_model(Some(&levels));
    readout.set_tooltip_text(Some("Zoom — and where to go"));
    readout.add_css_class("zoom-readout");
    state.zooming.label.set_width_chars(8);
    state.zooming.label.set_xalign(0.5);
    zoom_group.append(&readout);

    let into = gtk::Button::from_icon_name("zoom-in-symbolic");
    into.set_tooltip_text(Some("Zoom in"));
    into.connect_clicked(glib::clone!(
        #[strong] state,
        move |_| zoom_about_centre(&state, scaled_zoom(&state, ZOOM_PER_NOTCH))
    ));
    zoom_group.append(&into);
    zoom_group
}

fn build_history_group(state: &App) -> gtk::Box {

    let history_group = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    history_group.add_css_class("linked");
    history_group.set_margin_end(8);
    for (icon, tooltip, redo) in [
        ("edit-undo-symbolic", "Undo (Ctrl+Z)", false),
        ("edit-redo-symbolic", "Redo (Ctrl+Shift+Z)", true),
    ] {
        let button = gtk::Button::from_icon_name(icon);
        button.set_tooltip_text(Some(tooltip));
        button.connect_clicked(glib::clone!(
            #[strong] state,
            move |_| step_history(&state, redo)
        ));
        history_group.append(&button);
    }

    let steps = gtk::MenuButton::new();
    steps.set_icon_name("document-open-recent-symbolic");
    steps.set_tooltip_text(Some("Every step you have taken"));
    steps.set_popover(Some(&build_history(state)));
    history_group.append(&steps);
    history_group
}

#[derive(Clone)]
pub(super) struct State {

    pub(super) banner: gtk::Box,

    pub(super) banner_label: gtk::Label,

    pub(super) mask_name_label: gtk::Label,
    pub(super) mask_where_label: gtk::Label,

    pub(super) mask_crumb: gtk::Box,
    pub(super) mask_crumb_label: gtk::Label,

    pub(super) page: gtk::Box,
    pub(super) strip_line: gtk::Separator,

    pub(super) viewport: gtk::Overlay,

    pub(super) banner_eye: gtk::ToggleButton,
    pub(super) library_crumb: gtk::Button,
    pub(super) photo_crumb: gtk::Button,

    pub(super) rating_button: gtk::MenuButton,
    pub(super) before: gtk::ToggleButton,
}

impl State {
    pub(super) fn new() -> Self {
        Self {
            banner: gtk::Box::new(gtk::Orientation::Horizontal, 6),
            banner_label: gtk::Label::new(None),
            mask_name_label: gtk::Label::new(None),
            mask_where_label: gtk::Label::new(None),
            mask_crumb: gtk::Box::new(gtk::Orientation::Horizontal, 2),
            mask_crumb_label: gtk::Label::new(None),
            page: gtk::Box::new(gtk::Orientation::Vertical, 0),
            viewport: gtk::Overlay::new(),
            strip_line: gtk::Separator::new(gtk::Orientation::Horizontal),
            banner_eye: gtk::ToggleButton::new(),
            library_crumb: gtk::Button::new(),
            photo_crumb: gtk::Button::new(),
            rating_button: gtk::MenuButton::new(),
            before: gtk::ToggleButton::new(),
        }
    }
}
