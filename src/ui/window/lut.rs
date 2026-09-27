use super::*;

thread_local! {

    static CONTROLS: RefCell<Option<(gtk::ListBox, gtk::Box, gtk::Label, gtk::Scale)>> = const { RefCell::new(None) };

    static SHOWN: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

fn names() -> Vec<String> {
    std::iter::once("None".to_string()).chain(numa::io::luts::list()).collect()
}

pub(super) fn build(state: &App) -> gtk::Box {
    let column = gtk::Box::new(gtk::Orientation::Vertical, 0);

    let card = gtk::Box::new(gtk::Orientation::Vertical, 0);
    card.add_css_class("strength-card");
    let name = section_header("");
    name.set_margin_top(0);
    card.append(&name);
    let amount = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 100.0, 1.0);
    amount.set_value(100.0);
    set_neutral(&amount, 100.0);
    card.append(&slider_row(state, "Amount", &amount, Readout::Positive(0)));
    card.set_visible(false);
    column.append(&card);

    let list = gtk::ListBox::new();
    list.add_css_class("navigation-sidebar");
    list.set_selection_mode(gtk::SelectionMode::Single);
    list.update_property(&[gtk::accessible::Property::Label("LUT")]);
    list.set_tooltip_text(Some("A look from a .cube or .3dl file, over the finished photograph"));
    column.append(&list);

    let import = gtk::Button::with_label("Import…");
    import.set_tooltip_text(Some("Add .cube or .3dl files to Numa's LUTs"));
    import.set_halign(gtk::Align::Start);
    import.set_margin_top(6);
    column.append(&import);

    list.connect_row_selected(glib::clone!(
        #[strong] state,
        move |_, _| commit(&state)
    ));
    amount.connect_value_changed(glib::clone!(
        #[strong] state,
        move |_| commit(&state)
    ));
    import.connect_clicked(glib::clone!(
        #[strong] state,
        move |button| choose_files(&state, button)
    ));
    CONTROLS.with_borrow_mut(|controls| *controls = Some((list, card, name, amount)));
    write(state);
    column
}

pub(super) fn write(state: &App) {
    let Some((list, card, name, amount)) = CONTROLS.with_borrow(|controls| controls.clone()) else { return };
    let lut = state.open.borrow().as_ref().and_then(|photo| photo.document.lut.clone());
    let was = state.applying.replace(true);
    let mut shown = names();

    let position = match &lut {
        Some(choice) => shown.iter().position(|each| *each == choice.name).unwrap_or_else(|| {
            shown.push(choice.name.clone());
            shown.len() - 1
        }),
        None => 0,
    };
    list.remove_all();
    for each in &shown {
        let label = gtk::Label::new(Some(each));
        label.set_xalign(0.0);
        label.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
        list.append(&label);
    }
    list.select_row(list.row_at_index(position as i32).as_ref());
    SHOWN.with_borrow_mut(|names| *names = shown);
    amount.set_value(lut.as_ref().map_or(100.0, |choice| choice.amount as f64));
    show_card(&card, &name, lut.as_ref().map(|choice| choice.name.as_str()));
    state.applying.set(was);
}

fn show_card(card: &gtk::Box, name: &gtk::Label, chosen: Option<&str>) {
    card.set_visible(chosen.is_some());
    name.set_text(&chosen.unwrap_or_default().to_uppercase());
}

fn commit(state: &App) {
    if state.applying.get() {
        return;
    }
    let Some((list, card, name, amount)) = CONTROLS.with_borrow(|controls| controls.clone()) else { return };
    let chosen = list
        .selected_row()
        .map(|row| row.index())
        .filter(|index| *index > 0)
        .and_then(|index| SHOWN.with_borrow(|names| names.get(index as usize).cloned()));
    show_card(&card, &name, chosen.as_deref());
    {
        let mut open = state.open.borrow_mut();
        let Some(photo) = open.as_mut() else { return };
        let wanted = chosen.map(|name| numa::core::lut::LutChoice { name, amount: amount.value() as f32 });
        if photo.document.lut == wanted {
            return;
        }
        photo.document.lut = wanted;
    }
    request_render(state);
    schedule_history_push(state);
}

fn choose_files(state: &App, button: &gtk::Button) {
    let filter = gtk::FileFilter::new();
    filter.set_name(Some("LUTs (.cube, .3dl)"));
    for pattern in ["*.cube", "*.CUBE", "*.3dl", "*.3DL"] {
        filter.add_pattern(pattern);
    }
    let filters = gio::ListStore::new::<gtk::FileFilter>();
    filters.append(&filter);
    let dialog = gtk::FileDialog::new();
    dialog.set_title("Import LUTs");
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
        write(&state);
        let Some(name) = last else { return };
        if let Some((list, ..)) = CONTROLS.with_borrow(|controls| controls.clone()) {
            if let Some(position) = SHOWN.with_borrow(|names| names.iter().position(|each| *each == name)) {
                list.select_row(list.row_at_index(position as i32).as_ref());
            }
        }
    });
}
