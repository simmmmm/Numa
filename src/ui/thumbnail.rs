use gtk::gdk;
use gtk::glib::{self, Bytes};
use gtk::prelude::*;
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
    apply: Box<dyn Fn(gdk::Texture)>,
}

thread_local! {
    static QUEUE: RefCell<VecDeque<Job>> = const { RefCell::new(VecDeque::new()) };
    static IN_FLIGHT: Cell<usize> = const { Cell::new(0) };

    static ASKED: Cell<usize> = const { Cell::new(0) };
    static DONE: Cell<usize> = const { Cell::new(0) };
    static DECODED: Cell<bool> = const { Cell::new(false) };
}

pub fn progress() -> Option<(usize, usize, bool)> {
    (ASKED.get() > DONE.get()).then(|| (DONE.get(), ASKED.get(), DECODED.get()))
}

pub fn load_thumbnail<F: Fn(gdk::Texture) + 'static>(path: &Path, mtime: i64, max_edge: u32, apply: F) {
    QUEUE.with(|queue| {
        queue.borrow_mut().push_back(Job {
            path: path.to_path_buf(),
            mtime,
            max_edge,
            apply: Box::new(apply),
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
        let Some(job) = QUEUE.with(|queue| queue.borrow_mut().pop_front()) else { return };

        IN_FLIGHT.set(IN_FLIGHT.get() + 1);
        glib::spawn_future_local(async move {
            let (path, mtime, max_edge) = (job.path, job.mtime, job.max_edge);
            let loaded = gtk::gio::spawn_blocking(move || {
                let decoded = !thumbs::is_cached(&path, mtime, max_edge);
                (decoded, thumbs::load(&path, mtime, max_edge))
            })
            .await
            .map(|(decoded, loaded)| {
                DECODED.set(DECODED.get() || decoded);
                loaded
            });

            IN_FLIGHT.set(IN_FLIGHT.get().saturating_sub(1));
            DONE.set(DONE.get() + 1);

            if let Ok(Ok(image)) = loaded {
                let (width, height) = (image.width() as i32, image.height() as i32);
                let texture = gdk::MemoryTexture::new(
                    width,
                    height,
                    gdk::MemoryFormat::R8g8b8,
                    &Bytes::from_owned(image.into_raw()),
                    width as usize * 3,
                );
                (job.apply)(texture.upcast());
            }

            pump();
            settle();
        });
    }
}

fn settle() {
    if IN_FLIGHT.get() == 0 && QUEUE.with(|queue| queue.borrow().is_empty()) {
        ASKED.set(0);
        DONE.set(0);
        DECODED.set(false);
    }
}
