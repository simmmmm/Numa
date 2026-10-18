use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use numa_cull as cull;
use numa_io::catalog::{Library, Photo};
use numa_io::thumbs;
use rayon::prelude::*;

use crate::activity::Kind;
use crate::Server;

const EVERY: Duration = Duration::from_secs(15 * 60);

const CHUNK: usize = 16;

#[derive(Default)]
pub struct Nudge {
    asked: Mutex<bool>,
    wake: Condvar,
}

impl Nudge {
    pub fn nudge(&self) {
        *self.asked.lock().unwrap_or_else(|poisoned| poisoned.into_inner()) = true;
        self.wake.notify_all();
    }

    fn taken(&self) -> bool {
        std::mem::take(&mut *self.asked.lock().unwrap_or_else(|poisoned| poisoned.into_inner()))
    }

    fn pending(&self) -> bool {
        *self.asked.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    fn wait(&self, at_most: Duration) {
        let asked = self.asked.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        let _ = self.wake.wait_timeout_while(asked, at_most, |asked| !*asked);
    }
}

fn lower_priority() {

    unsafe {
        libc::setpriority(libc::PRIO_PROCESS, 0, 10);
    }
}

pub fn half_the_cores() -> usize {
    (std::thread::available_parallelism().map_or(2, |n| n.get()) / 2).max(1)
}

pub fn start(server: Arc<Server>) {
    let fetching = server.clone();
    let _ = std::thread::Builder::new().name("numa-models".into()).spawn(move || {
        lower_priority();
        crate::models::fetch(&fetching);

        numa_infer::release_all();

        fetching.jobs.nudge();
    });
    let _ = std::thread::Builder::new().name("numa-jobs".into()).spawn(move || {
        lower_priority();
        let pool = rayon::ThreadPoolBuilder::new()
            .num_threads(half_the_cores())
            .thread_name(|k| format!("numa-job-{k}"))
            .start_handler(|_| lower_priority())
            .build()
            .expect("the background's threads");
        run(&server, &pool);
    });
}

fn run(server: &Arc<Server>, pool: &rayon::ThreadPool) {

    let marker = server.data.join("gpu-decode-attempt");
    match marker.exists() {
        true => log::warn!("the last start died in the graphics driver; decoding on the processor"),
        false => numa_io::raw::warm_up_gpu(Some(marker)),
    }

    let mut failed: HashSet<i64> = HashSet::new();
    let mut scanned: Option<Instant> = None;
    loop {
        if !server.activity.paused() && crate::night::due(server) {
            crate::night::run(server, &mut failed);
            scanned = Some(Instant::now());
        }
        if !server.activity.paused() && scanned.is_none_or(|at| at.elapsed() >= EVERY) {
            scan(server, false);
            scanned = Some(Instant::now());
        }
        server.jobs.taken();
        let plan = plan(server, &failed);

        if server.activity.paused() {
            server.activity.hold();
            continue;
        }
        let done = pool.install(|| {
            ahead(server, plan.thumbnails, &mut failed) && analyse(server, plan.analyse, &mut failed) && read_words(server, plan.words) && see_things(server)
        });
        if done {
            let next = EVERY.saturating_sub(scanned.map_or(Duration::ZERO, |at| at.elapsed()));
            server.jobs.wait(next);
        }
    }
}

pub fn scan(server: &Arc<Server>, full: bool) -> usize {
    let started = Instant::now();
    server.activity.plan(Kind::Scan, 0, None);
    server.activity.start(Kind::Scan);
    let mut folders = crate::manage::found(server);
    server.home.set_found(folders.clone());
    for account in server.store.accounts().unwrap_or_default() {
        let folder = server.home.sent_folder(&account.name);
        if std::fs::create_dir_all(&folder).is_ok() {
            folders.push(folder);
        }
    }
    let home = server.clone();
    server.home.blocking(move |catalog| {
        for folder in &folders {
            if let Err(err) = catalog.add_library(folder) {
                log::warn!("{err}");
            }
        }
        home.shelve(None, catalog);
    });
    let mut changed = 0;
    let libraries = server.home.all_libraries();
    server.activity.total(Kind::Scan, libraries.len());
    for library in libraries {
        server.activity.at(Kind::Scan, library.id, None);
        match walk(server, library, full) {
            Some(Ok(changes)) => changed += changes.added + changes.updated + changes.removed,
            Some(Err(err)) => log::warn!("{err}"),
            None => {}
        }
        server.activity.add(Kind::Scan, 1);
    }

    crate::manage::empty_old(server);

    crate::by::learn(server);
    server.activity.finish(Kind::Scan);
    server.scanned.store(true, std::sync::atomic::Ordering::Relaxed);
    server.cards.stale.store(true, std::sync::atomic::Ordering::Relaxed);
    server.tidy.stale();
    let photos: usize = server.home.all_libraries().iter().map(|library| server.home.count(library.id)).sum();
    log::info!("scan: {photos} photographs, {changed} changed, in {:.1} s", started.elapsed().as_secs_f32());
    changed
}

pub fn rescan(server: &Arc<Server>, library: Library) -> Option<Result<numa_io::catalog::Changes, String>> {
    walk(server, library, false)
}

fn walk(server: &Arc<Server>, library: Library, full: bool) -> Option<Result<numa_io::catalog::Changes, String>> {
    let walked = library.clone();
    let known = match full {
        true => Default::default(),
        false => server.home.blocking(move |catalog| catalog.known_files(&walked).unwrap_or_default())?,
    };
    let mut found = numa_io::catalog::scan(&library.path, &known);
    let removed = server.store.removed(library.id);
    if !removed.is_empty() {
        found.files.retain(|(path, ..)| !removed.contains(&crate::manage::within(&library, path)));
    }
    let home = server.clone();
    server.home.blocking(move |catalog| {
        let changes = catalog.apply_scan(&library, &found);
        if changes.as_ref().is_ok_and(|changes| changes.any()) {
            home.shelve(Some(library.id), catalog);
        }
        changes
    })
}

fn everything(server: &Server) -> Vec<(Library, Vec<Photo>)> {
    let mut libraries = server.home.all_libraries();

    libraries.sort_by_key(|library| library.path.parent() != Some(server.home.sent.as_path()));
    libraries.into_iter().map(|library| (library.clone(), server.home.photos(&[library.id]))).collect()
}

type Thumbnails = Vec<(Library, Vec<(Photo, Option<String>)>)>;

struct Plan {
    thumbnails: Thumbnails,
    analyse: Vec<(Library, Vec<Photo>)>,
    words: Vec<(Library, Vec<(PathBuf, i64)>)>,
}

fn plan(server: &Arc<Server>, failed: &HashSet<i64>) -> Plan {
    let all = everything(server);
    let photos = all.iter().map(|(_, photos)| photos.len()).sum();
    let thumbnails: Thumbnails = all
        .iter()
        .map(|(library, photos)| (library.clone(), missing_thumbnails(server, photos.iter().filter(|photo| !failed.contains(&photo.id)).cloned().collect())))
        .collect();
    let faces = cull::faces::is_installed() && cull::people::is_installed();
    let analyse: Vec<(Library, Vec<Photo>)> = match faces {
        false => Vec::new(),
        true => all
            .iter()
            .map(|(library, _)| {
                let id = library.id;
                let pending = server.home.blocking(move |catalog| catalog.unanalysed(id, cull::VERSION).unwrap_or_default()).unwrap_or_default();
                (library.clone(), pending.into_iter().filter(|photo| !failed.contains(&photo.id)).collect())
            })
            .collect(),
    };
    let words: Vec<(Library, Vec<(PathBuf, i64)>)> = match numa_io::words::installed() {
        false => Vec::new(),
        true => {
            let libraries: Vec<Library> = all.iter().map(|(library, _)| library.clone()).collect();
            let indexes = crate::timeline::indexes(server, &libraries);
            all.into_iter()
                .map(|(library, photos)| {
                    let index = &indexes[&library.id];
                    let pending = photos.into_iter().filter(|photo| index.get(&photo.path, photo.mtime).is_none()).map(|photo| (photo.path, photo.mtime)).collect();
                    (library, pending)
                })
                .collect()
        }
    };
    let activity = &server.activity;
    for kind in [Kind::Thumbnails, Kind::Analyse, Kind::Words, Kind::Things] {
        activity.all(kind, photos);
    }
    activity.plan(Kind::Thumbnails, thumbnails.iter().map(|(_, list)| list.len()).sum(), None);
    let waits = |installed: bool, why| (!installed).then_some(why);
    activity.plan(Kind::Analyse, analyse.iter().map(|(_, list)| list.len()).sum(), waits(faces, "Waits for the face models to download"));
    let reading = numa_io::words::installed();
    let reading_total = words.iter().map(|(_, list)| list.len()).sum();
    activity.plan(Kind::Words, reading_total, waits(reading, "Waits for SigLIP 2 to download"));

    activity.plan(Kind::Things, reading_total, waits(reading, "Waits for search in words"));
    Plan { thumbnails, analyse, words }
}

fn missing_thumbnails(server: &Arc<Server>, photos: Vec<Photo>) -> Vec<(Photo, Option<String>)> {
    let ids: Vec<i64> = photos.iter().filter(|photo| photo.edited).map(|photo| photo.id).collect();
    let edits: std::collections::HashMap<i64, String> = server
        .home
        .blocking(move |catalog| ids.into_iter().filter_map(|id| Some((id, catalog.edits_json(id).ok()??))).collect())
        .unwrap_or_default();
    photos
        .into_iter()
        .map(|photo| {
            let edits = edits.get(&photo.id).cloned();
            (photo, edits)
        })
        .filter(|(photo, edits)| {
            !thumbs::is_cached(&photo.path, photo.mtime, thumbs::STEPS[0], edits.as_deref())
                || !thumbs::is_cached(&photo.path, photo.mtime, thumbs::LIBRARY_EDGE, edits.as_deref())
        })
        .collect()
}

fn stop(server: &Server) -> bool {
    server.jobs.pending() || server.activity.paused()
}

fn ahead(server: &Arc<Server>, plan: Thumbnails, failed: &mut HashSet<i64>) -> bool {
    let (started, mut made) = (Instant::now(), 0usize);
    server.activity.start(Kind::Thumbnails);
    for (library, missing) in plan {
        if missing.is_empty() {
            continue;
        }
        server.activity.at(Kind::Thumbnails, library.id, None);
        let (mut shaped, mut nudged) = (false, false);
        for chunk in missing.chunks(CHUNK) {
            if stop(server) {
                nudged = true;
                break;
            }

            let done: Vec<Result<Option<(i64, f32)>, i64>> = chunk
                .par_iter()
                .filter(|_| !server.activity.paused())
                .map(|(photo, edits)| {
                    let (path, edits) = (&photo.path, edits.as_deref());
                    let large = thumbs::load(path, photo.mtime, thumbs::STEPS[0], edits).map_err(|err| log::info!("{}: {err}", path.display())).map_err(|()| photo.id)?;
                    if !thumbs::is_cached(path, photo.mtime, thumbs::LIBRARY_EDGE, edits) {
                        let small = image::DynamicImage::ImageRgb8(large.clone()).thumbnail(thumbs::LIBRARY_EDGE, thumbs::LIBRARY_EDGE).into_rgb8();
                        thumbs::store(path, photo.mtime, thumbs::LIBRARY_EDGE, edits, &small);
                    }
                    let shape = large.width() as f32 / large.height().max(1) as f32;
                    Ok((large.height() > 0 && crate::photos::reshaped(photo.aspect, shape)).then_some((photo.id, shape)))
                })
                .collect();
            made += done.len();
            server.activity.add(Kind::Thumbnails, done.len());
            failed.extend(done.iter().filter_map(|made| made.as_ref().err()));
            let aspects: Vec<(i64, f32)> = done.into_iter().filter_map(Result::ok).flatten().collect();
            if !aspects.is_empty() {
                shaped = true;
                server.home.blocking(move |catalog| drop(catalog.set_aspects(&aspects)));
            }
        }

        if shaped {
            let (home, id) = (server.clone(), library.id);
            server.home.blocking(move |catalog| home.shelve(Some(id), catalog));
        }
        if nudged {
            return false;
        }
    }
    server.activity.finish(Kind::Thumbnails);
    if made > 0 {
        let seconds = started.elapsed().as_secs_f32();
        log::info!("thumbnails: {made} photographs in {seconds:.1} s, {:.0} ms each", seconds * 1000.0 / made as f32);
    }
    true
}

fn analyse(server: &Arc<Server>, plan: Vec<(Library, Vec<Photo>)>, failed: &mut HashSet<i64>) -> bool {
    if !(cull::faces::is_installed() && cull::people::is_installed()) {
        return true;
    }
    server.activity.start(Kind::Analyse);
    for (library, pending) in plan {
        if pending.is_empty() {
            continue;
        }
        let id = library.id;
        server.activity.at(Kind::Analyse, id, None);
        let started = Instant::now();
        for chunk in pending.chunks(CHUNK) {
            if stop(server) {
                return false;
            }
            let measured: Vec<(i64, Result<numa_io::analysis::Measured, String>)> = chunk
                .par_iter()
                .filter(|_| !server.activity.paused())
                .map(|photo| (photo.id, numa_io::analysis::measure_photo(&photo.path)))
                .collect();
            server.activity.add(Kind::Analyse, measured.len());
            let mut batch = Vec::new();
            for (id, measured) in measured {
                match measured {
                    Ok((frame, faces, sharpness, embeddings)) => batch.push((id, frame, faces, sharpness, embeddings)),
                    Err(err) => {
                        log::info!("analyse: {err}");
                        failed.insert(id);
                    }
                }
            }
            server.home.blocking(move |catalog| catalog.save_analysis_batch(cull::VERSION, &batch).map_err(|err| log::warn!("{err}")));
        }
        server.activity.at(Kind::Analyse, id, Some("Grouping bursts"));
        let home = server.clone();
        server.home.blocking(move |catalog| {
            let _ = numa_io::analysis::regroup_bursts(catalog, id);
            home.shelve(Some(id), catalog);
        });
        server.tidy.stale();
        log::info!("analyse: {} photographs of {} in {:.1} s", pending.len(), library.label(), started.elapsed().as_secs_f32());
    }
    server.activity.finish(Kind::Analyse);
    true
}

fn read_words(server: &Arc<Server>, plan: Vec<(Library, Vec<(PathBuf, i64)>)>) -> bool {
    if !numa_io::words::installed() {
        return true;
    }
    server.activity.start(Kind::Words);
    for (library, pending) in plan {
        if pending.is_empty() {
            continue;
        }
        server.activity.at(Kind::Words, library.id, None);
        let started = Instant::now();
        for chunk in pending.chunks(CHUNK * 2) {
            if stop(server) {
                return false;
            }
            match numa_io::words::read_photos(chunk.to_vec()) {
                Ok(read) => {
                    let mut indexes = crate::timeline::indexes(server, std::slice::from_ref(&library));
                    if let Some(Err(err)) = indexes.get_mut(&library.id).map(|index| index.add(read)) {
                        log::warn!("words: {}: {err}", library.path.display());
                        break;
                    }
                }
                Err(err) => {
                    log::warn!("words: {err}");
                    break;
                }
            }
            server.activity.add(Kind::Words, chunk.len());
        }
        log::info!("words: {} photographs of {} in {:.1} s", pending.len(), library.label(), started.elapsed().as_secs_f32());
    }
    server.activity.finish(Kind::Words);
    true
}

fn see_things(server: &Arc<Server>) -> bool {
    if !numa_io::words::installed() {
        return true;
    }
    let things = match server.things.get() {
        Some(things) => things,
        None => match numa_io::words::things() {
            Ok(made) => server.things.get_or_init(|| made),
            Err(err) => {
                log::warn!("things numa saw: {err}");
                return true;
            }
        },
    };
    let libraries = server.home.all_libraries();
    let mut indexes = crate::timeline::indexes(server, &libraries);
    let unseen = |index: &numa_io::words::Index| index.len().saturating_sub(index.seen().count());
    server.activity.plan(Kind::Things, libraries.iter().map(|library| unseen(&indexes[&library.id])).sum(), None);
    server.activity.start(Kind::Things);
    for library in &libraries {
        let Some(index) = indexes.get_mut(&library.id) else { continue };
        let count = unseen(index);
        if count > 0 {
            server.activity.at(Kind::Things, library.id, None);
            index.see(things);
            server.activity.add(Kind::Things, count);
            let saw = index.seen().filter(|saw| !saw.is_empty()).count();
            log::info!("things numa saw: {saw} of {} photographs of {} have one", index.len(), library.label());
        }
    }
    server.activity.finish(Kind::Things);
    true
}
