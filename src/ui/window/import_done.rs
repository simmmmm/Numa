use super::*;
use numa::io::catalog::Scan;
use numa::io::import::{self, Place};
use numa::io::offload::{self, Item, Report, Stage, Target};
use numa::io::receipt;
use std::collections::HashMap;
use std::ffi::OsString;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

pub(super) const SECOND_COPY: &str = "import_second_copy";
pub(super) const SECOND_ON: &str = "import_second_on";

pub(super) const CHECK: &str = "import_check";

const RECHECK: &str = "recheck_copies";
const RECHECK_LAST: &str = "recheck_last";
const MONTH: i64 = 30 * 86_400;

const RECHECK_AFTER: u32 = 10 * 60;

#[derive(Clone)]
pub(super) struct Run {
    pub(super) card_root: PathBuf,
    pub(super) card_name: String,

    pub(super) older: Vec<(PathBuf, PathBuf, u64)>,
    pub(super) second: Option<PathBuf>,
    pub(super) check: bool,

    pub(super) marks: Rc<HashMap<PathBuf, (u8, Flag)>>,
    pub(super) places: Vec<Place>,

    pub(super) stand_ins: Rc<HashMap<PathBuf, PathBuf>>,

    pub(super) positions: Rc<HashMap<PathBuf, (f64, f64)>>,
}

#[derive(Debug, Clone, Copy, Default)]
pub(super) struct Kept {

    pub(super) rated: usize,

    pub(super) replaced: usize,
    pub(super) shapes: usize,
    pub(super) behind: usize,

    pub(super) placed: usize,
}

pub(super) fn plan(card: &Card, places: &[Place], pattern: &str, marks: &HashMap<PathBuf, (u8, Flag)>, second: Option<&Path>) -> Vec<Item> {
    let offset = import_offset();
    let mut items = Vec::new();
    for (shoot, place) in card.shoots.iter().zip(places) {
        let folder = match place {
            Place::Library(library) => library.path.clone(),
            Place::New(path) => path.clone(),
        };

        let mut stems: HashMap<OsString, usize> = HashMap::new();
        for &at in &shoot.photos {
            let photo = &card.found[at];
            if marks.get(&photo.path).is_some_and(|(_, flag)| *flag == Flag::Rejected) {
                continue;
            }
            let next = stems.len() + 1;
            let n = *stems.entry(photo.path.file_stem().unwrap_or_default().to_owned()).or_insert(next);
            let name = import::named(pattern, photo, n, offset);
            items.push(Item {
                card: photo.path.clone(),
                size: photo.size,
                second: second.map(|root| Target::Free(offload::second_folder(root, &folder), name.clone())),
                to: Target::Free(folder.clone(), name),
            });
        }
    }
    items
}

pub(super) fn run_checked(state: &App, items: Vec<Item>, run: Run) {
    let total = items.len();
    let second = items.iter().any(|item| item.second.is_some());
    let (progress, text, bar) = progress_toast(state, &Cancel::default());
    text.set_text(&format!("Importing {}", places::grouped(total as i64)));
    let stop = Arc::new(AtomicBool::new(false));
    progress.connect_button_clicked(glib::clone!(
        #[strong] stop,
        move |_| stop.store(true, Ordering::Relaxed)
    ));
    let counts: Arc<[AtomicUsize; 3]> = Arc::default();
    let (text, bar) = (glib::SendWeakRef::from(text.downgrade()), glib::SendWeakRef::from(bar.downgrade()));
    let step: offload::Step = Arc::new(move |stage: Stage| {
        counts[stage as usize].fetch_add(1, Ordering::Relaxed);
        let [copied, seconded, decoded] = [0, 1, 2].map(|at| counts[at].load(Ordering::Relaxed));
        let (text, bar) = (text.clone(), bar.clone());
        glib::MainContext::default().invoke(move || {
            if let (Some(text), Some(bar)) = (text.upgrade(), bar.upgrade()) {
                text.set_text(&progress_said(total, copied, second.then_some(seconded), decoded));
                let hands = if second { 3 } else { 2 };
                let done = copied + decoded + if second { seconded } else { 0 };
                bar.set_fraction(done as f64 / (hands * total.max(1)) as f64);
            }
        });
    });

    let mut roots: Vec<(PathBuf, numa::io::catalog::Known)> = Vec::new();
    for place in &run.places {
        let (root, known) = match place {
            Place::Library(library) => (library.path.clone(), state.catalog.known_files(library).unwrap_or_default()),
            Place::New(path) => (path.clone(), Default::default()),
        };
        if !roots.iter().any(|(already, _)| *already == root) {
            roots.push((root, known));
        }
    }

    let state = state.clone();
    let check = run.check;
    glib::spawn_future_local(async move {
        let started = std::time::Instant::now();
        let first = gio::spawn_blocking(move || {
            let (report, later) = offload::run(items, check, stop, step);

            let scans: Vec<(PathBuf, Scan)> = roots
                .into_iter()
                .filter(|(root, _)| root.is_dir())
                .map(|(root, known)| {
                    let found = numa::io::catalog::scan(&root, &known);
                    (root, found)
                })
                .collect();
            (report, later, scans)
        })
        .await;
        let Ok((report, later, scans)) = first else {
            progress.dismiss();
            state.toast("The import stopped with an error");
            return;
        };
        log::info!("import: library copied and checked in {:.1?}", started.elapsed());
        let kept = catalogue(&state, &run, &report, &scans);

        let second_root = run.second.clone();
        let (card_name, check) = (run.card_name.clone(), run.check);
        let when = glib::DateTime::now_local().ok();
        let stamp = when.as_ref().and_then(|now| now.format("%Y-%m-%d-%H%M").ok()).map_or_else(|| "import".into(), |text| text.to_string());
        let said = when.as_ref().and_then(|now| now.format("%-d %b %Y %H:%M").ok()).map(|text| text.to_string()).unwrap_or_default();
        let rest = gio::spawn_blocking(move || {
            let report = later.join(report);
            let header = receipt_header(&said, &card_name, check, &report, second_root.as_deref());
            let files: Vec<receipt::Entry> = report
                .landed
                .iter()
                .map(|landed| (landed.hash, landed.to.parent().unwrap_or(&landed.to).to_path_buf(), landed.to.clone(), landed.second.clone()))
                .collect();
            let receipts = receipt::write_all(&files, second_root.as_deref(), &stamp, &header);
            (report, receipts)
        })
        .await;
        progress.dismiss();
        let Ok((report, receipts)) = rest else {
            state.toast("The second copy or the check of the card stopped with an error");
            return;
        };
        log::info!(
            "import: {} photographs, every copy and decode done in {:.1?}, {} problems",
            report.landed.len(),
            started.elapsed(),
            report.problems.len()
        );

        tonight::after_import(&state, &report);
        import_verdict::done_dialog(&state, import_verdict::Done::new(run, report, receipts, kept));
    });
}

fn progress_said(total: usize, copied: usize, second: Option<usize>, decoded: usize) -> String {
    let of = |done: usize| format!("{} of {}", places::grouped(done as i64), places::grouped(total as i64));
    if copied < total {
        return format!("Importing {}", of(copied));
    }
    let mut still = Vec::new();
    if let Some(second) = second.filter(|second| *second < total) {
        still.push(format!("Second copy {}", of(second)));
    }
    if decoded < total {
        still.push(format!("Checking the card {}", of(decoded)));
    }
    match still.is_empty() {
        true => "Writing the receipt".to_string(),
        false => still.join(" \u{b7} "),
    }
}

fn receipt_header(when: &str, card: &str, check: bool, report: &Report, second: Option<&Path>) -> Vec<String> {
    let mut roots: Vec<String> = Vec::new();
    for landed in &report.landed {
        let root = landed.to.parent().unwrap_or(&landed.to).display().to_string();
        if !roots.contains(&root) {
            roots.push(root);
        }
    }
    let mut header = vec![
        format!("Numa import receipt \u{b7} {when}"),
        format!(
            "From {card} \u{b7} {} photographs \u{b7} {}",
            report.landed.len(),
            if check { "each copy read back and the same as the card" } else { "not read back" }
        ),
        format!("Library: {}", roots.join(", ")),
    ];
    if let Some(second) = second {
        header.push(format!("Second copy: {}", second.display()));
    }
    if !report.problems.is_empty() {
        header.push(format!("{} problems: see the import in Numa", report.problems.len()));
    }
    header
}

fn catalogue(state: &App, run: &Run, report: &Report, scans: &[(PathBuf, Scan)]) -> Kept {
    let mut opened = None;
    let mut applied: Vec<&Path> = Vec::new();
    let mut touched: Vec<Library> = Vec::new();
    for place in &run.places {
        let library = match place {
            Place::Library(library) => Ok(library.clone()),
            Place::New(path) if path.is_dir() => state.catalog.add_library(path),
            Place::New(_) => continue,
        };

        let apply = |library: Library| match scans.iter().find(|(root, _)| *root == library.path) {
            Some((root, found)) if !applied.contains(&root.as_path()) => {
                applied.push(root.as_path());
                state.catalog.apply_scan(&library, found).map(|_| library)
            }
            _ => Ok(library),
        };
        match library.and_then(apply) {
            Ok(library) => {
                opened.get_or_insert(library.id);
                if !touched.iter().any(|known| known.id == library.id) {
                    touched.push(library);
                }
            }
            Err(err) => state.toast(&format!("Copied, but {err}")),
        }
    }

    let mut kept = import_jpegs::take_places(state, &run.stand_ins, report, &touched);

    let mut placed: Vec<(i64, f64, f64)> = Vec::new();
    for library in &touched {
        let photos = state.catalog.photos(library.id, &Filter::default()).unwrap_or_default();
        let by_path: HashMap<&Path, &numa::io::catalog::Photo> = photos.iter().map(|photo| (photo.path.as_path(), photo)).collect();
        for landed in &report.landed {
            let Some(photo) = by_path.get(landed.to.as_path()) else { continue };
            let (stars, flag) = run.marks.get(&landed.card).copied().unwrap_or((0, Flag::None));
            if flag == Flag::Picked {
                let _ = state.catalog.set_flag(photo.id, Flag::Picked);
            }
            if stars > 0 {
                let _ = state.catalog.set_rating(photo.id, stars);
            } else if let Some(camera) = landed.rating.filter(|_| photo.rating == 0) {
                if state.catalog.set_rating(photo.id, camera).is_ok() {
                    kept.rated += 1;
                }
            }
            if let Some(&(latitude, longitude)) = run.positions.get(&landed.card) {
                placed.push((photo.id, latitude, longitude));
            }
        }
    }

    match state.catalog.set_positions(&placed) {
        Ok(()) => kept.placed = placed.len(),
        Err(err) => state.toast(&format!("Copied, but the places were not kept: {err}")),
    }
    state.libraries.filter.borrow_mut().in_one_library();
    reload_libraries(state);
    if let Some(id) = opened {
        select_library(state, id);
        state.stack.set_visible_child_name("library");

        if let Some(button) = state.libraries.analyse_button.borrow().clone().filter(|button| button.is_sensitive()) {
            analyse_library(state, &button);
        }
        words::read_imported(state, &touched.iter().map(|library| library.id).collect::<Vec<_>>());
    }
    kept
}

pub(super) fn name_of(path: &Path) -> String {
    path.file_name().unwrap_or_default().to_string_lossy().into_owned()
}

pub(super) fn place_label(path: &Path) -> String {
    let mount = gio::VolumeMonitor::get().mounts().into_iter().find(|mount| {
        mount.root().path().is_some_and(|root| path.starts_with(&root) && root.parent().is_some())
    });
    match mount {
        Some(mount) if path.file_name().is_some() => format!("{} \u{b7} {}", mount.name(), name_of(path)),
        _ => path.display().to_string(),
    }
}

pub(super) fn recheck_row(state: &App) -> adw::SwitchRow {
    let row = adw::SwitchRow::new();
    row.set_title("Check Copies Again");
    row.set_subtitle("Once a month, while Numa is idle");
    row.set_active(state.catalog.setting(RECHECK).as_deref() != Some("no"));
    row.connect_active_notify(glib::clone!(
        #[strong] state,
        move |row| {
            let _ = state.catalog.set_setting(RECHECK, if row.is_active() { "yes" } else { "no" });
        }
    ));
    row
}

pub(super) fn recheck_now_and_then(state: &App) {
    let state = state.clone();
    glib::timeout_add_seconds_local_once(RECHECK_AFTER, move || {
        let now = glib::DateTime::now_utc().map_or(0, |now| now.to_unix());
        let last: i64 = state.catalog.setting(RECHECK_LAST).and_then(|last| last.parse().ok()).unwrap_or(0);
        if state.catalog.setting(RECHECK).as_deref() == Some("no") || numa::core::power::frugal() || now - last < MONTH {
            return;
        }
        let mut roots: Vec<PathBuf> = state.libraries.all.borrow().iter().map(|library| library.path.clone()).collect();
        roots.extend(state.catalog.setting(SECOND_COPY).map(PathBuf::from).filter(|root| root.is_dir()));
        glib::spawn_future_local(async move {
            let stop = Arc::new(AtomicBool::new(false));
            let found = gio::spawn_blocking(move || {

                std::thread::spawn(move || receipt::recheck(&roots, &stop)).join().ok()
            })
            .await
            .ok()
            .flatten();
            let Some((read, differ)) = found else { return };
            log::info!("receipts: {read} files read again, {} no longer match", differ.len());
            let _ = state.catalog.set_setting(RECHECK_LAST, &now.to_string());
            if !differ.is_empty() {

                let names: Vec<String> = differ.iter().take(2).map(|path| path.display().to_string()).collect();
                let more = if differ.len() > 2 { format!(" and {} more", differ.len() - 2) } else { String::new() };
                let toast = adw::Toast::new(&glib::markup_escape_text(&format!(
                    "{} no longer the same as when imported: {}{more}",
                    if differ.len() == 1 { "1 photograph is".to_string() } else { format!("{} photographs are", differ.len()) },
                    names.join(", ")
                )));
                toast.set_timeout(0);
                state.toasts.add_toast(toast);
            }
        });
    });
}
