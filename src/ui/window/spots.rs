use super::*;
use numa::io::places::Place as Town;

#[derive(Clone, Copy)]
pub(super) struct Spot {
    pub(super) at: (f32, f32),
    pub(super) town: Option<&'static Town>,
}

thread_local! {

    static FOUND: RefCell<HashMap<i64, (i64, Option<Spot>)>> = RefCell::default();
    static READING: Cell<bool> = const { Cell::new(false) };

    static WAITING: RefCell<Vec<Box<dyn FnOnce()>>> = RefCell::default();
}

pub(super) fn spot_of(photo: &Photo) -> Option<Option<Spot>> {
    FOUND.with(|found| found.borrow().get(&photo.id).filter(|(mtime, _)| *mtime == photo.mtime).map(|(_, spot)| *spot))
}

pub(super) fn town_of(photo: &Photo) -> Option<&'static Town> {
    spot_of(photo).flatten().and_then(|spot| spot.town)
}

pub(super) fn named(town: &Town, name: &str) -> bool {
    [&town.name, &town.region, &town.country].iter().any(|own| own.eq_ignore_ascii_case(name))
}

pub(super) fn positions() -> Vec<(f32, f32)> {
    FOUND.with(|found| found.borrow().values().filter_map(|(_, spot)| spot.map(|spot| spot.at)).collect())
}

fn spot((latitude, longitude): (f64, f64), named: &mut HashMap<(u32, u32), Option<&'static Town>>) -> Spot {
    let at = (latitude as f32, longitude as f32);
    Spot { at, town: *named.entry((at.0.to_bits(), at.1.to_bits())).or_insert_with(|| numa::io::places::nearest(at)) }
}

pub(super) fn now(path: &Path) -> Option<Spot> {
    numa::io::exif::position(path).map(|at| spot(at, &mut HashMap::new()))
}

pub(super) fn read(state: &App, photos: &[Photo], done: impl FnOnce() + 'static) -> bool {
    let wanted: Vec<Photo> = FOUND.with(|found| {
        let found = found.borrow();
        photos.iter().filter(|photo| found.get(&photo.id).is_none_or(|(mtime, _)| *mtime != photo.mtime)).cloned().collect()
    });
    if wanted.is_empty() {
        return false;
    }
    let (at, unread) = state.catalog.where_taken(&wanted).unwrap_or_else(|err| {
        log::warn!("places: {err}");
        (HashMap::new(), wanted.clone())
    });
    let to_read: std::collections::HashSet<i64> = unread.iter().map(|photo| photo.id).collect();
    let mut named = HashMap::new();
    let kept: Vec<(i64, (i64, Option<Spot>))> = wanted
        .iter()
        .filter(|photo| !to_read.contains(&photo.id))
        .map(|photo| (photo.id, (photo.mtime, at.get(&photo.id).map(|&at| spot(at, &mut named)))))
        .collect();
    FOUND.with(|found| found.borrow_mut().extend(kept));
    if unread.is_empty() {
        return false;
    }
    WAITING.with(|waiting| waiting.borrow_mut().push(Box::new(done)));
    if READING.replace(true) {
        return true;
    }
    let catalog = state.catalog.clone();
    glib::spawn_future_local(async move {

        for batch in unread.chunks(1024) {
            let batch = batch.to_vec();
            let read = gio::spawn_blocking(move || numa::io::catalog::read_where(&batch)).await.unwrap_or_default();
            if let Err(err) = catalog.keep_where(&read) {
                log::warn!("places: {err}");
            }
            let mut named = HashMap::new();
            let spots: Vec<(i64, (i64, Option<Spot>))> = read.into_iter().map(|(id, mtime, at)| (id, (mtime, at.map(|at| spot(at, &mut named))))).collect();
            FOUND.with(|found| found.borrow_mut().extend(spots));
        }
        READING.set(false);
        for done in WAITING.with(|waiting| std::mem::take(&mut *waiting.borrow_mut())) {
            done();
        }
    });
    true
}
