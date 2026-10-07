use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;

use numa_core::camera_look::LookFit;
use numa_core::document::Document;
use rayon::prelude::*;

use crate::raw;

pub const EDGE: u32 = 1104;

pub const BLOCK: usize = 4;

const MARGIN: f32 = 0.04;

const CLIPPED: f32 = 250.0 / 255.0;

const LEAST_CORRELATION: f32 = 0.8;

pub struct Views {
    pub width: usize,
    pub height: usize,
    pub ours: Vec<[f32; 3]>,
    pub camera: Vec<[f32; 3]>,

    pub usable: Vec<bool>,

    pub scale: f32,
    pub shift: [f32; 2],
    pub correlation: f32,

    mapping: Mapping,
}

#[derive(Clone, Copy)]
struct Mapping {
    origin: [f32; 2],
    step: f32,
    size: [usize; 2],
    scale: f32,
    shift: [f32; 2],
}

impl Mapping {

    fn at(&self, x: f32, y: f32) -> [f32; 2] {
        let [w, h] = self.size.map(|v| v as f32);
        [
            self.origin[0] + self.step * (w / 2.0 + (x + 0.5 - w / 2.0) * self.scale) + self.shift[0] - 0.5,
            self.origin[1] + self.step * (h / 2.0 + (y + 0.5 - h / 2.0) * self.scale) + self.shift[1] - 0.5,
        ]
    }
}

struct Plane {
    width: usize,
    height: usize,
    data: Vec<[f32; 3]>,
}

impl Plane {
    fn read(&self, at: [f32; 2]) -> Option<[f32; 3]> {
        let [x, y] = at;
        if !(x >= 0.0 && y >= 0.0 && x <= (self.width - 1) as f32 && y <= (self.height - 1) as f32) {
            return None;
        }
        let (x0, y0) = ((x as usize).min(self.width - 2), (y as usize).min(self.height - 2));
        let (fx, fy) = (x - x0 as f32, y - y0 as f32);
        let p = |xx: usize, yy: usize| self.data[yy * self.width + xx];
        let (a, b, c, d) = (p(x0, y0), p(x0 + 1, y0), p(x0, y0 + 1), p(x0 + 1, y0 + 1));
        Some(std::array::from_fn(|k| (a[k] * (1.0 - fx) + b[k] * fx) * (1.0 - fy) + (c[k] * (1.0 - fx) + d[k] * fx) * fy))
    }
}

fn luma(p: [f32; 3]) -> f32 {
    0.2126 * p[0] + 0.7152 * p[1] + 0.0722 * p[2]
}

pub fn untouched(document: &Document) -> Document {
    let mut plain = Document::new(document.source.path.clone());
    plain.colour_profile = document.colour_profile.clone();
    plain
}

fn plane_of(frame: &numa_render::Frame<u16>) -> Plane {
    Plane {
        width: frame.width() as usize,
        height: frame.height() as usize,
        data: frame.pixels().map(|p| p.0.map(|v| v as f32 / 65535.0)).collect(),
    }
}

impl Views {

    pub fn of(document: &Document) -> Result<Views, String> {
        let path = Path::new(&document.source.path);
        let jpeg = raw::camera_jpeg(path, EDGE).ok_or("This raw carries no camera JPEG")?;
        let jpeg = jpeg.thumbnail(EDGE, EDGE).into_rgb8();
        let camera = Plane {
            width: jpeg.width() as usize,
            height: jpeg.height() as usize,
            data: jpeg.pixels().map(|p| p.0.map(|v| v as f32 / 255.0)).collect(),
        };
        let (proxy, full) = raw::proxy_from_mosaic(path, EDGE)?;
        let plain = untouched(document);
        let scale = proxy.width.max(proxy.height) as f32 / full.0.max(full.1) as f32;
        let ours = plane_of(&numa_render::develop16_at(&plain, &proxy, &crate::inputs::render_inputs(&plain), scale));
        Views::line_up_first(ours, camera)
    }

    fn line_up_first(ours: Plane, camera: Plane) -> Result<Views, String> {
        let (w, h) = (camera.width, camera.height);
        if w < 64 || h < 64 || (w > h) != (ours.width > ours.height) {
            return Err("The camera's JPEG is not this frame's shape".into());
        }

        let (ow, oh) = (ours.width as f32, ours.height as f32);
        let step = (ow / w as f32).min(oh / h as f32);
        let origin = [(ow - step * w as f32) / 2.0, (oh - step * h as f32) / 2.0];

        let luma_of_ours = Plane { width: ours.width, height: ours.height, data: ours.data.iter().map(|p| [luma(*p); 3]).collect() };
        let inner = |x: usize, y: usize| {
            let (mx, my) = ((w as f32 * MARGIN) as usize, (h as f32 * MARGIN) as usize);
            x >= mx && y >= my && x < w - mx && y < h - my
        };
        let points = |every: usize| -> Vec<(usize, usize, f32)> {
            (0..h)
                .step_by(every)
                .flat_map(|y| (0..w).step_by(every).map(move |x| (x, y)))
                .filter(|(x, y)| inner(*x, *y))
                .map(|(x, y)| (x, y, luma(camera.data[y * w + x])))
                .collect()
        };
        let correlation = |points: &[(usize, usize, f32)], scale: f32, shift: [f32; 2]| {
            let mapping = Mapping { origin, step, size: [w, h], scale, shift };
            let mut sums = [0.0f64; 6];
            for (x, y, theirs) in points {
                let Some(mine) = luma_of_ours.read(mapping.at(*x as f32, *y as f32)) else { continue };
                let (a, b) = (mine[0] as f64, *theirs as f64);
                for (sum, v) in sums.iter_mut().zip([1.0, a, b, a * b, a * a, b * b]) {
                    *sum += v;
                }
            }
            let [n, sa, sb, sab, saa, sbb] = sums;
            let cov = sab - sa * sb / n;
            let spread = ((saa - sa * sa / n) * (sbb - sb * sb / n)).sqrt();
            if n < 100.0 || spread <= 0.0 { -1.0 } else { (cov / spread) as f32 }
        };
        let best = |points: &[(usize, usize, f32)], candidates: Vec<(f32, [f32; 2])>| {
            candidates
                .into_par_iter()
                .map(|(scale, shift)| (correlation(points, scale, shift), scale, shift))
                .reduce(|| (-2.0, 1.0, [0.0; 2]), |a, b| if b.0 > a.0 { b } else { a })
        };

        let coarse = (-6..=10)
            .flat_map(|s| (-4..=4).flat_map(move |dy| (-4..=4).map(move |dx| (1.0 + s as f32 * 0.01, [dx as f32 * 2.0, dy as f32 * 2.0]))))
            .collect();
        let (_, scale, shift) = best(&points(6), coarse);
        let fine = (-4..=4)
            .flat_map(|s| {
                (-4..=4).flat_map(move |dy| (-4..=4).map(move |dx| (scale + s as f32 * 0.0025, [shift[0] + dx as f32 * 0.5, shift[1] + dy as f32 * 0.5])))
            })
            .collect();
        let points = points(3);
        let (_, scale, shift) = best(&points, fine);
        let finer = (-2..=2)
            .flat_map(|dy| (-2..=2).map(move |dx| (scale, [shift[0] + dx as f32 * 0.125, shift[1] + dy as f32 * 0.125])))
            .collect();
        let (correlation, scale, shift) = best(&points, finer);
        if !(correlation >= LEAST_CORRELATION) {
            return Err("The camera's JPEG does not match this frame".into());
        }
        let mapping = Mapping { origin, step, size: [w, h], scale, shift };
        let mut views = Views { width: w, height: h, ours: Vec::new(), camera: camera.data, usable: Vec::new(), scale, shift, correlation, mapping };
        let (lined, inside) = views.line_up_plane(&ours);
        views.usable = (0..w * h).map(|i| inside[i] && inner(i % w, i / w)).collect();
        views.ours = lined;
        Ok(views)
    }

    fn line_up_plane(&self, ours: &Plane) -> (Vec<[f32; 3]>, Vec<bool>) {
        let read: Vec<Option<[f32; 3]>> = (0..self.width * self.height)
            .into_par_iter()
            .map(|i| ours.read(self.mapping.at((i % self.width) as f32, (i / self.width) as f32)))
            .collect();
        (read.iter().map(|p| p.unwrap_or_default()).collect(), read.iter().map(Option::is_some).collect())
    }

    pub fn line_up(&self, frame: &numa_render::Frame<u16>) -> Vec<[f32; 3]> {
        self.line_up_plane(&plane_of(frame)).0
    }

    pub fn clipped(&self, i: usize) -> bool {
        self.camera[i].iter().any(|v| *v >= CLIPPED)
    }

    pub fn pairs(&self, block: usize, keep: impl Fn(usize, usize) -> bool) -> Vec<([f32; 3], [f32; 3])> {
        let mut pairs = Vec::new();
        for by in (0..self.height - block + 1).step_by(block) {
            for bx in (0..self.width - block + 1).step_by(block) {
                if !keep(bx, by) {
                    continue;
                }
                let indices = (by..by + block).flat_map(|y| (bx..bx + block).map(move |x| y * self.width + x));
                let mut sums = ([0.0f32; 3], [0.0f32; 3]);
                let mut whole = true;
                for i in indices {
                    if !self.usable[i] || self.clipped(i) {
                        whole = false;
                        break;
                    }
                    for k in 0..3 {
                        sums.0[k] += self.ours[i][k];
                        sums.1[k] += self.camera[i][k];
                    }
                }
                if whole {
                    let n = (block * block) as f32;
                    pairs.push((sums.0.map(|v| v / n), sums.1.map(|v| v / n)));
                }
            }
        }
        pairs
    }
}

pub fn fit(document: &Document) -> Result<LookFit, String> {
    let views = Views::of(document)?;
    numa_core::camera_look::fit(&views.pairs(BLOCK, |_, _| true)).ok_or_else(|| "The camera's JPEG is too small or too clipped to follow".into())
}

fn kept_fit(document: &Document) -> Result<LookFit, String> {
    type Kept = HashMap<(String, Option<String>), Result<LookFit, String>>;
    static KEPT: Mutex<Option<Kept>> = Mutex::new(None);
    let key = (document.source.path.clone(), document.colour_profile.clone());
    if let Some(found) = KEPT.lock().unwrap_or_else(|p| p.into_inner()).get_or_insert_with(HashMap::new).get(&key) {
        return found.clone();
    }
    let found = fit(document);
    KEPT.lock().unwrap_or_else(|p| p.into_inner()).get_or_insert_with(HashMap::new).insert(key, found.clone());
    found
}

pub fn fill(document: &mut Document) -> Result<(), String> {
    if !document.camera_look.as_ref().is_some_and(|look| look.fit.is_none()) {
        return Ok(());
    }
    let fit = kept_fit(document)?;
    if let Some(look) = document.camera_look.as_mut() {
        look.fit = Some(fit);
    }
    Ok(())
}

pub fn describe(path: &Path) -> Result<String, String> {
    if !raw::is_raw(path) {
        return Err("Only for raw photos".into());
    }
    Ok(fujifilm_recipe(path).unwrap_or_else(|| "The camera's own rendering".into()))
}

fn fujifilm_recipe(path: &Path) -> Option<String> {
    use std::io::Read;
    if !raw::is_raf(path) {
        return None;
    }
    let mut bytes = Vec::new();
    std::fs::File::open(path).ok()?.take(2 * 1024 * 1024).read_to_end(&mut bytes).ok()?;
    let tag = |id: u16| raw::makernote_tag(&bytes, id);
    let one = |id: u16| tag(id).and_then(|v| v.first().copied());
    let signed = |v: f32| match v {
        v if v.abs() < 1e-3 => None,
        v if v > 0.0 => Some(format!("+{}", trim(v))),
        v => Some(format!("\u{2212}{}", trim(-v))),
    };

    let colour = one(0x1003).map(|v| v as u32);
    let monochrome = match colour {
        Some(0x300..=0x30F) => Some("Monochrome"),
        Some(0x310) => Some("Sepia"),
        Some(0x500..=0x5FF) => Some("Acros"),
        _ => None,
    };
    let mut parts: Vec<String> = Vec::new();
    if let Some(film) = monochrome.or_else(|| raw::film_mode(path)) {
        parts.push(film.to_string());
    }
    let colour = match colour {
        Some(0x080) => Some(1.0),
        Some(0x100) => Some(2.0),
        Some(0x0C0) => Some(3.0),
        Some(0x0E0) => Some(4.0),
        Some(0x180) => Some(-1.0),
        Some(0x400) => Some(-2.0),
        Some(0x4C0) => Some(-3.0),
        Some(0x4E0) => Some(-4.0),
        _ => None,
    };
    for (name, value) in [
        ("Colour", colour),
        ("Highlights", one(0x1041).map(|v| -v / 16.0)),
        ("Shadows", one(0x1040).map(|v| -v / 16.0)),
    ] {
        if let Some(text) = value.and_then(signed) {
            parts.push(format!("{name} {text}"));
        }
    }
    if let Some(range) = one(0x1403).or_else(|| one(0x140B)).filter(|range| *range > 100.0) {
        parts.push(format!("DR{range}"));
    }
    if let Some(shift) = tag(0x100A).filter(|v| v.len() >= 2 && (v[0] != 0.0 || v[1] != 0.0)) {
        let step = |v: f32| signed(v / 20.0).unwrap_or_else(|| "0".into());
        parts.push(format!("WB R{} B{}", step(shift[0]), step(shift[1])));
    }
    if let Some(text) = one(0x100F).map(|v| v / 1000.0).and_then(signed) {
        parts.push(format!("Clarity {text}"));
    }
    (!parts.is_empty()).then(|| parts.join(" · "))
}

fn trim(v: f32) -> String {
    let text = format!("{v:.1}");
    text.trim_end_matches(".0").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_moved_frame_is_found_and_matches_itself() {
        let (w, h) = (300usize, 200usize);
        let scene = |x: f32, y: f32| {
            let v = 0.5 + 0.3 * (x * 0.13).sin() * (y * 0.07).cos();
            [v, (v * 0.8 + 0.1 * (x * 0.05).cos()).clamp(0.0, 1.0), (1.0 - v) * 0.7]
        };
        let ours = Plane { width: w, height: h, data: (0..w * h).map(|i| scene((i % w) as f32, (i / w) as f32)).collect() };
        let camera = Plane { width: w, height: h, data: (0..w * h).map(|i| scene((i % w) as f32 + 2.0, (i / w) as f32 - 3.0)).collect() };
        let views = Views::line_up_first(ours, camera).unwrap();
        assert!((views.shift[0] - 2.0).abs() < 0.3 && (views.shift[1] + 3.0).abs() < 0.3, "{:?} at {}", views.shift, views.scale);
        assert!(views.correlation > 0.99);
        let fit = numa_core::camera_look::fit(&views.pairs(BLOCK, |_, _| true)).unwrap();
        for p in [[0.2, 0.4, 0.3], [0.7, 0.6, 0.2], [0.5, 0.5, 0.5]] {
            let got = fit.apply(p);
            assert!((0..3).all(|k| (got[k] - p[k]).abs() < 0.02), "{p:?} → {got:?}");
        }
    }

    #[test]
    fn a_jpeg_is_not_followed() {
        assert_eq!(describe(Path::new("/nowhere/photo.jpg")), Err("Only for raw photos".to_string()));
    }
}
