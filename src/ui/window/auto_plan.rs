use super::*;

use render::auto::plan::{self as auto_level, Level};
use render::auto::scene::{Camera, Scene};

#[derive(Clone)]
pub(super) struct Plan {
    pub(super) card: gtk::Box,
    rows: gtk::Box,
    pub(super) line: gtk::DrawingArea,

    pill: gtk::Label,
    pill_layer: gtk::Fixed,
    shown: Rc<RefCell<Option<Shown>>>,
}

struct Shown {

    offered_on: Option<([f32; 4], f32)>,
    generation: u64,

    horizon: Option<([f32; 4], String)>,

    outline: Option<Vec<Vec<[f32; 2]>>>,

    crops: Vec<render::auto::crop::Proposal>,

    hovered: Option<usize>,
    as_shot: bool,
    before: Document,
    before_shown: Option<gtk::gdk::Paintable>,
}

impl Shown {
    fn before_crop(&self) -> ([f32; 4], f32) {
        self.offered_on.unwrap_or_else(|| self.before.crop().unwrap_or(([0.0, 0.0, 1.0, 1.0], 0.0)))
    }
}

impl Plan {
    pub(super) fn new() -> Self {
        let card = gtk::Box::new(gtk::Orientation::Vertical, 8);
        card.add_css_class("photo-card");

        card.add_css_class("numa-content");

        card.set_halign(gtk::Align::Start);
        card.set_valign(gtk::Align::Start);
        card.set_margin_top(12);
        card.set_margin_start(12);
        card.set_visible(false);
        let line = gtk::DrawingArea::new();
        line.set_can_target(false);
        line.set_visible(false);
        let pill = gtk::Label::new(None);
        pill.add_css_class("photo-pill");
        let pill_layer = gtk::Fixed::new();
        pill_layer.set_can_target(false);
        pill_layer.put(&pill, 0.0, 0.0);
        pill_layer.set_visible(false);
        Self { card, rows: gtk::Box::new(gtk::Orientation::Vertical, 6), line, pill, pill_layer, shown: Rc::default() }
    }
}

pub(super) fn build_plan_card(state: &App) -> gtk::Box {
    let plan = &state.light.plan;
    let heading = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    let title = gtk::Label::new(Some("AUTO"));
    title.add_css_class("section-header");
    title.set_hexpand(true);
    title.set_xalign(0.0);
    let close = gtk::Button::from_icon_name("window-close-symbolic");
    close.add_css_class("flat");
    close.set_tooltip_text(Some("Close"));
    close.connect_clicked(glib::clone!(
        #[strong] state,
        move |_| hide_plan(&state)
    ));
    heading.append(&title);
    heading.append(&close);

    let foot = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    let compare = gtk::Button::with_label("Compare");
    compare.add_css_class("flat");
    compare.set_tooltip_text(Some("The photograph before Auto, beside it"));
    compare.connect_clicked(glib::clone!(
        #[strong] state,
        move |_| {
            if let Some(frame) = before_frame(&state) {
                hold_frame(&state, &frame, "Before Auto");
            }
        }
    ));
    let undo = gtk::Button::with_label("Undo All");
    undo.add_css_class("flat");
    undo.set_tooltip_text(Some("The photograph as it was before Auto"));
    undo.connect_clicked(glib::clone!(
        #[strong] state,
        move |_| {
            let before = state.light.plan.shown.borrow().as_ref().map(|shown| shown.before.clone());
            if let Some(before) = before {
                restore_snapshot(&state, "Before Auto", &before);
            }
            hide_plan(&state);
        }
    ));
    foot.append(&compare);
    foot.append(&undo);

    plan.card.append(&heading);
    plan.card.append(&plan.rows);
    plan.card.append(&foot);
    plan.card.clone()
}

pub(super) fn build_plan_line(state: &App) -> gtk::DrawingArea {
    let area = state.light.plan.line.clone();
    state.canvas.connect_paintable_notify(glib::clone!(
        #[weak] area,
        move |_| area.queue_draw()
    ));
    area.set_draw_func(glib::clone!(
        #[strong] state,
        move |_, context, width, height| {
            let hide = || {
                let layer = state.light.plan.pill_layer.clone();
                if layer.is_visible() {
                    glib::idle_add_local_once(move || layer.set_visible(false));
                }
            };
            if is_cropping(&state) {
                return hide();
            }
            let (x, y, w, h) = content_rect(&state, width as f64, height as f64);
            draw_subject(&state, context, (x, y, w, h));
            draw_crops(&state, context, (x, y, w, h));
            let shown = state.light.plan.shown.borrow();
            let Some(([u0, v0, u1, v1], label)) = shown.as_ref().and_then(|shown| shown.horizon.clone()) else { return hide() };
            let open = state.open.borrow();
            let Some(photo) = open.as_ref() else { return };
            let (rect, angle) = photo.document.crop().unwrap_or(([0.0, 0.0, 1.0, 1.0], 0.0));
            let (frame_w, frame_h) = oriented_pixels(photo);
            let into = |point| numa::core::image::into_crop(frame_w, frame_h, rect, angle, photo.document.perspective(), point);
            let (a, b) = (into([u0, v0]), into([u1, v1]));
            let at = |p: [f32; 2]| (x + w * p[0] as f64, y + h * p[1] as f64);
            let ((ax, ay), (bx, by)) = (at(a), at(b));
            context.save().ok();
            context.rectangle(x, y, w, h);
            context.clip();
            for (colour, width) in [(0.0, 3.0), (1.0, 1.5)] {
                context.set_source_rgba(colour, colour, colour, 0.85);
                context.set_line_width(width);
                context.move_to(ax, ay);
                context.line_to(bx, by);
                let _ = context.stroke();
            }

            let pill = &state.light.plan.pill;
            let (pw, ph) = (pill.width().max(1) as f64, pill.height().max(1) as f64);
            let px = (x + w - pw - 12.0).max(x);
            let t = ((px + pw / 2.0 - ax) / (bx - ax)).clamp(0.0, 1.0);
            let py = (ay + (by - ay) * t - ph - 6.0).clamp(y, y + h - ph);
            let (layer, pill, label) = (state.light.plan.pill_layer.clone(), pill.clone(), label.clone());
            glib::idle_add_local_once(move || {
                pill.set_text(&label);
                layer.move_(&pill, px, py);
                layer.set_visible(true);
            });
            context.restore().ok();
        }
    ));
    area
}

fn draw_subject(state: &App, context: &gtk::cairo::Context, content: (f64, f64, f64, f64)) {
    let shown = state.light.plan.shown.borrow();
    if let Some(paths) = shown.as_ref().and_then(|shown| shown.outline.as_ref()) {
        draw_ants(context, content, paths, 0.0);
    }
}

fn draw_crops(state: &App, context: &gtk::cairo::Context, (x, y, w, h): (f64, f64, f64, f64)) {
    let shown = state.light.plan.shown.borrow();

    let Some(shown) = shown.as_ref().filter(|shown| shown.as_shot && !shown.crops.is_empty() && !previewing()) else { return };
    let open = state.open.borrow();
    let Some(photo) = open.as_ref() else { return };
    let (rect, angle) = photo.document.crop().unwrap_or(([0.0, 0.0, 1.0, 1.0], 0.0));
    let (fw, fh) = oriented_pixels(photo);
    let perspective = photo.document.perspective();
    let outline = |crop: &render::auto::crop::Proposal| {
        for (i, corner) in numa::core::image::crop_corners(fw, fh, crop.rect, angle, perspective).iter().enumerate() {
            let [u, v] = numa::core::image::into_crop(fw, fh, rect, angle, perspective, *corner);
            let (px, py) = (x + w * u as f64, y + h * v as f64);
            if i == 0 { context.move_to(px, py) } else { context.line_to(px, py) }
        }
        context.close_path();
    };
    let hovered = shown.hovered.and_then(|at| shown.crops.get(at));
    context.set_line_width(1.5);
    for (at, crop) in shown.crops.iter().enumerate() {
        let alpha = match hovered {
            Some(one) if std::ptr::eq(one, crop) => 0.9,
            Some(_) => 0.2,
            None => 0.8 - at as f64 * 0.2,
        };
        for (colour, offset) in [(0.0, 0.0), (1.0, 5.0)] {
            context.set_source_rgba(colour, colour, colour, alpha);
            context.set_dash(&[5.0, 5.0], offset);
            outline(crop);
            let _ = context.stroke();
        }
    }
    context.set_dash(&[], 0.0);
}

fn hover_crop(state: &App, at: Option<usize>) {
    let wanted = state.light.plan.shown.borrow_mut().as_mut().and_then(|shown| {
        shown.hovered = at;
        let (_, angle) = shown.before_crop();
        at.and_then(|at| shown.crops.get(at)).map(|crop| (crop.rect, angle))
    });
    state.light.plan.line.queue_draw();
    match wanted {
        Some((rect, angle)) => preview_document(state, move |photo| {
            let mut document = photo.document.clone();
            auto_level::apply(&mut document, rect, angle);
            Some(document)
        }),
        None => presets::end_preview(state),
    }
}

pub(super) fn build_plan_pill(state: &App) -> gtk::Fixed {
    state.light.plan.pill_layer.clone()
}

pub(super) fn hide_plan(state: &App) {
    let plan = &state.light.plan;
    plan.shown.replace(None);
    plan.card.set_visible(false);
    plan.line.set_visible(false);
    plan.pill_layer.set_visible(false);
}

pub(super) fn auto_press(state: &App) {
    let before = state.open.borrow().as_ref().map(|photo| photo.document.clone());
    let Some(before) = before else { return };
    let generation = state.open_generation.get();
    let state = state.clone();
    glib::spawn_future_local(async move {

        let scene = scene_now(&state, &state.light.auto_waiting).await;
        if state.open_generation.get() != generation {
            return;
        }
        let level = scene.as_ref().map(|scene| auto_level::level(scene, &before));
        if let Some(Level::Apply { rect, angle, .. }) = level {
            reframe(&state, rect, angle, None, "Auto · level");
        }
        let horizon = scene.as_ref().and_then(|scene| scene.horizon().zip(level)).and_then(|(line, level)| match level {
            Level::Apply { angle, by, .. } | Level::Offer { angle, by, .. } => {
                Some((line, format!("{:.1}° · {}", angle.abs(), by.name().trim_start_matches("the "))))
            }
            Level::Leave(_) => None,
        });
        state.light.plan.shown.replace(Some(Shown { generation, horizon, outline: None, crops: Vec::new(), hovered: None, as_shot: true, offered_on: None, before, before_shown: None }));

        let framing = state.open.borrow().as_ref().filter(|photo| photo.document.retouch().is_identity()).map(|photo| {
            let (rect, angle) = photo.document.crop().unwrap_or(([0.0, 0.0, 1.0, 1.0], 0.0));
            (rect, angle, photo.document.perspective(), photo.full_size)
        });
        let crops = match (scene.clone(), framing) {
            (Some(scene), Some((rect, angle, perspective, size))) => {
                busy_in(&state.light.auto_waiting, move || render::auto::crop::proposals(&scene, rect, angle, perspective, size)).await.ok()
            }
            _ => None,
        };
        if state.open_generation.get() != generation {
            return;
        }
        if let (Some(shown), Some((crops, true)), Some((rect, angle, _, _))) = (state.light.plan.shown.borrow_mut().as_mut(), crops.clone(), framing) {
            shown.crops = crops;
            shown.offered_on = Some((rect, angle));
        }
        let rows_state = state.clone();
        auto_tone_then(&state, move |applied, subject| {
            push_named(&rows_state, "Auto · light");

            if let (Some(shown), Some(subject)) = (rows_state.light.plan.shown.borrow_mut().as_mut(), subject.filter(|_| applied.subject.is_some())) {
                let mut paths = numa::core::mask::outline(&subject, OUTLINE_EDGE);
                paths.truncate(400);
                shown.outline = Some(paths);
            }
            fill_plan(&rows_state, scene.as_deref(), level, &auto_tone_toast(applied), crops.map_or(false, |(found, _)| !found.is_empty()));
        });
    });
}

enum Act {
    Switch(bool, Box<dyn Fn(&App, bool)>),
    Button(&'static str, Box<dyn Fn(&App)>),

    Choose(Vec<String>, Box<dyn Fn(&App, usize)>, Box<dyn Fn(&App, Option<usize>)>),
}

fn fill_plan(state: &App, scene: Option<&Scene>, level: Option<Level>, light: &str, read_crops: bool) {
    let plan = &state.light.plan;
    while let Some(row) = plan.rows.first_child() {
        plan.rows.remove(&row);
    }
    let Some(before) = plan.shown.borrow().as_ref().map(|shown| shown.before.clone()) else { return };
    let (was_rect, was_angle) = before.crop().unwrap_or(([0.0, 0.0, 1.0, 1.0], 0.0));
    let was_vertical = before.perspective().vertical;
    let level_switch = |rect: [f32; 4], angle: f32, on: bool| {
        Act::Switch(on, Box::new(move |state: &App, on| {
            match on {
                true => reframe(state, rect, angle, None, "Auto · level"),
                false => reframe(state, was_rect, was_angle, None, "Auto · level off"),
            }
            relight(state);
        }))
    };
    let level_row = match level {
        Some(level @ Level::Apply { rect, angle, .. }) => ("Level", level.says(), Some(level_switch(rect, angle, true))),
        Some(level @ Level::Offer { rect, angle, .. }) => ("Level", level.says(), Some(level_switch(rect, angle, false))),
        Some(level) => ("Level", level.says(), None),
        None => ("Level", "Not read on a merged photograph".to_string(), None),
    };
    let upright_row = scene.and_then(|scene| {
        let vertical = auto_level::upright(scene, before.perspective())?;
        if !before.masks().is_empty() || !before.retouch().is_identity() {
            return Some(("Verticals", "Not squared up: your masks or spots would move".to_string(), None));
        }
        let lean = scene.upright?;
        let size = (scene.size[0] as f32, scene.size[1] as f32);
        let square = Act::Switch(false, Box::new(move |state: &App, on| match on {
            true => square_up(state, vertical, lean, size),
            false => {
                reframe(state, was_rect, was_angle, Some(was_vertical), "Auto · verticals off");
                relight(state);
            }
        }));
        Some(("Verticals", format!("The buildings lean: square them up by {vertical:+.0}"), Some(square)))
    });
    let light_row = ("Light", light.to_string(), Some(Act::Switch(true, Box::new(move |state: &App, on| match on {
        true => relight(state),
        false => light_back(state),
    }))));
    let (others, sharper) = burst_now(state);
    let burst_row = sharper.map(|(best, at, of)| {
        let open = Act::Button("Open", Box::new(move |state: &App| open_photo(state, best)));
        ("Burst", format!("Frame {at} of {of} is the sharpest of this burst"), Some(open))
    });

    let same_row = (!others.is_empty()).then(|| {
        let n = others.len();
        let give = Act::Button("Apply", Box::new(move |state: &App| same_light(state, others.clone())));
        let what = if n == 1 { "the other frame".to_string() } else { format!("the other {n} frames") };
        ("Same Light", format!("Give {what} of this burst this light"), Some(give))
    });
    let crops = plan.shown.borrow().as_ref().map(|shown| shown.crops.clone()).unwrap_or_default();
    let crop_row = match (crops.is_empty(), read_crops) {
        (false, _) => {
            let says = crops.iter().enumerate().map(|(i, c)| format!("{}. {}", i + 1, c.why)).collect::<Vec<_>>().join("\n");
            let labels = std::iter::once("As shot".to_string()).chain((1..=crops.len()).map(|i| i.to_string())).collect();
            let choose = Act::Choose(labels, Box::new(choose_crop), Box::new(|state: &App, at: Option<usize>| hover_crop(state, at.and_then(|at| at.checked_sub(1)))));
            Some(("Crop", says, Some(choose)))
        }
        (true, _) if !before.retouch().is_identity() => Some(("Crop", "No crops offered: healed spots would move".to_string(), None)),
        (true, true) => Some(("Crop", "Nothing in the frame asks for a crop".to_string(), None)),
        (true, false) => None,
    };
    for (name, says, act) in [Some(level_row), upright_row, Some(light_row), crop_row, burst_row, same_row].into_iter().flatten() {
        plan.rows.append(&plan_row(state, name, &says, act));
    }

    if !plan.card.is_visible() {
        plan.card.set_opacity(0.0);
        plan.card.set_visible(true);
        let target = adw::PropertyAnimationTarget::new(&plan.card, "opacity");
        let fade = adw::TimedAnimation::new(&plan.card, 0.0, 1.0, 200, target);
        fade.set_easing(adw::Easing::EaseOutCubic);
        fade.play();
    }
    plan.line.set_visible(true);
    plan.line.queue_draw();
}

fn plan_row(state: &App, name: &str, says: &str, act: Option<Act>) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    let words = gtk::Box::new(gtk::Orientation::Vertical, 0);
    words.set_hexpand(true);
    let part = gtk::Label::new(Some(name));
    part.add_css_class("heading");
    part.set_xalign(0.0);
    let why = gtk::Label::new(Some(says));
    why.add_css_class("caption");
    why.add_css_class("dim-label");
    why.set_wrap(true);
    why.set_max_width_chars(36);
    why.set_xalign(0.0);
    words.append(&part);
    words.append(&why);
    row.append(&words);
    match act {
        Some(Act::Switch(on, act)) => {
            let switch = gtk::Switch::new();
            switch.set_active(on);
            switch.set_valign(gtk::Align::Center);
            switch.update_property(&[gtk::accessible::Property::Label(name)]);
            switch.connect_active_notify(glib::clone!(
                #[strong] state,
                move |switch| act(&state, switch.is_active())
            ));
            row.append(&switch);
        }
        Some(Act::Choose(labels, act, hover)) => {

            let group = gtk::Box::new(gtk::Orientation::Horizontal, 0);
            group.add_css_class("linked");
            group.set_valign(gtk::Align::Center);
            let act = Rc::new(act);
            let hover = Rc::new(hover);
            let mut first: Option<gtk::ToggleButton> = None;
            for (at, label) in labels.iter().enumerate() {
                let button = gtk::ToggleButton::with_label(label);
                button.set_group(first.as_ref());
                button.set_active(at == 0);
                button.connect_toggled(glib::clone!(
                    #[strong] state,
                    #[strong] act,
                    move |button| {
                        if button.is_active() {
                            act(&state, at);
                        }
                    }
                ));
                let motion = gtk::EventControllerMotion::new();
                motion.connect_enter(glib::clone!(
                    #[strong] state,
                    #[strong] hover,
                    move |_, _, _| hover(&state, Some(at))
                ));
                motion.connect_leave(glib::clone!(
                    #[strong] state,
                    #[strong] hover,
                    move |_| hover(&state, None)
                ));
                button.add_controller(motion);
                first.get_or_insert(button.clone());
                group.append(&button);
            }
            row.append(&group);
        }
        Some(Act::Button(label, act)) => {
            let button = gtk::Button::with_label(label);
            button.set_valign(gtk::Align::Center);
            button.connect_clicked(glib::clone!(
                #[strong] state,
                move |_| act(&state)
            ));
            row.append(&button);
        }
        None => {}
    }
    row
}

fn choose_crop(state: &App, at: usize) {
    let chosen = state.light.plan.shown.borrow().as_ref().map(|shown| {
        let shot = shown.before_crop();
        (shot, if at == 0 { None } else { shown.crops.get(at - 1).map(|c| c.rect) })
    });
    let Some(((shot_rect, angle), crop)) = chosen else { return };
    match crop {
        Some(rect) => reframe(state, rect, angle, None, "Auto · crop"),
        None => reframe(state, shot_rect, angle, None, "Auto · as shot"),
    }
    if let Some(shown) = state.light.plan.shown.borrow_mut().as_mut() {
        shown.as_shot = crop.is_none();
    }
    state.light.plan.line.queue_draw();
    relight(state);
}

fn light_back(state: &App) {
    let Some(before) = state.light.plan.shown.borrow().as_ref().map(|shown| shown.before.clone()) else { return };
    let basic = {
        let mut open = state.open.borrow_mut();
        let Some(photo) = open.as_mut() else { return };
        photo.history.push(EditState::of(&photo.document));
        let (mut basic, was) = (photo.document.basic(), before.basic());
        basic.tone.exposure = was.tone.exposure;
        basic.tone.highlights = was.tone.highlights;
        basic.tone.whites = was.tone.whites;
        basic.tone.blacks = was.tone.blacks;
        basic.presence.hdr = was.presence.hdr;
        basic.presence.vibrance = was.presence.vibrance;
        photo.document.set_basic(basic);
        let angle = photo.document.auto.and_then(|record| record.angle);
        let vertical = photo.document.auto.and_then(|record| record.vertical);
        let record = numa::core::document::AutoRecord { angle, vertical, ..before.auto.unwrap_or_default() };
        photo.document.auto = (record != Default::default()).then_some(record);
        photo.view = None;
        basic
    };
    if state.mask_overlay.selected_mask.get().is_none() {
        state.applying.set(true);
        state.sliders.write(basic);
        state.applying.set(false);
        refresh_slider_marks(state);
    }
    adjustments_changed(state);
    push_named(state, "Auto · light off");
    request_render(state);
}

fn burst_now(state: &App) -> (Vec<(i64, std::path::PathBuf)>, Option<(i64, usize, usize)>) {
    let id = match state.open.borrow().as_ref().map(|photo| photo.source.clone()) {
        Some(Source::Photo { id, .. }) => id,
        _ => return (Vec::new(), None),
    };
    let Ok(photos) = state.catalog.photos(numa::io::catalog::library_of(id), &Default::default()) else { return (Vec::new(), None) };
    let others = numa::io::catalog::burst_of(&photos, id).into_iter().filter(|photo| photo.id != id).map(|photo| (photo.id, photo.path.clone())).collect();
    let sharper = numa::io::catalog::sharper_in_burst(&photos, id).map(|(best, at, of)| (best.id, at, of));
    (others, sharper)
}

fn same_light(state: &App, others: Vec<(i64, std::path::PathBuf)>) {
    let Some((reference, working)) = state.open.borrow().as_ref().map(|photo| (photo.document.clone(), photo.working.clone())) else { return };
    let Some(wanted) = render::auto::key(&render::auto::framed(&reference, &working), &reference) else { return };
    let basic = reference.basic();
    let documents: Vec<(i64, std::path::PathBuf, Document)> = others
        .into_iter()
        .map(|(id, path)| {
            let document = state.catalog.load_edits(id).ok().flatten().unwrap_or_else(|| Document::new(path.to_string_lossy().to_string()));
            (id, path, document)
        })
        .collect();
    let state = state.clone();
    glib::spawn_future_local(async move {
        let matched = busy_in(&state.light.auto_waiting, move || {
            documents
                .into_iter()
                .filter_map(|(id, path, mut document)| {
                    let (proxy, _) = raw::editor_proxy(&path, AUTO_PROXY).ok()?;
                    let inputs = numa::io::inputs::render_inputs(&document);
                    let working = render::to_working_space(&document, &proxy, &inputs);
                    let shown = render::auto::framed(&document, &working);
                    render::auto::match_light(&shown, &document, &basic, wanted).apply(&mut document);
                    Some((id, document))
                })
                .collect::<Vec<_>>()
        })
        .await;
        let Ok(matched) = matched else { return };
        let failed = matched.iter().filter(|(id, document)| state.catalog.save_edits(*id, document).is_err()).count();
        reload_grid(&state);
        state.toast(&match failed {
            0 => format!("This light given to {} more of the burst", matched.len()),
            n => format!("This light given to {} more of the burst, {n} failed", matched.len() - n),
        });
    });
}

const AUTO_PROXY: u32 = 2400;

fn relight(state: &App) {
    let state = state.clone();
    auto_tone_then(&state.clone(), move |_, _| push_named(&state, "Auto · light"));
}

fn square_up(state: &App, vertical: f32, lean: render::auto::evidence::Lean, (width, height): (f32, f32)) {
    let Some((rect, angle, was)) = state.open.borrow().as_ref().map(|photo| {
        let (rect, angle) = photo.document.crop().unwrap_or(([0.0, 0.0, 1.0, 1.0], 0.0));
        (rect, angle, photo.document.perspective())
    }) else {
        return;
    };
    let perspective = Perspective { vertical, ..was };
    let roll = render::auto::evidence::Evidence {
        source: render::auto::evidence::Source::Buildings,
        reading: render::auto::evidence::Reading::Lean(lean.lean),
        sigma: lean.sigma,
    }
    .angle(perspective, width, height);
    let view = numa::core::image::crop_in_view(width, height, rect, angle, was);
    let wanted = numa::core::image::crop_from_view(width, height, view, roll, perspective);
    let kept = numa::core::image::crop_inside(wanted, roll, perspective, width, height);
    reframe(state, kept, roll, Some(vertical), "Auto · verticals");
    relight(state);
}

fn reframe(state: &App, rect: [f32; 4], angle: f32, vertical: Option<f32>, name: &str) {
    let was = geometry_now(state);
    {
        let mut open = state.open.borrow_mut();
        let Some(photo) = open.as_mut() else { return };
        photo.history.push(EditState::of(&photo.document));
        if let Some(vertical) = vertical {
            photo.document.set_perspective(Perspective { vertical, ..photo.document.perspective() });
            let mut record = photo.document.auto.unwrap_or_default();
            record.vertical = (vertical != 0.0).then_some(vertical);
            photo.document.auto = Some(record);
        }
        auto_level::apply(&mut photo.document, rect, angle);
        photo.view = None;
    }
    state.applying.set(true);
    state.crop.straighten.set_value(angle as f64);
    state.applying.set(false);
    write_perspective(state);

    carry_masks(state, was);
    push_named(state, name);
    request_render(state);
    if needs_full_resolution(state, state.zooming.level.get()) {
        ensure_full_resolution(state);
    }
}

fn push_named(state: &App, name: &str) {
    if let Some(photo) = state.open.borrow_mut().as_mut() {
        photo.history.push_named(EditState::of(&photo.document), name);
    }
    schedule_save(state);
    refresh_history(state);
}

pub(super) fn camera_of(path: &Path, document: &Document, summary: Option<&raw::Summary>) -> Camera {
    Camera {
        focal35: raw::shot(path).map(|(_, focal)| focal),
        roll: raw::roll_angle(path),
        bias: summary.and_then(|summary| summary.exposure_bias),
        program: summary.and_then(|summary| summary.exposure_program),
        dynamic_range: raw::dynamic_range_mode(path),
        focus: raw::af_point(path).filter(|point| !point.zone && document.rotation() == 0.0 && !document.mirrored()).map(|point| [point.x, point.y]),
    }
}

pub(super) async fn scene_now(state: &App, waiting: &Waiting) -> Option<Arc<Scene>> {
    let (id, path, document, working, found, summary) = {
        let open = state.open.borrow();
        let photo = open.as_ref()?;
        let Source::Photo { id, path } = &photo.source else { return None };

        let found = photo.segmentation.clone().filter(|_| Scene::same_frame(&photo.document));
        (*id, path.clone(), photo.document.clone(), photo.working.clone(), found, photo.summary.clone())
    };
    if let Some(scene) = state.catalog.scene(id, &Scene::asked(&document)) {
        return Some(Arc::new(scene));
    }
    let generation = state.open_generation.get();
    let read = busy_in(waiting, move || {
        let camera = camera_of(&path, &document, summary.as_ref());
        render::auto::scene::read(&document, &working, found, camera)
    })
    .await
    .ok()?;
    if state.open_generation.get() != generation {
        return None;
    }
    if let Err(err) = state.catalog.save_scene(id, &read) {
        log::warn!("the scene read was not kept: {err}");
    }
    Some(Arc::new(read))
}

pub(super) fn show_before_auto(state: &App) -> bool {
    if !state.light.plan.card.is_visible() {
        return false;
    }
    let generation = state.open_generation.get();
    let held = state.light.plan.shown.borrow().as_ref().filter(|shown| shown.generation == generation).map(|shown| shown.before_shown.clone());
    let paintable = match held {
        None => return false,
        Some(Some(held)) => Some(held),
        Some(None) => {
            let made = before_frame(state).map(|frame| crate::ui::pixel_paintable::PixelPaintable::new(texture_from(&frame)).upcast());
            if let Some(shown) = state.light.plan.shown.borrow_mut().as_mut() {
                shown.before_shown = made.clone();
            }
            made
        }
    };
    if let Some(paintable) = paintable {
        state.light.plan.line.set_visible(false);
        state.light.plan.pill_layer.set_visible(false);
        state.canvas.set_paintable(Some(&paintable));
        apply_zoom(state);
    }
    true
}

fn before_frame(state: &App) -> Option<image::RgbImage> {
    let shown = state.light.plan.shown.borrow();
    let before = &shown.as_ref().filter(|shown| shown.generation == state.open_generation.get())?.before;
    let open = state.open.borrow();
    let photo = open.as_ref()?;
    let working = render::to_working_space(before, &*photo.proxy, &photo.inputs);
    let proxy_scale = photo.proxy.width.max(photo.proxy.height) as f32 / photo.full_size.0.max(photo.full_size.1).max(1) as f32;
    Some(render::apply_stack(before, &working, proxy_scale))
}

pub(super) fn end_before_auto(state: &App) {
    state.light.plan.line.set_visible(state.light.plan.card.is_visible());
    render_current(state);
}
