use super::*;

const MOST: usize = 12;

const GAP: i32 = 12;

struct Frame {
    at: usize,

    n: usize,
    frame: gtk::Overlay,
    picture: gtk::Picture,
    mark: gtk::Label,
    note: gtk::Label,
}

#[derive(Clone)]
pub(super) struct State {
    grid: gtk::Grid,
    frames: Rc<RefCell<Vec<Frame>>>,

    shown: Rc<RefCell<Option<(Vec<usize>, usize)>>>,

    one: Rc<Cell<bool>>,

    pill: gtk::Label,
}

impl State {
    pub(super) fn new() -> Self {
        Self {
            grid: gtk::Grid::new(),
            frames: Rc::default(),
            shown: Rc::default(),
            one: Rc::new(Cell::new(false)),
            pill: gtk::Label::new(None),
        }
    }
}

pub(super) fn build(state: &App) -> gtk::Grid {
    let grid = state.loupe.group.grid.clone();
    grid.add_css_class("loupe-group");
    grid.set_row_spacing(GAP as u32);
    grid.set_column_spacing(GAP as u32);
    grid.set_row_homogeneous(true);
    grid.set_column_homogeneous(true);
    grid.set_margin_start(16);
    grid.set_margin_end(16);
    grid.set_margin_top(16);
    grid.set_margin_bottom(16);
    grid.set_visible(false);
    grid
}

pub(super) fn build_pill(state: &App) -> gtk::Label {
    let pill = state.loupe.group.pill.clone();
    pill.add_css_class("photo-pill");
    pill.set_halign(gtk::Align::Start);
    pill.set_valign(gtk::Align::Start);
    pill.set_margin_start(16);
    pill.set_margin_top(16);
    pill.set_visible(false);
    pill
}

fn members(state: &App, at: usize) -> Option<Vec<usize>> {

    let scope = state.loupe.scope.borrow().clone();
    let all: Vec<usize> = match scope.contains(&at) {
        true => scope,
        false => burst_run(state, at).collect(),
    };
    if all.len() < 2 {
        return None;
    }
    let here = all.iter().position(|index| *index == at)?;
    let start = here / MOST * MOST;
    Some(all[start..(start + MOST).min(all.len())].to_vec())
}

pub(super) fn active(state: &App) -> bool {
    state.loupe.group.shown.borrow().is_some()
}

pub(super) fn wanted(state: &App) -> Vec<usize> {
    state.loupe.group.shown.borrow().as_ref().map_or_else(Vec::new, |(members, _)| members.clone())
}

pub(super) fn toggle_one(state: &App) {
    let Some(at) = state.loupe.at.get() else { return };
    if members(state, at).is_none() {
        return;
    }
    state.loupe.group.one.set(!state.loupe.group.one.get());
    if state.loupe.group.one.get() {
        loupe_zoom::reset(state);
    }
    refresh(state);
    paint(state);
}

pub(super) fn reset(state: &App) {
    state.loupe.group.one.set(false);
    state.loupe.group.shown.replace(None);
    state.loupe.group.grid.set_visible(false);
    state.loupe.group.pill.set_visible(false);
}

pub(super) fn refresh(state: &App) {
    let Some(at) = state.loupe.at.get() else { return };
    let group = &state.loupe.group;
    let wanted = members(state, at);

    let same = wanted.as_ref().is_some_and(|members| group.shown.borrow().as_ref().is_some_and(|(shown, _)| shown == members));
    if !same {
        group.one.set(false);
    }
    let side_by_side = wanted.filter(|_| !group.one.get() && !loupe_zoom::zoomed(state));
    match side_by_side {
        Some(members) => {
            let columns = columns(state, members.len(), aspect_of(state, members[0]));
            let fresh = group.shown.borrow().as_ref() != Some(&(members.clone(), columns));
            if fresh {
                lay_out(state, &members, columns);
                group.shown.replace(Some((members, columns)));
                paint(state);
            }
        }
        None => {
            group.shown.replace(None);
        }
    }
    let active = active(state);
    group.grid.set_visible(active);
    if let Some(photo) = state.loupe.stage.child() {
        photo.set_visible(!active);
    }
    for side in [&state.loupe.previous, &state.loupe.next] {
        side.set_visible(!active && state.loupe.neighbours.get());
    }
    let common = mark(state);
    say(state, at, common);
    state.loupe.stage.queue_allocate();
    state.loupe.picked_frame.queue_draw();
}

fn aspect_of(state: &App, at: usize) -> f64 {
    let id = id_at(state, at);
    id.and_then(|id| state.grid.cards.borrow().get(&id).and_then(|photo| photo.aspect)).map_or(1.5, |aspect| aspect as f64)
}

fn columns(state: &App, count: usize, aspect: f64) -> usize {
    let (width, height) = (state.loupe.stage.width() as f64 - 32.0, state.loupe.stage.height() as f64 - 32.0);
    if width <= 0.0 || height <= 0.0 {
        return (count as f64).sqrt().ceil() as usize;
    }
    let gap = GAP as f64;
    let area = |columns: usize| {
        let rows = count.div_ceil(columns);
        let cell_w = (width - gap * (columns - 1) as f64) / columns as f64;
        let cell_h = (height - gap * (rows - 1) as f64) / rows as f64;
        let w = cell_w.min(cell_h * aspect);
        w * w / aspect
    };
    (1..=count).max_by(|a, b| area(*a).total_cmp(&area(*b))).unwrap_or(1)
}

fn lay_out(state: &App, members: &[usize], columns: usize) {
    let group = &state.loupe.group;
    while let Some(child) = group.grid.first_child() {
        group.grid.remove(&child);
    }
    let aspect = aspect_of(state, members[0]) as f32;
    let mut frames = Vec::new();
    for (n, &at) in members.iter().enumerate() {
        let picture = gtk::Picture::new();
        picture.set_content_fit(gtk::ContentFit::Cover);
        picture.set_can_shrink(true);
        let frame = gtk::Overlay::new();
        frame.set_child(Some(&picture));
        frame.set_overflow(gtk::Overflow::Hidden);
        let note = gtk::Label::new(None);
        let mark = gtk::Label::new(None);
        for (label, valign) in [(&note, gtk::Align::Start), (&mark, gtk::Align::End)] {
            label.add_css_class("card-badge");
            label.set_halign(gtk::Align::Start);
            label.set_valign(valign);
            label.set_can_target(false);
            frame.add_overlay(label);
        }

        let click = gtk::GestureClick::new();
        click.connect_pressed(glib::clone!(
            #[strong] state,
            move |_, presses, _, _| {
                go_to(&state, at);
                if presses == 2 {
                    toggle_one(&state);
                }
            }
        ));
        frame.add_controller(click);
        let shaped = gtk::AspectFrame::new(0.5, 0.5, aspect, false);
        shaped.set_child(Some(&frame));
        shaped.set_hexpand(true);
        shaped.set_vexpand(true);
        group.grid.attach(&shaped, (n % columns) as i32, (n / columns) as i32, 1, 1);
        frames.push(Frame { at, n: n + 1, frame, picture, mark, note });
    }
    group.frames.replace(frames);
}

fn mark(state: &App) -> Option<String> {
    let at = state.loupe.at.get();
    let scale = state.libraries.scale.get();
    let cards = state.grid.cards.borrow();
    let frames = state.loupe.group.frames.borrow();
    let first_word = |at: usize| {
        id_at(state, at)
            .and_then(|id| cards.get(&id))
            .and_then(|photo| numa::io::notes::issues(photo, &scale).into_iter().next())
            .map(|issue| sentence(issue.split(" \u{b7} ").next().unwrap_or_default()))
    };
    let words: Vec<Option<String>> = frames.iter().map(|frame| first_word(frame.at)).collect();
    let common = words.first().cloned().flatten().filter(|word| words.iter().all(|other| other.as_deref() == Some(word.as_str())));
    for (frame, word) in frames.iter().zip(words) {
        match Some(frame.at) == at {
            true => frame.frame.add_css_class("current"),
            false => frame.frame.remove_css_class("current"),
        }
        let Some(photo) = id_at(state, frame.at).and_then(|id| cards.get(&id)) else { continue };
        match photo.flag == Flag::Rejected {
            true => frame.frame.add_css_class("rejected"),
            false => frame.frame.remove_css_class("rejected"),
        }
        let mut marks = vec![frame.n.to_string()];
        match photo.flag {
            Flag::Picked => marks.push("\u{2691}".to_string()),
            Flag::Rejected => marks.push("\u{2715}".to_string()),
            Flag::None => {}
        }
        if photo.rating > 0 {
            marks.push(format!("\u{2605}{}", photo.rating));
        }
        frame.mark.set_text(&marks.join("\u{2002}"));

        let word = word.filter(|_| common.is_none());
        let word = word.or_else(|| photo.best_of_burst.then(|| if photo.face_sharpness.is_some() { "Sharpest face" } else { "Sharpest" }.to_string()));
        frame.note.set_visible(word.is_some());
        frame.note.set_text(word.as_deref().unwrap_or_default());
    }
    common
}

fn say(state: &App, at: usize, common: Option<String>) {
    let pill = &state.loupe.group.pill;
    if active(state) {
        let count = state.loupe.group.frames.borrow().len();
        state.loupe.group.grid.set_margin_top(if common.is_some() { 56 } else { 16 });
        pill.set_visible(common.is_some());
        if let Some(word) = common {
            pill.set_text(&format!("{word} \u{b7} all {count}"));
            pill.set_tooltip_text(Some("Numa saw this in every frame of the burst"));
        }
        return;
    }
    let scale = state.libraries.scale.get();
    let mut said: Vec<String> = {
        let cards = state.grid.cards.borrow();
        id_at(state, at).and_then(|id| cards.get(&id)).map(|photo| numa::io::notes::issues(photo, &scale)).unwrap_or_default()
    };
    said.extend(echo_note(state, at));
    let Some(first) = said.first() else {
        pill.set_visible(false);
        return;
    };
    let more = match said.len() {
        1 => String::new(),
        n => format!("  +{}", n - 1),
    };
    pill.set_text(&format!("{}{more}", sentence(first)));
    pill.set_tooltip_text(Some(&said.iter().map(|line| sentence(line)).collect::<Vec<_>>().join("\n")));
    pill.set_visible(true);
}

fn sentence(text: &str) -> String {
    let mut chars = text.chars();
    chars.next().map_or(String::new(), |first| first.to_uppercase().chain(chars).collect())
}

pub(super) fn paint(state: &App) {
    let textures = state.loupe.textures.borrow();
    for frame in state.loupe.group.frames.borrow().iter() {
        let texture = id_at(state, frame.at).and_then(|id| textures.get(&id).cloned());
        let now = frame.picture.paintable().and_then(|paintable| paintable.downcast::<gtk::gdk::Texture>().ok());
        if now != texture {
            frame.picture.set_paintable(texture.as_ref());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::sentence;

    #[test]
    fn a_note_starts_a_sentence() {
        assert_eq!(sentence("soft · slow shutter"), "Soft · slow shutter");
        assert_eq!(sentence("eyes closed?"), "Eyes closed?");
        assert_eq!(sentence(""), "");
    }
}
