use super::*;
use serde_json::{json, Value};
use std::collections::{BTreeMap, BTreeSet};
use std::time::{Duration, Instant};

const SETTLE_MS: u64 = 450;

#[derive(Clone, Debug, PartialEq)]
enum Val {
    Num(f64),
    On(bool),
    Pick(u32),
}

struct Control {
    widget: gtk::Widget,
    name: String,
}

struct Learned {
    scale: gtk::Scale,
    tab: &'static str,
    name: String,
    path: String,
    k: f64,
    b: f64,
}

pub(super) fn run_audit(state: &App) {
    println!("AUDIT start");
    let ready = wait_until(240_000, || {
        state.open.borrow().as_ref().is_some_and(|photo| !photo.segmenting && !pending_masks(photo))
    });
    if !ready {
        println!("AUDIT refused: no photograph open, or its masks never arrived");
        return;
    }
    if let Err(why) = isolated(state) {
        println!("AUDIT refused: {why}");
        return;
    }
    settle(2000);

    let mut learned = Vec::new();
    walk_scope(state, "photo", None, &mut learned);

    let masks = state.open.borrow().as_ref().map_or(0, |photo| photo.document.masks().len());
    match masks {
        0 => println!("AUDIT mask0 | skipped: this photograph has no mask"),
        _ => {
            select_mask(state, Some(0));
            settle(SETTLE_MS);
            walk_scope(state, "mask0", Some(0), &mut Vec::new());
            select_mask(state, None);
            settle(SETTLE_MS);
        }
    }

    check_steps(state, &learned);

    settle(2500);
    println!("AUDIT done");
    std::process::exit(0);
}

fn isolated(state: &App) -> Result<(), String> {
    let open = state.open.borrow();
    let path = match open.as_ref().map(|photo| &photo.source) {
        Some(Source::Photo { path, .. }) => path.clone(),
        _ => return Err("no catalog photograph open".into()),
    };
    let home = std::env::var_os("XDG_DATA_HOME").ok_or("XDG_DATA_HOME is not set")?;
    match path.starts_with(&home) {
        true => Ok(()),
        false => Err(format!(
            "{} is not inside XDG_DATA_HOME; its library's catalog is the real one",
            path.display()
        )),
    }
}

fn settle(ms: u64) {
    let done = Rc::new(Cell::new(false));
    glib::timeout_add_local_once(
        Duration::from_millis(ms),
        glib::clone!(
            #[strong] done,
            move || done.set(true)
        ),
    );
    let context = glib::MainContext::default();
    while !done.get() {
        context.iteration(true);
    }

    for _ in 0..1000 {
        if !context.pending() {
            break;
        }
        context.iteration(false);
    }
}

fn wait_until(ms: u64, done: impl Fn() -> bool) -> bool {
    let start = Instant::now();
    while !done() {
        if start.elapsed() > Duration::from_millis(ms) {
            return false;
        }
        settle(100);
    }
    true
}

fn document_json(state: &App) -> Option<(Value, Value)> {
    let open = state.open.borrow();
    let photo = open.as_ref()?;
    let document = &photo.document;
    let raw = serde_json::to_value(document).ok()?;
    let mut view = raw.clone();
    let object = view.as_object_mut()?;
    object.remove("operations");
    let (rect, angle) = document.crop().unwrap_or(([0.0, 0.0, 1.0, 1.0], 0.0));
    for (key, value) in [

        ("white_balance", json!(document.white_balance.unwrap_or(photo.as_shot))),
        ("basic", json!(document.basic())),
        ("curves", json!(document.curves())),
        ("mixer", json!(document.mixer())),
        ("point_colours", json!(document.point_colours())),
        ("grading", json!(document.grading())),
        ("retouch", json!(document.retouch())),
        ("beautify", json!(document.beautify())),
        ("crop", json!({ "rect": rect, "angle": angle })),
        ("perspective", json!(document.perspective())),
        ("rotation", json!(document.rotation())),
        ("mirrored", json!(document.mirrored())),
        ("masks", json!(document.masks())),
    ] {
        object.insert(key.to_string(), value);
    }
    Some((raw, view))
}

fn flatten(value: &Value, at: &str, out: &mut BTreeMap<String, Value>) {
    match value {
        Value::Object(map) => {
            for (key, inner) in map {
                let path = if at.is_empty() { key.clone() } else { format!("{at}.{key}") };
                flatten(inner, &path, out);
            }
        }
        Value::Array(items) if items.len() <= 64 => {
            for (index, inner) in items.iter().enumerate() {
                flatten(inner, &format!("{at}[{index}]"), out);
            }
        }
        _ => {
            out.insert(at.to_string(), value.clone());
        }
    }
}

type Change = (String, Option<Value>, Option<Value>);

fn changes(before: &Value, after: &Value) -> Vec<Change> {
    let (mut old, mut new) = (BTreeMap::new(), BTreeMap::new());
    flatten(before, "", &mut old);
    flatten(after, "", &mut new);
    let paths: BTreeSet<&String> = old.keys().chain(new.keys()).collect();
    paths
        .into_iter()
        .filter(|path| old.get(*path) != new.get(*path))
        .map(|path| (path.clone(), old.get(path).cloned(), new.get(path).cloned()))
        .collect()
}

fn show(value: &Option<Value>) -> String {
    match value {
        None => "∅".into(),
        Some(Value::Number(number)) => {
            let text = format!("{:.4}", number.as_f64().unwrap_or(f64::NAN));
            text.trim_end_matches('0').trim_end_matches('.').to_string()
        }
        Some(other) => {
            let text = other.to_string();
            match text.chars().count() > 40 {
                true => format!("{}…", text.chars().take(40).collect::<String>()),
                false => text,
            }
        }
    }
}

fn describe(written: &[Change]) -> String {
    match written.is_empty() {
        true => "(nothing)".into(),
        false => written
            .iter()
            .map(|(path, old, new)| match (show(old), show(new)) {

                (a, b) if a == b => format!("{path} {}→{}", json!(old), json!(new)),
                (a, b) => format!("{path} {a}→{b}"),
            })
            .collect::<Vec<_>>()
            .join(", "),
    }
}

fn stored_order(raw: &Value) -> String {
    raw["operations"]
        .as_array()
        .map(|operations| {
            operations.iter().map(|operation| operation["type"].as_str().unwrap_or("?")).collect::<Vec<_>>().join(",")
        })
        .unwrap_or_default()
}

fn controls(page: &gtk::Widget) -> Vec<Control> {
    fn walk(widget: &gtk::Widget, section: &mut String, out: &mut Vec<(gtk::Widget, String)>) {
        if let Some(label) = widget.downcast_ref::<gtk::Label>() {
            if label.has_css_class("section-header") {
                let text = label.text().to_lowercase();
                let mut letters = text.chars();
                *section = letters.next().map_or(String::new(), |first| first.to_uppercase().chain(letters).collect());
            }
        }
        if value_of(widget).is_some() {
            out.push((widget.clone(), section.clone()));
            return;
        }

        if widget.is::<gtk::MenuButton>() || widget.is::<gtk::SpinButton>() {
            return;
        }
        let mut child = widget.first_child();
        while let Some(this) = child {
            walk(&this, section, out);
            child = this.next_sibling();
        }
    }

    let mut found = Vec::new();
    walk(page, &mut String::new(), &mut found);
    let mut seen: HashMap<String, usize> = HashMap::new();
    found
        .into_iter()
        .map(|(widget, section)| {
            let label = label_of(&widget);

            let in_a_row = widget.ancestor(adw::ActionRow::static_type()).is_some();
            let mut name = match section.is_empty() || in_a_row {
                true => label,
                false => format!("{section} › {label}"),
            };
            let count = seen.entry(name.clone()).or_insert(0);
            *count += 1;
            if *count > 1 {
                name = format!("{name} #{count}");
            }
            Control { widget, name }
        })
        .collect()
}

fn label_of(widget: &gtk::Widget) -> String {
    if let Some(row) = widget.downcast_ref::<adw::PreferencesRow>() {
        return row.title().to_string();
    }
    let own = widget
        .downcast_ref::<gtk::Button>()
        .and_then(|button| button.label())
        .or_else(|| widget.downcast_ref::<gtk::CheckButton>().and_then(|check| check.label()))
        .map(|label| label.to_string());
    let hint = || {
        widget
            .tooltip_text()
            .map(|tip| tip.to_string())
            .or_else(|| widget.downcast_ref::<gtk::Button>().and_then(|button| button.icon_name()).map(|icon| icon.to_string()))
            .unwrap_or_else(|| widget.type_().name().to_string())
    };
    let is_button = widget.is::<gtk::ToggleButton>() || widget.is::<gtk::CheckButton>();
    if is_button {
        let row = widget.ancestor(adw::ActionRow::static_type()).and_downcast::<adw::ActionRow>();
        return match (own, row) {
            (Some(own), _) => own,
            (None, Some(row)) => format!("{}: {}", row.title(), hint()),
            (None, None) => hint(),
        };
    }

    let mut at = widget.parent();
    for _ in 0..3 {
        let Some(ancestor) = at else { break };
        if let Some(text) = first_label(&ancestor, widget) {
            return text;
        }
        at = ancestor.parent();
    }
    hint()
}

fn first_label(widget: &gtk::Widget, not_inside: &gtk::Widget) -> Option<String> {
    if widget == not_inside {
        return None;
    }
    if let Some(label) = widget.downcast_ref::<gtk::Label>() {
        let text = label.text();
        if !text.is_empty() && !label.has_css_class("slider-value") && !label.has_css_class("section-header") {
            return Some(text.to_string());
        }
    }
    let mut child = widget.first_child();
    while let Some(this) = child {
        if let Some(text) = first_label(&this, not_inside) {
            return Some(text);
        }
        child = this.next_sibling();
    }
    None
}

fn value_of(widget: &gtk::Widget) -> Option<Val> {
    if let Some(row) = widget.downcast_ref::<adw::SpinRow>() {
        return Some(Val::Num(row.value()));
    }
    if let Some(row) = widget.downcast_ref::<adw::SwitchRow>() {
        return Some(Val::On(row.is_active()));
    }
    if let Some(row) = widget.downcast_ref::<adw::ComboRow>() {
        return Some(Val::Pick(row.selected()));
    }
    if let Some(scale) = widget.downcast_ref::<gtk::Scale>() {
        return Some(Val::Num(scale.value()));
    }
    if let Some(switch) = widget.downcast_ref::<gtk::Switch>() {
        return Some(Val::On(switch.is_active()));
    }
    if let Some(picker) = widget.downcast_ref::<gtk::DropDown>() {
        return Some(Val::Pick(picker.selected()));
    }
    if let Some(check) = widget.downcast_ref::<gtk::CheckButton>() {
        return Some(Val::On(check.is_active()));
    }
    widget.downcast_ref::<gtk::ToggleButton>().map(|toggle| Val::On(toggle.is_active()))
}

fn put(widget: &gtk::Widget, value: &Val) {
    match value {
        Val::Num(number) => {
            if let Some(row) = widget.downcast_ref::<adw::SpinRow>() {
                row.set_value(*number);
            } else if let Some(scale) = widget.downcast_ref::<gtk::Scale>() {
                scale.set_value(*number);
            }
        }
        Val::On(on) => {
            if let Some(row) = widget.downcast_ref::<adw::SwitchRow>() {
                row.set_active(*on);
            } else if let Some(switch) = widget.downcast_ref::<gtk::Switch>() {
                switch.set_active(*on);
            } else if let Some(check) = widget.downcast_ref::<gtk::CheckButton>() {
                check.set_active(*on);
            } else if let Some(toggle) = widget.downcast_ref::<gtk::ToggleButton>() {
                toggle.set_active(*on);
            }
        }
        Val::Pick(index) => {
            if let Some(row) = widget.downcast_ref::<adw::ComboRow>() {
                row.set_selected(*index);
            } else if let Some(picker) = widget.downcast_ref::<gtk::DropDown>() {
                picker.set_selected(*index);
            }
        }
    }
}

fn nudge(widget: &gtk::Widget) {
    if let Some(toggle) = widget.downcast_ref::<gtk::ToggleButton>() {
        toggle.emit_clicked();
        return;
    }
    if let Some(check) = widget.downcast_ref::<gtk::CheckButton>() {
        check.emit_by_name::<()>("activate", &[]);
        return;
    }
    let adjustment = widget
        .downcast_ref::<adw::SpinRow>()
        .map(|row| row.adjustment())
        .or_else(|| widget.downcast_ref::<gtk::Scale>().map(|scale| scale.adjustment()));
    if let Some(adjustment) = adjustment {
        let (low, high, step, now) =
            (adjustment.lower(), adjustment.upper(), adjustment.step_increment(), adjustment.value());
        let reach = (high - low) / 4.0;
        let wanted = if now + reach <= high { now + reach } else { now - reach };
        let snapped = match step > 0.0 {
            true => (low + ((wanted - low) / step).round() * step).clamp(low, high),
            false => wanted,
        };
        adjustment.set_value(if snapped == now { wanted } else { snapped });
        return;
    }
    let items = |model: Option<gio::ListModel>| model.map_or(0, |model| model.n_items());
    match value_of(widget) {
        Some(Val::On(on)) => put(widget, &Val::On(!on)),
        Some(Val::Pick(at)) => {
            let count = widget
                .downcast_ref::<adw::ComboRow>()
                .map(|row| items(row.model()))
                .or_else(|| widget.downcast_ref::<gtk::DropDown>().map(|picker| items(picker.model())))
                .unwrap_or(0);
            if count > 1 {
                put(widget, &Val::Pick((at + 1) % count));
            }
        }
        _ => (),
    }
}

fn skip_reason(state: &App, control: &Control) -> Option<&'static str> {
    let widget = &control.widget;
    let pipettes = [&state.colour.white_pipette, &state.colour.point_pipette, &state.colour.band_pipette];
    if pipettes.iter().any(|pipette| pipette.upcast_ref::<gtk::Widget>() == widget) {
        return Some("a pipette: arms a pick on the canvas, and Pick a neutral is never pressed");
    }
    if control.name.ends_with("AI denoise") {
        return Some("starts SCUNet, a model, on the whole photograph");
    }
    if !widget.is_visible() {
        return Some("hidden in this scope");
    }
    if !widget.is_sensitive() {
        return Some("insensitive");
    }
    None
}

fn page_of(state: &App, tab: &str) -> Option<gtk::Widget> {
    state.panel.stack.child_by_name(tab)
}

fn walk_scope(state: &App, scope: &str, mask: Option<usize>, learned: &mut Vec<Learned>) {
    let start = document_json(state).map(|(raw, _)| raw);
    let tabs: Vec<&'static str> = match mask {
        Some(_) => MASK_TABS.to_vec(),
        None => PANEL_TABS.iter().map(|tab| tab.0).filter(|name| *name != "presets").collect(),
    };
    for tab in tabs {

        if tab != "masks" && tab != "retouch" {
            show_panel_tab(state, tab);
            settle(SETTLE_MS);
        }
        let Some(page) = page_of(state, tab) else { continue };
        let count = controls(&page).len();
        let (mut checked, mut skipped) = (0, 0);
        for index in 0..count {
            match audit_control(state, scope, tab, mask, index, learned) {
                true => checked += 1,
                false => skipped += 1,
            }
        }
        println!("AUDIT {scope} | {tab} | {checked} checked, {skipped} skipped");
    }
    if mask.is_none() {
        show_panel_tab(state, "light");
        settle(SETTLE_MS);
    }
    let end = document_json(state).map(|(raw, _)| raw);
    let back = if start == end { "yes" } else { "NO" };
    println!("AUDIT {scope} | end | document as it started: {back}");
}

fn audit_control(
    state: &App,
    scope: &str,
    tab: &'static str,
    mask: Option<usize>,
    index: usize,
    learned: &mut Vec<Learned>,
) -> bool {
    let Some(page) = page_of(state, tab) else { return false };
    let all = controls(&page);
    let Some(control) = all.get(index) else { return false };
    let name = control.name.clone();
    if let Some(why) = skip_reason(state, control) {
        println!("AUDIT {scope} | {tab} | {name} | skipped: {why}");
        return false;
    }
    let before: Vec<Option<Val>> = all.iter().map(|control| value_of(&control.widget)).collect();
    let Some((raw_before, view_before)) = document_json(state) else { return false };
    let Some(original) = before[index].clone() else { return false };
    let crop_before = state.open.borrow().as_ref().and_then(|photo| photo.document.crop());

    nudge(&control.widget);
    let moved_to = value_of(&control.widget);
    if moved_to.as_ref() == Some(&original) {
        println!("AUDIT {scope} | {tab} | {name} | skipped: a click leaves it where it is (the chosen one of its group)");
        return false;
    }
    settle(SETTLE_MS);
    let Some((raw_moved, view_moved)) = document_json(state) else { return false };
    let written = changes(&view_before, &view_moved);

    put_back(&page, index, &before, &original);
    settle(SETTLE_MS);
    let Some((raw_reset, view_reset)) = document_json(state) else { return false };

    let mut notes = Vec::new();
    if written.is_empty() && raw_moved != raw_before {
        notes.push(format!("stored stack changed: {} → {}", stored_order(&raw_before), stored_order(&raw_moved)));
    }
    if let Some(mask) = mask {
        let own = format!("masks[{mask}].");
        let leaked: Vec<Change> = written.iter().filter(|change| !change.0.starts_with(&own)).cloned().collect();
        if !leaked.is_empty() {
            notes.push(format!("WRITES OUTSIDE THE MASK: {}", describe(&leaked)));
        }
    }
    let exact = raw_reset == raw_before;
    if !exact {
        let left = changes(&view_before, &view_reset);
        notes.push(match left.is_empty() {
            true => format!("stored as {} → {}", stored_order(&raw_before), stored_order(&raw_reset)),
            false => format!("left behind: {}", describe(&left)),
        });
    }
    let now = controls(&page);
    let moved: Vec<&str> = now
        .iter()
        .enumerate()
        .filter(|(at, control)| *at != index && before.get(*at).cloned().flatten() != value_of(&control.widget))
        .map(|(_, control)| control.name.as_str())
        .collect();
    if !moved.is_empty() {
        notes.push(format!("other controls not back: {}", moved.join(", ")));
    }
    if tab == "crop" {
        put_crop_back(state, crop_before, &mut notes);
    }

    let exact = if exact { "yes" } else { "NO" };
    let notes = if notes.is_empty() { String::new() } else { format!(" | {}", notes.join("; ")) };
    println!("AUDIT {scope} | {tab} | {name} | {} | reset exact: {exact}{notes}", describe(&written));

    if mask.is_none() {
        learn(control, tab, &name, &original, moved_to, &written, learned);
    }
    true
}

fn put_back(page: &gtk::Widget, index: usize, before: &[Option<Val>], original: &Val) {

    let now = controls(page);
    for (at, control) in now.iter().enumerate() {
        let was_on = before.get(at) == Some(&Some(Val::On(true)));
        if at != index && was_on && value_of(&control.widget) == Some(Val::On(false)) {
            put(&control.widget, &Val::On(true));
        }
    }
    if let Some(control) = controls(page).get(index) {
        if value_of(&control.widget).as_ref() != Some(original) {
            put(&control.widget, original);
        }
    }
}

fn put_crop_back(state: &App, before: Option<([f32; 4], f32)>, notes: &mut Vec<String>) {
    let now = state.open.borrow().as_ref().and_then(|photo| photo.document.crop());
    if now == before {
        return;
    }
    let (rect, angle) = before.unwrap_or(([0.0, 0.0, 1.0, 1.0], 0.0));
    let view = state.open.borrow().as_ref().map_or(rect, |photo| view_of_crop(photo, rect, angle));
    state.crop.rect.set(view);
    state.crop.area.queue_draw();
    commit_crop(state);
    settle(SETTLE_MS);
    notes.push("rectangle put back by hand afterwards".into());
}

fn learn(
    control: &Control,
    tab: &'static str,
    name: &str,
    original: &Val,
    moved_to: Option<Val>,
    written: &[Change],
    learned: &mut Vec<Learned>,
) {
    let Some(scale) = control.widget.downcast_ref::<gtk::Scale>() else { return };
    let [(path, Some(old), Some(new))] = written else { return };
    let (Val::Num(v0), Some(Val::Num(v1))) = (original, moved_to) else { return };
    let (Some(f0), Some(f1)) = (old.as_f64(), new.as_f64()) else { return };
    if (v1 - v0).abs() < f64::EPSILON {
        return;
    }
    let k = (f1 - f0) / (v1 - v0);
    learned.push(Learned { scale: scale.clone(), tab, name: name.to_string(), path: path.clone(), k, b: f0 - k * v0 });
}

fn asks_a_model(document: &Document) -> bool {
    document.ai_denoise > 0.0
        || document.ai_sharpen > 0.0
        || document.masks().iter().any(|mask| {
            matches!(mask.shape, Shape::Segment { .. }) || !mask.points.is_empty() || mask.matte || mask.fine
        })
}

fn open_id(state: &App) -> Option<i64> {
    match state.open.borrow().as_ref().map(|photo| &photo.source) {
        Some(Source::Photo { id, .. }) => Some(*id),
        _ => None,
    }
}

fn check_steps(state: &App, learned: &[Learned]) {
    let Some(start) = open_id(state) else { return };
    let route: Vec<i64> = {
        let order = state.grid.order.borrow();
        let at = order.iter().position(|id| *id == start).unwrap_or(usize::MAX);
        order.iter().skip(at.saturating_add(1)).take(2).copied().collect()
    };
    if route.len() < 2 {
        println!("AUDIT step | not run: fewer than two photographs after this one");
        return;
    }
    for id in &route {
        if state.catalog.load_edits(*id).ok().flatten().is_some_and(|document| asks_a_model(&document)) {
            println!("AUDIT step | not run: photograph {id} would ask a model when opened");
            return;
        }
    }
    let registered = REGISTERED.with(|registered| registered.borrow().len());
    println!(
        "AUDIT step | {} of {registered} registered sliders have one field to check against",
        learned.len()
    );
    if !step_to(state, true, route[0]) {
        return;
    }
    check_sliders(state, route[0], learned);
    let before = document_json(state).map(|(raw, _)| raw);
    let originals: Vec<f64> = learned.iter().map(|one| one.scale.value()).collect();
    for one in learned {
        nudge(one.scale.upcast_ref());
    }
    settle(SETTLE_MS);
    println!("AUDIT step | {} sliders moved on {}", learned.len(), at(route[0]));
    if !step_to(state, true, route[1]) {
        return;
    }
    check_sliders(state, route[1], learned);
    if !step_to(state, false, route[0]) {
        return;
    }
    check_sliders(state, route[0], learned);
    for (one, value) in learned.iter().zip(originals) {
        one.scale.set_value(value);
    }
    settle(SETTLE_MS);
    let back = if document_json(state).map(|(raw, _)| raw) == before { "yes" } else { "NO" };
    println!("AUDIT step | {} put back as it was: {back}", at(route[0]));
}

fn step_to(state: &App, forward: bool, id: i64) -> bool {
    step_photo(state, forward);
    let opened = wait_until(60_000, || open_id(state) == Some(id));
    match opened {
        true => settle(SETTLE_MS * 2),
        false => println!("AUDIT step | {} did not open", at(id)),
    }
    opened
}

fn at(id: i64) -> String {
    format!("{}:{}", id >> 32, id & 0xffff_ffff)
}

fn check_sliders(state: &App, id: i64, learned: &[Learned]) {
    let Some((_, view)) = document_json(state) else { return };
    let mut leaves = BTreeMap::new();
    flatten(&view, "", &mut leaves);
    let photo = at(id);
    let (mut fine, mut absent, mut stale) = (0, 0, 0);
    for one in learned {
        let Some(field) = leaves.get(&one.path).and_then(Value::as_f64) else {
            absent += 1;
            continue;
        };
        let wanted = (field - one.b) / one.k;
        let adjustment = one.scale.adjustment();
        let tolerance = (adjustment.step_increment() / 2.0).max((adjustment.upper() - adjustment.lower()) * 1e-4);
        if (one.scale.value() - wanted).abs() <= tolerance {
            fine += 1;
            continue;
        }
        stale += 1;
        println!(
            "AUDIT step | {photo} | {} | {} | shows {}, {} is {} (wants {:.3}) | STALE",
            one.tab,
            one.name,
            one.scale.value(),
            one.path,
            field,
            wanted
        );
    }
    println!("AUDIT step | {photo} | {fine} match, {stale} stale, {absent} with no such field");
}
