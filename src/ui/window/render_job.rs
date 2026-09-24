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
    geometry: Geometry,
    proxy_scale: f32,
    started: Option<std::time::Instant>,
}

#[derive(Default)]
struct Made {

    working: Option<(Arc<LinearImage>, ColourKey)>,

    draft: Option<Option<Arc<LinearImage>>>,

    view: Option<Option<ViewTile>>,
    tone_guide: Option<(u64, bool, Arc<render::local::ToneGuide>)>,
    behind: Option<(u64, Arc<image::RgbImage>)>,
}

pub(super) struct Done {
    serial: u64,
    generation: u64,
    base_key: ColourKey,
    made: Made,
    drafting: bool,
    rendered: image::RgbImage,
    placement: Option<crate::ui::pixel_paintable::Placement>,
    backdrop: Option<image::RgbImage>,
    histogram: render::histogram::Histogram,
    tile: Option<[f32; 4]>,
    wants_full: bool,
    have_full: bool,
    path: &'static str,
    recut: bool,
    started: Option<std::time::Instant>,
}

pub(super) fn schedule_render(state: &App) {
    if state.render.render_pending.replace(true) {
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
    state.render.in_flight.set(true);
    let state = state.clone();
    glib::spawn_future_local(async move {
        let done = gio::spawn_blocking(move || work(job)).await;
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
        ColourWork { proxy: photo.proxy.clone(), inputs: photo.inputs.clone() }
    });
    let document = rendered_document(state, photo);

    let wants_full = zoom > proxy_runs_out_at(photo);
    let have_full = photo.full_working.is_some() && photo.full_working_key.as_ref() == Some(&key);
    let region = region_to_render(&document, wanted, frame.0, frame.1);
    let needed = edge_for(region, frame, zoom, drafting);

    let all_of_it = region[2] >= 0.999 && region[3] >= 0.999;
    let wants_full = wants_full && !(drafting && all_of_it);

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
            full: photo.full_working.clone(),
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
        proxy_scale,
        started,
    })
}

fn work(job: Job) -> Done {
    let Job { serial, generation, document, key, base_key, colour, mut frames, drafting, .. } = job;
    let mut made = Made::default();

    if let Some(colour) = colour {
        colour_stage(&document, &key, colour, drafting, &mut frames, &mut made);
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
    let usable_view = wants_full && have_full && frames.view.as_ref().is_some_and(fits);

    let path = match (usable_view, wants_full && have_full) {
        (true, _) => "tile",
        (_, true) => "full",
        _ => "proxy",
    };

    let guide = match usable_view && measures_tone(&document) {
        true => tone_guide(&mut frames, &mut made, &document, &key, drafting),
        false => None,
    };
    let usable_view = usable_view && (guide.is_some() || !measures_tone(&document));
    let (rendered, placement, whole_frame) = match (usable_view, frames.view.as_ref()) {
        (true, Some(view)) => render_view_tile(&document, view, job.frame.0, job.frame.1, guide.as_deref()),

        _ if wants_full && have_full && !drafting => {
            let full = frames.full.as_deref().expect("checked by have_full");
            (render::apply_stack(&document, full, 1.0), None, true)
        }

        _ => (render_proxy(&mut frames, &mut made, &document, job.proxy_scale, drafting), None, true),
    };
    let tile = (!whole_frame).then_some(frames.view.as_ref().map_or([0.0, 0.0, 1.0, 1.0], |view| view.rect));
    let (backdrop, histogram) =
        whole_frame_behind(&mut frames, &mut made, &document, &key, whole_frame, job.proxy_scale, drafting, &rendered);

    Done {
        serial,
        generation,
        base_key,
        made,
        drafting,
        rendered,
        placement,
        backdrop,
        histogram,
        tile,
        wants_full,
        have_full,
        path,
        recut,
        started: job.started,
    }
}

fn colour_stage(document: &Document, key: &ColourKey, colour: ColourWork, drafting: bool, frames: &mut Frames, made: &mut Made) {
    match drafting {
        true => {
            let half = colour.proxy.width.max(colour.proxy.height) / 2;
            let small = colour.proxy.downscaled(half);
            frames.draft = small.map(|small| Arc::new(render::to_working_space(document, small, &colour.inputs)));
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

    present(state, done.rendered, done.placement, done.backdrop, done.histogram, done.wants_full, done.have_full);
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

    let rendered = match guide {
        Some(guide) => render::apply_pixels_guided(document, &*view.image, detail_scale, view.rect, guide),
        None => render::apply_pixels(document, &*view.image, detail_scale, view.rect),
    };
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

    if drafting && frames.draft.is_none() {
        let half = frames.working.width.max(frames.working.height) / 2;
        frames.draft = frames.working.downscaled(half).map(Arc::new);
        made.draft = Some(frames.draft.clone());
    }
    match frames.draft.as_deref().filter(|_| drafting) {
        Some(small) => {
            let scale = proxy_scale * small.width as f32 / frames.working.width.max(1) as f32;
            render::apply_stack(document, small, scale)
        }
        None => render::apply_stack(document, &*frames.working, proxy_scale),
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
    let backdrop = (!whole_frame).then(|| match frames.draft.as_deref().filter(|_| drafting) {
        Some(small) => {
            let scale = proxy_scale * small.width as f32 / frames.working.width.max(1) as f32;
            render::apply_stack(document, small, scale)
        }

        None => {
            let print = fingerprint(document, key);
            match frames.behind.as_ref().filter(|(was, _)| *was == print) {
                Some((_, kept)) => (**kept).clone(),
                None => {
                    let frame = render::apply_stack(document, &*frames.working, proxy_scale);
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
