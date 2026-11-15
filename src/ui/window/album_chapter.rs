use super::*;
use numa::io::album_page::{ChapterLayout, Offer};
use numa::io::catalog::{AlbumChapter, AlbumKind, AlbumPage, Photo, CHAPTER_NAME_LIMIT};

const PRINT: i32 = 36;
const PRINT_HIGH: i32 = 112;
const PRINT_WIDE: i32 = 168;

pub(super) fn block(state: &App, page: &AlbumPage, chapter: &AlbumChapter, laid: &ChapterLayout, photos: &HashMap<i64, &Photo>, ahead: bool, here: bool) -> gtk::Box {
    let block = gtk::Box::new(gtk::Orientation::Vertical, 8);
    let mut line = album_words::chapter_line(chapter, laid);
    if here {
        line = format!("{line} · Here Now");
    } else if ahead && chapter.days.is_some() {
        line = format!("{line} · Ahead");
    } else if ahead {
        line = "Ahead".to_string();
    }
    block.append(&kit::section_header(&line));
    let name = gtk::Label::new(Some(&chapter.name));
    name.add_css_class("album-chapter-name");
    if ahead {
        name.add_css_class("ahead");
    } else if laid.count == 0 {
        name.add_css_class("bare");
    }
    name.set_xalign(0.0);
    name.set_hexpand(true);
    name.set_ellipsize(gtk::pango::EllipsizeMode::End);
    let head = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    head.append(&name);
    head.append(&chapter_menu(chapter));
    block.append(&head);

    let prompt = match (chapter.place.is_some(), laid.count) {
        (true, _) => "Write a few lines about this place",
        (false, 0) => "Write what you want of it",
        (false, _) => "Write about it",
    };
    block.append(&words_field(state, chapter, prompt));
    block.append(&album_days::days(state, page, Some(chapter), &laid.by_day, photos, true));
    for offer in &laid.offers {
        block.append(&offer_row(state, page, chapter, laid.count > 0, offer));
    }
    block
}

fn chapter_menu(chapter: &AlbumChapter) -> gtk::MenuButton {
    let menu = gio::Menu::new();
    for (label, action) in [("Rename…", "album.rename-chapter"), ("Remove Chapter…", "album.remove-chapter")] {
        let item = gio::MenuItem::new(Some(label), None);
        item.set_action_and_target_value(Some(action), Some(&chapter.key.to_variant()));
        menu.append_item(&item);
    }
    let button = gtk::MenuButton::new();
    button.set_icon_name("view-more-symbolic");
    button.add_css_class("flat");
    button.set_valign(gtk::Align::Center);
    button.set_tooltip_text(Some("More"));
    button.set_menu_model(Some(&menu));
    button
}

fn words_field(state: &App, chapter: &AlbumChapter, prompt: &str) -> gtk::Box {
    let key = chapter.key.clone();
    let keep = Box::new(move |state: &App, words: &str| state.catalog.set_chapter_words(&key, words));
    let left = Box::new(|state: &App, _: &str, changed: bool| {
        if changed {
            album_round::after_a_round(state);
        }
    });
    text_field::field(state, &chapter.words, prompt, &format!("Words for {}", chapter.name), text_field::Kept { keep, left })
}

fn offer_row(state: &App, page: &AlbumPage, chapter: &AlbumChapter, has_photos: bool, offer: &Offer) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    row.add_css_class("album-offer");
    let prints = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    for (at, id) in offer.ids.iter().take(3).enumerate() {
        let picture = gtk::Picture::new();
        picture.set_content_fit(gtk::ContentFit::Cover);
        picture.set_can_shrink(true);
        if has_photos {
            picture.set_size_request(PRINT, PRINT);
        } else {
            picture.set_size_request(PRINT_WIDE, PRINT_HIGH);
        }
        picture.set_overflow(gtk::Overflow::Hidden);
        picture.add_css_class("print");

        if let Some((path, mtime)) = state.catalog.file_of(*id) {
            let (wanted, shown) = (picture.downgrade(), picture.clone());
            thumbnail::load_thumbnail_while(&path, mtime, GRID_THUMB_EDGE, state.catalog.edits_json(*id).ok().flatten(), None, move || wanted.upgrade().is_some(), move |texture| shown.set_paintable(Some(&texture)));
        }

        let look = gtk::Button::new();
        look.add_css_class("flat");
        look.add_css_class("album-photo");
        look.set_child(Some(&picture));
        look.set_tooltip_text(Some("Look at what is offered"));
        let (state, ids) = (state.clone(), offer.ids.clone());
        look.connect_clicked(move |_| today::open_these(&state, &ids, at));
        prints.append(&look);
    }
    row.append(&prints);
    let (line, why) = album_words::offer_lines(page, chapter, has_photos, offer);
    let said = gtk::Label::new(Some(&line));
    said.add_css_class("heading");
    said.add_css_class("numeric");
    said.set_xalign(0.0);
    let reason = gtk::Label::new(Some(&why));
    reason.add_css_class("caption");
    reason.add_css_class("dim-label");
    reason.set_xalign(0.0);
    let words = gtk::Box::new(gtk::Orientation::Vertical, 0);
    words.set_hexpand(true);
    words.set_valign(gtk::Align::Center);
    words.append(&said);
    words.append(&reason);
    row.append(&words);
    let chips = kit::chip_row();
    chips.set_valign(gtk::Align::Center);
    let add = gtk::Button::with_label(&format!("Add {}", places::grouped(offer.ids.len() as i64)));
    let (state, key, name, ids) = (state.clone(), chapter.key.clone(), chapter.name.clone(), offer.ids.clone());
    add.connect_clicked(move |_| take(&state, &key, &name, &ids));
    chips.append(&add);
    row.append(&chips);
    if has_photos {
        return row;
    }

    row.remove(&prints);
    let above = gtk::Box::new(gtk::Orientation::Vertical, 8);
    above.add_css_class("album-offer");
    prints.set_halign(gtk::Align::Start);
    above.append(&prints);
    row.remove_css_class("album-offer");
    above.append(&row);
    above
}

fn take(state: &App, chapter: &str, name: &str, ids: &[i64]) {
    let new = match state.catalog.add_to_chapter(chapter, ids) {
        Ok(new) => new,
        Err(err) => return state.toast(&err),
    };
    changed(state, ids);
    let toast = adw::Toast::new(&glib::markup_escape_text(&format!("{} added to {name}", places::grouped(ids.len() as i64))));
    toast.set_button_label(Some("Undo"));
    let (undoing, chapter, ids, album) = (state.clone(), chapter.to_string(), ids.to_vec(), state.libraries.album.key.borrow().clone());
    toast.connect_button_clicked(move |_| {
        let undone = undoing.catalog.take_from_chapter(&chapter, &ids).and_then(|()| match &album {
            Some(album) if !new.is_empty() => undoing.catalog.set_in_album(album, &new, false),
            _ => Ok(()),
        });
        if let Err(err) = undone {
            undoing.toast(&err);
        }
        changed(&undoing, &ids);
    });
    state.toasts.add_toast(toast);
}

pub(super) fn changed(state: &App, ids: &[i64]) {
    refresh_places(state);
    let key = state.libraries.album.key.borrow().clone();
    match key {
        Some(key) if showing_grid_of(state, &key) && album_page::has_page(state, &key) => album_page::show(state, &key),

        Some(key) if state.stack.visible_child_name().as_deref() == Some("album") && !album_page::has_page(state, &key) => picker::show_place(state, Place::Album(key)),
        _ => album_page::refill(state),
    }

    album_round::after_a_round(state);
    let mut libraries: Vec<i64> = ids.iter().map(|id| numa::io::catalog::library_of(*id)).collect();
    libraries.sort_unstable();
    libraries.dedup();
    for library in libraries {
        home_sync::edited(state, library);
    }
}

fn showing_grid_of(state: &App, key: &str) -> bool {
    let filter = state.libraries.filter.borrow();
    state.stack.visible_child_name().as_deref() == Some("library") && filter.album.as_deref() == Some(key) && filter.chapter.is_none()
}

pub(super) fn add_field(state: &App, page: &AlbumPage) -> gtk::Entry {
    let entry = gtk::Entry::new();
    entry.set_placeholder_text(Some("Add a Chapter…"));
    entry.set_primary_icon_name(Some("list-add-symbolic"));
    entry.set_max_length(CHAPTER_NAME_LIMIT as i32);
    entry.add_css_class("album-add");
    let (state, album) = (state.clone(), page.key.clone());
    entry.connect_activate(move |entry| {
        let name = entry.text().trim().to_string();
        if name.is_empty() {
            return;
        }
        match state.catalog.add_chapter(&album, &name, None, None) {
            Ok(_) => changed(&state, &[]),
            Err(err) => state.toast(&err),
        }
    });
    entry
}

pub(super) fn start(page: &AlbumPage) -> adw::Clamp {
    let block = gtk::Box::new(gtk::Orientation::Vertical, 8);
    let empty = adw::StatusPage::new();
    empty.add_css_class("compact");
    empty.add_css_class("album-empty");
    empty.set_icon_name(Some("image-x-generic-symbolic"));
    empty.set_title("No Photographs Yet");

    empty.set_description(Some(&glib::markup_escape_text(&format!("Select photographs in Photos or a library, right-click them and choose Add to Album, then {}.", page.name))));
    block.append(&empty);

    let mut ways = Vec::new();
    if page.kind == AlbumKind::Album {
        ways.push(("numa-route-symbolic", "A Trip", "Places, a map and the route between them.", "album.make-trip"));
        ways.push(("numa-heart-symbolic", "A Wedding", "Six chapters to begin with, from Getting Ready to the Party.", "album.make-wedding"));
    }
    if page.kind != AlbumKind::Trip {
        ways.push(("view-list-symbolic", "Chapters of Your Own", "Name the parts, and write a line under each.", "album.add-chapter"));
    }
    if !ways.is_empty() {
        block.append(&kit::section_header("Begin with"));

        let list = gtk::ListBox::new();
        list.add_css_class("album-plain");
        list.add_css_class("album-begin");
        list.set_selection_mode(gtk::SelectionMode::None);
        for (icon, title, line, action) in ways {
            let row = adw::ActionRow::new();
            row.set_title(title);
            row.set_subtitle(line);
            row.set_activatable(true);
            row.add_prefix(&gtk::Image::from_icon_name(icon));
            row.add_suffix(&gtk::Image::from_icon_name("go-next-symbolic"));
            row.connect_activated(move |row| {
                let _ = row.activate_action(action, None);
            });
            list.append(&row);
        }
        block.append(&list);
    }
    let column = adw::Clamp::new();
    column.set_maximum_size(880);
    column.set_child(Some(&block));
    column
}

pub(super) fn fill_side(state: &App, page: &AlbumPage) {
    let side = &state.libraries.album.side;
    if page.chapters.is_empty() {
        return;
    }
    side.append(&kit::section_header("Chapters"));
    let list = gtk::ListBox::new();
    list.set_selection_mode(gtk::SelectionMode::None);
    list.add_css_class("album-plain");
    for (chapter, laid) in page.chapters.iter().zip(&page.layout.chapters) {

        let offered = laid.offers.iter().map(|offer| offer.ids.len()).max().unwrap_or(0);
        let count = match (laid.count, offered) {
            (0, 0) => "nothing yet".to_string(),
            (0, offered) => format!("{} offered", places::grouped(offered as i64)),
            (count, _) => places::grouped(count as i64),
        };
        let row = adw::ActionRow::new();
        row.set_title(&glib::markup_escape_text(&chapter.name));
        let said = gtk::Label::new(Some(&count));
        said.add_css_class("caption");
        said.add_css_class("dim-label");
        said.add_css_class("numeric");
        row.add_suffix(&said);
        row.set_activatable(true);
        let (state, key) = (state.clone(), chapter.key.clone());
        row.connect_activated(move |_| scroll_to(&state, &key));
        list.append(&row);
    }
    side.append(&list);
    if page.layout.loose > 0 {
        let line = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        let said = gtk::Label::new(Some(&match page.layout.loose {
            1 => "1 photo is in no chapter yet.".to_string(),
            n => format!("{} photos are in no chapter yet.", places::grouped(n as i64)),
        }));
        said.add_css_class("caption");
        said.add_css_class("dim-label");
        said.set_wrap(true);
        said.set_xalign(0.0);
        said.set_hexpand(true);
        let show = gtk::Button::with_label("Show Them");
        show.add_css_class("flat");
        let (state, album) = (state.clone(), page.key.clone());
        show.connect_clicked(move |_| album_page::open_grid(&state, &album, numa::io::catalog::NO_CHAPTER, None, None));
        line.append(&said);
        line.append(&show);
        side.append(&line);
    }
}

pub(super) fn scroll_to(state: &App, key: &str) {
    let column = &state.libraries.album.column;
    let mut line = column.first_child();
    while let Some(part) = line {
        let mut child = part.first_child();
        while let Some(row) = child {
            if row.widget_name() == key {

                let root = &state.libraries.album.root;
                let Some(scrolled) = root.child().and_then(|viewport| viewport.first_child()) else { return };
                if let Some(point) = row.compute_point(&scrolled, &gtk::graphene::Point::new(0.0, 0.0)) {
                    root.vadjustment().set_value(f64::from(point.y()));
                }
                return;
            }
            child = row.next_sibling();
        }
        line = part.next_sibling();
    }
}

pub(super) fn header_end(state: &App) -> gtk::MenuButton {
    let button = gtk::MenuButton::new();
    button.set_icon_name("view-more-symbolic");
    button.set_valign(gtk::Align::Center);
    button.set_tooltip_text(Some("Album"));
    let menu = gio::Menu::new();
    button.set_menu_model(Some(&menu));
    button.set_create_popup_func(glib::clone!(
        #[strong] state,
        move |_| {
            let kind = state.libraries.album.key.borrow().as_ref().and_then(|key| state.catalog.album_kind(key).ok()).unwrap_or(AlbumKind::Album);
            fill_menu(&menu, kind, state.libraries.album.contents.get(), None);
        }
    ));
    button
}

pub(super) fn fill_menu(menu: &gio::Menu, kind: AlbumKind, (photos, chapters): (usize, usize), target: Option<&str>) {
    menu.remove_all();
    let add = |section: &gio::Menu, label: &str, name: &str| match target {
        Some(key) => {
            let item = gio::MenuItem::new(Some(label), None);
            item.set_action_and_target_value(Some(&format!("albums.{name}")), Some(&key.to_variant()));
            section.append_item(&item);
        }
        None => section.append(Some(label), Some(&format!("album.{name}"))),
    };
    if photos > 0 || chapters > 0 {
        let first = gio::Menu::new();
        if kind != AlbumKind::Trip {
            add(&first, "Add a Chapter…", "add-chapter");
        }
        if kind == AlbumKind::Trip {
            add(&first, "Share…", "share");
        }
        if kind == AlbumKind::Album {
            add(&first, "Make a Trip", "make-trip");
            add(&first, "Make a Wedding", "make-wedding");
        } else {
            add(&first, "Make an Album", "make-album");
        }
        menu.append_section(None, &first);
    }
    if photos > 0 {
        let out = gio::Menu::new();
        add(&out, "Export…", "export");
        menu.append_section(None, &out);
    }
    let second = gio::Menu::new();
    add(&second, "Rename…", "rename");
    add(&second, "Delete Album…", "delete");
    menu.append_section(None, &second);
}

pub(super) fn install_actions(state: &App, window: &adw::ApplicationWindow) {
    let group = gio::SimpleActionGroup::new();

    let keyed = gio::SimpleActionGroup::new();
    let plain = |name: &str, run: fn(&App, &adw::ApplicationWindow, &str)| {
        let action = gio::SimpleAction::new(name, None);
        action.connect_activate(glib::clone!(
            #[strong] state,
            #[weak] window,
            move |_, _| {
                let key = state.libraries.album.key.borrow().clone();
                if let Some(key) = key {
                    run(&state, &window, &key);
                }
            }
        ));
        group.add_action(&action);
        let of = gio::SimpleAction::new(name, Some(glib::VariantTy::STRING));
        of.connect_activate(glib::clone!(
            #[strong] state,
            #[weak] window,
            move |_, key| {
                let Some(key) = key.and_then(|key| key.get::<String>()) else { return };

                let (state, window) = (state.clone(), window.clone());
                glib::idle_add_local_full(glib::Priority::LOW, move || {
                    run(&state, &window, &key);
                    glib::ControlFlow::Break
                });
            }
        ));
        keyed.add_action(&of);
    };
    plain("add-chapter", |state, window, album| ask_name(state, window, "Add a Chapter", "", "Add", CHAPTER_NAME_LIMIT, name_chapter(album.to_string())));
    plain("make-trip", |state, _, album| make(state, album, AlbumKind::Trip));
    plain("make-wedding", |state, _, album| make(state, album, AlbumKind::Wedding));
    plain("make-album", |state, _, album| make(state, album, AlbumKind::Album));
    plain("rename", |state, window, album| {
        let name = album_name(state, album);
        let album = album.to_string();
        ask_name(state, window, "Rename Album", &name, "Rename", CHAPTER_NAME_LIMIT, Box::new(move |state, name| state.catalog.rename_album(&album, name).map_err(String::from)));
    });
    plain("delete", delete_album);
    plain("share", album_share::ask);
    plain("export", export_album::ask);
    let chapter = |name: &str, run: fn(&App, &adw::ApplicationWindow, &str)| {
        let action = gio::SimpleAction::new(name, Some(glib::VariantTy::STRING));
        action.connect_activate(glib::clone!(
            #[strong] state,
            #[weak] window,
            move |_, key| {
                if let Some(key) = key.and_then(|key| key.get::<String>()) {
                    run(&state, &window, &key);
                }
            }
        ));
        group.add_action(&action);
    };
    chapter("rename-chapter", |state, window, key| {
        let name = state.catalog.chapter(key).ok().flatten().map(|chapter| chapter.name).unwrap_or_default();
        let key = key.to_string();
        ask_name(state, window, "Rename Chapter", &name, "Rename", CHAPTER_NAME_LIMIT, Box::new(move |state, name| state.catalog.rename_chapter(&key, name)));
    });
    chapter("remove-chapter", remove_chapter);
    window.insert_action_group("album", Some(&group));
    window.insert_action_group("albums", Some(&keyed));
}

fn album_name(state: &App, album: &str) -> String {
    state.catalog.albums().ok().and_then(|albums| albums.into_iter().find(|(key, _)| key == album)).map(|(_, name)| name).unwrap_or_default()
}

type Named = Box<dyn Fn(&App, &str) -> Result<(), String>>;

fn name_chapter(album: String) -> Named {
    Box::new(move |state, name| state.catalog.add_chapter(&album, name, None, None).map(|_| ()))
}

fn ask_name(state: &App, window: &adw::ApplicationWindow, heading: &str, name: &str, done: &str, limit: usize, run: Named) {
    let entry = gtk::Entry::new();
    entry.set_text(name);

    entry.set_placeholder_text(Some(if heading.ends_with("Album") { "Name of the album" } else { "Name of the chapter" }));
    entry.set_max_length(limit as i32);
    entry.set_activates_default(true);
    let ask = adw::AlertDialog::new(Some(heading), None);
    ask.set_extra_child(Some(&entry));
    ask.add_response("cancel", "Cancel");
    ask.add_response("done", done);
    ask.set_default_response(Some("done"));
    ask.set_close_response("cancel");
    ask.set_response_enabled("done", !name.trim().is_empty());
    entry.connect_changed(glib::clone!(
        #[weak] ask,
        move |entry| ask.set_response_enabled("done", !entry.text().trim().is_empty())
    ));
    let (state, typed) = (state.clone(), entry.clone());
    ask.connect_response(None, move |_, response| {
        let name = typed.text().trim().to_string();
        if response != "done" || name.is_empty() {
            return;
        }
        match run(&state, &name) {
            Ok(()) => changed(&state, &[]),
            Err(err) => state.toast(&err),
        }
    });
    ask.present(Some(window));
    entry.grab_focus();
}

fn make(state: &App, album: &str, kind: AlbumKind) {
    match state.catalog.set_album_kind(album, kind) {
        Ok(()) => changed(state, &[]),
        Err(err) => state.toast(&err),
    }
}

fn remove_chapter(state: &App, window: &adw::ApplicationWindow, key: &str) {
    let name = state.catalog.chapter(key).ok().flatten().map(|chapter| chapter.name).unwrap_or_default();
    let ask = adw::AlertDialog::new(Some(&format!("Remove the chapter {name}?")), Some("Its photographs stay in the album, in no chapter."));
    ask.add_response("cancel", "Cancel");
    ask.add_response("remove", "Remove Chapter");
    ask.set_response_appearance("remove", adw::ResponseAppearance::Destructive);
    ask.set_close_response("cancel");
    let (state, key) = (state.clone(), key.to_string());
    ask.connect_response(None, move |_, response| {
        if response != "remove" {
            return;
        }
        match state.catalog.remove_chapter(&key) {
            Ok(()) => {
                changed(&state, &[]);
                state.toast("Chapter removed");
            }
            Err(err) => state.toast(&err),
        }
    });
    ask.present(Some(window));
}

fn delete_album(state: &App, window: &adw::ApplicationWindow, album: &str) {
    let name = album_name(state, album);
    let ask = adw::AlertDialog::new(Some(&format!("Delete the album {name}?")), Some("Its photographs stay in their libraries."));
    ask.add_response("cancel", "Cancel");
    ask.add_response("delete", "Delete Album");
    ask.set_response_appearance("delete", adw::ResponseAppearance::Destructive);
    ask.set_close_response("cancel");
    let (state, album) = (state.clone(), album.to_string());
    ask.connect_response(None, move |_, response| {
        if response != "delete" {
            return;
        }
        match state.catalog.delete_album(&album) {

            Ok(()) if state.libraries.album.key.borrow().as_deref() == Some(album.as_str()) => {
                state.libraries.album.key.replace(None);
                refresh_places(&state);
                show_albums(&state);
                state.stack.set_visible_child_name("albums");
            }
            Ok(()) => {
                refresh_places(&state);
                if state.stack.visible_child_name().as_deref() == Some("albums") {
                    show_albums(&state);
                }
            }
            Err(err) => state.toast(&err),
        }
    });
    ask.present(Some(window));
}
