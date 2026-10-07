use std::collections::BTreeMap;

use numa_core::color::{WhiteBalance, MAX_KELVIN, MIN_KELVIN};
use numa_core::document::{Basic, Document};
use numa_core::lut::LutChoice;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::presets::{at_strength, Preset};
use crate::workflows::{Did, Part, EVEN_MOST};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Layers {

    pub even: bool,

    pub tone: [f32; 6],
    pub vibrance: f32,
    pub noise: f32,

    pub balance: Option<(f32, f32)>,

    pub frames: BTreeMap<i64, Frame>,

    pub matched: Option<i64>,

    pub keys: Vec<Key>,

    pub look: Option<Look>,
    pub look_on: bool,

    pub numa: Option<Did>,
    pub was: Option<Was>,

    pub apart: Vec<i64>,
}

impl Default for Layers {
    fn default() -> Self {
        Self {
            even: true,
            tone: [0.0; 6],
            vibrance: 0.0,
            noise: 0.0,
            balance: None,
            frames: BTreeMap::new(),
            matched: None,
            keys: Vec::new(),
            look: None,
            look_on: true,
            numa: None,
            was: None,
            apart: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Frame {

    pub at: i64,

    pub stops: f32,

    pub balance: Option<(f32, f32)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Key {
    pub id: i64,

    pub stops: f32,

    pub balance: (f32, f32),
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Was {
    pub tone: [f32; 6],
    pub balance: Option<(f32, f32)>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Look {

    pub name: String,

    #[serde(default)]
    pub preset: Option<Preset>,

    pub strength: f32,
}

impl Look {

    pub fn label(&self) -> &str {
        self.name.rsplit('/').next().unwrap_or(&self.name)
    }

    fn alone(&self, neutral: &Document) -> Document {
        match &self.preset {
            Some(preset) => at_strength(neutral, preset, self.strength / 100.0),
            None => {
                let mut document = neutral.clone();
                document.lut = (self.strength > 0.0).then(|| LutChoice { name: self.name.clone(), amount: self.strength });
                document
            }
        }
    }

    fn apply(&self, document: &mut Document, on: bool) {
        let neutral = Document::new(document.source.path.clone());
        let looked = self.alone(&neutral);
        let mut basic = shifted(&document.basic(), &neutral.basic(), &looked.basic(), if on { 1.0 } else { -1.0 });
        if on {
            clamp(&mut basic);
        }
        document.set_basic(basic);
        if let Some(grading) = own_or(document.grading(), neutral.grading(), looked.grading(), on) {
            document.set_grading(grading);
        }
        if let Some(mixer) = own_or(document.mixer(), neutral.mixer(), looked.mixer(), on) {
            document.set_mixer(mixer);
        }
        if let Some(points) = own_or(document.point_colours(), neutral.point_colours(), looked.point_colours(), on) {
            document.set_point_colours(points);
        }
        if let Some(curves) = own_or(document.curves(), neutral.curves(), looked.curves(), on) {
            document.set_curves(curves);
        }
        if let Some(lut) = own_or(document.lut.clone(), neutral.lut.clone(), looked.lut.clone(), on) {
            document.lut = lut;
        }
        if let Some(simulation) = own_or(document.film_simulation.clone(), neutral.film_simulation.clone(), looked.film_simulation.clone(), on) {
            document.film_simulation = simulation;
        }
    }
}

fn own_or<T: Serialize>(now: T, rest: T, looked: T, on: bool) -> Option<T> {
    let json = |part: &T| serde_json::to_value(part).ok();
    let (now_json, rest_json, looked_json) = (json(&now), json(&rest), json(&looked));
    if looked_json == rest_json {
        return None;
    }
    match on {
        true => (now_json == rest_json).then_some(looked),
        false => (now_json == looked_json).then_some(rest),
    }
}

fn shifted<T: Serialize + DeserializeOwned + Clone>(base: &T, from: &T, to: &T, sign: f64) -> T {
    let (Ok(b), Ok(f), Ok(t)) = (serde_json::to_value(base), serde_json::to_value(from), serde_json::to_value(to)) else { return base.clone() };
    serde_json::from_value(shift(&b, &f, &t, sign)).unwrap_or_else(|_| base.clone())
}

fn shift(base: &Value, from: &Value, to: &Value, sign: f64) -> Value {
    match (base, from, to) {
        (Value::Number(b), Value::Number(f), Value::Number(t)) if b.is_f64() || f.is_f64() || t.is_f64() => {
            let (b, f, t) = (b.as_f64().unwrap_or(0.0), f.as_f64().unwrap_or(0.0), t.as_f64().unwrap_or(0.0));
            if t == f {
                return base.clone();
            }

            let out = b + sign * (t - f);
            let out = if (out - f).abs() < 1e-4 { f } else { out };
            serde_json::Number::from_f64(out).map_or_else(|| base.clone(), Value::Number)
        }
        (Value::Object(b), Value::Object(f), Value::Object(t)) => Value::Object(
            b.iter()
                .map(|(key, value)| {
                    let moved = match (f.get(key), t.get(key)) {
                        (Some(f), Some(t)) => shift(value, f, t, sign),
                        _ => value.clone(),
                    };
                    (key.clone(), moved)
                })
                .collect(),
        ),
        (Value::Array(b), Value::Array(f), Value::Array(t)) if b.len() == f.len() && f.len() == t.len() => {
            Value::Array(b.iter().zip(f).zip(t).map(|((b, f), t)| shift(b, f, t, sign)).collect())
        }
        _ => base.clone(),
    }
}

fn clamp(basic: &mut Basic) {
    let tone = &mut basic.tone;
    tone.exposure = tone.exposure.clamp(-5.0, 5.0);
    for value in [&mut tone.contrast, &mut tone.highlights, &mut tone.shadows, &mut tone.whites, &mut tone.blacks, &mut basic.presence.vibrance, &mut basic.presence.saturation] {
        *value = value.clamp(-100.0, 100.0);
    }
    basic.detail.denoise_luma = basic.detail.denoise_luma.clamp(0.0, 100.0);
}

fn near(a: Option<(f32, f32)>, b: Option<(f32, f32)>) -> bool {
    match (a, b) {
        (Some(a), Some(b)) => (a.0 - b.0).abs() < 0.5 && (a.1 - b.1).abs() < 0.05,
        (None, None) => true,
        _ => false,
    }
}

fn mired(kelvin: f32) -> f32 {
    1e6 / kelvin.max(1.0)
}

fn balance((temperature, tint): (f32, f32)) -> WhiteBalance {
    WhiteBalance { temperature, tint }
}

impl Layers {

    pub fn compose(&self, id: i64, own: &Document) -> Document {
        let mut document = own.clone();
        if self.apart.contains(&id) {
            return document;
        }
        if self.even {
            let (stops, shown) = self.even_of(id);
            let mut basic = document.basic();
            self.add(&mut basic, stops, 1.0);
            clamp(&mut basic);
            document.set_basic(basic);

            if own.white_balance.is_none() {
                document.white_balance = shown.map(balance);
            }
        }
        if let (true, Some(look)) = (self.look_on, &self.look) {
            look.apply(&mut document, true);
        }
        document
    }

    pub fn decompose(&self, id: i64, document: &Document) -> Document {
        let mut own = document.clone();
        if self.apart.contains(&id) {
            return own;
        }
        if let (true, Some(look)) = (self.look_on, &self.look) {
            look.apply(&mut own, false);
        }
        if self.even {
            let (stops, shown) = self.even_of(id);
            let mut basic = own.basic();
            self.add(&mut basic, stops, -1.0);
            snap(&mut basic);
            own.set_basic(basic);
            if near(own.white_balance.map(|wb| (wb.temperature, wb.tint)), shown) {
                own.white_balance = None;
            }
        }
        own
    }

    pub fn even_of(&self, id: i64) -> (f32, Option<(f32, f32)>) {
        let frame = self.frames.get(&id).copied().unwrap_or_default();
        let (ramp, ramped) = self.ramp(frame.at).map_or((0.0, None), |(stops, balance)| (stops, Some(balance)));
        let evening = if self.evening() { frame.stops } else { 0.0 };
        (evening + ramp, ramped.or(frame.balance).or(self.balance))
    }

    fn add(&self, basic: &mut Basic, stops: f32, sign: f32) {
        let tone = &mut basic.tone;
        for (value, by) in [
            (&mut tone.exposure, self.tone[0] + stops),
            (&mut tone.contrast, self.tone[1]),
            (&mut tone.highlights, self.tone[2]),
            (&mut tone.shadows, self.tone[3]),
            (&mut tone.whites, self.tone[4]),
            (&mut tone.blacks, self.tone[5]),
            (&mut basic.presence.vibrance, self.vibrance),
            (&mut basic.detail.denoise_luma, self.noise),
        ] {
            *value += sign * by;
        }
    }

    pub fn evening(&self) -> bool {
        self.numa.as_ref().is_none_or(|did| !did.off.contains(&Part::Even))
    }

    pub fn ramp(&self, at: i64) -> Option<(f32, (f32, f32))> {
        let mut keys: Vec<(i64, &Key)> = self.keys.iter().filter_map(|key| Some((self.frames.get(&key.id)?.at, key))).collect();
        keys.sort_by_key(|(at, _)| *at);
        let (first, last) = (keys.first()?, keys.last()?);
        let (a, b) = match keys.windows(2).find(|pair| at >= pair[0].0 && at <= pair[1].0) {
            _ if at <= first.0 => (first, first),
            _ if at >= last.0 => (last, last),
            Some(pair) => (&pair[0], &pair[1]),
            None => (last, last),
        };
        let t = if b.0 > a.0 { (at - a.0) as f32 / (b.0 - a.0) as f32 } else { 0.0 };
        let (ka, kb) = (a.1, b.1);
        let temperature = 1e6 / (mired(ka.balance.0) + (mired(kb.balance.0) - mired(ka.balance.0)) * t);
        Some((ka.stops + (kb.stops - ka.stops) * t, (temperature, ka.balance.1 + (kb.balance.1 - ka.balance.1) * t)))
    }

    pub fn link(&mut self, id: i64, at: i64) {
        self.frames.entry(id).or_default().at = at;
    }

    pub fn has(&self, part: Part) -> bool {
        match part {
            Part::Light => self.numa.as_ref().is_some_and(|did| did.light.is_some() || did.warmth.is_some()),
            Part::Even => self.frames.values().any(|frame| frame.stops != 0.0),
            _ => false,
        }
    }

    pub fn is_on(&self, part: Part) -> bool {
        self.has(part) && self.numa.as_ref().is_none_or(|did| !did.off.contains(&part))
    }

    pub fn put_numa(&mut self, light: Option<[f32; 6]>, warmth: Option<(f32, f32)>) {
        self.was = Some(Was { tone: self.tone, balance: self.balance });
        if let Some(light) = light {
            self.tone = light;
        }
        if warmth.is_some() {
            self.balance = warmth;
        }
        self.numa = Some(Did { light, warmth, ..Did::default() });
    }

    pub fn switch(&mut self, part: Part, on: bool) {
        let Some(did) = self.numa.as_mut() else {

            if part == Part::Even {
                self.numa = Some(Did { off: if on { vec![] } else { vec![Part::Even] }, ..Did::default() });
            }
            return;
        };
        did.off.retain(|off| *off != part);
        if !on {
            did.off.push(part);
        }
        let (light, warmth) = (did.light, did.warmth);
        if part != Part::Light {
            return;
        }
        let was = self.was.unwrap_or_default();
        if let Some(light) = light {
            for (at, now) in self.tone.iter_mut().enumerate() {
                let (from, to) = if on { (was.tone[at], light[at]) } else { (light[at], was.tone[at]) };
                if (*now - from).abs() < 1e-4 {
                    *now = to;
                }
            }
        }
        if let Some(warmth) = warmth {
            let (from, to) = if on { (was.balance, Some(warmth)) } else { (Some(warmth), was.balance) };
            if near(self.balance, from) {
                self.balance = to;
            }
        }
    }

    pub fn without_numa(&mut self) {
        self.switch(Part::Light, false);
        self.numa = None;
        self.was = None;
        for frame in self.frames.values_mut() {
            frame.stops = 0.0;
        }
    }
}

fn snap(basic: &mut Basic) {
    let rest = Basic::default();
    let tone = &mut basic.tone;
    for (value, rest) in [
        (&mut tone.exposure, rest.tone.exposure),
        (&mut tone.contrast, rest.tone.contrast),
        (&mut tone.highlights, rest.tone.highlights),
        (&mut tone.shadows, rest.tone.shadows),
        (&mut tone.whites, rest.tone.whites),
        (&mut tone.blacks, rest.tone.blacks),
        (&mut basic.presence.vibrance, rest.presence.vibrance),
        (&mut basic.detail.denoise_luma, rest.detail.denoise_luma),
    ] {
        if (*value - rest).abs() < 1e-4 {
            *value = rest;
        }
    }
}

fn sliders(basic: &Basic) -> [f32; 8] {
    let tone = &basic.tone;
    [tone.exposure, tone.contrast, tone.highlights, tone.shadows, tone.whites, tone.blacks, basic.presence.vibrance, basic.detail.denoise_luma]
}

fn most_common(values: &[f32]) -> f32 {
    let mut best = (0, values.first().copied().unwrap_or(0.0));
    for value in values {
        let count = values.iter().filter(|other| (*other - value).abs() < 1e-3).count();
        if count > best.0 {
            best = (count, *value);
        }
    }
    best.1
}

pub fn adopt(frames: &[(i64, i64, Document, Option<Did>)]) -> (Layers, Vec<Document>) {
    let mut layers = Layers::default();
    let mut owns: Vec<Document> = frames.iter().map(|(_, _, document, _)| document.clone()).collect();
    let numa = frames.iter().find_map(|(_, _, _, did)| did.clone().filter(|did| did.has(Part::Light) || did.has(Part::Even)));
    for ((id, at, _, did), own) in frames.iter().zip(owns.iter_mut()) {
        let stops = did.as_ref().and_then(|did| did.even).unwrap_or(0.0);
        if did.as_ref().is_some_and(|did| did.is_on(Part::Even)) {
            let mut basic = own.basic();
            basic.tone.exposure -= stops;
            own.set_basic(basic);
        }
        layers.frames.insert(*id, Frame { at: *at, stops, balance: None });
    }
    let rest = sliders(&Basic::default());
    let shared: Vec<f32> = (0..8).map(|at| most_common(&owns.iter().map(|own| sliders(&own.basic())[at]).collect::<Vec<_>>()) - rest[at]).collect();
    for own in owns.iter_mut() {
        let mut basic = own.basic();
        let tone = &mut basic.tone;
        for (value, by) in [&mut tone.exposure, &mut tone.contrast, &mut tone.highlights, &mut tone.shadows, &mut tone.whites, &mut tone.blacks, &mut basic.presence.vibrance, &mut basic.detail.denoise_luma].into_iter().zip(&shared) {
            *value -= by;
        }
        snap(&mut basic);
        own.set_basic(basic);
    }
    layers.tone.copy_from_slice(&shared[..6]);
    (layers.vibrance, layers.noise) = (shared[6], shared[7]);
    let balances: Vec<Option<(f32, f32)>> = owns.iter().map(|own| own.white_balance.map(|wb| (wb.temperature, wb.tint))).collect();
    if let Some(Some(first)) = balances.first().copied().filter(|_| balances.iter().all(|other| near(*other, balances[0]))) {
        layers.balance = Some(first);
        for own in owns.iter_mut() {
            own.white_balance = None;
        }
    }
    if let Some(did) = numa {
        let off = did.off.iter().copied().filter(|part| matches!(part, Part::Light | Part::Even)).collect();
        layers.numa = Some(Did { light: did.light, warmth: did.warmth, off, ..Did::default() });

        layers.was = Some(Was::default());
    }
    (layers, owns)
}

#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Reading {
    pub light: Option<(f32, bool)>,
    pub grey: Option<(f32, f32)>,
    pub body: Option<u64>,
}

pub fn body(xyz_to_cam: &[[f32; 3]; 3]) -> u64 {
    xyz_to_cam.iter().flatten().fold(0u64, |hash, value| hash.rotate_left(7) ^ value.to_bits() as u64)
}

pub fn matched(reference: Reading, shown: (f32, f32), frames: &[Reading]) -> Vec<(f32, Option<(f32, f32)>)> {
    let stop = |reading: Reading| reading.light.filter(|(light, flash)| !flash && *light > 0.0).map(|(light, _)| light.log2());

    let overall = |body: Option<u64>| {
        let greys: Vec<(f32, f32)> = frames.iter().filter(|frame| frame.body == body).filter_map(|frame| frame.grey).collect();
        (!greys.is_empty()).then(|| {
            let count = greys.len() as f32;
            (greys.iter().map(|grey| mired(grey.0)).sum::<f32>() / count, greys.iter().map(|grey| grey.1).sum::<f32>() / count)
        })
    };
    let reference_camera = overall(reference.body).or(reference.grey.map(|grey| (mired(grey.0), grey.1)));
    frames
        .iter()
        .map(|frame| {
            let stops = match (stop(reference), stop(*frame)) {
                (Some(r), Some(f)) if (r - f).abs() >= 0.05 && (r - f).abs() <= EVEN_MOST => r - f,
                _ => 0.0,
            };
            let balance = match (frame.body, overall(frame.body), reference_camera) {
                (None, ..) => None,
                _ if frame.body == reference.body => Some(shown),
                (_, Some(camera), Some(reference_camera)) => {
                    Some(((1e6 / (mired(shown.0) + camera.0 - reference_camera.0)).clamp(MIN_KELVIN, MAX_KELVIN), shown.1 + camera.1 - reference_camera.1))
                }
                _ => None,
            };
            (stops, balance)
        })
        .collect()
}

const NEAR_GREY: f32 = 0.2;

pub fn average(pixels: &[f32], clip: f32, shot: [f32; 3]) -> Option<[f32; 3]> {
    let (mut all, mut grey) = (([0.0f64; 3], 0usize), ([0.0f64; 3], 0usize));
    let add = |(sum, count): &mut ([f64; 3], usize), pixel: &[f32]| {
        for (total, value) in sum.iter_mut().zip(pixel) {
            *total += *value as f64;
        }
        *count += 1;
    };
    for pixel in pixels.chunks_exact(3) {
        if pixel.iter().any(|value| *value >= clip * 0.95 || *value <= clip * 0.01) {
            continue;
        }
        add(&mut all, pixel);
        let balanced = [pixel[0] * shot[0], pixel[1] * shot[1], pixel[2] * shot[2]];
        let (high, low) = (balanced[0].max(balanced[1]).max(balanced[2]), balanced[0].min(balanced[1]).min(balanced[2]));
        if high > 0.0 && (high - low) / high < NEAR_GREY {
            add(&mut grey, pixel);
        }
    }
    let mean = |(sum, count): ([f64; 3], usize)| sum.map(|total| (total / count as f64) as f32);
    match (grey.1 >= 64 && grey.1 * 50 >= all.1, all.1 >= 64) {
        (true, _) => Some(mean(grey)),
        (false, true) => Some(mean(all)),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use numa_core::document::EditParts;

    fn with(exposure: f32, shadows: f32) -> Document {
        let mut document = Document::new("a.RAF".to_string());
        let mut basic = document.basic();
        basic.tone.exposure = exposure;
        basic.tone.shadows = shadows;
        document.set_basic(basic);
        document
    }

    fn warm_look(strength: f32) -> Look {
        let mut document = Document::new(String::new());
        let mut basic = document.basic();
        basic.tone.shadows = 8.0;
        basic.presence.vibrance = 10.0;
        document.set_basic(basic);
        let mut grading = document.grading();
        grading.global.hue = 40.0;
        grading.global.saturation = 28.0;
        document.set_grading(grading);
        let parts = EditParts { white_balance: false, tone: true, colour: true, curve: false, detail: false, geometry: false };
        Look { name: "Numa/Warm".to_string(), preset: Some(Preset { parts, document }), strength }
    }

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-4
    }

    #[test]
    fn a_frame_is_its_own_edits_between_even_and_look() {
        let mut layers = Layers { tone: [0.3, 0.0, -20.0, 0.0, 0.0, 0.0], balance: Some((5200.0, 4.0)), look: Some(warm_look(100.0)), ..Layers::default() };
        layers.frames.insert(1, Frame { at: 0, stops: 0.5, balance: None });

        let own = with(0.2, 12.0);
        let shown = layers.compose(1, &own);
        let basic = shown.basic();
        assert!(close(basic.tone.exposure, 1.0), "own, the moment's and the evening: {}", basic.tone.exposure);
        assert!(close(basic.tone.shadows, 20.0), "own and the look's +8: {}", basic.tone.shadows);
        assert!(close(basic.tone.highlights, -20.0));
        assert!(close(basic.presence.vibrance, 10.0));
        assert_eq!(shown.white_balance, Some(WhiteBalance { temperature: 5200.0, tint: 4.0 }));
        assert_eq!(shown.grading().global.saturation, 28.0, "the look's grade, where the frame has none");

        let back = layers.decompose(1, &shown);
        assert!(close(back.basic().tone.exposure, 0.2) && close(back.basic().tone.shadows, 12.0));
        assert_eq!(back.white_balance, None);
        assert_eq!(back.grading().global.saturation, 0.0);

        let mut edited = shown.clone();
        let mut basic = edited.basic();
        basic.tone.exposure += 0.5;
        edited.set_basic(basic);
        assert!(close(layers.decompose(1, &edited).basic().tone.exposure, 0.7));
    }

    #[test]
    fn a_moment_changed_is_every_frame_changed_and_its_own_stay() {
        let mut layers = Layers::default();
        let frames = [with(0.0, 0.0), with(0.4, 0.0)];
        layers.tone[0] = 0.5;
        let lit: Vec<f32> = frames.iter().enumerate().map(|(id, own)| layers.compose(id as i64, own).basic().tone.exposure).collect();
        assert!(close(lit[0], 0.5) && close(lit[1], 0.9));
        layers.look = Some(warm_look(50.0));
        let half = layers.compose(1, &frames[1]).basic();
        assert!(close(half.tone.exposure, 0.9), "a look keeps the frame's exposure");
        assert!(close(half.tone.shadows, 4.0), "half the look's shadows");
        layers.look_on = false;
        assert_eq!(layers.compose(1, &frames[1]).basic().tone.shadows, 0.0, "the look switched off");
        layers.even = false;
        assert!(close(layers.compose(1, &frames[1]).basic().tone.exposure, 0.4), "Even switched off: the frame's own");
    }

    #[test]
    fn a_grade_of_the_frames_own_stays_under_the_look() {
        let layers = Layers { look: Some(warm_look(100.0)), ..Layers::default() };
        let mut own = Document::new("a.RAF".to_string());
        let mut grading = own.grading();
        grading.shadows.hue = 200.0;
        grading.shadows.saturation = 30.0;
        own.set_grading(grading);
        let shown = layers.compose(1, &own);
        assert_eq!(shown.grading().shadows.saturation, 30.0, "this photo's grade");
        assert_eq!(layers.decompose(1, &shown).grading().shadows.hue, 200.0);
    }

    #[test]
    fn a_frame_apart_and_a_lut_look() {
        let mut layers = Layers { tone: [1.0, 0.0, 0.0, 0.0, 0.0, 0.0], look: Some(Look { name: "Kodachrome".to_string(), preset: None, strength: 60.0 }), ..Layers::default() };
        let own = with(0.0, 0.0);
        assert_eq!(layers.compose(3, &own).lut, Some(LutChoice { name: "Kodachrome".to_string(), amount: 60.0 }));
        layers.apart.push(3);
        let alone = layers.compose(3, &own);
        assert!(alone.lut.is_none() && alone.basic().tone.exposure == 0.0, "detached, the frame's own only");
    }

    #[test]
    fn numas_light_switches_only_what_is_still_numas() {
        let mut layers = Layers::default();
        layers.put_numa(Some([0.4, 10.0, -20.0, 0.0, 0.0, 0.0]), Some((4800.0, 2.0)));
        assert_eq!(layers.tone[2], -20.0);
        layers.tone[1] = 25.0;
        layers.switch(Part::Light, false);
        assert_eq!((layers.tone[0], layers.tone[1], layers.tone[2]), (0.0, 25.0, 0.0), "moved since, so kept");
        assert_eq!(layers.balance, None);
        layers.switch(Part::Light, true);
        assert_eq!((layers.tone[0], layers.tone[2]), (0.4, -20.0));
        assert!(layers.is_on(Part::Light) && !layers.has(Part::Even));
        layers.frames.insert(1, Frame { at: 0, stops: 1.0, balance: None });
        layers.switch(Part::Even, false);
        assert_eq!(layers.even_of(1).0, 0.0, "the evening off");
        layers.without_numa();
        assert_eq!(layers.tone[0], 0.0);
        assert!(!layers.has(Part::Even) && layers.numa.is_none());
    }

    #[test]
    fn a_moment_from_before_layers_renders_the_same() {

        let did = |even: f32| Some(Did { light: Some([0.4, 0.0, 0.0, 0.0, 0.0, 0.0]), even: Some(even), ..Did::default() });
        let mut frames: Vec<(i64, i64, Document, Option<Did>)> = vec![(1, 100, with(1.4, 0.0), did(1.0)), (2, 110, with(-0.1, 0.0), did(-0.5)), (3, 120, with(0.7, 0.0), did(0.0)), (4, 130, with(0.4, 0.0), did(0.0))];
        for (_, _, document, _) in frames.iter_mut() {
            document.white_balance = Some(WhiteBalance { temperature: 5600.0, tint: 3.0 });
        }
        let (layers, owns) = adopt(&frames);
        for ((id, _, before, _), own) in frames.iter().zip(&owns) {
            let after = layers.compose(*id, own);
            assert!(close(after.basic().tone.exposure, before.basic().tone.exposure), "{id}: {} against {}", after.basic().tone.exposure, before.basic().tone.exposure);
            assert_eq!(after.white_balance, before.white_balance);
        }
        assert!(close(layers.tone[0], 0.4), "the light every frame shares is the moment's");
        assert_eq!(layers.balance, Some((5600.0, 3.0)));
        assert!(close(owns[2].basic().tone.exposure, 0.3), "the third keeps its own");
        assert!(owns[0].white_balance.is_none() && owns[3].basic().is_identity());
        assert!(layers.is_on(Part::Light) && layers.is_on(Part::Even));
    }

    #[test]
    fn key_frames_ramp_the_light_by_time() {
        let mut layers = Layers::default();
        for (id, at) in [(1, 0), (2, 50), (3, 100), (4, 200)] {
            layers.link(id, at);
        }
        layers.keys = vec![Key { id: 3, stops: 1.0, balance: (3000.0, 10.0) }, Key { id: 1, stops: 0.0, balance: (6000.0, 0.0) }];
        let (stops, (kelvin, tint)) = layers.ramp(50).unwrap();
        assert!(close(stops, 0.5) && close(tint, 5.0));

        assert!((kelvin - 4000.0).abs() < 1.0, "{kelvin}");
        assert_eq!(layers.ramp(200), Some((1.0, (3000.0, 10.0))), "after the last key, the last");
        assert_eq!(layers.even_of(4).1, Some((3000.0, 10.0)));
        layers.keys.clear();
        assert_eq!(layers.ramp(50), None);
    }

    #[test]
    fn matching_gives_one_camera_one_balance_and_moves_another_by_its_own() {

        let reference = Reading { light: Some((8.0, false)), grey: Some((5000.0, 0.0)), body: Some(1) };
        let same = Reading { light: Some((4.0, false)), grey: Some((3000.0, 20.0)), body: Some(1) };
        let other = Reading { light: Some((8.0, false)), grey: Some((4600.0, 15.0)), body: Some(2) };
        let flash = Reading { light: Some((1.0, true)), grey: None, body: None };
        let out = matched(reference, (4000.0, 2.0), &[reference, same, other, flash]);
        assert!(close(out[0].0, 0.0) && near(out[0].1, Some((4000.0, 2.0))), "the reference stays as shown");
        assert!(close(out[1].0, 1.0), "a stop less light, a stop more");
        assert!(near(out[1].1, Some((4000.0, 2.0))), "its own camera: its balance, whatever the frame shows");

        let first = (mired(5000.0) + mired(3000.0)) / 2.0;
        let (kelvin, tint) = out[2].1.unwrap();
        assert!((mired(kelvin) - mired(4000.0) - (mired(4600.0) - first)).abs() < 0.5, "{kelvin}");
        assert!(close(tint, 2.0 + 15.0 - 10.0));
        assert_eq!(out[3], (0.0, None), "the flash frame and an unread one are left");
    }

    #[test]
    fn the_lights_colour_is_read_from_what_is_near_grey() {

        let shot = [2.0, 1.0, 2.0];
        let mut pixels = [0.2f32, 0.4, 0.2].repeat(100);
        pixels.extend([0.45, 0.2, 0.05].repeat(400));
        pixels.extend([1.0, 1.0, 1.0].repeat(100));
        pixels.extend([0.0, 0.0, 0.0].repeat(100));
        let light = average(&pixels, 1.0, shot).unwrap();
        assert!(close(light[0], 0.2) && close(light[1], 0.4) && close(light[2], 0.2), "the flowers, the clipped and the black left out: {light:?}");

        let flowers = average(&[0.45f32, 0.2, 0.05].repeat(100), 1.0, shot).unwrap();
        assert!(close(flowers[0], 0.45));
        assert_eq!(average(&[0.5; 30], 1.0, shot), None, "too little to read");
    }
}
