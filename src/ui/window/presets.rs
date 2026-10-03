use super::*;

pub(super) struct PresetOn {
    pub(super) base: Document,
    pub(super) left: EditState,
    pub(super) name: String,
    pub(super) preset: numa::io::presets::Preset,
    pub(super) amount: f64,
}

impl OpenPhoto {

    pub(super) fn preset_base(&self) -> &Document {
        match &self.before_preset {
            Some(on) if on.left == EditState::of(&self.document) => &on.base,
            _ => &self.document,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Kind {
    Presets,
    Luts,
}

const NO_LUT: &str = "None";

impl Kind {
    fn names(self) -> Vec<String> {
        match self {
            Kind::Presets => numa::io::presets::list(&numa::io::presets::dir()),
            Kind::Luts => std::iter::once(NO_LUT.to_string()).chain(numa::io::luts::list()).collect(),
        }
    }

    pub(super) fn folder(self) -> PathBuf {
        match self {
            Kind::Presets => numa::io::presets::dir(),
            Kind::Luts => numa::core::paths::luts_dir(),
        }
    }

    fn file(self, name: &str) -> Option<PathBuf> {
        match self {
            Kind::Presets => Some(self.folder().join(format!("{name}.json"))),
            Kind::Luts if name == NO_LUT => None,
            Kind::Luts => Some(self.folder().join(name)),
        }
    }

    pub(super) fn title(self, name: &str) -> (String, String) {
        match (self, name.split_once('/')) {
            (Kind::Presets, Some((group, title))) => (title.to_string(), group.to_string()),
            (Kind::Presets, None) => (name.to_string(), String::new()),
            (Kind::Luts, _) => (name.rsplit_once('.').map_or(name, |(stem, _)| stem).to_string(), String::new()),
        }
    }

    fn on(self, photo: &OpenPhoto, name: &str) -> Option<Document> {
        match self {
            Kind::Presets => {
                let preset = numa::io::presets::load(&self.folder(), name).ok()?;
                let mut document = photo.preset_base().clone();
                document.copy_from(&preset.document, preset.parts);
                Some(document)
            }
            Kind::Luts => {
                let mut document = photo.document.clone();
                document.lut = (name != NO_LUT).then(|| numa::core::lut::LutChoice { name: name.to_string(), amount: 100.0 });
                Some(document)
            }
        }
    }

    fn apply(self, state: &App, name: &str) {
        match self {
            Kind::Presets => {
                let label = name.rsplit('/').next().unwrap_or(name).to_string();
                match numa::io::presets::load(&self.folder(), name) {
                    Ok(preset) => apply_edit(state, &preset.document, preset.parts, &format!("“{label}” applied to"), Some(&label)),
                    Err(err) => state.toast(&err),
                }
            }
            Kind::Luts => lut::choose(state, (name != NO_LUT).then_some(name)),
        }
    }

    fn refresh(self, state: &App) {
        match self {
            Kind::Presets => fill_presets_page(state),
            Kind::Luts => lut::fill(state),
        }
    }
}

pub(super) fn preset_browser(state: &App, done: impl Fn() + Clone + 'static) -> gtk::Box {
    browser(state, Kind::Presets, done)
}

pub(super) fn browser(state: &App, kind: Kind, done: impl Fn() + Clone + 'static) -> gtk::Box {
    let names = kind.names();
    let column = gtk::Box::new(gtk::Orientation::Vertical, 8);
    let search = gtk::SearchEntry::new();
    search.set_placeholder_text(Some(match kind {
        Kind::Presets => "Search presets and groups",
        Kind::Luts => "Search LUTs",
    }));
    column.append(&search);

    if names.is_empty() {
        let empty = adw::StatusPage::new();
        empty.set_title("No Presets Yet");
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

        let factory = card_factory(state, kind);
        factory.connect_bind(glib::clone!(
            #[strong] state,
            move |_, item| {
            let Some(item) = item.downcast_ref::<gtk::ListItem>() else { return };
            let Some(name) = item.item().and_downcast::<gtk::StringObject>().map(|s| s.string()) else { return };
            let Some(row) = item.child().and_downcast::<gtk::Box>() else { return };

            row.set_widget_name(&name);
            let (title, group) = kind.title(&name);
            if let Some(label) = row.first_child().and_then(|card| card.next_sibling()).and_downcast::<gtk::Label>() {
                label.set_text(&title);
            }
            if let Some(label) = row.last_child().and_downcast::<gtk::Label>() {
                label.set_visible(!group.is_empty());
                label.set_text(&group);
            }
            let Some(card) = row.first_child().and_downcast::<gtk::Picture>() else { return };

            card.set_paintable(gtk::gdk::Paintable::NONE);
            fill_card(&state, kind, &card, &name);
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
                kind.apply(&state, &name);
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

pub(super) fn preview_preset(state: &App, kind: Kind, name: &str) {
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
                let paintable = {
                    let open = state.open.borrow();
                    let Some(photo) = open.as_ref() else { return };

                    let Some(document) = kind.on(photo, &name) else { return };

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
                        &render::apply_stack(&document, working, scale),
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
    let id = selected_ids(state).first().copied().ok_or("Select a photo to make a preset from")?;
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
            dialog.set_title("Import Presets");
            start_in_downloads(&dialog);
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
            dialog.set_title("Import a Folder of Presets");
            start_in_downloads(&dialog);
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

fn card_factory(state: &App, kind: Kind) -> gtk::SignalListItemFactory {
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
            move |_, _, _| preview_preset(&state, kind, &row.widget_name())
        ));
        hover.connect_leave(glib::clone!(
            #[strong] state,
            move |_| end_preview(&state)
        ));
        row.add_controller(hover);
        row.add_controller(throw_away(&state, kind, &row));

        item.downcast_ref::<gtk::ListItem>().map(|item| item.set_child(Some(&row)));
    }));

    factory
}

pub(super) fn start_in_downloads(dialog: &gtk::FileDialog) {
    let folder = glib::user_special_dir(glib::UserDirectory::Downloads)
        .filter(|dir| dir.is_dir())
        .unwrap_or_else(glib::home_dir);
    dialog.set_initial_folder(Some(&gio::File::for_path(folder)));
}

fn throw_away(state: &App, kind: Kind, row: &gtk::Box) -> gtk::GestureClick {
    let click = gtk::GestureClick::new();
    click.set_button(gtk::gdk::BUTTON_SECONDARY);
    click.connect_pressed(glib::clone!(
        #[strong] state,
        #[weak] row,
        move |gesture, _, x, y| {
            let name = row.widget_name().to_string();
            let Some(file) = kind.file(&name) else { return };
            gesture.set_state(gtk::EventSequenceState::Claimed);
            let button = gtk::Button::with_label("Move to Trash");
            button.add_css_class("flat");
            let popover = gtk::Popover::new();
            popover.set_child(Some(&button));
            popover.set_parent(&row);
            popover.set_pointing_to(Some(&gtk::gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
            popover.connect_closed(|popover| popover.unparent());
            button.connect_clicked(glib::clone!(
                #[strong] state,
                #[weak] popover,
                move |_| {
                    popover.popdown();
                    let (title, _) = kind.title(&name);
                    match gio::File::for_path(&file).trash(gio::Cancellable::NONE) {
                        Ok(()) => state.toast(&format!("“{title}” moved to the Trash")),
                        Err(err) => state.toast(&format!("Could not move “{title}” to the Trash: {err}")),
                    }
                    kind.refresh(&state);
                }
            ));
            popover.popup();
        }
    ));
    click
}

const CARD: (i32, i32) = (130, 87);
const CARD_EDGE: u32 = 360;

thread_local! {

    static CANVAS: RefCell<Option<(String, Arc<LinearImage>)>> = const { RefCell::new(None) };

    static CARDS: RefCell<HashMap<String, gtk::gdk::Texture>> = RefCell::new(HashMap::new());

    static WAITING: RefCell<std::collections::VecDeque<(Kind, String, glib::WeakRef<gtk::Picture>)>> = const { RefCell::new(std::collections::VecDeque::new()) };
    static DRAWING: Cell<bool> = const { Cell::new(false) };
}

pub(super) fn forget_cards(state: &App) {
    let key = state.open.borrow().as_ref().map(|photo| {
        let stack = photo.preset_base();
        format!(
            "{}\u{1f}{}\u{1f}{}",
            source_key(&photo.source),
            serde_json::to_string(stack).unwrap_or_default(),
            serde_json::to_string(&photo.document).unwrap_or_default()
        )
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

fn card_job(state: &App, kind: Kind, name: &str) -> Option<(String, Document, Arc<LinearImage>)> {
    let open = state.open.borrow();
    let photo = open.as_ref()?;
    let key = format!(
        "{}\u{1f}{}\u{1f}{}",
        source_key(&photo.source),
        serde_json::to_string(photo.preset_base()).unwrap_or_default(),
        serde_json::to_string(&photo.document).unwrap_or_default()
    );
    let small = CANVAS.with(|canvas| {
        let mut canvas = canvas.borrow_mut();
        if canvas.as_ref().map(|(was, _)| was != &key).unwrap_or(true) {
            let small = photo.proxy.downscaled(CARD_EDGE).unwrap_or_else(|| (*photo.proxy).clone());
            *canvas = Some((key.clone(), Arc::new(small)));
        }
        canvas.as_ref().map(|(_, image)| image.clone())
    })?;
    Some((key, kind.on(photo, name)?, small))
}

fn card_key(kind: Kind, name: &str) -> String {
    format!("{}\u{1f}{name}", kind as u8)
}

fn fill_card(state: &App, kind: Kind, card: &gtk::Picture, name: &str) {
    if let Some(texture) = CARDS.with(|cards| cards.borrow().get(&card_key(kind, name)).cloned()) {
        card.set_paintable(Some(&texture));
        return;
    }
    WAITING.with_borrow_mut(|waiting| waiting.push_back((kind, name.to_string(), card.downgrade())));
    if DRAWING.replace(true) {
        return;
    }
    let state = state.clone();
    glib::spawn_future_local(async move {
        while let Some((kind, name, card)) = WAITING.with_borrow_mut(|waiting| waiting.pop_front()) {
            let shows = |card: &gtk::Picture| card.parent().is_some_and(|row| row.widget_name() == name);
            let Some(card) = card.upgrade().filter(shows) else { continue };
            if let Some(texture) = CARDS.with(|cards| cards.borrow().get(&card_key(kind, &name)).cloned()) {
                card.set_paintable(Some(&texture));
                continue;
            }
            let Some((key, document, small)) = card_job(&state, kind, &name) else { continue };
            let Ok(rendered) = gio::spawn_blocking(move || render::develop(&document, &*small, &render_inputs(&document))).await else {
                continue;
            };

            if CANVAS.with(|canvas| canvas.borrow().as_ref().map(|(was, _)| was != &key).unwrap_or(true)) {
                continue;
            }
            let texture = texture_from(&rendered);
            CARDS.with(|cards| cards.borrow_mut().insert(card_key(kind, &name), texture.clone()));
            if shows(&card) {
                card.set_paintable(Some(&texture));
            }
        }
        DRAWING.set(false);
    });
}

pub(super) fn fill_presets_page(state: &App) {

    forget_cards(state);
    let page = &state.panel.presets_page;
    while let Some(child) = page.first_child() {
        page.remove(&child);
    }
    let card = strength_card(state);
    if let Some(parent) = card.parent().and_downcast::<gtk::Box>() {
        parent.remove(&card);
    }
    page.append(&card);
    refresh_strength(state);

    let save = gtk::Button::from_icon_name("list-add-symbolic");
    save.set_tooltip_text(Some("Save Settings as Preset…"));
    save.set_action_name(Some("win.preset-save"));
    save.add_css_class("flat");
    let more = more_menu(&[
        &[("Import Presets…", "win.preset-import"), ("Import a Folder of Presets…", "win.preset-import-folder")],
        &[("Open Presets Folder", "win.preset-folder")],
    ]);
    let label = section_row("Presets", &[save.upcast_ref(), more.upcast_ref()]);
    label.set_margin_bottom(6);
    page.append(&label);
    let browser = preset_browser(state, || {});
    browser.set_vexpand(true);
    page.append(&browser);
}

thread_local! {

    static STRENGTH: std::cell::OnceCell<(gtk::Box, gtk::Label, gtk::Scale)> = const { std::cell::OnceCell::new() };
}

fn strength_card(state: &App) -> gtk::Box {
    STRENGTH.with(|strength| {
        strength
            .get_or_init(|| {
                let card = gtk::Box::new(gtk::Orientation::Vertical, 0);
                card.add_css_class("strength-card");
                let name = section_header("");
                name.set_margin_top(0);
                card.append(&name);
                let scale = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 100.0, 1.0);
                scale.set_value(100.0);
                set_neutral(&scale, 100.0);

                let held = Rc::new(Cell::new(false));
                scale.connect_value_changed(glib::clone!(
                    #[strong] state,
                    #[strong] held,
                    move |scale| {
                        if !state.applying.get() && strength_moved(&state, scale.value() / 100.0) {
                            held.set(true);
                            state.applying.set(true);
                        }
                    }
                ));
                card.append(&slider_row(state, "Strength", &scale, Readout::Positive(0)));
                scale.connect_value_changed(glib::clone!(
                    #[strong] state,
                    move |_| {
                        if held.replace(false) {
                            state.applying.set(false);
                        }
                    }
                ));
                (card, name, scale)
            })
            .0
            .clone()
    })
}

fn strength_moved(state: &App, amount: f64) -> bool {
    {
        let mut open = state.open.borrow_mut();
        let Some(photo) = open.as_mut() else { return false };
        let Some(on) = photo.before_preset.as_mut() else { return false };
        if on.left != EditState::of(&photo.document) {
            return false;
        }
        photo.document = numa::io::presets::at_strength(&on.base, &on.preset, amount as f32);
        on.left = EditState::of(&photo.document);
        on.amount = amount;
    }

    reload_open_document(state);
    true
}

pub(super) fn refresh_strength(state: &App) {
    let Some((card, name, scale)) = STRENGTH.with(|strength| strength.get().cloned()) else { return };
    let chosen = state.open.borrow().as_ref().and_then(|photo| match &photo.before_preset {
        Some(on) if on.left == EditState::of(&photo.document) => Some((on.name.clone(), on.amount)),
        _ => None,
    });
    card.set_visible(chosen.is_some());
    if let Some((label, at)) = chosen {
        name.set_text(&label.to_uppercase());
        state.applying.set(true);
        scale.set_value(at * 100.0);
        state.applying.set(false);
    }
}
