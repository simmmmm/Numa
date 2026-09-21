use super::*;

pub(super) fn colour_stage(state: &App, photo: &mut OpenPhoto) -> ColourKey {
    let key = colour_key(&photo.document);
    if key != photo.working_key {

        photo.inputs = render_inputs(&photo.document);

        match state.render.drafting.get() {
            true => {
                let half = photo.proxy.width.max(photo.proxy.height) / 2;
                let small = photo.proxy.downscaled(half as u32);
                photo.draft = small
                    .as_ref()
                    .map(|small| render::to_working_space(&photo.document, small, &photo.inputs));
            }
            false => {
                photo.working =
                    render::to_working_space(&photo.document, &photo.proxy, &photo.inputs);
                photo.working_key = key.clone();

                photo.draft = None;
            }
        }
    }

    key
}

pub(super) fn whole_frame_behind(
    state: &App,
    photo: &mut OpenPhoto,
    document: &Document,
    whole_frame: bool,
    proxy_scale: f32,
    rendered: &image::RgbImage,
    key: &ColourKey,
) -> (Option<image::RgbImage>, render::histogram::Histogram) {

    let draft = state.render.drafting.get().then(|| photo.draft.as_ref()).flatten();
    let backdrop = (!whole_frame).then(|| match draft {
        Some(small) => {
            let scale = proxy_scale * small.width as f32 / photo.working.width.max(1) as f32;
            render::apply_stack(document, small, scale)
        }

        None => match photo.behind.as_ref().filter(|(was, _)| was == key) {
            Some((_, kept)) => kept.clone(),
            None => {
                let made = render::apply_stack(document, &photo.working, proxy_scale);
                photo.behind = Some((key.clone(), made.clone()));
                made
            }
        },
    });

    let histogram = match &backdrop {
        Some(frame) => render::histogram::of(frame),
        None => render::histogram::of(&rendered),
    };

    (backdrop, histogram)
}

pub(super) fn proxy_edge(state: &App) -> u32 {
    let scale = state.canvas.scale_factor().max(1) as u32;
    let widest = state
        .canvas
        .display()
        .monitors()
        .into_iter()
        .flatten()
        .filter_map(|object| object.downcast::<gtk::gdk::Monitor>().ok())
        .map(|monitor| {
            let at = monitor.geometry();
            at.width().max(at.height()) as u32
        })
        .max()
        .unwrap_or(PROXY_EDGE);
    (widest * scale).clamp(PROXY_FLOOR, PROXY_EDGE)
}

pub(super) fn pending_masks(photo: &OpenPhoto) -> bool {
    photo.document.masks().iter().any(Mask::is_pending)
}

pub(super) fn settle_render(state: &App) {
    let generation = state.render.settle_generation.get().wrapping_add(1);
    state.render.settle_generation.set(generation);

    let state = state.clone();

    glib::timeout_add_local_once(std::time::Duration::from_millis(130), move || {
        if state.render.settle_generation.get() != generation || !state.render.drafting.get() {
            return;
        }
        state.render.drafting.set(false);

        super::masks::settle_edges(&state);
        if state.render.render_pending.replace(true) {
            return;
        }
        let state = state.clone();
        state.canvas.clone().add_tick_callback(move |_, _| {
            state.render.render_pending.set(false);
            render_current(&state);
            glib::ControlFlow::Break
        });
    });
}

pub(super) fn thumb_edge(state: &App) -> u32 {
    let wanted = proxy_edge(state) / 2;
    numa::io::thumbs::STEPS
        .into_iter()
        .find(|&step| step >= wanted)
        .unwrap_or(numa::io::thumbs::STEPS[numa::io::thumbs::STEPS.len() - 1])
}
