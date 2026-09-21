use super::*;

const TILE: f32 = 132.0;

pub(super) fn build_folders_page(state: &App) -> gtk::ScrolledWindow {
    let scroller = gtk::ScrolledWindow::new();
    scroller.set_hscrollbar_policy(gtk::PolicyType::Never);
    scroller.set_vexpand(true);
    let clamp = adw::Clamp::new();
    clamp.set_maximum_size(1600);
    clamp.set_child(Some(&state.libraries.shelves));
    state.libraries.shelves.set_margin_top(18);
    state.libraries.shelves.set_margin_bottom(36);
    state.libraries.shelves.set_margin_start(24);
    state.libraries.shelves.set_margin_end(24);
    scroller.set_child(Some(&clamp));
    scroller
}

pub(super) fn show_folders(state: &App) {
    let column = &state.libraries.shelves;
    while let Some(child) = column.first_child() {
        column.remove(&child);
    }
    let libraries = state.libraries.all.borrow().clone();
    let (mut loose, mut grouped) = shelve_libraries(&libraries);
    places::in_order(state, &mut loose);
    grouped.iter_mut().for_each(|(_, shelf)| places::in_order(state, shelf));

    let mut first: Vec<Place> = Vec::new();
    if libraries.len() > 1 {
        first.push(Place::Everywhere);
    }
    first.extend(loose.into_iter().map(|library| Place::Library(library.clone())));
    if !first.is_empty() {
        column.append(&shelf(state, "Libraries", &first));
    }
    for (parent, libraries) in grouped {
        let places: Vec<Place> = libraries.into_iter().map(|library| Place::Library(library.clone())).collect();
        column.append(&shelf(state, &parent, &places));
    }
    if libraries.is_empty() {
        let empty = adw::StatusPage::new();
        empty.set_icon_name(Some("folder-pictures-symbolic"));
        empty.set_title("No Libraries Yet");
        empty.set_description(Some("Add a folder of photographs, or import from a card or a camera"));
        column.append(&empty);
    }
    state.stack.set_visible_child_name("folders");
}

pub(super) fn libraries_crumb(state: &App) -> gtk::Button {
    let crumb = gtk::Button::with_label("Libraries");
    crumb.add_css_class("flat");
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

fn shelf(state: &App, heading: &str, places: &[Place]) -> gtk::Box {
    let shelf = gtk::Box::new(gtk::Orientation::Vertical, 12);
    shelf.set_margin_bottom(24);
    let title = gtk::Label::new(Some(heading));
    title.set_xalign(0.0);
    title.add_css_class("title-3");
    shelf.append(&title);

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

fn folder(state: &App, place: &Place) -> gtk::Button {
    let read = places::row(state, place);
    let fan = places::fan(TILE);
    fan.set_halign(gtk::Align::Center);
    places::show_covers(&fan, &read.covers, TILE);

    let name = gtk::Label::new(Some(&match place {
        Place::Everywhere => "All libraries".to_string(),
        Place::Library(library) => library.label(),
        Place::Album(album) => album.clone(),
        Place::Person(person) => person.clone(),
    }));
    name.add_css_class("heading");
    name.set_ellipsize(gtk::pango::EllipsizeMode::End);
    name.set_max_width_chars(22);
    let under = gtk::Label::new(Some(&read.subtitle));
    under.add_css_class("caption");
    under.add_css_class("dim-label");
    under.set_ellipsize(gtk::pango::EllipsizeMode::End);
    under.set_max_width_chars(26);

    let inside = gtk::Box::new(gtk::Orientation::Vertical, 6);
    inside.set_margin_top(12);
    inside.set_margin_bottom(12);
    inside.append(&fan);
    inside.append(&name);
    inside.append(&under);

    let button = gtk::Button::new();
    button.add_css_class("flat");
    button.set_child(Some(&inside));
    button.set_tooltip_text(match place {
        Place::Library(library) => Some(library.path.to_string_lossy().into_owned()),
        _ => None,
    }.as_deref());
    let place = place.clone();
    button.connect_clicked(glib::clone!(
        #[strong] state,
        move |_| {
            state.stack.set_visible_child_name("library");
            choose_place(&state, place.clone());
        }
    ));
    button
}
