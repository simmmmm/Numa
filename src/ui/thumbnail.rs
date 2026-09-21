use gtk::gdk;
use gtk::glib;
use numa::io::thumbs;
use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};

fn max_concurrent() -> usize {
    std::thread::available_parallelism().map_or(4, |n| n.get().clamp(2, 8))
}

struct Job {
    path: PathBuf,
    mtime: i64,
    max_edge: u32,

    edits: Option<String>,
    apply: Box<dyn Fn(gdk::Texture)>,

    still_wanted: Box<dyn Fn() -> bool>,
}

thread_local! {
    static QUEUE: RefCell<VecDeque<Job>> = const { RefCell::new(VecDeque::new()) };
    static IN_FLIGHT: Cell<usize> = const { Cell::new(0) };

    static ASKED: Cell<usize> = const { Cell::new(0) };
    static DONE: Cell<usize> = const { Cell::new(0) };
    static DECODED: Cell<bool> = const { Cell::new(false) };

    static RENDERING: Cell<bool> = const { Cell::new(false) };
}

pub fn progress() -> Option<(usize, usize, bool)> {
    (ASKED.get() > DONE.get()).then(|| (DONE.get(), ASKED.get(), DECODED.get()))
}

pub fn load_thumbnail<F: Fn(gdk::Texture) + 'static>(
    path: &Path,
    mtime: i64,
    max_edge: u32,
    edits: Option<String>,
    apply: F,
) {
    load_thumbnail_while(path, mtime, max_edge, edits, || true, apply)
}

pub fn load_thumbnail_while<W: Fn() -> bool + 'static, F: Fn(gdk::Texture) + 'static>(
    path: &Path,
    mtime: i64,
    max_edge: u32,
    edits: Option<String>,
    still_wanted: W,
    apply: F,
) {
    QUEUE.with(|queue| {
        queue.borrow_mut().push_back(Job {
            path: path.to_path_buf(),
            mtime,
            max_edge,
            edits,
            apply: Box::new(apply),
            still_wanted: Box::new(still_wanted),
        })
    });
    ASKED.set(ASKED.get() + 1);
    pump();
}

pub fn cancel_pending() {
    let dropped = QUEUE.with(|queue| queue.borrow_mut().drain(..).count());
    ASKED.set(ASKED.get().saturating_sub(dropped));
    settle();
}

fn pump() {
    while IN_FLIGHT.get() < max_concurrent() {
        let Some(job) = next_job() else { return };

        let rendering = job.edits.is_some() && !thumbs::is_cached(&job.path, job.mtime, job.max_edge, job.edits.as_deref());
        RENDERING.set(RENDERING.get() || rendering);
        IN_FLIGHT.set(IN_FLIGHT.get() + 1);
        glib::spawn_future_local(async move {
            let (path, mtime, max_edge, edits) = (job.path, job.mtime, job.max_edge, job.edits);
            let loaded = gtk::gio::spawn_blocking(move || {
                let edits = edits.as_deref();
                let decoded = !thumbs::is_cached(&path, mtime, max_edge, edits);
                (decoded, thumbs::load(&path, mtime, max_edge, edits))
            })
            .await
            .map(|(decoded, loaded)| {
                DECODED.set(DECODED.get() || decoded);
                loaded
            });

            if rendering {
                RENDERING.set(false);
            }
            IN_FLIGHT.set(IN_FLIGHT.get().saturating_sub(1));
            DONE.set(DONE.get() + 1);

            if let Ok(Ok(image)) = loaded {

                (job.apply)(crate::ui::display::texture(image));
            }

            pump();
            settle();
        });
    }
}

fn next_job() -> Option<Job> {
    loop {
        let job = QUEUE.with(|queue| {
            let mut queue = queue.borrow_mut();
            let at = match RENDERING.get() {
                false => 0,
                true => queue.iter().position(|job| job.edits.is_none())?,
            };
            queue.remove(at)
        })?;

        if (job.still_wanted)() {
            return Some(job);
        }
        DONE.set(DONE.get() + 1);
    }
}

fn settle() {
    if IN_FLIGHT.get() == 0 && QUEUE.with(|queue| queue.borrow().is_empty()) {
        ASKED.set(0);
        DONE.set(0);
        DECODED.set(false);
    }
}
