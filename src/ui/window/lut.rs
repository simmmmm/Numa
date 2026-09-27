use super::*;

thread_local! {

    static CONTROLS: RefCell<Option<(gtk::DropDown, gtk::Scale)>> = const { RefCell::new(None) };
}

fn names() -> Vec<String> {
    std::iter::once("None".to_string()).chain(numa::io::luts::list()).collect()
}

pub(super) fn build(state: &App, global_only: &dyn Fn(&gtk::Widget)) -> gtk::Box {
    let column = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let header = section_header("LUT");
    column.append(&header);

    let line = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    line.set_margin_top(4);
    line.set_margin_bottom(4);
    let picker = gtk::DropDown::from_strings(&[]);
    picker.set_hexpand(true);
    picker.set_tooltip_text(Some("A look from a .cube or .3dl file, over the finished photograph"));
    picker.update_property(&[gtk::accessible::Property::Label("LUT")]);
    let import = gtk::Button::with_label("Import…");
    import.set_tooltip_text(Some("Add .cube or .3dl files to Numa's LUTs"));
    line.append(&picker);
    line.append(&import);
    column.append(&line);

    let amount = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 100.0, 1.0);
    amount.set_value(100.0);
    set_neutral(&amount, 100.0);
    let row = slider_row(state, "Amount", &amount, Readout::Positive(0));
    row.set_visible(false);
    column.append(&row);

    global_only(column.as_ref());

    picker.connect_selected_notify(glib::clone!(
        #[strong] state,
        move |_| commit(&state)
    ));
    amount.connect_value_changed(glib::clone!(
        #[strong] state,
        move |_| commit(&state)
    ));
    import.connect_clicked(glib::clone!(
        #[strong] state,
        move |button| choose_files(&state, button)
    ));
    CONTROLS.with_borrow_mut(|controls| *controls = Some((picker, amount)));
    write(state);
    column
}

pub(super) fn write(state: &App) {
    let Some((picker, amount)) = CONTROLS.with_borrow(|controls| controls.clone()) else { return };
    let lut = state.open.borrow().as_ref().and_then(|photo| photo.document.lut.clone());
    let was = state.applying.replace(true);
    let mut list = names();

    let position = match &lut {
        Some(choice) => list.iter().position(|name| *name == choice.name).unwrap_or_else(|| {
            list.push(choice.name.clone());
            list.len() - 1
        }),
        None => 0,
    };
    let strings: Vec<&str> = list.iter().map(String::as_str).collect();
    picker.set_model(Some(&gtk::StringList::new(&strings)));
    picker.set_selected(position as u32);
    amount.set_value(lut.as_ref().map_or(100.0, |choice| choice.amount as f64));
    show_amount(&amount, lut.is_some());
    state.applying.set(was);
}

fn show_amount(amount: &gtk::Scale, on: bool) {
    if let Some(row) = amount.parent() {
        row.set_visible(on);
    }
}

fn commit(state: &App) {
    if state.applying.get() {
        return;
    }
    let Some((picker, amount)) = CONTROLS.with_borrow(|controls| controls.clone()) else { return };
    let chosen = picker
        .selected_item()
        .and_downcast::<gtk::StringObject>()
        .map(|item| item.string().to_string())
        .filter(|_| picker.selected() > 0);
    show_amount(&amount, chosen.is_some());
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
        if let Some((picker, _)) = CONTROLS.with_borrow(|controls| controls.clone()) {
            if let Some(position) = names().iter().position(|each| *each == name) {
                picker.set_selected(position as u32);
            }
        }
    });
}
