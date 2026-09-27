use super::*;
use std::sync::mpsc;

pub(super) type Prepared = (LinearImage, (u32, u32), Option<raw::Summary>, bool);

#[derive(Clone, Default)]
pub(super) struct State {

    slot: Rc<RefCell<Option<Ahead>>>,

    backward: Rc<Cell<bool>>,

    after: Rc<Cell<Option<u64>>>,
}

struct Ahead {
    path: PathBuf,
    mtime: i64,
    edge: u32,
    decoded: mpsc::Receiver<Result<Prepared, String>>,
}

pub(super) fn heading(state: &App, forward: bool) {
    state.render.prefetch.backward.set(!forward);
}

pub(super) fn after_opening(state: &App, generation: u64) {
    state.render.prefetch.after.set(Some(generation));
}

pub(super) fn on_screen(state: &App, generation: u64) {
    let ahead = &state.render.prefetch;
    if ahead.after.get() != Some(generation) {
        return;
    }
    ahead.after.set(None);

    let current = state.open.borrow().as_ref().and_then(|photo| match &photo.source {
        Source::Photo { id, .. } => Some(*id),
        Source::Bracket { .. } => None,
    });
    let Some(current) = current else { return };
    let next = {
        let order = state.grid.order.borrow();
        let Some(at) = order.iter().position(|id| *id == current) else { return };
        let step = if ahead.backward.get() { at.checked_sub(1) } else { at.checked_add(1) };
        step.and_then(|step| order.get(step).copied())
    };
    let Some((photo, _)) = next.and_then(|id| state.grid.cards.borrow().get(&id).cloned()) else { return };
    let edge = proxy_edge(state);

    let mut slot = ahead.slot.borrow_mut();
    if slot.as_ref().is_some_and(|held| held.path == photo.path && held.mtime == photo.mtime && held.edge == edge) {
        return;
    }

    let (sender, decoded) = mpsc::channel();
    *slot = Some(Ahead { path: photo.path.clone(), mtime: photo.mtime, edge, decoded });
    let path = photo.path;
    std::thread::spawn(move || {

        let _ = sender.send(prepare(&path, edge));
        give_back_freed_memory();
    });
}

pub(super) fn take(ahead: &State, path: &Path, mtime: i64, edge: u32) -> Option<(mpsc::Receiver<Result<Prepared, String>>, bool)> {
    let mut slot = ahead.slot.borrow_mut();
    let held = slot.as_ref()?;
    if held.path != path || held.mtime != mtime || held.edge != edge {
        return None;
    }
    let decoded = slot.take()?.decoded;
    match decoded.try_recv() {

        Ok(prepared) => {
            let (sender, decoded) = mpsc::channel();
            sender.send(prepared).ok()?;
            Some((decoded, true))
        }
        Err(mpsc::TryRecvError::Empty) => Some((decoded, false)),
        Err(mpsc::TryRecvError::Disconnected) => None,
    }
}

pub(super) fn forget(state: &App) {
    state.render.prefetch.slot.borrow_mut().take();
    state.render.prefetch.after.set(None);
}

pub(super) fn prepare(path: &Path, edge: u32) -> Result<Prepared, String> {
    let mut laps = raw::Laps::start();
    let linear = raw::decode_for_editing(path)?;
    laps.lap("decode");

    let (full_size, proxy) = ((linear.width, linear.height), linear.downscaled(edge).unwrap_or(linear));
    laps.lap("downscale");

    let corrected = raw::lens_profile(path).is_some_and(|profile| profile.corrects_anything());
    laps.lap("lens again");
    let summary = raw::summary(path);
    laps.lap("summary");
    laps.report("prepare", path);
    Ok((proxy, full_size, summary, corrected))
}

pub(super) fn stand_in(state: &App, photo: &Photo, generation: u64) {
    let edits = photo.edited.then(|| state.catalog.edits_json(photo.id).ok().flatten()).flatten();

    if photo.edited && edits.is_none() {
        return;
    }
    let Some(file) = numa::io::thumbs::cached(&photo.path, photo.mtime, thumb_edge(state), edits.as_deref()) else {
        return;
    };
    let asked = timing().then(std::time::Instant::now);
    let state = state.clone();
    glib::spawn_future_local(async move {
        let Ok(Some(image)) = gio::spawn_blocking(move || image::open(&file).ok().map(|image| image.into_rgb8())).await else {
            return;
        };

        if state.open_generation.get() != generation || state.canvas.paintable().is_some() {
            return;
        }
        state.canvas.set_paintable(Some(&texture_from(image)));
        if let Some(asked) = asked {
            log::info!("stand-in {:.0} ms after the photograph was asked for", asked.elapsed().as_secs_f32() * 1000.0);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn holding(path: &str, decoded: mpsc::Receiver<Result<Prepared, String>>) -> State {
        let ahead = State::default();
        *ahead.slot.borrow_mut() = Some(Ahead { path: PathBuf::from(path), mtime: 7, edge: 1920, decoded });
        ahead
    }

    #[test]
    fn the_decode_ahead_is_taken_only_by_its_own_photograph() {
        let (sender, decoded) = mpsc::channel();
        let ahead = holding("/a.raf", decoded);
        assert!(take(&ahead, Path::new("/b.raf"), 7, 1920).is_none());
        assert!(take(&ahead, Path::new("/a.raf"), 8, 1920).is_none());
        assert!(take(&ahead, Path::new("/a.raf"), 7, 1280).is_none());

        let (decoded, ready) = take(&ahead, Path::new("/a.raf"), 7, 1920).expect("its own photograph");
        assert!(!ready, "still running");
        assert!(take(&ahead, Path::new("/a.raf"), 7, 1920).is_none(), "taken once");
        sender.send(Err("done".into())).unwrap();
        assert_eq!(decoded.recv().unwrap().err().as_deref(), Some("done"));

        let (sender, decoded) = mpsc::channel();
        sender.send(Err("finished".into())).unwrap();
        let (decoded, ready) = take(&holding("/a.raf", decoded), Path::new("/a.raf"), 7, 1920).unwrap();
        assert!(ready);
        assert_eq!(decoded.recv().unwrap().err().as_deref(), Some("finished"));

        let (sender, decoded) = mpsc::channel::<Result<Prepared, String>>();
        drop(sender);
        assert!(take(&holding("/a.raf", decoded), Path::new("/a.raf"), 7, 1920).is_none());
    }
}
