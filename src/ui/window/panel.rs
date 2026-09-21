use super::*;

pub(super) fn build_adjustment_panel(state: &App) -> gtk::Box {
    let pages = state.panel.stack.clone();
    pages.set_vexpand(true);
    let all = state.sliders.each();

    {
        let scratch = Sliders::new();
        scratch.write(Basic::default());

        for ((_, real, _), (_, rest, _)) in all.iter().zip(scratch.each().iter()).skip(2) {
            set_neutral(real, rest.value());
        }

        state.applying.set(true);
        state.sliders.write(Basic::default());
        state.mask_overlay.sliders_hold.set(None);
        state.applying.set(false);
    }

    state.panel.scoped.borrow_mut().clear();
    let global_only = |widget: &gtk::Widget| {
        state.panel.scoped.borrow_mut().push((widget.clone(), false));
    };
    let mask_only = |widget: &gtk::Widget| {
        state.panel.scoped.borrow_mut().push((widget.clone(), true));
    };

    pages.add_named(&wrap_page(&build_light(state, &all, &global_only)), Some("light"));

    pages.add_named(&wrap_page(&build_colour(state, &all, &global_only, &mask_only)), Some("colour"));
    pages.add_named(&wrap_page(&build_effects(state, &all, &global_only)), Some("effects"));
    pages.add_named(&wrap_page(&build_grade(state, &global_only)), Some("grade"));

    pages.add_named(&wrap_page(&build_detail(state, &all, &global_only)), Some("detail"));

    let masks = page_column();
    masks.add_css_class("quiet");
    masks.append(&section_header("Masks"));
    masks.append(&build_masks(state));
    pages.add_named(&wrap_page(&masks), Some("masks"));

    let crop = page_column();
    crop.add_css_class("quiet");
    crop.append(&build_crop_controls(state));

    let retouch = page_column();
    retouch.add_css_class("quiet");

    retouch.append(&section_header("Spots"));
    retouch.append(&build_retouch(state));

    retouch.append(&build_face(state));
    pages.add_named(&wrap_page(&retouch), Some("retouch"));

    pages.add_named(&wrap_page(&crop), Some("crop"));

    let presets = state.panel.presets_page.clone();

    presets.add_css_class("adjustment");
    presets.add_css_class("presets-page");
    pages.add_named(&presets, Some("presets"));

    pages.set_visible_child_name("light");

    let tabs = build_rail(state);

    let panel = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let histogram = build_histogram(state);
    histogram.set_margin_top(14);
    histogram.set_margin_start(14);
    histogram.set_margin_end(14);
    panel.append(&histogram);
    panel.append(&build_mask_banner(state));

    let body = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    body.append(&tabs);
    body.append(&pages);
    panel.append(&body);

    panel.set_hexpand(false);
    panel
}

thread_local! {

    pub(super) static RAIL_DOTS: RefCell<Vec<(&'static str, gtk::Box)>> = const { RefCell::new(Vec::new()) };
}

pub(super) const MASK_TABS: [&str; 4] = ["light", "colour", "effects", "detail"];

pub(super) fn refresh_rail_dots(state: &App) {
    fn scales(widget: &gtk::Widget, found: &mut Vec<gtk::Scale>) {
        if let Some(scale) = widget.downcast_ref::<gtk::Scale>() {
            found.push(scale.clone());
        }
        let mut child = widget.first_child();
        while let Some(this) = child {
            scales(&this, found);
            child = this.next_sibling();
        }
    }

    RAIL_DOTS.with(|dots| {
        for (name, dot) in dots.borrow().iter() {
            let Some(page) = state.panel.stack.child_by_name(name) else { continue };
            let mut found = Vec::new();
            scales(&page, &mut found);
            let moved = found.iter().any(|scale| match neutral_of(scale) {
                Some(rest) => (scale.value() - rest).abs() > 1e-4,

                None => false,
            });
            dot.set_visible(moved);
        }
    });
}

pub(super) const PANEL_TABS: [(&str, &str, &str, Option<&str>); 9] = [
    ("presets", "Presets", "Presets", Some("starred-symbolic")),
    ("light", "Light", "Light", Some("display-brightness-symbolic")),
    ("colour", "Colour", "Colour", Some("color-select-symbolic")),
    ("effects", "Effects", "Presence, vignette and grain", None),
    ("grade", "Grade", "Colour grading", None),
    ("detail", "Detail", "Detail — judge these at 1:1", Some("edit-find-symbolic")),
    ("masks", "Masks", "Masks", None),
    ("retouch", "Retouch", "Heal, clone and the face", None),
    ("crop", "Crop", "Crop and straighten", Some("edit-cut-symbolic")),
];

pub(super) fn page_column() -> gtk::Box {
    let column = gtk::Box::new(gtk::Orientation::Vertical, 2);
    column.set_margin_top(6);
    column.set_margin_bottom(18);
    column.set_margin_start(14);
    column.set_margin_end(14);
    column
}

pub(super) fn wrap_page(column: &gtk::Box) -> gtk::ScrolledWindow {
    let scroller = gtk::ScrolledWindow::new();
    scroller.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    scroller.add_css_class("adjustment");
    scroller.set_child(Some(column));
    scroller.set_vexpand(true);
    scroller
}

pub(super) fn show_panel_tab(state: &App, name: &str) {
    let was_cropping = is_cropping(state);
    if name == "presets" {
        fill_presets_page(state);
    }

    match name {
        "masks" if segment::is_installed() => ensure_segmentation(state),
        "retouch" => ensure_faces(state),
        _ => {}
    }
    state.panel.stack.set_visible_child_name(name);

    let cropping = name == "crop";
    if was_cropping != cropping {
        toggle_crop(state, cropping);
    }

    let retouching = name == "retouch";
    if state.retouch.on.get() != retouching {
        toggle_retouch(state, retouching);
    }

    state.applying.set(true);
    for (tab, button) in state.panel.tabs.borrow().iter() {
        button.set_active(*tab == name);
    }
    state.applying.set(false);
}

pub(super) fn frame_aspect(state: &App) -> f32 {
    let (width, height) = frame_pixels(state);
    width / height
}

pub(super) fn frame_pixels(state: &App) -> (f32, f32) {
    state.open.borrow().as_ref().map_or((3.0, 2.0), oriented_pixels)
}

pub(super) fn oriented_pixels(photo: &OpenPhoto) -> (f32, f32) {
    let (width, height) = (photo.working.width as f32, photo.working.height as f32);
    if matches!(photo.document.rotation() as i32, 90 | 270) {
        (height, width)
    } else {
        (width, height)
    }
}

pub(super) fn hold_aspect(rect: [f32; 4], handle: usize, target: f32) -> [f32; 4] {
    const MIN: f32 = 0.05;
    let [x, y, w, h] = rect;
    let target = target.max(1e-3);
    let (left_side, top_side) = (handle == 0 || handle == 2, handle == 0 || handle == 1);
    let anchor_x = if left_side { x + w } else { x };
    let anchor_y = if top_side { y + h } else { y };

    let (mut width, mut height) = (w.max(MIN), h.max(MIN));
    if width / height > target {
        height = width / target;
    } else {
        width = height * target;
    }

    let room_x = if left_side { anchor_x } else { 1.0 - anchor_x };
    let room_y = if top_side { anchor_y } else { 1.0 - anchor_y };
    let scale = (room_x / width).min(room_y / height).min(1.0).max(0.0);
    width *= scale;
    height *= scale;

    [
        if left_side { anchor_x - width } else { anchor_x },
        if top_side { anchor_y - height } else { anchor_y },
        width,
        height,
    ]
}

pub(super) fn apply_aspect(state: &App, ratio: f32) {
    let [x, y, width, height] = state.crop.rect.get();
    let (centre_x, centre_y) = (x + width / 2.0, y + height / 2.0);

    let target = ratio / frame_aspect(state);
    let (mut new_width, mut new_height) = if width / height > target {
        (height * target, height)
    } else {
        (width, width / target)
    };

    new_width = new_width.min(1.0);
    new_height = new_height.min(1.0);

    state.crop.rect.set([
        (centre_x - new_width / 2.0).clamp(0.0, 1.0 - new_width),
        (centre_y - new_height / 2.0).clamp(0.0, 1.0 - new_height),
        new_width,
        new_height,
    ]);
    state.crop.area.queue_draw();
}

pub(super) fn panel_floor(panel: &gtk::Box) -> i32 {
    let (minimum, _, _, _) = panel.measure(gtk::Orientation::Horizontal, -1);
    minimum.max(PANEL_WIDTH)
}

pub(super) fn narrow_dropdown(picker: &gtk::DropDown) {
    let factory = gtk::SignalListItemFactory::new();
    factory.connect_setup(|_, item| {
        let label = gtk::Label::new(None);
        label.set_xalign(0.0);
        label.set_ellipsize(gtk::pango::EllipsizeMode::End);
        label.set_max_width_chars(12);
        item.downcast_ref::<gtk::ListItem>()
            .expect("a list item")
            .set_child(Some(&label));
    });
    factory.connect_bind(|_, item| {
        let item = item.downcast_ref::<gtk::ListItem>().expect("a list item");
        let Some(text) = item.item().and_downcast::<gtk::StringObject>() else { return };
        let Some(label) = item.child().and_downcast::<gtk::Label>() else { return };
        label.set_text(&text.string());
        label.set_tooltip_text(Some(&text.string()));
    });
    picker.set_factory(Some(&factory));
}

#[derive(Clone)]
pub(super) struct State {

    pub(super) tab_strip: gtk::Box,
    pub(super) stack: gtk::Stack,
    pub(super) tabs: Rc<RefCell<Vec<(&'static str, gtk::ToggleButton)>>>,

    pub(super) scoped: Rc<RefCell<Vec<(gtk::Widget, bool)>>>,

    pub(super) presets_page: gtk::Box,

    pub(super) history_list: gtk::ListBox,
}

impl State {
    pub(super) fn new() -> Self {
        Self {
            tab_strip: gtk::Box::new(gtk::Orientation::Horizontal, 0),
            stack: gtk::Stack::new(),
            tabs: Rc::new(RefCell::new(Vec::new())),
            scoped: Rc::new(RefCell::new(Vec::new())),
            presets_page: gtk::Box::new(gtk::Orientation::Vertical, 0),
            history_list: gtk::ListBox::new(),
        }
    }
}
