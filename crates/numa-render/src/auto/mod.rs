use numa_core::curve;
use numa_core::document::{AutoRecord, Basic, Document};
use numa_core::mask::{Mask, Shape};
use numa_core::mask::Alpha;
use numa_core::image::LinearImage;
use numa_core::tone::{self, MIDDLE_GREY};

use super::ToneCurve;

const MOST_EXPOSURE: f32 = 0.75;

const BLOWN: f32 = 0.988;
const DIM: f32 = 0.74;

const BLACK_POINT: f32 = 0.035;
const DEEPEST_RENDERED: f32 = 0.09;
const WHITE_POINT: f32 = 0.94;

const CLOSE_ENOUGH: f32 = 0.012;

const WORTH_MOVING: f32 = 0.006;

const SUBJECT_LIT: f32 = 0.72;
const SUBJECT_DIM: f32 = 0.55;

const MOST_SUBJECT_EXPOSURE: f32 = 2.0;
const MOST_FINISHED_SUBJECT_EXPOSURE: f32 = 1.0;
const MOST_SUBJECT_DOWN: f32 = 1.0;
const LEAST_SUBJECT_EXPOSURE: f32 = 0.33;

const SUBJECT_SHARE: f32 = 0.02;

pub fn faces_in(frame: &image::RgbImage) -> Vec<[f32; 4]> {
    let (w, h) = (frame.width() as f32, frame.height() as f32);
    numa_cull::faces::detect(frame)
        .unwrap_or_default()
        .iter()
        .map(|face| [face.x / w, face.y / h, face.width / w, face.height / h])
        .collect()
}

pub struct Lit {
    pub alpha: Alpha,
    pub faces: usize,
}

pub fn lit_part(subject: &Alpha, faces: &[[f32; 4]], animal: bool, focus: Option<[f32; 2]>) -> Option<Lit> {
    let (width, height) = (subject.width, subject.height);
    if subject.data.iter().sum::<f32>() < subject.data.len() as f32 * SUBJECT_SHARE {
        return None;
    }
    let at = |u: f32, v: f32| {
        let (x, y) = (((u * width as f32) as usize).min(width - 1), ((v * height as f32) as usize).min(height - 1));
        subject.data[y * width + x]
    };
    if let Some([u, v]) = focus {

        let near = (-2..=2).flat_map(|i| (-2..=2).map(move |j| (i as f32 * 0.01, j as f32 * 0.01)));
        if !near.into_iter().any(|(du, dv)| at((u + du).clamp(0.0, 1.0), (v + dv).clamp(0.0, 1.0)) > 0.5) {
            return None;
        }
    }
    let mut theirs: Vec<&[f32; 4]> = faces.iter().filter(|[x, y, w, h]| at(x + w / 2.0, y + h / 2.0) > 0.5).collect();
    if let (Some([u, v]), true) = (focus, theirs.len() > 1) {
        let distance = |[x, y, w, h]: &&[f32; 4]| (x + w / 2.0 - u).hypot(y + h / 2.0 - v);
        theirs.sort_by(|a, b| distance(a).total_cmp(&distance(b)));
        theirs.truncate(1);
    }
    if theirs.is_empty() {
        return animal.then(|| Lit { alpha: subject.clone(), faces: 0 });
    }
    let in_a_face = |px: usize, py: usize| {
        let (u, v) = ((px as f32 + 0.5) / width as f32, (py as f32 + 0.5) / height as f32);
        theirs.iter().any(|[x, y, w, h]| (*x..x + w).contains(&u) && (*y..y + h).contains(&v))
    };
    let data = (0..width * height).map(|i| if in_a_face(i % width, i / width) { subject.data[i] } else { 0.0 }).collect();
    Some(Lit { alpha: Alpha::new(width, height, data), faces: theirs.len() })
}

#[derive(Debug, Clone, Copy, Default)]
pub struct Intent {

    pub compensation: Option<f32>,

    pub headroom: bool,

    pub monochrome: bool,
}

impl Intent {

    pub fn of(compensation: Option<f32>, dynamic_range: Option<u16>, colour: Option<u16>) -> Self {
        Self {
            compensation,
            headroom: dynamic_range.is_some_and(|dr| dr > 100),
            monochrome: colour.is_some_and(|c| (0x300..=0x310).contains(&c) || (0x500..=0x503).contains(&c)),
        }
    }

    fn meant(&self) -> Option<f32> {
        self.compensation.filter(|bias| *bias <= MEANT_LOW)
    }
}

#[derive(Default)]
pub struct Told<'a> {

    pub subject: Option<&'a Alpha>,

    pub lights: Option<&'a Alpha>,

    pub faces: f32,
    pub intent: Intent,
}

pub const LIGHTS: [u16; 6] = [36, 82, 85, 87, 134, 136];

const LOW_KEY: f32 = 0.15;
const NIGHT_SKY: f32 = 0.04;

const MOST_VIBRANCE: f32 = 22.0;

const COLOUR_TARGET: f32 = 0.30;

const LOW: f32 = 0.005;
const HIGH: f32 = 0.995;

pub struct Auto {
    pub basic: Basic,

    pub subject: Option<f32>,

    pub held_low: Option<f32>,

    pub silhouette: bool,

    free: Free,
}

#[derive(Default)]
pub struct Applied {

    pub subject: Option<f32>,

    pub held_low: Option<f32>,

    pub silhouette: bool,

    pub of: Option<&'static str>,

    pub kept: Vec<&'static str>,

    pub changed: bool,
}

impl Auto {

    pub fn apply(&self, document: &mut Document) -> Applied {
        let was = document.basic();
        let mut basic = was;
        let mut record = document.auto.unwrap_or_default();
        let mut kept = Vec::new();
        let free = self.free;
        for (name, was_free, now, wanted, set_last) in [
            (Some("exposure"), free.exposure, &mut basic.tone.exposure, self.basic.tone.exposure, &mut record.exposure),
            (Some("highlights"), free.highlights, &mut basic.tone.highlights, self.basic.tone.highlights, &mut record.highlights),
            (Some("whites"), free.whites, &mut basic.tone.whites, self.basic.tone.whites, &mut record.whites),
            (Some("blacks"), free.blacks, &mut basic.tone.blacks, self.basic.tone.blacks, &mut record.blacks),
            (None, free.hdr, &mut basic.presence.hdr, self.basic.presence.hdr, &mut record.hdr),
            (Some("vibrance"), free.vibrance, &mut basic.presence.vibrance, self.basic.presence.vibrance, &mut record.vibrance),
        ] {
            if was_free && AutoRecord::may_set(*now, *set_last) {
                *now = wanted;
                *set_last = (wanted != 0.0).then_some(wanted);
            } else if *now != 0.0 {
                kept.extend(name);
            }
        }
        document.set_basic(basic);
        document.auto = (record != AutoRecord::default()).then_some(record);

        let mut masks = document.masks();
        let mine = autos(&masks);
        let mut at = 0;
        masks.retain(|_| {
            let keep = !mine[at];
            at += 1;
            keep
        });
        document.set_masks(masks);
        let changed = document.basic() != was;
        let subject = self.subject.filter(|_| free.exposure && document.basic().tone.exposure == self.basic.tone.exposure);
        Applied { subject, held_low: self.held_low, silhouette: self.silhouette, of: None, kept, changed }
    }
}

fn autos(masks: &[Mask]) -> Vec<bool> {
    let subtracted: Vec<u32> = masks.iter().flat_map(|mask| mask.minus_masks.iter().copied()).collect();
    masks
        .iter()
        .map(|mask| (mask.as_auto_left_it() || mask.from_old_auto()) && !subtracted.contains(&mask.id))
        .collect()
}

fn has_their_subject(masks: &[Mask], autos: &[bool]) -> bool {
    masks.iter().zip(autos).any(|(mask, auto)| !auto && mask.shape == Shape::Subject && !mask.inverted)
}

pub fn framed(document: &Document, working: &LinearImage) -> LinearImage {
    crate::geometry_only(&crate::mask_geometry(document), working)
}

pub fn subject_mask(
    found: &crate::segment::Segmentation,
    frame: &image::RgbImage,
    width: usize,
    height: usize,
) -> Option<Mask> {
    let classes = crate::segment::MATTEABLE.to_vec();
    if !crate::segment::named_something(&found.alpha(&classes)) {
        return None;
    }

    let mut mask = Mask::new(Shape::Subject);
    mask.matte = true;
    crate::resolve_mask(&mut mask, Some(found), None, Some(frame), width, height);
    Some(mask)
}

#[derive(Clone, Copy)]
struct Free {
    exposure: bool,
    highlights: bool,
    whites: bool,
    blacks: bool,
    hdr: bool,
    vibrance: bool,
}

impl Free {

    fn of(basic: &Basic, record: &AutoRecord) -> Self {
        let may = AutoRecord::may_set;
        Self {
            exposure: may(basic.tone.exposure, record.exposure),
            highlights: may(basic.tone.highlights, record.highlights),
            whites: may(basic.tone.whites, record.whites),
            blacks: may(basic.tone.blacks, record.blacks),
            hdr: may(basic.presence.hdr, record.hdr),
            vibrance: may(basic.presence.vibrance, record.vibrance),
        }
    }

    fn at_rest(&self, basic: &mut Basic) {
        for (free, value) in [
            (self.exposure, &mut basic.tone.exposure),
            (self.highlights, &mut basic.tone.highlights),
            (self.whites, &mut basic.tone.whites),
            (self.blacks, &mut basic.tone.blacks),
            (self.hdr, &mut basic.presence.hdr),
            (self.vibrance, &mut basic.presence.vibrance),
        ] {
            if free {
                *value = 0.0;
            }
        }
    }
}

struct Look {
    curve: Option<[f32; curve::LOOKUP]>,
    finished: bool,
}

impl Look {
    fn of(document: &Document, finished: bool) -> Self {
        let composite = document.curve();
        Self { curve: (!composite.is_identity()).then(|| composite.lookup()), finished }
    }

    fn display(&self, lit: f32, basic: &Basic) -> f32 {
        self.shown_with(lit, basic, &ToneCurve::new(basic))
    }

    fn shown_with(&self, lit: f32, basic: &Basic, curve: &ToneCurve) -> f32 {
        let contrast = basic.tone.contrast / 100.0;
        let slope = if contrast >= 0.0 { 1.0 + contrast } else { 1.0 / (1.0 - contrast) };
        let lit = match (slope - 1.0).abs() > f32::EPSILON {
            true => MIDDLE_GREY * (lit.max(0.0) / MIDDLE_GREY).powf(slope),
            false => lit,
        };
        let shown = tone::shown(lit * curve.gain(lit), self.finished);
        match &self.curve {
            Some(table) => crate::through(table, shown),
            None => shown,
        }
    }

    fn shapes_the_top(&self) -> bool {
        self.curve.as_ref().is_some_and(|table| crate::through(table, 1.0) < 0.99)
    }

    fn shapes_the_bottom(&self) -> bool {
        self.curve.as_ref().is_some_and(|table| crate::through(table, 0.0) > 0.01)
    }
}

fn changed(basic: &Basic, change: impl FnOnce(&mut Basic)) -> Basic {
    let mut changed = *basic;
    change(&mut changed);
    changed
}

pub fn tone(image: &LinearImage, subject: Option<&Alpha>, document: &Document) -> Auto {
    tone_told(image, document, &Told { subject, ..Default::default() })
}

const MEANT_LOW: f32 = -0.95;

const FLAT: f32 = 0.35;

const MOST_VIBRANCE_ON_FACES: f32 = 12.0;
const FACES: f32 = 0.02;

pub fn tone_told(image: &LinearImage, document: &Document, told: &Told) -> Auto {
    let subject = told.subject;
    let intent = told.intent;
    let held_low = intent.meant();

    let finished = image.display_referred;
    let look = Look::of(document, finished);

    let mut basic = document.basic();
    let masks = document.masks();
    let theirs = has_their_subject(&masks, &autos(&masks));
    let mut free = Free::of(&basic, &document.auto.unwrap_or_default());
    free.hdr &= !theirs;
    let top_is_theirs = !free.whites || !free.highlights || look.shapes_the_top();
    let bottom_is_theirs = !free.blacks || look.shapes_the_bottom();
    free.whites &= !top_is_theirs;
    free.highlights &= !top_is_theirs;
    free.blacks &= !bottom_is_theirs;
    free.at_rest(&mut basic);

    let Some(sorted) = luminances(image, None) else {
        return Auto { basic, subject: None, held_low: None, silhouette: false, free };
    };

    let at = |fraction: f32| {
        let index = ((sorted.len() - 1) as f32 * fraction).round() as usize;
        sorted[index.min(sorted.len() - 1)].max(1e-6)
    };

    let low_key = tone::shown(at(0.5), finished) < LOW_KEY || tone::shown(at(0.1), finished) < NIGHT_SKY;

    let top = top_end(image, told.lights, finished, low_key).unwrap_or_else(|| at(HIGH));

    let inside = subject.filter(|_| !theirs).and_then(|alpha| luminances(image, Some(alpha)));
    let meant_dark = intent.meant().is_some() || warm_light(image, &sorted);
    let silhouette = inside.as_deref().is_some_and(|values| is_silhouette(values, &sorted, finished, meant_dark));
    let for_subject = inside
        .as_deref()
        .filter(|_| !silhouette)
        .and_then(|values| exposure_for_the_subject(values, &sorted, finished));

    let for_subject = for_subject.filter(|stops| held_low.is_none() || *stops < 0.0);

    let flat = tone::shown(at(0.95), finished) - tone::shown(at(0.05), finished) < FLAT;
    if free.exposure {
        let ends = exposure_from_the_ends(top, finished);
        let ends = if low_key { ends.min(0.0) } else { ends };
        let wanted = for_subject.unwrap_or(ends);
        basic.tone.exposure = if held_low.is_some() { wanted.min(0.0) } else { wanted };
    }
    let gain = 2.0f32.powf(basic.tone.exposure);

    if !top_is_theirs {
        basic.tone.whites =
            solve(|amount| look.display(top * gain, &changed(&basic, |b| b.tone.whites = amount)), WHITE_POINT);

        if intent.headroom {
            basic.tone.whites = basic.tone.whites.min(0.0);
        }
    }

    if !bottom_is_theirs {
        let lit = at(LOW) * gain;
        let target = match look.display(lit, &basic) {
            crushed if crushed < BLACK_POINT && !low_key => Some(BLACK_POINT),
            lifted if lifted > DEEPEST_RENDERED && !flat => Some(DEEPEST_RENDERED),
            _ => None,
        };
        basic.tone.blacks = target.map_or(0.0, |target| {
            solve(|amount| look.display(lit, &changed(&basic, |b| b.tone.blacks = amount)), target)
        });
    }

    if finished {
        if free.whites {
            basic.tone.whites = basic.tone.whites.max(0.0);
        }
        if free.blacks {
            basic.tone.blacks = basic.tone.blacks.min(0.0);
        }
    }

    let highest = look.display(top * gain, &basic);
    if !top_is_theirs && highest > WHITE_POINT + 0.005 {
        basic.tone.highlights = solve(
            |amount| look.display(top * gain, &changed(&basic, |b| b.tone.highlights = amount)),
            WHITE_POINT,
        )
        .min(0.0);
    }

    if free.vibrance {
        let most = if told.faces > FACES { MOST_VIBRANCE_ON_FACES } else { MOST_VIBRANCE };
        basic.presence.vibrance = if intent.monochrome { 0.0 } else { vibrance(image).min(most) };
    }

    Auto { basic, subject: for_subject.filter(|_| free.exposure), held_low, silhouette: silhouette && free.exposure, free }
}

fn exposure_from_the_ends(brightest: f32, finished: bool) -> f32 {
    let wanted = match tone::shown(brightest, finished) {
        below if below < DIM => Some(tone::scene_for(WHITE_POINT, finished)),
        _ => None,
    };
    wanted.map(|target| (target / brightest).log2().clamp(-MOST_EXPOSURE, MOST_EXPOSURE)).unwrap_or(0.0)
}

pub fn key(image: &LinearImage, document: &Document) -> Option<f32> {
    let sorted = luminances(image, None)?;
    let basic = document.basic();
    let look = Look::of(document, image.display_referred);
    Some(look.display(sorted[sorted.len() / 2] * 2.0f32.powf(basic.tone.exposure), &basic))
}

pub fn match_light(image: &LinearImage, document: &Document, reference: &Basic, wanted: f32) -> Auto {
    let mut basic = document.basic();
    let free = Free::of(&basic, &document.auto.unwrap_or_default());
    free.at_rest(&mut basic);
    for (free, now, theirs) in [
        (free.highlights, &mut basic.tone.highlights, reference.tone.highlights),
        (free.whites, &mut basic.tone.whites, reference.tone.whites),
        (free.blacks, &mut basic.tone.blacks, reference.tone.blacks),
        (free.vibrance, &mut basic.presence.vibrance, reference.presence.vibrance),
    ] {
        if free {
            *now = theirs;
        }
    }
    let look = Look::of(document, image.display_referred);
    if let (true, Some(sorted)) = (free.exposure, luminances(image, None)) {
        let middle = sorted[sorted.len() / 2];
        let shown = |stops: f32| look.display(middle * 2.0f32.powf(stops), &changed(&basic, |b| b.tone.exposure = stops));
        let (mut low, mut high) = (-MOST_SUBJECT_EXPOSURE, MOST_SUBJECT_EXPOSURE);
        for _ in 0..30 {
            let middle = (low + high) / 2.0;
            if shown(middle) < wanted { low = middle } else { high = middle }
        }
        basic.tone.exposure = ((low + high) / 2.0 * 100.0).round() / 100.0;
    }
    Auto { basic, subject: None, held_low: None, silhouette: false, free }
}

fn exposure_for_the_subject(values: &[f32], frame: &[f32], finished: bool) -> Option<f32> {
    let at = |fraction: f32| values[((values.len() - 1) as f32 * fraction) as usize].max(1e-6);
    let (lit, top) = (at(0.9), at(0.98));

    let around = frame[((frame.len() - 1) as f32 * 0.75) as usize].max(1e-6);
    let in_shadow = lit * 2.0 <= around;
    if !finished && tone::shown(top, finished) > BLOWN {
        let down = (tone::scene_for(WHITE_POINT, finished) / top).log2();
        return (down <= -LEAST_SUBJECT_EXPOSURE).then(|| down.max(-MOST_SUBJECT_DOWN));
    }
    let most = if finished { MOST_FINISHED_SUBJECT_EXPOSURE } else { MOST_SUBJECT_EXPOSURE };

    let headroom = (tone::scene_for(WHITE_POINT, finished) / top).log2();
    let up = (tone::scene_for(SUBJECT_LIT, finished) / lit).log2().min(headroom);
    (in_shadow && tone::shown(lit, finished) < SUBJECT_DIM && up >= LEAST_SUBJECT_EXPOSURE).then(|| up.min(most))
}

const SILHOUETTE: f32 = 0.05;

const WARM: f32 = 1.5;

fn is_silhouette(values: &[f32], frame: &[f32], finished: bool, meant: bool) -> bool {
    let middle = values[values.len() / 2].max(1e-6);
    let around = frame[((frame.len() - 1) as f32 * 0.75) as usize];
    meant && tone::shown(middle, finished) < SILHOUETTE && around >= middle * 8.0
}

fn warm_light(image: &LinearImage, sorted: &[f32]) -> bool {
    let bright = sorted[(sorted.len() - 1) * 3 / 4];
    let step = (image.pixel_count() / 50_000).max(1);
    let (mut red, mut blue) = (0.0f64, 0.0f64);
    for p in image.data.chunks_exact(3).step_by(step) {
        if 0.2126 * p[0] + 0.7152 * p[1] + 0.0722 * p[2] >= bright {
            red += p[0] as f64;
            blue += p[2] as f64;
        }
    }
    blue > 0.0 && red / blue >= WARM as f64
}

const POINT_LIGHT: f32 = 0.0005;

const LIGHT_BRIGHT: f32 = 0.95;

const LIGHTS_EDGE: u32 = 256;

fn top_end(image: &LinearImage, lights: Option<&Alpha>, finished: bool, night: bool) -> Option<f32> {
    let small = image.downscaled(LIGHTS_EDGE);
    let small = small.as_ref().unwrap_or(image);
    let (w, h) = (small.width as usize, small.height as usize);
    let luma: Vec<f32> = small.data.chunks_exact(3).map(|p| 0.2126 * p[0] + 0.7152 * p[1] + 0.0722 * p[2]).collect();
    let mut keep: Vec<f32> = (0..w * h)
        .map(|i| {
            let named = lights.is_some_and(|alpha| {
                let ax = ((i % w) * alpha.width / w).min(alpha.width - 1);
                let ay = ((i / w) * alpha.height / h).min(alpha.height - 1);
                alpha.data[ay * alpha.width + ax] > 0.5
            });
            if named { 0.0 } else { 1.0 }
        })
        .collect();
    let bright = |i: usize| tone::shown(luma[i], finished) > LIGHT_BRIGHT;
    let mut seen = vec![false; w * h];
    for start in (0..w * h).filter(|_| night) {
        if seen[start] || !bright(start) {
            continue;
        }
        seen[start] = true;
        let (mut stack, mut spot) = (vec![start], Vec::new());
        while let Some(i) = stack.pop() {
            spot.push(i);
            let (x, y) = (i % w, i / w);
            for (nx, ny) in [(x.wrapping_sub(1), y), (x + 1, y), (x, y.wrapping_sub(1)), (x, y + 1)] {
                if nx < w && ny < h && !seen[ny * w + nx] && bright(ny * w + nx) {
                    seen[ny * w + nx] = true;
                    stack.push(ny * w + nx);
                }
            }
        }
        if (spot.len() as f32) < POINT_LIGHT * (w * h) as f32 {
            spot.into_iter().for_each(|i| keep[i] = 0.0);
        }
    }
    if keep.iter().all(|k| *k > 0.5) {
        return None;
    }
    let values = luminances(image, Some(&Alpha::new(w, h, keep)))?;
    Some(values[((values.len() - 1) as f32 * HIGH).round() as usize].max(1e-6))
}

fn vibrance(image: &LinearImage) -> f32 {
    let colour = colourfulness(image);
    (((COLOUR_TARGET - colour) / COLOUR_TARGET) * 100.0).clamp(0.0, MOST_VIBRANCE).round()
}

fn colourfulness(image: &LinearImage) -> f32 {
    let pixels = image.pixel_count();
    if pixels == 0 {
        return COLOUR_TARGET;
    }
    let step = (pixels / 50_000).max(1);
    let mut values: Vec<f32> = image
        .data
        .chunks_exact(3)
        .step_by(step)
        .filter_map(|pixel| {
            let high = pixel[0].max(pixel[1]).max(pixel[2]);
            let low = pixel[0].min(pixel[1]).min(pixel[2]);

            (high > 0.004).then(|| ((high - low) / high).clamp(0.0, 1.0))
        })
        .collect();

    if values.len() < 64 {
        return COLOUR_TARGET;
    }
    values.sort_by(f32::total_cmp);
    values[values.len() / 2]
}

fn solve(measure: impl Fn(f32) -> f32, wanted: f32) -> f32 {
    let resting = measure(0.0);
    if (resting - wanted).abs() < CLOSE_ENOUGH {
        return 0.0;
    }

    let (mut low, mut high) = (-100.0f32, 100.0f32);
    let amount = if measure(low) > wanted {
        low
    } else if measure(high) < wanted {
        high
    } else {
        for _ in 0..16 {
            let middle = (low + high) / 2.0;
            if measure(middle) < wanted {
                low = middle;
            } else {
                high = middle;
            }
        }
        (low + high) / 2.0
    };

    if (measure(amount) - resting).abs() < WORTH_MOVING || amount.abs() < 2.0 {
        return 0.0;
    }

    if amount.abs() >= 100.0 && (measure(amount) - wanted).abs() >= CLOSE_ENOUGH {
        return 0.0;
    }
    amount
}

fn luminances(image: &LinearImage, only: Option<&Alpha>) -> Option<Vec<f32>> {
    let pixels = image.pixel_count();
    if pixels == 0 {
        return None;
    }

    let step = (pixels / 100_000).max(1);
    let (width, height) = (image.width as usize, image.height as usize);

    let mut values: Vec<f32> = image
        .data
        .chunks_exact(3)
        .enumerate()
        .step_by(step)
        .filter(|(index, _)| match only {

            Some(alpha) => {
                let (x, y) = (index % width, index / width);
                let ax = (x * alpha.width / width.max(1)).min(alpha.width.saturating_sub(1));
                let ay = (y * alpha.height / height.max(1)).min(alpha.height.saturating_sub(1));
                alpha.data.get(ay * alpha.width + ax).copied().unwrap_or(0.0) > 0.6
            }
            None => true,
        })
        .map(|(_, pixel)| 0.2126 * pixel[0] + 0.7152 * pixel[1] + 0.0722 * pixel[2])
        .filter(|value| value.is_finite())
        .collect();

    if values.len() < 64 {
        return None;
    }
    values.sort_by(f32::total_cmp);
    Some(values)
}

pub mod corners;
pub mod crop;
pub mod evidence;
mod level;
pub mod plan;
pub mod scene;
pub mod sea;

#[cfg(test)]
mod tests;
