use super::*;

#[derive(Debug, Clone, PartialEq)]
pub(super) struct EditState {
    pub(super) basic: Basic,
    pub(super) white_balance: Option<WhiteBalance>,

    pub(super) crop: Option<([f32; 4], f32)>,
    pub(super) rotation: f32,

    pub(super) mirrored: bool,
    pub(super) film_simulation: Option<String>,

    pub(super) colour_profile: Option<String>,

    pub(super) curves: [Curve; 4],
    pub(super) mixer: Mixer,

    pub(super) point_colours: PointColours,

    pub(super) grading: Grading,

    pub(super) retouch: Retouch,

    pub(super) perspective: Perspective,

    pub(super) working_space: ColourSpace,

    pub(super) beautify: Beautify,
    pub(super) masks: Vec<Mask>,

    pub(super) ai_denoise: f32,

    pub(super) ai_sharpen: f32,
}

impl EditState {

    pub(super) fn restore(&self, document: &mut Document) {
        let (rect, angle) = self.crop.unwrap_or(([0.0, 0.0, 1.0, 1.0], 0.0));
        document.working_space = self.working_space;
        document.set_perspective(self.perspective);
        document.set_crop(rect, angle);
        document.set_rotation(self.rotation);
        document.set_mirrored(self.mirrored);
        document.film_simulation = self.film_simulation.clone();
        document.colour_profile = self.colour_profile.clone();
        document.set_basic(self.basic);
        document.white_balance = self.white_balance;
        document.set_curves(self.curves.clone());
        document.set_mixer(self.mixer);
        document.set_point_colours(self.point_colours.clone());
        document.set_grading(self.grading);
        document.set_retouch(self.retouch.clone());
        document.set_beautify(self.beautify);
        document.set_masks(self.masks.clone());
        document.ai_denoise = self.ai_denoise;
        document.ai_sharpen = self.ai_sharpen;
    }

    pub(super) fn untouched() -> Self {
        Self::of(&Document::new(String::new()))
    }

    pub(super) fn of(document: &Document) -> Self {
        Self {
            basic: document.basic(),
            white_balance: document.white_balance,
            crop: document.crop(),
            rotation: document.rotation(),
            mirrored: document.mirrored(),
            film_simulation: document.film_simulation.clone(),
            colour_profile: document.colour_profile.clone(),
            curves: document.curves(),
            mixer: document.mixer(),
            point_colours: document.point_colours(),
            grading: document.grading(),
            retouch: document.retouch(),
            perspective: document.perspective(),
            working_space: document.working_space,
            beautify: document.beautify(),
            masks: document.masks(),
            ai_denoise: document.ai_denoise,
            ai_sharpen: document.ai_sharpen,
        }
    }
}

pub(super) type Geometry = (f32, Option<([f32; 4], f32)>, [f32; 2]);

pub(super) fn geometry_of_document(document: &Document) -> Geometry {

    let basic = document.basic();
    (document.rotation(), document.crop(), [basic.optics.lens_distortion, basic.optics.lens_vignetting])
}

#[derive(Clone)]
pub(super) struct ViewTile {

    pub(super) rect: [f32; 4],

    pub(super) key: ColourKey,

    pub(super) geometry: Geometry,

    pub(super) edge: u32,
    pub(super) image: Arc<LinearImage>,
}

pub(super) struct History {
    pub(super) states: Vec<EditState>,

    pub(super) position: usize,

    pub(super) names: Vec<Option<String>>,
}

pub(super) const HISTORY_KEPT: usize = 100;

impl History {
    pub(super) fn new(initial: EditState) -> Self {
        Self { states: vec![initial], position: 0, names: vec![None] }
    }

    pub(super) fn resumed(saved: Option<(Vec<EditState>, usize)>, now: EditState) -> Self {
        let mut history = match saved {
            Some((states, position)) if !states.is_empty() => {
                let position = position.min(states.len() - 1);
                Self { names: vec![None; states.len()], states, position }
            }
            _ if now != EditState::untouched() => Self::new(EditState::untouched()),
            _ => Self::new(now.clone()),
        };
        history.push(now);
        history
    }

    pub(super) fn push(&mut self, state: EditState) {
        if self.states[self.position] == state {
            return;
        }

        self.states.truncate(self.position + 1);
        self.names.truncate(self.position + 1);
        self.states.push(state);
        self.names.push(None);
        if self.states.len() > HISTORY_KEPT {
            self.states.remove(0);
            self.names.remove(0);
        }
        self.position = self.states.len() - 1;
    }

    pub(super) fn push_named(&mut self, state: EditState, name: &str) {
        if self.states[self.position] != state {
            self.push(state);
            self.names[self.position] = Some(name.to_string());
        }
    }

    pub(super) fn undo(&mut self) -> Option<EditState> {
        if self.position == 0 {
            return None;
        }
        self.position -= 1;
        Some(self.states[self.position].clone())
    }

    pub(super) fn redo(&mut self) -> Option<EditState> {
        if self.position + 1 >= self.states.len() {
            return None;
        }
        self.position += 1;
        Some(self.states[self.position].clone())
    }

    pub(super) fn go_to(&mut self, position: usize) -> Option<EditState> {
        if position >= self.states.len() || position == self.position {
            return None;
        }
        self.position = position;
        Some(self.states[self.position].clone())
    }

    pub(super) fn steps(&self) -> Vec<String> {

        let first = if self.states[0] == EditState::untouched() { "Original" } else { "Opened" };
        let mut steps = vec![first.to_string()];
        for (pair, name) in self.states.windows(2).zip(&self.names[1..]) {
            steps.push(name.clone().unwrap_or_else(|| pair[1].difference_from(&pair[0])));
        }
        steps
    }
}

impl EditState {

    pub(super) fn difference_from(&self, previous: &EditState) -> String {
        let sliders: [(&str, fn(&Basic) -> f32); 37] = [
            ("Exposure", |b| b.tone.exposure),
            ("Contrast", |b| b.tone.contrast),
            ("Highlights", |b| b.tone.highlights),
            ("Shadows", |b| b.tone.shadows),
            ("Whites", |b| b.tone.whites),
            ("Blacks", |b| b.tone.blacks),
            ("Vibrance", |b| b.presence.vibrance),
            ("Saturation", |b| b.presence.saturation),
            ("HDR", |b| b.presence.hdr),
            ("Clarity", |b| b.presence.clarity),
            ("Texture", |b| b.presence.texture),
            ("Sharpening", |b| b.detail.sharpen),
            ("Sharpening radius", |b| b.detail.sharpen_radius),
            ("Sharpening mask", |b| b.detail.sharpen_masking),
            ("Noise reduction", |b| b.detail.denoise_luma),
            ("Noise detail", |b| b.detail.denoise_detail),
            ("Noise contrast", |b| b.detail.denoise_contrast),
            ("Colour noise", |b| b.detail.denoise_colour),
            ("Defringe", |b| b.detail.defringe),
            ("Moiré", |b| b.detail.moire),
            ("Dehaze", |b| b.effects.dehaze),
            ("Vignette", |b| b.effects.vignette),
            ("Vignette midpoint", |b| b.effects.vignette_midpoint),
            ("Vignette roundness", |b| b.effects.vignette_roundness),
            ("Vignette feather", |b| b.effects.vignette_feather),
            ("Grain", |b| b.effects.grain),
            ("Grain size", |b| b.effects.grain_size),
            ("Grain roughness", |b| b.effects.grain_roughness),
            ("Shadows tint", |b| b.calibration.shadow_tint),
            ("Red hue", |b| b.calibration.red_hue),
            ("Red saturation", |b| b.calibration.red_saturation),
            ("Green hue", |b| b.calibration.green_hue),
            ("Green saturation", |b| b.calibration.green_saturation),
            ("Blue hue", |b| b.calibration.blue_hue),
            ("Blue saturation", |b| b.calibration.blue_saturation),
            ("Lens distortion", |b| b.optics.lens_distortion),
            ("Lens vignetting", |b| b.optics.lens_vignetting),
        ];

        let mut changed: Vec<String> = sliders
            .iter()
            .filter(|(_, read)| read(&self.basic) != read(&previous.basic))
            .map(|(name, _)| name.to_string())
            .collect();

        if self.white_balance != previous.white_balance {
            changed.push("White balance".to_string());
        }
        if self.crop != previous.crop {
            changed.push("Crop".to_string());
        }
        if self.rotation != previous.rotation {
            changed.push("Rotation".to_string());
        }
        if self.mirrored != previous.mirrored {
            changed.push("Flip".to_string());
        }
        if self.curves != previous.curves {
            changed.push("Tone curve".to_string());
        }
        if self.mixer != previous.mixer {
            changed.push("Colour mixer".to_string());
        }
        if self.point_colours != previous.point_colours {
            changed.push("Point colour".to_string());
        }
        if self.grading != previous.grading {
            changed.push("Colour grading".to_string());
        }
        if self.retouch != previous.retouch {
            let (now, was) = (self.retouch.spots.len(), previous.retouch.spots.len());
            changed.push(match now.cmp(&was) {
                std::cmp::Ordering::Greater => "Retouched".to_string(),
                std::cmp::Ordering::Less => "Removed a retouch".to_string(),
                std::cmp::Ordering::Equal => "Moved a retouch".to_string(),
            });
        }
        if self.film_simulation != previous.film_simulation {
            changed.push("Film simulation".to_string());
        }
        if self.colour_profile != previous.colour_profile {
            changed.push("Camera profile".to_string());
        }
        if self.working_space != previous.working_space {
            changed.push("Colour space".to_string());
        }
        if self.perspective != previous.perspective {
            changed.push("Perspective".to_string());
        }
        if self.beautify != previous.beautify {
            changed.push("Face".to_string());
        }
        if self.ai_denoise != previous.ai_denoise {
            changed.push("AI denoise".to_string());
        }
        if self.ai_sharpen != previous.ai_sharpen {
            changed.push("AI sharpen".to_string());
        }
        if let Some(mask) = masks_changed(&self.masks, &previous.masks) {
            changed.push(mask);
        }

        match changed.len() {
            0 => "No change".to_string(),
            1 => changed.remove(0),
            2 => changed.join(" and "),
            many => format!("{} and {} more", changed.remove(0), many - 1),
        }
    }
}

pub(super) fn at_rest_of(now: Basic, balance: WhiteBalance, as_shot: WhiteBalance) -> [bool; SLIDER_COUNT] {
    let rest = Basic::default();
    let same = |a: f32, b: f32| (a - b).abs() < 1e-4;
    [

        same(balance.temperature, as_shot.temperature),
        same(balance.tint, as_shot.tint),
        same(now.tone.exposure, rest.tone.exposure),
        same(now.tone.contrast, rest.tone.contrast),
        same(now.tone.highlights, rest.tone.highlights),
        same(now.tone.shadows, rest.tone.shadows),
        same(now.tone.whites, rest.tone.whites),
        same(now.tone.blacks, rest.tone.blacks),
        same(now.presence.hdr, rest.presence.hdr),
        same(now.presence.vibrance, rest.presence.vibrance),
        same(now.presence.saturation, rest.presence.saturation),
        same(now.presence.clarity, rest.presence.clarity),
        same(now.presence.texture, rest.presence.texture),

        same(now.detail.sharpen, rest.detail.sharpen),
        same(now.detail.sharpen_radius, rest.detail.sharpen_radius),
        same(now.detail.sharpen_masking, rest.detail.sharpen_masking),
        same(now.detail.denoise_luma, rest.detail.denoise_luma),
        same(now.detail.denoise_detail, rest.detail.denoise_detail),
        same(now.detail.denoise_contrast, rest.detail.denoise_contrast),
        same(now.detail.denoise_colour, rest.detail.denoise_colour),
        same(now.detail.defringe, rest.detail.defringe),
        same(now.detail.moire, rest.detail.moire),
        same(now.effects.dehaze, rest.effects.dehaze),
        same(now.effects.vignette, rest.effects.vignette),
        same(now.effects.vignette_midpoint, rest.effects.vignette_midpoint),
        same(now.effects.vignette_roundness, rest.effects.vignette_roundness),
        same(now.effects.vignette_feather, rest.effects.vignette_feather),
        same(now.effects.grain, rest.effects.grain),
        same(now.effects.grain_size, rest.effects.grain_size),
        same(now.effects.grain_roughness, rest.effects.grain_roughness),
        same(now.calibration.shadow_tint, rest.calibration.shadow_tint),
        same(now.calibration.red_hue, rest.calibration.red_hue),
        same(now.calibration.red_saturation, rest.calibration.red_saturation),
        same(now.calibration.green_hue, rest.calibration.green_hue),
        same(now.calibration.green_saturation, rest.calibration.green_saturation),
        same(now.calibration.blue_hue, rest.calibration.blue_hue),
        same(now.calibration.blue_saturation, rest.calibration.blue_saturation),
        same(now.optics.lens_distortion, rest.optics.lens_distortion),
        same(now.optics.lens_vignetting, rest.optics.lens_vignetting),
    ]
}

pub(super) fn refresh_slider_marks(state: &App) {
    let as_shot = state
        .open
        .borrow()
        .as_ref()
        .map(|photo| photo.as_shot)
        .unwrap_or(WhiteBalance { temperature: 5500.0, tint: 0.0 });

    let mark = |scale: &gtk::Scale, rest: bool| {
        let row = scale.parent();
        for widget in std::iter::once(scale.clone().upcast::<gtk::Widget>()).chain(row) {
            if rest {
                widget.remove_css_class("touched");
            } else {
                widget.add_css_class("touched");
            }
        }
    };

    state.sliders.white_balance_at_rest(as_shot);

    let panel = state.sliders.each();
    for ((_, scale, _), rest) in panel.iter().zip(state.sliders.at_rest(as_shot)) {
        mark(scale, rest);
    }

    refresh_rail_dots(state);

    REGISTERED.with(|registered| {
        for scale in registered.borrow().iter() {
            if panel.iter().any(|(_, theirs, _)| *theirs == scale) {
                continue;
            }
            mark(scale, (scale.value() - neutral_of(scale).unwrap_or(0.0)).abs() < 1e-4);
        }
    });
}

pub(super) fn build_history(state: &App) -> gtk::Popover {
    let column = gtk::Box::new(gtk::Orientation::Vertical, 8);
    column.set_margin_top(6);
    column.set_margin_bottom(6);
    column.set_margin_start(6);
    column.set_margin_end(6);

    let heading = gtk::Label::new(Some("HISTORY"));
    heading.set_xalign(0.0);
    heading.add_css_class("section-header");
    column.append(&heading);

    state.panel.history_list.set_selection_mode(gtk::SelectionMode::None);
    state.panel.history_list.add_css_class("boxed-list");

    let scroller = gtk::ScrolledWindow::new();
    scroller.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    scroller.set_propagate_natural_height(true);
    scroller.set_max_content_height(420);
    scroller.set_width_request(260);
    scroller.set_child(Some(&state.panel.history_list));
    column.append(&scroller);

    let note = gtk::Label::new(Some("Newest first. Click a step to go back to it."));
    note.set_xalign(0.0);
    note.set_wrap(true);
    note.add_css_class("profile-note");
    column.append(&note);

    let heading = gtk::Label::new(Some("SNAPSHOTS"));
    heading.set_xalign(0.0);
    heading.add_css_class("section-header");
    column.append(&heading);
    let snapshots = gtk::ListBox::new();
    snapshots.set_selection_mode(gtk::SelectionMode::None);
    snapshots.add_css_class("boxed-list");
    column.append(&snapshots);

    let popover = gtk::Popover::new();
    popover.set_child(Some(&column));

    popover.connect_show(glib::clone!(
        #[strong] state,
        move |_| {
            refresh_history(&state);
            refresh_snapshots(&state, &snapshots);
        }
    ));
    popover
}

pub(super) fn refresh_snapshots(state: &App, list: &gtk::ListBox) {
    while let Some(row) = list.first_child() {
        list.remove(&row);
    }
    let id = match state.open.borrow().as_ref().map(|photo| &photo.source) {
        Some(Source::Photo { id, .. }) => *id,
        _ => return,
    };
    let saved = state.catalog.snapshots(id).unwrap_or_default();

    let save = adw::EntryRow::new();
    save.set_title("Save snapshot…");
    let button = gtk::Button::from_icon_name("list-add-symbolic");
    button.set_tooltip_text(Some("Save snapshot"));
    button.set_valign(gtk::Align::Center);
    button.add_css_class("flat");
    save.add_suffix(&button);
    let fallback = (saved.len() + 1..)
        .map(|n| format!("Snapshot {n}"))
        .find(|name| saved.iter().all(|(taken, _)| taken != name));
    let commit = glib::clone!(
        #[strong] state,
        #[weak] list,
        #[weak] save,
        move || {
            let text = save.text();
            let name = if text.trim().is_empty() { fallback.clone().unwrap_or_default() } else { text.to_string() };
            let Some(document) = state.open.borrow().as_ref().map(|photo| photo.document.clone()) else { return };
            match state.catalog.save_snapshot(id, &name, &document) {
                Ok(()) => refresh_snapshots(&state, &list),
                Err(err) => state.toast(&err),
            }
        }
    );
    save.connect_entry_activated(glib::clone!(#[strong] commit, move |_| commit()));
    button.connect_clicked(move |_| commit());
    list.append(&save);

    for (name, created) in saved {
        let row = adw::ActionRow::new();
        set_row_title(&row, &name);
        if let Some(when) = glib::DateTime::from_unix_local(created).and_then(|t| t.format("%e %b %Y, %H:%M")).ok() {
            row.set_subtitle(when.trim());
        }
        row.set_activatable(true);
        row.connect_activated(glib::clone!(
            #[strong] state,
            #[strong] name,
            move |_| match state.catalog.load_snapshot(id, &name) {
                Ok(document) => restore_snapshot(&state, &name, &document),
                Err(err) => state.toast(&format!("Could not read the snapshot: {err}")),
            }
        ));

        let delete = gtk::Button::from_icon_name("user-trash-symbolic");
        delete.set_tooltip_text(Some("Delete snapshot"));
        delete.set_valign(gtk::Align::Center);
        delete.add_css_class("flat");
        delete.connect_clicked(glib::clone!(
            #[strong] state,
            #[weak] list,
            move |_| {
                let Ok(document) = state.catalog.load_snapshot(id, &name) else { return };
                if let Err(err) = state.catalog.delete_snapshot(id, &name) {
                    state.toast(&err);
                    return;
                }
                refresh_snapshots(&state, &list);
                let toast = adw::Toast::new(&format!("Deleted “{name}”"));
                toast.set_button_label(Some("Undo"));
                toast.connect_button_clicked(glib::clone!(
                    #[strong] state,
                    #[strong] name,
                    #[weak] list,
                    move |_| {
                        if let Err(err) = state.catalog.save_snapshot(id, &name, &document) {
                            state.toast(&err);
                        }
                        refresh_snapshots(&state, &list);
                    }
                ));
                state.toasts.add_toast(toast);
            }
        ));
        row.add_suffix(&delete);
        list.append(&row);
    }
}

pub(super) fn restore_snapshot(state: &App, name: &str, document: &Document) {
    let stepped = state.open.borrow_mut().as_mut().map(|photo| {
        photo.history.push(EditState::of(&photo.document));
        photo.history.push_named(EditState::of(document), name);
        (photo.history.states[photo.history.position].clone(), photo.as_shot)
    });
    if let Some((edit, as_shot)) = stepped {
        apply_history(state, edit, as_shot);
    }
}

pub(super) fn refresh_history(state: &App) {
    while let Some(row) = state.panel.history_list.first_child() {
        state.panel.history_list.remove(&row);
    }

    let (steps, position) = {
        let open = state.open.borrow();
        let Some(photo) = open.as_ref() else { return };
        (photo.history.steps(), photo.history.position)
    };

    for (index, step) in steps.iter().enumerate().rev() {
        let row = adw::ActionRow::new();
        row.set_title(step);
        row.set_activatable(true);
        row.connect_activated(glib::clone!(
            #[strong] state,
            move |_| jump_history(&state, index)
        ));
        if index == position {
            row.add_css_class("current-step");
        }

        if index > position {
            row.add_css_class("dim-label");
        }
        state.panel.history_list.append(&row);
    }
}

pub(super) fn masks_changed(now: &[Mask], before: &[Mask]) -> Option<String> {
    if now.len() > before.len() {
        return Some(format!("Added {}", mask_label(now, now.len() - 1)));
    }
    if now.len() < before.len() {
        return Some("Removed a mask".to_string());
    }

    let (index, mask) = now
        .iter()
        .zip(before)
        .position(|(now, before)| now != before)
        .map(|index| (index, &now[index]))?;

    let was = &before[index];
    let what = if mask.basic != was.basic {
        "Adjusted"
    } else if mask.inverted != was.inverted {
        "Inverted"

    } else if mask.visible != was.visible {
        if mask.visible { "Showed" } else { "Hid" }
    } else if mask.name != was.name {
        "Renamed"
    } else if mask.opacity != was.opacity {
        "Set the strength of"
    } else if mask.strokes.len() != was.strokes.len() {
        "Drew on"
    } else if mask.points.len() != was.points.len() {
        "Pointed at"
    } else {
        "Changed"
    };
    Some(format!("{what} {}", mask_label(now, index)))
}
