use super::*;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

static REQUEST: OnceLock<(PathBuf, Vec<PathBuf>)> = OnceLock::new();
static FAILED: AtomicBool = AtomicBool::new(false);

pub fn request_self_test(folder: PathBuf, raws: Vec<PathBuf>) {
    let _ = REQUEST.set((folder, raws));
}

pub fn self_test_failed() -> bool {
    FAILED.load(Ordering::Relaxed)
}

pub(super) fn start_self_test(state: &App, window: &adw::ApplicationWindow) {
    let Some((folder, raws)) = REQUEST.get() else { return };
    say(&format!("Numa {}", env!("CARGO_PKG_VERSION")));

    glib::timeout_add_seconds_local_once(1800, || {
        say("FAILED: still busy after 30 minutes");
        std::process::exit(1);
    });
    let (state, window) = (state.clone(), window.clone());
    glib::spawn_future_local(async move {

        glib::timeout_future_seconds(5).await;
        for raw in raws {
            let name = raw.file_name().unwrap_or_default().to_string_lossy().to_string();
            let started = Instant::now();
            match develop(&state, raw, folder, &name).await {
                Ok(file) => say(&format!("{name}: wrote {} in {:.1} s", file.display(), started.elapsed().as_secs_f64())),
                Err(why) => {
                    FAILED.store(true, Ordering::Relaxed);
                    say(&format!("{name}: FAILED: {why}"));
                }
            }
        }
        say(&match numa::gpu::describe(raw::card_frugal()) {
            Some(card) => format!("decoded on: gpu ({card})"),
            None => "decoded on: cpu".to_string(),
        });
        glib::timeout_future_seconds(3).await;
        window.close();
    });
}

fn say(line: &str) {
    println!("self-test: {line}");
}

pub(super) fn told(message: &str) {
    if REQUEST.get().is_some() {
        say(&format!("toast: {message}"));
    }
}

async fn develop(state: &App, raw: &Path, out: &Path, name: &str) -> Result<PathBuf, String> {
    let raw = raw.canonicalize().map_err(|err| format!("{}: {err}", raw.display()))?;
    let folder = raw.parent().ok_or("the raw has no folder")?;
    say(&format!("{name}: adding its folder"));
    let library = match state.catalog.libraries().unwrap_or_default().into_iter().find(|library| library.path == folder) {
        Some(library) => library,
        None => state.catalog.add_library(folder).map_err(|err| err.to_string())?,
    };
    sync_library_beside(state, &library, "Reading the library…").await?;
    say(&format!("{name}: opening it in the editor"));
    let drawn = state.render.presented.get();
    show_opened(state, library, &raw, true);
    let open = || {
        state.open.borrow().as_ref().is_some_and(|photo| matches!(&photo.source, Source::Photo { path, .. } if *path == raw))
    };
    until(180, "open and draw", || open() && state.render.presented.get() > drawn && !state.render.in_flight.get()).await?;
    say(&format!("{name}: exporting"));
    let job = open_job(state).ok_or("nothing is open")?;
    let done: Rc<RefCell<Option<Result<PathBuf, String>>>> = Rc::default();
    let out = out.to_path_buf();
    exporting::run_export_then(state, vec![job], export::ExportSettings::default(), out.clone(), {
        let done = done.clone();
        move |_, written, failures, last| {
            *done.borrow_mut() = Some(match (written, failures) {
                (1, []) => Ok(out.join(last)),
                (_, [why, ..]) => Err(why.clone()),
                _ => Err("nothing was written".to_string()),
            });
        }
    });
    until(600, "export", || done.borrow().is_some()).await?;
    done.take().unwrap_or_else(|| Err("nothing was written".to_string()))
}

async fn until(seconds: u64, what: &str, ready: impl Fn() -> bool) -> Result<(), String> {
    let started = Instant::now();
    while !ready() {
        if started.elapsed() > Duration::from_secs(seconds) {
            return Err(format!("did not {what} within {seconds} s"));
        }
        glib::timeout_future(Duration::from_millis(100)).await;
    }
    Ok(())
}
