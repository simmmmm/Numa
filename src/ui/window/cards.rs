use super::*;

pub(super) struct LazyThumb {

    pub(super) id: i64,
    pub(super) path: PathBuf,
    pub(super) mtime: i64,

    pub(super) edited: bool,
    pub(super) edge: u32,

    pub(super) asked: u32,

    pub(super) fitted: u32,

    pub(super) texture: Option<gtk::gdk::Texture>,

    pub(super) wanted: bool,
}

impl LazyThumb {
    pub(super) fn new(id: i64, path: PathBuf, mtime: i64, edited: bool, edge: u32) -> Self {
        Self { id, path, mtime, edited, edge, asked: 0, fitted: 0, texture: None, wanted: false }
    }
}

const THUMBNAIL_MARGIN: usize = 60;
const THUMBNAIL_KEEP: usize = 240;

pub(super) fn grid_edge(state: &App) -> u32 {
    thumb_edge(state)
}

pub(super) fn schedule_thumbnails(state: &App) {
    let generation = state.grid.thumbnail_generation.get().wrapping_add(1);
    state.grid.thumbnail_generation.set(generation);

    let state = state.clone();
    glib::timeout_add_local_once(std::time::Duration::from_millis(60), move || {
        if state.grid.thumbnail_generation.get() != generation {
            return;
        }
        sweep_thumbnails(&state);
    });
}

fn watch_thumbnails(state: &App) {
    if state.grid.thumbnail_watch.replace(true) {
        return;
    }
    let (state, watch) = (state.clone(), ThumbnailWatch::default());
    thumbnail::watch(move || thumbnails_moved(&state, &watch));
}

#[derive(Clone, Default)]
struct ThumbnailWatch {

    started: Rc<Cell<Option<std::time::Instant>>>,
    shown: Rc<RefCell<Option<(adw::Toast, gtk::Label, gtk::ProgressBar, std::time::Instant)>>>,
}

fn thumbnails_moved(state: &App, watch: &ThumbnailWatch) {
    let Some((done, asked, decoding)) = thumbnail::progress() else {
        if watch.started.take().is_some() {
            state.grid.scroller.update_state(&[gtk::accessible::State::Busy(false)]);
        }
        if let Some((toast, _, _, up)) = watch.shown.take() {
            dismiss(toast, up);
        }
        return;
    };
    let started = match watch.started.get() {
        Some(started) => started,
        None => {
            let now = std::time::Instant::now();
            watch.started.set(Some(now));
            state.grid.scroller.update_state(&[gtk::accessible::State::Busy(true)]);

            glib::timeout_add_local_once(
                BUSY_AFTER,
                glib::clone!(
                    #[strong] state,
                    #[strong] watch,
                    move || thumbnails_moved(&state, &watch)
                ),
            );
            now
        }
    };
    let mut shown = watch.shown.borrow_mut();
    if shown.is_none() && (!decoding || started.elapsed() < BUSY_AFTER) {
        return;
    }
    let (_, text, bar, _) = shown.get_or_insert_with(|| {
        let (toast, text, bar) = progress_toast(state, &Cancel(Rc::default()));
        toast.set_button_label(None);
        (toast, text, bar, std::time::Instant::now())
    });
    text.set_text(&format!("Making thumbnails — {done} of {asked}"));
    bar.set_fraction(done as f64 / asked.max(1) as f64);
}

#[derive(Clone, Copy, PartialEq)]
enum List {
    Grid,
    Strip,
}

pub(super) fn sweep_thumbnails(state: &App) {

    watch_thumbnails(state);
    let wall = &state.grid.wall;
    if let (Some((first, last)), true) = (wall.visible(), wall.is_mapped()) {
        grid_shown(first, last, wall.len());

        let fit = (wall.row_height() * wall.scale_factor() as f32 * 1.5).ceil() as u32;
        sweep(state, List::Grid, (first, last), fit);
    }
    let strip = &state.filmstrip.strip;
    if let (Some(shown), true) = (strip_in_sight(state), strip.is_mapped()) {
        let fit = (strip.height() as f32 * strip.scale_factor() as f32 * 1.5).ceil() as u32;
        sweep(state, List::Strip, shown, fit);
    }
}

fn strip_in_sight(state: &App) -> Option<(usize, usize)> {
    let strip = &state.filmstrip.strip;
    let width = strip.width() as f32;
    let live = state.filmstrip.live.borrow();
    let mut seen = live.iter().filter(|(_, frame)| {
        frame.is_mapped()
            && frame.compute_bounds(strip).is_some_and(|bounds| bounds.x() + bounds.width() >= 0.0 && bounds.x() <= width)
    });
    let first = *seen.next()?.0;
    Some((first, seen.last().map_or(first, |(index, _)| *index)))
}

fn lazy_of(state: &App, which: List) -> &Rc<RefCell<Vec<LazyThumb>>> {
    match which {
        List::Grid => &state.grid.lazy,
        List::Strip => &state.filmstrip.lazy,
    }
}

fn sweep(state: &App, which: List, (first, last): (usize, usize), fit: u32) {
    let list = lazy_of(state, which);
    let count = list.borrow().len();
    if count == 0 {
        return;
    }
    let fit_to = (fit > 0).then_some(fit);

    let (load, keep) = bands(list.borrow()[0].edge, fit_to, (first, last), count);

    let held_now: Vec<usize> = list
        .borrow()
        .iter()
        .enumerate()
        .filter(|(index, card)| card.wanted && !keep.contains(index))
        .map(|(index, _)| index)
        .collect();
    for index in held_now {
        list.borrow_mut()[index].wanted = false;
        list.borrow_mut()[index].texture = None;
        painted(state, which, index);
    }

    for index in load {
        let (wanted, blurry) = {
            let cards = list.borrow();
            let card = &cards[index];
            (card.wanted, card.asked != card.edge || fit_to.is_some_and(|fit| fit > card.fitted))
        };
        if wanted && !blurry {
            continue;
        }
        let (path, mtime, edge, id, edited) = {
            let mut cards = list.borrow_mut();
            cards[index].wanted = true;
            cards[index].asked = cards[index].edge;
            cards[index].fitted = fit_to.unwrap_or(u32::MAX);
            let card = &cards[index];
            (card.path.clone(), card.mtime, card.edge, card.id, card.edited)
        };

        let edits = edited.then(|| state.catalog.edits_json(id).ok().flatten()).flatten();

        let rendered = edits
            .as_deref()
            .is_some_and(|edits| numa::io::thumbs::is_cached(&path, mtime, edge, Some(edits)));
        if edits.is_some() && !rendered {
            let (state, asked) = (state.clone(), path.clone());
            thumbnail::load_thumbnail_while(&path, mtime, edge, None, fit_to, || true, move |texture| {
                arrived(&state, which, index, &asked, texture, true);
            });
        }

        let (wanted_list, wanted_path) = (list.clone(), path.clone());
        let still_wanted = move || {
            let cards = wanted_list.borrow();
            cards.get(index).is_some_and(|card| card.wanted && card.path == wanted_path)
        };
        let (state, asked) = (state.clone(), path.clone());
        thumbnail::load_thumbnail_while(&path, mtime, edge, edits, fit_to, still_wanted, move |texture| {
            arrived(&state, which, index, &asked, texture, false);
        });
    }
}

fn bands(edge: u32, fit_to: Option<u32>, (first, last): (usize, usize), count: usize) -> (std::ops::Range<usize>, std::ops::Range<usize>) {
    let held_edge = fit_to.map_or(edge, |fit| fit.min(edge));
    let shrink = |cards: usize| {
        let fewer = cards as f32 * (GRID_THUMB_EDGE as f32 / held_edge as f32).powi(2).min(1.0);
        (fewer as usize).max(12)
    };
    let (margin, held) = (shrink(THUMBNAIL_MARGIN), shrink(THUMBNAIL_KEEP).max(shrink(THUMBNAIL_MARGIN) + 12));
    let load = first.saturating_sub(margin)..(last + margin + 1).min(count);
    let keep = first.saturating_sub(held)..(last + held + 1).min(count);
    (load, keep)
}

fn arrived(state: &App, which: List, index: usize, path: &Path, texture: gtk::gdk::Texture, stand_in: bool) {
    {
        let mut cards = lazy_of(state, which).borrow_mut();
        let Some(card) = cards.get_mut(index).filter(|card| card.wanted && card.path == path) else { return };
        if stand_in && card.texture.is_some() {
            return;
        }
        card.texture = Some(texture.clone());
    }
    painted(state, which, index);
    if which == List::Grid {
        learn_aspect(state, index, &texture);
    }
}

fn painted(state: &App, which: List, index: usize) {
    match which {
        List::Grid => state.grid.wall.rebind(index),
        List::Strip => rebind_frame(state, index),
    }
}

fn learn_aspect(state: &App, index: usize, texture: &gtk::gdk::Texture) {
    let aspect = texture.width() as f32 / texture.height().max(1) as f32;
    let Some(was) = state.grid.wall.aspect(index) else { return };
    if (was - aspect).abs() < 0.01 * aspect {
        return;
    }
    state.grid.wall.set_aspect(index, aspect);
    let Some(id) = state.grid.lazy.borrow().get(index).map(|card| card.id) else { return };
    let first = {
        let mut pending = state.grid.learnt.borrow_mut();
        pending.push((id, aspect));
        pending.len() == 1
    };
    if first {
        let state = state.clone();
        glib::timeout_add_local_once(std::time::Duration::from_secs(2), move || {
            let learnt = state.grid.learnt.take();
            if let Err(err) = state.catalog.set_aspects(&learnt) {
                log::warn!("could not keep {} photographs' shapes: {err}", learnt.len());
            }
        });
    }
}

pub(super) fn refresh_thumbnail(state: &App, id: i64, edited: bool) {
    for list in [&state.grid.lazy, &state.filmstrip.lazy] {
        for card in list.borrow_mut().iter_mut().filter(|card| card.id == id) {
            card.wanted = false;
            card.asked = 0;
            card.edited = edited;
        }
    }

    schedule_thumbnails(state);
}

#[derive(Clone, Copy, PartialEq)]
pub(super) enum Saving {
    StillEditing,
    MovingOn,
}

pub(super) fn save_open_edits(state: &App) {
    save_edits(state, Saving::MovingOn)
}

pub(super) fn save_edits(state: &App, when: Saving) {

    let touched = {
        let open = state.open.borrow();
        match open.as_ref().map(|photo| (&photo.source, photo.document.is_untouched())) {
            Some((Source::Photo { id, .. }, untouched)) => Some((*id, !untouched)),
            _ => None,
        }
    };

    let was = touched.and_then(|(id, _)| {
        let stale = state.catalog.edits_json(id).ok().flatten()?;
        let cards = state.grid.cards.borrow();
        let photo = cards.get(&id)?;
        Some((stale, photo.path.clone(), photo.mtime))
    });

    let saved = {
        let open = state.open.borrow();
        let Some(photo) = open.as_ref() else { return };
        if photo.edits_unreadable && photo.document.is_untouched() {
            return;
        }
        match &photo.source {
            Source::Photo { id, path } => state.catalog.save_edits(*id, &photo.document).and_then(|()| {

                let states: Vec<Document> = photo
                    .history
                    .states
                    .iter()
                    .map(|step| {
                        let mut document = Document::new(path.to_string_lossy().to_string());
                        step.restore(&mut document);
                        document
                    })
                    .collect();
                state.catalog.save_history(*id, &states, photo.history.position)
            }),
            Source::Bracket { .. } => Ok(()),
        }
    };
    if let Err(err) = saved {
        state.toast(&format!("Could not save adjustments: {err}"));
        return;
    }
    let now = touched.and_then(|(id, _)| state.catalog.edits_json(id).ok().flatten());
    if let Some((stale, path, mtime)) = &was {
        if now.as_deref() != Some(stale.as_str()) {
            numa::io::thumbs::forget(path, *mtime, thumb_edge(state), Some(stale));
        }
    }

    if let (Saving::MovingOn, Some(edits), Some((_, path, mtime))) = (when, &now, &was) {
        let edge = thumb_edge(state);
        let asked = timing().then(std::time::Instant::now);
        type Made = Box<dyn FnOnce() -> image::RgbImage + Send>;
        let made: Option<(&str, Made)> = {
            let open = state.open.borrow();
            open.as_ref().map(|photo| {
                let print = fingerprint(&photo.document, &colour_key(&photo.document));
                let shown = state.render.on_screen.borrow_mut().take().filter(|(generation, document, _)| {
                    *generation == state.open_generation.get() && fingerprint(document, &colour_key(document)) == print
                });
                let behind = photo.behind.as_ref().filter(|(was, _)| *was == print).map(|(_, frame)| frame.clone());
                match (shown, behind) {
                    (Some((_, _, frame)), _) => ("the screen's", Box::new(move || frame) as Made),
                    (_, Some(frame)) => ("the backdrop's", Box::new(move || Arc::unwrap_or_clone(frame)) as Made),
                    _ => {
                        let scale = photo.proxy.width.max(photo.proxy.height) as f32
                            / photo.full_size.0.max(photo.full_size.1).max(1) as f32;
                        let (document, working) = (photo.document.clone(), photo.working.clone());
                        ("rendered", Box::new(move || render::apply_stack(&document, &*working, scale)) as Made)
                    }
                }
            })
        };

        if let Some((from, made)) = made {
            let (state, path, mtime, edits) = (state.clone(), path.clone(), *mtime, edits.clone());
            glib::spawn_future_local(async move {
                let _ = gio::spawn_blocking(move || {
                    let small = image::DynamicImage::ImageRgb8(made()).thumbnail(edge, edge).into_rgb8();
                    numa::io::thumbs::store(&path, mtime, edge, Some(&edits), &small);
                })
                .await;
                if let Some(asked) = asked {
                    log::info!("card's picture on the way out, {from}, in {:.1} ms off the main thread", asked.elapsed().as_secs_f32() * 1000.0);
                }
                if let Some((id, edited)) = touched {
                    refresh_thumbnail(&state, id, edited);
                }
            });
            return;
        }
    }

    if let (Saving::MovingOn, Some((id, edited))) = (when, touched) {
        refresh_thumbnail(state, id, edited);
    }
}
