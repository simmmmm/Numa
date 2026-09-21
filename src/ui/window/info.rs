use super::*;

pub(super) fn build_histogram(state: &App) -> gtk::Box {
    let column = gtk::Box::new(gtk::Orientation::Vertical, 4);
    column.set_margin_bottom(4);

    let area = state.info.histogram_area.clone();
    area.set_content_height(84);
    area.set_hexpand(true);
    area.add_css_class("histogram");

    let histogram = state.info.histogram.clone();
    area.set_draw_func(move |_, context, width, height| {
        draw_histogram(context, width as f64, height as f64, histogram.borrow().as_ref());
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

    column.append(&stacked);
    column
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

        let picture = gtk::Image::from_paintable(Some(&texture_from(seen.portrait.clone())));
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

    let open = state.open.borrow();
    let Some(photo) = open.as_ref() else { return };

    if let Some(people) = people_group(state, photo) {
        state.info.page.append(&people);
    }

    if let Some(summary) = &photo.summary {
        let body = adw::PreferencesGroup::new();
        body.set_title("Camera");
        if let Some(camera) = &summary.camera {
            body.add(&fact_row("Body", camera));
        }
        if let Some(lens) = &summary.lens {
            body.add(&fact_row("Lens", lens));
        }
        if let Some(mode) = &summary.film_mode {

            body.add(&fact_row("Film mode", mode));
        }
        state.info.page.append(&body);

        let shot = adw::PreferencesGroup::new();
        shot.set_title("Exposure");
        if let Some(focal) = summary.focal_length {
            shot.add(&fact_row("Focal length", &format!("{focal:.0} mm")));
        }
        if let Some(aperture) = summary.aperture {
            shot.add(&fact_row("Aperture", &format!("f/{aperture:.1}")));
        }
        if let Some(shutter) = summary.shutter_text() {
            shot.add(&fact_row("Shutter", &shutter));
        }
        if let Some(iso) = summary.iso {
            shot.add(&fact_row("ISO", &iso.to_string()));
        }
        match summary.exposure_bias {
            Some(bias) if bias != 0.0 => {
                shot.add(&fact_row("Compensation", &format!("{bias:+.1} EV")))
            }
            _ => {}
        }
        if let Some(taken) = &summary.taken {
            shot.add(&fact_row("Taken", taken));
        }
        state.info.page.append(&shot);

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

    let mut lines: Vec<String> = Vec::new();
    let (width, height) = (photo.full_size.0, photo.full_size.1);
    lines.push(match &photo.full_working {
        Some(full) => format!("Loaded: {} × {} full", full.width, full.height),
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
            photo.full_working.is_some() && photo.full_working_key.as_ref() != Some(&key);
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

    state.info.render_info.set_text(&lines.join("\n"));
    state.info.render_info.set_xalign(0.0);
    state.info.render_info.set_wrap(true);
    state.info.render_info.set_selectable(true);
    state.info.render_info.add_css_class("profile-note");

    let disclosure = gtk::Expander::new(None);
    disclosure.set_label_widget(Some(&section_header("Rendering")));
    disclosure.set_child(Some(&state.info.render_info));
    state.info.page.append(&disclosure);
}

#[derive(Clone)]
pub(super) struct State {

    pub(super) page: gtk::Box,

    pub(super) render_info: gtk::Label,

    pub(super) button: gtk::MenuButton,
    pub(super) histogram_area: gtk::DrawingArea,
    pub(super) histogram: Rc<RefCell<Option<render::histogram::Histogram>>>,
    pub(super) shadow_clip: gtk::ToggleButton,
    pub(super) highlight_clip: gtk::ToggleButton,
}

impl State {
    pub(super) fn new() -> Self {
        Self {
            page: gtk::Box::new(gtk::Orientation::Vertical, 18),
            render_info: gtk::Label::new(None),
            button: gtk::MenuButton::new(),
            histogram_area: gtk::DrawingArea::new(),
            histogram: Rc::new(RefCell::new(None)),
            shadow_clip: gtk::ToggleButton::new(),
            highlight_clip: gtk::ToggleButton::new(),
        }
    }
}
