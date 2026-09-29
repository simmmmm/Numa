use super::*;

struct Frames {
    working: Arc<LinearImage>,
    draft: Option<Arc<LinearImage>>,
    full: Option<Arc<LinearImage>>,

    view: Option<ViewTile>,
    tone_guide: Option<(u64, bool, Arc<render::local::ToneGuide>)>,
    behind: Option<(u64, Arc<image::RgbImage>)>,
}

struct ColourWork {
    proxy: Arc<LinearImage>,
    inputs: render::RenderInputs,

    source: Option<Arc<LinearImage>>,

    edge: u32,
}

pub(super) struct Job {
    serial: u64,
    generation: u64,
    document: Document,
    key: ColourKey,

    base_key: ColourKey,
    colour: Option<ColourWork>,
    frames: Frames,
    drafting: bool,
    frame: (u32, u32),
    visible: [f32; 4],
    region: [f32; 4],
    needed: u32,
    wants_full: bool,
    have_full: bool,

    full_is_whole: bool,
    geometry: Geometry,
    proxy_scale: f32,

    scope_kind: render::scope::Kind,

    card: Option<CardAsk>,

    recolour: Option<Recolour>,

    first: Option<(image::RgbImage, render::histogram::Histogram)>,
    started: Option<std::time::Instant>,
    planned: std::time::Instant,
}

struct Recolour {
    native: Native,
    inputs: render::RenderInputs,
    full_size: (u32, u32),

    region: [f32; 4],
    needed: u32,

    kept: Option<ViewTile>,
}

#[derive(Default)]
struct Made {

    working: Option<(Arc<LinearImage>, ColourKey)>,

    draft: Option<Option<Arc<LinearImage>>>,

    view: Option<Option<ViewTile>>,
    tone_guide: Option<(u64, bool, Arc<render::local::ToneGuide>)>,
    behind: Option<(u64, Arc<image::RgbImage>)>,

    camera: Option<ViewTile>,

    source: Option<(std::sync::Weak<LinearImage>, u32, Arc<LinearImage>)>,
}

pub(super) struct Done {
    serial: u64,
    generation: u64,

    document: Document,
    base_key: ColourKey,
    made: Made,
    drafting: bool,
    rendered: Picture,
    placement: Option<crate::ui::pixel_paintable::Placement>,
    backdrop: Option<image::RgbImage>,
    histogram: render::histogram::Histogram,
    scope: Option<render::scope::Scope>,
    tile: Option<[f32; 4]>,
    wants_full: bool,
    have_full: bool,
    path: &'static str,
    recut: bool,
    started: Option<std::time::Instant>,

    laps: String,
    worked: std::time::Instant,
}

pub(super) fn schedule_render(state: &App) {
    if state.render.render_pending.replace(true) {
        return;
    }

    if prefetch::has_first(state) {
        let state = state.clone();
        glib::idle_add_local_full(glib::Priority::HIGH, move || {
            state.render.render_pending.set(false);
            start_render(&state);
            glib::ControlFlow::Break
        });
        return;
    }
    let state = state.clone();
    state.canvas.clone().add_tick_callback(move |_, _| {
        state.render.render_pending.set(false);
        start_render(&state);
        glib::ControlFlow::Break
    });
}

fn start_render(state: &App) {
    if state.render.in_flight.get() {
        state.render.again.set(true);
        return;
    }
    let Some(job) = plan(state) else { return };

    if job.first.is_some() {
        finish(state, work(job));
        return;
    }
    state.render.in_flight.set(true);
    let state = state.clone();

    let working = gio::spawn_blocking(move || work(job));
    glib::spawn_future_local(async move {
        let done = working.await;
        state.render.in_flight.set(false);
        match done {
            Ok(done) => finish(&state, done),
            Err(_) => log::warn!("a render stopped before it finished"),
        }
        if state.render.again.replace(false) {
            schedule_render(&state);
        }
    });
}

pub(super) fn render_current(state: &App) {
    if let Some(job) = plan(state) {
        finish(state, work(job));
    }
}

fn plan(state: &App) -> Option<Job> {
    let started = timing().then(std::time::Instant::now);

    let zoom = state.zooming.level.get();
    let frame = displayed_size(state)?;
    let visible = visible_rect(state).unwrap_or([0.0, 0.0, 1.0, 1.0]);
    let wanted = tile_for(state);
    let drafting = state.render.drafting.get();

    let mut open = state.open.borrow_mut();
    let photo = open.as_mut()?;

    let key = colour_key(&photo.document);
    let colour = (key != photo.working_key).then(|| {

        photo.inputs = render_inputs(&photo.document);
        ColourWork { proxy: photo.proxy.clone(), inputs: photo.inputs.clone(), source: None, edge: photo.proxy.width.max(photo.proxy.height) / 2 }
    });
    let document = rendered_document(state, photo);
    let card = card_render::ask(state, photo);

    let wants_full = zoom > proxy_runs_out_at(photo);
    let region = region_to_render(&document, wanted, frame.0, frame.1);

    let needs = full_needs(&document, region, photo.full_size);
    let (have_full, full_is_whole) = match (&photo.full_working, &photo.full_working_key) {
        (Some(_), Some((held_key, held))) if *held_key == key => (held.covers(needs), *held == FullHeld::Whole),
        _ => (false, false),
    };
    let needed = edge_for(region, frame, zoom, drafting);

    let all_of_it = region[2] >= 0.999 && region[3] >= 0.999;
    let wants_full = wants_full && !(drafting && all_of_it);

    let recolour = recolour_ask(state, photo, &document, &key, (wants_full && !have_full, visible, frame, zoom));
    let mut colour = colour;
    if let Some(colour) = colour.as_mut() {

        if recolour.is_some() {
            colour.edge /= 2;
        }
        let kept = state.render.draft_source.borrow();
        colour.source = kept.as_ref().filter(|(of, edge, _)| std::ptr::eq(of.as_ptr(), Arc::as_ptr(&colour.proxy)) && *edge == colour.edge).map(|(_, _, source)| source.clone());
    }

    let first = prefetch::take_first(state)
        .filter(|(print, ..)| colour.is_none() && card.is_none() && !drafting && !wants_full && *print == fingerprint(&document, &key))
        .map(|(_, frame, histogram)| (frame, histogram));

    let proxy_scale = photo.proxy.width.max(photo.proxy.height) as f32
        / photo.full_size.0.max(photo.full_size.1).max(1) as f32;

    let serial = state.render.planned.get() + 1;
    state.render.planned.set(serial);
    Some(Job {
        serial,
        generation: state.open_generation.get(),
        geometry: geometry_of_document(&document),
        document,
        base_key: photo.working_key.clone(),
        key,
        colour,
        frames: Frames {
            working: photo.working.clone(),
            draft: photo.draft.clone(),
            full: photo.full_working.clone().filter(|_| have_full),
            view: match drafting {
                true => photo.draft_view.clone(),
                false => photo.view.clone(),
            },
            tone_guide: photo.tone_guide.clone(),
            behind: photo.behind.clone(),
        },
        drafting,
        frame,
        visible,
        region,
        needed,
        wants_full,
        have_full,
        full_is_whole,
        proxy_scale,
        scope_kind: state.info.scope_kind.get(),
        card,
        recolour,
        first,
        started,
        planned: std::time::Instant::now(),
    })
}

fn work(job: Job) -> Done {
    let Job { serial, generation, document, key, base_key, colour, mut frames, drafting, ref card, .. } = job;
    let mut made = Made::default();

    let mut laps = String::new();
    let mut lap_at = std::time::Instant::now();
    if let Some(started) = job.started {
        laps += &format!("plan {:.1}, queued {:.1}", ms(job.planned - started), ms(job.planned.elapsed()));
    }
    let mut lap = |name: &str| {
        if job.started.is_some() {
            laps += &format!(", {name} {:.1}", ms(lap_at.elapsed()));
            lap_at = std::time::Instant::now();
        }
    };

    if let Some((frame, histogram)) = job.first {
        let scope = render::scope::of(&frame, job.scope_kind);
        let (rendered, worked) = (Picture::Pixels(frame), std::time::Instant::now());
        let (wants_full, have_full, started) = (job.wants_full, job.have_full, job.started);
        return Done { serial, generation, document, base_key, made, drafting, rendered, placement: None, backdrop: None, histogram, scope, tile: None, wants_full, have_full, path: "ahead", recut: false, started, laps, worked };
    }

    let on_card = card.as_ref().filter(|_| !(job.wants_full && job.have_full)).and_then(|ask| render_on_card(ask, &document, job.proxy_scale, job.scope_kind));
    if let Some(colour) = colour {

        if !(on_card.is_some() && drafting) {
            colour_stage(&document, &key, colour, drafting, &mut frames, &mut made);
            lap("colour");
        }
    }
    if let Some((rendered, histogram)) = on_card {
        let scope = match &rendered {
            Picture::Pixels(image) => render::scope::of(image, job.scope_kind),
            _ => None,
        };
        let (wants_full, have_full) = (job.wants_full, job.have_full);
        let worked = std::time::Instant::now();
        return Done { serial, generation, document, base_key, made, drafting, rendered, placement: None, backdrop: None, histogram, scope, tile: None, wants_full, have_full, path: "card", recut: false, started: job.started, laps, worked };
    }

    let (wants_full, have_full) = (job.wants_full, job.have_full);
    let fits = |view: &ViewTile| {
        view.key == key && view.geometry == job.geometry && covers(view.rect, job.visible) && same_kind(view.rect, job.region)
    };
    let reusable = frames.view.as_ref().is_some_and(|view| fits(view) && view.edge >= job.needed);
    let recut = wants_full && have_full && !reusable;
    if recut {
        let full = frames.full.as_deref().expect("checked by have_full");
        frames.view = cut_view_tile(full, &document, &key, job.geometry, job.region, job.needed);
        made.view = Some(frames.view.clone());
    }

    let recoloured = job.recolour.and_then(|ask| recoloured(ask, &document, &key, job.geometry, &mut made));
    let is_recoloured = recoloured.is_some();
    if recoloured.is_some() {
        frames.view = recoloured;
    }
    let usable_view = is_recoloured || (wants_full && have_full && frames.view.as_ref().is_some_and(fits));

    let path = match (is_recoloured, usable_view, wants_full && have_full) {
        (true, ..) => "recoloured",
        (_, true, _) => "tile",
        (_, _, true) => "full",
        _ => "proxy",
    };

    let guide = match usable_view && measures_tone(&document) {
        true => tone_guide(&mut frames, &mut made, &document, &key, drafting),
        false => None,
    };
    let usable_view = usable_view && (guide.is_some() || !measures_tone(&document));
    let (rendered, placement, whole_frame) = match (usable_view, frames.view.as_ref()) {
        (true, Some(view)) => render_view_tile(&document, view, job.frame.0, job.frame.1, guide.as_deref()),

        _ if wants_full && have_full && job.full_is_whole && !drafting => {
            let full = frames.full.as_deref().expect("checked by have_full");
            (render::apply_stack(&document, full, 1.0), None, true)
        }

        _ => (render_proxy(&mut frames, &mut made, &document, job.proxy_scale, drafting), None, true),
    };
    lap("stack");
    let tile = (!whole_frame).then_some(frames.view.as_ref().map_or([0.0, 0.0, 1.0, 1.0], |view| view.rect));

    let have_full = have_full || (is_recoloured && usable_view && drafting);
    let (backdrop, histogram) =
        whole_frame_behind(&mut frames, &mut made, &document, &key, whole_frame, job.proxy_scale, drafting, &rendered);

    let scope = render::scope::of(backdrop.as_ref().unwrap_or(&rendered), job.scope_kind);
    lap("behind and histogram");

    Done {
        serial,
        generation,
        document,
        base_key,
        made,
        drafting,
        rendered: Picture::Pixels(rendered),
        placement,
        backdrop,
        histogram,
        scope,
        tile,
        wants_full,
        have_full,
        path,
        recut,
        started: job.started,
        laps,
        worked: std::time::Instant::now(),
    }
}

fn ms(duration: std::time::Duration) -> f32 {
    duration.as_secs_f32() * 1000.0
}

fn colour_stage(document: &Document, key: &ColourKey, colour: ColourWork, drafting: bool, frames: &mut Frames, made: &mut Made) {
    match drafting {
        true => {

            let small = colour.source.or_else(|| {
                let small = Arc::new(colour.proxy.downscaled(colour.edge)?);
                made.source = Some((Arc::downgrade(&colour.proxy), colour.edge, small.clone()));
                Some(small)
            });
            frames.draft = small.map(|small| Arc::new(render::to_working_space(document, &*small, &colour.inputs)));
            made.draft = Some(frames.draft.clone());
        }
        false => {
            frames.working = Arc::new(render::to_working_space(document, &*colour.proxy, &colour.inputs));
            made.working = Some((frames.working.clone(), key.clone()));

            frames.draft = None;
            made.draft = Some(None);
        }
    }
}

fn finish(state: &App, done: Done) {
    if state.open_generation.get() != done.generation {
        return;
    }
    {
        let mut open = state.open.borrow_mut();
        let Some(photo) = open.as_mut() else { return };

        let current = photo.working_key == done.base_key;
        if let Some((working, key)) = done.made.working.clone().filter(|_| current) {
            photo.working = working;
            photo.working_key = key;
        }
        if let Some(draft) = done.made.draft.clone().filter(|_| current) {
            photo.draft = draft;
        }
        if let Some(view) = done.made.view.clone() {
            match (done.drafting, view.is_some()) {
                (true, true) => photo.draft_view = view,
                (false, true) => photo.view = view,

                (_, false) => {
                    photo.view = None;
                    photo.draft_view = None;
                }
            }
        }
        if let Some(guide) = done.made.tone_guide.clone() {
            photo.tone_guide = Some(guide);
        }
        if let Some(behind) = done.made.behind.clone() {
            photo.behind = Some(behind);
        }
        if let Some(camera) = done.made.camera.clone() {
            *state.render.camera_view.borrow_mut() = Some((done.generation, camera));
        }
        if let Some(source) = done.made.source.clone() {
            *state.render.draft_source.borrow_mut() = Some(source);
        }
    }

    if done.serial < state.render.presented.get() {
        return;
    }
    state.render.presented.set(done.serial);
    state.render.rendered_from_full.set(done.wants_full && done.have_full);
    state.render.tile.set(done.tile);
    state.render.rendered_size.set(done.rendered.dimensions());

    if let Some(started) = done.started {
        let (width, height) = done.rendered.dimensions();
        log::info!(
            "render {}{}{} {width}x{height} in {:.1} ms{}",
            done.path,
            if done.drafting { " draft" } else { "" },
            if done.recut { " recut" } else { "" },
            started.elapsed().as_secs_f32() * 1000.0,
            buffers(state),
        );
    }

    *state.info.scope.borrow_mut() = done.scope;
    let presenting = std::time::Instant::now();
    let back = ms(presenting - done.worked);
    let kept = present(state, done.rendered, done.placement, done.backdrop, done.histogram, done.wants_full, done.have_full);

    if !done.drafting && done.tile.is_none() && matches!(done.path, "proxy" | "ahead") {
        *state.render.on_screen.borrow_mut() = kept.map(|frame| (done.generation, done.document, frame));
    }
    if done.started.is_some() {
        log_painted(state, format!("{}, back on main {back:.1}, present {:.1}", done.laps, ms(presenting.elapsed())), presenting);
    }

    if !done.drafting {
        state.render.coming.take();
    }
    first_frames(state, done.drafting);
    prefetch::on_screen(state, done.generation);
}

fn log_painted(state: &App, laps: String, presenting: std::time::Instant) {
    let Some(clock) = state.canvas.frame_clock() else { return };
    let id = Rc::new(Cell::new(None::<glib::SignalHandlerId>));
    let handle = id.clone();
    id.set(Some(clock.connect_after_paint(move |clock| {
        log::info!("frame: {laps}, painted {:.1} ms after present", ms(presenting.elapsed()));
        if let Some(id) = handle.take() {
            clock.disconnect(id);
        }
    })));
}

fn first_frames(state: &App, drafting: bool) {
    let Some((asked, said, at)) = state.render.opened_at.get() else { return };
    let ms = asked.elapsed().as_secs_f32() * 1000.0;

    let cpu = raw::cpu_ms() - at;
    if !said {
        log::info!("first frame {ms:.0} ms after the photograph was asked for, cpu {cpu:.0} ms");
    }
    if drafting {
        state.render.opened_at.set(Some((asked, true, at)));
        return;
    }
    log::info!("sharp frame {ms:.0} ms after the photograph was asked for, cpu {cpu:.0} ms");
    state.render.opened_at.set(None);
}

fn tone_guide(
    frames: &mut Frames,
    made: &mut Made,
    document: &Document,
    key: &ColourKey,
    drafting: bool,
) -> Option<Arc<render::local::ToneGuide>> {
    let print = fingerprint(document, key);
    if let Some((was, from_full, kept)) = &frames.tone_guide {
        if *was == print && (*from_full || drafting) {
            return Some(kept.clone());
        }
    }
    let (source, from_full) = match (drafting, frames.full.as_deref()) {
        (false, Some(full)) => (full, true),
        _ => (frames.draft.as_deref().unwrap_or(&*frames.working), false),
    };
    let guide = Arc::new(render::measure_tone(document, source)?);
    frames.tone_guide = Some((print, from_full, guide.clone()));
    made.tone_guide = frames.tone_guide.clone();
    Some(guide)
}

fn recolour_ask(state: &App, photo: &OpenPhoto, document: &Document, key: &ColourKey, view: (bool, [f32; 4], (u32, u32), f64)) -> Option<Recolour> {
    let (soft, visible, frame, zoom) = view;
    let native = photo.full_native.clone().filter(|_| soft)?;

    photo.full_working_key.as_ref().filter(|(held, _)| held != key)?;
    if document.ai_denoise > 0.0 || document.ai_sharpen > 0.0 {
        return None;
    }
    let region = region_to_render(document, Some(visible), frame.0, frame.1);
    if region == [0.0, 0.0, 1.0, 1.0] {
        return None;
    }
    let kept = state.render.camera_view.borrow().as_ref().filter(|(opened, _)| *opened == state.open_generation.get()).map(|(_, view)| view.clone());
    Some(Recolour { native, inputs: photo.inputs.clone(), full_size: photo.full_size, region, needed: edge_for(region, frame, zoom, true), kept })
}

fn recoloured(ask: Recolour, document: &Document, key: &ColourKey, geometry: Geometry, made: &mut Made) -> Option<ViewTile> {
    let camera = match ask.kept.filter(|view| view.rect == ask.region && view.geometry == geometry && view.edge == ask.needed) {
        Some(view) => view,
        None => {
            let part = render::tile_box(document, ask.full_size.0, ask.full_size.1, ask.region)?;
            let frame = ask.native.camera(part, ask.full_size);
            let view = cut_view_tile(&frame, document, key, geometry, ask.region, ask.needed)?;
            made.camera = Some(view.clone());
            view
        }
    };
    let image = Arc::new(render::to_working_space(document, &*camera.image, &ask.inputs));
    Some(ViewTile { image, key: key.clone(), ..camera })
}

fn cut_view_tile(
    full: &LinearImage,
    document: &Document,
    key: &ColourKey,
    geometry: Geometry,
    region: [f32; 4],
    needed: u32,
) -> Option<ViewTile> {

    let cut = match render::tile_in_source(document, region) {
        Some(within) => Some(full.cropped(within, 0.0, Default::default())),
        None => render::cut_turned_tile(document, full, region),
    }?;
    let image = cut.downscaled(needed).unwrap_or(cut);
    Some(ViewTile { rect: region, key: key.clone(), geometry, edge: needed, image: Arc::new(image) })
}

fn render_view_tile(
    document: &Document,
    view: &ViewTile,
    frame_width: u32,
    frame_height: u32,
    guide: Option<&render::local::ToneGuide>,
) -> (image::RgbImage, Option<crate::ui::pixel_paintable::Placement>, bool) {
    let covered = (view.rect[2] * frame_width as f32).max(1.0);
    let detail_scale = view.image.width as f32 / covered;

    let rendered = render::apply_pixels_kept(document, &view.image, detail_scale, view.rect, guide);
    let placement = crate::ui::pixel_paintable::Placement {
        frame: (frame_width as f64, frame_height as f64),
        tile: (
            (view.rect[0] * frame_width as f32) as f64,
            (view.rect[1] * frame_height as f32) as f64,
            (view.rect[2] * frame_width as f32) as f64,
            (view.rect[3] * frame_height as f32) as f64,
        ),
    };
    let whole = view.rect == [0.0, 0.0, 1.0, 1.0];
    (rendered, Some(placement), whole)
}

fn render_proxy(frames: &mut Frames, made: &mut Made, document: &Document, proxy_scale: f32, drafting: bool) -> image::RgbImage {
    if drafting {
        draft_of(frames, made);
    }

    match frames.draft.as_ref().filter(|_| drafting) {
        Some(small) => {
            let scale = proxy_scale * small.width as f32 / frames.working.width.max(1) as f32;
            render::apply_stack_kept(document, small, scale)
        }
        None => render::apply_stack_kept(document, &frames.working, proxy_scale),
    }
}

fn draft_of(frames: &mut Frames, made: &mut Made) {
    if frames.draft.is_none() {
        let half = frames.working.width.max(frames.working.height) / 2;
        frames.draft = frames.working.downscaled(half).map(Arc::new);
        made.draft = Some(frames.draft.clone());
    }
}

#[allow(clippy::too_many_arguments)]
fn whole_frame_behind(
    frames: &mut Frames,
    made: &mut Made,
    document: &Document,
    key: &ColourKey,
    whole_frame: bool,
    proxy_scale: f32,
    drafting: bool,
    rendered: &image::RgbImage,
) -> (Option<image::RgbImage>, render::histogram::Histogram) {

    if drafting && !whole_frame {
        draft_of(frames, made);
    }
    let backdrop = (!whole_frame).then(|| match frames.draft.as_ref().filter(|_| drafting) {
        Some(small) => {
            let scale = proxy_scale * small.width as f32 / frames.working.width.max(1) as f32;
            render::apply_stack_kept(document, small, scale)
        }

        None => {
            let print = fingerprint(document, key);
            match frames.behind.as_ref().filter(|(was, _)| *was == print) {
                Some((_, kept)) => (**kept).clone(),
                None => {
                    let frame = render::apply_stack_kept(document, &frames.working, proxy_scale);
                    frames.behind = Some((print, Arc::new(frame.clone())));
                    made.behind = frames.behind.clone();
                    frame
                }
            }
        }
    });
    let histogram = match &backdrop {
        Some(frame) => render::histogram::of(frame),
        None => render::histogram::of(rendered),
    };
    (backdrop, histogram)
}
