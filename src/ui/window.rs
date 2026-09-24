use adw::prelude::*;
use gtk::{gio, glib};
use numa::core::curve::Curve;
use numa::cull;
use numa::core::color::{self, WhiteBalance};
use numa::core::document::{Balance, Basic, Calibration, Detail, Document, EditParts, Effects, Optics, Perspective, Presence, Tone};
use numa::core::image::LinearImage;
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
mod downloads;
mod masks;
mod colour;
mod effects;
mod filter;
mod grade;
mod retouch;
mod crop;
mod light;
mod detail;
mod slider;
mod icons;
mod measure;
mod audit;
mod presets;
mod pipeline;
mod reference;
mod zoom;
mod cards;
mod exporting;
mod startup;
mod places;
mod cullbar;
mod busy;
mod header;
mod libraries_dialog;
mod people;
mod desktop;
mod preferences;
mod add_library;
mod updates;
mod libraries;
mod library_page;
mod mask_overlay;
mod mask_list;
mod mask_parts;
mod pipettes;
mod mask_edits;
mod retouch_overlay;
mod panel_values;
mod copy_paste;
mod filmstrip;
mod culling;
mod picker;
mod albums;
mod grid;
mod merge;
mod loupe;
mod loupe_zoom;
mod importing;
mod folders;
mod compare;
mod badges;
mod rating;
mod history;
mod panel_sliders;
mod editor_page;
mod export_ui;
mod panel;
mod render_loop;
mod render_job;
mod zooming;
mod geometry;
mod overlays;
mod info;
mod open;
use masks::*;
use colour::*;
use effects::*;
use filter::*;
use grade::*;
use retouch::*;
use crop::*;
use light::*;
use detail::*;
use slider::*;
use icons::*;
use measure::*;
use audit::*;
use presets::*;
use pipeline::*;
use reference::*;
use zoom::*;
use cards::*;
use busy::*;
use header::*;
use libraries_dialog::*;
use people::*;
use desktop::*;
use preferences::*;
use add_library::*;
use updates::*;
use libraries::*;
use library_page::*;
use mask_overlay::*;
use mask_list::*;
use mask_parts::*;
use pipettes::*;
use mask_edits::*;
use retouch_overlay::*;
use panel_values::*;
use copy_paste::*;
use filmstrip::*;
use culling::*;
use picker::*;
use albums::*;
use grid::*;
use merge::*;
use loupe::*;
use importing::*;
use folders::*;
use compare::*;
use badges::*;
use rating::*;
use history::*;
use panel_sliders::*;
use editor_page::*;
use export_ui::*;
use panel::*;
use render_loop::*;
use render_job::*;
use zooming::*;
use geometry::*;
use overlays::*;
use info::*;
use open::*;
use mask_toolbar::*;
pub use header::show_app_about;

const GRID_THUMB_EDGE: u32 = numa::io::thumbs::LIBRARY_EDGE;

const PROXY_EDGE: u32 = 2400;
const PROXY_FLOOR: u32 = 1400;

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

const PANEL_WIDTH: i32 = 372;

const RAIL_WIDTH: i32 = 66;

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

    libraries: libraries::State,

    grid: grid::State,

    open: Rc<RefCell<Option<OpenPhoto>>>,

    zooming: zooming::State,
    stack: gtk::Stack,
    canvas: gtk::Picture,

    colour: colour::State,

    info: info::State,

    editor_page: editor_page::State,

    crop: crop::State,

    overlays: overlays::State,

    light: light::State,

    sliders: Sliders,

    applying: Rc<Cell<bool>>,

    render: render_loop::State,

    open_generation: Rc<Cell<u64>>,

    filmstrip: filmstrip::State,

    grade: grade::State,

    retouch: retouch::State,

    mask_overlay: mask_overlay::State,

    busy: Rc<Cell<bool>>,

    panel: panel::State,

    masks: masks::State,

    reference: reference::State,

    loupe: loupe::State,

    compare: compare::State,

    copy_paste: copy_paste::State,

    export: export_ui::State,
    toasts: adw::ToastOverlay,
}

impl App {

    fn toast(&self, message: &str) {
        self.toasts.add_toast(adw::Toast::new(&glib::markup_escape_text(message)));
    }
}

pub fn build_window(app: &adw::Application) -> adw::ApplicationWindow {
    load_css();

    let window = adw::ApplicationWindow::new(app);
    window.set_title(Some("Numa"));

    let Some(catalog) = open_catalog_or_explain(app, &window) else { return window };

    downloads::start_gpu(&catalog);

    let canvas = gtk::Picture::new();
    canvas.set_can_shrink(true);
    canvas.set_content_fit(gtk::ContentFit::Contain);

    let empty = gtk::Label::new(Some("Add a folder to start. Your photos stay where they are."));
    empty.add_css_class("empty-state");
    empty.set_wrap(true);

    let state = App {
        catalog: Rc::new(catalog),
        libraries: libraries::State::new(),
        grid: grid::State::new(empty),
        open: Rc::new(RefCell::new(None)),
        zooming: zooming::State::new(),
        stack: gtk::Stack::new(),
        canvas,
        colour: colour::State::new(),
        info: info::State::new(),
        editor_page: editor_page::State::new(),
        crop: crop::State::new(),
        overlays: overlays::State::new(),
        light: light::State::new(),
        sliders: Sliders::new(),
        applying: Rc::new(Cell::new(false)),
        render: render_loop::State::new(),
        open_generation: Rc::new(Cell::new(0)),
        filmstrip: filmstrip::State::new(),
        grade: grade::State::new(),
        masks: masks::State::new(),
        retouch: retouch::State::new(),
        mask_overlay: mask_overlay::State::new(),
        busy: Rc::new(Cell::new(false)),
        panel: panel::State::new(),
        reference: reference::State::new(),
        loupe: loupe::State::new(),
        compare: compare::State::new(),
        copy_paste: copy_paste::State::new(),
        export: export_ui::State::new(),
        toasts: adw::ToastOverlay::new(),
    };

    recall_window_state(&state, &window);
    build_window_content(&state, &window);
    install_window_lifecycle(&state, &window);

    window
}

fn open_catalog_or_explain(app: &adw::Application, window: &adw::ApplicationWindow) -> Option<Catalog> {
    match Catalog::open_default() {
        Ok(catalog) => Some(catalog),
        Err(err) => {

            let dialog = adw::AlertDialog::new(Some("Cannot open the catalog"), Some(&err));
            dialog.add_response("quit", "Quit");
            dialog.present(Some(window));
            let app = app.clone();
            dialog.connect_response(None, move |_, _| app.quit());
            None
        }
    }
}

fn recall_window_state(state: &App, window: &adw::ApplicationWindow) {

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
        *state.libraries.filter.borrow_mut() = filter;
    }
    if let Some(settings) = state.catalog.recall::<export::ExportSettings>(EXPORT_SETTINGS) {
        *state.export.settings.borrow_mut() = settings;
    }
}

fn build_window_content(state: &App, window: &adw::ApplicationWindow) {

    watch_the_button(state, window);
    state.stack.add_named(&build_library_page(state, window), Some("library"));

    state.stack.add_named(&build_folders_page(state), Some("folders"));
    state.stack.set_visible_child_name("library");

    glib::idle_add_local_once(glib::clone!(
        #[strong] state,
        move || startup::ensure_editor_page(&state)
    ));

    let view = gtk::Box::new(gtk::Orientation::Vertical, 0);
    view.append(&build_header(state, window));
    view.append(&state.stack);

    state.toasts.set_child(Some(&view));
    window.set_content(Some(&state.toasts));

    install_rating_shortcuts(state, window);

    name_icon_buttons(view.upcast_ref());
    startup::fill_the_window(state, window);

    glib::timeout_add_seconds_local_once(60, || {
        std::thread::spawn(|| {
            unsafe { libc::nice(19) };
            let freed = numa::io::thumbs::prune(numa::io::thumbs::CACHE_BUDGET);
            if freed > 0 {
                log::info!("thumbnail cache: {} MB of the least used let go", freed >> 20);
            }
        });
    });
}

fn install_window_lifecycle(state: &App, window: &adw::ApplicationWindow) {

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
            #[weak] window,
            #[upgrade_or] glib::ControlFlow::Break,
            move || {

                if window.is_active() {
                    rescan_in_background(&state);
                }
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
}

fn render_inputs(document: &Document) -> render::RenderInputs {
    render::RenderInputs {
        profile: document
            .colour_profile
            .as_deref()
            .filter(|name| *name != render::NO_COLOUR_PROFILE)
            .and_then(numa::io::dcp::by_name),
        denoised: (document.ai_denoise > 0.0)
            .then(|| numa::io::denoised::load(std::path::Path::new(&document.source.path)))
            .flatten(),

        sharpened: (document.ai_sharpen > 0.0)
            .then(|| numa::io::denoised::load_sharpened(std::path::Path::new(&document.source.path), document.ai_denoise > 0.0))
            .flatten(),
    }
}

struct OpenPhoto {
    source: Source,

    edits_unreadable: bool,
    proxy: Arc<LinearImage>,

    working: Arc<LinearImage>,

    draft: Option<Arc<LinearImage>>,

    inputs: render::RenderInputs,

    summary: Option<raw::Summary>,

    lens_corrected: bool,

    segmentation: Option<Arc<Segmentation>>,

    mask_frame: Option<Arc<image::RgbImage>>,

    embedding: Option<Arc<sam::Embedding>>,
    embedding_pending: bool,

    faces_pending: bool,

    people: Vec<SeenFace>,

    segmenting: bool,

    animal: Option<(std::sync::Weak<Segmentation>, numa::render::classify::Guess)>,

    full_size: (u32, u32),

    full_working: Option<Arc<LinearImage>>,
    full_working_key: Option<ColourKey>,

    full_native: Option<std::sync::Arc<LinearImage>>,

    view: Option<ViewTile>,

    draft_view: Option<ViewTile>,

    behind: Option<(u64, Arc<image::RgbImage>)>,

    tone_guide: Option<(u64, bool, Arc<render::local::ToneGuide>)>,

    baseline: Option<gtk::gdk::Paintable>,

    working_key: ColourKey,
    document: Document,

    before_preset: Option<Document>,
    history: History,

    as_shot: WhiteBalance,
}

#[derive(Clone)]
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

#[cfg(test)]
mod tests;

#[cfg(test)]
mod shelves;

#[cfg(test)]
mod crop_aspect;

#[cfg(test)]
mod brush_scale;

#[cfg(test)]
mod mask_names;
mod mask_toolbar;
mod watermark;
