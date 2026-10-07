use super::*;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;

const WHOLE_FRAME_PROXY: bool = false;

pub(super) type Prepared = (Arc<LinearImage>, (u32, u32), Option<raw::Summary>, bool);

const RECENT: usize = 3;

#[derive(Clone, PartialEq)]
struct Key {
    path: PathBuf,
    mtime: i64,
    edge: u32,
    automatic: dcp::Automatic,

    profiles: u64,
}

pub(super) type Coloured = (Option<String>, render::RenderInputs, Arc<LinearImage>, Option<First>);

pub(super) type First = (u64, image::RgbImage, render::histogram::Histogram);

pub(super) type Decoded = Result<(Prepared, Option<Coloured>), String>;

#[derive(Clone, Default)]
pub(super) struct State {

    slot: Rc<RefCell<Option<Ahead>>>,

    backward: Rc<Cell<bool>>,

    after: Rc<Cell<Option<u64>>>,

    run: Rc<Cell<u32>>,
    stepped: Rc<Cell<bool>>,

    recent: Rc<RefCell<VecDeque<(Key, Prepared)>>>,

    first: Rc<RefCell<Option<(u64, First)>>>,
}

struct Ahead {
    path: PathBuf,
    mtime: i64,
    edge: u32,
    decoded: mpsc::Receiver<Decoded>,

    stop: Option<Arc<AtomicBool>>,
    started: std::time::Instant,
}

impl Drop for Ahead {
    fn drop(&mut self) {
        let Some(stop) = &self.stop else { return };
        stop.store(true, Ordering::Relaxed);
        if timing() {
            let finished = !matches!(self.decoded.try_recv(), Err(mpsc::TryRecvError::Empty));
            log::info!(
                "ahead: {} let go {} ms after it started, {}",
                self.path.file_name().unwrap_or_default().to_string_lossy(),
                self.started.elapsed().as_millis(),
                if finished { "decoded for nothing" } else { "stopped" }
            );
        }
    }
}

pub(super) fn heading(state: &App, forward: bool) {
    stepping(&state.render.prefetch, forward);
}

fn stepping(ahead: &State, forward: bool) {
    let same = ahead.backward.get() != forward;
    ahead.run.set(if same { ahead.run.get() + 1 } else { 1 });
    ahead.backward.set(!forward);
    ahead.stepped.set(true);
}

pub(super) fn opening(state: &App) {
    opened(&state.render.prefetch);
}

fn opened(ahead: &State) {
    if !ahead.stepped.replace(false) {
        ahead.run.set(0);
    }
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
    let Some(photo) = next.and_then(|id| state.grid.cards.borrow().get(&id).cloned()) else { return };
    let edge = proxy_edge(state);

    if kept(ahead, &key(&photo.path, photo.mtime, edge)) {
        return;
    }

    if numa::core::power::frugal() && ahead.run.get() < 2 {
        return;
    }

    let mut slot = ahead.slot.borrow_mut();
    if slot.as_ref().is_some_and(|held| held.path == photo.path && held.mtime == photo.mtime && held.edge == edge) {
        return;
    }

    let render_ahead = !numa::core::power::frugal() && !card_render::card_may_render();

    let edits = state.catalog.edits_json(photo.id).ok().flatten();
    let document = match &edits {
        Some(json) => serde_json::from_str::<Document>(json).ok(),
        None => Some(Document::new(photo.path.to_string_lossy().to_string())),
    }
    .filter(|document| document.ai_denoise <= 0.0 && document.ai_sharpen <= 0.0);

    if let Some(held) = slot.take() {
        let_go(ahead, held);
    }
    let (sender, decoded) = mpsc::channel();
    let stop = Arc::new(AtomicBool::new(false));
    *slot = Some(Ahead {
        path: photo.path.clone(),
        mtime: photo.mtime,
        edge,
        decoded,
        stop: Some(stop.clone()),
        started: std::time::Instant::now(),
    });
    let path = photo.path;
    std::thread::spawn(move || {
        let decoded = numa::core::power::background(|| {
            numa::core::power::stoppable(Some(stop), || {
                let prepared = prepare(&path, edge)?;
                raw::stopped()?;
                let Some(document) = document else { return Ok((prepared, None)) };
                let inputs = render_inputs(&document);
                let working = Arc::new(render::to_working_space(&document, &*prepared.0, &inputs));
                raw::stopped()?;
                let first = (render_ahead && document.masks().is_empty()).then(|| first_frame(&document, &working, &prepared));
                Ok((prepared, Some((edits, inputs, working, first))))
            })
        });

        let _ = sender.send(decoded);
        give_back_freed_memory();
    });
}

pub(super) fn take(ahead: &State, path: &Path, mtime: i64, edge: u32) -> Option<(mpsc::Receiver<Decoded>, bool)> {
    let mut slot = ahead.slot.borrow_mut();
    if let Some(held) = slot.take_if(|held| held.path != path || held.mtime != mtime || held.edge != edge) {
        let_go(ahead, held);
    }

    if let Some(prepared) = recall(ahead, &key(path, mtime, edge)) {
        let (sender, decoded) = mpsc::channel();
        sender.send(Ok((prepared, None))).ok()?;
        return Some((decoded, true));
    }
    let mut held = slot.take()?;
    held.stop = None;
    if timing() {
        log::info!("ahead: {} opened {} ms after it started", path.file_name().unwrap_or_default().to_string_lossy(), held.started.elapsed().as_millis());
    }

    let decoded = std::mem::replace(&mut held.decoded, mpsc::channel().1);
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

fn let_go(ahead: &State, mut held: Ahead) {
    let Ok(Ok((prepared, _))) = held.decoded.try_recv() else { return };
    held.stop = None;
    let key = key(&held.path, held.mtime, held.edge);
    let mut recent = ahead.recent.borrow_mut();
    if !recent.iter().any(|(kept, _)| *kept == key) {
        recent.push_back((key, prepared));
        recent.truncate(RECENT);
    }
}

pub(super) fn remember(ahead: &State, path: &Path, mtime: i64, edge: u32, prepared: &Prepared) {
    let key = key(path, mtime, edge);
    let mut recent = ahead.recent.borrow_mut();
    recent.retain(|(held, _)| *held != key);
    recent.push_front((key, prepared.clone()));
    recent.truncate(RECENT);
}

fn kept(ahead: &State, key: &Key) -> bool {
    ahead.recent.borrow().iter().any(|(held, _)| held == key)
}

fn recall(ahead: &State, key: &Key) -> Option<Prepared> {
    let mut recent = ahead.recent.borrow_mut();
    let at = recent.iter().position(|(held, _)| held == key)?;
    let entry = recent.remove(at)?;
    let prepared = entry.1.clone();
    recent.push_front(entry);
    Some(prepared)
}

fn key(path: &Path, mtime: i64, edge: u32) -> Key {
    Key { path: path.to_path_buf(), mtime, edge, automatic: dcp::automatic(), profiles: dcp::generation() }
}

fn first_frame(document: &Document, working: &Arc<LinearImage>, (proxy, full_size, ..): &Prepared) -> First {
    let scale = proxy.width.max(proxy.height) as f32 / full_size.0.max(full_size.1).max(1) as f32;
    let frame = render::apply_stack_kept(document, working, scale);
    let histogram = render::histogram::of(&frame);
    (fingerprint(document, &colour_key(document)), frame, histogram)
}

pub(super) fn made_ahead(state: &App, generation: u64, first: Option<First>) {
    *state.render.prefetch.first.borrow_mut() = first.map(|first| (generation, first));
}

pub(super) fn has_first(state: &App) -> bool {
    state.render.prefetch.first.borrow().as_ref().is_some_and(|(opened, _)| *opened == state.open_generation.get())
}

pub(super) fn take_first(state: &App) -> Option<First> {
    let (opened, first) = state.render.prefetch.first.borrow_mut().take()?;
    (opened == state.open_generation.get()).then_some(first)
}

pub(super) fn forget(state: &App) {
    state.render.prefetch.slot.borrow_mut().take();
    state.render.prefetch.after.set(None);
    state.render.prefetch.first.borrow_mut().take();
}

pub(super) fn prepare(path: &Path, edge: u32) -> Result<Prepared, String> {
    let mut laps = raw::Laps::start();

    let (proxy, full_size) = match WHOLE_FRAME_PROXY {
        false => numa::io::previews::editor_proxy(path, edge)?,
        true => {
            let linear = raw::decode_for_editing(path)?;
            let full_size = (linear.width, linear.height);
            (Arc::new(linear.downscaled(edge).unwrap_or(linear)), full_size)
        }
    };
    laps.lap("decode");

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
        state.canvas.set_paintable(Some(&texture_from(&image)));
        if let Some(asked) = asked {
            log::info!("stand-in {:.0} ms after the photograph was asked for", asked.elapsed().as_secs_f32() * 1000.0);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn holding(path: &str, decoded: mpsc::Receiver<Decoded>) -> State {
        let ahead = State::default();
        let (stop, started) = (Some(Arc::new(AtomicBool::new(false))), std::time::Instant::now());
        *ahead.slot.borrow_mut() = Some(Ahead { path: PathBuf::from(path), mtime: 7, edge: 1920, decoded, stop, started });
        ahead
    }

    fn prepared(width: u32) -> Prepared {
        (Arc::new(LinearImage::new(width, 1, vec![0.0; width as usize * 3])), (width, 1), None, false)
    }

    #[test]
    fn the_last_three_opened_are_kept_and_the_oldest_goes() {
        let ahead = State::default();
        for (at, name) in ["/a.raf", "/b.raf", "/c.raf"].into_iter().enumerate() {
            remember(&ahead, Path::new(name), 7, 1920, &prepared(at as u32 + 1));
        }
        assert!(recall(&ahead, &key(Path::new("/a.raf"), 8, 1920)).is_none(), "the file changed");
        assert!(recall(&ahead, &key(Path::new("/a.raf"), 7, 1280)).is_none(), "another proxy size");
        let (proxy, ..) = recall(&ahead, &key(Path::new("/a.raf"), 7, 1920)).expect("kept");
        assert_eq!(proxy.width, 1, "its own decode");

        remember(&ahead, Path::new("/d.raf"), 7, 1920, &prepared(4));
        assert!(recall(&ahead, &key(Path::new("/b.raf"), 7, 1920)).is_none());
        for kept in ["/a.raf", "/c.raf", "/d.raf"] {
            assert!(recall(&ahead, &key(Path::new(kept), 7, 1920)).is_some(), "{kept}");
        }
        let (decoded, ready) = take(&ahead, Path::new("/c.raf"), 7, 1920).expect("taken from the kept ones");
        assert!(ready);
        assert!(decoded.recv().unwrap().is_ok());
    }

    #[test]
    fn the_decode_ahead_is_taken_only_by_its_own_photograph() {
        let stop_of = |ahead: &State| ahead.slot.borrow().as_ref().and_then(|held| held.stop.clone()).unwrap();
        for (path, mtime, edge) in [("/b.raf", 7, 1920), ("/a.raf", 8, 1920), ("/a.raf", 7, 1280)] {
            let (_sender, decoded) = mpsc::channel();
            let ahead = holding("/a.raf", decoded);
            let stop = stop_of(&ahead);
            assert!(take(&ahead, Path::new(path), mtime, edge).is_none());
            assert!(stop.load(Ordering::Relaxed) && ahead.slot.borrow().is_none(), "let go and stopped");
        }

        let (sender, decoded) = mpsc::channel();
        let ahead = holding("/a.raf", decoded);
        let stop = stop_of(&ahead);
        let (decoded, ready) = take(&ahead, Path::new("/a.raf"), 7, 1920).expect("its own photograph");
        assert!(!ready, "still running");
        assert!(!stop.load(Ordering::Relaxed), "taken, not stopped");
        assert!(take(&ahead, Path::new("/a.raf"), 7, 1920).is_none(), "taken once");
        sender.send(Err("done".into())).unwrap();
        assert_eq!(decoded.recv().unwrap().err().as_deref(), Some("done"));

        let (sender, decoded) = mpsc::channel();
        sender.send(Err("finished".into())).unwrap();
        let (decoded, ready) = take(&holding("/a.raf", decoded), Path::new("/a.raf"), 7, 1920).unwrap();
        assert!(ready);
        assert_eq!(decoded.recv().unwrap().err().as_deref(), Some("finished"));

        let (sender, decoded) = mpsc::channel();
        sender.send(Ok((prepared(5), None))).unwrap();
        let ahead = holding("/a.raf", decoded);
        assert!(take(&ahead, Path::new("/b.raf"), 7, 1920).is_none());
        assert!(kept(&ahead, &key(Path::new("/a.raf"), 7, 1920)), "kept for coming back");

        let (sender, decoded) = mpsc::channel::<Decoded>();
        drop(sender);
        assert!(take(&holding("/a.raf", decoded), Path::new("/a.raf"), 7, 1920).is_none());
    }

    #[test]
    fn a_run_of_steps_is_counted_until_a_jump() {
        let ahead = State::default();
        let step = |forward| {
            stepping(&ahead, forward);
            opened(&ahead);
        };
        step(true);
        step(true);
        assert_eq!(ahead.run.get(), 2);
        step(false);
        assert_eq!(ahead.run.get(), 1);
        opened(&ahead);
        assert_eq!(ahead.run.get(), 0, "a jump");
    }
}
