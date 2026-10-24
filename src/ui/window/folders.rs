use super::*;

const TILE: f32 = 132.0;

pub(super) fn build_page(column: &gtk::Box) -> gtk::ScrolledWindow {
    let scroller = gtk::ScrolledWindow::new();
    scroller.set_hscrollbar_policy(gtk::PolicyType::Never);
    scroller.set_vexpand(true);
    let clamp = adw::Clamp::new();
    clamp.set_maximum_size(1600);
    column.set_margin_top(24);
    column.set_margin_bottom(32);
    column.set_margin_start(24);
    column.set_margin_end(24);
    clamp.set_child(Some(column));
    scroller.set_child(Some(&clamp));
    scroller
}

pub(super) fn clear(column: &gtk::Box) {
    while let Some(child) = column.first_child() {
        column.remove(&child);
    }
}

pub(super) fn show_folders(state: &App) {
    let column = &state.libraries.shelves;
    clear(column);
    library_bar::forget_stacks();
    let libraries = state.libraries.all.borrow().clone();

    let (at_home, here): (Vec<Library>, Vec<Library>) = libraries.iter().cloned().partition(|library| home_sync::is_home(state, library.id));
    let (mut loose, mut grouped) = shelve_libraries(&here);
    places::in_order(state, &mut loose);
    places::shelves_in_order(state, &mut grouped);

    if !state.catalog.recall::<bool>(INVITED_HOME).unwrap_or(false) {
        if home::account(state).signed_in() {
            state.catalog.remember(INVITED_HOME, &true);
        } else if !libraries.is_empty() {
            column.append(&home_invite(state));
        }
    }
    if libraries.is_empty() {
        let empty = adw::StatusPage::new();
        no_libraries_yet(state, &empty);
        column.append(&empty);
        state.stack.set_visible_child_name("libraries");
        return;
    }

    let row = commands(&[("Add Folder…", "win.add-folder")]);
    row.append(&library_bar::select_toggle(state));
    column.append(&row);
    let first: Vec<Place> = loose.into_iter().map(|library| Place::Library(library.clone())).collect();
    if !first.is_empty() {
        column.append(&shelf(state, None, &first));
    }
    for (parent, libraries) in grouped {
        let places: Vec<Place> = libraries.into_iter().map(|library| Place::Library(library.clone())).collect();
        column.append(&shelf(state, Some(&parent), &places));
    }
    let mut at_home: Vec<&Library> = at_home.iter().collect();
    places::in_order(state, &mut at_home);
    if !at_home.is_empty() {
        let places: Vec<Place> = at_home.into_iter().map(|library| Place::Library(library.clone())).collect();
        column.append(&shelf(state, Some(home::NUMA_HOME), &places));
    }
    state.stack.set_visible_child_name("libraries");
}

pub(super) fn show_albums(state: &App) {
    let column = &state.libraries.albums;
    clear(column);
    let albums: Vec<Place> = state.catalog.albums().unwrap_or_default().into_iter().map(|(key, _)| Place::Album(key)).collect();
    match albums.is_empty() {
        true => {
            let empty = adw::StatusPage::new();
            empty.set_title("No Albums Yet");
            empty.set_description(Some("Give one a name, then choose its photographs."));
            let new = primary_button("New Album…");
            new.add_css_class("pill");
            new.set_halign(gtk::Align::Center);
            new.set_action_name(Some("win.new-album"));
            empty.set_child(Some(&new));
            column.append(&empty);
        }
        false => {
            column.append(&commands(&[("New Album…", "win.new-album")]));
            column.append(&shelf(state, None, &albums));
        }
    }
    state.stack.set_visible_child_name("albums");
}

fn commands(entries: &[(&str, &str)]) -> gtk::Box {
    let row = chip_row();
    row.set_margin_bottom(16);
    for (label, action) in entries {
        let button = gtk::Button::with_label(label);
        button.set_action_name(Some(action));
        row.append(&button);
    }
    row
}

const INVITED_HOME: &str = "invited-home";

fn home_invite(state: &App) -> gtk::Box {
    let card = gtk::Box::new(gtk::Orientation::Horizontal, 16);
    card.add_css_class("card");
    card.add_css_class("home-invite");
    card.set_margin_bottom(24);
    card.set_halign(gtk::Align::Start);

    let stack = gtk::Overlay::new();
    stack.set_child(Some(&places::fan(64.0)));
    let house = gtk::Image::from_icon_name("go-home-symbolic");
    house.add_css_class("home-badge");
    house.set_halign(gtk::Align::End);
    house.set_valign(gtk::Align::End);
    stack.add_overlay(&house);
    card.append(&stack);

    let words = gtk::Box::new(gtk::Orientation::Vertical, 4);
    words.set_valign(gtk::Align::Center);
    words.set_hexpand(true);
    let label = section_header(home::NUMA_HOME);
    label.set_margin_top(0);
    words.append(&label);
    let title = gtk::Label::new(Some("Keep them at home"));
    title.set_xalign(0.0);
    title.add_css_class("heading");
    words.append(&title);
    let line = gtk::Label::new(Some("Every photograph in the house on your own NAS. Nothing goes to the cloud."));
    line.set_xalign(0.0);
    line.set_wrap(true);
    line.add_css_class("dim-label");
    words.append(&line);
    card.append(&words);

    let set_up = primary_button("Set Up…");
    set_up.set_valign(gtk::Align::Center);
    set_up.connect_clicked(|button| {
        let _ = button.activate_action("win.preferences", None);
    });
    card.append(&set_up);

    let close = gtk::Button::from_icon_name("window-close-symbolic");
    close.add_css_class("flat");
    close.add_css_class("circular");
    close.set_valign(gtk::Align::Start);
    close.set_tooltip_text(Some("Not Now"));
    close.connect_clicked(glib::clone!(
        #[strong] state,
        #[weak] card,
        move |_| {
            state.catalog.remember(INVITED_HOME, &true);
            if let Some(column) = card.parent().and_downcast::<gtk::Box>() {
                column.remove(&card);
            }
        }
    ));
    card.append(&close);
    card
}

pub(super) fn libraries_crumb(state: &App) -> gtk::Button {
    let crumb = gtk::Button::with_label("Libraries");
    crumb.add_css_class("flat");
    crumb.add_css_class("crumb-quiet");
    crumb.set_tooltip_text(Some("All libraries, as folders"));
    crumb.set_valign(gtk::Align::Center);
    crumb.connect_clicked(glib::clone!(
        #[strong] state,
        move |_| {

            if state.stack.visible_child_name().as_deref() == Some("editor") {
                close_editor(&state);
            }
            show_folders(&state);
        }
    ));
    crumb
}

pub(super) fn shelf(state: &App, heading: Option<&str>, places: &[Place]) -> gtk::Box {
    let shelf = gtk::Box::new(gtk::Orientation::Vertical, 12);
    shelf.set_margin_bottom(24);

    if let Some(heading) = heading {
        let title = section_header(heading);
        title.set_margin_top(0);
        shelf.append(&title);
    }

    let flow = gtk::FlowBox::new();
    flow.set_selection_mode(gtk::SelectionMode::None);
    flow.set_homogeneous(true);
    flow.set_column_spacing(12);
    flow.set_row_spacing(12);
    flow.set_min_children_per_line(1);
    flow.set_max_children_per_line(12);
    for place in places {
        flow.append(&folder(state, place));
    }
    shelf.append(&flow);
    shelf
}

fn two_lines(caption: &str) -> String {
    const LINE: usize = 32;
    match caption.rsplit_once(" \u{b7} ") {
        Some((facts, last)) if caption.chars().count() > LINE => format!("{facts}\n{last}"),
        _ => caption.to_string(),
    }
}

fn folder(state: &App, place: &Place) -> gtk::Button {
    let read = places::row(state, place);
    let fan = places::fan(TILE);
    places::show_covers(&fan, &read.covers, TILE);

    let name = gtk::Label::new(Some(&places::name_of(state, place)));
    name.add_css_class("heading");
    name.set_ellipsize(gtk::pango::EllipsizeMode::End);
    name.set_max_width_chars(22);

    let library_id = match place {
        Place::Library(library) => Some(library.id),
        _ => None,
    };
    let at_home = library_id.is_some_and(|id| home_sync::is_home(state, id));
    let linked = library_id.is_some_and(|id| home_sync::way_of(state, id).is_some());
    let said = library_id.and_then(|id| home_sync::sync_said(state, id));
    let stack = gtk::Overlay::new();
    stack.set_halign(gtk::Align::Center);
    stack.set_child(Some(&fan));
    if linked {
        let marks = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        marks.set_halign(gtk::Align::End);
        marks.set_valign(gtk::Align::End);
        if said.is_some() {
            let synced = gtk::Image::from_icon_name("view-refresh-symbolic");
            synced.add_css_class("home-badge");
            synced.set_tooltip_text(Some("Synced"));
            marks.append(&synced);
        }
        let house = gtk::Image::from_icon_name("go-home-symbolic");
        house.add_css_class("home-badge");
        house.set_tooltip_text(Some(home::NUMA_HOME));
        marks.append(&house);
        stack.add_overlay(&marks);
    }
    let prefix = format!("{} \u{b7} ", home::NUMA_HOME);
    let caption = read.subtitle.strip_prefix(prefix.as_str()).filter(|_| at_home).unwrap_or(&read.subtitle).to_string();

    let under = gtk::Label::new(Some(&match &said {
        Some(said) => format!("{caption}\n{said}"),
        None => two_lines(&caption),
    }));
    if let (Some(id), Some(_)) = (library_id, &said) {
        home_sync::follow_caption(id, &under, &format!("{caption}\n"));
    }
    under.add_css_class("caption");
    under.add_css_class("dim-label");
    under.set_wrap(true);
    under.set_wrap_mode(gtk::pango::WrapMode::WordChar);
    under.set_justify(gtk::Justification::Center);
    under.set_max_width_chars(30);

    let inside = gtk::Box::new(gtk::Orientation::Vertical, 6);
    inside.set_margin_top(12);
    inside.set_margin_bottom(12);
    inside.append(&stack);
    inside.append(&name);
    inside.append(&under);

    let button = gtk::Button::new();
    button.add_css_class("flat");
    button.set_child(Some(&inside));
    button.set_tooltip_text(match place {
        Place::Library(library) => Some(library.path.to_string_lossy().into_owned()),
        _ => None,
    }.as_deref());
    if let Place::Library(library) = place {
        library_menu(state, &button, library.clone(), at_home);

        library_bar::choosable(library, &stack, &button);
    }
    let place = place.clone();
    button.connect_clicked(glib::clone!(
        #[strong] state,
        move |_| {
            if matches!(&place, Place::Library(library) if library_bar::chose(&state, library.id)) {
                return;
            }
            show_place(&state, place.clone())
        }
    ));
    button
}

fn library_menu(state: &App, button: &gtk::Button, library: Library, at_home: bool) {
    let click = gtk::GestureClick::new();
    click.set_button(gtk::gdk::BUTTON_SECONDARY);
    click.connect_pressed(glib::clone!(
        #[strong] state,
        #[weak] button,
        move |_, _, x, y| {
            let model = gio::Menu::new();
            let actions = gio::SimpleActionGroup::new();
            let popover = gtk::PopoverMenu::from_model(Some(&model));
            if let Some(label) = home_pair::label(home_pair::commands(&state, &library)) {
                model.append(Some(label), Some("library.sync"));
                let sync = gio::SimpleAction::new("sync", None);
                sync.connect_activate(glib::clone!(
                    #[strong] state,
                    #[strong] library,
                    #[weak] popover,
                    move |_, _| {

                        popover.popdown();
                        let (state, library) = (state.clone(), library.clone());
                        glib::idle_add_local_once(move || home_pair::run(&state, &library));
                    }
                ));
                actions.add_action(&sync);
            }
            match at_home {
                true => {}
                false => {
                    model.append(Some("Remove from Numa…"), Some("library.remove"));
                    let remove = gio::SimpleAction::new("remove", None);
                    remove.connect_activate(glib::clone!(
                        #[strong] state,
                        #[strong] library,
                        #[weak] button,
                        move |_, _| {
                            let Some(window) = button.root().and_downcast::<adw::ApplicationWindow>() else { return };
                            let count = state.catalog.photo_count(library.id).ok();
                            let again = glib::clone!(
                                #[strong] state,
                                move || show_folders(&state)
                            );
                            confirm_remove_library(&state, &window, library.clone(), count, again);
                        }
                    ));
                    actions.add_action(&remove);
                }
            }
            button.insert_action_group("library", Some(&actions));
            popover.set_parent(&button);
            popover.set_has_arrow(false);
            popover.set_pointing_to(Some(&gtk::gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
            popover.connect_closed(|popover| {
                let popover = popover.clone();
                glib::idle_add_local_once(move || popover.unparent());
            });
            popover.popup();
        }
    ));
    button.add_controller(click);
}
