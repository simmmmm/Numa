use super::*;

thread_local! {

    static CONTROLS: RefCell<Option<(gtk::Box, gtk::Box, gtk::Label, gtk::Scale, gtk::Stack, gtk::Button)>> = const { RefCell::new(None) };
}

pub(super) fn build(state: &App) -> gtk::Box {
    let page = gtk::Box::new(gtk::Orientation::Vertical, 0);
    page.set_vexpand(true);

    let foot = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let name = section_header("Amount");
    name.set_hexpand(true);
    name.set_ellipsize(gtk::pango::EllipsizeMode::End);
    let remove = gtk::Button::from_icon_name("window-close-symbolic");
    remove.add_css_class("label-action");
    remove.set_tooltip_text(Some("Remove LUT"));
    remove.connect_clicked(glib::clone!(
        #[strong] state,
        move |_| choose(&state, None)
    ));
    let line = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    line.add_css_class("section-line");
    line.append(&name);
    line.append(&remove);
    foot.append(&line);
    let amount = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 100.0, 1.0);
    amount.set_value(100.0);
    set_neutral(&amount, 100.0);
    let row = slider_row(state, "Amount", &amount, Readout::Positive(0));
    amount.connect_value_changed(glib::clone!(
        #[strong] state,
        move |amount| {
            if state.applying.get() {
                return;
            }
            let changed = {
                let mut open = state.open.borrow_mut();
                let Some(lut) = open.as_mut().and_then(|photo| photo.document.lut.as_mut()) else { return };
                let changed = lut.amount != amount.value() as f32;
                lut.amount = amount.value() as f32;
                changed
            };
            if changed {
                request_render(&state);
                schedule_history_push(&state);
            }
        }
    ));
    let note = gtk::Label::new(Some("Choose a LUT to set its amount here."));
    note.set_xalign(0.0);
    note.set_wrap(true);
    note.add_css_class("note");
    let slot = gtk::Stack::new();
    slot.add_named(&row, Some("amount"));
    slot.add_named(&note, Some("note"));
    let card = gtk::Box::new(gtk::Orientation::Vertical, 0);
    card.add_css_class("panel-card");
    card.append(&slot);
    foot.append(&card);

    CONTROLS.with_borrow_mut(|controls| *controls = Some((page.clone(), foot, name, amount, slot, remove)));
    fill(state);
    page
}

pub(super) fn foot() -> Option<gtk::Box> {
    CONTROLS.with_borrow(|controls| controls.as_ref().map(|controls| controls.1.clone()))
}

pub(super) fn fill(state: &App) {
    let Some((page, ..)) = CONTROLS.with_borrow(|controls| controls.clone()) else { return };

    forget_cards(state);
    while let Some(child) = page.first_child() {
        page.remove(&child);
    }
    let browser = browser(state, Kind::Luts, || {});
    browser.set_vexpand(true);
    take_search(state, "luts", &browser);
    page.append(&browser);
    write(state);
}

pub(super) fn more_button(state: &App) -> gtk::MenuButton {
    let more = more_menu(&[&[("Import LUTs…", "lut.import")], &[("Open LUTs Folder", "lut.folder")]]);
    let actions = gio::SimpleActionGroup::new();
    let import = gio::SimpleAction::new("import", None);
    import.connect_activate(glib::clone!(
        #[strong] state,
        #[weak] more,
        move |_, _| choose_files(&state, more.upcast_ref())
    ));
    let folder = gio::SimpleAction::new("folder", None);
    folder.connect_activate(glib::clone!(
        #[weak] more,
        move |_, _| open_folder(more.upcast_ref(), Kind::Luts.folder())
    ));
    actions.add_action(&import);
    actions.add_action(&folder);
    more.insert_action_group("lut", Some(&actions));
    more
}

pub(super) fn write(state: &App) {
    let Some((_, _, name, amount, slot, remove)) = CONTROLS.with_borrow(|controls| controls.clone()) else { return };
    let lut = state.open.borrow().as_ref().and_then(|photo| photo.document.lut.clone());
    mark_applied(state, Kind::Luts);
    let was = state.applying.replace(true);
    amount.set_value(lut.as_ref().map_or(100.0, |choice| choice.amount as f64));
    state.applying.set(was);
    slot.set_visible_child_name(if lut.is_some() { "amount" } else { "note" });

    remove.set_opacity(if lut.is_some() { 1.0 } else { 0.0 });
    remove.set_sensitive(lut.is_some());
    remove.set_can_target(lut.is_some());

    name.set_text(&match lut {
        Some(choice) => format!("Amount · {}", Kind::Luts.title(&choice.name).0),
        None => "Amount".to_string(),
    }
    .to_uppercase());
}

pub(super) fn choose(state: &App, chosen: Option<&str>) {
    {
        let mut open = state.open.borrow_mut();
        let Some(photo) = open.as_mut() else { return };
        let amount = photo.document.lut.as_ref().map_or(100.0, |lut| lut.amount);
        let wanted = chosen.map(|name| numa::core::lut::LutChoice { name: name.to_string(), amount });
        if photo.document.lut == wanted {
            return;
        }
        photo.document.lut = wanted;
    }
    write(state);
    request_render(state);
    schedule_history_push(state);
}

fn choose_files(state: &App, button: &gtk::Widget) {
    let filter = gtk::FileFilter::new();
    filter.set_name(Some("LUTs (.cube, .3dl)"));
    for pattern in ["*.cube", "*.CUBE", "*.3dl", "*.3DL"] {
        filter.add_pattern(pattern);
    }
    let filters = gio::ListStore::new::<gtk::FileFilter>();
    filters.append(&filter);
    let dialog = gtk::FileDialog::new();
    dialog.set_title("Import LUTs");
    start_in_downloads(&dialog);
    dialog.set_filters(Some(&filters));
    let window = button.root().and_downcast::<gtk::Window>();
    let state = state.clone();
    glib::spawn_future_local(async move {
        let Ok(files) = dialog.open_multiple_future(window.as_ref()).await else { return };
        let mut last = None;
        for file in files.iter::<gio::File>().flatten() {
            let Some(path) = file.path() else { continue };
            match numa::io::luts::import(&path) {
                Ok(name) => last = Some(name),
                Err(err) => state.toast(&format!("Could not import the LUT: {err}")),
            }
        }
        if let Some(name) = last {
            choose(&state, Some(&name));
        }
        fill(&state);
    });
}

fn open_folder(button: &gtk::Widget, dir: std::path::PathBuf) {
    if let Err(err) = std::fs::create_dir_all(&dir) {
        log::warn!("could not create {}: {err}", dir.display());
    }
    let window = button.root().and_downcast::<gtk::Window>();
    gtk::FileLauncher::new(Some(&gio::File::for_path(&dir))).launch(window.as_ref(), None::<&gio::Cancellable>, |result| {
        if let Err(err) = result {
            log::warn!("could not open the LUT folder: {err}");
        }
    });
}
