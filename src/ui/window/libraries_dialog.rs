use super::*;

pub(super) fn libraries_dialog(state: &App, window: &adw::ApplicationWindow) {
    let dialog = adw::Dialog::new();
    dialog.set_title("Libraries");
    dialog.set_content_width(560);
    dialog.set_content_height(520);

    let page = adw::PreferencesPage::new();
    let group = adw::PreferencesGroup::new();
    group.set_title("Folders in the catalog");
    group.set_description(Some(
        "The photographs stay where they are, and so does everything done to \
         them: each folder keeps its own catalog. Removing a folder here only \
         stops showing it.",
    ));

    let add = gtk::Button::from_icon_name("folder-open-symbolic");
    add.set_tooltip_text(Some("Add a folder"));
    add.add_css_class("flat");
    add.connect_clicked(glib::clone!(
        #[strong] state,
        #[weak] window,
        #[weak] dialog,
        move |_| {
            dialog.close();
            add_library_dialog(&state, &window);
        }
    ));
    group.set_header_suffix(Some(&add));

    let libraries = state.libraries.all.borrow().clone();
    if libraries.is_empty() {
        let empty = adw::ActionRow::new();
        empty.set_title("No folders yet");
        empty.set_subtitle("Add one to start.");
        group.add(&empty);
    }

    for library in libraries {
        let counted = state.catalog.photo_count(library.id);
        let row = adw::EntryRow::new();

        set_row_title(&row, &match counted {
            Ok(count) => format!("{count} photo(s) · {}", library.path.display()),
            Err(_) => format!("Not connected · {}", library.path.display()),
        });
        row.set_text(&library.label());
        row.set_show_apply_button(true);

        connect_rename(state, window, &dialog, &row, &library);

        if !library.path.is_dir() {
            let locate = locate_button(state, window, &dialog, &library);
            row.add_suffix(&locate);
        }

        let remove = remove_button(state, window, &dialog, library, counted.ok());
        row.add_suffix(&remove);
        group.add(&row);
    }

    page.add(&group);

    let bar = adw::ToolbarView::new();
    bar.add_top_bar(&adw::HeaderBar::new());
    bar.set_content(Some(&page));
    dialog.set_child(Some(&bar));
    dialog.present(Some(window));
}

fn connect_rename(
    state: &App,
    window: &adw::ApplicationWindow,
    dialog: &adw::Dialog,
    row: &adw::EntryRow,
    library: &Library,
) {
    row.connect_apply(glib::clone!(
        #[strong] state,
        #[strong] library,
        #[weak] dialog,
        #[weak] window,
        move |row| {
            let name = row.text().trim().to_string();
            let confirm = adw::AlertDialog::new(
                Some("Rename the folder?"),
                Some(&format!(
                    "“{}” becomes “{name}” on disk, with everything in it. Other programs \
                     that remember the old name will not find it there any more.",
                    library.path.display(),
                )),
            );
            confirm.add_response("cancel", "Cancel");
            confirm.add_response("rename", "Rename");
            confirm.set_response_appearance("rename", adw::ResponseAppearance::Suggested);
            confirm.set_default_response(Some("cancel"));

            let (state, dialog, parent) = (state.clone(), dialog.clone(), window.clone());
            confirm.connect_response(None, move |confirm, response| {
                confirm.close();
                if response != "rename" {
                    return;
                }

                if state.open.borrow().is_some() {
                    close_editor(&state);
                }
                match state.catalog.rename_library_folder(library.id, &name) {
                    Ok(_) => {
                        reload_libraries(&state);
                        dialog.close();
                        libraries_dialog(&state, &parent);
                    }
                    Err(err) => state.toast(&format!("Could not rename: {err}")),
                }
            });
            confirm.present(Some(&window));
        }
    ));
}

fn locate_button(
    state: &App,
    window: &adw::ApplicationWindow,
    dialog: &adw::Dialog,
    library: &Library,
) -> gtk::Button {
    let locate = gtk::Button::from_icon_name("folder-open-symbolic");
    locate.set_tooltip_text(Some("Find this folder where it is now"));
    locate.set_valign(gtk::Align::Center);
    locate.add_css_class("flat");
    locate.connect_clicked(glib::clone!(
        #[strong] state,
        #[strong] library,
        #[weak] dialog,
        #[weak] window,
        move |_| {
            let picker = gtk::FileDialog::new();
            picker.set_title(&format!("Where is {} now?", library.label()));
            let (state, library) = (state.clone(), library.clone());
            let (dialog, parent) = (dialog.clone(), window.clone());
            glib::spawn_future_local(async move {
                let Ok(file) = picker.select_folder_future(Some(&parent)).await else { return };
                let Some(path) = file.path() else { return };
                match state.catalog.relocate_library(library.id, &path) {
                    Ok(()) => {
                        reload_libraries(&state);
                        select_library(&state, library.id);
                        state.toast(&format!("{} is now {}", library.label(), path.display()));
                        dialog.close();
                        libraries_dialog(&state, &parent);
                    }
                    Err(err) => state.toast(&err),
                }
            });
        }
    ));
    locate
}

fn remove_button(
    state: &App,
    window: &adw::ApplicationWindow,
    dialog: &adw::Dialog,
    library: Library,
    count: Option<i64>,
) -> gtk::Button {
    let remove = gtk::Button::from_icon_name("user-trash-symbolic");
    remove.set_tooltip_text(Some("Remove from the catalog"));
    remove.set_valign(gtk::Align::Center);
    remove.add_css_class("flat");
    remove.connect_clicked(glib::clone!(
        #[strong] state,
        #[weak] dialog,
        #[weak] window,
        move |_| {

            let which = match count {
                Some(count) => format!("The {count} photograph(s)"),
                None => "Its photographs".to_string(),
            };
            let confirm = adw::AlertDialog::new(
                Some(&format!("Remove {}?", library.label())),
                Some(&format!(
                    "{which} stay on disk, and so do their ratings, \
                     flags and edits — they are kept in the folder itself. Adding the \
                     folder back brings everything back.",
                )),
            );
            confirm.add_response("cancel", "Cancel");
            confirm.add_response("remove", "Remove");
            confirm.set_response_appearance("remove", adw::ResponseAppearance::Destructive);
            confirm.set_default_response(Some("cancel"));

            let state = state.clone();
            let dialog = dialog.clone();
            let parent = window.clone();
            confirm.connect_response(None, move |confirm, response| {
                confirm.close();
                if response != "remove" {
                    return;
                }
                match state.catalog.remove_library(library.id) {
                    Ok(()) => {
                        reload_libraries(&state);
                        dialog.close();
                        libraries_dialog(&state, &parent);
                    }
                    Err(err) => state.toast(&format!("Could not remove: {err}")),
                }
            });
            confirm.present(Some(&window));
        }
    ));
    remove
}
