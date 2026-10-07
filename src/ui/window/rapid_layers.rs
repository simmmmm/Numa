use super::rapid::{self, of};
use super::rapid_panel::{amount, as_shot};
use super::*;
use numa::io::catalog::library_of;
use numa::io::layers::{self, Key, Layers, Look, Reading};
use numa::io::workflows::{Control, Did, Kind, Part};

type Frame = (i64, i64, Document, Option<Did>);

fn frames_of(state: &App, m: usize) -> Vec<Frame> {
    let Some((ids, times)) = of(state).moments.borrow().get(m).map(|moment| (moment.ids.clone(), moment.times.clone())) else { return Vec::new() };
    ids.iter().zip(times).filter_map(|(id, at)| Some((*id, at, rapid::own_of(state, *id)?, state.catalog.numa_did(*id)))).collect()
}

fn majority(frames: &[Frame]) -> Option<i64> {
    let mut counts: HashMap<i64, usize> = HashMap::new();
    for record in frames.iter().filter_map(|(_, _, own, _)| own.moment) {
        *counts.entry(record).or_default() += 1;
    }
    counts.into_iter().max_by_key(|(record, count)| (*count, -record)).map(|(record, _)| record)
}

pub(super) fn read(state: &App, m: usize) -> Option<(Option<i64>, Layers)> {
    read_owns(state, m).map(|(record, layers, _)| (record, layers))
}

fn read_owns(state: &App, m: usize) -> Option<(Option<i64>, Layers, Vec<(i64, Document)>)> {
    let frames = frames_of(state, m);
    let library = library_of(frames.first()?.0);
    Some(match majority(&frames) {
        Some(record) => (Some(record), state.catalog.layers(library, record).unwrap_or_default(), frames.into_iter().map(|(id, _, own, _)| (id, own)).collect()),
        None => {
            let (layers, owns) = layers::adopt(&frames);
            (None, layers, frames.iter().map(|(id, ..)| *id).zip(owns).collect())
        }
    })
}

pub(super) fn claim(state: &App, m: usize) -> Option<(i64, Layers)> {
    let frames = frames_of(state, m);
    let library = library_of(frames.first()?.0);
    let ids: Vec<i64> = frames.iter().map(|(id, ..)| *id).collect();
    let key = of(state).moments.borrow().get(m)?.key;
    let fresh = || {
        let taken: Vec<i64> = state.catalog.all_layers(library).into_iter().map(|(record, _)| record).collect();
        if taken.contains(&key) { taken.iter().max().map_or(key, |most| most + 1) } else { key }
    };
    let found = majority(&frames).and_then(|record| Some((record, state.catalog.layers(library, record)?)));
    let (record, mut layers, owns) = match found {
        Some((record, mut shared)) if shared.frames.keys().any(|id| !ids.contains(id)) => {
            let mut mine = shared.clone();
            mine.frames.retain(|id, _| ids.contains(id));
            mine.apart.retain(|id| ids.contains(id));
            mine.keys.retain(|key| ids.contains(&key.id));
            shared.frames.retain(|id, _| !ids.contains(id));
            shared.keys.retain(|key| !ids.contains(&key.id));
            let _ = state.catalog.keep_layers(library, record, &shared);
            (fresh(), mine, frames.iter().map(|(_, _, own, _)| own.clone()).collect::<Vec<_>>())
        }
        Some((record, layers)) => (record, layers, frames.iter().map(|(_, _, own, _)| own.clone()).collect()),
        None => {
            let (layers, owns) = layers::adopt(&frames);

            for (id, _, _, did) in &frames {
                if let Some(did) = did {
                    let _ = state.catalog.keep_numa_did(*id, &did.shape_only());
                }
            }
            (fresh(), layers, owns)
        }
    };
    let mut failed = 0;
    for ((id, at, was, _), mut own) in frames.iter().zip(owns) {
        layers.link(*id, *at);
        if layers.apart.contains(id) {
            continue;
        }
        own.moment = Some(record);
        if serde_json::to_string(&own).ok() != serde_json::to_string(was).ok() && state.catalog.save_own_edits(*id, &own).is_err() {
            failed += 1;
        }
    }
    if failed > 0 {
        state.toast(&format!("{failed} photograph(s) could not be written"));
    }
    state.catalog.keep_layers(library, record, &layers).map_err(|err| state.toast(&err)).ok()?;
    Some((record, layers))
}

pub(super) fn change(state: &App, m: usize, change: impl FnOnce(&mut Layers)) {
    let Some((record, mut layers)) = claim(state, m) else { return };
    change(&mut layers);
    keep(state, m, record, &layers);
}

fn keep(state: &App, m: usize, record: i64, layers: &Layers) {
    if store(state, m, record, layers) {
        rapid::repaint_moment(state, m);
    }
}

pub(super) fn store(state: &App, m: usize, record: i64, layers: &Layers) -> bool {
    let Some(first) = of(state).moments.borrow().get(m).map(|moment| moment.ids[0]) else { return false };
    match state.catalog.keep_layers(library_of(first), record, layers) {
        Ok(()) => true,
        Err(err) => {
            state.toast(&err);
            false
        }
    }
}

fn refill(state: &App, m: usize) {
    let state = state.clone();
    glib::idle_add_local_once(move || {
        if rapid::chosen(&state) == Some(m) {
            rapid_panel::fill(&state, m);
            let (at, t) = of(&state).at.get();
            rapid::focus(&state, at, t);
        }
    });
}

pub(super) fn moment_value(layers: &Layers, control: Control, shot: WhiteBalance) -> f32 {
    match control {
        Control::Exposure => layers.tone[0],
        Control::Contrast => layers.tone[1],
        Control::Highlights => layers.tone[2],
        Control::Shadows => layers.tone[3],
        Control::Whites => layers.tone[4],
        Control::Vibrance => layers.vibrance,
        Control::Noise => layers.noise,
        Control::Warmth => layers.balance.map_or(shot.temperature, |(temperature, _)| temperature),
    }
}

pub(super) fn set_moment_value(layers: &mut Layers, control: Control, value: f32, shot: WhiteBalance) {
    let (lower, upper, _) = control.range();
    let value = value.clamp(lower, upper);
    match control {
        Control::Exposure => layers.tone[0] = value,
        Control::Contrast => layers.tone[1] = value,
        Control::Highlights => layers.tone[2] = value,
        Control::Shadows => layers.tone[3] = value,
        Control::Whites => layers.tone[4] = value,
        Control::Vibrance => layers.vibrance = value,
        Control::Noise => layers.noise = value,
        Control::Warmth if layers.balance.is_none() && (value - shot.temperature).abs() < 1.0 => {}
        Control::Warmth => layers.balance = Some((value, layers.balance.map_or(shot.tint, |(_, tint)| tint))),
    }
}

fn readout(control: Control) -> (Readout, usize) {
    match control {
        Control::Exposure => (Readout::Signed(2), 2),
        Control::Warmth => (Readout::Kelvin, 0),
        Control::Noise => (Readout::Positive(0), 0),
        _ => (Readout::Signed(0), 0),
    }
}

fn label(text: &str, class: &str) -> gtk::Label {
    let label = gtk::Label::new(Some(text));
    label.add_css_class(class);
    label.set_xalign(0.0);
    label.set_wrap(true);
    label
}

fn slider(name: &str, (lower, upper, neutral): (f32, f32, f32), value: f32, readout: (Readout, usize), set: impl Fn(f32) + 'static) -> gtk::Box {
    let (readout, digits) = readout;
    let step = if digits == 2 { 0.01 } else { 1.0 };
    let scale = numa_slider(&gtk::Adjustment::new(neutral as f64, lower as f64, upper as f64, step, step * 10.0, 0.0), neutral as f64);
    scale.set_digits(digits as i32);
    scale.set_hexpand(true);

    connect_wheel(&scale);
    let row = gtk::Box::new(gtk::Orientation::Vertical, 0);
    row.add_css_class("slider-row");
    let head = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let words = label(name, "slider-name");
    words.set_hexpand(true);
    let shown = gtk::Label::new(None);
    shown.add_css_class("slider-value");
    head.append(&words);
    head.append(&shown);
    row.append(&head);
    row.append(&scale);
    scale.set_value(value as f64);
    shown.set_text(&readout.format(scale.value()));
    let pending: Rc<RefCell<Option<glib::SourceId>>> = Rc::default();
    let set = Rc::new(set);
    scale.connect_value_changed(move |scale| {
        shown.set_text(&readout.format(scale.value()));
        if let Some(waiting) = pending.take() {
            waiting.remove();
        }
        let (to, set, pending_) = (scale.value() as f32, set.clone(), pending.clone());
        pending.replace(Some(glib::timeout_add_local_once(std::time::Duration::from_millis(250), move || {
            pending_.take();
            set(to);
        })));
    });
    row
}

fn head(title: &str, about: &str, end: Option<&gtk::Widget>) -> gtk::Box {
    let words = gtk::Box::new(gtk::Orientation::Vertical, 1);
    words.set_hexpand(true);
    words.append(&label(title, "rapid-layer-title"));
    words.append(&label(about, "rapid-layer-about"));
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    row.append(&words);
    if let Some(end) = end {
        end.set_valign(gtk::Align::Start);
        row.append(end);
    }
    row
}

fn block() -> gtk::Box {
    let column = gtk::Box::new(gtk::Orientation::Vertical, 4);
    column.add_css_class("rapid-layer");
    column
}

pub(super) fn section(state: &App, m: usize) -> gtk::Box {
    let column = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let Some((_, layers)) = read(state, m) else { return column };
    column.append(&even_block(state, m, &layers));
    column.append(&look_block(state, m, &layers));

    let hint = label("Change the look once and every photo of the moment follows; a photo's own changes stay its own.", "rapid-layer-hint");
    column.append(&hint);
    let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    buttons.append(&key_frames(state, m));
    column.append(&buttons);
    column
}

fn look_block(state: &App, m: usize, layers: &Layers) -> gtk::Box {
    let column = block();
    let switch = gtk::Switch::new();
    switch.set_active(layers.look_on);

    switch.set_visible(layers.look.is_some());
    switch.set_tooltip_text(Some("The look on the moment, or off to see it without"));
    switch.connect_active_notify(glib::clone!(
        #[strong] state,
        move |switch| change(&state, m, |layers| layers.look_on = switch.is_active())
    ));
    let about = match &layers.look {
        Some(look) => format!("{} · the whole moment, live", look.label()),
        None => "As Shot · nothing over the moment".to_string(),
    };
    column.append(&head("Look", &about, Some(switch.upcast_ref())));

    let presets = numa::io::presets::list(&numa::io::presets::dir());
    let luts = numa::io::luts::list();
    let names: Vec<String> = std::iter::once("As Shot".to_string()).chain(presets.iter().cloned()).chain(luts.iter().map(|lut| format!("{lut} · LUT"))).collect();
    let choose = gtk::DropDown::from_strings(&names.iter().map(String::as_str).collect::<Vec<_>>());
    choose.set_enable_search(true);

    choose.set_search_match_mode(gtk::StringFilterMatchMode::Substring);
    choose.set_expression(Some(gtk::PropertyExpression::new(gtk::StringObject::static_type(), None::<gtk::Expression>, "string")));
    choose.set_tooltip_text(Some("The moment's look: one of your presets or LUTs, or as the camera shot it"));
    let at = layers.look.as_ref().and_then(|look| match look.preset {
        Some(_) => presets.iter().position(|name| *name == look.name).map(|at| at + 1),
        None => luts.iter().position(|name| *name == look.name).map(|at| at + 1 + presets.len()),
    });
    choose.set_selected(at.unwrap_or(0) as u32);
    let strength = layers.look.as_ref().map_or(100.0, |look| look.strength);
    choose.connect_selected_notify(glib::clone!(
        #[strong] state,
        move |choose| {
            let at = choose.selected() as usize;
            let look = match at {
                0 => None,
                at if at <= presets.len() => match numa::io::presets::load(&numa::io::presets::dir(), &presets[at - 1]) {
                    Ok(preset) => Some(Look { name: presets[at - 1].clone(), preset: Some(preset), strength }),
                    Err(err) => {
                        state.toast(&err);
                        return;
                    }
                },
                at => luts.get(at - 1 - presets.len()).map(|name| Look { name: name.clone(), preset: None, strength }),
            };
            change(&state, m, move |layers| {
                layers.look = look;
                layers.look_on = true;
            });
            refill(&state, m);
        }
    ));
    column.append(&choose);
    if layers.look.is_some() {
        let set = glib::clone!(
            #[strong] state,
            move |to: f32| change(&state, m, |layers| {
                if let Some(look) = layers.look.as_mut() {
                    look.strength = to;
                }
            })
        );
        column.append(&slider("Strength", (0.0, 100.0, 0.0), strength, (Readout::Positive(0), 0), set));
    }
    column
}

fn shown_photo(state: &App) -> Option<(i64, String)> {
    let id = rapid::focused(state)?;
    let name = state.grid.cards.borrow().get(&id).and_then(|photo| photo.path.file_stem().map(|stem| stem.to_string_lossy().to_string()))?;
    Some((id, name))
}

pub(super) fn detach_or_join(state: &App, m: usize, id: i64, detach: bool) {
    let Some((record, mut layers)) = claim(state, m) else { return };
    let Some(own) = rapid::own_of(state, id) else { return };
    let own = match detach {
        true => {
            let mut baked = layers.compose(id, &own);
            baked.moment = None;
            layers.apart.push(id);
            baked
        }
        false => {
            layers.apart.retain(|apart| *apart != id);
            let mut joined = layers.decompose(id, &own);
            joined.moment = Some(record);
            joined
        }
    };
    if let Err(err) = state.catalog.save_own_edits(id, &own) {
        state.toast(&err);
        return;
    }
    keep(state, m, record, &layers);
    refill(state, m);
}

fn even_block(state: &App, m: usize, layers: &Layers) -> gtk::Box {
    let column = block();
    let switch = gtk::Switch::new();
    switch.set_active(layers.even);
    switch.set_tooltip_text(Some("The moment's exposure, white balance and sliders on every frame, or off to see each frame as its own"));
    switch.connect_active_notify(glib::clone!(
        #[strong] state,
        move |switch| {
            change(&state, m, |layers| layers.even = switch.is_active());
            refill(&state, m);
        }
    ));
    let mut about = "Camera exposure and the moment's white balance".to_string();
    let evened: Vec<f32> = layers.frames.values().map(|frame| frame.stops).filter(|stops| *stops != 0.0).collect();
    if layers.evening() && !evened.is_empty() {
        let most = evened.iter().copied().map(f32::abs).fold(0.0, f32::max);
        about.push_str(&format!(" · {} in line, up to {most:.1} stops", rapid::photos(evened.len())));
    }
    if let Some(name) = layers.matched.and_then(|id| state.grid.cards.borrow().get(&id).and_then(|photo| photo.path.file_stem().map(|stem| stem.to_string_lossy().to_string()))) {
        about.push_str(&format!(" · matched to {name}"));
    }
    match layers.keys.len() {
        0 => {}
        1 => about.push_str(" · one key frame"),
        n => about.push_str(&format!(" · ramped between {n} key frames")),
    }
    column.append(&head("Even", &about, Some(switch.upcast_ref())));
    let key = of(state).moments.borrow().get(m).map(|moment| moment.key);
    let Some(key) = key else { return column };
    for control in Kind::controls(rapid::kind_of_view(state)) {
        let row = moment_row(state, m, key, *control, layers);
        row.set_sensitive(layers.even && row.is_sensitive());
        column.append(&row);
    }
    column
}

fn moment_row(state: &App, m: usize, key: i64, control: Control, layers: &Layers) -> gtk::Box {
    let known = of(state).as_shot.borrow().contains_key(&key);
    let shot = as_shot(state, key);
    let (lower, upper, rest) = control.range();
    let neutral = if control == Control::Warmth { shot.temperature } else { rest };
    let set = glib::clone!(
        #[strong] state,
        move |to: f32| {
            let shot = as_shot(&state, key);
            change(&state, m, |layers| set_moment_value(layers, control, to, shot));
            rapid_numa::learn(&state, m, control, to);
        }
    );
    let row = slider(control.name(), (lower, upper, neutral), moment_value(layers, control, shot), readout(control), set);
    if control == Control::Warmth && !known {
        row.set_sensitive(false);
        read_as_shot(state, m, key);
    }

    if control == Control::Warmth && !layers.keys.is_empty() {
        row.set_sensitive(false);
        row.set_tooltip_text(Some("The key frames set the white balance through the moment"));
    }
    row
}

fn read_as_shot(state: &App, m: usize, key: i64) {
    let Some(path) = state.grid.cards.borrow().get(&key).map(|photo| photo.path.clone()) else { return };
    let state = state.clone();
    glib::spawn_future_local(async move {
        let shot = gio::spawn_blocking(move || camera_balance(&path)).await.ok().flatten();
        of(&state).as_shot.borrow_mut().insert(key, shot.unwrap_or(WhiteBalance { temperature: 5500.0, tint: 0.0 }));
        if rapid::chosen(&state) == Some(m) {
            rapid_panel::fill(&state, m);
        }
    });
}

fn camera_balance(path: &Path) -> Option<WhiteBalance> {
    raw::proxy_from_mosaic(path, 512).ok().and_then(|(proxy, _)| proxy.profile).map(|profile| profile.as_shot_white_balance())
}

fn reading(path: &Path) -> (Reading, Option<WhiteBalance>) {
    let light = raw::camera_exposure(path);
    let Some((proxy, _)) = raw::proxy_from_mosaic(path, 256).ok() else { return (Reading { light, ..Reading::default() }, None) };
    let Some(profile) = proxy.profile else { return (Reading { light, ..Reading::default() }, None) };
    let grey = layers::average(&proxy.data, proxy.clip.unwrap_or(1.0), profile.as_shot).and_then(|camera| profile.neutral_of_camera(camera)).map(|wb| (wb.temperature, wb.tint));
    (Reading { light, grey, body: Some(layers::body(&profile.xyz_to_cam)) }, Some(profile.as_shot_white_balance()))
}

pub(super) fn match_to(state: &App, m: usize) {
    let Some((reference, name)) = shown_photo(state) else { return };
    let Some(frames) = of(state).moments.borrow().get(m).map(|moment| moment.ids.clone()) else { return };
    let paths: Vec<(i64, PathBuf)> = {
        let cards = state.grid.cards.borrow();
        frames.iter().filter_map(|id| Some((*id, cards.get(id)?.path.clone()))).collect()
    };
    state.toast(&format!("Matching {} to {name}…", rapid::photos(paths.len())));
    let state = state.clone();
    glib::spawn_future_local(async move {
        let read = gio::spawn_blocking(move || {

            let pool = rayon::ThreadPoolBuilder::new().num_threads(4).build().ok();
            let each = |path: &PathBuf| reading(path);
            use rayon::prelude::*;
            let readings: Vec<(Reading, Option<WhiteBalance>)> = match pool {
                Some(pool) => pool.install(|| paths.par_iter().map(|(_, path)| each(path)).collect()),
                None => paths.iter().map(|(_, path)| each(path)).collect(),
            };
            paths.into_iter().map(|(id, _)| id).zip(readings).collect::<Vec<_>>()
        })
        .await;
        let Ok(readings) = read else { return };
        let Some((record, mut layers)) = claim(&state, m) else { return };
        let Some((reference_reading, camera)) = readings.iter().find(|(id, _)| *id == reference).map(|(_, read)| *read) else { return };

        let mut own = rapid::own_of(&state, reference).unwrap_or_else(|| Document::new(String::new()));
        let shown = own.white_balance.map(|wb| (wb.temperature, wb.tint)).or(layers.even_of(reference).1).or(camera.map(|wb| (wb.temperature, wb.tint)));
        let Some(shown) = shown else {
            state.toast("This photograph has no camera white balance to match to");
            return;
        };
        let mut basic = own.basic();
        layers.tone[0] += basic.tone.exposure;
        basic.tone.exposure = 0.0;
        own.set_basic(basic);
        own.white_balance = None;
        if let Err(err) = state.catalog.save_own_edits(reference, &own) {
            state.toast(&err);
            return;
        }
        let only: Vec<Reading> = readings.iter().map(|(_, (reading, _))| *reading).collect();
        let matched = layers::matched(reference_reading, shown, &only);
        let mut lined = 0;
        for ((id, _), (stops, balance)) in readings.iter().zip(matched) {
            let frame = layers.frames.entry(*id).or_default();
            frame.stops = stops;
            frame.balance = balance;
            lined += usize::from(stops != 0.0 || balance.is_some()) * usize::from(*id != reference);
        }
        layers.frames.entry(reference).or_default().balance = Some(shown);
        layers.matched = Some(reference);
        layers.even = true;
        layers.switch(Part::Even, true);
        keep(&state, m, record, &layers);
        refill(&state, m);
        state.toast(&format!("The moment matched to {name} · {} brought in line", rapid::photos(lined)));
    });
}

fn key_frames(state: &App, m: usize) -> gtk::MenuButton {
    let button = gtk::MenuButton::new();
    button.set_label("Key Frames…");
    button.add_css_class("flat");
    button.set_tooltip_text(Some("Where the light changes through the moment — a ceremony into sunset, golden hour — set two or three frames, and Numa ramps the light between them"));
    let column = gtk::Box::new(gtk::Orientation::Vertical, 6);
    column.set_width_request(300);
    let popover = gtk::Popover::new();
    popover.add_css_class("numa-content");
    popover.set_child(Some(&column));
    button.set_popover(Some(&popover));
    popover.connect_show(glib::clone!(
        #[strong] state,
        #[weak] column,
        move |_| fill_keys(&state, m, &column)
    ));

    popover.connect_closed(glib::clone!(
        #[strong] state,
        move |_| {
            let state = state.clone();
            glib::idle_add_local_once(move || rapid::fill_list(&state));
        }
    ));
    button
}

fn fill_keys(state: &App, m: usize, column: &gtk::Box) {
    while let Some(child) = column.first_child() {
        column.remove(&child);
    }
    let Some((_, layers)) = read(state, m) else { return };
    let header = section_header("Key frames");
    header.set_margin_top(0);
    column.append(&header);
    let about = label("Set the light on a few frames; every frame between is ramped by when it was taken.", "rapid-layer-about");
    about.set_max_width_chars(36);
    column.append(&about);
    let key = of(state).moments.borrow().get(m).map(|moment| moment.key);

    let moments = layers.balance.map_or_else(|| key.map_or(5500.0, |key| as_shot(state, key).temperature), |(temperature, _)| temperature);
    let mut keys: Vec<(i64, Key)> = layers.keys.iter().map(|key| (layers.frames.get(&key.id).map_or(0, |frame| frame.at), *key)).collect();
    keys.sort_by_key(|(at, _)| *at);
    for (at, key) in keys {
        let name = state.grid.cards.borrow().get(&key.id).and_then(|photo| photo.path.file_stem().map(|stem| stem.to_string_lossy().to_string())).unwrap_or_default();
        let top = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        top.set_margin_top(8);
        let words = label(&format!("{} · {name}", rapid::clock(at)), "rapid-layer-title");
        words.set_hexpand(true);
        let remove = gtk::Button::from_icon_name("list-remove-symbolic");
        remove.add_css_class("flat");
        remove.set_tooltip_text(Some("No longer a key frame"));
        remove.connect_clicked(glib::clone!(
            #[strong] state,
            #[weak] column,
            move |_| {
                change(&state, m, |layers| layers.keys.retain(|other| other.id != key.id));
                fill_keys(&state, m, &column);
            }
        ));
        top.append(&words);
        top.append(&remove);
        column.append(&top);
        let id = key.id;
        let set_stops = glib::clone!(
            #[strong] state,
            move |to: f32| change(&state, m, |layers| layers.keys.iter_mut().filter(|key| key.id == id).for_each(|key| key.stops = to))
        );
        column.append(&slider("Exposure", (-2.0, 2.0, 0.0), key.stops, readout(Control::Exposure), set_stops));
        let set_warmth = glib::clone!(
            #[strong] state,
            move |to: f32| change(&state, m, |layers| layers.keys.iter_mut().filter(|key| key.id == id).for_each(|key| key.balance.0 = to))
        );
        let (lower, upper, _) = Control::Warmth.range();
        column.append(&slider("Warmth", (lower, upper, moments), key.balance.0, readout(Control::Warmth), set_warmth));
    }
    let focused = rapid::focused(state).filter(|id| of(state).moments.borrow().get(m).is_some_and(|moment| moment.ids.contains(id)));
    let add = gtk::Button::with_label("Add This Photo");
    add.set_margin_top(8);
    add.set_sensitive(focused.is_some_and(|id| !layers.keys.iter().any(|key| key.id == id)));
    add.set_tooltip_text(Some("The photograph with the keys as a key frame, starting from the light it has now"));
    add.connect_clicked(glib::clone!(
        #[strong] state,
        #[weak] column,
        move |_| {
            let Some(id) = focused else { return };
            add_key(&state, m, id, &column);
        }
    ));
    column.append(&add);
}

fn add_key(state: &App, m: usize, id: i64, column: &gtk::Box) {
    let Some(path) = state.grid.cards.borrow().get(&id).map(|photo| photo.path.clone()) else { return };
    let state = state.clone();
    let column = column.clone();
    glib::spawn_future_local(async move {
        let camera = gio::spawn_blocking(move || camera_balance(&path)).await.ok().flatten();
        let own = rapid::own_of(&state, id).and_then(|own| own.white_balance).map(|wb| (wb.temperature, wb.tint));
        change(&state, m, |layers| {
            let (stops, shown) = layers.ramp(layers.frames.get(&id).map_or(0, |frame| frame.at)).map_or((0.0, None), |(stops, balance)| (stops, Some(balance)));
            let balance = own.or(shown).or(layers.even_of(id).1).or(camera.map(|wb| (wb.temperature, wb.tint))).unwrap_or((5500.0, 0.0));
            layers.keys.push(Key { id, stops, balance });
        });
        fill_keys(&state, m, &column);
    });
}

pub(super) fn key_ids(state: &App) -> Vec<i64> {
    let Some(library) = rapid::library_of_view(state) else { return Vec::new() };
    state.catalog.all_layers(library).into_iter().flat_map(|(_, layers)| layers.keys.into_iter().map(|key| key.id)).collect()
}

pub(super) fn note_on_moment(state: &App, m: usize, read: &[(Control, f32)], shot: WhiteBalance) {
    change(state, m, |layers| {
        for (control, delta) in read {
            let now = moment_value(layers, *control, shot);
            set_moment_value(layers, *control, now + delta, shot);
        }
    });
}

pub(super) fn said(read: &[(Control, f32)]) -> String {
    read.iter().map(|(control, delta)| amount(*control, *delta)).collect::<Vec<_>>().join(", ")
}
