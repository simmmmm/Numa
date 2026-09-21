use super::*;

pub(super) fn build_filter_bar(state: &App, window: &adw::ApplicationWindow) -> gtk::Box {
    let bar = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    bar.add_css_class("toolbar-row");

    let (rating, flag, folder_picker, file_type) = build_filter_pickers(state);

    let sort = build_sort_picker(state);
    let direction = build_sort_direction(state);

    restore_filters(state, &rating, &flag, &sort, &direction, &file_type);

    let analyse = build_analyse_button(state, window);

    bar.append(&rating);
    bar.append(&flag);
    bar.append(&folder_picker);
    bar.append(&file_type);
    bar.append(&sort);
    bar.append(&direction);

    let everyone = gtk::Button::with_label("People");
    everyone.set_tooltip_text(Some("The faces in this library, grouped, to put names to"));
    everyone.set_visible(cull::people::is_installed());
    everyone.connect_clicked(glib::clone!(
        #[strong] state,
        #[weak] window,
        move |_| people_dialog(&state, &window)
    ));
    bar.append(&everyone);

    let gap = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    gap.set_hexpand(true);
    bar.append(&gap);

    let merge = gtk::Button::with_label("Merge HDR");
    merge.set_tooltip_text(Some("Combine the selected exposures into one image"));
    merge.set_sensitive(false);
    merge.connect_clicked(glib::clone!(
        #[strong] state,
        move |button| merge_selection(&state, button)
    ));
    bar.append(&merge);
    bar.append(&analyse);

    let (group, export) = export_buttons(state, |state| export_selected_now(state));
    group.set_sensitive(false);

    state.export.library_export.replace(Some(export.clone()));
    bar.append(&group);

    state.grid.wall.connect_selection_changed(glib::clone!(
        #[weak] merge,
        #[weak] group,
        #[weak] export,
        move |wall| {
            let chosen = wall.selected().len();
            merge.set_sensitive(chosen >= 2);
            group.set_sensitive(chosen > 0);
            export.set_label(&match chosen {
                0 | 1 => "Export".to_string(),
                many => format!("Export {many}"),
            });
        }
    ));

    let sizes = build_grid_sizes(state);
    bar.append(&sizes);

    bar
}

fn build_filter_pickers(state: &App) -> (gtk::DropDown, gtk::DropDown, gtk::DropDown, gtk::DropDown) {
    let rating = gtk::DropDown::from_strings(&["Any rating", "1★+", "2★+", "3★+", "4★+", "5★"]);
    rating.connect_selected_notify(glib::clone!(
        #[strong] state,
        move |picker| {
            if state.applying.get() {
                return;
            }
            state.libraries.filter.borrow_mut().min_rating = picker.selected() as u8;
            reload_grid(&state);
        }
    ));

    let flag = gtk::DropDown::from_strings(&["All photos", "Picked", "Rejected", "Unflagged"]);
    flag.connect_selected_notify(glib::clone!(
        #[strong] state,
        move |picker| {
            if state.applying.get() {
                return;
            }
            state.libraries.filter.borrow_mut().flag = match picker.selected() {
                1 => Some(Flag::Picked),
                2 => Some(Flag::Rejected),
                3 => Some(Flag::None),
                _ => None,
            };
            reload_grid(&state);
        }
    ));

    let folder_picker = state.libraries.folder_picker.clone();
    folder_picker.set_tooltip_text(Some("Only the photographs in one folder of this library"));
    folder_picker.set_visible(false);
    folder_picker.connect_selected_notify(glib::clone!(
        #[strong] state,
        move |picker| {
            if state.applying.get() {
                return;
            }
            let chosen = (picker.selected() as usize)
                .checked_sub(1)
                .and_then(|index| state.libraries.folders.borrow().get(index).cloned());
            state.libraries.folder.replace(chosen);
            reload_grid(&state);
        }
    ));

    let file_type = gtk::DropDown::from_strings(&["All files", "RAW only", "JPEG and others"]);
    file_type.connect_selected_notify(glib::clone!(
        #[strong] state,
        move |picker| {
            if state.applying.get() {
                return;
            }
            state.libraries.filter.borrow_mut().file_type = match picker.selected() {
                1 => FileType::Raw,
                2 => FileType::NotRaw,
                _ => FileType::Any,
            };
            reload_grid(&state);
        }
    ));

    (rating, flag, folder_picker, file_type)
}

fn restore_filters(
    state: &App,
    rating: &gtk::DropDown,
    flag: &gtk::DropDown,
    sort: &gtk::DropDown,
    direction: &gtk::ToggleButton,
    file_type: &gtk::DropDown,
) {
    let (min_rating, saved_flag, saved_sort, saved_reversed, saved_type) = {
        let filter = state.libraries.filter.borrow();
        (filter.min_rating, filter.flag, filter.sort, filter.reversed, filter.file_type)
    };
    state.applying.set(true);
    rating.set_selected(min_rating.min(5) as u32);
    flag.set_selected(match saved_flag {
        Some(Flag::Picked) => 1,
        Some(Flag::Rejected) => 2,
        Some(Flag::None) => 3,
        None => 0,
    });
    sort.set_selected(sort_entry(saved_sort));
    direction.set_active(saved_reversed);
    file_type.set_selected(match saved_type {
        FileType::Any => 0,
        FileType::Raw => 1,
        FileType::NotRaw => 2,
    });
    state.applying.set(false);
}

fn build_analyse_button(state: &App, window: &adw::ApplicationWindow) -> adw::SplitButton {

    let cull_menu = gio::Menu::new();
    for (name, label) in [
        ("questionable", "Only questionable"),
        ("best-of-burst", "Only best of burst"),
    ] {
        let action = gio::SimpleAction::new_stateful(name, None, &false.to_variant());
        action.connect_activate(glib::clone!(
            #[strong] state,
            move |action, _| {
                let on = !action.state().and_then(|state| state.get::<bool>()).unwrap_or(false);
                action.set_state(&on.to_variant());

                {
                    let mut filter = state.libraries.filter.borrow_mut();
                    match action.name().as_str() {
                        "questionable" => filter.only_questionable = on,
                        _ => filter.best_of_burst = on,
                    }
                }
                reload_grid(&state);
            }
        ));
        window.add_action(&action);
        cull_menu.append(Some(label), Some(&format!("win.{name}")));
    }

    let analyse = adw::SplitButton::new();
    analyse.set_label("Analyse");
    analyse.set_tooltip_text(Some("Analyse sharpness, blown highlights, bursts and faces"));
    analyse.set_menu_model(Some(&cull_menu));
    analyse.connect_clicked(glib::clone!(
        #[strong] state,
        move |button| analyse_library(&state, button)
    ));
    state.libraries.analyse_button.replace(Some(analyse.clone()));
    analyse
}

fn build_grid_sizes(state: &App) -> gtk::MenuButton {

    let (height, spacing) = state
        .catalog
        .recall::<(f32, f32)>(GRID_SIZES)
        .unwrap_or((justified::ROW_HEIGHT, justified::SPACING));

    let stepped = |lower: f64, upper: f64, step: f64, value: f32| {
        let scale = gtk::Scale::with_range(gtk::Orientation::Horizontal, lower, upper, step);
        let mut mark = lower;
        while mark <= upper {
            scale.add_mark(mark, gtk::PositionType::Bottom, None);
            mark += step;
        }
        let snap = move |value: f64| (lower + ((value - lower) / step).round() * step).clamp(lower, upper);
        scale.set_value(snap(value as f64));
        scale.connect_change_value(move |scale, _, value| {
            scale.set_value(snap(value));
            glib::Propagation::Stop
        });
        scale
    };
    let size = stepped(100.0, 460.0, 60.0, height);
    let gap = stepped(0.0, 40.0, 8.0, spacing);
    state.grid.wall.set_sizes(size.value() as f32, gap.value() as f32);

    let panel = gtk::Box::new(gtk::Orientation::Vertical, 12);
    panel.set_size_request(280, -1);
    panel.set_margin_top(6);
    panel.set_margin_bottom(6);
    for (name, scale) in [("Photo size", &size), ("Space between", &gap)] {

        let title = gtk::Label::builder().label(name).xalign(0.0).margin_start(12).margin_end(12).build();
        let row = gtk::Box::new(gtk::Orientation::Vertical, 0);
        row.append(&title);
        row.append(scale);
        panel.append(&row);
    }
    for scale in [&size, &gap] {
        scale.connect_value_changed(glib::clone!(
            #[strong] state,
            #[weak] size,
            #[weak] gap,
            move |_| {
                let sizes = (size.value() as f32, gap.value() as f32);
                state.grid.wall.set_sizes(sizes.0, sizes.1);
                state.catalog.remember(GRID_SIZES, &sizes);

                let edge = grid_edge(&state);
                let mut changed = false;
                for card in state.grid.lazy.borrow_mut().iter_mut().filter(|card| card.edge != edge) {
                    card.edge = edge;
                    changed = true;
                }
                if changed {
                    schedule_thumbnails(&state);
                }
            }
        ));
    }
    let popover = gtk::Popover::new();
    popover.set_child(Some(&panel));
    let sizes = gtk::MenuButton::new();
    sizes.set_icon_name("numa-sliders-symbolic");
    sizes.set_tooltip_text(Some("Size of the photographs and the space between them"));
    sizes.set_popover(Some(&popover));
    sizes
}

fn build_sort_picker(state: &App) -> gtk::DropDown {

    let picker = gtk::DropDown::from_strings(&[
        "By date",
        "By name",
        "By rating",
        "By sharpness",
        "By suggestion",
    ]);
    picker.connect_selected_notify(glib::clone!(
        #[strong] state,
        move |picker| {
            if state.applying.get() {
                return;
            }
            state.libraries.filter.borrow_mut().sort = sort_at(picker.selected());
            reload_grid(&state);
        }
    ));
    picker
}

pub(super) fn build_sort_direction(state: &App) -> gtk::ToggleButton {
    let arrow = gtk::ToggleButton::new();
    arrow.set_icon_name("view-sort-descending-symbolic");
    arrow.set_tooltip_text(Some("Reverse the order"));
    arrow.add_css_class("flat");
    arrow.connect_toggled(glib::clone!(
        #[strong] state,
        move |button| {
            if state.applying.get() {
                return;
            }
            state.libraries.filter.borrow_mut().reversed = button.is_active();
            reload_grid(&state);
        }
    ));
    arrow
}

fn sort_at(index: u32) -> Sort {
    match index {
        1 => Sort::Name,
        2 => Sort::Rating,
        3 => Sort::Sharpness,
        4 => Sort::Suggested,
        _ => Sort::Captured,
    }
}

fn sort_entry(sort: Sort) -> u32 {
    (0..5).find(|index| sort_at(*index) == sort).unwrap_or(0)
}
