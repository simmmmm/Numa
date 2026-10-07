use super::import_done::{name_of, place_label, recheck_row, run_checked, Kept, Run};
use super::*;
use numa::io::import::Place;
use numa::io::offload::{self, Fault, Item, Older, Problem, Report, Target};
use numa::io::receipt;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;

const CARD_PACE: f64 = 300e6;

pub(super) struct Done {
    run: Run,
    report: Report,
    receipts: Vec<PathBuf>,
    kept: Kept,

    older: Vec<(PathBuf, PathBuf, u64)>,
    checked: Option<Older>,
}

impl Done {
    pub(super) fn new(run: Run, report: Report, receipts: Vec<PathBuf>, kept: Kept) -> Self {
        let mut older = run.older.clone();
        older.extend(report.existing.iter().cloned());
        Self { run, report, receipts, kept, older, checked: None }
    }

    fn safe(&self) -> bool {
        self.report.safe_to_format() && (self.older.is_empty() || self.checked.as_ref().is_some_and(Older::safe))
    }

    fn problems(&self) -> Vec<&Problem> {
        self.report.problems.iter().chain(self.checked.iter().flat_map(|checked| &checked.problems)).collect()
    }
}

#[derive(Clone)]
struct Page {
    state: App,
    dialog: adw::Dialog,
    view: adw::ToolbarView,
    bar: gtk::Box,
    done: Rc<RefCell<Done>>,
}

pub(super) fn done_dialog(state: &App, done: Done) {
    let Some(window) = state.stack.root().and_downcast::<adw::ApplicationWindow>() else { return };
    let dialog = adw::Dialog::new();
    dialog.set_title("Import");
    dialog.set_content_width(560);
    dialog.set_content_height(712);
    let view = adw::ToolbarView::new();
    view.add_top_bar(&adw::HeaderBar::new());
    let bar = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    bar.set_halign(gtk::Align::Center);
    view.add_bottom_bar(&bar);
    dialog.set_child(Some(&view));
    let page = Page { state: state.clone(), dialog: dialog.clone(), view, bar, done: Rc::new(RefCell::new(done)) };
    show(&page);
    dialog.present(Some(&window));
}

fn show(page: &Page) {
    let done = page.done.borrow();
    let content = adw::PreferencesPage::new();
    let (safe, title, lines) = verdict(&page.state, &done);
    content.add(&headline(safe, title, &lines));
    content.add(&checked_list(page, &done));
    let problems = done.problems();
    if !problems.is_empty() {
        content.add(&problems_group(&problems));
    }
    let later = adw::PreferencesGroup::new();
    later.set_title("Later");
    let again_row = recheck_row(&page.state);
    again_row.set_title("Check These Copies Again");
    later.add(&again_row);
    content.add(&later);
    page.view.set_content(Some(&content));
    while let Some(child) = page.bar.first_child() {
        page.bar.remove(&child);
    }
    page.bar.append(&done_buttons(page, &done));
}

fn count(n: usize, one: &str, many: &str) -> String {
    format!("{} {}", places::grouped(n as i64), if n == 1 { one } else { many })
}

fn holding(state: &App, older: &[(PathBuf, PathBuf, u64)]) -> String {
    let mut names: Vec<String> = Vec::new();
    for library in state.libraries.all.borrow().iter() {
        if older.iter().any(|(_, file, _)| file.starts_with(&library.path)) {
            names.push(library.label());
        }
    }
    match names.len() {
        0 => "a library".to_string(),
        1..=3 => names.join(" and "),
        more => format!("{} and {} more", names[..2].join(", "), more - 2),
    }
}

fn verdict(state: &App, done: &Done) -> (bool, &'static str, Vec<String>) {
    let (report, safe) = (&done.report, done.safe());
    let unreadable: Vec<&Problem> = report.problems.iter().filter(|p| matches!(p.fault, Fault::Unreadable(_))).collect();
    let copies_wrong = report.problems.len() - unreadable.len();
    let older_wrong = done.checked.as_ref().map_or(0, |checked| checked.problems.len());
    let raws = report.landed.iter().filter(|landed| raw::is_raw(&landed.to)).count();
    let photographs = count(report.landed.len(), "photograph", "photographs");
    let (title, mut lines): (&str, Vec<String>) = if safe {
        let mut lines = vec![format!("{photographs} on two drives, both read back.")];
        if raws > 0 {
            lines.push("Every raw on the card opened without a fault.".into());
        }
        if !done.older.is_empty() {
            let one = done.older.len() == 1;
            lines.push(format!(
                "The {} on the card {} {}.",
                count(done.older.len(), "older frame", "older frames"),
                if one { "matches" } else { "match" },
                if one { "its copy" } else { "their copies" }
            ));
        }
        ("Safe to Format", lines)
    } else if !report.problems.is_empty() || report.stopped || older_wrong > 0 {
        let mut lines = Vec::new();
        match unreadable.as_slice() {
            [] => {}
            [one] => lines.push(format!("{} could not be read \u{2014} reshoot while you can.", name_of(&one.to))),
            many => lines.push(format!("{} could not be read \u{2014} reshoot while you can.", count(many.len(), "frame", "frames"))),
        }
        if copies_wrong > 0 {
            lines.push(format!("{} did not come out the same as the card. The card still has them.", count(copies_wrong, "copy", "copies")));
        }
        if older_wrong > 0 {
            let one = older_wrong == 1;
            lines.push(format!(
                "{} on the card {} not match {} copies \u{2014} keep the card.",
                count(older_wrong, "older frame", "older frames"),
                if one { "does" } else { "do" },
                if one { "its" } else { "their" }
            ));
        }
        if report.stopped {
            lines.push("Stopped before every photograph was copied and checked.".into());
        }
        ("Not Safe to Format", lines)
    } else if !report.check {
        ("Copied", vec![format!("{photographs} copied, not read back, so not safe to format.")])
    } else if !report.second {
        ("Copied and Checked", vec!["Add a second copy to be safe to format.".into()])
    } else {
        ("Copied and Checked", vec![format!("{photographs} on two drives, both read back.")])
    };
    if !done.older.is_empty() && done.checked.is_none() {
        let n = done.older.len();
        lines.push(format!(
            "{} on the card {} already in {}: check {} too to be safe to format.",
            count(n, "older frame", "older frames"),
            if n == 1 { "was" } else { "were" },
            holding(state, &done.older),
            if n == 1 { "it" } else { "them" }
        ));
    }
    if done.kept.rated > 0 {
        lines.push(format!("{} rated in the camera.", places::grouped(done.kept.rated as i64)));
    }
    lines.extend(import_jpegs::said(&done.kept));
    if done.kept.placed > 0 {
        lines.push(format!("{} placed from the track.", places::grouped(done.kept.placed as i64)));
    }
    (safe, title, lines)
}

fn checked_list(page: &Page, done: &Done) -> adw::PreferencesGroup {
    let (run, report) = (&done.run, &done.report);
    let list = adw::PreferencesGroup::new();
    let mut roots: Vec<PathBuf> = Vec::new();
    for landed in &report.landed {
        if let Some(root) = landed.to.parent().filter(|root| !roots.iter().any(|known| known == root)) {
            roots.push(root.to_path_buf());
        }
    }
    let verb = if report.check { "checked" } else { "copied" };
    let wrong_in = |library: bool| {
        report.problems.iter().any(|p| match p.fault {
            Fault::NotCopied(_) | Fault::Differs => library,
            Fault::SecondNotCopied(_) | Fault::SecondDiffers => !library,
            _ => false,
        })
    };
    for root in &roots {
        let here = report.landed.iter().filter(|landed| landed.to.parent() == Some(root.as_path())).count();
        list.add(&status_row("Library", &format!("{} \u{b7} {} {verb}", root.display(), places::grouped(here as i64)), report.check && !wrong_in(true)));
    }
    if let Some(second) = &run.second {
        let there = report.landed.iter().filter(|landed| landed.second.is_some()).count();
        let all = there == report.landed.len() && !wrong_in(false);
        list.add(&status_row("Second Copy", &format!("{} \u{b7} {} {verb}", place_label(second), places::grouped(there as i64)), report.check && all));
    }
    let raws = report.landed.iter().filter(|landed| raw::is_raw(&landed.to)).count();
    let unreadable = report.problems.iter().filter(|p| matches!(p.fault, Fault::Unreadable(_))).count();
    let card_said = match unreadable {
        0 if report.decoded == raws => format!("{} \u{b7} every raw decoded \u{b7} 0 problems", run.card_name),
        0 => format!("{} \u{b7} {} of {} raws decoded", run.card_name, report.decoded, raws),
        problems => format!("{} \u{b7} {}", run.card_name, count(problems, "frame could not be read", "frames could not be read")),
    };
    list.add(&status_row("The Card", &card_said, unreadable == 0 && report.decoded == raws));
    if !done.older.is_empty() {
        list.add(&older_row(page, done));
    }
    if let Some(first) = done.receipts.first().cloned() {
        let row = adw::ActionRow::new();
        row.set_title("Receipt");
        row.set_subtitle("Kept in the library, with every file's checksum");
        let show = gtk::Button::with_label("Show");
        show.set_valign(gtk::Align::Center);
        show.set_tooltip_text(Some("Show the receipt in the file manager. xxh128sum -c checks it."));
        let dialog = page.dialog.clone();
        show.connect_clicked(move |_| {
            let window = dialog.root().and_downcast::<gtk::Window>();
            gtk::FileLauncher::new(Some(&gio::File::for_path(&first))).open_containing_folder(window.as_ref(), None::<&gio::Cancellable>, |result| {
                if let Err(err) = result {
                    log::warn!("could not show the receipt: {err}");
                }
            });
        });
        row.add_suffix(&show);
        list.add(&row);
    }
    list
}

fn older_row(page: &Page, done: &Done) -> adw::ActionRow {
    let n = done.older.len();
    let row = adw::ActionRow::new();
    row.set_title(&count(n, "older frame", "older frames"));
    let held = format!("Already in {}", holding(&page.state, &done.older));
    match &done.checked {
        Some(checked) => {
            let mut said = format!("{held} \u{b7} {} checked", places::grouped(checked.checked.len() as i64));
            if !checked.problems.is_empty() {
                said.push_str(&format!(" \u{b7} {} not", places::grouped(checked.problems.len() as i64)));
            }
            set_row_subtitle(&row, &said);
            let mark = gtk::Image::from_icon_name(if checked.safe() { "object-select-symbolic" } else { "dialog-warning-symbolic" });
            if !checked.safe() {
                mark.add_css_class("dim-label");
            }
            row.add_suffix(&mark);
        }
        None => {
            let bytes: u64 = done.older.iter().map(|(_, _, size)| size).sum();
            let seconds = bytes as f64 / CARD_PACE;
            let cost = match seconds {
                s if s < 90.0 => format!("about {} s", s.round().max(1.0) as u64),
                s => format!("about {} min", (s / 60.0).round() as u64),
            };
            set_row_subtitle(&row, &format!("{held} \u{b7} {cost} to read them from the card"));
            let button = gtk::Button::with_label("Check Them Too");
            button.set_valign(gtk::Align::Center);
            button.set_tooltip_text(Some("Read each one off the card and its copies, and compare"));
            button.connect_clicked(glib::clone!(
                #[strong] page,
                #[weak] row,
                move |button| check_them(&page, &row, button)
            ));
            row.add_suffix(&button);
        }
    }
    row
}

fn check_them(page: &Page, row: &adw::ActionRow, button: &gtk::Button) {
    button.set_sensitive(false);
    let done = page.done.borrow();
    let pairs: Vec<(PathBuf, PathBuf)> = done.older.iter().map(|(card, file, _)| (card.clone(), file.clone())).collect();
    let (second, card_name) = (done.run.second.clone(), done.run.card_name.clone());
    drop(done);
    let libraries: Vec<PathBuf> = page.state.libraries.all.borrow().iter().map(|library| library.path.clone()).collect();
    let total = pairs.len();
    let counted = Arc::new(AtomicUsize::new(0));
    let stop = Arc::new(AtomicBool::new(false));
    page.dialog.connect_closed(glib::clone!(
        #[strong] stop,
        move |_| stop.store(true, Ordering::Relaxed)
    ));
    let row_ref = glib::SendWeakRef::from(row.downgrade());
    let when = glib::DateTime::now_local().ok();
    let stamp = when.as_ref().and_then(|now| now.format("%Y-%m-%d-%H%M").ok()).map_or_else(|| "check".into(), |text| text.to_string());
    let said = when.as_ref().and_then(|now| now.format("%-d %b %Y %H:%M").ok()).map(|text| text.to_string()).unwrap_or_default();
    let page = page.clone();
    glib::spawn_future_local(async move {
        let found = gio::spawn_blocking(move || {
            let step = || {
                let n = counted.fetch_add(1, Ordering::Relaxed) + 1;
                let row = row_ref.clone();
                glib::MainContext::default().invoke(move || {
                    if let Some(row) = row.upgrade() {
                        set_row_subtitle(&row, &format!("Checking {} of {}", places::grouped(n as i64), places::grouped(total as i64)));
                    }
                });
            };
            let older = offload::check_older(&pairs, second.as_deref(), &stop, step);

            let root_of = |file: &Path| {
                libraries
                    .iter()
                    .filter(|root| file.starts_with(root))
                    .max_by_key(|root| root.components().count())
                    .cloned()
                    .unwrap_or_else(|| file.parent().unwrap_or(file).to_path_buf())
            };
            let files: Vec<receipt::Entry> =
                older.checked.iter().map(|(hash, file, there)| (*hash, root_of(file), file.clone(), there.clone())).collect();
            let header = vec![
                format!("Numa check of older frames \u{b7} {said}"),
                format!("From {card_name} \u{b7} {} frames already in a library, each the same as the card", files.len()),
            ];
            let receipts = match files.is_empty() || older.stopped {
                true => Vec::new(),
                false => receipt::write_all(&files, second.as_deref(), &format!("{stamp}-older"), &header),
            };
            (older, receipts)
        })
        .await;
        let Ok((older, receipts)) = found else {
            page.state.toast("Checking the older frames stopped with an error");
            return;
        };
        if older.stopped {
            return;
        }
        log::info!("import: {} older frames checked, {} problems", older.checked.len(), older.problems.len());
        {
            let mut done = page.done.borrow_mut();
            done.receipts.extend(receipts);
            done.checked = Some(older);
        }
        show(&page);
    });
}

fn done_buttons(page: &Page, done: &Done) -> gtk::Box {
    let (state, dialog, run) = (&page.state, &page.dialog, done.run.clone());
    let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    buttons.set_halign(gtk::Align::Center);
    buttons.set_margin_top(12);
    buttons.set_margin_bottom(16);
    let finish = gtk::Button::with_label("Done");
    finish.add_css_class("pill");
    finish.connect_clicked(glib::clone!(
        #[weak] dialog,
        move |_| {
            dialog.close();
        }
    ));
    buttons.append(&finish);

    let again = done.report.again();
    if !again.is_empty() {
        let retry = primary_button("Try Again");
        retry.add_css_class("pill");
        retry.connect_clicked(glib::clone!(
            #[strong] state,
            #[weak] dialog,
            move |_| {
                if !run.card_root.is_dir() {
                    state.toast("Put the card back in to try again");
                    return;
                }
                dialog.close();
                let mut retry = run.clone();
                retry.places = libraries_of(&state, &again);
                retry.older.clear();
                run_checked(&state, again.clone(), retry);
            }
        ));
        buttons.append(&retry);
    } else if let Some(mount) = card_mount(&run.card_root) {
        let eject = primary_button("Eject Card");
        eject.add_css_class("pill");
        eject.connect_clicked(glib::clone!(
            #[strong] state,
            #[weak] dialog,
            move |_| {
                dialog.close();
                eject_card(&state, mount.clone());
            }
        ));
        buttons.append(&eject);
    } else {
        primary(&finish);
    }
    buttons
}

fn headline(safe: bool, title: &str, lines: &[String]) -> adw::PreferencesGroup {
    let column = gtk::Box::new(gtk::Orientation::Vertical, 8);
    column.set_margin_top(24);
    column.set_margin_bottom(12);
    let mark = gtk::Image::from_icon_name(if safe || title.starts_with("Copied") { "object-select-symbolic" } else { "dialog-warning-symbolic" });
    mark.set_halign(gtk::Align::Center);
    mark.set_valign(gtk::Align::Center);
    match safe {
        true => {
            mark.set_pixel_size(28);
            mark.add_css_class("import-safe");
        }
        false => {
            mark.set_pixel_size(40);
            mark.add_css_class("dim-label");
        }
    }
    column.append(&mark);
    let verdict = gtk::Label::new(Some(title));
    verdict.add_css_class("title-1");
    column.append(&verdict);
    let said = gtk::Label::new(Some(&lines.join("\n")));
    said.add_css_class("dim-label");
    said.set_justify(gtk::Justification::Center);
    said.set_wrap(true);
    column.append(&said);
    let group = adw::PreferencesGroup::new();
    group.add(&column);
    group
}

fn status_row(title: &str, subtitle: &str, good: bool) -> adw::ActionRow {
    let row = adw::ActionRow::new();
    row.set_title(title);
    set_row_subtitle(&row, subtitle);
    let mark = gtk::Image::from_icon_name(if good { "object-select-symbolic" } else { "dialog-warning-symbolic" });
    if !good {
        mark.add_css_class("dim-label");
    }
    mark.set_tooltip_text(Some(if good { "Checked" } else { "Not checked, or not all of it" }));
    row.add_suffix(&mark);
    row
}

fn problems_group(problems: &[&Problem]) -> adw::PreferencesGroup {
    const SHOWN: usize = 40;
    let group = adw::PreferencesGroup::new();
    group.set_title("Problems");
    for problem in problems.iter().take(SHOWN) {
        let row = adw::ActionRow::new();
        row.set_title(&glib::markup_escape_text(&name_of(&problem.to)));
        let said = match &problem.fault {
            Fault::Unreadable(_) => "Could not be read \u{2014} reshoot while you can".to_string(),
            Fault::Differs => "The library's copy is not the same as the card".to_string(),
            Fault::NotCopied(err) => format!("Could not be copied into the library: {err}"),
            Fault::SecondDiffers => "The second copy is not the same as the card".to_string(),
            Fault::SecondNotCopied(err) => format!("Could not be copied to the second place: {err}"),
            Fault::NoMatch => "No copy is the same as the card \u{2014} keep the card".to_string(),
            Fault::Unchecked(err) => format!("Could not be checked: {err}"),
        };
        set_row_subtitle(&row, &said);
        group.add(&row);
    }
    if problems.len() > SHOWN {
        let row = adw::ActionRow::new();
        row.set_title(&format!("And {} more", problems.len() - SHOWN));
        group.add(&row);
    }
    group
}

fn libraries_of(state: &App, items: &[Item]) -> Vec<Place> {
    let mut places = Vec::new();
    for item in items {
        let Target::Exactly(to) = &item.to else { continue };
        let library = state.libraries.all.borrow().iter().find(|library| Some(library.path.as_path()) == to.parent()).cloned();
        let place = library.map(Place::Library).unwrap_or_else(|| Place::New(to.parent().unwrap_or(to).to_path_buf()));
        if !places.contains(&place) {
            places.push(place);
        }
    }
    places
}

fn card_mount(root: &Path) -> Option<gio::Mount> {
    gio::VolumeMonitor::get()
        .mounts()
        .into_iter()
        .find(|mount| mount.root().path().as_deref() == Some(root) && (mount.can_eject() || mount.can_unmount()))
}

fn eject_card(state: &App, mount: gio::Mount) {
    let state = state.clone();
    glib::spawn_future_local(async move {
        let name = mount.name();
        let result = match mount.can_eject() {
            true => mount.eject_with_operation_future(gio::MountUnmountFlags::NONE, None::<&gio::MountOperation>).await,
            false => mount.unmount_with_operation_future(gio::MountUnmountFlags::NONE, None::<&gio::MountOperation>).await,
        };
        match result {
            Ok(()) => state.toast(&format!("{name} can be taken out")),
            Err(err) => state.toast(&format!("Could not eject {name}: {}", err.message())),
        }
    });
}
