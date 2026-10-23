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

fn spot_at(path: &Path) -> Option<Spot> {
    let (latitude, longitude) = numa::io::exif::position(path)?;
    let at = (latitude as f32, longitude as f32);
    Some(Spot { at, town: numa::io::places::nearest(at) })
}

pub(super) fn now(path: &Path) -> Option<Spot> {
    spot_at(path)
}

pub(super) fn read(photos: &[Photo], done: impl FnOnce() + 'static) -> bool {
    let wanted: Vec<(i64, i64, PathBuf)> = FOUND.with(|found| {
        let found = found.borrow();
        photos
            .iter()
            .filter(|photo| found.get(&photo.id).is_none_or(|(mtime, _)| *mtime != photo.mtime))
            .map(|photo| (photo.id, photo.mtime, photo.path.clone()))
            .collect()
    });
    if wanted.is_empty() {
        return false;
    }
    WAITING.with(|waiting| waiting.borrow_mut().push(Box::new(done)));
    if READING.replace(true) {
        return true;
    }
    glib::spawn_future_local(async move {
        let read = gio::spawn_blocking(move || {

            let mut named: HashMap<(u32, u32), Option<&'static Town>> = HashMap::new();
            wanted
                .into_iter()
                .map(|(id, mtime, path)| {
                    let spot = numa::io::exif::position(&path).map(|(latitude, longitude)| {
                        let at = (latitude as f32, longitude as f32);
                        Spot { at, town: *named.entry((at.0.to_bits(), at.1.to_bits())).or_insert_with(|| numa::io::places::nearest(at)) }
                    });
                    (id, (mtime, spot))
                })
                .collect::<Vec<_>>()
        })
        .await
        .unwrap_or_default();
        FOUND.with(|found| found.borrow_mut().extend(read));
        READING.set(false);
        for done in WAITING.with(|waiting| std::mem::take(&mut *waiting.borrow_mut())) {
            done();
        }
    });
    true
}
