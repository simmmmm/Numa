use super::*;

pub(super) fn preset_browser(state: &App, done: impl Fn() + Clone + 'static) -> gtk::Box {
    let names = numa::io::presets::list(&numa::io::presets::dir());
    let column = gtk::Box::new(gtk::Orientation::Vertical, 8);
    let search = gtk::SearchEntry::new();
    search.set_placeholder_text(Some("Search presets and groups"));
    column.append(&search);

    if names.is_empty() {
        let empty = adw::StatusPage::new();
        empty.set_title("No presets yet");
        empty.set_description(Some("Save the settings of a photograph, or import presets from Lightroom or Capture One."));
        empty.set_vexpand(true);
        column.append(&empty);
    } else {
        let model = gtk::StringList::new(&names.iter().map(String::as_str).collect::<Vec<_>>());
        let filter = gtk::StringFilter::new(Some(gtk::PropertyExpression::new(
            gtk::StringObject::static_type(),
            gtk::Expression::NONE,
            "string",
        )));
        filter.set_ignore_case(true);
        filter.set_match_mode(gtk::StringFilterMatchMode::Substring);
        search.bind_property("text", &filter, "search").build();
        let filtered = gtk::FilterListModel::new(Some(model), Some(filter));
        let selection = gtk::NoSelection::new(Some(filtered.clone()));

        let factory = card_factory(state);
        factory.connect_bind(glib::clone!(
            #[strong] state,
            move |_, item| {
            let Some(item) = item.downcast_ref::<gtk::ListItem>() else { return };
            let Some(name) = item.item().and_downcast::<gtk::StringObject>().map(|s| s.string()) else { return };
            let Some(row) = item.child().and_downcast::<gtk::Box>() else { return };

            row.set_widget_name(&name);
            let (group, title) = match name.split_once('/') {
                Some((group, title)) => (group.to_string(), title.to_string()),
                None => (String::new(), name.to_string()),
            };
            if let Some(label) = row.first_child().and_then(|card| card.next_sibling()).and_downcast::<gtk::Label>() {
                label.set_text(&title);
            }
            if let Some(label) = row.last_child().and_downcast::<gtk::Label>() {
                label.set_visible(!group.is_empty());
                label.set_text(&group);
            }
            let Some(card) = row.first_child().and_downcast::<gtk::Picture>() else { return };

            card.set_paintable(gtk::gdk::Paintable::NONE);
            fill_card(&state, &card, &name);
        }));

        let list = gtk::GridView::new(Some(selection), Some(factory));
        list.set_min_columns(2);
        list.set_max_columns(2);
        list.set_single_click_activate(true);
        list.add_css_class("navigation-sidebar");
        let apply = glib::clone!(
            #[strong] state,
            #[strong] filtered,
            move |position: u32| {
                let Some(name) = filtered.item(position).and_downcast::<gtk::StringObject>().map(|s| s.string()) else {
                    return;
                };
                done();
                let label = name.rsplit('/').next().unwrap_or(&name).to_string();
                match numa::io::presets::load(&numa::io::presets::dir(), &name) {
                    Ok(preset) => apply_edit(&state, &preset.document, preset.parts, &format!("“{label}” applied to"), true),
                    Err(err) => state.toast(&err),
                }
            }
        );
        list.connect_activate({
            let apply = apply.clone();
            move |_, position| apply(position)
        });

        search.connect_activate({
            let filtered = filtered.clone();
            move |_| {
                if filtered.n_items() > 0 {
                    apply(0);
                }
            }
        });

        let scroller = gtk::ScrolledWindow::new();

        scroller.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
        scroller.set_vexpand(true);
        scroller.set_child(Some(&list));
        column.append(&scroller);
    }
    column
}

thread_local! {

    static HOVER: Cell<u64> = const { Cell::new(0) };

    static SHOWING: Cell<bool> = const { Cell::new(false) };
}

const HOVER_REST: u64 = 160;

pub(super) fn preview_preset(state: &App, name: &str) {
    if name.is_empty() || state.open.borrow().is_none() {
        return;
    }
    let booking = HOVER.with(|hover| {
        hover.set(hover.get() + 1);
        hover.get()
    });
    let name = name.to_string();
    glib::timeout_add_local_once(
        std::time::Duration::from_millis(HOVER_REST),
        glib::clone!(
            #[strong] state,
            move || {
                if HOVER.with(|hover| hover.get()) != booking {
                    return;
                }
                let Ok(preset) = numa::io::presets::load(&numa::io::presets::dir(), &name) else {
                    return;
                };
                let paintable = {
                    let open = state.open.borrow();
                    let Some(photo) = open.as_ref() else { return };

                    let mut document =
                        photo.before_preset.clone().unwrap_or_else(|| photo.document.clone());
                    document.copy_from(&preset.document, preset.parts);

                    let scale = photo.proxy.width.max(photo.proxy.height) as f32
                        / photo.full_size.0.max(photo.full_size.1).max(1) as f32;

                    let built;
                    let working = match colour_key(&document) == photo.working_key {
                        true => &*photo.working,
                        false => {
                            built = render::to_working_space(&document, &*photo.proxy, &render_inputs(&document));
                            &built
                        }
                    };
                    crate::ui::pixel_paintable::PixelPaintable::new(texture_from(
                        render::apply_stack(&document, working, scale),
                    ))
                };

                if HOVER.with(|hover| hover.get()) != booking {
                    return;
                }
                state.canvas.set_paintable(Some(&paintable.upcast::<gtk::gdk::Paintable>()));
                SHOWING.set(true);
                apply_zoom(&state);
            }
        ),
    );
}

pub(super) fn end_preview(state: &App) {
    let booked = HOVER.with(|hover| {
        hover.set(hover.get() + 1);
        hover.get()
    });
    let _ = booked;

    if SHOWING.replace(false) && state.open.borrow().is_some() {
        render_current(state);
    }
}

pub(super) fn edit_here(state: &App) -> Result<Document, String> {
    if let Some(photo) = state.open.borrow().as_ref() {
        return Ok(photo.document.clone());
    }
    let id = selected_cards(state)
        .first()
        .and_then(|child| child.widget_name().parse::<i64>().ok())
        .ok_or("Select a photo to make a preset from")?;
    state.catalog.load_edits(id)?.ok_or_else(|| "That photo has no edits to keep".to_string())
}

pub(super) fn install_preset_actions(state: &App, window: &adw::ApplicationWindow) {
    let apply = gio::SimpleAction::new("preset-choose", None);
    apply.connect_activate(glib::clone!(
        #[strong] state,
        #[weak] window,
        move |_, _| preset_picker(&state, &window)
    ));

    let save = gio::SimpleAction::new("preset-save", None);
    save.connect_activate(glib::clone!(
        #[strong] state,
        #[weak] window,
        move |_, _| match edit_here(&state) {
            Ok(source) => save_preset_dialog(&state, &window, source),
            Err(err) => state.toast(&err),
        }
    ));

    let import = gio::SimpleAction::new("preset-import", None);
    import.connect_activate(glib::clone!(
        #[strong] state,
        #[weak] window,
        move |_, _| {
            let dialog = gtk::FileDialog::new();
            dialog.set_title("Import presets");
            let filter = gtk::FileFilter::new();
            filter.set_name(Some("Presets — Numa, Lightroom, Capture One"));
            for suffix in ["json", "xmp", "lrtemplate", "costyle", "costylepack"] {
                filter.add_suffix(suffix);
            }
            let filters = gio::ListStore::new::<gtk::FileFilter>();
            filters.append(&filter);
            dialog.set_filters(Some(&filters));
            let (state, parent) = (state.clone(), window.clone());
            dialog.open_multiple(Some(&window), gio::Cancellable::NONE, move |chosen| {
                let Ok(files) = chosen else { return };
                import_presets(&state, &parent, chosen_paths(&files));
            });
        }
    ));
    let import_folder = gio::SimpleAction::new("preset-import-folder", None);
    import_folder.connect_activate(glib::clone!(
        #[strong] state,
        #[weak] window,
        move |_, _| {
            let dialog = gtk::FileDialog::new();
            dialog.set_title("Import a folder of presets");
            let (state, parent) = (state.clone(), window.clone());
            dialog.select_multiple_folders(Some(&window), gio::Cancellable::NONE, move |chosen| {
                let Ok(folders) = chosen else { return };
                import_presets(&state, &parent, chosen_paths(&folders));
            });
        }
    ));

    let folder = gio::SimpleAction::new("preset-folder", None);
    folder.connect_activate(glib::clone!(
        #[weak] window,
        move |_, _| {
            let dir = numa::io::presets::dir();
            if let Err(err) = std::fs::create_dir_all(&dir) {
                log::warn!("could not create {}: {err}", dir.display());
            }
            gtk::FileLauncher::new(Some(&gio::File::for_path(&dir))).launch(
                Some(&window),
                None::<&gio::Cancellable>,
                |result| {
                    if let Err(err) = result {
                        log::warn!("could not open the presets folder: {err}");
                    }
                },
            );
        }
    ));

    for action in [&apply, &save, &import, &import_folder, &folder] {
        window.add_action(action);
    }
    fill_presets_menu(state);
}

fn chosen_paths(files: &gio::ListModel) -> Vec<PathBuf> {
    (0..files.n_items())
        .filter_map(|index| files.item(index).and_downcast::<gio::File>()?.path())
        .collect()
}

fn import_presets(state: &App, window: &adw::ApplicationWindow, paths: Vec<PathBuf>) {
    if paths.is_empty() {
        return;
    }
    let (state, window) = (state.clone(), window.clone());
    glib::spawn_future_local(async move {
        let Ok(imported) = busy(&state, "Importing presets…", move || {
            numa::io::presets::import(&numa::io::presets::dir(), &paths)
        })
        .await
        else {
            state.toast("Importing failed — see the log");
            return;
        };
        for refused in &imported.refused {
            log::info!("preset not imported: {refused}");
        }
        fill_presets_page(&state);

        let mut said = match imported.presets {
            1 => "Imported 1 preset".to_string(),
            n => format!("Imported {n} presets"),
        };
        if imported.existing > 0 {
            said.push_str(&format!(" · {} already there", imported.existing));
        }
        if !imported.refused.is_empty() {
            said.push_str(&format!(" · {} not usable", imported.refused.len()));
        }

        if imported.ignored.is_empty() {
            state.toast(&said);
            return;
        }
        let mut lines: Vec<(&str, usize)> = imported.ignored.iter().map(|(what, n)| (*what, *n)).collect();
        lines.sort_by(|a, b| b.1.cmp(&a.1));
        let body = format!(
            "Translated presets come close rather than exactly: the sliders mean \
             slightly different things in each application.\n\nNot carried over:\n{}",
            lines.iter().map(|(what, n)| format!("• {what} — in {n}")).collect::<Vec<_>>().join("\n")
        );
        let alert = adw::AlertDialog::new(Some(&said), Some(&body));
        alert.add_response("ok", "OK");
        alert.present(Some(&window));
    });
}

fn card_factory(state: &App) -> gtk::SignalListItemFactory {
let factory = gtk::SignalListItemFactory::new();
    factory.connect_setup(glib::clone!(
        #[strong] state,
        move |_, item| {
        let row = gtk::Box::new(gtk::Orientation::Vertical, 2);
        row.set_margin_top(6);
        row.set_margin_bottom(6);

        let card = gtk::Picture::new();

        card.set_size_request(-1, CARD.1);
        card.set_content_fit(gtk::ContentFit::Cover);
        card.add_css_class("preset-card");
        row.append(&card);
        let title = gtk::Label::new(None);
        title.set_xalign(0.0);
        title.set_ellipsize(gtk::pango::EllipsizeMode::End);

        title.set_max_width_chars(12);
        let group = gtk::Label::new(None);
        group.set_xalign(0.0);
        group.set_ellipsize(gtk::pango::EllipsizeMode::End);
        group.set_max_width_chars(12);
        group.add_css_class("dim-label");
        group.add_css_class("caption");
        row.append(&title);
        row.append(&group);

        let hover = gtk::EventControllerMotion::new();
        hover.connect_enter(glib::clone!(
            #[strong] state,
            #[weak] row,
            move |_, _, _| preview_preset(&state, &row.widget_name())
        ));
        hover.connect_leave(glib::clone!(
            #[strong] state,
            move |_| end_preview(&state)
        ));
        row.add_controller(hover);

        item.downcast_ref::<gtk::ListItem>().map(|item| item.set_child(Some(&row)));
    }));

    factory
}

const CARD: (i32, i32) = (130, 87);
const CARD_EDGE: u32 = 360;

thread_local! {

    static CANVAS: RefCell<Option<(String, LinearImage)>> = const { RefCell::new(None) };

    static CARDS: RefCell<HashMap<String, gtk::gdk::Texture>> = RefCell::new(HashMap::new());
}

pub(super) fn forget_cards(state: &App) {
    let key = state.open.borrow().as_ref().map(|photo| {
        let stack = photo.before_preset.as_ref().unwrap_or(&photo.document);
        format!("{}\u{1f}{}", source_key(&photo.source), serde_json::to_string(stack).unwrap_or_default())
    });
    let stale = CANVAS.with(|canvas| match (&key, canvas.borrow().as_ref()) {
        (Some(key), Some((was, _))) => key != was,
        (None, None) => false,
        _ => true,
    });
    if stale {
        CANVAS.with(|canvas| *canvas.borrow_mut() = None);
        CARDS.with(|cards| cards.borrow_mut().clear());
    }
}

fn source_key(source: &Source) -> String {
    match source {
        Source::Photo { id, .. } => format!("photo:{id}"),
        Source::Bracket { paths } => format!("bracket:{}", paths.len()),
    }
}

fn render_card(state: &App, name: &str) -> Option<gtk::gdk::Texture> {
    if let Some(texture) = CARDS.with(|cards| cards.borrow().get(name).cloned()) {
        return Some(texture);
    }
    let preset = numa::io::presets::load(&numa::io::presets::dir(), name).ok()?;

    let open = state.open.borrow();
    let photo = open.as_ref()?;
    let key = format!(
        "{}\u{1f}{}",
        source_key(&photo.source),
        serde_json::to_string(photo.before_preset.as_ref().unwrap_or(&photo.document)).unwrap_or_default()
    );
    let small = CANVAS.with(|canvas| {
        let mut canvas = canvas.borrow_mut();
        if canvas.as_ref().map(|(was, _)| was != &key).unwrap_or(true) {
            let small = photo.proxy.downscaled(CARD_EDGE).unwrap_or_else(|| (*photo.proxy).clone());
            *canvas = Some((key, small));
        }
        canvas.as_ref().map(|(_, image)| image.clone())
    })?;

    let mut document = photo.before_preset.clone().unwrap_or_else(|| photo.document.clone());
    document.copy_from(&preset.document, preset.parts);
    let rendered = render::develop(&document, &small, &render_inputs(&document));
    let texture = texture_from(rendered);
    CARDS.with(|cards| cards.borrow_mut().insert(name.to_string(), texture.clone()));
    Some(texture)
}

fn fill_card(state: &App, card: &gtk::Picture, name: &str) {
    if let Some(texture) = CARDS.with(|cards| cards.borrow().get(name).cloned()) {
        card.set_paintable(Some(&texture));
        return;
    }
    let name = name.to_string();
    glib::idle_add_local_once(glib::clone!(
        #[strong] state,
        #[weak] card,
        move || {
            let Some(row) = card.parent() else { return };
            if row.widget_name() != name {
                return;
            }

            if let Some(texture) = timed("a preset card", || render_card(&state, &name)) {
                card.set_paintable(Some(&texture));
            }
        }
    ));
}

pub(super) fn fill_presets_page(state: &App) {

    forget_cards(state);
    let page = &state.panel.presets_page;
    while let Some(child) = page.first_child() {
        page.remove(&child);
    }
    let browser = preset_browser(state, || {});
    browser.set_vexpand(true);
    page.append(&browser);

    let actions = gtk::FlowBox::new();
    actions.set_selection_mode(gtk::SelectionMode::None);
    actions.set_column_spacing(6);
    actions.set_row_spacing(6);
    actions.set_margin_top(6);
    for (label, action) in [
        ("Save current…", "win.preset-save"),
        ("Import…", "win.preset-import"),
        ("Import folder…", "win.preset-import-folder"),
        ("Open folder", "win.preset-folder"),
    ] {
        let button = gtk::Button::with_label(label);
        button.set_action_name(Some(action));
        actions.append(&button);
    }
    page.append(&actions);
}
