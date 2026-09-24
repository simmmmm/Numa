use super::*;

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

pub(super) const SETTLE: std::time::Duration = std::time::Duration::from_millis(130);

pub(super) fn watch_the_button(state: &App, window: &impl IsA<gtk::Widget>) {
    let events = gtk::EventControllerLegacy::new();
    events.set_propagation_phase(gtk::PropagationPhase::Capture);
    events.connect_event(glib::clone!(
        #[strong] state,
        move |_, event| {
            match event.event_type() {
                gtk::gdk::EventType::ButtonPress => state.render.hand_down.set(true),
                gtk::gdk::EventType::ButtonRelease => {
                    if state.render.hand_down.replace(false) {

                        settle_render(&state);
                    }
                }
                _ => {}
            }
            glib::Propagation::Proceed
        }
    ));
    window.as_ref().add_controller(events);
}

pub(super) fn settle_render(state: &App) {
    let generation = state.render.settle_generation.get().wrapping_add(1);
    state.render.settle_generation.set(generation);

    let state = state.clone();
    glib::timeout_add_local_once(SETTLE, move || {
        if state.render.settle_generation.get() != generation {
            return;
        }

        if state.render.hand_down.get() {
            return;
        }
        let drafted = state.render.drafting.replace(false);

        let reshaped = super::masks::settle_edges(&state);
        if !drafted && !reshaped {
            return;
        }
        schedule_render(&state);
    });
}

pub(super) fn thumb_edge(state: &App) -> u32 {
    let wanted = proxy_edge(state) / 2;
    numa::io::thumbs::STEPS
        .into_iter()
        .find(|&step| step >= wanted)
        .unwrap_or(numa::io::thumbs::STEPS[numa::io::thumbs::STEPS.len() - 1])
}
