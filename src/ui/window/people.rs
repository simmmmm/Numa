use super::*;

pub(super) fn people_dialog(state: &App, window: &adw::ApplicationWindow) {

    if first_time(state, "explained-faces") {
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
        let (state, parent) = (state.clone(), window.clone());
        alert.connect_response(None, move |_, _| {
            mark_seen(&state, "explained-faces");
            people_dialog(&state, &parent);
        });
        alert.present(Some(window));
        return;
    }
    let dialog = adw::Dialog::new();
    dialog.set_title("People");
    dialog.set_content_width(560);
    dialog.set_content_height(640);

    let holder = adw::Bin::new();
    let bar = adw::ToolbarView::new();
    bar.add_top_bar(&adw::HeaderBar::new());
    bar.set_content(Some(&holder));
    dialog.set_child(Some(&bar));
    fill_people(state, &dialog, &holder, true);
    dialog.present(Some(window));
}

pub(super) fn fill_people(state: &App, dialog: &adw::Dialog, holder: &adw::Bin, fetch: bool) {
    let Some(library) = state.libraries.current.borrow().clone() else { return };
    let faces = state.catalog.faces(library.id).unwrap_or_else(|err| {
        log::warn!("could not read the faces: {err}");
        Vec::new()
    });
    if faces.is_empty() {
        let empty = adw::StatusPage::new();
        empty.set_icon_name(Some("avatar-default-symbolic"));
        empty.set_title("No faces yet");
        empty.set_description(Some("Analyse the library to find the people in it."));
        holder.set_child(Some(&empty));
        return;
    }

    let known = state.catalog.known_faces().unwrap_or_default();
    let everyone = state.catalog.people(library.id).unwrap_or_default();

    let names: Vec<Option<String>> = faces
        .iter()
        .map(|face| cull::people::recognise(&face.embedding, &known).map(|(name, _)| name.to_string()))
        .collect();

    let mut wanted: Vec<numa::io::catalog::StoredFace> = Vec::new();

    let page = adw::PreferencesPage::new();

    if !everyone.is_empty() {
        let group = named_group(state, dialog, holder, &faces, &names, &everyone, &mut wanted);
        page.add(&group);
    }

    let group = unnamed_group(state, dialog, holder, &faces, &names, &mut wanted);
    page.add(&group);

    let ignored = state.catalog.ignored_count(library.id).unwrap_or(0);
    if ignored > 0 {
        let aside = adw::PreferencesGroup::new();
        let row = adw::ActionRow::new();
        row.set_title(&format!("{ignored} face(s) set aside as nobody to name"));
        let back = gtk::Button::with_label("Ask again");
        back.set_valign(gtk::Align::Center);
        back.connect_clicked(glib::clone!(
            #[strong] state,
            #[weak] dialog,
            #[weak] holder,
            move |_| {
                if let Some(library) = state.libraries.current.borrow().clone() {
                    if let Err(err) = state.catalog.unignore_all(library.id) {
                        state.toast(&format!("Could not bring them back: {err}"));
                    }
                }
                fill_people(&state, &dialog, &holder, false);
            }
        ));
        row.add_suffix(&back);
        aside.add(&row);
        page.add(&aside);
    }
    holder.set_child(Some(&page));

    if fetch && !wanted.is_empty() {
        fetch_portraits(state, dialog, holder, library.id, wanted);
    }
}

fn picture(face: &numa::io::catalog::StoredFace) -> gtk::Widget {
    let texture = face
        .portrait
        .as_ref()
        .and_then(|bytes| gtk::gdk::Texture::from_bytes(&glib::Bytes::from(bytes)).ok());

    let image = match texture {
        Some(texture) => gtk::Image::from_paintable(Some(&texture)),
        None => gtk::Image::from_icon_name("avatar-default-symbolic"),
    };
    image.set_pixel_size(44);
    image.set_valign(gtk::Align::Center);
    image.add_css_class("face-portrait");
    image.upcast()
}

fn named_group(
    state: &App,
    dialog: &adw::Dialog,
    holder: &adw::Bin,
    faces: &[numa::io::catalog::StoredFace],
    names: &[Option<String>],
    everyone: &[(String, Vec<i64>)],
    wanted: &mut Vec<numa::io::catalog::StoredFace>,
) -> adw::PreferencesGroup {
    let group = adw::PreferencesGroup::new();
    group.set_title("Named");
    group.set_description(Some("Rename someone here and it changes everywhere; give two people the same name and they become one."));
    for (name, photos) in everyone {
        let row = adw::EntryRow::new();
        row.set_title(&format!("{} photograph(s)", photos.len()));
        row.set_text(name);
        row.set_show_apply_button(true);
        let face = faces
            .iter()
            .zip(names)
            .filter(|(_, face_name)| face_name.as_deref().is_some_and(|n| n.eq_ignore_ascii_case(name)))
            .map(|(face, _)| face)
            .find(|face| face.portrait.is_some())
            .or_else(|| {
                faces.iter().zip(names).find(|(_, n)| n.as_deref().is_some_and(|n| n.eq_ignore_ascii_case(name))).map(|(f, _)| f)
            });
        if let Some(face) = face {
            if face.portrait.is_none() {
                wanted.push(face.clone());
            }
            row.add_prefix(&picture(face));
        }

        let show = gtk::Button::with_label("Show");
        show.set_valign(gtk::Align::Center);
        show.add_css_class("flat");
        show.set_tooltip_text(Some("Only the photographs this person is in"));
        show.connect_clicked(glib::clone!(
            #[strong] state,
            #[weak] dialog,
            #[strong] name,
            move |_| {
                state.libraries.filter.borrow_mut().in_one_library();
                state.libraries.filter.borrow_mut().person = Some(name.clone());
                reload_grid(&state);
                dialog.close();
            }
        ));
        row.add_suffix(&show);

        let forget = gtk::Button::from_icon_name("user-trash-symbolic");
        forget.set_valign(gtk::Align::Center);
        forget.add_css_class("flat");
        forget.set_tooltip_text(Some("Forget this name"));
        forget.connect_clicked(glib::clone!(
            #[strong] state,
            #[weak] dialog,
            #[weak] holder,
            #[strong] name,
            move |_| {
                if let Err(err) = state.catalog.rename_person(&name, "") {
                    state.toast(&format!("Could not forget the name: {err}"));
                    return;
                }
                reload_grid(&state);
                fill_people(&state, &dialog, &holder, false);
            }
        ));
        row.add_suffix(&forget);

        row.connect_apply(glib::clone!(
            #[strong] state,
            #[weak] dialog,
            #[weak] holder,
            #[strong] name,
            move |row| {
                if let Err(err) = state.catalog.rename_person(&name, &row.text()) {
                    state.toast(&format!("Could not rename: {err}"));
                    return;
                }
                let state = state.clone();
                glib::idle_add_local_once(move || {
                    reload_grid(&state);
                    fill_people(&state, &dialog, &holder, false);
                });
            }
        ));
        group.add(&row);
    }
    group
}

fn unnamed_group(
    state: &App,
    dialog: &adw::Dialog,
    holder: &adw::Bin,
    faces: &[numa::io::catalog::StoredFace],
    names: &[Option<String>],
    wanted: &mut Vec<numa::io::catalog::StoredFace>,
) -> adw::PreferencesGroup {

    let unnamed: Vec<&numa::io::catalog::StoredFace> =
        faces.iter().zip(names).filter(|(_, name)| name.is_none()).map(|(face, _)| face).collect();
    let embeddings: Vec<[f32; cull::people::LENGTH]> = unnamed.iter().map(|face| face.embedding).collect();
    let mut once = 0usize;
    let group = adw::PreferencesGroup::new();
    group.set_title("Not named yet");
    for members in cull::people::groups(&embeddings) {
        let members: Vec<&numa::io::catalog::StoredFace> = members.iter().map(|index| unnamed[*index]).collect();
        let mut photos: Vec<i64> = members.iter().map(|face| face.photo_id).collect();
        photos.sort_unstable();
        photos.dedup();
        if photos.len() < 2 {
            once += members.len();
            continue;
        }

        let row = adw::EntryRow::new();
        row.set_title(&format!("In {} photographs — who is this?", photos.len()));
        row.set_show_apply_button(true);
        let pictures = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        for face in members.iter().take(3) {
            if face.portrait.is_none() {
                wanted.push((*face).clone());
            }
            pictures.append(&picture(face));
        }
        row.add_prefix(&pictures);

        let faces: Vec<(i64, [f32; cull::people::LENGTH])> =
            members.iter().map(|face| (face.photo_id, face.embedding)).collect();

        let ignore = gtk::Button::from_icon_name("user-trash-symbolic");
        ignore.set_valign(gtk::Align::Center);
        ignore.add_css_class("flat");
        ignore.set_tooltip_text(Some("Nobody to name — stop asking about these faces"));
        ignore.connect_clicked(glib::clone!(
            #[strong] state,
            #[weak] dialog,
            #[weak] holder,
            #[strong] faces,
            move |_| {
                if let Err(err) = state.catalog.ignore_faces(&faces) {
                    state.toast(&format!("Could not set them aside: {err}"));
                    return;
                }
                fill_people(&state, &dialog, &holder, false);
            }
        ));
        row.add_suffix(&ignore);

        let name_them = glib::clone!(
            #[strong] state,
            #[weak] dialog,
            #[weak] holder,
            move |row: &adw::EntryRow| {
                let text = row.text().to_string();
                if text.trim().is_empty() {
                    return;
                }
                for (photo, embedding) in &faces {
                    if let Err(err) = state.catalog.name_face(*photo, embedding, &text) {
                        state.toast(&format!("Could not save the name: {err}"));
                        return;
                    }
                }
                let state = state.clone();
                glib::idle_add_local_once(move || {
                    reload_grid(&state);
                    fill_people(&state, &dialog, &holder, false);
                });
            }
        );
        row.connect_apply(name_them.clone());
        row.connect_entry_activated(name_them);
        group.add(&row);
    }
    if once > 0 {
        group.set_description(Some(&format!(
            "{once} face(s) seen in only one photograph are left out — name those from the photograph's info page."
        )));
    }
    group
}

pub(super) fn fetch_portraits(
    state: &App,
    dialog: &adw::Dialog,
    holder: &adw::Bin,
    library_id: i64,
    wanted: Vec<numa::io::catalog::StoredFace>,
) {
    let paths: HashMap<i64, PathBuf> = state
        .catalog
        .photos(library_id, &Filter::default())
        .unwrap_or_default()
        .into_iter()
        .map(|photo| (photo.id, photo.path))
        .collect();
    let mut by_photo: HashMap<i64, Vec<numa::io::catalog::StoredFace>> = HashMap::new();
    for face in wanted {
        by_photo.entry(face.photo_id).or_default().push(face);
    }
    let work: Vec<(PathBuf, Vec<numa::io::catalog::StoredFace>)> = by_photo
        .into_iter()
        .filter_map(|(photo, faces)| Some((paths.get(&photo)?.clone(), faces)))
        .collect();

    let state = state.clone();
    let dialog = dialog.clone();
    let holder = holder.clone();
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

        if !found.is_empty() && dialog.parent().is_some() {
            fill_people(&state, &dialog, &holder, false);
        }
    });
}
