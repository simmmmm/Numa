use numa_core::lens::LensProfile;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

const RADII: [f32; 9] = [
    0.35352114, 0.5, 0.6126761, 0.7070423, 0.7908451, 0.86619717, 0.93521124, 1.0, 1.0605633,
];

const DISTANCE: f32 = 1000.0;

#[derive(Debug, Clone)]
pub struct Camera {
    pub maker: String,
    pub model: String,
    pub mount: String,
    pub crop: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Distortion {

    Poly3(f32),

    Poly5(f32, f32),

    PtLens(f32, f32, f32),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Tca {

    Linear(f32, f32),

    Poly3([f32; 3], [f32; 3]),
}

#[derive(Debug, Clone)]
pub struct Lens {
    pub maker: String,
    pub model: String,
    pub mounts: Vec<String>,

    pub crop: f32,
    pub aspect: f32,
    pub distortion: Vec<(f32, Distortion)>,
    pub tca: Vec<(f32, Tca)>,

    pub vignetting: Vec<([f32; 3], [f32; 3])>,

    focal_span: f32,

    words: Vec<Vec<String>>,
}

#[derive(Debug, Default)]
pub struct Database {
    pub cameras: Vec<Camera>,
    pub lenses: Vec<Lens>,

    compat: HashMap<String, Vec<String>>,
}

impl Database {

    pub fn load_dir(dir: &Path) -> Result<Self, String> {
        let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
            .map_err(|err| format!("{}: {err}", dir.display()))?
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| path.extension().is_some_and(|ext| ext == "xml"))
            .collect();
        files.sort();
        let mut db = Database::default();
        for file in files {
            let text = std::fs::read_to_string(&file).map_err(|err| format!("{}: {err}", file.display()))?;
            db.load_str(&text).map_err(|err| format!("{}: {err}", file.display()))?;
        }
        Ok(db)
    }

    pub fn load_str(&mut self, xml: &str) -> Result<(), String> {
        let options = roxmltree::ParsingOptions { allow_dtd: true, ..Default::default() };
        let doc = roxmltree::Document::parse_with_options(xml, options).map_err(|err| err.to_string())?;
        for node in doc.root_element().children().filter(|n| n.is_element()) {
            match node.tag_name().name() {
                "mount" => {
                    if let Some(name) = text(node, "name") {
                        let compat = elements(node, "compat").filter_map(|n| n.text()).map(|t| t.trim().to_string());
                        self.compat.entry(name).or_default().extend(compat);
                    }
                }
                "camera" => {
                    let (Some(maker), Some(model)) = (text(node, "maker"), text(node, "model")) else { continue };
                    self.cameras.push(Camera {
                        maker,
                        model,
                        mount: text(node, "mount").unwrap_or_default(),
                        crop: text(node, "cropfactor").and_then(|t| t.parse().ok()).unwrap_or(1.0),
                    });
                }
                "lens" => self.lenses.extend(lens(node)),
                _ => {}
            }
        }
        Ok(())
    }

    pub fn find_cameras(&self, maker: &str, model: &str) -> Vec<&Camera> {
        let (maker, model) = (words(maker), words(model));
        let mut found: Vec<(usize, &Camera)> = self
            .cameras
            .iter()
            .filter_map(|camera| {
                let theirs = words(&camera.maker);
                if !contains_all(&theirs, &maker) && !contains_all(&maker, &theirs) {
                    return None;
                }

                let not_maker = |w: &String| !theirs.contains(w) && !maker.contains(w);
                let mut theirs_model: Vec<String> = words(&camera.model).into_iter().filter(not_maker).collect();
                let mut ours: Vec<String> = model.iter().filter(|w| not_maker(w)).cloned().collect();
                if ours.is_empty() {
                    (theirs_model, ours) = (words(&camera.model), model.clone());
                }
                (!ours.is_empty() && contains_all(&theirs_model, &ours))
                    .then(|| (theirs_model.len() - ours.len(), camera))
            })
            .collect();
        found.sort_by_key(|(extra, _)| *extra);
        found.into_iter().map(|(_, camera)| camera).collect()
    }

    pub fn find_lenses(&self, camera: &Camera, model: &str) -> Vec<&Lens> {
        let ours = words(model);
        if ours.is_empty() {
            return Vec::new();
        }
        let numbers_only = !ours.iter().any(|w| w != "mm" && w.chars().any(char::is_alphabetic));
        let mut mounts = vec![camera.mount.as_str()];
        mounts.extend(self.compat.get(&camera.mount).into_iter().flatten().map(String::as_str));
        let mut found: Vec<(usize, &Lens)> = self
            .lenses
            .iter()
            .filter(|lens| lens.mounts.iter().any(|m| mounts.contains(&m.as_str())))
            .filter(|lens| lens.crop * 0.96 <= camera.crop)

            .filter(|lens| !lens.model.contains('+') || model.contains('+'))
            .filter_map(|lens| {
                let extra = lens.words.iter().filter(|w| contains_all(w, &ours)).map(|w| w.len() - ours.len()).min()?;
                (extra == 0 || !numbers_only).then_some((extra, lens))
            })
            .collect();
        found.sort_by(|(a, x), (b, y)| a.cmp(b).then((camera.crop / x.crop).ln().abs().total_cmp(&(camera.crop / y.crop).ln().abs())));
        found.into_iter().map(|(_, lens)| lens).collect()
    }

    pub fn fixed_lens(&self, camera: &Camera) -> Option<&Lens> {
        if self.compat.contains_key(&camera.mount) {
            return None;
        }
        let mut lenses = self.lenses.iter().filter(|lens| {
            lens.mounts.contains(&camera.mount)
                && lens.crop * 0.96 <= camera.crop
                && !lens.model.contains(" + ")
                && !lens.model.contains(", with")
        });
        let lens = lenses.next()?;
        lenses.next().is_none().then_some(lens)
    }
}

fn words(text: &str) -> Vec<String> {
    let kind = |c: char| {
        if c.is_ascii_digit() {
            0
        } else if c.is_ascii_punctuation() {
            1
        } else {
            2
        }
    };
    let mut out: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut last = None;
    for c in text.chars() {
        let k = (!c.is_whitespace()).then(|| kind(c));
        if (k.is_none() || k != last) && !current.is_empty() {
            out.push(std::mem::take(&mut current));
        }
        if k.is_some() {
            current.extend(c.to_lowercase());
        }
        last = k;
    }
    if !current.is_empty() {
        out.push(current);
    }
    out.retain(|w| w != "f" && !(w.len() == 1 && w.chars().all(|c| c.is_ascii_punctuation())));
    out
}

fn contains_all(have: &[String], wanted: &[String]) -> bool {
    let mut left: Vec<&String> = have.iter().collect();
    wanted.iter().all(|w| match left.iter().position(|h| *h == w) {
        Some(i) => {
            left.swap_remove(i);
            true
        }
        None => false,
    })
}

fn elements<'a, 'input>(
    node: roxmltree::Node<'a, 'input>,
    name: &'static str,
) -> impl Iterator<Item = roxmltree::Node<'a, 'input>> {
    node.children().filter(move |n| n.has_tag_name(name))
}

fn text(node: roxmltree::Node, name: &'static str) -> Option<String> {
    elements(node, name).find(|n| n.attribute("lang").is_none()).and_then(|n| n.text()).map(|t| t.trim().to_string())
}

fn lens(node: roxmltree::Node) -> Option<Lens> {
    let model = text(node, "model")?;
    let aspect = text(node, "aspect-ratio")
        .and_then(|t| match t.split_once(':') {
            Some((w, h)) => Some(w.trim().parse::<f32>().ok()? / h.trim().parse::<f32>().ok()?),
            None => t.parse().ok(),
        })
        .unwrap_or(1.5);
    let mut lens = Lens {
        maker: text(node, "maker").unwrap_or_default(),

        words: elements(node, "model")
            .filter(|n| matches!(n.attribute("lang"), None | Some("en")))
            .filter_map(|n| n.text())
            .map(words)
            .collect(),
        model,
        mounts: elements(node, "mount").filter_map(|n| n.text()).map(|t| t.trim().to_string()).collect(),
        crop: text(node, "cropfactor").and_then(|t| t.parse().ok()).unwrap_or(1.0),
        aspect: aspect.max(1.0 / aspect),
        distortion: Vec::new(),
        tca: Vec::new(),
        vignetting: Vec::new(),
        focal_span: elements(node, "focal")
            .next()
            .and_then(|n| Some(n.attribute("max")?.trim().parse::<f32>().ok()? - n.attribute("min")?.trim().parse::<f32>().ok()?))
            .filter(|span| *span > 0.0)
            .unwrap_or(1.0),
    };
    for entry in elements(node, "calibration").flat_map(|c| c.children()).filter(|n| n.is_element()) {
        let n = |attr: &str, default: f32| entry.attribute(attr).and_then(|v| v.trim().parse().ok()).unwrap_or(default);
        let focal = n("focal", 0.0);
        match (entry.tag_name().name(), entry.attribute("model").unwrap_or("")) {
            ("distortion", "poly3") => lens.distortion.push((focal, Distortion::Poly3(n("k1", 0.0)))),
            ("distortion", "poly5") => lens.distortion.push((focal, Distortion::Poly5(n("k1", 0.0), n("k2", 0.0)))),
            ("distortion", "ptlens") => {
                lens.distortion.push((focal, Distortion::PtLens(n("a", 0.0), n("b", 0.0), n("c", 0.0))))
            }
            ("tca", "linear") => lens.tca.push((focal, Tca::Linear(n("kr", 1.0), n("kb", 1.0)))),
            ("tca", "poly3") => lens.tca.push((
                focal,
                Tca::Poly3([n("br", 0.0), n("cr", 0.0), n("vr", 1.0)], [n("bb", 0.0), n("cb", 0.0), n("vb", 1.0)]),
            )),
            ("vignetting", "pa") => lens.vignetting.push((
                [focal, n("aperture", 0.0), n("distance", 1000.0)],
                [n("k1", 0.0), n("k2", 0.0), n("k3", 0.0)],
            )),

            _ => {}
        }
    }
    Some(lens)
}

impl Distortion {
    fn params(self) -> [f32; 3] {
        match self {
            Distortion::Poly3(k1) => [k1, 0.0, 0.0],
            Distortion::Poly5(k1, k2) => [k1, k2, 0.0],
            Distortion::PtLens(a, b, c) => [a, b, c],
        }
    }

    fn with(self, p: [f32; 3]) -> Self {
        match self {
            Distortion::Poly3(_) => Distortion::Poly3(p[0]),
            Distortion::Poly5(..) => Distortion::Poly5(p[0], p[1]),
            Distortion::PtLens(..) => Distortion::PtLens(p[0], p[1], p[2]),
        }
    }

    pub fn ratio(self, r: f32) -> f32 {
        match self {
            Distortion::Poly3(k1) => 1.0 - k1 + k1 * r * r,
            Distortion::Poly5(k1, k2) => 1.0 + k1 * r * r + k2 * r.powi(4),
            Distortion::PtLens(a, b, c) => a * r.powi(3) + b * r * r + c * r + 1.0 - a - b - c,
        }
    }
}

impl Tca {
    fn params(self) -> [f32; 6] {
        match self {
            Tca::Linear(r, b) => [r, b, 0.0, 0.0, 0.0, 0.0],
            Tca::Poly3(r, b) => [r[0], r[1], r[2], b[0], b[1], b[2]],
        }
    }

    fn with(self, p: [f32; 6]) -> Self {
        match self {
            Tca::Linear(..) => Tca::Linear(p[0], p[1]),
            Tca::Poly3(..) => Tca::Poly3([p[0], p[1], p[2]], [p[3], p[4], p[5]]),
        }
    }

    pub fn ratios(self, r: f32) -> (f32, f32) {
        match self {
            Tca::Linear(kr, kb) => (kr, kb),
            Tca::Poly3([br, cr, vr], [bb, cb, vb]) => (br * r * r + cr * r + vr, bb * r * r + cb * r + vb),
        }
    }
}

fn along_focal<const N: usize>(points: &[(f32, [f32; N])], focal: f32, scaled: [bool; N]) -> Option<[f32; N]> {

    let (mut below, mut above): ([Option<usize>; 2], [Option<usize>; 2]) = ([None; 2], [None; 2]);
    for (i, (f, _)) in points.iter().enumerate() {
        let (side, gap) = if *f <= focal { (&mut below, focal - f) } else { (&mut above, f - focal) };
        let gap_of = |j: Option<usize>| j.map_or(f32::MAX, |j: usize| (points[j].0 - focal).abs());
        if gap < gap_of(side[0]) {
            side[1] = side[0];
            side[0] = Some(i);
        } else if gap < gap_of(side[1]) {
            side[1] = Some(i);
        }
    }
    let (i1, i2) = match (below[0], above[0]) {
        (Some(i1), Some(i2)) => (i1, i2),
        (Some(i), None) | (None, Some(i)) => return Some(points[i].1),
        (None, None) => return None,
    };
    let at = |j: usize, k: usize| points[j].1[k] * if scaled[k] { points[j].0 } else { 1.0 };
    let t = (focal - points[i1].0) / (points[i2].0 - points[i1].0);
    let (t2, t3) = (t * t, t * t * t);
    let mut out = [0.0; N];
    for k in 0..N {
        let (p1, p2) = (at(i1, k), at(i2, k));
        let m1 = below[1].map_or(p2 - p1, |i0| (p2 - at(i0, k)) / 2.0);
        let m2 = above[1].map_or(p2 - p1, |i3| (at(i3, k) - p1) / 2.0);
        let value = (2.0 * t3 - 3.0 * t2 + 1.0) * p1 + (t3 - 2.0 * t2 + t) * m1 + (-2.0 * t3 + 3.0 * t2) * p2 + (t3 - t2) * m2;
        out[k] = if scaled[k] { value / focal } else { value };
    }
    Some(out)
}

fn same_model<M: Copy, const N: usize>(
    entries: &[(f32, M)],
    focal: f32,
    params: fn(M) -> [f32; N],
) -> Option<(M, Vec<(f32, [f32; N])>)> {
    let nearest = entries.iter().min_by(|a, b| (a.0 - focal).abs().total_cmp(&(b.0 - focal).abs()))?.1;
    let kind = std::mem::discriminant(&nearest);
    let same = entries.iter().filter(|(_, m)| std::mem::discriminant(m) == kind).map(|(f, m)| (*f, params(*m)));
    Some((nearest, same.collect()))
}

impl Lens {
    pub fn distortion_at(&self, focal: f32) -> Option<Distortion> {
        let (model, points) = same_model(&self.distortion, focal, Distortion::params)?;
        Some(model.with(along_focal(&points, focal, [true; 3])?))
    }

    pub fn tca_at(&self, focal: f32) -> Option<Tca> {
        let (model, points) = same_model(&self.tca, focal, Tca::params)?;
        let scaled = match model {
            Tca::Linear(..) => [false; 6],
            Tca::Poly3(..) => [true, true, false, true, true, false],
        };
        Some(model.with(along_focal(&points, focal, scaled)?))
    }

    pub fn vignetting_at(&self, focal: f32, aperture: f32, distance: f32) -> Option<[f32; 3]> {
        let place = |[f, a, d]: [f32; 3]| [f / self.focal_span, 4.0 / a, 0.1 / d];
        let here = place([focal, aperture, distance]);
        let (mut sum, mut total, mut nearest) = ([0.0f32; 3], 0.0f32, f32::MAX);
        for (at, k) in &self.vignetting {
            let d = here.iter().zip(place(*at)).map(|(a, b)| (a - b) * (a - b)).sum::<f32>().sqrt();
            if d < 1e-4 {
                return Some(*k);
            }
            nearest = nearest.min(d);
            let w = d.powf(-3.5);
            total += w;
            for i in 0..3 {
                sum[i] += w * k[i];
            }
        }
        (nearest <= 1.0).then(|| sum.map(|s| s / total))
    }
}

pub fn database_dir() -> Option<PathBuf> {
    let mut candidates = Vec::new();
    if let Some(dir) = std::env::current_exe().ok().and_then(|exe| exe.parent().map(Path::to_path_buf)) {
        candidates.push(dir.join("lensfun"));
        candidates.push(dir.join("../Resources/lensfun"));
        candidates.push(dir.join("../share").join(crate::NAME).join("lensfun"));
    }
    candidates.push(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../data/lensfun"));
    candidates.into_iter().find(|dir| dir.is_dir())
}

pub fn warm() {
    database();
}

fn database() -> Option<&'static Database> {
    static DB: OnceLock<Option<Database>> = OnceLock::new();
    DB.get_or_init(|| {
        let Some(dir) = database_dir() else {
            log::warn!("lens database not found beside the application");
            return None;
        };
        Database::load_dir(&dir).map_err(|err| log::warn!("lens database unavailable: {err}")).ok()
    })
    .as_ref()
}

pub fn profile(
    make: &str,
    camera: &str,
    model: &str,
    focal: f32,
    aperture: f32,
    width: u32,
    height: u32,
) -> Option<LensProfile> {
    profile_in(database()?, make, camera, model, focal, aperture, width, height)
}

#[allow(clippy::too_many_arguments)]
pub fn profile_in(
    db: &Database,
    make: &str,
    camera: &str,
    model: &str,
    focal: f32,
    aperture: f32,
    width: u32,
    height: u32,
) -> Option<LensProfile> {
    if width < 2 || height < 2 || !(focal > 0.0) {
        return None;
    }

    let body = *db.find_cameras(make, camera).first()?;
    let lens = db.find_lenses(body, model).first().copied().or_else(|| db.fixed_lens(body))?;

    let pixel = (((width - 1) as f32).hypot((height - 1) as f32)) / (width as f32).hypot(height as f32);
    let geometric = (1.0 + lens.aspect * lens.aspect).sqrt() * pixel;

    let bends = lens.distortion_at(focal);
    let fringes = lens.tca_at(focal);
    let darkens = lens.vignetting_at(focal, aperture, DISTANCE);
    if bends.is_none() && fringes.is_none() && darkens.is_none() {
        return None;
    }

    let mut distortion = Vec::new();
    let mut red = Vec::new();
    let mut blue = Vec::new();
    let mut transmission = Vec::new();
    for r in RADII {
        if let Some(model) = bends {
            let centre = model.ratio(0.0);
            distortion.push((model.ratio(r * geometric / centre) / centre - 1.0) * 100.0);
        }
        if let Some(model) = fringes {
            let (kr, kb) = model.ratios(r * geometric);
            red.push(kr - 1.0);
            blue.push(kb - 1.0);
        }
        if let Some([k1, k2, k3]) = darkens {
            let r2 = (r * pixel).powi(2);
            transmission.push(1.0 + k1 * r2 + k2 * r2 * r2 + k3 * r2 * r2 * r2);
        }
    }

    if transmission.iter().any(|t| !(0.1..=1.5).contains(t)) {
        return None;
    }
    if distortion.iter().any(|d| d.abs() > 20.0) {
        return None;
    }
    if red.iter().chain(blue.iter()).any(|c| c.abs() > 0.05) {
        return None;
    }

    if transmission.is_empty() {
        transmission = vec![1.0; RADII.len()];
    }

    Some(LensProfile { radii: RADII.to_vec(), transmission, distortion, red, blue })
}

#[cfg(test)]
mod tests {
    use super::*;

    const FRAME: (u32, u32) = (7728, 5152);

    #[test]
    fn words_are_lensfuns_words() {
        assert_eq!(
            words("smc PENTAX-DA 12-24mm F4 ED AL [IF]"),
            ["smc", "pentax", "da", "12", "24", "mm", "4", "ed", "al", "if"]
        );
        assert_eq!(words("XF27mmF2.8"), ["xf", "27", "mmf", "2", "8"]);
        assert_eq!(words("f/2.8"), ["2", "8"]);
    }

    #[test]
    fn the_models_are_the_documented_formulas() {
        let r = 0.8f32;
        assert!((Distortion::Poly3(-0.02).ratio(r) - (1.0 + 0.02 - 0.02 * r * r)).abs() < 1e-6);
        assert!((Distortion::Poly5(0.1, -0.05).ratio(r) - (1.0 + 0.1 * r * r - 0.05 * r.powi(4))).abs() < 1e-6);
        let (a, b, c) = (0.01, -0.03, 0.02);
        let ptlens = a * r.powi(3) + b * r * r + c * r + 1.0 - a - b - c;
        assert!((Distortion::PtLens(a, b, c).ratio(r) - ptlens).abs() < 1e-6);

        for model in [Distortion::Poly3(-0.02), Distortion::PtLens(a, b, c)] {
            assert!((model.ratio(1.0) - 1.0).abs() < 1e-6);
        }
        assert_eq!(Tca::Linear(1.001, 0.999).ratios(r), (1.001, 0.999));
        let (red, _) = Tca::Poly3([0.001, 0.002, 0.999], [0.0, 0.0, 1.0]).ratios(r);
        assert!((red - (0.001 * r * r + 0.002 * r + 0.999)).abs() < 1e-6);
    }

    #[test]
    fn interpolation_passes_through_the_calibrations() {
        let points = [(10.0, [1.0]), (20.0, [3.0]), (40.0, [2.0]), (80.0, [2.0])];
        for scaled in [[false], [true]] {
            for (f, v) in points {
                assert!((along_focal(&points, f, scaled).unwrap()[0] - v[0]).abs() < 1e-6);
            }
            assert_eq!(along_focal(&points, 5.0, scaled), Some([1.0]));
            assert_eq!(along_focal(&points, 100.0, scaled), Some([2.0]));
            let middle = along_focal(&points, 15.0, scaled).unwrap()[0];
            assert!(middle > 1.0 && middle < 3.0);
        }
    }

    #[test]
    fn the_corner_of_the_frame_is_the_corner_of_the_model() {

        let expected = 1.0 - 0.9412 + 0.1158 + 0.1433;
        let corner = RADII.iter().position(|r| *r == 1.0).expect("a sample on the corner");

        for frame in [FRAME, (6000, 4000), (3000, 2000)] {
            let profile = profile("Fujifilm", "X-T5", "XF27mmF2.8", 27.0, 2.8, frame.0, frame.1)
                .expect("a lens the database holds");
            let difference = (profile.transmission[corner] - expected).abs();
            assert!(
                difference < 0.005,
                "{:?}: corner transmission {} is not the model's {expected}",
                frame,
                profile.transmission[corner]
            );
        }
    }

    #[test]
    fn a_zoom_bends_both_ways_along_its_range() {
        let wide = profile("Fujifilm", "X-T5", "XF16-80mmF4 R OIS WR", 16.0, 4.0, FRAME.0, FRAME.1)
            .expect("the wide end");
        let long = profile("Fujifilm", "X-T5", "XF16-80mmF4 R OIS WR", 80.0, 4.0, FRAME.0, FRAME.1)
            .expect("the long end");

        assert!(wide.distortion[7] < -5.0, "16 mm should barrel: {:?}", wide.distortion);
        assert!(long.distortion[7] > 4.0, "80 mm should pincushion: {:?}", long.distortion);

        for profile in [&wide, &long] {
            assert!(profile.transmission[7] < profile.transmission[0]);
        }
    }

    #[test]
    fn a_lens_nobody_has_measured_is_left_alone() {
        assert!(profile("Fujifilm", "X-T5", "Some Glass 50mm", 50.0, 2.0, FRAME.0, FRAME.1).is_none());

        assert!(profile("Fujifilm", "X-T5", "AF 23/1.4 XF", 23.0, 1.4, FRAME.0, FRAME.1).is_none());
    }

    #[test]
    fn a_body_is_found_under_the_names_cameras_write() {
        let db = database().expect("the database in data/lensfun");
        let found = |make, model| db.find_cameras(make, model).first().map(|c| c.model.clone());
        assert_eq!(found("FUJIFILM", "X-T5").as_deref(), Some("X-T5"));
        assert_eq!(found("Canon", "EOS R5").as_deref(), Some("Canon EOS R5"));
        assert_eq!(found("Nikon", "Z 6").as_deref(), Some("Nikon Z 6"));

        assert_eq!(found("RICOH IMAGING COMPANY, LTD.", "PENTAX K-1").as_deref(), Some("Pentax K-1"));
        assert_eq!(found("Canon", "Canon EOS R6m2").as_deref(), Some("Canon EOS R6m2"));
        assert_eq!(found("OLYMPUS CORPORATION", "E-M10MarkIV").as_deref(), Some("E-M10MarkIV"));
    }

    #[test]
    fn lens_names_as_cameras_write_them() {
        let db = database().expect("the database in data/lensfun");
        let lens = |make: &str, model: &str, name: &str| {
            let body = db.find_cameras(make, model).into_iter().next().expect(model);
            db.find_lenses(body, name).first().copied().or_else(|| db.fixed_lens(body)).map(|l| l.model.clone())
        };

        assert_eq!(lens("Canon", "Canon EOS 5D Mark IV", "24-70mm"), None);

        assert_eq!(lens("Canon", "Canon EOS 5D Mark IV", "EF70-200mm f/2.8L IS III USM"), None);
        assert_eq!(
            lens("OLYMPUS CORPORATION", "E-M10MarkIV", "OLYMPUS M.12-45mm F4.0").as_deref(),
            Some("OLYMPUS OM 12-45mm F4.0")
        );

        assert!(lens("RICOH IMAGING COMPANY, LTD.", "RICOH GR IIIx", "GR LENS").is_some());
        assert!(lens("LEICA CAMERA AG", "LEICA Q2", "28.0 mm f/1.7").is_some());
        assert_eq!(lens("RICOH IMAGING COMPANY, LTD.", "RICOH GR III", "GR LENS"), None);
    }
}
