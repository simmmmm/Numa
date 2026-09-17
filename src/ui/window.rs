use adw::prelude::*;
use gtk::{gio, glib};
use numa::core::curve::Curve;
use numa::cull;
use numa::core::color::{self, WhiteBalance};
use numa::core::document::{Basic, Document, EditParts, Perspective};
use numa::core::image::{fitted_crop, LinearImage};
use numa::core::beautify::{Beautify, Portrait};
use numa::core::grading::{Grading, Range};
use numa::core::mask::{Alpha, Mask, Pixels, RegionPoint, Shape, Stroke};
use numa::core::mixer::{Mixer, BANDS};
use numa::core::point::{PointColour, PointColours};
use numa::core::retouch::{Retouch, Spot};
use numa::core::space::ColourSpace;
use numa::io::catalog::{Catalog, FileType, Filter, Flag, Library, Photo, Sort};
use numa::io::dcp;
use numa::io::export;
use numa::io::raw;
use numa::render;
use numa::render::bracket;
use numa::render::sam;
use numa::render::segment::{self, Segmentation};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::Arc;

use crate::ui::justified::{self, Justified};
use crate::ui::thumbnail;

mod ai_denoise;

const GRID_THUMB_EDGE: u32 = numa::io::thumbs::LIBRARY_EDGE;

const PROXY_EDGE: u32 = 2400;

const SLIDER_COUNT: usize = 39;

const FIT_ZOOM: f64 = 0.0;

const ZOOM_PER_NOTCH: f64 = 1.25;

const MAX_ZOOM: f64 = 8.0;

const FULL_RESOLUTION_ZOOM: f64 = 1.0;

fn needs_full_resolution(state: &App, zoom: f64) -> bool {
    let open = state.open.borrow();
    match open.as_ref() {
        Some(photo) => zoom > proxy_runs_out_at(photo),
        None => zoom >= FULL_RESOLUTION_ZOOM,
    }
}

const PANEL_WIDTH: i32 = 240;

const FILMSTRIP_STEP: f64 = 110.0;

const LAST_LIBRARY: &str = "last-library";

const WINDOW_STATE: &str = "window";
const GRID_FILTER: &str = "filter";

const GRID_SIZES: &str = "grid-sizes";

const OFFERED_MODELS: &str = "offered-models";

const OFFERED_MENU_ENTRY: &str = "offered-menu-entry";
const EXPORT_SETTINGS: &str = "export";

#[derive(serde::Serialize, serde::Deserialize)]
struct WindowState {
    width: i32,
    height: i32,
    maximised: bool,
}

const CSS: &str = include_str!("style.css");

fn load_css() {
    let provider = gtk::CssProvider::new();
    provider.load_from_string(CSS);

    if let Some(display) = gtk::gdk::Display::default() {
        gtk::style_context_add_provider_for_display(
            &display,
            &provider,
            gtk::STYLE_PROVIDER_PRIORITY_APPLICATION,
        );

        #[cfg(debug_assertions)]
        gtk::IconTheme::for_display(&display)
            .add_search_path(concat!(env!("CARGO_MANIFEST_DIR"), "/data/icons"));
    }
}

#[derive(Clone)]
struct App {

    catalog: Rc<Catalog>,
    library: Rc<RefCell<Option<Library>>>,
    libraries: Rc<RefCell<Vec<Library>>>,
    filter: Rc<RefCell<Filter>>,

    picker_places: Rc<RefCell<Vec<Place>>>,

    albums_menu: gio::Menu,

    cards: Rc<RefCell<HashMap<i64, (Photo, gtk::Label)>>>,

    open: Rc<RefCell<Option<OpenPhoto>>>,
    zoom: Rc<Cell<f64>>,

    wall: Justified,
    empty: gtk::Label,

    welcome: adw::StatusPage,
    stack: gtk::Stack,
    canvas: gtk::Picture,
    library_picker: gtk::DropDown,
    zoom_label: gtk::Label,

    profile_picker: gtk::DropDown,

    picking_band: Rc<Cell<bool>>,
    band_pipette: gtk::ToggleButton,

    white_pipette: gtk::ToggleButton,
    picking_white: Rc<Cell<bool>>,

    mixer_swatches: Rc<RefCell<Vec<gtk::ToggleButton>>>,

    picking_point: Rc<Cell<bool>>,
    point_pipette: gtk::ToggleButton,
    point_swatches: gtk::Box,
    point_selected: Rc<Cell<usize>>,
    point_sliders: Vec<gtk::Scale>,
    point_controls: gtk::Box,
    point_show: gtk::ToggleButton,

    space_picker: gtk::DropDown,
    space_note: gtk::Label,
    profile_label: gtk::Label,

    info: gtk::Box,

    render_info: gtk::Label,
    before: gtk::ToggleButton,
    crop_area: gtk::DrawingArea,

    guides: Rc<Cell<u8>>,
    guides_area: gtk::DrawingArea,
    guides_button: gtk::Button,
    crop_rect: Rc<Cell<[f32; 4]>>,

    crop_ratio: Rc<Cell<Option<f32>>>,
    straighten: gtk::Scale,

    perspective_sliders: Vec<gtk::Scale>,

    guided: gtk::ToggleButton,
    guide_lines: Rc<RefCell<Vec<numa::core::guided::Guide>>>,
    crop_controls: gtk::Box,
    histogram_area: gtk::DrawingArea,

    curve_area: gtk::DrawingArea,

    curve_channel: Rc<Cell<usize>>,
    histogram: Rc<RefCell<Option<render::histogram::Histogram>>>,
    shadow_clip: gtk::ToggleButton,
    highlight_clip: gtk::ToggleButton,

    canvas_scroller: gtk::ScrolledWindow,

    sliders: Sliders,

    applying: Rc<Cell<bool>>,

    loading_full: Rc<Cell<bool>>,

    rendered_size: Rc<Cell<(u32, u32)>>,

    open_generation: Rc<Cell<u64>>,

    full_resolution_stale: Rc<Cell<bool>>,

    failed_key: Rc<RefCell<Option<ColourKey>>>,

    switching_library: Rc<Cell<bool>>,

    tile: Rc<Cell<Option<[f32; 4]>>>,

    order: Rc<RefCell<Vec<i64>>>,
    filmstrip: gtk::Box,

    strip_badges: Rc<RefCell<HashMap<i64, gtk::Label>>>,

    rating_button: gtk::MenuButton,

    info_button: gtk::MenuButton,

    mixer_sliders: Vec<gtk::Scale>,
    mixer_band: Rc<Cell<usize>>,

    grading_sliders: Vec<gtk::Scale>,
    grading_shape: Vec<gtk::Scale>,
    grading_range: Rc<Cell<usize>>,

    retouch_area: gtk::DrawingArea,
    retouch_sliders: Vec<gtk::Scale>,

    retouch_tool: Rc<Cell<Option<bool>>>,
    retouch_heal: gtk::ToggleButton,
    retouch_clone: gtk::ToggleButton,
    retouch_on: Rc<Cell<bool>>,
    selected_spot: Rc<Cell<Option<usize>>>,
    retouch_list: gtk::ListBox,

    selected_mask: Rc<Cell<Option<usize>>>,

    busy: Rc<Cell<bool>>,

    grid_stale: Rc<Cell<bool>>,

    scanning: Rc<Cell<bool>>,

    folder: Rc<RefCell<Option<PathBuf>>>,
    folder_picker: gtk::DropDown,
    folders: Rc<RefCell<Vec<PathBuf>>>,

    analyse_button: Rc<RefCell<Option<adw::SplitButton>>>,

    mask_button: gtk::MenuButton,

    mask_banner: gtk::Box,

    panel_tab_strip: gtk::Box,

    presets_page: gtk::Box,

    mask_editor: gtk::Box,

    mask_banner_eye: gtk::ToggleButton,

    show_dots: Rc<Cell<bool>>,

    mask_outline: Rc<RefCell<Vec<Vec<[f32; 2]>>>>,

    mask_wash: Rc<RefCell<Option<gtk::cairo::ImageSurface>>>,
    mask_wash_size: Rc<Cell<(usize, usize)>>,
    mask_dot_cache: Rc<RefCell<Vec<([f32; 2], bool, MaskPart)>>>,
    ants_phase: Rc<Cell<f64>>,
    ants_running: Rc<Cell<bool>>,
    show_ants: Rc<Cell<bool>>,

    brush_owner: Rc<Cell<Option<usize>>>,

    face_sliders: Vec<gtk::Scale>,
    face_section: gtk::Box,
    face_note: gtk::Label,

    found_box: gtk::FlowBox,

    previewing: Rc<Cell<bool>>,

    drafting: Rc<Cell<bool>>,
    settle_generation: Rc<Cell<u64>>,
    library_crumb: gtk::Button,
    photo_crumb: gtk::Button,
    mask_list: gtk::ListBox,

    history_list: gtk::ListBox,

    mask_parts: gtk::ListBox,
    mask_parts_header: gtk::Label,

    brush: Rc<Cell<MaskTool>>,
    brush_radius: Rc<Cell<f32>>,
    brush_paint: gtk::ToggleButton,
    brush_lasso: gtk::ToggleButton,
    brush_controls: gtk::Box,

    dots_toggle: gtk::ToggleButton,

    brush_at: Rc<Cell<Option<(f32, f32)>>>,

    show_coverage: Rc<Cell<bool>>,
    mask_empty: gtk::Label,

    global_only: Rc<RefCell<Vec<gtk::Widget>>>,
    panel_stack: gtk::Stack,
    panel_tabs: Rc<RefCell<Vec<(&'static str, gtk::ToggleButton)>>>,

    mask_area: gtk::DrawingArea,
    filmstrip_scroller: gtk::ScrolledWindow,

    grid_scroller: gtk::ScrolledWindow,

    face_names_area: gtk::DrawingArea,
    show_face_names: Rc<Cell<bool>>,
    face_names: Rc<RefCell<Vec<([f32; 4], String)>>>,

    reference_button: gtk::ToggleButton,
    reference_pane: gtk::Box,
    reference_picture: gtk::Picture,
    reference_caption: gtk::Label,

    loupe: gtk::Box,
    loupe_picture: gtk::Picture,
    loupe_caption: gtk::Label,

    loupe_at: Rc<Cell<Option<usize>>>,

    grid_cards: Rc<RefCell<Vec<LazyThumb>>>,
    strip_cards: Rc<RefCell<Vec<LazyThumb>>>,

    thumbnail_generation: Rc<Cell<u64>>,

    thumbnail_watch: Rc<Cell<bool>>,

    clipboard: Rc<RefCell<Option<Document>>>,
    clipboard_parts: Rc<Cell<EditParts>>,

    presets_menu: gio::Menu,

    export_settings: Rc<RefCell<export::ExportSettings>>,

    export_button: Rc<RefCell<Option<gtk::Button>>>,
    library_export: Rc<RefCell<Option<gtk::Button>>>,

    rendered_from_full: Rc<Cell<bool>>,

    crop_at_open: Rc<Cell<Option<([f32; 4], f32, f32)>>>,

    recentring: Rc<Cell<bool>>,

    zoom_generation: Rc<Cell<u64>>,

    history_generation: Rc<Cell<u64>>,

    save_generation: Rc<Cell<u64>>,

    render_pending: Rc<Cell<bool>>,
    toasts: adw::ToastOverlay,
}

impl App {
    fn toast(&self, message: &str) {
        self.toasts.add_toast(adw::Toast::new(message));
    }
}

const BUSY_AFTER: std::time::Duration = std::time::Duration::from_millis(400);

#[derive(Clone, Default)]
struct Cancel(Rc<Cell<bool>>);

impl Cancel {
    fn stop(&self) {
        self.0.set(true);
    }

    fn stopped(&self) -> bool {
        self.0.get()
    }
}

fn busy<T: Send + 'static>(
    state: &App,
    label: &str,
    work: impl FnOnce() -> T + Send + 'static,
) -> impl std::future::Future<Output = Result<T, Box<dyn std::any::Any + Send>>> {
    busy_until(state, label, work)
}

fn progress_toast(state: &App, cancel: &Cancel) -> (adw::Toast, gtk::Label, gtk::ProgressBar) {
    let column = gtk::Box::new(gtk::Orientation::Vertical, 4);
    let text = gtk::Label::new(None);

    text.add_css_class("numeric");
    let bar = gtk::ProgressBar::new();
    bar.set_hexpand(true);
    column.append(&text);
    column.append(&bar);

    let toast = adw::Toast::new("");
    toast.set_custom_title(Some(&column));
    toast.set_timeout(0);
    toast.set_button_label(Some("Stop"));
    toast.connect_button_clicked(glib::clone!(
        #[strong] cancel,
        move |_| cancel.stop()
    ));
    state.toasts.add_toast(toast.clone());
    (toast, text, bar)
}

fn busy_until<T: Send + 'static>(
    state: &App,
    label: &str,
    work: impl FnOnce() -> T + Send + 'static,
) -> impl std::future::Future<Output = Result<T, Box<dyn std::any::Any + Send>>> {
    let state = state.clone();
    let label = label.to_string();
    async move {
        let done = Rc::new(Cell::new(false));
        let shown: Rc<RefCell<Option<adw::Toast>>> = Rc::new(RefCell::new(None));
        glib::timeout_add_local_once(
            BUSY_AFTER,
            glib::clone!(
                #[strong] done,
                #[strong] shown,
                #[strong] state,
                move || {
                    if done.get() {
                        return;
                    }
                    let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
                    let spinner = gtk::Spinner::new();
                    spinner.set_spinning(true);
                    row.append(&spinner);
                    row.append(&gtk::Label::new(Some(&label)));
                    let toast = adw::Toast::new("");
                    toast.set_custom_title(Some(&row));
                    toast.set_timeout(0);
                    state.toasts.add_toast(toast.clone());
                    *shown.borrow_mut() = Some(toast);
                }
            ),
        );

        let out = gtk::gio::spawn_blocking(work).await;
        done.set(true);
        if let Some(toast) = shown.borrow_mut().take() {
            toast.dismiss();
        }
        out
    }
}

fn busy_sync(state: &App, label: &str, work: impl FnOnce(&App) + 'static) {
    if state.busy.replace(true) {
        return;
    }
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let spinner = gtk::Spinner::new();
    spinner.set_spinning(true);
    row.append(&spinner);
    row.append(&gtk::Label::new(Some(label)));
    let toast = adw::Toast::new("");
    toast.set_custom_title(Some(&row));
    toast.set_timeout(0);
    state.toasts.add_toast(toast.clone());

    let state = state.clone();
    let label = label.to_string();

    glib::idle_add_local_once(move || {
        timed(&label, || work(&state));
        toast.dismiss();
        state.busy.set(false);
    });
}

fn timed<T>(label: &str, work: impl FnOnce() -> T) -> T {
    let started = std::time::Instant::now();
    let out = work();
    let took = started.elapsed();
    if took > BUSY_AFTER {
        log::warn!("{label} took {took:.0?} on the main thread");
    }
    out
}

pub fn build_window(app: &adw::Application) -> adw::ApplicationWindow {
    load_css();

    let window = adw::ApplicationWindow::new(app);
    window.set_title(Some("Numa"));

    let catalog = match Catalog::open_default() {
        Ok(catalog) => catalog,
        Err(err) => {

            let dialog = adw::AlertDialog::new(Some("Cannot open the catalog"), Some(&err));
            dialog.add_response("quit", "Quit");
            dialog.present(Some(&window));
            let app = app.clone();
            dialog.connect_response(None, move |_, _| app.quit());
            return window;
        }
    };

    let canvas = gtk::Picture::new();
    canvas.set_can_shrink(true);
    canvas.set_content_fit(gtk::ContentFit::Contain);

    let empty = gtk::Label::new(Some("Add a folder to start. Your photos stay where they are."));
    empty.add_css_class("empty-state");
    empty.set_wrap(true);

    let state = App {
        catalog: Rc::new(catalog),
        library: Rc::new(RefCell::new(None)),
        libraries: Rc::new(RefCell::new(Vec::new())),
        filter: Rc::new(RefCell::new(Filter::default())),
        picker_places: Rc::new(RefCell::new(Vec::new())),
        albums_menu: gio::Menu::new(),
        cards: Rc::new(RefCell::new(HashMap::new())),
        open: Rc::new(RefCell::new(None)),
        zoom: Rc::new(Cell::new(FIT_ZOOM)),
        wall: Justified::default(),
        empty,
        welcome: adw::StatusPage::new(),
        stack: gtk::Stack::new(),
        canvas,
        library_picker: gtk::DropDown::from_strings(&[]),
        zoom_label: gtk::Label::new(Some("Fit")),
        profile_picker: gtk::DropDown::from_strings(&[]),
        picking_band: Rc::new(Cell::new(false)),
        band_pipette: gtk::ToggleButton::new(),
        white_pipette: gtk::ToggleButton::new(),
        picking_white: Rc::new(Cell::new(false)),
        mixer_swatches: Rc::new(RefCell::new(Vec::new())),
        picking_point: Rc::new(Cell::new(false)),
        point_pipette: gtk::ToggleButton::new(),
        point_swatches: gtk::Box::new(gtk::Orientation::Horizontal, 6),
        point_selected: Rc::new(Cell::new(0)),
        point_sliders: [(-100.0, 100.0), (-100.0, 100.0), (-100.0, 100.0), (0.0, 100.0)]
            .into_iter()
            .map(|(low, high)| gtk::Scale::with_range(gtk::Orientation::Horizontal, low, high, 1.0))
            .collect(),
        point_controls: gtk::Box::new(gtk::Orientation::Vertical, 0),
        point_show: gtk::ToggleButton::new(),
        space_picker: gtk::DropDown::from_strings(&[]),
        space_note: gtk::Label::new(None),
        profile_label: gtk::Label::new(None),
        info: gtk::Box::new(gtk::Orientation::Vertical, 18),
        render_info: gtk::Label::new(None),
        before: gtk::ToggleButton::new(),
        crop_area: gtk::DrawingArea::new(),
        guides: Rc::new(Cell::new(0)),
        guides_area: gtk::DrawingArea::new(),
        guides_button: gtk::Button::new(),
        crop_rect: Rc::new(Cell::new([0.0, 0.0, 1.0, 1.0])),
        crop_ratio: Rc::new(Cell::new(None)),
        straighten: gtk::Scale::with_range(gtk::Orientation::Horizontal, -15.0, 15.0, 0.1),
        perspective_sliders: (0..3)
            .map(|_| gtk::Scale::with_range(gtk::Orientation::Horizontal, -100.0, 100.0, 1.0))
            .collect(),
        guided: gtk::ToggleButton::with_label("Guided"),
        guide_lines: Rc::new(RefCell::new(Vec::new())),
        crop_controls: gtk::Box::new(gtk::Orientation::Vertical, 6),
        histogram_area: gtk::DrawingArea::new(),
        curve_area: gtk::DrawingArea::new(),
        curve_channel: Rc::new(Cell::new(0)),
        histogram: Rc::new(RefCell::new(None)),
        shadow_clip: gtk::ToggleButton::new(),
        highlight_clip: gtk::ToggleButton::new(),
        canvas_scroller: gtk::ScrolledWindow::new(),
        sliders: Sliders::new(),
        applying: Rc::new(Cell::new(false)),
        loading_full: Rc::new(Cell::new(false)),
        rendered_size: Rc::new(Cell::new((0, 0))),
        open_generation: Rc::new(Cell::new(0)),
        full_resolution_stale: Rc::new(Cell::new(false)),
        failed_key: Rc::new(RefCell::new(None)),
        switching_library: Rc::new(Cell::new(false)),
        tile: Rc::new(Cell::new(None)),
        order: Rc::new(RefCell::new(Vec::new())),
        filmstrip: gtk::Box::new(gtk::Orientation::Horizontal, 6),
        strip_badges: Rc::new(RefCell::new(HashMap::new())),
        rating_button: gtk::MenuButton::new(),
        info_button: gtk::MenuButton::new(),
        mixer_sliders: (0..3)
            .map(|_| gtk::Scale::with_range(gtk::Orientation::Horizontal, -100.0, 100.0, 1.0))
            .collect(),
        mixer_band: Rc::new(Cell::new(0)),
        grading_sliders: vec![
            gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 360.0, 1.0),
            gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 100.0, 1.0),
            gtk::Scale::with_range(gtk::Orientation::Horizontal, -100.0, 100.0, 1.0),
        ],
        grading_shape: vec![
            gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 100.0, 1.0),
            gtk::Scale::with_range(gtk::Orientation::Horizontal, -100.0, 100.0, 1.0),
        ],
        grading_range: Rc::new(Cell::new(0)),
        retouch_area: gtk::DrawingArea::new(),
        retouch_sliders: vec![

            gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.5, 20.0, 0.1),
            gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 100.0, 1.0),
            gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 100.0, 1.0),
        ],
        retouch_tool: Rc::new(Cell::new(None)),
        retouch_heal: gtk::ToggleButton::new(),
        retouch_clone: gtk::ToggleButton::new(),
        retouch_on: Rc::new(Cell::new(false)),
        selected_spot: Rc::new(Cell::new(None)),
        retouch_list: gtk::ListBox::new(),
        selected_mask: Rc::new(Cell::new(None)),
        grid_stale: Rc::new(Cell::new(false)),
        scanning: Rc::new(Cell::new(false)),
        folder: Rc::default(),
        folder_picker: gtk::DropDown::from_strings(&["All folders"]),
        folders: Rc::default(),
        analyse_button: Rc::default(),
        busy: Rc::new(Cell::new(false)),
        mask_button: gtk::MenuButton::new(),
        global_only: Rc::new(RefCell::new(Vec::new())),
        panel_stack: gtk::Stack::new(),
        panel_tabs: Rc::new(RefCell::new(Vec::new())),
        mask_banner: gtk::Box::new(gtk::Orientation::Horizontal, 6),
        panel_tab_strip: gtk::Box::new(gtk::Orientation::Horizontal, 0),
        presets_page: gtk::Box::new(gtk::Orientation::Vertical, 0),
        mask_editor: gtk::Box::new(gtk::Orientation::Vertical, 8),
        mask_banner_eye: gtk::ToggleButton::new(),
        show_dots: Rc::new(Cell::new(true)),
        mask_outline: Rc::new(RefCell::new(Vec::new())),
        mask_wash: Rc::new(RefCell::new(None)),
        mask_wash_size: Rc::new(Cell::new((0, 0))),
        mask_dot_cache: Rc::new(RefCell::new(Vec::new())),
        ants_phase: Rc::new(Cell::new(0.0)),
        ants_running: Rc::new(Cell::new(false)),
        show_ants: Rc::new(Cell::new(true)),
        brush_owner: Rc::new(Cell::new(None)),
        face_sliders: (0..5)
            .map(|_| gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 100.0, 1.0))
            .collect(),
        face_section: gtk::Box::new(gtk::Orientation::Vertical, 0),
        face_note: gtk::Label::new(None),
        found_box: gtk::FlowBox::new(),
        previewing: Rc::new(Cell::new(false)),
        drafting: Rc::new(Cell::new(false)),
        settle_generation: Rc::new(Cell::new(0)),
        library_crumb: gtk::Button::new(),
        photo_crumb: gtk::Button::new(),
        mask_list: gtk::ListBox::new(),
        history_list: gtk::ListBox::new(),
        mask_parts: gtk::ListBox::new(),
        mask_parts_header: gtk::Label::new(None),
        brush: Rc::new(Cell::new(MaskTool::Off)),
        brush_radius: Rc::new(Cell::new(DEFAULT_BRUSH)),
        brush_paint: gtk::ToggleButton::new(),
        brush_lasso: gtk::ToggleButton::new(),
        brush_controls: gtk::Box::new(gtk::Orientation::Vertical, 6),
        dots_toggle: gtk::ToggleButton::new(),
        brush_at: Rc::new(Cell::new(None)),
        show_coverage: Rc::new(Cell::new(true)),
        mask_empty: gtk::Label::new(None),
        mask_area: gtk::DrawingArea::new(),
        filmstrip_scroller: gtk::ScrolledWindow::new(),
        grid_scroller: gtk::ScrolledWindow::new(),
        face_names_area: gtk::DrawingArea::new(),
        show_face_names: Rc::new(Cell::new(false)),
        face_names: Rc::new(RefCell::new(Vec::new())),
        reference_button: gtk::ToggleButton::new(),
        reference_pane: gtk::Box::new(gtk::Orientation::Vertical, 6),
        reference_picture: gtk::Picture::new(),
        reference_caption: gtk::Label::new(None),
        loupe: gtk::Box::new(gtk::Orientation::Vertical, 6),
        loupe_picture: gtk::Picture::new(),
        loupe_caption: gtk::Label::new(None),
        loupe_at: Rc::new(Cell::new(None)),
        grid_cards: Rc::new(RefCell::new(Vec::new())),
        strip_cards: Rc::new(RefCell::new(Vec::new())),
        thumbnail_generation: Rc::new(Cell::new(0)),
        thumbnail_watch: Rc::new(Cell::new(false)),
        clipboard: Rc::new(RefCell::new(None)),
        clipboard_parts: Rc::new(Cell::new(EditParts::default())),
        presets_menu: gio::Menu::new(),
        export_settings: Rc::new(RefCell::new(export::ExportSettings::default())),
        export_button: Rc::new(RefCell::new(None)),
        library_export: Rc::new(RefCell::new(None)),
        rendered_from_full: Rc::new(Cell::new(false)),
        crop_at_open: Rc::new(Cell::new(None)),
        recentring: Rc::new(Cell::new(false)),
        zoom_generation: Rc::new(Cell::new(0)),
        history_generation: Rc::new(Cell::new(0)),
        save_generation: Rc::new(Cell::new(0)),
        render_pending: Rc::new(Cell::new(false)),
        toasts: adw::ToastOverlay::new(),
    };

    match state.catalog.recall::<WindowState>(WINDOW_STATE) {
        Some(remembered) => {
            window.set_default_size(remembered.width.max(640), remembered.height.max(480));
            if remembered.maximised {
                window.maximize();
            }
        }
        None => window.set_default_size(1280, 860),
    }
    if let Some(filter) = state.catalog.recall::<Filter>(GRID_FILTER) {
        *state.filter.borrow_mut() = filter;
    }
    if let Some(settings) = state.catalog.recall::<export::ExportSettings>(EXPORT_SETTINGS) {
        *state.export_settings.borrow_mut() = settings;
    }

    state.stack.add_named(&build_library_page(&state, &window), Some("library"));
    state.stack.add_named(&build_editor_page(&state), Some("editor"));
    state.stack.set_visible_child_name("library");

    let view = gtk::Box::new(gtk::Orientation::Vertical, 0);
    view.append(&build_header(&state, &window));
    view.append(&state.stack);

    state.toasts.set_child(Some(&view));
    window.set_content(Some(&state.toasts));

    install_rating_shortcuts(&state, &window);

    name_icon_buttons(view.upcast_ref());
    reload_libraries(&state);
    offer_additional_files(&state, &window);
    integrate_appimage(&state);

    glib::timeout_add_seconds_local_once(
        30,
        glib::clone!(
            #[strong] state,
            #[weak] window,
            move || consider_update_check(&state, &window)
        ),
    );

    let open_at = std::env::var("NUMA_OPEN").ok().and_then(|at| match at.split_once(':') {
        Some((library, photo)) => Some((library.parse::<i64>().ok()? << 32) | photo.parse::<i64>().ok()?),
        None => at.parse::<i64>().ok(),
    });
    if let Some(id) = open_at {
        glib::timeout_add_local(
            std::time::Duration::from_millis(100),
            glib::clone!(
                #[strong] state,
                move || {
                    if !state.cards.borrow().contains_key(&id) {
                        return glib::ControlFlow::Continue;
                    }
                    open_photo(&state, id);
                    glib::ControlFlow::Break
                }
            ),
        );
    }

    window.connect_is_active_notify(glib::clone!(
        #[strong] state,
        move |window| {
            if window.is_active() {
                rescan_in_background(&state);
            }
        }
    ));
    glib::timeout_add_seconds_local(
        60,
        glib::clone!(
            #[strong] state,
            move || {
                rescan_in_background(&state);
                glib::ControlFlow::Continue
            }
        ),
    );

    window.connect_close_request(glib::clone!(
        #[strong] state,
        move |window| {
            save_open_edits(&state);

            let (width, height) = window.default_size();
            state.catalog.remember(
                WINDOW_STATE,
                &WindowState { width, height, maximised: window.is_maximized() },
            );
            glib::Propagation::Proceed
        }
    ));

    window
}

fn build_header(state: &App, window: &adw::ApplicationWindow) -> adw::HeaderBar {
    let header = adw::HeaderBar::new();

    let add = gtk::Button::from_icon_name("folder-open-symbolic");
    add.set_tooltip_text(Some("Add folder to library"));
    add.connect_clicked(glib::clone!(
        #[strong] state,
        #[weak] window,
        move |_| add_library_dialog(&state, &window)
    ));

    let rescan = gtk::Button::from_icon_name("view-refresh-symbolic");
    rescan.set_tooltip_text(Some("Rescan this folder"));
    rescan.connect_clicked(glib::clone!(
        #[strong] state,
        move |_| {
            let Some(library) = state.library.borrow().clone() else {
                state.toast("No library selected — pick one to rescan");
                return;
            };
            if state.filter.borrow().spans_libraries() {
                rescan_everywhere(&state);
                return;
            }
            match state.catalog.sync_library(&library) {
                Ok(added) => {
                    reload_grid(&state);
                    state.toast(&format!("Rescanned: {added} new photo(s)"));
                }
                Err(err) => state.toast(&format!("Rescan failed: {err}")),
            }
        }
    ));

    let library_actions = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    library_actions.append(&add);

    state.library_picker.bind_property("visible", &rescan, "visible").sync_create().build();
    library_actions.append(&rescan);

    let back = gtk::Button::from_icon_name("go-previous-symbolic");
    back.set_tooltip_text(Some("Back to the library"));
    back.set_valign(gtk::Align::Center);
    back.connect_clicked(glib::clone!(
        #[strong] state,
        move |_| close_editor(&state)
    ));

    let start = gtk::Stack::new();
    start.add_named(&library_actions, Some("library"));
    start.add_named(&back, Some("editor"));
    start.set_visible_child_name("library");

    header.pack_start(&start);

    state.library_picker.set_tooltip_text(Some("Library, album or person"));
    state.library_picker.set_hexpand(false);
    state.library_picker.connect_selected_notify(glib::clone!(
        #[strong] state,
        move |picker| {
            if state.switching_library.get() {
                return;
            }
            let Some(place) = state.picker_places.borrow().get(picker.selected() as usize).cloned() else {
                return;
            };
            choose_place(&state, place);
        }
    ));

    let title = gtk::Stack::new();

    let picker_holder = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    picker_holder.append(&state.library_picker);
    title.add_named(&picker_holder, Some("library"));
    title.add_named(&build_crumbs(state), Some("editor"));
    title.set_visible_child_name("library");
    header.set_title_widget(Some(&title));

    state.stack.connect_visible_child_name_notify(move |stack| {
        if let Some(name) = stack.visible_child_name() {
            title.set_visible_child_name(&name);
            start.set_visible_child_name(&name);
        }
    });

    let manage = gio::SimpleAction::new("libraries", None);
    manage.connect_activate(glib::clone!(
        #[strong] state,
        #[weak] window,
        move |_, _| libraries_dialog(&state, &window)
    ));
    window.add_action(&manage);
    install_album_actions(state, window);

    let shortcuts = gio::SimpleAction::new("shortcuts", None);
    shortcuts.connect_activate(glib::clone!(
        #[weak] window,
        move |_, _| shortcuts_dialog(&window)
    ));
    window.add_action(&shortcuts);
    if let Some(app) = window.application() {
        app.set_accels_for_action("win.shortcuts", &["<primary>slash"]);
    }

    let preferences = gio::SimpleAction::new("preferences", None);
    preferences.connect_activate(glib::clone!(
        #[strong] state,
        #[weak] window,
        move |_, _| preferences_dialog(&state, &window)
    ));
    window.add_action(&preferences);
    if let Some(app) = window.application() {
        app.set_accels_for_action("win.preferences", &["<primary>comma"]);
    }

    let about = gio::SimpleAction::new("about", None);
    about.connect_activate(glib::clone!(
        #[strong] state,
        #[weak] window,
        move |_, _| show_about(Some(window.upcast_ref()), Some(&state))
    ));
    window.add_action(&about);

    let open_file = gio::SimpleAction::new("open-file", None);
    open_file.connect_activate(glib::clone!(
        #[strong] state,
        #[weak] window,
        move |_, _| {
            let dialog = gtk::FileDialog::new();
            dialog.set_title("Open a photograph");
            let (state, parent) = (state.clone(), window.clone());
            dialog.open(Some(&window), gio::Cancellable::NONE, move |chosen| {
                if let Some(path) = chosen.ok().and_then(|file| file.path()) {
                    open_path(&state, &parent, path);
                }
            });
        }
    ));
    window.add_action(&open_file);
    if let Some(app) = window.application() {
        app.set_accels_for_action("win.open-file", &["<primary>o"]);
    }
    let open_path_action = gio::SimpleAction::new("open-path", Some(&String::static_variant_type()));
    open_path_action.connect_activate(glib::clone!(
        #[strong] state,
        #[weak] window,
        move |_, path| {
            if let Some(path) = path.and_then(|path| path.get::<String>()) {
                open_path(&state, &window, PathBuf::from(path));
            }
        }
    ));
    window.add_action(&open_path_action);

    let menu = gio::Menu::new();
    menu.append(Some("Open…"), Some("win.open-file"));
    menu.append(Some("Libraries…"), Some("win.libraries"));
    menu.append(Some("Albums…"), Some("win.albums"));
    menu.append(Some("Preferences"), Some("win.preferences"));
    menu.append(Some("Keyboard shortcuts"), Some("win.shortcuts"));
    menu.append(Some("About"), Some("win.about"));
    menu.append(Some("Quit"), Some("app.quit"));
    let menu_button = gtk::MenuButton::new();
    menu_button.set_menu_model(Some(&menu));
    menu_button.set_icon_name("open-menu-symbolic");
    header.pack_end(&menu_button);

    header
}

pub fn show_app_about(parent: Option<&gtk::Window>) {
    show_about(parent, None);
}

fn show_about(parent: Option<&gtk::Window>, state: Option<&App>) {
    let about = adw::AboutDialog::new();
    about.set_application_name("Numa");

    let dark = format!("{}-dark", crate::APP_ID);
    let themed = gtk::gdk::Display::default().map(|display| gtk::IconTheme::for_display(&display));
    let icon = match &themed {
        Some(theme) if adw::StyleManager::default().is_dark() && theme.has_icon(&dark) => &dark,
        _ => crate::APP_ID,
    };
    about.set_application_icon(icon);
    about.set_developers(&["Tijmen"]);
    about.set_version(env!("CARGO_PKG_VERSION"));

    about.add_legal_section(
        "Camera profiles",
        Some("RawTherapee DCP profiles"),
        gtk::License::Custom,
        Some("Released CC0 by the RawTherapee project.\nhttps://rawtherapee.com"),
    );

    about.set_debug_info(&debug_info(state));
    about.set_debug_info_filename("numa-debug-info.txt");
    about.set_issue_url("https://github.com/simmmmm/Numa/issues/new");

    about.present(parent);
}

fn debug_info(state: Option<&App>) -> String {
    let mut lines = vec![
        format!("Numa {}", env!("CARGO_PKG_VERSION")),
        format!("GTK {}.{}.{}", gtk::major_version(), gtk::minor_version(), gtk::micro_version()),
        format!("libadwaita {}.{}.{}", adw::major_version(), adw::minor_version(), adw::micro_version()),
        format!(
            "Renderer: {}",
            std::env::var("GSK_RENDERER").unwrap_or_else(|_| "default (GSK_RENDERER unset)".to_string())
        ),
        format!("AppImage: {}", std::env::var("APPIMAGE").unwrap_or_else(|_| "no".to_string())),
        String::new(),
        "Models:".to_string(),
    ];
    for (name, installed) in [
        ("Found masks (EfficientViT)", numa::render::segment::is_installed()),
        ("Click to select (SlimSAM)", numa::render::sam::is_installed()),
        ("Clean edges (IS-Net)", numa::render::matte::is_installed()),
        ("Faces (YuNet)", numa::cull::faces::is_installed()),
        ("People (SFace)", numa::cull::people::is_installed()),
        ("Animals (PP-ResNet50)", numa::render::classify::is_installed()),
    ] {
        lines.push(format!("  {name}: {}", if installed { "installed" } else { "not installed" }));
    }

    if let Some(state) = state {
        if let Some(library) = state.library.borrow().as_ref() {
            lines.push(String::new());
            lines.push(format!("Library: {} photo(s)", state.cards.borrow().len()));

            let mut cameras: Vec<String> = state
                .cards
                .borrow()
                .values()
                .take(40)
                .filter_map(|(photo, _)| numa::io::raw::summary(&photo.path)?.camera)
                .collect();
            cameras.sort();
            cameras.dedup();
            lines.push(format!("Cameras (sampled): {}", if cameras.is_empty() { "none read".to_string() } else { cameras.join(", ") }));
            let _ = library;
        }
    }

    lines.push(String::new());
    lines.push("Logs go to the terminal Numa was started from. For more, start it with".to_string());
    lines.push("RUST_LOG=numa=debug and include that output.".to_string());
    lines.join("\n")
}

fn libraries_dialog(state: &App, window: &adw::ApplicationWindow) {
    let dialog = adw::Dialog::new();
    dialog.set_title("Libraries");
    dialog.set_content_width(560);
    dialog.set_content_height(520);

    let page = adw::PreferencesPage::new();
    let group = adw::PreferencesGroup::new();
    group.set_title("Folders in the catalog");
    group.set_description(Some(
        "The photographs stay where they are, and so does everything done to \
         them: each folder keeps its own catalog. Removing a folder here only \
         stops showing it.",
    ));

    let add = gtk::Button::from_icon_name("folder-open-symbolic");
    add.set_tooltip_text(Some("Add a folder"));
    add.add_css_class("flat");
    add.connect_clicked(glib::clone!(
        #[strong] state,
        #[weak] window,
        #[weak] dialog,
        move |_| {
            dialog.close();
            add_library_dialog(&state, &window);
        }
    ));
    group.set_header_suffix(Some(&add));

    let libraries = state.libraries.borrow().clone();
    if libraries.is_empty() {
        let empty = adw::ActionRow::new();
        empty.set_title("No folders yet");
        empty.set_subtitle("Add one to start.");
        group.add(&empty);
    }

    for library in libraries {
        let counted = state.catalog.photo_count(library.id);
        let count = counted.as_ref().copied().unwrap_or(0);
        let row = adw::EntryRow::new();

        row.set_title(&match counted {
            Ok(count) => format!("{count} photo(s) · {}", library.path.display()),
            Err(_) => format!("Not connected · {}", library.path.display()),
        });
        row.set_text(&library.label());
        row.set_show_apply_button(true);

        row.connect_apply(glib::clone!(
            #[strong] state,
            #[strong] library,
            #[weak] dialog,
            #[weak] window,
            move |row| {
                let name = row.text().trim().to_string();
                let confirm = adw::AlertDialog::new(
                    Some("Rename the folder?"),
                    Some(&format!(
                        "“{}” becomes “{name}” on disk, with everything in it. Other programs \
                         that remember the old name will not find it there any more.",
                        library.path.display(),
                    )),
                );
                confirm.add_response("cancel", "Cancel");
                confirm.add_response("rename", "Rename");
                confirm.set_response_appearance("rename", adw::ResponseAppearance::Suggested);
                confirm.set_default_response(Some("cancel"));

                let (state, dialog, parent) = (state.clone(), dialog.clone(), window.clone());
                confirm.connect_response(None, move |confirm, response| {
                    confirm.close();
                    if response != "rename" {
                        return;
                    }

                    if state.open.borrow().is_some() {
                        close_editor(&state);
                    }
                    match state.catalog.rename_library_folder(library.id, &name) {
                        Ok(_) => {
                            reload_libraries(&state);
                            dialog.close();
                            libraries_dialog(&state, &parent);
                        }
                        Err(err) => state.toast(&format!("Could not rename: {err}")),
                    }
                });
                confirm.present(Some(&window));
            }
        ));

        let remove = gtk::Button::from_icon_name("user-trash-symbolic");
        remove.set_tooltip_text(Some("Remove from the catalog"));
        remove.set_valign(gtk::Align::Center);
        remove.add_css_class("flat");
        remove.connect_clicked(glib::clone!(
            #[strong] state,
            #[weak] dialog,
            #[weak] window,
            move |_| {
                let confirm = adw::AlertDialog::new(
                    Some(&format!("Remove {}?", library.label())),
                    Some(&format!(
                        "The {count} photograph(s) stay on disk, and so do their ratings, \
                         flags and edits — they are kept in the folder itself. Adding the \
                         folder back brings everything back.",
                    )),
                );
                confirm.add_response("cancel", "Cancel");
                confirm.add_response("remove", "Remove");
                confirm.set_response_appearance("remove", adw::ResponseAppearance::Destructive);
                confirm.set_default_response(Some("cancel"));

                let state = state.clone();
                let dialog = dialog.clone();
                let parent = window.clone();
                confirm.connect_response(None, move |confirm, response| {
                    confirm.close();
                    if response != "remove" {
                        return;
                    }
                    match state.catalog.remove_library(library.id) {
                        Ok(()) => {
                            reload_libraries(&state);
                            dialog.close();
                            libraries_dialog(&state, &parent);
                        }
                        Err(err) => state.toast(&format!("Could not remove: {err}")),
                    }
                });
                confirm.present(Some(&window));
            }
        ));
        row.add_suffix(&remove);
        group.add(&row);
    }

    page.add(&group);

    let bar = adw::ToolbarView::new();
    bar.add_top_bar(&adw::HeaderBar::new());
    bar.set_content(Some(&page));
    dialog.set_child(Some(&bar));
    dialog.present(Some(window));
}

fn people_dialog(state: &App, window: &adw::ApplicationWindow) {

    if first_time(state, "explained-faces") {
        let alert = adw::AlertDialog::new(
            Some("Faces stay on this computer"),
            Some(
                "Numa finds and recognises faces with models that run here. Nothing is \
                 uploaded, and nothing leaves this computer.\n\nThe names you give are kept \
                 in the library's own .numa folder — so they travel with the folder when it \
                 is copied or shared.",
            ),
        );
        alert.add_response("ok", "Continue");
        let (state, parent) = (state.clone(), window.clone());
        alert.connect_response(None, move |_, _| {
            mark_seen(&state, "explained-faces");
            people_dialog(&state, &parent);
        });
        alert.present(Some(window));
        return;
    }
    let dialog = adw::Dialog::new();
    dialog.set_title("People");
    dialog.set_content_width(560);
    dialog.set_content_height(640);

    let holder = adw::Bin::new();
    let bar = adw::ToolbarView::new();
    bar.add_top_bar(&adw::HeaderBar::new());
    bar.set_content(Some(&holder));
    dialog.set_child(Some(&bar));
    fill_people(state, &dialog, &holder, true);
    dialog.present(Some(window));
}

fn fill_people(state: &App, dialog: &adw::Dialog, holder: &adw::Bin, fetch: bool) {
    let Some(library) = state.library.borrow().clone() else { return };
    let faces = state.catalog.faces(library.id).unwrap_or_else(|err| {
        log::warn!("could not read the faces: {err}");
        Vec::new()
    });
    if faces.is_empty() {
        let empty = adw::StatusPage::new();
        empty.set_icon_name(Some("avatar-default-symbolic"));
        empty.set_title("No faces yet");
        empty.set_description(Some("Analyse the library to find the people in it."));
        holder.set_child(Some(&empty));
        return;
    }

    let known = state.catalog.known_faces().unwrap_or_default();
    let everyone = state.catalog.people(library.id).unwrap_or_default();

    let names: Vec<Option<String>> = faces
        .iter()
        .map(|face| cull::people::recognise(&face.embedding, &known).map(|(name, _)| name.to_string()))
        .collect();

    let picture = |face: &numa::io::catalog::StoredFace| -> gtk::Widget {
        let texture = face
            .portrait
            .as_ref()
            .and_then(|bytes| gtk::gdk::Texture::from_bytes(&glib::Bytes::from(bytes)).ok());

        let image = match texture {
            Some(texture) => gtk::Image::from_paintable(Some(&texture)),
            None => gtk::Image::from_icon_name("avatar-default-symbolic"),
        };
        image.set_pixel_size(44);
        image.set_valign(gtk::Align::Center);
        image.add_css_class("face-portrait");
        image.upcast()
    };
    let mut wanted: Vec<numa::io::catalog::StoredFace> = Vec::new();

    let page = adw::PreferencesPage::new();

    if !everyone.is_empty() {
        let group = adw::PreferencesGroup::new();
        group.set_title("Named");
        group.set_description(Some("Rename someone here and it changes everywhere; give two people the same name and they become one."));
        for (name, photos) in &everyone {
            let row = adw::EntryRow::new();
            row.set_title(&format!("{} photograph(s)", photos.len()));
            row.set_text(name);
            row.set_show_apply_button(true);
            let face = faces
                .iter()
                .zip(&names)
                .filter(|(_, face_name)| face_name.as_deref().is_some_and(|n| n.eq_ignore_ascii_case(name)))
                .map(|(face, _)| face)
                .find(|face| face.portrait.is_some())
                .or_else(|| {
                    faces.iter().zip(&names).find(|(_, n)| n.as_deref().is_some_and(|n| n.eq_ignore_ascii_case(name))).map(|(f, _)| f)
                });
            if let Some(face) = face {
                if face.portrait.is_none() {
                    wanted.push(face.clone());
                }
                row.add_prefix(&picture(face));
            }

            let show = gtk::Button::with_label("Show");
            show.set_valign(gtk::Align::Center);
            show.add_css_class("flat");
            show.set_tooltip_text(Some("Only the photographs this person is in"));
            show.connect_clicked(glib::clone!(
                #[strong] state,
                #[weak] dialog,
                #[strong] name,
                move |_| {
                    state.filter.borrow_mut().in_one_library();
                    state.filter.borrow_mut().person = Some(name.clone());
                    reload_grid(&state);
                    dialog.close();
                }
            ));
            row.add_suffix(&show);

            let forget = gtk::Button::from_icon_name("user-trash-symbolic");
            forget.set_valign(gtk::Align::Center);
            forget.add_css_class("flat");
            forget.set_tooltip_text(Some("Forget this name"));
            forget.connect_clicked(glib::clone!(
                #[strong] state,
                #[weak] dialog,
                #[weak] holder,
                #[strong] name,
                move |_| {
                    if let Err(err) = state.catalog.rename_person(&name, "") {
                        state.toast(&format!("Could not forget the name: {err}"));
                        return;
                    }
                    reload_grid(&state);
                    fill_people(&state, &dialog, &holder, false);
                }
            ));
            row.add_suffix(&forget);

            row.connect_apply(glib::clone!(
                #[strong] state,
                #[weak] dialog,
                #[weak] holder,
                #[strong] name,
                move |row| {
                    if let Err(err) = state.catalog.rename_person(&name, &row.text()) {
                        state.toast(&format!("Could not rename: {err}"));
                        return;
                    }
                    let state = state.clone();
                    glib::idle_add_local_once(move || {
                        reload_grid(&state);
                        fill_people(&state, &dialog, &holder, false);
                    });
                }
            ));
            group.add(&row);
        }
        page.add(&group);
    }

    let unnamed: Vec<&numa::io::catalog::StoredFace> =
        faces.iter().zip(&names).filter(|(_, name)| name.is_none()).map(|(face, _)| face).collect();
    let embeddings: Vec<[f32; cull::people::LENGTH]> = unnamed.iter().map(|face| face.embedding).collect();
    let mut once = 0usize;
    let group = adw::PreferencesGroup::new();
    group.set_title("Not named yet");
    for members in cull::people::groups(&embeddings) {
        let members: Vec<&numa::io::catalog::StoredFace> = members.iter().map(|index| unnamed[*index]).collect();
        let mut photos: Vec<i64> = members.iter().map(|face| face.photo_id).collect();
        photos.sort_unstable();
        photos.dedup();
        if photos.len() < 2 {
            once += members.len();
            continue;
        }

        let row = adw::EntryRow::new();
        row.set_title(&format!("In {} photographs — who is this?", photos.len()));
        row.set_show_apply_button(true);
        let pictures = gtk::Box::new(gtk::Orientation::Horizontal, 4);
        for face in members.iter().take(3) {
            if face.portrait.is_none() {
                wanted.push((*face).clone());
            }
            pictures.append(&picture(face));
        }
        row.add_prefix(&pictures);

        let faces: Vec<(i64, [f32; cull::people::LENGTH])> =
            members.iter().map(|face| (face.photo_id, face.embedding)).collect();

        let ignore = gtk::Button::from_icon_name("user-trash-symbolic");
        ignore.set_valign(gtk::Align::Center);
        ignore.add_css_class("flat");
        ignore.set_tooltip_text(Some("Nobody to name — stop asking about these faces"));
        ignore.connect_clicked(glib::clone!(
            #[strong] state,
            #[weak] dialog,
            #[weak] holder,
            #[strong] faces,
            move |_| {
                if let Err(err) = state.catalog.ignore_faces(&faces) {
                    state.toast(&format!("Could not set them aside: {err}"));
                    return;
                }
                fill_people(&state, &dialog, &holder, false);
            }
        ));
        row.add_suffix(&ignore);

        let name_them = glib::clone!(
            #[strong] state,
            #[weak] dialog,
            #[weak] holder,
            move |row: &adw::EntryRow| {
                let text = row.text().to_string();
                if text.trim().is_empty() {
                    return;
                }
                for (photo, embedding) in &faces {
                    if let Err(err) = state.catalog.name_face(*photo, embedding, &text) {
                        state.toast(&format!("Could not save the name: {err}"));
                        return;
                    }
                }
                let state = state.clone();
                glib::idle_add_local_once(move || {
                    reload_grid(&state);
                    fill_people(&state, &dialog, &holder, false);
                });
            }
        );
        row.connect_apply(name_them.clone());
        row.connect_entry_activated(name_them);
        group.add(&row);
    }
    if once > 0 {
        group.set_description(Some(&format!(
            "{once} face(s) seen in only one photograph are left out — name those from the photograph's info page."
        )));
    }
    page.add(&group);

    let ignored = state.catalog.ignored_count(library.id).unwrap_or(0);
    if ignored > 0 {
        let aside = adw::PreferencesGroup::new();
        let row = adw::ActionRow::new();
        row.set_title(&format!("{ignored} face(s) set aside as nobody to name"));
        let back = gtk::Button::with_label("Ask again");
        back.set_valign(gtk::Align::Center);
        back.connect_clicked(glib::clone!(
            #[strong] state,
            #[weak] dialog,
            #[weak] holder,
            move |_| {
                if let Some(library) = state.library.borrow().clone() {
                    if let Err(err) = state.catalog.unignore_all(library.id) {
                        state.toast(&format!("Could not bring them back: {err}"));
                    }
                }
                fill_people(&state, &dialog, &holder, false);
            }
        ));
        row.add_suffix(&back);
        aside.add(&row);
        page.add(&aside);
    }
    holder.set_child(Some(&page));

    if fetch && !wanted.is_empty() {
        fetch_portraits(state, dialog, holder, library.id, wanted);
    }
}

fn fetch_portraits(
    state: &App,
    dialog: &adw::Dialog,
    holder: &adw::Bin,
    library_id: i64,
    wanted: Vec<numa::io::catalog::StoredFace>,
) {
    let paths: HashMap<i64, PathBuf> = state
        .catalog
        .photos(library_id, &Filter::default())
        .unwrap_or_default()
        .into_iter()
        .map(|photo| (photo.id, photo.path))
        .collect();
    let mut by_photo: HashMap<i64, Vec<numa::io::catalog::StoredFace>> = HashMap::new();
    for face in wanted {
        by_photo.entry(face.photo_id).or_default().push(face);
    }
    let work: Vec<(PathBuf, Vec<numa::io::catalog::StoredFace>)> = by_photo
        .into_iter()
        .filter_map(|(photo, faces)| Some((paths.get(&photo)?.clone(), faces)))
        .collect();

    let state = state.clone();
    let dialog = dialog.clone();
    let holder = holder.clone();
    glib::spawn_future_local(async move {
        let found = gtk::gio::spawn_blocking(move || {
            use rayon::prelude::*;
            work.par_iter()
                .flat_map_iter(|(path, faces)| {
                    let Ok(image) = raw::load_scaled(path, 640) else { return Vec::new() };
                    let detected = cull::faces::detect(&image).unwrap_or_default();
                    faces
                        .iter()
                        .filter_map(|stored| {
                            let best = detected
                                .iter()
                                .filter_map(|face| Some((face, cull::people::embed(&image, face)?)))
                                .max_by(|a, b| {
                                    cull::people::likeness(&a.1, &stored.embedding)
                                        .total_cmp(&cull::people::likeness(&b.1, &stored.embedding))
                                })?;
                            (cull::people::likeness(&best.1, &stored.embedding) >= 0.9)
                                .then(|| cull::people::portrait_jpeg(&image, best.0))
                                .flatten()
                                .map(|jpeg| (stored.id, jpeg))
                        })
                        .collect::<Vec<_>>()
                })
                .collect::<Vec<_>>()
        })
        .await;
        let Ok(found) = found else { return };
        for (face, jpeg) in &found {
            if let Err(err) = state.catalog.set_portrait(*face, jpeg) {
                log::warn!("could not keep a face's picture: {err}");
            }
        }

        if !found.is_empty() && dialog.parent().is_some() {
            fill_people(&state, &dialog, &holder, false);
        }
    });
}

type ModelFile = (&'static str, &'static str, u64);

const MODELS: [(&str, &str, &[ModelFile]); 7] = [
    ("EfficientViT-Seg B2", "Sky, greenery and the other found masks · Apache-2.0 · 61 MB", &[(
        "efficientvit_seg_b2_ade20k_1024.onnx",
        "https://github.com/simmmmm/Numa/releases/download/models/efficientvit_seg_b2_ade20k_1024.onnx",
        61_277_554,
    )]),
    ("SlimSAM", "Click to select · Apache-2.0 · 40 MB", &[
        ("sam_encoder.onnx", "https://huggingface.co/Xenova/slimsam-77-uniform/resolve/main/onnx/vision_encoder.onnx", 23_276_014),
        ("sam_decoder.onnx", "https://huggingface.co/Xenova/slimsam-77-uniform/resolve/main/onnx/prompt_encoder_mask_decoder.onnx", 16_557_892),
    ]),
    ("IS-Net", "Subject edges and Refine edge · Apache-2.0 · 179 MB", &[(
        "isnet.onnx",
        "https://github.com/danielgatis/rembg/releases/download/v0.0.0/isnet-general-use.onnx",
        178_648_008,
    )]),
    ("YuNet", "Finding faces · MIT · 0.2 MB", &[(
        "face_detection_yunet_2023mar.onnx",
        "https://media.githubusercontent.com/media/opencv/opencv_zoo/main/models/face_detection_yunet/face_detection_yunet_2023mar.onnx",
        232_589,
    )]),
    ("SFace", "Recognising people · Apache-2.0 · 39 MB", &[(
        "face_recognition_sface_2021dec.onnx",
        "https://media.githubusercontent.com/media/opencv/opencv_zoo/main/models/face_recognition_sface/face_recognition_sface_2021dec.onnx",
        38_696_353,
    )]),
    ("PP-ResNet50", "Naming the animal in a subject mask · Apache-2.0 · 103 MB", &[(
        "image_classification_ppresnet50_2022jan.onnx",
        "https://media.githubusercontent.com/media/opencv/opencv_zoo/main/models/image_classification_ppresnet/image_classification_ppresnet50_2022jan.onnx",
        102_567_035,
    )]),

    ("SCUNet", "AI denoise · Apache-2.0 · 77 MB", &[
        ("scunet_color_real_psnr.onnx", "https://huggingface.co/Heliosoph/scunet-onnx/resolve/main/scunet_color_real_psnr.onnx", 3_798_678),
        ("scunet_color_real_psnr.onnx.data", "https://huggingface.co/Heliosoph/scunet-onnx/resolve/main/scunet_color_real_psnr.onnx.data", 73_138_176),
    ]),
];

const MIRROR: &str = "https://github.com/simmmmm/Numa/releases/download/models";

const PROFILES_ARCHIVE: ModelFile = (
    "rawtherapee-dcpprofiles-5.13.tar.gz",
    "https://github.com/simmmmm/Numa/releases/download/models/rawtherapee-dcpprofiles-5.13.tar.gz",
    67_267_725,
);

fn download_dir(file: &str) -> PathBuf {
    if file.ends_with(".tar.gz") { numa::io::data_dir() } else { numa::io::models_dir() }
}

fn model_on_disk((file, url, _): &(&str, &str, u64)) -> Option<PathBuf> {
    if file.ends_with(".tar.gz") {
        let unpacked = dcp::downloaded_profiles_dir();
        return unpacked.is_dir().then_some(unpacked);
    }
    let linked = url.rsplit('/').next().unwrap_or(file);
    numa::io::model_file(&[file, linked])
}

fn profiles_available() -> bool {
    let own = dcp::profiles_dir();
    dcp::search_paths().into_iter().filter(|dir| Some(dir) != own.as_ref()).any(|dir| {
        std::fs::read_dir(dir).is_ok_and(|entries| {
            entries.flatten().any(|entry| entry.path().extension().is_some_and(|ext| ext.eq_ignore_ascii_case("dcp")))
        })
    })
}

fn appimage() -> Option<PathBuf> {
    std::env::var_os("APPIMAGE").map(PathBuf::from).filter(|path| path.is_file())
}

fn menu_entry_files() -> Option<(PathBuf, PathBuf)> {
    let data = dirs::data_dir()?;
    Some((
        data.join("applications/com.tijmen.Numa.desktop"),
        data.join("icons/hicolor/256x256/apps/com.tijmen.Numa.png"),
    ))
}

fn menu_entry_for(path: &std::path::Path, icon: &std::path::Path) -> String {

    let quoted: String = path
        .to_string_lossy()
        .chars()
        .flat_map(|c| match c {
            '"' | '`' | '$' | '\\' => vec!['\\', c],
            c => vec![c],
        })
        .collect();
    include_str!("../../data/com.tijmen.Numa.desktop")
        .lines()
        .map(|line| match line {
            "Exec=numa %F" => format!("TryExec={}\nExec=\"{quoted}\" %F", path.display()),

            "Icon=com.tijmen.Numa" => format!("Icon={}", icon.display()),
            line => line.to_string(),
        })
        .chain(std::iter::once("X-Numa-AppImage=true".to_string()))
        .collect::<Vec<_>>()
        .join("\n")
        + "\n"
}

fn menu_entry_target() -> Option<PathBuf> {
    let (entry, _) = menu_entry_files()?;
    let text = std::fs::read_to_string(entry).ok()?;
    if !text.contains("X-Numa-AppImage=true") {
        return None;
    }
    text.lines().find_map(|line| line.strip_prefix("TryExec=")).map(PathBuf::from)
}

fn write_menu_entry(path: &std::path::Path) -> Result<(), String> {
    let (entry, icon) = menu_entry_files().ok_or("no data folder")?;
    for file in [&entry, &icon] {
        if let Some(dir) = file.parent() {
            std::fs::create_dir_all(dir).map_err(|err| err.to_string())?;
        }
    }
    std::fs::write(&icon, include_bytes!("../../data/icons/hicolor/256x256/apps/com.tijmen.Numa.png"))
        .map_err(|err| err.to_string())?;
    std::fs::write(&entry, menu_entry_for(path, &icon)).map_err(|err| err.to_string())
}

fn remove_menu_entry() {
    if menu_entry_target().is_none() {
        return;
    }
    if let Some((entry, icon)) = menu_entry_files() {
        let _ = std::fs::remove_file(entry);
        let _ = std::fs::remove_file(icon);
    }
}

fn integrate_appimage(state: &App) {
    let Some(path) = appimage() else { return };
    if menu_entry_target().is_some() {

        let current = menu_entry_files().and_then(|(entry, icon)| {
            Some((std::fs::read_to_string(entry).ok()?, menu_entry_for(&path, &icon)))
        });
        if current.is_none_or(|(have, want)| have != want) {
            if let Err(err) = write_menu_entry(&path) {
                log::warn!("could not update the menu entry: {err}");
            }
        }
        return;
    }
    if state.catalog.recall::<bool>(OFFERED_MENU_ENTRY).unwrap_or(false) {
        return;
    }
    state.catalog.remember(OFFERED_MENU_ENTRY, &true);

    let toast = adw::Toast::new("Add Numa to the applications menu?");
    toast.set_button_label(Some("Add"));
    toast.set_timeout(0);
    toast.connect_button_clicked(glib::clone!(
        #[strong] state,
        move |_| match write_menu_entry(&path) {
            Ok(()) => state.toast("Numa is in the applications menu"),
            Err(err) => state.toast(&format!("Could not add the menu entry: {err}")),
        }
    ));
    state.toasts.add_toast(toast);
}

fn offer_additional_files(state: &App, window: &adw::ApplicationWindow) {
    let any = MODELS.iter().any(|(_, _, files)| files.iter().all(|file| model_on_disk(file).is_some()));
    if any || state.catalog.recall::<bool>(OFFERED_MODELS).unwrap_or(false) {
        return;
    }
    state.catalog.remember(OFFERED_MODELS, &true);

    let missing = missing_model_files();
    let megabytes = |files: &[ModelFile]| (files.iter().map(|(_, _, bytes)| bytes).sum::<u64>() + 500_000) / 1_000_000;
    let files_of = |names: &[&str]| -> Vec<ModelFile> {
        MODELS
            .iter()
            .filter(|(name, _, _)| names.contains(name))
            .flat_map(|(_, _, files)| files.iter().copied())
            .collect()
    };

    let dialog = adw::Dialog::new();
    dialog.set_content_width(560);
    let view = adw::ToolbarView::new();
    let header = adw::HeaderBar::new();
    header.set_show_title(false);
    view.add_top_bar(&header);

    let column = gtk::Box::new(gtk::Orientation::Vertical, 18);
    column.set_margin_start(24);
    column.set_margin_end(24);
    column.set_margin_bottom(24);

    let icon = gtk::Image::from_icon_name("folder-download-symbolic");
    icon.set_pixel_size(64);
    icon.add_css_class("dim-label");
    column.append(&icon);
    let title = gtk::Label::new(Some("Additional files"));
    title.add_css_class("title-1");
    column.append(&title);
    let intro = gtk::Label::new(Some(
        "A few features need extra files that are not part of the download. \
         Everything else in Numa works without them.",
    ));
    intro.set_wrap(true);
    intro.set_justify(gtk::Justification::Center);
    column.append(&intro);

    let features = adw::PreferencesGroup::new();
    let mut rows = vec![
        ("weather-few-clouds-symbolic", "Masks that find things for you", "The sky, greenery, buildings, water, people", files_of(&["EfficientViT-Seg B2"])),
        ("input-mouse-symbolic", "Select anything with a click", "Including things no preset has a name for", files_of(&["SlimSAM"])),
        ("edit-cut-symbolic", "Clean edges", "Hair, fur and feathers in a subject mask", files_of(&["IS-Net"])),
        ("system-users-symbolic", "Faces and people", "Face retouching, and browsing by person", files_of(&["YuNet", "SFace"])),
        ("emoji-nature-symbolic", "Animals by name", "A subject mask that says Bird or Dog", files_of(&["PP-ResNet50"])),
        ("image-x-generic-symbolic", "AI denoise", "Clean high-ISO photographs, kept once worked out", files_of(&["SCUNet"])),
    ];

    if missing.contains(&PROFILES_ARCHIVE) {
        rows.push(("camera-photo-symbolic", "Colour for your camera", "Camera profiles for 161 cameras, from RawTherapee", vec![PROFILES_ARCHIVE]));
    }
    for (icon, title, subtitle, files) in rows {
        let row = adw::ActionRow::new();
        row.set_title(title);
        row.set_subtitle(subtitle);
        row.add_prefix(&gtk::Image::from_icon_name(icon));
        let size = gtk::Label::new(Some(&format!("{} MB", megabytes(&files).max(1))));
        size.add_css_class("dim-label");
        size.add_css_class("numeric");
        row.add_suffix(&size);
        features.add(&row);
    }

    let models = adw::ExpanderRow::new();
    models.set_title("What is downloaded");
    models.set_subtitle("Machine-learning models that run on this computer, and camera profiles");
    models.add_prefix(&gtk::Image::from_icon_name("dialog-information-symbolic"));
    for (name, what, _) in MODELS {
        let row = adw::ActionRow::new();
        row.set_title(name);
        row.set_subtitle(what);
        models.add_row(&row);
    }
    let profiles = adw::ActionRow::new();
    profiles.set_title("RawTherapee camera profiles");
    profiles.set_subtitle("Colour for your camera · GPL-3.0 · 67 MB");
    models.add_row(&profiles);
    features.add(&models);
    column.append(&features);

    let buttons = gtk::Box::new(gtk::Orientation::Horizontal, 12);
    buttons.set_halign(gtk::Align::Center);
    let later = gtk::Button::with_label("Not Now");
    later.add_css_class("pill");
    let download = gtk::Button::with_label(&format!("Download {} MB", megabytes(&missing)));
    download.add_css_class("pill");
    download.add_css_class("suggested-action");
    buttons.append(&later);
    buttons.append(&download);
    column.append(&buttons);

    let scroller = gtk::ScrolledWindow::new();
    scroller.set_hscrollbar_policy(gtk::PolicyType::Never);
    scroller.set_propagate_natural_height(true);
    scroller.set_child(Some(&column));
    view.set_content(Some(&scroller));
    dialog.set_child(Some(&view));

    later.connect_clicked(glib::clone!(
        #[weak] dialog,
        move |_| {
            dialog.close();
        }
    ));
    download.connect_clicked(glib::clone!(
        #[strong] state,
        #[weak] dialog,
        move |_| {
            dialog.close();

            let cancel = Cancel::default();
            let (toast, text, bar) = progress_toast(&state, &cancel);
            let progress = move |got: u64, total: u64| {
                text.set_text(&format!(
                    "Downloading additional files — {} of {} MB",
                    got / 1_000_000,
                    total / 1_000_000
                ));
                bar.set_fraction(got as f64 / total.max(1) as f64);
            };
            let state = state.clone();
            download_model(missing.clone(), progress, cancel, move |result| {
                toast.dismiss();
                match result {
                    Ok(()) => state.toast("Additional files installed — restart Numa to use them"),
                    Err(err) if err == "stopped" => {
                        state.toast("Download stopped — continue any time from Preferences")
                    }
                    Err(err) => {
                        log::warn!("downloading the additional files: {err}");
                        state.toast("Some files could not be downloaded — Preferences shows which")
                    }
                }
            });
        }
    ));

    glib::idle_add_local_once(glib::clone!(
        #[weak] window,
        move || dialog.present(Some(&window))
    ));
}

fn download_model(
    files: Vec<ModelFile>,
    progress: impl Fn(u64, u64) + 'static,
    cancel: Cancel,
    done: impl FnOnce(Result<(), String>) + 'static,
) {
    let total: u64 = files.iter().map(|(_, _, bytes)| bytes).sum();
    let running: Rc<RefCell<Option<gio::Subprocess>>> = Rc::default();
    let ticker = {
        let (files, running, cancel) = (files.clone(), running.clone(), cancel.clone());
        glib::timeout_add_local(std::time::Duration::from_millis(250), move || {
            if cancel.stopped() {
                if let Some(process) = running.borrow().as_ref() {
                    process.force_exit();
                }
            }
            let got: u64 = files
                .iter()
                .map(|entry @ (file, _, bytes)| {
                    if model_on_disk(entry).is_some() {
                        return *bytes;
                    }
                    std::fs::metadata(download_dir(file).join(format!("{file}.part"))).map_or(0, |meta| meta.len())
                })
                .sum();
            progress(got.min(total), total);
            glib::ControlFlow::Continue
        })
    };

    glib::spawn_future_local(async move {
        let result = async {
            let mut failed = Vec::new();
            for entry @ (file, url, _) in &files {
                if model_on_disk(entry).is_some() {
                    continue;
                }
                let dir = download_dir(file);
                std::fs::create_dir_all(&dir).map_err(|err| err.to_string())?;
                let part = dir.join(format!("{file}.part"));

                let mirror = format!("{MIRROR}/{file}");
                let mut error = String::new();
                let mut fetched = false;
                let urls = if mirror == *url { vec![*url] } else { vec![mirror.as_str(), *url] };
                for url in urls {
                    if cancel.stopped() {
                        return Err("stopped".to_string());
                    }
                    let finished = run(
                        &running,
                        &["curl".as_ref(), "--fail".as_ref(), "--location".as_ref(), "--retry".as_ref(), "3".as_ref(),
                          "--continue-at".as_ref(), "-".as_ref(), "--output".as_ref(), part.as_os_str(), url.as_ref()],
                    )
                    .await;
                    if cancel.stopped() {
                        return Err("stopped".to_string());
                    }
                    match finished {
                        Ok(()) => {
                            fetched = true;
                            break;
                        }

                        Err(err) => {
                            let _ = std::fs::remove_file(&part);
                            error = err;
                        }
                    }
                }

                if !fetched {
                    failed.push(format!("{file}: {error}"));
                    continue;
                }
                if file.ends_with(".tar.gz") {
                    let unpacked = run(&running, &["tar".as_ref(), "-xzf".as_ref(), part.as_os_str(), "-C".as_ref(), dir.as_os_str()]).await;
                    let _ = std::fs::remove_file(&part);
                    if let Err(err) = unpacked {
                        failed.push(format!("{file}: {err}"));
                    }
                } else {
                    std::fs::rename(&part, dir.join(file)).map_err(|err| err.to_string())?;
                }
            }
            if failed.is_empty() { Ok(()) } else { Err(failed.join("; ")) }
        }
        .await;
        ticker.remove();
        done(result);
    });
}

async fn run(running: &Rc<RefCell<Option<gio::Subprocess>>>, argv: &[&std::ffi::OsStr]) -> Result<(), String> {
    let process = gio::Subprocess::newv(argv, gio::SubprocessFlags::STDERR_PIPE)
        .map_err(|_| format!("{} is not installed", argv[0].to_string_lossy()))?;
    running.replace(Some(process.clone()));
    let finished = process.wait_check_future().await;
    running.replace(None);
    finished.map_err(|err| err.message().to_string())
}

fn missing_model_files() -> Vec<ModelFile> {
    let mut missing: Vec<ModelFile> = MODELS
        .iter()
        .flat_map(|(_, _, files)| files.iter().copied())
        .filter(|file| model_on_disk(file).is_none())
        .collect();
    if !profiles_available() {
        missing.push(PROFILES_ARCHIVE);
    }
    missing
}

fn download_row(
    dialog: &adw::PreferencesDialog,
    name: &'static str,
    what: &'static str,
    files: &'static [ModelFile],
) -> (adw::ActionRow, Option<gtk::Button>) {
    let row = adw::ActionRow::new();
    row.set_title(name);
    row.set_subtitle(what);

    let state = gtk::Stack::new();
    state.set_valign(gtk::Align::Center);
    let installed = gtk::Label::new(Some("Installed"));
    let get = gtk::Button::with_label("Download");
    let bar = gtk::ProgressBar::new();
    bar.set_size_request(120, -1);
    bar.set_valign(gtk::Align::Center);
    state.add_named(&installed, Some("installed"));
    state.add_named(&get, Some("get"));
    state.add_named(&bar, Some("busy"));

    let present = files.iter().all(|file| model_on_disk(file).is_some());
    state.set_visible_child_name(if present { "installed" } else { "get" });

    get.connect_clicked(glib::clone!(
        #[weak] dialog,
        #[weak] state,
        #[weak] bar,
        #[weak] row,
        move |_| {
            state.set_visible_child_name("busy");
            let bar = bar.clone();
            let progress = move |got: u64, total: u64| bar.set_fraction(got as f64 / total.max(1) as f64);
            download_model(files.to_vec(), progress, Cancel::default(), move |result| match result {
                Ok(()) => {
                    state.set_visible_child_name("installed");
                    dialog.add_toast(adw::Toast::new(&format!("{name} installed — restart Numa to use it")));
                }
                Err(err) => {
                    log::warn!("downloading {name} failed: {err}");
                    state.set_visible_child_name("get");
                    row.set_subtitle(&format!("Download failed: {err}"));

                    if let Some((_, url, _)) = files.iter().find(|file| model_on_disk(file).is_none()) {
                        let link = gtk::Button::from_icon_name("web-browser-symbolic");
                        link.set_tooltip_text(Some("Open the download link in a browser"));
                        link.set_valign(gtk::Align::Center);
                        link.add_css_class("flat");
                        let url = url.to_string();
                        link.connect_clicked(move |button| {
                            let window = button.root().and_downcast::<gtk::Window>();
                            gtk::UriLauncher::new(&url).launch(window.as_ref(), None::<&gio::Cancellable>, |_| {});
                        });
                        row.add_suffix(&link);
                    }
                }
            });
        }
    ));
    row.add_suffix(&state);
    (row, (!present).then_some(get))
}

fn preferences_dialog(state: &App, window: &adw::ApplicationWindow) {
    let dialog = adw::PreferencesDialog::new();
    let page = adw::PreferencesPage::new();
    page.set_title("Add-ons and storage");
    page.set_icon_name(Some("folder-symbolic"));

    let updates = adw::PreferencesGroup::new();
    updates.set_title("Updates");
    let check = adw::SwitchRow::new();
    check.set_title("Check for new versions");
    check.set_subtitle("Once a day, from the releases page on GitHub. Nothing about you or your photographs is sent.");
    check.set_active(state.catalog.setting(UPDATE_CHECK).as_deref() == Some("yes"));
    check.connect_active_notify(glib::clone!(
        #[strong] state,
        move |row| {
            let _ = state.catalog.set_setting(UPDATE_CHECK, if row.is_active() { "yes" } else { "no" });
        }
    ));
    updates.add(&check);
    page.add(&updates);

    let folder_row = |title: &str, dir: PathBuf| {
        let row = adw::ActionRow::new();
        row.set_title(title);
        row.set_subtitle(&dir.display().to_string());
        row.set_subtitle_selectable(true);
        let open = gtk::Button::from_icon_name("folder-open-symbolic");
        open.set_tooltip_text(Some("Open in the file manager"));
        open.set_valign(gtk::Align::Center);
        open.add_css_class("flat");
        open.connect_clicked(glib::clone!(
            #[weak] window,
            move |_| {
                if let Err(err) = std::fs::create_dir_all(&dir) {
                    log::warn!("could not create {}: {err}", dir.display());
                }
                gtk::FileLauncher::new(Some(&gio::File::for_path(&dir))).launch(
                    Some(&window),
                    None::<&gio::Cancellable>,
                    |result| {
                        if let Err(err) = result {
                            log::warn!("could not open the folder: {err}");
                        }
                    },
                );
            }
        ));
        row.add_suffix(&open);
        row
    };

    let models_dir = numa::io::models_dir();
    let models = adw::PreferencesGroup::new();
    models.set_title("Models");
    models.set_description(Some(
        "Masks, faces and subject edges use machine-learning models that run on this \
         computer. They are not part of the download: together they are about 500 MB, \
         each comes from its own project under its own licence, and not everyone \
         wants every feature. Download them here, or fetch the files yourself and put \
         them in the models folder. Everything else works without them.",
    ));
    let everything = gtk::Button::with_label("Download all");
    everything.set_valign(gtk::Align::Center);
    models.set_header_suffix(Some(&everything));
    models.add(&folder_row("Models folder", models_dir.clone()));
    let mut downloads: Vec<gtk::Button> = Vec::new();
    for (name, what, files) in MODELS {
        let (row, get) = download_row(&dialog, name, what, files);
        downloads.extend(get);
        models.add(&row);
    }
    everything.set_sensitive(!downloads.is_empty());

    everything.connect_clicked(move |button| {
        button.set_sensitive(false);
        for get in &downloads {
            get.emit_clicked();
        }
    });
    page.add(&models);

    let profiles = adw::PreferencesGroup::new();
    profiles.set_title("Camera profiles");
    profiles.set_description(Some(
        "A camera profile (.dcp) refines the colour of one camera model. Without one, \
         colour comes from the matrix inside the raw file, which is what every raw \
         developer falls back to. RawTherapee's profiles may be shipped and are included \
         where the package allows; Adobe's may not, so profiles from Adobe DNG Converter \
         have to be added here by you. Numa matches a profile on the camera model \
         written inside it, so the file name does not matter.",
    ));
    let (download, _) = download_row(
        &dialog,
        "RawTherapee camera profiles",
        "161 cameras from Canon, Fujifilm, Nikon, Sony and more · GPL-3.0 · 67 MB",
        std::slice::from_ref(&PROFILES_ARCHIVE),
    );
    profiles.add(&download);
    let own = dcp::profiles_dir();
    for dir in dcp::search_paths() {
        let count = std::fs::read_dir(&dir)
            .map(|entries| {
                entries
                    .flatten()
                    .filter(|entry| entry.path().extension().is_some_and(|ext| ext.eq_ignore_ascii_case("dcp")))
                    .count()
            })
            .unwrap_or(0);
        let yours = Some(&dir) == own.as_ref();

        if !yours && count == 0 {
            continue;
        }
        let title = if yours { "Your profiles" } else { "Found profiles" };
        let row = folder_row(title, dir);
        let label = gtk::Label::new(Some(&match count {
            1 => "1 profile".to_string(),
            count => format!("{count} profiles"),
        }));
        label.add_css_class("dim-label");
        row.add_suffix(&label);
        profiles.add(&row);
    }
    page.add(&profiles);

    if let Some(path) = appimage() {
        let menu = adw::PreferencesGroup::new();
        menu.set_title("Applications Menu");
        let row = adw::SwitchRow::new();
        row.set_title("Show Numa in the applications menu");
        row.set_subtitle(&path.display().to_string());
        row.set_active(menu_entry_target().is_some());
        row.connect_active_notify(glib::clone!(
            #[weak] dialog,
            move |row| {
                if row.is_active() {
                    if let Err(err) = write_menu_entry(&path) {
                        dialog.add_toast(adw::Toast::new(&format!("Could not add the menu entry: {err}")));
                        row.set_active(false);
                    }
                } else {
                    remove_menu_entry();
                }
            }
        ));
        menu.add(&row);
        page.add(&menu);
    }

    let storage = adw::PreferencesGroup::new();
    storage.set_title("Storage");
    storage.set_description(Some(
        "Ratings, edits and names are kept in a hidden .numa folder inside each library, \
         so they travel with the photographs. These are Numa's own files on this computer.",
    ));

    for (title, dir) in [
        ("Settings and list of libraries", numa::io::data_dir()),
        ("Models", numa::io::models_dir()),
        ("Presets", numa::io::presets::dir()),
        ("Thumbnails (safe to delete)", numa::io::thumbs::cache_dir()),
    ] {
        let row = folder_row(title, dir.clone());
        let shown = dir.display().to_string();
        glib::spawn_future_local(glib::clone!(
            #[weak] row,
            async move {
                let Ok(bytes) = gio::spawn_blocking(move || folder_size(&dir)).await else { return };
                row.set_subtitle(&format!("{shown} · {}", glib::format_size(bytes)));
            }
        ));
        storage.add(&row);
    }
    page.add(&storage);

    let uninstall = adw::PreferencesGroup::new();
    uninstall.set_title("Removing Numa");
    uninstall.set_description(Some(
        "Delete the AppImage, and the folders above if their contents should go too. \
         Each library keeps its ratings, edits and names in a hidden .numa folder inside it: \
         delete that as well only if that work should go with it. The photographs themselves \
         are never touched.",
    ));
    page.add(&uninstall);

    dialog.add(&page);
    dialog.present(Some(window));
}

fn folder_size(dir: &std::path::Path) -> u64 {
    let mut total = 0;
    let mut pending = vec![dir.to_path_buf()];
    while let Some(dir) = pending.pop() {
        for entry in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
            match entry.metadata() {
                Ok(meta) if meta.is_dir() => pending.push(entry.path()),
                Ok(meta) => total += meta.len(),
                Err(_) => {}
            }
        }
    }
    total
}

fn shortcuts_dialog(window: &adw::ApplicationWindow) {
    let dialog = adw::Dialog::new();
    dialog.set_title("Keyboard shortcuts");
    dialog.set_content_width(420);
    dialog.set_content_height(560);

    let page = adw::PreferencesPage::new();

    let row = |title: &str, keys: &str| {
        let row = adw::ActionRow::new();
        row.set_title(title);
        let label = gtk::Label::new(Some(keys));
        label.add_css_class("dim-label");
        row.add_suffix(&label);
        row
    };

    let app_group = adw::PreferencesGroup::new();
    app_group.set_title("Application");
    app_group.add(&row("Quit", "Ctrl+Q"));
    app_group.add(&row("Keyboard shortcuts", "Ctrl+/"));
    app_group.add(&row("Preferences", "Ctrl+,"));
    page.add(&app_group);

    let library_group = adw::PreferencesGroup::new();
    library_group.set_title("Library");
    library_group.add(&row("Look at one photograph, and put it away again", "Space"));
    library_group.add(&row("Previous / next photograph in the loupe", "Left / Right"));
    library_group.add(&row("Open the photograph in the loupe in the editor", "Enter"));
    library_group.add(&row("Rate the selection 0–5 stars", "0–5"));
    library_group.add(&row("Flag the selection picked", "P"));
    library_group.add(&row("Flag the selection rejected", "X"));
    library_group.add(&row("Clear the selection's flag", "U"));
    library_group.add(&row("Paste copied edits onto the selection", "Ctrl+V"));
    page.add(&library_group);

    let editor_group = adw::PreferencesGroup::new();
    editor_group.set_title("Editor");
    editor_group.add(&row("Previous photo in the filmstrip", "Left / Page Up"));
    editor_group.add(&row("Next photo in the filmstrip", "Right / Page Down"));
    editor_group.add(&row("Rate the open photo 0–5 stars", "0–5"));
    editor_group.add(&row("Flag the open photo picked", "P"));
    editor_group.add(&row("Flag the open photo rejected", "X"));
    editor_group.add(&row("Clear the open photo's flag", "U"));
    editor_group.add(&row("Hold to compare against the as-shot original", "Space"));
    editor_group.add(&row("Show what the camera recorded", "I"));
    editor_group.add(&row("Guides: none, thirds, grid", "G"));
    editor_group.add(&row("Panel tabs, in order", "Alt+1 – Alt+7"));
    editor_group.add(&row("Copy this photograph's edits", "Ctrl+C"));
    editor_group.add(&row("Copy the edited picture", "Ctrl+Shift+C"));
    editor_group.add(&row("Paste edits onto this photograph", "Ctrl+V"));
    editor_group.add(&row("Undo", "Ctrl+Z"));
    editor_group.add(&row("Redo", "Ctrl+Shift+Z"));
    page.add(&editor_group);

    let bar = adw::ToolbarView::new();
    bar.add_top_bar(&adw::HeaderBar::new());
    bar.set_content(Some(&page));
    dialog.set_child(Some(&bar));
    dialog.present(Some(window));
}

fn add_library_dialog(state: &App, window: &adw::ApplicationWindow) {
    let dialog = gtk::FileDialog::new();
    dialog.set_title("Add folder to library");

    let state = state.clone();
    let window = window.clone();
    glib::spawn_future_local(async move {
        let Ok(file) = dialog.select_folder_future(Some(&window)).await else { return };
        let Some(path) = file.path() else { return };

        let added = state
            .catalog
            .add_library(&path)
            .and_then(|library| Ok((state.catalog.sync_library(&library)?, library)));

        match added {
            Ok((count, library)) => {

                state.filter.borrow_mut().in_one_library();
                reload_libraries(&state);
                select_library(&state, library.id);
                state.toast(&match count {
                    0 => format!("No photos found in {}", path.display()),
                    n => format!("Added {n} photo(s) from {}", path.display()),
                });
                if count > 0 {
                    describe_new_library(&state, &window, count);
                }
            }

            Err(err) => {
                let alert = adw::AlertDialog::new(Some("Could Not Add This Folder"), Some(&err));
                alert.add_response("ok", "OK");
                alert.present(Some(&window));
            }
        }
    });
}

fn remember_recent(path: &Path) {
    let uri = gio::File::for_path(path).uri();
    gtk::RecentManager::default().add_item(&uri);
}

fn open_path(state: &App, window: &adw::ApplicationWindow, path: PathBuf) {
    let path = path.canonicalize().unwrap_or(path);
    if !raw::is_supported(&path) {
        state.toast("Numa cannot open that kind of file");
        return;
    }
    let libraries = state.catalog.libraries().unwrap_or_default();

    let holder = libraries
        .iter()
        .filter(|library| path.starts_with(&library.path))
        .max_by_key(|library| library.path.components().count())
        .cloned();

    let show = |state: &App, library: Library, path: &Path| {
        if let Err(err) = state.catalog.sync_library(&library) {
            state.toast(&format!("Could not read the library: {err}"));
            return;
        }

        let sort = state.filter.borrow().sort;
        state.filter.replace(Filter { sort, ..Filter::default() });
        reload_libraries(state);
        select_library(state, library.id);
        let id = state.cards.borrow().iter().find(|(_, (photo, _))| photo.path == path).map(|(id, _)| *id);
        match id {
            Some(id) => {
                remember_recent(path);
                open_photo(state, id);
            }
            None => state.toast("The photograph is not in the library"),
        }
    };

    if let Some(library) = holder {
        show(state, library, &path);
        return;
    }
    let Some(folder) = path.parent().map(Path::to_path_buf) else { return };
    let alert = adw::AlertDialog::new(
        Some("Add this folder as a library?"),
        Some(&format!(
            "Edits are kept in a library, inside its folder, so a photograph is opened as part of one. \
             “{}” is not in any library yet.",
            folder.display()
        )),
    );
    alert.add_response("cancel", "Cancel");
    alert.add_response("add", "Add and Open");
    alert.set_response_appearance("add", adw::ResponseAppearance::Suggested);
    let state = state.clone();
    alert.connect_response(None, move |_, response| {
        if response != "add" {
            return;
        }
        match state.catalog.add_library(&folder) {
            Ok(library) => show(&state, library, &path),
            Err(err) => state.toast(&format!("Could not add the folder: {err}")),
        }
    });
    alert.present(Some(window));
}

const UPDATE_CHECK: &str = "check-for-updates";
const UPDATE_CHECKED_AT: &str = "updates-checked-at";

fn consider_update_check(state: &App, window: &adw::ApplicationWindow) {
    if state.library.borrow().is_none() {
        return;
    }
    match state.catalog.setting(UPDATE_CHECK).as_deref() {
        Some("yes") => check_for_update(state, window),
        Some(_) => {}
        None => {
            let alert = adw::AlertDialog::new(
                Some("Look for new versions?"),
                Some(
                    "The AppImage does not update itself. Numa can look at its releases page \
                     on GitHub once a day and say when there is a new version. Nothing about you \
                     or your photographs is sent. This can be changed in Preferences.",
                ),
            );
            alert.add_response("no", "No");
            alert.add_response("yes", "Check Daily");
            alert.set_response_appearance("yes", adw::ResponseAppearance::Suggested);
            let (state, parent) = (state.clone(), window.clone());
            alert.connect_response(None, move |_, response| {
                let _ = state.catalog.set_setting(UPDATE_CHECK, response);
                if response == "yes" {
                    check_for_update(&state, &parent);
                }
            });
            alert.present(Some(window));
        }
    }
}

fn check_for_update(state: &App, window: &adw::ApplicationWindow) {
    let now = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |d| d.as_secs());
    let last: u64 = state.catalog.setting(UPDATE_CHECKED_AT).and_then(|at| at.parse().ok()).unwrap_or(0);
    if now.saturating_sub(last) < 24 * 3600 {
        return;
    }
    let _ = state.catalog.set_setting(UPDATE_CHECKED_AT, &now.to_string());
    let argv: [&std::ffi::OsStr; 5] =
        ["curl".as_ref(), "--silent".as_ref(), "--location".as_ref(), "--max-time".as_ref(), "15".as_ref()];
    let argv: Vec<&std::ffi::OsStr> = argv.into_iter().chain([numa::io::update::RELEASES.as_ref()]).collect();
    let Ok(process) = gio::Subprocess::newv(&argv, gio::SubprocessFlags::STDOUT_PIPE | gio::SubprocessFlags::STDERR_SILENCE)
    else {
        return;
    };
    let (state, window) = (state.clone(), window.clone());
    glib::spawn_future_local(async move {
        let Ok((Some(json), _)) = process.communicate_utf8_future(None).await else { return };
        let Some((version, page)) = numa::io::update::newer_release(&json, env!("CARGO_PKG_VERSION")) else { return };
        let toast = adw::Toast::new(&format!("Numa {version} is available"));
        toast.set_button_label(Some("What's New"));
        toast.set_timeout(0);
        toast.connect_button_clicked(move |_| {
            gtk::UriLauncher::new(&page).launch(Some(&window), gio::Cancellable::NONE, |_| {});
        });
        state.toasts.add_toast(toast);
    });
}

fn first_time(state: &App, key: &str) -> bool {
    state.catalog.setting(key).is_none()
}

fn mark_seen(state: &App, key: &str) {
    if let Err(err) = state.catalog.set_setting(key, "1") {
        log::warn!("could not remember {key}: {err}");
    }
}

fn key_hint(state: &App, key: &'static str, text: &str) -> adw::Banner {
    let banner = adw::Banner::new(text);
    banner.set_button_label(Some("Got it"));
    banner.set_revealed(first_time(state, key));
    banner.connect_button_clicked(glib::clone!(
        #[strong] state,
        move |banner| {
            banner.set_revealed(false);
            mark_seen(&state, key);
        }
    ));
    banner
}

fn describe_new_library(state: &App, window: &adw::ApplicationWindow, count: usize) {
    let paths: Vec<PathBuf> = state.cards.borrow().values().map(|(photo, _)| photo.path.clone()).take(300).collect();
    let (state, window) = (state.clone(), window.clone());
    glib::spawn_future_local(async move {
        let found = gio::spawn_blocking(move || {
            let mut cameras: std::collections::BTreeMap<String, (usize, bool)> = Default::default();
            let mut unread = 0;
            for path in &paths {
                match numa::io::raw::summary(path) {
                    Some(summary) if numa::io::raw::is_raw(path) => {
                        let name = summary.camera.clone().unwrap_or_else(|| format!("{} {}", summary.make, summary.model));
                        let entry = cameras.entry(name).or_insert_with(|| {
                            (0, numa::io::dcp::find(&summary.make, &summary.model).is_some())
                        });
                        entry.0 += 1;
                    }
                    Some(_) => {}
                    None if numa::io::raw::is_raw(path) => unread += 1,
                    None => {}
                }
            }
            (cameras, unread, paths.len())
        })
        .await;
        let Ok((cameras, unread, sampled)) = found else { return };

        let mut lines = Vec::new();
        for (camera, (_, profiled)) in &cameras {
            lines.push(if *profiled {
                format!("• {camera} — camera profile found")
            } else {
                format!("• {camera} — no camera profile; colour comes from the matrix in the file, which is fine")
            });
        }
        if unread > 0 {
            lines.push(format!("• {unread} RAW file(s) whose details could not be read"));
        }
        let cameras_text = if lines.is_empty() {
            "No RAW files in the first photographs looked at.".to_string()
        } else {
            format!("Cameras, from {sampled} of the {count} photographs:\n{}", lines.join("\n"))
        };
        let body = format!(
            "{cameras_text}\n\nAnalyse measures sharpness, blown highlights, bursts and faces. \
             Suggested ratings, “best of burst” and People depend on it. It runs while you \
             browse and can be stopped; for {count} photographs expect some minutes."
        );
        let alert = adw::AlertDialog::new(Some("What Numa found"), Some(&body));
        alert.add_response("later", "Later");
        alert.add_response("analyse", "Analyse Now");
        alert.set_response_appearance("analyse", adw::ResponseAppearance::Suggested);
        alert.set_default_response(Some("analyse"));
        let state = state.clone();
        alert.connect_response(None, move |_, response| {
            if response == "analyse" {
                if let Some(button) = state.analyse_button.borrow().as_ref() {
                    button.emit_clicked();
                }
            }
        });
        alert.present(Some(&window));
    });
}

fn refresh_folders(state: &App, library: &Library, photos: &[Photo]) {
    let mut folders: std::collections::BTreeSet<PathBuf> = Default::default();
    for photo in photos {
        let Ok(relative) = photo.path.strip_prefix(&library.path) else { continue };
        let mut parent = relative.parent();
        while let Some(folder) = parent.filter(|folder| !folder.as_os_str().is_empty()) {
            folders.insert(folder.to_path_buf());
            parent = folder.parent();
        }
    }
    let folders: Vec<PathBuf> = folders.into_iter().collect();
    if *state.folders.borrow() == folders && !folders.is_empty() {
        return;
    }
    let labels: Vec<String> = std::iter::once("All folders".to_string())
        .chain(folders.iter().map(|folder| {
            let depth = folder.components().count().saturating_sub(1);
            let name = folder.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
            format!("{}{name}", "    ".repeat(depth))
        }))
        .collect();
    let chosen = state.folder.borrow().clone();
    let selected = chosen.and_then(|chosen| folders.iter().position(|folder| *folder == chosen)).map_or(0, |index| index + 1);

    state.applying.set(true);
    let picker = &state.folder_picker;
    picker.set_model(Some(&gtk::StringList::new(&labels.iter().map(String::as_str).collect::<Vec<_>>())));
    picker.set_selected(selected as u32);
    picker.set_visible(!folders.is_empty());
    state.applying.set(false);
    if selected == 0 {
        state.folder.replace(None);
    }
    *state.folders.borrow_mut() = folders;
}

fn rescan_in_background(state: &App) {
    let Some(library) = state.library.borrow().clone() else { return };
    if state.scanning.replace(true) {
        return;
    }
    let state = state.clone();
    glib::spawn_future_local(async move {
        let root = library.path.clone();
        let found = gio::spawn_blocking(move || numa::io::catalog::scan(&root)).await;
        state.scanning.set(false);
        let Ok(found) = found else { return };

        if state.library.borrow().as_ref().map(|open| open.id) != Some(library.id) {
            return;
        }
        let changes = match state.catalog.apply_scan(&library, &found) {
            Ok(changes) => changes,
            Err(err) => {
                log::warn!("rescan of {}: {err}", library.path.display());
                return;
            }
        };
        if !changes.any() {
            return;
        }
        if state.stack.visible_child_name().as_deref() != Some("library") {
            state.grid_stale.set(true);
            return;
        }
        let adjustment = state.grid_scroller.vadjustment();
        let was = adjustment.value();
        reload_grid(&state);
        glib::timeout_add_local_once(std::time::Duration::from_millis(80), move || adjustment.set_value(was));
        if changes.added > 0 {
            state.toast(&match changes.added {
                1 => "1 new photo".to_string(),
                n => format!("{n} new photos"),
            });
        }
    });
}

fn reload_libraries(state: &App) {
    let libraries = state.catalog.libraries().unwrap_or_default();

    let previous = state
        .library
        .borrow()
        .as_ref()
        .map(|library| library.id)
        .or_else(|| state.catalog.setting(LAST_LIBRARY)?.parse().ok());
    let is_empty = libraries.is_empty();

    *state.libraries.borrow_mut() = libraries;

    state.library_picker.set_visible(!is_empty);

    match previous {
        Some(id) => select_library(state, id),
        None => select_library_index(state, 0),
    }
}

fn remember_library(state: &App, library: Option<&Library>) {
    let Some(library) = library else { return };
    if let Err(err) = state.catalog.set_setting(LAST_LIBRARY, &library.id.to_string()) {
        log::warn!("could not remember the library: {err}");
    }
}

fn select_library(state: &App, id: i64) {
    let index = state.libraries.borrow().iter().position(|library| library.id == id);
    select_library_index(state, index.unwrap_or(0) as u32);
}

fn select_library_index(state: &App, index: u32) {
    let selected = state.libraries.borrow().get(index as usize).cloned();
    remember_library(state, selected.as_ref());

    if state.library.borrow().as_ref().map(|library| library.id) != selected.as_ref().map(|library| library.id) {
        state.folder.replace(None);
    }
    *state.library.borrow_mut() = selected;

    reload_grid(state);
}

fn copy_into_library(state: &App, library: Library, dropped: Vec<PathBuf>) {
    let state = state.clone();
    glib::spawn_future_local(async move {
        let folder = library.path.clone();
        let Ok(copied) = busy(&state, "Copying into the library…", move || {
            numa::io::catalog::copy_into(&dropped, &folder)
        })
        .await
        else {
            state.toast("Copying failed — see the log for which file");
            return;
        };

        if let Err(err) = state.catalog.sync_library(&library) {
            state.toast(&format!("Copied, but the library could not be rescanned: {err}"));
            return;
        }

        if state.library.borrow().as_ref().map(|open| open.id) == Some(library.id) {
            reload_grid(&state);
        }

        let mut said = match copied.photos {
            0 => "No photographs copied".to_string(),
            1 => "Copied 1 photograph".to_string(),
            n => format!("Copied {n} photographs"),
        };
        if !copied.existing.is_empty() {
            said.push_str(&format!(" · {} already there, left as they were", copied.existing.len()));
        }
        if !copied.failed.is_empty() {
            said.push_str(&format!(" · {} could not be copied", copied.failed.len()));
        }
        state.toast(&said);
    });
}

fn build_library_page(state: &App, window: &adw::ApplicationWindow) -> gtk::Box {
    let page = gtk::Box::new(gtk::Orientation::Vertical, 0);

    let bar = build_filter_bar(state, window);

    state.welcome.bind_property("visible", &bar, "visible").invert_boolean().sync_create().build();
    page.append(&bar);
    let hint = key_hint(
        state,
        "hint-library-keys",
        "Cull from the keyboard: 0–5 rate, P picks, X rejects, U clears — Ctrl+/ lists every shortcut",
    );
    state.welcome.bind_property("visible", &hint, "visible").invert_boolean().sync_create().build();
    page.append(&hint);

    state.empty.set_vexpand(true);
    page.append(&state.empty);

    let welcome = state.welcome.clone();
    welcome.set_vexpand(true);
    welcome.set_visible(false);
    welcome.set_icon_name(Some("folder-pictures-symbolic"));
    welcome.set_title("Add a Folder of Photographs");
    welcome.set_description(Some(
        "Numa shows your photographs where they are. They are never moved, copied or changed.\n\n\
         Ratings, edits and names are kept in a hidden .numa folder inside the folder you add, \
         so it needs to be a folder you can write to — and they go wherever the folder goes.",
    ));
    let add = gtk::Button::with_label("Add Folder…");
    add.set_halign(gtk::Align::Center);
    add.add_css_class("pill");
    add.add_css_class("suggested-action");
    add.connect_clicked(glib::clone!(
        #[strong] state,
        #[weak] window,
        move |_| add_library_dialog(&state, &window)
    ));
    welcome.set_child(Some(&add));
    page.append(&welcome);

    state.wall.add_css_class("photo-rows");
    state.wall.set_margin_top(12);
    state.wall.set_margin_bottom(12);
    state.wall.set_margin_start(12);
    state.wall.set_margin_end(12);

    let scroller = state.grid_scroller.clone();
    scroller.set_hexpand(true);
    scroller.set_vexpand(true);

    let viewport = gtk::Viewport::new(gtk::Adjustment::NONE, gtk::Adjustment::NONE);
    viewport.set_scroll_to_focus(false);
    viewport.set_child(Some(&state.wall));
    scroller.set_child(Some(&viewport));

    let adjustment = scroller.vadjustment();
    adjustment.connect_value_changed(glib::clone!(
        #[strong] state,
        move |_| schedule_thumbnails(&state)
    ));
    adjustment.connect_changed(glib::clone!(
        #[strong] state,
        move |_| schedule_thumbnails(&state)
    ));
    scroller.connect_map(glib::clone!(
        #[strong] state,
        move |_| schedule_thumbnails(&state)
    ));

    let drop = gtk::DropTarget::new(gtk::gdk::FileList::static_type(), gtk::gdk::DragAction::COPY);
    drop.connect_drop(glib::clone!(
        #[strong] state,
        move |_, value, _, _| {
            let Ok(files) = value.get::<gtk::gdk::FileList>() else { return false };
            let dropped: Vec<PathBuf> = files.files().iter().filter_map(|file| file.path()).collect();
            let Some(library) = state.library.borrow().clone() else { return false };
            if dropped.is_empty() {
                return false;
            }

            if state.filter.borrow().spans_libraries() {
                state.toast("Pick a library to copy these into");
                return false;
            }
            copy_into_library(&state, library, dropped);
            true
        }
    ));

    page.add_controller(drop);

    let over = gtk::Overlay::new();
    over.set_child(Some(&scroller));
    over.add_overlay(&build_loupe(state));
    page.append(&over);

    let loupe_keys = gtk::EventControllerKey::new();
    loupe_keys.set_propagation_phase(gtk::PropagationPhase::Capture);
    loupe_keys.connect_key_pressed(glib::clone!(
        #[strong] state,
        move |_, key, _, _| loupe_key(&state, key)
    ));
    window.add_controller(loupe_keys);

    state.wall.connect_card_activated(glib::clone!(
        #[strong] state,
        move |_, card| open_in_editor(&state, card)
    ));
    install_photo_menu(state, window);

    page
}

fn debug_assert_missing_actions(menu: &gio::Menu, window: &adw::ApplicationWindow) {
    fn walk(model: &gio::MenuModel, into: &mut Vec<String>) {
        for index in 0..model.n_items() {
            if let Some(action) = model
                .item_attribute_value(index, gio::MENU_ATTRIBUTE_ACTION, None)
                .and_then(|value| value.get::<String>())
            {
                into.push(action);
            }
            for link in [gio::MENU_LINK_SECTION, gio::MENU_LINK_SUBMENU] {
                if let Some(child) = model.item_link(index, link) {
                    walk(&child, into);
                }
            }
        }
    }

    let mut named = Vec::new();
    walk(menu.upcast_ref::<gio::MenuModel>(), &mut named);
    for action in named {
        let Some(bare) = action.strip_prefix("win.") else { continue };
        if !window.has_action(bare) {
            log::error!("menu entry points at win.{bare}, which is not a registered action");
        } else if !window.is_action_enabled(bare) {

            log::error!("menu entry win.{bare} exists but is disabled");
        }
    }
}

fn install_photo_menu(state: &App, window: &adw::ApplicationWindow) {
    let menu = gio::Menu::new();
    menu.append(Some("Edit"), Some("win.photo-edit"));

    let rating = gio::Menu::new();
    for stars in 0..=5i32 {
        let label = match stars {
            0 => "No rating".to_string(),
            n => "★".repeat(n as usize),
        };
        let item = gio::MenuItem::new(Some(&label), None);
        item.set_action_and_target_value(Some("win.photo-rate"), Some(&stars.to_variant()));
        rating.append_item(&item);
    }
    menu.append_submenu(Some("Rating"), &rating);

    let flags = gio::Menu::new();
    for (label, which) in [("Pick", "pick"), ("Reject", "reject"), ("Clear flag", "none")] {
        let item = gio::MenuItem::new(Some(label), None);
        item.set_action_and_target_value(Some("win.photo-flag"), Some(&which.to_variant()));
        flags.append_item(&item);
    }
    menu.append_section(None, &flags);

    let settings = gio::Menu::new();
    settings.append(Some("Export…"), Some("win.photo-export"));

    settings.append(Some("Copy settings"), Some("win.photo-copy"));
    settings.append(Some("Paste settings"), Some("win.photo-paste"));
    settings.append(Some("Choose what to paste…"), Some("win.photo-paste-choose"));
    settings.append_submenu(Some("Presets"), &state.presets_menu);
    menu.append_section(None, &settings);
    menu.append_section(None, &state.albums_menu);

    let destructive = gio::Menu::new();
    destructive.append(Some("Move to Trash…"), Some("win.photo-delete"));
    menu.append_section(None, &destructive);

    let edit = gio::SimpleAction::new("photo-edit", None);
    edit.connect_activate(glib::clone!(
        #[strong] state,
        move |_, _| {
            let Some(child) = selected_cards(&state).first().cloned() else { return };
            open_in_editor(&state, &child);
        }
    ));

    let rate = gio::SimpleAction::new("photo-rate", Some(&i32::static_variant_type()));
    rate.connect_activate(glib::clone!(
        #[strong] state,
        move |_, stars| {
            let stars = stars.and_then(|value| value.get::<i32>()).unwrap_or(0);
            rate_here(&state, Action::Rate(stars.clamp(0, 5) as u8));
        }
    ));

    let flag = gio::SimpleAction::new("photo-flag", Some(&String::static_variant_type()));
    flag.connect_activate(glib::clone!(
        #[strong] state,
        move |_, which| {
            let which = which.and_then(|value| value.get::<String>()).unwrap_or_default();
            let flag = match which.as_str() {
                "pick" => Flag::Picked,
                "reject" => Flag::Rejected,
                _ => Flag::None,
            };
            rate_here(&state, Action::Flag(flag));
        }
    ));

    let delete_photos = gio::SimpleAction::new("photo-delete", None);
    delete_photos.connect_activate(glib::clone!(
        #[strong] state,
        #[weak] window,
        move |_, _| confirm_delete(&state, &window)
    ));

    let zoom_action = gio::SimpleAction::new("zoom", Some(&String::static_variant_type()));
    zoom_action.connect_activate(glib::clone!(
        #[strong] state,
        move |_, level| {
            let target = match level.and_then(|value| value.get::<String>()).as_deref() {
                Some("fit") | None => FIT_ZOOM,
                Some(percent) => match percent.parse::<f64>() {
                    Ok(percent) => percent / 100.0,
                    Err(_) => return,
                },
            };
            set_zoom(&state, target);
        }
    ));
    window.add_action(&zoom_action);

    let add_mask_action = gio::SimpleAction::new("add-mask", Some(&String::static_variant_type()));
    add_mask_action.connect_activate(glib::clone!(
        #[strong] state,
        move |_, kind| {
            let kind = match kind.and_then(|value| value.get::<String>()).as_deref() {
                Some("radial") => MaskKind::Radial,
                Some("brush") => MaskKind::Brush,
                Some("click") => MaskKind::Click,
                Some("colour-range") => MaskKind::ColourRange,
                Some("luminance-range") => MaskKind::LuminanceRange,
                _ => MaskKind::Linear,
            };
            add_mask(&state, kind);
        }
    ));
    window.add_action(&add_mask_action);

    let pick = gio::SimpleAction::new("pick-mask", Some(&i32::static_variant_type()));
    pick.connect_activate(glib::clone!(
        #[strong] state,
        move |_, index| {
            let index = index.and_then(|value| value.get::<i32>()).unwrap_or(-1);
            select_mask(&state, usize::try_from(index).ok());
        }
    ));
    window.add_action(&pick);

    let invert = gio::SimpleAction::new("invert-mask", None);
    invert.connect_activate(glib::clone!(
        #[strong] state,
        move |_, _| {
            let Some(index) = state.selected_mask.get() else { return };
            let inverted = mask_at(&state, index).is_some_and(|mask| mask.inverted);
            set_mask_inverted(&state, index, !inverted);
        }
    ));
    window.add_action(&invert);

    let delete_mask = gio::SimpleAction::new("delete-mask", None);
    delete_mask.connect_activate(glib::clone!(
        #[strong] state,
        move |_, _| {
            let Some(index) = state.selected_mask.get() else { return };
            remove_mask(&state, index);
        }
    ));
    window.add_action(&delete_mask);

    let export = gio::SimpleAction::new("photo-export", None);
    export.connect_activate(glib::clone!(
        #[strong] state,
        #[weak] window,
        move |_, _| export_selection(&state, &window)
    ));
    window.add_action(&export);

    let copy = gio::SimpleAction::new("photo-copy", None);
    copy.connect_activate(glib::clone!(
        #[strong] state,
        move |_, _| copy_from_selection(&state)
    ));
    window.add_action(&copy);

    for (name, ask) in [("photo-paste", false), ("photo-paste-choose", true)] {
        let paste = gio::SimpleAction::new(name, None);
        paste.connect_activate(glib::clone!(
            #[strong] state,
            #[weak] window,
            move |_, _| paste_settings(&state, &window, ask)
        ));
        window.add_action(&paste);
    }

    for action in [&edit, &rate, &flag, &delete_photos] {
        window.add_action(action);
    }
    install_preset_actions(state, window);

    debug_assert_missing_actions(&menu, window);

    let rows_popover = gtk::PopoverMenu::from_model(Some(&menu));
    rows_popover.set_parent(&state.wall);
    rows_popover.set_has_arrow(false);
    rows_popover.set_halign(gtk::Align::Start);
    let click = gtk::GestureClick::new();
    click.set_button(gtk::gdk::BUTTON_SECONDARY);
    click.connect_pressed(glib::clone!(
        #[strong] state,
        #[weak] rows_popover,
        move |_, _, x, y| {
            let Some(card) = state.wall.card_at(x, y) else { return };
            if !state.wall.is_selected(&card) {
                state.wall.select_only(&card);
            }
            rows_popover.set_pointing_to(Some(&gtk::gdk::Rectangle::new(x as i32, y as i32, 1, 1)));
            rows_popover.popup();
        }
    ));
    state.wall.add_controller(click);
}

fn selected_cards(state: &App) -> Vec<gtk::Widget> {
    state.wall.selected()
}

fn confirm_delete(state: &App, window: &adw::ApplicationWindow) {
    let selected = selected_cards(state);
    if selected.is_empty() {
        state.toast("Select a photo first");
        return;
    }

    let doomed: Vec<(i64, PathBuf)> = {
        let cards = state.cards.borrow();
        selected
            .iter()
            .filter_map(|child| child.widget_name().parse::<i64>().ok())
            .filter_map(|id| cards.get(&id).map(|(photo, _)| (id, photo.path.clone())))
            .collect()
    };
    if doomed.is_empty() {
        return;
    }

    let title = match doomed.as_slice() {
        [(_, path)] => format!(
            "Move {} to the trash?",
            path.file_name().unwrap_or_default().to_string_lossy()
        ),
        many => format!("Move {} photographs to the trash?", many.len()),
    };

    let dialog = adw::AlertDialog::new(
        Some(&title),
        Some(
            "The file goes to your desktop's trash, where it can be put back.              Its rating, flag and edits are forgotten here and those do not come back.",
        ),
    );
    dialog.add_response("cancel", "Cancel");
    dialog.add_response("trash", "Move to Trash");
    dialog.set_response_appearance("trash", adw::ResponseAppearance::Destructive);
    dialog.set_default_response(Some("cancel"));
    dialog.set_close_response("cancel");

    let state = state.clone();
    dialog.connect_response(None, move |dialog, response| {
        dialog.close();
        if response != "trash" {
            return;
        }

        let mut trashed = 0usize;
        let mut failures = Vec::new();
        for (id, path) in &doomed {

            match gio::File::for_path(path).trash(gio::Cancellable::NONE) {
                Ok(()) => {
                    if let Err(err) = state.catalog.remove_photo(*id) {
                        log::warn!("{}: {err}", path.display());
                    }
                    trashed += 1;
                }
                Err(err) => failures.push(format!("{}: {err}", path.display())),
            }
        }

        reload_grid(&state);
        state.toast(&match failures.as_slice() {
            [] => format!("Moved {trashed} photo(s) to the trash"),
            [only] => format!("Could not delete — {only}"),
            many => format!("Moved {trashed}, could not delete {}", many.len()),
        });
        for failure in &failures {
            log::warn!("{failure}");
        }
    });

    dialog.present(Some(window));
}

fn build_mask_overlay(state: &App) -> gtk::DrawingArea {
    let area = state.mask_area.clone();
    area.set_visible(false);
    area.set_can_target(true);

    let grabbed: Rc<RefCell<Option<(Handle, Shape, f32, f32)>>> = Rc::new(RefCell::new(None));

    let painting: Rc<RefCell<Option<Stroke>>> = Rc::new(RefCell::new(None));

    area.set_draw_func(glib::clone!(
        #[strong] state,
        #[strong] painting,
        move |_, context, width, height| {
            let content = content_rect(&state, width as f64, height as f64);
            let Some(mask) = selected_mask(&state) else {

                if state.previewing.get() && state.show_ants.get() {
                    draw_ants(context, content, &state.mask_outline.borrow(), state.ants_phase.get());
                }
                return;
            };

            if state.show_coverage.get() && mask.visible {
                if let Some(surface) = state.mask_wash.borrow().as_ref() {
                    draw_coverage(context, content, surface, state.mask_wash_size.get());
                }
            }

            if state.show_ants.get() {
                draw_ants(
                    context,
                    content,
                    &state.mask_outline.borrow(),
                    state.ants_phase.get(),
                );
            }

            draw_mask(context, content, &mask.shape);

            if state.show_dots.get() {
                draw_dots(context, content, &state.mask_dot_cache.borrow());
            }

            match (state.brush.get(), painting.borrow().as_ref()) {

                (MaskTool::Lasso, Some(stroke)) if stroke.fill => {
                    draw_lasso(context, content, &stroke.points, stroke.erase)
                }
                (MaskTool::Brush, painting) => {

                    if let Some(stroke) = painting.filter(|_| !state.show_coverage.get()) {
                        draw_stroke(context, content, stroke);
                    }
                    draw_brush(context, content, &state)
                }
                _ => {}
            }
        }
    ));

    let moved: Rc<Cell<bool>> = Rc::new(Cell::new(false));
    let drag = gtk::GestureDrag::new();
    drag.connect_drag_begin(glib::clone!(
        #[strong] state,
        #[strong] moved,
        #[strong] grabbed,
        #[strong] painting,
        move |gesture, x, y| {
            moved.set(false);
            let Some(mask) = selected_mask(&state) else { return };
            let Some((u, v)) = mask_point(&state, x, y) else { return };

            let claim = |gesture: &gtk::GestureDrag| {
                gesture.set_state(gtk::EventSequenceState::Claimed);
            };

            let tool = state.brush.get();
            if tool == MaskTool::Off {

                if !matches!(mask.shape, Shape::Linear { .. } | Shape::Radial { .. }) {
                    return;
                }
                let handle = nearest_mask_handle(&mask.shape, u, v);
                *grabbed.borrow_mut() = Some((handle, mask.shape.clone(), u, v));
                claim(gesture);
                return;
            }
            claim(gesture);
            let away = taking_away(gesture.current_event_state());

            if mask.map.0.is_none() {
                if let Some(index) = state.selected_mask.get() {
                    rebuild_mask_map(&state, index);
                }
            }

            let mut stroke = match tool {
                MaskTool::Lasso => Stroke::lasso(away),
                _ => Stroke::new(state.brush_radius.get(), BRUSH_FEATHER, away),
            };
            stroke.points.push([u, v]);
            if !stroke.fill {
                paint_segment(&state, [u, v], [u, v], &stroke);
            }
            *painting.borrow_mut() = Some(stroke);
        }
    ));
    drag.connect_drag_update(glib::clone!(
        #[strong] state,
        #[strong] moved,
        #[strong] grabbed,
        #[strong] painting,
        move |gesture, dx, dy| {
            moved.set(true);
            let Some((start_x, start_y)) = gesture.start_point() else { return };
            let Some((u, v)) = mask_point(&state, start_x + dx, start_y + dy) else { return };

            if let Some(stroke) = painting.borrow_mut().as_mut() {
                let last = *stroke.points.last().unwrap_or(&[u, v]);

                stroke.points.push([u, v]);
                if stroke.fill {

                    state.mask_area.queue_draw();
                } else {
                    paint_segment(&state, last, [u, v], stroke);
                }
                return;
            }

            let Some((handle, original, from_u, from_v)) = grabbed.borrow().clone() else { return };
            let moved = move_mask_handle(original, handle, [u - from_u, v - from_v], [u, v]);
            update_mask_shape(&state, moved);
        }
    ));
    drag.connect_drag_end(glib::clone!(
        #[strong] grabbed,
        #[strong] state,
        #[strong] painting,
        move |_, _, _| {
            let dragged = grabbed.borrow_mut().take().is_some();

            let finished = painting.borrow_mut().take();

            if let Some(stroke) = finished {
                if let Some(index) = state.selected_mask.get() {
                    let filled = stroke.fill;
                    {
                        let mut open = state.open.borrow_mut();
                        if let Some(photo) = open.as_mut() {
                            if let Some(mask) = photo.document.mask_mut(index) {
                                mask.strokes.push(stroke);
                            }
                        }
                    }

                    if filled {
                        rebuild_mask_map(&state, index);
                    } else {

                        refresh_outline(&state);
                    }
                    request_render(&state);
                    show_coverage(&state);
                }
                refresh_masks(&state);
            }

            if dragged {
                refresh_outline(&state);
            }
            schedule_history_push(&state);
        }
    ));
    area.add_controller(drag);

    let motion = gtk::EventControllerMotion::new();
    motion.connect_motion(glib::clone!(
        #[strong] state,
        move |_, x, y| {
            if state.brush.get() == MaskTool::Off {
                return;
            }
            state.brush_at.set(Some((x as f32, y as f32)));
            state.mask_area.queue_draw();
        }
    ));
    motion.connect_leave(glib::clone!(
        #[strong] state,
        move |_| {
            state.brush_at.set(None);
            state.mask_area.queue_draw();
        }
    ));
    area.add_controller(motion);

    let click = gtk::GestureClick::new();
    click.connect_released(glib::clone!(
        #[strong] state,
        #[strong] moved,
        move |gesture, _, x, y| {

            if state.brush.get() != MaskTool::Off || moved.get() {
                return;
            }
            let Some(mask) = selected_mask(&state) else { return };
            let Some((u, v)) = mask_point(&state, x, y) else { return };

            if is_gradient(&mask) {
                return;
            }

            if matches!(mask.shape, Shape::ColourRange { .. } | Shape::LuminanceRange { .. }) {
                pick_range(&state, u, v);
                return;
            }

            if state.show_dots.get() {
                let content = content_rect(
                    &state,
                    state.mask_area.width() as f64,
                    state.mask_area.height() as f64,
                );
                let (left, top, width, height) = content;
                let index = state.selected_mask.get().unwrap_or(0);
                for (at, _, part) in mask_dots(&state, &mask) {
                    let dx = left + at[0] as f64 * width - x;
                    let dy = top + at[1] as f64 * height - y;
                    if dx.hypot(dy) <= DOT_REACH {
                        toggle_mask_part(&state, index, part);
                        return;
                    }
                }
            }

            if (0.0..=1.0).contains(&u) && (0.0..=1.0).contains(&v) {
                point_at(&state, u, v, taking_away(gesture.current_event_state()));
            }
        }
    ));
    area.add_controller(click);

    area
}

fn is_gradient(mask: &Mask) -> bool {
    matches!(mask.shape, Shape::Linear { .. } | Shape::Radial { .. })
        && mask.strokes.is_empty()
        && mask.points.is_empty()
}

fn mask_name(mask: &Mask) -> String {

    if let Some(name) = mask.name.as_ref().filter(|name| !name.trim().is_empty()) {
        return name.trim().to_string();
    }
    match &mask.shape {
        Shape::Linear { .. } => "Linear".to_string(),
        Shape::Radial { .. } => "Radial".to_string(),
        Shape::Segment { classes } => segment::name_for(classes),

        Shape::Painted if mask.strokes.is_empty() && !mask.points.is_empty() => "Click".to_string(),
        Shape::Painted => "Brush".to_string(),
        Shape::ColourRange { .. } => "Colour range".to_string(),
        Shape::LuminanceRange { .. } => "Luminance range".to_string(),
    }
}

fn mask_label(masks: &[Mask], index: usize) -> String {
    let name = mask_name(&masks[index]);
    let same: Vec<usize> =
        (0..masks.len()).filter(|other| mask_name(&masks[*other]) == name).collect();
    match same.len() > 1 {
        true => {
            format!("{name} {}", same.iter().position(|other| *other == index).unwrap_or(0) + 1)
        }
        false => name,
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Handle {
    Start,
    End,
    Centre,
    EdgeX,
    EdgeY,
    Whole,
}

const MASK_GRAB: f32 = 0.05;

fn nearest_mask_handle(shape: &Shape, u: f32, v: f32) -> Handle {
    let near = |point: [f32; 2]| {
        let (dx, dy) = (u - point[0], v - point[1]);
        (dx * dx + dy * dy).sqrt() <= MASK_GRAB
    };

    match shape {
        Shape::Linear { from, to } => {
            if near(*from) {
                Handle::Start
            } else if near(*to) {
                Handle::End
            } else {
                Handle::Whole
            }
        }

        Shape::Segment { .. }
        | Shape::Painted
        | Shape::ColourRange { .. }
        | Shape::LuminanceRange { .. } => Handle::Whole,
        Shape::Radial { centre, radius, .. } => {
            if near(*centre) {
                Handle::Centre
            } else if near([centre[0] + radius[0], centre[1]]) {
                Handle::EdgeX
            } else if near([centre[0], centre[1] + radius[1]]) {
                Handle::EdgeY
            } else {
                Handle::Whole
            }
        }
    }
}

fn move_mask_handle(shape: Shape, handle: Handle, shift: [f32; 2], at: [f32; 2]) -> Shape {
    let clamp = |point: [f32; 2]| [point[0].clamp(-0.5, 1.5), point[1].clamp(-0.5, 1.5)];

    match (shape, handle) {

        (shape @ (Shape::ColourRange { .. } | Shape::LuminanceRange { .. }), _) => shape,

        (Shape::Linear { to, .. }, Handle::Start) => Shape::Linear { from: clamp(at), to },
        (Shape::Linear { from, .. }, Handle::End) => Shape::Linear { from, to: clamp(at) },
        (Shape::Linear { from, to }, _) => Shape::Linear {
            from: clamp([from[0] + shift[0], from[1] + shift[1]]),
            to: clamp([to[0] + shift[0], to[1] + shift[1]]),
        },

        (Shape::Radial { radius, feather, .. }, Handle::Centre) => {
            Shape::Radial { centre: clamp(at), radius, feather }
        }
        (Shape::Radial { centre, radius, feather }, Handle::EdgeX) => Shape::Radial {
            centre,

            radius: [(at[0] - centre[0]).abs().max(0.01), radius[1]],
            feather,
        },
        (Shape::Radial { centre, radius, feather }, Handle::EdgeY) => Shape::Radial {
            centre,
            radius: [radius[0], (at[1] - centre[1]).abs().max(0.01)],
            feather,
        },
        (Shape::Radial { centre, radius, feather }, _) => Shape::Radial {
            centre: clamp([centre[0] + shift[0], centre[1] + shift[1]]),
            radius,
            feather,
        },

        (shape @ (Shape::Segment { .. } | Shape::Painted), _) => shape,
    }
}

fn selected_mask(state: &App) -> Option<Mask> {
    let index = state.selected_mask.get()?;
    let open = state.open.borrow();
    open.as_ref()?.document.masks().get(index).cloned()
}

fn canvas_point(state: &App, x: f64, y: f64) -> Option<(f32, f32)> {
    let (left, top, width, height) =
        content_rect(state, state.canvas.width() as f64, state.canvas.height() as f64);
    (width > 0.0 && height > 0.0)
        .then(|| (((x - left) / width) as f32, ((y - top) / height) as f32))
}

fn mask_point(state: &App, x: f64, y: f64) -> Option<(f32, f32)> {
    let (left, top, width, height) = content_rect(
        state,
        state.mask_area.width() as f64,
        state.mask_area.height() as f64,
    );
    (width > 0.0 && height > 0.0)
        .then(|| (((x - left) / width) as f32, ((y - top) / height) as f32))
}

fn update_mask_shape(state: &App, shape: Shape) {
    let Some(index) = state.selected_mask.get() else { return };
    let painted = {
        let mut open = state.open.borrow_mut();
        let Some(photo) = open.as_mut() else { return };
        let Some(mask) = photo.document.mask_mut(index) else { return };
        mask.shape = shape;
        photo.view = None;
        mask.wants_pixels()
    };

    if painted {
        rebuild_mask_map(state, index);
    }

    state.mask_area.queue_draw();
    request_render(state);
}

fn mask_dots(state: &App, mask: &Mask) -> Vec<([f32; 2], bool, MaskPart)> {
    let mut dots = Vec::new();

    if let Shape::Segment { classes } = &mask.shape {
        let found = state.open.borrow().as_ref().and_then(|photo| photo.segmentation.clone());
        if let Some(found) = found {
            for class in classes {
                if let Some(at) = found.centre_of(*class) {
                    dots.push((at, !mask.muted.contains(class), MaskPart::Class(*class)));
                }
            }
        }
    }

    for (index, point) in mask.points.iter().enumerate() {
        dots.push((point.at, point.enabled, MaskPart::Point(index)));
    }

    dots
}

const DOT_REACH: f64 = 13.0;

fn draw_dots(
    context: &gtk::cairo::Context,
    content: (f64, f64, f64, f64),
    dots: &[([f32; 2], bool, MaskPart)],
) {
    let (left, top, width, height) = content;
    for (at, on, _) in dots {
        let (x, y) = (left + at[0] as f64 * width, top + at[1] as f64 * height);

        context.arc(x, y, 7.0, 0.0, std::f64::consts::TAU);
        context.set_source_rgba(0.0, 0.0, 0.0, 0.55);
        context.set_line_width(3.5);
        let _ = context.stroke();

        context.arc(x, y, 7.0, 0.0, std::f64::consts::TAU);
        if *on {
            context.set_source_rgba(1.0, 1.0, 1.0, 0.95);
        } else {
            context.set_source_rgba(1.0, 1.0, 1.0, 0.45);
        }
        context.set_line_width(1.6);
        let _ = context.stroke();

        if *on {

            context.arc(x, y, 3.0, 0.0, std::f64::consts::TAU);
            context.set_source_rgba(1.0, 1.0, 1.0, 0.95);
            let _ = context.fill();
        }
    }
}

fn draw_ants(
    context: &gtk::cairo::Context,
    content: (f64, f64, f64, f64),
    paths: &[Vec<[f32; 2]>],
    phase: f64,
) {
    if paths.is_empty() {
        return;
    }
    let (left, top, width, height) = content;
    const PERIOD: f64 = 12.0;

    for (colour, offset) in [((0.0, 0.0, 0.0, 0.85), 0.0), ((1.0, 1.0, 1.0, 0.95), PERIOD / 2.0)] {
        context.set_source_rgba(colour.0, colour.1, colour.2, colour.3);
        context.set_line_width(1.5);
        context.set_dash(&[PERIOD / 2.0, PERIOD / 2.0], phase + offset);
        for path in paths {
            let mut points = path.iter();
            let Some(first) = points.next() else { continue };
            context.move_to(left + first[0] as f64 * width, top + first[1] as f64 * height);
            for at in points {
                context.line_to(left + at[0] as f64 * width, top + at[1] as f64 * height);
            }
            let _ = context.stroke();
        }
    }
    context.set_dash(&[], 0.0);
}

const OUTLINE_EDGE: usize = 640;

fn gradient_alpha(state: &App, mask: &Mask) -> Option<std::sync::Arc<Alpha>> {
    if mask.map.0.is_some() || !matches!(mask.shape, Shape::Linear { .. } | Shape::Radial { .. }) {
        return None;
    }
    let (width, height) = mask_raster_size(state);
    if width == 0 || height == 0 {
        return None;
    }
    let data = (0..width * height)
        .map(|index| {
            let u = ((index % width) as f32 + 0.5) / width as f32;
            let v = ((index / width) as f32 + 0.5) / height as f32;
            mask.shape.weight(u, v)
        })
        .collect();
    Some(std::sync::Arc::new(Alpha::new(width, height, data)))
}

fn refresh_outline(state: &App) {
    let selected = selected_mask(state);
    let made = selected.as_ref().and_then(|mask| gradient_alpha(state, mask));
    let traced = selected
        .as_ref()
        .and_then(|mask| {
            let visible = mask.visible;
            mask.map.0.as_ref().or(made.as_ref()).filter(|_| visible).map(|alpha| {
                let mut paths = numa::core::mask::outline(alpha, OUTLINE_EDGE);

                if paths.len() > 400 {
                    paths.truncate(400);
                }
                paths
            })
        })
        .unwrap_or_default();

    *state.mask_outline.borrow_mut() = traced;

    let wash = selected.as_ref().and_then(|mask| {
        let alpha = mask.map.0.as_ref().or(made.as_ref())?;
        state.mask_wash_size.set((alpha.width, alpha.height));
        build_wash(alpha, mask.inverted, mask.opacity)
    });
    *state.mask_wash.borrow_mut() = wash;
    *state.mask_dot_cache.borrow_mut() = match &selected {
        Some(mask) => mask_dots(state, mask),
        None => Vec::new(),
    };

    start_ants(state);
    state.mask_area.queue_draw();
}

fn start_ants(state: &App) {
    if state.ants_running.get() || state.mask_outline.borrow().is_empty() {
        return;
    }
    state.ants_running.set(true);

    let state = state.clone();
    state.mask_area.clone().add_tick_callback(move |_, clock| {
        if state.mask_outline.borrow().is_empty() || !state.mask_area.is_visible() {
            state.ants_running.set(false);
            return glib::ControlFlow::Break;
        }

        let seconds = clock.frame_time() as f64 / 1_000_000.0;
        state.ants_phase.set(-(seconds * 8.0) % 12.0);
        state.mask_area.queue_draw();
        glib::ControlFlow::Continue
    });
}

fn draw_brush(context: &gtk::cairo::Context, content: (f64, f64, f64, f64), state: &App) {
    let Some((x, y)) = state.brush_at.get() else { return };
    let (_, _, width, height) = content;
    let radius = state.brush_radius.get() as f64 * width.max(height);

    context.arc(x as f64, y as f64, radius.max(1.0), 0.0, std::f64::consts::TAU);

    context.set_line_width(3.0);
    context.set_source_rgba(0.0, 0.0, 0.0, 0.5);
    let _ = context.stroke_preserve();
    context.set_line_width(1.2);
    context.set_source_rgba(1.0, 1.0, 1.0, 0.95);
    let _ = context.stroke();
}

fn draw_lasso(
    context: &gtk::cairo::Context,
    content: (f64, f64, f64, f64),
    points: &[[f32; 2]],
    taking_away: bool,
) {
    let (left, top, width, height) = content;
    let Some(first) = points.first() else { return };

    context.move_to(left + first[0] as f64 * width, top + first[1] as f64 * height);
    for point in &points[1..] {
        context.line_to(left + point[0] as f64 * width, top + point[1] as f64 * height);
    }
    context.close_path();

    context.set_line_width(3.0);
    context.set_source_rgba(0.0, 0.0, 0.0, 0.5);
    let _ = context.stroke_preserve();
    context.set_line_width(1.2);
    if taking_away {
        context.set_source_rgba(1.0, 0.45, 0.4, 0.95);
    } else {
        context.set_source_rgba(1.0, 1.0, 1.0, 0.95);
    }
    let _ = context.stroke();
}

fn draw_stroke(context: &gtk::cairo::Context, content: (f64, f64, f64, f64), stroke: &Stroke) {
    let (left, top, width, height) = content;
    let long = width.max(height);
    context.set_line_width((stroke.radius as f64 * 2.0 * long).max(1.0));
    context.set_line_cap(gtk::cairo::LineCap::Round);
    context.set_line_join(gtk::cairo::LineJoin::Round);

    if stroke.erase {
        context.set_source_rgba(1.0, 1.0, 1.0, 0.35);
    } else {
        context.set_source_rgba(0.95, 0.3, 0.3, 0.4);
    }

    let mut points = stroke.points.iter();
    let Some(first) = points.next() else { return };
    context.move_to(left + first[0] as f64 * width, top + first[1] as f64 * height);
    for at in points {
        context.line_to(left + at[0] as f64 * width, top + at[1] as f64 * height);
    }

    if stroke.points.len() == 1 {
        context.line_to(left + first[0] as f64 * width + 0.01, top + first[1] as f64 * height);
    }
    let _ = context.stroke();
}

fn build_wash(alpha: &Alpha, inverted: bool, opacity: f32) -> Option<gtk::cairo::ImageSurface> {
    if alpha.width == 0 || alpha.height == 0 {
        return None;
    }
    let mut surface = gtk::cairo::ImageSurface::create(
        gtk::cairo::Format::A8,
        alpha.width as i32,
        alpha.height as i32,
    )
    .ok()?;
    paint_wash(&mut surface, alpha, inverted, opacity, (0, 0, alpha.width, alpha.height));
    Some(surface)
}

fn paint_wash(
    surface: &mut gtk::cairo::ImageSurface,
    alpha: &Alpha,
    inverted: bool,
    opacity: f32,
    bounds: (usize, usize, usize, usize),
) {
    let stride = surface.stride() as usize;
    let Ok(mut pixels) = surface.data() else { return };
    let opacity = opacity.clamp(0.0, 1.0);
    let (left, top, right, bottom) =
        (bounds.0, bounds.1, bounds.2.min(alpha.width), bounds.3.min(alpha.height));
    for y in top..bottom {
        for x in left..right {
            let value = alpha.data[y * alpha.width + x];
            let value = if inverted { 1.0 - value } else { value };

            let value = value * opacity;
            pixels[y * stride + x] = (value * 255.0).clamp(0.0, 255.0) as u8;
        }
    }
}

fn draw_coverage(
    context: &gtk::cairo::Context,
    content: (f64, f64, f64, f64),
    surface: &gtk::cairo::ImageSurface,
    size: (usize, usize),
) {
    let (left, top, width, height) = content;
    if size.0 == 0 || size.1 == 0 {
        return;
    }

    let _ = context.save();
    context.translate(left, top);
    context.scale(width / size.0 as f64, height / size.1 as f64);

    context.set_source_rgba(0.24, 0.52, 1.0, 0.38);
    let _ = context.mask_surface(surface, 0.0, 0.0);
    let _ = context.restore();
}

fn draw_mask(context: &gtk::cairo::Context, content: (f64, f64, f64, f64), shape: &Shape) {
    let (left, top, width, height) = content;
    let at = |point: [f32; 2]| (left + point[0] as f64 * width, top + point[1] as f64 * height);

    context.set_line_width(1.5);
    context.set_source_rgba(1.0, 1.0, 1.0, 0.85);

    let handles: Vec<(f64, f64)> = match shape {

        Shape::Segment { .. }
        | Shape::Painted
        | Shape::ColourRange { .. }
        | Shape::LuminanceRange { .. } => Vec::new(),
        Shape::Linear { from, to } => {
            let (x0, y0) = at(*from);
            let (x1, y1) = at(*to);

            context.move_to(x0, y0);
            context.line_to(x1, y1);
            let _ = context.stroke();

            let (dx, dy) = (x1 - x0, y1 - y0);
            let length = (dx * dx + dy * dy).sqrt().max(1.0);
            let (across_x, across_y) = (-dy / length * 400.0, dx / length * 400.0);
            context.set_source_rgba(1.0, 1.0, 1.0, 0.35);
            for (x, y) in [(x0, y0), (x1, y1)] {
                context.move_to(x - across_x, y - across_y);
                context.line_to(x + across_x, y + across_y);
            }
            let _ = context.stroke();

            vec![(x0, y0), (x1, y1)]
        }
        Shape::Radial { centre, radius, feather } => {
            let (cx, cy) = at(*centre);
            let (rx, ry) = (*radius as [f32; 2]).map(f64::from).into();
            let (rx, ry) = (rx * width, ry * height);

            let ellipse = |context: &gtk::cairo::Context, scale: f64| {
                let _ = context.save();
                context.translate(cx, cy);
                context.scale((rx * scale).max(0.1), (ry * scale).max(0.1));
                context.arc(0.0, 0.0, 1.0, 0.0, std::f64::consts::TAU);
                let _ = context.restore();
                let _ = context.stroke();
            };

            ellipse(context, 1.0);

            context.set_source_rgba(1.0, 1.0, 1.0, 0.35);
            ellipse(context, (1.0 - *feather as f64).max(0.05));

            vec![(cx, cy), (cx + rx, cy), (cx, cy + ry)]
        }
    };

    context.set_source_rgb(1.0, 1.0, 1.0);
    for (x, y) in handles {
        context.arc(x, y, 5.0, 0.0, std::f64::consts::TAU);
        let _ = context.fill();
    }
}

fn face_icon() -> gtk::DrawingArea {
    let area = gtk::DrawingArea::new();
    area.set_content_width(16);
    area.set_content_height(16);

    area.set_draw_func(|area, context, width, height| {
        let colour = area.color();
        context.set_source_rgba(
            colour.red() as f64,
            colour.green() as f64,
            colour.blue() as f64,
            colour.alpha() as f64,
        );
        context.scale((width as f64).min(height as f64) / 16.0, (height as f64).min(width as f64) / 16.0);
        paint_face(context);
    });

    area
}

fn paint_face(context: &gtk::cairo::Context) {
    use std::f64::consts::{PI, TAU};

    context.set_line_width(1.0);
    context.set_line_cap(gtk::cairo::LineCap::Round);

    context.arc(6.5, 9.5, 5.0, 0.0, TAU);
    let _ = context.stroke();

    for x in [4.5, 8.5] {
        context.arc(x, 8.0, 0.7, 0.0, TAU);
        let _ = context.fill();
    }

    context.arc(6.5, 9.0, 2.9, 0.5, PI - 0.5);
    let _ = context.stroke();

    let (cx, cy, long, short) = (13.0, 3.0, 3.0, 0.9);
    for point in 0..8 {
        let angle = point as f64 * std::f64::consts::FRAC_PI_4 - std::f64::consts::FRAC_PI_2;
        let reach = if point % 2 == 0 { long } else { short };
        let (x, y) = (cx + angle.cos() * reach, cy + angle.sin() * reach);
        if point == 0 {
            context.move_to(x, y);
        } else {
            context.line_to(x, y);
        }
    }
    context.close_path();
    let _ = context.fill();
}

fn mask_icon() -> gtk::DrawingArea {
    let area = gtk::DrawingArea::new();
    area.set_content_width(16);
    area.set_content_height(16);

    area.set_draw_func(|area, context, width, height| {

        let colour = area.color();
        let rgb = (colour.red() as f64, colour.green() as f64, colour.blue() as f64);
        draw_mask_icon(context, width, height, rgb);
    });

    area
}

fn draw_mask_icon(context: &gtk::cairo::Context, width: i32, height: i32, rgb: (f64, f64, f64)) {
    {
        let (r, g, b) = rgb;
        let scale = (width.min(height) as f64 / 16.0).max(0.1);
        let _ = context.save();
        context.translate(
            (width as f64 - 16.0 * scale) / 2.0,
            (height as f64 - 16.0 * scale) / 2.0,
        );
        context.scale(scale, scale);

        context.set_line_width(1.4);
        context.set_source_rgba(r, g, b, 0.95);
        context.arc(6.0, 8.0, 4.6, 0.0, std::f64::consts::TAU);
        let _ = context.stroke();

        context.set_source_rgba(r, g, b, 0.38);
        context.arc(10.0, 8.0, 4.6, 0.0, std::f64::consts::TAU);
        let _ = context.fill();

        let _ = context.restore();
    }
}

fn build_mask_editor(state: &App) -> gtk::Box {
    let column = state.mask_editor.clone();
    column.set_margin_top(14);

    column.append(&section_header("Show"));
    let views = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    views.add_css_class("linked");
    views.add_css_class("aspect-ratios");
    views.set_margin_bottom(8);
    for (label, tooltip, on, set) in [
        (
            "Wash",
            "Tint the covered area — the only one that shows a soft edge as soft",
            state.show_coverage.clone(),
            0,
        ),
        ("Outline", "A moving dashed line along the mask's edge", state.show_ants.clone(), 1),
        ("Points", "A dot for each part the mask is made of", state.show_dots.clone(), 2),
    ] {
        let toggle =
            if set == 2 { state.dots_toggle.clone() } else { gtk::ToggleButton::new() };
        toggle.set_label(label);
        toggle.set_hexpand(true);
        toggle.set_active(on.get());
        toggle.set_tooltip_text(Some(tooltip));
        toggle.connect_toggled(glib::clone!(
            #[strong] state,
            #[strong] on,
            move |button| {
                on.set(button.is_active());

                if set == 1 && button.is_active() {
                    start_ants(&state);
                }
                state.mask_area.queue_draw();
            }
        ));
        views.append(&toggle);
    }
    column.append(&views);

    let brush = state.brush_controls.clone();
    brush.set_orientation(gtk::Orientation::Vertical);
    brush.set_spacing(6);
    brush.append(&section_header("Draw on it"));

    let modes = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    modes.add_css_class("linked");
    for (button, label, tooltip, mode) in [
        (&state.brush_paint, "Brush", "Drag to paint — hold Shift to rub out", MaskTool::Brush),
        (&state.brush_lasso, "Lasso", "Drag round something — hold Shift to take it out", MaskTool::Lasso),
    ] {
        button.set_label(label);
        button.set_hexpand(true);
        button.set_tooltip_text(Some(tooltip));
        button.connect_toggled(glib::clone!(
            #[strong] state,
            move |button| {
                if state.applying.get() {
                    return;
                }

                state.applying.set(true);
                let other = match mode {
                    MaskTool::Brush => &state.brush_lasso,
                    _ => &state.brush_paint,
                };
                if button.is_active() {
                    other.set_active(false);
                }
                state.applying.set(false);

                state.brush.set(if button.is_active() { mode } else { MaskTool::Off });
                state.mask_area.queue_draw();
            }
        ));
        modes.append(button);
    }
    brush.append(&modes);

    let size = gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 1.0, 0.005);
    size.set_value(brush_travel(DEFAULT_BRUSH));
    size.set_draw_value(false);
    let size_label = gtk::Label::new(None);
    size_label.set_xalign(0.0);
    size_label.add_css_class("slider-name");
    let show_size = {
        let size_label = size_label.clone();
        move |radius: f32| {
            let percent = radius as f64 * 100.0;

            let text = match percent {
                small if small < 1.0 => format!("Size {small:.2} %"),
                middle if middle < 10.0 => format!("Size {middle:.1} %"),
                large => format!("Size {large:.0} %"),
            };
            size_label.set_text(&text)
        }
    };
    show_size(DEFAULT_BRUSH);
    size.connect_value_changed(glib::clone!(
        #[strong] state,
        move |scale| {
            let radius = brush_size(scale.value());
            state.brush_radius.set(radius);
            show_size(radius);
            state.mask_area.queue_draw();
        }
    ));
    brush.append(&size_label);
    brush.append(&size);
    brush.set_visible(false);
    column.append(&brush);

    let header = state.mask_parts_header.clone();

    header.set_text("THIS MASK");
    header.set_xalign(0.0);
    header.set_margin_top(14);
    header.add_css_class("section-header");
    header.set_visible(false);
    column.append(&header);

    state.mask_parts.set_selection_mode(gtk::SelectionMode::None);
    state.mask_parts.add_css_class("boxed-list");
    state.mask_parts.set_visible(false);
    column.append(&state.mask_parts);

    column.set_visible(false);
    column
}

fn build_masks(state: &App) -> gtk::Box {
    let column = gtk::Box::new(gtk::Orientation::Vertical, 8);

    let add = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    add.add_css_class("linked");
    for (label, kind) in [
        ("Linear", MaskKind::Linear),
        ("Radial", MaskKind::Radial),
        ("Brush", MaskKind::Brush),
        ("Click", MaskKind::Click),
    ] {
        let button = gtk::Button::with_label(label);
        button.set_hexpand(true);

        if kind == MaskKind::Click && !segment::is_installed() && !sam::is_installed() {
            button.set_sensitive(false);
            button.set_tooltip_text(Some("Needs a model — see Preferences"));
        }
        button.connect_clicked(glib::clone!(
            #[strong] state,
            move |_| add_mask(&state, kind)
        ));
        add.append(&button);
    }
    column.append(&add);

    let found = state.found_box.clone();
    found.set_selection_mode(gtk::SelectionMode::None);
    found.set_max_children_per_line(2);
    found.set_homogeneous(true);
    found.set_row_spacing(4);
    found.set_column_spacing(4);
    if segment::is_installed() {
        column.append(&section_header("In this photograph"));
        column.append(&found);
    }

    column.append(&section_header("Find by range"));
    let ranges = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    ranges.add_css_class("linked");
    for (label, kind, tooltip) in [
        ("Colour", MaskKind::ColourRange, "Every pixel of one colour, wherever it is"),
        ("Brightness", MaskKind::LuminanceRange, "Every pixel between two brightnesses"),
    ] {
        let button = gtk::Button::with_label(label);
        button.set_hexpand(true);
        button.set_tooltip_text(Some(tooltip));
        button.connect_clicked(glib::clone!(
            #[strong] state,
            move |_| add_mask(&state, kind)
        ));
        ranges.append(&button);
    }
    column.append(&ranges);

    let note = gtk::Label::new(Some(if segment::is_installed() {
        "Then click the photograph to add what is under the cursor. \
         Shift-click, Shift-drag or Shift-lasso takes it away again."
    } else {
        "No model installed — Preferences (Ctrl+,) says which and where."
    }));
    note.set_xalign(0.0);
    note.set_wrap(true);
    note.add_css_class("profile-note");
    column.append(&note);

    state.mask_list.set_selection_mode(gtk::SelectionMode::None);
    state.mask_list.add_css_class("boxed-list");
    column.append(&state.mask_list);

    let empty = state.mask_empty.clone();
    empty.set_text("No masks yet. A linear gradient is the sky tool; a radial is for a face.");
    empty.set_xalign(0.0);
    empty.set_wrap(true);
    empty.add_css_class("profile-note");
    column.append(&empty);

    column
}

fn set_panel_scope(state: &App) {
    let cropping = is_cropping(state);
    let masked = state.selected_mask.get().is_some() && !cropping;

    for widget in state.global_only.borrow().iter() {
        widget.set_visible(!masked);
    }

    state.crop_controls.set_visible(cropping);
    state.mask_banner.set_visible(masked);

    let drawable = masked && !selected_mask(state).is_some_and(|mask| is_gradient(&mask));

    state.dots_toggle.set_visible(drawable);
    state.brush_controls.set_visible(drawable);
    if !drawable && state.brush.get() != MaskTool::Off {
        state.brush.set(MaskTool::Off);
        state.applying.set(true);
        state.brush_paint.set_active(false);
        state.brush_lasso.set_active(false);
        state.applying.set(false);
    }

    state.panel_tab_strip.set_visible(!masked);
    state.mask_editor.set_visible(masked);
}

fn refresh_masks(state: &App) {
    let masks = state.open.borrow().as_ref().map(|photo| photo.document.masks());
    let Some(masks) = masks else { return };
    let selected = state.selected_mask.get();

    let menu = gio::Menu::new();

    let layers = gio::Menu::new();
    let none = gio::MenuItem::new(Some("No mask — edit the photograph"), None);
    none.set_action_and_target_value(Some("win.pick-mask"), Some(&(-1i32).to_variant()));
    layers.append_item(&none);
    for (index, mask) in masks.iter().enumerate() {
        let item = gio::MenuItem::new(
            Some(&format!(
                "{}{}",
                mask_label(&masks, index),
                if mask.is_idle() { " — nothing set" } else { "" }
            )),
            None,
        );
        item.set_action_and_target_value(Some("win.pick-mask"), Some(&(index as i32).to_variant()));
        layers.append_item(&item);
    }
    menu.append_section(None, &layers);

    let add = gio::Menu::new();
    for (label, kind) in [
        ("Add linear gradient", "linear"),
        ("Add radial gradient", "radial"),
        ("Add brush mask", "brush"),
        ("Add click mask", "click"),
        ("Add colour range", "colour-range"),
        ("Add luminance range", "luminance-range"),
    ] {
        let item = gio::MenuItem::new(Some(label), None);
        item.set_action_and_target_value(Some("win.add-mask"), Some(&kind.to_variant()));
        add.append_item(&item);
    }
    menu.append_section(None, &add);

    if selected.is_some() {
        let selected_actions = gio::Menu::new();
        selected_actions.append(Some("Invert this mask"), Some("win.invert-mask"));
        selected_actions.append(Some("Delete this mask"), Some("win.delete-mask"));
        menu.append_section(None, &selected_actions);
    }

    state.mask_button.set_menu_model(Some(&menu));

    while let Some(row) = state.mask_list.first_child() {
        state.mask_list.remove(&row);
    }
    for (index, mask) in masks.iter().enumerate() {
        let row = adw::ActionRow::new();
        row.set_title(&mask_label(&masks, index));
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
        state.mask_list.append(&row);
    }

    refresh_mask_parts(state, &masks, selected);
    refresh_outline(state);
    state.mask_list.set_visible(!masks.is_empty());
    state.mask_empty.set_visible(masks.is_empty());

    let current = selected.filter(|index| *index < masks.len());
    state
        .mask_button
        .set_label(&current.map_or_else(String::new, |index| mask_label(&masks, index)));

    if let Some(mask) = current.map(|index| &masks[index]) {
        state.applying.set(true);
        state.mask_banner_eye.set_active(mask.visible);
        state.applying.set(false);
        state.mask_banner_eye.set_icon_name(if mask.visible {
            "view-reveal-symbolic"
        } else {
            "view-conceal-symbolic"
        });
        state.mask_banner_eye.set_tooltip_text(Some(if mask.visible {
            "Hide this mask"
        } else {
            "Show this mask"
        }));
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum MaskTool {

    Off,
    Brush,
    Lasso,
}

const DEFAULT_BRUSH: f32 = 0.05;

const BRUSH_SMALLEST: f32 = 0.0008;
const BRUSH_LARGEST: f32 = 0.25;

fn brush_size(travel: f64) -> f32 {
    let travel = travel.clamp(0.0, 1.0) as f32;
    BRUSH_SMALLEST + travel * travel * (BRUSH_LARGEST - BRUSH_SMALLEST)
}

fn brush_travel(radius: f32) -> f64 {
    (((radius - BRUSH_SMALLEST) / (BRUSH_LARGEST - BRUSH_SMALLEST)).max(0.0).sqrt()) as f64
}

const BRUSH_FEATHER: f32 = 0.5;

#[derive(Clone, Copy, PartialEq, Eq)]
enum MaskKind {
    Linear,
    Radial,
    Brush,

    ColourRange,
    LuminanceRange,

    Click,
}

fn ensure_embedding(state: &App) {
    if !sam::is_installed() {
        return;
    }
    let request = {
        let mut open = state.open.borrow_mut();
        let Some(photo) = open.as_mut() else { return };
        if photo.embedding.is_some() || photo.embedding_pending {
            return;
        }
        photo.embedding_pending = true;

        let mut geometry = Document::new(photo.document.source.path.clone());
        geometry.set_perspective(photo.document.perspective());
        if let Some((rect, angle)) = photo.document.crop() {
            geometry.set_crop(rect, angle);
        }
        geometry.set_rotation(photo.document.rotation());
        geometry.set_mirrored(photo.document.mirrored());
        (geometry, photo.working.clone())
    };

    let (geometry, working) = request;
    let generation = state.open_generation.get();
    let state = state.clone();
    glib::spawn_future_local(async move {
        let made = busy(&state, "Working out what can be clicked…", move || {
            let frame = render::apply_stack(&geometry, &working, 1.0);
            sam::encode(&frame)
        })
        .await;

        if state.open_generation.get() != generation {
            return;
        }

        let index = {
            let mut open = state.open.borrow_mut();
            let Some(photo) = open.as_mut() else { return };
            photo.embedding_pending = false;
            photo.embedding = made.ok().flatten().map(Arc::new);
            state.selected_mask.get()
        };

        if let Some(index) = index {
            rebuild_mask_map(&state, index);
            refresh_masks(&state);
            request_render(&state);
            show_coverage(&state);
        }
    });
}

fn ensure_segmentation(state: &App) {
    let request = {
        let mut open = state.open.borrow_mut();
        let Some(photo) = open.as_mut() else { return };
        if photo.segmentation.is_some() || photo.segmenting {
            return;
        }
        photo.segmenting = true;

        let mut geometry = Document::new(photo.document.source.path.clone());
        geometry.set_perspective(photo.document.perspective());
        if let Some((rect, angle)) = photo.document.crop() {
            geometry.set_crop(rect, angle);
        }
        geometry.set_rotation(photo.document.rotation());
        geometry.set_mirrored(photo.document.mirrored());
        (geometry, photo.working.clone())
    };

    let (geometry, working) = request;

    refresh_found(state);

    let generation = state.open_generation.get();
    let state = state.clone();
    glib::spawn_future_local(async move {
        let found = busy(&state, "Finding what is in the photograph…", move || {
            let frame = render::apply_stack(&geometry, &working, 1.0);
            segment::of(&frame)
        })
        .await;

        if state.open_generation.get() != generation {
            return;
        }

        if let Some(photo) = state.open.borrow_mut().as_mut() {
            photo.segmenting = false;
            photo.segmentation = found.ok().flatten().map(Arc::new);

            let mut masks = photo.document.masks();
            for mask in masks.iter_mut().filter(|mask| mask.wants_pixels()) {
                mask.map = Pixels(None);
            }
            photo.document.set_masks(masks);
        }

        fill_segment_masks(&state);
        refresh_found(&state);
        refresh_masks(&state);
        request_render(&state);
        state.mask_area.queue_draw();

        let Some(found) = state.open.borrow().as_ref().and_then(|photo| photo.segmentation.clone())
        else {
            return;
        };
        if !numa::render::classify::is_installed() {
            return;
        }
        let asked = Arc::downgrade(&found);
        let guess = gtk::gio::spawn_blocking(move || numa::render::classify::animal(&found)).await;
        if state.open_generation.get() != generation {
            return;
        }
        let Ok(Some(guess)) = guess else { return };
        if let Some(photo) = state.open.borrow_mut().as_mut() {
            photo.animal = Some((asked, guess));
        }
        refresh_found(&state);
    });
}

fn refresh_found(state: &App) {
    let (found, looking, animal) = {
        let open = state.open.borrow();
        match open.as_ref() {
            Some(photo) => (photo.segmentation.clone(), photo.segmenting, photo.animal.clone()),
            None => (None, false, None),
        }
    };

    let row = &state.found_box;
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

    let mut things = found.found();

    let named_a_subject =
        things.iter().any(|thing| thing.classes.iter().any(|c| segment::MATTEABLE.contains(c)));
    if !named_a_subject && numa::render::matte::is_installed() {
        things.push(segment::Found {
            name: "Subject".to_string(),
            classes: segment::MATTEABLE.to_vec(),
            share: 0.0,
        });
    }
    if things.is_empty() {
        note("Nothing it could name");
        return;
    }

    let animal = animal
        .filter(|(asked, _)| std::ptr::eq(asked.as_ptr(), Arc::as_ptr(&found)))
        .map(|(_, guess)| guess);

    for thing in things {
        let mut tooltip = match thing.share > 0.0 {
            true => format!("About {:.0} % of this photograph", thing.share * 100.0),
            false => "Not named here — the subject model will look for one".to_string(),
        };
        let mut name = thing.name.clone();
        if let Some(guess) = animal.as_ref().filter(|_| thing.classes.contains(&126)) {
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
            move |_| add_segment_mask(&state, classes.clone(), &chosen)
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

fn preview_found(state: &App, classes: &[u16]) {
    let paths = {
        let open = state.open.borrow();
        let Some(found) = open.as_ref().and_then(|photo| photo.segmentation.clone()) else {
            return;
        };
        let coarse = found.coarse(classes);
        let alpha = Alpha::new(coarse.width, coarse.height, coarse.data);
        let mut paths = numa::core::mask::outline(&alpha, OUTLINE_EDGE);
        paths.truncate(400);
        paths
    };
    if paths.is_empty() {
        return;
    }
    *state.mask_outline.borrow_mut() = paths;
    state.previewing.set(true);
    state.mask_area.set_visible(true);
    start_ants(state);
    state.mask_area.queue_draw();
}

fn end_preview(state: &App) {
    if !state.previewing.get() {
        return;
    }
    state.previewing.set(false);
    state.mask_area.set_visible(state.selected_mask.get().is_some());
    refresh_outline(state);
}

fn fill_segment_masks(state: &App) {
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

fn drop_empty_masks(state: &App) {
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

                    let covered: f64 = alpha.data.iter().map(|v| *v as f64).sum();
                    covered / alpha.data.len().max(1) as f64 <= 0.001
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

fn mask_raster_size(state: &App) -> (usize, usize) {
    let (width, height) = displayed_size(state).unwrap_or((3, 2));
    render::raster_size(width, height, render::MASK_RASTER)
}

fn selected_shape(state: &App, index: usize) -> Option<Shape> {
    let open = state.open.borrow();
    open.as_ref()?.document.masks().get(index).map(|mask| mask.shape.clone())
}

fn mask_frame(state: &App) -> Option<Arc<image::RgbImage>> {
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
        let working = render::to_working_space(&geometry, &photo.proxy);
        Arc::new(render::apply_stack(&geometry, &working, 1.0))
    };

    if let Some(photo) = state.open.borrow_mut().as_mut() {
        photo.mask_frame = Some(made.clone());
    }
    Some(made)
}

fn rebuild_mask_map(state: &App, index: usize) {
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
    photo.view = None;

    drop(open);

    refresh_outline(state);
}

fn subject_alpha(state: &App) -> Option<Arc<Alpha>> {
    if !segment::is_installed() {
        return None;
    }
    let frame = mask_frame(state)?;
    let (width, height) = mask_raster_size(state);

    let found = match state.open.borrow().as_ref()?.segmentation.clone() {
        Some(found) => Some(found),
        None => segment::of(&frame).map(Arc::new),
    }?;
    if let Some(photo) = state.open.borrow_mut().as_mut() {
        photo.segmentation = Some(found.clone());
    }

    let mut mask = Mask::new(Shape::Segment { classes: segment::MATTEABLE.to_vec() });
    mask.matte = true;
    render::resolve_mask(&mut mask, Some(&found), None, Some(&frame), width, height);
    mask.map.0.clone()
}

fn add_segment_mask(state: &App, classes: Vec<u16>, name: &str) {
    if !segment::is_installed() {
        state.toast("No segmentation model installed — see Preferences");
        return;
    }

    let added = {
        let mut open = state.open.borrow_mut();
        let Some(photo) = open.as_mut() else { return };
        let mut masks = photo.document.masks();

        let existing = masks.iter().position(|mask| {
            mask.shape == Shape::Segment { classes: classes.clone() } && mask.is_idle()
        });
        if let Some(index) = existing {
            drop(open);
            select_mask(state, Some(index));
            return;
        }

        let mut mask = Mask::new(Shape::Segment { classes: classes.clone() });

        mask.matte = classes.iter().all(|class| segment::MATTEABLE.contains(class));

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

fn taking_away(state: gtk::gdk::ModifierType) -> bool {
    state.contains(gtk::gdk::ModifierType::SHIFT_MASK)
}

fn paint_segment(state: &App, from: [f32; 2], to: [f32; 2], stroke: &Stroke) {
    let Some(index) = state.selected_mask.get() else { return };
    {
        let mut open = state.open.borrow_mut();
        let Some(photo) = open.as_mut() else { return };
        let Some(mask) = photo.document.mask_mut(index) else { return };
        let Some(alpha) = mask.map.0.as_mut() else { return };

        stroke.draw_segment(Arc::make_mut(alpha), from, to);
        photo.view = None;
    }

    if let (Some(surface), Some(mask)) =
        (state.mask_wash.borrow_mut().as_mut(), selected_mask(state))
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
            paint_wash(surface, alpha, mask.inverted, mask.opacity, bounds);
        }
    }

    state.mask_area.queue_draw();
}

fn point_at(state: &App, u: f32, v: f32, subtract: bool) {
    let Some(index) = state.selected_mask.get() else { return };

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

fn add_mask(state: &App, kind: MaskKind) {
    busy_sync(state, "Adding the mask…", move |state| add_mask_now(state, kind))
}

fn add_mask_now(state: &App, kind: MaskKind) {
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
        state.applying.set(true);
        state.brush_lasso.set_active(false);
        state.brush_paint.set_active(true);
        state.applying.set(false);
        state.brush.set(MaskTool::Brush);
    }
    if kind == MaskKind::Click {

        ensure_segmentation(state);
        ensure_embedding(state);
        state.toast("Click the photograph to add what is under the cursor");
    }

    schedule_history_push(state);
}

fn add_mask_reordering(state: &App, row: &adw::ActionRow, index: usize) {
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

fn refresh_mask_parts(state: &App, masks: &[Mask], selected: Option<usize>) {
    while let Some(row) = state.mask_parts.first_child() {
        state.mask_parts.remove(&row);
    }

    let Some(mask) = selected.and_then(|index| masks.get(index)) else {
        state.mask_parts.set_visible(false);
        state.mask_parts_header.set_visible(false);
        return;
    };
    let index = selected.unwrap_or(0);

    let name = adw::EntryRow::new();
    name.set_title("Name");
    name.set_text(mask.name.as_deref().unwrap_or(""));

    name.set_tooltip_text(Some(&format!("Blank for \u{201c}{}\u{201d}", mask_name(mask))));
    name.connect_apply(glib::clone!(
        #[strong] state,
        move |entry| {
            if !state.applying.get() {
                rename_mask(&state, index, &entry.text());
            }
        }
    ));
    state.mask_parts.append(&name);

    let opacity = adw::SpinRow::with_range(0.0, 100.0, 1.0);
    opacity.set_title("Strength");
    opacity.set_subtitle("How much of this mask applies");
    opacity.set_value((mask.opacity * 100.0) as f64);
    opacity.connect_value_notify(glib::clone!(
        #[strong] state,
        move |row| {
            if !state.applying.get() {
                set_mask_opacity(&state, index, row.value() as f32 / 100.0);
            }
        }
    ));
    state.mask_parts.append(&opacity);

    if is_gradient(mask) {
        state.mask_parts.set_visible(true);
        state.mask_parts_header.set_visible(true);
        return;
    }

    let matte = adw::ActionRow::new();
    matte.set_title("Refine edge");
    let search = gtk::Button::with_label(if mask.matte { "Search again" } else { "Refine" });
    search.set_valign(gtk::Align::Center);
    matte.add_suffix(&search);

    let subject = match &mask.shape {
        Shape::Segment { classes } => {
            classes.iter().any(|class| segment::MATTEABLE.contains(class))
        }
        Shape::Painted => true,
        _ => false,
    };
    let ready = subject && numa::render::matte::is_installed() && segment::is_installed();
    matte.set_subtitle(match (subject, numa::render::matte::is_installed()) {
        (false, _) => "Only for people and animals — the model has nothing to say here",
        (true, false) => "Needs the models — see Preferences",
        (true, true) => "Search for the real edge, from where Edge puts it — slower, and worth it on a subject",
    });
    matte.set_sensitive(ready);
    search.connect_clicked(glib::clone!(
        #[strong] state,
        move |button| refine_mask_edge(&state, index, button)
    ));
    state.mask_parts.append(&matte);

    let ranged: &[(&str, &str, f64, f64, f64, f64, u8)] = match &mask.shape {
        Shape::ColourRange { hue, spread, saturation, .. } => &[
            ("Hue", "Which colour, in degrees round the wheel", 0.0, 359.0, 1.0, *hue as f64, 0),
            ("Spread", "How far either side of it still counts", 1.0, 180.0, 1.0, *spread as f64, 1),
            (
                "Minimum saturation",
                "Below this a pixel is grey, and grey has no colour to match",
                0.0,
                100.0,
                1.0,
                (*saturation * 100.0) as f64,
                2,
            ),
        ],
        Shape::LuminanceRange { low, high, softness, .. } => &[
            ("From", "The dark end of the range", 0.0, 100.0, 1.0, (*low * 100.0) as f64, 3),
            ("To", "The bright end", 0.0, 100.0, 1.0, (*high * 100.0) as f64, 4),
            (
                "Softness",
                "How gradually it lets go at both ends",
                0.0,
                100.0,
                1.0,
                (*softness * 100.0) as f64,
                5,
            ),
        ],
        _ => &[],
    };
    if !ranged.is_empty() {
        let chosen = matches!(
            mask.shape,
            Shape::ColourRange { picked: true, .. } | Shape::LuminanceRange { picked: true, .. }
        );
        if chosen {
            state.mask_parts.append(&build_range_swatch(state, &mask.shape));
        }
        let hint = gtk::Label::new(Some(match chosen {
            true => "Click the photograph again to choose something else.",
            false => "Click the photograph to choose what this selects. \
                      Nothing is selected until you do.",
        }));
        hint.set_xalign(0.0);
        hint.set_wrap(true);
        hint.set_margin_start(14);
        hint.set_margin_end(14);
        hint.set_margin_bottom(4);
        hint.add_css_class("profile-note");
        state.mask_parts.append(&hint);

        if !chosen {
            state.mask_parts.set_visible(true);
            state.mask_parts_header.set_visible(true);
            return;
        }
    }
    for (title, subtitle, lo, hi, step, now, which) in ranged.iter().copied() {
        let row = adw::SpinRow::with_range(lo, hi, step);
        row.set_title(title);
        row.set_subtitle(subtitle);
        row.set_value(now);
        shift_moves_ten(&row, &row.adjustment());
        row.connect_value_notify(glib::clone!(
            #[strong] state,
            move |row| {
                if !state.applying.get() {
                    set_mask_range(&state, index, which, row.value() as f32);
                }
            }
        ));
        state.mask_parts.append(&row);
    }

    for (title, subtitle, lo, hi, now, set) in [
        (
            "Feather",
            "How wide the mask's border is",
            0.0,
            100.0,
            mask.feather as f64,
            0u8,
        ),
        (
            "Edge",
            "Pull the edge in, or push it out",
            -100.0,
            100.0,
            mask.shift as f64,
            1,
        ),
    ] {
        let row = adw::SpinRow::with_range(lo, hi, 1.0);
        row.set_title(title);
        row.set_subtitle(subtitle);
        row.set_value(now);

        shift_moves_ten(&row, &row.adjustment());
        row.connect_value_notify(glib::clone!(
            #[strong] state,
            move |row| {
                if !state.applying.get() {
                    set_mask_edge(&state, index, set, row.value() as f32);
                }
            }
        ));
        state.mask_parts.append(&row);
    }

    let copy = adw::ActionRow::new();
    copy.set_title("Duplicate");
    copy.set_subtitle("The same mask again, to grade twice");
    copy.set_activatable(true);
    copy.connect_activated(glib::clone!(
        #[strong] state,
        move |_| duplicate_mask(&state, index)
    ));
    let copy_icon = gtk::Image::from_icon_name("edit-copy-symbolic");
    copy_icon.add_css_class("dim-label");
    copy.add_suffix(&copy_icon);
    state.mask_parts.append(&copy);

    let mut parts: Vec<(String, MaskPart, bool)> = Vec::new();
    if let Shape::Segment { classes } = &mask.shape {
        for class in classes {
            parts.push((
                segment::label(*class).unwrap_or("something").to_string(),
                MaskPart::Class(*class),
                !mask.muted.contains(class),
            ));
        }
    }

    let segmentation = (!sam::is_installed())
        .then(|| state.open.borrow().as_ref().and_then(|photo| photo.segmentation.clone()))
        .flatten();
    for (at, point) in mask.points.iter().enumerate() {
        let named = segmentation
            .as_ref()
            .map(|found| found.class_at(point.at[0], point.at[1]))
            .and_then(segment::label);
        let doing = if point.subtract { "Without" } else { "With" };
        parts.push((
            match named {
                Some(name) => format!("{doing} {name}"),
                None => format!(
                    "{doing} what is at {:.0} %, {:.0} %",
                    point.at[0] * 100.0,
                    point.at[1] * 100.0
                ),
            },
            MaskPart::Point(at),
            point.enabled,
        ));
    }

    let kind = |stroke: &Stroke| match (stroke.fill, stroke.erase) {
        (true, false) => "Lassoed in",
        (true, true) => "Lassoed out",
        (false, false) => "Painted",
        (false, true) => "Rubbed out",
    };
    let mut at = 0;
    while at < mask.strokes.len() {
        let stroke = &mask.strokes[at];
        let (name, on) = (kind(stroke), stroke.enabled);

        let run = mask.strokes[at..]
            .iter()
            .take_while(|next| kind(next) == name && next.enabled == on)
            .count();
        parts.push((
            if run > 1 { format!("{name} \u{00d7}{run}") } else { name.to_string() },
            MaskPart::Stroke { at, run },
            on,
        ));
        at += run;
    }

    let last = parts.len() == 1 && matches!(mask.shape, Shape::Segment { .. });
    for (name, part, enabled) in parts {
        let row = adw::ActionRow::new();
        row.set_title(&name);
        row.add_css_class("mask-part");

        let on = gtk::ToggleButton::new();
        on.set_icon_name(if enabled { "view-reveal-symbolic" } else { "view-conceal-symbolic" });
        on.set_active(enabled);
        on.set_valign(gtk::Align::Center);
        on.add_css_class("flat");
        on.set_tooltip_text(Some(if enabled { "Switch this part off" } else { "Switch it back on" }));
        on.connect_toggled(glib::clone!(
            #[strong] state,
            move |button| {
                if !state.applying.get() && button.is_active() != enabled {
                    toggle_mask_part(&state, index, part);
                }
            }
        ));
        row.add_suffix(&on);

        let remove = gtk::Button::from_icon_name("window-close-symbolic");
        remove.set_valign(gtk::Align::Center);
        remove.add_css_class("flat");

        remove.set_sensitive(!(last && matches!(part, MaskPart::Class(_))));
        remove.set_tooltip_text(Some("Take this back out of the mask"));
        remove.connect_clicked(glib::clone!(
            #[strong] state,
            move |_| remove_mask_part(&state, index, part)
        ));
        row.add_suffix(&remove);
        state.mask_parts.append(&row);
    }

    let any = state.mask_parts.first_child().is_some();
    state.mask_parts.set_visible(any);
    state.mask_parts_header.set_visible(any);
}

fn schedule_save(state: &App) {
    let generation = state.save_generation.get().wrapping_add(1);
    state.save_generation.set(generation);

    let state = state.clone();
    glib::timeout_add_local_once(std::time::Duration::from_secs(2), move || {
        if state.save_generation.get() != generation {
            return;
        }
        save_open_edits(&state);
    });
}

fn mask_at(state: &App, index: usize) -> Option<Mask> {
    state.open.borrow().as_ref()?.document.masks().get(index).cloned()
}

fn set_mask_visible(state: &App, index: usize, visible: bool) {
    {
        let mut open = state.open.borrow_mut();
        let Some(photo) = open.as_mut() else { return };
        let Some(mask) = photo.document.mask_mut(index) else { return };
        if mask.visible == visible {
            return;
        }
        mask.visible = visible;
        photo.view = None;
    }

    refresh_masks(state);
    request_render(state);
    state.mask_area.queue_draw();
    schedule_history_push(state);
}

fn refine_mask_edge(state: &App, index: usize, button: &gtk::Button) {
    let (width, height) = mask_raster_size(state);
    let request = {
        let mut open = state.open.borrow_mut();
        let Some(photo) = open.as_mut() else { return };
        let segmentation = photo.segmentation.clone();
        let embedding = photo.embedding.clone();
        let Some(mask) = photo.document.mask_mut(index) else { return };
        mask.matte = true;
        mask.matte_edge = mask.shift;
        (mask.clone(), segmentation, embedding)
    };
    schedule_history_push(state);

    let (mut mask, segmentation, embedding) = request;
    let generation = state.open_generation.get();
    let state = state.clone();

    button.set_sensitive(false);
    let button = button.clone();
    glib::spawn_future_local(async move {
        let worked = mask.clone();
        let resolved = busy(&state, "Tracing the edge…", move || {

            render::resolve_mask(
                &mut mask,
                segmentation.as_deref(),
                embedding.as_deref(),
                None,
                width,
                height,
            );
            mask
        })
        .await;
        button.set_label("Search again");
        button.set_sensitive(true);

        if state.open_generation.get() != generation {
            return;
        }
        let Ok(resolved) = resolved else { return };
        let Some(unshaped) = resolved.unshaped.0.clone() else { return };
        {
            let mut open = state.open.borrow_mut();
            let Some(photo) = open.as_mut() else { return };
            let Some(mask) = photo.document.mask_mut(index) else { return };

            let mut asked = mask.clone();
            asked.feather = worked.feather;
            asked.shift = worked.shift;
            if asked != worked {
                return;
            }
            mask.unshaped = Pixels(Some(unshaped));
            mask.reshape_edge(width, height);
            photo.view = None;
        }
        refresh_outline(&state);
        request_render(&state);
        show_coverage(&state);
    });
}

fn arm_band_pipette(state: &App) {
    let armed = state.picking_band.get() || state.picking_white.get() || state.picking_point.get();
    state.canvas.set_cursor(armed.then(pipette_cursor).as_ref());
}

fn pipette_cursor() -> gtk::gdk::Cursor {
    use gtk::cairo;
    const SIZE: i32 = 32;
    let fallback = gtk::gdk::Cursor::from_name("crosshair", None);
    let Ok(mut surface) = cairo::ImageSurface::create(cairo::Format::ARgb32, SIZE, SIZE) else {
        return fallback.unwrap_or_else(|| gtk::gdk::Cursor::from_name("default", None).expect("a default cursor"));
    };
    if let Ok(context) = cairo::Context::new(&surface) {
        context.set_line_cap(cairo::LineCap::Round);

        let body = |context: &cairo::Context, width: f64| {
            context.set_line_width(width);
            context.move_to(3.0, 29.0);
            context.line_to(20.0, 12.0);
            let _ = context.stroke();
            context.arc(23.5, 8.5, width * 0.9 + 1.5, 0.0, std::f64::consts::TAU);
            let _ = context.fill();
        };
        context.set_source_rgba(1.0, 1.0, 1.0, 0.95);
        body(&context, 6.0);
        context.set_source_rgba(0.12, 0.12, 0.14, 1.0);
        body(&context, 3.0);

        context.set_line_width(2.0);
        context.move_to(16.0, 11.0);
        context.line_to(21.0, 16.0);
        let _ = context.stroke();
    }
    surface.flush();
    let (width, height, stride) = (surface.width(), surface.height(), surface.stride() as usize);
    let Ok(data) = surface.data() else { return fallback.expect("crosshair") };

    let texture = gtk::gdk::MemoryTexture::new(
        width,
        height,
        gtk::gdk::MemoryFormat::B8g8r8a8Premultiplied,
        &glib::Bytes::from(&data[..]),
        stride,
    );
    gtk::gdk::Cursor::from_texture(&texture, 3, 29, fallback.as_ref())
}

fn pick_white(state: &App, u: f32, v: f32) {
    state.applying.set(true);
    state.white_pipette.set_active(false);
    state.applying.set(false);
    state.picking_white.set(false);
    arm_band_pipette(state);

    let Some(frame) = mask_frame(state) else { return };
    let Some(profile) = state.open.borrow().as_ref().and_then(|photo| photo.proxy.profile) else {
        state.toast("This photograph has no camera white balance to set");
        return;
    };
    let (width, height) = (frame.width() as i64, frame.height() as i64);
    let (cx, cy) = ((u.clamp(0.0, 1.0) * width as f32) as i64, (v.clamp(0.0, 1.0) * height as f32) as i64);
    let decode = |byte: u8| {
        let value = byte as f32 / 255.0;
        if value <= 0.040_45 { value / 12.92 } else { ((value + 0.055) / 1.055).powf(2.4) }
    };
    let mut sum = [0.0f32; 3];
    let mut count = 0.0;
    for y in (cy - 2).max(0)..(cy + 3).min(height) {
        for x in (cx - 2).max(0)..(cx + 3).min(width) {
            let pixel = frame.get_pixel(x as u32, y as u32).0;

            if pixel.iter().any(|channel| *channel >= 250) {
                continue;
            }
            for channel in 0..3 {
                sum[channel] += decode(pixel[channel]);
            }
            count += 1.0;
        }
    }
    let balance = (count > 0.0).then(|| profile.neutral_balance(sum.map(|value| value / count))).flatten();
    let Some(balance) = balance else {
        state.toast("Nothing to measure there — pick something grey that is not blown out");
        return;
    };
    state.sliders.write_white_balance(WhiteBalance {
        temperature: balance.temperature.clamp(2000.0, 15000.0),
        tint: balance.tint,
    });
}

fn pick_band(state: &App, u: f32, v: f32) {
    let Some(frame) = mask_frame(state) else { return };
    let x = ((u.clamp(0.0, 1.0) * frame.width() as f32) as u32).min(frame.width() - 1);
    let y = ((v.clamp(0.0, 1.0) * frame.height() as f32) as u32).min(frame.height() - 1);
    let pixel = frame.get_pixel(x, y).0;
    let [hue, saturation, value] = numa::core::profile::rgb_to_hsv([
        pixel[0] as f32 / 255.0,
        pixel[1] as f32 / 255.0,
        pixel[2] as f32 / 255.0,
    ]);

    state.applying.set(true);
    state.band_pipette.set_active(false);
    state.applying.set(false);
    state.picking_band.set(false);
    arm_band_pipette(state);

    if saturation < 0.08 || value < 0.04 {
        state.toast("That pixel has no colour to pick — try something less grey");
        return;
    }

    let degrees = hue * 60.0;
    let (band, name) = BANDS
        .iter()
        .enumerate()
        .min_by(|(_, (_, a)), (_, (_, b))| {
            let apart = |centre: f32| {
                let gap = (degrees - centre).rem_euclid(360.0);
                gap.min(360.0 - gap)
            };
            apart(*a).total_cmp(&apart(*b))
        })
        .map(|(index, (name, _))| (index, *name))
        .unwrap_or((0, "Red"));

    state.mixer_band.set(band);
    state.applying.set(true);
    if let Some(button) = state.mixer_swatches.borrow().get(band) {
        button.set_active(true);
    }
    state.applying.set(false);
    tint_mixer(state);
    write_mixer(state);
    state.toast(&format!("{name} — the sliders are about that now"));
}

fn build_range_swatch(_state: &App, shape: &Shape) -> gtk::DrawingArea {
    let area = gtk::DrawingArea::new();
    area.set_content_height(34);
    area.set_margin_start(14);
    area.set_margin_end(14);
    area.set_margin_top(4);
    area.set_margin_bottom(6);

    let shape = shape.clone();
    area.set_draw_func(move |_, context, width, height| {
        let (width, height) = (width as f64, height as f64);
        let radius = 4.0;

        let gradient = gtk::cairo::LinearGradient::new(0.0, 0.0, width, 0.0);
        match &shape {
            Shape::ColourRange { .. } => {
                for step in 0..=12 {
                    let at = step as f64 / 12.0;
                    let (r, g, b) = hue_rgb(at * 360.0);
                    gradient.add_color_stop_rgb(at, r, g, b);
                }
            }
            _ => {
                gradient.add_color_stop_rgb(0.0, 0.0, 0.0, 0.0);
                gradient.add_color_stop_rgb(1.0, 1.0, 1.0, 1.0);
            }
        }
        rounded(context, 0.0, 0.0, width, height, radius);
        let _ = context.set_source(&gradient);
        let _ = context.fill();

        let (from, to) = match &shape {
            Shape::ColourRange { hue, spread, .. } => {
                let centre = hue.rem_euclid(360.0) / 360.0;
                let half = (spread / 360.0) as f64;
                (centre as f64 - half, centre as f64 + half)
            }
            Shape::LuminanceRange { low, high, .. } => (*low as f64, *high as f64),
            _ => (0.0, 1.0),
        };

        context.set_source_rgba(0.0, 0.0, 0.0, 0.62);

        for shift in [-1.0, 0.0, 1.0] {
            let (left, right) = ((from + shift).max(0.0), (to + shift).min(1.0));
            if right <= left {
                continue;
            }
            context.rectangle(0.0, 0.0, left * width, height);
            let _ = context.fill();
            context.rectangle(right * width, 0.0, width - right * width, height);
            let _ = context.fill();
        }

        let centre = match &shape {
            Shape::ColourRange { hue, .. } => (hue.rem_euclid(360.0) / 360.0) as f64,
            Shape::LuminanceRange { low, high, .. } => ((low + high) * 0.5) as f64,
            _ => 0.5,
        };
        let x = centre * width;
        context.set_line_width(2.0);
        context.set_source_rgba(1.0, 1.0, 1.0, 0.95);
        context.move_to(x, 0.0);
        context.line_to(x, height);
        let _ = context.stroke();
    });

    area
}

fn hue_rgb(degrees: f64) -> (f64, f64, f64) {
    let h = degrees.rem_euclid(360.0) / 60.0;
    let x = 1.0 - (h % 2.0 - 1.0).abs();
    match h as u32 {
        0 => (1.0, x, 0.0),
        1 => (x, 1.0, 0.0),
        2 => (0.0, 1.0, x),
        3 => (0.0, x, 1.0),
        4 => (x, 0.0, 1.0),
        _ => (1.0, 0.0, x),
    }
}

fn rounded(context: &gtk::cairo::Context, x: f64, y: f64, width: f64, height: f64, radius: f64) {
    use std::f64::consts::PI;
    context.new_sub_path();
    context.arc(x + width - radius, y + radius, radius, -PI / 2.0, 0.0);
    context.arc(x + width - radius, y + height - radius, radius, 0.0, PI / 2.0);
    context.arc(x + radius, y + height - radius, radius, PI / 2.0, PI);
    context.arc(x + radius, y + radius, radius, PI, 1.5 * PI);
    context.close_path();
}

fn pick_range(state: &App, u: f32, v: f32) {
    let Some(index) = state.selected_mask.get() else { return };
    let Some(frame) = mask_frame(state) else {
        state.toast("Nothing to sample yet");
        return;
    };

    let x = ((u.clamp(0.0, 1.0) * frame.width() as f32) as u32).min(frame.width() - 1);
    let y = ((v.clamp(0.0, 1.0) * frame.height() as f32) as u32).min(frame.height() - 1);
    let pixel = frame.get_pixel(x, y).0;
    let rgb = [pixel[0] as f32 / 255.0, pixel[1] as f32 / 255.0, pixel[2] as f32 / 255.0];

    let named = {
        let mut open = state.open.borrow_mut();
        let Some(photo) = open.as_mut() else { return };
        let Some(mask) = photo.document.mask_mut(index) else { return };
        match &mut mask.shape {
            Shape::ColourRange { hue, saturation, picked, .. } => {
                *picked = true;
                let [sampled, sampled_saturation, value] =
                    numa::core::profile::rgb_to_hsv(rgb);

                if sampled_saturation < 0.08 || value < 0.04 {
                    None
                } else {
                    *hue = sampled * 60.0;

                    *saturation =
                        saturation.min((sampled_saturation * 0.6).clamp(0.05, 0.9));
                    Some(format!("Matching this colour — {:.0}°", sampled * 60.0))
                }
            }
            Shape::LuminanceRange { low, high, picked, .. } => {
                *picked = true;
                let luma = 0.2126 * rgb[0] + 0.7152 * rgb[1] + 0.0722 * rgb[2];

                let half = ((*high - *low) * 0.5).max(0.05);
                *low = (luma - half).clamp(0.0, 1.0);
                *high = (luma + half).clamp(0.0, 1.0);
                Some(format!("Matching this brightness — {:.0}%", luma * 100.0))
            }
            _ => None,
        }
    };

    let Some(said) = named else {
        state.toast("That pixel has no colour to match — try something less grey");
        return;
    };

    rebuild_mask_map(state, index);
    refresh_masks(state);
    request_render(state);
    show_coverage(state);
    schedule_history_push(state);
    state.toast(&said);
}

fn set_mask_range(state: &App, index: usize, which: u8, value: f32) {
    {
        let mut open = state.open.borrow_mut();
        let Some(photo) = open.as_mut() else { return };
        let Some(mask) = photo.document.mask_mut(index) else { return };
        match (&mut mask.shape, which) {
            (Shape::ColourRange { hue, .. }, 0) => *hue = value,
            (Shape::ColourRange { spread, .. }, 1) => *spread = value,
            (Shape::ColourRange { saturation, .. }, 2) => *saturation = value / 100.0,
            (Shape::LuminanceRange { low, .. }, 3) => *low = value / 100.0,
            (Shape::LuminanceRange { high, .. }, 4) => *high = value / 100.0,
            (Shape::LuminanceRange { softness, .. }, 5) => *softness = value / 100.0,
            _ => return,
        }
        photo.view = None;
    }

    rebuild_mask_map(state, index);
    request_render(state);
    show_coverage(state);
    schedule_history_push(state);
}

fn set_mask_edge(state: &App, index: usize, which: u8, value: f32) {
    {
        let mut open = state.open.borrow_mut();
        let Some(photo) = open.as_mut() else { return };
        let Some(mask) = photo.document.mask_mut(index) else { return };
        let target = if which == 0 { &mut mask.feather } else { &mut mask.shift };
        if (*target - value).abs() < 1e-4 {
            return;
        }
        *target = value;
        photo.view = None;
    }

    let reshaped = {
        let (width, height) = mask_raster_size(state);
        let mut open = state.open.borrow_mut();
        open.as_mut()
            .and_then(|photo| photo.document.mask_mut(index))
            .is_some_and(|mask| mask.reshape_edge(width, height))
    };
    if reshaped {

        refresh_outline(state);
    } else {
        rebuild_mask_map(state, index);
    }
    request_render(state);
    show_coverage(state);
    schedule_history_push(state);
}

fn set_mask_opacity(state: &App, index: usize, opacity: f32) {
    {
        let mut open = state.open.borrow_mut();
        let Some(photo) = open.as_mut() else { return };
        let Some(mask) = photo.document.mask_mut(index) else { return };
        if (mask.opacity - opacity).abs() < 1e-4 {
            return;
        }
        mask.opacity = opacity.clamp(0.0, 1.0);
        photo.view = None;
    }

    request_render(state);
    state.mask_area.queue_draw();
    schedule_history_push(state);
}

fn rename_mask(state: &App, index: usize, name: &str) {
    {
        let mut open = state.open.borrow_mut();
        let Some(photo) = open.as_mut() else { return };
        let Some(mask) = photo.document.mask_mut(index) else { return };
        let trimmed = name.trim();
        let wanted = (!trimmed.is_empty()).then(|| trimmed.to_string());
        if mask.name == wanted {
            return;
        }
        mask.name = wanted;
    }

    refresh_masks(state);
    schedule_history_push(state);
}

fn duplicate_mask(state: &App, index: usize) {
    {
        let mut open = state.open.borrow_mut();
        let Some(photo) = open.as_mut() else { return };
        let mut masks = photo.document.masks();
        let Some(original) = masks.get(index).cloned() else { return };
        masks.insert(index + 1, original);
        photo.document.set_masks(masks);
        photo.view = None;
    }

    select_mask(state, Some(index + 1));
    refresh_masks(state);
    request_render(state);
    schedule_history_push(state);
}

fn set_mask_inverted(state: &App, index: usize, inverted: bool) {
    {
        let mut open = state.open.borrow_mut();
        let Some(photo) = open.as_mut() else { return };
        let Some(mask) = photo.document.mask_mut(index) else { return };
        if mask.inverted == inverted {
            return;
        }
        mask.inverted = inverted;
        photo.view = None;
    }

    refresh_masks(state);
    request_render(state);
    state.mask_area.queue_draw();
    schedule_history_push(state);
}

fn move_mask(state: &App, from: usize, to: usize) {
    if from == to {
        return;
    }

    let selected = state.selected_mask.get();
    {
        let mut open = state.open.borrow_mut();
        let Some(photo) = open.as_mut() else { return };
        let mut masks = photo.document.masks();
        if from >= masks.len() || to > masks.len() {
            return;
        }
        let mask = masks.remove(from);
        masks.insert(to.min(masks.len()), mask);
        photo.document.set_masks(masks);
        photo.view = None;
    }

    state.selected_mask.set(match selected {
        Some(at) if at == from => Some(to.min(index_limit(state))),
        Some(at) if at > from && at <= to => Some(at - 1),
        Some(at) if at < from && at >= to => Some(at + 1),
        other => other,
    });

    refresh_masks(state);
    request_render(state);
    schedule_history_push(state);
}

fn index_limit(state: &App) -> usize {
    state
        .open
        .borrow()
        .as_ref()
        .map(|photo| photo.document.masks().len().saturating_sub(1))
        .unwrap_or(0)
}

fn toggle_mask_part(state: &App, index: usize, part: MaskPart) {
    {
        let mut open = state.open.borrow_mut();
        let Some(photo) = open.as_mut() else { return };
        let Some(mask) = photo.document.mask_mut(index) else { return };
        match part {
            MaskPart::Class(class) => {
                if let Some(at) = mask.muted.iter().position(|held| *held == class) {
                    mask.muted.remove(at);
                } else {

                    let Shape::Segment { classes } = &mask.shape else { return };
                    let live = classes.iter().filter(|c| !mask.muted.contains(c)).count();
                    if live <= 1 {
                        return;
                    }
                    mask.muted.push(class);
                }
            }
            MaskPart::Point(at) => match mask.points.get_mut(at) {
                Some(point) => point.enabled = !point.enabled,
                None => return,
            },
            MaskPart::Stroke { at, run } => {
                let Some(first) = mask.strokes.get(at) else { return };
                let wanted = !first.enabled;
                for stroke in mask.strokes.iter_mut().skip(at).take(run) {
                    stroke.enabled = wanted;
                }
            }
        }
        photo.view = None;
    }

    rebuild_mask_map(state, index);
    refresh_masks(state);
    request_render(state);
    show_coverage(state);
    schedule_history_push(state);
}

fn remove_mask_part(state: &App, index: usize, part: MaskPart) {
    {
        let mut open = state.open.borrow_mut();
        let Some(photo) = open.as_mut() else { return };
        let Some(mask) = photo.document.mask_mut(index) else { return };
        match part {
            MaskPart::Class(class) => {
                let Shape::Segment { classes } = &mut mask.shape else { return };

                if classes.len() <= 1 {
                    return;
                }
                classes.retain(|held| *held != class);
            }
            MaskPart::Point(at) => {
                if at >= mask.points.len() {
                    return;
                }
                mask.points.remove(at);
            }
            MaskPart::Stroke { at, run } => {
                if at >= mask.strokes.len() {
                    return;
                }

                let end = (at + run).min(mask.strokes.len());
                mask.strokes.drain(at..end);
            }
        }
        photo.view = None;
    }

    rebuild_mask_map(state, index);
    refresh_masks(state);
    request_render(state);
    show_coverage(state);
    schedule_history_push(state);
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum MaskPart {
    Class(u16),
    Point(usize),

    Stroke { at: usize, run: usize },
}

fn remove_mask(state: &App, index: usize) {
    busy_sync(state, "Removing the mask…", move |state| remove_mask_now(state, index))
}

fn remove_mask_now(state: &App, index: usize) {
    {
        let mut open = state.open.borrow_mut();
        let Some(photo) = open.as_mut() else { return };
        let mut masks = photo.document.masks();
        if index >= masks.len() {
            return;
        }
        masks.remove(index);
        photo.document.set_masks(masks);
        photo.view = None;
    }

    select_mask(state, None);
    schedule_history_push(state);
}

fn select_mask(state: &App, index: Option<usize>) {
    timed("select_mask", || select_mask_now(state, index))
}

fn select_mask_now(state: &App, index: Option<usize>) {
    state.selected_mask.set(index);

    let target = {
        let open = state.open.borrow();
        let Some(photo) = open.as_ref() else { return };
        match index.and_then(|index| photo.document.masks().get(index).cloned()) {
            Some(mask) => mask.basic,
            None => {

                state.selected_mask.set(None);
                photo.document.basic()
            }
        }
    };

    let selected = state.selected_mask.get();
    let inverted = selected
        .and_then(|index| {
            let open = state.open.borrow();
            open.as_ref()?.document.masks().get(index).map(|mask| mask.inverted)
        })
        .unwrap_or(false);

    let _ = inverted;
    state.applying.set(true);
    state.sliders.write(target);
    refresh_slider_marks(state);
    state.applying.set(false);

    if selected != state.brush_owner.get() {
        state.brush_owner.set(selected);
        state.applying.set(true);
        state.brush_paint.set_active(false);
        state.brush_lasso.set_active(false);
        state.applying.set(false);
        state.brush.set(MaskTool::Off);
    }

    refresh_masks(state);

    set_panel_scope(state);

    if selected.is_some() {
        show_panel_tab(state, "light");
        show_coverage(state);
    }

    state.mask_area.set_visible(state.selected_mask.get().is_some());

    let picking = selected_mask(state)
        .is_some_and(|mask| matches!(mask.shape, Shape::ColourRange { .. } | Shape::LuminanceRange { .. }));
    let cursor = picking.then(|| {
        let fallback = gtk::gdk::Cursor::from_name("crosshair", None);
        gtk::gdk::Cursor::from_name("color-picker", fallback.as_ref())
    });
    state.mask_area.set_cursor(cursor.flatten().as_ref());

    state.mask_area.queue_draw();
    request_render(state);
}

fn build_mixer(state: &App) -> gtk::Box {
    let column = gtk::Box::new(gtk::Orientation::Vertical, 0);

    let colours = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    colours.add_css_class("mixer-bands");
    colours.set_margin_bottom(12);
    colours.set_halign(gtk::Align::Start);
    let mut first: Option<gtk::ToggleButton> = None;
    for (band, (name, _)) in BANDS.iter().enumerate() {
        let swatch = gtk::ToggleButton::new();

        swatch.set_halign(gtk::Align::Center);
        swatch.set_valign(gtk::Align::Center);
        swatch.set_tooltip_text(Some(name));
        swatch.add_css_class(&format!("band-{band}"));
        match &first {
            None => {
                swatch.set_active(true);
                first = Some(swatch.clone());
            }
            Some(first) => swatch.set_group(Some(first)),
        }
        swatch.connect_toggled(glib::clone!(
            #[strong] state,
            move |swatch| {
                if !swatch.is_active() || state.applying.get() {
                    return;
                }
                state.mixer_band.set(band);

                tint_mixer(&state);
                write_mixer(&state);
            }
        ));
        state.mixer_swatches.borrow_mut().push(swatch.clone());
        colours.append(&swatch);
    }

    let pipette = state.band_pipette.clone();

    pipette.add_css_class("band-pipette");
    pipette.set_icon_name("color-select-symbolic");
    pipette.set_tooltip_text(Some("Pick the colour from the photograph"));
    pipette.add_css_class("flat");
    pipette.set_valign(gtk::Align::Center);
    pipette.connect_toggled(glib::clone!(
        #[strong] state,
        move |button| {
            if state.applying.get() {
                return;
            }
            state.picking_band.set(button.is_active());
            if button.is_active() {
                disarm_pipettes(&state, "band");
            }
            arm_band_pipette(&state);
            if button.is_active() {
                state.toast("Click the photograph to choose the colour");
            }
        }
    ));
    colours.set_valign(gtk::Align::Center);
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    row.append(&colours);
    row.append(&pipette);
    row.set_margin_bottom(12);
    colours.set_margin_bottom(0);

    column.append(&row);

    for (channel, name) in ["Hue", "Saturation", "Luminance"].into_iter().enumerate() {
        let scale = &state.mixer_sliders[channel];
        scale.add_css_class("mixer-track");
        scale.add_css_class(match channel {
            0 => "track-hue",
            1 => "track-saturation",
            _ => "track-luminance",
        });
        scale.connect_value_changed(glib::clone!(
            #[strong] state,
            move |_| {
                if !state.applying.get() {
                    read_mixer(&state);
                }
            }
        ));
        column.append(&slider_row(state, name, scale, Readout::Signed(0)));
    }

    tint_mixer(state);
    column
}

fn tint_mixer(state: &App) {
    let band = state.mixer_band.get().min(BANDS.len() - 1);
    for scale in &state.mixer_sliders {
        for other in 0..BANDS.len() {
            scale.remove_css_class(&format!("band-{other}"));
        }
        scale.add_css_class(&format!("band-{band}"));
    }
}

fn build_retouch_overlay(state: &App) -> gtk::DrawingArea {
    let area = state.retouch_area.clone();
    area.set_visible(false);
    area.set_can_target(true);

    let grabbed: Rc<RefCell<Option<(usize, bool, Spot, f32, f32)>>> = Rc::new(RefCell::new(None));

    area.set_draw_func(glib::clone!(
        #[strong] state,
        move |_, context, width, height| {
            let spots = match state.open.borrow().as_ref() {
                Some(photo) => photo.document.retouch().spots,
                None => return,
            };
            let (left, top, shown_width, shown_height) =
                content_rect(&state, width as f64, height as f64);
            if shown_width <= 0.0 {
                return;
            }
            let long_edge = shown_width.max(shown_height);
            let selected = state.selected_spot.get();

            for (index, spot) in spots.iter().enumerate() {
                let radius = spot.radius as f64 * long_edge;
                let to = (
                    left + spot.at[0] as f64 * shown_width,
                    top + spot.at[1] as f64 * shown_height,
                );
                let from = (
                    left + spot.from[0] as f64 * shown_width,
                    top + spot.from[1] as f64 * shown_height,
                );
                let current = Some(index) == selected;
                let alpha = if current { 1.0 } else { 0.55 };

                context.set_source_rgba(0.0, 0.0, 0.0, 0.5 * alpha);
                context.set_line_width(3.0);
                context.move_to(from.0, from.1);
                context.line_to(to.0, to.1);
                let _ = context.stroke();
                context.set_source_rgba(1.0, 1.0, 1.0, 0.9 * alpha);
                context.set_line_width(1.0);
                context.move_to(from.0, from.1);
                context.line_to(to.0, to.1);
                let _ = context.stroke();

                for (centre, dashed) in [(to, false), (from, true)] {
                    context.set_dash(if dashed { &[5.0, 4.0] } else { &[] }, 0.0);
                    context.arc(centre.0, centre.1, radius, 0.0, std::f64::consts::TAU);
                    context.set_source_rgba(0.0, 0.0, 0.0, 0.6 * alpha);
                    context.set_line_width(3.0);
                    let _ = context.stroke_preserve();
                    context.set_source_rgba(1.0, 1.0, 1.0, 0.95 * alpha);
                    context.set_line_width(1.5);
                    let _ = context.stroke();
                }
                context.set_dash(&[], 0.0);
            }
        }
    ));

    let drag = gtk::GestureDrag::new();
    drag.connect_drag_begin(glib::clone!(
        #[strong] state,
        #[strong] grabbed,
        move |gesture, x, y| {
            let Some((u, v)) = retouch_point(&state, x, y) else { return };

            if let Some((index, source, spot)) = spot_at(&state, u, v) {

                gesture.set_state(gtk::EventSequenceState::Claimed);
                state.selected_spot.set(Some(index));
                *grabbed.borrow_mut() = Some((index, source, spot, u, v));
                refresh_retouch(&state);
                state.retouch_area.queue_draw();
                return;
            }

            if state.retouch_tool.get().is_none() {
                return;
            }
            let Some(index) = place_spot(&state, u, v) else { return };
            gesture.set_state(gtk::EventSequenceState::Claimed);
            let spot = current_spots(&state).get(index).copied().unwrap_or_default();

            *grabbed.borrow_mut() = Some((index, true, spot, u, v));
        }
    ));
    drag.connect_drag_update(glib::clone!(
        #[strong] state,
        #[strong] grabbed,
        move |_, dx, dy| {
            let held = *grabbed.borrow();
            let Some((index, source, original, _, _)) = held else { return };
            let Some((width, height)) = frame_size(&state) else { return };
            let moved = [(dx / width) as f32, (dy / height) as f32];

            let mut spots = current_spots(&state);
            let Some(spot) = spots.get_mut(index) else { return };
            let anchor = if source { original.from } else { original.at };
            let moved_to = [
                (anchor[0] + moved[0]).clamp(0.0, 1.0),
                (anchor[1] + moved[1]).clamp(0.0, 1.0),
            ];
            if source {
                spot.from = moved_to;
            } else {

                spot.at = moved_to;
                spot.from = [
                    (original.from[0] + moved[0]).clamp(0.0, 1.0),
                    (original.from[1] + moved[1]).clamp(0.0, 1.0),
                ];
            }
            write_spots(&state, spots, false);
        }
    ));
    drag.connect_drag_end(glib::clone!(
        #[strong] state,
        #[strong] grabbed,
        move |_, _, _| {

            let released = grabbed.borrow_mut().take();
            if released.is_some() {
                let spots = current_spots(&state);
                write_spots(&state, spots, true);
                refresh_retouch(&state);
            }
        }
    ));
    area.add_controller(drag);

    area
}

fn retouch_point(state: &App, x: f64, y: f64) -> Option<(f32, f32)> {
    let (left, top, width, height) = content_rect(
        state,
        state.retouch_area.width() as f64,
        state.retouch_area.height() as f64,
    );
    (width > 0.0 && height > 0.0)
        .then(|| (((x - left) / width) as f32, ((y - top) / height) as f32))
}

fn frame_size(state: &App) -> Option<(f64, f64)> {
    let (_, _, width, height) = content_rect(
        state,
        state.retouch_area.width() as f64,
        state.retouch_area.height() as f64,
    );
    (width > 0.0 && height > 0.0).then_some((width, height))
}

fn current_spots(state: &App) -> Vec<Spot> {
    state
        .open
        .borrow()
        .as_ref()
        .map(|photo| photo.document.retouch().spots)
        .unwrap_or_default()
}

fn spot_at(state: &App, u: f32, v: f32) -> Option<(usize, bool, Spot)> {
    let (width, height) = frame_size(state)?;
    let long_edge = width.max(height);
    let spots = current_spots(state);

    for (index, spot) in spots.iter().enumerate().rev() {
        let radius = spot.radius as f64 * long_edge;
        let reach = radius.max(8.0);
        for (source, centre) in [(true, spot.from), (false, spot.at)] {
            let dx = (u - centre[0]) as f64 * width;
            let dy = (v - centre[1]) as f64 * height;
            if dx.hypot(dy) <= reach {
                return Some((index, source, *spot));
            }
        }
    }
    None
}

fn place_spot(state: &App, u: f32, v: f32) -> Option<usize> {
    let radius = state.retouch_sliders[0].value() as f32 / 100.0;
    let feather = state.retouch_sliders[1].value() as f32 / 100.0;
    let opacity = state.retouch_sliders[2].value() as f32 / 100.0;

    let (width, height) = frame_size(state)?;
    let long_edge = width.max(height);
    let step = radius * 3.0 * (long_edge / width) as f32;
    let from_x = if u + step < 0.98 { u + step } else { u - step };

    let spot = Spot {
        at: [u, v],
        from: [from_x.clamp(0.0, 1.0), v],
        radius,
        feather,
        opacity,
        heal: state.retouch_tool.get()?,
    };

    let mut spots = current_spots(state);
    spots.push(spot);
    let index = spots.len() - 1;
    state.selected_spot.set(Some(index));
    write_spots(state, spots, true);
    refresh_retouch(state);
    refresh_face(state);
    refresh_found(state);
    Some(index)
}

fn write_spots(state: &App, spots: Vec<Spot>, settle: bool) {
    {
        let mut open = state.open.borrow_mut();
        let Some(photo) = open.as_mut() else { return };
        photo.document.set_retouch(Retouch { spots });
    }
    request_render(state);
    state.retouch_area.queue_draw();
    if settle {
        schedule_history_push(state);
    }
}

fn remove_spot(state: &App, index: usize) {
    let mut spots = current_spots(state);
    if index >= spots.len() {
        return;
    }
    spots.remove(index);
    state.selected_spot.set(None);
    write_spots(state, spots, true);
    refresh_retouch(state);
    refresh_face(state);
    refresh_found(state);
}

fn build_face(state: &App) -> gtk::Box {
    let column = state.face_section.clone();
    column.set_margin_top(14);
    column.append(&section_header("Face"));

    let note = state.face_note.clone();
    note.set_xalign(0.0);
    note.set_wrap(true);
    note.set_margin_bottom(6);
    note.add_css_class("profile-note");
    column.append(&note);

    let names = [
        ("Spots", "Take out what is small, dark and temporary — a pimple, not a freckle"),
        ("Skin", "Flatten what is blotchy and keep what is texture"),
        ("Evenness", "The same for colour alone, which is where blotchiness lives"),
        ("Red eye", "Only where the red is out of proportion — a brown iris is not red eye"),
        ("Teeth", "Only where a mouth has something bright and yellow in it"),
    ];
    for (index, (name, tooltip)) in names.into_iter().enumerate() {
        let scale = &state.face_sliders[index];
        scale.set_tooltip_text(Some(tooltip));
        scale.connect_value_changed(glib::clone!(
            #[strong] state,
            move |_| {
                if !state.applying.get() {
                    read_face(&state);
                }
            }
        ));
        let row = slider_row(state, name, scale, Readout::Positive(0));
        if index == 0 {
            row.add_css_class("lead");
        }
        column.append(&row);
    }

    column.set_visible(false);
    column
}

fn write_face(state: &App) {
    let beautify = state
        .open
        .borrow()
        .as_ref()
        .map(|photo| photo.document.beautify())
        .unwrap_or_default();

    state.applying.set(true);
    for (index, value) in
        [beautify.spots, beautify.skin, beautify.evenness, beautify.red_eye, beautify.teeth]
            .into_iter()
            .enumerate()
    {
        state.face_sliders[index].set_value(value as f64);
    }
    state.applying.set(false);
}

fn read_face(state: &App) {
    {
        let mut open = state.open.borrow_mut();
        let Some(photo) = open.as_mut() else { return };
        let mut beautify = photo.document.beautify();
        beautify.spots = state.face_sliders[0].value() as f32;
        beautify.skin = state.face_sliders[1].value() as f32;
        beautify.evenness = state.face_sliders[2].value() as f32;
        beautify.red_eye = state.face_sliders[3].value() as f32;
        beautify.teeth = state.face_sliders[4].value() as f32;
        photo.document.set_beautify(beautify);
    }

    request_render(state);
    schedule_history_push(state);
}

fn refresh_face(state: &App) {
    let found = state
        .open
        .borrow()
        .as_ref()
        .map(|photo| photo.document.faces.len())
        .unwrap_or(0);

    state.face_section.set_visible(found > 0);
    state.face_note.set_text(&match found {
        0 => String::new(),
        1 => "One face found.".to_string(),
        many => format!("{many} faces found — these apply to all of them."),
    });
    write_face(state);
}

fn ensure_faces(state: &App) {
    if !cull::faces::is_installed() {
        return;
    }
    let request = {
        let mut open = state.open.borrow_mut();
        let Some(photo) = open.as_mut() else { return };
        if !photo.document.faces.is_empty() || photo.faces_pending {
            return;
        }

        let mut geometry = Document::new(photo.document.source.path.clone());
        geometry.set_perspective(photo.document.perspective());
        if let Some((rect, angle)) = photo.document.crop() {
            geometry.set_crop(rect, angle);
        }
        geometry.set_rotation(photo.document.rotation());
        geometry.set_mirrored(photo.document.mirrored());
        photo.faces_pending = true;
        (geometry, photo.working.clone())
    };

    let (geometry, working) = request;
    let generation = state.open_generation.get();
    let state = state.clone();
    glib::spawn_future_local(async move {
        let found = busy(&state, "Looking for faces…", move || {
            let frame = render::apply_stack(&geometry, &working, 1.0);
            let (width, height) = (frame.width() as f32, frame.height() as f32);
            cull::faces::detect(&frame).map(|faces| {
                let portraits = faces
                    .iter()
                    .map(|face| Portrait {
                        at: [
                            face.x / width,
                            face.y / height,
                            face.width / width,
                            face.height / height,
                        ],
                        points: face.landmarks.map(|(x, y)| [x / width, y / height]),
                    })
                    .collect::<Vec<_>>();

                let people = faces
                    .iter()
                    .filter_map(|face| {
                        Some(SeenFace {
                            at: [
                                face.x / width,
                                face.y / height,
                                face.width / width,
                                face.height / height,
                            ],
                            embedding: cull::people::embed(&frame, face)?,
                            portrait: cull::people::align(&frame, face)?,
                        })
                    })
                    .collect::<Vec<_>>();
                (portraits, people)
            })
        })
        .await;

        if state.open_generation.get() != generation {
            return;
        }

        let any = {
            let mut open = state.open.borrow_mut();
            let Some(photo) = open.as_mut() else { return };
            photo.faces_pending = false;
            let (portraits, people) = found.ok().flatten().unwrap_or_default();
            photo.document.faces = portraits;
            photo.people = people;
            !photo.document.faces.is_empty()
        };

        refresh_face(&state);
        refresh_found(&state);
        refresh_info(&state);

        refresh_face_names(&state);
        if any {
            request_render(&state);
        }
    });
}

fn build_retouch(state: &App) -> gtk::Box {
    let column = gtk::Box::new(gtk::Orientation::Vertical, 0);

    let tools = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    tools.add_css_class("linked");
    tools.set_margin_bottom(8);

    for (button, other, label, tooltip, heal) in [
        (
            &state.retouch_heal,
            &state.retouch_clone,
            "Heal",
            "The source's texture, this place's brightness",
            true,
        ),
        (
            &state.retouch_clone,
            &state.retouch_heal,
            "Clone",
            "The source exactly as it is",
            false,
        ),
    ] {
        button.set_label(label);
        button.set_hexpand(true);
        button.set_tooltip_text(Some(tooltip));
        button.connect_toggled(glib::clone!(
            #[strong] state,
            #[weak] other,
            move |button| {
                if state.applying.get() {
                    return;
                }
                state.applying.set(true);
                if button.is_active() {
                    other.set_active(false);
                }
                state.applying.set(false);

                state.retouch_tool.set(button.is_active().then_some(heal));

                if button.is_active() {
                    if let Some(index) = state.selected_spot.get() {
                        let mut spots = current_spots(&state);
                        if let Some(spot) = spots.get_mut(index) {
                            spot.heal = heal;
                            write_spots(&state, spots, true);
                            refresh_retouch(&state);
                        }
                    }
                }
            }
        ));
        tools.append(button);
    }
    column.append(&tools);

    for (index, value) in [(0usize, 3.0), (1, 50.0), (2, 100.0)] {
        state.retouch_sliders[index].set_value(value);
        set_neutral(&state.retouch_sliders[index], value);
    }

    let names = [
        ("Size", Readout::Positive(1)),
        ("Feather", Readout::Positive(0)),
        ("Strength", Readout::Positive(0)),
    ];
    for (index, (name, readout)) in names.into_iter().enumerate() {
        let scale = &state.retouch_sliders[index];
        scale.connect_value_changed(glib::clone!(
            #[strong] state,
            move |scale| {
                if state.applying.get() {
                    return;
                }

                let Some(at) = state.selected_spot.get() else { return };
                let mut spots = current_spots(&state);
                let Some(spot) = spots.get_mut(at) else { return };
                let value = scale.value() as f32;
                match index {
                    0 => spot.radius = value / 100.0,
                    1 => spot.feather = value / 100.0,
                    _ => spot.opacity = value / 100.0,
                }
                write_spots(&state, spots, true);
            }
        ));
        let row = slider_row(state, name, scale, readout);
        if index == 0 {
            row.add_css_class("lead");
        }
        column.append(&row);
    }

    let note = gtk::Label::new(Some(
        "Click the blemish, then drag to where the replacement should come from.",
    ));
    note.set_xalign(0.0);
    note.set_wrap(true);
    note.set_margin_top(4);
    note.set_margin_bottom(8);
    note.add_css_class("profile-note");
    column.append(&note);

    state.retouch_list.set_selection_mode(gtk::SelectionMode::None);
    state.retouch_list.add_css_class("boxed-list");
    column.append(&state.retouch_list);

    column
}

fn refresh_retouch(state: &App) {
    let spots = current_spots(state);
    let selected = state.selected_spot.get().filter(|index| *index < spots.len());
    state.selected_spot.set(selected);

    while let Some(row) = state.retouch_list.first_child() {
        state.retouch_list.remove(&row);
    }

    for (index, spot) in spots.iter().enumerate() {
        let row = adw::ActionRow::new();
        row.set_title(&format!(
            "{} {}",
            if spot.heal { "Healed" } else { "Cloned" },
            index + 1
        ));
        row.set_subtitle(&format!("{:.1} % across", spot.radius * 100.0));
        row.add_css_class("mask-part");
        row.set_activatable(true);
        if Some(index) == selected {
            row.add_css_class("current-mask");
        }
        row.connect_activated(glib::clone!(
            #[strong] state,
            move |_| {
                state.selected_spot.set(Some(index));
                refresh_retouch(&state);
        refresh_face(&state);
        refresh_found(&state);
                state.retouch_area.queue_draw();
            }
        ));

        let remove = gtk::Button::from_icon_name("window-close-symbolic");
        remove.set_valign(gtk::Align::Center);
        remove.add_css_class("flat");
        remove.set_tooltip_text(Some("Put this back the way it was"));
        remove.connect_clicked(glib::clone!(
            #[strong] state,
            move |_| remove_spot(&state, index)
        ));
        row.add_suffix(&remove);
        state.retouch_list.append(&row);
    }

    state.retouch_list.set_visible(!spots.is_empty());

    if let Some(spot) = selected.and_then(|index| spots.get(index)) {
        state.applying.set(true);
        state.retouch_sliders[0].set_value(spot.radius as f64 * 100.0);
        state.retouch_sliders[1].set_value(spot.feather as f64 * 100.0);
        state.retouch_sliders[2].set_value(spot.opacity as f64 * 100.0);

        let _ = spot.heal;
        state.applying.set(false);
    }
}

fn toggle_retouch(state: &App, on: bool) {
    state.retouch_on.set(on);
    state.retouch_area.set_visible(on);
    if !on {
        state.selected_spot.set(None);
    }

    state.retouch_tool.set(None);
    state.applying.set(true);
    state.retouch_heal.set_active(false);
    state.retouch_clone.set_active(false);
    state.applying.set(false);
    refresh_retouch(state);
    refresh_face(state);
    refresh_found(state);
    state.retouch_area.queue_draw();
}

fn build_grading(state: &App) -> gtk::Box {
    let column = gtk::Box::new(gtk::Orientation::Vertical, 0);

    let ranges = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    ranges.add_css_class("linked");
    ranges.add_css_class("aspect-ratios");
    ranges.set_margin_bottom(6);
    let mut first: Option<gtk::ToggleButton> = None;
    for (index, name) in ["Shadow", "Mid", "High", "All"].iter().enumerate() {
        let tab = gtk::ToggleButton::with_label(name);
        tab.set_hexpand(true);
        match &first {
            None => {
                tab.set_active(true);
                first = Some(tab.clone());
            }
            Some(first) => tab.set_group(Some(first)),
        }
        tab.connect_toggled(glib::clone!(
            #[strong] state,
            move |tab| {
                if !tab.is_active() {
                    return;
                }
                state.grading_range.set(index);

                write_grading(&state);
        refresh_retouch(&state);
        refresh_face(&state);
        refresh_found(&state);
            }
        ));
        ranges.append(&tab);
    }
    column.append(&ranges);

    let names = [("Hue", Readout::Degrees), ("Saturation", Readout::Positive(0)), ("Brightness", Readout::Signed(0))];
    for (index, (name, readout)) in names.into_iter().enumerate() {
        let scale = &state.grading_sliders[index];
        if index == 0 {

            scale.add_css_class("hue-slider");
        }
        scale.connect_value_changed(glib::clone!(
            #[strong] state,
            move |_| {
                if !state.applying.get() {
                    read_grading(&state);
                }
                tint_grading_hue(&state);
            }
        ));
        column.append(&slider_row(state, name, scale, readout));
    }

    let shaping = [
        ("Blending", Readout::Positive(0)),
        ("Balance", Readout::Signed(0)),
    ];
    set_neutral(&state.grading_shape[0], Grading::default().blending as f64);
    for (index, (name, readout)) in shaping.into_iter().enumerate() {
        let scale = &state.grading_shape[index];
        scale.connect_value_changed(glib::clone!(
            #[strong] state,
            move |_| {
                if !state.applying.get() {
                    read_grading(&state);
                }
            }
        ));
        column.append(&slider_row(state, name, scale, readout));
    }

    column
}

fn tint_grading_hue(state: &App) {
    let idle = state.grading_sliders[1].value() <= 0.0;
    let hue = &state.grading_sliders[0];
    if idle {
        hue.add_css_class("idle");
    } else {
        hue.remove_css_class("idle");
    }
}

fn write_grading(state: &App) {
    let grading = state
        .open
        .borrow()
        .as_ref()
        .map(|photo| photo.document.grading())
        .unwrap_or_default();
    let range = range_of(&grading, state.grading_range.get());

    state.applying.set(true);
    state.grading_sliders[0].set_value(range.hue as f64);
    state.grading_sliders[1].set_value(range.saturation as f64);
    state.grading_sliders[2].set_value(range.luminance as f64);
    state.grading_shape[0].set_value(grading.blending as f64);
    state.grading_shape[1].set_value(grading.balance as f64);
    state.applying.set(false);
    tint_grading_hue(state);
}

fn read_grading(state: &App) {
    {
        let mut open = state.open.borrow_mut();
        let Some(photo) = open.as_mut() else { return };
        let mut grading = photo.document.grading();
        {
            let range = range_of_mut(&mut grading, state.grading_range.get());
            range.hue = state.grading_sliders[0].value() as f32;
            range.saturation = state.grading_sliders[1].value() as f32;
            range.luminance = state.grading_sliders[2].value() as f32;
        }
        grading.blending = state.grading_shape[0].value() as f32;
        grading.balance = state.grading_shape[1].value() as f32;
        photo.document.set_grading(grading);
    }

    request_render(state);
    schedule_history_push(state);
}

fn range_of(grading: &Grading, which: usize) -> Range {
    match which {
        0 => grading.shadows,
        1 => grading.midtones,
        2 => grading.highlights,
        _ => grading.global,
    }
}

fn range_of_mut(grading: &mut Grading, which: usize) -> &mut Range {
    match which {
        0 => &mut grading.shadows,
        1 => &mut grading.midtones,
        2 => &mut grading.highlights,
        _ => &mut grading.global,
    }
}

fn write_mixer(state: &App) {
    let mixer = state.open.borrow().as_ref().map(|photo| photo.document.mixer());
    let mixer = mixer.unwrap_or_default();
    let band = state.mixer_band.get().min(BANDS.len() - 1);

    state.applying.set(true);
    for (channel, scale) in state.mixer_sliders.iter().enumerate() {
        scale.set_value(mixer.bands[band][channel] as f64);
    }
    state.applying.set(false);
}

fn read_mixer(state: &App) {
    let band = state.mixer_band.get().min(BANDS.len() - 1);
    {
        let mut open = state.open.borrow_mut();
        let Some(photo) = open.as_mut() else { return };
        let mut mixer = photo.document.mixer();
        for (channel, scale) in state.mixer_sliders.iter().enumerate() {
            mixer.bands[band][channel] = scale.value() as f32;
        }
        photo.document.set_mixer(mixer);

        photo.view = None;
    }

    request_render(state);
    schedule_history_push(state);
}

fn disarm_pipettes(state: &App, keep: &str) {
    state.applying.set(true);
    for (name, button, flag) in [
        ("band", &state.band_pipette, &state.picking_band),
        ("white", &state.white_pipette, &state.picking_white),
        ("point", &state.point_pipette, &state.picking_point),
    ] {
        if name != keep {
            button.set_active(false);
            flag.set(false);
        }
    }
    state.applying.set(false);
}

fn build_point_colours(state: &App) -> gtk::Box {
    let column = gtk::Box::new(gtk::Orientation::Vertical, 0);

    let row = gtk::Box::new(gtk::Orientation::Horizontal, 10);
    row.set_margin_bottom(12);
    let pipette = state.point_pipette.clone();
    pipette.set_icon_name("color-select-symbolic");
    pipette.set_tooltip_text(Some("Pick a colour from the photograph to adjust"));
    pipette.add_css_class("flat");
    pipette.set_valign(gtk::Align::Center);
    pipette.connect_toggled(glib::clone!(
        #[strong] state,
        move |button| {
            if state.applying.get() {
                return;
            }
            state.picking_point.set(button.is_active());
            if button.is_active() {
                disarm_pipettes(&state, "point");
                state.toast("Click the photograph to pick a colour");
            }
            arm_band_pipette(&state);
        }
    ));
    state.point_swatches.set_valign(gtk::Align::Center);
    row.append(&state.point_swatches);
    row.append(&pipette);
    column.append(&row);

    let controls = state.point_controls.clone();
    let names = [
        ("Hue", Readout::Signed(0)),
        ("Saturation", Readout::Signed(0)),
        ("Luminance", Readout::Signed(0)),
        ("Range", Readout::Middle),
    ];
    for (index, (name, readout)) in names.into_iter().enumerate() {
        let scale = &state.point_sliders[index];
        scale.connect_value_changed(glib::clone!(
            #[strong] state,
            move |_| {
                if !state.applying.get() {
                    read_point_colours(&state);
                }
            }
        ));
        controls.append(&slider_row(state, name, scale, readout));
    }

    let actions = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    let show = state.point_show.clone();
    show.set_label("Show affected area");
    show.set_tooltip_text(Some("Grey out everything this colour does not reach"));
    show.connect_toggled(glib::clone!(
        #[strong] state,
        move |_| request_render(&state)
    ));
    let remove = gtk::Button::from_icon_name("user-trash-symbolic");
    remove.add_css_class("flat");
    remove.set_tooltip_text(Some("Remove this colour"));
    remove.connect_clicked(glib::clone!(
        #[strong] state,
        move |_| {
            {
                let mut open = state.open.borrow_mut();
                let Some(photo) = open.as_mut() else { return };
                let mut points = photo.document.point_colours();
                let selected = state.point_selected.get();
                if selected < points.points.len() {
                    points.points.remove(selected);
                }
                state.point_selected.set(selected.saturating_sub(1));
                photo.document.set_point_colours(points);
            }
            write_point_colours(&state);
            request_render(&state);
            schedule_history_push(&state);
        }
    ));
    actions.append(&show);
    actions.append(&remove);
    controls.append(&actions);
    column.append(&controls);

    write_point_colours(state);
    column
}

fn write_point_colours(state: &App) {
    let points = state.open.borrow().as_ref().map(|photo| photo.document.point_colours());
    let points = points.unwrap_or_default().points;
    let selected = state.point_selected.get().min(points.len().saturating_sub(1));
    state.point_selected.set(selected);

    while let Some(child) = state.point_swatches.first_child() {
        state.point_swatches.remove(&child);
    }
    let mut first: Option<gtk::ToggleButton> = None;
    for (index, point) in points.iter().enumerate() {
        let swatch = gtk::ToggleButton::new();
        swatch.set_tooltip_text(Some("Adjust this colour"));
        swatch.set_valign(gtk::Align::Center);
        let dot = gtk::DrawingArea::new();
        dot.set_content_width(16);
        dot.set_content_height(16);
        let colour = point.swatch().map(|linear| {
            let linear = linear.clamp(0.0, 1.0) as f64;
            if linear <= 0.003_130_8 { linear * 12.92 } else { 1.055 * linear.powf(1.0 / 2.4) - 0.055 }
        });
        dot.set_draw_func(move |_, context, width, height| {
            context.set_source_rgb(colour[0], colour[1], colour[2]);
            let radius = width.min(height) as f64 / 2.0;
            context.arc(width as f64 / 2.0, height as f64 / 2.0, radius, 0.0, std::f64::consts::TAU);
            let _ = context.fill();
        });
        swatch.set_child(Some(&dot));
        match &first {
            None => first = Some(swatch.clone()),
            Some(first) => swatch.set_group(Some(first)),
        }
        swatch.set_active(index == selected);
        swatch.connect_toggled(glib::clone!(
            #[strong] state,
            move |swatch| {
                if swatch.is_active() && state.point_selected.replace(index) != index {
                    write_point_colours(&state);
                }
            }
        ));
        state.point_swatches.append(&swatch);
    }

    state.point_controls.set_visible(!points.is_empty());
    let point = points.get(selected).copied().unwrap_or_default();
    state.applying.set(true);
    if points.is_empty() {
        state.point_show.set_active(false);
    }
    for (scale, value) in state
        .point_sliders
        .iter()
        .zip([point.hue, point.saturation, point.luminance, point.range])
    {
        scale.set_value(value as f64);
    }
    state.applying.set(false);

    if state.point_show.is_active() {
        request_render(state);
    }
}

fn read_point_colours(state: &App) {
    {
        let mut open = state.open.borrow_mut();
        let Some(photo) = open.as_mut() else { return };
        let mut points = photo.document.point_colours();
        let Some(point) = points.points.get_mut(state.point_selected.get()) else { return };
        let value = |index: usize| state.point_sliders[index].value() as f32;
        point.hue = value(0);
        point.saturation = value(1);
        point.luminance = value(2);
        point.range = value(3);
        photo.document.set_point_colours(points);
    }
    request_render(state);
    schedule_history_push(state);
}

fn pick_point(state: &App, u: f32, v: f32) {
    disarm_pipettes(state, "");
    arm_band_pipette(state);

    let Some(frame) = mask_frame(state) else { return };
    let (width, height) = (frame.width() as i64, frame.height() as i64);
    let (cx, cy) = ((u.clamp(0.0, 1.0) * width as f32) as i64, (v.clamp(0.0, 1.0) * height as f32) as i64);
    let decode = |byte: u8| {
        let value = byte as f32 / 255.0;
        if value <= 0.040_45 { value / 12.92 } else { ((value + 0.055) / 1.055).powf(2.4) }
    };
    let mut sum = [0.0f32; 3];
    let mut count = 0.0;
    for y in (cy - 2).max(0)..(cy + 3).min(height) {
        for x in (cx - 2).max(0)..(cx + 3).min(width) {
            let pixel = frame.get_pixel(x as u32, y as u32).0;
            for channel in 0..3 {
                sum[channel] += decode(pixel[channel]);
            }
            count += 1.0;
        }
    }
    if count == 0.0 {
        return;
    }

    {
        let mut open = state.open.borrow_mut();
        let Some(photo) = open.as_mut() else { return };
        let mut points = photo.document.point_colours();
        points.points.push(PointColour::picked(sum.map(|value| value / count)));
        state.point_selected.set(points.points.len() - 1);
        photo.document.set_point_colours(points);
    }
    write_point_colours(state);
    request_render(state);
    schedule_history_push(state);
}

fn fill_presets_menu(state: &App) {

    let menu = &state.presets_menu;
    menu.remove_all();
    menu.append(Some("Apply a preset…"), Some("win.preset-choose"));
    menu.append(Some("Save settings as preset…"), Some("win.preset-save"));
    let manage = gio::Menu::new();
    manage.append(Some("Import presets…"), Some("win.preset-import"));
    manage.append(Some("Import a folder of presets…"), Some("win.preset-import-folder"));
    manage.append(Some("Open presets folder"), Some("win.preset-folder"));
    menu.append_section(None, &manage);
}

fn preset_picker(state: &App, window: &adw::ApplicationWindow) {
    let dialog = adw::Dialog::new();
    dialog.set_title("Presets");
    dialog.set_content_width(460);
    dialog.set_content_height(560);

    let column = preset_browser(state, glib::clone!(
        #[weak] dialog,
        move || {
            dialog.close();
        }
    ));
    column.set_margin_start(12);
    column.set_margin_end(12);
    column.set_margin_bottom(12);

    let bar = adw::ToolbarView::new();
    bar.add_top_bar(&adw::HeaderBar::new());
    bar.set_content(Some(&column));
    dialog.set_child(Some(&bar));
    dialog.present(Some(window));
    if let Some(search) = column.first_child() {
        search.grab_focus();
    }
}

fn fill_presets_page(state: &App) {
    let page = &state.presets_page;
    while let Some(child) = page.first_child() {
        page.remove(&child);
    }
    let browser = preset_browser(state, || {});
    browser.set_vexpand(true);
    page.append(&browser);

    let actions = gtk::FlowBox::new();
    actions.set_selection_mode(gtk::SelectionMode::None);
    actions.set_column_spacing(6);
    actions.set_row_spacing(6);
    actions.set_margin_top(6);
    for (label, action) in [
        ("Save current…", "win.preset-save"),
        ("Import…", "win.preset-import"),
        ("Import folder…", "win.preset-import-folder"),
        ("Open folder", "win.preset-folder"),
    ] {
        let button = gtk::Button::with_label(label);
        button.set_action_name(Some(action));
        actions.append(&button);
    }
    page.append(&actions);
}

fn preset_browser(state: &App, done: impl Fn() + Clone + 'static) -> gtk::Box {
    let names = numa::io::presets::list(&numa::io::presets::dir());
    let column = gtk::Box::new(gtk::Orientation::Vertical, 8);
    let search = gtk::SearchEntry::new();
    search.set_placeholder_text(Some("Search presets and groups"));
    column.append(&search);

    if names.is_empty() {
        let empty = adw::StatusPage::new();
        empty.set_title("No presets yet");
        empty.set_description(Some("Save the settings of a photograph, or import presets from Lightroom or Capture One."));
        empty.set_vexpand(true);
        column.append(&empty);
    } else {
        let model = gtk::StringList::new(&names.iter().map(String::as_str).collect::<Vec<_>>());
        let filter = gtk::StringFilter::new(Some(gtk::PropertyExpression::new(
            gtk::StringObject::static_type(),
            gtk::Expression::NONE,
            "string",
        )));
        filter.set_ignore_case(true);
        filter.set_match_mode(gtk::StringFilterMatchMode::Substring);
        search.bind_property("text", &filter, "search").build();
        let filtered = gtk::FilterListModel::new(Some(model), Some(filter));
        let selection = gtk::NoSelection::new(Some(filtered.clone()));

        let factory = gtk::SignalListItemFactory::new();
        factory.connect_setup(|_, item| {
            let row = gtk::Box::new(gtk::Orientation::Vertical, 2);
            row.set_margin_top(6);
            row.set_margin_bottom(6);
            let title = gtk::Label::new(None);
            title.set_xalign(0.0);
            title.set_ellipsize(gtk::pango::EllipsizeMode::End);
            let group = gtk::Label::new(None);
            group.set_xalign(0.0);
            group.set_ellipsize(gtk::pango::EllipsizeMode::End);
            group.add_css_class("dim-label");
            group.add_css_class("caption");
            row.append(&title);
            row.append(&group);
            item.downcast_ref::<gtk::ListItem>().map(|item| item.set_child(Some(&row)));
        });
        factory.connect_bind(|_, item| {
            let Some(item) = item.downcast_ref::<gtk::ListItem>() else { return };
            let Some(name) = item.item().and_downcast::<gtk::StringObject>().map(|s| s.string()) else { return };
            let Some(row) = item.child().and_downcast::<gtk::Box>() else { return };
            let (group, title) = match name.split_once('/') {
                Some((group, title)) => (group.to_string(), title.to_string()),
                None => (String::new(), name.to_string()),
            };
            if let Some(label) = row.first_child().and_downcast::<gtk::Label>() {
                label.set_text(&title);
            }
            if let Some(label) = row.last_child().and_downcast::<gtk::Label>() {
                label.set_visible(!group.is_empty());
                label.set_text(&group);
            }
        });

        let list = gtk::ListView::new(Some(selection), Some(factory));
        list.set_single_click_activate(true);
        list.add_css_class("navigation-sidebar");
        let apply = glib::clone!(
            #[strong] state,
            #[strong] filtered,
            move |position: u32| {
                let Some(name) = filtered.item(position).and_downcast::<gtk::StringObject>().map(|s| s.string()) else {
                    return;
                };
                done();
                let label = name.rsplit('/').next().unwrap_or(&name).to_string();
                match numa::io::presets::load(&numa::io::presets::dir(), &name) {
                    Ok(preset) => apply_edit(&state, &preset.document, preset.parts, &format!("“{label}” applied to")),
                    Err(err) => state.toast(&err),
                }
            }
        );
        list.connect_activate({
            let apply = apply.clone();
            move |_, position| apply(position)
        });

        search.connect_activate({
            let filtered = filtered.clone();
            move |_| {
                if filtered.n_items() > 0 {
                    apply(0);
                }
            }
        });

        let scroller = gtk::ScrolledWindow::new();
        scroller.set_vexpand(true);
        scroller.set_child(Some(&list));
        column.append(&scroller);
    }
    column
}

fn edit_here(state: &App) -> Result<Document, String> {
    if let Some(photo) = state.open.borrow().as_ref() {
        return Ok(photo.document.clone());
    }
    let id = selected_cards(state)
        .first()
        .and_then(|child| child.widget_name().parse::<i64>().ok())
        .ok_or("Select a photo to make a preset from")?;
    state.catalog.load_edits(id)?.ok_or_else(|| "That photo has no edits to keep".to_string())
}

fn install_preset_actions(state: &App, window: &adw::ApplicationWindow) {
    let apply = gio::SimpleAction::new("preset-choose", None);
    apply.connect_activate(glib::clone!(
        #[strong] state,
        #[weak] window,
        move |_, _| preset_picker(&state, &window)
    ));

    let save = gio::SimpleAction::new("preset-save", None);
    save.connect_activate(glib::clone!(
        #[strong] state,
        #[weak] window,
        move |_, _| match edit_here(&state) {
            Ok(source) => save_preset_dialog(&state, &window, source),
            Err(err) => state.toast(&err),
        }
    ));

    let import = gio::SimpleAction::new("preset-import", None);
    import.connect_activate(glib::clone!(
        #[strong] state,
        #[weak] window,
        move |_, _| {
            let dialog = gtk::FileDialog::new();
            dialog.set_title("Import presets");
            let filter = gtk::FileFilter::new();
            filter.set_name(Some("Presets — Numa, Lightroom, Capture One"));
            for suffix in ["json", "xmp", "lrtemplate", "costyle", "costylepack"] {
                filter.add_suffix(suffix);
            }
            let filters = gio::ListStore::new::<gtk::FileFilter>();
            filters.append(&filter);
            dialog.set_filters(Some(&filters));
            let (state, parent) = (state.clone(), window.clone());
            dialog.open_multiple(Some(&window), gio::Cancellable::NONE, move |chosen| {
                let Ok(files) = chosen else { return };
                import_presets(&state, &parent, chosen_paths(&files));
            });
        }
    ));
    let import_folder = gio::SimpleAction::new("preset-import-folder", None);
    import_folder.connect_activate(glib::clone!(
        #[strong] state,
        #[weak] window,
        move |_, _| {
            let dialog = gtk::FileDialog::new();
            dialog.set_title("Import a folder of presets");
            let (state, parent) = (state.clone(), window.clone());
            dialog.select_multiple_folders(Some(&window), gio::Cancellable::NONE, move |chosen| {
                let Ok(folders) = chosen else { return };
                import_presets(&state, &parent, chosen_paths(&folders));
            });
        }
    ));

    let folder = gio::SimpleAction::new("preset-folder", None);
    folder.connect_activate(glib::clone!(
        #[weak] window,
        move |_, _| {
            let dir = numa::io::presets::dir();
            if let Err(err) = std::fs::create_dir_all(&dir) {
                log::warn!("could not create {}: {err}", dir.display());
            }
            gtk::FileLauncher::new(Some(&gio::File::for_path(&dir))).launch(
                Some(&window),
                None::<&gio::Cancellable>,
                |result| {
                    if let Err(err) = result {
                        log::warn!("could not open the presets folder: {err}");
                    }
                },
            );
        }
    ));

    for action in [&apply, &save, &import, &import_folder, &folder] {
        window.add_action(action);
    }
    fill_presets_menu(state);
}

fn chosen_paths(files: &gio::ListModel) -> Vec<PathBuf> {
    (0..files.n_items())
        .filter_map(|index| files.item(index).and_downcast::<gio::File>()?.path())
        .collect()
}

fn import_presets(state: &App, window: &adw::ApplicationWindow, paths: Vec<PathBuf>) {
    if paths.is_empty() {
        return;
    }
    let (state, window) = (state.clone(), window.clone());
    glib::spawn_future_local(async move {
        let Ok(imported) = busy(&state, "Importing presets…", move || {
            numa::io::presets::import(&numa::io::presets::dir(), &paths)
        })
        .await
        else {
            state.toast("Importing failed — see the log");
            return;
        };
        for refused in &imported.refused {
            log::info!("preset not imported: {refused}");
        }
        fill_presets_page(&state);

        let mut said = match imported.presets {
            1 => "Imported 1 preset".to_string(),
            n => format!("Imported {n} presets"),
        };
        if imported.existing > 0 {
            said.push_str(&format!(" · {} already there", imported.existing));
        }
        if !imported.refused.is_empty() {
            said.push_str(&format!(" · {} not usable", imported.refused.len()));
        }

        if imported.ignored.is_empty() {
            state.toast(&said);
            return;
        }
        let mut lines: Vec<(&str, usize)> = imported.ignored.iter().map(|(what, n)| (*what, *n)).collect();
        lines.sort_by(|a, b| b.1.cmp(&a.1));
        let body = format!(
            "Translated presets come close rather than exactly: the sliders mean \
             slightly different things in each application.\n\nNot carried over:\n{}",
            lines.iter().map(|(what, n)| format!("• {what} — in {n}")).collect::<Vec<_>>().join("\n")
        );
        let alert = adw::AlertDialog::new(Some(&said), Some(&body));
        alert.add_response("ok", "OK");
        alert.present(Some(&window));
    });
}

fn save_preset_dialog(state: &App, window: &adw::ApplicationWindow, source: Document) {
    let dialog = adw::Dialog::new();
    dialog.set_title("Save as preset");
    dialog.set_content_width(420);

    let page = adw::PreferencesPage::new();
    let naming = adw::PreferencesGroup::new();
    let name = adw::EntryRow::new();
    name.set_title("Name");
    naming.add(&name);
    page.add(&naming);

    let group = adw::PreferencesGroup::new();
    group.set_title("What it carries");
    let chosen = parts_checklist(&group, state.clipboard_parts.get());
    page.add(&group);

    let save = gtk::Button::with_label("Save");
    save.add_css_class("suggested-action");
    save.set_halign(gtk::Align::End);
    save.set_margin_top(12);
    save.connect_clicked(glib::clone!(
        #[strong] state,
        #[weak] dialog,
        #[weak] name,
        move |_| {
            let parts = chosen();
            if !parts.any() {
                state.toast("Choose at least one part for the preset");
                return;
            }
            let mut document = Document::new(String::new());
            document.copy_from(&source, parts);
            let preset = numa::io::presets::Preset { parts, document };
            match numa::io::presets::save(&numa::io::presets::dir(), &name.text(), &preset) {
                Ok(()) => {
                    fill_presets_page(&state);
                    state.toast(&format!("Saved “{}”", name.text().trim()));
                    dialog.close();
                }
                Err(err) => state.toast(&err),
            }
        }
    ));

    let column = gtk::Box::new(gtk::Orientation::Vertical, 0);
    column.append(&page);
    column.append(&save);
    column.set_margin_bottom(12);
    column.set_margin_end(12);

    let bar = adw::ToolbarView::new();
    bar.add_top_bar(&adw::HeaderBar::new());
    bar.set_content(Some(&column));
    dialog.set_child(Some(&bar));
    dialog.present(Some(window));
}

fn copy_settings(state: &App) {
    let copied = state.open.borrow().as_ref().map(|photo| photo.document.clone());
    match copied {
        Some(document) => {
            *state.clipboard.borrow_mut() = Some(document);
            state.toast("Settings copied");
        }
        None => state.toast("Open a photo to copy its settings"),
    }
}

fn copy_from_selection(state: &App) {
    if state.open.borrow().is_some() {
        copy_settings(state);
        return;
    }

    let Some(child) = selected_cards(state).first().cloned() else {
        state.toast("Select a photo to copy its settings");
        return;
    };
    let Ok(id) = child.widget_name().parse::<i64>() else { return };

    match state.catalog.load_edits(id) {
        Ok(Some(document)) => {
            *state.clipboard.borrow_mut() = Some(document);
            state.toast("Settings copied");
        }

        Ok(None) => state.toast("That photo has no edits to copy"),
        Err(err) => state.toast(&format!("Could not read its settings: {err}")),
    }
}

fn paste_settings(state: &App, window: &adw::ApplicationWindow, ask: bool) {
    if state.clipboard.borrow().is_none() {
        state.toast("Nothing copied yet");
        return;
    }

    if !ask {
        apply_clipboard(state);
        return;
    }

    let dialog = adw::Dialog::new();
    dialog.set_title("Paste settings");
    dialog.set_content_width(420);

    let page = adw::PreferencesPage::new();
    let group = adw::PreferencesGroup::new();
    group.set_title("What travels");
    group.set_description(Some(
        "The crop is off by default: it is drawn against one photograph's \
         content and rarely means the same thing on another.",
    ));

    let chosen = parts_checklist(&group, state.clipboard_parts.get());

    let apply = gtk::Button::with_label("Paste");
    apply.add_css_class("suggested-action");
    apply.set_halign(gtk::Align::End);
    apply.set_margin_top(12);
    apply.connect_clicked(glib::clone!(
        #[strong] state,
        #[weak] dialog,
        move |_| {
            state.clipboard_parts.set(chosen());
            dialog.close();
            apply_clipboard(&state);
        }
    ));

    let column = gtk::Box::new(gtk::Orientation::Vertical, 0);
    page.add(&group);
    column.append(&page);
    column.append(&apply);
    column.set_margin_bottom(12);
    column.set_margin_end(12);

    let bar = adw::ToolbarView::new();
    bar.add_top_bar(&adw::HeaderBar::new());
    bar.set_content(Some(&column));
    dialog.set_child(Some(&bar));
    dialog.present(Some(window));
}

fn parts_checklist(group: &adw::PreferencesGroup, parts: EditParts) -> impl Fn() -> EditParts {
    let rows: Vec<(gtk::Switch, fn(&mut EditParts, bool))> = [
        ("White balance", parts.white_balance, (|p: &mut EditParts, on| p.white_balance = on) as fn(&mut EditParts, bool)),
        ("Tone", parts.tone, |p: &mut EditParts, on| p.tone = on),
        ("Colour", parts.colour, |p: &mut EditParts, on| p.colour = on),
        ("Tone curve", parts.curve, |p: &mut EditParts, on| p.curve = on),
        ("Detail", parts.detail, |p: &mut EditParts, on| p.detail = on),
        ("Crop and rotation", parts.geometry, |p: &mut EditParts, on| p.geometry = on),
    ]
    .into_iter()
    .map(|(title, on, setter)| {
        let row = adw::ActionRow::new();
        row.set_title(title);
        let switch = gtk::Switch::new();
        switch.set_active(on);
        switch.set_valign(gtk::Align::Center);
        row.add_suffix(&switch);
        row.set_activatable_widget(Some(&switch));
        group.add(&row);
        (switch, setter)
    })
    .collect();

    move || {
        let mut chosen = EditParts::nothing();
        for (switch, setter) in &rows {
            setter(&mut chosen, switch.is_active());
        }
        chosen
    }
}

fn apply_clipboard(state: &App) {
    let parts = state.clipboard_parts.get();
    let source = state.clipboard.borrow().clone();
    let Some(source) = source else { return };
    apply_edit(state, &source, parts, "Pasted onto");
}

fn apply_edit(state: &App, source: &Document, parts: EditParts, done: &str) {
    if !parts.any() {
        state.toast("Nothing selected to apply");
        return;
    }

    if state.stack.visible_child_name().as_deref() == Some("editor") {
        {
            let mut open = state.open.borrow_mut();
            let Some(photo) = open.as_mut() else { return };
            photo.document.copy_from(source, parts);
        }

        reload_open_document(state);
        state.toast(&format!("{done} this photo"));
        return;
    }

    let selected: Vec<i64> = selected_cards(state)
        .iter()
        .filter_map(|child| child.widget_name().parse::<i64>().ok())
        .collect();

    if selected.is_empty() {
        state.toast("Select photos first");
        return;
    }

    let mut failed = 0;
    for id in &selected {
        let existing = state.catalog.load_edits(*id).ok().flatten();
        let mut document = match (existing, state.cards.borrow().get(id)) {
            (Some(document), _) => document,
            (None, Some((photo, _))) => Document::new(photo.path.to_string_lossy().to_string()),
            (None, None) => continue,
        };
        document.copy_from(source, parts);
        if state.catalog.save_edits(*id, &document).is_err() {
            failed += 1;
        }
    }

    reload_grid(state);
    state.toast(&match failed {
        0 => format!("{done} {} photo(s)", selected.len()),
        n => format!("{done} {}, {n} failed", selected.len() - n),
    });
}

fn reload_open_document(state: &App) {
    let loaded = state.open.borrow().as_ref().map(|photo| {
        (photo.document.basic(), photo.document.white_balance.unwrap_or(photo.as_shot),
         photo.document.crop().unwrap_or(([0.0, 0.0, 1.0, 1.0], 0.0)))
    });
    let Some((basic, balance, (rect, angle))) = loaded else { return };

    state.applying.set(true);
    state.sliders.write(basic);
    state.sliders.write_white_balance(balance);
    refresh_slider_marks(state);
    state.crop_rect.set(rect);
    state.straighten.set_value(angle as f64);
    state.applying.set(false);

    state.curve_area.queue_draw();
    state.crop_area.queue_draw();

    fill_segment_masks(state);
    write_mixer(state);
    write_point_colours(state);
    write_grading(state);
    write_perspective(state);
    write_space_note(state);
    ai_denoise::write(state);
    refresh_retouch(state);
    refresh_face(state);
    refresh_found(state);
    refresh_masks(state);
    select_mask(state, None);
    refresh_profile_picker(state);

    if let Some(photo) = state.open.borrow_mut().as_mut() {
        photo.view = None;
    }
    request_render(state);
    schedule_history_push(state);
}

fn build_filmstrip(state: &App) {
    while let Some(child) = state.filmstrip.first_child() {
        state.filmstrip.remove(&child);
    }
    state.strip_badges.borrow_mut().clear();
    state.strip_cards.borrow_mut().clear();

    const HEIGHT: f32 = 64.0;
    let cards = state.cards.borrow();
    for id in state.order.borrow().iter() {
        let Some((photo, _)) = cards.get(id) else { continue };

        let aspect = numa::io::thumbs::cached_size(&photo.path, photo.mtime, GRID_THUMB_EDGE)
            .map_or(1.5, |(width, height)| width as f32 / height.max(1) as f32)
            .clamp(0.5, 2.5);
        let picture = gtk::Picture::new();
        picture.set_content_fit(gtk::ContentFit::Cover);
        picture.set_size_request((HEIGHT * aspect).round() as i32, HEIGHT as i32);
        picture.set_overflow(gtk::Overflow::Hidden);

        let stacked = gtk::Overlay::new();
        stacked.set_child(Some(&picture));
        if photo.edited {
            let mark = gtk::Image::from_icon_name("document-edit-symbolic");
            mark.set_pixel_size(11);
            mark.add_css_class("edited-mark");
            mark.set_halign(gtk::Align::Start);
            mark.set_valign(gtk::Align::Start);
            stacked.add_overlay(&mark);
        }

        let badge = gtk::Label::new(Some(&strip_badge_text(photo.rating, photo.flag)));
        badge.add_css_class("strip-badge");
        badge.set_halign(gtk::Align::End);
        badge.set_valign(gtk::Align::Start);
        badge.set_visible(!badge.text().is_empty());
        if photo.flag == Flag::Rejected {
            badge.add_css_class("rejected");
        }
        stacked.add_overlay(&badge);
        state.strip_badges.borrow_mut().insert(*id, badge);

        let frame = gtk::Button::new();
        frame.set_child(Some(&stacked));
        frame.add_css_class("filmstrip-frame");
        frame.set_tooltip_text(photo.path.file_name().and_then(|name| name.to_str()));
        frame.set_widget_name(&id.to_string());
        frame.connect_clicked(glib::clone!(
            #[strong] state,
            #[strong] id,
            move |_| open_photo(&state, id)
        ));

        state.strip_cards.borrow_mut().push(LazyThumb {
            path: photo.path.clone(),
            mtime: photo.mtime,
            edge: GRID_THUMB_EDGE,
            asked: 0,
            picture,
            widget: frame.clone().upcast(),
            wanted: false,
        });
        state.filmstrip.append(&frame);
    }

    let state = state.clone();
    state.filmstrip.clone().add_tick_callback(move |_, _| {
        sweep_thumbnails(&state);
        glib::ControlFlow::Break
    });
}

fn mark_filmstrip(state: &App, current: i64) {
    let mut found = None;
    let mut child = state.filmstrip.first_child();
    while let Some(widget) = child {
        child = widget.next_sibling();
        if widget.widget_name() == current.to_string() {
            widget.add_css_class("current");
            found = Some(widget);
        } else {
            widget.remove_css_class("current");
        }
    }

    let Some(widget) = found else { return };
    let state = state.clone();
    let frames_waited = Cell::new(0);
    state.filmstrip.clone().add_tick_callback(move |_, _| {

        if !widget.has_css_class("current") {
            return glib::ControlFlow::Break;
        }

        let adjustment = state.filmstrip_scroller.hadjustment();
        let bounds = widget.compute_bounds(&state.filmstrip);
        let measured = bounds.is_some_and(|bounds| bounds.width() > 0.0) && adjustment.upper() > 0.0;

        frames_waited.set(frames_waited.get() + 1);
        if !measured {

            return if frames_waited.get() < 30 {
                glib::ControlFlow::Continue
            } else {
                glib::ControlFlow::Break
            };
        }

        let bounds = bounds.expect("measured");
        let centre = bounds.x() as f64 + bounds.width() as f64 / 2.0;
        let reach = adjustment.upper() - adjustment.page_size();

        adjustment.set_value((centre - adjustment.page_size() / 2.0).clamp(0.0, reach.max(0.0)));
        glib::ControlFlow::Break
    });
}

struct LazyThumb {
    path: PathBuf,
    mtime: i64,
    edge: u32,

    asked: u32,
    picture: gtk::Picture,

    widget: gtk::Widget,

    wanted: bool,
}

const THUMBNAIL_MARGIN: usize = 60;
const THUMBNAIL_KEEP: usize = 240;

fn grid_edge(state: &App) -> u32 {
    let wanted = state.wall.row_height() * 1.2 * 1.5 * state.wall.scale_factor().max(1) as f32;
    [GRID_THUMB_EDGE, 640, 960, 1280, 1920]
        .into_iter()
        .find(|&edge| edge as f32 >= wanted)
        .unwrap_or(1920)
}

fn schedule_thumbnails(state: &App) {
    let generation = state.thumbnail_generation.get().wrapping_add(1);
    state.thumbnail_generation.set(generation);

    let state = state.clone();
    glib::timeout_add_local_once(std::time::Duration::from_millis(60), move || {
        if state.thumbnail_generation.get() != generation {
            return;
        }
        sweep_thumbnails(&state);
    });
}

fn watch_thumbnails(state: &App) {
    if state.thumbnail_watch.replace(true) {
        return;
    }
    let state = state.clone();
    let shown: RefCell<Option<(adw::Toast, gtk::Label, gtk::ProgressBar)>> = RefCell::new(None);
    let started = std::time::Instant::now();
    glib::timeout_add_local(std::time::Duration::from_millis(100), move || {
        let Some((done, asked, decoding)) = thumbnail::progress() else {
            if let Some((toast, _, _)) = shown.take() {
                toast.dismiss();
            }
            state.thumbnail_watch.set(false);
            return glib::ControlFlow::Break;
        };
        let mut shown = shown.borrow_mut();
        if shown.is_none() && (!decoding || started.elapsed() < BUSY_AFTER) {
            return glib::ControlFlow::Continue;
        }
        let (_, text, bar) = shown.get_or_insert_with(|| {
            let (toast, text, bar) = progress_toast(&state, &Cancel(Rc::default()));
            toast.set_button_label(None);
            (toast, text, bar)
        });
        text.set_text(&format!("Making thumbnails — {done} of {asked}"));
        bar.set_fraction(done as f64 / asked.max(1) as f64);
        glib::ControlFlow::Continue
    });
}

fn sweep_thumbnails(state: &App) {
    sweep(&state.grid_cards, &state.wall, &state.grid_scroller.vadjustment(), false);
    sweep(
        &state.strip_cards,
        &state.filmstrip,
        &state.filmstrip_scroller.hadjustment(),
        true,
    );
    watch_thumbnails(state);
}

fn sweep(
    list: &Rc<RefCell<Vec<LazyThumb>>>,
    container: &impl IsA<gtk::Widget>,
    adjustment: &gtk::Adjustment,
    across: bool,
) {
    let count = list.borrow().len();
    if count == 0 {
        return;
    }

    let Some((first, last)) = visible_cards(list, container, adjustment, across, count) else {
        return;
    };

    let edge = list.borrow()[0].edge;
    let shrink = |cards: usize| {
        let fewer = cards as f32 * (GRID_THUMB_EDGE as f32 / edge as f32).powi(2).min(1.0);
        (fewer as usize).max(12)
    };
    let (margin, held) = (shrink(THUMBNAIL_MARGIN), shrink(THUMBNAIL_KEEP).max(shrink(THUMBNAIL_MARGIN) + 12));
    let load = first.saturating_sub(margin)..(last + margin + 1).min(count);
    let keep = first.saturating_sub(held)..(last + held + 1).min(count);

    for index in 0..count {
        let (wanted, in_load, in_keep, blurry) = {
            let cards = list.borrow();
            let card = &cards[index];
            (card.wanted, load.contains(&index), keep.contains(&index), card.asked != card.edge)
        };

        match (wanted, in_load, in_keep) {
            (false, true, _) | (true, true, _) if !wanted || blurry => {
                let (path, mtime, edge) = {
                    let mut cards = list.borrow_mut();
                    cards[index].wanted = true;
                    cards[index].asked = cards[index].edge;
                    let card = &cards[index];
                    (card.path.clone(), card.mtime, card.edge)
                };

                let list = list.clone();
                let asked = path.clone();
                thumbnail::load_thumbnail(&path, mtime, edge, move |texture| {
                    let cards = list.borrow();
                    let still =
                        cards.get(index).filter(|card| card.wanted && card.path == asked);
                    if let Some(card) = still {
                        card.picture.set_paintable(Some(&texture));
                    }

                });
            }
            (true, _, false) => {
                let mut cards = list.borrow_mut();
                cards[index].wanted = false;
                cards[index].picture.set_paintable(gtk::gdk::Paintable::NONE);
            }
            _ => {}
        }
    }
}

fn visible_cards(
    list: &Rc<RefCell<Vec<LazyThumb>>>,
    container: &impl IsA<gtk::Widget>,
    adjustment: &gtk::Adjustment,
    across: bool,
    count: usize,
) -> Option<(usize, usize)> {
    let (near, far) = (adjustment.value(), adjustment.value() + adjustment.page_size());
    if adjustment.page_size() <= 0.0 {
        return None;
    }

    let bounds = |index: usize| {
        let cards = list.borrow();
        let bounds = cards.get(index)?.widget.compute_bounds(container)?;
        Some(if across {
            (bounds.x() as f64, (bounds.x() + bounds.width()) as f64)
        } else {
            (bounds.y() as f64, (bounds.y() + bounds.height()) as f64)
        })
    };

    let (mut low, mut high) = (0usize, count - 1);
    while low < high {
        let middle = (low + high) / 2;
        match bounds(middle) {
            Some((_, card_far)) if card_far < near => low = middle + 1,
            Some(_) => high = middle,

            None => return None,
        }
    }
    let first = low;

    let mut last = first;
    while last + 1 < count {
        match bounds(last + 1) {
            Some((card_near, _)) if card_near <= far => last += 1,
            _ => break,
        }
    }

    Some((first, last))
}

fn cull_note(photo: &Photo) -> String {
    let frame = cull::Frame {
        sharpness: photo.sharpness.unwrap_or_default(),
        blown: photo.blown.unwrap_or_default(),
        ..Default::default()
    };

    let mut parts: Vec<String> = Vec::new();
    if let Some(suggested) = photo.suggested {
        parts.push(format!("~{suggested:.1}★"));
    }
    if photo.best_of_burst {
        parts.push("best of burst".to_string());
    }

    match (photo.face_sharpness, photo.sharpness) {
        (Some(face), Some(_)) if face < cull::SOFT => parts.push("soft face".to_string()),
        _ if photo.sharpness.is_some() && frame.is_soft() => parts.push("soft".to_string()),
        _ => {}
    }
    if photo.blown.is_some() && frame.is_blown() {
        parts.push("blown".to_string());
    }
    parts.join(" · ")
}

fn cull_detail(photo: &Photo) -> String {
    let (Some(sharpness), Some(blown)) = (photo.sharpness, photo.blown) else {
        return "Not measured yet".to_string();
    };

    let mut lines = vec![
        format!("Sharpness {sharpness:.2} (soft below {:.2})", cull::SOFT),
        format!("Blown {:.1} % (a lot above {:.0} %)", blown * 100.0, cull::BLOWN * 100.0),
    ];
    match (photo.faces, photo.face_sharpness) {
        (Some(0), _) => lines.push("No faces found".to_string()),
        (Some(count), Some(face)) => {
            lines.push(format!("{count} face(s), sharpest {face:.2} — this is what is scored"));
        }
        (Some(count), None) => lines.push(format!("{count} face(s), too small to judge")),
        (None, _) => lines.push("Faces not looked for".to_string()),
    }
    if let Some(suggested) = photo.suggested {
        lines.push(format!("Suggested {suggested:.1} of 5 — a suggestion, not a rating"));
    }
    lines.join("\n")
}

fn analyse_library(state: &App, button: &adw::SplitButton) {
    let Some(library) = state.library.borrow().clone() else {
        state.toast("No library selected");
        return;
    };

    let pending = match state.catalog.unanalysed(library.id, cull::VERSION) {
        Ok(pending) => pending,
        Err(err) => {
            state.toast(&format!("Could not read the catalog: {err}"));
            return;
        }
    };

    button.set_sensitive(false);
    let label = button.label().unwrap_or_default().to_string();

    const CHUNK: usize = 64;

    const FACE_EDGE: u32 = 640;

    let state = state.clone();
    let button = button.clone();
    glib::spawn_future_local(async move {
        let total = pending.len();
        let mut done = 0usize;
        let mut failed = 0usize;

        let cancel = Cancel::default();

        let measured_so_far = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let progress = (total > 0).then(|| progress_toast(&state, &cancel));
        let ticking = progress.as_ref().map(|(_, text, bar)| {
            let (text, bar) = (text.clone(), bar.clone());
            let counted = measured_so_far.clone();
            let show = move || {
                let n = counted.load(std::sync::atomic::Ordering::Relaxed).min(total);
                let fraction = n as f64 / total as f64;
                text.set_text(&format!(
                    "Analysing the library — {n} of {total} · {:.0} %",
                    fraction * 100.0
                ));
                bar.set_fraction(fraction);
            };
            show();
            glib::timeout_add_local(std::time::Duration::from_millis(250), move || {
                show();
                glib::ControlFlow::Continue
            })
        });

        for chunk in pending.chunks(CHUNK) {
            if cancel.stopped() {
                break;
            }
            let work: Vec<(i64, std::path::PathBuf)> =
                chunk.iter().map(|photo| (photo.id, photo.path.clone())).collect();

            let counted = measured_so_far.clone();
            let measured = gtk::gio::spawn_blocking(move || {
                use rayon::prelude::*;
                work.par_iter()
                    .inspect(|_| {
                        counted.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    })
                    .map(|(id, path)| {

                        let measured = raw::load_scaled(path, FACE_EDGE).map(|image| {
                            let frame = cull::measure::of(&image);

                            let found = cull::faces::detect(&image);
                            let faces = found.as_ref().map(|faces| faces.len() as u32);
                            let sharpest = found
                                .as_ref()
                                .and_then(|faces| cull::faces::sharpest_face(&image, faces));

                            let embeddings = found
                                .iter()
                                .flatten()
                                .filter_map(|face| {
                                    Some((
                                        cull::people::embed(&image, face)?,
                                        cull::people::portrait_jpeg(&image, face),
                                    ))
                                })
                                .collect::<Vec<_>>();
                            (frame, faces, sharpest, embeddings)
                        });
                        (*id, measured)
                    })
                    .collect::<Vec<_>>()
            })
            .await;

            let Ok(measured) = measured else { break };

            let mut batch = Vec::with_capacity(measured.len());
            for (id, measured) in measured {
                match measured {
                    Ok((frame, faces, face_sharpness, embeddings)) => {
                        batch.push((id, frame, faces, face_sharpness, embeddings));
                    }
                    Err(err) => {
                        log::warn!("{err}");
                        failed += 1;
                    }
                }
            }

            if let Err(err) = state.catalog.save_analysis_batch(cull::VERSION, &batch) {
                log::warn!("could not store analysis: {err}");
                failed += batch.len();
            }

            done += chunk.len();
            button.set_label(&format!("{done} / {total}"));
        }

        if let Some(ticking) = ticking {
            ticking.remove();
        }

        if cancel.stopped() {
            if let Some((toast, _, _)) = &progress {
                toast.dismiss();
            }
            button.set_label(&label);
            button.set_sensitive(true);
            state.toast(&format!(
                "Stopped after {done} of {total} — what was analysed is kept"
            ));
            reload_grid(&state);
            return;
        }

        if let Some((_, text, bar)) = &progress {
            text.set_text("Grouping bursts…");
            bar.set_fraction(1.0);
        }
        let grouped = regroup_bursts(&state, library.id);
        if let Some((toast, _, _)) = &progress {
            toast.dismiss();
        }

        button.set_label(&label);
        button.set_sensitive(true);
        reload_grid(&state);

        let faces = if cull::faces::is_installed() {
            String::new()
        } else {
            format!(" · {}", cull::faces::model_missing_message())
        };

        state.toast(&match (total, failed, grouped) {
            (0, _, Ok((bursts, learned))) => format!("Already analysed — {bursts} burst(s) · {learned}{faces}"),
            (_, 0, Ok((bursts, learned))) => {
                format!("Analysed {total} photo(s) — {bursts} burst(s) · {learned}{faces}")
            }
            (_, n, Ok((bursts, learned))) => format!(
                "Analysed {} of {total} — {bursts} burst(s), {n} unreadable · {learned}{faces}",
                total - n
            ),
            (_, _, Err(err)) => format!("Grouping failed: {err}"),
        });
    });
}

fn regroup_bursts(state: &App, library_id: i64) -> Result<(usize, cull::learn::Outcome), String> {
    let analysed = state.catalog.analysed(library_id)?;
    if analysed.is_empty() {
        return Ok((0, cull::learn::Outcome::TooFew { rated: 0 }));
    }

    let marks: std::collections::HashMap<i64, Photo> =
        state.catalog.photos(library_id, &Filter::default())?.into_iter().map(|photo| (photo.id, photo)).collect();

    let frames: Vec<cull::Frame> = analysed.iter().map(|(_, frame, _)| *frame).collect();
    let hashes: Vec<u64> = frames.iter().map(|frame| frame.hash).collect();
    let groups = cull::bursts(&hashes, cull::BURST_TOLERANCE);
    let best = cull::best_of_each(&frames, &groups);

    let best: std::collections::HashSet<usize> = best.into_iter().collect();
    let samples: Vec<cull::learn::Sample> = analysed
        .iter()
        .enumerate()
        .map(|(index, (id, frame, face))| {
            let photo = marks.get(id);
            cull::learn::Sample::new(
                frame,
                photo.and_then(|photo| photo.faces),
                *face,
                best.contains(&index),
                groups[index],
                photo.map_or(0, |photo| photo.rating),
                photo.is_some_and(|photo| photo.flag == Flag::Rejected),
            )
        })
        .collect();
    let (suggested, learned) = cull::learn::score(&samples);
    let rows: Vec<(i64, usize, bool, f32)> = analysed
        .iter()
        .enumerate()
        .map(|(index, (id, _, _))| (*id, groups[index], best.contains(&index), suggested[index]))
        .collect();

    state.catalog.save_bursts(&rows)?;
    Ok((best.len(), learned))
}

fn picker_headers(model: &crate::ui::sections::Sections) -> gtk::SignalListItemFactory {
    let headers = gtk::SignalListItemFactory::new();
    headers.connect_setup(|_, header| {

        let label = gtk::Label::builder()
            .xalign(0.0)
            .margin_start(18)
            .margin_end(18)
            .margin_bottom(3)
            .css_classes(["caption-heading", "dim-label"])
            .build();
        header.downcast_ref::<gtk::ListHeader>().unwrap().set_child(Some(&label));
    });
    let model = model.downgrade();
    headers.connect_bind(move |_, header| {
        let header = header.downcast_ref::<gtk::ListHeader>().unwrap();
        if let (Some(label), Some(model)) = (header.child().and_downcast::<gtk::Label>(), model.upgrade()) {
            label.set_label(&model.title(header.start()));
        }
    });
    headers
}

#[derive(Clone)]
enum Place {

    Everywhere,
    Library(Library),

    Album(String),

    Person(String),
}

fn refresh_picker(state: &App) {
    let names = state.catalog.names().unwrap_or_else(|err| {
        log::warn!("could not read who has a name: {err}");
        Vec::new()
    });
    let albums = state.catalog.albums().unwrap_or_else(|err| {
        log::warn!("could not read the albums: {err}");
        Vec::new()
    });
    let libraries = state.libraries.borrow().clone();

    let everywhere = libraries.len() > 1;
    {
        let mut filter = state.filter.borrow_mut();
        if filter.person.as_ref().is_some_and(|person| !names.iter().any(|name| name.eq_ignore_ascii_case(person))) {
            filter.person = None;
        }
        if filter.album.as_ref().is_some_and(|album| !albums.iter().any(|(key, _)| key == album)) {
            filter.album = None;
        }
        filter.all_libraries &= everywhere;
    }
    fill_albums_menu(state, &albums);

    let mut places = Vec::new();
    let mut library_labels = Vec::new();
    if everywhere {
        places.push(Place::Everywhere);
        library_labels.push("All libraries".to_string());
    }
    places.extend(libraries.iter().cloned().map(Place::Library));
    library_labels.extend(libraries.iter().map(Library::label));
    places.extend(albums.iter().map(|(key, _)| Place::Album(key.clone())));
    places.extend(names.iter().cloned().map(Place::Person));
    let sections = [
        ("Libraries", library_labels),
        ("Albums", albums.iter().map(|(_, name)| name.clone()).collect()),
        ("People", names),
    ];

    let index = {
        let filter = state.filter.borrow();
        let open = state.library.borrow().as_ref().map(|open| open.id);
        places.iter().position(|place| match place {
            Place::Everywhere => filter.all_libraries,
            Place::Album(key) => filter.album.as_ref() == Some(key),
            Place::Person(name) => filter.person.as_ref().is_some_and(|person| person.eq_ignore_ascii_case(name)),
            Place::Library(library) => !filter.spans_libraries() && open == Some(library.id),
        })
    };

    state.switching_library.set(true);

    let same = state
        .library_picker
        .model()
        .and_downcast::<crate::ui::sections::Sections>()
        .is_some_and(|model| model.is(&sections));
    if !same {
        let model = crate::ui::sections::Sections::new(&sections);
        state.library_picker.set_model(Some(&model));

        state.library_picker.set_header_factory((model.section_count() > 1).then(|| picker_headers(&model)).as_ref());
    }
    *state.picker_places.borrow_mut() = places;
    if let Some(index) = index {
        state.library_picker.set_selected(index as u32);
    }
    state.switching_library.set(false);
}

fn choose_place(state: &App, place: Place) {
    let unchanged = {
        let filter = state.filter.borrow();
        match &place {
            Place::Everywhere => filter.all_libraries,
            Place::Album(key) => filter.album.as_ref() == Some(key),
            Place::Person(name) => filter.person.as_ref() == Some(name),
            Place::Library(library) => {
                !filter.spans_libraries() && state.library.borrow().as_ref().is_some_and(|open| open.id == library.id)
            }
        }
    };
    if unchanged {
        return;
    }

    state.filter.borrow_mut().in_one_library();
    match place {
        Place::Everywhere => state.filter.borrow_mut().all_libraries = true,
        Place::Album(key) => state.filter.borrow_mut().album = Some(key),
        Place::Person(name) => state.filter.borrow_mut().person = Some(name),
        Place::Library(library) => {
            remember_library(state, Some(&library));
            *state.library.borrow_mut() = Some(library);
        }
    }
    reload_grid(state);
}

fn rescan_everywhere(state: &App) {
    let mut added = 0;
    for library in state.libraries.borrow().iter() {
        match state.catalog.sync_library(library) {
            Ok(count) => added += count,
            Err(err) => log::warn!("rescan of {}: {err}", library.path.display()),
        }
    }
    reload_grid(state);
    state.toast(&format!("Rescanned: {added} new photo(s)"));
}

fn selected_ids(state: &App) -> Vec<i64> {
    selected_cards(state).iter().filter_map(|child| child.widget_name().parse::<i64>().ok()).collect()
}

fn fill_albums_menu(state: &App, albums: &[(String, String)]) {
    let menu = &state.albums_menu;
    menu.remove_all();
    let add = gio::Menu::new();
    let existing = gio::Menu::new();
    for (key, name) in albums {
        let item = gio::MenuItem::new(Some(name), None);
        item.set_action_and_target_value(Some("win.photo-album-add"), Some(&key.to_variant()));
        existing.append_item(&item);
    }
    add.append_section(None, &existing);
    add.append(Some("New album…"), Some("win.photo-album-new"));
    menu.append_submenu(Some("Add to album"), &add);
    if state.filter.borrow().album.is_some() {
        menu.append(Some("Remove from album"), Some("win.photo-album-remove"));
    }
}

fn install_album_actions(state: &App, window: &adw::ApplicationWindow) {
    let add = gio::SimpleAction::new("photo-album-add", Some(&String::static_variant_type()));
    add.connect_activate(glib::clone!(
        #[strong] state,
        move |_, key| {
            let Some(key) = key.and_then(|key| key.get::<String>()) else { return };
            add_selection_to_album(&state, &key);
        }
    ));
    window.add_action(&add);

    let new = gio::SimpleAction::new("photo-album-new", None);
    new.connect_activate(glib::clone!(
        #[strong] state,
        #[weak] window,
        move |_, _| {
            if selected_ids(&state).is_empty() {
                state.toast("Select a photo first");
                return;
            }
            let entry = gtk::Entry::new();
            entry.set_placeholder_text(Some("Name"));
            entry.set_activates_default(true);
            let ask = adw::AlertDialog::new(Some("New Album"), None);
            ask.set_extra_child(Some(&entry));
            ask.add_response("cancel", "Cancel");
            ask.add_response("create", "Create");
            ask.set_response_appearance("create", adw::ResponseAppearance::Suggested);
            ask.set_default_response(Some("create"));
            ask.set_close_response("cancel");
            let state = state.clone();
            ask.connect_response(None, move |_, response| {
                if response != "create" {
                    return;
                }
                match state.catalog.create_album(&entry.text()) {
                    Ok(key) => {
                        refresh_picker(&state);
                        add_selection_to_album(&state, &key);
                    }
                    Err(err) => state.toast(&err),
                }
            });
            ask.present(Some(&window));
        }
    ));
    window.add_action(&new);

    let remove = gio::SimpleAction::new("photo-album-remove", None);
    remove.connect_activate(glib::clone!(
        #[strong] state,
        move |_, _| {
            let Some(key) = state.filter.borrow().album.clone() else { return };
            match state.catalog.set_in_album(&key, &selected_ids(&state), false) {
                Ok(()) => reload_grid(&state),
                Err(err) => state.toast(&format!("Could not remove from the album: {err}")),
            }
        }
    ));
    window.add_action(&remove);

    let manage = gio::SimpleAction::new("albums", None);
    manage.connect_activate(glib::clone!(
        #[strong] state,
        #[weak] window,
        move |_, _| albums_dialog(&state, &window)
    ));
    window.add_action(&manage);
}

fn add_selection_to_album(state: &App, key: &str) {
    let ids = selected_ids(state);
    if ids.is_empty() {
        state.toast("Select a photo first");
        return;
    }
    let name = state.catalog.albums().unwrap_or_default().into_iter().find(|(have, _)| have == key);
    match state.catalog.set_in_album(key, &ids, true) {
        Ok(()) => state.toast(&format!("Added {} to {}", ids.len(), name.map(|(_, name)| name).unwrap_or_default())),
        Err(err) => state.toast(&format!("Could not add to the album: {err}")),
    }
}

fn albums_dialog(state: &App, window: &adw::ApplicationWindow) {
    let dialog = adw::Dialog::new();
    dialog.set_title("Albums");
    dialog.set_content_width(460);
    dialog.set_content_height(420);

    let page = adw::PreferencesPage::new();
    let group = adw::PreferencesGroup::new();
    group.set_description(Some(
        "An album can hold photographs from any library. Deleting one leaves the \
         photographs where they are.",
    ));
    let albums = state.catalog.albums().unwrap_or_else(|err| {
        state.toast(&format!("Could not read the albums: {err}"));
        Vec::new()
    });
    if albums.is_empty() {
        let empty = adw::ActionRow::new();
        empty.set_title("No albums yet");
        empty.set_subtitle("Select photographs, right-click, and choose Add to album.");
        group.add(&empty);
    }

    for (key, name) in albums {
        let row = adw::EntryRow::new();
        row.set_text(&name);
        row.set_show_apply_button(true);
        row.connect_apply(glib::clone!(
            #[strong] state,
            #[strong] key,
            move |row| match state.catalog.rename_album(&key, &row.text()) {
                Ok(()) => refresh_picker(&state),
                Err(err) => state.toast(&err),
            }
        ));

        let delete = gtk::Button::from_icon_name("user-trash-symbolic");
        delete.set_tooltip_text(Some("Delete album"));
        delete.set_valign(gtk::Align::Center);
        delete.add_css_class("flat");
        delete.connect_clicked(glib::clone!(
            #[strong] state,
            #[weak] dialog,
            #[weak] window,
            move |_| {
                let confirm = adw::AlertDialog::new(
                    Some(&format!("Delete {name}?")),
                    Some("The photographs stay where they are, with their ratings and edits; only the album goes."),
                );
                confirm.add_response("cancel", "Cancel");
                confirm.add_response("delete", "Delete");
                confirm.set_response_appearance("delete", adw::ResponseAppearance::Destructive);
                confirm.set_default_response(Some("cancel"));
                confirm.set_close_response("cancel");
                let (state, key, dialog, parent) = (state.clone(), key.clone(), dialog.clone(), window.clone());
                confirm.connect_response(None, move |_, response| {
                    if response != "delete" {
                        return;
                    }
                    match state.catalog.delete_album(&key) {
                        Ok(()) => {

                            reload_grid(&state);
                            dialog.close();
                            albums_dialog(&state, &parent);
                        }
                        Err(err) => state.toast(&format!("Could not delete the album: {err}")),
                    }
                });
                confirm.present(Some(&window));
            }
        ));
        row.add_suffix(&delete);
        group.add(&row);
    }
    page.add(&group);

    let bar = adw::ToolbarView::new();
    bar.add_top_bar(&adw::HeaderBar::new());
    bar.set_content(Some(&page));
    dialog.set_child(Some(&bar));
    dialog.present(Some(window));
}

fn build_filter_bar(state: &App, window: &adw::ApplicationWindow) -> gtk::Box {
    let bar = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    bar.add_css_class("toolbar-row");

    let rating = gtk::DropDown::from_strings(&["Any rating", "1★+", "2★+", "3★+", "4★+", "5★"]);
    rating.connect_selected_notify(glib::clone!(
        #[strong] state,
        move |picker| {
            if state.applying.get() {
                return;
            }
            state.filter.borrow_mut().min_rating = picker.selected() as u8;
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
            state.filter.borrow_mut().flag = match picker.selected() {
                1 => Some(Flag::Picked),
                2 => Some(Flag::Rejected),
                3 => Some(Flag::None),
                _ => None,
            };
            reload_grid(&state);
        }
    ));

    let folder_picker = state.folder_picker.clone();
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
                .and_then(|index| state.folders.borrow().get(index).cloned());
            state.folder.replace(chosen);
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
            state.filter.borrow_mut().file_type = match picker.selected() {
                1 => FileType::Raw,
                2 => FileType::NotRaw,
                _ => FileType::Any,
            };
            reload_grid(&state);
        }
    ));

    let sort = gtk::DropDown::from_strings(&[
        "By date",
        "By name",
        "By rating",
        "By sharpness",
        "By suggestion",
    ]);
    sort.connect_selected_notify(glib::clone!(
        #[strong] state,
        move |picker| {
            if state.applying.get() {
                return;
            }
            state.filter.borrow_mut().sort = match picker.selected() {
                1 => Sort::Name,
                2 => Sort::Rating,
                3 => Sort::Sharpness,
                4 => Sort::Suggested,
                _ => Sort::Captured,
            };
            reload_grid(&state);
        }
    ));

    {
        let (min_rating, saved_flag, saved_sort, saved_type) = {
            let filter = state.filter.borrow();
            (filter.min_rating, filter.flag, filter.sort, filter.file_type)
        };
        state.applying.set(true);
        rating.set_selected(min_rating.min(5) as u32);
        flag.set_selected(match saved_flag {
            Some(Flag::Picked) => 1,
            Some(Flag::Rejected) => 2,
            Some(Flag::None) => 3,
            None => 0,
        });
        sort.set_selected(match saved_sort {
            Sort::Name => 1,
            Sort::Rating => 2,
            Sort::Sharpness => 3,
            Sort::Suggested => 4,
            Sort::Captured => 0,
        });
        file_type.set_selected(match saved_type {
            FileType::Any => 0,
            FileType::Raw => 1,
            FileType::NotRaw => 2,
        });
        state.applying.set(false);
    }

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
                    let mut filter = state.filter.borrow_mut();
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
    state.analyse_button.replace(Some(analyse.clone()));

    bar.append(&rating);
    bar.append(&flag);
    bar.append(&folder_picker);
    bar.append(&file_type);
    bar.append(&sort);

    let everyone = gtk::Button::with_label("People");
    everyone.set_tooltip_text(Some("The faces in this library, grouped, to put names to"));
    everyone.set_visible(cull::people::is_installed());
    everyone.connect_clicked(glib::clone!(
        #[strong] state,
        #[weak] window,
        move |_| people_dialog(&state, &window)
    ));
    bar.append(&everyone);

    let hint = gtk::Label::new(Some("0–5 rate · P pick · X reject"));
    hint.add_css_class("dim-label");
    hint.set_hexpand(true);
    hint.set_halign(gtk::Align::Center);
    bar.append(&hint);

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

    state.library_export.replace(Some(export.clone()));
    bar.append(&group);

    state.wall.connect_selection_changed(glib::clone!(
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
    state.wall.set_sizes(size.value() as f32, gap.value() as f32);

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
                state.wall.set_sizes(sizes.0, sizes.1);
                state.catalog.remember(GRID_SIZES, &sizes);

                let edge = grid_edge(&state);
                let mut changed = false;
                for card in state.grid_cards.borrow_mut().iter_mut().filter(|card| card.edge != edge) {
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
    bar.append(&sizes);

    bar
}

fn reload_grid(state: &App) {

    close_loupe(state);

    state.catalog.remember(GRID_FILTER, &*state.filter.borrow());

    thumbnail::cancel_pending();

    state.wall.remove_all();
    state.cards.borrow_mut().clear();
    state.grid_cards.borrow_mut().clear();

    let Some(library) = state.library.borrow().clone() else {
        state.empty.set_visible(false);
        state.welcome.set_visible(true);
        return;
    };
    state.welcome.set_visible(false);

    refresh_picker(state);

    let filter = state.filter.borrow().clone();
    let photos = match filter.spans_libraries() {
        true => state.catalog.photos_everywhere(&filter),
        false => state.catalog.photos(library.id, &filter),
    };
    let mut photos = match photos {
        Ok(photos) => photos,
        Err(err) => {
            state.toast(&format!("Could not read the catalog: {err}"));
            return;
        }
    };

    let across_libraries = filter.spans_libraries();
    refresh_folders(state, &library, if across_libraries { &[] } else { &photos });
    if let (Some(folder), false) = (state.folder.borrow().clone(), across_libraries) {
        let root = library.path.join(folder);
        photos.retain(|photo| photo.path.starts_with(&root));
    }

    state.empty.set_text("No photos match this filter.");
    state.empty.set_visible(photos.is_empty());

    *state.order.borrow_mut() = photos.iter().map(|photo| photo.id).collect();

    let edge = grid_edge(state);
    let aspects: Vec<f32> = {
        use rayon::prelude::*;
        photos
            .par_iter()
            .map(|photo| {
                numa::io::thumbs::cached_size(&photo.path, photo.mtime, edge)
                    .or_else(|| numa::io::thumbs::cached_size(&photo.path, photo.mtime, GRID_THUMB_EDGE))
                    .map_or(justified::UNKNOWN_ASPECT, |(width, height)| {
                        width as f32 / height as f32
                    })
            })
            .collect()
    };
    for (photo, aspect) in photos.iter().zip(aspects) {
        state.wall.append(&build_card(state, photo), aspect);
    }

    let state = state.clone();
    state.grid_scroller.clone().add_tick_callback(move |_, _| {
        sweep_thumbnails(&state);
        glib::ControlFlow::Break
    });
}

fn merge_selection(state: &App, button: &gtk::Button) {
    let paths: Vec<PathBuf> = {
        let cards = state.cards.borrow();
        selected_cards(state)
            .iter()
            .filter_map(|child| child.widget_name().parse::<i64>().ok())
            .filter_map(|id| cards.get(&id).map(|(photo, _)| photo.path.clone()))
            .collect()
    };

    if paths.len() < 2 {
        state.toast("Select at least two exposures");
        return;
    }

    button.set_sensitive(false);
    state.toast(&format!("Merging {} exposures…", paths.len()));

    let state = state.clone();
    let button = button.clone();
    let sources = paths.clone();
    glib::spawn_future_local(async move {
        let merged = busy(&state, "Merging the bracket…", move || {
            let mut frames = Vec::with_capacity(paths.len());
            for path in &paths {
                let (image, exposure) = raw::decode_with_exposure(path)?;
                let exposure = exposure.ok_or_else(|| {
                    format!("{} has no exposure metadata", path.display())
                })?;
                frames.push(bracket::Frame { image, exposure });
            }
            let full = bracket::merge(&frames)?;
            Ok::<_, String>(full.downscaled(PROXY_EDGE).unwrap_or(full))
        })
        .await;

        button.set_sensitive(true);

        let proxy = match merged {
            Ok(Ok(proxy)) => proxy,
            Ok(Err(err)) => {
                state.toast(&format!("Merge failed: {err}"));
                return;
            }
            Err(_) => {
                state.toast("Merge was cancelled");
                return;
            }
        };

        open_merged(&state, proxy, sources);
    });
}

fn open_merged(state: &App, proxy: LinearImage, paths: Vec<PathBuf>) {

    begin_open(state);
    state.canvas.set_paintable(gtk::gdk::Paintable::NONE);
    state.before.set_active(false);

    state.face_names.borrow_mut().clear();
    leave_crop(state);
    state.loading_full.set(false);
    state.crop_rect.set([0.0, 0.0, 1.0, 1.0]);

    state.applying.set(true);
    state.straighten.set_value(0.0);
    state.applying.set(false);

    let as_shot = proxy
        .profile
        .map(|profile| profile.as_shot_white_balance())
        .unwrap_or(WhiteBalance { temperature: 5500.0, tint: 0.0 });

    let mut document = Document::new("merged".to_string());

    document.set_basic(Basic { hdr: 50.0, ..Default::default() });
    let basic = document.basic();

    state.sliders.temperature.set_sensitive(proxy.profile.is_some());
    state.sliders.tint.set_sensitive(proxy.profile.is_some());

    let working_key = colour_key(&document);
    let working = render::to_working_space(&document, &proxy);
    let full_size = (proxy.width, proxy.height);

    *state.open.borrow_mut() = Some(OpenPhoto {
        source: Source::Bracket { paths },
        summary: None,
        segmentation: None,
        mask_frame: None,
        embedding: None,
        embedding_pending: false,
        faces_pending: false,
        people: Vec::new(),
        segmenting: false,
        animal: None,
        draft: None,
        full_size,
        proxy,
        working,
        working_key,
        full_working: None,
        full_working_key: None,
        view: None,
        baseline: None,
        history: History::new(EditState::of(&document)),
        document,
        as_shot,
        edits_unreadable: false,
    });

    state.zoom.set(FIT_ZOOM);
    state.stack.set_visible_child_name("editor");

    state.applying.set(true);
    state.sliders.write(basic);
    state.sliders.write_white_balance(as_shot);
    refresh_slider_marks(state);
    state.applying.set(false);
    state.curve_area.queue_draw();

    sync_document(state);

    refresh_profile_picker(state);
    refresh_crumbs(state);
    request_render(state);
}

const LOUPE_EDGE: u32 = 1920;

fn build_loupe(state: &App) -> gtk::Box {
    let loupe = state.loupe.clone();
    loupe.set_visible(false);
    loupe.add_css_class("loupe");
    loupe.set_margin_top(0);

    state.loupe_picture.set_vexpand(true);
    state.loupe_picture.set_hexpand(true);
    state.loupe_picture.set_can_shrink(true);

    state.loupe_picture.set_content_fit(gtk::ContentFit::Contain);
    loupe.append(&state.loupe_picture);

    state.loupe_caption.add_css_class("loupe-caption");
    state.loupe_caption.set_margin_bottom(8);
    state.loupe_caption.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
    loupe.append(&state.loupe_caption);

    let click = gtk::GestureClick::new();
    click.connect_released(glib::clone!(
        #[strong] state,
        move |_, _, _, _| close_loupe(&state)
    ));
    loupe.add_controller(click);

    loupe
}

fn open_loupe(state: &App) {
    let selected = selected_cards(state);
    let Some(first) = selected.first() else {
        state.toast("Select a photo first");
        return;
    };
    let at = state.grid_cards.borrow().iter().position(|card| card.widget == *first);
    if let Some(at) = at {
        show_loupe(state, at);
    }
}

fn show_loupe(state: &App, at: usize) {
    let Some((path, mtime)) = state
        .grid_cards
        .borrow()
        .get(at)
        .map(|card| (card.path.clone(), card.mtime))
    else {
        return;
    };

    state.loupe_at.set(Some(at));
    state.loupe.set_visible(true);

    state.loupe_picture.set_paintable(gtk::gdk::Paintable::NONE);
    refresh_loupe_caption(state);

    let loupe = state.loupe_picture.clone();
    let at_open = state.loupe_at.clone();
    thumbnail::load_thumbnail(&path, mtime, LOUPE_EDGE, move |texture| {

        if at_open.get() == Some(at) {
            loupe.set_paintable(Some(&texture));
        }
    });
}

fn loupe_key(state: &App, key: gtk::gdk::Key) -> glib::Propagation {
    if state.loupe_at.get().is_none() {
        return glib::Propagation::Proceed;
    }
    match key {
        gtk::gdk::Key::space | gtk::gdk::Key::Escape => close_loupe(state),
        gtk::gdk::Key::Left | gtk::gdk::Key::Page_Up => step_loupe(state, false),
        gtk::gdk::Key::Right | gtk::gdk::Key::Page_Down => step_loupe(state, true),
        gtk::gdk::Key::Return | gtk::gdk::Key::KP_Enter => {
            let card = selected_cards(state).first().cloned();
            close_loupe(state);
            if let Some(card) = card {
                open_in_editor(state, &card);
            }
        }
        _ => return glib::Propagation::Proceed,
    }
    glib::Propagation::Stop
}

fn close_loupe(state: &App) {
    state.loupe_at.set(None);
    state.loupe.set_visible(false);
    state.loupe_picture.set_paintable(gtk::gdk::Paintable::NONE);
}

fn step_loupe(state: &App, forward: bool) {
    let Some(at) = state.loupe_at.get() else { return };
    let count = state.grid_cards.borrow().len();
    let next = match forward {
        true => (at + 1).min(count.saturating_sub(1)),
        false => at.saturating_sub(1),
    };
    if next == at {
        return;
    }

    let card = state.grid_cards.borrow().get(next).map(|card| card.widget.clone());
    if let Some(card) = card {
        state.wall.select_only(&card);
        state.wall.reveal(&card);
    }
    show_loupe(state, next);
}

fn refresh_loupe_caption(state: &App) {
    let Some(at) = state.loupe_at.get() else { return };
    let id = state.grid_cards.borrow().get(at).and_then(|card| card.widget.widget_name().parse::<i64>().ok());
    let Some(id) = id else { return };
    let cards = state.cards.borrow();
    let Some((photo, badge)) = cards.get(&id) else { return };
    let name = photo.path.file_name().map(|name| name.to_string_lossy().into_owned()).unwrap_or_default();
    state.loupe_caption.set_text(&format!("{name}   {}", badge.text()));
}

fn build_card(state: &App, photo: &Photo) -> gtk::Widget {
    let card = gtk::Box::new(gtk::Orientation::Vertical, 5);

    let picture = gtk::Picture::new();

    picture.set_can_shrink(true);
    picture.set_vexpand(true);
    picture.set_content_fit(gtk::ContentFit::Cover);

    picture.connect_paintable_notify(glib::clone!(
        #[weak(rename_to = wall)] state.wall,
        #[weak] card,
        move |picture| {
            let Some(paintable) = picture.paintable() else { return };
            let aspect = paintable.intrinsic_aspect_ratio();
            if aspect > 0.0 {
                wall.set_aspect(&card, aspect as f32);
            }
        }
    ));
    picture.add_css_class("thumbnail");

    picture.set_overflow(gtk::Overflow::Hidden);

    let card_thumb = LazyThumb {
        path: photo.path.clone(),
        mtime: photo.mtime,
        edge: grid_edge(state),
        asked: 0,
        picture: picture.clone(),

        widget: picture.clone().upcast(),
        wanted: false,
    };

    let name = gtk::Label::new(photo.path.file_name().and_then(|name| name.to_str()));
    name.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
    name.set_max_width_chars(20);
    name.add_css_class("photo-name");

    let titled = gtk::Box::new(gtk::Orientation::Horizontal, 4);
    titled.set_halign(gtk::Align::Center);
    if photo.edited {
        let mark = gtk::Image::from_icon_name("document-edit-symbolic");
        mark.set_pixel_size(11);
        mark.add_css_class("edited-mark");
        mark.set_tooltip_text(Some("Has adjustments"));
        titled.append(&mark);
    }
    titled.append(&name);

    let badge = gtk::Label::new(Some(&badge_text(photo.rating, photo.flag)));
    badge.add_css_class("photo-badge");
    style_badge(&badge, photo.rating, photo.flag);

    let note = cull_note(photo);
    if !note.is_empty() {
        let hint = gtk::Label::new(Some(&note));
        hint.add_css_class("cull-note");
        hint.set_tooltip_text(Some(&cull_detail(photo)));
        hint.set_ellipsize(gtk::pango::EllipsizeMode::End);
        card.append(&picture);
        card.append(&hint);
    } else {
        card.append(&picture);
    }
    card.append(&titled);
    card.append(&badge);

    badge.set_ellipsize(gtk::pango::EllipsizeMode::End);
    let child: gtk::Widget = card.clone().upcast();
    child.set_tooltip_text(photo.path.to_str());

    child.set_widget_name(&photo.id.to_string());

    state.grid_cards.borrow_mut().push(LazyThumb { widget: child.clone(), ..card_thumb });

    state.cards.borrow_mut().insert(photo.id, (photo.clone(), badge));
    child
}

fn style_badge(badge: &gtk::Label, rating: u8, flag: Flag) {
    badge.remove_css_class("rated");
    badge.remove_css_class("rejected");

    if flag == Flag::Rejected {
        badge.add_css_class("rejected");
    } else if rating > 0 || flag == Flag::Picked {
        badge.add_css_class("rated");
    }
}

fn badge_text(rating: u8, flag: Flag) -> String {
    let stars: String = (1..=5).map(|n| if n <= rating { '★' } else { '☆' }).collect();
    match flag {
        Flag::Picked => format!("{stars}  ⚑"),
        Flag::Rejected => format!("{stars}  ✕"),
        Flag::None => stars,
    }
}

fn strip_badge_text(rating: u8, flag: Flag) -> String {
    let stars = match rating {
        0 => String::new(),
        n => format!("{n}\u{2605}"),
    };
    match flag {
        Flag::Picked => format!("{stars} \u{2691}").trim().to_string(),
        Flag::Rejected => format!("{stars} \u{2715}").trim().to_string(),
        Flag::None => stars,
    }
}

fn rate_open_photo(state: &App, action: Action) {
    let id = state.open.borrow().as_ref().and_then(|photo| match &photo.source {
        Source::Photo { id, .. } => Some(*id),

        Source::Bracket { .. } => None,
    });
    let Some(id) = id else {
        state.toast("A merged image has no place in the catalog yet");
        return;
    };

    let known = state.cards.borrow().get(&id).map(|(photo, _)| (photo.rating, photo.flag));
    let (rating, flag) = match (action, known) {
        (Action::Rate(rating), Some((_, flag))) => (rating, flag),
        (Action::Flag(flag), Some((rating, _))) => (rating, flag),
        (Action::Rate(rating), None) => (rating, Flag::None),
        (Action::Flag(flag), None) => (0, flag),
    };

    let saved = match action {
        Action::Rate(rating) => state.catalog.set_rating(id, rating),
        Action::Flag(flag) => state.catalog.set_flag(id, flag),
    };
    if let Err(err) = saved {
        state.toast(&format!("Could not save: {err}"));
        return;
    }

    if let Some((photo, badge)) = state.cards.borrow_mut().get_mut(&id) {
        photo.rating = rating;
        photo.flag = flag;
        badge.set_text(&badge_text(rating, flag));
        style_badge(badge, rating, flag);
    }
    if let Some(badge) = state.strip_badges.borrow().get(&id) {
        badge.set_text(&strip_badge_text(rating, flag));
        badge.set_visible(!badge.text().is_empty());
    }
    write_rating_button(state, rating, flag);

    let filter = state.filter.borrow();
    if filter.min_rating > 0 || filter.flag.is_some() {
        state.grid_stale.set(true);
    }
}

fn rate_here(state: &App, action: Action) {
    if state.stack.visible_child_name().as_deref() == Some("editor") {
        rate_open_photo(state, action);
    } else {
        apply_to_selection(state, action);
    }
}

fn write_rating_button(state: &App, rating: u8, flag: Flag) {

    let label = match (rating, flag) {
        (0, Flag::None) => "\u{2606}".to_string(),
        (0, Flag::Picked) => "\u{2606} \u{2691}".to_string(),
        (0, Flag::Rejected) => "\u{2606} \u{2715}".to_string(),
        (n, Flag::Picked) => format!("\u{2605} {n} \u{2691}"),
        (n, Flag::Rejected) => format!("\u{2605} {n} \u{2715}"),
        (n, Flag::None) => format!("\u{2605} {n}"),
    };
    state.rating_button.set_label(&label);

    state.rating_button.remove_css_class("rated");
    state.rating_button.remove_css_class("rejected");
    if flag == Flag::Rejected {
        state.rating_button.add_css_class("rejected");
    } else if rating > 0 || flag == Flag::Picked {
        state.rating_button.add_css_class("rated");
    }
}

fn name_icon_buttons(root: &gtk::Widget) {
    let mut pending = vec![root.clone()];
    while let Some(widget) = pending.pop() {
        let unnamed = widget.is::<gtk::Button>() || widget.is::<gtk::MenuButton>() || widget.is::<gtk::ToggleButton>();
        if unnamed {
            let has_label = widget
                .downcast_ref::<gtk::Button>()
                .and_then(|button| button.label())
                .is_some_and(|label| !label.is_empty());
            if let (false, Some(tooltip)) = (has_label, widget.tooltip_text()) {
                widget.update_property(&[gtk::accessible::Property::Label(&tooltip)]);
            }
        }
        let mut child = widget.first_child();
        while let Some(current) = child {
            child = current.next_sibling();
            pending.push(current);
        }
    }
}

fn install_rating_shortcuts(state: &App, window: &adw::ApplicationWindow) {
    let keys = gtk::EventControllerKey::new();
    keys.connect_key_pressed(glib::clone!(
        #[strong] state,
        #[strong] window,
        move |_, key, _, modifiers| {
            if state.stack.visible_child_name().as_deref() == Some("editor") {

                match key {
                    gtk::gdk::Key::Left | gtk::gdk::Key::Page_Up => {
                        step_photo(&state, false);
                        return glib::Propagation::Stop;
                    }
                    gtk::gdk::Key::Right | gtk::gdk::Key::Page_Down => {
                        step_photo(&state, true);
                        return glib::Propagation::Stop;
                    }
                    _ => {}
                }

                if !modifiers.intersects(gtk::gdk::ModifierType::CONTROL_MASK | gtk::gdk::ModifierType::ALT_MASK) {
                    let action = match key.to_unicode() {
                        Some(digit @ '0'..='5') => {
                            Some(Action::Rate(digit as u8 - b'0'))
                        }
                        Some('p' | 'P') => Some(Action::Flag(Flag::Picked)),
                        Some('x' | 'X') => Some(Action::Flag(Flag::Rejected)),
                        Some('u' | 'U') => Some(Action::Flag(Flag::None)),
                        _ => None,
                    };
                    if let Some(action) = action {
                        rate_open_photo(&state, action);
                        return glib::Propagation::Stop;
                    }

                    if matches!(key.to_unicode(), Some('g' | 'G')) {
                        cycle_guides(&state);
                        return glib::Propagation::Stop;
                    }

                    if matches!(key.to_unicode(), Some('i' | 'I')) {
                        let info = &state.info_button;
                        if info.is_active() {
                            info.popdown();
                        } else {
                            info.popup();
                        }
                        return glib::Propagation::Stop;
                    }
                }

                if modifiers.contains(gtk::gdk::ModifierType::ALT_MASK) {
                    if let Some(digit @ '1'..='7') = key.to_unicode() {
                        let index = digit as usize - '1' as usize;
                        if let Some((name, _, _)) = PANEL_TABS.get(index) {
                            show_panel_tab(&state, name);
                            return glib::Propagation::Stop;
                        }
                    }
                }

                if key == gtk::gdk::Key::space {
                    state.before.set_active(true);
                    return glib::Propagation::Stop;
                }
                if !modifiers.contains(gtk::gdk::ModifierType::CONTROL_MASK) {
                    return glib::Propagation::Proceed;
                }
                return match key.to_unicode() {

                    Some('c' | 'C') if modifiers.contains(gtk::gdk::ModifierType::SHIFT_MASK) => {
                        copy_image(&state);
                        glib::Propagation::Stop
                    }
                    Some('c' | 'C') => {
                        copy_settings(&state);
                        glib::Propagation::Stop
                    }
                    Some('v' | 'V') => {
                        paste_settings(&state, &window, false);
                        glib::Propagation::Stop
                    }
                    Some('z') => {
                        step_history(&state, false);
                        glib::Propagation::Stop
                    }

                    Some('Z') => {
                        step_history(&state, true);
                        glib::Propagation::Stop
                    }
                    _ => glib::Propagation::Proceed,
                };
            }

            if state.stack.visible_child_name().as_deref() != Some("library") {
                return glib::Propagation::Proceed;
            }

            if state.loupe_at.get().is_none() && key == gtk::gdk::Key::space {
                open_loupe(&state);
                return glib::Propagation::Stop;
            }

            if modifiers.contains(gtk::gdk::ModifierType::CONTROL_MASK) {
                return match key.to_unicode() {
                    Some('v' | 'V') => {
                        paste_settings(&state, &window, false);
                        glib::Propagation::Stop
                    }
                    _ => glib::Propagation::Proceed,
                };
            }

            let action = match key.to_unicode() {
                Some(digit @ '0'..='5') => Action::Rate(digit as u8 - b'0'),
                Some('p' | 'P') => Action::Flag(Flag::Picked),
                Some('x' | 'X') => Action::Flag(Flag::Rejected),
                Some('u' | 'U') => Action::Flag(Flag::None),
                _ => return glib::Propagation::Proceed,
            };

            apply_to_selection(&state, action);
            refresh_loupe_caption(&state);
            glib::Propagation::Stop
        }
    ));
    keys.connect_key_released(glib::clone!(
        #[strong] state,
        move |_, key, _, _| {
            if key == gtk::gdk::Key::space {
                state.before.set_active(false);
            }
        }
    ));
    window.add_controller(keys);
}

#[derive(Clone, Copy)]
enum Action {
    Rate(u8),
    Flag(Flag),
}

fn apply_to_selection(state: &App, action: Action) {
    let selected = selected_cards(state);
    if selected.is_empty() {
        state.toast("Select a photo first");
        return;
    }

    let cards = state.cards.borrow();

    for child in &selected {
        let Ok(id) = child.widget_name().parse::<i64>() else { continue };

        let result = match action {
            Action::Rate(rating) => state.catalog.set_rating(id, rating),
            Action::Flag(flag) => state.catalog.set_flag(id, flag),
        };

        if let Err(err) = result {
            drop(cards);
            state.toast(&format!("Could not save: {err}"));
            return;
        }

        if let Some((_, badge)) = cards.get(&id) {
            let (rating, flag) = match action {
                Action::Rate(rating) => (rating, flag_from_badge(&badge.text())),
                Action::Flag(flag) => (rating_from_badge(&badge.text()), flag),
            };
            badge.set_text(&badge_text(rating, flag));
            style_badge(badge, rating, flag);
        }
    }

    drop(cards);

    let filter = state.filter.borrow();
    let narrowing = filter.min_rating > 0 || filter.flag.is_some();
    drop(filter);
    if narrowing {
        reload_grid(state);
    }
}

fn rating_from_badge(text: &str) -> u8 {
    text.chars().filter(|c| *c == '★').count() as u8
}

fn flag_from_badge(text: &str) -> Flag {
    if text.contains('⚑') {
        Flag::Picked
    } else if text.contains('✕') {
        Flag::Rejected
    } else {
        Flag::None
    }
}

struct SeenFace {

    at: [f32; 4],
    embedding: [f32; cull::people::LENGTH],
    portrait: image::RgbImage,
}

struct OpenPhoto {
    source: Source,

    edits_unreadable: bool,
    proxy: LinearImage,

    working: LinearImage,

    draft: Option<LinearImage>,

    summary: Option<raw::Summary>,

    segmentation: Option<Arc<Segmentation>>,

    mask_frame: Option<Arc<image::RgbImage>>,

    embedding: Option<Arc<sam::Embedding>>,
    embedding_pending: bool,

    faces_pending: bool,

    people: Vec<SeenFace>,

    segmenting: bool,

    animal: Option<(std::sync::Weak<Segmentation>, numa::render::classify::Guess)>,

    full_size: (u32, u32),

    full_working: Option<LinearImage>,
    full_working_key: Option<ColourKey>,

    view: Option<ViewTile>,

    baseline: Option<gtk::gdk::Paintable>,

    working_key: ColourKey,
    document: Document,
    history: History,

    as_shot: WhiteBalance,
}

#[derive(Debug, Clone, PartialEq)]
struct EditState {
    basic: Basic,
    white_balance: Option<WhiteBalance>,

    crop: Option<([f32; 4], f32)>,
    rotation: f32,

    mirrored: bool,
    film_simulation: Option<String>,

    colour_profile: Option<String>,

    curves: [Curve; 4],
    mixer: Mixer,

    point_colours: PointColours,

    grading: Grading,

    retouch: Retouch,

    perspective: Perspective,

    working_space: ColourSpace,

    beautify: Beautify,
    masks: Vec<Mask>,

    ai_denoise: f32,
}

impl EditState {

    fn restore(&self, document: &mut Document) {
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
    }

    fn untouched() -> Self {
        Self::of(&Document::new(String::new()))
    }

    fn of(document: &Document) -> Self {
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
        }
    }
}

type Geometry = (f32, Option<([f32; 4], f32)>, [f32; 2]);

fn geometry_of_document(document: &Document) -> Geometry {

    let basic = document.basic();
    (document.rotation(), document.crop(), [basic.lens_distortion, basic.lens_vignetting])
}

struct ViewTile {

    rect: [f32; 4],

    key: ColourKey,

    geometry: Geometry,

    edge: u32,
    image: LinearImage,
}

struct History {
    states: Vec<EditState>,

    position: usize,

    names: Vec<Option<String>>,
}

const HISTORY_KEPT: usize = 100;

impl History {
    fn new(initial: EditState) -> Self {
        Self { states: vec![initial], position: 0, names: vec![None] }
    }

    fn resumed(saved: Option<(Vec<EditState>, usize)>, now: EditState) -> Self {
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

    fn push(&mut self, state: EditState) {
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

    fn push_named(&mut self, state: EditState, name: &str) {
        if self.states[self.position] != state {
            self.push(state);
            self.names[self.position] = Some(name.to_string());
        }
    }

    fn undo(&mut self) -> Option<EditState> {
        if self.position == 0 {
            return None;
        }
        self.position -= 1;
        Some(self.states[self.position].clone())
    }

    fn redo(&mut self) -> Option<EditState> {
        if self.position + 1 >= self.states.len() {
            return None;
        }
        self.position += 1;
        Some(self.states[self.position].clone())
    }

    fn go_to(&mut self, position: usize) -> Option<EditState> {
        if position >= self.states.len() || position == self.position {
            return None;
        }
        self.position = position;
        Some(self.states[self.position].clone())
    }

    fn steps(&self) -> Vec<String> {

        let first = if self.states[0] == EditState::untouched() { "Original" } else { "Opened" };
        let mut steps = vec![first.to_string()];
        for (pair, name) in self.states.windows(2).zip(&self.names[1..]) {
            steps.push(name.clone().unwrap_or_else(|| pair[1].difference_from(&pair[0])));
        }
        steps
    }
}

impl EditState {

    fn difference_from(&self, previous: &EditState) -> String {
        let sliders: [(&str, fn(&Basic) -> f32); 37] = [
            ("Exposure", |b| b.exposure),
            ("Contrast", |b| b.contrast),
            ("Highlights", |b| b.highlights),
            ("Shadows", |b| b.shadows),
            ("Whites", |b| b.whites),
            ("Blacks", |b| b.blacks),
            ("Vibrance", |b| b.vibrance),
            ("Saturation", |b| b.saturation),
            ("HDR", |b| b.hdr),
            ("Clarity", |b| b.clarity),
            ("Texture", |b| b.texture),
            ("Sharpening", |b| b.sharpen),
            ("Sharpening radius", |b| b.sharpen_radius),
            ("Sharpening mask", |b| b.sharpen_masking),
            ("Noise reduction", |b| b.denoise_luma),
            ("Noise detail", |b| b.denoise_detail),
            ("Noise contrast", |b| b.denoise_contrast),
            ("Colour noise", |b| b.denoise_colour),
            ("Defringe", |b| b.defringe),
            ("Moiré", |b| b.moire),
            ("Dehaze", |b| b.dehaze),
            ("Vignette", |b| b.vignette),
            ("Vignette midpoint", |b| b.vignette_midpoint),
            ("Vignette roundness", |b| b.vignette_roundness),
            ("Vignette feather", |b| b.vignette_feather),
            ("Grain", |b| b.grain),
            ("Grain size", |b| b.grain_size),
            ("Grain roughness", |b| b.grain_roughness),
            ("Shadows tint", |b| b.shadow_tint),
            ("Red hue", |b| b.red_hue),
            ("Red saturation", |b| b.red_saturation),
            ("Green hue", |b| b.green_hue),
            ("Green saturation", |b| b.green_saturation),
            ("Blue hue", |b| b.blue_hue),
            ("Blue saturation", |b| b.blue_saturation),
            ("Lens distortion", |b| b.lens_distortion),
            ("Lens vignetting", |b| b.lens_vignetting),
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

fn at_rest_of(now: Basic, balance: WhiteBalance, as_shot: WhiteBalance) -> [bool; SLIDER_COUNT] {
    let rest = Basic::default();
    let same = |a: f32, b: f32| (a - b).abs() < 1e-4;
    [

        same(balance.temperature, as_shot.temperature),
        same(balance.tint, as_shot.tint),
        same(now.exposure, rest.exposure),
        same(now.contrast, rest.contrast),
        same(now.highlights, rest.highlights),
        same(now.shadows, rest.shadows),
        same(now.whites, rest.whites),
        same(now.blacks, rest.blacks),
        same(now.hdr, rest.hdr),
        same(now.vibrance, rest.vibrance),
        same(now.saturation, rest.saturation),
        same(now.clarity, rest.clarity),
        same(now.texture, rest.texture),

        same(now.sharpen, rest.sharpen),
        same(now.sharpen_radius, rest.sharpen_radius),
        same(now.sharpen_masking, rest.sharpen_masking),
        same(now.denoise_luma, rest.denoise_luma),
        same(now.denoise_detail, rest.denoise_detail),
        same(now.denoise_contrast, rest.denoise_contrast),
        same(now.denoise_colour, rest.denoise_colour),
        same(now.defringe, rest.defringe),
        same(now.moire, rest.moire),
        same(now.dehaze, rest.dehaze),
        same(now.vignette, rest.vignette),
        same(now.vignette_midpoint, rest.vignette_midpoint),
        same(now.vignette_roundness, rest.vignette_roundness),
        same(now.vignette_feather, rest.vignette_feather),
        same(now.grain, rest.grain),
        same(now.grain_size, rest.grain_size),
        same(now.grain_roughness, rest.grain_roughness),
        same(now.shadow_tint, rest.shadow_tint),
        same(now.red_hue, rest.red_hue),
        same(now.red_saturation, rest.red_saturation),
        same(now.green_hue, rest.green_hue),
        same(now.green_saturation, rest.green_saturation),
        same(now.blue_hue, rest.blue_hue),
        same(now.blue_saturation, rest.blue_saturation),
        same(now.lens_distortion, rest.lens_distortion),
        same(now.lens_vignetting, rest.lens_vignetting),
    ]
}

fn refresh_slider_marks(state: &App) {
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

    let panel = state.sliders.each();
    for ((_, scale, _), rest) in panel.iter().zip(state.sliders.at_rest(as_shot)) {
        mark(scale, rest);
    }

    REGISTERED.with(|registered| {
        for scale in registered.borrow().iter() {
            if panel.iter().any(|(_, theirs, _)| *theirs == scale) {
                continue;
            }
            mark(scale, (scale.value() - neutral_of(scale).unwrap_or(0.0)).abs() < 1e-4);
        }
    });
}

fn build_history(state: &App) -> gtk::Popover {
    let column = gtk::Box::new(gtk::Orientation::Vertical, 8);
    column.set_margin_top(6);
    column.set_margin_bottom(6);
    column.set_margin_start(6);
    column.set_margin_end(6);

    let heading = gtk::Label::new(Some("HISTORY"));
    heading.set_xalign(0.0);
    heading.add_css_class("section-header");
    column.append(&heading);

    state.history_list.set_selection_mode(gtk::SelectionMode::None);
    state.history_list.add_css_class("boxed-list");

    let scroller = gtk::ScrolledWindow::new();
    scroller.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    scroller.set_propagate_natural_height(true);
    scroller.set_max_content_height(420);
    scroller.set_width_request(260);
    scroller.set_child(Some(&state.history_list));
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

fn refresh_snapshots(state: &App, list: &gtk::ListBox) {
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
        row.set_title(&glib::markup_escape_text(&name));
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

fn restore_snapshot(state: &App, name: &str, document: &Document) {
    let stepped = state.open.borrow_mut().as_mut().map(|photo| {
        photo.history.push(EditState::of(&photo.document));
        photo.history.push_named(EditState::of(document), name);
        (photo.history.states[photo.history.position].clone(), photo.as_shot)
    });
    if let Some((edit, as_shot)) = stepped {
        apply_history(state, edit, as_shot);
    }
}

fn refresh_history(state: &App) {
    while let Some(row) = state.history_list.first_child() {
        state.history_list.remove(&row);
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
        state.history_list.append(&row);
    }
}

fn masks_changed(now: &[Mask], before: &[Mask]) -> Option<String> {
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

enum Source {

    Photo { id: i64, path: PathBuf },

    Bracket { paths: Vec<PathBuf> },
}

impl Source {

    fn full_resolution(&self) -> Result<LinearImage, String> {
        match self {

            Source::Photo { path, .. } if raw::is_raw(path) => raw::decode_linear_best(path),
            Source::Photo { path, .. } => raw::decode_linear_any(path),
            Source::Bracket { paths } => {
                let mut frames = Vec::with_capacity(paths.len());
                for path in paths {
                    let (image, exposure) = raw::decode_with_exposure(path)?;
                    let exposure = exposure
                        .ok_or_else(|| format!("{} has no exposure metadata", path.display()))?;
                    frames.push(bracket::Frame { image, exposure });
                }
                bracket::merge(&frames)
            }
        }
    }

    fn name(&self) -> PathBuf {
        match self {
            Source::Photo { path, .. } => path.clone(),
            Source::Bracket { paths } => {
                let stem = paths
                    .first()
                    .and_then(|path| path.file_stem())
                    .map_or_else(|| "merged".to_string(), |stem| stem.to_string_lossy().to_string());
                PathBuf::from(format!("{stem} HDR"))
            }
        }
    }
}

#[derive(Clone)]
struct Sliders {
    temperature: gtk::Scale,
    tint: gtk::Scale,
    exposure: gtk::Scale,
    contrast: gtk::Scale,
    highlights: gtk::Scale,
    shadows: gtk::Scale,
    whites: gtk::Scale,
    blacks: gtk::Scale,
    vibrance: gtk::Scale,
    saturation: gtk::Scale,
    hdr: gtk::Scale,
    clarity: gtk::Scale,
    texture: gtk::Scale,
    sharpen: gtk::Scale,
    sharpen_radius: gtk::Scale,
    sharpen_masking: gtk::Scale,
    denoise_luma: gtk::Scale,

    denoise_detail: gtk::Scale,
    denoise_contrast: gtk::Scale,
    denoise_colour: gtk::Scale,

    defringe: gtk::Scale,

    moire: gtk::Scale,

    dehaze: gtk::Scale,
    vignette: gtk::Scale,
    vignette_midpoint: gtk::Scale,
    vignette_roundness: gtk::Scale,
    vignette_feather: gtk::Scale,
    grain: gtk::Scale,
    grain_size: gtk::Scale,
    grain_roughness: gtk::Scale,
    shadow_tint: gtk::Scale,
    red_hue: gtk::Scale,
    red_saturation: gtk::Scale,
    green_hue: gtk::Scale,
    green_saturation: gtk::Scale,
    blue_hue: gtk::Scale,
    blue_saturation: gtk::Scale,

    lens_distortion: gtk::Scale,
    lens_vignetting: gtk::Scale,
}

impl Sliders {
    fn new() -> Self {
        let amount = || gtk::Scale::with_range(gtk::Orientation::Horizontal, -100.0, 100.0, 1.0);
        let positive = || gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 100.0, 1.0);
        Self {

            temperature: gtk::Scale::with_range(gtk::Orientation::Horizontal, 2000.0, 15000.0, 10.0),
            tint: gtk::Scale::with_range(
                gtk::Orientation::Horizontal,
                -color::MAX_TINT as f64,
                color::MAX_TINT as f64,
                1.0,
            ),

            exposure: gtk::Scale::with_range(gtk::Orientation::Horizontal, -5.0, 5.0, 0.05),
            contrast: amount(),
            highlights: amount(),
            shadows: amount(),
            whites: amount(),
            blacks: amount(),
            vibrance: amount(),
            saturation: amount(),
            hdr: amount(),
            clarity: amount(),
            texture: amount(),

            sharpen: positive(),
            sharpen_radius: gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.5, 3.0, 0.1),
            sharpen_masking: positive(),
            denoise_luma: positive(),
            denoise_detail: positive(),
            denoise_contrast: positive(),
            denoise_colour: positive(),
            defringe: positive(),
            moire: positive(),
            dehaze: amount(),
            vignette: amount(),
            vignette_midpoint: positive(),
            vignette_roundness: amount(),
            vignette_feather: positive(),
            grain: positive(),
            grain_size: positive(),
            grain_roughness: positive(),
            shadow_tint: amount(),
            red_hue: amount(),
            red_saturation: amount(),
            green_hue: amount(),
            green_saturation: amount(),
            blue_hue: amount(),
            blue_saturation: amount(),
            lens_distortion: amount(),
            lens_vignetting: amount(),
        }
    }

    fn white_balance(&self) -> WhiteBalance {
        WhiteBalance {
            temperature: self.temperature.value() as f32,
            tint: self.tint.value() as f32,
        }
    }

    fn write_white_balance(&self, balance: WhiteBalance) {
        self.temperature.set_value(balance.temperature as f64);
        self.tint.set_value(balance.tint as f64);
    }

    fn read(&self) -> Basic {
        Basic {
            exposure: self.exposure.value() as f32,
            contrast: self.contrast.value() as f32,
            highlights: self.highlights.value() as f32,
            shadows: self.shadows.value() as f32,
            whites: self.whites.value() as f32,
            blacks: self.blacks.value() as f32,
            vibrance: self.vibrance.value() as f32,
            saturation: self.saturation.value() as f32,
            hdr: self.hdr.value() as f32,
            clarity: self.clarity.value() as f32,
            texture: self.texture.value() as f32,
            sharpen: self.sharpen.value() as f32,
            sharpen_radius: self.sharpen_radius.value() as f32,
            sharpen_masking: self.sharpen_masking.value() as f32,
            denoise_luma: self.denoise_luma.value() as f32,
            denoise_detail: self.denoise_detail.value() as f32,
            denoise_contrast: self.denoise_contrast.value() as f32,
            denoise_colour: self.denoise_colour.value() as f32,
            defringe: self.defringe.value() as f32,
            moire: self.moire.value() as f32,
            dehaze: self.dehaze.value() as f32,
            vignette: self.vignette.value() as f32,
            vignette_midpoint: self.vignette_midpoint.value() as f32,
            vignette_roundness: self.vignette_roundness.value() as f32,
            vignette_feather: self.vignette_feather.value() as f32,
            grain: self.grain.value() as f32,
            grain_size: self.grain_size.value() as f32,
            grain_roughness: self.grain_roughness.value() as f32,
            shadow_tint: self.shadow_tint.value() as f32,
            red_hue: self.red_hue.value() as f32,
            red_saturation: self.red_saturation.value() as f32,
            green_hue: self.green_hue.value() as f32,
            green_saturation: self.green_saturation.value() as f32,
            blue_hue: self.blue_hue.value() as f32,
            blue_saturation: self.blue_saturation.value() as f32,
            lens_distortion: self.lens_distortion.value() as f32,
            lens_vignetting: self.lens_vignetting.value() as f32,
        }
    }

    fn at_rest(&self, as_shot: WhiteBalance) -> [bool; SLIDER_COUNT] {
        at_rest_of(self.read(), self.white_balance(), as_shot)
    }

    fn write(&self, basic: Basic) {
        const _: () = assert!(
            SLIDER_COUNT == 39,
            "a slider was added to `each`; add it to `write` and `at_rest_of` too"
        );

        self.exposure.set_value(basic.exposure as f64);
        self.contrast.set_value(basic.contrast as f64);
        self.highlights.set_value(basic.highlights as f64);
        self.shadows.set_value(basic.shadows as f64);
        self.whites.set_value(basic.whites as f64);
        self.blacks.set_value(basic.blacks as f64);
        self.vibrance.set_value(basic.vibrance as f64);
        self.saturation.set_value(basic.saturation as f64);
        self.hdr.set_value(basic.hdr as f64);
        self.clarity.set_value(basic.clarity as f64);

        self.texture.set_value(basic.texture as f64);
        self.sharpen.set_value(basic.sharpen as f64);
        self.sharpen_radius.set_value(basic.sharpen_radius as f64);
        self.sharpen_masking.set_value(basic.sharpen_masking as f64);
        self.denoise_luma.set_value(basic.denoise_luma as f64);
        self.denoise_detail.set_value(basic.denoise_detail as f64);
        self.denoise_contrast.set_value(basic.denoise_contrast as f64);
        self.denoise_colour.set_value(basic.denoise_colour as f64);
        self.defringe.set_value(basic.defringe as f64);
        self.moire.set_value(basic.moire as f64);
        self.dehaze.set_value(basic.dehaze as f64);
        self.vignette.set_value(basic.vignette as f64);
        self.vignette_midpoint.set_value(basic.vignette_midpoint as f64);
        self.vignette_roundness.set_value(basic.vignette_roundness as f64);
        self.vignette_feather.set_value(basic.vignette_feather as f64);
        self.grain.set_value(basic.grain as f64);
        self.grain_size.set_value(basic.grain_size as f64);
        self.grain_roughness.set_value(basic.grain_roughness as f64);
        self.shadow_tint.set_value(basic.shadow_tint as f64);
        self.red_hue.set_value(basic.red_hue as f64);
        self.red_saturation.set_value(basic.red_saturation as f64);
        self.green_hue.set_value(basic.green_hue as f64);
        self.green_saturation.set_value(basic.green_saturation as f64);
        self.blue_hue.set_value(basic.blue_hue as f64);
        self.blue_saturation.set_value(basic.blue_saturation as f64);
        self.lens_distortion.set_value(basic.lens_distortion as f64);
        self.lens_vignetting.set_value(basic.lens_vignetting as f64);
    }

    fn each(&self) -> [(&'static str, &gtk::Scale, Readout); SLIDER_COUNT] {
        [
            ("Temperature", &self.temperature, Readout::Kelvin),
            ("Tint", &self.tint, Readout::Signed(0)),
            ("Exposure", &self.exposure, Readout::Signed(2)),
            ("Contrast", &self.contrast, Readout::Signed(0)),
            ("Highlights", &self.highlights, Readout::Signed(0)),
            ("Shadows", &self.shadows, Readout::Signed(0)),
            ("Whites", &self.whites, Readout::Signed(0)),
            ("Blacks", &self.blacks, Readout::Signed(0)),
            ("HDR", &self.hdr, Readout::Signed(0)),
            ("Vibrance", &self.vibrance, Readout::Signed(0)),
            ("Saturation", &self.saturation, Readout::Signed(0)),
            ("Clarity", &self.clarity, Readout::Signed(0)),
            ("Texture", &self.texture, Readout::Signed(0)),
            ("Sharpening", &self.sharpen, Readout::Positive(0)),
            ("Radius", &self.sharpen_radius, Readout::Radius),
            ("Masking", &self.sharpen_masking, Readout::Positive(0)),
            ("Noise reduction", &self.denoise_luma, Readout::Positive(0)),
            ("Detail", &self.denoise_detail, Readout::Middle),
            ("Contrast", &self.denoise_contrast, Readout::Positive(0)),
            ("Colour noise", &self.denoise_colour, Readout::Positive(0)),
            ("Defringe", &self.defringe, Readout::Positive(0)),
            ("Moiré", &self.moire, Readout::Positive(0)),
            ("Dehaze", &self.dehaze, Readout::Signed(0)),
            ("Amount", &self.vignette, Readout::Signed(0)),
            ("Midpoint", &self.vignette_midpoint, Readout::Middle),
            ("Roundness", &self.vignette_roundness, Readout::Signed(0)),
            ("Feather", &self.vignette_feather, Readout::Middle),
            ("Amount", &self.grain, Readout::Positive(0)),
            ("Size", &self.grain_size, Readout::Positive(0)),
            ("Roughness", &self.grain_roughness, Readout::Middle),
            ("Shadows tint", &self.shadow_tint, Readout::Signed(0)),
            ("Red hue", &self.red_hue, Readout::Signed(0)),
            ("Red saturation", &self.red_saturation, Readout::Signed(0)),
            ("Green hue", &self.green_hue, Readout::Signed(0)),
            ("Green saturation", &self.green_saturation, Readout::Signed(0)),
            ("Blue hue", &self.blue_hue, Readout::Signed(0)),
            ("Blue saturation", &self.blue_saturation, Readout::Signed(0)),
            ("Distortion", &self.lens_distortion, Readout::Signed(0)),
            ("Vignetting", &self.lens_vignetting, Readout::Signed(0)),
        ]
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Readout {

    Signed(usize),

    Kelvin,

    Positive(usize),

    Radius,

    Degrees,

    Middle,
}

impl Readout {
    fn format(self, value: f64) -> String {
        match self {
            Readout::Kelvin => format!("{value:.0} K"),
            Readout::Signed(_) if value == 0.0 => "0".to_string(),
            Readout::Signed(decimals) => {
                let sign = if value > 0.0 { '+' } else { '\u{2212}' };
                format!("{sign} {:.*}", decimals, value.abs())
            }
            Readout::Positive(decimals) => format!("{:.*}", decimals, value),
            Readout::Radius => format!("{value:.1} px"),
            Readout::Middle => format!("{value:.0}"),
            Readout::Degrees => format!("{value:.0}\u{00b0}"),
        }
    }
}

fn build_editor_page(state: &App) -> gtk::Box {
    let page = gtk::Box::new(gtk::Orientation::Vertical, 0);

    page.append(&build_editor_bar(state));
    page.append(&key_hint(
        state,
        "hint-editor-keys",
        "Hold Space to compare with the original · double-click a slider to reset it · I shows the camera's details",
    ));

    let body = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    body.set_hexpand(true);
    body.set_vexpand(true);

    let scroller = state.canvas_scroller.clone();
    scroller.set_hexpand(true);
    scroller.set_vexpand(true);
    scroller.add_css_class("canvas-area");

    let pipette_click = gtk::GestureClick::new();
    pipette_click.connect_released(glib::clone!(
        #[strong] state,
        move |_, _, x, y| {
            let Some((u, v)) = canvas_point(&state, x, y) else { return };
            if state.picking_white.get() {
                pick_white(&state, u, v);
            } else if state.picking_band.get() {
                pick_band(&state, u, v);
            } else if state.picking_point.get() {
                pick_point(&state, u, v);
            }
        }
    ));
    state.canvas.add_controller(pipette_click);

    let overlay = gtk::Overlay::new();
    overlay.set_child(Some(&state.canvas));
    overlay.add_overlay(&build_guides_overlay(state));
    overlay.add_overlay(&build_face_names_overlay(state));
    overlay.add_overlay(&build_crop_overlay(state));
    overlay.add_overlay(&build_mask_overlay(state));
    overlay.add_overlay(&build_retouch_overlay(state));
    scroller.set_child(Some(&overlay));

    for adjustment in [scroller.hadjustment(), scroller.vadjustment()] {
        adjustment.connect_value_changed(glib::clone!(
            #[strong] state,
            move |_| {
                let Some(tile) = state.tile.get() else { return };
                let Some(visible) = visible_rect(&state) else { return };

                let inside = visible[0] >= tile[0]
                    && visible[1] >= tile[1]
                    && visible[0] + visible[2] <= tile[0] + tile[2]
                    && visible[1] + visible[3] <= tile[1] + tile[3];
                if !inside {
                    request_render(&state);
                }
            }
        ));
    }

    let scroll = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::VERTICAL);
    scroll.connect_scroll(glib::clone!(
        #[strong] state,
        move |_, _, dy| {

            let factor = ZOOM_PER_NOTCH.powf(-dy);
            zoom_about_centre(&state, scaled_zoom(&state, factor));
            glib::Propagation::Stop
        }
    ));
    scroller.add_controller(scroll);

    let drag = gtk::GestureDrag::new();
    let anchor = Rc::new(Cell::new((0.0f64, 0.0f64)));
    drag.connect_drag_begin(glib::clone!(
        #[strong] state,
        #[strong] anchor,
        move |_, _, _| {
            anchor.set((
                state.canvas_scroller.hadjustment().value(),
                state.canvas_scroller.vadjustment().value(),
            ));
        }
    ));
    drag.connect_drag_update(glib::clone!(
        #[strong] state,
        #[strong] anchor,
        move |_, dx, dy| {
            let (x, y) = anchor.get();

            state.canvas_scroller.hadjustment().set_value(x - dx);
            state.canvas_scroller.vadjustment().set_value(y - dy);
        }
    ));
    scroller.add_controller(drag);

    for adjustment in [scroller.hadjustment(), scroller.vadjustment()] {
        adjustment.connect_page_size_notify(glib::clone!(
            #[strong] state,
            move |_| {
                if state.zoom.get() == FIT_ZOOM {
                    apply_zoom(&state);
                }
            }
        ));
    }

    let beside = gtk::Box::new(gtk::Orientation::Horizontal, 0);

    beside.set_homogeneous(true);
    beside.append(&build_reference_pane(state));
    beside.append(&scroller);

    let panel = build_adjustment_panel(state);
    let split = gtk::Paned::new(gtk::Orientation::Horizontal);
    split.set_start_child(Some(&beside));
    split.set_end_child(Some(&panel));
    split.set_resize_start_child(true);
    split.set_resize_end_child(false);

    split.set_shrink_end_child(true);
    split.set_position(-1);

    body.append(&split);
    split.set_hexpand(true);

    let panel_split = split.clone();
    split.add_tick_callback(move |split, _| {
        let width = split.width();
        if width > PANEL_WIDTH * 2 {
            panel_split.set_position(width - panel_floor(&panel));
            return glib::ControlFlow::Break;
        }
        glib::ControlFlow::Continue
    });

    page.append(&body);

    let strip = state.filmstrip_scroller.clone();
    strip.set_policy(gtk::PolicyType::Automatic, gtk::PolicyType::Never);
    strip.set_child(Some(&state.filmstrip));
    strip.add_css_class("filmstrip");
    state.filmstrip.set_margin_top(6);
    state.filmstrip.set_margin_bottom(6);
    state.filmstrip.set_margin_start(8);
    state.filmstrip.set_margin_end(8);

    let wheel = gtk::EventControllerScroll::new(
        gtk::EventControllerScrollFlags::BOTH_AXES | gtk::EventControllerScrollFlags::DISCRETE,
    );
    wheel.connect_scroll(glib::clone!(
        #[strong] state,
        move |_, dx, dy| {
            let adjustment = state.filmstrip_scroller.hadjustment();

            let step = if dx.abs() > dy.abs() { dx } else { dy };
            let reach = adjustment.upper() - adjustment.page_size();
            adjustment.set_value((adjustment.value() + step * FILMSTRIP_STEP).clamp(0.0, reach.max(0.0)));
            glib::Propagation::Stop
        }
    ));
    strip.add_controller(wheel);

    let adjustment = strip.hadjustment();
    adjustment.connect_value_changed(glib::clone!(
        #[strong] state,
        move |_| schedule_thumbnails(&state)
    ));
    adjustment.connect_changed(glib::clone!(
        #[strong] state,
        move |_| schedule_thumbnails(&state)
    ));

    page.append(&gtk::Separator::new(gtk::Orientation::Horizontal));
    page.append(&strip);
    page
}

fn build_crumbs(state: &App) -> gtk::Box {
    let crumbs = gtk::Box::new(gtk::Orientation::Horizontal, 2);
    crumbs.add_css_class("breadcrumbs");
    crumbs.set_halign(gtk::Align::Center);

    for (crumb, tooltip) in [
        (&state.library_crumb, "Back to the library"),
        (&state.photo_crumb, "The whole photograph"),
    ] {
        crumb.add_css_class("flat");
        crumb.set_tooltip_text(Some(tooltip));

        crumb.set_can_shrink(true);
    }

    state.library_crumb.connect_clicked(glib::clone!(
        #[strong] state,
        move |_| close_editor(&state)
    ));

    state.photo_crumb.connect_clicked(glib::clone!(
        #[strong] state,
        move |_| select_mask(&state, None)
    ));

    crumbs.append(&state.library_crumb);
    crumbs.append(&crumb_arrow());
    crumbs.append(&state.photo_crumb);

    crumbs
}

fn show_coverage(state: &App) {
    state.mask_area.queue_draw();
}

fn build_mask_banner(state: &App) -> gtk::Box {
    let banner = state.mask_banner.clone();
    banner.add_css_class("mask-scope");
    banner.set_margin_start(14);
    banner.set_margin_end(14);
    banner.set_margin_top(10);

    let back = gtk::Button::from_icon_name("go-previous-symbolic");
    back.add_css_class("flat");
    back.set_tooltip_text(Some("Back to all masks, and edit the photograph"));
    back.connect_clicked(glib::clone!(
        #[strong] state,
        move |_| {
            select_mask(&state, None);
            show_panel_tab(&state, "masks");
        }
    ));
    banner.append(&back);

    state.mask_button.add_css_class("flat");
    state.mask_button.set_hexpand(true);
    state.mask_button.set_tooltip_text(Some("The masks on this photograph"));
    banner.append(&state.mask_button);

    let eye = state.mask_banner_eye.clone();
    eye.add_css_class("flat");
    eye.connect_toggled(glib::clone!(
        #[strong] state,
        move |button| {
            if state.applying.get() {
                return;
            }
            if let Some(index) = state.selected_mask.get() {
                set_mask_visible(&state, index, button.is_active());
            }
        }
    ));
    banner.append(&eye);

    banner.set_visible(false);
    banner
}

fn crumb_arrow() -> gtk::Label {
    let arrow = gtk::Label::new(Some("\u{203a}"));
    arrow.add_css_class("crumb-arrow");
    arrow
}

fn refresh_crumbs(state: &App) {
    state.library_crumb.set_label(
        &state
            .library
            .borrow()
            .as_ref()
            .map(Library::label)
            .unwrap_or_else(|| "Library".to_string()),
    );

    let name = match state.open.borrow().as_ref().map(|photo| &photo.source) {
        Some(Source::Photo { path, .. }) => {
            path.file_name().map(|name| name.to_string_lossy().into_owned())
        }
        Some(Source::Bracket { paths }) => Some(format!("Merge of {} frames", paths.len())),
        None => None,
    };
    state.photo_crumb.set_label(name.as_deref().unwrap_or("\u{2014}"));
}

fn build_editor_bar(state: &App) -> gtk::Box {
    let bar = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    bar.add_css_class("toolbar-row");

    let copy = gtk::Button::from_icon_name("edit-copy-symbolic");
    copy.set_tooltip_text(Some("Copy this photo's settings (Ctrl+C)"));
    copy.connect_clicked(glib::clone!(
        #[strong] state,
        move |_| copy_settings(&state)
    ));
    bar.append(&copy);

    let facts = page_column();
    facts.append(&section_header("This photograph"));
    facts.append(&build_info(state));
    let scroller = gtk::ScrolledWindow::new();
    scroller.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    scroller.set_propagate_natural_height(true);
    scroller.set_max_content_height(600);

    facts.set_size_request(380, -1);
    scroller.set_child(Some(&facts));
    let popover = gtk::Popover::new();
    popover.set_child(Some(&scroller));

    let guides = state.guides_button.clone();
    guides.set_icon_name("view-grid-symbolic");
    guides.set_tooltip_text(Some("Guides: none (G)"));
    guides.connect_clicked(glib::clone!(
        #[strong] state,
        move |_| cycle_guides(&state)
    ));
    bar.append(&guides);

    let info = state.info_button.clone();
    info.set_icon_name("dialog-information-symbolic");
    info.set_tooltip_text(Some("What the camera recorded (I)"));
    info.set_popover(Some(&popover));
    bar.append(&info);

    let ratings = gio::Menu::new();
    let stars = gio::Menu::new();
    for value in (0..=5i32).rev() {
        let label = match value {
            0 => "No rating".to_string(),
            n => format!("{} {}", "\u{2605}".repeat(n as usize), n),
        };
        let item = gio::MenuItem::new(Some(&label), None);
        item.set_action_and_target_value(Some("win.photo-rate"), Some(&value.to_variant()));
        stars.append_item(&item);
    }
    ratings.append_section(None, &stars);

    let flags = gio::Menu::new();
    for (label, which) in [("Pick", "pick"), ("Reject", "reject"), ("Clear flag", "none")] {
        let item = gio::MenuItem::new(Some(label), None);
        item.set_action_and_target_value(Some("win.photo-flag"), Some(&which.to_variant()));
        flags.append_item(&item);
    }
    ratings.append_section(None, &flags);

    state.rating_button.set_menu_model(Some(&ratings));
    state.rating_button.set_tooltip_text(Some("Rating — or press 0 to 5, P, X"));
    state.rating_button.add_css_class("rating-button");
    bar.append(&state.rating_button);

    let zoom_group = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    zoom_group.add_css_class("linked");
    zoom_group.set_margin_start(8);

    let out = gtk::Button::from_icon_name("zoom-out-symbolic");
    out.set_tooltip_text(Some("Zoom out"));
    out.connect_clicked(glib::clone!(
        #[strong] state,
        move |_| zoom_about_centre(&state, scaled_zoom(&state, 1.0 / ZOOM_PER_NOTCH))
    ));
    zoom_group.append(&out);

    let levels = gio::Menu::new();
    let fit = gio::Menu::new();
    fit.append(Some("Fit to window"), Some("win.zoom::fit"));
    levels.append_section(None, &fit);
    let steps = gio::Menu::new();
    for percent in [50u32, 100, 200, 400] {
        steps.append(Some(&format!("{percent} %")), Some(&format!("win.zoom::{percent}")));
    }
    levels.append_section(None, &steps);

    let readout = gtk::MenuButton::new();
    readout.set_child(Some(&state.zoom_label));
    readout.set_menu_model(Some(&levels));
    readout.set_tooltip_text(Some("Zoom — and where to go"));
    readout.add_css_class("zoom-readout");
    state.zoom_label.set_width_chars(8);
    state.zoom_label.set_xalign(0.5);
    zoom_group.append(&readout);

    let into = gtk::Button::from_icon_name("zoom-in-symbolic");
    into.set_tooltip_text(Some("Zoom in"));
    into.connect_clicked(glib::clone!(
        #[strong] state,
        move |_| zoom_about_centre(&state, scaled_zoom(&state, ZOOM_PER_NOTCH))
    ));
    zoom_group.append(&into);
    bar.append(&zoom_group);

    let spacer = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    spacer.set_hexpand(true);
    bar.append(&spacer);

    let history_group = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    history_group.add_css_class("linked");
    history_group.set_margin_end(8);
    for (icon, tooltip, redo) in [
        ("edit-undo-symbolic", "Undo (Ctrl+Z)", false),
        ("edit-redo-symbolic", "Redo (Ctrl+Shift+Z)", true),
    ] {
        let button = gtk::Button::from_icon_name(icon);
        button.set_tooltip_text(Some(tooltip));
        button.connect_clicked(glib::clone!(
            #[strong] state,
            move |_| step_history(&state, redo)
        ));
        history_group.append(&button);
    }

    let steps = gtk::MenuButton::new();
    steps.set_icon_name("document-open-recent-symbolic");
    steps.set_tooltip_text(Some("Every step you have taken"));
    steps.set_popover(Some(&build_history(state)));
    history_group.append(&steps);

    bar.append(&history_group);

    state.before.set_label("Before");
    state.before.set_tooltip_text(Some("Show the frame as shot (hold Space)"));
    state.before.set_margin_end(8);
    state.before.connect_toggled(glib::clone!(
        #[strong] state,
        move |button| {
            if button.is_active() {
                show_baseline(&state);
            } else {
                render_current(&state);
            }
        }
    ));
    bar.append(&state.before);

    let reference = state.reference_button.clone();
    reference.set_label("Reference");
    reference.set_margin_end(8);
    reference.set_tooltip_text(Some(
        "Keep this frame beside the next ones, to match them to it",
    ));
    reference.connect_toggled(glib::clone!(
        #[strong] state,
        move |button| match button.is_active() {
            true => set_reference(&state),
            false => clear_reference(&state),
        }
    ));
    bar.append(&reference);

    let (group, export) = export_buttons(state, |state| export_now(state));
    state.export_button.replace(Some(export));
    refresh_export_button(state);
    bar.append(&group);

    bar
}

struct ExportJob {
    source: Source,
    document: Document,
}

fn open_job(state: &App) -> Option<ExportJob> {
    state.open.borrow().as_ref().map(|photo| ExportJob {
        source: match &photo.source {
            Source::Photo { id, path } => Source::Photo { id: *id, path: path.clone() },
            Source::Bracket { paths } => Source::Bracket { paths: paths.clone() },
        },
        document: photo.document.clone(),
    })
}

fn export_current(state: &App, parent: &impl IsA<gtk::Widget>) {
    let Some(job) = open_job(state) else { return };
    export_dialog(state, parent, vec![job]);
}

fn export_now(state: &App) {
    let Some(job) = open_job(state) else { return };
    let Some(directory) = export_directory(state) else { return };
    let settings = state.export_settings.borrow().clone();
    run_export(state, vec![job], settings, directory);
}

fn export_directory(state: &App) -> Option<PathBuf> {
    if let Some(folder) = state.export_settings.borrow().folder.clone() {
        return Some(folder);
    }
    match state.library.borrow().clone() {
        Some(library) => Some(library.export_dir()),
        None => {
            state.toast("No library selected — pick one to export into");
            None
        }
    }
}

fn export_buttons(state: &App, act: fn(&App)) -> (gtk::Box, gtk::Button) {
    let group = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    group.add_css_class("linked");

    let export = gtk::Button::with_label("Export");
    export.add_css_class("suggested-action");
    export.connect_clicked(glib::clone!(
        #[strong] state,
        move |_| act(&state)
    ));
    group.append(&export);

    let more = gtk::Button::from_icon_name("pan-end-symbolic");
    more.add_css_class("suggested-action");
    more.set_tooltip_text(Some("Export settings…"));
    more.connect_clicked(glib::clone!(
        #[strong] state,
        move |button| {

            if state.stack.visible_child_name().as_deref() == Some("editor") {
                export_current(&state, button);
            } else {
                export_selection(&state, button);
            }
        }
    ));
    group.append(&more);

    (group, export)
}

fn refresh_export_button(state: &App) {
    let summary = state.export_settings.borrow().summary();
    for button in [&state.export_button, &state.library_export] {
        if let Some(button) = button.borrow().clone() {
            button.set_tooltip_text(Some(&format!(
                "Export — {summary}. The arrow asks for something else"
            )));
        }
    }
}

fn export_selected_now(state: &App) {
    let jobs = selected_jobs(state);
    if jobs.is_empty() {
        state.toast("Select photos to export");
        return;
    }
    let Some(directory) = export_directory(state) else { return };
    let settings = state.export_settings.borrow().clone();
    run_export(state, jobs, settings, directory);
}

fn export_selection(state: &App, parent: &impl IsA<gtk::Widget>) {
    let jobs = selected_jobs(state);
    if jobs.is_empty() {
        state.toast("Select photos to export");
        return;
    }
    export_dialog(state, parent, jobs);
}

fn selected_jobs(state: &App) -> Vec<ExportJob> {
    let selected = selected_cards(state);
    let cards = state.cards.borrow();
    selected
        .iter()
        .filter_map(|child| child.widget_name().parse::<i64>().ok())
        .filter_map(|id| cards.get(&id).map(|(photo, _)| (id, photo.path.clone())))
        .map(|(id, path)| {
            let document = state
                .catalog
                .load_edits(id)
                .ok()
                .flatten()
                .unwrap_or_else(|| Document::new(path.to_string_lossy().to_string()));
            ExportJob { source: Source::Photo { id, path }, document }
        })
        .collect()
}

fn export_dialog(state: &App, parent: &impl IsA<gtk::Widget>, jobs: Vec<ExportJob>) {
    let Some(library) = state.library.borrow().clone() else {
        state.toast("No library selected — pick one to export into");
        return;
    };

    let settings = state.export_settings.borrow().clone();

    let group = adw::PreferencesGroup::new();

    let formats = gtk::StringList::new(&["JPEG", "PNG"]);
    let format = adw::ComboRow::new();
    format.set_title("Format");
    format.set_model(Some(&formats));
    format.set_selected(match settings.format {
        export::Format::Jpeg => 0,
        export::Format::Png => 1,
    });
    group.add(&format);

    let space_names: Vec<&str> = ColourSpace::ALL.iter().map(|space| space.name()).collect();
    let spaces = gtk::StringList::new(&space_names);
    let space = adw::ComboRow::new();
    space.set_title("Colour space");
    space.set_subtitle(settings.space.note());
    space.set_model(Some(&spaces));
    space.set_selected(
        ColourSpace::ALL.iter().position(|one| *one == settings.space).unwrap_or(0) as u32,
    );
    space.connect_selected_notify(|row| {
        let chosen = ColourSpace::ALL.get(row.selected() as usize).copied().unwrap_or_default();
        row.set_subtitle(chosen.note());
    });
    group.add(&space);

    let quality = adw::SpinRow::with_range(1.0, 100.0, 1.0);
    quality.set_title("Quality");
    quality.set_subtitle("92 is where a second copy stops being distinguishable");
    quality.set_value(settings.quality as f64);
    quality.set_sensitive(settings.format.is_lossy());
    group.add(&quality);

    const EDGES: [u32; 5] = [4096, 2560, 2048, 1600, 1080];
    let mut labels = vec!["Full size".to_string()];
    labels.extend(EDGES.iter().map(|edge| format!("{edge} px on the long edge")));
    let sizes = gtk::StringList::new(&labels.iter().map(String::as_str).collect::<Vec<_>>());
    let size = adw::ComboRow::new();
    size.set_title("Size");
    size.set_model(Some(&sizes));
    size.set_selected(match settings.size {
        export::Size::Full => 0,
        export::Size::LongEdge(edge) => {
            EDGES.iter().position(|e| *e == edge).map_or(0, |at| at as u32 + 1)
        }
    });
    group.add(&size);

    let sharpen = adw::SwitchRow::new();
    sharpen.set_title("Sharpen after resizing");
    sharpen.set_subtitle("Puts back the edge the reduction took off");
    sharpen.set_active(settings.sharpen);
    sharpen.set_sensitive(!matches!(settings.size, export::Size::Full));
    group.add(&sharpen);

    let metadata = adw::SwitchRow::new();
    metadata.set_title("Keep the camera's data");
    metadata.set_subtitle("Camera, lens, exposure, date. JPEG only");
    metadata.set_active(settings.metadata);
    group.add(&metadata);

    let destination = Rc::new(RefCell::new(
        settings.folder.clone().unwrap_or_else(|| library.export_dir()),
    ));
    let folder = adw::ActionRow::new();
    folder.set_title("Folder");
    folder.set_subtitle(&destination.borrow().display().to_string());
    folder.set_subtitle_lines(2);
    let choose = gtk::Button::from_icon_name("folder-open-symbolic");
    choose.set_valign(gtk::Align::Center);
    choose.add_css_class("flat");
    choose.set_tooltip_text(Some("Choose another folder"));
    choose.connect_clicked(glib::clone!(
        #[weak] folder,
        #[strong] destination,
        move |button| {
            let dialog = gtk::FileDialog::new();
            dialog.set_title("Export into");
            dialog.set_initial_folder(Some(&gtk::gio::File::for_path(
                destination.borrow().as_path(),
            )));
            let window = button.root().and_downcast::<gtk::Window>();
            dialog.select_folder(
                window.as_ref(),
                gtk::gio::Cancellable::NONE,
                glib::clone!(
                    #[weak] folder,
                    #[strong] destination,
                    move |chosen| {

                        let Some(path) = chosen.ok().and_then(|file| file.path()) else { return };
                        folder.set_subtitle(&path.display().to_string());
                        *destination.borrow_mut() = path;
                    }
                ),
            );
        }
    ));
    folder.add_suffix(&choose);
    folder.set_activatable_widget(Some(&choose));
    group.add(&folder);

    size.connect_selected_notify(glib::clone!(
        #[weak] sharpen,
        move |row| sharpen.set_sensitive(row.selected() != 0)
    ));

    format.connect_selected_notify(glib::clone!(
        #[weak] quality,
        #[weak] metadata,
        move |row| {
            let lossy = row.selected() == 0;
            quality.set_sensitive(lossy);
            metadata.set_sensitive(lossy);
        }
    ));

    let jobs_holder = Rc::new(RefCell::new(jobs));
    let count = jobs_holder.borrow().len();

    let dialog = adw::AlertDialog::new(
        Some(&match count {
            1 => "Export this photograph".to_string(),
            many => format!("Export {many} photographs"),
        }),
        Some(&format!("Into {}", library.export_dir().display())),
    );
    dialog.set_extra_child(Some(&group));
    dialog.add_response("cancel", "Cancel");
    dialog.add_response("export", "Export");
    dialog.set_response_appearance("export", adw::ResponseAppearance::Suggested);
    dialog.set_default_response(Some("export"));
    dialog.set_close_response("cancel");

    dialog.connect_response(
        None,
        glib::clone!(
            #[strong] state,
            #[strong] jobs_holder,
            move |_, response| {
                if response != "export" {
                    return;
                }
                let chosen = export::ExportSettings {
                    template: settings.template.clone(),
                    sharpen: sharpen.is_active(),
                    folder: Some(destination.borrow().clone()),
                    format: match format.selected() {
                        1 => export::Format::Png,
                        _ => export::Format::Jpeg,
                    },
                    quality: quality.value() as u8,
                    size: match size.selected() {
                        0 => export::Size::Full,
                        at => export::Size::LongEdge(EDGES[(at - 1) as usize]),
                    },
                    metadata: metadata.is_active(),
                    space: ColourSpace::ALL
                        .get(space.selected() as usize)
                        .copied()
                        .unwrap_or_default(),
                };
                *state.export_settings.borrow_mut() = chosen.clone();
                state.catalog.remember(EXPORT_SETTINGS, &chosen);
                refresh_export_button(&state);

                let jobs = std::mem::take(&mut *jobs_holder.borrow_mut());
                let directory = destination.borrow().clone();
                run_export(&state, jobs, chosen, directory);
            }
        ),
    );

    dialog.present(Some(parent));
}

fn run_export(
    state: &App,
    jobs: Vec<ExportJob>,
    settings: export::ExportSettings,
    directory: PathBuf,
) {
    let total = jobs.len();
    let progress = adw::Toast::new(&format!("Exporting {total}…"));
    progress.set_timeout(0);

    let cancel = Cancel::default();
    if total > 1 {
        progress.set_button_label(Some("Stop"));
        progress.connect_button_clicked(glib::clone!(
            #[strong] cancel,
            move |_| cancel.stop()
        ));
    }
    state.toasts.add_toast(progress.clone());

    let state = state.clone();
    glib::spawn_future_local(async move {
        let mut written = 0usize;
        let mut failures: Vec<String> = Vec::new();
        let mut last = String::new();
        let mut stopped = false;

        for (index, job) in jobs.into_iter().enumerate() {
            if cancel.stopped() {
                stopped = true;
                break;
            }
            if total > 1 {
                progress.set_title(&format!("Exporting {} of {total}…", index + 1));
            }

            let settings = settings.clone();
            let directory = directory.clone();
            let result = busy(&state, "Exporting…", move || {
                let destination = export::next_path(&directory, &job.source.name(), &settings)?;
                let linear = job.source.full_resolution()?;

                if job.document.ai_denoise > 0.0 && numa::render::ai_denoise::is_installed() {
                    numa::render::ai_denoise::ensure(std::path::Path::new(&job.document.source.path), &linear, |_, _| true)?;
                }

                let mut document = render::with_masks_resolved(&job.document, &linear);

                document.output_space = settings.space;
                let image = export::fit(render::develop(&document, &linear), &settings);
                let raf = match &job.source {
                    Source::Photo { path, .. } => Some(path.clone()),
                    Source::Bracket { .. } => None,
                };
                export::save(&image, &destination, &settings, raf.as_deref())?;
                Ok::<PathBuf, String>(destination)
            })
            .await;

            match result {
                Ok(Ok(destination)) => {
                    written += 1;
                    last = destination.file_name().unwrap_or_default().to_string_lossy().into();
                    remember_recent(&destination);
                }
                Ok(Err(err)) => failures.push(err),
                Err(_) => failures.push("cancelled".to_string()),
            }
        }

        progress.dismiss();
        if stopped {
            state.toast(&format!(
                "Stopped after {written} of {total} — those files are written"
            ));
            return;
        }
        state.toast(&match (written, failures.as_slice()) {
            (1, []) => format!("Exported {last}"),
            (n, []) => format!("Exported {n} photographs"),
            (0, [only]) => format!("Export failed: {only}"),
            (n, many) => format!("Exported {n}, {} failed", many.len()),
        });
        for failure in &failures {
            log::warn!("export: {failure}");
        }
    });
}

fn build_adjustment_panel(state: &App) -> gtk::Box {
    let pages = state.panel_stack.clone();
    pages.set_vexpand(true);
    let all = state.sliders.each();

    {
        let scratch = Sliders::new();
        scratch.write(Basic::default());

        for ((_, real, _), (_, rest, _)) in all.iter().zip(scratch.each().iter()).skip(2) {
            set_neutral(real, rest.value());
        }
    }

    state.global_only.borrow_mut().clear();
    let global_only = |widget: &gtk::Widget| {
        state.global_only.borrow_mut().push(widget.clone());
    };

    let light = page_column();

    light.add_css_class("quiet");

    light.append(&section_header("Tone"));

    let auto_tone_button = gtk::Button::with_label("Auto");
    auto_tone_button.set_tooltip_text(Some(
        "Set exposure and the black and white points from this photograph",
    ));
    auto_tone_button.set_margin_bottom(6);
    auto_tone_button.connect_clicked(glib::clone!(
        #[strong] state,

        move |_| busy_sync(&state, "Looking at the photograph…", auto_tone)
    ));

    if false {
        global_only(auto_tone_button.as_ref());
        light.append(&auto_tone_button);
    }
    for (name, scale, readout) in &all[2..9] {
        let row = slider_row(state, name, scale, *readout);

        if *name == "Exposure" {
            row.add_css_class("lead");
        }

        if matches!(*name, "HDR" | "Clarity" | "Texture") {
            global_only(row.as_ref());
        }
        light.append(&row);
    }

    light.append(&build_mask_editor(state));

    let curve_header = section_header("Tone curve");
    let curve = build_tone_curve(state);
    global_only(curve_header.as_ref());
    global_only(curve.as_ref());
    light.append(&curve_header);
    light.append(&curve);

    let presence_header = section_header("Presence");
    global_only(presence_header.as_ref());
    light.append(&presence_header);

    for (name, scale, readout) in all[11..13].iter().chain(&all[22..23]) {
        let row = slider_row(state, name, scale, *readout);
        global_only(row.as_ref());
        light.append(&row);
    }

    for (title, range) in [("Vignette", 23..27), ("Grain", 27..30)] {
        let header = section_header(title);
        global_only(header.as_ref());
        light.append(&header);
        for (name, scale, readout) in &all[range] {
            let row = slider_row(state, name, scale, *readout);
            global_only(row.as_ref());
            light.append(&row);
        }
    }
    pages.add_named(&wrap_page(&light), Some("light"));

    let colour = page_column();

    colour.add_css_class("quiet");
    let balance_header = section_header("White balance");
    global_only(balance_header.as_ref());
    colour.append(&balance_header);
    for (name, scale, readout) in &all[..2] {

        let row = slider_row(state, name, scale, *readout);
        if *name == "Temperature" {
            row.add_css_class("lead");
        }
        global_only(row.as_ref());
        colour.append(&row);
    }

    let white = state.white_pipette.clone();
    let inside = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    inside.append(&gtk::Image::from_icon_name("color-select-symbolic"));
    inside.append(&gtk::Label::new(Some("Pick a neutral")));
    white.set_child(Some(&inside));
    white.set_tooltip_text(Some("Click something in the photograph that should be grey or white"));
    white.set_halign(gtk::Align::Start);
    white.set_margin_top(4);
    white.connect_toggled(glib::clone!(
        #[strong] state,
        move |button| {
            if state.applying.get() {
                return;
            }
            state.picking_white.set(button.is_active());
            if button.is_active() {
                disarm_pipettes(&state, "white");
            }
            arm_band_pipette(&state);
        }
    ));
    global_only(white.as_ref());
    colour.append(&white);
    let profile_header = section_header("Profile");
    let profile = build_profile_picker(state);
    global_only(profile_header.as_ref());
    global_only(profile.as_ref());
    colour.append(&profile_header);
    colour.append(&profile);

    let space_header = section_header("Colour space");
    global_only(space_header.as_ref());
    colour.append(&space_header);
    let space = build_space_picker(state);
    global_only(space.as_ref());
    colour.append(&space);

    colour.append(&section_header("Colour"));
    for (name, scale, readout) in &all[9..11] {
        let row = slider_row(state, name, scale, *readout);
        if *name == "Vibrance" {
            row.add_css_class("lead");
        }
        colour.append(&row);
    }
    let mixer_header = section_header("Colour mixer");
    let mixer = build_mixer(state);
    global_only(mixer_header.as_ref());
    global_only(mixer.as_ref());
    colour.append(&mixer_header);
    colour.append(&mixer);

    let point_header = section_header("Point colour");
    let point = build_point_colours(state);
    global_only(point_header.as_ref());
    global_only(point.as_ref());
    colour.append(&point_header);
    colour.append(&point);

    let grading_header = section_header("Colour grading");
    let grading = build_grading(state);
    global_only(grading_header.as_ref());
    global_only(grading.as_ref());
    colour.append(&grading_header);
    colour.append(&grading);

    let calibration = section_header("Calibration");
    global_only(calibration.as_ref());
    colour.append(&calibration);
    for (name, scale, readout) in &all[30..37] {
        let row = slider_row(state, name, scale, *readout);
        global_only(row.as_ref());
        colour.append(&row);
    }
    pages.add_named(&wrap_page(&colour), Some("colour"));

    let detail = page_column();

    detail.add_css_class("quiet");

    let sharpening = section_header("Sharpening");
    global_only(sharpening.as_ref());
    detail.append(&sharpening);
    for (name, scale, readout) in &all[13..16] {
        let row = slider_row(state, name, scale, *readout);
        if *name == "Sharpening" {
            row.add_css_class("lead");
        }
        global_only(row.as_ref());
        detail.append(&row);
    }
    let noise = section_header("Noise");
    global_only(noise.as_ref());
    detail.append(&noise);
    for (name, scale, readout) in &all[16..20] {
        let row = slider_row(state, name, scale, *readout);
        global_only(row.as_ref());
        detail.append(&row);
    }
    let ai = ai_denoise::build(state);
    global_only(ai.as_ref());
    detail.append(&ai);

    let lens = section_header("Lens");
    global_only(lens.as_ref());
    detail.append(&lens);

    for (name, scale, readout) in all[20..22].iter().chain(&all[37..39]) {
        let row = slider_row(state, name, scale, *readout);
        global_only(row.as_ref());
        detail.append(&row);
    }

    let lens_note = gtk::Label::new(Some(
        "These act only where the fault is — a fringed edge, a moiré pattern — \
         and on a frame without one they correctly do nothing. Zoom to 100 % on \
         a hard edge to judge them.",
    ));
    lens_note.set_xalign(0.0);
    lens_note.set_wrap(true);
    lens_note.set_margin_top(8);
    lens_note.add_css_class("profile-note");
    global_only(lens_note.as_ref());
    detail.append(&lens_note);
    let note = gtk::Label::new(Some(
        "Below 100 % these work on detail smaller than a screen pixel. Zoom in to judge them.",
    ));
    note.set_xalign(0.0);
    note.set_wrap(true);
    note.set_margin_top(8);
    note.add_css_class("profile-note");
    global_only(note.as_ref());
    detail.append(&note);
    pages.add_named(&wrap_page(&detail), Some("detail"));

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

    retouch.append(&section_header("Heal and clone"));
    retouch.append(&build_retouch(state));

    retouch.append(&build_face(state));
    pages.add_named(&wrap_page(&retouch), Some("retouch"));

    pages.add_named(&wrap_page(&crop), Some("crop"));

    let presets = state.presets_page.clone();
    presets.set_margin_top(6);
    presets.set_margin_bottom(12);
    presets.set_margin_start(14);
    presets.set_margin_end(14);
    pages.add_named(&presets, Some("presets"));

    let tabs = state.panel_tab_strip.clone();
    tabs.add_css_class("linked");

    tabs.set_homogeneous(true);
    tabs.set_margin_start(14);
    tabs.set_margin_end(14);
    tabs.set_margin_bottom(8);

    let mut first: Option<gtk::ToggleButton> = None;
    for (name, tooltip, icon) in PANEL_TABS {
        let tab = gtk::ToggleButton::new();
        tab.set_hexpand(true);
        tab.set_tooltip_text(Some(tooltip));
        match icon {

            None if name == "masks" => tab.set_child(Some(&mask_icon())),
            None => tab.set_child(Some(&face_icon())),
            Some(icon) => tab.set_icon_name(icon),
        }
        match &first {
            None => {
                tab.set_active(true);
                first = Some(tab.clone());
            }
            Some(first) => tab.set_group(Some(first)),
        }
        tab.connect_toggled(glib::clone!(
            #[strong] state,
            move |tab| {
                if tab.is_active() && !state.applying.get() {
                    show_panel_tab(&state, name);
                }
            }
        ));
        state.panel_tabs.borrow_mut().push((name, tab.clone()));
        tabs.append(&tab);
    }

    let panel = gtk::Box::new(gtk::Orientation::Vertical, 0);
    let histogram = build_histogram(state);
    histogram.set_margin_top(14);
    histogram.set_margin_start(14);
    histogram.set_margin_end(14);
    panel.append(&histogram);
    panel.append(&build_mask_banner(state));
    panel.append(&tabs);
    panel.append(&pages);

    panel.set_hexpand(false);
    panel
}

const PANEL_TABS: [(&str, &str, Option<&str>); 7] = [
    ("light", "Light", Some("display-brightness-symbolic")),
    ("colour", "Colour", Some("color-select-symbolic")),
    ("detail", "Detail — judge these at 1:1", Some("edit-find-symbolic")),
    ("masks", "Masks", None),
    ("retouch", "Heal, clone and the face", None),
    ("crop", "Crop and straighten", Some("edit-cut-symbolic")),
    ("presets", "Presets", Some("starred-symbolic")),
];

fn page_column() -> gtk::Box {
    let column = gtk::Box::new(gtk::Orientation::Vertical, 2);
    column.set_margin_top(6);
    column.set_margin_bottom(18);
    column.set_margin_start(14);
    column.set_margin_end(14);
    column
}

fn wrap_page(column: &gtk::Box) -> gtk::ScrolledWindow {
    let scroller = gtk::ScrolledWindow::new();
    scroller.set_policy(gtk::PolicyType::Never, gtk::PolicyType::Automatic);
    scroller.add_css_class("adjustment");
    scroller.set_child(Some(column));
    scroller.set_vexpand(true);
    scroller
}

fn show_panel_tab(state: &App, name: &str) {
    let was_cropping = is_cropping(state);
    if name == "presets" {
        fill_presets_page(state);
    }
    state.panel_stack.set_visible_child_name(name);

    let cropping = name == "crop";
    if was_cropping != cropping {
        toggle_crop(state, cropping);
    }

    let retouching = name == "retouch";
    if state.retouch_on.get() != retouching {
        toggle_retouch(state, retouching);
    }

    state.applying.set(true);
    for (tab, button) in state.panel_tabs.borrow().iter() {
        button.set_active(*tab == name);
    }
    state.applying.set(false);
}

fn draw_tone_distribution(
    context: &gtk::cairo::Context,
    width: f64,
    height: f64,
    histogram: Option<&render::histogram::Histogram>,
) {
    let Some(histogram) = histogram else { return };

    let scale = histogram.scale() as f64;
    let step = width / render::histogram::BINS as f64;

    context.set_source_rgba(1.0, 1.0, 1.0, 0.13);
    context.move_to(0.0, height);
    for bin in 0..render::histogram::BINS {

        let count = histogram.channels.iter().map(|channel| channel[bin]).max().unwrap_or(0);
        let value = (count as f64 / scale).min(1.0);
        context.line_to(bin as f64 * step, height - value * height);
    }
    context.line_to(width, height);
    context.close_path();
    let _ = context.fill();
}

fn build_tone_curve(state: &App) -> gtk::Box {
    let column = gtk::Box::new(gtk::Orientation::Vertical, 4);
    column.set_margin_bottom(6);

    let area = state.curve_area.clone();
    area.set_content_height(PANEL_WIDTH - 28);
    area.set_hexpand(true);
    area.add_css_class("tone-curve");

    area.set_draw_func(glib::clone!(
        #[strong] state,
        move |_, context, width, height| {
            let open = state.open.borrow();
            let curves = open.as_ref().map(|photo| photo.document.curves());
            draw_curve(
                context,
                width as f64,
                height as f64,
                curves.as_ref(),
                state.curve_channel.get(),
                state.histogram.borrow().as_ref(),
            );
        }
    ));

    let held: Rc<Cell<Option<usize>>> = Rc::new(Cell::new(None));

    const GRAB: f32 = 0.04;

    let drag = gtk::GestureDrag::new();
    drag.connect_drag_begin(glib::clone!(
        #[strong] state,
        #[strong] held,
        move |_, x, y| {
            let Some(at) = curve_point(&state, x, y) else { return };
            let index = with_curve(&state, |curve| curve.place(at, GRAB));
            held.set(index);
            state.curve_area.queue_draw();
        }
    ));
    drag.connect_drag_update(glib::clone!(
        #[strong] state,
        #[strong] held,
        move |gesture, dx, dy| {
            let Some(index) = held.get() else { return };
            let Some((sx, sy)) = gesture.start_point() else { return };
            let Some(at) = curve_point(&state, sx + dx, sy + dy) else { return };

            with_curve(&state, |curve| curve.move_point(index, at));
            state.curve_area.queue_draw();
        }
    ));
    drag.connect_drag_end(glib::clone!(
        #[strong] held,
        move |_, _, _| held.set(None)
    ));
    area.add_controller(drag);

    let remove = gtk::GestureClick::new();
    remove.set_button(gtk::gdk::BUTTON_SECONDARY);
    remove.connect_pressed(glib::clone!(
        #[strong] state,
        move |_, _, x, y| {
            let Some([at, _]) = curve_point(&state, x, y) else { return };
            let found = state.open.borrow().as_ref().and_then(|photo| {
                let curve = photo.document.curves()[state.curve_channel.get()].clone();
                let nearest = curve
                    .points()
                    .iter()
                    .enumerate()
                    .min_by(|(_, a), (_, b)| (a[0] - at).abs().total_cmp(&(b[0] - at).abs()))?;
                ((nearest.1[0] - at).abs() <= GRAB).then_some(nearest.0)
            });

            let Some(index) = found else { return };
            with_curve(&state, |curve| curve.remove(index));
            state.curve_area.queue_draw();
        }
    ));
    area.add_controller(remove);

    let channels = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    channels.add_css_class("linked");
    let mut first: Option<gtk::ToggleButton> = None;
    for (index, (label, tooltip)) in
        [("RGB", "All channels"), ("R", "Red"), ("G", "Green"), ("B", "Blue")].into_iter().enumerate()
    {
        let button = gtk::ToggleButton::with_label(label);
        button.set_hexpand(true);
        button.set_tooltip_text(Some(tooltip));
        button.set_active(index == 0);
        if let Some(first) = &first {
            button.set_group(Some(first));
        } else {
            first = Some(button.clone());
        }
        button.connect_toggled(glib::clone!(
            #[strong] state,
            move |button| {
                if button.is_active() {
                    state.curve_channel.set(index);
                    state.curve_area.queue_draw();
                }
            }
        ));
        channels.append(&button);
    }

    let hint = gtk::Label::new(Some("Drag to shape · right-click a point to remove"));
    hint.set_xalign(0.0);
    hint.add_css_class("profile-note");

    column.append(&channels);
    column.append(&area);
    column.append(&hint);
    column
}

const HANDLE: f64 = 4.0;

fn plot_rect(width: f64, height: f64) -> (f64, f64, f64, f64) {
    let inset = (HANDLE + 1.0).min(width / 4.0).min(height / 4.0);
    (inset, inset, (width - inset * 2.0).max(1.0), (height - inset * 2.0).max(1.0))
}

fn curve_point(state: &App, x: f64, y: f64) -> Option<[f32; 2]> {
    let (width, height) = (state.curve_area.width() as f64, state.curve_area.height() as f64);
    if width <= 0.0 || height <= 0.0 {
        return None;
    }

    let (left, top, plot_width, plot_height) = plot_rect(width, height);
    Some([
        ((x - left) / plot_width).clamp(0.0, 1.0) as f32,
        (1.0 - (y - top) / plot_height).clamp(0.0, 1.0) as f32,
    ])
}

fn with_curve<T>(state: &App, edit: impl FnOnce(&mut Curve) -> T) -> Option<T> {
    let out = {
        let mut open = state.open.borrow_mut();
        let photo = open.as_mut()?;
        let mut curves = photo.document.curves();
        let out = edit(&mut curves[state.curve_channel.get()]);
        photo.document.set_curves(curves);
        out
    };

    request_render(state);
    schedule_history_push(state);
    Some(out)
}

fn draw_curve(
    context: &gtk::cairo::Context,
    width: f64,
    height: f64,
    curves: Option<&[Curve; 4]>,
    channel: usize,
    histogram: Option<&render::histogram::Histogram>,
) {

    let _ = context.save();
    draw_tone_distribution(context, width, height, histogram);
    let _ = context.restore();

    let (left, top, plot_width, plot_height) = plot_rect(width, height);
    let at = |x: f64, y: f64| (left + x * plot_width, top + (1.0 - y) * plot_height);

    context.set_line_width(1.0);
    context.set_source_rgba(1.0, 1.0, 1.0, 0.10);
    for step in 1..4 {
        let fraction = step as f64 / 4.0;
        let (vertical, _) = at(fraction, 0.0);
        let (_, horizontal) = at(0.0, fraction);
        context.move_to(vertical, top);
        context.line_to(vertical, top + plot_height);
        context.move_to(left, horizontal);
        context.line_to(left + plot_width, horizontal);
    }
    let _ = context.stroke();

    context.set_dash(&[3.0, 3.0], 0.0);
    context.set_source_rgba(1.0, 1.0, 1.0, 0.18);
    let (x0, y0) = at(0.0, 0.0);
    let (x1, y1) = at(1.0, 1.0);
    context.move_to(x0, y0);
    context.line_to(x1, y1);
    let _ = context.stroke();
    context.set_dash(&[], 0.0);

    let Some(curves) = curves else { return };
    let colours = [(0.95, 0.95, 0.95), (0.95, 0.42, 0.40), (0.45, 0.85, 0.45), (0.45, 0.62, 0.98)];

    const SAMPLES: usize = 128;
    let trace = |curve: &Curve| {
        for step in 0..=SAMPLES {
            let x = step as f64 / SAMPLES as f64;
            let (px, py) = at(x, curve.value_at(x as f32) as f64);
            context.line_to(px, py);
        }
        let _ = context.stroke();
    };

    context.set_line_width(1.2);
    for (index, other) in curves.iter().enumerate() {
        if index != channel && !other.is_identity() {
            let (r, g, b) = colours[index];
            context.set_source_rgba(r, g, b, 0.35);
            trace(other);
        }
    }

    let curve = &curves[channel.min(3)];
    let (r, g, b) = colours[channel.min(3)];
    context.set_line_width(1.8);
    context.set_source_rgb(r, g, b);
    trace(curve);

    for [x, y] in curve.points() {
        let (px, py) = at(*x as f64, *y as f64);
        context.arc(px, py, HANDLE, 0.0, std::f64::consts::TAU);
        context.set_source_rgb(r, g, b);
        let _ = context.fill();
    }
}

fn build_space_picker(state: &App) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Vertical, 4);
    row.set_margin_bottom(6);

    let names: Vec<&str> = ColourSpace::ALL.iter().map(|space| space.name()).collect();
    let picker = state.space_picker.clone();
    picker.set_model(Some(&gtk::StringList::new(&names)));
    picker.set_hexpand(true);
    narrow_dropdown(&picker);
    picker.connect_selected_notify(glib::clone!(
        #[strong] state,
        move |picker| {
            if state.applying.get() {
                return;
            }
            let Some(chosen) = ColourSpace::ALL.get(picker.selected() as usize).copied() else {
                return;
            };
            {
                let mut open = state.open.borrow_mut();
                let Some(photo) = open.as_mut() else { return };
                if photo.document.working_space == chosen {
                    return;
                }
                photo.document.working_space = chosen;

                photo.working = render::to_working_space(&photo.document, &photo.proxy);
                photo.full_working = None;
                photo.full_working_key = None;
                photo.draft = None;
                photo.view = None;
            }
            write_space_note(&state);
            request_render(&state);
            schedule_history_push(&state);
        }
    ));

    row.append(&picker);
    state.space_note.set_xalign(0.0);
    state.space_note.set_wrap(true);
    state.space_note.add_css_class("profile-note");
    row.append(&state.space_note);
    write_space_note(state);

    row
}

fn write_space_note(state: &App) {
    let space = state
        .open
        .borrow()
        .as_ref()
        .map(|photo| photo.document.working_space)
        .unwrap_or_default();

    state.applying.set(true);
    let at = ColourSpace::ALL.iter().position(|one| *one == space).unwrap_or(0);
    state.space_picker.set_selected(at as u32);
    state.applying.set(false);

    let note = match space {
        ColourSpace::Srgb => space.note().to_string(),
        other => format!("{}. The screen still shows sRGB", other.note()),
    };
    state.space_note.set_text(&note);
}

fn build_profile_picker(state: &App) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Vertical, 4);
    row.set_margin_bottom(6);

    state.profile_picker.set_hexpand(true);
    narrow_dropdown(&state.profile_picker);
    state.profile_picker.connect_selected_notify(glib::clone!(
        #[strong] state,
        move |picker| {
            if state.applying.get() {
                return;
            }
            let Some(chosen) = profile_choices(&state).get(picker.selected() as usize).cloned()
            else {
                return;
            };

            {
                let mut open = state.open.borrow_mut();
                let Some(photo) = open.as_mut() else { return };
                if photo.document.colour_profile == chosen {
                    return;
                }
                photo.document.colour_profile = chosen;

                photo.working = render::to_working_space(&photo.document, &photo.proxy);
                photo.full_working = None;
                photo.full_working_key = None;

                photo.draft = None;
                photo.view = None;
            }

            request_render(&state);
            schedule_history_push(&state);
        }
    ));

    row.append(&state.profile_picker);

    state.profile_label.set_xalign(0.0);
    state.profile_label.set_wrap(true);
    state.profile_label.add_css_class("profile-note");
    row.append(&state.profile_label);

    row
}

fn profile_choices(state: &App) -> Vec<Option<String>> {
    let mut choices = vec![None, Some(render::NO_COLOUR_PROFILE.to_string())];
    choices.extend(camera_profiles(state).into_iter().map(Some));
    choices
}

fn camera_profiles(state: &App) -> Vec<String> {
    let open = state.open.borrow();
    let Some(summary) = open.as_ref().and_then(|photo| photo.summary.as_ref()) else {
        return Vec::new();
    };
    dcp::names_for_camera(&summary.make, &summary.model)
}

fn refresh_profile_picker(state: &App) {
    let (chosen, automatic) = {
        let open = state.open.borrow();
        let Some(photo) = open.as_ref() else { return };
        (
            photo.document.colour_profile.clone(),
            photo.proxy.rendering.as_ref().map(|profile| profile.name.clone()),
        )
    };

    let mut labels = vec![
        match &automatic {
            Some(name) => format!("Automatic — {name}"),
            None => "Automatic — none for this camera".to_string(),
        },
        "None (colour matrix only)".to_string(),
    ];
    labels.extend(camera_profiles(state));
    let model = gtk::StringList::new(&labels.iter().map(String::as_str).collect::<Vec<_>>());

    state.applying.set(true);
    state.profile_picker.set_model(Some(&model));
    let index = profile_choices(state)
        .iter()
        .position(|choice| *choice == chosen)
        .unwrap_or(0);
    state.profile_picker.set_selected(index as u32);
    state.applying.set(false);

    state.profile_label.set_text(
        &dcp::profiles_dir()
            .map(|dir| format!("More profiles: .dcp files in {}", dir.display()))
            .unwrap_or_default(),
    );
}

fn build_crop_controls(state: &App) -> gtk::Box {
    let column = state.crop_controls.clone();
    column.set_orientation(gtk::Orientation::Vertical);
    column.set_spacing(6);
    column.set_visible(false);

    column.append(&section_header("Crop"));

    let aspects = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    aspects.add_css_class("linked");
    let mut first: Option<gtk::ToggleButton> = None;
    for (label, ratio) in [
        ("Free", None),
        ("1:1", Some(1.0)),
        ("4:5", Some(0.8)),
        ("3:2", Some(1.5)),
        ("16:9", Some(16.0 / 9.0)),
    ] {
        let button = gtk::ToggleButton::with_label(label);
        button.set_hexpand(true);
        match &first {
            None => {
                button.set_active(true);
                first = Some(button.clone());
            }
            Some(first) => button.set_group(Some(first)),
        }
        button.connect_toggled(glib::clone!(
            #[strong] state,
            move |button| {
                if !button.is_active() || state.applying.get() {
                    return;
                }
                state.crop_ratio.set(ratio);
                if let Some(ratio) = ratio {
                    apply_aspect(&state, ratio);
                }
                commit_crop(&state);
            }
        ));
        aspects.append(&button);
    }
    aspects.add_css_class("aspect-ratios");
    column.append(&aspects);
    let free = first.clone();

    let turns = gtk::Box::new(gtk::Orientation::Horizontal, 0);
    turns.add_css_class("linked");
    for (icon, degrees) in [
        ("object-rotate-left-symbolic", -90.0f32),
        ("object-rotate-right-symbolic", 90.0),
    ] {
        let button = gtk::Button::from_icon_name(icon);
        button.set_hexpand(true);
        button.connect_clicked(glib::clone!(
            #[strong] state,
            move |_| {
                {
                    let mut open = state.open.borrow_mut();
                    let Some(photo) = open.as_mut() else { return };
                    let turned = photo.document.rotation() + degrees;
                    photo.document.set_rotation(turned);
                }

                state.crop_rect.set([0.0, 0.0, 1.0, 1.0]);
                state.crop_area.queue_draw();
                commit_crop(&state);
            }
        ));
        turns.append(&button);
    }

    for (icon, tooltip, vertical) in [
        ("object-flip-horizontal-symbolic", "Flip left to right", false),
        ("object-flip-vertical-symbolic", "Flip top to bottom", true),
    ] {
        let button = gtk::Button::from_icon_name(icon);
        button.set_hexpand(true);
        button.set_tooltip_text(Some(tooltip));
        button.connect_clicked(glib::clone!(
            #[strong] state,
            move |_| flip_frame(&state, vertical)
        ));
        turns.append(&button);
    }
    column.append(&turns);

    let straighten = state.straighten.clone();
    let row = slider_row(state, "Straighten", &straighten, Readout::Signed(1));
    row.add_css_class("lead");

    straighten.connect_value_changed(glib::clone!(
        #[strong] state,
        move |_| {
            if !state.applying.get() {
                commit_crop(&state);
            }
        }
    ));
    column.append(&row);

    column.append(&section_header("Perspective"));
    for (index, (name, tooltip)) in [
        ("Vertical", "Verticals that converge — a camera pointed up or down"),
        ("Horizontal", "Horizontals that converge — a camera turned left or right"),
        ("Aspect", "What correcting either costs: the frame squashed along the axis it fixed"),
    ]
    .into_iter()
    .enumerate()
    {
        let scale = state.perspective_sliders[index].clone();
        scale.set_tooltip_text(Some(tooltip));
        scale.connect_value_changed(glib::clone!(
            #[strong] state,
            move |_| {
                if !state.applying.get() {
                    read_perspective(&state);
                }
            }
        ));
        let row = slider_row(state, name, &scale, Readout::Signed(0));
        if index == 0 {
            row.add_css_class("lead");
        }
        column.append(&row);
    }

    let auto = gtk::Button::with_label("Auto");
    auto.set_tooltip_text(Some("Straighten and square up from the lines in the photograph"));
    auto.set_margin_top(4);
    auto.connect_clicked(glib::clone!(
        #[strong] state,
        move |_| auto_perspective(&state)
    ));
    column.append(&guided_row(state, &auto));

    let finish = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    finish.set_margin_top(10);

    let reset = gtk::Button::with_label("Reset");
    reset.set_hexpand(true);
    reset.set_tooltip_text(Some("The whole frame again"));
    reset.connect_clicked(glib::clone!(
        #[strong] state,
        #[strong] free,
        move |_| {

            state.crop_ratio.set(None);
            if let Some(free) = &free {
                state.applying.set(true);
                free.set_active(true);
                state.applying.set(false);
            }
            state.crop_rect.set([0.0, 0.0, 1.0, 1.0]);
            state.straighten.set_value(0.0);
            state.applying.set(true);
            for slider in &state.perspective_sliders {
                slider.set_value(0.0);
            }
            state.applying.set(false);
            {
                let mut open = state.open.borrow_mut();
                if let Some(photo) = open.as_mut() {
                    photo.document.set_perspective(Perspective::default());
                    photo.view = None;
                }
            }
            state.crop_area.queue_draw();
            commit_crop(&state);
        }
    ));
    finish.append(&reset);

    let done = gtk::Button::with_label("Done");
    done.set_hexpand(true);
    done.add_css_class("suggested-action");
    done.set_tooltip_text(Some("Keep this crop and leave the tool"));
    done.connect_clicked(glib::clone!(
        #[strong] state,
        move |_| {
            commit_crop(&state);
            show_panel_tab(&state, "light");
        }
    ));
    finish.append(&done);
    column.append(&finish);

    column
}

fn frame_aspect(state: &App) -> f32 {
    let (width, height) = frame_pixels(state);
    width / height
}

fn frame_pixels(state: &App) -> (f32, f32) {
    state.open.borrow().as_ref().map_or((3.0, 2.0), |photo| {
        let (width, height) = (photo.working.width as f32, photo.working.height as f32);
        if matches!(photo.document.rotation() as i32, 90 | 270) {
            (height, width)
        } else {
            (width, height)
        }
    })
}

fn hold_aspect(rect: [f32; 4], handle: usize, target: f32) -> [f32; 4] {
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

fn apply_aspect(state: &App, ratio: f32) {
    let [x, y, width, height] = state.crop_rect.get();
    let (centre_x, centre_y) = (x + width / 2.0, y + height / 2.0);

    let target = ratio / frame_aspect(state);
    let (mut new_width, mut new_height) = if width / height > target {
        (height * target, height)
    } else {
        (width, width / target)
    };

    new_width = new_width.min(1.0);
    new_height = new_height.min(1.0);

    state.crop_rect.set([
        (centre_x - new_width / 2.0).clamp(0.0, 1.0 - new_width),
        (centre_y - new_height / 2.0).clamp(0.0, 1.0 - new_height),
        new_width,
        new_height,
    ]);
    state.crop_area.queue_draw();
}

fn panel_floor(panel: &gtk::Box) -> i32 {
    let (minimum, _, _, _) = panel.measure(gtk::Orientation::Horizontal, -1);
    minimum.max(PANEL_WIDTH)
}

fn narrow_dropdown(picker: &gtk::DropDown) {
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

fn section_header(title: &str) -> gtk::Label {
    let label = gtk::Label::new(Some(&title.to_uppercase()));
    label.set_xalign(0.0);
    label.set_margin_top(14);
    label.set_margin_bottom(8);
    label.add_css_class("section-header");
    label
}

fn shift_moves_ten(widget: &impl IsA<gtk::Widget>, adjustment: &gtk::Adjustment) {

    adjustment.set_page_increment(adjustment.step_increment() * 10.0);

    let keys = gtk::EventControllerKey::new();

    keys.set_propagation_phase(gtk::PropagationPhase::Capture);
    let adjustment = adjustment.clone();
    keys.connect_key_pressed(move |_, key, _, modifiers| {
        if !modifiers.contains(gtk::gdk::ModifierType::SHIFT_MASK) {
            return glib::Propagation::Proceed;
        }
        use gtk::gdk::Key;
        let by = match key {
            Key::Up | Key::Right | Key::KP_Up | Key::KP_Right => adjustment.page_increment(),
            Key::Down | Key::Left | Key::KP_Down | Key::KP_Left => -adjustment.page_increment(),
            _ => return glib::Propagation::Proceed,
        };
        let moved = (adjustment.value() + by).clamp(adjustment.lower(), adjustment.upper());
        adjustment.set_value(moved);
        glib::Propagation::Stop
    });
    widget.as_ref().add_controller(keys);
}

thread_local! {

    static NEUTRALS: RefCell<HashMap<usize, f64>> = RefCell::new(HashMap::new());

    static REGISTERED: RefCell<Vec<gtk::Scale>> = const { RefCell::new(Vec::new()) };
}

fn set_neutral(scale: &gtk::Scale, value: f64) {
    NEUTRALS.with(|neutrals| neutrals.borrow_mut().insert(scale.as_ptr() as usize, value));
}

fn neutral_of(scale: &gtk::Scale) -> Option<f64> {
    NEUTRALS.with(|neutrals| neutrals.borrow().get(&(scale.as_ptr() as usize)).copied())
}

fn slider_row(state: &App, name: &str, scale: &gtk::Scale, readout: Readout) -> gtk::Box {
    REGISTERED.with(|registered| registered.borrow_mut().push(scale.clone()));
    let row = gtk::Box::new(gtk::Orientation::Vertical, 0);
    row.set_margin_bottom(6);

    let header = gtk::Box::new(gtk::Orientation::Horizontal, 8);
    let title = gtk::Label::new(Some(name));
    title.set_xalign(0.0);
    title.set_hexpand(true);
    title.add_css_class("slider-name");

    let value = gtk::Label::new(Some("0"));
    value.set_xalign(1.0);
    value.set_width_chars(6);
    value.add_css_class("slider-value");

    header.append(&title);
    header.append(&value);

    scale.set_hexpand(true);
    shift_moves_ten(scale, &scale.adjustment());

    scale.set_draw_value(false);

    scale.set_has_origin(false);

    scale.connect_value_changed(glib::clone!(
        #[strong] state,
        #[weak] value,
        move |scale| {
            value.set_text(&readout.format(scale.value()));
            adjustments_changed(&state);
        }
    ));

    let reset = glib::clone!(
        #[strong] state,
        #[weak] scale,
        move || {
            let original = match readout {
                Readout::Kelvin => state
                    .open
                    .borrow()
                    .as_ref()
                    .map_or(5500.0, |photo| photo.as_shot.temperature as f64),

                _ if neutral_of(&scale).is_some() => neutral_of(&scale).unwrap_or_default(),
                Readout::Signed(_) | Readout::Positive(_) | Readout::Degrees => 0.0,

                Readout::Radius => 1.0,
                Readout::Middle => 50.0,
            };
            scale.set_value(original);
        }
    );

    let right_click = gtk::GestureClick::new();
    right_click.set_button(gtk::gdk::BUTTON_SECONDARY);
    right_click.connect_pressed(glib::clone!(
        #[strong] reset,
        move |_, _, _, _| reset()
    ));
    scale.add_controller(right_click);

    let double_click = gtk::GestureClick::new();
    double_click.set_button(gtk::gdk::BUTTON_PRIMARY);

    double_click.set_propagation_phase(gtk::PropagationPhase::Capture);
    double_click.connect_pressed(glib::clone!(
        #[strong] reset,
        move |gesture, presses, _, _| {
            if presses == 2 {
                gesture.set_state(gtk::EventSequenceState::Claimed);
                reset();
            }
        }
    ));
    scale.add_controller(double_click);

    let on_value = gtk::GestureClick::new();
    on_value.set_button(0);
    on_value.connect_pressed(glib::clone!(
        #[strong] reset,
        move |gesture, presses, _, _| {
            let button = gesture.current_button();
            if button == gtk::gdk::BUTTON_SECONDARY || (button == gtk::gdk::BUTTON_PRIMARY && presses == 2) {
                reset();
            }
        }
    ));
    value.add_controller(on_value);
    value.set_tooltip_text(Some("Right-click to reset"));
    scale.set_tooltip_text(Some("Right-click to reset"));

    let wheel = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::VERTICAL);
    wheel.set_propagation_phase(gtk::PropagationPhase::Capture);
    wheel.connect_scroll(move |controller, _, dy| {

        let Some(scroller) = controller
            .widget()
            .and_then(|scale| scale.ancestor(gtk::ScrolledWindow::static_type()))
            .and_downcast::<gtk::ScrolledWindow>()
        else {
            return glib::Propagation::Proceed;
        };
        let adjustment = scroller.vadjustment();
        let step = adjustment.step_increment().max(24.0);
            let target = (adjustment.value() + dy * step)
                .clamp(0.0, (adjustment.upper() - adjustment.page_size()).max(0.0));
        adjustment.set_value(target);
        glib::Propagation::Stop
    });
    scale.add_controller(wheel);

    row.append(&header);
    row.append(scale);
    row
}

fn adjustments_changed(state: &App) {

    if state.applying.get() {
        return;
    }

    if state.show_coverage.replace(false) {
        state.mask_area.queue_draw();
    }

    sync_document(state);
    request_render(state);
    schedule_history_push(state);
    refresh_slider_marks(state);
}

fn sync_document(state: &App) {
    let mut open = state.open.borrow_mut();
    let Some(photo) = open.as_mut() else { return };

    if let Some(index) = state.selected_mask.get() {
        let mut masks = photo.document.masks();
        if let Some(mask) = masks.get_mut(index) {
            mask.basic = state.sliders.read();
            photo.document.set_masks(masks);
            photo.view = None;
            return;
        }
        state.selected_mask.set(None);
    }

    photo.document.set_basic(state.sliders.read());

    let balance = state.sliders.white_balance();
    photo.document.white_balance = (balance != photo.as_shot).then_some(balance);
}

fn request_render(state: &App) {

    state.drafting.set(true);
    settle_render(state);

    if state.render_pending.replace(true) {
        return;
    }

    let state = state.clone();
    state.canvas.clone().add_tick_callback(move |_, _| {
        state.render_pending.set(false);
        render_current(&state);
        glib::ControlFlow::Break
    });
}

fn settle_render(state: &App) {
    let generation = state.settle_generation.get().wrapping_add(1);
    state.settle_generation.set(generation);

    let state = state.clone();

    glib::timeout_add_local_once(std::time::Duration::from_millis(130), move || {
        if state.settle_generation.get() != generation || !state.drafting.get() {
            return;
        }
        state.drafting.set(false);
        if state.render_pending.replace(true) {
            return;
        }
        let state = state.clone();
        state.canvas.clone().add_tick_callback(move |_, _| {
            state.render_pending.set(false);
            render_current(&state);
            glib::ControlFlow::Break
        });
    });
}

fn colour_key(document: &Document) -> ColourKey {
    (document.white_balance, document.colour_profile.clone(), document.ai_denoise)
}

type ColourKey = (Option<WhiteBalance>, Option<String>, f32);

fn proxy_runs_out_at(photo: &OpenPhoto) -> f64 {
    let full = photo.full_size.0.max(photo.full_size.1) as f64;
    let proxy = photo.proxy.width.max(photo.proxy.height) as f64;
    if full <= 0.0 || proxy <= 0.0 {
        return 1.0;
    }
    proxy / full
}

fn covers(outer: [f32; 4], inner: [f32; 4]) -> bool {
    inner[0] >= outer[0] - 1e-4
        && inner[1] >= outer[1] - 1e-4
        && inner[0] + inner[2] <= outer[0] + outer[2] + 1e-4
        && inner[1] + inner[3] <= outer[1] + outer[3] + 1e-4
}

fn build_reference_pane(state: &App) -> gtk::Box {
    let pane = state.reference_pane.clone();
    pane.set_visible(false);
    pane.add_css_class("reference-pane");
    pane.set_hexpand(true);

    state.reference_picture.set_vexpand(true);
    state.reference_picture.set_hexpand(true);
    state.reference_picture.set_can_shrink(true);
    state.reference_picture.set_content_fit(gtk::ContentFit::Contain);
    pane.append(&state.reference_picture);

    state.reference_caption.add_css_class("reference-caption");
    state.reference_caption.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
    state.reference_caption.set_margin_bottom(6);
    state.reference_caption.set_halign(gtk::Align::Center);
    pane.append(&state.reference_caption);

    pane
}

fn set_reference(state: &App) {
    let rendered = {
        let mut open = state.open.borrow_mut();
        let Some(photo) = open.as_mut() else {
            state.reference_button.set_active(false);
            return;
        };
        let name = match &photo.source {
            Source::Photo { path, .. } => {
                path.file_name().map(|name| name.to_string_lossy().into_owned())
            }
            Source::Bracket { paths } => Some(format!("Merge of {} frames", paths.len())),
        };

        let key = colour_key(&photo.document);
        if key != photo.working_key {
            photo.working = render::to_working_space(&photo.document, &photo.proxy);
            photo.working_key = key;
            photo.draft = None;
        }
        let scale = photo.proxy.width.max(photo.proxy.height) as f32
            / photo.full_size.0.max(photo.full_size.1).max(1) as f32;
        let document = render::with_masks_resolved(&photo.document, &photo.working);
        (render::apply_stack(&document, &photo.working, scale), name)
    };
    let (frame, name) = rendered;

    state.reference_picture.set_paintable(Some(&texture_from(frame)));
    state.reference_caption.set_text(&match name {
        Some(name) => format!("Reference \u{00b7} {name}"),
        None => "Reference".to_string(),
    });
    state.reference_pane.set_visible(true);
    state.toast("This frame is the reference \u{2014} step to another to compare");
}

fn clear_reference(state: &App) {
    state.reference_pane.set_visible(false);
    state.reference_picture.set_paintable(gtk::gdk::Paintable::NONE);

    state.reference_button.set_active(false);
}

fn render_current(state: &App) {

    let zoom = state.zoom.get();
    let Some((frame_width, frame_height)) = displayed_size(state) else { return };
    let visible = visible_rect(state).unwrap_or([0.0, 0.0, 1.0, 1.0]);
    let wanted = tile_for(state);

    let mut open = state.open.borrow_mut();
    let Some(photo) = open.as_mut() else { return };

    let key = colour_key(&photo.document);
    if key != photo.working_key {
        photo.working = render::to_working_space(&photo.document, &photo.proxy);
        photo.working_key = key.clone();

        photo.draft = None;
    }

    let document = rendered_document(state, &photo.document);

    let wants_full = zoom > proxy_runs_out_at(photo);
    let have_full = photo.full_working.is_some() && photo.full_working_key.as_ref() == Some(&key);

    let region = render::tiles_cleanly(&document)
        .then_some(wanted)
        .flatten()

        .filter(|rect| {
            render::spots_within(&document, *rect, [frame_width as f32, frame_height as f32])
        })
        .unwrap_or([0.0, 0.0, 1.0, 1.0]);

    let needed = (region[2] as f64 * frame_width as f64 * zoom)
        .max(region[3] as f64 * frame_height as f64 * zoom)
        .ceil()
        .clamp(1.0, u32::MAX as f64) as u32;

    let geometry = geometry_of_document(&document);
    let reusable = photo.view.as_ref().is_some_and(|view| {
        view.key == key
            && view.geometry == geometry
            && view.edge >= needed
            && covers(view.rect, visible)
    });

    if wants_full && have_full && !reusable {
        if let Some(within) = render::tile_in_source(&document, region) {
            let full = photo.full_working.as_ref().expect("checked by have_full");
            let cut = full.cropped(within, 0.0, Default::default());
            let image = cut.downscaled(needed).unwrap_or(cut);
            photo.view = Some(ViewTile {
                rect: region,
                key: key.clone(),
                geometry,
                edge: needed,
                image,
            });
        } else {

            photo.view = None;
        }
    }

    let usable_view = wants_full
        && have_full
        && photo.view.as_ref().is_some_and(|view| {
            view.key == key && view.geometry == geometry && covers(view.rect, visible)
        });

    let proxy_scale = photo.proxy.width.max(photo.proxy.height) as f32
        / photo.full_size.0.max(photo.full_size.1).max(1) as f32;

    let (mut rendered, placement, whole_frame) = match (usable_view, &photo.view) {
        (true, Some(view)) => {
            let covered = (view.rect[2] * frame_width as f32).max(1.0);
            let detail_scale = view.image.width as f32 / covered;

            let rendered = render::apply_pixels(&document, &view.image, detail_scale, view.rect);
            let placement = crate::ui::pixel_paintable::Placement {
                frame: (frame_width as f64, frame_height as f64),
                tile: (
                    (view.rect[0] * frame_width as f32) as f64,
                    (view.rect[1] * frame_height as f32) as f64,
                    (view.rect[2] * frame_width as f32) as f64,
                    (view.rect[3] * frame_height as f32) as f64,
                ),
            };
            let whole = view.rect == [0.0, 0.0, 1.0, 1.0];
            (rendered, Some(placement), whole)
        }

        (_, _) if wants_full && have_full => {
            let full = photo.full_working.as_ref().expect("checked by have_full");
            (render::apply_stack(&document, full, 1.0), None, true)
        }

        _ => {

            if state.drafting.get() && photo.draft.is_none() {
                let half = photo.working.width.max(photo.working.height) / 2;
                photo.draft = photo.working.downscaled(half as u32);
            }
            let draft = state.drafting.get().then(|| photo.draft.as_ref()).flatten();
            let rendered = match draft {
                Some(small) => {
                    let scale = proxy_scale * small.width as f32
                        / photo.working.width.max(1) as f32;
                    render::apply_stack(&document, small, scale)
                }
                None => render::apply_stack(&document, &photo.working, proxy_scale),
            };
            (rendered, None, true)
        }
    };

    state.rendered_from_full.set(wants_full && have_full);
    state.tile.set((!whole_frame).then_some(
        photo.view.as_ref().map_or([0.0, 0.0, 1.0, 1.0], |view| view.rect),
    ));

    let backdrop = (!whole_frame)
        .then(|| render::apply_stack(&document, &photo.working, proxy_scale));

    let histogram = match &backdrop {
        Some(frame) => render::histogram::of(frame),
        None => render::histogram::of(&rendered),
    };

    state.rendered_size.set(rendered.dimensions());
    drop(open);

    write_zoom_label(state);

    if wants_full && !have_full {
        ensure_full_resolution(state);
    }

    state.shadow_clip.set_visible(histogram.is_shadow_clipped());
    state.highlight_clip.set_visible(histogram.is_highlight_clipped());
    *state.histogram.borrow_mut() = Some(histogram);
    state.histogram_area.queue_draw();

    if state.before.is_active() {
        return;
    }

    render::histogram::mark_clipping(
        &mut rendered,
        render::histogram::ClippingOverlay {
            shadows: state.shadow_clip.is_active(),
            highlights: state.highlight_clip.is_active(),
        },
    );

    show(state, rendered, placement, backdrop);

    refresh_info(state);
}

fn schedule_history_push(state: &App) {

    schedule_save(state);

    let generation = state.history_generation.get().wrapping_add(1);
    state.history_generation.set(generation);

    let state = state.clone();
    glib::timeout_add_local_once(std::time::Duration::from_millis(400), move || {
        if state.history_generation.get() != generation {
            return;
        }
        if let Some(photo) = state.open.borrow_mut().as_mut() {
            let snapshot = EditState::of(&photo.document);
            photo.history.push(snapshot);
        }
    });
}

fn step_history(state: &App, redo: bool) {
    let stepped = state.open.borrow_mut().as_mut().and_then(|photo| {

        let snapshot = EditState::of(&photo.document);
        photo.history.push(snapshot);
        let stepped = if redo { photo.history.redo() } else { photo.history.undo() };

        stepped.map(|state| (state, photo.as_shot))
    });

    let Some((edit, as_shot)) = stepped else {
        state.toast(if redo { "Nothing to redo" } else { "Nothing to undo" });
        return;
    };

    apply_history(state, edit, as_shot);
}

fn jump_history(state: &App, position: usize) {
    let stepped = state.open.borrow_mut().as_mut().and_then(|photo| {

        let snapshot = EditState::of(&photo.document);
        photo.history.push(snapshot);
        photo.history.go_to(position).map(|state| (state, photo.as_shot))
    });

    let Some((edit, as_shot)) = stepped else { return };
    apply_history(state, edit, as_shot);
}

fn apply_history(state: &App, edit: EditState, as_shot: WhiteBalance) {
    state.applying.set(true);
    state.sliders.write(edit.basic);
    state.sliders.write_white_balance(edit.white_balance.unwrap_or(as_shot));
    refresh_slider_marks(state);

    let (rect, angle) = edit.crop.unwrap_or(([0.0, 0.0, 1.0, 1.0], 0.0));
    state.crop_rect.set(rect);
    state.straighten.set_value(angle as f64);
    state.applying.set(false);

    {
        let mut open = state.open.borrow_mut();
        if let Some(photo) = open.as_mut() {
            let space_moved = photo.document.working_space != edit.working_space;
            edit.restore(&mut photo.document);
            if space_moved {
                photo.working = render::to_working_space(&photo.document, &photo.proxy);
                photo.full_working = None;
                photo.full_working_key = None;
                photo.draft = None;
            }
            photo.view = None;
        }
    }
    write_mixer(state);
    write_point_colours(state);
    write_grading(state);
    write_perspective(state);
    ai_denoise::write(state);
    refresh_retouch(state);
    refresh_face(state);
    refresh_found(state);
    refresh_masks(state);
    select_mask(state, None);
    refresh_profile_picker(state);
    state.crop_area.queue_draw();
    state.curve_area.queue_draw();

    sync_document(state);
    request_render(state);

    refresh_history(state);
}

fn copy_image(state: &App) {
    let job = state.open.borrow().as_ref().map(|photo| {
        let document = photo.document.clone();
        let working = photo.working.clone();
        let scale = working.width.max(working.height) as f32 / photo.full_size.0.max(photo.full_size.1).max(1) as f32;
        (document, working, scale)
    });
    let Some((document, working, scale)) = job else { return };
    let state = state.clone();
    glib::spawn_future_local(async move {
        let Ok(image) = busy(&state, "Copying the picture…", move || {
            let document = render::with_masks_resolved(&document, &working);
            render::apply_stack(&document, &working, scale)
        })
        .await
        else {
            return;
        };
        if let Some(display) = gtk::gdk::Display::default() {
            display.clipboard().set_texture(&texture_from(image));
            state.toast("Picture copied");
        }
    });
}

fn texture_from(image: image::RgbImage) -> gtk::gdk::Texture {
    let (width, height) = (image.width() as i32, image.height() as i32);
    gtk::gdk::MemoryTexture::new(
        width,
        height,
        gtk::gdk::MemoryFormat::R8g8b8,
        &glib::Bytes::from_owned(image.into_raw()),
        width as usize * 3,
    )
    .upcast()
}

fn show(
    state: &App,
    image: image::RgbImage,
    placement: Option<crate::ui::pixel_paintable::Placement>,
    backdrop: Option<image::RgbImage>,
) {
    let texture = texture_from(image);
    let paintable = match placement {
        Some(placement) => crate::ui::pixel_paintable::PixelPaintable::with_placement(
            texture,
            placement,
            backdrop.map(texture_from),
        ),
        None => crate::ui::pixel_paintable::PixelPaintable::new(texture),
    };
    state.canvas.set_paintable(Some(&paintable));
    apply_zoom(state);
}

fn set_zoom(state: &App, zoom: f64) {
    let was_full = needs_full_resolution(state, state.zoom.get());

    state.tile.set(None);
    state.zoom.set(zoom);
    apply_zoom(state);

    let wants_full = needs_full_resolution(state, zoom);
    if wants_full {

        let generation = state.zoom_generation.get().wrapping_add(1);
        state.zoom_generation.set(generation);

        let state = state.clone();
        glib::timeout_add_local_once(std::time::Duration::from_millis(250), move || {
            let still_wanted = needs_full_resolution(&state, state.zoom.get());
            if state.zoom_generation.get() == generation && still_wanted {
                ensure_full_resolution(&state);
            }
        });
    } else if was_full {

        if let Some(photo) = state.open.borrow_mut().as_mut() {
            photo.full_working = None;
            photo.full_working_key = None;
            photo.view = None;
        }
        request_render(state);
        refresh_info(state);
    }
}

fn ensure_full_resolution(state: &App) {
    let request = {
        let open = state.open.borrow();
        let Some(photo) = open.as_ref() else { return };

        let key = colour_key(&photo.document);
        if photo.full_working_key.as_ref() == Some(&key) {

            state.full_resolution_stale.set(false);
            return;
        }

        if state.failed_key.borrow().as_ref() == Some(&key) {
            return;
        }
        if state.loading_full.get() {

            state.full_resolution_stale.set(true);
            return;
        }

        let source = match &photo.source {
            Source::Photo { id, path } => Source::Photo { id: *id, path: path.clone() },
            Source::Bracket { paths } => Source::Bracket { paths: paths.clone() },
        };
        (source, photo.document.clone(), key)
    };

    let (source, document, key) = request;
    state.loading_full.set(true);
    state.full_resolution_stale.set(false);
    state.zoom_label.set_text("…");

    let generation = state.open_generation.get();
    let state = state.clone();
    glib::spawn_future_local(async move {
        let decoded = busy(&state, "Decoding the original…", move || {
            let full = source.full_resolution()?;
            Ok::<_, String>(render::to_working_space(&document, &full))
        })
        .await;

        state.loading_full.set(false);

        if state.open_generation.get() != generation {
            return;
        }

        if !needs_full_resolution(&state, state.zoom.get()) {
            state.full_resolution_stale.set(false);
            return;
        }

        match decoded {
            Ok(Ok(working)) => {
                if let Some(photo) = state.open.borrow_mut().as_mut() {
                    photo.full_working = Some(working);
                    photo.full_working_key = Some(key);
                }
                request_render(&state);
                refresh_info(&state);
            }
            Ok(Err(err)) => {
                *state.failed_key.borrow_mut() = Some(key);
                state.toast(&format!("Could not load full resolution: {err}"));
            }
            Err(_) => {}
        }

        apply_zoom(&state);

        if state.full_resolution_stale.get() {
            ensure_full_resolution(&state);
        }
    });
}

fn fit_scale(state: &App) -> f64 {
    let Some((width, height)) = displayed_size(state) else {
        return 1.0;
    };
    let (available_x, available_y) = (
        state.canvas_scroller.width() as f64,
        state.canvas_scroller.height() as f64,
    );

    if width == 0 || height == 0 || available_x <= 0.0 || available_y <= 0.0 {
        return 1.0;
    }

    (available_x / width as f64).min(available_y / height as f64) * device_scale(state)
}

fn device_scale(state: &App) -> f64 {
    state.canvas.scale_factor().max(1) as f64
}

fn scaled_zoom(state: &App, factor: f64) -> f64 {
    let floor = fit_scale(state);
    let wanted = effective_zoom(state).max(1e-6) * factor;

    if wanted <= floor * 1.01 {
        FIT_ZOOM
    } else {
        wanted.min(MAX_ZOOM)
    }
}

fn effective_zoom(state: &App) -> f64 {
    if state.zoom.get() == FIT_ZOOM {
        fit_scale(state)
    } else {
        state.zoom.get()
    }
}

fn zoom_about_centre(state: &App, zoom: f64) {
    let horizontal = state.canvas_scroller.hadjustment();
    let vertical = state.canvas_scroller.vadjustment();

    let before = (effective_zoom(state) / device_scale(state)).max(1e-6);

    let focus = [&horizontal, &vertical].map(|adjustment| {
        (adjustment.value() + adjustment.page_size() / 2.0) / before
    });

    set_zoom(state, zoom);

    if state.recentring.replace(true) {
        return;
    }

    let state = state.clone();
    glib::idle_add_local_once(move || {
        state.recentring.set(false);
        let after = effective_zoom(&state) / device_scale(&state);
        for (adjustment, centre) in [
            (state.canvas_scroller.hadjustment(), focus[0]),
            (state.canvas_scroller.vadjustment(), focus[1]),
        ] {
            adjustment.set_value(centre * after - adjustment.page_size() / 2.0);
        }
    });
}

fn apply_zoom(state: &App) {
    let zoom = state.zoom.get();

    if zoom == FIT_ZOOM {
        state.canvas.set_size_request(-1, -1);
        state.canvas.set_halign(gtk::Align::Fill);
        state.canvas.set_valign(gtk::Align::Fill);
        state.zoom_label.set_text(&format!("Fit {:.0}%", effective_zoom(state) * 100.0));
        return;
    }

    let Some((full_width, full_height)) = displayed_size(state) else {
        return;
    };
    let scale = device_scale(state);
    let width = (full_width as f64 * zoom / scale).round() as i32;
    let height = (full_height as f64 * zoom / scale).round() as i32;

    state.canvas.set_size_request(width.max(1), height.max(1));
    state.canvas.set_halign(gtk::Align::Center);
    state.canvas.set_valign(gtk::Align::Center);

    write_zoom_label(state);
}

fn write_zoom_label(state: &App) {
    let zoom = state.zoom.get();
    if zoom == FIT_ZOOM {
        state.zoom_label.set_text(&format!("Fit {:.0}%", effective_zoom(state) * 100.0));
        return;
    }
    let full_ready = state.rendered_from_full.get();
    state.zoom_label.set_text(&match (needs_full_resolution(state, zoom), full_ready) {
        (true, false) if state.loading_full.get() => "…".to_string(),
        (true, false) => format!("{:.0}% soft", zoom * 100.0),
        _ => format!("{:.0}%", zoom * 100.0),
    });
}

fn leave_crop(state: &App) {
    if is_cropping(state) {
        show_panel_tab(state, "light");
    }
}

fn is_cropping(state: &App) -> bool {
    state.panel_stack.visible_child_name().as_deref() == Some("crop")
}

fn toggle_crop(state: &App, active: bool) {
    if active {
        state.crop_at_open.set(geometry_now(state));
        let rect = state
            .open
            .borrow()
            .as_ref()
            .and_then(|photo| photo.document.crop())
            .map_or([0.0, 0.0, 1.0, 1.0], |(rect, _)| rect);
        state.crop_rect.set(rect);

        set_zoom(state, FIT_ZOOM);
        state.crop_area.set_visible(true);
        state.crop_area.queue_draw();
    } else {
        state.crop_area.set_visible(false);

        state.guided.set_active(false);

        if geometry_now(state) != state.crop_at_open.get() {
            remake_masks(state);
        }
    }

    set_panel_scope(state);
    commit_crop(state);
}

fn geometry_now(state: &App) -> Option<([f32; 4], f32, f32)> {
    let open = state.open.borrow();
    let photo = open.as_ref()?;
    let (rect, angle) = photo.document.crop().unwrap_or(([0.0, 0.0, 1.0, 1.0], 0.0));
    Some((rect, angle, photo.document.rotation()))
}

fn remake_masks(state: &App) {
    let wanted = {
        let open = state.open.borrow();
        let Some(photo) = open.as_ref() else { return };
        photo.document.masks().iter().any(Mask::wants_pixels)
    };
    if !wanted {
        return;
    }

    {
        let mut open = state.open.borrow_mut();
        let Some(photo) = open.as_mut() else { return };
        photo.segmentation = None;
        photo.mask_frame = None;
        photo.segmenting = false;
        let mut masks = photo.document.masks();
        for mask in masks.iter_mut() {
            mask.map = Pixels(None);
        }
        photo.document.set_masks(masks);
        photo.view = None;
    }

    fill_segment_masks(state);
    if state
        .open
        .borrow()
        .as_ref()
        .is_some_and(|photo| photo.document.masks().iter().any(Mask::is_pending))
    {
        ensure_segmentation(state);
    }

    ensure_faces(state);
    request_render(state);
    state.mask_area.queue_draw();
}

fn auto_perspective(state: &App) {
    let measured = {
        let open = state.open.borrow();
        let Some(photo) = open.as_ref() else { return };

        let mut framing = Document::new(photo.document.source.path.clone());
        framing.set_rotation(photo.document.rotation());
        framing.set_mirrored(photo.document.mirrored());
        if let Some((rect, angle)) = photo.document.crop() {
            framing.set_crop(rect, angle);
        }
        let framed = render::geometry_only(&framing, &photo.working);

        let small = framed.downscaled(AUTO_EDGE).unwrap_or(framed);
        let luma = numa::core::plane::Plane::new(
            small.width as usize,
            small.height as usize,
            small
                .data
                .chunks_exact(3)
                .map(|pixel| 0.2126 * pixel[0] + 0.7152 * pixel[1] + 0.0722 * pixel[2])
                .collect(),
        );
        let (mut perspective, angle) = render::auto::perspective(&luma);

        if let Some(([_, _, width, height], _)) = photo.document.crop() {
            perspective.vertical /= height;
            perspective.horizontal /= width;
        }
        (perspective, angle)
    };

    let (perspective, angle) = measured;
    if perspective.is_identity() && angle == 0.0 {
        state.toast("No straight lines to square up to");
        return;
    }

    state.applying.set(true);
    for (slider, value) in state.perspective_sliders.iter().zip([
        perspective.vertical,
        perspective.horizontal,
        perspective.aspect,
    ]) {
        slider.set_value(value as f64);
    }

    if angle != 0.0 {
        let straightened = (state.straighten.value() + angle as f64).clamp(-15.0, 15.0);
        state.straighten.set_value(straightened);
    }
    state.applying.set(false);

    apply_perspective(state, perspective);
    state.toast("Squared up — every slider is yours to change");
}

const AUTO_EDGE: u32 = 900;

fn auto_tone(state: &App) {

    let subject = subject_alpha(state);

    let measured = {
        let open = state.open.borrow();
        let Some(photo) = open.as_ref() else { return };
        render::auto::tone(&photo.working, subject.as_deref())
    };

    let (basic, added) = {
        let mut open = state.open.borrow_mut();
        let Some(photo) = open.as_mut() else { return };
        let added = measured.apply(&mut photo.document);
        photo.view = None;
        (photo.document.basic(), added)
    };

    match added {

        Some(index) => {
            fill_segment_masks(state);
            ensure_segmentation(state);
            refresh_masks(state);
            select_mask(state, Some(index));
        }

        None => {
            state.applying.set(true);
            state.sliders.write(basic);
            state.applying.set(false);
            refresh_slider_marks(state);
        }
    }

    adjustments_changed(state);
    request_render(state);
    schedule_history_push(state);
    state.toast(match added {
        Some(_) => "The subject has its own exposure — every slider is yours to change",
        None => "Exposure and the endpoints set — every slider is yours to change",
    });
}

fn read_perspective(state: &App) {
    apply_perspective(
        state,
        Perspective {
            vertical: state.perspective_sliders[0].value() as f32,
            horizontal: state.perspective_sliders[1].value() as f32,
            aspect: state.perspective_sliders[2].value() as f32,
        },
    );
}

fn flip_frame(state: &App, vertical: bool) {
    {
        let mut open = state.open.borrow_mut();
        let Some(photo) = open.as_mut() else { return };
        let document = &mut photo.document;
        let quarter = (document.rotation() / 90.0).round() as i32 % 2 != 0;

        if vertical != quarter {
            document.set_rotation(document.rotation() + 180.0);
        }
        document.set_mirrored(!document.mirrored());

        let mut perspective = document.perspective();
        if vertical {
            perspective.vertical = -perspective.vertical;
        } else {
            perspective.horizontal = -perspective.horizontal;
        }
        document.set_perspective(perspective);
        photo.view = None;
    }
    let [x, y, w, h] = state.crop_rect.get();
    state.crop_rect.set(if vertical { [x, 1.0 - y - h, w, h] } else { [1.0 - x - w, y, w, h] });
    state.straighten.set_value(-state.straighten.value());
    write_perspective(state);
    state.crop_area.queue_draw();
    commit_crop(state);
}

fn apply_perspective(state: &App, perspective: Perspective) {
    {
        let mut open = state.open.borrow_mut();
        let Some(photo) = open.as_mut() else { return };
        photo.document.set_perspective(perspective);

        photo.view = None;
    }
    let (width, height) = frame_pixels(state);
    state.crop_rect.set(fitted_crop(
        state.crop_rect.get(),
        state.straighten.value() as f32,
        perspective,
        width,
        height,
    ));
    state.crop_area.queue_draw();
    commit_crop(state);
}

fn write_perspective(state: &App) {
    let perspective = state
        .open
        .borrow()
        .as_ref()
        .map(|photo| photo.document.perspective())
        .unwrap_or_default();

    state.applying.set(true);
    for (slider, value) in state.perspective_sliders.iter().zip([
        perspective.vertical,
        perspective.horizontal,
        perspective.aspect,
    ]) {
        slider.set_value(value as f64);
    }
    state.applying.set(false);
}

fn commit_crop(state: &App) {
    {
        let mut open = state.open.borrow_mut();
        let Some(photo) = open.as_mut() else { return };
        photo
            .document
            .set_crop(state.crop_rect.get(), state.straighten.value() as f32);
    }

    request_render(state);
    schedule_history_push(state);

    if needs_full_resolution(state, state.zoom.get()) {
        ensure_full_resolution(state);
    }
}

fn build_face_names_overlay(state: &App) -> gtk::DrawingArea {
    let area = state.face_names_area.clone();
    area.set_can_target(false);
    area.set_visible(false);
    state.canvas.connect_paintable_notify(glib::clone!(
        #[weak] area,
        move |_| area.queue_draw()
    ));
    area.set_draw_func(glib::clone!(
        #[strong] state,
        move |_, context, width, height| {
            let (x, y, w, h) = content_rect(&state, width as f64, height as f64);
            context.select_font_face(
                "Sans",
                gtk::cairo::FontSlant::Normal,
                gtk::cairo::FontWeight::Normal,
            );
            context.set_font_size(13.0);
            for (at, name) in state.face_names.borrow().iter() {
                let (face_x, face_y) = (x + w * at[0] as f64, y + h * at[1] as f64);
                let (face_w, face_h) = (w * at[2] as f64, h * at[3] as f64);

                context.set_source_rgba(1.0, 1.0, 1.0, 0.7);
                context.set_line_width(1.0);
                context.rectangle(face_x.round() + 0.5, face_y.round() + 0.5, face_w.round(), face_h.round());
                let _ = context.stroke();

                let Ok(extents) = context.text_extents(name) else { continue };
                let pad = 5.0;
                let (label_w, label_h) = (extents.width() + pad * 2.0, 20.0);

                let label_x = (face_x + face_w / 2.0 - label_w / 2.0).clamp(x, x + w - label_w);
                let label_y = (face_y + face_h + 4.0).min(y + h - label_h);
                context.set_source_rgba(0.0, 0.0, 0.0, 0.55);
                context.rectangle(label_x, label_y, label_w, label_h);
                let _ = context.fill();
                context.set_source_rgba(1.0, 1.0, 1.0, 0.95);
                context.move_to(label_x + pad, label_y + label_h - 6.0);
                let _ = context.show_text(name);
            }
        }
    ));
    area
}

fn refresh_face_names(state: &App) {
    if !state.show_face_names.get() {
        return;
    }
    let open = state.open.borrow();
    let Some(photo) = open.as_ref() else { return };
    let Source::Photo { id, .. } = photo.source else { return };

    let named = state.catalog.named_faces().unwrap_or_else(|err| {
        log::warn!("could not read the named faces: {err}");
        Vec::new()
    });
    let elsewhere: Vec<(String, [f32; cull::people::LENGTH])> =
        named.iter().map(|(_, name, embedding)| (name.clone(), *embedding)).collect();

    let names = photo
        .people
        .iter()
        .filter_map(|seen| {
            let here = named
                .iter()
                .filter(|(photo_id, _, _)| *photo_id == id)
                .map(|(_, name, embedding)| {
                    (name, cull::people::likeness(embedding, &seen.embedding))
                })
                .filter(|(_, alike)| *alike >= 0.8)
                .max_by(|a, b| a.1.total_cmp(&b.1))
                .map(|(name, _)| name.clone());
            let name = match here {
                Some(name) => name,
                None => format!("{}?", cull::people::recognise(&seen.embedding, &elsewhere)?.0),
            };

            (!name.trim_end_matches('?').is_empty()).then_some((seen.at, name))
        })
        .collect();

    *state.face_names.borrow_mut() = names;
    state.face_names_area.queue_draw();
}

fn show_face_names(state: &App, on: bool) {
    state.show_face_names.set(on);
    state.face_names_area.set_visible(on);
    if on {
        refresh_face_names(state);
    } else {
        state.face_names.borrow_mut().clear();
    }
    state.face_names_area.queue_draw();
}

fn build_guides_overlay(state: &App) -> gtk::DrawingArea {
    let area = state.guides_area.clone();
    area.set_can_target(false);
    area.set_visible(false);

    state.canvas.connect_paintable_notify(glib::clone!(
        #[weak] area,
        move |_| area.queue_draw()
    ));
    area.set_draw_func(glib::clone!(
        #[strong] state,
        move |_, context, width, height| {
            let (x, y, w, h) = content_rect(&state, width as f64, height as f64);
            let divisions = match state.guides.get() {
                1 => 3,
                2 => 8,
                _ => return,
            };

            for (colour, line) in [((0.0, 0.0, 0.0, 0.45), 2.0), ((1.0, 1.0, 1.0, 0.7), 1.0)] {
                context.set_source_rgba(colour.0, colour.1, colour.2, colour.3);
                context.set_line_width(line);
                for step in 1..divisions {
                    let fraction = step as f64 / divisions as f64;
                    context.move_to((x + w * fraction).round() + 0.5, y);
                    context.line_to((x + w * fraction).round() + 0.5, y + h);
                    context.move_to(x, (y + h * fraction).round() + 0.5);
                    context.line_to(x + w, (y + h * fraction).round() + 0.5);
                }
                let _ = context.stroke();
            }
        }
    ));
    area
}

fn cycle_guides(state: &App) {
    let next = (state.guides.get() + 1) % 3;
    state.guides.set(next);
    state.guides_area.set_visible(next != 0);
    state.guides_area.queue_draw();
    state.guides_button.set_tooltip_text(Some(match next {
        1 => "Guides: thirds (G)",
        2 => "Guides: grid (G)",
        _ => "Guides: none (G)",
    }));
}

fn build_crop_overlay(state: &App) -> gtk::DrawingArea {
    let area = state.crop_area.clone();
    area.set_visible(false);
    area.set_can_target(true);

    area.set_draw_func(glib::clone!(
        #[strong] state,
        move |_, context, width, height| {
            let content = content_rect(&state, width as f64, height as f64);
            draw_crop(context, width as f64, height as f64, content, state.crop_rect.get());
            draw_guide_lines(&state, context);
        }
    ));

    let grabbed: Rc<Cell<Option<(usize, [f32; 4], f64, f64)>>> = Rc::new(Cell::new(None));

    let guide_grab: GuideGrab = Rc::new(Cell::new(None));

    let drag = gtk::GestureDrag::new();
    drag.connect_drag_begin(glib::clone!(
        #[strong] state,
        #[strong] grabbed,
        #[strong] guide_grab,
        move |gesture, x, y| {
            if state.guided.is_active() {
                gesture.set_state(gtk::EventSequenceState::Claimed);
                guide_drag_begin(&state, &guide_grab, x, y);
                return;
            }
            let (ox, oy, width, height) = content_rect(
                &state,
                state.crop_area.width() as f64,
                state.crop_area.height() as f64,
            );
            if width <= 0.0 || height <= 0.0 {
                return;
            }

            gesture.set_state(gtk::EventSequenceState::Claimed);
            grabbed.set(Some((
                nearest_handle(state.crop_rect.get(), (x - ox) / width, (y - oy) / height),
                state.crop_rect.get(),
                x,
                y,
            )));
        }
    ));
    drag.connect_drag_update(glib::clone!(
        #[strong] state,
        #[strong] grabbed,
        #[strong] guide_grab,
        move |_, dx, dy| {
            if guide_grab.get().is_some() {
                guide_drag_update(&state, &guide_grab, dx, dy);
                return;
            }
            let Some((handle, start, _, _)) = grabbed.get() else { return };
            let (_, _, width, height) = content_rect(
                &state,
                state.crop_area.width() as f64,
                state.crop_area.height() as f64,
            );
            if width <= 0.0 || height <= 0.0 {
                return;
            }

            let mut moved = move_handle(
                start,
                handle,
                (dx / width) as f32,
                (dy / height) as f32,
            );

            if let Some(ratio) = state.crop_ratio.get() {
                if handle < 4 {
                    moved = hold_aspect(moved, handle, ratio / frame_aspect(&state));
                }
            }
            state.crop_rect.set(moved);
            state.crop_area.queue_draw();
        }
    ));
    drag.connect_drag_end(glib::clone!(
        #[strong] state,
        #[strong] grabbed,
        #[strong] guide_grab,
        move |_, _, _| {
            if state.guided.is_active() {
                guide_drag_end(&state, &guide_grab);
                return;
            }
            grabbed.set(None);

            commit_crop(&state);
        }
    ));
    area.add_controller(drag);

    area
}

type GuideGrab = Rc<Cell<Option<(usize, usize, f64, f64)>>>;

const MOST_GUIDES: usize = 4;

const GUIDE_REACH: f64 = 12.0;

fn guided_row(state: &App, auto: &gtk::Button) -> gtk::Box {
    let row = gtk::Box::new(gtk::Orientation::Horizontal, 6);
    row.set_margin_top(4);
    auto.set_margin_top(0);
    auto.set_hexpand(true);
    row.append(auto);

    let guided = state.guided.clone();
    guided.set_hexpand(true);
    guided.set_tooltip_text(Some("Draw up to four lines along what should be upright or level"));
    let clear = gtk::Button::with_label("Clear");
    clear.set_hexpand(true);
    clear.set_tooltip_text(Some("Remove the guides"));
    clear.set_sensitive(false);
    guided.connect_toggled(glib::clone!(
        #[strong] state,
        #[weak] clear,
        move |button| {
            clear.set_sensitive(button.is_active());
            if !button.is_active() {
                state.guide_lines.borrow_mut().clear();
            }
            state.crop_area.queue_draw();
        }
    ));
    clear.connect_clicked(glib::clone!(
        #[strong] state,
        move |_| {
            state.guide_lines.borrow_mut().clear();
            state.crop_area.queue_draw();
        }
    ));
    row.append(&guided);
    row.append(&clear);
    row
}

fn guide_frame(state: &App) -> (f32, f32, f32, Perspective) {
    let (width, height) = frame_pixels(state);
    let perspective = state.open.borrow().as_ref().map(|photo| photo.document.perspective()).unwrap_or_default();
    (width, height, state.straighten.value() as f32, perspective)
}

fn guide_to_widget(state: &App, point: [f32; 2]) -> Option<(f64, f64)> {
    let (ox, oy, shown_width, shown_height) =
        content_rect(state, state.crop_area.width() as f64, state.crop_area.height() as f64);
    let (width, height, angle, perspective) = guide_frame(state);
    let [x, y] = numa::core::guided::to_frame(point, width, height, angle, perspective)?;
    Some((ox + x as f64 * shown_width, oy + y as f64 * shown_height))
}

fn widget_to_guide(state: &App, x: f64, y: f64) -> Option<[f32; 2]> {
    let (ox, oy, shown_width, shown_height) =
        content_rect(state, state.crop_area.width() as f64, state.crop_area.height() as f64);
    if shown_width <= 0.0 || shown_height <= 0.0 {
        return None;
    }
    let fraction = [((x - ox) / shown_width).clamp(0.0, 1.0) as f32, ((y - oy) / shown_height).clamp(0.0, 1.0) as f32];
    let (width, height, angle, perspective) = guide_frame(state);
    Some(numa::core::guided::to_source(fraction, width, height, angle, perspective))
}

fn guide_drag_begin(state: &App, grab: &GuideGrab, x: f64, y: f64) {
    let ends: Vec<_> = state
        .guide_lines
        .borrow()
        .iter()
        .enumerate()
        .flat_map(|(line, ends)| [(line, 0, ends[0]), (line, 1, ends[1])])
        .collect();
    let nearest = ends
        .into_iter()
        .filter_map(|(line, end, point)| {
            let (px, py) = guide_to_widget(state, point)?;
            Some((line, end, (px - x).hypot(py - y)))
        })
        .filter(|(_, _, distance)| *distance < GUIDE_REACH)
        .min_by(|a, b| a.2.total_cmp(&b.2));
    if let Some((line, end, _)) = nearest {
        grab.set(Some((line, end, x, y)));
        return;
    }

    if state.guide_lines.borrow().len() >= MOST_GUIDES {
        state.toast("Four guides at most — move one, or Clear");
        return;
    }
    let Some(point) = widget_to_guide(state, x, y) else { return };
    let mut lines = state.guide_lines.borrow_mut();
    lines.push([point, point]);
    grab.set(Some((lines.len() - 1, 1, x, y)));
}

fn guide_drag_update(state: &App, grab: &GuideGrab, dx: f64, dy: f64) {
    let Some((line, end, x, y)) = grab.get() else { return };
    let Some(point) = widget_to_guide(state, x + dx, y + dy) else { return };
    if let Some(ends) = state.guide_lines.borrow_mut().get_mut(line) {
        ends[end] = point;
    }
    state.crop_area.queue_draw();
}

fn guide_drag_end(state: &App, grab: &GuideGrab) {
    if grab.take().is_none() {
        return;
    }
    let drawn = state.guide_lines.borrow().clone();
    let lines: Vec<_> = drawn
        .into_iter()
        .filter(|[a, b]| match (guide_to_widget(state, *a), guide_to_widget(state, *b)) {
            (Some(a), Some(b)) => (a.0 - b.0).hypot(a.1 - b.1) >= GUIDE_REACH,
            _ => false,
        })
        .collect();
    *state.guide_lines.borrow_mut() = lines.clone();
    state.crop_area.queue_draw();
    if lines.is_empty() {
        return;
    }

    let (width, height, angle, perspective) = guide_frame(state);
    let (perspective, angle) = numa::core::guided::solve(&lines, width, height, angle, perspective);
    state.applying.set(true);
    state.straighten.set_value(angle as f64);
    state.applying.set(false);
    apply_perspective(state, perspective);
    write_perspective(state);
}

fn draw_guide_lines(state: &App, context: &gtk::cairo::Context) {
    if !state.guided.is_active() {
        return;
    }
    let lines = state.guide_lines.borrow().clone();
    let ends: Vec<_> = lines
        .iter()
        .filter_map(|[a, b]| Some((guide_to_widget(state, *a)?, guide_to_widget(state, *b)?)))
        .collect();
    for (colour, line, radius) in [((0.0, 0.0, 0.0, 0.6), 3.0, 5.5), ((1.0, 1.0, 1.0, 0.95), 1.5, 4.0)] {
        context.set_source_rgba(colour.0, colour.1, colour.2, colour.3);
        context.set_line_width(line);
        for ((ax, ay), (bx, by)) in &ends {
            context.move_to(*ax, *ay);
            context.line_to(*bx, *by);
        }
        let _ = context.stroke();
        for ((ax, ay), (bx, by)) in &ends {
            for (x, y) in [(ax, ay), (bx, by)] {
                context.new_sub_path();
                context.arc(*x, *y, radius, 0.0, std::f64::consts::TAU);
            }
        }
        let _ = context.fill();
    }
}

fn content_rect(state: &App, width: f64, height: f64) -> (f64, f64, f64, f64) {
    let Some(paintable) = state.canvas.paintable() else {
        return (0.0, 0.0, width, height);
    };
    let (image_width, image_height) = (
        paintable.intrinsic_width() as f64,
        paintable.intrinsic_height() as f64,
    );
    if image_width <= 0.0 || image_height <= 0.0 || width <= 0.0 || height <= 0.0 {
        return (0.0, 0.0, width, height);
    }

    let scale = (width / image_width).min(height / image_height);
    let shown_width = image_width * scale;
    let shown_height = image_height * scale;

    (
        (width - shown_width) / 2.0,
        (height - shown_height) / 2.0,
        shown_width,
        shown_height,
    )
}

fn nearest_handle(rect: [f32; 4], x: f64, y: f64) -> usize {
    let [rx, ry, rw, rh] = rect.map(f64::from);
    let corners = [
        (rx, ry),
        (rx + rw, ry),
        (rx, ry + rh),
        (rx + rw, ry + rh),
    ];

    const GRAB: f64 = 0.05;

    corners
        .iter()
        .enumerate()
        .map(|(index, (cx, cy))| (index, (x - cx).hypot(y - cy)))
        .filter(|(_, distance)| *distance < GRAB)
        .min_by(|a, b| a.1.total_cmp(&b.1))
        .map_or(4, |(index, _)| index)
}

fn move_handle(rect: [f32; 4], handle: usize, dx: f32, dy: f32) -> [f32; 4] {
    const MIN: f32 = 0.05;
    let [x, y, w, h] = rect;

    let (mut left, mut top, mut right, mut bottom) = (x, y, x + w, y + h);

    match handle {
        0 => { left += dx; top += dy; }
        1 => { right += dx; top += dy; }
        2 => { left += dx; bottom += dy; }
        3 => { right += dx; bottom += dy; }
        _ => {

            let dx = dx.clamp(-left, 1.0 - right);
            let dy = dy.clamp(-top, 1.0 - bottom);
            return [left + dx, top + dy, w, h];
        }
    }

    left = left.clamp(0.0, 1.0);
    top = top.clamp(0.0, 1.0);
    right = right.clamp(0.0, 1.0);
    bottom = bottom.clamp(0.0, 1.0);

    if right - left < MIN {
        if handle == 0 || handle == 2 { left = right - MIN; } else { right = left + MIN; }
    }
    if bottom - top < MIN {
        if handle == 0 || handle == 1 { top = bottom - MIN; } else { bottom = top + MIN; }
    }

    [
        left.clamp(0.0, 1.0 - MIN),
        top.clamp(0.0, 1.0 - MIN),
        (right - left).clamp(MIN, 1.0),
        (bottom - top).clamp(MIN, 1.0),
    ]
}

fn draw_crop(
    context: &gtk::cairo::Context,
    width: f64,
    height: f64,
    content: (f64, f64, f64, f64),
    rect: [f32; 4],
) {
    let (ox, oy, content_width, content_height) = content;
    let [x, y, w, h] = rect.map(f64::from);
    let left = ox + x * content_width;
    let top = oy + y * content_height;
    let right = ox + (x + w) * content_width;
    let bottom = oy + (y + h) * content_height;

    context.set_source_rgba(0.0, 0.0, 0.0, 0.55);
    context.rectangle(0.0, 0.0, width, height);
    context.rectangle(left, top, right - left, bottom - top);
    context.set_fill_rule(gtk::cairo::FillRule::EvenOdd);
    let _ = context.fill();
    context.set_fill_rule(gtk::cairo::FillRule::Winding);

    context.set_source_rgba(1.0, 1.0, 1.0, 0.25);
    context.set_line_width(1.0);
    for third in 1..3 {
        let fraction = third as f64 / 3.0;
        context.move_to(left + (right - left) * fraction, top);
        context.line_to(left + (right - left) * fraction, bottom);
        context.move_to(left, top + (bottom - top) * fraction);
        context.line_to(right, top + (bottom - top) * fraction);
    }
    let _ = context.stroke();

    context.set_source_rgba(1.0, 1.0, 1.0, 0.9);
    context.set_line_width(1.5);
    context.rectangle(left, top, right - left, bottom - top);
    let _ = context.stroke();

    for (cx, cy) in [(left, top), (right, top), (left, bottom), (right, bottom)] {
        context.rectangle(cx - 6.0, cy - 6.0, 12.0, 12.0);
    }
    let _ = context.fill();
}

fn show_baseline(state: &App) {
    let texture = {
        let mut open = state.open.borrow_mut();
        let Some(photo) = open.as_mut() else { return };

        if photo.baseline.is_none() {
            let original = Document::new(photo.document.source.path.clone());
            let working = render::to_working_space(&original, &photo.proxy);
            let proxy_scale = photo.proxy.width.max(photo.proxy.height) as f32
                / photo.full_size.0.max(photo.full_size.1).max(1) as f32;
            photo.baseline = Some(

                crate::ui::pixel_paintable::PixelPaintable::new(texture_from(render::apply_stack(
                    &original,
                    &working,
                    proxy_scale,
                )))
                .upcast(),
            );
        }
        photo.baseline.clone()
    };

    if let Some(paintable) = texture {
        state.canvas.set_paintable(Some(&paintable));
        apply_zoom(state);
    }
}

fn build_histogram(state: &App) -> gtk::Box {
    let column = gtk::Box::new(gtk::Orientation::Vertical, 4);
    column.set_margin_bottom(4);

    let area = state.histogram_area.clone();
    area.set_content_height(84);
    area.set_hexpand(true);
    area.add_css_class("histogram");

    let histogram = state.histogram.clone();
    area.set_draw_func(move |_, context, width, height| {
        draw_histogram(context, width as f64, height as f64, histogram.borrow().as_ref());
    });

    let stacked = gtk::Overlay::new();
    stacked.set_child(Some(&area));
    for (button, tooltip, class, side) in [
        (&state.shadow_clip, "Show clipped shadows", "clip-shadow", gtk::Align::Start),
        (&state.highlight_clip, "Show clipped highlights", "clip-highlight", gtk::Align::End),
    ] {
        button.set_tooltip_text(Some(tooltip));
        button.add_css_class("clip-light");
        button.add_css_class(class);
        button.set_has_frame(false);
        button.set_halign(side);
        button.set_valign(gtk::Align::End);
        button.set_margin_start(4);
        button.set_margin_end(4);
        button.set_margin_bottom(4);
        button.connect_toggled(glib::clone!(
            #[strong] state,
            move |_| request_render(&state)
        ));
        stacked.add_overlay(button);
    }

    column.append(&stacked);
    column
}

fn draw_histogram(
    context: &gtk::cairo::Context,
    width: f64,
    height: f64,
    histogram: Option<&render::histogram::Histogram>,
) {
    let Some(histogram) = histogram else { return };

    let scale = histogram.scale() as f64;
    let step = width / render::histogram::BINS as f64;

    context.set_operator(gtk::cairo::Operator::Add);

    for (index, colour) in [(0, (0.85, 0.2, 0.2)), (1, (0.2, 0.8, 0.3)), (2, (0.25, 0.45, 0.95))] {
        context.set_source_rgba(colour.0, colour.1, colour.2, 0.62);
        context.move_to(0.0, height);

        for (bin, count) in histogram.channels[index].iter().enumerate() {

            let value = (*count as f64 / scale).min(1.0);
            context.line_to(bin as f64 * step, height - value * height);
        }

        context.line_to(width, height);
        context.close_path();
        let _ = context.fill();
    }
}

fn build_info(state: &App) -> gtk::Box {
    let column = state.info.clone();
    column.set_orientation(gtk::Orientation::Vertical);
    column.set_spacing(18);
    column.set_margin_top(6);
    column
}

fn fact_row(title: &str, value: &str) -> adw::ActionRow {
    let row = adw::ActionRow::new();
    row.set_title(title);
    row.set_title_lines(1);

    let answer = gtk::Label::new(Some(value));
    answer.set_xalign(1.0);
    answer.add_css_class("dim-label");
    answer.set_ellipsize(gtk::pango::EllipsizeMode::Middle);
    answer.set_max_width_chars(22);
    answer.set_tooltip_text(Some(value));
    row.add_suffix(&answer);
    row
}

fn people_group(state: &App, photo: &OpenPhoto) -> Option<adw::PreferencesGroup> {
    let Source::Photo { id, .. } = photo.source else { return None };
    if photo.people.is_empty() {
        return None;
    }
    let named = state.catalog.named_faces().unwrap_or_else(|err| {
        log::warn!("could not read the named faces: {err}");
        Vec::new()
    });
    let elsewhere: Vec<(String, [f32; cull::people::LENGTH])> =
        named.iter().map(|(_, name, embedding)| (name.clone(), *embedding)).collect();

    let group = adw::PreferencesGroup::new();
    group.set_title("People");
    for seen in &photo.people {

        let here = named
            .iter()
            .filter(|(photo_id, _, _)| *photo_id == id)
            .map(|(_, name, embedding)| (name, cull::people::likeness(embedding, &seen.embedding)))
            .filter(|(_, alike)| *alike >= 0.8)
            .max_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(name, _)| name.clone());
        let guess = cull::people::recognise(&seen.embedding, &elsewhere);

        let row = adw::EntryRow::new();
        match (&here, guess) {
            (Some(name), _) => {
                row.set_title("Named");
                row.set_text(name);
            }
            (None, Some((name, _))) => {
                row.set_title("Looks like — Enter to confirm");
                row.set_text(name);
            }
            (None, None) => row.set_title("Who is this?"),
        }
        row.set_show_apply_button(true);

        let picture = gtk::Image::from_paintable(Some(&texture_from(seen.portrait.clone())));
        picture.set_pixel_size(40);
        picture.set_valign(gtk::Align::Center);
        picture.add_css_class("face-portrait");
        row.add_prefix(&picture);

        let embedding = seen.embedding;
        let name = move |state: &App, row: &adw::EntryRow| {
            let text = row.text().to_string();
            if let Err(err) = state.catalog.name_face(id, &embedding, &text) {
                log::warn!("could not name a face: {err}");
                state.toast("The name could not be saved");
                return;
            }

            let state = state.clone();
            glib::idle_add_local_once(move || {
                refresh_info(&state);
                refresh_face_names(&state);
            });
        };
        row.connect_apply(glib::clone!(
            #[strong] state,
            move |row| name(&state, row)
        ));

        row.connect_entry_activated(glib::clone!(
            #[strong] state,
            move |row| name(&state, row)
        ));
        group.add(&row);
    }

    let on_canvas = adw::SwitchRow::new();
    on_canvas.set_title("Show the names on the photograph");
    on_canvas.set_active(state.show_face_names.get());
    on_canvas.connect_active_notify(glib::clone!(
        #[strong] state,
        move |row| show_face_names(&state, row.is_active())
    ));
    group.add(&on_canvas);

    Some(group)
}

fn refresh_info(state: &App) {
    while let Some(child) = state.info.first_child() {
        state.info.remove(&child);
    }

    let open = state.open.borrow();
    let Some(photo) = open.as_ref() else { return };

    if let Some(people) = people_group(state, photo) {
        state.info.append(&people);
    }

    if let Some(summary) = &photo.summary {
        let body = adw::PreferencesGroup::new();
        body.set_title("Camera");
        if let Some(camera) = &summary.camera {
            body.add(&fact_row("Body", camera));
        }
        if let Some(lens) = &summary.lens {
            body.add(&fact_row("Lens", lens));
        }
        if let Some(mode) = &summary.film_mode {

            body.add(&fact_row("Film mode", mode));
        }
        state.info.append(&body);

        let shot = adw::PreferencesGroup::new();
        shot.set_title("Exposure");
        if let Some(focal) = summary.focal_length {
            shot.add(&fact_row("Focal length", &format!("{focal:.0} mm")));
        }
        if let Some(aperture) = summary.aperture {
            shot.add(&fact_row("Aperture", &format!("f/{aperture:.1}")));
        }
        if let Some(shutter) = summary.shutter_text() {
            shot.add(&fact_row("Shutter", &shutter));
        }
        if let Some(iso) = summary.iso {
            shot.add(&fact_row("ISO", &iso.to_string()));
        }
        match summary.exposure_bias {
            Some(bias) if bias != 0.0 => {
                shot.add(&fact_row("Compensation", &format!("{bias:+.1} EV")))
            }
            _ => {}
        }
        if let Some(taken) = &summary.taken {
            shot.add(&fact_row("Taken", taken));
        }
        state.info.append(&shot);

        let frame = adw::PreferencesGroup::new();
        frame.set_title("Frame");
        frame.add(&fact_row(
            "Size",
            &format!("{} × {}", summary.sensor.0, summary.sensor.1),
        ));
        frame.add(&fact_row("Resolution", &format!("{:.1} MP", summary.megapixels())));
        if let Some(bytes) = summary.file_size {
            frame.add(&fact_row("File", &format!("{:.0} MB", bytes as f64 / 1_048_576.0)));
        }
        state.info.append(&frame);
    }

    let mut lines: Vec<String> = Vec::new();
    let (width, height) = (photo.full_size.0, photo.full_size.1);
    lines.push(match &photo.full_working {
        Some(full) => format!("Loaded: {} × {} full", full.width, full.height),
        None => format!(
            "Loaded: {} × {} proxy of {width} × {height}",
            photo.proxy.width, photo.proxy.height
        ),
    });

    let zoom = state.zoom.get();
    if zoom != FIT_ZOOM {
        let (shown_width, shown_height) = displayed_size(state).unwrap_or(photo.full_size);
        let canvas = ((shown_width as f64 * zoom) as u32, (shown_height as f64 * zoom) as u32);

        let over = canvas.0.max(canvas.1) > 16384 && state.tile.get().is_none();
        lines.push(format!(
            "Canvas: {} × {}{}",
            canvas.0,
            canvas.1,
            if over { "  (past the GPU's limit)" } else { "" }
        ));
    }

    let (rendered_width, rendered_height) = state.rendered_size.get();
    if rendered_width > 0 {
        let from = if state.rendered_from_full.get() { "full" } else { "proxy" };
        let tiled = if state.tile.get().is_some() { ", tile" } else { "" };
        lines.push(format!("Rendered: {rendered_width} × {rendered_height} ({from}{tiled})"));
    }

    let key = colour_key(&photo.document);
    if zoom > proxy_runs_out_at(photo) && !state.rendered_from_full.get() {
        let key_is_stale =
            photo.full_working.is_some() && photo.full_working_key.as_ref() != Some(&key);
        lines.push(
            match (state.loading_full.get(), state.failed_key.borrow().as_ref() == Some(&key)) {
                (true, _) => "Waiting: the original is being decoded".to_string(),
                (_, true) => "Soft: the original could not be decoded".to_string(),
                _ if key_is_stale => {
                    "Soft: the colour changed since the original was decoded".to_string()
                }
                _ => "Soft: the original is not loaded".to_string(),
            },
        );
    }
    drop(open);

    state.render_info.set_text(&lines.join("\n"));
    state.render_info.set_xalign(0.0);
    state.render_info.set_wrap(true);
    state.render_info.set_selectable(true);
    state.render_info.add_css_class("profile-note");

    let disclosure = gtk::Expander::new(None);
    disclosure.set_label_widget(Some(&section_header("Rendering")));
    disclosure.set_child(Some(&state.render_info));
    state.info.append(&disclosure);
}

fn begin_open(state: &App) -> u64 {
    state.tile.set(None);
    *state.failed_key.borrow_mut() = None;
    state.full_resolution_stale.set(false);
    let generation = state.open_generation.get().wrapping_add(1);
    state.open_generation.set(generation);
    generation
}

fn rendered_document(state: &App, document: &Document) -> Document {
    let mut document = document.clone();

    if state.point_show.is_active() {
        let mut points = document.point_colours();
        points.highlight = Some(state.point_selected.get());
        document.set_point_colours(points);
    }
    if is_cropping(state) {

        let angle = document.crop().map_or(0.0, |(_, angle)| angle);
        document.set_crop([0.0, 0.0, 1.0, 1.0], angle);
    }
    document
}

fn visible_rect(state: &App) -> Option<[f32; 4]> {
    let bounds = state.canvas.compute_bounds(&state.canvas_scroller)?;
    let (shown_width, shown_height) = (bounds.width() as f64, bounds.height() as f64);
    if shown_width <= 0.0 || shown_height <= 0.0 {
        return None;
    }

    let left = (-bounds.x() as f64).clamp(0.0, shown_width);
    let top = (-bounds.y() as f64).clamp(0.0, shown_height);
    let right =
        (state.canvas_scroller.width() as f64 - bounds.x() as f64).clamp(left, shown_width);
    let bottom =
        (state.canvas_scroller.height() as f64 - bounds.y() as f64).clamp(top, shown_height);

    Some([
        (left / shown_width) as f32,
        (top / shown_height) as f32,
        ((right - left) / shown_width) as f32,
        ((bottom - top) / shown_height) as f32,
    ])
}

fn tile_for(state: &App) -> Option<[f32; 4]> {

    const MARGIN: f32 = 0.5;

    let [x, y, width, height] = visible_rect(state)?;
    if width >= 0.9 && height >= 0.9 {
        return None;
    }

    let grow = |start: f32, length: f32| {
        let low = (start - length * MARGIN).max(0.0);
        let high = (start + length * (1.0 + MARGIN)).min(1.0);
        (low, high - low)
    };
    let (left, tile_width) = grow(x, width);
    let (top, tile_height) = grow(y, height);

    (tile_width > 0.0 && tile_height > 0.0).then_some([left, top, tile_width, tile_height])
}

fn displayed_size(state: &App) -> Option<(u32, u32)> {
    let open = state.open.borrow();
    let photo = open.as_ref()?;
    let document = rendered_document(state, &photo.document);

    let (mut width, mut height) = photo.full_size;
    if matches!(document.rotation() as i32, 90 | 270) {
        std::mem::swap(&mut width, &mut height);
    }
    if let Some(([_, _, crop_width, crop_height], _)) = document.crop() {

        width = ((width as f32 * crop_width).round() as u32).max(1);
        height = ((height as f32 * crop_height).round() as u32).max(1);
    }
    Some((width, height))
}

fn open_in_editor(state: &App, child: &impl IsA<gtk::Widget>) {
    let Ok(id) = child.widget_name().parse::<i64>() else { return };
    open_photo(state, id);
}

fn step_photo(state: &App, forward: bool) {
    let current = state.open.borrow().as_ref().and_then(|photo| match &photo.source {
        Source::Photo { id, .. } => Some(*id),
        Source::Bracket { .. } => None,
    });
    let Some(current) = current else { return };

    let next = {
        let order = state.order.borrow();
        let Some(at) = order.iter().position(|id| *id == current) else { return };
        let step = if forward { at + 1 } else { at.checked_sub(1).unwrap_or(usize::MAX) };
        order.get(step).copied()
    };

    match next {
        Some(id) => open_photo(state, id),

        None => state.toast(if forward { "Last photo" } else { "First photo" }),
    }
}

fn open_photo(state: &App, id: i64) {
    let Some((photo, _)) = state.cards.borrow().get(&id).cloned() else { return };

    save_open_edits(state);

    let entering = state.open.borrow().is_none();

    state.canvas.set_paintable(gtk::gdk::Paintable::NONE);
    state.before.set_active(false);

    state.face_names.borrow_mut().clear();
    leave_crop(state);
    state.loading_full.set(false);
    *state.open.borrow_mut() = None;
    state.stack.set_visible_child_name("editor");
    state.zoom.set(FIT_ZOOM);
    state.zoom_label.set_text("…");

    if entering {
        build_filmstrip(state);
    }
    mark_filmstrip(state, id);
    write_rating_button(state, photo.rating, photo.flag);
    write_mixer(state);
    write_point_colours(state);
    write_grading(state);
    write_perspective(state);
    refresh_retouch(state);
    refresh_face(state);
    refresh_found(state);

    state.selected_mask.set(None);

    let loaded = state.catalog.load_edits(photo.id);

    let edits_unreadable = loaded.is_err();
    if let Err(err) = &loaded {
        log::warn!("{}: could not read the stored edits: {err}", photo.path.display());
        state.toast("This photograph's stored edits could not be read — opening without them");
    }
    let document = loaded
        .ok()
        .flatten()
        .unwrap_or_else(|| Document::new(photo.path.to_string_lossy().to_string()));

    let generation = begin_open(state);
    let state = state.clone();
    let path = photo.path.clone();
    let ai_denoised = document.ai_denoise > 0.0;
    glib::spawn_future_local(async move {
        let decoded = busy(&state, "Opening…", move || {
            let linear = raw::decode_linear_any(&path)?;

            let full_size = (linear.width, linear.height);

            let proxy = linear.downscaled(PROXY_EDGE).unwrap_or(linear);

            if ai_denoised {
                render::ai_denoise::warm(&path, proxy.width, proxy.height);
            }
            Ok::<_, String>((proxy, full_size, raw::summary(&path)))
        })
        .await;

        if state.open_generation.get() != generation {
            return;
        }

        let (proxy, full_size, summary) = match decoded {
            Ok(Ok(proxy)) => proxy,
            Ok(Err(err)) => {
                state.toast(&format!("Could not open: {err}"));
                close_editor(&state);
                return;
            }
            Err(_) => {
                state.toast("Decoding was cancelled");
                close_editor(&state);
                return;
            }
        };

        refresh_profile_picker(&state);

        let basic = document.basic();

        let as_shot = proxy
            .profile
            .map(|profile| profile.as_shot_white_balance())
            .unwrap_or(WhiteBalance { temperature: 5500.0, tint: 0.0 });
        let document_balance = document.white_balance;
        let balance = document_balance.unwrap_or(as_shot);

        let adjustable = proxy.profile.is_some();
        state.sliders.temperature.set_sensitive(adjustable);
        state.sliders.tint.set_sensitive(adjustable);

        let working_key = colour_key(&document);
        let working = render::to_working_space(&document, &proxy);

        *state.open.borrow_mut() = Some(OpenPhoto {
            source: Source::Photo { id: photo.id, path: photo.path.clone() },
            summary,
            segmentation: None,
            mask_frame: None,
            embedding: None,
            embedding_pending: false,
            faces_pending: false,
            people: Vec::new(),
            segmenting: false,
            animal: None,
            draft: None,
            full_size,
            proxy,
            working,
            working_key,
            full_working: None,
            full_working_key: None,
            view: None,
            baseline: None,
            history: History::resumed(
                state
                    .catalog
                    .load_history(photo.id)
                    .ok()
                    .flatten()
                    .map(|(states, position)| (states.iter().map(EditState::of).collect(), position)),
                EditState::of(&document),
            ),
            document,
            as_shot,
            edits_unreadable,
        });

        state.applying.set(true);
        state.sliders.write(basic);
        state.sliders.write_white_balance(balance);
        refresh_slider_marks(&state);
        let (rect, angle) = state
            .open
            .borrow()
            .as_ref()
            .and_then(|photo| photo.document.crop())
            .unwrap_or(([0.0, 0.0, 1.0, 1.0], 0.0));
        state.crop_rect.set(rect);
        state.straighten.set_value(angle as f64);
        state.applying.set(false);
        state.curve_area.queue_draw();
        write_mixer(&state);
        write_point_colours(&state);
        write_grading(&state);
        write_perspective(&state);
        write_space_note(&state);
        ai_denoise::write(&state);
        refresh_retouch(&state);
        refresh_face(&state);
        refresh_found(&state);
        refresh_masks(&state);
        select_mask(&state, None);
        refresh_crumbs(&state);

        fill_segment_masks(&state);

        if segment::is_installed() {
            ensure_segmentation(&state);
        }

        ensure_faces(&state);
        adjustments_changed(&state);
        refresh_info(&state);
    });
}

fn save_open_edits(state: &App) {
    let saved = {
        let open = state.open.borrow();
        let Some(photo) = open.as_ref() else { return };
        if photo.edits_unreadable && photo.document.is_untouched() {
            return;
        }
        match &photo.source {
            Source::Photo { id, path } => state.catalog.save_edits(*id, &photo.document).and_then(|()| {

                let states: Vec<Document> = photo
                    .history
                    .states
                    .iter()
                    .map(|step| {
                        let mut document = Document::new(path.to_string_lossy().to_string());
                        step.restore(&mut document);
                        document
                    })
                    .collect();
                state.catalog.save_history(*id, &states, photo.history.position)
            }),
            Source::Bracket { .. } => Ok(()),
        }
    };
    if let Err(err) = saved {
        state.toast(&format!("Could not save adjustments: {err}"));
    }
}

fn close_editor(state: &App) {
    save_open_edits(state);

    clear_reference(state);
    if let Some(photo) = state.open.borrow().as_ref() {

        if matches!(photo.source, Source::Bracket { .. }) {
            state.toast("Merged image discarded — export it to keep it");
        }
    }
    begin_open(state);
    *state.open.borrow_mut() = None;
    state.stack.set_visible_child_name("library");
    if state.grid_stale.replace(false) {
        reload_grid(state);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_menu_entry_starts_the_appimage_it_was_written_for() {
        let entry = menu_entry_for(
            std::path::Path::new("/home/a b/Apps/Numa $1.AppImage"),
            std::path::Path::new("/home/a b/.local/share/icons/hicolor/256x256/apps/com.tijmen.Numa.png"),
        );
        assert!(entry.contains("Exec=\"/home/a b/Apps/Numa \\$1.AppImage\" %F\n"), "{entry}");
        assert!(entry.contains("TryExec=/home/a b/Apps/Numa $1.AppImage\n"));
        assert!(entry.contains("Icon=/home/a b/.local/share/icons/hicolor/256x256/apps/com.tijmen.Numa.png\n"));
        assert!(entry.contains("X-Numa-AppImage=true"));
        assert!(!entry.contains("Exec=numa"));
    }

    #[test]
    fn a_restored_snapshot_is_the_snapshot() {
        use numa::core::grading::Grading;
        use numa::core::mixer::Mixer;
        use numa::core::space::ColourSpace;

        let mut edited = Document::new("a.RAF".into());
        edited.set_basic(Basic {
            exposure: 1.25,
            contrast: -18.0,
            whites: 40.0,
            defringe: 30.0,
            moire: 15.0,
            ..Default::default()
        });
        edited.white_balance = Some(WhiteBalance { temperature: 7100.0, tint: 6.0 });
        edited.colour_profile = Some("Adobe Standard".into());
        edited.working_space = ColourSpace::AdobeRgb;
        edited.ai_denoise = 60.0;
        edited.set_crop([0.1, 0.2, 0.6, 0.5], 2.5);
        edited.set_rotation(90.0);
        edited.set_perspective(Perspective { vertical: -22.0, horizontal: 8.0, aspect: 4.0 });
        edited.set_curve(Curve::new([[0.0, 0.0], [0.4, 0.62], [1.0, 1.0]]));
        edited.set_mixer(Mixer::default());
        edited.set_point_colours(PointColours {
            points: vec![PointColour { hue: 40.0, ..Default::default() }],
            highlight: None,
        });
        edited.set_grading(Grading::default());
        edited.set_beautify(Beautify { spots: 40.0, skin: 25.0, ..Default::default() });
        edited.set_masks(vec![Mask::new(Shape::radial())]);

        let snapshot = EditState::of(&edited);

        let mut other = Document::new("a.RAF".into());
        other.set_basic(Basic { exposure: -2.0, saturation: 55.0, ..Default::default() });
        other.working_space = ColourSpace::ProPhoto;
        other.set_crop([0.0, 0.0, 0.3, 0.3], -7.0);

        snapshot.restore(&mut other);
        assert_eq!(
            EditState::of(&other),
            snapshot,
            "a field of the snapshot is not being put back"
        );

        assert_eq!(other.basic().exposure, 1.25);
        assert_eq!(other.basic().contrast, -18.0);
        assert_eq!(other.working_space, ColourSpace::AdobeRgb);
    }

    #[test]
    fn the_curve_editor_stays_inside_its_box() {
        const PAD: usize = 24;
        let (width, height) = (240.0f64, 212.0f64);
        let (full_width, full_height) =
            (width as usize + PAD * 2, height as usize + PAD * 2);

        let mut surface = gtk::cairo::ImageSurface::create(
            gtk::cairo::Format::ARgb32,
            full_width as i32,
            full_height as i32,
        )
        .unwrap();

        {
            let context = gtk::cairo::Context::new(&surface).unwrap();
            context.translate(PAD as f64, PAD as f64);

            let curve = Curve::new([
                [0.0, 0.0],
                [0.13, 0.08],
                [0.5, 1.0],
                [0.72, 0.0],
                [1.0, 1.0],
            ]);
            let curves = [curve.clone(), curve.clone(), curve.clone(), curve];
            draw_curve(&context, width, height, Some(&curves), 1, None);
        }

        let stride = surface.stride() as usize;
        let data = surface.data().unwrap();
        let mut outside = Vec::new();
        for y in 0..full_height {
            for x in 0..full_width {
                let inside = x >= PAD
                    && y >= PAD
                    && x < PAD + width as usize
                    && y < PAD + height as usize;
                if inside {
                    continue;
                }

                if data[y * stride + x * 4 + 3] != 0 {
                    outside.push((x as i32 - PAD as i32, y as i32 - PAD as i32));
                }
            }
        }

        assert!(
            outside.is_empty(),
            "{} pixels drawn outside the box, first at {:?}",
            outside.len(),
            &outside[..outside.len().min(4)]
        );
    }

    #[test]
    fn the_mask_icon_is_two_circles_one_faded() {
        let size = 128;
        let mut surface =
            gtk::cairo::ImageSurface::create(gtk::cairo::Format::ARgb32, size, size).unwrap();
        {
            let context = gtk::cairo::Context::new(&surface).unwrap();
            context.set_source_rgb(0.0, 0.0, 0.0);
            let _ = context.paint();
            draw_mask_icon(&context, size, size, (1.0, 1.0, 1.0));
        }

        let stride = surface.stride() as usize;
        let data = surface.data().unwrap();
        let at = |x: usize, y: usize| data[y * stride + x * 4 + 2] as i32;

        let unit = size as usize / 16;

        let outline = (0..3 * unit)
            .map(|x| at(x, 8 * unit))
            .max()
            .unwrap_or(0);
        assert!(outline > 180, "the left circle has no outline: {outline}");

        let inside_left = at(4 * unit, 8 * unit);
        assert!(inside_left < 80, "the left circle was filled in: {inside_left}");

        let inside_right = at(13 * unit, 8 * unit);
        assert!(
            (60..180).contains(&inside_right),
            "the right circle is not a faded fill: {inside_right}"
        );

        assert!(at(0, 0) < 30, "the corner is not background: {}", at(0, 0));
    }

    #[test]
    fn a_history_is_resumed_not_restarted() {
        let untouched = Document::new("x".into());
        let mut cropped = untouched.clone();
        cropped.set_crop([0.1, 0.1, 0.8, 0.8], 0.0);
        let mut brighter = cropped.clone();
        brighter.set_basic(Basic { exposure: 1.0, ..Default::default() });
        let (untouched, cropped, brighter) = (EditState::of(&untouched), EditState::of(&cropped), EditState::of(&brighter));

        let mut fresh = History::resumed(None, cropped.clone());
        assert_eq!(fresh.steps(), ["Original", "Crop"]);
        assert_eq!(fresh.undo(), Some(untouched.clone()), "the crop can be taken back");

        let saved = Some((vec![untouched.clone(), cropped.clone(), brighter.clone()], 1));
        let resumed = History::resumed(saved.clone(), cropped.clone());
        assert_eq!((resumed.states.len(), resumed.position), (3, 1));

        let pasted = History::resumed(Some((vec![untouched.clone(), cropped.clone()], 1)), brighter.clone());
        assert_eq!(pasted.steps(), ["Original", "Crop", "Exposure"]);

        assert_eq!(History::resumed(None, untouched.clone()).steps(), ["Original"]);

        let mut long = History::new(untouched);
        for step in 0..HISTORY_KEPT + 20 {
            let mut document = Document::new("x".into());
            document.set_basic(Basic { exposure: step as f32 / 100.0 + 0.01, ..Default::default() });
            long.push(EditState::of(&document));
        }
        assert_eq!((long.states.len(), long.position), (HISTORY_KEPT, HISTORY_KEPT - 1));
    }

    #[test]
    fn a_snapshot_put_back_is_one_named_step() {
        let mut saved = Document::new("x".into());
        saved.set_crop([0.1, 0.1, 0.8, 0.8], 0.0);
        saved.set_basic(Basic { exposure: 0.7, ..Default::default() });
        let json = serde_json::to_string(&saved).unwrap();

        let mut document = Document::new("x".into());
        document.set_basic(Basic { contrast: 30.0, ..Default::default() });
        let before = EditState::of(&document);
        let mut history = History::resumed(None, before.clone());

        let snapshot: Document = serde_json::from_str(&json).unwrap();
        history.push_named(EditState::of(&snapshot), "Before the sky");
        history.states[history.position].restore(&mut document);
        assert_eq!(EditState::of(&document), EditState::of(&saved));
        assert_eq!(history.steps(), ["Original", "Contrast", "Before the sky"]);

        history.push_named(EditState::of(&snapshot), "Before the sky");
        assert_eq!(history.states.len(), 3);
        assert_eq!(history.undo(), Some(before));

        history.push(EditState::untouched());
        assert_eq!(history.steps(), ["Original", "Contrast", "Contrast"]);
    }

    #[test]
    fn choosing_a_camera_profile_is_something_to_step_back_out_of() {
        let mut document = Document::new("x".into());
        let before = EditState::of(&document);

        document.colour_profile = Some("Adobe Standard".to_string());
        let after = EditState::of(&document);
        assert_ne!(before, after, "the snapshot did not notice the profile");

        let mut history = History::new(before.clone());
        history.push(after.clone());
        assert_eq!(history.undo(), Some(before));
        assert_eq!(history.redo(), Some(after));
    }

    #[test]
    fn the_marks_line_up_with_the_sliders_they_describe() {
        let as_shot = WhiteBalance { temperature: 5500.0, tint: 0.0 };
        let rest = at_rest_of(Basic::default(), as_shot, as_shot);
        assert!(rest.iter().all(|at| *at), "an untouched photograph has a mark on it");

        let moved = at_rest_of(
            Basic { exposure: 0.4, hdr: 20.0, ..Default::default() },
            as_shot,
            as_shot,
        );
        let marked: Vec<usize> =
            moved.iter().enumerate().filter(|(_, at)| !**at).map(|(index, _)| index).collect();
        assert_eq!(marked, vec![2, 8], "the wrong sliders would have been coloured");

        let warmed = WhiteBalance { temperature: 7000.0, tint: 0.0 };
        assert!(!at_rest_of(Basic::default(), warmed, as_shot)[0]);
        assert!(at_rest_of(Basic::default(), as_shot, as_shot)[0]);
    }

    #[test]
    fn dragging_a_mask_moves_what_was_grabbed() {
        let linear = Shape::Linear { from: [0.2, 0.8], to: [0.2, 0.2] };

        assert!(nearest_mask_handle(&linear, 0.21, 0.79) == Handle::Start);
        assert!(nearest_mask_handle(&linear, 0.2, 0.21) == Handle::End);
        assert!(nearest_mask_handle(&linear, 0.9, 0.5) == Handle::Whole);

        let moved = move_mask_handle(linear.clone(), Handle::End, [0.3, 0.1], [0.5, 0.3]);
        assert_eq!(moved, Shape::Linear { from: [0.2, 0.8], to: [0.5, 0.3] });

        let Shape::Linear { from, to } = move_mask_handle(linear, Handle::Whole, [0.1, -0.2], [0.0, 0.0])
        else {
            panic!("a linear gradient stopped being one");
        };
        assert!((from[0] - 0.3).abs() < 1e-6 && (to[0] - 0.3).abs() < 1e-6);
        assert!((from[1] - 0.6).abs() < 1e-6 && (to[1] - 0.0).abs() < 1e-6);
    }

    #[test]
    fn a_radial_resizes_from_its_edge_and_never_inverts() {
        let radial = Shape::Radial { centre: [0.5, 0.5], radius: [0.2, 0.3], feather: 0.4 };

        assert!(nearest_mask_handle(&radial, 0.5, 0.5) == Handle::Centre);
        assert!(nearest_mask_handle(&radial, 0.7, 0.5) == Handle::EdgeX);
        assert!(nearest_mask_handle(&radial, 0.5, 0.8) == Handle::EdgeY);

        let Shape::Radial { radius, feather, .. } =
            move_mask_handle(radial.clone(), Handle::EdgeX, [0.0, 0.0], [0.85, 0.5])
        else {
            panic!("a radial stopped being one");
        };
        assert!((radius[0] - 0.35).abs() < 1e-6);
        assert_eq!(radius[1], 0.3, "the other axis moved");
        assert_eq!(feather, 0.4);

        let Shape::Radial { radius, .. } =
            move_mask_handle(radial, Handle::EdgeX, [0.0, 0.0], [0.1, 0.5])
        else {
            panic!("a radial stopped being one");
        };
        assert!(radius[0] > 0.0, "the radius went through zero: {radius:?}");
    }

    #[test]
    fn the_corners_of_the_graph_are_the_corners_of_the_plot() {
        let (width, height) = (240.0, 212.0);
        let (left, top, plot_width, plot_height) = plot_rect(width, height);

        assert!(left > 0.0 && top > 0.0, "the plot must be inset");
        assert!((left + plot_width) < width && (top + plot_height) < height);

        assert!(left >= HANDLE && top >= HANDLE);

        let (_, _, tiny_width, tiny_height) = plot_rect(4.0, 4.0);
        assert!(tiny_width > 0.0 && tiny_height > 0.0);
    }
}

#[cfg(test)]
mod icon_proof {

    #[test]
    #[ignore]
    fn icons_to_look_at() {
        let out = std::env::var("OUT").unwrap_or("/tmp".into());
        for (name, paint) in [
            ("face", super::paint_face as fn(&gtk::cairo::Context)),
        ] {
            for size in [16, 64] {
                let mut surface =
                    gtk::cairo::ImageSurface::create(gtk::cairo::Format::ARgb32, size, size)
                        .unwrap();
                let context = gtk::cairo::Context::new(&surface).unwrap();
                context.set_source_rgb(1.0, 1.0, 1.0);
                let _ = context.paint();
                context.set_source_rgb(0.1, 0.1, 0.1);
                context.scale(size as f64 / 16.0, size as f64 / 16.0);
                paint(&context);
                drop(context);
                let stride = surface.stride() as usize;
                let data = surface.data().unwrap();

                std::fs::write(
                    format!("{out}/icon-{name}-{size}.raw"),
                    (0..size as usize)
                        .flat_map(|y| data[y * stride..y * stride + size as usize * 4].to_vec())
                        .collect::<Vec<u8>>(),
                )
                .unwrap();
            }
        }
    }
}

#[cfg(test)]
mod crop_aspect {
    use super::hold_aspect;

    #[test]
    fn a_held_crop_keeps_its_shape() {

        let held = hold_aspect([0.1, 0.1, 0.5, 0.3], 3, 1.0);
        assert!((held[2] - held[3]).abs() < 1e-5, "square: {held:?}");
        assert!((held[0] - 0.1).abs() < 1e-5 && (held[1] - 0.1).abs() < 1e-5, "anchor held");

        let held = hold_aspect([0.2, 0.2, 0.4, 0.4], 0, 2.0);
        assert!((held[0] + held[2] - 0.6).abs() < 1e-5, "right edge held: {held:?}");
        assert!((held[1] + held[3] - 0.6).abs() < 1e-5, "bottom edge held: {held:?}");
        assert!((held[2] / held[3] - 2.0).abs() < 1e-4, "twice as wide: {held:?}");

        let held = hold_aspect([0.0, 0.0, 0.9, 0.9], 3, 3.0);
        assert!(held[0] + held[2] <= 1.0 + 1e-5 && held[1] + held[3] <= 1.0 + 1e-5, "{held:?}");
        assert!((held[2] / held[3] - 3.0).abs() < 1e-4, "still three to one: {held:?}");
    }
}

#[cfg(test)]
mod brush_scale {
    use super::{brush_size, brush_travel, BRUSH_LARGEST, BRUSH_SMALLEST, DEFAULT_BRUSH};

    #[test]
    fn the_small_end_of_the_brush_has_the_travel() {
        assert!((brush_size(0.0) - BRUSH_SMALLEST).abs() < 1e-6);
        assert!((brush_size(1.0) - BRUSH_LARGEST).abs() < 1e-6);

        assert!((brush_size(brush_travel(DEFAULT_BRUSH)) - DEFAULT_BRUSH).abs() < 1e-5);

        assert!(brush_size(0.5) < 0.07, "{}", brush_size(0.5));

        assert!(brush_size(0.1) < 0.005, "{}", brush_size(0.1));
    }
}

#[cfg(test)]
mod mask_names {
    use numa::core::mask::{Mask, Shape};

    fn radial(name: Option<&str>) -> Mask {
        let mut mask = Mask::new(Shape::radial());
        mask.name = name.map(str::to_string);
        mask
    }

    #[test]
    fn numbered_only_when_shared() {
        let one = [radial(Some("Bird"))];
        assert_eq!(super::mask_label(&one, 0), "Bird");

        let two = [radial(Some("Bird")), radial(Some("Sky"))];
        assert_eq!(super::mask_label(&two, 0), "Bird");
        assert_eq!(super::mask_label(&two, 1), "Sky");

        let three = [radial(Some("Bird")), radial(Some("Sky")), radial(Some("Bird"))];
        assert_eq!(super::mask_label(&three, 0), "Bird 1");
        assert_eq!(super::mask_label(&three, 1), "Sky");
        assert_eq!(super::mask_label(&three, 2), "Bird 2");

        let plain = [radial(None), radial(None)];
        assert_eq!(super::mask_label(&plain, 0), "Radial 1");
        assert_eq!(super::mask_label(&plain, 1), "Radial 2");
    }
}
