use super::*;
use numa::io::capture::{self, Capture};
use numa::io::track::{self, Placing, Track};
use rayon::prelude::*;

fn said(placing: &Placing, total: usize) -> String {
    let count = |n: usize| places::grouped(n as i64);
    let mut said = format!("{} of {} placed", count(placing.placed.len()), count(total));
    if placing.outside > 0 {
        said.push_str(&format!(" \u{b7} {} outside the track", count(placing.outside)));
    }
    if placing.by_camera > 0 {
        said.push_str(&format!(" \u{b7} {} placed by the camera", count(placing.by_camera)));
    }
    said
}

async fn choose_track(window: Option<gtk::Window>) -> Option<PathBuf> {
    let filter = gtk::FileFilter::new();
    filter.set_name(Some("GPX or Timeline export"));
    for pattern in ["*.gpx", "*.GPX", "*.json", "*.JSON"] {
        filter.add_pattern(pattern);
    }
    let filters = gio::ListStore::new::<gtk::FileFilter>();
    filters.append(&filter);
    let chooser = gtk::FileDialog::new();
    chooser.set_title("Places from a track");
    chooser.set_filters(Some(&filters));
    chooser.set_default_filter(Some(&filter));
    chooser.open_future(window.as_ref()).await.ok()?.path()
}

fn read_and_place(path: &Path, photographs: &[PathBuf]) -> Result<(Track, Placing), String> {
    let track = track::read(path)?;
    let captures: Vec<Capture> = photographs.par_iter().map(|path| capture::read(path)).collect();
    let placing = track::place_all(&track, &captures, import_offset());
    Ok((track, placing))
}

pub(super) fn import_group(frames: Vec<PathBuf>, placed: Rc<RefCell<HashMap<PathBuf, (f64, f64)>>>) -> adw::PreferencesGroup {
    let group = adw::PreferencesGroup::new();
    group.set_title("Places");
    let row = adw::ActionRow::new();
    row.set_title("From a Track…");
    row.set_subtitle("GPX, or your Google Timeline export \u{b7} matched by time");
    let choose = gtk::Button::with_label("Choose…");
    choose.set_valign(gtk::Align::Center);
    choose.add_css_class("flat");
    row.add_suffix(&choose);
    row.set_activatable_widget(Some(&choose));
    group.add(&row);

    let frames = Arc::new(frames);
    let result: Rc<RefCell<Option<adw::ActionRow>>> = Rc::default();
    choose.connect_clicked(glib::clone!(
        #[weak] group,
        move |button| {
            let (frames, placed, result, window) = (frames.clone(), placed.clone(), result.clone(), button.root().and_downcast::<gtk::Window>());
            glib::spawn_future_local(async move {
                let Some(path) = choose_track(window).await else { return };

                group.set_header_suffix(Some(&spinner("Reading the track")));
                let (from, total) = (path.clone(), frames.len());
                let read = gio::spawn_blocking(move || read_and_place(&from, &frames).map(|done| (done, frames))).await;
                group.set_header_suffix(gtk::Widget::NONE);
                let shown = adw::ActionRow::new();
                match read {
                    Ok(Ok(((track, placing), frames))) => {
                        let mut places = placed.borrow_mut();
                        places.clear();
                        places.extend(placing.placed.iter().map(|&(at, latitude, longitude)| (frames[at].clone(), (latitude, longitude))));
                        let (first, last) = (track.fixes[0].at, track.fixes[track.fixes.len() - 1].at);
                        set_row_title(&shown, &format!("{}, {}", track.name, days(first, last)));
                        set_row_subtitle(&shown, &said(&placing, total));
                        let mark = gtk::Image::from_icon_name(if placing.placed.is_empty() { "dialog-warning-symbolic" } else { "object-select-symbolic" });
                        if placing.placed.is_empty() {
                            mark.add_css_class("dim-label");
                        }
                        shown.add_suffix(&mark);
                        log::info!("track: {} fixes, {}", track.fixes.len(), said(&placing, total));
                    }
                    Ok(Err(err)) => {
                        placed.borrow_mut().clear();
                        set_row_title(&shown, &path.file_name().unwrap_or_default().to_string_lossy());
                        set_row_subtitle(&shown, &format!("Not used: {err}"));
                    }
                    Err(_) => {
                        placed.borrow_mut().clear();
                        set_row_title(&shown, "The track could not be read");
                    }
                }
                if let Some(earlier) = result.borrow_mut().replace(shown.clone()) {
                    group.remove(&earlier);
                }
                group.add(&shown);
            });
        }
    ));
    group
}

pub(super) fn install_track_action(state: &App, window: &adw::ApplicationWindow) {
    let action = gio::SimpleAction::new("places-from-track", None);
    action.connect_activate(glib::clone!(
        #[strong] state,
        #[weak] window,
        move |_, _| add_places(&state, window.upcast_ref())
    ));
    window.add_action(&action);
}

fn add_places(state: &App, window: &gtk::Window) {
    let selected = selected_ids(state);
    let photographs: Vec<(i64, PathBuf)> = {
        let cards = state.grid.cards.borrow();
        let ids: Vec<i64> = match selected.is_empty() {
            true => state.grid.lazy.borrow().iter().map(|card| card.id).collect(),
            false => selected,
        };
        ids.into_iter().filter_map(|id| cards.get(&id).map(|photo| (id, photo.path.clone()))).collect()
    };
    if photographs.is_empty() {
        state.toast("No photographs here to place");
        return;
    }
    if refused_offline(state, photographs.iter().map(|(id, _)| *id), "placing") {
        return;
    }
    let ids: Vec<i64> = photographs.iter().map(|(id, _)| *id).collect();
    let had = state.catalog.positions(&ids).unwrap_or_default();
    let photographs: Vec<(i64, PathBuf)> = photographs.into_iter().filter(|(id, _)| !had.contains_key(id)).collect();
    if photographs.is_empty() {
        state.toast("Every photograph here has a place already");
        return;
    }
    let (state, window) = (state.clone(), window.clone());
    glib::spawn_future_local(async move {
        let Some(path) = choose_track(Some(window)).await else { return };
        let paths: Vec<PathBuf> = photographs.iter().map(|(_, path)| path.clone()).collect();

        let read = busy(&state, "Placing from the track", move || read_and_place(&path, &paths)).await;
        let (track, placing) = match read {
            Ok(Ok(done)) => done,
            Ok(Err(err)) => return state.toast(&format!("Could not use the track: {err}")),
            Err(_) => return state.toast("Reading the track stopped with an error"),
        };
        let placed: Vec<(i64, f64, f64)> = placing.placed.iter().map(|&(at, latitude, longitude)| (photographs[at].0, latitude, longitude)).collect();
        if let Err(err) = state.catalog.set_positions(&placed) {
            return state.toast(&format!("The places were not kept: {err}"));
        }
        log::info!("track {}: {} fixes, {}", track.name, track.fixes.len(), said(&placing, photographs.len()));
        let toast = adw::Toast::new(&glib::markup_escape_text(&said(&placing, photographs.len())));
        if !placed.is_empty() {
            toast.set_button_label(Some("Undo"));
            let ids: Vec<i64> = placed.iter().map(|(id, _, _)| *id).collect();
            toast.connect_button_clicked(glib::clone!(
                #[strong] state,
                move |_| {
                    if let Err(err) = state.catalog.clear_positions(&ids) {
                        state.toast(&format!("The places could not be taken back: {err}"));
                    }
                }
            ));
        }
        state.toasts.add_toast(toast);
    });
}
