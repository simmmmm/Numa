use super::*;

thread_local! {

    static CHOSEN: RefCell<(Vec<i64>, Option<i64>, bool)> = const { RefCell::new((Vec::new(), None, false)) };

    static SHOWN: RefCell<Vec<(i64, gtk::Widget)>> = const { RefCell::new(Vec::new()) };

    static HELD: Cell<(bool, bool)> = const { Cell::new((false, false)) };
}

#[derive(Clone)]
pub(super) struct Bar {
    bar: gtk::ActionBar,
    count: gtk::Label,
    open: gtk::Button,
    rename: gtk::Button,
    more: gtk::MenuButton,

    actions: gio::SimpleActionGroup,
    select: Rc<RefCell<Option<gtk::ToggleButton>>>,
}

impl Bar {
    pub(super) fn new() -> Self {
        let bar = gtk::ActionBar::new();
        bar.set_revealed(false);
        let count = gtk::Label::new(None);
        count.add_css_class("heading");
        count.add_css_class("numeric");
        let more = gtk::MenuButton::new();
        more.set_icon_name("view-more-symbolic");
        more.set_tooltip_text(Some("More"));
        let actions = gio::SimpleActionGroup::new();
        Self { bar, count, open: gtk::Button::with_label("Open"), rename: gtk::Button::with_label("Rename…"), more, actions, select: Rc::default() }
    }
}

fn chosen() -> Vec<i64> {
    CHOSEN.with(|chosen| chosen.borrow().0.clone())
}

fn choosing() -> bool {
    CHOSEN.with(|chosen| {
        let chosen = chosen.borrow();
        chosen.2 || !chosen.0.is_empty()
    })
}

pub(super) fn page(state: &App, stacks: &gtk::ScrolledWindow) -> gtk::Box {
    let bar = &state.libraries.bar;
    let clear = gtk::Button::from_icon_name("edit-clear-all-symbolic");
    clear.add_css_class("flat");
    clear.set_tooltip_text(Some("Clear (Esc)"));
    clear.connect_clicked(glib::clone!(
        #[strong] state,
        move |_| let_go(&state)
    ));
    bar.count.set_margin_start(4);
    bar.count.set_margin_end(8);
    let everything = gtk::Button::with_label("Select All");
    everything.connect_clicked(glib::clone!(
        #[strong] state,
        move |_| {
            let all: Vec<i64> = SHOWN.with(|shown| shown.borrow().iter().map(|(id, _)| *id).collect());
            CHOSEN.with(|chosen| chosen.borrow_mut().0 = all);
            paint(&state);
        }
    ));
    let lead = chip_row();
    lead.set_valign(gtk::Align::Center);
    lead.append(&everything);
    bar.bar.pack_start(&clear);
    bar.bar.pack_start(&bar.count);
    bar.bar.pack_start(&lead);
    bar.bar.pack_end(&bar.more);
    bar.bar.pack_end(&bar.rename);
    bar.bar.pack_end(&bar.open);
    bar.open.connect_clicked(glib::clone!(
        #[strong] state,
        move |_| {
            if let [library] = libraries(&state).as_slice() {
                let library = library.clone();
                let_go(&state);
                show_place(&state, Place::Library(library));
            }
        }
    ));
    bar.rename.connect_clicked(glib::clone!(
        #[strong] state,
        move |_| {
            if let [library] = libraries(&state).as_slice() {
                rename_library(&state, library);
            }
        }
    ));
    install_more(state);
    let page = gtk::Box::new(gtk::Orientation::Vertical, 0);
    page.append(stacks);
    page.append(&bar.bar);
    let keys = gtk::EventControllerKey::new();
    keys.connect_key_pressed(glib::clone!(
        #[strong] state,
        move |_, key, _, _| match key == gtk::gdk::Key::Escape && choosing() {
            true => {
                let_go(&state);
                glib::Propagation::Stop
            }
            false => glib::Propagation::Proceed,
        }
    ));
    page.add_controller(keys);

    state.stack.connect_visible_child_name_notify(glib::clone!(
        #[strong] state,
        move |stack| {
            if stack.visible_child_name().as_deref() != Some("libraries") && choosing() {
                let_go(&state);
            }
        }
    ));
    page
}

fn install_more(state: &App) {
    let model = gio::Menu::new();
    let sync = gio::Menu::new();
    let actions = state.libraries.bar.actions.clone();
    for (label, name) in [("Sync on This Device", "sync-here"), ("Sync to Numa Home", "sync-home"), ("Stop Syncing…", "stop-sync")] {
        let item = gio::MenuItem::new(Some(label), Some(&format!("chosen.{name}")));
        item.set_attribute_value("hidden-when", Some(&"action-disabled".to_variant()));
        sync.append_item(&item);
        let action = gio::SimpleAction::new(name, None);
        action.connect_activate(glib::clone!(
            #[strong] state,
            move |action, _| {
                let chosen = libraries(&state);
                let_go(&state);
                match action.name().as_str() {
                    "stop-sync" => home_pair::stop_syncing(&state, chosen),
                    _ => home_pair::start_syncing(&state, chosen),
                }
            }
        ));
        actions.add_action(&action);
    }
    model.append_section(None, &sync);
    let here = gio::Menu::new();
    for (label, name) in [("Remove from Numa…", "chosen.remove"), ("Move to Trash…", "chosen.trash")] {
        let item = gio::MenuItem::new(Some(label), Some(name));
        item.set_attribute_value("hidden-when", Some(&"action-disabled".to_variant()));
        here.append_item(&item);
    }
    model.append_section(None, &here);
    let remove = gio::SimpleAction::new("remove", None);
    remove.connect_activate(glib::clone!(
        #[strong] state,
        move |_, _| confirm_remove(&state)
    ));
    let trash = gio::SimpleAction::new("trash", None);
    trash.connect_activate(glib::clone!(
        #[strong] state,
        move |_, _| confirm_trash(&state)
    ));
    actions.add_action(&remove);
    actions.add_action(&trash);
    let bar = &state.libraries.bar;
    bar.more.insert_action_group("chosen", Some(&actions));
    bar.more.set_menu_model(Some(&model));
}

pub(super) fn select_toggle(state: &App) -> gtk::ToggleButton {
    let select = gtk::ToggleButton::with_label("Select");
    select.set_tooltip_text(Some("Choose libraries with a click (Esc to stop)"));
    select.set_active(CHOSEN.with(|chosen| chosen.borrow().2));
    select.connect_toggled(glib::clone!(
        #[strong] state,
        move |select| match select.is_active() {
            true => {
                CHOSEN.with(|chosen| chosen.borrow_mut().2 = true);
                paint(&state);
            }
            false if choosing() => let_go(&state),
            false => {}
        }
    ));
    state.libraries.bar.select.replace(Some(select.clone()));
    select
}

pub(super) fn forget_stacks() {
    SHOWN.with(|shown| shown.borrow_mut().clear());
}

pub(super) fn choosable(library: &Library, stack: &gtk::Overlay, button: &gtk::Button) {
    let tick = gtk::Image::from_icon_name("object-select-symbolic");
    tick.add_css_class("chosen-tick");
    tick.set_halign(gtk::Align::Start);
    tick.set_valign(gtk::Align::Start);
    tick.set_visible(chosen().contains(&library.id));
    stack.add_overlay(&tick);
    SHOWN.with(|shown| shown.borrow_mut().push((library.id, tick.upcast())));
    let click = gtk::GestureClick::new();
    click.set_propagation_phase(gtk::PropagationPhase::Capture);
    click.connect_pressed(|gesture, _, _, _| {
        let held = gesture.current_event_state();
        HELD.set((held.contains(gtk::gdk::ModifierType::CONTROL_MASK), held.contains(gtk::gdk::ModifierType::SHIFT_MASK)));
    });
    button.add_controller(click);
}

pub(super) fn chose(state: &App, id: i64) -> bool {
    let (ctrl, shift) = HELD.replace((false, false));
    let order: Vec<i64> = SHOWN.with(|shown| shown.borrow().iter().map(|(id, _)| *id).collect());
    let took = choose(id, ctrl, shift, &order);
    if took {
        paint(state);
    }
    took
}

fn choose(id: i64, ctrl: bool, shift: bool, order: &[i64]) -> bool {
    if !(choosing() || ctrl || shift) || !order.contains(&id) {
        return false;
    }
    CHOSEN.with(|chosen| {
        let (ids, anchor, _) = &mut *chosen.borrow_mut();
        let run = anchor.and_then(|anchor| Some((order.iter().position(|one| *one == anchor)?, order.iter().position(|one| *one == id)?)));
        match (shift, run) {
            (true, Some((a, b))) => {
                for one in &order[a.min(b)..=a.max(b)] {
                    if !ids.contains(one) {
                        ids.push(*one);
                    }
                }
            }
            _ => {
                match ids.iter().position(|one| *one == id) {
                    Some(at) => drop(ids.remove(at)),
                    None => ids.push(id),
                }
                *anchor = Some(id);
            }
        }
    });
    true
}

fn let_go(state: &App) {
    CHOSEN.with(|chosen| *chosen.borrow_mut() = (Vec::new(), None, false));
    paint(state);
}

fn libraries(state: &App) -> Vec<Library> {
    let ids = chosen();
    state.libraries.all.borrow().iter().filter(|library| ids.contains(&library.id)).cloned().collect()
}

fn counted(n: usize) -> String {
    match n {
        1 => "1 library".to_string(),
        n => format!("{} libraries", places::grouped(n as i64)),
    }
}

fn paint(state: &App) {
    let ids = chosen();
    SHOWN.with(|shown| shown.borrow().iter().for_each(|(id, tick)| tick.set_visible(ids.contains(id))));
    let bar = &state.libraries.bar;
    let mode = CHOSEN.with(|chosen| chosen.borrow().2);
    if let Some(select) = bar.select.borrow().as_ref() {
        if select.is_active() != (mode || !ids.is_empty()) {
            select.set_active(mode || !ids.is_empty());
        }
    }
    bar.bar.set_revealed(mode || !ids.is_empty());
    bar.count.set_text(&match ids.len() {
        0 => "Choose libraries".to_string(),
        n => format!("{} chosen", counted(n)),
    });

    let chosen = libraries(state);
    let here = chosen.iter().all(|library| !home_sync::is_home(state, library.id));
    let offered = home_pair::offered(state, &chosen);
    for (name, on) in [("sync-here", offered.here), ("sync-home", offered.home), ("stop-sync", offered.stop), ("remove", here), ("trash", here)] {
        if let Some(action) = bar.actions.lookup_action(name).and_downcast::<gio::SimpleAction>() {
            action.set_enabled(on);
        }
    }
    bar.open.set_visible(ids.len() == 1);
    bar.rename.set_visible(ids.len() == 1 && here);
    bar.more.set_visible(!ids.is_empty() && (here || offered != home_pair::Commands::default()));
}

fn names(libraries: &[Library]) -> String {
    match libraries {
        [one] => one.label(),
        many => counted(many.len()),
    }
}

fn close_if_inside(state: &App, libraries: &[Library]) {
    let inside = matches!(
        state.open.borrow().as_ref().map(|open| &open.source),
        Some(Source::Photo { id, .. }) if libraries.iter().any(|library| numa::io::catalog::library_of(*id) == library.id)
    );
    if inside {
        close_editor(state);
    }
}

pub(super) fn rename_library(state: &App, library: &Library) {
    let Some(window) = state.stack.root().and_downcast::<adw::ApplicationWindow>() else { return };
    let dialog = adw::AlertDialog::new(
        Some("Rename Library"),
        Some("The folder itself is renamed, in its place. Other programs that remember the old name will not find it there any more."),
    );
    let field = gtk::Entry::new();
    field.set_text(&library.label());
    field.set_activates_default(true);
    dialog.set_extra_child(Some(&field));
    dialog.add_response("cancel", "Cancel");
    dialog.add_response("rename", "Rename");
    dialog.set_default_response(Some("rename"));
    dialog.set_close_response("cancel");
    let (state, library) = (state.clone(), library.clone());
    dialog.connect_response(None, move |_, response| {
        if response != "rename" {
            return;
        }
        close_if_inside(&state, std::slice::from_ref(&library));
        match state.catalog.rename_library_folder(library.id, field.text().trim()) {
            Ok(_) => {
                let_go(&state);
                reload_libraries(&state);
                if state.stack.visible_child_name().as_deref() == Some("libraries") {
                    show_folders(&state);
                }
            }
            Err(err) => state.toast(&format!("Could not rename: {err}")),
        }
    });
    dialog.present(Some(&window));
}

fn confirm_remove(state: &App) {
    let Some(window) = state.stack.root().and_downcast::<adw::ApplicationWindow>() else { return };
    let chosen = libraries(state);
    if chosen.is_empty() {
        return;
    }
    let dialog = adw::AlertDialog::new(
        Some(&format!("Remove {} from Numa?", names(&chosen))),
        Some("The folders stay where they are, and so do their photographs' ratings, flags and edits: each folder keeps its own catalog. Adding a folder back brings everything back."),
    );
    dialog.add_response("cancel", "Cancel");
    dialog.add_response("remove", "Remove from Numa");
    dialog.set_default_response(Some("cancel"));
    dialog.set_close_response("cancel");
    let state = state.clone();
    dialog.connect_response(None, move |_, response| {
        if response != "remove" {
            return;
        }
        close_if_inside(&state, &chosen);
        let removed = chosen.iter().filter(|library| state.catalog.remove_library(library.id).map_err(|err| log::warn!("{}: {err}", library.path.display())).is_ok()).count();
        let_go(&state);
        reload_libraries(&state);
        show_folders(&state);
        state.toast(&format!("{} removed", counted(removed)));
    });
    dialog.present(Some(&window));
}

fn confirm_trash(state: &App) {
    let Some(window) = state.stack.root().and_downcast::<adw::ApplicationWindow>() else { return };
    let chosen = libraries(state);
    if chosen.is_empty() {
        return;
    }
    if let Some(away) = chosen.iter().find(|library| !library.path.is_dir()) {
        state.toast(&format!("{} is not connected — moving it to the trash waits until it is back", away.label()));
        return;
    }
    let photos: i64 = chosen.iter().filter_map(|library| state.catalog.photo_count(library.id).ok()).sum();
    let dialog = adw::AlertDialog::new(
        Some(&format!("Move {} to the Trash?", names(&chosen))),
        Some(&format!(
            "The folders go to your desktop's trash with their {}, edits and all, where they can be put back. Adding a folder back then brings everything back.",
            home_sync::photographs(photos as usize)
        )),
    );
    dialog.add_response("cancel", "Cancel");
    dialog.add_response("trash", "Move to Trash");
    dialog.set_response_appearance("trash", adw::ResponseAppearance::Destructive);
    dialog.set_default_response(Some("cancel"));
    dialog.set_close_response("cancel");
    let state = state.clone();
    dialog.connect_response(None, move |_, response| {
        if response == "trash" {
            trash(&state, chosen.clone());
        }
    });
    dialog.present(Some(&window));
}

fn trash(state: &App, chosen: Vec<Library>) {
    close_if_inside(state, &chosen);
    for library in &chosen {
        if let Err(err) = state.catalog.remove_library(library.id) {
            log::warn!("{}: {err}", library.path.display());
        }
    }
    let_go(state);
    reload_libraries(state);
    show_folders(state);
    let state = state.clone();
    glib::spawn_future_local(async move {
        let folders: Vec<PathBuf> = chosen.iter().map(|library| library.path.clone()).collect();
        let label = format!("Moving {} to the trash…", counted(folders.len()));
        let Ok(results) = busy(&state, &label, move || {
            folders.iter().map(|folder| gio::File::for_path(folder).trash(gio::Cancellable::NONE).map_err(|err| format!("{}: {err}", folder.display()))).collect::<Vec<_>>()
        })
        .await
        else {
            return;
        };
        let failed: Vec<String> = results.into_iter().filter_map(Result::err).collect();
        failed.iter().for_each(|failure| log::warn!("{failure}"));
        state.toast(&match failed.as_slice() {
            [] => format!("Moved {} to Trash", counted(chosen.len())),
            [only] => format!("Removed from Numa, but not moved to the trash — {only}"),
            many => format!("Moved {}; {} could not be moved and are only removed from Numa", counted(chosen.len() - many.len()), counted(many.len())),
        });
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_press_chooses_as_home_chooses() {
        let order = [1, 2, 3, 4, 5];
        CHOSEN.with(|chosen| *chosen.borrow_mut() = (Vec::new(), None, false));
        assert!(!choose(2, false, false, &order), "a plain press opens");
        assert!(choose(2, true, false, &order), "Ctrl starts");
        assert!(choose(4, false, true, &order), "Shift runs from the last pressed");
        assert_eq!(chosen(), [2, 3, 4]);
        assert!(choose(3, false, false, &order), "once choosing, a press chooses");
        assert_eq!(chosen(), [2, 4]);
        assert!(!choose(9, false, false, &order), "one not on the page is not chosen");
    }
}
