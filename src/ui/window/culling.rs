use super::*;

pub(super) use numa::io::analysis::{measure_photo, regroup_bursts, FACE_EDGE};
#[cfg(test)]
pub(super) use numa::io::analysis::twins;

pub(super) use numa::io::notes::cull_note;

pub(super) fn analyse_library(state: &App, button: &gtk::Button) {
    let Some(library) = state.libraries.current.borrow().clone() else {
        state.toast("No library selected");
        return;
    };
    if state.catalog.is_offline(library.id) {
        state.toast(&format!("{} is not connected — Analyse waits until it is back", library.label()));
        return;
    }

    let pending = match state.catalog.unanalysed(library.id, cull::VERSION) {
        Ok(pending) => pending,
        Err(err) => {
            state.toast(&format!("Could not read the catalog: {err}"));
            return;
        }
    };

    button.set_sensitive(false);
    let label = button.label().unwrap_or_default().to_string();

    let state = state.clone();
    let button = button.clone();
    glib::spawn_future_local(analyse_pending(state, button, library, pending, label));
}

async fn analyse_pending(
    state: App,
    button: gtk::Button,
    library: Library,
    pending: Vec<Photo>,
    label: String,
) {

    const CHUNK: usize = 64;

    let total = pending.len();
    let mut done = 0usize;
    let mut failed = 0usize;

    let cancel = Cancel::default();

    let (measured_so_far, progress, ticking) = analysis_progress(&state, &cancel, total);

    for chunk in pending.chunks(CHUNK) {
        if cancel.stopped() {
            break;
        }
        let work: Vec<(i64, std::path::PathBuf)> =
            chunk.iter().map(|photo| (photo.id, photo.path.clone())).collect();

        let counted = measured_so_far.clone();
        let measured = gtk::gio::spawn_blocking(move || {
            use rayon::prelude::*;
            work.par_iter()
                .inspect(|_| {
                    counted.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                })
                .map(|(id, path)| {
                    let measured = measure_photo(path);
                    (*id, measured)
                })
                .collect::<Vec<_>>()
        })
        .await;

        let Ok(measured) = measured else {
            log::warn!("analysis stopped: a worker failed after {done} of {total}");
            failed += total - done;
            break;
        };

        let mut batch = Vec::with_capacity(measured.len());
        for (id, measured) in measured {
            match measured {
                Ok((frame, faces, face_sharpness, embeddings)) => {
                    batch.push((id, frame, faces, face_sharpness, embeddings));
                }
                Err(err) => {
                    log::warn!("{err}");
                    failed += 1;
                }
            }
        }

        if let Err(err) = state.catalog.save_analysis_batch(cull::VERSION, &batch) {
            log::warn!("could not store analysis: {err}");
            failed += batch.len();
        }

        done += chunk.len();
        button.set_label(&format!("{done} / {total}"));
    }

    if let Some(ticking) = ticking {
        ticking.remove();
    }

    if cancel.stopped() {
        if let Some((toast, _, _)) = &progress {
            toast.dismiss();
        }
        button.set_label(&label);
        button.set_sensitive(true);
        state.toast(&format!(
            "Stopped after {done} of {total} — what was analysed is kept"
        ));
        reload_grid(&state);
        return;
    }

    if let Some((_, text, bar)) = &progress {
        text.set_text("Grouping bursts…");
        bar.set_fraction(1.0);
    }
    let grouped = regroup_bursts(&state.catalog, library.id);
    if let Some((toast, _, _)) = &progress {
        toast.dismiss();
    }

    button.set_label(&label);
    button.set_sensitive(true);
    reload_grid(&state);

    state.toast(&analysis_summary(total, failed, grouped));
}

fn analysis_progress(
    state: &App,
    cancel: &Cancel,
    total: usize,
) -> (
    Arc<std::sync::atomic::AtomicUsize>,
    Option<(adw::Toast, gtk::Label, gtk::ProgressBar)>,
    Option<glib::SourceId>,
) {

    let measured_so_far = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let progress = (total > 0).then(|| progress_toast(&state, &cancel));
    let ticking = progress.as_ref().map(|(_, text, bar)| {
        let (text, bar) = (text.clone(), bar.clone());
        let counted = measured_so_far.clone();
        let show = move || {
            let n = counted.load(std::sync::atomic::Ordering::Relaxed).min(total);
            let fraction = n as f64 / total as f64;
            text.set_text(&format!(
                "Analysing the library — {n} of {total} · {:.0} %",
                fraction * 100.0
            ));
            bar.set_fraction(fraction);
        };
        show();
        glib::timeout_add_local(std::time::Duration::from_millis(250), move || {
            show();
            glib::ControlFlow::Continue
        })
    });
    (measured_so_far, progress, ticking)
}

use numa::io::analysis::summary as analysis_summary;
