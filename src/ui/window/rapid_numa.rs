use super::rapid::{self, of};
use super::rapid_panel::{self, amount, as_shot};
use numa::io::layers::Layers;
use super::*;
use numa::io::workflows::{self as kinds, light_of, Control, Did, Kind, Part, Style};

#[derive(Clone, Default)]
pub(super) struct Ahead {

    running: Rc<Cell<bool>>,
}

const LEARNED: [Control; 6] = [Control::Exposure, Control::Contrast, Control::Highlights, Control::Shadows, Control::Whites, Control::Warmth];

const SHAPES: [Part; 3] = [Part::Straighten, Part::Verticals, Part::Frame];

const ASKED: &str = "numa-do-parts";

fn offered(kind: Option<Kind>) -> Vec<Part> {
    let mut parts = vec![Part::Light, Part::Even, Part::Straighten, Part::Verticals];
    if Kind::frame(kind).is_some() {
        parts.push(Part::Frame);
    }
    parts
}

fn ask(state: &App, m: usize, parent: &gtk::Widget) {
    let kind = rapid::kind_of_view(state);
    let asked: Vec<Part> = state.catalog.recall(ASKED).unwrap_or_else(|| [Part::Light, Part::Even].into_iter().chain(Kind::shape(kind).iter().copied()).collect());
    let list = gtk::ListBox::new();
    list.add_css_class("boxed-list");
    list.set_selection_mode(gtk::SelectionMode::None);
    let rows: Vec<(Part, adw::SwitchRow)> = offered(kind)
        .into_iter()
        .map(|part| {
            let row = adw::SwitchRow::new();
            let (title, about) = match part {
                Part::Light => ("Light", "One light for the moment, from its middle frame, with your style in that light".to_string()),
                Part::Even => ("Even Exposure", "Each frame in line where the camera let in more or less light".to_string()),
                Part::Straighten => ("Straighten", "Level by the horizon and the uprights".to_string()),
                Part::Verticals => ("Square the Verticals", "Converging verticals upright".to_string()),
                Part::Frame => ("Frame", format!("Cropped {} on the subject", Kind::frame(kind).map_or_else(String::new, |frame| frame.label()))),
            };
            row.set_title(title);
            row.set_subtitle(&about);
            row.set_active(asked.contains(&part));
            list.append(&row);
            (part, row)
        })
        .collect();
    let dialog = adw::AlertDialog::new(Some("Let Numa Do…"), Some("Each part stays a switch in the panel, and Undo All takes everything back."));
    dialog.set_extra_child(Some(&list));
    dialog.add_responses(&[("cancel", "Cancel"), ("every", "Every Moment"), ("this", "This Moment")]);
    dialog.set_response_appearance("this", adw::ResponseAppearance::Suggested);
    dialog.set_default_response(Some("this"));
    dialog.set_close_response("cancel");
    dialog.connect_response(
        None,
        glib::clone!(
            #[strong] state,
            move |_, response| {
                let parts: Vec<Part> = rows.iter().filter(|(_, row)| row.is_active()).map(|(part, _)| *part).collect();
                if parts.is_empty() || response == "cancel" {
                    return;
                }
                state.catalog.remember(ASKED, &parts);
                match response {
                    "this" => do_moment(&state, m, parts),
                    _ => do_every(&state, parts),
                }
            }
        ),
    );
    dialog.present(Some(parent));
}

fn do_moment(state: &App, m: usize, parts: Vec<Part>) {
    let Some((key, ids)) = of(state).moments.borrow().get(m).map(|moment| (moment.key, moment.ids.clone())) else { return };
    state.toast(&format!("Numa is working on {}…", rapid::photos(ids.len())));
    run(state, key, ids, parts, |state, changed| {
        if changed == 0 {
            state.toast("Nothing in this moment needed it");
        }
    });
}

fn do_every(state: &App, parts: Vec<Part>) {
    if of(state).ahead.running.replace(true) {
        return;
    }
    let keys: Vec<i64> = of(state).moments.borrow().iter().map(|moment| moment.key).collect();
    let cancel = Cancel::default();
    let progress = progress_toast(state, &cancel);
    next(state.clone(), keys, (0, 0), parts, cancel, progress);
}

fn next(state: App, keys: Vec<i64>, (at, changed): (usize, usize), parts: Vec<Part>, cancel: Cancel, progress: (adw::Toast, gtk::Label, gtk::ProgressBar)) {
    let total = keys.len();
    if cancel.stopped() || at >= total {
        progress.0.dismiss();
        of(&state).ahead.running.set(false);
        if !cancel.stopped() {
            state.toast(&format!("Numa did {total} moment{}: {} changed", if total == 1 { "" } else { "s" }, rapid::photos(changed)));
        }
        return;
    }

    let found = of(&state).moments.borrow().iter().find(|moment| moment.key == keys[at]).map(|moment| moment.ids.clone());
    let Some(ids) = found else {
        next(state, keys, (at + 1, changed), parts, cancel, progress);
        return;
    };
    progress.1.set_text(&format!("Numa is working on moment {} of {total}", at + 1));
    progress.2.set_fraction(at as f64 / total as f64);
    let key = keys[at];
    let asked = parts.clone();
    run(&state, key, ids, asked, move |state, more| next(state.clone(), keys, (at + 1, changed + more), parts, cancel, progress));
}

fn style_of(state: &App, kind: Kind) -> Style {
    state.catalog.recall(&format!("style/{}", kind.name())).unwrap_or_default()
}

fn keep_style(state: &App, kind: Kind, style: &Style) {
    state.catalog.remember(&format!("style/{}", kind.name()), style);
}

fn run(state: &App, key: i64, ids: Vec<i64>, parts: Vec<Part>, then: impl FnOnce(&App, usize) + 'static) {
    let kind = rapid::kind_of_view(state);
    let frames: Vec<(i64, Document)> = ids.iter().filter_map(|id| rapid::own_of(state, *id).map(|document| (*id, document))).collect();
    let style = kind.map(|kind| style_of(state, kind)).unwrap_or_default();
    let state = state.clone();
    glib::spawn_future_local(async move {
        let asked = parts.clone();
        let worked = gio::spawn_blocking(move || work(kind, &style, frames, &asked)).await;
        let Ok((shot, dids)) = worked else {
            then(&state, 0);
            return;
        };
        if let Some(shot) = shot {
            of(&state).as_shot.borrow_mut().entry(key).or_insert(shot);
        }
        let m = of(&state).moments.borrow().iter().position(|moment| moment.key == key);
        let mut changed = 0;
        if let Some(m) = m {
            changed = put_on(&state, m, &dids, &parts);
            rapid::repaint_moment(&state, m);
            if rapid::chosen(&state) == Some(m) {
                let state = state.clone();
                glib::idle_add_local_once(move || rapid_panel::fill(&state, m));
            }
        }
        then(&state, changed);
    });
}

fn put_on(state: &App, m: usize, dids: &[(i64, Did)], parts: &[Part]) -> usize {
    let (lit, even) = (parts.contains(&Part::Light), parts.contains(&Part::Even));
    let mut changed = std::collections::HashSet::new();
    if let (true, Some(first), Some((record, mut layers))) = (lit || even, dids.first().map(|(_, did)| did), rapid_layers::claim(state, m)) {
        let was = layers.tone;
        if lit {
            layers.put_numa(first.light, first.warmth);
        }
        if even {
            for (id, did) in dids {
                layers.frames.entry(*id).or_default().stops = did.even.unwrap_or(0.0);
            }
        }
        rapid_layers::store(state, m, record, &layers);
        let moved = |did: &Did| (lit && did.light.is_some()) || (even && did.even.is_some());
        changed.extend(dids.iter().filter(|(_, did)| moved(did)).map(|(id, _)| *id));

        let worked: Vec<i64> = dids.iter().map(|(id, _)| *id).collect();
        let ids = of(state).moments.borrow().get(m).map(|moment| moment.ids.clone()).unwrap_or_default();
        for id in ids.into_iter().filter(|id| lit && !worked.contains(id) && !layers.apart.contains(id)) {
            let Some(mut own) = rapid::own_of(state, id) else { continue };
            let mut basic = own.basic();
            let tone = &mut basic.tone;
            for (value, at) in [&mut tone.exposure, &mut tone.contrast, &mut tone.highlights, &mut tone.shadows, &mut tone.whites, &mut tone.blacks].into_iter().zip(0..) {
                *value -= layers.tone[at] - was[at];
            }
            own.set_basic(basic);
            let _ = state.catalog.save_own_edits(id, &own);
        }
    }
    for (id, did) in dids {
        let shape = did.shape_only();
        if !SHAPES.iter().any(|part| shape.has(*part)) {
            continue;
        }
        let Some(current) = rapid::own_of(state, *id) else { continue };
        if let Err(err) = state.catalog.keep_before_numa(*id, &current) {
            log::warn!("Numa's work on {id} not kept: {err}");
            continue;
        }
        if state.catalog.save_own_edits(*id, &shape.first(&current)).is_ok() {
            let _ = state.catalog.keep_numa_did(*id, &merged(state.catalog.numa_did(*id), shape, parts));
            changed.insert(*id);
        }
    }
    changed.len()
}

fn merged(was: Option<Did>, now: Did, asked: &[Part]) -> Did {
    let Some(mut was) = was else { return now };
    if asked.contains(&Part::Straighten) {
        was.angle = now.angle;
    }
    if asked.contains(&Part::Verticals) {
        was.vertical = now.vertical;
    }
    if asked.contains(&Part::Frame) {
        was.view = now.view;
    }
    was.size = now.size.or(was.size);
    was.off.retain(|part| !asked.contains(part));
    was
}

fn work(kind: Option<Kind>, style: &Style, frames: Vec<(i64, Document)>, parts: &[Part]) -> (Option<WhiteBalance>, Vec<(i64, Did)>) {
    let Some((_, middle)) = frames.get(frames.len() / 2) else { return (None, Vec::new()) };
    let (light, warmth, shot) = match parts.contains(&Part::Light) {
        true => light(style, middle).unwrap_or((None, None, None)),
        false => (None, None, None),
    };
    let evens = match parts.contains(&Part::Even) {
        true => kinds::evened(&frames.iter().map(|(_, document)| raw::camera_exposure(Path::new(&document.source.path))).collect::<Vec<_>>()),
        false => vec![0.0; frames.len()],
    };
    let shapes: Vec<Part> = parts.iter().copied().filter(|part| SHAPES.contains(part)).collect();
    let dids = frames
        .iter()
        .zip(evens)
        .map(|((id, document), even)| {
            let mut did = Did { light, warmth, even: (even != 0.0).then_some(even), ..Did::default() };
            if !shapes.is_empty() {
                shape(kind, &shapes, document, &mut did);
            }
            (*id, did)
        })
        .collect();
    (shot, dids)
}

fn working_of(document: &Document, edge: u32) -> Option<LinearImage> {
    let (proxy, _) = raw::proxy_from_mosaic(Path::new(&document.source.path), edge).ok()?;
    let inputs = numa::io::inputs::render_inputs(document);
    Some(render::to_working_space(document, &proxy, &inputs))
}

#[allow(clippy::type_complexity)]
fn light(style: &Style, middle: &Document) -> Option<(Option<[f32; 6]>, Option<(f32, f32)>, Option<WhiteBalance>)> {
    let (proxy, _) = raw::proxy_from_mosaic(Path::new(&middle.source.path), 1600).ok()?;
    let shot = proxy.profile.as_ref().map(|profile| profile.as_shot_white_balance());
    let inputs = numa::io::inputs::render_inputs(middle);
    let working = render::to_working_space(middle, &proxy, &inputs);
    let tone = render::auto::tone(&working, None, middle).basic.tone;
    let bucket = light_of(shot.map_or(5500.0, |shot| shot.temperature));
    let with = |control: Control, value: f32| {
        let (lower, upper, _) = control.range();
        (value + style.offset(bucket, control)).clamp(lower, upper)
    };
    let values = [
        with(Control::Exposure, tone.exposure),
        with(Control::Contrast, tone.contrast),
        with(Control::Highlights, tone.highlights),
        with(Control::Shadows, tone.shadows),
        with(Control::Whites, tone.whites),
        tone.blacks,
    ];
    let light = values.iter().any(|value| value.abs() >= 0.01).then_some(values);
    let warm = style.offset(bucket, Control::Warmth);
    let warmth = shot.filter(|_| warm.abs() >= 1.0).map(|shot| (shot.temperature + warm, shot.tint));
    Some((light, warmth, shot))
}

fn shape(kind: Option<Kind>, parts: &[Part], document: &Document, did: &mut Did) {
    let Some(working) = working_of(document, 1024) else { return };
    let (width, height) = (working.width as f32, working.height as f32);
    let kept = document.perspective();

    let (straighten, verticals) = (parts.contains(&Part::Straighten), parts.contains(&Part::Verticals));
    if straighten || verticals {
        let camera = auto_plan::camera_of(Path::new(&document.source.path), document, None);
        let scene = render::auto::scene::read(document, &working, None, camera);
        if straighten {
            use render::auto::evidence::{decide, Verdict};
            did.angle = match decide(&scene.level, &scene.frame(kept, 0.0)) {
                Verdict::Level { angle, .. } => Some(angle).filter(|angle| angle.abs() >= 0.2),
                _ => None,
            };
        }
        if verticals {
            did.vertical = render::auto::plan::upright(&scene, Perspective { vertical: 0.0, ..kept });
        }
    }
    if let (true, Some(wanted)) = (parts.contains(&Part::Frame), Kind::frame(kind)) {
        let mut geometry = Document::new(document.source.path.clone());
        geometry.set_rotation(document.rotation());
        geometry.set_mirrored(document.mirrored());
        geometry.set_perspective(Perspective { vertical: did.vertical.unwrap_or(kept.vertical), ..kept });
        let frame = render::apply_stack(&geometry, &working, 1.0);
        let (w, h) = (frame.width() as f32, frame.height() as f32);
        let regions = segment::of(&frame)
            .map(|found| {
                let (gw, gh, winners) = found.winners();
                let cells: Vec<bool> = winners.iter().map(|class| segment::MATTEABLE.contains(class)).collect();
                kinds::regions(gw, gh, &cells)
            })
            .unwrap_or_default();
        let faces: Vec<[f32; 4]> = cull::faces::detect(&frame).unwrap_or_default().iter().map(|face| [face.x / w, face.y / h, face.width / w, face.height / h]).collect();
        did.view = kinds::framing(w / h, wanted, &regions, &faces).ok();
    }

    if did.angle.is_some() || did.vertical.is_some() || did.view.is_some() {
        did.size = Some(if matches!(document.rotation() as i32, 90 | 270) { (height, width) } else { (width, height) });
    }
}

pub(super) fn learn(state: &App, m: usize, control: Control, value: f32) {
    let (Some(kind), true) = (rapid::kind_of_view(state), LEARNED.contains(&control)) else { return };
    let Some(key) = of(state).moments.borrow().get(m).map(|moment| moment.key) else { return };
    let Some(did) = rapid_layers::read(state, m).and_then(|(_, layers)| layers.numa) else { return };
    let shot = as_shot(state, key);
    let bucket = light_of(shot.temperature);
    let index = [Control::Exposure, Control::Contrast, Control::Highlights, Control::Shadows, Control::Whites].iter().position(|c| *c == control);
    let numas = match (control, index) {
        (Control::Warmth, _) => did.warmth.map_or(shot.temperature, |(temperature, _)| temperature),
        (_, Some(at)) => did.light.map_or(0.0, |light| light[at]),
        _ => return,
    };
    let mut style = style_of(state, kind);
    let now = glib::DateTime::now_local().map_or(0, |now| now.to_unix());
    let delta = value - numas + style.offset(bucket, control);
    if style.changes.last().is_some_and(|last| last.light == bucket && last.control == control && now - last.at < 60) {
        style.changes.pop();
    }
    style.learn(now, bucket, control, delta, &format!("{} set on a moment", control.name()));
    keep_style(state, kind, &style);
}

pub(super) fn learn_note(state: &App, m: usize, text: &str, read: &[(Control, f32)]) -> bool {
    let Some(kind) = rapid::kind_of_view(state) else {
        state.toast("Choose the kind of shoot first: a style is kept per kind");
        return false;
    };
    let Some(key) = of(state).moments.borrow().get(m).map(|moment| moment.key) else { return false };
    let bucket = light_of(as_shot(state, key).temperature);
    let mut style = style_of(state, kind);
    let now = glib::DateTime::now_local().map_or(0, |now| now.to_unix());
    for (control, delta) in read.iter().filter(|(control, _)| LEARNED.contains(control)) {
        style.learn(now, bucket, *control, *delta, &format!("“{text}”"));
    }
    keep_style(state, kind, &style);
    true
}

pub(super) fn did_section(state: &App, m: usize) -> gtk::Box {
    let column = gtk::Box::new(gtk::Orientation::Vertical, 4);

    let head = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let title = section_header("Numa did");
    title.set_hexpand(true);
    head.append(&title);
    let undo_all = gtk::Button::with_label("Undo All…");
    undo_all.add_css_class("flat");
    undo_all.set_action_name(Some("win.undo-numa"));
    undo_all.set_tooltip_text(Some("Take back everything Numa did in this library"));
    head.append(&undo_all);
    column.append(&head);
    let Some(ids) = of(state).moments.borrow().get(m).map(|moment| moment.ids.clone()) else { return column };
    let layers = rapid_layers::read(state, m).map(|(_, layers)| layers).unwrap_or_default();
    let dids: Vec<(i64, Did)> = ids.iter().filter_map(|id| state.catalog.numa_did(*id).map(|did| (*id, did))).collect();
    let go = gtk::Button::with_label("Let Numa Do…");
    go.set_tooltip_text(Some("The parts you choose — light, evening, straightening — on this moment or on every one; each a switch here after, and Undo All takes it back"));
    go.set_halign(gtk::Align::Start);
    go.connect_clicked(glib::clone!(
        #[strong] state,
        move |button| ask(&state, m, button.upcast_ref())
    ));
    if dids.is_empty() && !layers.has(Part::Light) && !layers.has(Part::Even) {
        column.append(&line("Nothing yet", "dim-label"));
        column.append(&go);
        return column;
    }
    let mut any = false;
    for part in [Part::Light, Part::Even] {
        if layers.has(part) {
            any = true;
            column.append(&part_row(state, m, part, layers.is_on(part), &moment_detail(part, &layers)));
        }
    }
    for part in [Part::Straighten, Part::Verticals, Part::Frame] {
        let with: Vec<&Did> = dids.iter().map(|(_, did)| did).filter(|did| did.has(part)).collect();
        if with.is_empty() {
            continue;
        }
        any = true;
        let on = with.iter().any(|did| did.is_on(part));
        column.append(&part_row(state, m, part, on, &detail(part, &with, Kind::frame(rapid::kind_of_view(state)))));
    }
    if !any {
        column.append(&line("Nothing needed changing", "dim-label"));
    }
    column.append(&go);
    column
}

fn line(text: &str, class: &str) -> gtk::Label {
    let label = gtk::Label::new(Some(text));
    label.add_css_class(class);
    label.set_xalign(0.0);
    label.set_wrap(true);
    label
}

fn moment_detail(part: Part, layers: &Layers) -> String {
    match part {
        Part::Light => {
            let mut said: Vec<String> = Vec::new();
            let did = layers.numa.clone().unwrap_or_default();
            if let Some(light) = did.light {
                for (at, control) in [Control::Exposure, Control::Contrast, Control::Highlights, Control::Shadows, Control::Whites].into_iter().enumerate() {
                    if light[at].abs() >= if control == Control::Exposure { 0.01 } else { 0.5 } {
                        said.push(amount(control, light[at]));
                    }
                }
                if light[5].abs() >= 0.5 {
                    said.push(format!("Blacks {}", Readout::Signed(0).format(light[5] as f64)));
                }
            }
            if let Some((temperature, _)) = did.warmth {
                said.push(format!("Warmth {temperature:.0} K"));
            }
            format!("{} — once, for the whole moment", said.join(" · "))
        }
        _ => {
            let evened: Vec<f32> = layers.frames.values().map(|frame| frame.stops).filter(|stops| *stops != 0.0).collect();
            let most = evened.iter().copied().map(f32::abs).fold(0.0, f32::max);
            format!("{} the camera exposed differently, brought in line with the moment — up to {most:.1} stops", rapid::photos(evened.len()))
        }
    }
}

fn detail(part: Part, with: &[&Did], frame: Option<kinds::Frame>) -> String {
    let frames = rapid::photos(with.len());
    match part {
        Part::Straighten => {
            let most = with.iter().filter_map(|did| did.angle).map(f32::abs).fold(0.0, f32::max);
            format!("{frames}, up to {most:.1}°, by the horizon and the uprights")
        }
        Part::Verticals => format!("{frames} squared up"),
        _ => format!("{frames}, {} — the largest there is, the subject on a third", frame.map_or_else(String::new, |frame| frame.label())),
    }
}

fn part_row(state: &App, m: usize, part: Part, on: bool, detail: &str) -> gtk::Box {
    let switch = gtk::Switch::new();
    switch.set_active(on);
    switch.set_valign(gtk::Align::Start);
    switch.connect_active_notify(glib::clone!(
        #[strong] state,
        move |switch| {
            switch_part(&state, m, part, switch.is_active());
            let state = state.clone();
            glib::idle_add_local_once(move || rapid_panel::fill(&state, m));
        }
    ));
    let words = gtk::Box::new(gtk::Orientation::Vertical, 1);
    words.set_hexpand(true);
    words.append(&line(part.name(), "rapid-part-name"));
    words.append(&line(detail, "dim-label"));
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    row.add_css_class("rapid-part");
    if !on {
        row.add_css_class("off");
    }
    row.append(&words);
    row.append(&switch);
    row
}

fn switch_part(state: &App, m: usize, part: Part, on: bool) {
    if matches!(part, Part::Light | Part::Even) {
        rapid_layers::change(state, m, |layers| layers.switch(part, on));
        return;
    }
    let ids = of(state).moments.borrow().get(m).map(|moment| moment.ids.clone()).unwrap_or_default();
    for id in ids {
        let (Some(mut did), Some(before), Some(current)) = (state.catalog.numa_did(id), state.catalog.before_numa(id), rapid::own_of(state, id)) else { continue };
        if !did.has(part) {
            continue;
        }
        let switched = did.switch(part, on, &before, &current);
        if state.catalog.save_own_edits(id, &switched).is_ok() {
            let _ = state.catalog.keep_numa_did(id, &did);
        }
    }
    rapid::repaint_moment(state, m);
}

fn style_dialog(state: &App, window: &adw::ApplicationWindow) {
    let Some(kind) = rapid::kind_of_view(state) else {
        state.toast("Choose the kind of shoot first: a style is kept per kind");
        return;
    };
    let style = style_of(state, kind);
    let page = adw::PreferencesPage::new();
    let learned = adw::PreferencesGroup::new();
    learned.set_title(&format!("What Numa Does on {}", kind.name()));
    learned.set_description(Some("On top of its own answer, in each light: the mean of your latest changes there."));
    if style.changes.is_empty() {
        learned.add(&adw::ActionRow::builder().title("Nothing learned yet").subtitle("Set a moment's sliders after Numa, or keep a note as My Style").build());
    }
    let dialog = adw::Dialog::new();
    for (light, control, offset, count) in style.learned() {
        let row = adw::ActionRow::builder().title(format!("{light} · {}", amount(control, offset))).subtitle(format!("from {count} change(s)")).build();
        let forget = gtk::Button::with_label("Forget");
        forget.set_valign(gtk::Align::Center);
        forget.connect_clicked(glib::clone!(
            #[strong] state,
            #[weak] dialog,
            #[weak] window,
            move |_| {
                let mut style = style_of(&state, kind);
                style.forget(&light, control);
                keep_style(&state, kind, &style);
                dialog.close();
                style_dialog(&state, &window);
            }
        ));
        row.add_suffix(&forget);
        learned.add(&row);
    }
    page.add(&learned);
    let recent = adw::PreferencesGroup::new();
    recent.set_title("Latest Changes");
    recent.set_description(Some("Undo takes one out of what Numa learned; the photographs keep it."));
    for (at, change) in style.changes.iter().enumerate().rev().take(12) {
        let when = glib::DateTime::from_unix_local(change.at).ok().and_then(|time| time.format("%-d %b, %H:%M").ok()).map(|text| text.to_string()).unwrap_or_default();
        let row = adw::ActionRow::builder().title(format!("{} · {}", amount(change.control, change.delta), change.light)).subtitle(format!("{when} · {}", change.said)).build();
        let undo = gtk::Button::with_label("Undo");
        undo.set_valign(gtk::Align::Center);
        undo.connect_clicked(glib::clone!(
            #[strong] state,
            #[weak] dialog,
            #[weak] window,
            move |_| {
                let mut style = style_of(&state, kind);
                if at < style.changes.len() {
                    style.changes.remove(at);
                }
                keep_style(&state, kind, &style);
                dialog.close();
                style_dialog(&state, &window);
            }
        ));
        row.add_suffix(&undo);
        recent.add(&row);
    }
    if !style.changes.is_empty() {
        page.add(&recent);
    }
    let bar = adw::ToolbarView::new();
    bar.add_top_bar(&adw::HeaderBar::new());
    bar.set_content(Some(&page));
    dialog.set_title("Your Style");
    dialog.set_content_width(560);
    dialog.set_content_height(560);
    dialog.set_child(Some(&bar));
    dialog.present(Some(window));
}

fn undo_numas_work(state: &App, window: &adw::ApplicationWindow) {
    let Some(library) = rapid::library_of_view(state) else {
        state.toast("Open one library to undo Numa's work in it");
        return;
    };
    let (before, flags) = state.catalog.numas_work(library);

    let records: Vec<(i64, Layers)> = state.catalog.all_layers(library).into_iter().filter(|(_, layers)| layers.numa.is_some() || layers.has(Part::Even)).collect();
    if before.is_empty() && flags.is_empty() && records.is_empty() {
        state.toast("Nothing Numa did here is on record");
        return;
    }
    let alert = adw::AlertDialog::new(
        Some("Undo Numa's Work?"),
        Some(&format!("{} photograph(s) go back to how they were before Numa worked on them.", places::grouped(before.len() as i64))),
    );
    alert.add_response("cancel", "Cancel");
    alert.add_response("undo", "Undo Numa's Work");
    alert.set_response_appearance("undo", adw::ResponseAppearance::Destructive);
    let state = state.clone();
    alert.connect_response(None, move |_, response| {
        if response != "undo" {
            return;
        }
        let mut failed = 0;
        for (id, document) in &before {

            let mut document = document.clone();
            document.moment = state.catalog.own_edits(*id).ok().flatten().and_then(|own| own.moment);
            if state.catalog.save_own_edits(*id, &document).is_err() {
                failed += 1;
            }
        }
        for (record, layers) in &records {
            let mut layers = layers.clone();
            layers.without_numa();
            if state.catalog.keep_layers(library, *record, &layers).is_err() {
                failed += 1;
            }
        }
        let ids: Vec<i64> = before.iter().map(|(id, _)| *id).chain(flags.iter().map(|(id, _)| *id)).collect();
        let _ = state.catalog.forget_numa(&ids);
        reload_grid(&state);
        state.toast(&match failed {
            0 => format!("Numa's work undone on {} photograph(s)", places::grouped(before.len() as i64)),
            n => format!("Numa's work undone, {n} could not be written"),
        });
    });
    alert.present(Some(window));
}

pub(super) fn install(state: &App, window: &adw::ApplicationWindow) {
    let undo = gio::SimpleAction::new("undo-numa", None);
    undo.connect_activate(glib::clone!(
        #[strong] state,
        #[weak] window,
        move |_, _| undo_numas_work(&state, &window)
    ));
    window.add_action(&undo);
    let style = gio::SimpleAction::new("rapid-style", None);
    style.connect_activate(glib::clone!(
        #[strong] state,
        #[weak] window,
        move |_, _| style_dialog(&state, &window)
    ));
    window.add_action(&style);
}

#[cfg(test)]
mod tests {
    use super::{merged, Did, Part};

    #[test]
    fn a_part_asked_again_replaces_only_itself() {
        let was = Did { angle: Some(1.0), view: Some([0.1, 0.1, 0.8, 0.8]), off: vec![Part::Straighten, Part::Frame], ..Did::default() };
        let now = Did { angle: Some(-0.5), ..Did::default() };
        let kept = merged(Some(was), now, &[Part::Straighten]);
        assert_eq!(kept.angle, Some(-0.5));
        assert_eq!(kept.view, Some([0.1, 0.1, 0.8, 0.8]));
        assert_eq!(kept.off, vec![Part::Frame]);
    }
}
