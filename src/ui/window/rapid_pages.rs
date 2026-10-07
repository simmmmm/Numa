use super::rapid::{self, of};
use super::*;

#[derive(Default)]
struct Check {

    picks: Vec<(i64, usize)>,
    at: usize,
}

#[derive(Clone)]
pub(super) struct Pages {
    big: gtk::Picture,
    check_title: gtk::Label,
    strip: gtk::Box,
    check: Rc<RefCell<Check>>,
}

fn picture() -> gtk::Picture {
    let picture = gtk::Picture::new();
    picture.set_can_shrink(true);
    picture.set_content_fit(gtk::ContentFit::Contain);
    picture.set_hexpand(true);
    picture.set_vexpand(true);
    picture
}

impl Pages {
    pub(super) fn new() -> Self {
        let title = || {
            let label = gtk::Label::new(None);
            label.add_css_class("title-4");
            label.set_xalign(0.0);
            label
        };
        let strip = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        strip.set_halign(gtk::Align::Center);
        Self {
            big: picture(),
            check_title: title(),
            strip,
            check: Rc::default(),
        }
    }
}

fn pages(state: &App) -> &Pages {
    &of(state).pages
}

fn key_button(state: &App, label: &str, tip: &str, key: gtk::gdk::Key) -> gtk::Button {
    let button = gtk::Button::with_label(label);
    button.set_tooltip_text(Some(tip));
    button.connect_clicked(glib::clone!(
        #[strong] state,
        move |_| {
            check_key(&state, key);
        }
    ));
    button
}

fn page(head: &impl IsA<gtk::Widget>, body: &impl IsA<gtk::Widget>, keys: &[gtk::Button]) -> gtk::Box {
    let bar = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    bar.add_css_class("rapid-bar");
    head.set_hexpand(true);
    bar.append(head);
    for key in keys {
        bar.append(key);
    }
    let column = gtk::Box::new(gtk::Orientation::Vertical, 0);
    column.add_css_class("rapid-page");
    column.add_css_class("numa-content");
    column.append(&bar);
    column.append(body);
    column
}

pub(super) fn build(state: &App, views: &gtk::Stack) {
    use gtk::gdk::Key;
    let pages = pages(state);
    let body = gtk::Box::new(gtk::Orientation::Vertical, 12);
    body.set_margin_start(16);
    body.set_margin_end(16);
    body.set_margin_bottom(12);
    body.append(&pages.big);
    let strip = gtk::ScrolledWindow::new();
    strip.set_vscrollbar_policy(gtk::PolicyType::Never);
    strip.set_min_content_height(80);
    strip.set_child(Some(&pages.strip));
    body.append(&strip);
    let keys = [
        key_button(state, "Back", "The pick before (←)", Key::Left),
        key_button(state, "Unpick", "Out of the picks; it stays in its moment (X)", Key::x),
        key_button(state, "Edit", "This photograph in the editor; Back returns here (E)", Key::e),
        key_button(state, "Next", "The next pick, and Deliver after the last (→ or Space)", Key::Right),
        key_button(state, "Leave", "Back to Moments (Esc)", Key::Escape),
    ];
    views.add_named(&page(&pages.check_title, &body, &keys), Some("check"));
}

fn photo(state: &App, id: i64) -> Option<Photo> {
    state.grid.cards.borrow().get(&id).cloned()
}

fn show(state: &App, id: i64, picture: &gtk::Picture, edge: u32) {
    picture.set_paintable(None::<&gtk::gdk::Paintable>);
    if let Some(photo) = photo(state, id) {
        rapid::load(state, &photo, picture, edge);
    }
}

fn all_picks(state: &App) -> Vec<(i64, usize)> {
    let rapid = of(state);
    let moments = rapid.moments.borrow();
    moments.iter().enumerate().flat_map(|(m, moment)| rapid::picked(state, &moment.ids).into_iter().map(move |id| (id, m))).collect()
}

fn start_check(state: &App) {
    let picks = all_picks(state);
    if picks.is_empty() {
        state.toast("Nothing picked yet — P picks a frame");
        return;
    }
    *pages(state).check.borrow_mut() = Check { picks, at: 0 };
    of(state).views.set_visible_child_name("check");
    show_check(state);
}

pub(super) fn show_check(state: &App) {
    let pages = pages(state);
    let check = pages.check.borrow();
    let Some(&(id, m)) = check.picks.get(check.at) else { return };
    let time = of(state).moments.borrow().get(m).map(|moment| rapid::clock(moment.start())).unwrap_or_default();
    pages.check_title.set_text(&format!("Check · {} of {} · the moment at {time}", check.at + 1, check.picks.len()));
    show(state, id, &pages.big, 2048);
    while let Some(child) = pages.strip.first_child() {
        pages.strip.remove(&child);
    }
    for (at, (other, _)) in check.picks.iter().enumerate().filter(|(_, (_, of_moment))| *of_moment == m) {
        let aspect = photo(state, *other).and_then(|photo| photo.aspect).unwrap_or(1.5);
        let tile = rapid::tile(state, *other, None, Some(((64.0 * aspect) as i32, 64)));
        if *other == id {
            tile.add_css_class("focus");
        }
        let click = gtk::GestureClick::new();
        click.connect_pressed(glib::clone!(
            #[strong] state,
            move |_, _, _, _| {
                of(&state).pages.check.borrow_mut().at = at;
                show_check(&state);
            }
        ));
        tile.add_controller(click);
        pages.strip.append(&tile);
    }
}

pub(super) fn check_key(state: &App, key: gtk::gdk::Key) -> glib::Propagation {
    use gtk::gdk::Key;
    let pages = pages(state);
    let (count, at) = {
        let check = pages.check.borrow();
        (check.picks.len(), check.at)
    };
    match key {
        Key::Right | Key::space => {
            if at + 1 >= count {
                leave_check(state);
                deliver_checked(state, true);
                return glib::Propagation::Stop;
            }
            pages.check.borrow_mut().at += 1;
        }
        Key::Left => pages.check.borrow_mut().at = at.saturating_sub(1),
        Key::x | Key::X | Key::u | Key::U => {
            let Some((id, _)) = pages.check.borrow().picks.get(at).copied() else { return glib::Propagation::Stop };
            rapid::set_flags(state, &[id], Flag::None);
            state.toast("Unpicked — it stays in its moment");
            let mut check = pages.check.borrow_mut();
            check.picks.remove(at);
            if check.picks.is_empty() {
                drop(check);
                leave_check(state);
                return glib::Propagation::Stop;
            }
            check.at = at.min(check.picks.len() - 1);
        }
        Key::e | Key::E => {
            if let Some((id, _)) = pages.check.borrow().picks.get(at).copied() {
                open_photo(state, id);
            }
            return glib::Propagation::Stop;
        }
        Key::Escape => {
            leave_check(state);
            return glib::Propagation::Stop;
        }
        _ => return glib::Propagation::Proceed,
    }
    show_check(state);
    glib::Propagation::Stop
}

fn leave_check(state: &App) {
    of(state).views.set_visible_child_name("rapid");
    rapid::rebuild(state);
}

fn export(state: &App, ids: &[i64]) {
    export_in(state, ids, &HashMap::new());
}

fn export_in(state: &App, ids: &[i64], folders: &HashMap<i64, String>) {
    let placed: Vec<(i64, Option<String>)> = ids.iter().map(|id| (*id, folders.get(id).cloned())).collect();
    export_placed(state, &placed);
}

fn export_placed(state: &App, placed: &[(i64, Option<String>)]) {
    let jobs: Vec<ExportJob> = placed
        .iter()
        .filter_map(|(id, folder)| Some((photo(state, *id)?, folder.clone())))
        .map(|(photo, subfolder)| {
            let document = state.catalog.load_edits(photo.id).ok().flatten().unwrap_or_else(|| Document::new(photo.path.to_string_lossy().to_string()));
            ExportJob { source: Source::Photo { id: photo.id, path: photo.path.clone() }, document, to: None, subfolder }
        })
        .collect();
    export_dialog(state, &of(state).views, jobs);
}

fn by_person(state: &App, picks: &[i64]) -> Vec<(String, Vec<i64>)> {
    let Some(library) = rapid::library_of_view(state) else { return Vec::new() };
    state
        .catalog
        .people(library)
        .unwrap_or_default()
        .into_iter()
        .map(|(name, photos)| (name.replace(['/', '\\'], "-"), picks.iter().copied().filter(|id| photos.contains(id)).collect::<Vec<i64>>()))
        .filter(|(_, picked)| !picked.is_empty())
        .collect()
}

fn by_chapter(state: &App, picks: &[(i64, usize)]) -> (Vec<(String, usize)>, HashMap<i64, String>) {
    let chapters = rapid_chapters::of_moments(state);
    let mut order: Vec<(String, usize)> = Vec::new();
    let mut folders = HashMap::new();
    for (m, chapter) in chapters.iter().enumerate() {
        if let Some((name, true)) = chapter {
            order.push((format!("{:02} {}", order.len() + 1, name.replace(['/', '\\'], "-")), 0));
        }
        let Some(folder) = chapter.as_ref().and_then(|_| order.last_mut()) else { continue };
        for (id, _) in picks.iter().filter(|(_, at)| *at == m) {
            folder.1 += 1;
            folders.insert(*id, folder.0.clone());
        }
    }
    (order, folders)
}

pub(super) fn export_teaser(state: &App) {
    let teaser = rapid::kept_ids(state, "teaser");
    if teaser.is_empty() {
        state.toast("No teaser yet — in First Look, P puts a frame in it");
        return;
    }
    export(state, &teaser);
}

pub(super) fn deliver(state: &App) {
    deliver_checked(state, false);
}

fn deliver_checked(state: &App, checked: bool) {
    let picks: Vec<i64> = all_picks(state).into_iter().map(|(id, _)| id).collect();
    let teaser = rapid::kept_ids(state, "teaser");
    let (moments, bare) = {
        let all = of(state).moments.borrow();
        (all.len(), all.iter().filter(|moment| rapid::picked(state, &moment.ids).is_empty()).count())
    };
    let mut body = format!("{} picked across {moments} moments · {} in the teaser.", places::grouped(picks.len() as i64), teaser.len());
    if bare > 0 && !picks.is_empty() {
        body.push_str(&format!(" {bare} moment(s) have no pick."));
    }
    let (chapters, folders) = by_chapter(state, &all_picks(state));
    let alert = adw::AlertDialog::new(Some("Deliver"), Some(&body));
    let in_folders = gtk::Switch::new();
    let people = by_person(state, &picks);

    let directory = export_directory(state);
    let sent: Vec<Option<String>> = people.iter().map(|(name, _)| directory.as_ref().and_then(|directory| written_on(&directory.join(name)))).collect();
    if let Some(extra) = extras(&chapters, &people, &sent, &in_folders) {
        alert.set_extra_child(Some(&extra));
    }
    alert.add_response("cancel", "Cancel");
    if checked {
        alert.add_response("check", "Check Again");
    }
    alert.add_response("teaser", "Export Teaser…");

    let waiting: Vec<(String, Vec<i64>)> = people.iter().zip(&sent).filter(|(_, sent)| sent.is_none()).map(|(person, _)| person.clone()).collect();
    let people = if waiting.is_empty() { people } else { waiting.clone() };
    if !people.is_empty() {
        let label = match waiting.is_empty() {
            true => format!("Export for All {} People Again…", people.len()),
            false => format!("Export for {} People…", people.len()),
        };
        alert.add_response("people", &label);
    }
    alert.add_response("picks", &format!("Export {}…", picks_said(picks.len())));
    if !checked {
        alert.add_response("check", "Check the Picks First");
    }
    alert.set_response_enabled("check", !picks.is_empty());
    alert.set_response_enabled("teaser", !teaser.is_empty());
    alert.set_response_enabled("picks", !picks.is_empty());
    let way_on = if checked { "picks" } else { "check" };
    alert.set_response_appearance(way_on, adw::ResponseAppearance::Suggested);
    alert.set_default_response(Some(if picks.is_empty() { "cancel" } else { way_on }));
    let parent = of(state).views.clone();
    let state = state.clone();
    alert.connect_response(None, move |_, response| match response {
        "check" => start_check(&state),
        "teaser" => export(&state, &teaser),
        "people" => {
            let placed: Vec<(i64, Option<String>)> = people.iter().flat_map(|(name, ids)| ids.iter().map(move |id| (*id, Some(name.clone())))).collect();
            export_placed(&state, &placed);
        }
        "picks" => export_in(&state, &picks, &if in_folders.is_active() { folders.clone() } else { HashMap::new() }),
        _ => {}
    });
    alert.present(Some(&parent));
}

fn extras(chapters: &[(String, usize)], people: &[(String, Vec<i64>)], sent: &[Option<String>], in_folders: &gtk::Switch) -> Option<gtk::Box> {
    if chapters.is_empty() && people.is_empty() {
        return None;
    }
    let list = gtk::Box::new(gtk::Orientation::Vertical, 6);
    let line = |name: &str, about: &str| {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let label = gtk::Label::new(Some(name));
        label.set_hexpand(true);
        label.set_xalign(0.0);
        let about = gtk::Label::new(Some(about));
        about.add_css_class("dim-label");
        row.append(&label);
        row.append(&about);
        row
    };

    for (name, count) in chapters {
        list.append(&line(name, &format!("{count} picked")));
    }
    if !chapters.is_empty() {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        row.set_margin_top(8);
        row.set_margin_bottom(8);
        let label = gtk::Label::new(Some("A folder per chapter"));
        label.set_hexpand(true);
        label.set_xalign(0.0);
        in_folders.set_active(true);
        in_folders.set_valign(gtk::Align::Center);
        row.append(&label);
        row.append(in_folders);
        list.append(&row);
    }
    for ((name, ids), sent) in people.iter().zip(sent) {
        let about = match sent {
            Some(day) => format!("{} picked · written {day}", ids.len()),
            None => format!("{} picked", ids.len()),
        };
        list.append(&line(name, &about));
    }
    Some(list)
}

fn written_on(folder: &Path) -> Option<String> {
    let newest = std::fs::read_dir(folder).ok()?.filter_map(|entry| entry.ok()?.metadata().ok()?.modified().ok()).max()?;
    let seconds = newest.duration_since(std::time::UNIX_EPOCH).ok()?.as_secs() as i64;
    glib::DateTime::from_unix_local(seconds).ok()?.format("%-d %b").ok().map(|day| day.to_string())
}

fn picks_said(count: usize) -> String {
    match count {
        1 => "1 Pick".to_string(),
        n => format!("{} Picks", places::grouped(n as i64)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_person_has_theirs_when_their_folder_holds_a_file() {
        let folder = std::env::temp_dir().join(format!("numa-sent-{}", std::process::id()));
        assert_eq!(written_on(&folder), None, "no folder, nothing sent");
        std::fs::create_dir_all(&folder).unwrap();
        assert_eq!(written_on(&folder), None, "an empty folder is nothing sent");
        std::fs::write(folder.join("DSCF0413.jpg"), b"x").unwrap();
        assert!(written_on(&folder).is_some_and(|day| !day.is_empty()));
        std::fs::remove_dir_all(&folder).unwrap();
    }
}
