use super::*;

pub(super) fn reload_grid(state: &App) {

    close_loupe(state);

    state.catalog.remember(GRID_FILTER, &*state.libraries.filter.borrow());

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

    state.libraries.scale.set(match state.libraries.filter.borrow().spans_libraries() {
        true => cull::Scale::default(),
        false => state.catalog.scale(library.id).unwrap_or_default(),
    });

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
    state.grid.empty.set_text("No photos match this filter.");
    state.grid.empty.set_visible(photos.is_empty());

    *state.grid.order.borrow_mut() = photos.iter().map(|photo| photo.id).collect();

    let edge = grid_edge(state);
    let aspects: Vec<f32> = {
        use rayon::prelude::*;
        photos
            .par_iter()
            .map(|photo| {
                numa::io::thumbs::cached_size(&photo.path, photo.mtime, edge)
                    .or_else(|| numa::io::thumbs::cached_size(&photo.path, photo.mtime, GRID_THUMB_EDGE))
                    .map_or(justified::UNKNOWN_ASPECT, |(width, height)| {
                        width as f32 / height as f32
                    })
            })
            .collect()
    };
    for (photo, aspect) in photos.iter().zip(aspects) {
        state.grid.wall.append(&build_card(state, photo), aspect);
    }

    let state = state.clone();
    state.grid.scroller.clone().add_tick_callback(move |_, _| {
        sweep_thumbnails(&state);
        glib::ControlFlow::Break
    });
}

#[derive(Clone)]
pub(super) struct State {

    pub(super) wall: Justified,
    pub(super) empty: gtk::Label,

    pub(super) welcome: adw::StatusPage,

    pub(super) cards: Rc<RefCell<HashMap<i64, (Photo, gtk::Label)>>>,

    pub(super) order: Rc<RefCell<Vec<i64>>>,

    pub(super) lazy: Rc<RefCell<Vec<LazyThumb>>>,

    pub(super) scroller: gtk::ScrolledWindow,

    pub(super) stale: Rc<Cell<bool>>,

    pub(super) thumbnail_generation: Rc<Cell<u64>>,

    pub(super) thumbnail_watch: Rc<Cell<bool>>,
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
            scroller: gtk::ScrolledWindow::new(),
            stale: Rc::new(Cell::new(false)),
            thumbnail_generation: Rc::new(Cell::new(0)),
            thumbnail_watch: Rc::new(Cell::new(false)),
        }
    }
}
