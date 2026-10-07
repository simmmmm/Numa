use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};

use numa::io::fit;

use super::*;

thread_local! {

    static RUNNING: Cell<bool> = const { Cell::new(false) };
}

pub(super) fn row(state: &App, dialog: &adw::PreferencesDialog) -> adw::ActionRow {
    let row = adw::ActionRow::new();
    row.set_title("Profile for Your Camera");
    row.set_subtitle("Made from your own neutral photographs");
    let button = gtk::Button::with_label("Make…");
    button.set_valign(gtk::Align::Center);
    button.connect_clicked(glib::clone!(
        #[strong] state,
        #[weak] dialog,
        move |button| choose(&state, &dialog, button)
    ));
    row.add_suffix(&button);
    row
}

fn raw_paths(state: &App) -> Vec<PathBuf> {
    let filter = Filter { file_type: FileType::Raw, ..Default::default() };
    state.catalog.photos_everywhere(&filter).unwrap_or_default().into_iter().map(|photo| photo.path).collect()
}

fn choose(state: &App, dialog: &adw::PreferencesDialog, button: &gtk::Button) {
    if RUNNING.get() {
        dialog.add_toast(adw::Toast::new("A profile is being made"));
        return;
    }
    let open = state.open.borrow().as_ref().and_then(|photo| photo.summary.clone()).map(|s| (fit::tidy(&s.make), fit::tidy(&s.model)));
    let paths = raw_paths(state);
    button.set_sensitive(false);
    let (state, dialog, button) = (state.clone(), dialog.clone(), button.clone());
    glib::spawn_future_local(async move {
        let mut cameras = gio::spawn_blocking(move || fit::cameras(&paths)).await.unwrap_or_default();
        button.set_sensitive(true);
        if cameras.is_empty() {
            dialog.add_toast(adw::Toast::new("No raw photographs in your libraries"));
            return;
        }
        if let Some(at) = cameras.iter().position(|camera| Some((&camera.make, &camera.model)) == open.as_ref().map(|(a, b)| (a, b))) {
            let camera = cameras.remove(at);
            cameras.insert(0, camera);
        }
        ask(&state, &dialog, cameras);
    });
}

fn needs(camera: &fit::Camera) -> String {

    let (setting, unread) = match numa::io::style::setting(&camera.make) {
        Some(setting) => (format!(" — {setting} —"), String::new()),
        None => (
            ", with no picture style or film look,".to_string(),
            "\n\nNuma cannot read which style this camera was set to, so bring only photographs taken that way.".to_string(),
        ),
    };
    format!(
        "Numa learns your camera's colours from the picture it made of each raw photograph, and uses them for every photograph from this camera.\n\n\
         It takes at least {} raw photographs from your {} in its standard colour{setting} taken on more than one day, with plenty of colour in them.{unread}\n\n\
         Numa looks for them in your libraries, and keeps the profile only if it comes out nearer your camera's colours than now.",
        fit::FEWEST,
        camera.model
    )
}

fn ask(state: &App, dialog: &adw::PreferencesDialog, cameras: Vec<fit::Camera>) {
    let cameras = Rc::new(cameras);
    let alert = adw::AlertDialog::new(Some("A Profile for Your Camera"), Some(&needs(&cameras[0])));
    let names: Vec<String> = cameras.iter().map(|camera| format!("{} {}", camera.make, camera.model)).collect();
    let picker = gtk::DropDown::from_strings(&names.iter().map(String::as_str).collect::<Vec<_>>());
    picker.connect_selected_notify(glib::clone!(
        #[weak] alert,
        #[strong] cameras,
        move |picker| {
            if let Some(camera) = cameras.get(picker.selected() as usize) {
                alert.set_body(&needs(camera));
            }
        }
    ));
    alert.set_extra_child(Some(&picker));
    alert.add_response("cancel", "Cancel");
    alert.add_response("make", "Make Profile");
    alert.set_response_appearance("make", adw::ResponseAppearance::Suggested);
    alert.set_default_response(Some("make"));
    alert.set_close_response("cancel");
    let (state, preferences) = (state.clone(), dialog.clone());
    alert.connect_response(None, move |_, response| {
        let Some(camera) = cameras.get(picker.selected() as usize).filter(|_| response == "make") else { return };

        preferences.close();
        look_and_fit(&state, camera.model.clone(), camera.like.clone());
    });
    alert.present(Some(dialog));
}

fn look_and_fit(state: &App, model: String, like: PathBuf) {
    if RUNNING.replace(true) {
        return;
    }
    let paths = raw_paths(state);
    let cancel = Cancel::default();
    let (toast, text, bar) = progress_toast(state, &cancel);
    text.set_text("Looking for neutral photographs");

    let learning = Arc::new(AtomicBool::new(false));
    let done = Arc::new(AtomicUsize::new(0));
    let total = Arc::new(AtomicUsize::new(0));
    let stop = Arc::new(AtomicBool::new(false));
    let ticker = {
        let (learning, done, total, stop) = (learning.clone(), done.clone(), total.clone(), stop.clone());
        glib::timeout_add_local(std::time::Duration::from_millis(250), move || {
            if cancel.stopped() {
                stop.store(true, Ordering::Relaxed);
            }
            if learning.load(Ordering::Relaxed) {
                text.set_text("Learning your camera's colours");
            }
            let (done, total) = (done.load(Ordering::Relaxed), total.load(Ordering::Relaxed));
            if total > 0 {
                bar.set_fraction(done as f64 / total as f64);
            }
            glib::ControlFlow::Continue
        })
    };

    let state = state.clone();
    glib::spawn_future_local(async move {
        let stopped = stop.clone();
        let result = gio::spawn_blocking(move || {
            let progress = |finished, of| {
                done.store(finished, Ordering::Relaxed);
                total.store(of, Ordering::Relaxed);
                !stop.load(Ordering::Relaxed)
            };
            let found = fit::find_frames(&paths, &like, &progress)?;
            if found.neutral < fit::FEWEST || found.days < 2 {
                return Ok(Err(found));
            }
            learning.store(true, Ordering::Relaxed);
            fit::own_profile(&found.picked, &progress).map(Ok)
        })
        .await;
        ticker.remove();
        toast.dismiss();
        RUNNING.set(false);
        match result {
            _ if stopped.load(Ordering::Relaxed) => {}
            Ok(Ok(Ok(made))) => kept(&state, made),
            Ok(Ok(Err(found))) => state.toast(&too_few(&found, &model)),
            Ok(Err(why)) => state.toast(&why),
            Err(_) => state.toast("Making the profile failed"),
        }
    });
}

fn too_few(found: &fit::Found, model: &str) -> String {
    let photographs = |n: usize| if n == 1 { "photograph" } else { "photographs" };
    let mut said = format!("{} neutral {} from your {model}", places::grouped(found.neutral as i64), photographs(found.neutral));
    if found.neutral < fit::FEWEST {
        said += &format!("; a profile takes {}", fit::FEWEST);
    } else {
        said += ", all from one day; a profile takes another day too";
    }
    if let (Some(style), true) = (&found.commonest, found.other > 0) {
        said += &format!(" · {} in another style, most {style}", places::grouped(found.other as i64));
    }
    said
}

fn kept(state: &App, made: fit::Made) {
    let Some(file) = made.file else {
        state.toast("No profile kept: it came out no nearer your camera's colours than now");
        return;
    };
    log::info!(
        "RENDER-025: {} from {} frames, {} held out: {:.4} before, {:.4} fitted",
        file.display(),
        made.fitted_on,
        made.held_out,
        made.before,
        made.fitted
    );
    let nearer = ((1.0 - made.fitted / made.before) * 100.0).round();
    state.toast(&format!("Profile made · {nearer} % nearer your camera's own colours"));

    prefetch::forget(state);
    let on_automatic = state.open.borrow().as_ref().is_some_and(|photo| photo.document.colour_profile.is_none());
    match open_id(state).filter(|_| on_automatic) {
        Some(id) => open_photo(state, id),
        None => refresh_profile_picker(state),
    }
}
