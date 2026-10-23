use super::*;

#[derive(Clone, Copy, PartialEq, Debug)]
enum Facet {
    Rating,
    Flag,
    Questionable,
    BestOfBurst,
    Type,
    Folder,

    Words,
    Thing,

    Person,
    Place,
    Camera,
    Date,
}

const EVERY_FACET: [Facet; 12] = [
    Facet::Rating,
    Facet::Flag,
    Facet::Questionable,
    Facet::BestOfBurst,
    Facet::Type,
    Facet::Folder,
    Facet::Words,
    Facet::Thing,
    Facet::Person,
    Facet::Place,
    Facet::Camera,
    Facet::Date,
];

const FLAGS: [(&str, Flag); 3] = [("Picked", Flag::Picked), ("Rejected", Flag::Rejected), ("No Flag", Flag::None)];

const TYPES: [(&str, FileType); 2] = [("RAW Only", FileType::Raw), ("JPEG and Others", FileType::NotRaw)];

const SORTS: [(&str, &str, Sort); 5] = [
    ("date", "Date", Sort::Captured),
    ("name", "Name", Sort::Name),
    ("rating", "Rating", Sort::Rating),
    ("sharpness", "Sharpness", Sort::Sharpness),
    ("suggestion", "Suggestion", Sort::Suggested),
];

#[derive(Clone)]
struct Shown {

    actions: Vec<gio::SimpleAction>,
    chip_row: gtk::Box,

    quick: Vec<gtk::ToggleButton>,
    count: gtk::Label,

    order: gtk::MenuButton,
    direction: gtk::Button,
}

pub(super) fn build_filter_bar(state: &App, window: &adw::ApplicationWindow) -> gtk::Revealer {
    let actions = install_sort_actions(state, window);
    let analyse = build_analyse_button(state);

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
    cullbar::back_to_grid(state, &sizes);
    end.append(&sizes);
    end.append(&analyse);
    end.append(&group);
    cullbar::keep_focus(end.upcast_ref());

    state.grid.welcome.bind_property("visible", end, "visible").invert_boolean().sync_create().build();

    let (chips, chip_row, quick, count, order, direction) = build_chips(state);
    state.grid.welcome.bind_property("visible", &chips, "visible").invert_boolean().sync_create().build();
    let shown = Shown { actions, chip_row, quick, count, order, direction };

    state.libraries.show_filter.replace(Some(Box::new(glib::clone!(
        #[strong] state,
        move || show_filter(&state, &shown)
    ))));
    chips
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

fn build_chips(state: &App) -> (gtk::Revealer, gtk::Box, Vec<gtk::ToggleButton>, gtk::Label, gtk::MenuButton, gtk::Button) {
    let quick_row = chip_row();
    let quick: Vec<gtk::ToggleButton> = QUICK
        .iter()
        .enumerate()
        .map(|(at, label)| {
            let button = gtk::ToggleButton::with_label(label);
            button.set_focus_on_click(false);

            if at == 0 {
                button.set_tooltip_text(Some("Everything in the library: clears what narrows it"));
            }
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
    chip_row.set_hexpand(true);
    let count = gtk::Label::new(None);
    count.add_css_class("dim-label");
    count.add_css_class("numeric");
    let bar = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    bar.add_css_class("filter-chips");

    let filter = filter_menu::build(state);
    cullbar::back_to_grid(state, &filter);
    let lead = super::chip_row();
    lead.append(&filter);
    bar.append(&lead);
    let line = gtk::Separator::new(gtk::Orientation::Vertical);
    line.set_margin_top(4);
    line.set_margin_bottom(4);
    bar.append(&line);
    bar.append(&quick_row);
    bar.append(&chip_row);
    bar.append(&count);
    let (order, direction) = order_buttons();
    bar.append(&order);
    bar.append(&direction);
    bar.append(&select_toggle(state));
    let chips = gtk::Revealer::new();
    chips.set_transition_type(gtk::RevealerTransitionType::SlideDown);
    chips.set_child(Some(&bar));
    chips.set_reveal_child(true);
    (chips, chip_row, quick, count, order, direction)
}

fn select_toggle(state: &App) -> gtk::Box {
    let select = gtk::ToggleButton::with_label("Select");
    select.set_focus_on_click(false);
    select.set_tooltip_text(Some("Choose photographs with a click (Esc to stop)"));
    select.connect_toggled(glib::clone!(
        #[strong] state,
        move |select| {
            state.grid.wall.set_choosing(select.is_active());

            state.grid.wall.grab_focus();
        }
    ));
    state.grid.wall.connect_selection_changed(glib::clone!(
        #[weak] select,
        move |wall| select.set_active(wall.choosing())
    ));
    let row = chip_row();
    row.append(&select);
    row
}

fn order_buttons() -> (gtk::MenuButton, gtk::Button) {
    let keys = gio::Menu::new();
    for (target, label, _) in SORTS {
        let item = gio::MenuItem::new(Some(label), None);
        item.set_action_and_target_value(Some("win.sort"), Some(&target.to_variant()));
        keys.append_item(&item);
    }
    let order = gtk::MenuButton::new();
    order.set_menu_model(Some(&keys));
    order.set_has_frame(false);
    order.set_label("Date");
    order.set_tooltip_text(Some("Sort by"));
    let direction = gtk::Button::with_label("Oldest First");
    direction.add_css_class("flat");
    direction.set_action_name(Some("win.sort-reversed"));
    direction.set_focus_on_click(false);
    (order, direction)
}

pub(super) fn sort_name(sort: Sort) -> &'static str {
    SORTS.iter().find(|(_, _, known)| *known == sort).map_or("Date", |(_, label, _)| *label)
}

fn direction_words(sort: Sort, reversed: bool) -> (&'static str, &'static str) {
    let (ahead, back) = match sort {
        Sort::Captured => ("Oldest First", "Newest First"),
        Sort::Name => ("A to Z", "Z to A"),
        Sort::Rating => ("Highest First", "Lowest First"),
        Sort::Sharpness => ("Sharpest First", "Softest First"),
        Sort::Suggested => ("Best First", "Weakest First"),
    };
    if reversed { (back, ahead) } else { (ahead, back) }
}

fn show_filter(state: &App, shown: &Shown) {
    let filter = state.libraries.filter.borrow().clone();
    let folder = state.libraries.folder.borrow().clone();
    let sort = SORTS.iter().find(|(_, _, sort)| *sort == filter.sort).map_or("date", |(target, _, _)| *target);
    for action in &shown.actions {
        let value = match action.name().as_str() {
            "sort" => sort.to_variant(),
            _ => filter.reversed.to_variant(),
        };
        action.set_state(&value);
    }
    shown.order.set_label(sort_name(filter.sort));
    let (now, other) = direction_words(filter.sort, filter.reversed);
    shown.direction.set_label(now);
    shown.direction.set_tooltip_text(Some(&format!("{now} — click for {}", other.to_lowercase())));

    let (words, thing) = words::chips(state);
    let narrow = state.libraries.narrow.borrow().clone();
    let facets = facets(&filter, folder.as_deref(), words.as_deref(), thing, &narrow);
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
    if !facets.is_empty() {
        let total = unnarrowed(state).map_or(String::new(), |total| format!(" of {}", places::grouped(total as i64)));
        shown.count.set_text(&format!("{}{total}", places::grouped(state.grid.wall.len() as i64)));
    }
    shown.count.set_visible(!facets.is_empty());
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

    if facet == Facet::Thing {
        chip.add_css_class("saw-chip");
    }
    chip.set_focus_on_click(false);
    chip.set_tooltip_text(Some("Remove This Filter"));
    chip.connect_clicked(glib::clone!(
        #[strong] state,
        move |_| clear(&state, &[facet])
    ));
    chip
}

fn facets(filter: &Filter, folder: Option<&Path>, words: Option<&str>, thing: Option<&str>, narrow: &filter_menu::Narrow) -> Vec<(Facet, String)> {
    let mut facets = Vec::new();
    if filter.min_rating > 0 {
        facets.push((Facet::Rating, filter_menu::rated(filter.min_rating)));
    }
    if let Some((label, _)) = FLAGS.iter().find(|(_, flag)| Some(*flag) == filter.flag) {
        facets.push((Facet::Flag, label.to_string()));
    }
    if filter.only_questionable {
        facets.push((Facet::Questionable, "Questionable".to_string()));
    }
    if filter.best_of_burst {
        facets.push((Facet::BestOfBurst, "Best of Each Burst".to_string()));
    }
    if let Some((label, _)) = TYPES.iter().find(|(_, kind)| *kind == filter.file_type) {
        facets.push((Facet::Type, label.to_string()));
    }
    if let Some(folder) = folder {
        facets.push((Facet::Folder, folder.display().to_string()));
    }
    if let Some(words) = words {
        facets.push((Facet::Words, format!("Words: {words}")));
    }
    if let Some(thing) = thing {
        facets.push((Facet::Thing, thing.to_string()));
    }
    for (facet, said) in [(Facet::Person, &narrow.person), (Facet::Place, &narrow.place), (Facet::Camera, &narrow.camera)] {
        if let Some(said) = said {
            facets.push((facet, said.clone()));
        }
    }
    if let Some(date) = &narrow.date {
        facets.push((Facet::Date, tokens::date_label(date)));
    }
    facets
}

pub(super) fn searched(state: &App) -> Option<String> {
    let (words, thing) = words::chips(state);
    let narrow = state.libraries.narrow.borrow().clone();
    let mut said: Vec<String> = facets(&Filter::default(), None, None, thing, &narrow).into_iter().map(|(_, label)| label).collect();
    said.extend(words.map(|words| format!("\u{201c}{words}\u{201d}")));
    (!said.is_empty()).then(|| said.join(" \u{b7} "))
}

pub(super) fn drop_last_token(state: &App) {
    let (_, thing) = words::chips(state);
    let narrow = state.libraries.narrow.borrow().clone();
    if let Some((facet, _)) = facets(&Filter::default(), None, None, thing, &narrow).pop() {
        clear(state, &[facet]);
    }
}

fn let_through(filter: &mut Filter, facet: Facet) {
    match facet {
        Facet::Rating => filter.min_rating = 0,
        Facet::Flag => filter.flag = None,
        Facet::Questionable => filter.only_questionable = false,
        Facet::BestOfBurst => filter.best_of_burst = false,
        Facet::Type => filter.file_type = FileType::Any,

        Facet::Folder | Facet::Words | Facet::Thing | Facet::Person | Facet::Place | Facet::Camera | Facet::Date => {}
    }
}

fn clear(state: &App, facets: &[Facet]) {
    for facet in facets {
        let_through(&mut state.libraries.filter.borrow_mut(), *facet);
    }
    if facets.contains(&Facet::Folder) {
        state.libraries.folder.replace(None);
    }
    if facets.contains(&Facet::Thing) {
        words::forget_thing(state);
    }
    if facets.contains(&Facet::Words) {
        words::forget_query(state);
    }
    {
        let mut narrow = state.libraries.narrow.borrow_mut();
        for facet in facets {
            match facet {
                Facet::Person => narrow.person = None,
                Facet::Place => narrow.place = None,
                Facet::Camera => narrow.camera = None,
                Facet::Date => narrow.date = None,
                _ => {}
            }
        }
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
    let syncing = Rc::new(Cell::new(false));
    for scale in [&size, &gap] {
        scale.connect_value_changed(glib::clone!(
            #[strong] state,
            #[strong] syncing,
            #[weak] size,
            #[weak] gap,
            move |_| {
                let sizes = (size.value() as f32, gap.value() as f32);
                if syncing.get() {
                    return;
                }
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
    popover.connect_show(glib::clone!(
        #[strong] state,
        #[weak] size,
        #[weak] gap,
        move |_| {
            let (height, spacing) = state.catalog.recall::<(f32, f32)>(GRID_SIZES).unwrap_or((justified::ROW_HEIGHT, justified::SPACING));
            syncing.set(true);
            size.set_value(height as f64);
            gap.set_value(spacing as f64);
            syncing.set(false);
        }
    ));
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
        let none = filter_menu::Narrow::default();
        assert!(facets(&Filter::default(), None, None, None, &none).is_empty());
        let filter = Filter { min_rating: 3, flag: Some(Flag::Picked), best_of_burst: true, ..Filter::default() };
        let narrow = filter_menu::Narrow { place: Some("Kyoto".into()), date: Some("2024-03".into()), ..none.clone() };
        let chips: Vec<String> = facets(&filter, Some(Path::new("2024/Rome")), Some("bride with bouquet"), Some("bouquet"), &narrow)
            .into_iter()
            .map(|(_, label)| label)
            .collect();
        assert_eq!(chips, ["★ 3+", "Picked", "Best of Each Burst", "2024/Rome", "Words: bride with bouquet", "bouquet", "Kyoto", "March 2024"]);
        let mut cleared = filter.clone();
        for facet in EVERY_FACET {
            let_through(&mut cleared, facet);
        }
        assert!(facets(&cleared, None, None, None, &none).is_empty());
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
        assert!(facets(&filter, None, None, None, &filter_menu::Narrow::default()).is_empty());
    }
}
