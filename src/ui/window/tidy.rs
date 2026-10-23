use super::*;
use numa::io::tidy::{self as shared, Card, Ledger};

const FRAME: i32 = 200;

thread_local! {

    static FILLED: Cell<u64> = const { Cell::new(0) };

    static OPEN: Cell<(Option<i64>, Option<&'static str>)> = const { Cell::new((None, None)) };

    static COMPARING: Cell<bool> = const { Cell::new(false) };
}

struct Shelved {
    libraries: Vec<Library>,
    photos: HashMap<i64, Vec<Photo>>,
    scales: HashMap<i64, cull::Scale>,
}

impl shared::Shelf for Shelved {
    fn library(&self, id: i64) -> Option<Library> {
        self.libraries.iter().find(|library| library.id == id).cloned()
    }
    fn photos(&self, library: &Library) -> Vec<Photo> {
        self.photos.get(&library.id).cloned().unwrap_or_default()
    }
    fn scale(&self, library: &Library) -> cull::Scale {
        self.scales.get(&library.id).copied().unwrap_or_default()
    }
}

struct Found {
    cards: Vec<Card>,

    twins: HashMap<i64, Vec<(i64, PathBuf)>>,

    pending: usize,
}

fn look(state: &App, compare: bool) -> impl std::future::Future<Output = Option<Found>> {
    let libraries: Vec<Library> = state
        .libraries
        .all
        .borrow()
        .iter()
        .filter(|library| !home_sync::is_home(state, library.id) && !state.catalog.is_offline(library.id) && library.path.is_dir())
        .cloned()
        .collect();
    let newest = Filter { reversed: true, ..Filter::default() };
    let (mut photos, mut twins, mut scales) = (HashMap::new(), HashMap::new(), HashMap::new());
    for library in &libraries {
        let every = state.catalog.photos(library.id, &newest).unwrap_or_default();
        let paired = numa::io::analysis::twins(every.iter().map(|photo| (photo.id, photo.path.as_path(), photo.taken)), numa::io::raw::is_raw);
        for photo in every.iter().filter(|photo| paired.contains_key(&photo.id)) {
            twins.entry(paired[&photo.id]).or_insert_with(Vec::new).push((photo.id, photo.path.clone()));
        }
        scales.insert(library.id, shared::scale(&every));
        photos.insert(library.id, every.into_iter().filter(|photo| !paired.contains_key(&photo.id)).collect::<Vec<_>>());
    }
    let (settled, mut known) = (state.catalog.settled(), state.catalog.hashes());
    let shelf = Shelved { libraries, photos, scales };
    let state = state.clone();
    async move {
        let worked = gio::spawn_blocking(move || {
            let mut made = Vec::new();
            let every = shelf.photos.values().flatten().cloned();
            let (groups, pending) = shared::copies(every, &mut known, |photo, size| {
                if !compare {
                    return shared::Hash::Later;
                }
                let hashed = shared::hash_file(photo);
                if let shared::Hash::Made(hash) = &hashed {
                    made.push((photo.path.to_string_lossy().into_owned(), photo.mtime, size, hash.clone()));
                }
                hashed
            });
            let by_id: HashMap<i64, &Photo> = shelf.photos.values().flatten().map(|photo| (photo.id, photo)).collect();
            let copies: Vec<Vec<Photo>> = groups.iter().map(|ids| ids.iter().filter_map(|id| by_id.get(id).map(|photo| (*photo).clone())).collect()).collect();
            let cards = shared::cards(&shelf, &shelf.libraries, &copies, &settled, None, None);
            (cards, pending, made)
        })
        .await;
        let (cards, pending, made) = worked.ok()?;
        for (path, mtime, size, hash) in &made {
            state.catalog.note_hash(path, *mtime, *size, hash);
        }
        Some(Found { cards, twins, pending })
    }
}

pub(super) async fn waiting(state: &App) -> usize {
    look(state, false).await.map_or(0, |found| found.cards.len())
}

pub(super) fn show(state: &App) {
    OPEN.set((None, None));
    fill(state);
    state.stack.set_visible_child_name("tidy");
}

pub(super) fn show_library(state: &App, library: i64) {
    OPEN.set((Some(library), None));
    fill(state);
    state.stack.set_visible_child_name("tidy");
}

fn fill(state: &App) {
    let column = &state.libraries.tidy;
    clear(column);
    let filled = FILLED.get() + 1;
    FILLED.set(filled);
    let (library, _) = OPEN.get();
    let library = library.and_then(|id| state.libraries.all.borrow().iter().find(|one| one.id == id).cloned());
    column.append(&match &library {
        Some(library) => library_head(state, library),
        None => head(),
    });
    let waiting = gtk::Label::new(Some("Looking…"));
    waiting.set_xalign(0.0);
    waiting.add_css_class("dim-label");
    column.append(&waiting);
    let state = state.clone();
    glib::spawn_future_local(async move {
        let found = look(&state, false).await;
        if FILLED.get() != filled {
            return;
        }
        column_remove(&state, &waiting);
        let Some(found) = found else { return };
        let column = state.libraries.tidy.clone();
        if found.pending > 0 {
            let line = gtk::Label::new(Some(&format!(
                "Comparing {} files of the same size byte for byte; exact copies among them show here when that is done.",
                places::grouped(found.pending as i64)
            )));
            line.set_xalign(0.0);
            line.set_wrap(true);
            line.add_css_class("caption");
            line.add_css_class("dim-label");
            column.append(&line);
            compare(&state);
        }
        match library {
            Some(library) => library_sets(&state, &column, &library, found),
            None => libraries(&state, &column, &found),
        }
    });
}

fn column_remove(state: &App, widget: &impl IsA<gtk::Widget>) {
    if widget.parent().as_ref() == Some(state.libraries.tidy.upcast_ref()) {
        state.libraries.tidy.remove(widget);
    }
}

fn compare(state: &App) {
    if COMPARING.replace(true) {
        return;
    }
    let state = state.clone();
    glib::spawn_future_local(async move {
        look(&state, true).await;
        COMPARING.set(false);
        if state.stack.visible_child_name().as_deref() == Some("tidy") {
            fill(&state);
        }
    });
}

const ABOUT: &str = "Photographs you may not need, in small sets. Nothing goes without your choice, and what goes waits in your desktop's trash, where it can be put back.";

fn head() -> gtk::Box {
    let head = gtk::Box::new(gtk::Orientation::Vertical, 4);
    let title = gtk::Label::new(Some("Tidy Up"));
    title.set_xalign(0.0);
    title.add_css_class("title-2");
    head.append(&title);
    head.append(&body(ABOUT));
    head.set_margin_bottom(12);
    head
}

fn body(text: &str) -> gtk::Label {
    let line = gtk::Label::new(Some(text));
    line.set_xalign(0.0);
    line.set_wrap(true);
    line.set_max_width_chars(90);
    line
}

fn library_head(state: &App, library: &Library) -> gtk::Box {
    let back = gtk::Button::from_icon_name("go-previous-symbolic");
    back.add_css_class("flat");
    back.add_css_class("circular");
    back.set_valign(gtk::Align::Center);
    back.set_tooltip_text(Some("Tidy Up"));
    back.connect_clicked(glib::clone!(
        #[strong] state,
        move |_| show(&state)
    ));
    let title = gtk::Label::new(Some(&library.label()));
    title.set_xalign(0.0);
    title.add_css_class("title-2");
    let caption = gtk::Label::new(Some("Tidy Up"));
    caption.set_xalign(0.0);
    caption.add_css_class("caption");
    caption.add_css_class("dim-label");
    let titles = gtk::Box::new(gtk::Orientation::Vertical, 0);
    titles.append(&title);
    titles.append(&caption);
    let line = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    line.append(&back);
    line.append(&titles);
    let head = gtk::Box::new(gtk::Orientation::Vertical, 12);
    head.append(&line);
    head.append(&body(ABOUT));
    head.set_margin_bottom(12);
    head
}

fn nothing(text: &str) -> adw::StatusPage {
    let empty = adw::StatusPage::new();
    empty.set_title("Nothing to Tidy");
    empty.set_description(Some(text));
    empty
}

fn libraries(state: &App, column: &gtk::Box, found: &Found) {
    let waits = shared::per_library(found.cards.iter().map(|card| card.photos.iter().map(|photo| photo.id).collect()));
    if waits.is_empty() {
        column.append(&nothing("Numa found nothing to choose. It looks again each time Tidy Up opens."));
        return;
    }
    let shots: HashMap<i64, &Photo> = found.cards.iter().flat_map(|card| card.photos.iter()).map(|photo| (photo.id, photo)).collect();
    let flow = gtk::FlowBox::new();
    flow.set_selection_mode(gtk::SelectionMode::None);
    flow.set_homogeneous(true);
    flow.set_column_spacing(12);
    flow.set_row_spacing(12);
    flow.set_max_children_per_line(12);
    for place in state.libraries.places.borrow().iter() {
        let Place::Library(library) = place else { continue };
        let Some(waiting) = waits.get(&library.id) else { continue };
        let covers: Vec<(PathBuf, i64, Option<String>)> = waiting
            .covers
            .iter()
            .filter_map(|id| shots.get(id))
            .map(|photo| (photo.path.clone(), photo.mtime, state.catalog.edits_json(photo.id).ok().flatten()))
            .collect();
        let stack = today::stack(&library.label(), &format!("{} to tidy", places::photos(waiting.photos)), &covers);
        let id = library.id;
        stack.connect_clicked(glib::clone!(
            #[strong] state,
            move |_| show_library(&state, id)
        ));
        flow.append(&stack);
    }
    column.append(&flow);
}

fn library_sets(state: &App, column: &gtk::Box, library: &Library, found: Found) {
    let (_, kind) = OPEN.get();
    let twins = Rc::new(found.twins);
    let cards: Vec<Card> = found.cards.into_iter().filter(|card| card.photos.iter().any(|photo| numa::io::catalog::library_of(photo.id) == library.id)).collect();
    let chips = chip_row();
    chips.set_margin_bottom(12);
    let mut first: Option<gtk::ToggleButton> = None;
    let counts: Vec<(Option<&'static str>, String, usize)> = std::iter::once((None, "All".to_string(), cards.len()))
        .chain(shared::KINDS.iter().filter(|(key, ..)| *key != "who").map(|(key, title, _)| (Some(*key), title.to_string(), cards.iter().filter(|card| card.kind == *key).count())))
        .filter(|(key, _, count)| key.is_none() || *count > 0 || kind == *key)
        .collect();
    for (key, title, count) in counts {
        let chip = gtk::ToggleButton::with_label(&format!("{title}  {}", places::grouped(count as i64)));
        chip.set_active(kind == key);
        chip.set_focus_on_click(false);
        match &first {
            Some(first) => chip.set_group(Some(first)),
            None => first = Some(chip.clone()),
        }
        chip.connect_toggled(glib::clone!(
            #[strong] state,
            move |chip| {
                if chip.is_active() && OPEN.get().1 != key {
                    OPEN.set((OPEN.get().0, key));
                    fill(&state);
                }
            }
        ));
        chips.append(&chip);
    }
    column.append(&chips);
    let shown: Vec<Card> = cards.into_iter().filter(|card| kind.is_none_or(|kind| card.kind == kind)).collect();
    if shown.is_empty() {
        column.append(&nothing(match kind {
            Some(_) => "Nothing of this kind is waiting.",
            None => "Numa found nothing to choose here. It looks again each time Tidy Up opens.",
        }));
        return;
    }
    let sets = gtk::Box::new(gtk::Orientation::Vertical, 32);
    for card in shown {
        sets.append(&set(state, Rc::new(card), twins.clone()));
    }
    column.append(&sets);
}

fn going_words(card: &Card, keep: &std::collections::BTreeSet<i64>) -> Option<String> {
    let all = card.photos.len();
    let going = all - keep.len();
    let suggested = keep.len() == card.keep.len() && card.keep.iter().all(|id| keep.contains(id));
    match going {
        0 => None,
        1 if going == all => Some("Move to Trash".to_string()),
        _ if going == all => Some(format!("Move {going} to Trash")),
        _ if suggested && !card.keep.is_empty() => Some("Keep Best".to_string()),
        _ => Some(format!("Keep {}, Move {going} to Trash", keep.len())),
    }
}

fn set(state: &App, card: Rc<Card>, twins: Rc<HashMap<i64, Vec<(i64, PathBuf)>>>) -> gtk::Box {
    let (_, title, _) = shared::KINDS.iter().find(|(key, ..)| *key == card.kind).copied().unwrap_or(("", "", ""));
    let box_ = gtk::Box::new(gtk::Orientation::Vertical, 8);
    let label = section_header(&format!("{title} \u{b7} {}", card.photos.len()));
    label.set_margin_top(0);
    box_.append(&label);
    box_.append(&body(&card.why));
    let keep = Rc::new(RefCell::new(card.keep.iter().copied().collect::<std::collections::BTreeSet<i64>>()));
    let frames = gtk::FlowBox::new();
    frames.add_css_class("tidy-frames");
    frames.set_selection_mode(gtk::SelectionMode::None);
    frames.set_column_spacing(8);
    frames.set_row_spacing(8);
    frames.set_max_children_per_line(12);
    let go = primary_button("");
    let all = gtk::Button::with_label("Keep All");
    let said = gtk::Label::new(None);
    said.add_css_class("dim-label");
    said.set_wrap(true);
    said.set_max_width_chars(50);
    let paint = Rc::new(glib::clone!(
        #[strong] card,
        #[strong] keep,
        #[weak] go,
        move || {
            let words = going_words(&card, &keep.borrow());
            go.set_visible(words.is_some());
            go.set_label(&words.unwrap_or_default());
        }
    ));
    let ids: Vec<i64> = card.photos.iter().map(|photo| photo.id).collect();
    for (at, photo) in card.photos.iter().enumerate() {
        frames.append(&frame(state, &card, photo, at, &ids, &keep, &paint));
    }
    box_.append(&frames);
    let foot = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    foot.append(&go);
    foot.append(&all);
    foot.append(&said);
    box_.append(&foot);
    paint();
    go.connect_clicked(glib::clone!(
        #[strong] state,
        #[strong] card,
        #[strong] keep,
        #[strong] twins,
        #[weak] box_,
        #[weak] said,
        move |_| {
            let keep: Vec<i64> = keep.borrow().iter().copied().collect();
            act(&state, &card, &keep, &twins, &box_, &said);
        }
    ));
    all.connect_clicked(glib::clone!(
        #[strong] state,
        #[strong] card,
        #[strong] twins,
        #[weak] box_,
        #[weak] said,
        move |_| {
            let keep: Vec<i64> = card.photos.iter().map(|photo| photo.id).collect();
            act(&state, &card, &keep, &twins, &box_, &said);
        }
    ));
    box_
}

fn frame(
    state: &App,
    card: &Card,
    photo: &Photo,
    at: usize,
    ids: &[i64],
    keep: &Rc<RefCell<std::collections::BTreeSet<i64>>>,
    paint: &Rc<impl Fn() + 'static>,
) -> gtk::Box {
    let width = (FRAME as f32 * photo.aspect.unwrap_or(1.5).clamp(0.5, 2.5)) as i32;
    let picture = gtk::Picture::new();
    picture.set_content_fit(gtk::ContentFit::Cover);
    picture.set_size_request(width, FRAME);
    let edits = state.catalog.edits_json(photo.id).ok().flatten();
    let (wanted, shown) = (picture.downgrade(), picture.downgrade());
    crate::ui::thumbnail::load_thumbnail_while(
        &photo.path,
        photo.mtime,
        GRID_THUMB_EDGE,
        edits,
        None,
        move || wanted.upgrade().is_some(),
        move |texture| {
            if let Some(picture) = shown.upgrade() {
                picture.set_paintable(Some(&texture));
            }
        },
    );
    let over = gtk::Overlay::new();
    over.set_child(Some(&picture));
    let tick = gtk::Image::from_icon_name("object-select-symbolic");
    tick.add_css_class("chosen-tick");
    tick.set_halign(gtk::Align::Start);
    tick.set_valign(gtk::Align::Start);
    over.add_overlay(&tick);
    if card.keep.first() == Some(&photo.id) {
        let best = gtk::Label::new(Some("Numa's Pick"));
        best.add_css_class("tidy-best");
        best.set_halign(gtk::Align::Start);
        best.set_valign(gtk::Align::End);
        over.add_overlay(&best);
    }
    let pick = gtk::ToggleButton::new();
    pick.add_css_class("tidy-frame");
    pick.set_child(Some(&over));
    let kept = keep.borrow().contains(&photo.id);
    pick.set_active(kept);
    tick.set_visible(kept);
    pick.set_tooltip_text(Some(&format!("Keep this one, {} of {}", at + 1, ids.len())));
    let id = photo.id;
    pick.connect_toggled(glib::clone!(
        #[strong] keep,
        #[strong] paint,
        #[weak] tick,
        move |pick| {
            match pick.is_active() {
                true => keep.borrow_mut().insert(id),
                false => keep.borrow_mut().remove(&id),
            };
            tick.set_visible(pick.is_active());
            paint();
        }
    ));

    let look = gtk::Button::from_icon_name("view-fullscreen-symbolic");
    look.add_css_class("osd");
    look.add_css_class("circular");
    look.set_halign(gtk::Align::End);
    look.set_valign(gtk::Align::Start);
    look.set_margin_top(8);
    look.set_margin_end(8);
    look.set_tooltip_text(Some("Look Closer"));
    let ids = ids.to_vec();
    look.connect_clicked(glib::clone!(
        #[strong] state,
        move |_| today::open_these(&state, &ids, at)
    ));
    let lifted = gtk::Overlay::new();
    lifted.set_child(Some(&pick));
    lifted.add_overlay(&look);
    let mut notes = card.notes.get(&photo.id).cloned().unwrap_or_default();
    if let (Some(stars), "bursts") = (photo.suggested, card.kind) {
        notes.push(format!("~{stars:.1} stars"));
    }
    let caption = gtk::Label::new(Some(&notes.join(" \u{b7} ")));
    caption.set_visible(!notes.is_empty());
    caption.set_xalign(0.0);
    caption.set_wrap(true);
    caption.set_max_width_chars(1);
    caption.add_css_class("caption");
    let file = gtk::Label::new(photo.path.file_name().map(|name| name.to_string_lossy().into_owned()).as_deref());
    file.set_xalign(0.0);
    file.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
    file.add_css_class("caption");
    file.add_css_class("dim-label");
    file.set_tooltip_text(Some(&photo.path.to_string_lossy()));
    let frame = gtk::Box::new(gtk::Orientation::Vertical, 4);
    frame.set_size_request(width, -1);
    frame.append(&lifted);
    frame.append(&caption);
    frame.append(&file);
    frame
}

fn act(state: &App, card: &Card, keep: &[i64], twins: &HashMap<i64, Vec<(i64, PathBuf)>>, set: &gtk::Box, said: &gtk::Label) {
    let going = shared::going(card, keep);
    let mut doomed: Vec<(i64, PathBuf)> = card.photos.iter().filter(|photo| going.contains(&photo.id)).map(|photo| (photo.id, photo.path.clone())).collect();
    doomed.extend(going.iter().filter_map(|id| twins.get(id)).flatten().cloned());
    if doomed.is_empty() {
        state.catalog.settle(&card.settle);
        state.toast("Kept; not asked again");
        drop_set(state, set);
        return;
    }
    if refused_offline(state, doomed.iter().map(|(id, _)| *id), "moving to the trash") {
        return;
    }
    let (state, settle, set, said, count) = (state.clone(), card.settle.clone(), set.clone(), said.clone(), going.len());
    glib::spawn_future_local(async move {
        let paths: Vec<PathBuf> = doomed.iter().map(|(_, path)| path.clone()).collect();
        let Ok(results) = busy(&state, "Moving them to the trash…", move || {
            paths.iter().map(|path| gio::File::for_path(path).trash(gio::Cancellable::NONE).map_err(|err| err.to_string())).collect::<Vec<_>>()
        })
        .await
        else {
            return;
        };
        let mut failed = Vec::new();
        for ((id, path), result) in doomed.iter().zip(results) {
            match result {
                Ok(()) => {
                    if let Err(err) = state.catalog.remove_photo(*id) {
                        log::warn!("{}: {err}", path.display());
                    }
                }
                Err(err) => failed.push(format!("{}: {err}", path.display())),
            }
        }
        state.grid.stale.set(true);
        if let [first, ..] = failed.as_slice() {
            failed.iter().for_each(|failure| log::warn!("{failure}"));
            said.set_text(&format!("Could not move to the trash — {first}"));
            return;
        }
        state.catalog.settle(&settle);
        state.toast(&format!("Moved {} to Trash", places::photos(count)));
        drop_set(&state, &set);
    });
}

fn drop_set(state: &App, set: &gtk::Box) {
    let Some(sets) = set.parent().and_downcast::<gtk::Box>() else { return };
    sets.remove(set);
    if sets.first_child().is_none() {
        fill(state);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card(photos: usize, keep: &[i64]) -> Card {
        let photo = |id: i64| Photo {
            id, path: PathBuf::from(format!("/p/{id}.raf")), mtime: 0, taken: None, rating: 0, flag: Flag::None, sharpness: None, blown: None,
            best_of_burst: false, burst: None, echo: None, exposure: None, focal35: None, raw_clipped: None, raw_dark: None, eyes_closed: None,
            faces: None, face_sharpness: None, brightness: None, contrast: None, suggested: None, edited: false, aspect: None,
        };
        Card {
            key: String::new(), kind: "bursts", photos: (1..=photos as i64).map(photo).collect(), keep: keep.to_vec(), why: String::new(),
            settle: Vec::new(), notes: HashMap::new(),
        }
    }

    #[test]
    fn the_going_button_says_what_numa_home_says() {
        let run = card(4, &[2]);
        let kept = |ids: &[i64]| ids.iter().copied().collect::<std::collections::BTreeSet<i64>>();
        assert_eq!(going_words(&run, &kept(&[2])).as_deref(), Some("Keep Best"));
        assert_eq!(going_words(&run, &kept(&[2, 3])).as_deref(), Some("Keep 2, Move 2 to Trash"));
        assert_eq!(going_words(&run, &kept(&[])).as_deref(), Some("Move 4 to Trash"));
        assert_eq!(going_words(&run, &kept(&[1, 2, 3, 4])), None, "all kept: only Keep All");
        let unusable = card(1, &[]);
        assert_eq!(going_words(&unusable, &kept(&[])).as_deref(), Some("Move to Trash"));
    }
}
