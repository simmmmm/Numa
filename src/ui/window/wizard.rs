use super::downloads::{download_additional, megabytes, not_yet_here};
use super::*;

use numa::io::models::{files_of, ModelFile};

const WIZARD: &str = "first-start-wizard";
const STEPS: usize = 3;

struct Tile {
    icon: &'static str,
    title: &'static str,
    line: &'static str,
    models: &'static [&'static str],
    ticked: bool,
}

const TILES: [Tile; 13] = [
    Tile { icon: "weather-few-clouds-symbolic", title: "Sky and greenery", line: "Finds the sky, trees and grass so you can edit them alone.", models: &["EfficientViT-Seg B2"], ticked: true },
    Tile { icon: "input-mouse-symbolic", title: "Click to select", line: "Click on something in the photograph to select it.", models: &["SlimSAM"], ticked: true },
    Tile { icon: "avatar-default-symbolic", title: "The subject", line: "Picks out the subject and its edges.", models: &["BiRefNet"], ticked: false },
    Tile { icon: "edit-cut-symbolic", title: "Hair and fur", line: "Cleaner edges around hair and fur.", models: &["ViTMatte-S"], ticked: false },
    Tile { icon: "system-users-symbolic", title: "Faces and people", line: "Finds faces and groups the same person together.", models: &["YuNet", "SFace"], ticked: true },
    Tile { icon: "view-reveal-symbolic", title: "Closed eyes", line: "Flags photographs where eyes are closed, when culling.", models: &["Open-closed eye"], ticked: true },
    Tile { icon: "emoji-nature-symbolic", title: "Animals", line: "Names the animal when the subject is one.", models: &["PP-ResNet50"], ticked: false },
    Tile { icon: "edit-clear-symbolic", title: "Remove a spot", line: "Fills a spot from what is around it.", models: &["LaMa"], ticked: false },
    Tile { icon: "edit-delete-symbolic", title: "Remove people", line: "Finds passers-by so you can remove them.", models: &["YOLOX-s"], ticked: false },
    Tile { icon: "image-x-generic-symbolic", title: "Sharpen", line: "Reduces blur from camera shake.", models: &["Restormer"], ticked: false },
    Tile { icon: "zoom-in-symbolic", title: "Enlarge", line: "Exports at twice the size.", models: &["RealPLKSR"], ticked: false },
    Tile { icon: "weather-clear-night-symbolic", title: "Reduce noise", line: "Cleans up grain in high-ISO photographs.", models: &["SCUNet"], ticked: false },
    Tile { icon: "system-search-symbolic", title: "Search by words", line: "Find photographs by describing them.", models: &[numa::io::words::MODEL], ticked: false },
];

fn opens(asked: Option<bool>, libraries: usize) -> bool {
    asked.is_none() && libraries == 0
}

fn default_ticks() -> [bool; TILES.len()] {
    TILES.map(|tile| tile.ticked)
}

fn files_for(ticks: &[bool]) -> Vec<ModelFile> {
    let mut files: Vec<ModelFile> = Vec::new();
    for (tile, _) in TILES.iter().zip(ticks).filter(|(_, ticked)| **ticked) {
        for file in files_of(tile.models) {
            if !files.contains(&file) {
                files.push(file);
            }
        }
    }
    not_yet_here(files)
}

fn size_text(megabytes: u64) -> String {
    match megabytes {
        0 => "< 1 MB".to_string(),
        1..=999 => format!("{megabytes} MB"),
        _ => format!("{:.1} GB", megabytes as f64 / 1000.0),
    }
}

pub(super) fn open_if_first_start(state: &App, window: &adw::ApplicationWindow) -> bool {
    let beside = std::env::args().nth(1).is_some_and(|arg| arg == "--self-test")
        || ["NUMA_OPEN", "NUMA_COMPARE", "NUMA_IMPORT"].iter().any(|name| std::env::var_os(name).is_some());
    let libraries = state.catalog.libraries().map_or(0, |libraries| libraries.len());
    let asked = state.catalog.recall::<bool>(WIZARD);
    if beside || !opens(asked, libraries) {

        if asked.is_none() {
            state.catalog.remember(WIZARD, &true);
        }
        return false;
    }

    glib::idle_add_local_once(glib::clone!(
        #[strong] state,
        #[weak] window,
        move || present(&state, &window)
    ));
    true
}

fn step_page(step: usize, title: &str, intro: &str, body: &impl IsA<gtk::Widget>) -> gtk::Box {
    let column = gtk::Box::new(gtk::Orientation::Vertical, 12);
    column.set_margin_start(24);
    column.set_margin_end(24);
    column.set_margin_top(12);
    let place = gtk::Label::new(Some(&format!("Step {step} of {STEPS}")));
    place.add_css_class("dim-label");
    place.add_css_class("caption");
    column.append(&place);
    let heading = gtk::Label::new(Some(title));
    heading.add_css_class("title-1");
    heading.set_wrap(true);
    column.append(&heading);
    let line = gtk::Label::new(Some(intro));
    line.set_wrap(true);
    line.set_xalign(0.5);
    line.set_justify(gtk::Justification::Center);
    column.append(&line);
    body.set_vexpand(true);
    column.append(body);
    column
}

fn scrolling(card: &gtk::Box) -> gtk::ScrolledWindow {
    gtk::ScrolledWindow::builder().hscrollbar_policy(gtk::PolicyType::Never).child(card).build()
}

fn button_row(grey: &[&gtk::Button], white: &gtk::Button) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    row.set_halign(gtk::Align::Center);
    row.set_margin_top(12);
    row.set_margin_bottom(24);
    for button in grey {
        button.add_css_class("pill");
        row.append(*button);
    }
    white.add_css_class("pill");
    row.append(white);
    row
}

type Go = Rc<dyn Fn(usize)>;

fn present(state: &App, window: &adw::ApplicationWindow) {
    let dialog = adw::Dialog::new();
    dialog.set_content_width(720);
    dialog.set_content_height(640);
    let view = adw::ToolbarView::new();
    let header = adw::HeaderBar::new();
    header.set_show_title(false);
    let back = gtk::Button::from_icon_name("go-previous-symbolic");
    back.set_tooltip_text(Some("Back"));
    back.set_visible(false);
    header.pack_start(&back);
    view.add_top_bar(&header);
    let pages = gtk::Stack::new();
    pages.set_transition_type(gtk::StackTransitionType::SlideLeftRight);
    view.set_content(Some(&pages));
    dialog.set_child(Some(&view));

    let go: Go = Rc::new(glib::clone!(
        #[weak] pages,
        #[weak] back,
        move |step: usize| {
            pages.set_visible_child_name(&step.to_string());
            back.set_visible(step > 0);
        }
    ));
    back.connect_clicked(glib::clone!(
        #[weak] pages,
        #[strong] go,
        move |_| {
            let at: usize = pages.visible_child_name().and_then(|name| name.parse().ok()).unwrap_or(0);
            go(at.saturating_sub(1));
        }
    ));
    pages.add_named(&photographs_step(state, window, &go), Some("0"));
    pages.add_named(&home_step(state, window, &go), Some("1"));
    pages.add_named(&extras_step(state, &dialog), Some("2"));
    ask_once(state, &dialog);
    go(0);
    dialog.present(Some(window));
}

fn photographs_step(state: &App, window: &adw::ApplicationWindow, go: &Go) -> gtk::Box {
    let continue_1 = primary_button("Continue");
    continue_1.set_sensitive(state.catalog.libraries().is_ok_and(|libraries| !libraries.is_empty()));
    let add = gtk::Button::with_label("Add Folder…");
    let card = gtk::Box::new(gtk::Orientation::Vertical, 12);
    card.add_css_class("card");
    card.set_valign(gtk::Align::Center);
    card.set_halign(gtk::Align::Center);
    card.set_margin_top(16);
    card.set_margin_bottom(16);
    let stack = places::fan(96.0);
    stack.set_halign(gtk::Align::Center);
    stack.set_margin_top(24);
    card.append(&stack);
    let empty_title = gtk::Label::new(Some("No Libraries Yet"));
    empty_title.add_css_class("title-3");
    card.append(&empty_title);
    let empty_line = gtk::Label::new(Some("Add the folder you keep your photographs in."));
    empty_line.add_css_class("dim-label");
    empty_line.set_wrap(true);
    empty_line.set_margin_start(24);
    empty_line.set_margin_end(24);
    card.append(&empty_line);
    add.add_css_class("pill");
    add.set_halign(gtk::Align::Center);
    add.set_margin_bottom(24);
    card.append(&add);
    add.connect_clicked(glib::clone!(
        #[strong] state,
        #[weak] window,
        #[weak] continue_1,
        #[weak] empty_title,
        #[weak] empty_line,
        move |_| {
            let then: Rc<dyn Fn(&Path)> = Rc::new(move |path| {
                continue_1.set_sensitive(true);
                empty_title.set_text(&path.file_name().map_or_else(|| "Added".to_string(), |name| name.to_string_lossy().into_owned()));
                empty_line.set_text(&format!("Added {}.", path.display()));
            });
            add_library::add_library_then(&state, &window, Some(then));
        }
    ));
    let one = step_page(1, "Where are your photographs?", "Choose a folder. Numa never moves or changes your photographs; it keeps its edits beside them.", &scrolling(&card));
    one.append(&button_row(&[], &continue_1));
    continue_1.connect_clicked(glib::clone!(#[strong] go, move |_| go(1)));
    one
}

fn home_step(state: &App, window: &adw::ApplicationWindow, go: &Go) -> gtk::Box {
    let home_card = gtk::Box::new(gtk::Orientation::Vertical, 8);
    home_card.set_valign(gtk::Align::Center);
    let house = gtk::Image::from_icon_name("go-home-symbolic");
    house.set_pixel_size(64);
    house.add_css_class("dim-label");
    home_card.append(&house);
    for (text, heading) in [
        ("Open the same libraries in Numa and in any browser.", true),
        ("Your photographs stay on your own server, not a cloud service.", false),
        ("Connect it later from Preferences if you prefer.", false),
    ] {
        let label = gtk::Label::new(Some(text));
        label.set_wrap(true);
        label.set_justify(gtk::Justification::Center);
        label.add_css_class(if heading { "heading" } else { "dim-label" });
        home_card.append(&label);
    }
    let later_2 = gtk::Button::with_label("Not Now");
    let connect = primary_button("Connect…");
    let two = step_page(2, "Your photographs, at home", "Numa Home is optional. It is a photo server you run yourself, such as on a NAS.", &scrolling(&home_card));
    two.append(&button_row(&[&later_2], &connect));
    later_2.connect_clicked(glib::clone!(#[strong] go, move |_| go(2)));
    connect.connect_clicked(glib::clone!(
        #[strong] state,
        #[strong] go,
        #[weak] window,
        move |_| {
            let prefs = preferences::preferences_dialog_at(&state, &window, Some("home"));
            prefs.connect_closed(glib::clone!(#[strong] go, move |_| go(2)));
        }
    ));
    two
}

fn extras_step(state: &App, dialog: &adw::Dialog) -> adw::ToolbarView {
    let ticks = Rc::new(RefCell::new(default_ticks()));
    let boxes: Vec<gtk::CheckButton> = TILES.iter().map(|_| gtk::CheckButton::new()).collect();
    let everything = gtk::CheckButton::new();
    let summary = gtk::Label::new(None);
    summary.add_css_class("numeric");
    summary.set_xalign(0.0);
    summary.set_hexpand(true);
    let download = primary_button("Download");
    let refresh = Rc::new(glib::clone!(
        #[strong] ticks,
        #[weak] summary,
        #[weak] download,
        move || {
            let ticks = ticks.borrow();
            let files = files_for(&*ticks);
            let count = ticks.iter().filter(|ticked| **ticked).count();
            summary.set_text(&format!("{count} selected · {}", size_text(megabytes(&files))));
            download.set_sensitive(!files.is_empty());
        }
    ));
    let mut all_mb = 0;
    let mut rows = Vec::new();
    for (index, (tile, check)) in TILES.iter().zip(&boxes).enumerate() {
        let (row, missing) = tile_row(index, tile, check, &ticks, &refresh);
        all_mb += missing;
        rows.push(row);
    }
    let body = boxed_list(&rows);
    body.set_margin_top(8);
    body.set_margin_bottom(16);
    let later_3 = gtk::Button::with_label("Not Now");
    let bar = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    bar.append(&summary);
    later_3.add_css_class("pill");
    download.add_css_class("pill");
    let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    buttons.append(&later_3);
    buttons.append(&download);
    bar.append(&buttons);

    let narrow = adw::Breakpoint::new(adw::BreakpointCondition::new_length(adw::BreakpointConditionLengthType::MaxWidth, 560.0, adw::LengthUnit::Px));
    narrow.add_setter(&bar, "orientation", Some(&gtk::Orientation::Vertical.to_value()));
    narrow.add_setter(&buttons, "halign", Some(&gtk::Align::End.to_value()));
    dialog.add_breakpoint(narrow);
    let three = step_page(3, "Choose what Numa can do", "Everything here runs on your own computer. Your photographs are never sent online. Each one downloads once, only when you choose Download, and you can add more in Preferences.", &body);

    let scroll = gtk::ScrolledWindow::builder().hscrollbar_policy(gtk::PolicyType::Never).child(&three).build();
    let page = adw::ToolbarView::new();
    page.set_content(Some(&scroll));

    let foot = gtk::Box::new(gtk::Orientation::Vertical, 12);
    foot.set_margin_start(24);
    foot.set_margin_end(24);
    foot.set_margin_top(12);
    foot.set_margin_bottom(12);
    foot.append(&everything_row(&everything, &boxes, all_mb));
    foot.append(&bar);
    page.add_bottom_bar(&foot);
    page.set_bottom_bar_style(adw::ToolbarStyle::Raised);
    refresh();
    later_3.connect_clicked(glib::clone!(#[weak] dialog, move |_| { dialog.close(); }));
    download.connect_clicked(glib::clone!(
        #[strong] state,
        #[weak] dialog,
        #[strong] ticks,
        move |_| {
            download_additional(&state, files_for(&*ticks.borrow()));
            dialog.close();
        }
    ));
    page
}

fn tile_row(
    index: usize,
    tile: &Tile,
    check: &gtk::CheckButton,
    ticks: &Rc<RefCell<[bool; TILES.len()]>>,
    refresh: &Rc<impl Fn() + 'static>,
) -> (adw::ActionRow, u64) {
    let files = files_of(tile.models);
    let here = not_yet_here(files.clone()).is_empty();
    let mb = megabytes(&files);
    let missing = megabytes(&not_yet_here(files));

    let row = adw::ActionRow::new();
    row.set_title(tile.title);
    row.set_subtitle(tile.line);
    row.add_prefix(&gtk::Image::from_icon_name(tile.icon));
    let size = gtk::Label::new(Some(&if here { "Installed".to_string() } else { size_text(mb) }));
    size.add_css_class("dim-label");
    size.add_css_class("numeric");
    row.add_suffix(&size);
    check.set_valign(gtk::Align::Center);
    check.update_property(&[gtk::accessible::Property::Label(tile.title)]);
    check.set_active(tile.ticked && !here);
    check.set_sensitive(!here);
    if here {
        ticks.borrow_mut()[index] = false;
    }
    row.add_suffix(check);
    row.set_activatable_widget(Some(check));
    check.connect_toggled(glib::clone!(
        #[strong] ticks,
        #[strong] refresh,
        move |check| {
            ticks.borrow_mut()[index] = check.is_active();
            refresh();
        }
    ));
    (row, missing)
}

fn everything_row(everything: &gtk::CheckButton, boxes: &[gtk::CheckButton], all_mb: u64) -> gtk::ListBox {
    let boxes = boxes.to_vec();
    let row = adw::ActionRow::new();
    row.set_title("Download Everything");
    row.set_subtitle(&format!("All of the above · {} · best on Wi-Fi", size_text(all_mb)));
    everything.set_valign(gtk::Align::Center);
    row.add_suffix(everything);
    row.set_activatable_widget(Some(everything));
    everything.update_property(&[gtk::accessible::Property::Label("Download Everything")]);
    everything.connect_toggled(glib::clone!(
        #[strong] boxes,
        move |everything| {
            for check in boxes.iter().filter(|check| check.is_sensitive()) {
                check.set_active(everything.is_active());
            }
        }
    ));
    boxed_list(&[row])
}

fn boxed_list(rows: &[adw::ActionRow]) -> gtk::ListBox {
    let list = gtk::ListBox::new();
    list.set_selection_mode(gtk::SelectionMode::None);
    list.add_css_class("boxed-list");
    for row in rows {
        list.append(row);
    }
    list
}

fn ask_once(state: &App, dialog: &adw::Dialog) {
    dialog.connect_closed(glib::clone!(
        #[strong] state,
        move |_| {
            state.catalog.remember(WIZARD, &true);
            state.catalog.remember(OFFERED_MODELS, &true);
        }
    ));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_wizard_opens_on_a_first_start_only() {
        assert!(opens(None, 0));
        assert!(!opens(Some(true), 0), "finished or skipped: never again");
        assert!(!opens(None, 2), "an update from before the wizard");
    }

    #[test]
    fn four_things_are_ticked_at_first_and_nothing_downloads_unasked() {
        let ticked: Vec<&str> = TILES.iter().filter(|tile| tile.ticked).map(|tile| tile.title).collect();
        assert_eq!(ticked, ["Sky and greenery", "Click to select", "Faces and people", "Closed eyes"]);
        assert!(files_for(&[false; TILES.len()]).is_empty());
        assert_eq!(size_text(0), "< 1 MB");
        assert_eq!(size_text(140), "140 MB");
        assert_eq!(size_text(2200), "2.2 GB");
    }
}
