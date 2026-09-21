use super::*;

pub(super) const FACE_EDGE: u32 = 640;

pub(super) fn cull_note(photo: &Photo, scale: &cull::Scale) -> String {
    let frame = cull::Frame {
        sharpness: photo.sharpness.unwrap_or_default(),
        blown: photo.blown.unwrap_or_default(),
        ..Default::default()
    };

    let mut parts: Vec<String> = Vec::new();
    if let Some(suggested) = photo.suggested {
        parts.push(format!("~{suggested:.1}★"));
    }
    if photo.best_of_burst {
        parts.push("best of burst".to_string());
    }

    match (photo.face_sharpness, photo.sharpness) {
        (Some(face), Some(_)) if face < scale.soft => parts.push("soft face".to_string()),
        _ if photo.sharpness.is_some() && scale.is_soft(&frame) => parts.push("soft".to_string()),
        _ => {}
    }
    if photo.blown.is_some() && frame.is_blown() {
        parts.push("blown".to_string());
    }
    parts.join(" · ")
}

pub(super) fn cull_detail(photo: &Photo, scale: &cull::Scale) -> String {
    let (Some(sharpness), Some(blown)) = (photo.sharpness, photo.blown) else {
        return "Not measured yet".to_string();
    };

    let mut lines = vec![
        format!("Sharpness {sharpness:.2} (soft below {:.2} in this library)", scale.soft),
        format!("Blown {:.1} % (a lot above {:.0} %)", blown * 100.0, cull::BLOWN * 100.0),
    ];
    match (photo.faces, photo.face_sharpness) {
        (Some(0), _) => lines.push("No faces found".to_string()),
        (Some(count), Some(face)) => {
            lines.push(format!("{count} face(s), sharpest {face:.2} — this is what is scored"));
        }
        (Some(count), None) => lines.push(format!("{count} face(s), too small to judge")),
        (None, _) => lines.push("Faces not looked for".to_string()),
    }
    if let Some(suggested) = photo.suggested {
        lines.push(format!("Suggested {suggested:.1} of 5 — a suggestion, not a rating"));
    }
    lines.join("\n")
}

pub(super) fn analyse_library(state: &App, button: &adw::SplitButton) {
    let Some(library) = state.libraries.current.borrow().clone() else {
        state.toast("No library selected");
        return;
    };

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
    button: adw::SplitButton,
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
    let grouped = regroup_bursts(&state, library.id);
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

fn measure_photo(
    path: &Path,
) -> Result<(cull::Frame, Option<u32>, Option<f32>, Vec<([f32; cull::people::LENGTH], Option<Vec<u8>>)>), String> {

    raw::load_scaled(path, FACE_EDGE).map(|image| {
        let frame = cull::measure::of(&image);

        let found = cull::faces::detect(&image);
        let faces = found.as_ref().map(|faces| faces.len() as u32);
        let sharpest = found
            .as_ref()
            .and_then(|faces| cull::faces::sharpest_face(&image, faces));

        let embeddings = found
            .iter()
            .flatten()
            .filter_map(|face| {
                Some((
                    cull::people::embed(&image, face)?,
                    cull::people::portrait_jpeg(&image, face),
                ))
            })
            .collect::<Vec<_>>();
        (frame, faces, sharpest, embeddings)
    })
}

fn analysis_summary(total: usize, failed: usize, grouped: Result<(usize, cull::learn::Outcome), String>) -> String {

    let faces = if cull::faces::is_installed() {
        String::new()
    } else {
        format!(" · {}", cull::faces::model_missing_message())
    };

    match (total, failed, grouped) {
        (0, _, Ok((bursts, learned))) => format!("Already analysed — {bursts} burst(s) · {learned}{faces}"),
        (_, 0, Ok((bursts, learned))) => {
            format!("Analysed {total} photo(s) — {bursts} burst(s) · {learned}{faces}")
        }
        (_, n, Ok((bursts, learned))) => format!(
            "Analysed {} of {total} — {bursts} burst(s), {n} unreadable · {learned}{faces}",
            total - n
        ),
        (_, _, Err(err)) => format!("Grouping failed: {err}"),
    }
}

pub(super) fn regroup_bursts(state: &App, library_id: i64) -> Result<(usize, cull::learn::Outcome), String> {
    let analysed = state.catalog.analysed(library_id)?;
    if analysed.is_empty() {
        return Ok((0, cull::learn::Outcome::TooFew { rated: 0 }));
    }

    let marks: std::collections::HashMap<i64, Photo> =
        state.catalog.photos(library_id, &Filter::default())?.into_iter().map(|photo| (photo.id, photo)).collect();

    let frames: Vec<cull::Frame> = analysed.iter().map(|(_, frame, _)| *frame).collect();
    let hashes: Vec<u64> = frames.iter().map(|frame| frame.hash).collect();
    let groups = cull::bursts(&hashes, cull::BURST_TOLERANCE);
    let faces: Vec<Option<f32>> = analysed.iter().map(|(_, _, face)| *face).collect();
    let best = cull::best_of_each(&frames, &faces, &groups);

    let best: std::collections::HashSet<usize> = best.into_iter().collect();

    let scale = cull::Scale::of(analysed.iter().map(|(_, frame, face)| face.unwrap_or(frame.sharpness)));
    let samples: Vec<cull::learn::Sample> = analysed
        .iter()
        .enumerate()
        .map(|(index, (id, frame, face))| {
            let photo = marks.get(id);
            cull::learn::Sample::new(
                scale,
                frame,
                photo.and_then(|photo| photo.faces),
                *face,
                best.contains(&index),
                groups[index],
                photo.map_or(0, |photo| photo.rating),
                photo.is_some_and(|photo| photo.flag == Flag::Rejected),
            )
        })
        .collect();
    let (suggested, learned) = cull::learn::score(&samples);
    let rows: Vec<(i64, usize, bool, f32)> = analysed
        .iter()
        .enumerate()
        .map(|(index, (id, _, _))| (*id, groups[index], best.contains(&index), suggested[index]))
        .collect();

    state.catalog.save_bursts(&rows)?;
    Ok((best.len(), learned))
}
