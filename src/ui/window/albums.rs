use super::*;

pub(super) fn fill_albums_menu(state: &App, albums: &[(String, String)]) {
    let menu = &state.libraries.albums_menu;
    menu.remove_all();
    let add = gio::Menu::new();
    let existing = gio::Menu::new();
    for (key, name) in albums {
        let item = gio::MenuItem::new(Some(name), None);
        item.set_action_and_target_value(Some("win.photo-album-add"), Some(&key.to_variant()));
        existing.append_item(&item);
    }
    add.append_section(None, &existing);
    add.append(Some("New album…"), Some("win.photo-album-new"));
    menu.append_submenu(Some("Add to album"), &add);
    if state.libraries.filter.borrow().album.is_some() {
        menu.append(Some("Remove from album"), Some("win.photo-album-remove"));
    }
}

pub(super) fn install_album_actions(state: &App, window: &adw::ApplicationWindow) {
    let add = gio::SimpleAction::new("photo-album-add", Some(&String::static_variant_type()));
    add.connect_activate(glib::clone!(
        #[strong] state,
        move |_, key| {
            let Some(key) = key.and_then(|key| key.get::<String>()) else { return };
            add_selection_to_album(&state, &key);
        }
    ));
    window.add_action(&add);

    let new = gio::SimpleAction::new("photo-album-new", None);
    new.connect_activate(glib::clone!(
        #[strong] state,
        #[weak] window,
        move |_, _| {
            if selected_ids(&state).is_empty() {
                state.toast("Select a photo first");
                return;
            }
            let entry = gtk::Entry::new();
            entry.set_placeholder_text(Some("Name"));
            entry.set_activates_default(true);
            let ask = adw::AlertDialog::new(Some("New Album"), None);
            ask.set_extra_child(Some(&entry));
            ask.add_response("cancel", "Cancel");
            ask.add_response("create", "Create");
            ask.set_response_appearance("create", adw::ResponseAppearance::Suggested);
            ask.set_default_response(Some("create"));
            ask.set_close_response("cancel");
            let state = state.clone();
            ask.connect_response(None, move |_, response| {
                if response != "create" {
                    return;
                }
                match state.catalog.create_album(&entry.text()) {
                    Ok(key) => {
                        refresh_picker(&state);
                        add_selection_to_album(&state, &key);
                    }
                    Err(err) => state.toast(&err),
                }
            });
            ask.present(Some(&window));
        }
    ));
    window.add_action(&new);

    let remove = gio::SimpleAction::new("photo-album-remove", None);
    remove.connect_activate(glib::clone!(
        #[strong] state,
        move |_, _| {
            let Some(key) = state.libraries.filter.borrow().album.clone() else { return };
            match state.catalog.set_in_album(&key, &selected_ids(&state), false) {
                Ok(()) => reload_grid(&state),
                Err(err) => state.toast(&format!("Could not remove from the album: {err}")),
            }
        }
    ));
    window.add_action(&remove);

    let manage = gio::SimpleAction::new("albums", None);
    manage.connect_activate(glib::clone!(
        #[strong] state,
        #[weak] window,
        move |_, _| albums_dialog(&state, &window)
    ));
    window.add_action(&manage);
}

pub(super) fn add_selection_to_album(state: &App, key: &str) {
    let ids = selected_ids(state);
    if ids.is_empty() {
        state.toast("Select a photo first");
        return;
    }
    let name = state.catalog.albums().unwrap_or_default().into_iter().find(|(have, _)| have == key);
    match state.catalog.set_in_album(key, &ids, true) {
        Ok(()) => state.toast(&format!("Added {} to {}", ids.len(), name.map(|(_, name)| name).unwrap_or_default())),
        Err(err) => state.toast(&format!("Could not add to the album: {err}")),
    }
}

pub(super) fn albums_dialog(state: &App, window: &adw::ApplicationWindow) {
    let dialog = adw::Dialog::new();
    dialog.set_title("Albums");
    dialog.set_content_width(460);
    dialog.set_content_height(420);

    let page = adw::PreferencesPage::new();
    let group = adw::PreferencesGroup::new();
    group.set_description(Some(
        "An album can hold photographs from any library. Deleting one leaves the \
         photographs where they are.",
    ));
    let albums = state.catalog.albums().unwrap_or_else(|err| {
        state.toast(&format!("Could not read the albums: {err}"));
        Vec::new()
    });
    if albums.is_empty() {
        let empty = adw::ActionRow::new();
        empty.set_title("No albums yet");
        empty.set_subtitle("Select photographs, right-click, and choose Add to album.");
        group.add(&empty);
    }

    for (key, name) in albums {
        let row = adw::EntryRow::new();
        row.set_text(&name);
        row.set_show_apply_button(true);
        row.connect_apply(glib::clone!(
            #[strong] state,
            #[strong] key,
            move |row| match state.catalog.rename_album(&key, &row.text()) {
                Ok(()) => refresh_picker(&state),
                Err(err) => state.toast(&err),
            }
        ));

        let delete = gtk::Button::from_icon_name("user-trash-symbolic");
        delete.set_tooltip_text(Some("Delete album"));
        delete.set_valign(gtk::Align::Center);
        delete.add_css_class("flat");
        delete.connect_clicked(glib::clone!(
            #[strong] state,
            #[weak] dialog,
            #[weak] window,
            move |_| {
                let confirm = adw::AlertDialog::new(
                    Some(&format!("Delete {name}?")),
                    Some("The photographs stay where they are, with their ratings and edits; only the album goes."),
                );
                confirm.add_response("cancel", "Cancel");
                confirm.add_response("delete", "Delete");
                confirm.set_response_appearance("delete", adw::ResponseAppearance::Destructive);
                confirm.set_default_response(Some("cancel"));
                confirm.set_close_response("cancel");
                let (state, key, dialog, parent) = (state.clone(), key.clone(), dialog.clone(), window.clone());
                confirm.connect_response(None, move |_, response| {
                    if response != "delete" {
                        return;
                    }
                    match state.catalog.delete_album(&key) {
                        Ok(()) => {

                            reload_grid(&state);
                            dialog.close();
                            albums_dialog(&state, &parent);
                        }
                        Err(err) => state.toast(&format!("Could not delete the album: {err}")),
                    }
                });
                confirm.present(Some(&window));
            }
        ));
        row.add_suffix(&delete);
        group.add(&row);
    }
    page.add(&group);

    let bar = adw::ToolbarView::new();
    bar.add_top_bar(&adw::HeaderBar::new());
    bar.set_content(Some(&page));
    dialog.set_child(Some(&bar));
    dialog.present(Some(window));
}
