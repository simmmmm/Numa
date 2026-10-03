use super::*;

#[derive(Clone, Copy, PartialEq, Debug)]
enum Facet {
    Rating,
    Flag,
    Questionable,
    BestOfBurst,
    Type,
    Folder,
}

const EVERY_FACET: [Facet; 6] =
    [Facet::Rating, Facet::Flag, Facet::Questionable, Facet::BestOfBurst, Facet::Type, Facet::Folder];

const FLAGS: [(&str, Option<Flag>); 4] =
    [("All", None), ("Picked", Some(Flag::Picked)), ("Unflagged", Some(Flag::None)), ("Rejected", Some(Flag::Rejected))];

const TYPES: [(&str, FileType); 3] =
    [("All Files", FileType::Any), ("RAW Only", FileType::Raw), ("JPEG and Others", FileType::NotRaw)];

const SORTS: [(&str, &str, Sort); 5] = [
    ("date", "Date", Sort::Captured),
    ("name", "Name", Sort::Name),
    ("rating", "Rating", Sort::Rating),
    ("sharpness", "Sharpness", Sort::Sharpness),
    ("suggestion", "Suggestion", Sort::Suggested),
];

#[derive(Clone)]
struct Shown {
    button: gtk::MenuButton,
    rating: Vec<gtk::ToggleButton>,
    flag: Vec<gtk::ToggleButton>,
    kind: Vec<gtk::ToggleButton>,

    actions: Vec<gio::SimpleAction>,
    chip_row: gtk::Box,

    quick: Vec<gtk::ToggleButton>,
    count: gtk::Label,
    clear: gtk::Button,
}

pub(super) fn build_filter_bar(state: &App, window: &adw::ApplicationWindow) -> gtk::Revealer {
    let mut actions = install_sort_actions(state, window);
    let analyse = build_analyse_button(state);
    actions.extend(install_cull_filters(state, window));
    let (button, rating, flag, kind) = build_filter_button(state);

    let (group, export) = export_buttons(state, |state| export_selected_now(state));
    group.set_sensitive(false);
    state.export.library_export.replace(Some(export.clone()));
    state.grid.wall.connect_selection_changed(glib::clone!(
        #[weak] group,
        #[weak] export,
        move |wall| {
            let chosen = wall.selected().len();
            group.set_sensitive(chosen > 0);
            export.set_label(&match chosen {
                0 | 1 => "Export".to_string(),
                many => format!("Export {}", places::grouped(many as i64)),
            });
        }
    ));

    let end = &state.grid.header_end;
    let sizes = build_grid_sizes(state);
    for menu in [&sizes, &button] {
        cullbar::back_to_grid(state, menu);
    }
    end.append(&sizes);
    end.append(&analyse);
    end.append(&button);
    end.append(&group);
    cullbar::keep_focus(end.upcast_ref());

    state.grid.welcome.bind_property("visible", end, "visible").invert_boolean().sync_create().build();

    let (chips, chip_row, quick, count, clear) = build_chips(state);
    state.grid.welcome.bind_property("visible", &chips, "visible").invert_boolean().sync_create().build();
    let shown = Shown { button, rating, flag, kind, actions, chip_row, quick, count, clear };

    state.libraries.show_filter.replace(Some(Box::new(glib::clone!(
        #[strong] state,
        move || show_filter(&state, &shown)
    ))));
    chips
}

fn build_filter_button(state: &App) -> (gtk::MenuButton, Vec<gtk::ToggleButton>, Vec<gtk::ToggleButton>, Vec<gtk::ToggleButton>) {
    let column = gtk::Box::new(gtk::Orientation::Vertical, 6);
    column.set_size_request(340, -1);
    column.set_margin_top(6);
    column.set_margin_bottom(6);
    column.set_margin_start(6);
    column.set_margin_end(6);
    let heading = |text: &str| {
        let label = gtk::Label::new(Some(text));
        label.add_css_class("section-header");
        label.set_xalign(0.0);
        label.set_margin_top(6);
        label
    };

    column.append(&heading("RATING"));
    let (row, rating) = toggle_row(state, &["Any", "1+", "2+", "3+", "4+", "5"], |filter, at| filter.min_rating = at as u8);
    column.append(&row);

    column.append(&heading("FLAG"));
    let labels = FLAGS.map(|(label, _)| label);
    let (row, flag) = toggle_row(state, &labels, |filter, at| filter.flag = FLAGS[at].1);
    column.append(&row);

    column.append(&heading("ANALYSE"));
    for (label, action) in [("Only Questionable", "win.questionable"), ("Only Best of Each Burst", "win.best-of-burst")] {
        let check = gtk::CheckButton::with_label(label);
        check.set_action_name(Some(action));
        column.append(&check);
    }

    column.append(&heading("TYPE"));
    let labels = TYPES.map(|(label, _)| label);
    let (row, kind) = toggle_row(state, &labels, |filter, at| filter.file_type = TYPES[at].1);
    column.append(&row);

    let folders = heading("FOLDER");
    let picker = build_folder_picker(state);
    picker.bind_property("visible", &folders, "visible").sync_create().build();
    column.append(&folders);
    column.append(&picker);

    let popover = gtk::Popover::new();
    popover.add_css_class("numa-content");
    popover.set_child(Some(&column));
    let button = gtk::MenuButton::new();
    button.set_label("Filter");
    button.set_tooltip_text(Some("Narrow the library by rating, flag, Analyse, type or folder"));
    button.set_popover(Some(&popover));
    (button, rating, flag, kind)
}

fn toggle_row(state: &App, labels: &[&str], set: fn(&mut Filter, usize)) -> (gtk::Box, Vec<gtk::ToggleButton>) {
    let row = chip_row();
    let buttons: Vec<gtk::ToggleButton> = labels
        .iter()
        .enumerate()
        .map(|(at, label)| {
            let button = gtk::ToggleButton::with_label(label);
            button.set_hexpand(true);
            button.connect_toggled(glib::clone!(
                #[strong] state,
                move |button| {

                    if !button.is_active() || state.applying.get() {
                        return;
                    }
                    set(&mut state.libraries.filter.borrow_mut(), at);
                    reload_grid(&state);
                }
            ));
            row.append(&button);
            button
        })
        .collect();
    for button in &buttons[1..] {
        button.set_group(Some(&buttons[0]));
    }
    (row, buttons)
}

fn build_folder_picker(state: &App) -> gtk::DropDown {
    let picker = state.libraries.folder_picker.clone();
    picker.set_tooltip_text(Some("Only the photographs in one folder of this library"));
    picker.set_visible(false);
    picker.connect_selected_notify(glib::clone!(
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
    picker
}

const QUICK: [&str; 3] = ["All", "Picks", "★ 3+"];

fn quick_on(filter: &Filter, narrowed: bool) -> [bool; 3] {
    [!narrowed, filter.flag == Some(Flag::Picked), filter.min_rating == 3]
}

fn quick_press(filter: &mut Filter, at: usize, on: bool) {
    match at {
        0 => EVERY_FACET.iter().for_each(|facet| let_through(filter, *facet)),
        1 => filter.flag = on.then_some(Flag::Picked),
        _ => filter.min_rating = if on { 3 } else { 0 },
    }
}

fn build_chips(state: &App) -> (gtk::Revealer, gtk::Box, Vec<gtk::ToggleButton>, gtk::Label, gtk::Button) {
    let quick_row = chip_row();
    let quick: Vec<gtk::ToggleButton> = QUICK
        .iter()
        .enumerate()
        .map(|(at, label)| {
            let button = gtk::ToggleButton::with_label(label);
            button.set_focus_on_click(false);
            button.connect_toggled(glib::clone!(
                #[strong] state,
                move |button| {
                    if state.applying.get() {
                        return;
                    }
                    if at == 0 {
                        clear(&state, &EVERY_FACET);
                        return;
                    }
                    quick_press(&mut state.libraries.filter.borrow_mut(), at, button.is_active());
                    reload_grid(&state);
                }
            ));
            quick_row.append(&button);
            button
        })
        .collect();
    let chip_row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    let count = gtk::Label::new(None);
    count.add_css_class("dim-label");
    count.add_css_class("numeric");
    count.set_hexpand(true);
    count.set_xalign(1.0);
    let everything = gtk::Button::with_label("Clear");
    everything.add_css_class("flat");
    everything.connect_clicked(glib::clone!(
        #[strong] state,
        move |_| clear(&state, &EVERY_FACET)
    ));
    let bar = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    bar.add_css_class("filter-chips");
    bar.append(&quick_row);
    bar.append(&chip_row);
    bar.append(&count);
    bar.append(&everything);
    everything.set_focus_on_click(false);
    let chips = gtk::Revealer::new();
    chips.set_transition_type(gtk::RevealerTransitionType::SlideDown);
    chips.set_child(Some(&bar));
    chips.set_reveal_child(true);
    (chips, chip_row, quick, count, everything)
}

fn show_filter(state: &App, shown: &Shown) {
    let filter = state.libraries.filter.borrow().clone();
    let folder = state.libraries.folder.borrow().clone();
    let applying = state.applying.replace(true);
    shown.rating[filter.min_rating.min(5) as usize].set_active(true);
    shown.flag[FLAGS.iter().position(|(_, flag)| *flag == filter.flag).unwrap_or(0)].set_active(true);
    shown.kind[TYPES.iter().position(|(_, kind)| *kind == filter.file_type).unwrap_or(0)].set_active(true);
    let folders = state.libraries.folders.borrow().clone();
    let at = folder.as_ref().and_then(|folder| folders.iter().position(|known| known == folder)).map_or(0, |at| at + 1);
    state.libraries.folder_picker.set_selected(at as u32);
    state.applying.set(applying);
    let sort = SORTS.iter().find(|(_, _, sort)| *sort == filter.sort).map_or("date", |(target, _, _)| *target);
    for action in &shown.actions {
        let value = match action.name().as_str() {
            "questionable" => filter.only_questionable.to_variant(),
            "best-of-burst" => filter.best_of_burst.to_variant(),
            "sort" => sort.to_variant(),
            _ => filter.reversed.to_variant(),
        };
        action.set_state(&value);
    }

    let facets = facets(&filter, folder.as_deref());
    let applying = state.applying.replace(true);
    for (button, on) in shown.quick.iter().zip(quick_on(&filter, !facets.is_empty())) {
        button.set_active(on);
    }
    state.applying.set(applying);
    while let Some(chip) = shown.chip_row.first_child() {
        shown.chip_row.remove(&chip);
    }

    for (facet, label) in facets.iter().filter(|(facet, _)| !quick_says(&filter, *facet)) {
        shown.chip_row.append(&chip(state, *facet, label));
    }
    shown.button.set_label(&match facets.len() {
        0 => "Filter".to_string(),
        count => format!("Filter · {count}"),
    });
    if !facets.is_empty() {
        let total = unnarrowed(state).map_or(String::new(), |total| format!(" of {}", places::grouped(total as i64)));
        shown.count.set_text(&format!("{}{total}", places::grouped(state.grid.wall.len() as i64)));
    }
    shown.count.set_visible(!facets.is_empty());
    shown.clear.set_visible(!facets.is_empty());
}

fn quick_says(filter: &Filter, facet: Facet) -> bool {
    match facet {
        Facet::Flag => filter.flag == Some(Flag::Picked),
        Facet::Rating => filter.min_rating == 3,
        _ => false,
    }
}

fn chip(state: &App, facet: Facet, label: &str) -> gtk::Button {
    let inside = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    inside.append(&gtk::Label::new(Some(label)));
    inside.append(&gtk::Image::from_icon_name("window-close-symbolic"));
    let chip = gtk::Button::new();
    chip.set_child(Some(&inside));
    chip.add_css_class("filter-chip");
    chip.set_focus_on_click(false);
    chip.set_tooltip_text(Some("Remove This Filter"));
    chip.connect_clicked(glib::clone!(
        #[strong] state,
        move |_| clear(&state, &[facet])
    ));
    chip
}

fn facets(filter: &Filter, folder: Option<&Path>) -> Vec<(Facet, String)> {
    let mut facets = Vec::new();
    match filter.min_rating {
        0 => {}
        5 => facets.push((Facet::Rating, "★ 5".to_string())),
        stars => facets.push((Facet::Rating, format!("★ {stars} and Up"))),
    }
    if let Some((label, _)) = FLAGS.iter().skip(1).find(|(_, flag)| *flag == filter.flag) {
        facets.push((Facet::Flag, label.to_string()));
    }
    if filter.only_questionable {
        facets.push((Facet::Questionable, "Questionable".to_string()));
    }
    if filter.best_of_burst {
        facets.push((Facet::BestOfBurst, "Best of Each Burst".to_string()));
    }
    if let Some((label, _)) = TYPES.iter().skip(1).find(|(_, kind)| *kind == filter.file_type) {
        facets.push((Facet::Type, label.to_string()));
    }
    if let Some(folder) = folder {
        facets.push((Facet::Folder, folder.display().to_string()));
    }
    facets
}

fn let_through(filter: &mut Filter, facet: Facet) {
    match facet {
        Facet::Rating => filter.min_rating = 0,
        Facet::Flag => filter.flag = None,
        Facet::Questionable => filter.only_questionable = false,
        Facet::BestOfBurst => filter.best_of_burst = false,
        Facet::Type => filter.file_type = FileType::Any,

        Facet::Folder => {}
    }
}

fn clear(state: &App, facets: &[Facet]) {
    for facet in facets {
        let_through(&mut state.libraries.filter.borrow_mut(), *facet);
    }
    if facets.contains(&Facet::Folder) {
        state.libraries.folder.replace(None);
    }
    reload_grid(state);
}

fn unnarrowed(state: &App) -> Option<usize> {
    let library = state.libraries.current.borrow().clone()?;
    let mut filter = state.libraries.filter.borrow().clone();
    for facet in EVERY_FACET {
        let_through(&mut filter, facet);
    }
    let photos = match filter.spans_libraries() {
        true => state.catalog.photos_everywhere(&filter),
        false => state.catalog.photos(library.id, &filter),
    };
    photos.ok().map(|photos| photos.len())
}

pub(super) fn sort_menu() -> gio::Menu {
    let orders = gio::Menu::new();
    for (target, label, _) in SORTS {
        let item = gio::MenuItem::new(Some(label), None);
        item.set_action_and_target_value(Some("win.sort"), Some(&target.to_variant()));
        orders.append_item(&item);
    }
    orders.append(Some("Reverse Order"), Some("win.sort-reversed"));
    let menu = gio::Menu::new();
    menu.append_section(Some("Sort By"), &orders);
    menu
}

fn install_sort_actions(state: &App, window: &adw::ApplicationWindow) -> Vec<gio::SimpleAction> {
    let sort = gio::SimpleAction::new_stateful("sort", Some(glib::VariantTy::STRING), &"date".to_variant());
    sort.connect_activate(glib::clone!(
        #[strong] state,
        move |action, target| {
            let Some(target) = target.and_then(|target| target.get::<String>()) else { return };
            let Some((_, _, sort)) = SORTS.iter().find(|(name, _, _)| *name == target) else { return };
            action.set_state(&target.to_variant());
            state.libraries.filter.borrow_mut().sort = *sort;
            reload_grid(&state);
        }
    ));

    let reversed = gio::SimpleAction::new_stateful("sort-reversed", None, &false.to_variant());
    reversed.connect_activate(glib::clone!(
        #[strong] state,
        move |action, _| {
            let on = !action.state().and_then(|state| state.get::<bool>()).unwrap_or(false);
            action.set_state(&on.to_variant());
            state.libraries.filter.borrow_mut().reversed = on;
            reload_grid(&state);
        }
    ));
    window.add_action(&sort);
    window.add_action(&reversed);
    vec![sort, reversed]
}

fn install_cull_filters(state: &App, window: &adw::ApplicationWindow) -> Vec<gio::SimpleAction> {
    ["questionable", "best-of-burst"]
        .into_iter()
        .map(|name| {
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
            action
        })
        .collect()
}

fn build_analyse_button(state: &App) -> gtk::Button {
    let analyse = gtk::Button::with_label("Analyse");
    analyse.add_css_class("flat");
    analyse.set_tooltip_text(Some("Analyse sharpness, blown highlights, bursts and faces"));
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
    for (name, scale) in [("Photo Size", &size), ("Space Between", &gap)] {

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
                for card in state.grid.lazy.borrow_mut().iter_mut().filter(|card| card.edge != edge) {
                    card.edge = edge;
                }
                schedule_thumbnails(&state);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_chip_for_each_facet_that_narrows_and_none_for_the_rest() {
        assert!(facets(&Filter::default(), None).is_empty());
        let filter = Filter { min_rating: 3, flag: Some(Flag::Picked), best_of_burst: true, ..Filter::default() };
        let chips: Vec<String> = facets(&filter, Some(Path::new("2024/Rome"))).into_iter().map(|(_, label)| label).collect();
        assert_eq!(chips, ["★ 3 and Up", "Picked", "Best of Each Burst", "2024/Rome"]);
        let mut cleared = filter.clone();
        for facet in EVERY_FACET {
            let_through(&mut cleared, facet);
        }
        assert!(facets(&cleared, None).is_empty());
    }

    #[test]
    fn the_quick_chips_say_and_set_all_picks_and_three_stars() {
        let mut filter = Filter::default();
        assert_eq!(quick_on(&filter, false), [true, false, false]);
        quick_press(&mut filter, 1, true);
        quick_press(&mut filter, 2, true);
        assert_eq!(quick_on(&filter, true), [false, true, true]);
        assert!(quick_says(&filter, Facet::Flag) && quick_says(&filter, Facet::Rating));
        filter.best_of_burst = true;
        quick_press(&mut filter, 0, true);
        assert!(facets(&filter, None).is_empty());
    }
}
