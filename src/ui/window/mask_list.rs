use super::*;

pub(super) fn refresh_masks(state: &App) {
    let masks = state.open.borrow().as_ref().map(|photo| photo.document.masks());
    let Some(masks) = masks else { return };
    let selected = state.mask_overlay.selected_mask.get();

    while let Some(row) = state.masks.mask_list.first_child() {
        state.masks.mask_list.remove(&row);
    }
    for (index, mask) in masks.iter().enumerate() {
        append_mask_list_row(state, &masks, selected, index, mask);
    }

    refresh_mask_parts(state, &masks, selected);
    refresh_outline(state);

    refresh_mask_toolbar(state);
    state.masks.mask_list.set_visible(!masks.is_empty());
    state.masks.mask_empty.set_visible(masks.is_empty());

    let current = selected.filter(|index| *index < masks.len());

    if let Some(mask) = current.map(|index| &masks[index]) {
        state.applying.set(true);
        state.editor_page.banner_eye.set_active(mask.visible);
        state.applying.set(false);
        state.editor_page.banner_eye.set_icon_name(if mask.visible {
            "view-reveal-symbolic"
        } else {
            "view-conceal-symbolic"
        });
        state.editor_page.banner_eye.set_tooltip_text(Some(if mask.visible {
            "Hide this mask"
        } else {
            "Show this mask"
        }));
    }
}

fn append_mask_list_row(
    state: &App,
    masks: &[Mask],
    selected: Option<usize>,
    index: usize,
    mask: &Mask,
) {
    let row = adw::ActionRow::new();
    row.set_title_lines(1);
    row.set_subtitle_lines(1);
    set_row_title(&row, &mask_label(masks, index));
    row.set_subtitle(&match (mask.visible, mask.is_pending(), mask.is_idle()) {
        (false, ..) => "Hidden".to_string(),
        (_, true, _) => "Working out where it is…".to_string(),
        (_, _, true) => "Nothing set yet".to_string(),

        _ if mask.opacity < 0.999 => format!("{:.0}% strength", mask.opacity * 100.0),
        _ => String::new(),
    });
    row.set_activatable(true);
    row.connect_activated(glib::clone!(
        #[strong] state,
        move |_| select_mask(&state, Some(index))
    ));
    if Some(index) == selected {
        row.add_css_class("current-mask");
    }

    let grip = gtk::Image::from_icon_name("list-drag-handle-symbolic");
    grip.add_css_class("dim-label");
    row.add_prefix(&grip);

    let shown = gtk::ToggleButton::new();
    shown.set_icon_name(if mask.visible {
        "view-reveal-symbolic"
    } else {
        "view-conceal-symbolic"
    });
    shown.set_active(mask.visible);
    shown.set_valign(gtk::Align::Center);
    shown.add_css_class("flat");
    shown.set_tooltip_text(Some(if mask.visible { "Hide this mask" } else { "Show this mask" }));
    shown.connect_toggled(glib::clone!(
        #[strong] state,
        move |button| {
            if !state.applying.get() {
                set_mask_visible(&state, index, button.is_active());
            }
        }
    ));
    row.add_suffix(&shown);

    let invert = gtk::ToggleButton::new();
    invert.set_icon_name("object-flip-horizontal-symbolic");
    invert.set_active(mask.inverted);
    invert.set_valign(gtk::Align::Center);
    invert.add_css_class("flat");
    invert.set_tooltip_text(Some("Invert — everything except this"));
    invert.connect_toggled(glib::clone!(
        #[strong] state,
        move |button| {
            if !state.applying.get() {
                set_mask_inverted(&state, index, button.is_active());
            }
        }
    ));
    row.add_suffix(&invert);

    let delete = gtk::Button::from_icon_name("user-trash-symbolic");
    delete.set_valign(gtk::Align::Center);
    delete.add_css_class("flat");
    delete.set_tooltip_text(Some("Delete this mask"));
    delete.connect_clicked(glib::clone!(
        #[strong] state,
        move |_| remove_mask(&state, index)
    ));
    row.add_suffix(&delete);

    add_mask_reordering(state, &row, index);
    state.masks.mask_list.append(&row);
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum MaskTool {

    Off,
    Brush,
    Lasso,
}

pub(super) const BRUSH_FEATHER: f32 = 0.5;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum MaskKind {
    Linear,
    Radial,
    Brush,

    ColourRange,
    LuminanceRange,

    Click,
}

pub(super) fn refresh_found(state: &App) {
    let (found, looking, animal) = {
        let open = state.open.borrow();
        match open.as_ref() {
            Some(photo) => (photo.segmentation.clone(), photo.segmenting, photo.animal.clone()),
            None => (None, false, None),
        }
    };

    let row = &state.masks.found_box;
    while let Some(child) = row.first_child() {
        row.remove(&child);
    }

    let note = |text: &str| {
        let chip = gtk::Button::with_label(text);
        chip.set_sensitive(false);
        chip.set_hexpand(true);
        row.append(&chip);
    };
    let Some(found) = found else {
        if looking {
            note("Looking at the photograph…");
        } else if !segment::is_installed() {
            note("No model installed");
        }
        return;
    };

    let groups = found.found();

    let subject: Vec<u16> = groups
        .iter()
        .flat_map(|thing| thing.classes.iter().copied())
        .filter(|class| segment::MATTEABLE.contains(class))
        .collect();

    let mut things: Vec<(segment::Found, bool, bool)> = Vec::new();
    if numa::render::matte::is_installed() {
        let classes = match subject.is_empty() {
            true => segment::MATTEABLE.to_vec(),
            false => subject,
        };
        for (name, inverted) in [("Subject", false), ("Background", true)] {
            things.push((
                segment::Found { name: name.to_string(), classes: classes.clone(), share: 0.0 },
                inverted,
                true,
            ));
        }
    }

    let mut groups = groups;
    groups.sort_by_key(|thing| !matches!(thing.name.as_str(), "Sky" | "Water"));
    things.extend(groups.into_iter().map(|thing| (thing, false, false)));
    if things.is_empty() {
        note("Nothing it could name");
        return;
    }

    let animal = animal
        .filter(|(asked, _)| std::ptr::eq(asked.as_ptr(), Arc::as_ptr(&found)))
        .map(|(_, guess)| guess);

    for (thing, inverted, pair) in things {
        let mut tooltip = match (thing.share > 0.0, inverted) {
            (true, _) => format!("About {:.0} % of this photograph", thing.share * 100.0),
            (false, false) => "What the photograph is of, whatever it is".to_string(),
            (false, true) => "Everything but what the photograph is of".to_string(),
        };
        let mut name = thing.name.clone();

        let is_animal_group = !pair && thing.classes.as_slice() == [126];
        if let Some(guess) = animal.as_ref().filter(|_| is_animal_group) {
            name = guess.name.to_string();
            let noun = guess.name.to_lowercase();
            let article = if noun.starts_with(['a', 'e', 'i', 'o', 'u']) { "an" } else { "a" };
            let sure = format!("Probably {article} {noun} — {:.0} %", guess.confidence * 100.0);

            tooltip = match thing.share > 0.0 {
                true => format!("{sure}. {tooltip}"),
                false => sure,
            };
        }
        let chip = gtk::Button::with_label(&name);
        chip.set_hexpand(true);
        chip.set_tooltip_text(Some(&tooltip));
        let classes = thing.classes.clone();
        let chosen = name.clone();
        chip.connect_clicked(glib::clone!(
            #[strong] state,
            move |_| add_segment_mask(&state, classes.clone(), &chosen, inverted)
        ));

        let hover = gtk::EventControllerMotion::new();
        let classes = thing.classes.clone();
        hover.connect_enter(glib::clone!(
            #[strong] state,
            move |_, _, _| preview_found(&state, &classes)
        ));
        hover.connect_leave(glib::clone!(
            #[strong] state,
            move |_| end_preview(&state)
        ));
        chip.add_controller(hover);
        row.append(&chip);
    }
}

pub(super) fn preview_found(state: &App, classes: &[u16]) {
    let paths = {
        let open = state.open.borrow();
        let Some(found) = open.as_ref().and_then(|photo| photo.segmentation.clone()) else {
            return;
        };
        let coarse = found.coarse(classes);
        let alpha = numa::core::mask::Stored::new(&Alpha::new(coarse.width, coarse.height, coarse.data));
        let mut paths = numa::core::mask::outline(&alpha, OUTLINE_EDGE);
        paths.truncate(400);
        paths
    };
    if paths.is_empty() {
        return;
    }
    *state.mask_overlay.outline.borrow_mut() = paths;
    state.mask_overlay.previewing.set(true);
    state.mask_overlay.area.set_visible(true);
    start_ants(state);
    state.mask_overlay.area.queue_draw();
}

pub(super) fn end_preview(state: &App) {
    if !state.mask_overlay.previewing.get() {
        return;
    }
    state.mask_overlay.previewing.set(false);
    state.mask_overlay.area.set_visible(state.mask_overlay.selected_mask.get().is_some());
    refresh_outline(state);
}

pub(super) fn fill_segment_masks(state: &App) {
    let missing: Vec<usize> = {
        let open = state.open.borrow();
        let Some(photo) = open.as_ref() else { return };
        photo
            .document
            .masks()
            .iter()
            .enumerate()
            .filter(|(_, mask)| mask.wants_pixels() && mask.map.0.is_none())
            .map(|(index, _)| index)
            .collect()
    };

    for index in missing {
        rebuild_mask_map(state, index);
    }

    drop_empty_masks(state);
}

pub(super) fn drop_empty_masks(state: &App) {
    let doomed: Vec<(usize, String)> = {
        let open = state.open.borrow();
        let Some(photo) = open.as_ref() else { return };
        photo
            .document
            .masks()
            .iter()
            .enumerate()
            .filter(|(_, mask)| {
                matches!(mask.shape, Shape::Segment { .. })
                    && !mask.is_pending()
                    && mask.points.is_empty()
                    && mask.strokes.is_empty()
            })
            .filter(|(_, mask)| {
                mask.map.0.as_ref().is_none_or(|alpha| {

                    let covered: f64 = alpha.values().map(|v| v as f64).sum();
                    covered / alpha.len().max(1) as f64 <= 0.001
                })
            })
            .map(|(index, mask)| (index, mask_name(mask)))
            .collect()
    };
    if doomed.is_empty() {
        return;
    }

    {
        let mut open = state.open.borrow_mut();
        let Some(photo) = open.as_mut() else { return };
        let mut masks = photo.document.masks();

        for (index, _) in doomed.iter().rev() {
            if *index < masks.len() {
                masks.remove(*index);
            }
        }
        photo.document.set_masks(masks);
        photo.view = None;
    }

    select_mask(state, None);
    refresh_masks(state);
    request_render(state);

    let names: Vec<String> = doomed.iter().map(|(_, name)| name.to_lowercase()).collect();
    state.toast(&format!(
        "Could not find {} here. Add a Click mask and click the thing itself.",
        names
            .iter()
            .map(|name| format!("{} {name}", if "aeiou".contains(&name[..1]) { "an" } else { "a" }))
            .collect::<Vec<_>>()
            .join(" or ")
    ));
}

pub(super) fn mask_raster_size(state: &App) -> (usize, usize) {
    let (width, height) = displayed_size(state).unwrap_or((3, 2));
    render::raster_size(width, height, render::MASK_RASTER)
}

pub(super) fn selected_shape(state: &App, index: usize) -> Option<Shape> {
    let open = state.open.borrow();
    open.as_ref()?.document.masks().get(index).map(|mask| mask.shape.clone())
}

pub(super) fn mask_frame(state: &App) -> Option<Arc<image::RgbImage>> {
    let made = {
        let open = state.open.borrow();
        let photo = open.as_ref()?;
        if let Some(found) = &photo.segmentation {
            return Some(Arc::new(found.photo().clone()));
        }
        if let Some(frame) = &photo.mask_frame {
            return Some(frame.clone());
        }

        let mut geometry = Document::new(photo.document.source.path.clone());
        geometry.set_perspective(photo.document.perspective());
        if let Some((rect, angle)) = photo.document.crop() {
            geometry.set_crop(rect, angle);
        }
        geometry.set_rotation(photo.document.rotation());
        geometry.set_mirrored(photo.document.mirrored());
        let working = render::to_working_space(&geometry, &*photo.proxy, &photo.inputs);
        Arc::new(render::apply_stack(&geometry, &working, 1.0))
    };

    if let Some(photo) = state.open.borrow_mut().as_mut() {
        photo.mask_frame = Some(made.clone());
    }
    Some(made)
}

pub(super) fn rebuild_mask_map(state: &App, index: usize) {
    let (width, height) = mask_raster_size(state);

    let wants_frame = selected_shape(state, index)
        .is_some_and(|shape| matches!(shape, Shape::ColourRange { .. } | Shape::LuminanceRange { .. }));
    let frame = wants_frame.then(|| mask_frame(state)).flatten();

    let mut open = state.open.borrow_mut();
    let Some(photo) = open.as_mut() else { return };

    let segmentation = photo.segmentation.clone();
    let embedding = photo.embedding.clone();
    let Some(mask) = photo.document.mask_mut(index) else { return };
    render::resolve_mask(
        mask,
        segmentation.as_deref(),
        embedding.as_deref(),
        frame.as_deref(),
        width,
        height,
    );

    mask.matte = false;
    mask.fine = false;
    photo.view = None;

    drop(open);

    refresh_outline(state);
}

pub(super) fn add_segment_mask(state: &App, classes: Vec<u16>, name: &str, inverted: bool) {
    if !segment::is_installed() {
        state.toast("No segmentation model installed — see Preferences");
        return;
    }

    let added = {
        let mut open = state.open.borrow_mut();
        let Some(photo) = open.as_mut() else { return };
        let mut masks = photo.document.masks();

        let existing = masks.iter().position(|mask| {
            mask.shape == Shape::Segment { classes: classes.clone() }
                && mask.inverted == inverted
                && mask.is_idle()
        });
        if let Some(index) = existing {
            drop(open);
            select_mask(state, Some(index));
            return;
        }

        let mut mask = Mask::new(Shape::Segment { classes: classes.clone() });

        mask.set_matte(classes.iter().all(|class| segment::MATTEABLE.contains(class)));

        mask.inverted = inverted;

        if name != segment::name_for(&classes) {
            mask.name = Some(name.to_string());
        }
        masks.push(mask);
        let index = masks.len() - 1;
        photo.document.set_masks(masks);
        photo.view = None;
        index
    };

    fill_segment_masks(state);
    ensure_segmentation(state);

    refresh_masks(state);
    select_mask(state, Some(added));
    schedule_history_push(state);
}

pub(super) fn taking_away(state: &App, modifiers: gtk::gdk::ModifierType) -> bool {
    modifiers.contains(gtk::gdk::ModifierType::SHIFT_MASK) != state.masks.toolbar.subtract.get()
}

pub(super) fn paint_segment(state: &App, from: [f32; 2], to: [f32; 2], stroke: &Stroke) {
    let Some(index) = state.mask_overlay.selected_mask.get() else { return };
    {
        let mut open = state.open.borrow_mut();
        let Some(photo) = open.as_mut() else { return };
        let Some(mask) = photo.document.mask_mut(index) else { return };
        let Some(alpha) = mask.map.0.as_mut() else { return };

        stroke.draw_segment(Arc::make_mut(alpha), from, to);
    }

    if let (Some(surface), Some(mask)) =
        (state.mask_overlay.wash.borrow_mut().as_mut(), selected_mask(state))
    {
        if let Some(alpha) = mask.map.0.as_ref() {
            let long = alpha.width.max(alpha.height) as f32;
            let radius = (stroke.radius * long).max(0.5) + 2.0;
            let x = |value: f32| value * alpha.width as f32;
            let y = |value: f32| value * alpha.height as f32;
            let bounds = (
                (x(from[0].min(to[0])) - radius).max(0.0) as usize,
                (y(from[1].min(to[1])) - radius).max(0.0) as usize,
                (x(from[0].max(to[0])) + radius).max(0.0) as usize + 1,
                (y(from[1].max(to[1])) + radius).max(0.0) as usize + 1,
            );
            paint_wash(surface, alpha, mask.inverted, bounds);
        }
    }

    state.mask_overlay.area.queue_draw();
}

pub(super) fn point_at(state: &App, u: f32, v: f32, subtract: bool) {
    let Some(index) = state.mask_overlay.selected_mask.get() else { return };

    let named = {
        let mut open = state.open.borrow_mut();
        let Some(photo) = open.as_mut() else { return };

        let named = (!sam::is_installed())
            .then(|| {
                photo
                    .segmentation
                    .as_ref()
                    .map(|segmentation| segmentation.class_at(u, v))
                    .and_then(segment::label)
            })
            .flatten();

        let Some(mask) = photo.document.mask_mut(index) else { return };
        mask.points.push(RegionPoint { at: [u, v], subtract, enabled: true });
        named
    };

    ensure_segmentation(state);

    ensure_embedding(state);
    rebuild_mask_map(state, index);

    state.toast(&match (subtract, named) {
        (false, None) => "Added what is there".to_string(),
        (true, None) => "Removed what is there".to_string(),
        (false, Some(name)) => format!("Added the {name}"),
        (true, Some(name)) => format!("Removed the {name}"),
    });
    refresh_masks(state);
    request_render(state);
    show_coverage(state);
    schedule_history_push(state);
}

pub(super) fn add_mask(state: &App, kind: MaskKind) {
    busy_sync(state, "Adding the mask…", move |state| add_mask_now(state, kind))
}

pub(super) fn add_mask_now(state: &App, kind: MaskKind) {
    let added = {
        let mut open = state.open.borrow_mut();
        let Some(photo) = open.as_mut() else { return };
        let mut masks = photo.document.masks();
        masks.push(Mask::new(match kind {
            MaskKind::Linear => Shape::linear(),
            MaskKind::Radial => Shape::radial(),
            MaskKind::Brush | MaskKind::Click => Shape::Painted,
            MaskKind::ColourRange => Shape::colour_range(),
            MaskKind::LuminanceRange => Shape::luminance_range(),
        }));
        let index = masks.len() - 1;
        photo.document.set_masks(masks);
        photo.view = None;
        index
    };

    fill_segment_masks(state);
    select_mask(state, Some(added));

    if kind == MaskKind::Brush {
        pick_tool(state, Tool::Brush);
    }
    if kind == MaskKind::Click {

        ensure_segmentation(state);
        ensure_embedding(state);
        state.toast("Click the photograph to add what is under the cursor");
    }

    schedule_history_push(state);
}

pub(super) fn add_mask_reordering(state: &App, row: &adw::ActionRow, index: usize) {
    let source = gtk::DragSource::new();
    source.set_actions(gtk::gdk::DragAction::MOVE);
    source.connect_prepare(move |_, _, _| {
        Some(gtk::gdk::ContentProvider::for_value(&(index as u32).to_value()))
    });

    source.connect_drag_begin(glib::clone!(
        #[weak] row,
        move |source, _| {
            let paintable = gtk::WidgetPaintable::new(Some(&row));
            source.set_icon(Some(&paintable), 20, 20);
        }
    ));
    row.add_controller(source);

    let target = gtk::DropTarget::new(glib::Type::U32, gtk::gdk::DragAction::MOVE);
    target.connect_drop(glib::clone!(
        #[strong] state,
        move |_, value, _, _| {
            let Ok(from) = value.get::<u32>() else { return false };
            move_mask(&state, from as usize, index);
            true
        }
    ));
    row.add_controller(target);
}
