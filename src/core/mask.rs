use std::sync::Arc;

use rayon::prelude::*;
use serde::{Deserialize, Serialize};

use crate::core::document::Basic;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Shape {

    Linear { from: [f32; 2], to: [f32; 2] },

    Radial {
        centre: [f32; 2],
        radius: [f32; 2],

        feather: f32,
    },

    Segment { classes: Vec<u16> },

    ColourRange {
        hue: f32,
        spread: f32,
        saturation: f32,

        #[serde(default)]
        picked: bool,
    },

    LuminanceRange {
        low: f32,
        high: f32,
        softness: f32,
        #[serde(default)]
        picked: bool,
    },

    Painted,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Stroke {

    pub points: Vec<[f32; 2]>,

    pub radius: f32,

    pub feather: f32,

    pub erase: bool,

    #[serde(default)]
    pub fill: bool,

    #[serde(default = "shown")]
    pub enabled: bool,
}

impl Stroke {
    pub fn new(radius: f32, feather: f32, erase: bool) -> Self {
        Self { points: Vec::new(), radius, feather, erase, fill: false, enabled: true }
    }

    pub fn lasso(erase: bool) -> Self {
        Self { points: Vec::new(), radius: 0.0, feather: 0.0, erase, fill: true, enabled: true }
    }

    pub fn draw(&self, alpha: &mut Alpha) {
        if self.fill {
            self.fill_outline(alpha);
            return;
        }
        for pair in self.points.windows(2) {
            self.draw_segment(alpha, pair[0], pair[1]);
        }

        if self.points.len() == 1 {
            self.draw_segment(alpha, self.points[0], self.points[0]);
        }
    }

    fn fill_outline(&self, alpha: &mut Alpha) {
        if self.points.len() < 3 || alpha.width == 0 || alpha.height == 0 {
            return;
        }
        let (width, height) = (alpha.width, alpha.height);
        let corners: Vec<[f32; 2]> = self
            .points
            .iter()
            .map(|p| [p[0] * width as f32, p[1] * height as f32])
            .collect();

        let top = corners.iter().map(|p| p[1]).fold(f32::MAX, f32::min).floor().max(0.0) as usize;
        let bottom = (corners.iter().map(|p| p[1]).fold(f32::MIN, f32::max).ceil())
            .min(height as f32 - 1.0)
            .max(0.0) as usize;

        let mut coverage = vec![0.0f32; width];
        for y in top..=bottom {
            coverage.iter_mut().for_each(|value| *value = 0.0);

            for sub in 0..2 {
                let scan = y as f32 + 0.25 + 0.5 * sub as f32;
                let mut crossings: Vec<f32> = Vec::new();
                for pair in 0..corners.len() {
                    let a = corners[pair];
                    let b = corners[(pair + 1) % corners.len()];

                    if (a[1] <= scan) != (b[1] <= scan) {
                        let t = (scan - a[1]) / (b[1] - a[1]);
                        crossings.push(a[0] + (b[0] - a[0]) * t);
                    }
                }
                crossings.sort_by(f32::total_cmp);

                for span in crossings.chunks_exact(2) {
                    let (from, to) = (span[0].max(0.0), span[1].min(width as f32));
                    if to <= from {
                        continue;
                    }
                    for x in from.floor() as usize..(to.ceil() as usize).min(width) {
                        let overlap = (to.min(x as f32 + 1.0) - from.max(x as f32)).clamp(0.0, 1.0);
                        coverage[x] += overlap * 0.5;
                    }
                }
            }

            for x in 0..width {
                let covered = coverage[x].clamp(0.0, 1.0);
                if covered <= 0.0 {
                    continue;
                }
                let value = &mut alpha.data[y * width + x];
                *value = if self.erase {
                    *value * (1.0 - covered)
                } else {
                    *value + (1.0 - *value) * covered
                };
            }
        }
    }

    pub fn draw_segment(&self, alpha: &mut Alpha, from: [f32; 2], to: [f32; 2]) {
        if alpha.width == 0 || alpha.height == 0 {
            return;
        }
        let long = alpha.width.max(alpha.height) as f32;
        let radius = (self.radius * long).max(0.5);
        let (width, height) = (alpha.width as f32, alpha.height as f32);

        let a = [from[0] * width, from[1] * height];
        let b = [to[0] * width, to[1] * height];

        let left = ((a[0].min(b[0]) - radius).floor().max(0.0)) as usize;
        let right = ((a[0].max(b[0]) + radius).ceil().min(width - 1.0)).max(0.0) as usize;
        let top = ((a[1].min(b[1]) - radius).floor().max(0.0)) as usize;
        let bottom = ((a[1].max(b[1]) + radius).ceil().min(height - 1.0)).max(0.0) as usize;

        let inner = radius * (1.0 - self.feather.clamp(0.0, 1.0));
        for y in top..=bottom {
            for x in left..=right {
                let distance = distance_to_segment([x as f32 + 0.5, y as f32 + 0.5], a, b);
                if distance >= radius {
                    continue;
                }
                let coverage = if distance <= inner {
                    1.0
                } else {
                    let t = ((distance - inner) / (radius - inner).max(1e-6)).clamp(0.0, 1.0);
                    1.0 - t * t * (3.0 - 2.0 * t)
                };

                let value = &mut alpha.data[y * alpha.width + x];
                *value = if self.erase {
                    *value * (1.0 - coverage)
                } else {

                    *value + (1.0 - *value) * coverage
                };
            }
        }
    }
}

fn distance_to_segment(point: [f32; 2], a: [f32; 2], b: [f32; 2]) -> f32 {
    let axis = [b[0] - a[0], b[1] - a[1]];
    let length = axis[0] * axis[0] + axis[1] * axis[1];
    let along = if length <= f32::EPSILON {
        0.0
    } else {
        (((point[0] - a[0]) * axis[0] + (point[1] - a[1]) * axis[1]) / length).clamp(0.0, 1.0)
    };
    let nearest = [a[0] + axis[0] * along, a[1] + axis[1] * along];
    let (dx, dy) = (point[0] - nearest[0], point[1] - nearest[1]);
    (dx * dx + dy * dy).sqrt()
}

#[derive(Debug, Clone, PartialEq)]
pub struct Alpha {
    pub width: usize,
    pub height: usize,
    pub data: Vec<f32>,
}

impl Alpha {
    pub fn new(width: usize, height: usize, data: Vec<f32>) -> Self {
        debug_assert_eq!(data.len(), width * height);
        Self { width, height, data }
    }

    pub fn sample(&self, u: f32, v: f32) -> f32 {
        if self.width == 0 || self.height == 0 {
            return 0.0;
        }
        let x = (u * self.width as f32 - 0.5).clamp(0.0, self.width as f32 - 1.0);
        let y = (v * self.height as f32 - 0.5).clamp(0.0, self.height as f32 - 1.0);
        let (x0, y0) = (x.floor() as usize, y.floor() as usize);
        let (x1, y1) = ((x0 + 1).min(self.width - 1), (y0 + 1).min(self.height - 1));
        let (fx, fy) = (x - x0 as f32, y - y0 as f32);

        let at = |x: usize, y: usize| self.data[y * self.width + x];
        let top = at(x0, y0) + (at(x1, y0) - at(x0, y0)) * fx;
        let bottom = at(x0, y1) + (at(x1, y1) - at(x0, y1)) * fx;
        top + (bottom - top) * fy
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct RegionPoint {
    pub at: [f32; 2],

    pub subtract: bool,

    #[serde(default = "shown")]
    pub enabled: bool,
}

#[derive(Debug, Clone, Default)]
pub struct Pixels(pub Option<Arc<Alpha>>);

impl PartialEq for Pixels {
    fn eq(&self, _: &Self) -> bool {
        true
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Mask {
    pub shape: Shape,

    #[serde(default)]
    pub basic: Basic,

    #[serde(default)]
    pub inverted: bool,

    #[serde(default)]
    pub points: Vec<RegionPoint>,

    #[serde(default)]
    pub strokes: Vec<Stroke>,

    #[serde(default = "shown")]
    pub visible: bool,

    #[serde(default)]
    pub matte: bool,

    #[serde(default)]
    pub matte_edge: f32,

    #[serde(default = "some_feather")]
    pub feather: f32,

    #[serde(default)]
    pub shift: f32,

    #[serde(default = "fully")]
    pub opacity: f32,

    #[serde(default)]
    pub name: Option<String>,

    #[serde(default)]
    pub muted: Vec<u16>,

    #[serde(skip)]
    pub map: Pixels,

    #[serde(skip)]
    pub unshaped: Pixels,
}

impl Shape {

    pub fn linear() -> Self {
        Shape::Linear { from: [0.5, 0.45], to: [0.5, 0.05] }
    }

    pub fn radial() -> Self {
        Shape::Radial { centre: [0.5, 0.5], radius: [0.28, 0.28], feather: 0.5 }
    }

    pub fn colour_range() -> Self {
        Shape::ColourRange { hue: 30.0, spread: 30.0, saturation: 0.2, picked: false }
    }

    pub fn luminance_range() -> Self {
        Shape::LuminanceRange { low: 0.3, high: 0.7, softness: 0.35, picked: false }
    }

    pub fn weight(&self, u: f32, v: f32) -> f32 {
        match self {
            Shape::Linear { from, to } => {
                let axis = [to[0] - from[0], to[1] - from[1]];
                let length = axis[0] * axis[0] + axis[1] * axis[1];

                if length <= f32::EPSILON {
                    return 1.0;
                }
                let along = ((u - from[0]) * axis[0] + (v - from[1]) * axis[1]) / length;
                along.clamp(0.0, 1.0)
            }

            Shape::Segment { .. }
            | Shape::Painted
            | Shape::ColourRange { .. }
            | Shape::LuminanceRange { .. } => 0.0,
            Shape::Radial { centre, radius, feather } => {
                let safe = |r: f32| if r.abs() < 1e-4 { 1e-4 } else { r.abs() };
                let dx = (u - centre[0]) / safe(radius[0]);
                let dy = (v - centre[1]) / safe(radius[1]);
                let distance = (dx * dx + dy * dy).sqrt();

                let inner = 1.0 - feather.clamp(0.0, 1.0);
                if distance <= inner {
                    return 1.0;
                }
                if distance >= 1.0 {
                    return 0.0;
                }
                let t = (distance - inner) / (1.0 - inner);

                let t = t.clamp(0.0, 1.0);
                1.0 - t * t * (3.0 - 2.0 * t)
            }
        }
    }
}

fn shown() -> bool {
    true
}

fn fully() -> f32 {
    1.0
}

const FEATHER_REACH: f32 = 0.11;

const SHIFT_REACH: f32 = 0.035;

fn some_feather() -> f32 {
    12.0
}

impl Mask {
    pub fn new(shape: Shape) -> Self {
        Self {
            shape,
            basic: Basic::default(),
            inverted: false,
            matte: false,
            matte_edge: 0.0,
            feather: some_feather(),
            shift: 0.0,
            muted: Vec::new(),
            visible: shown(),
            opacity: fully(),
            name: None,
            points: Vec::new(),
            strokes: Vec::new(),
            map: Pixels::default(),
            unshaped: Pixels::default(),
        }
    }

    pub fn wants_pixels(&self) -> bool {
        !self.strokes.is_empty()
            || !self.points.is_empty()
            || matches!(
                self.shape,
                Shape::Segment { .. }
                    | Shape::Painted
                    | Shape::ColourRange { .. }
                    | Shape::LuminanceRange { .. }
            )
    }

    pub fn is_pending(&self) -> bool {
        self.wants_pixels() && self.map.0.is_none()
    }

    pub fn rasterise(
        &self,
        found: Option<&Alpha>,
        regions: &[(Alpha, bool)],
        width: usize,
        height: usize,
    ) -> Alpha {
        let mut alpha = Alpha::new(width, height, vec![0.0; width * height]);

        match (&self.shape, found) {
            (Shape::Segment { .. }, Some(base)) => {
                for y in 0..height {
                    let v = (y as f32 + 0.5) / height as f32;
                    for x in 0..width {
                        let u = (x as f32 + 0.5) / width as f32;
                        alpha.data[y * width + x] = base.sample(u, v);
                    }
                }
            }

            (Shape::Segment { .. }, None) | (Shape::Painted, _) => {}
            (shape, _) => {
                for y in 0..height {
                    let v = (y as f32 + 0.5) / height as f32;
                    for x in 0..width {
                        let u = (x as f32 + 0.5) / width as f32;
                        alpha.data[y * width + x] = shape.weight(u, v);
                    }
                }
            }
        }

        for (region, subtract) in regions {
            for index in 0..alpha.data.len() {
                let (x, y) = (index % width, index / width);
                let covered = region.sample(
                    (x as f32 + 0.5) / width as f32,
                    (y as f32 + 0.5) / height as f32,
                );
                let value = &mut alpha.data[index];
                *value = if *subtract {
                    *value * (1.0 - covered)
                } else {
                    *value + (1.0 - *value) * covered
                };
            }
        }

        for stroke in self.strokes.iter().filter(|stroke| stroke.enabled) {
            stroke.draw(&mut alpha);
        }

        alpha
    }

    pub fn reshape_edge(&mut self, width: usize, height: usize) -> bool {
        let Some(unshaped) = self.unshaped.0.as_ref() else {
            return false;
        };
        if unshaped.width != width || unshaped.height != height {
            return false;
        }
        let mut alpha = Alpha::new(width, height, unshaped.data.clone());
        self.shape_the_edge(&mut alpha);
        self.map = Pixels(Some(Arc::new(alpha)));
        true
    }

    pub fn shape_the_edge(&self, alpha: &mut Alpha) {
        let searched = if self.matte { self.matte_edge } else { 0.0 };
        shape_edge(alpha, self.feather, self.shift - searched);
    }

    pub fn edge_to_search_from(&self, alpha: &mut Alpha) {
        shape_edge(alpha, 0.0, self.matte_edge);
    }
}

fn shape_edge(alpha: &mut Alpha, feather_setting: f32, shift_setting: f32) {
    let feather = feather_setting.clamp(0.0, 100.0) / 100.0;
    let shift = shift_setting.clamp(-100.0, 100.0) / 100.0;
    if feather <= 0.0 && shift.abs() < 1e-4 {
        return;
    }

    let long_edge = alpha.width.max(alpha.height) as f32;
    let reach = (feather * feather).max(shift.abs() * SHIFT_REACH / FEATHER_REACH);
    let radius = (reach * FEATHER_REACH * long_edge) as usize;
    if radius >= 1 {
        let plane = crate::core::plane::blur(
            &crate::core::plane::Plane::new(
                alpha.width,
                alpha.height,
                std::mem::take(&mut alpha.data),
            ),
            radius,
        );
        alpha.data = plane.data;
    }

    let steepness = (24.0 / (1.0 + feather_setting.clamp(0.0, 100.0) * 0.45)).max(1.0);

    let target = 0.5 + shift * 0.45;
    let bend = 1.0 / target - 2.0;

    for value in alpha.data.iter_mut() {
        let moved = *value / (bend * (1.0 - *value) + 1.0);
        let t = ((moved - 0.5) * steepness + 0.5).clamp(0.0, 1.0);
        *value = t * t * (3.0 - 2.0 * t);
    }
}

impl Mask {
    pub fn weight(&self, u: f32, v: f32) -> f32 {

        if self.is_pending() {
            return 0.0;
        }

        if !self.visible {
            return 0.0;
        }

        let weight = match &self.map.0 {
            Some(alpha) if self.wants_pixels() => alpha.sample(u, v),
            _ => self.shape.weight(u, v),
        };
        let weight = if self.inverted { 1.0 - weight } else { weight };

        weight * self.opacity.clamp(0.0, 1.0)
    }

    pub fn is_idle(&self) -> bool {
        self.basic.is_identity()
    }

    pub fn field(&self, width: usize, height: usize, region: [f32; 4]) -> Vec<f32> {
        let mut field = vec![0.0f32; width * height];
        if width == 0 || height == 0 {
            return field;
        }

        field.par_chunks_mut(width).enumerate().for_each(|(y, row)| {

            let v = region[1] + (y as f32 + 0.5) / height as f32 * region[3];
            for (x, cell) in row.iter_mut().enumerate() {
                let u = region[0] + (x as f32 + 0.5) / width as f32 * region[2];
                *cell = self.weight(u, v);
            }
        });

        field
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore]
    fn measure_feather_at_full_size() {
        let (w, h) = (2048usize, 1365usize);
        let hard: Vec<f32> = (0..w * h).map(|i| if i % w < w / 2 { 1.0 } else { 0.0 }).collect();
        for (feather, shift) in
            [(0.0, 0.0), (12.0, 0.0), (25.0, 0.0), (50.0, 0.0), (100.0, 0.0), (0.0, 100.0), (0.0, -100.0)]
        {
            let mut alpha = Alpha::new(w, h, hard.clone());
            let mut mask = Mask::new(Shape::radial());
            mask.feather = feather;
            mask.shift = shift;
            mask.shape_the_edge(&mut alpha);
            let row = h / 2;
            let band = (0..w).filter(|x| (0.02..0.98).contains(&alpha.data[row * w + x])).count();
            let border = (0..w).find(|x| alpha.data[row * w + x] < 0.5).unwrap_or(w);
            println!("feather {feather:5.0} shift {shift:5.0}: overgang {band:5} px, rand {border:5} (was 1024)");
        }
    }

    #[test]
    fn feather_widens_a_hard_edge_and_edge_moves_it() {

        let (w, h) = (1024usize, 64usize);
        let hard: Vec<f32> = (0..w * h).map(|i| if i % w < w / 2 { 1.0 } else { 0.0 }).collect();
        let shaped = |feather: f32, shift: f32| {
            let mut alpha = Alpha::new(w, h, hard.clone());
            let mut mask = Mask::new(Shape::radial());
            mask.feather = feather;
            mask.shift = shift;
            mask.shape_the_edge(&mut alpha);
            let row = h / 2;
            let band =
                (0..w).filter(|x| (0.02..0.98).contains(&alpha.data[row * w + x])).count();
            let border = (0..w).find(|x| alpha.data[row * w + x] < 0.5).unwrap_or(w);
            (band, border)
        };

        let (none, _) = shaped(0.0, 0.0);
        let (some, _) = shaped(25.0, 0.0);
        let (lots, _) = shaped(100.0, 0.0);
        assert_eq!(none, 0, "no feather is still a step");
        assert!(some >= 5, "a quarter of the way is already visible: {some}");
        assert!(lots > w / 16, "and full feather is soft: {lots} of {w}");
        assert!(lots > some * 3, "the slider keeps giving: {some} then {lots}");

        let (out_band, out_border) = shaped(0.0, 100.0);
        let (in_band, in_border) = shaped(0.0, -100.0);
        assert!(out_border > w / 2 + 8, "pushed out: {out_border}");
        assert!(in_border < w / 2 - 8, "pulled in: {in_border}");
        assert!(out_band < 4 && in_band < 4, "and still a hard edge: {out_band}, {in_band}");

        for (feather, shift) in [(100.0, 0.0), (0.0, 100.0), (100.0, 100.0)] {
            let mut alpha = Alpha::new(w, h, hard.clone());
            let mut mask = Mask::new(Shape::radial());
            mask.feather = feather;
            mask.shift = shift;
            mask.shape_the_edge(&mut alpha);
            let row = h / 2;
            assert!(alpha.data[row * w] > 0.99, "inside stays covered at {feather}/{shift}");
            assert!(alpha.data[row * w + w - 1] < 0.01, "outside stays clear at {feather}/{shift}");
        }
    }

    #[test]
    fn a_wide_feather_still_covers_nothing_outside_and_everything_inside() {
        for (feather, shift) in [(100.0, 0.0), (80.0, 0.0), (0.0, 100.0), (0.0, -100.0), (100.0, 60.0)] {
            let mut alpha = Alpha::new(3, 1, vec![0.0, 0.5, 1.0]);
            let mut mask = Mask::new(Shape::radial());
            mask.feather = feather;
            mask.shift = shift;
            mask.shape_the_edge(&mut alpha);
            assert!(alpha.data[0].abs() < 1e-5, "feather {feather} shift {shift}: outside {}", alpha.data[0]);
            assert!((alpha.data[2] - 1.0).abs() < 1e-5, "feather {feather} shift {shift}: inside {}", alpha.data[2]);
        }
    }

    #[test]
    fn feather_decides_how_wide_the_border_is() {

        let mut mask = Mask::new(Shape::Radial {
            centre: [0.5, 0.5],
            radius: [0.3, 0.3],
            feather: 0.0,
        });

        let width_of_border = |mask: &Mask| {
            let mut alpha = mask.rasterise(None, &[], 256, 256);
            mask.shape_the_edge(&mut alpha);
            alpha.data.iter().filter(|v| **v > 0.02 && **v < 0.98).count()
        };

        mask.feather = 0.0;
        let hard = width_of_border(&mask);
        mask.feather = 100.0;
        let soft = width_of_border(&mask);
        assert!(soft > hard * 2, "soft {soft} against hard {hard}");
    }

    #[test]
    fn shifting_the_edge_shrinks_and_grows_the_mask() {
        let mut mask = Mask::new(Shape::Radial {
            centre: [0.5, 0.5],
            radius: [0.3, 0.3],
            feather: 0.4,
        });

        let covered = |mask: &Mask| {
            let mut alpha = mask.rasterise(None, &[], 128, 128);
            mask.shape_the_edge(&mut alpha);
            alpha.data.iter().map(|v| *v as f64).sum::<f64>()
        };

        let middle = covered(&mask);
        mask.shift = -40.0;
        let pulled_in = covered(&mask);
        mask.shift = 40.0;
        let pushed_out = covered(&mask);

        assert!(pulled_in < middle, "in: {pulled_in} against {middle}");
        assert!(pushed_out > middle, "out: {pushed_out} against {middle}");
    }

    #[test]
    fn a_mask_saved_before_the_edge_controls_is_unchanged() {
        let mask = Mask::new(Shape::Linear { from: [0.0, 0.5], to: [1.0, 0.5] });
        let mut json: serde_json::Value = serde_json::to_value(&mask).unwrap();
        let fields = json.as_object_mut().unwrap();
        assert!(fields.remove("feather").is_some());
        assert!(fields.remove("shift").is_some());

        let older: Mask = serde_json::from_value(json).expect("an older mask still reads");
        assert_eq!(older.feather, mask.feather, "the default is what it always did");
        assert_eq!(older.shift, 0.0);
    }

    #[test]
    fn a_square_traces_to_one_closed_path() {
        let (w, h) = (64usize, 64usize);
        let mut alpha = Alpha::new(w, h, vec![0.0; w * h]);
        for y in 16..48 {
            for x in 16..48 {
                alpha.data[y * w + x] = 1.0;
            }
        }

        let paths = outline(&alpha, 64);
        assert_eq!(paths.len(), 1, "one shape, one path");
        let path = &paths[0];
        assert!(path.len() > 20, "and it is traced, not a stub: {}", path.len());

        for at in path {
            let near = (at[0] - 0.25).abs() < 0.06
                || (at[0] - 0.75).abs() < 0.06
                || (at[1] - 0.25).abs() < 0.06
                || (at[1] - 0.75).abs() < 0.06;
            assert!(near, "{at:?} is not on the square's edge");
        }

        let (first, last) = (path[0], *path.last().unwrap());
        assert!(
            (first[0] - last[0]).abs() < 0.03 && (first[1] - last[1]).abs() < 0.03,
            "the path should close: {first:?} to {last:?}"
        );
    }

    #[test]
    fn two_shapes_trace_to_two_paths() {
        let (w, h) = (64usize, 32usize);
        let mut alpha = Alpha::new(w, h, vec![0.0; w * h]);
        for y in 8..24 {
            for x in 4..20 {
                alpha.data[y * w + x] = 1.0;
            }
            for x in 44..60 {
                alpha.data[y * w + x] = 1.0;
            }
        }
        assert_eq!(outline(&alpha, 64).len(), 2);
    }

    #[test]
    fn tracing_a_real_mask_is_affordable() {
        let (w, h) = (2048usize, 1365usize);
        let mut alpha = Alpha::new(w, h, vec![0.0; w * h]);

        for y in 0..h {
            for x in 0..w {
                let dx = (x as f32 - 900.0) / 500.0;
                let dy = (y as f32 - 700.0) / 400.0;
                let wobble = ((x as f32 * 0.05).sin() + (y as f32 * 0.07).cos()) * 0.06;
                if dx * dx + dy * dy + wobble < 1.0 {
                    alpha.data[y * w + x] = 1.0;
                }
            }
        }

        let started = std::time::Instant::now();
        let paths = outline(&alpha, 640);
        let elapsed = started.elapsed();
        assert!(!paths.is_empty(), "the blob has an edge");
        assert!(elapsed.as_millis() < 120, "tracing took {elapsed:?}");
    }

    #[test]
    fn nothing_selected_has_no_outline() {
        assert!(outline(&Alpha::new(32, 32, vec![0.0; 32 * 32]), 32).is_empty());
        assert!(outline(&Alpha::new(32, 32, vec![1.0; 32 * 32]), 32).is_empty());
    }

    #[test]
    fn a_linear_gradient_ramps_along_its_own_axis() {

        let shape = Shape::Linear { from: [0.5, 0.0], to: [0.5, 1.0] };
        assert_eq!(shape.weight(0.5, 0.0), 0.0);
        assert!((shape.weight(0.5, 0.5) - 0.5).abs() < 1e-5);
        assert_eq!(shape.weight(0.5, 1.0), 1.0);

        assert_eq!(shape.weight(0.5, -2.0), 0.0);
        assert_eq!(shape.weight(0.5, 3.0), 1.0);

        assert_eq!(shape.weight(0.0, 0.5), shape.weight(1.0, 0.5));
    }

    #[test]
    fn a_gradient_with_no_length_covers_everything() {

        let shape = Shape::Linear { from: [0.5, 0.5], to: [0.5, 0.5] };
        assert_eq!(shape.weight(0.1, 0.9), 1.0);
    }

    #[test]
    fn a_radial_is_solid_inside_and_gone_outside() {
        let shape = Shape::Radial { centre: [0.5, 0.5], radius: [0.2, 0.2], feather: 0.5 };

        assert_eq!(shape.weight(0.5, 0.5), 1.0, "the middle is fully covered");
        assert_eq!(shape.weight(0.5, 0.9), 0.0, "well outside is not covered at all");

        assert_eq!(shape.weight(0.7, 0.5), 0.0);

        let mut previous = 1.1;
        for step in 0..=40 {
            let u = 0.5 + 0.2 * step as f32 / 40.0;
            let weight = shape.weight(u, 0.5);
            assert!(weight <= previous + 1e-6, "the falloff turned back at {u}");
            previous = weight;
        }
    }

    #[test]
    fn feather_widens_the_transition_without_moving_the_edge() {
        let hard = Shape::Radial { centre: [0.5, 0.5], radius: [0.3, 0.3], feather: 0.0 };
        let soft = Shape::Radial { centre: [0.5, 0.5], radius: [0.3, 0.3], feather: 1.0 };

        assert_eq!(hard.weight(0.81, 0.5), 0.0);
        assert_eq!(soft.weight(0.81, 0.5), 0.0);

        assert_eq!(hard.weight(0.65, 0.5), 1.0);
        assert!(soft.weight(0.65, 0.5) < 0.9);
    }

    #[test]
    fn a_radius_of_zero_does_not_divide_by_it() {
        let shape = Shape::Radial { centre: [0.5, 0.5], radius: [0.0, 0.0], feather: 0.5 };
        for (u, v) in [(0.5, 0.5), (0.0, 0.0), (1.0, 1.0)] {
            assert!(shape.weight(u, v).is_finite(), "at {u},{v}");
        }
    }

    #[test]
    fn inverting_is_the_other_side_of_the_same_shape() {
        let mut mask = Mask::new(Shape::Linear { from: [0.5, 0.0], to: [0.5, 1.0] });
        let before = mask.weight(0.5, 0.25);
        mask.inverted = true;
        assert!((mask.weight(0.5, 0.25) - (1.0 - before)).abs() < 1e-6);
    }

    #[test]
    fn a_tile_gets_the_same_mask_as_the_whole_frame() {
        let mask = Mask::new(Shape::Linear { from: [0.5, 0.0], to: [0.5, 1.0] });

        let whole = mask.field(40, 40, [0.0, 0.0, 1.0, 1.0]);
        let quarter = mask.field(80, 80, [0.5, 0.5, 0.5, 0.5]);

        let middle = quarter[40 * 80 + 40];
        assert!((middle - 0.75).abs() < 0.02, "the tile disagrees: {middle}");

        let same = whole[30 * 40 + 30];
        assert!((middle - same).abs() < 0.02, "{middle} against {same}");
    }

    #[test]
    fn a_found_mask_samples_its_pixels_and_covers_nothing_until_it_has_any() {
        let mut mask = Mask::new(Shape::Segment { classes: vec![2] });

        assert!(mask.is_pending());
        assert_eq!(mask.weight(0.5, 0.5), 0.0);
        mask.inverted = true;
        assert_eq!(mask.weight(0.5, 0.5), 0.0);
        mask.inverted = false;

        let mut painted = Mask::new(Shape::Painted);
        painted.inverted = true;
        assert!(painted.is_pending());
        assert_eq!(painted.weight(0.5, 0.5), 0.0, "an inverted painted mask covered the frame");

        let mut ramp = Mask::new(Shape::Linear { from: [0.5, 0.0], to: [0.5, 1.0] });
        ramp.strokes.push(Stroke::new(0.1, 0.5, true));
        assert!(ramp.is_pending());
        assert_eq!(ramp.weight(0.5, 0.9), 0.0);

        mask.map = Pixels(Some(Arc::new(Alpha::new(2, 1, vec![0.0, 1.0]))));
        assert!(!mask.is_pending());
        assert_eq!(mask.weight(0.0, 0.5), 0.0);
        assert_eq!(mask.weight(1.0, 0.5), 1.0);

        assert!((mask.weight(0.5, 0.5) - 0.5).abs() < 1e-5);

        mask.inverted = true;
        assert_eq!(mask.weight(1.0, 0.5), 0.0);
    }

    #[test]
    fn a_found_mask_is_the_same_mask_on_a_tile() {
        let mut mask = Mask::new(Shape::Segment { classes: vec![2] });
        mask.map = Pixels(Some(Arc::new(Alpha::new(
            2,
            2,
            vec![0.0, 0.0, 1.0, 1.0],
        ))));

        let whole = mask.field(20, 20, [0.0, 0.0, 1.0, 1.0]);
        let quarter = mask.field(40, 40, [0.5, 0.5, 0.5, 0.5]);
        assert!((quarter[20 * 40 + 20] - whole[15 * 20 + 15]).abs() < 0.02);
    }

    #[test]
    fn a_stroke_covers_the_line_between_its_points() {
        let mut alpha = Alpha::new(100, 100, vec![0.0; 100 * 100]);
        let mut stroke = Stroke::new(0.05, 0.5, false);
        stroke.points = vec![[0.2, 0.5], [0.8, 0.5]];
        stroke.draw(&mut alpha);

        let at = |alpha: &Alpha, u: f32, v: f32| alpha.sample(u, v);

        assert!(at(&alpha, 0.2, 0.5) > 0.9, "{}", at(&alpha, 0.2, 0.5));
        assert!(at(&alpha, 0.5, 0.5) > 0.9, "the middle of the stroke is a gap: {}", at(&alpha, 0.5, 0.5));
        assert!(at(&alpha, 0.8, 0.5) > 0.9);

        assert_eq!(at(&alpha, 0.5, 0.05), 0.0);
        assert_eq!(at(&alpha, 0.02, 0.5), 0.0);

        stroke.draw(&mut alpha);
        assert!(alpha.data.iter().all(|value| *value <= 1.0 + 1e-6));

        let mut rubber = Stroke::new(0.05, 0.5, true);
        rubber.points = vec![[0.2, 0.5], [0.8, 0.5]];
        rubber.draw(&mut alpha);
        assert!(at(&alpha, 0.5, 0.5) < 0.05, "the rubber left {}", at(&alpha, 0.5, 0.5));
    }

    #[test]
    fn painting_on_a_gradient_keeps_the_gradient_under_it() {
        let mut mask = Mask::new(Shape::Linear { from: [0.5, 0.0], to: [0.5, 1.0] });
        let mut rubber = Stroke::new(0.1, 0.5, true);
        rubber.points = vec![[0.5, 0.9]];
        mask.strokes.push(rubber);

        let alpha = mask.rasterise(None, &[], 64, 64);

        assert!((alpha.sample(0.1, 0.5) - 0.5).abs() < 0.05);

        assert!(alpha.sample(0.5, 0.9) < 0.1, "{}", alpha.sample(0.5, 0.9));
    }

    #[test]
    fn a_lasso_fills_what_it_encloses() {
        let mut alpha = Alpha::new(100, 100, vec![0.0; 100 * 100]);
        let mut lasso = Stroke::lasso(false);
        lasso.points = vec![[0.2, 0.2], [0.8, 0.2], [0.8, 0.8], [0.2, 0.8]];
        lasso.draw(&mut alpha);

        assert!(alpha.sample(0.5, 0.5) > 0.95, "the middle is not filled");
        assert_eq!(alpha.sample(0.05, 0.05), 0.0, "it leaked outside");

        let edge = alpha.data[50 * 100 + 20];
        assert!(edge > 0.0 && edge <= 1.0, "edge pixel reads {edge}");

        let mut cut = Stroke::lasso(true);
        cut.points = vec![[0.4, 0.4], [0.6, 0.4], [0.6, 0.6], [0.4, 0.6]];
        cut.draw(&mut alpha);
        assert!(alpha.sample(0.5, 0.5) < 0.05, "the hole is still filled");
        assert!(alpha.sample(0.3, 0.5) > 0.95, "it cut more than it was asked to");
    }

    #[test]
    fn a_mask_with_nothing_set_is_idle() {
        let mut mask = Mask::new(Shape::radial());
        assert!(mask.is_idle());

        mask.basic.exposure = -0.5;
        assert!(!mask.is_idle());
    }

    #[test]
    fn a_hidden_mask_covers_nothing_even_inverted() {
        let mut mask = Mask::new(Shape::Radial {
            centre: [0.5, 0.5],
            radius: [0.4, 0.4],
            feather: 0.0,
        });
        assert!(mask.weight(0.5, 0.5) > 0.9, "it covers its middle to start with");

        mask.visible = false;
        assert_eq!(mask.weight(0.5, 0.5), 0.0);

        mask.inverted = true;
        assert_eq!(mask.weight(0.0, 0.0), 0.0);
        assert_eq!(mask.weight(0.5, 0.5), 0.0);
    }

    #[test]
    fn opacity_fades_the_region_the_mask_settled_on() {
        let mut mask = Mask::new(Shape::Radial {
            centre: [0.5, 0.5],
            radius: [0.4, 0.4],
            feather: 0.0,
        });
        mask.opacity = 0.5;
        assert!((mask.weight(0.5, 0.5) - 0.5).abs() < 1e-6);
        assert_eq!(mask.weight(0.0, 0.0), 0.0);

        mask.inverted = true;

        assert!((mask.weight(0.0, 0.0) - 0.5).abs() < 1e-6);
        assert!(mask.weight(0.5, 0.5).abs() < 1e-6);
    }

    #[test]
    fn a_stack_saved_before_this_reads_back_visible_and_solid() {

        let mask = Mask::new(Shape::Linear { from: [0.5, 0.0], to: [0.5, 1.0] });
        let mut json: serde_json::Value = serde_json::to_value(&mask).unwrap();
        let fields = json.as_object_mut().unwrap();
        for added in ["visible", "opacity", "name"] {
            assert!(fields.remove(added).is_some(), "{added} should have been there to remove");
        }

        let older: Mask = serde_json::from_value(json).expect("an older mask still reads");
        assert!(older.visible);
        assert_eq!(older.opacity, 1.0);
        assert_eq!(older.name, None);
    }

    #[test]
    fn a_muted_stroke_draws_nothing_and_stays() {
        let mut mask = Mask::new(Shape::Painted);
        let mut stroke = Stroke::lasso(false);
        stroke.points = vec![[0.1, 0.1], [0.9, 0.1], [0.9, 0.9], [0.1, 0.9]];
        mask.strokes.push(stroke);

        let covered = mask.rasterise(None, &[], 32, 32);
        assert!(covered.data.iter().any(|v| *v > 0.5), "the lasso should cover something");

        mask.strokes[0].enabled = false;
        let empty = mask.rasterise(None, &[], 32, 32);
        assert!(empty.data.iter().all(|v| *v < 1e-6), "a muted stroke covers nothing");
        assert_eq!(mask.strokes.len(), 1, "and it is still there");
    }

    #[test]
    fn parts_saved_before_this_read_back_switched_on() {
        let mut mask = Mask::new(Shape::Segment { classes: vec![2] });
        mask.points.push(RegionPoint { at: [0.3, 0.3], subtract: false, enabled: true });
        mask.strokes.push(Stroke::new(0.05, 0.5, false));

        let mut json: serde_json::Value = serde_json::to_value(&mask).unwrap();
        let object = json.as_object_mut().unwrap();
        assert!(object.remove("muted").is_some());
        for list in ["points", "strokes"] {
            for item in object[list].as_array_mut().unwrap() {
                assert!(item.as_object_mut().unwrap().remove("enabled").is_some());
            }
        }

        let older: Mask = serde_json::from_value(json).expect("an older mask still reads");
        assert!(older.muted.is_empty());
        assert!(older.points[0].enabled);
        assert!(older.strokes[0].enabled);
    }
}

pub fn outline(alpha: &Alpha, long_edge: usize) -> Vec<Vec<[f32; 2]>> {
    if alpha.width < 2 || alpha.height < 2 || long_edge < 2 {
        return Vec::new();
    }

    let scale = long_edge as f32 / alpha.width.max(alpha.height) as f32;
    let width = ((alpha.width as f32 * scale).round() as usize).clamp(2, alpha.width);
    let height = ((alpha.height as f32 * scale).round() as usize).clamp(2, alpha.height);

    let at = |x: usize, y: usize| -> bool {
        let u = (x as f32 + 0.5) / width as f32;
        let v = (y as f32 + 0.5) / height as f32;
        alpha.sample(u, v) >= 0.5
    };

    let mut segments: Vec<([f32; 2], [f32; 2])> = Vec::new();
    let point = |x: f32, y: f32| [x / width as f32, y / height as f32];

    for y in 0..height - 1 {
        for x in 0..width - 1 {
            let corners =
                [at(x, y), at(x + 1, y), at(x + 1, y + 1), at(x, y + 1)];
            let case = (corners[0] as u8) | (corners[1] as u8) << 1
                | (corners[2] as u8) << 2
                | (corners[3] as u8) << 3;

            let (fx, fy) = (x as f32, y as f32);
            let top = point(fx + 1.0, fy + 0.5);
            let right = point(fx + 1.5, fy + 1.0);
            let bottom = point(fx + 1.0, fy + 1.5);
            let left = point(fx + 0.5, fy + 1.0);

            match case {
                1 | 14 => segments.push((left, top)),
                2 | 13 => segments.push((top, right)),
                3 | 12 => segments.push((left, right)),
                4 | 11 => segments.push((right, bottom)),
                5 => {
                    segments.push((left, top));
                    segments.push((right, bottom));
                }
                6 | 9 => segments.push((top, bottom)),
                7 | 8 => segments.push((left, bottom)),
                10 => {
                    segments.push((top, right));
                    segments.push((left, bottom));
                }
                _ => {}
            }
        }
    }

    chain(segments)
}

fn chain(segments: Vec<([f32; 2], [f32; 2])>) -> Vec<Vec<[f32; 2]>> {
    use std::collections::HashMap;

    fn key(at: [f32; 2]) -> (i32, i32) {
        ((at[0] * 8192.0).round() as i32, (at[1] * 8192.0).round() as i32)
    }

    let mut starts: HashMap<(i32, i32), Vec<usize>> = HashMap::new();
    for (index, (from, to)) in segments.iter().enumerate() {
        starts.entry(key(*from)).or_default().push(index);
        starts.entry(key(*to)).or_default().push(index);
    }

    let mut used = vec![false; segments.len()];
    let mut paths: Vec<Vec<[f32; 2]>> = Vec::new();

    for seed in 0..segments.len() {
        if used[seed] {
            continue;
        }
        used[seed] = true;
        let mut path = vec![segments[seed].0, segments[seed].1];

        for forward in [true, false] {
            loop {
                let tip = if forward { *path.last().unwrap() } else { path[0] };
                let Some(candidates) = starts.get(&key(tip)) else { break };
                let Some(&next) = candidates.iter().find(|index| !used[**index]) else {
                    break;
                };
                used[next] = true;
                let (from, to) = segments[next];
                let far = if key(from) == key(tip) { to } else { from };
                if forward {
                    path.push(far);
                } else {
                    path.insert(0, far);
                }
            }
        }

        if path.len() > 2 {
            paths.push(path);
        }
    }

    paths
}
