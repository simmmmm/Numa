use super::*;
use numa::io::picks;

pub(super) fn install_client_actions(state: &App, window: &adw::ApplicationWindow) {
    let paste = gio::SimpleAction::new("client-picks", None);
    paste.connect_activate(glib::clone!(
        #[strong] state,
        #[weak] window,
        move |_, _| client_picks_dialog(&state, &window)
    ));
    window.add_action(&paste);

    let copy = gio::SimpleAction::new("copy-names", None);
    copy.connect_activate(glib::clone!(
        #[strong] state,
        move |_, _| copy_names(&state)
    ));
    window.add_action(&copy);
}

fn copy_names(state: &App) {
    let ids = selected_ids(state);
    if ids.is_empty() {
        state.toast("Select a photo first");
        return;
    }
    let names = {
        let cards = state.grid.cards.borrow();
        picks::file_names(ids.iter().filter_map(|id| cards.get(id)).map(|photo| photo.path.as_path()))
    };
    state.grid.wall.clipboard().set_text(&names.join(", "));
    state.toast(&match names.as_slice() {
        [one] => format!("Copied {one}"),
        many => format!("Copied {} file names", places::grouped(many.len() as i64)),
    });
}

#[derive(Clone, Copy, PartialEq)]
enum MakeThem {
    Picks,
    FiveStars,
    Album,
}

impl MakeThem {
    const CHOICES: [&'static str; 3] = ["Picks", "5 Stars", "Add to Album…"];

    fn chosen(row: &adw::ComboRow) -> Self {
        match row.selected() {
            1 => Self::FiveStars,
            2 => Self::Album,
            _ => Self::Picks,
        }
    }

    fn label(self, found: usize, album: &str) -> String {
        let count = places::grouped(found as i64);
        match (self, found) {
            (Self::Picks, 0) => "Mark as Picks".to_string(),
            (Self::FiveStars, 0) => "Give 5 Stars".to_string(),
            (Self::Album, 0) => "Add to Album".to_string(),
            (Self::Picks, _) => format!("Mark {count} as Picks"),
            (Self::FiveStars, _) => format!("Give 5 Stars to {count}"),
            (Self::Album, _) => format!("Add {count} to {}", album.trim()),
        }
    }
}

fn photos_here(state: &App) -> Result<Vec<Photo>, String> {
    if state.libraries.filter.borrow().spans_libraries() {
        return state.catalog.photos_everywhere(&Filter::default());
    }
    match state.libraries.current.borrow().as_ref() {
        Some(library) => state.catalog.photos(library.id, &Filter::default()),
        None => Ok(Vec::new()),
    }
}

fn export_templates(state: &App) -> Vec<String> {
    let mut templates = vec![state.export.settings.borrow().template.clone(), export::ExportSettings::default().template];
    let presets = state.catalog.recall::<Vec<(String, export::ExportSettings)>>(EXPORT_PRESETS).unwrap_or_default();
    templates.extend(presets.into_iter().map(|(_, settings)| settings.template));
    templates
}

fn client_picks_dialog(state: &App, window: &adw::ApplicationWindow) {
    let photos = match photos_here(state) {
        Ok(photos) => photos,
        Err(err) => {
            state.toast(&format!("Could not read the catalog: {err}"));
            return;
        }
    };

    let before: Rc<HashMap<i64, (u8, Flag)>> = Rc::new(photos.iter().map(|photo| (photo.id, (photo.rating, photo.flag))).collect());
    let index = picks::Index::new(photos.into_iter().map(|photo| (photo.id, photo.path)).collect(), &export_templates(state));
    let nowhere = match state.libraries.filter.borrow().spans_libraries() {
        true => "any library",
        false => "this library",
    };

    let dialog = adw::Dialog::new();
    dialog.set_title("Paste the Client's Picks");
    dialog.set_content_width(600);

    let intro = gtk::Label::new(Some(
        "Copy the list from the gallery (Pixieset, Pic-Time, Picdrop…) and paste it here. \
         Names with or without extensions.",
    ));
    intro.set_wrap(true);
    intro.set_xalign(0.0);
    intro.add_css_class("dim-label");

    let (text, scroll) = text_area();
    let (found, missing, missing_names) = result_rows();
    let make = adw::ComboRow::new();
    make.set_title("Make Them");
    make.set_model(Some(&gtk::StringList::new(&MakeThem::CHOICES)));
    let album = adw::EntryRow::new();
    album.set_title("Album Name");
    album.set_text("Client's Picks");

    let list = gtk::ListBox::new();
    list.add_css_class("boxed-list");
    list.set_selection_mode(gtk::SelectionMode::None);
    list.append(&found);
    list.append(&missing);
    list.append(&make);
    list.append(&album);

    let cancel = gtk::Button::with_label("Cancel");
    cancel.add_css_class("pill");
    let mark = primary_button("Mark as Picks");
    mark.add_css_class("pill");
    let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    buttons.set_halign(gtk::Align::Center);
    buttons.set_margin_top(6);
    buttons.append(&cancel);
    buttons.append(&mark);

    let content = gtk::Box::new(gtk::Orientation::Vertical, 12);
    content.set_margin_top(4);
    content.set_margin_bottom(18);
    content.set_margin_start(22);
    content.set_margin_end(22);
    content.append(&intro);
    content.append(&scroll);
    content.append(&list);
    content.append(&buttons);
    let view = adw::ToolbarView::new();
    view.add_top_bar(&adw::HeaderBar::new());
    view.set_content(Some(&content));
    dialog.set_child(Some(&view));

    let result = Rc::new(RefCell::new(picks::Found::default()));
    let update = Rc::new(glib::clone!(
        #[strong] result,
        #[weak] text,
        #[weak] found,
        #[weak] missing,
        #[weak] missing_names,
        #[weak] make,
        #[weak] album,
        #[weak] mark,
        move || {
            let buffer = text.buffer();
            let now = index.find(&buffer.text(&buffer.start_iter(), &buffer.end_iter(), false));
            found.set_visible(now.total() > 0);
            found.set_title(&format!(
                "{} of {} found",
                places::grouped(now.found as i64),
                places::grouped(now.total() as i64)
            ));
            let folders: Vec<String> = now.folders.iter().map(|(folder, count)| format!("{folder} {count}")).collect();
            found.set_subtitle(&match folders.is_empty() {
                true => String::new(),
                false => format!("In {}", folders.join(" · ")),
            });
            missing.set_visible(!now.missing.is_empty());
            missing.set_title(&format!("{} not in {nowhere}", places::grouped(now.missing.len() as i64)));
            missing.set_subtitle(&now.missing.join(", "));
            missing_names.set_text(&now.missing.join("\n"));
            let how = MakeThem::chosen(&make);
            album.set_visible(how == MakeThem::Album);
            mark.set_label(&how.label(now.found, &album.text()));
            mark.set_sensitive(now.found > 0 && (how != MakeThem::Album || !album.text().trim().is_empty()));
            *result.borrow_mut() = now;
        }
    ));
    update();
    text.buffer().connect_changed(glib::clone!(
        #[strong] update,
        move |_| update()
    ));
    make.connect_selected_notify(glib::clone!(
        #[strong] update,
        move |_| update()
    ));
    album.connect_changed(move |_| update());

    cancel.connect_clicked(glib::clone!(
        #[weak] dialog,
        move |_| {
            dialog.close();
        }
    ));
    mark.connect_clicked(glib::clone!(
        #[strong] state,
        #[weak] dialog,
        #[weak] make,
        #[weak] album,
        move |_| {
            let found = result.borrow();
            mark_found(&state, &found.ids, found.found, MakeThem::chosen(&make), album.text().trim(), &before);
            dialog.close();
        }
    ));
    dialog.present(Some(window));
    text.grab_focus();
}

fn text_area() -> (gtk::TextView, gtk::ScrolledWindow) {
    let text = gtk::TextView::new();
    text.set_monospace(true);
    text.set_wrap_mode(gtk::WrapMode::WordChar);
    text.set_top_margin(10);
    text.set_bottom_margin(10);
    text.set_left_margin(12);
    text.set_right_margin(12);
    text.update_property(&[gtk::accessible::Property::Label("The client's picks")]);
    let scroll = gtk::ScrolledWindow::builder()
        .child(&text)
        .min_content_height(170)
        .hscrollbar_policy(gtk::PolicyType::Never)
        .build();
    scroll.add_css_class("card");
    (text, scroll)
}

fn result_rows() -> (adw::ActionRow, adw::ExpanderRow, gtk::Label) {
    let found = adw::ActionRow::new();
    found.set_use_markup(false);
    found.set_subtitle_lines(2);
    found.add_suffix(&gtk::Image::from_icon_name("object-select-symbolic"));
    let missing = adw::ExpanderRow::new();
    missing.set_use_markup(false);
    missing.set_subtitle_lines(1);
    let names = gtk::Label::new(None);
    names.set_wrap(true);
    names.set_selectable(true);
    names.set_xalign(0.0);
    names.set_margin_top(10);
    names.set_margin_bottom(10);
    names.set_margin_start(12);
    names.set_margin_end(12);
    missing.add_row(&names);
    (found, missing, names)
}

fn mark_found(state: &App, ids: &[i64], frames: usize, how: MakeThem, album: &str, before: &HashMap<i64, (u8, Flag)>) {
    let count = match frames {
        1 => "1 photograph".to_string(),
        many => format!("{} photographs", places::grouped(many as i64)),
    };
    let (message, undo): (String, Box<dyn Fn(&App)>) = match how {
        MakeThem::Picks | MakeThem::FiveStars => {
            let (action, message) = match how {
                MakeThem::Picks => (Action::Flag(Flag::Picked), format!("{count} marked as picks")),
                _ => (Action::Rate(5), format!("{count} given 5 stars")),
            };

            let mut was: Vec<(Action, Vec<i64>)> = Vec::new();
            for id in ids {
                let (rating, flag) = before.get(id).copied().unwrap_or((0, Flag::None));
                let back = match action {
                    Action::Flag(_) => Action::Flag(flag),
                    Action::Rate(_) => Action::Rate(rating),
                };
                match was.iter_mut().find(|(have, _)| *have == back) {
                    Some((_, group)) => group.push(*id),
                    None => was.push((back, vec![*id])),
                }
            }
            apply_to_ids(state, ids, action);
            (message, Box::new(move |state| was.iter().for_each(|(back, group)| apply_to_ids(state, group, *back))))
        }
        MakeThem::Album => {
            let existing = state.catalog.albums().unwrap_or_default().into_iter().find(|(_, name)| name.eq_ignore_ascii_case(album));
            let (key, name, made) = match existing {
                Some((key, name)) => (key, name, false),
                None => match state.catalog.create_album(album) {
                    Ok(key) => (key, album.to_string(), true),
                    Err(err) => {
                        state.toast(&err);
                        return;
                    }
                },
            };

            let inside: std::collections::HashSet<i64> = match made {
                true => Default::default(),
                false => state
                    .catalog
                    .photos_everywhere(&Filter { album: Some(key.clone()), ..Filter::default() })
                    .unwrap_or_default()
                    .into_iter()
                    .map(|photo| photo.id)
                    .collect(),
            };
            let added: Vec<i64> = ids.iter().copied().filter(|id| !inside.contains(id)).collect();
            if let Err(err) = state.catalog.set_in_album(&key, &added, true) {
                state.toast(&format!("Could not add to the album: {err}"));
                return;
            }
            album_changed(state, &key, made);
            (
                format!("{count} added to {name}"),
                Box::new(move |state| {
                    let undone = match made {
                        true => state.catalog.delete_album(&key),
                        false => state.catalog.set_in_album(&key, &added, false),
                    };
                    if let Err(err) = undone {
                        state.toast(&format!("Could not undo: {err}"));
                    }
                    album_changed(state, &key, made);
                }),
            )
        }
    };
    let toast = adw::Toast::new(&glib::markup_escape_text(&message));
    toast.set_button_label(Some("Undo"));
    toast.connect_button_clicked(glib::clone!(
        #[strong] state,
        move |_| undo(&state)
    ));
    state.toasts.add_toast(toast);
}

fn album_changed(state: &App, key: &str, made: bool) {
    if made {
        refresh_picker(state);
    }
    if state.libraries.filter.borrow().album.as_deref() == Some(key) {
        reload_grid(state);
    }
}
