use super::*;

pub(super) fn merge_selection(state: &App, button: &gtk::Button) {
    let paths: Vec<PathBuf> = {
        let cards = state.grid.cards.borrow();
        selected_cards(state)
            .iter()
            .filter_map(|child| child.widget_name().parse::<i64>().ok())
            .filter_map(|id| cards.get(&id).map(|(photo, _)| photo.path.clone()))
            .collect()
    };

    if paths.len() < 2 {
        state.toast("Select at least two exposures");
        return;
    }

    button.set_sensitive(false);
    state.toast(&format!("Merging {} exposures…", paths.len()));

    let state = state.clone();
    let button = button.clone();
    let sources = paths.clone();
    let edge = proxy_edge(&state);
    glib::spawn_future_local(async move {
        let merged = busy(&state, "Merging the bracket…", move || {
            let mut frames = Vec::with_capacity(paths.len());
            for path in &paths {
                let (image, exposure) = raw::decode_with_exposure(path)?;
                let exposure = exposure.ok_or_else(|| {
                    format!("{} has no exposure metadata", path.display())
                })?;
                frames.push(bracket::Frame { image, exposure });
            }
            let full = bracket::merge(&frames)?;
            Ok::<_, String>(full.downscaled(edge).unwrap_or(full))
        })
        .await;

        button.set_sensitive(true);

        let proxy = match merged {
            Ok(Ok(proxy)) => proxy,
            Ok(Err(err)) => {
                state.toast(&format!("Merge failed: {err}"));
                return;
            }
            Err(_) => {
                state.toast("Merge was cancelled");
                return;
            }
        };

        open_merged(&state, proxy, sources);
    });
}

pub(super) fn open_merged(state: &App, proxy: LinearImage, paths: Vec<PathBuf>) {

    begin_open(state);
    state.canvas.set_paintable(gtk::gdk::Paintable::NONE);
    state.editor_page.before.set_active(false);

    state.overlays.face_names.borrow_mut().clear();
    leave_crop(state);
    state.render.loading_full.set(false);
    state.crop.rect.set([0.0, 0.0, 1.0, 1.0]);

    state.applying.set(true);
    state.crop.straighten.set_value(0.0);
    state.applying.set(false);

    let as_shot = proxy
        .profile
        .map(|profile| profile.as_shot_white_balance())
        .unwrap_or(WhiteBalance { temperature: 5500.0, tint: 0.0 });

    let mut document = Document::new("merged".to_string());

    document.set_basic(Basic::with(|b| b.presence.hdr = 50.0));
    let basic = document.basic();

    state.sliders.balance.temperature.set_sensitive(proxy.profile.is_some());
    state.sliders.balance.tint.set_sensitive(proxy.profile.is_some());

    let working_key = colour_key(&document);
    let inputs = render_inputs(&document);
    let working = render::to_working_space(&document, &proxy, &inputs);
    let full_size = (proxy.width, proxy.height);

    *state.open.borrow_mut() = Some(OpenPhoto {
        source: Source::Bracket { paths },
        inputs,
        summary: None,

        lens_corrected: false,
        segmentation: None,
        mask_frame: None,
        embedding: None,
        embedding_pending: false,
        faces_pending: false,
        people: Vec::new(),
        segmenting: false,
        animal: None,
        draft: None,
        full_size,
        proxy,
        working,
        working_key,
        full_working: None,
        full_working_key: None,
        view: None,
        behind: None,
        baseline: None,
        before_preset: None,
        history: History::new(EditState::of(&document)),
        document,
        as_shot,
        edits_unreadable: false,
    });

    state.zooming.level.set(FIT_ZOOM);
    startup::show_editor(state);

    state.applying.set(true);
    state.sliders.write(basic);
    state.mask_overlay.sliders_hold.set(None);
    state.sliders.write_white_balance(as_shot);
    refresh_slider_marks(state);
    state.applying.set(false);
    state.light.curve_area.queue_draw();

    sync_document(state);

    refresh_profile_picker(state);
    write_lens(state);
    refresh_crumbs(state);
    request_render(state);
}
