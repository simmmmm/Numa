use super::*;
use numa::io::catalog::StoredFace;

pub(super) fn show_people(state: &App) {

    if first_time(state, "explained-faces") {
        mark_seen(state, "explained-faces");
        if let Some(window) = state.stack.root().and_downcast::<adw::ApplicationWindow>() {
            let alert = adw::AlertDialog::new(
                Some("Faces stay on this computer"),
                Some(
                    "Numa finds and recognises faces with models that run here. Nothing is \
                     uploaded, and nothing leaves this computer.\n\nThe names you give are kept \
                     in the library's own .numa folder — so they travel with the folder when it \
                     is copied or shared.",
                ),
            );
            alert.add_response("ok", "Continue");
            alert.present(Some(&window));
        }
    }
    fill_people(state, true);
    state.stack.set_visible_child_name("people");
}

struct Faces {

    each: Vec<(Vec<StoredFace>, Vec<Option<String>>)>,
    aside: i64,
}

fn read_faces(state: &App) -> Faces {

    let named = state.catalog.named_faces().unwrap_or_default();
    let known = state.catalog.known_from(&named).unwrap_or_default();
    let mut each = Vec::new();
    let mut aside = 0;
    for library in state.libraries.all.borrow().iter() {
        let Ok(faces) = state.catalog.faces(library.id) else { continue };
        aside += state.catalog.ignored_count(library.id).unwrap_or(0);
        let names = faces.iter().map(|face| cull::people::recognise(&face.embedding, &known).map(|(name, _)| name.to_string())).collect();
        each.push((faces, names));
    }
    Faces { each, aside }
}

pub(super) fn fill_people(state: &App, fetch: bool) {

    today::faces_changed();
    let column = &state.libraries.people;
    clear(column);
    let read = read_faces(state);
    let names = state.catalog.names().unwrap_or_default();
    let mut groups: Vec<(Vec<StoredFace>, usize)> = Vec::new();
    for (faces, who) in &read.each {
        for (members, photos) in numa::io::analysis::unnamed_groups(faces, who).0 {
            groups.push((members.iter().map(|&at| faces[at].clone()).collect(), photos.len()));
        }
    }
    groups.sort_by_key(|(_, photos)| std::cmp::Reverse(*photos));
    if names.is_empty() && groups.is_empty() {
        let empty = adw::StatusPage::new();
        empty.set_title("No People Yet");
        empty.set_description(Some("People show here once Analyse has found the faces in the photographs."));
        column.append(&empty);
        return;
    }
    let mut wanted: Vec<StoredFace> = Vec::new();
    if !names.is_empty() {
        let flow = gtk::FlowBox::new();
        flow.set_selection_mode(gtk::SelectionMode::None);
        flow.set_homogeneous(true);
        flow.set_column_spacing(12);
        flow.set_row_spacing(12);
        flow.set_max_children_per_line(12);
        for name in &names {

            let theirs: Vec<&StoredFace> = read
                .each
                .iter()
                .flat_map(|(faces, who)| faces.iter().zip(who).filter(|(_, who)| who.as_deref().is_some_and(|who| who.eq_ignore_ascii_case(name))).map(|(face, _)| face))
                .collect();
            let face = theirs.iter().find(|face| face.portrait.is_some()).or(theirs.first()).copied();
            if let Some(face) = face.filter(|face| face.portrait.is_none()) {
                wanted.push(face.clone());
            }
            flow.append(&person(state, name, face));
        }
        column.append(&flow);
    }
    if !groups.is_empty() {
        let label = section_header("Not named yet");
        label.set_margin_top(32);
        label.set_margin_bottom(12);
        column.append(&label);
        for (members, photos) in &groups {
            wanted.extend(members.iter().take(3).filter(|face| face.portrait.is_none()).cloned());
            column.append(&name_row(state, members, *photos));
        }
    }
    if read.aside > 0 {
        let line = gtk::Box::new(gtk::Orientation::Horizontal, 12);
        line.set_margin_top(24);
        let said = gtk::Label::new(Some(&format!("{} set aside as nobody to name", match read.aside {
            1 => "1 face".to_string(),
            n => format!("{} faces", places::grouped(n)),
        })));
        said.add_css_class("dim-label");
        let back = gtk::Button::with_label("Ask Again");
        back.connect_clicked(glib::clone!(
            #[strong] state,
            move |_| {
                for library in state.libraries.all.borrow().iter() {
                    if let Err(err) = state.catalog.unignore_all(library.id) {
                        log::warn!("could not bring the faces back: {err}");
                    }
                }
                fill_people(&state, false);
            }
        ));
        line.append(&said);
        line.append(&back);
        column.append(&line);
    }
    if fetch && !wanted.is_empty() {
        fetch_portraits(state, wanted);
    }
}

fn picture(face: Option<&StoredFace>, size: i32) -> gtk::Image {
    let texture = face
        .and_then(|face| face.portrait.as_ref())
        .and_then(|bytes| gtk::gdk::Texture::from_bytes(&glib::Bytes::from(bytes)).ok());

    let image = match texture {
        Some(texture) => gtk::Image::from_paintable(Some(&texture)),
        None => gtk::Image::from_icon_name("avatar-default-symbolic"),
    };
    image.set_pixel_size(size);
    image.set_valign(gtk::Align::Center);
    image.set_overflow(gtk::Overflow::Hidden);
    image.add_css_class("face-portrait");
    image
}

fn person(state: &App, name: &str, face: Option<&StoredFace>) -> gtk::Button {
    let count = places::row(state, &Place::Person(name.to_string())).count;
    let inside = gtk::Box::new(gtk::Orientation::Vertical, 4);
    inside.set_margin_top(12);
    inside.set_margin_bottom(12);
    let picture = picture(face, 120);
    picture.set_halign(gtk::Align::Center);
    picture.set_margin_bottom(4);
    let title = gtk::Label::new(Some(name));
    title.add_css_class("heading");
    title.set_ellipsize(gtk::pango::EllipsizeMode::End);
    let caption = gtk::Label::new(Some(&places::photos(count as usize)));
    caption.add_css_class("caption");
    caption.add_css_class("dim-label");
    inside.append(&picture);
    inside.append(&title);
    inside.append(&caption);
    let button = gtk::Button::new();
    button.add_css_class("flat");
    button.set_child(Some(&inside));
    let name = name.to_string();
    button.connect_clicked(glib::clone!(
        #[strong] state,
        #[strong] name,
        move |_| show_place(&state, Place::Person(name.clone()))
    ));
    person_menu(state, &button, name);
    button
}

fn person_menu(state: &App, button: &gtk::Button, name: String) {
    let click = gtk::GestureClick::new();
    click.set_button(gtk::gdk::BUTTON_SECONDARY);
    click.connect_pressed(glib::clone!(
        #[strong] state,
        #[weak] button,
        move |_, _, x, y| {
            let model = gio::Menu::new();
            model.append(Some("Rename…"), Some("person.rename"));
            model.append(Some("Forget Name"), Some("person.forget"));
            let actions = gio::SimpleActionGroup::new();
            let rename = gio::SimpleAction::new("rename", None);
            rename.connect_activate(glib::clone!(
                #[strong] state,
                #[strong] name,
                move |_, _| rename_person(&state, &name)
            ));

            let forget = gio::SimpleAction::new("forget", None);
            forget.connect_activate(glib::clone!(
                #[strong] state,
                #[strong] name,
                move |_, _| match state.catalog.rename_person(&name, "") {
                    Ok(()) => renamed(&state),
                    Err(err) => state.toast(&format!("Could not forget the name: {err}")),
                }
            ));
            actions.add_action(&rename);
            actions.add_action(&forget);
            button.insert_action_group("person", Some(&actions));
            let popover = gtk::PopoverMenu::from_model(Some(&model));
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

fn rename_person(state: &App, name: &str) {
    let Some(window) = state.stack.root().and_downcast::<adw::ApplicationWindow>() else { return };
    let entry = gtk::Entry::new();
    entry.set_text(name);
    entry.set_activates_default(true);
    let ask = adw::AlertDialog::new(
        Some("Rename"),
        Some("It changes everywhere; give two people the same name and they become one."),
    );
    ask.set_extra_child(Some(&entry));
    ask.add_response("cancel", "Cancel");
    ask.add_response("rename", "Rename");
    ask.set_default_response(Some("rename"));
    ask.set_close_response("cancel");
    let (state, name, typed) = (state.clone(), name.to_string(), entry.clone());
    ask.connect_response(None, move |_, response| {
        let to = typed.text().trim().to_string();
        if response != "rename" || to.is_empty() || to == name {
            return;
        }
        match state.catalog.rename_person(&name, &to) {
            Ok(()) => renamed(&state),
            Err(err) => state.toast(&format!("Could not rename: {err}")),
        }
    });
    ask.present(Some(&window));
    entry.grab_focus();
}

fn renamed(state: &App) {
    let state = state.clone();
    glib::idle_add_local_once(move || {
        reload_grid(&state);
        fill_people(&state, false);
    });
}

fn name_row(state: &App, members: &[StoredFace], photos: usize) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    row.set_margin_bottom(12);
    let faces = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    for face in members.iter().take(3) {
        faces.append(&picture(Some(face), 48));
    }
    let ask = gtk::Box::new(gtk::Orientation::Vertical, 4);
    ask.set_hexpand(true);
    ask.set_valign(gtk::Align::Center);
    let caption = gtk::Label::new(Some(&format!("In {}", places::photos(photos))));
    caption.set_xalign(0.0);
    caption.add_css_class("caption");
    caption.add_css_class("dim-label");
    let field = gtk::Entry::new();
    field.set_placeholder_text(Some("Who is this?"));
    ask.append(&caption);
    ask.append(&field);
    let name = gtk::Button::with_label("Name");
    name.set_valign(gtk::Align::Center);

    let nobody = gtk::Button::with_label("Nobody");
    nobody.set_valign(gtk::Align::Center);
    nobody.set_tooltip_text(Some("Nobody to name — stop asking about these faces"));
    let faces_of: Vec<(i64, [f32; cull::people::LENGTH])> = members.iter().map(|face| (face.photo_id, face.embedding)).collect();
    nobody.connect_clicked(glib::clone!(
        #[strong] state,
        #[strong] faces_of,
        move |_| match state.catalog.ignore_faces(&faces_of) {
            Ok(()) => fill_people(&state, false),
            Err(err) => state.toast(&format!("Could not set them aside: {err}")),
        }
    ));
    let name_them = glib::clone!(
        #[strong] state,
        #[weak] field,
        move || {
            let text = field.text().trim().to_string();
            if text.is_empty() {
                return;
            }
            for (photo, embedding) in &faces_of {
                if let Err(err) = state.catalog.name_face(*photo, embedding, &text) {
                    state.toast(&format!("Could not save the name: {err}"));
                    return;
                }
            }
            renamed(&state);
        }
    );
    field.connect_activate(glib::clone!(
        #[strong] name_them,
        move |_| name_them()
    ));
    name.connect_clicked(move |_| name_them());
    row.append(&faces);
    row.append(&ask);
    row.append(&name);
    row.append(&nobody);
    row
}

fn fetch_portraits(state: &App, wanted: Vec<StoredFace>) {
    let mut libraries: Vec<i64> = wanted.iter().map(|face| numa::io::catalog::library_of(face.photo_id)).collect();
    libraries.sort_unstable();
    libraries.dedup();
    let paths: HashMap<i64, PathBuf> = libraries
        .into_iter()
        .flat_map(|library| state.catalog.photos(library, &Filter::default()).unwrap_or_default())
        .map(|photo| (photo.id, photo.path))
        .collect();
    let mut by_photo: HashMap<i64, Vec<StoredFace>> = HashMap::new();
    for face in wanted {
        by_photo.entry(face.photo_id).or_default().push(face);
    }
    let work: Vec<(PathBuf, Vec<StoredFace>)> = by_photo
        .into_iter()
        .filter_map(|(photo, faces)| Some((paths.get(&photo)?.clone(), faces)))
        .collect();

    let state = state.clone();
    glib::spawn_future_local(async move {
        let found = gtk::gio::spawn_blocking(move || {
            use rayon::prelude::*;
            work.par_iter()
                .flat_map_iter(|(path, faces)| {
                    let Ok(image) = raw::load_scaled(path, FACE_EDGE) else { return Vec::new() };
                    let detected = cull::faces::detect(&image).unwrap_or_default();
                    faces
                        .iter()
                        .filter_map(|stored| {
                            let best = detected
                                .iter()
                                .filter_map(|face| Some((face, cull::people::embed(&image, face)?)))
                                .max_by(|a, b| {
                                    cull::people::likeness(&a.1, &stored.embedding)
                                        .total_cmp(&cull::people::likeness(&b.1, &stored.embedding))
                                })?;
                            (cull::people::likeness(&best.1, &stored.embedding) >= 0.9)
                                .then(|| cull::people::portrait_jpeg(&image, best.0))
                                .flatten()
                                .map(|jpeg| (stored.id, jpeg))
                        })
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>()
        })
        .await;
        let Ok(found) = found else { return };
        for (face, jpeg) in &found {
            if let Err(err) = state.catalog.set_portrait(*face, jpeg) {
                log::warn!("could not keep a face's picture: {err}");
            }
        }

        if !found.is_empty() && state.stack.visible_child_name().as_deref() == Some("people") {
            fill_people(&state, false);
        }
    });
}
