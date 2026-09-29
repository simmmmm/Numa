use super::*;

pub(super) fn build_histogram(state: &App) -> gtk::Box {
    let column = gtk::Box::new(gtk::Orientation::Vertical, 4);
    column.set_margin_bottom(4);

    let area = state.info.histogram_area.clone();
    area.set_content_height(84);
    area.set_hexpand(true);
    area.add_css_class("histogram");

    let histogram = state.info.histogram.clone();
    let (scope, kind) = (state.info.scope.clone(), state.info.scope_kind.clone());
    area.set_draw_func(move |_, context, width, height| {
        match (kind.get(), scope.borrow().as_ref()) {
            (render::scope::Kind::Histogram, _) | (_, None) => {
                draw_histogram(context, width as f64, height as f64, histogram.borrow().as_ref())
            }
            (_, Some(scope)) => draw_scope(context, width as f64, height as f64, scope),
        }
    });

    let stacked = gtk::Overlay::new();
    stacked.set_child(Some(&area));
    for (button, tooltip, class, side) in [
        (&state.info.shadow_clip, "Show clipped shadows", "clip-shadow", gtk::Align::Start),
        (&state.info.highlight_clip, "Show clipped highlights", "clip-highlight", gtk::Align::End),
    ] {
        button.set_tooltip_text(Some(tooltip));
        button.add_css_class("clip-light");
        button.add_css_class(class);
        button.set_has_frame(false);
        button.set_halign(side);
        button.set_valign(gtk::Align::End);
        button.set_margin_start(4);
        button.set_margin_end(4);
        button.set_margin_bottom(4);
        button.connect_toggled(glib::clone!(
            #[strong] state,
            move |_| request_render(&state)
        ));
        stacked.add_overlay(button);
    }

    install_scope_menu(state, &area);
    column.append(&stacked);
    column
}

const SCOPE_SETTING: &str = "scope";

fn install_scope_menu(state: &App, area: &gtk::DrawingArea) {
    let saved = state.catalog.setting(SCOPE_SETTING).map(|name| render::scope::Kind::from_name(&name)).unwrap_or_default();
    state.info.scope_kind.set(saved);

    let action = gio::SimpleAction::new_stateful("kind", Some(glib::VariantTy::STRING), &saved.name().to_variant());
    action.connect_activate(glib::clone!(
        #[strong] state,
        move |action, name| {
            let Some(name) = name.and_then(|name| name.str()) else { return };
            let kind = render::scope::Kind::from_name(name);
            action.set_state(&kind.name().to_variant());
            state.info.scope_kind.set(kind);
            let _ = state.catalog.set_setting(SCOPE_SETTING, kind.name());
            state.info.histogram_area.queue_draw();
            request_render(&state);
        }
    ));
    let group = gio::SimpleActionGroup::new();
    group.add_action(&action);
    area.insert_action_group("scope", Some(&group));

    let menu = gio::Menu::new();
    for kind in render::scope::Kind::ALL {
        menu.append(Some(kind.name()), Some(&format!("scope.kind::{}", kind.name())));
    }
    let popover = gtk::PopoverMenu::from_model(Some(&menu));
    popover.set_parent(area);
    popover.set_has_arrow(false);
    popover.set_halign(gtk::Align::Start);
    let click = gtk::GestureClick::new();
    click.set_button(gtk::gdk::BUTTON_SECONDARY);
    click.connect_pressed(glib::clone!(
        #[weak] popover,
        move |_, _, x, y| {
            popover.set_pointing_to(Some(&gtk::gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
            popover.popup();
        }
    ));
    area.add_controller(click);
}

fn draw_scope(context: &gtk::cairo::Context, width: f64, height: f64, scope: &render::scope::Scope) {

    let stride = scope.width as i32 * 4;
    let mut data = Vec::with_capacity(scope.rgba.len());
    for pixel in scope.rgba.chunks_exact(4) {
        data.extend_from_slice(&u32::from_be_bytes([0, pixel[0], pixel[1], pixel[2]]).to_ne_bytes());
    }
    let Ok(surface) = gtk::cairo::ImageSurface::create_for_data(
        data,
        gtk::cairo::Format::Rgb24,
        scope.width as i32,
        scope.height as i32,
        stride,
    ) else {
        return;
    };
    let _ = context.save();
    context.scale(width / scope.width as f64, height / scope.height as f64);
    let _ = context.set_source_surface(&surface, 0.0, 0.0);
    context.source().set_filter(gtk::cairo::Filter::Good);
    let _ = context.paint();
    let _ = context.restore();
}

pub(super) fn draw_histogram(
    context: &gtk::cairo::Context,
    width: f64,
    height: f64,
    histogram: Option<&render::histogram::Histogram>,
) {
    let Some(histogram) = histogram else { return };

    let scale = histogram.scale() as f64;
    let step = width / render::histogram::BINS as f64;

    context.set_operator(gtk::cairo::Operator::Add);

    for (index, colour) in [(0, (0.85, 0.2, 0.2)), (1, (0.2, 0.8, 0.3)), (2, (0.25, 0.45, 0.95))] {
        context.set_source_rgba(colour.0, colour.1, colour.2, 0.62);
        context.move_to(0.0, height);

        for (bin, count) in histogram.channels[index].iter().enumerate() {

            let value = (*count as f64 / scale).min(1.0);
            context.line_to(bin as f64 * step, height - value * height);
        }

        context.line_to(width, height);
        context.close_path();
        let _ = context.fill();
    }
}

pub(super) fn build_info(state: &App) -> gtk::Box {
    let column = state.info.page.clone();
    column.set_orientation(gtk::Orientation::Vertical);
    column.set_spacing(18);
    column.set_margin_top(6);

    let label = &state.info.render_info;
    label.set_xalign(0.0);
    label.set_wrap(true);
    label.set_selectable(true);
    label.add_css_class("profile-note");

    state.info.rendering.set_label_widget(Some(&section_header("Rendering")));
    state.info.rendering.set_child(Some(label));
    column
}

pub(super) fn fact_row(title: &str, value: &str) -> adw::ActionRow {
    let row = adw::ActionRow::new();
    row.set_title(title);
    row.set_title_lines(1);

    let answer = gtk::Label::new(Some(value));
    answer.set_xalign(1.0);
    answer.add_css_class("dim-label");
    answer.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
    answer.set_max_width_chars(22);
    answer.set_tooltip_text(Some(value));
    row.add_suffix(&answer);
    row
}

pub(super) fn people_group(state: &App, photo: &OpenPhoto) -> Option<adw::PreferencesGroup> {
    let Source::Photo { id, .. } = photo.source else { return None };
    if photo.people.is_empty() {
        return None;
    }
    let named = state.catalog.named_faces().unwrap_or_else(|err| {
        log::warn!("could not read the named faces: {err}");
        Vec::new()
    });
    let elsewhere: Vec<(String, [f32; cull::people::LENGTH])> =
        named.iter().map(|(_, name, embedding)| (name.clone(), *embedding)).collect();

    let group = adw::PreferencesGroup::new();
    group.set_title("People");
    for seen in &photo.people {

        let here = named
            .iter()
            .filter(|(photo_id, _, _)| *photo_id == id)
            .map(|(_, name, embedding)| (name, cull::people::likeness(embedding, &seen.embedding)))
            .filter(|(_, alike)| *alike >= 0.8)
            .max_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(name, _)| name.clone());
        let guess = cull::people::recognise(&seen.embedding, &elsewhere);

        let row = adw::EntryRow::new();
        match (&here, guess) {
            (Some(name), _) => {
                row.set_title("Named");
                row.set_text(name);
            }
            (None, Some((name, _))) => {
                row.set_title("Looks like — Enter to confirm");
                row.set_text(name);
            }
            (None, None) => row.set_title("Who is this?"),
        }
        row.set_show_apply_button(true);

        let picture = gtk::Image::from_paintable(Some(&texture_from(&seen.portrait)));
        picture.set_pixel_size(40);
        picture.set_valign(gtk::Align::Center);
        picture.add_css_class("face-portrait");
        row.add_prefix(&picture);

        let embedding = seen.embedding;
        let name = move |state: &App, row: &adw::EntryRow| {
            let text = row.text().to_string();
            if let Err(err) = state.catalog.name_face(id, &embedding, &text) {
                log::warn!("could not name a face: {err}");
                state.toast("The name could not be saved");
                return;
            }

            let state = state.clone();
            glib::idle_add_local_once(move || {
                refresh_info(&state);
                refresh_face_names(&state);
            });
        };
        row.connect_apply(glib::clone!(
            #[strong] state,
            move |row| name(&state, row)
        ));

        row.connect_entry_activated(glib::clone!(
            #[strong] state,
            move |row| name(&state, row)
        ));
        group.add(&row);
    }

    let on_canvas = adw::SwitchRow::new();
    on_canvas.set_title("Show the names on the photograph");
    on_canvas.set_active(state.overlays.show_face_names.get());
    on_canvas.connect_active_notify(glib::clone!(
        #[strong] state,
        move |row| show_face_names(&state, row.is_active())
    ));
    group.add(&on_canvas);

    Some(group)
}

pub(super) fn refresh_info(state: &App) {
    while let Some(child) = state.info.page.first_child() {
        state.info.page.remove(&child);
    }

    {
        let open = state.open.borrow();
        let Some(photo) = open.as_ref() else { return };
        append_photo_groups(state, photo);
    }
    state.info.page.append(&state.info.rendering);
    refresh_render_info(state);
}

fn append_photo_groups(state: &App, photo: &OpenPhoto) {

    if let Some(people) = people_group(state, photo) {
        state.info.page.append(&people);
    }

    let mut body: Vec<(&str, String)> = Vec::new();
    let mut shot: Vec<(&str, String)> = Vec::new();
    if let Some(summary) = &photo.summary {
        if let Some(camera) = &summary.camera {
            body.push(("Body", camera.clone()));
        }
        if let Some(lens) = &summary.lens {
            body.push(("Lens", lens.clone()));
        }
        if let Some(mode) = &summary.film_mode {

            body.push(("Film mode", mode.clone()));
        }

        if let Some(focal) = summary.focal_length {
            shot.push(("Focal length", format!("{focal:.0} mm")));
        }
        if let Some(aperture) = summary.aperture {
            shot.push(("Aperture", format!("f/{aperture:.1}")));
        }
        if let Some(shutter) = summary.shutter_text() {
            shot.push(("Shutter", shutter));
        }
        if let Some(iso) = summary.iso {
            shot.push(("ISO", iso.to_string()));
        }
        match summary.exposure_bias {
            Some(bias) if bias != 0.0 => shot.push(("Compensation", format!("{bias:+.1} EV"))),
            _ => {}
        }
        if let Some(taken) = &summary.taken {
            shot.push(("Taken", taken.clone()));
        }
    }

    if body.is_empty() && shot.is_empty() {
        let empty = adw::StatusPage::new();
        empty.set_title("No Camera Data");
        empty.set_description(Some("This file does not say which camera took it, or how."));
        empty.add_css_class("compact");
        state.info.page.append(&empty);
    }
    for (title, facts) in [("Camera", body), ("Exposure", shot)] {
        if facts.is_empty() {
            continue;
        }
        let group = adw::PreferencesGroup::new();
        group.set_title(title);
        for (name, value) in facts {
            group.add(&fact_row(name, &value));
        }
        state.info.page.append(&group);
    }

    if let Some(summary) = &photo.summary {
        let frame = adw::PreferencesGroup::new();
        frame.set_title("Frame");
        frame.add(&fact_row(
            "Size",
            &format!("{} × {}", summary.sensor.0, summary.sensor.1),
        ));
        frame.add(&fact_row("Resolution", &format!("{:.1} MP", summary.megapixels())));
        if let Some(bytes) = summary.file_size {
            frame.add(&fact_row("File", &format!("{:.0} MB", bytes as f64 / 1_048_576.0)));
        }
        state.info.page.append(&frame);
    }
}

pub(super) fn refresh_render_info(state: &App) {
    let open = state.open.borrow();
    let Some(photo) = open.as_ref() else { return };

    let mut lines: Vec<String> = Vec::new();
    let (width, height) = (photo.full_size.0, photo.full_size.1);
    lines.push(match &photo.full_working {
        Some(full) => match &photo.full_working_key {
            Some((_, FullHeld::Parts(parts))) => format!("Loaded: {} × {} full, {} part{} of it", full.width, full.height, parts.len(), if parts.len() == 1 { "" } else { "s" }),
            _ => format!("Loaded: {} × {} full", full.width, full.height),
        },
        None => format!(
            "Loaded: {} × {} proxy of {width} × {height}",
            photo.proxy.width, photo.proxy.height
        ),
    });

    let zoom = state.zooming.level.get();
    if zoom != FIT_ZOOM {
        let (shown_width, shown_height) = displayed_size(state).unwrap_or(photo.full_size);
        let canvas = ((shown_width as f64 * zoom) as u32, (shown_height as f64 * zoom) as u32);

        let over = canvas.0.max(canvas.1) > 16384 && state.render.tile.get().is_none();
        lines.push(format!(
            "Canvas: {} × {}{}",
            canvas.0,
            canvas.1,
            if over { "  (past the GPU's limit)" } else { "" }
        ));
    }

    let (rendered_width, rendered_height) = state.render.rendered_size.get();
    if rendered_width > 0 {
        let from = if state.render.rendered_from_full.get() { "full" } else { "proxy" };
        let tiled = if state.render.tile.get().is_some() { ", tile" } else { "" };
        lines.push(format!("Rendered: {rendered_width} × {rendered_height} ({from}{tiled})"));
    }

    lines.extend(numa::infer::gpu_report());

    let key = colour_key(&photo.document);
    if zoom > proxy_runs_out_at(photo) && !state.render.rendered_from_full.get() {
        let key_is_stale =
            photo.full_working.is_some() && photo.full_working_key.as_ref().is_some_and(|(held_key, _)| *held_key != key);
        lines.push(
            match (state.render.loading_full.get(), state.render.failed_key.borrow().as_ref() == Some(&key)) {
                (true, _) => "Waiting: the original is being decoded".to_string(),
                (_, true) => "Soft: the original could not be decoded".to_string(),
                _ if key_is_stale => {
                    "Soft: the colour changed since the original was decoded".to_string()
                }
                _ => "Soft: the original is not loaded".to_string(),
            },
        );
    }
    drop(open);

    let text = lines.join("\n");
    if state.info.render_info.text() != text {
        state.info.render_info.set_text(&text);
    }
}

#[derive(Clone)]
pub(super) struct State {

    pub(super) page: gtk::Box,

    pub(super) render_info: gtk::Label,

    pub(super) rendering: gtk::Expander,
    pub(super) histogram_area: gtk::DrawingArea,
    pub(super) histogram: Rc<RefCell<Option<render::histogram::Histogram>>>,

    pub(super) scope: Rc<RefCell<Option<render::scope::Scope>>>,
    pub(super) scope_kind: Rc<Cell<render::scope::Kind>>,
    pub(super) shadow_clip: gtk::ToggleButton,
    pub(super) highlight_clip: gtk::ToggleButton,
}

impl State {
    pub(super) fn new() -> Self {
        Self {
            page: gtk::Box::new(gtk::Orientation::Vertical, 18),
            render_info: gtk::Label::new(None),
            rendering: gtk::Expander::new(None),
            histogram_area: gtk::DrawingArea::new(),
            histogram: Rc::new(RefCell::new(None)),
            scope: Rc::new(RefCell::new(None)),
            scope_kind: Rc::new(Cell::new(render::scope::Kind::Histogram)),
            shadow_clip: gtk::ToggleButton::new(),
            highlight_clip: gtk::ToggleButton::new(),
        }
    }
}
