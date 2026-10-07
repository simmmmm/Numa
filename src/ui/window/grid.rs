use super::*;

fn filter_of_this_place(state: &App, library: &Library) {
    let key = {
        let filter = state.libraries.filter.borrow();
        match (&filter.album, &filter.person) {
            _ if filter.all_libraries => "filter/everywhere".to_string(),
            (Some(album), _) => format!("filter/album/{album}"),
            (_, Some(person)) => format!("filter/person/{}", person.to_lowercase()),
            _ => format!("filter/library/{}", library.id),
        }
    };

    if state.libraries.shown.replace(Some(key.clone())).is_some_and(|shown| shown != key) {
        let kept = state.catalog.recall::<Filter>(&key).unwrap_or_default();
        let mut filter = state.libraries.filter.borrow_mut();
        *filter = Filter {
            sort: filter.sort,
            reversed: filter.reversed,
            all_libraries: filter.all_libraries,
            album: filter.album.take(),
            person: filter.person.take(),
            ..kept
        };
    }

    state.catalog.remember(GRID_FILTER, &*state.libraries.filter.borrow());
    state.catalog.remember(&key, &*state.libraries.filter.borrow());
}

pub(super) fn reload_grid(state: &App) {

    close_loupe(state);

    thumbnail::cancel_pending();

    state.grid.wall.remove_all();
    state.grid.cards.borrow_mut().clear();
    state.grid.lazy.borrow_mut().clear();

    let Some(library) = state.libraries.current.borrow().clone() else {
        state.grid.empty.set_visible(false);
        state.grid.welcome.set_visible(true);
        return;
    };
    state.grid.welcome.set_visible(false);

    refresh_picker(state);
    filter_of_this_place(state, &library);

    let offline = state.catalog.is_offline(library.id) && !state.libraries.filter.borrow().spans_libraries();
    state.grid.offline.set_title(&format!(
        "{} is not connected — showing what Numa remembers. Stars and flags given now go in when it is back.",
        library.label()
    ));
    state.grid.offline.set_revealed(offline);

    state.libraries.scale.set(match state.libraries.filter.borrow().spans_libraries() {
        true => cull::Scale::default(),
        false => state.catalog.scale(library.id).unwrap_or_default(),
    });

    let timed = std::time::Instant::now();
    RELOADED.set(Some(timed));
    let filter = state.libraries.filter.borrow().clone();
    let photos = match filter.spans_libraries() {
        true => state.catalog.photos_everywhere(&filter),
        false => state.catalog.photos(library.id, &filter),
    };
    let mut photos = match photos {
        Ok(photos) => photos,
        Err(err) => {
            state.toast(&format!("Could not read the catalog: {err}"));
            return;
        }
    };

    let across_libraries = filter.spans_libraries();
    refresh_folders(state, &library, if across_libraries { &[] } else { &photos });
    if let (Some(folder), false) = (state.libraries.folder.borrow().clone(), across_libraries) {
        let root = library.path.join(folder);
        photos.retain(|photo| photo.path.starts_with(&root));
    }

    words::keep_reading(state);
    words::narrow(state, &mut photos);
    state.grid.empty.set_text(words::empty_text(state).unwrap_or("No photos match this filter."));
    state.grid.empty.set_visible(photos.is_empty());

    *state.grid.order.borrow_mut() = photos.iter().map(|photo| photo.id).collect();

    let edge = grid_edge(state);
    let aspects = aspects_of(state, &photos, edge);
    let aspects_at = timed.elapsed();

    let with_bursts: std::collections::HashSet<i64> =
        photos.iter().filter(|photo| photo.best_of_burst).map(|photo| numa::io::catalog::library_of(photo.id)).collect();
    *state.grid.bursts.borrow_mut() = with_bursts
        .into_iter()
        .flat_map(|library| {
            let sizes = state.catalog.burst_sizes(library).unwrap_or_default();
            sizes.into_iter().map(move |(burst, size)| ((library, burst), size))
        })
        .collect();

    *state.grid.lazy.borrow_mut() = photos
        .iter()
        .map(|photo| LazyThumb::new(photo.id, photo.path.clone(), photo.mtime, photo.edited, edge))
        .collect();
    *state.grid.cards.borrow_mut() = photos.into_iter().map(|photo| (photo.id, photo)).collect();
    state.grid.wall.fill(aspects);
    if timing() {
        eprintln!(
            "x-data: reload_grid {} cards: query+aspects {aspects_at:?}, cards {:?}",
            state.grid.wall.len(),
            timed.elapsed() - aspects_at
        );
    }

    if let Some(show) = state.libraries.show_filter.borrow().as_ref() {
        show();
    }

    sweep_thumbnails(state);

    rapid::refresh(state);
}

fn aspects_of(state: &App, photos: &[Photo], edge: u32) -> Vec<f32> {
    let found: Vec<(f32, bool)> = {
        use rayon::prelude::*;
        photos
            .par_iter()
            .map(|photo| match photo.aspect {
                Some(aspect) => (aspect, false),
                None => numa::io::thumbs::cached_size(&photo.path, photo.mtime, edge)
                    .or_else(|| numa::io::thumbs::cached_size(&photo.path, photo.mtime, GRID_THUMB_EDGE))
                    .map_or((justified::UNKNOWN_ASPECT, false), |(width, height)| {
                        (width as f32 / height.max(1) as f32, true)
                    }),
            })
            .collect()
    };
    let learnt: Vec<(i64, f32)> =
        photos.iter().zip(&found).filter(|(_, (_, read))| *read).map(|(photo, (aspect, _))| (photo.id, *aspect)).collect();
    if !learnt.is_empty() {
        if let Err(err) = state.catalog.set_aspects(&learnt) {
            log::warn!("could not keep {} photographs' shapes: {err}", learnt.len());
        }
    }
    found.into_iter().map(|(aspect, _)| aspect).collect()
}

thread_local! {

    static RELOADED: Cell<Option<std::time::Instant>> = const { Cell::new(None) };
}

pub(super) fn grid_shown(first: usize, last: usize, count: usize) {
    if let Some(reloaded) = RELOADED.take().filter(|_| timing()) {
        eprintln!(
            "timing: grid shown, cards {first}..={last} of {count}, {:.0} ms after the reload, {:.0} ms after start",
            reloaded.elapsed().as_secs_f64() * 1000.0,
            since_start_ms()
        );
    }
}

#[derive(Clone)]
pub(super) struct State {

    pub(super) wall: Justified,
    pub(super) empty: gtk::Label,

    pub(super) welcome: adw::StatusPage,

    pub(super) cards: Rc<RefCell<HashMap<i64, Photo>>>,

    pub(super) order: Rc<RefCell<Vec<i64>>>,

    pub(super) lazy: Rc<RefCell<Vec<LazyThumb>>>,

    pub(super) learnt: Rc<RefCell<Vec<(i64, f32)>>>,

    pub(super) scroller: gtk::ScrolledWindow,

    pub(super) stale: Rc<Cell<bool>>,

    pub(super) thumbnail_generation: Rc<Cell<u64>>,

    pub(super) thumbnail_watch: Rc<Cell<bool>>,

    pub(super) offline: adw::Banner,

    pub(super) bursts: Rc<RefCell<HashMap<(i64, i64), u32>>>,

    pub(super) header_end: gtk::Box,

    pub(super) card_banner: adw::Banner,
    pub(super) extras_banner: adw::Banner,
    pub(super) update_banner: adw::Banner,

    pub(super) clock_offer: gtk::Revealer,
}

impl State {
    pub(super) fn new(empty: gtk::Label) -> Self {
        Self {
            wall: Justified::default(),
            empty: empty,
            welcome: adw::StatusPage::new(),
            cards: Rc::new(RefCell::new(HashMap::new())),
            order: Rc::new(RefCell::new(Vec::new())),
            lazy: Rc::new(RefCell::new(Vec::new())),
            learnt: Rc::default(),
            scroller: gtk::ScrolledWindow::new(),
            stale: Rc::new(Cell::new(false)),
            thumbnail_generation: Rc::new(Cell::new(0)),
            thumbnail_watch: Rc::new(Cell::new(false)),
            offline: adw::Banner::new(""),
            bursts: Rc::default(),
            header_end: gtk::Box::new(gtk::Orientation::Horizontal, 6),
            card_banner: adw::Banner::new(""),
            extras_banner: adw::Banner::new(""),
            update_banner: adw::Banner::new(""),
            clock_offer: gtk::Revealer::new(),
        }
    }
}
