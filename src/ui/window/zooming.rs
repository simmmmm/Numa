use super::*;

pub(super) fn set_zoom(state: &App, zoom: f64) {
    let was_full = needs_full_resolution(state, state.zooming.level.get());

    state.render.tile.set(None);
    state.zooming.level.set(zoom);
    apply_zoom(state);

    let wants_full = needs_full_resolution(state, zoom);
    if wants_full {

        let generation = state.render.zoom_generation.get().wrapping_add(1);
        state.render.zoom_generation.set(generation);

        let state = state.clone();
        glib::timeout_add_local_once(std::time::Duration::from_millis(250), move || {
            let still_wanted = needs_full_resolution(&state, state.zooming.level.get());
            if state.render.zoom_generation.get() == generation && still_wanted {
                ensure_full_resolution(&state);
            }
        });
    } else if was_full {

        if let Some(photo) = state.open.borrow_mut().as_mut() {
            photo.full_working = None;
            photo.full_working_key = None;
            photo.view = None;
        }
        let (opened, zoomed) = (state.open_generation.get(), state.render.zoom_generation.get());
        let later = state.clone();
        glib::timeout_add_local_once(ORIGINAL_HELD, move || {

            if later.open_generation.get() == opened && later.render.zoom_generation.get() == zoomed {
                if let Some(photo) = later.open.borrow_mut().as_mut() {
                    photo.full_native = None;
                }
                give_back_freed_memory();
            }
        });
        request_render(state);
        refresh_render_info(state);
    }
}

const ORIGINAL_HELD: std::time::Duration = std::time::Duration::from_secs(30);

pub(super) fn ensure_full_resolution(state: &App) {

    let (wanted, frame) = (tile_for(state), displayed_size(state));
    let request = {
        let open = state.open.borrow();
        let Some(photo) = open.as_ref() else { return };

        let key = colour_key(&photo.document);
        let shown = rendered_document(state, photo);
        let region = frame.map_or(WHOLE_FRAME, |frame| region_to_render(&shown, wanted, frame.0, frame.1));
        let needs = full_needs(&shown, region, photo.full_size);
        if photo.full_working_key.as_ref().is_some_and(|(held_key, held)| *held_key == key && held.covers(needs)) {

            state.render.full_resolution_stale.set(false);
            return;
        }

        if state.render.failed_key.borrow().as_ref() == Some(&key) {
            return;
        }
        if state.render.loading_full.get() {

            state.render.full_resolution_stale.set(true);
            return;
        }

        let kept = match (&photo.full_working, &photo.full_working_key) {
            (Some(frame), Some((held_key, FullHeld::Parts(parts)))) if *held_key == key => Some((frame.clone(), parts.clone())),
            _ => None,
        };
        (photo.source.clone(), photo.document.clone(), key, photo.full_native.clone(), needs, kept, photo.full_size)
    };

    let (source, document, key, native, needs, kept, full_size) = request;
    state.render.loading_full.set(true);
    state.render.full_resolution_stale.set(false);
    state.zooming.label.set_text("…");

    let generation = state.open_generation.get();
    let state = state.clone();
    glib::spawn_future_local(async move {

        let hold = state.zooming.waiting.hold();
        let decoded = gio::spawn_blocking(move || {

            let native = match native {
                Some(native) => native,
                None => Native::open(&source)?,
            };
            let inputs = render_inputs(&document);
            let (working, held) = native.develop(&document, &inputs, needs, kept, full_size);
            Ok::<_, String>((working, held, native))
        })
        .await;

        state.render.loading_full.set(false);

        if state.open_generation.get() != generation {
            return;
        }

        if !needs_full_resolution(&state, state.zooming.level.get()) {
            state.render.full_resolution_stale.set(false);
            return;
        }

        match decoded {
            Ok(Ok((working, held, native))) => {
                if let Some(photo) = state.open.borrow_mut().as_mut() {
                    photo.full_working = Some(Arc::new(working));
                    photo.full_working_key = Some((key, held));
                    photo.full_native = Some(native);
                }

                state.render.coming.replace(Some(hold));
                request_render(&state);
                refresh_render_info(&state);
            }
            Ok(Err(err)) => {
                *state.render.failed_key.borrow_mut() = Some(key);
                state.toast(&format!("Could not load full resolution: {err}"));
            }
            Err(_) => {}
        }

        apply_zoom(&state);

        if state.render.full_resolution_stale.get() {
            ensure_full_resolution(&state);
        }
    });
}

#[derive(Clone, Debug, PartialEq)]
pub(super) enum FullHeld {
    Whole,
    Parts(Vec<[u32; 4]>),
}

impl FullHeld {

    pub(super) fn covers(&self, needs: Option<[u32; 4]>) -> bool {
        match (self, needs) {
            (FullHeld::Whole, _) => true,
            (FullHeld::Parts(_), None) => false,
            (FullHeld::Parts(parts), Some([x, y, width, height])) => parts
                .iter()
                .any(|[px, py, pw, ph]| *px <= x && *py <= y && x + width <= px + pw && y + height <= py + ph),
        }
    }

    pub(super) fn bytes(&self, frame: &LinearImage) -> usize {
        match self {
            FullHeld::Whole => frame.data.len() * 4,
            FullHeld::Parts(parts) => parts.iter().map(|[_, _, width, height]| *width as usize * *height as usize * 12).sum(),
        }
    }
}

pub(super) fn full_needs(document: &Document, region: [f32; 4], full_size: (u32, u32)) -> Option<[u32; 4]> {
    if region == WHOLE_FRAME || measures_tone(document) || document.ai_denoise > 0.0 || document.ai_sharpen > 0.0 {
        return None;
    }
    render::tile_box(document, full_size.0, full_size.1, region)
}

const WHOLE_FRAME: [f32; 4] = [0.0, 0.0, 1.0, 1.0];

const PART_SLACK: u32 = 128;

#[derive(Clone)]
pub(super) enum Native {
    Whole(Arc<LinearImage>),
    Parts(Arc<raw::region::Regions>),
}

impl Native {
    fn open(source: &Source) -> Result<Native, String> {
        match source {

            Source::Photo { path, .. } if raw::is_raw(path) && !raw::card_ready() => {
                Ok(Native::Parts(Arc::new(raw::region::Regions::open(path)?)))
            }
            _ => Ok(Native::Whole(Arc::new(source.full_resolution()?))),
        }
    }

    fn develop(
        &self,
        document: &Document,
        inputs: &render::RenderInputs,
        needs: Option<[u32; 4]>,
        kept: Option<(Arc<LinearImage>, Vec<[u32; 4]>)>,
        (width, height): (u32, u32),
    ) -> (LinearImage, FullHeld) {
        let whole = [0, 0, width, height];
        let part = needs.map(|[x, y, w, h]| {
            let (left, top) = (x.saturating_sub(PART_SLACK), y.saturating_sub(PART_SLACK));
            let (right, bottom) = ((x + w + PART_SLACK).min(width), (y + h + PART_SLACK).min(height));
            [left, top, right - left, bottom - top]
        });
        let (kept_frame, mut parts) = kept.map_or((None, Vec::new()), |(frame, parts)| (Some(frame), parts));
        let area = |[_, _, w, h]: [u32; 4]| w as u64 * h as u64;
        let Some(part) = part.filter(|part| (parts.iter().copied().map(area).sum::<u64>() + area(*part)) * 2 < area(whole)) else {
            let developed = match self {
                Native::Whole(full) => render::to_working_space(document, &**full, inputs),
                Native::Parts(regions) => render::to_working_space(document, regions.region(whole), inputs),
            };
            return (developed, FullHeld::Whole);
        };

        let mut laps = raw::Laps::start();
        let region = match self {
            Native::Whole(full) => cut(full, part),
            Native::Parts(regions) => regions.region(part),
        };
        laps.lap("develop");
        let developed = render::to_working_space(document, region, inputs);
        laps.lap("colour stage");
        let mut frame = LinearImage::new(width, height, vec![0.0; width as usize * height as usize * 3]).with_film_mode(developed.film_mode.clone());
        frame.white_point = developed.white_point;
        if let Some(kept) = &kept_frame {
            for &rect in &parts {
                place(&mut frame, rect, |row| &kept.data[((rect[1] + row) * width + rect[0]) as usize * 3..][..rect[2] as usize * 3]);
            }
        }
        place(&mut frame, part, |row| &developed.data[(row * part[2]) as usize * 3..][..part[2] as usize * 3]);
        laps.lap("placed");
        laps.report(&format!("full-resolution part {}×{}", part[2], part[3]), std::path::Path::new(&document.source.path));
        parts.push(part);
        (frame, FullHeld::Parts(parts))
    }
}

impl Native {

    pub(super) fn camera(&self, part: [u32; 4], (width, height): (u32, u32)) -> Arc<LinearImage> {
        let region = match self {
            Native::Whole(full) => return full.clone(),
            Native::Parts(regions) => regions.region(part),
        };
        let mut frame = LinearImage {
            width,
            height,
            data: vec![0.0; width as usize * height as usize * 3],
            white_point: region.white_point,
            profile: region.profile.clone(),
            clip: region.clip,
            rendering: region.rendering.clone(),
            film_mode: region.film_mode.clone(),
            display_referred: region.display_referred,
        };
        place(&mut frame, part, |row| &region.data[(row * part[2]) as usize * 3..][..part[2] as usize * 3]);
        Arc::new(frame)
    }
}

fn cut(full: &LinearImage, [x, y, width, height]: [u32; 4]) -> LinearImage {
    let stride = full.width as usize * 3;
    let mut data = Vec::with_capacity(width as usize * height as usize * 3);
    for row in y..y + height {
        let at = row as usize * stride + x as usize * 3;
        data.extend_from_slice(&full.data[at..at + width as usize * 3]);
    }
    LinearImage {
        width,
        height,
        data,
        white_point: full.white_point,
        profile: full.profile.clone(),
        clip: full.clip,
        rendering: full.rendering.clone(),
        film_mode: full.film_mode.clone(),
        display_referred: full.display_referred,
    }
}

fn place<'a>(frame: &mut LinearImage, [x, y, width, height]: [u32; 4], rows: impl Fn(u32) -> &'a [f32]) {
    let stride = frame.width as usize * 3;
    for row in 0..height {
        let at = (y + row) as usize * stride + x as usize * 3;
        frame.data[at..at + width as usize * 3].copy_from_slice(rows(row));
    }
}

pub(super) fn fit_scale(state: &App) -> f64 {
    let Some((width, height)) = displayed_size(state) else {
        return 1.0;
    };
    let (available_x, available_y) = (
        state.zooming.scroller.width() as f64,
        state.zooming.scroller.height() as f64,
    );

    if width == 0 || height == 0 || available_x <= 0.0 || available_y <= 0.0 {
        return 1.0;
    }

    (available_x / width as f64).min(available_y / height as f64) * device_scale(state)
}

pub(super) fn device_scale(state: &App) -> f64 {
    state.canvas.scale_factor().max(1) as f64
}

pub(super) fn scaled_zoom(state: &App, factor: f64) -> f64 {
    let floor = fit_scale(state);
    let wanted = effective_zoom(state).max(1e-6) * factor;

    if wanted <= floor * 1.01 {
        FIT_ZOOM
    } else {
        wanted.min(MAX_ZOOM)
    }
}

pub(super) fn effective_zoom(state: &App) -> f64 {
    if state.zooming.level.get() == FIT_ZOOM {
        fit_scale(state)
    } else {
        state.zooming.level.get()
    }
}

pub(super) fn zoom_about_centre(state: &App, zoom: f64) {
    let horizontal = state.zooming.scroller.hadjustment();
    let vertical = state.zooming.scroller.vadjustment();

    let before = (effective_zoom(state) / device_scale(state)).max(1e-6);

    let focus = [&horizontal, &vertical].map(|adjustment| {
        (adjustment.value() + adjustment.page_size() / 2.0) / before
    });

    zoom_keeping(state, zoom, focus);
}

pub(super) fn apply_zoom(state: &App) {
    let zoom = state.zooming.level.get();

    if zoom == FIT_ZOOM {
        state.canvas.set_size_request(-1, -1);
        state.canvas.set_halign(gtk::Align::Fill);
        state.canvas.set_valign(gtk::Align::Fill);
        write_zoom_label(state);
        queue_reference(state);
        return;
    }

    let Some((full_width, full_height)) = displayed_size(state) else {
        return;
    };
    let scale = device_scale(state);
    let width = (full_width as f64 * zoom / scale).round() as i32;
    let height = (full_height as f64 * zoom / scale).round() as i32;

    state.canvas.set_size_request(width.max(1), height.max(1));
    state.canvas.set_halign(gtk::Align::Center);
    state.canvas.set_valign(gtk::Align::Center);

    write_zoom_label(state);

    queue_reference(state);
}

pub(super) fn write_zoom_label(state: &App) {

    let note = camera_note(state);
    let zoom = state.zooming.level.get();

    state.zooming.label.set_visible(zoom != FIT_ZOOM || !note.is_empty());
    if zoom == FIT_ZOOM {
        let fit = format!("Fit {:.0}%", effective_zoom(state) * 100.0);
        state.zooming.label.set_text(&(fit + &note));
        return;
    }
    let full_ready = state.render.rendered_from_full.get();
    let said = match (needs_full_resolution(state, zoom), full_ready) {
        (true, false) if state.render.loading_full.get() => "…".to_string(),
        (true, false) => format!("{:.0}% soft", zoom * 100.0),
        _ => format!("{:.0}%", zoom * 100.0),
    };
    state.zooming.label.set_text(&(said + &note));
}

#[derive(Clone)]
pub(super) struct State {
    pub(super) level: Rc<Cell<f64>>,
    pub(super) label: gtk::Label,

    pub(super) waiting: Waiting,

    pub(super) scroller: gtk::ScrolledWindow,
}

impl State {
    pub(super) fn new(canvas: &gtk::Picture) -> Self {
        Self {
            level: Rc::new(Cell::new(FIT_ZOOM)),
            label: gtk::Label::new(Some("Fit")),
            waiting: Waiting::new("Loading the photograph", canvas),
            scroller: gtk::ScrolledWindow::new(),
        }
    }
}
