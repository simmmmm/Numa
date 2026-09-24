use super::*;

pub(super) struct LazyThumb {

    pub(super) id: i64,
    pub(super) path: PathBuf,
    pub(super) mtime: i64,

    pub(super) edited: bool,
    pub(super) edge: u32,

    pub(super) asked: u32,

    pub(super) fitted: u32,
    pub(super) picture: gtk::Picture,

    pub(super) widget: gtk::Widget,

    pub(super) wanted: bool,
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
    let state = state.clone();
    let shown: RefCell<Option<(adw::Toast, gtk::Label, gtk::ProgressBar)>> = RefCell::new(None);
    let started = std::time::Instant::now();
    glib::timeout_add_local(std::time::Duration::from_millis(100), move || {
        let Some((done, asked, decoding)) = thumbnail::progress() else {
            if let Some((toast, _, _)) = shown.take() {
                toast.dismiss();
            }
            state.grid.thumbnail_watch.set(false);
            return glib::ControlFlow::Break;
        };
        let mut shown = shown.borrow_mut();
        if shown.is_none() && (!decoding || started.elapsed() < BUSY_AFTER) {
            return glib::ControlFlow::Continue;
        }
        let (_, text, bar) = shown.get_or_insert_with(|| {
            let (toast, text, bar) = progress_toast(&state, &Cancel(Rc::default()));
            toast.set_button_label(None);
            (toast, text, bar)
        });
        text.set_text(&format!("Making thumbnails — {done} of {asked}"));
        bar.set_fraction(done as f64 / asked.max(1) as f64);
        glib::ControlFlow::Continue
    });
}

pub(super) fn sweep_thumbnails(state: &App) {
    sweep(state, &state.grid.lazy, &state.grid.wall, &state.grid.scroller.vadjustment(), false);
    sweep(
        state,
        &state.filmstrip.lazy,
        &state.filmstrip.strip,
        &state.filmstrip.scroller.hadjustment(),
        true,
    );
    watch_thumbnails(state);
}

fn sweep(
    state: &App,
    list: &Rc<RefCell<Vec<LazyThumb>>>,
    container: &impl IsA<gtk::Widget>,
    adjustment: &gtk::Adjustment,
    across: bool,
) {
    let count = list.borrow().len();
    if count == 0 {
        return;
    }

    let Some((first, last)) = visible_cards(list, container, adjustment, across, count) else {
        return;
    };

    let rows = match across {
        true => container.as_ref().height() as f32,
        false => state.grid.wall.row_height(),
    };
    let fit = (rows * container.as_ref().scale_factor() as f32 * 1.5).ceil() as u32;
    let fit_to = (fit > 0).then_some(fit);

    let edge = list.borrow()[0].edge;
    let held_edge = fit_to.map_or(edge, |fit| fit.min(edge));
    let shrink = |cards: usize| {
        let fewer = cards as f32 * (GRID_THUMB_EDGE as f32 / held_edge as f32).powi(2).min(1.0);
        (fewer as usize).max(12)
    };
    let (margin, held) = (shrink(THUMBNAIL_MARGIN), shrink(THUMBNAIL_KEEP).max(shrink(THUMBNAIL_MARGIN) + 12));
    let load = first.saturating_sub(margin)..(last + margin + 1).min(count);
    let keep = first.saturating_sub(held)..(last + held + 1).min(count);

    for index in 0..count {
        let (wanted, in_load, in_keep, blurry) = {
            let cards = list.borrow();
            let card = &cards[index];
            let blurry = card.asked != card.edge || fit_to.is_some_and(|fit| fit > card.fitted);
            (card.wanted, load.contains(&index), keep.contains(&index), blurry)
        };

        match (wanted, in_load, in_keep) {
            (false, true, _) | (true, true, _) if !wanted || blurry => {
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
                    let (list, asked) = (list.clone(), path.clone());
                    thumbnail::load_thumbnail_while(&path, mtime, edge, None, fit_to, || true, move |texture| {
                        let cards = list.borrow();
                        let still = cards.get(index).filter(|card| card.wanted && card.path == asked);
                        if let Some(card) = still.filter(|card| card.picture.paintable().is_none()) {
                            card.picture.set_paintable(Some(&texture));
                        }
                    });
                }

                let (wanted_list, wanted_path) = (list.clone(), path.clone());
                let still_wanted = move || {
                    let cards = wanted_list.borrow();
                    cards.get(index).is_some_and(|card| card.wanted && card.path == wanted_path)
                };
                let list = list.clone();
                let asked = path.clone();
                thumbnail::load_thumbnail_while(
                    &path,
                    mtime,
                    edge,
                    edits,
                    fit_to,
                    still_wanted,
                    move |texture| {
                        let cards = list.borrow();
                        let still =
                            cards.get(index).filter(|card| card.wanted && card.path == asked);
                        if let Some(card) = still {
                            card.picture.set_paintable(Some(&texture));
                        }
                    },
                );
            }
            (true, _, false) => {
                let mut cards = list.borrow_mut();
                cards[index].wanted = false;
                cards[index].picture.set_paintable(gtk::gdk::Paintable::NONE);
            }
            _ => {}
        }
    }
}

fn visible_cards(
    list: &Rc<RefCell<Vec<LazyThumb>>>,
    container: &impl IsA<gtk::Widget>,
    adjustment: &gtk::Adjustment,
    across: bool,
    count: usize,
) -> Option<(usize, usize)> {
    let (near, far) = (adjustment.value(), adjustment.value() + adjustment.page_size());
    if adjustment.page_size() <= 0.0 {
        return None;
    }

    let bounds = |index: usize| {
        let cards = list.borrow();
        let bounds = cards.get(index)?.widget.compute_bounds(container)?;
        Some(if across {
            (bounds.x() as f64, (bounds.x() + bounds.width()) as f64)
        } else {
            (bounds.y() as f64, (bounds.y() + bounds.height()) as f64)
        })
    };

    let (mut low, mut high) = (0usize, count - 1);
    while low < high {
        let middle = (low + high) / 2;
        match bounds(middle) {
            Some((_, card_far)) if card_far < near => low = middle + 1,
            Some(_) => high = middle,

            None => return None,
        }
    }
    let first = low;

    let mut last = first;
    while last + 1 < count {
        match bounds(last + 1) {
            Some((card_near, _)) if card_near <= far => last += 1,
            _ => break,
        }
    }

    Some((first, last))
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
        let (photo, _) = cards.get(&id)?;
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
        let made = {
            let open = state.open.borrow();
            open.as_ref().map(|photo| {
                let scale = photo.proxy.width.max(photo.proxy.height) as f32
                    / photo.full_size.0.max(photo.full_size.1).max(1) as f32;
                render::apply_stack(&photo.document, &*photo.working, scale)
            })
        };
        if let Some(made) = made {
            let small = image::DynamicImage::ImageRgb8(made)
                .thumbnail(edge, edge)
                .into_rgb8();
            numa::io::thumbs::store(path, *mtime, edge, Some(edits), &small);
        }
    }

    if let (Saving::MovingOn, Some((id, edited))) = (when, touched) {
        refresh_thumbnail(state, id, edited);
    }
}
