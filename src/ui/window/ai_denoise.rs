use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use numa::io::denoised;
use numa::render::ai_denoise;

use super::*;

thread_local! {

    static CONTROLS: RefCell<Option<(gtk::Switch, gtk::Scale)>> = const { RefCell::new(None) };

    static RUNNING: Cell<bool> = const { Cell::new(false) };
}

pub(super) fn build(state: &App) -> gtk::Box {
    let column = gtk::Box::new(gtk::Orientation::Vertical, 0);

    let header = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    header.set_margin_top(6);
    header.set_margin_bottom(6);
    let title = gtk::Label::new(Some("AI denoise"));
    title.set_xalign(0.0);
    title.set_hexpand(true);
    title.add_css_class("slider-name");
    let switch = gtk::Switch::new();
    switch.set_valign(gtk::Align::Center);
    switch.update_property(&[gtk::accessible::Property::Label("AI denoise")]);
    header.set_tooltip_text(Some(
        "A neural network (SCUNet) over the whole photograph at full size. \
         The first time takes minutes; after that it is kept.",
    ));
    header.append(&title);
    header.append(&switch);
    column.append(&header);

    let amount = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 100.0, 1.0);
    amount.set_value(100.0);
    set_neutral(&amount, 100.0);
    let row = slider_row(state, "Amount", &amount, Readout::Positive(0));
    row.set_sensitive(false);
    column.append(&row);

    switch.connect_active_notify(glib::clone!(
        #[strong] state,
        move |switch| switched(&state, switch.is_active())
    ));
    amount.connect_value_changed(glib::clone!(
        #[strong] state,
        move |_| commit(&state)
    ));

    CONTROLS.set(Some((switch, amount)));
    column
}

pub(super) fn write(state: &App) {
    let Some((switch, amount)) = CONTROLS.with_borrow(|controls| controls.clone()) else { return };
    let (value, path) = match state.open.borrow().as_ref() {
        Some(photo) => (photo.document.ai_denoise, photo_path(photo)),
        None => return,
    };

    let was = state.applying.replace(true);
    switch.set_active(value > 0.0);
    if value > 0.0 {
        amount.set_value(value as f64);
    }
    set_amount_sensitive(&amount, value > 0.0);
    state.applying.set(was);

    if let Some(path) = path.filter(|path| value > 0.0 && ai_denoise::is_installed() && !denoised::is_cached(path)) {
        run(state, path);
    }
}

fn photo_path(photo: &OpenPhoto) -> Option<PathBuf> {
    match &photo.source {
        Source::Photo { path, .. } => Some(path.clone()),
        Source::Bracket { .. } => None,
    }
}

fn set_amount_sensitive(amount: &gtk::Scale, on: bool) {
    if let Some(row) = amount.parent() {
        row.set_sensitive(on);
    }
}

fn set_switch(state: &App, on: bool) {
    let Some((switch, _)) = CONTROLS.with_borrow(|controls| controls.clone()) else { return };
    let was = state.applying.replace(true);
    switch.set_active(on);
    state.applying.set(was);
    commit(state);
    CONTROLS.with_borrow(|controls| controls.as_ref().map(|(_, amount)| set_amount_sensitive(amount, on)));
}

fn commit(state: &App) {
    if state.applying.get() {
        return;
    }
    let Some((on, value)) = CONTROLS.with_borrow(|controls| {
        controls.as_ref().map(|(switch, amount)| (switch.is_active(), amount.value() as f32))
    }) else {
        return;
    };
    {
        let mut open = state.open.borrow_mut();
        let Some(photo) = open.as_mut() else { return };
        let wanted = if on { value } else { 0.0 };
        if photo.document.ai_denoise == wanted {
            return;
        }
        photo.document.ai_denoise = wanted;
    }
    request_render(state);
    schedule_history_push(state);
}

fn switched(state: &App, on: bool) {
    if state.applying.get() {
        return;
    }
    CONTROLS.with_borrow(|controls| controls.as_ref().map(|(_, amount)| set_amount_sensitive(amount, on)));
    if !on {
        commit(state);
        return;
    }

    let path = match state.open.borrow().as_ref() {
        Some(photo) => photo_path(photo),
        None => return,
    };
    let Some(path) = path else {
        state.toast("AI denoise works on one photograph, not on a merge");
        set_switch(state, false);
        return;
    };
    if !ai_denoise::is_installed() {
        state.toast("AI denoise needs the SCUNet model — download it in Preferences");
        set_switch(state, false);
        return;
    }
    commit(state);
    if !denoised::is_cached(&path) {
        run(state, path);
    }
}

fn run(state: &App, path: PathBuf) {

    if RUNNING.replace(true) {
        return;
    }

    let cancel = Cancel::default();
    let (toast, text, bar) = progress_toast(state, &cancel);
    text.set_text("AI denoise — reading the photograph");

    let done = Arc::new(AtomicUsize::new(0));
    let total = Arc::new(AtomicUsize::new(0));
    let stop = Arc::new(AtomicBool::new(false));
    let started = std::time::Instant::now();
    let ticker = {
        let (done, total, stop) = (done.clone(), total.clone(), stop.clone());
        glib::timeout_add_local(std::time::Duration::from_millis(500), move || {
            if cancel.stopped() {
                stop.store(true, Ordering::Relaxed);
            }
            let (done, total) = (done.load(Ordering::Relaxed), total.load(Ordering::Relaxed));
            if total > 0 && done > 0 {
                let left = started.elapsed().as_secs_f64() / done as f64 * (total - done) as f64;
                text.set_text(&format!("AI denoise — about {} min left", (left / 60.0).ceil().max(1.0)));
                bar.set_fraction(done as f64 / total as f64);
            }
            glib::ControlFlow::Continue
        })
    };

    let proxy = state.open.borrow().as_ref().map(|photo| (photo.proxy.width, photo.proxy.height));
    let state = state.clone();
    glib::spawn_future_local(async move {
        let work = path.clone();
        let result = gio::spawn_blocking(move || {
            let full = Source::Photo { id: 0, path: work.clone() }.full_resolution()?;
            let kept = ai_denoise::ensure(&work, &full, |finished, tiles| {
                done.store(finished, Ordering::Relaxed);
                total.store(tiles, Ordering::Relaxed);
                !stop.load(Ordering::Relaxed)
            })?;
            drop(full);
            if let (true, Some((width, height))) = (kept, proxy) {
                ai_denoise::warm(&work, width, height);
            }
            Ok::<bool, String>(kept)
        })
        .await;
        ticker.remove();
        toast.dismiss();
        RUNNING.set(false);

        let still_open = state.open.borrow().as_ref().and_then(photo_path).is_some_and(|open| open == path);
        match result {
            Ok(Ok(true)) if still_open => {
                if let Some(photo) = state.open.borrow_mut().as_mut() {

                    photo.working = render::to_working_space(&photo.document, &photo.proxy);
                    photo.full_working = None;
                    photo.full_working_key = None;
                    photo.draft = None;
                    photo.view = None;
                }
                request_render(&state);
            }
            Ok(Ok(true)) => {}
            Ok(Ok(false)) if still_open => set_switch(&state, false),
            Ok(Ok(false)) => {}
            Ok(Err(err)) => {
                log::warn!("{}: AI denoise: {err}", path.display());
                state.toast(&format!("AI denoise failed: {err}"));
                if still_open {
                    set_switch(&state, false);
                }
            }
            Err(_) => state.toast("AI denoise failed"),
        }
    });
}
