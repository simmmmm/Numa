use super::*;

pub(super) async fn collect(
    written: &mut usize,
    last: &mut String,
    failures: &mut Vec<String>,
    writing: Option<gtk::gio::JoinHandle<Result<PathBuf, String>>>,
) {
    let Some(handle) = writing else { return };
    match handle.await {
        Ok(Ok(destination)) => {
            *written += 1;
            *last = destination.file_name().unwrap_or_default().to_string_lossy().into();
            remember_recent(&destination);
        }
        Ok(Err(err)) => failures.push(err),
        Err(_) => failures.push("the file could not be written".to_string()),
    }
}

fn descriptions(
    state: &App,
    jobs: &[ExportJob],
    settings: &export::ExportSettings,
) -> std::collections::HashMap<i64, export::Description> {
    if !settings.keywords {
        return Default::default();
    }
    let ids: Vec<i64> = jobs
        .iter()
        .filter_map(|job| match job.source {
            Source::Photo { id, .. } => Some(id),
            Source::Bracket { .. } => None,
        })
        .collect();
    state.catalog.descriptions(&ids).unwrap_or_else(|err| {
        log::warn!("export: no keywords: {err}");
        Default::default()
    })
}

fn ensure_passes(document: &Document, linear: &LinearImage) -> Result<(), String> {
    let path = std::path::Path::new(&document.source.path);
    if document.ai_denoise > 0.0 && numa::render::ai_denoise::is_installed() {
        numa::io::denoised::ensure(path, linear, |_, _| true)?;
    }
    if document.ai_sharpen > 0.0 && numa::render::ai_denoise::sharpen_installed() {
        numa::io::denoised::ensure_sharpened(path, linear, document.ai_denoise > 0.0, |_, _| true)?;
    }
    Ok(())
}

pub(super) fn run_export(
    state: &App,
    jobs: Vec<ExportJob>,
    settings: export::ExportSettings,
    directory: PathBuf,
) {
    let total = jobs.len();
    let progress = adw::Toast::new(&format!("Exporting {total}…"));
    progress.set_timeout(0);

    let cancel = Cancel::default();
    if total > 1 {
        progress.set_button_label(Some("Stop"));
        progress.connect_button_clicked(glib::clone!(
            #[strong] cancel,
            move |_| cancel.stop()
        ));
    }
    state.toasts.add_toast(progress.clone());

    let descriptions = descriptions(state, &jobs, &settings);

    let state = state.clone();
    glib::spawn_future_local(async move {
        let mut written = 0usize;
        let mut failures: Vec<String> = Vec::new();
        let mut last = String::new();
        let mut stopped = false;

        let mut writing = None;

        for (index, job) in jobs.into_iter().enumerate() {
            if cancel.stopped() {
                stopped = true;
                break;
            }
            if total > 1 {
                progress.set_title(&format!("Exporting {} of {total}…", index + 1));
            }

            let for_render = settings.clone();
            let job_source = job.source.clone();
            let result = busy(&state, "Exporting…", move || {
                let settings = for_render;
                let linear = job.source.full_resolution()?;
                ensure_passes(&job.document, &linear)?;

                let mut document = render::with_masks_resolved(&job.document, &linear);

                document.set_output_space(settings.written_space());

                let inputs = render_inputs(&document);

                let image = export::develop(&document, linear, &inputs, &settings);
                let mut image = export::enlarge(image, &settings)?;
                watermark::stamp(&mut image, &settings.watermark, &settings.copyright);
                let raf = match &job.source {
                    Source::Photo { path, .. } => Some(path.clone()),
                    Source::Bracket { .. } => None,
                };
                Ok::<_, String>((image, job.source.name(), raf))
            })
            .await;

            let started = match result {
                Ok(Ok((image, name, raf))) => {
                    let settings = settings.clone();
                    let about = match &job_source {
                        Source::Photo { id, .. } => descriptions.get(id).cloned().unwrap_or_default(),
                        Source::Bracket { .. } => Default::default(),
                    };
                    let directory = directory.clone();
                    Some(gtk::gio::spawn_blocking(move || {

                        let destination = export::next_path(&directory, &name, &settings)?;
                        export::write(&image, &destination, &settings, raf.as_deref(), &about)
                            .map(|()| destination)
                    }))
                }
                Ok(Err(err)) => {
                    failures.push(err);
                    None
                }
                Err(_) => {
                    failures.push("cancelled".to_string());
                    None
                }
            };

            collect(&mut written, &mut last, &mut failures, writing.take()).await;
            writing = started;
        }
        collect(&mut written, &mut last, &mut failures, writing.take()).await;

        progress.dismiss();
        if stopped {
            state.toast(&format!(
                "Stopped after {written} of {total} — those files are written"
            ));
            return;
        }
        state.toast(&match (written, failures.as_slice()) {
            (1, []) => format!("Exported {last}"),
            (n, []) => format!("Exported {n} photographs"),
            (0, [only]) => format!("Export failed: {only}"),
            (n, many) => format!("Exported {n}, {} failed", many.len()),
        });
        for failure in &failures {
            log::warn!("export: {failure}");
        }
    });
}
