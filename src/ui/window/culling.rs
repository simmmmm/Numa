use super::*;

pub(super) const FACE_EDGE: u32 = 640;

const EYE_EDGE: u32 = 4800;

pub(super) fn cull_note(photo: &Photo, scale: &cull::Scale) -> String {
    let frame = cull::Frame {
        sharpness: photo.sharpness.unwrap_or_default(),
        blown: photo.blown.unwrap_or_default(),

        brightness: photo.brightness.unwrap_or(0.5),
        contrast: photo.contrast.unwrap_or(1.0),
        exposure: photo.exposure.unwrap_or_default(),
        focal35: photo.focal35.unwrap_or_default(),
        raw_clipped: photo.raw_clipped,
        ..Default::default()
    };

    let mut parts: Vec<String> = Vec::new();
    if let Some(suggested) = photo.suggested {
        parts.push(format!("~{suggested:.1}★"));
    }

    if let Some(note) = frame.blank_note() {
        parts.push(note.to_string());
        return parts.join(" · ");
    }
    if photo.best_of_burst {
        parts.push("best of burst".to_string());
    }

    if photo.eyes_closed == Some(true) {
        parts.push("eyes closed?".to_string());
    }

    match (photo.face_sharpness, photo.sharpness) {
        (Some(face), Some(_)) if face < scale.soft => parts.push("soft face".to_string()),

        _ if photo.sharpness.is_some() && scale.is_soft(&frame) && frame.is_slow() => {
            parts.push("soft · slow shutter".to_string())
        }
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

        match photo.raw_clipped {
            Some(raw) => format!(
                "Blown {:.1} % in the raw, {:.1} % in the camera's JPEG{} (a lot above {:.0} %)",
                raw * 100.0,
                blown * 100.0,
                if blown > cull::BLOWN && raw <= cull::BLOWN { " — the raw holds it" } else { "" },
                cull::BLOWN * 100.0
            ),
            None => format!("Blown {:.1} % (a lot above {:.0} %)", blown * 100.0, cull::BLOWN * 100.0),
        },
    ];
    if let Some(dark) = photo.raw_dark.filter(|dark| *dark >= 0.01) {
        lines.push(format!("{:.0} % of the raw is deep shadow, within 1 % of black", dark * 100.0));
    }
    if let Some(contrast) = photo.contrast {
        lines.push(format!("Spread of tone {contrast:.3} (next to nothing in it below {:.2})", cull::BLANK));
    }

    if let (Some(exposure), Some(focal35)) = (photo.exposure, photo.focal35) {
        let shutter = match exposure >= 1.0 {
            true => format!("{exposure:.1} s"),
            false => format!("1/{:.0} s", 1.0 / exposure),
        };
        let stops = (exposure * focal35).log2();
        let past = match stops > 0.0 {
            true => format!("{stops:.1} stops slower than 1/{focal35:.0}"),
            false => format!("faster than 1/{focal35:.0}"),
        };
        lines.push(format!(
            "{shutter} at {focal35:.0} mm full-frame — {past} (the shutter is named as a reason past {:.0})",
            cull::HANDHELD.log2()
        ));
    }
    match (photo.faces, photo.face_sharpness) {
        (Some(0), _) => lines.push("No faces found".to_string()),
        (Some(_), _) if photo.eyes_closed == Some(true) => {
            lines.push("Both eyes of the largest face read as closed — a guess, check it".to_string())
        }
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

    raw::load_scaled(path, EYE_EDGE).map(|full| {
        let shrink = FACE_EDGE as f32 / full.width().max(full.height()) as f32;
        let image = match shrink < 1.0 {
            true => image::imageops::thumbnail(
                &full,
                ((full.width() as f32 * shrink).round() as u32).max(1),
                ((full.height() as f32 * shrink).round() as u32).max(1),
            ),
            false => full.clone(),
        };
        let mut frame = cull::measure::of(&image);

        if let Some((exposure, focal35)) = raw::shot(path) {
            frame.exposure = exposure;
            frame.focal35 = focal35;
        }

        if let Some((clipped, dark)) = raw::raw_levels(path) {
            frame.raw_clipped = Some(clipped);
            frame.raw_dark = Some(dark);
        }

        let found = cull::faces::detect(&image);
        let faces = found.as_ref().map(|faces| faces.len() as u32);
        let sharpest = found
            .as_ref()
            .and_then(|faces| cull::faces::sharpest_face(&image, faces));

        frame.eyes_closed = found
            .iter()
            .flatten()
            .max_by(|a, b| a.width.total_cmp(&b.width))
            .and_then(|face| cull::eyes::closed(&full, face, full.width() as f32 / image.width() as f32));

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

pub(super) fn twins<'a>(
    photos: impl IntoIterator<Item = (i64, &'a Path, Option<i64>)>,
    is_raw: impl Fn(&Path) -> bool,
) -> std::collections::HashMap<i64, i64> {
    let key = |path: &Path, taken: i64| (path.file_stem().map(|stem| stem.to_string_lossy().to_uppercase()), taken);
    let dated: Vec<(i64, &Path, i64)> =
        photos.into_iter().filter_map(|(id, path, taken)| Some((id, path, taken?))).collect();
    let raws: std::collections::HashMap<_, i64> =
        dated.iter().filter(|(_, path, _)| is_raw(path)).map(|(id, path, taken)| (key(path, *taken), *id)).collect();
    dated
        .iter()
        .filter(|(_, path, _)| !is_raw(path))
        .filter_map(|(id, path, taken)| Some((*id, *raws.get(&key(path, *taken))?)))
        .collect()
}

pub(super) fn regroup_bursts(state: &App, library_id: i64) -> Result<(usize, cull::learn::Outcome), String> {
    let analysed = state.catalog.analysed(library_id)?;
    if analysed.is_empty() {
        return Ok((0, cull::learn::Outcome::TooFew { rated: 0 }));
    }

    let marks: std::collections::HashMap<i64, Photo> =
        state.catalog.photos(library_id, &Filter::default())?.into_iter().map(|photo| (photo.id, photo)).collect();

    let measured: std::collections::HashSet<i64> = analysed.iter().map(|(id, _, _)| *id).collect();
    let twin_of: std::collections::HashMap<i64, i64> =
        twins(marks.values().map(|photo| (photo.id, photo.path.as_path(), photo.taken)), raw::is_raw)
            .into_iter()
            .filter(|(_, raw)| measured.contains(raw))
            .collect();
    let (twinned, analysed): (Vec<_>, Vec<_>) = analysed.into_iter().partition(|(id, _, _)| twin_of.contains_key(id));

    let frames: Vec<cull::Frame> = analysed.iter().map(|(_, frame, _)| *frame).collect();
    let hashes: Vec<u64> = frames.iter().map(|frame| frame.hash).collect();

    let taken: Vec<i64> = analysed
        .iter()
        .map(|(id, _, _)| marks.get(id).map_or(0, |photo| photo.taken.unwrap_or(photo.mtime)))
        .collect();
    let groups = cull::bursts(&hashes, &taken, cull::BURST_TOLERANCE);
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
    let mut rows: Vec<(i64, Option<usize>, bool, f32)> = analysed
        .iter()
        .enumerate()
        .map(|(index, (id, _, _))| (*id, Some(groups[index]), best.contains(&index), suggested[index]))
        .collect();
    let by_raw: std::collections::HashMap<i64, f32> = rows.iter().map(|(id, _, _, suggested)| (*id, *suggested)).collect();
    rows.extend(twinned.iter().filter_map(|(id, _, _)| Some((*id, None, false, *by_raw.get(twin_of.get(id)?)?))));

    state.catalog.save_bursts(&rows)?;

    let found = cull::echoes(&frames, &groups, &taken);
    let echoes: Vec<(i64, Option<i64>)> = analysed
        .iter()
        .zip(&found)
        .map(|((id, _, _), echo)| (*id, echo.map(|index| analysed[index].0)))
        .chain(twinned.iter().map(|(id, _, _)| (*id, None)))
        .collect();
    state.catalog.save_echoes(&echoes)?;
    Ok((best.len(), learned))
}
