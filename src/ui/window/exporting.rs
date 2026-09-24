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

fn ensure_passes(document: &Document, linear: &LinearImage, work: &Work) -> Result<(), String> {
    let path = std::path::Path::new(&document.source.path);
    if document.ai_denoise > 0.0 && numa::render::ai_denoise::is_installed() {
        work.doing(Doing::Denoise);
        numa::io::denoised::ensure(path, linear, work.progress())?;
    }
    if document.ai_sharpen > 0.0 && numa::render::ai_denoise::sharpen_installed() {
        work.doing(Doing::Sharpen);
        numa::io::denoised::ensure_sharpened(path, linear, document.ai_denoise > 0.0, work.progress())?;
    }
    work.stopped().then_some(()).map_or(Ok(()), |()| Err("stopped".to_string()))
}

#[derive(Clone, Default)]
struct Work {
    stop: Arc<std::sync::atomic::AtomicBool>,

    doing: Arc<std::sync::atomic::AtomicU8>,
    done: Arc<std::sync::atomic::AtomicUsize>,
    total: Arc<std::sync::atomic::AtomicUsize>,
}

#[derive(Clone, Copy)]
#[repr(u8)]
enum Doing {
    Develop = 0,
    Denoise = 1,
    Sharpen = 2,
    Enlarge = 3,
}

impl Work {
    fn doing(&self, part: Doing) {
        use std::sync::atomic::Ordering::Relaxed;
        self.doing.store(part as u8, Relaxed);
        self.done.store(0, Relaxed);
        self.total.store(0, Relaxed);
    }

    fn stopped(&self) -> bool {
        self.stop.load(std::sync::atomic::Ordering::Relaxed)
    }

    fn progress(&self) -> impl FnMut(usize, usize) -> bool + '_ {
        use std::sync::atomic::Ordering::Relaxed;
        move |done, total| {
            self.done.store(done, Relaxed);
            self.total.store(total, Relaxed);
            !self.stopped()
        }
    }

    fn describe(&self) -> Option<String> {
        use std::sync::atomic::Ordering::Relaxed;
        let (done, total) = (self.done.load(Relaxed), self.total.load(Relaxed));
        let name = match self.doing.load(Relaxed) {
            1 => "AI denoise",
            2 => "AI sharpen",
            3 => "Super Resolution",
            _ => return None,
        };
        Some(match total {
            0 => name.to_string(),
            total => format!("{name} {}%", done * 100 / total),
        })
    }
}

fn develop_one(job: ExportJob, settings: &export::ExportSettings, work: &Work) -> Result<(export::Developed, PathBuf, Option<PathBuf>), String> {
    work.doing(Doing::Develop);
    let linear = job.source.full_resolution()?;
    ensure_passes(&job.document, &linear, work)?;

    let mut document = render::with_masks_resolved(&job.document, &linear);

    document.set_output_space(settings.written_space());

    let inputs = render_inputs(&document);

    work.doing(Doing::Develop);
    let image = export::develop(&document, linear, &inputs, settings);
    if settings.size == export::Size::Double {
        work.doing(Doing::Enlarge);
    }
    let mut image = export::enlarge(image, settings, work.progress())?;
    work.doing(Doing::Develop);
    watermark::stamp(&mut image, &settings.watermark, &settings.copyright);
    let raf = match &job.source {
        Source::Photo { path, .. } => Some(path.clone()),
        Source::Bracket { .. } => None,
    };
    Ok((image, job.source.name(), raf))
}

fn follow(progress: &adw::Toast, work: &Work, title: String) -> glib::SourceId {
    glib::timeout_add_local(
        std::time::Duration::from_millis(500),
        glib::clone!(
            #[strong] work,
            #[weak] progress,
            #[upgrade_or] glib::ControlFlow::Break,
            move || {
                match work.describe() {
                    Some(doing) => progress.set_title(&format!("{} — {doing}", title.trim_end_matches('…'))),
                    None => progress.set_title(&title),
                }
                glib::ControlFlow::Continue
            }
        ),
    )
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

    let work = Work::default();
    let has_models = settings.size == export::Size::Double
        || jobs.iter().any(|job| job.document.ai_denoise > 0.0 || job.document.ai_sharpen > 0.0);
    if total > 1 || has_models {
        progress.set_button_label(Some("Stop"));
        progress.connect_button_clicked(glib::clone!(
            #[strong] cancel,
            #[strong] work,
            move |_| {
                cancel.stop();
                work.stop.store(true, std::sync::atomic::Ordering::Relaxed);
            }
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
            let title = match total {
                1 => "Exporting…".to_string(),
                _ => format!("Exporting {} of {total}…", index + 1),
            };
            progress.set_title(&title);
            let ticker = follow(&progress, &work, title);

            let for_render = settings.clone();
            let job_source = job.source.clone();
            let worker = work.clone();
            let result = busy(&state, "Exporting…", move || develop_one(job, &for_render, &worker)).await;
            ticker.remove();
            if work.stopped() {
                stopped = true;
                collect(&mut written, &mut last, &mut failures, writing.take()).await;
                break;
            }

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
