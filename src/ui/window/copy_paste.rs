use super::*;

pub(super) fn fill_presets_menu(state: &App) {

    let menu = &state.copy_paste.presets_menu;
    menu.remove_all();
    menu.append(Some("Apply a preset…"), Some("win.preset-choose"));
    menu.append(Some("Save settings as preset…"), Some("win.preset-save"));
    let manage = gio::Menu::new();
    manage.append(Some("Import presets…"), Some("win.preset-import"));
    manage.append(Some("Import a folder of presets…"), Some("win.preset-import-folder"));
    manage.append(Some("Open presets folder"), Some("win.preset-folder"));
    menu.append_section(None, &manage);
}

pub(super) fn preset_picker(state: &App, window: &adw::ApplicationWindow) {
    let dialog = adw::Dialog::new();
    dialog.set_title("Presets");
    dialog.set_content_width(460);
    dialog.set_content_height(560);

    let column = preset_browser(state, glib::clone!(
        #[weak] dialog,
        move || {
            dialog.close();
        }
    ));
    column.set_margin_start(12);
    column.set_margin_end(12);
    column.set_margin_bottom(12);

    let bar = adw::ToolbarView::new();
    bar.add_top_bar(&adw::HeaderBar::new());
    bar.set_content(Some(&column));
    dialog.set_child(Some(&bar));
    dialog.present(Some(window));
    if let Some(search) = column.first_child() {
        search.grab_focus();
    }
}

pub(super) fn save_preset_dialog(state: &App, window: &adw::ApplicationWindow, source: Document) {
    let dialog = adw::Dialog::new();
    dialog.set_title("Save as preset");
    dialog.set_content_width(420);

    let page = adw::PreferencesPage::new();
    let naming = adw::PreferencesGroup::new();
    let name = adw::EntryRow::new();
    name.set_title("Name");
    naming.add(&name);
    page.add(&naming);

    let group = adw::PreferencesGroup::new();
    group.set_title("What it carries");
    let chosen = parts_checklist(&group, state.copy_paste.parts.get());
    page.add(&group);

    let save = gtk::Button::with_label("Save");
    save.add_css_class("suggested-action");
    save.set_halign(gtk::Align::End);
    save.set_margin_top(12);
    save.connect_clicked(glib::clone!(
        #[strong] state,
        #[weak] dialog,
        #[weak] name,
        move |_| {
            let parts = chosen();
            if !parts.any() {
                state.toast("Choose at least one part for the preset");
                return;
            }
            let mut document = Document::new(String::new());
            document.copy_from(&source, parts);
            let preset = numa::io::presets::Preset { parts, document };
            match numa::io::presets::save(&numa::io::presets::dir(), &name.text(), &preset) {
                Ok(()) => {
                    fill_presets_page(&state);
                    state.toast(&format!("Saved “{}”", name.text().trim()));
                    dialog.close();
                }
                Err(err) => state.toast(&err),
            }
        }
    ));

    let column = gtk::Box::new(gtk::Orientation::Vertical, 0);
    column.append(&page);
    column.append(&save);
    column.set_margin_bottom(12);
    column.set_margin_end(12);

    let bar = adw::ToolbarView::new();
    bar.add_top_bar(&adw::HeaderBar::new());
    bar.set_content(Some(&column));
    dialog.set_child(Some(&bar));
    dialog.present(Some(window));
}

pub(super) fn copy_settings(state: &App) {
    let copied = state.open.borrow().as_ref().map(|photo| photo.document.clone());
    match copied {
        Some(document) => {
            *state.copy_paste.document.borrow_mut() = Some(document);
            state.toast("Settings copied");
        }
        None => state.toast("Open a photo to copy its settings"),
    }
}

pub(super) fn copy_from_selection(state: &App) {
    if state.open.borrow().is_some() {
        copy_settings(state);
        return;
    }

    let Some(child) = selected_cards(state).first().cloned() else {
        state.toast("Select a photo to copy its settings");
        return;
    };
    let Ok(id) = child.widget_name().parse::<i64>() else { return };

    match state.catalog.load_edits(id) {
        Ok(Some(document)) => {
            *state.copy_paste.document.borrow_mut() = Some(document);
            state.toast("Settings copied");
        }

        Ok(None) => state.toast("That photo has no edits to copy"),
        Err(err) => state.toast(&format!("Could not read its settings: {err}")),
    }
}

pub(super) fn paste_settings(state: &App, window: &adw::ApplicationWindow, ask: bool) {
    if state.copy_paste.document.borrow().is_none() {
        state.toast("Nothing copied yet");
        return;
    }

    if !ask {
        apply_clipboard(state);
        return;
    }

    let dialog = adw::Dialog::new();
    dialog.set_title("Paste settings");
    dialog.set_content_width(420);

    let page = adw::PreferencesPage::new();
    let group = adw::PreferencesGroup::new();
    group.set_title("What travels");
    group.set_description(Some(
        "The crop is off by default: it is drawn against one photograph's \
         content and rarely means the same thing on another.",
    ));

    let chosen = parts_checklist(&group, state.copy_paste.parts.get());

    let apply = gtk::Button::with_label("Paste");
    apply.add_css_class("suggested-action");
    apply.set_halign(gtk::Align::End);
    apply.set_margin_top(12);
    apply.connect_clicked(glib::clone!(
        #[strong] state,
        #[weak] dialog,
        move |_| {
            state.copy_paste.parts.set(chosen());
            dialog.close();
            apply_clipboard(&state);
        }
    ));

    let column = gtk::Box::new(gtk::Orientation::Vertical, 0);
    page.add(&group);
    column.append(&page);
    column.append(&apply);
    column.set_margin_bottom(12);
    column.set_margin_end(12);

    let bar = adw::ToolbarView::new();
    bar.add_top_bar(&adw::HeaderBar::new());
    bar.set_content(Some(&column));
    dialog.set_child(Some(&bar));
    dialog.present(Some(window));
}

pub(super) fn parts_checklist(group: &adw::PreferencesGroup, parts: EditParts) -> impl Fn() -> EditParts {
    let rows: Vec<(gtk::Switch, fn(&mut EditParts, bool))> = [
        ("White balance", parts.white_balance, (|p: &mut EditParts, on| p.white_balance = on) as fn(&mut EditParts, bool)),
        ("Tone", parts.tone, |p: &mut EditParts, on| p.tone = on),
        ("Colour", parts.colour, |p: &mut EditParts, on| p.colour = on),
        ("Tone curve", parts.curve, |p: &mut EditParts, on| p.curve = on),
        ("Detail", parts.detail, |p: &mut EditParts, on| p.detail = on),
        ("Crop and rotation", parts.geometry, |p: &mut EditParts, on| p.geometry = on),
    ]
    .into_iter()
    .map(|(title, on, setter)| {
        let row = adw::ActionRow::new();
        row.set_title(title);
        let switch = gtk::Switch::new();
        switch.set_active(on);
        switch.set_valign(gtk::Align::Center);
        row.add_suffix(&switch);
        row.set_activatable_widget(Some(&switch));
        group.add(&row);
        (switch, setter)
    })
    .collect();

    move || {
        let mut chosen = EditParts::nothing();
        for (switch, setter) in &rows {
            setter(&mut chosen, switch.is_active());
        }
        chosen
    }
}

pub(super) fn apply_clipboard(state: &App) {
    let parts = state.copy_paste.parts.get();
    let source = state.copy_paste.document.borrow().clone();
    let Some(source) = source else { return };
    apply_edit(state, &source, parts, "Pasted onto", false);
}

pub(super) fn apply_edit(state: &App, source: &Document, parts: EditParts, done: &str, from_preset: bool) {
    if !parts.any() {
        state.toast("Nothing selected to apply");
        return;
    }

    if state.stack.visible_child_name().as_deref() == Some("editor") {
        {
            let mut open = state.open.borrow_mut();
            let Some(photo) = open.as_mut() else { return };
            if from_preset {

                let before =
                    photo.before_preset.get_or_insert_with(|| photo.document.clone()).clone();
                photo.document = before;
            }
            photo.document.copy_from(source, parts);
        }

        reload_open_document(state);
        state.toast(&format!("{done} this photo"));
        return;
    }

    let selected: Vec<i64> = selected_cards(state)
        .iter()
        .filter_map(|child| child.widget_name().parse::<i64>().ok())
        .collect();

    if selected.is_empty() {
        state.toast("Select photos first");
        return;
    }

    let mut failed = 0;
    for id in &selected {
        let existing = state.catalog.load_edits(*id).ok().flatten();
        let mut document = match (existing, state.grid.cards.borrow().get(id)) {
            (Some(document), _) => document,
            (None, Some((photo, _))) => Document::new(photo.path.to_string_lossy().to_string()),
            (None, None) => continue,
        };
        document.copy_from(source, parts);
        if state.catalog.save_edits(*id, &document).is_err() {
            failed += 1;
        }
    }

    reload_grid(state);
    state.toast(&match failed {
        0 => format!("{done} {} photo(s)", selected.len()),
        n => format!("{done} {}, {n} failed", selected.len() - n),
    });
}

pub(super) fn reload_open_document(state: &App) {
    let loaded = state.open.borrow().as_ref().map(|photo| {
        (photo.document.basic(), photo.document.white_balance.unwrap_or(photo.as_shot),
         photo.document.crop().unwrap_or(([0.0, 0.0, 1.0, 1.0], 0.0)))
    });
    let Some((basic, balance, (_, angle))) = loaded else { return };

    state.applying.set(true);
    state.sliders.write(basic);
    state.mask_overlay.sliders_hold.set(None);
    state.sliders.write_white_balance(balance);
    refresh_slider_marks(state);
    state.crop.rect.set(tool_rect(state).0);
    state.crop.straighten.set_value(angle as f64);
    state.applying.set(false);

    state.light.curve_area.queue_draw();
    state.crop.area.queue_draw();

    fill_segment_masks(state);
    write_mixer(state);
    write_point_colours(state);
    write_grading(state);
    write_perspective(state);
    write_lens(state);
    ai_denoise::write(state);
    refresh_retouch(state);
    refresh_face(state);
    refresh_found(state);
    refresh_masks(state);
    select_mask(state, None);
    refresh_profile_picker(state);

    if let Some(photo) = state.open.borrow_mut().as_mut() {
        photo.view = None;
    }
    request_render(state);
    schedule_history_push(state);
}

#[derive(Clone)]
pub(super) struct State {

    pub(super) document: Rc<RefCell<Option<Document>>>,
    pub(super) parts: Rc<Cell<EditParts>>,

    pub(super) presets_menu: gio::Menu,
}

impl State {
    pub(super) fn new() -> Self {
        Self {
            document: Rc::new(RefCell::new(None)),
            parts: Rc::new(Cell::new(EditParts::default())),
            presets_menu: gio::Menu::new(),
        }
    }
}
