use super::*;
use gtk::subclass::prelude::*;
use libshumate::prelude::*;
use libshumate::subclass::prelude::*;

pub(super) fn dir() -> PathBuf {
    numa::core::paths::cache_dir().join("map")
}

pub(super) fn file() -> PathBuf {
    dir().join("tiles.db")
}

pub(super) const CREDIT: &str = "© OpenStreetMap · Protomaps";

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct TileSource {
        pub file: RefCell<PathBuf>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for TileSource {
        const NAME: &'static str = "NumaTileSource";
        type Type = super::TileSource;
        type ParentType = libshumate::DataSource;
    }

    impl ObjectImpl for TileSource {}

    impl DataSourceImpl for TileSource {

        fn start_request(&self, x: i32, y: i32, zoom: i32, _: Option<gio::Cancellable>) -> Option<libshumate::DataSourceRequest> {
            let request = libshumate::DataSourceRequest::new(x, y, zoom);
            let answer = self.tile_data_future(x, y, zoom);
            let answered = request.clone();
            glib::spawn_future_local(async move {
                match answer.await {
                    Ok(bytes) => answered.emit_data(&bytes, true),
                    Err(err) => answered.emit_error(&err),
                }
            });
            Some(request)
        }

        fn tile_data_future(
            &self,
            x: i32,
            y: i32,
            zoom: i32,
        ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<glib::Bytes, glib::Error>> + 'static>> {
            let file = self.file.borrow().clone();
            Box::pin(async move {
                let (z, x, y) = (u8::try_from(zoom).unwrap_or(u8::MAX), x.max(0) as u32, y.max(0) as u32);
                let data = gio::spawn_blocking(move || numa::io::map::tile_or_above(&file, z, x, y)).await.ok().flatten();
                data.map(glib::Bytes::from_owned).ok_or_else(|| glib::Error::new(gio::IOErrorEnum::NotFound, "the map has no tile here yet"))
            })
        }
    }
}

glib::wrapper! {

    pub struct TileSource(ObjectSubclass<imp::TileSource>) @extends libshumate::DataSource;
}

impl TileSource {
    fn new() -> Self {
        let source: Self = glib::Object::new();
        *source.imp().file.borrow_mut() = file();
        source.set_min_zoom_level(0);
        source.set_max_zoom_level(u32::from(numa::io::map::DETAIL));
        source
    }
}

fn colour(dark: bool, role: &str) -> String {
    let token = match (dark, role) {
        (true, _) => format!("map_{role}"),
        (false, "land") => "ground_sheet".into(),
        (false, "water") => "ground_chip".into(),
        (false, "road" | "border") => "rule".into(),
        (false, "road_major") => "track_hover".into(),
        (false, "label") => "label".into(),
        (false, _) => "name".into(),
    };
    let (dark_half, light_half) = TOKENS.split_once("/* light */").unwrap_or((TOKENS, ""));
    let find = |half: &str| {
        half.lines().find_map(|line| line.trim().strip_prefix(&format!("@define-color numa_{token} "))?.strip_suffix(';').map(str::to_string))
    };
    match dark {
        true => find(dark_half),
        false => find(light_half).or_else(|| find(dark_half)),
    }
    .unwrap_or_default()
}

pub(super) fn style(dark: bool) -> String {
    let c = |role: &str| colour(dark, role);
    let kind = |kinds: &[&str]| serde_json::json!(["in", ["get", "kind"], ["literal", kinds]]);
    let label = |colour: String, size: serde_json::Value, spaced: bool| {
        let mut layout = serde_json::json!({ "text-field": ["coalesce", ["get", "name:en"], ["get", "name"]], "text-size": size, "text-max-width": 8 });
        if spaced {
            layout["text-transform"] = "uppercase".into();
            layout["text-letter-spacing"] = 0.12.into();
        }
        (layout, serde_json::json!({ "text-color": colour, "text-halo-color": c("land"), "text-halo-width": 1.2 }))
    };
    let symbol = |id: &str, from: &str, filter: serde_json::Value, (layout, paint): (serde_json::Value, serde_json::Value)| {
        serde_json::json!({ "id": id, "type": "symbol", "source": "numa", "source-layer": from, "filter": filter, "layout": layout, "paint": paint })
    };

    let (mut towns, town_paint) = label(c("label_place"), serde_json::json!(["interpolate", ["linear"], ["zoom"], 4, 11, 12, 13]), false);
    towns["symbol-sort-key"] = serde_json::json!(["get", "min_zoom"]);
    let layers = serde_json::json!([
        { "id": "sea", "type": "background", "paint": { "background-color": c("water") } },
        { "id": "land", "type": "fill", "source": "numa", "source-layer": "earth", "paint": { "fill-color": c("land") } },
        { "id": "water", "type": "fill", "source": "numa", "source-layer": "water", "filter": ["==", ["geometry-type"], "Polygon"], "paint": { "fill-color": c("water") } },
        { "id": "roads", "type": "line", "source": "numa", "source-layer": "roads", "minzoom": 11, "filter": kind(&["minor_road"]),
          "paint": { "line-color": c("road"), "line-width": ["interpolate", ["exponential", 1.6], ["zoom"], 11, 0.5, 18, 6] } },
        { "id": "major", "type": "line", "source": "numa", "source-layer": "roads", "filter": kind(&["highway", "major_road"]),
          "paint": { "line-color": c("road_major"), "line-width": ["interpolate", ["exponential", 1.6], ["zoom"], 5, 0.4, 18, 10] } },
        { "id": "regions", "type": "line", "source": "numa", "source-layer": "boundaries", "minzoom": 4, "filter": kind(&["region"]),
          "paint": { "line-color": c("border"), "line-width": 0.6, "line-dasharray": [2, 2] } },
        { "id": "borders", "type": "line", "source": "numa", "source-layer": "boundaries", "filter": kind(&["country"]),
          "paint": { "line-color": c("border"), "line-width": 1 } },
        symbol("seas", "water", serde_json::json!(["==", ["geometry-type"], "Point"]), label(c("label"), 11.into(), true)),
        symbol("countries", "places", kind(&["country"]), label(c("label"), 11.into(), true)),
        symbol("towns", "places", kind(&["locality"]), (towns, town_paint)),
    ]);
    serde_json::json!({
        "version": 8,
        "name": "Numa",
        "sources": { "numa": { "type": "vector", "tiles": ["numa:{z}/{x}/{y}"], "maxzoom": numa::io::map::DETAIL } },
        "layers": layers,
    })
    .to_string()
}

pub(super) fn new_map() -> Option<(libshumate::Map, libshumate::MarkerLayer)> {
    if !libshumate::VectorRenderer::is_supported() {
        log::warn!("map: this libshumate has no vector renderer");
        return None;
    }
    let renderer = make_renderer(adw::StyleManager::default().is_dark())?;
    let map = libshumate::Map::new();
    let viewport = map.viewport()?;
    viewport.set_reference_map_source(Some(&renderer));
    viewport.set_min_zoom_level(1);
    viewport.set_max_zoom_level(16);
    let layer = libshumate::MapLayer::new(&renderer, &viewport);
    map.add_layer(&layer);
    let markers = libshumate::MarkerLayer::new(&viewport);
    map.add_layer(&markers);
    map.add_css_class("numa-map");
    let held = Rc::new(RefCell::new(layer));
    adw::StyleManager::default().connect_dark_notify(glib::clone!(
        #[weak] map,
        #[weak] viewport,
        #[weak] markers,
        move |style| {
            let Some(renderer) = make_renderer(style.is_dark()) else { return };
            let fresh = libshumate::MapLayer::new(&renderer, &viewport);
            map.insert_layer_behind(&fresh, Some(&markers));
            map.remove_layer(&held.replace(fresh));
            viewport.set_reference_map_source(Some(&renderer));
        }
    ));
    Some((map, markers))
}

fn make_renderer(dark: bool) -> Option<libshumate::VectorRenderer> {
    match libshumate::VectorRenderer::new("numa", &style(dark)) {
        Ok(renderer) => {
            renderer.set_data_source("numa", &TileSource::new());
            Some(renderer)
        }
        Err(err) => {
            log::warn!("map: the style would not load: {err}");
            None
        }
    }
}

pub(super) fn refresh(map: &libshumate::Map) {
    let mut child = map.first_child();
    while let Some(widget) = child {
        child = widget.next_sibling();
        if let Some(layer) = widget.downcast_ref::<libshumate::MapLayer>() {
            if let Some(renderer) = layer.map_source().and_downcast::<libshumate::VectorRenderer>() {
                renderer.set_data_source("numa", &TileSource::new());
            }
        }
    }
}

fn mercator(latitude: f64, longitude: f64) -> (f64, f64) {
    let latitude = latitude.clamp(-85.0, 85.0).to_radians();
    ((longitude + 180.0) / 360.0, (1.0 - latitude.tan().asinh() / std::f64::consts::PI) / 2.0)
}

fn unmercator(x: f64, y: f64) -> (f64, f64) {
    let latitude = (std::f64::consts::PI * (1.0 - 2.0 * y)).sinh().atan().to_degrees();
    (latitude, x * 360.0 - 180.0)
}

pub(super) fn fit(map: &libshumate::Map, area: [f64; 4], padding: f64, most: f64) {
    let Some(viewport) = map.viewport() else { return };
    let (west, south) = mercator(area[1], area[0]);
    let (east, north) = mercator(area[3], area[2]);
    let tile = viewport.reference_map_source().map_or(512, |source| source.tile_size()) as f64;
    let (width, height) = ((map.width() as f64 - 2.0 * padding).max(64.0), (map.height() as f64 - 2.0 * padding).max(64.0));
    let span = |across: f64, room: f64| if across <= 0.0 { most } else { (room / (across * tile)).log2() };
    let zoom = span(east - west, width).min(span(south - north, height)).clamp(1.0, most);
    let (latitude, longitude) = unmercator((west + east) / 2.0, (north + south) / 2.0);
    viewport.set_zoom_level(zoom);
    map.center_on(latitude, longitude);
}

pub(super) fn pixel(map: &libshumate::Map, latitude: f64, longitude: f64) -> Option<(f64, f64)> {
    map.viewport().map(|viewport| viewport.location_to_widget_coords(map, latitude, longitude))
}

pub(super) fn world_pixel(latitude: f64, longitude: f64, zoom: f64) -> (f64, f64) {
    let size = 512.0 * 2f64.powf(zoom);
    let (x, y) = mercator(latitude, longitude);
    (x * size, y * size)
}

thread_local! {

    static BRINGING: Cell<bool> = const { Cell::new(false) };
}

pub(super) fn bring(done: impl FnOnce(bool) + 'static) {
    if BRINGING.replace(true) {
        return;
    }
    let positions = spots::positions();
    glib::spawn_future_local(async move {
        let brought = gio::spawn_blocking(move || numa::io::map::bring(&dir(), &positions, |_, _| {})).await;
        BRINGING.set(false);
        match brought {
            Ok(Ok((tiles, bytes))) => {
                if tiles > 0 {
                    log::info!("map: {tiles} tiles, {} MB", bytes / 1_000_000);
                }
                done(tiles > 0);
            }
            Ok(Err(err)) => {
                log::warn!("map: {err}");
                done(false);
            }
            Err(_) => done(false),
        }
    });
}

pub(super) fn bringing() -> bool {
    BRINGING.get()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_position_goes_to_the_world_and_back() {
        let (x, y) = mercator(38.7223, -9.1393);
        let (latitude, longitude) = unmercator(x, y);
        assert!((latitude - 38.7223).abs() < 1e-9 && (longitude + 9.1393).abs() < 1e-9);
        assert_eq!(mercator(0.0, 0.0), (0.5, 0.5));
    }

    #[test]
    fn the_style_is_the_tokens_greys_and_one_source() {
        let style: serde_json::Value = serde_json::from_str(&style(true)).unwrap();
        assert_eq!(style["sources"].as_object().map(|sources| sources.len()), Some(1));
        assert_eq!(style["layers"][0]["paint"]["background-color"], colour(true, "water"));
        assert!(colour(true, "land").starts_with('#') && colour(false, "land").starts_with('#'));
        assert_ne!(colour(true, "land"), colour(false, "land"), "the light scheme has its own ground");
    }
}
