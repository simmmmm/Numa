use super::*;

pub(super) fn adjustments_changed(state: &App) {

    if state.applying.get() {
        return;
    }

    if !state.mask_overlay.wash_resting.replace(true) && state.mask_overlay.show_coverage.get() {
        state.mask_overlay.area.queue_draw();
    }

    if let Some(photo) = state.open.borrow_mut().as_mut() {
        photo.before_preset = None;
    }

    sync_document(state);
    request_render(state);
    schedule_history_push(state);
    refresh_slider_marks(state);
}

pub(super) fn sync_document(state: &App) {
    let mut open = state.open.borrow_mut();
    let Some(photo) = open.as_mut() else { return };

    if let Some(index) = state.mask_overlay.selected_mask.get() {

        if state.mask_overlay.sliders_hold.get() != Some(index) {
            log::warn!("the sliders hold {:?} while mask {index} is selected: not written", state.mask_overlay.sliders_hold.get());
            drop(open);
            let state = state.clone();
            glib::idle_add_local_once(move || select_mask(&state, Some(index)));
            return;
        }
        let mut masks = photo.document.masks();
        if let Some(mask) = masks.get_mut(index) {

            mask.basic = state.sliders.read();
            mask.basic.balance.temperature = state.colour.mask_temperature.value() as f32;
            mask.basic.balance.tint = -(state.colour.mask_tint.value() as f32);

            mask.colour.hue = state.colour.mask_hue.value() as f32;
            mask.colour.saturation = state.colour.mask_colour_strength.value() as f32;
            photo.document.set_masks(masks);
            return;
        }
        state.mask_overlay.selected_mask.set(None);
    }

    if let Some(held) = state.mask_overlay.sliders_hold.get() {
        log::warn!("the sliders hold mask {held} while the photograph is selected: not written");
        drop(open);
        let state = state.clone();
        glib::idle_add_local_once(move || select_mask(&state, None));
        return;
    }

    photo.document.set_basic(state.sliders.read());

    let balance = state.sliders.white_balance();
    photo.document.white_balance = (balance != photo.as_shot).then_some(balance);
}

pub(super) fn request_render(state: &App) {

    let now = std::time::Instant::now();
    let previous = state.render.last_request.replace(Some(now));
    let burst = previous.is_some_and(|previous| now.duration_since(previous) < SETTLE);
    state.render.drafting.set(burst || state.render.hand_down.get());
    settle_render(state);
    schedule_render(state);
}

pub(super) fn colour_key(document: &Document) -> ColourKey {
    (document.white_balance, document.colour_profile.clone(), document.ai_denoise, document.ai_sharpen)
}

pub(super) type ColourKey = (Option<WhiteBalance>, Option<String>, f32, f32);

pub(super) fn proxy_runs_out_at(photo: &OpenPhoto) -> f64 {
    let full = photo.full_size.0.max(photo.full_size.1) as f64;
    let proxy = photo.proxy.width.max(photo.proxy.height) as f64;
    if full <= 0.0 || proxy <= 0.0 {
        return 1.0;
    }
    proxy / full
}

pub(super) fn same_kind(cut: [f32; 4], region: [f32; 4]) -> bool {
    const WHOLE: [f32; 4] = [0.0, 0.0, 1.0, 1.0];
    (cut == WHOLE) == (region == WHOLE)
}

pub(super) fn covers(outer: [f32; 4], inner: [f32; 4]) -> bool {
    inner[0] >= outer[0] - 1e-4
        && inner[1] >= outer[1] - 1e-4
        && inner[0] + inner[2] <= outer[0] + outer[2] + 1e-4
        && inner[1] + inner[3] <= outer[1] + outer[3] + 1e-4
}

pub(super) fn timing() -> bool {
    static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ON.get_or_init(|| std::env::var_os("NUMA_TIMING").is_some())
}

pub(super) fn since_start_ms() -> f64 {
    let stat = std::fs::read_to_string("/proc/self/stat").unwrap_or_default();
    let started = stat.rsplit(')').next().and_then(|rest| rest.split_whitespace().nth(19)?.parse::<f64>().ok());
    let uptime = std::fs::read_to_string("/proc/uptime").ok().and_then(|up| up.split_whitespace().next()?.parse::<f64>().ok());
    match (started, uptime) {
        (Some(ticks), Some(up)) => (up - ticks / 100.0) * 1000.0,
        _ => 0.0,
    }
}

pub(super) fn time_scrolling(widget: &impl IsA<gtk::Widget>, adjustment: &gtk::Adjustment, what: &'static str) {
    if !timing() {
        return;
    }
    fn main_thread_ms() -> f64 {
        let stat = std::fs::read_to_string("/proc/thread-self/stat").unwrap_or_default();
        let fields: Vec<f64> = stat.rsplit(')').next().unwrap_or("").split_whitespace().filter_map(|f| f.parse().ok()).collect();
        fields.get(11).zip(fields.get(12)).map_or(0.0, |(user, sys)| (user + sys) * 10.0)
    }
    let moved = Rc::new(Cell::new(std::time::Instant::now()));
    let running = Rc::new(Cell::new(false));
    let widget = widget.as_ref().clone();
    adjustment.connect_value_changed(move |_| {
        moved.set(std::time::Instant::now());
        if running.replace(true) {
            return;
        }
        let (moved, running) = (moved.clone(), running.clone());
        let (gaps, previous, cpu) = (RefCell::new(Vec::<f64>::new()), Cell::new(None::<i64>), main_thread_ms());
        widget.add_tick_callback(move |_, clock| {
            let now = clock.frame_time();
            if let Some(previous) = previous.replace(Some(now)) {
                gaps.borrow_mut().push((now - previous) as f64 / 1000.0);
            }
            if moved.get().elapsed() < std::time::Duration::from_millis(300) {
                return glib::ControlFlow::Continue;
            }
            running.set(false);
            let mut gaps = gaps.take();
            gaps.sort_by(f64::total_cmp);
            let at = |q: f64| gaps.get(((gaps.len() as f64 - 1.0) * q).round() as usize).copied().unwrap_or(0.0);
            eprintln!(
                "timing: {what} scroll {} frames, median {:.1} ms, p95 {:.1} ms, max {:.1} ms, {} over 25 ms, main thread {:.0} cpu-ms",
                gaps.len(),
                at(0.5),
                at(0.95),
                at(1.0),
                gaps.iter().filter(|gap| **gap > 25.0).count(),
                main_thread_ms() - cpu
            );
            glib::ControlFlow::Break
        });
    });
}

pub(super) fn buffers(state: &App) -> String {
    let mut held = 0usize;
    let mut parts = Vec::new();
    if let Some(photo) = state.open.borrow().as_ref() {
        let mut note = |name: &str, bytes: usize| {
            if bytes > 0 {
                held += bytes;
                parts.push(format!("{name} {:.0} MB", bytes as f64 / 1_048_576.0));
            }
        };
        let linear = |image: &LinearImage| image.width as usize * image.height as usize * 3 * 4;
        note("proxy", linear(&*photo.proxy));
        note("working", linear(&*photo.working));
        note("draft", photo.draft.as_deref().map_or(0, linear));
        note("full", photo.full_working.as_deref().zip(photo.full_working_key.as_ref()).map_or(0, |(frame, (_, held))| held.bytes(frame)));
        note("tile", photo.view.as_ref().map_or(0, |view| linear(&view.image)));
        note("mask frame", photo.mask_frame.as_ref().map_or(0, |frame| frame.len()));
        note("denoised", photo.inputs.denoised.as_ref().map_or(0, |frame| frame.len() * 2));
    }

    let resident = std::fs::read_to_string("/proc/self/statm")
        .ok()
        .and_then(|line| line.split_whitespace().nth(1)?.parse::<usize>().ok())
        .map_or(0.0, |pages| pages as f64 * 4096.0 / 1_048_576.0);
    format!(" — held {:.0} MB ({}), resident {resident:.0} MB", held as f64 / 1_048_576.0, parts.join(", "))
}

pub(super) fn measures_tone(document: &Document) -> bool {
    let basic = document.basic();
    basic.presence.hdr != 0.0 || basic.presence.clarity != 0.0 || basic.presence.texture != 0.0
}

pub(super) fn fingerprint(document: &Document, key: &ColourKey) -> u64 {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    serde_json::to_string(document).unwrap_or_default().hash(&mut hasher);
    format!("{key:?}").hash(&mut hasher);
    hasher.finish()
}

pub(super) fn edge_for(region: [f32; 4], frame: (u32, u32), zoom: f64, drafting: bool) -> u32 {
    let needed = (region[2] as f64 * frame.0 as f64 * zoom)
        .max(region[3] as f64 * frame.1 as f64 * zoom)
        .ceil()
        .clamp(1.0, u32::MAX as f64) as u32;
    match drafting {
        true => (needed / 2).max(1),
        false => needed,
    }
}

pub(super) fn region_to_render(
    document: &Document,
    wanted: Option<[f32; 4]>,
    frame_width: u32,
    frame_height: u32,
) -> [f32; 4] {

    let guided = render::tiles_with_guide(document) && std::env::var_os("NUMA_WHOLE_FRAME").is_none();
    (render::tiles_cleanly(document) || guided)
        .then_some(wanted)
        .flatten()

        .filter(|rect| {
            render::spots_within(document, *rect, [frame_width as f32, frame_height as f32])
        })
        .unwrap_or([0.0, 0.0, 1.0, 1.0])
}

pub(super) fn present(
    state: &App,
    rendered: Picture,
    placement: Option<crate::ui::pixel_paintable::Placement>,
    backdrop: Option<image::RgbImage>,
    histogram: render::histogram::Histogram,
    wants_full: bool,
    have_full: bool,
) -> Option<image::RgbImage> {

    write_zoom_label(state);

    if wants_full && !have_full {
        ensure_full_resolution(state);
    }

    state.info.shadow_clip.set_visible(histogram.is_shadow_clipped());
    state.info.highlight_clip.set_visible(histogram.is_highlight_clipped());
    *state.info.histogram.borrow_mut() = Some(histogram);
    state.info.histogram_area.queue_draw();

    if state.editor_page.before.is_active() {
        return match rendered {
            Picture::Pixels(image) => Some(image),
            _ => None,
        };
    }

    let kept = show(state, rendered, placement, backdrop);

    refresh_render_info(state);
    kept
}

pub(super) fn schedule_history_push(state: &App) {

    schedule_save(state);

    let generation = state.render.history_generation.get().wrapping_add(1);
    state.render.history_generation.set(generation);

    let state = state.clone();
    glib::timeout_add_local_once(std::time::Duration::from_millis(400), move || {
        if state.render.history_generation.get() != generation {
            return;
        }
        if let Some(photo) = state.open.borrow_mut().as_mut() {
            let snapshot = EditState::of(&photo.document);
            photo.history.push(snapshot);
        }
    });
}

pub(super) fn step_history(state: &App, redo: bool) {
    let stepped = state.open.borrow_mut().as_mut().and_then(|photo| {

        let snapshot = EditState::of(&photo.document);
        photo.history.push(snapshot);
        let stepped = if redo { photo.history.redo() } else { photo.history.undo() };

        stepped.map(|state| (state, photo.as_shot))
    });

    let Some((edit, as_shot)) = stepped else {
        state.toast(if redo { "Nothing to redo" } else { "Nothing to undo" });
        return;
    };

    apply_history(state, edit, as_shot);
}

pub(super) fn jump_history(state: &App, position: usize) {
    let stepped = state.open.borrow_mut().as_mut().and_then(|photo| {

        let snapshot = EditState::of(&photo.document);
        photo.history.push(snapshot);
        photo.history.go_to(position).map(|state| (state, photo.as_shot))
    });

    let Some((edit, as_shot)) = stepped else { return };
    apply_history(state, edit, as_shot);
}

pub(super) fn apply_history(state: &App, edit: EditState, as_shot: WhiteBalance) {
    state.applying.set(true);
    state.sliders.write(edit.basic);
    state.mask_overlay.sliders_hold.set(None);
    state.sliders.write_white_balance(edit.white_balance.unwrap_or(as_shot));
    refresh_slider_marks(state);

    let angle = edit.crop.map_or(0.0, |(_, angle)| angle);
    state.crop.straighten.set_value(angle as f64);
    state.applying.set(false);

    {
        let mut open = state.open.borrow_mut();
        if let Some(photo) = open.as_mut() {
            let space_moved = photo.document.working_space != edit.working_space;
            edit.restore(&mut photo.document);
            if space_moved {
                photo.inputs = render_inputs(&photo.document);
                photo.working = Arc::new(render::to_working_space(&photo.document, &*photo.proxy, &photo.inputs));
                photo.full_working = None;
                photo.full_working_key = None;
                photo.draft = None;
            }
            photo.view = None;
        }
    }
    state.crop.rect.set(tool_rect(state).0);
    state.crop.fit_base.set(None);
    write_mixer(state);
    write_point_colours(state);
    write_grading(state);
    write_perspective(state);
    ai_denoise::write(state);
    lut::write(state);
    refresh_retouch(state);
    refresh_face(state);
    refresh_found(state);

    fill_segment_masks(state);
    refresh_masks(state);
    select_mask(state, None);
    refresh_profile_picker(state);
    state.crop.area.queue_draw();
    state.light.curve_area.queue_draw();

    sync_document(state);
    request_render(state);

    schedule_save(state);

    refresh_history(state);
}

pub(super) fn copy_image(state: &App) {
    let job = state.open.borrow().as_ref().map(|photo| {
        let document = photo.document.clone();
        let working = photo.working.clone();
        let scale = working.width.max(working.height) as f32 / photo.full_size.0.max(photo.full_size.1).max(1) as f32;
        (document, working, scale)
    });
    let Some((document, working, scale)) = job else { return };
    let state = state.clone();
    glib::spawn_future_local(async move {
        let Ok(image) = busy(&state, "Copying the picture…", move || {
            let document = render::with_masks_resolved(&document, &working);
            render::apply_stack(&document, &*working, scale)
        })
        .await
        else {
            return;
        };
        if let Some(display) = gtk::gdk::Display::default() {

            display.clipboard().set_texture(&crate::ui::display::unconverted(image));
            state.toast("Picture copied");
        }
    });
}

pub(super) fn texture_from(image: &image::RgbImage) -> gtk::gdk::Texture {
    crate::ui::display::texture(image)
}

pub(super) fn show(
    state: &App,
    image: Picture,
    placement: Option<crate::ui::pixel_paintable::Placement>,
    backdrop: Option<image::RgbImage>,
) -> Option<image::RgbImage> {

    let (texture, kept) = image.texture(state);
    let texture = texture?;
    let paintable = match placement {
        Some(placement) => crate::ui::pixel_paintable::PixelPaintable::with_placement(
            texture,
            placement,
            backdrop.as_ref().map(texture_from),
        ),
        None => crate::ui::pixel_paintable::PixelPaintable::new(texture),
    };
    state.canvas.set_paintable(Some(&paintable));
    apply_zoom(state);
    kept
}

#[derive(Clone)]
pub(super) struct State {

    pub(super) rendered_size: Rc<Cell<(u32, u32)>>,

    pub(super) full_resolution_stale: Rc<Cell<bool>>,

    pub(super) failed_key: Rc<RefCell<Option<ColourKey>>>,

    pub(super) loading_full: Rc<Cell<bool>>,

    pub(super) tile: Rc<Cell<Option<[f32; 4]>>>,

    pub(super) rendered_from_full: Rc<Cell<bool>>,

    pub(super) recentring: Rc<Cell<bool>>,

    pub(super) zoom_generation: Rc<Cell<u64>>,

    pub(super) history_generation: Rc<Cell<u64>>,

    pub(super) save_generation: Rc<Cell<u64>>,

    pub(super) render_pending: Rc<Cell<bool>>,

    pub(super) drafting: Rc<Cell<bool>>,
    pub(super) settle_generation: Rc<Cell<u64>>,

    pub(super) last_request: Rc<Cell<Option<std::time::Instant>>>,

    pub(super) hand_down: Rc<Cell<bool>>,

    pub(super) in_flight: Rc<Cell<bool>>,
    pub(super) again: Rc<Cell<bool>>,

    pub(super) planned: Rc<Cell<u64>>,
    pub(super) presented: Rc<Cell<u64>>,

    pub(super) opened_at: Rc<Cell<Option<(std::time::Instant, bool, f32)>>>,

    pub(super) prefetch: super::prefetch::State,

    pub(super) coming: Rc<RefCell<Option<Hold>>>,

    pub(super) on_screen: Rc<RefCell<Option<(u64, Document, image::RgbImage)>>>,

    pub(super) camera_view: Rc<RefCell<Option<(u64, ViewTile)>>>,

    pub(super) draft_source: Rc<RefCell<Option<(std::sync::Weak<LinearImage>, u32, Arc<LinearImage>)>>>,
}

impl State {
    pub(super) fn new() -> Self {
        Self {
            rendered_size: Rc::new(Cell::new((0, 0))),
            full_resolution_stale: Rc::new(Cell::new(false)),
            failed_key: Rc::new(RefCell::new(None)),
            loading_full: Rc::new(Cell::new(false)),
            tile: Rc::new(Cell::new(None)),
            rendered_from_full: Rc::new(Cell::new(false)),
            recentring: Rc::new(Cell::new(false)),
            zoom_generation: Rc::new(Cell::new(0)),
            history_generation: Rc::new(Cell::new(0)),
            save_generation: Rc::new(Cell::new(0)),
            render_pending: Rc::new(Cell::new(false)),
            drafting: Rc::new(Cell::new(false)),
            settle_generation: Rc::new(Cell::new(0)),
            last_request: Rc::default(),
            hand_down: Rc::new(Cell::new(false)),
            in_flight: Rc::default(),
            again: Rc::default(),
            planned: Rc::default(),
            presented: Rc::default(),
            opened_at: Rc::default(),
            prefetch: Default::default(),
            coming: Rc::default(),
            on_screen: Rc::default(),
            camera_view: Rc::default(),
            draft_source: Rc::default(),
        }
    }
}
