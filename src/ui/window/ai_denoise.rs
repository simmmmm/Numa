use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use numa::io::denoised;
use numa::render::ai_denoise;

use super::*;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Pass {
    Denoise,
    Sharpen,
}

impl Pass {
    const BOTH: [Pass; 2] = [Pass::Denoise, Pass::Sharpen];

    fn index(self) -> usize {
        self as usize
    }

    fn name(self) -> &'static str {
        match self {
            Pass::Denoise => "AI denoise",
            Pass::Sharpen => "AI sharpen",
        }
    }

    fn about(self) -> &'static str {
        match self {
            Pass::Denoise => "A neural network (SCUNet) over the whole photograph at full size. \
                              The first time takes minutes; after that it is kept.",
            Pass::Sharpen => "Undoes the smear of a hand that moved (Restormer), over the whole \
                              photograph at full size. Not for a lens that missed focus. \
                              The first time takes minutes; after that it is kept.",
        }
    }

    fn amount(self, document: &Document) -> f32 {
        match self {
            Pass::Denoise => document.ai_denoise,
            Pass::Sharpen => document.ai_sharpen,
        }
    }

    fn set(self, document: &mut Document, value: f32) {
        match self {
            Pass::Denoise => document.ai_denoise = value,
            Pass::Sharpen => document.ai_sharpen = value,
        }
    }

    fn installed(self) -> bool {
        match self {
            Pass::Denoise => ai_denoise::is_installed(),
            Pass::Sharpen => ai_denoise::sharpen_installed(),
        }
    }

    fn missing(self) -> &'static str {
        match self {
            Pass::Denoise => "AI denoise needs the SCUNet model — download it in Preferences",
            Pass::Sharpen => "AI sharpen needs the Restormer model — download it in Preferences",
        }
    }

    fn kept(self, path: &Path, document: &Document) -> bool {
        match self {
            Pass::Denoise => denoised::is_cached(path),
            Pass::Sharpen => denoised::is_sharpened(path, document.ai_denoise > 0.0),
        }
    }
}

thread_local! {

    static CONTROLS: RefCell<[Option<(gtk::Switch, gtk::Scale)>; 2]> = const { RefCell::new([None, None]) };

    static RUNNING: Cell<bool> = const { Cell::new(false) };
}

fn controls(pass: Pass) -> Option<(gtk::Switch, gtk::Scale)> {
    CONTROLS.with_borrow(|controls| controls[pass.index()].clone())
}

pub(super) fn build(state: &App, pass: Pass) -> gtk::Box {
    let column = gtk::Box::new(gtk::Orientation::Vertical, 0);
    {
        let header = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        header.set_margin_top(6);
        header.set_margin_bottom(6);
        let title = gtk::Label::new(Some(pass.name()));
        title.set_xalign(0.0);
        title.set_hexpand(true);
        title.add_css_class("slider-name");
        let switch = gtk::Switch::new();
        switch.set_valign(gtk::Align::Center);
        switch.update_property(&[gtk::accessible::Property::Label(pass.name())]);
        header.set_tooltip_text(Some(pass.about()));
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
            move |switch| switched(&state, pass, switch.is_active())
        ));
        amount.connect_value_changed(glib::clone!(
            #[strong] state,
            move |_| commit(&state, pass)
        ));
        CONTROLS.with_borrow_mut(|controls| controls[pass.index()] = Some((switch, amount)));
    }
    column
}

pub(super) fn write(state: &App) {
    for pass in Pass::BOTH {
        let Some((switch, amount)) = controls(pass) else { continue };
        let Some(value) = state.open.borrow().as_ref().map(|photo| pass.amount(&photo.document)) else { return };
        let was = state.applying.replace(true);
        switch.set_active(value > 0.0);
        if value > 0.0 {
            amount.set_value(value as f64);
        }
        set_amount_sensitive(&amount, value > 0.0);
        state.applying.set(was);
    }
    start_what_is_owed(state);
}

fn start_what_is_owed(state: &App) {
    let owed = {
        let open = state.open.borrow();
        let Some(photo) = open.as_ref() else { return };
        let Some(path) = photo_path(photo) else { return };
        [Pass::Sharpen, Pass::Denoise].into_iter().find(|pass| {
            pass.amount(&photo.document) > 0.0 && pass.installed() && !pass.kept(&path, &photo.document)
        })
        .map(|pass| (pass, path, photo.document.ai_denoise > 0.0))
    };
    if let Some((pass, path, on_denoised)) = owed {
        run(state, path, pass, on_denoised);
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

fn set_switch(state: &App, pass: Pass, on: bool) {
    let Some((switch, amount)) = controls(pass) else { return };
    let was = state.applying.replace(true);
    switch.set_active(on);
    state.applying.set(was);
    commit(state, pass);
    set_amount_sensitive(&amount, on);
}

fn commit(state: &App, pass: Pass) {
    if state.applying.get() {
        return;
    }
    let Some((switch, amount)) = controls(pass) else { return };
    let (on, value) = (switch.is_active(), amount.value() as f32);
    {
        let mut open = state.open.borrow_mut();
        let Some(photo) = open.as_mut() else { return };
        let wanted = if on { value } else { 0.0 };
        if pass.amount(&photo.document) == wanted {
            return;
        }
        pass.set(&mut photo.document, wanted);
    }
    request_render(state);
    schedule_history_push(state);
}

fn switched(state: &App, pass: Pass, on: bool) {
    if state.applying.get() {
        return;
    }
    if let Some((_, amount)) = controls(pass) {
        set_amount_sensitive(&amount, on);
    }
    if !on {
        commit(state, pass);

        start_what_is_owed(state);
        return;
    }

    let has_file = state.open.borrow().as_ref().and_then(photo_path).is_some();
    if !has_file {
        state.toast(&format!("{} works on one photograph, not on a merge", pass.name()));
        set_switch(state, pass, false);
        return;
    }
    if !pass.installed() {
        state.toast(pass.missing());
        set_switch(state, pass, false);
        return;
    }
    commit(state, pass);
    start_what_is_owed(state);
}

fn run(state: &App, path: PathBuf, pass: Pass, on_denoised: bool) {

    if RUNNING.replace(true) {
        state.toast(&format!("{} starts when the pass under way is done", pass.name()));
        return;
    }

    let cancel = Cancel::default();
    let (toast, text, bar) = progress_toast(state, &cancel);
    text.set_text(&format!("{} — reading the photograph", pass.name()));

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
                text.set_text(&format!("{} — about {} min left", pass.name(), (left / 60.0).ceil().max(1.0)));
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
            let progress = |finished, tiles| {
                done.store(finished, Ordering::Relaxed);
                total.store(tiles, Ordering::Relaxed);
                !stop.load(Ordering::Relaxed)
            };
            let kept = match pass {
                Pass::Denoise => denoised::ensure(&work, &full, progress)?,
                Pass::Sharpen => denoised::ensure_sharpened(&work, &full, on_denoised, progress)?,
            };
            drop(full);
            if let (true, Some((width, height))) = (kept, proxy) {
                let stored = match pass {
                    Pass::Denoise => denoised::load(&work),
                    Pass::Sharpen => denoised::load_sharpened(&work, on_denoised),
                };
                if let Some(stored) = stored {
                    ai_denoise::warm(&work, &stored, width, height);
                    return Ok((kept, Some(stored)));
                }
            }
            Ok::<_, String>((kept, None))
        })
        .await;

        let (result, _held) = match result {
            Ok(Ok((kept, held))) => (Ok(Ok(kept)), held),
            Ok(Err(err)) => (Ok(Err(err)), None),
            Err(err) => (Err(err), None),
        };
        ticker.remove();
        toast.dismiss();
        RUNNING.set(false);

        let still_open = state.open.borrow().as_ref().and_then(photo_path).is_some_and(|open| open == path);
        match result {
            Ok(Ok(true)) if still_open => {
                if let Some(photo) = state.open.borrow_mut().as_mut() {

                    photo.inputs = render_inputs(&photo.document);
                    photo.working = Arc::new(render::to_working_space(&photo.document, &*photo.proxy, &photo.inputs));
                    photo.full_working = None;
                    photo.full_working_key = None;
                    photo.draft = None;
                    photo.view = None;
                }
                request_render(&state);

                start_what_is_owed(&state);
            }
            Ok(Ok(true)) => {}
            Ok(Ok(false)) if still_open => set_switch(&state, pass, false),
            Ok(Ok(false)) => {}
            Ok(Err(err)) => {
                log::warn!("{}: {}: {err}", path.display(), pass.name());
                state.toast(&format!("{} failed: {err}", pass.name()));
                if still_open {
                    set_switch(&state, pass, false);
                }
            }
            Err(_) => state.toast(&format!("{} failed", pass.name())),
        }
    });
}
