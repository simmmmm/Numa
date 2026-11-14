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

    let places = settings.metadata && !settings.strip_location;
    if !settings.keywords && !places {
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

    fn describe(&self) -> Option<(String, f64)> {
        use std::sync::atomic::Ordering::Relaxed;
        let (done, total) = (self.done.load(Relaxed), self.total.load(Relaxed));
        let name = match self.doing.load(Relaxed) {
            1 => "AI denoise",
            2 => "AI sharpen",
            3 => "Super Resolution",
            _ => return None,
        };
        Some(match total {
            0 => (name.to_string(), 0.0),
            total => (format!("{name} {}%", done * 100 / total), done as f64 / total as f64),
        })
    }
}

fn develop_one(mut job: ExportJob, settings: &export::ExportSettings, work: &Work) -> Result<(export::Developed, PathBuf, Option<PathBuf>), String> {
    work.doing(Doing::Develop);

    if let Err(why) = numa::io::camera_look::fill(&mut job.document) {
        log::info!("As Shot left out of an export: {why}");
    }
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

fn follow(text: &gtk::Label, bar: &gtk::ProgressBar, work: &Work, title: String, (index, total): (usize, usize)) -> glib::SourceId {
    glib::timeout_add_local(
        std::time::Duration::from_millis(500),
        glib::clone!(
            #[strong] work,
            #[weak] text,
            #[weak] bar,
            #[upgrade_or] glib::ControlFlow::Break,
            move || {
                match work.describe() {
                    Some((doing, part)) => {
                        text.set_text(&format!("{} — {doing}", title.trim_end_matches('…')));
                        bar.set_fraction((index as f64 + part) / total as f64);
                    }
                    None => text.set_text(&title),
                }
                glib::ControlFlow::Continue
            }
        ),
    )
}

pub(super) fn run_export(state: &App, jobs: Vec<ExportJob>, settings: export::ExportSettings, directory: PathBuf) {
    run_export_then(state, jobs, settings, directory, exported);
}

pub(super) fn exported(state: &App, written: usize, failures: &[String], last: &str) {
    state.toast(&match (written, failures) {
        (1, []) => format!("Exported {last}"),
        (n, []) => format!("Exported {n} photographs"),
        (0, [only]) => format!("Export failed: {only}"),
        (n, many) => format!("Exported {n}, {} failed", many.len()),
    });
}

pub(super) fn run_export_then(
    state: &App,
    jobs: Vec<ExportJob>,
    settings: export::ExportSettings,
    directory: PathBuf,
    done: impl FnOnce(&App, usize, &[String], &str) + 'static,
) {
    let total = jobs.len();

    let cancel = Cancel::default();

    let work = Work::default();
    let has_models = settings.size == export::Size::Double
        || jobs.iter().any(|job| job.document.ai_denoise > 0.0 || job.document.ai_sharpen > 0.0);

    let progress = (total > 1 || has_models).then(|| {
        let (toast, text, bar) = progress_toast(state, &cancel);
        toast.connect_button_clicked(glib::clone!(
            #[strong] work,
            move |_| work.stop.store(true, std::sync::atomic::Ordering::Relaxed)
        ));
        text.set_text(&format!("Exporting {total}…"));
        (toast, text, bar)
    });

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
                _ => format!("Exporting {} of {total}", index + 1),
            };
            let ticker = progress.as_ref().map(|(_, text, bar)| {
                text.set_text(&title);
                bar.set_fraction(index as f64 / total as f64);
                has_models.then(|| follow(text, bar, &work, title, (index, total)))
            });

            let for_render = settings.clone();
            let job_source = job.source.clone();
            let job_directory = directory.clone();
            let worker = work.clone();
            let develop = move || develop_one(job, &for_render, &worker);
            let result = match &progress {
                Some(_) => gtk::gio::spawn_blocking(develop).await,
                None => busy(&state, "Exporting…", develop).await,
            };
            if let Some(ticker) = ticker.flatten() {
                ticker.remove();
            }
            if work.stopped() {
                stopped = true;
                collect(&mut written, &mut last, &mut failures, writing.take()).await;
                break;
            }

            collect(&mut written, &mut last, &mut failures, writing.take()).await;
            let started = match result {
                Ok(Ok((image, name, raf))) => {
                    let settings = settings.clone();
                    let about = match &job_source {
                        Source::Photo { id, .. } => descriptions.get(id).cloned().unwrap_or_default(),
                        Source::Bracket { .. } => Default::default(),
                    };
                    let directory = job_directory;
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

            writing = started;
        }
        collect(&mut written, &mut last, &mut failures, writing.take()).await;

        if let Some((toast, _, _)) = progress {
            toast.dismiss();
        }
        if stopped {
            state.toast(&format!(
                "Stopped after {written} of {total} — those files are written"
            ));
            return;
        }
        for failure in &failures {
            log::warn!("export: {failure}");
        }
        done(&state, written, &failures, &last);
    });
}

pub(super) fn copy_edge(catalog: &Catalog) -> u32 {
    export::copy_edge(catalog.recall::<u64>(COPY_EDGE))
}

pub(super) fn keep_copy_edge(catalog: &Catalog, edge: u32) {
    catalog.remember(COPY_EDGE, &edge);
}

pub(super) fn copy_job(state: &App) -> Option<ExportJob> {
    match state.stack.visible_child_name().as_deref() {
        Some("editor") => open_job(state),
        Some("library") => {
            let mut jobs = selected_jobs(state);
            (jobs.len() == 1).then(|| jobs.remove(0))
        }
        _ => None,
    }
}

pub(super) fn copy_photo(state: &App, job: Option<ExportJob>) {
    let Some(job) = job else {
        state.toast("Select one photo to copy");
        return;
    };
    if state.export.copying.replace(true) {
        return;
    }
    let settings = export::ExportSettings::copy(copy_edge(&state.catalog));
    let state = state.clone();
    glib::spawn_future_local(async move {
        let started = std::time::Instant::now();
        let developed = busy(&state, "Copying the photo…", move || develop_one(job, &settings, &Work::default())).await;
        state.export.copying.set(false);
        let image = match developed {
            Ok(Ok((export::Developed::Eight(image), _, _))) => image,
            Ok(Err(why)) => {
                log::warn!("copy: {why}");
                return state.toast("Could not copy. Use Export.");
            }
            _ => return state.toast("Could not copy. Use Export."),
        };
        let (width, height) = image.dimensions();
        log::info!("copy: {width} × {height} px in {:.0?}", started.elapsed());
        let Some(display) = gtk::gdk::Display::default() else { return };

        display.clipboard().set_texture(&crate::ui::display::unconverted(image));
        state.toast(&format!("Photo copied, {width} × {height} px"));
    });
}

#[cfg(test)]
mod copy_tests {
    use super::*;

    #[test]
    fn the_copy_size_is_kept() {
        let folder = std::env::temp_dir().join(format!("numa-copy-edge-{}", std::process::id()));
        std::fs::create_dir_all(&folder).unwrap();
        let catalog = Catalog::open(&folder.join("catalog.db")).unwrap();
        assert_eq!(copy_edge(&catalog), 4096, "until one is chosen");
        keep_copy_edge(&catalog, 1080);
        assert_eq!(copy_edge(&Catalog::open(&folder.join("catalog.db")).unwrap()), 1080);
        catalog.remember(COPY_EDGE, &3000u32);
        assert_eq!(copy_edge(&catalog), 4096, "a size no client copies at");
        std::fs::remove_dir_all(&folder).unwrap();
    }
}
