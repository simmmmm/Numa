use std::ops::Range;

use numa_core::color::WhiteBalance;
use numa_core::document::Document;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Events,
    Sports,
    Portraits,
    Property,
    Products,
    Wildlife,
    Travel,
}

impl Kind {
    pub const ALL: [Kind; 7] = [Kind::Events, Kind::Sports, Kind::Portraits, Kind::Property, Kind::Products, Kind::Wildlife, Kind::Travel];

    pub fn name(self) -> &'static str {
        match self {
            Kind::Events => "Weddings and Events",
            Kind::Sports => "Sports",
            Kind::Portraits => "Portraits",
            Kind::Property => "Property and Interiors",
            Kind::Products => "Products and Food",
            Kind::Wildlife => "Wildlife and Landscape",
            Kind::Travel => "Travel and Street",
        }
    }

    pub fn from_name(name: &str) -> Option<Kind> {
        Kind::ALL.into_iter().find(|kind| kind.name() == name)
    }

    pub fn controls(kind: Option<Kind>) -> &'static [Control] {
        use Control::*;
        match kind {
            None => &[Exposure, Warmth, Contrast, Highlights, Shadows],
            Some(Kind::Events | Kind::Property) => &[Exposure, Warmth, Highlights, Shadows],
            Some(Kind::Sports) => &[Exposure, Warmth, Contrast, Noise],
            Some(Kind::Portraits) => &[Exposure, Warmth, Shadows, Vibrance],
            Some(Kind::Products) => &[Exposure, Whites, Warmth, Contrast],
            Some(Kind::Wildlife) => &[Exposure, Highlights, Contrast, Noise],
            Some(Kind::Travel) => &[Exposure, Warmth, Contrast, Vibrance],
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Control {
    Exposure,
    Warmth,
    Contrast,
    Highlights,
    Shadows,
    Whites,
    Vibrance,
    Noise,
}

impl Control {
    pub fn name(self) -> &'static str {
        match self {
            Control::Exposure => "Exposure",
            Control::Warmth => "Warmth",
            Control::Contrast => "Contrast",
            Control::Highlights => "Highlights",
            Control::Shadows => "Shadows",
            Control::Whites => "Whites",
            Control::Vibrance => "Vibrance",
            Control::Noise => "Noise",
        }
    }

    pub fn range(self) -> (f32, f32, f32) {
        match self {
            Control::Exposure => (-4.0, 4.0, 0.0),
            Control::Warmth => (2500.0, 10000.0, 5500.0),
            Control::Noise => (0.0, 100.0, 0.0),
            _ => (-100.0, 100.0, 0.0),
        }
    }

    pub fn get(self, document: &Document, as_shot: WhiteBalance) -> f32 {
        let basic = document.basic();
        match self {
            Control::Exposure => basic.tone.exposure,
            Control::Warmth => document.white_balance.unwrap_or(as_shot).temperature,
            Control::Contrast => basic.tone.contrast,
            Control::Highlights => basic.tone.highlights,
            Control::Shadows => basic.tone.shadows,
            Control::Whites => basic.tone.whites,
            Control::Vibrance => basic.presence.vibrance,
            Control::Noise => basic.detail.denoise_luma,
        }
    }

    pub fn set(self, document: &mut Document, value: f32, as_shot: WhiteBalance) {
        if self == Control::Warmth {
            let tint = document.white_balance.unwrap_or(as_shot).tint;
            document.white_balance = Some(WhiteBalance { temperature: value, tint });
            return;
        }
        let mut basic = document.basic();
        match self {
            Control::Exposure => basic.tone.exposure = value,
            Control::Contrast => basic.tone.contrast = value,
            Control::Highlights => basic.tone.highlights = value,
            Control::Shadows => basic.tone.shadows = value,
            Control::Whites => basic.tone.whites = value,
            Control::Vibrance => basic.presence.vibrance = value,
            Control::Noise => basic.detail.denoise_luma = value,
            Control::Warmth => unreachable!(),
        }
        document.set_basic(basic);
    }

    pub fn reset(self, document: &mut Document) {
        match self {
            Control::Warmth => document.white_balance = None,
            other => other.set(document, other.range().2, WhiteBalance { temperature: 5500.0, tint: 0.0 }),
        }
    }
}

impl Kind {

    pub fn shape(kind: Option<Kind>) -> &'static [Part] {
        match kind {
            None => &[],
            Some(Kind::Events | Kind::Travel) => &[Part::Straighten],
            Some(Kind::Sports) => &[Part::Frame],

            Some(Kind::Portraits) => &[],
            Some(Kind::Property) => &[Part::Straighten, Part::Verticals],
            Some(Kind::Products) => &[Part::Frame],
            Some(Kind::Wildlife) => &[],
        }
    }

    pub fn chapters(kind: Option<Kind>) -> &'static [&'static str] {
        match kind {
            Some(Kind::Events) => &["Getting Ready", "First Look", "Ceremony", "Portraits", "Reception", "Party"],
            Some(Kind::Sports) => &["Warm-Up", "First Half", "Second Half", "Celebration"],
            Some(Kind::Portraits) => &["Setup", "Session", "Details"],
            Some(Kind::Property) => &["Exterior", "Living", "Kitchen", "Bedrooms", "Bathrooms", "Garden"],
            Some(Kind::Products) => &["Hero", "Details", "In Use", "Group"],
            Some(Kind::Wildlife) => &["Morning", "Midday", "Evening"],
            Some(Kind::Travel) => &["Arrival", "City", "Nature", "Food", "Night"],
            None => &["Opening", "Main", "Closing"],
        }
    }

    pub fn suggests_chapters(kind: Option<Kind>) -> bool {
        matches!(kind, Some(Kind::Events | Kind::Travel))
    }

    pub fn keep(kind: Option<Kind>) -> usize {
        match kind {
            Some(Kind::Sports | Kind::Property | Kind::Travel) => 1,
            Some(Kind::Products) => 3,
            _ => 2,
        }
    }

    pub fn by_likeness(kind: Option<Kind>) -> bool {
        matches!(kind, Some(Kind::Products | Kind::Property))
    }

    pub fn frame(kind: Option<Kind>) -> Option<Frame> {
        match kind {
            Some(Kind::Sports) => Some(Frame { width: 4, height: 5, on: Placing::Subject }),
            Some(Kind::Products) => Some(Frame { width: 1, height: 1, on: Placing::Subject }),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Part {
    Light,
    Even,
    Straighten,
    Verticals,
    Frame,
}

impl Part {
    pub fn name(self) -> &'static str {
        match self {
            Part::Light => "Light",
            Part::Even => "Even Exposure",
            Part::Straighten => "Straightened",
            Part::Verticals => "Verticals",
            Part::Frame => "Framed",
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Did {

    #[serde(default)]
    pub light: Option<[f32; 6]>,

    #[serde(default)]
    pub warmth: Option<(f32, f32)>,

    #[serde(default)]
    pub even: Option<f32>,
    #[serde(default)]
    pub angle: Option<f32>,

    #[serde(default)]
    pub view: Option<[f32; 4]>,
    #[serde(default)]
    pub vertical: Option<f32>,

    #[serde(default)]
    pub size: Option<(f32, f32)>,
    #[serde(default)]
    pub off: Vec<Part>,
}

impl Did {
    pub fn has(&self, part: Part) -> bool {
        match part {
            Part::Light => self.light.is_some() || self.warmth.is_some(),
            Part::Even => self.even.is_some(),
            Part::Straighten => self.angle.is_some(),
            Part::Verticals => self.vertical.is_some(),
            Part::Frame => self.view.is_some(),
        }
    }

    pub fn is_on(&self, part: Part) -> bool {
        self.has(part) && !self.off.contains(&part)
    }

    pub fn put_on(&self, before: &Document, current: &Document) -> Document {
        let mut document = current.clone();

        let on = self.is_on(Part::Light);
        let swap = |now: f32, was: f32, numas: f32| {
            let (from, to) = if on { (was, numas) } else { (numas, was) };
            if (now - from).abs() < 1e-4 { to } else { now }
        };
        if let Some(light) = self.light {
            let mut basic = document.basic();
            let was = before.basic().tone;
            let tone = &mut basic.tone;

            let even = self.even.filter(|_| self.is_on(Part::Even)).unwrap_or(0.0);
            for (now, was, numas) in [
                (&mut tone.exposure, was.exposure + even, light[0] + even),
                (&mut tone.contrast, was.contrast, light[1]),
                (&mut tone.highlights, was.highlights, light[2]),
                (&mut tone.shadows, was.shadows, light[3]),
                (&mut tone.whites, was.whites, light[4]),
                (&mut tone.blacks, was.blacks, light[5]),
            ] {
                *now = swap(*now, was, numas);
            }
            document.set_basic(basic);
        }
        if let Some((temperature, tint)) = self.warmth {
            let shown = |balance: Option<WhiteBalance>| balance.map(|balance| (balance.temperature, balance.tint));
            let (from, to) = if on { (shown(before.white_balance), Some((temperature, tint))) } else { (Some((temperature, tint)), shown(before.white_balance)) };
            if shown(document.white_balance) == from {
                document.white_balance = to.map(|(temperature, tint)| WhiteBalance { temperature, tint });
            }
        }
        let Some((width, height)) = self.size else { return document };
        let mut perspective = before.perspective();
        if let (true, Some(vertical)) = (self.is_on(Part::Verticals), self.vertical) {
            perspective.vertical = vertical;
        }
        let (kept, kept_angle) = before.crop().unwrap_or(([0.0, 0.0, 1.0, 1.0], 0.0));
        let view = match (self.is_on(Part::Frame), self.view) {
            (true, Some(view)) => view,
            _ => numa_core::image::crop_in_view(width, height, kept, kept_angle, before.perspective()),
        };
        let angle = match (self.is_on(Part::Straighten), self.angle) {
            (true, Some(angle)) => angle,
            _ => kept_angle,
        };
        document.set_perspective(perspective);
        let wanted = numa_core::image::crop_from_view(width, height, view, angle, perspective);
        document.set_crop(numa_core::image::crop_inside(wanted, angle, perspective, width, height), angle);
        document
    }
}

impl Did {

    pub fn shape_only(&self) -> Did {
        let off = self.off.iter().copied().filter(|part| !matches!(part, Part::Light | Part::Even)).collect();
        Did { light: None, warmth: None, even: None, off, ..self.clone() }
    }

    pub fn first(&self, current: &Document) -> Document {
        let mut document = Did { even: None, ..self.clone() }.put_on(current, current);
        if let Some(even) = self.even.filter(|_| self.is_on(Part::Even)) {
            let mut basic = document.basic();
            basic.tone.exposure += even;
            document.set_basic(basic);
        }
        document
    }

    pub fn switch(&mut self, part: Part, on: bool, before: &Document, current: &Document) -> Document {
        let was = self.is_on(part);
        self.off.retain(|off| *off != part);
        if !on {
            self.off.push(part);
        }
        if part != Part::Even {
            return self.put_on(before, current);
        }
        let mut document = current.clone();
        if let (Some(even), true) = (self.even, was != self.is_on(part)) {
            let mut basic = document.basic();
            basic.tone.exposure += if on { even } else { -even };
            document.set_basic(basic);
        }
        document
    }
}

pub const EVEN_MOST: f32 = 1.5;

pub fn evened(exposures: &[Option<(f32, bool)>]) -> Vec<f32> {
    let stops: Vec<Option<f32>> = exposures.iter().map(|exposure| exposure.filter(|(light, flash)| !flash && *light > 0.0).map(|(light, _)| light.log2())).collect();
    let mut known: Vec<f32> = stops.iter().flatten().copied().collect();
    known.sort_by(f32::total_cmp);
    let Some(middle) = known.get(known.len() / 2).copied() else { return vec![0.0; exposures.len()] };
    stops
        .iter()
        .map(|stop| match stop.map(|stop| middle - stop) {
            Some(by) if by.abs() >= 0.05 && by.abs() <= EVEN_MOST => by,
            _ => 0.0,
        })
        .collect()
}

pub fn keep_few(scores: &[Option<f32>], keep: usize) -> Vec<usize> {
    let keep = keep.min(scores.iter().flatten().count());
    if keep == 0 {
        return Vec::new();
    }
    let stretch = scores.len() as f32 / keep as f32;
    let mut chosen: Vec<usize> = Vec::new();
    for part in 0..keep {
        let (from, to) = ((part as f32 * stretch).round() as usize, (((part + 1) as f32 * stretch).round() as usize).min(scores.len()));
        let best = (from..to).filter(|at| !chosen.contains(at)).filter_map(|at| scores[at].map(|score| (at, score))).max_by(|a, b| a.1.total_cmp(&b.1));

        let best = best.or_else(|| (0..scores.len()).filter(|at| !chosen.contains(at)).filter_map(|at| scores[at].map(|score| (at, score))).max_by(|a, b| a.1.total_cmp(&b.1)));
        if let Some((at, _)) = best {
            chosen.push(at);
        }
    }
    chosen.sort_unstable();
    chosen
}

pub fn camera_bursts(taken: &[Option<i64>], hashes: &[Option<u64>]) -> Vec<usize> {
    let mut stacks = Vec::with_capacity(taken.len());
    let mut current = 0usize;
    for index in 0..taken.len() {
        let joins = index > 0
            && matches!((taken[index - 1], taken[index]), (Some(a), Some(b)) if (b - a).abs() <= BURST_SECONDS)
            && matches!((hashes[index - 1], hashes[index]), (Some(a), Some(b)) if numa_cull::distance(a, b) <= numa_cull::BURST_STEP);
        if index > 0 && !joins {
            current += 1;
        }
        stacks.push(current);
    }
    stacks
}

const BURST_SECONDS: i64 = 1;

pub fn alike(count: usize, links: &[(usize, usize)]) -> Vec<Vec<usize>> {
    let mut root: Vec<usize> = (0..count).collect();
    fn find(root: &mut [usize], at: usize) -> usize {
        let mut at = at;
        while root[at] != at {
            root[at] = root[root[at]];
            at = root[at];
        }
        at
    }
    for &(a, b) in links.iter().filter(|(a, b)| *a < count && *b < count) {
        let (ra, rb) = (find(&mut root, a), find(&mut root, b));
        root[ra.max(rb)] = ra.min(rb);
    }
    let mut groups: Vec<Vec<usize>> = Vec::new();
    let mut of_root: std::collections::HashMap<usize, usize> = std::collections::HashMap::new();
    for at in 0..count {
        let r = find(&mut root, at);
        match of_root.get(&r) {
            Some(&group) => groups[group].push(at),
            None => {
                of_root.insert(r, groups.len());
                groups.push(vec![at]);
            }
        }
    }
    groups
}

pub fn light_of(kelvin: f32) -> &'static str {
    match kelvin {
        k if k < 3800.0 => "Warm light",
        k if k < 5000.0 => "Mixed light",
        _ => "Daylight",
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Change {
    pub at: i64,
    pub light: String,
    pub control: Control,
    pub delta: f32,
    pub said: String,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Style {
    pub changes: Vec<Change>,
}

const LEARNED_FROM: usize = 20;

impl Style {
    pub fn learn(&mut self, at: i64, light: &str, control: Control, delta: f32, said: &str) {
        if delta.abs() < 1e-3 {
            return;
        }
        self.changes.push(Change { at, light: light.to_string(), control, delta, said: said.to_string() });
    }

    pub fn offset(&self, light: &str, control: Control) -> f32 {
        let recent: Vec<f32> = self.changes.iter().rev().filter(|change| change.light == light && change.control == control).take(LEARNED_FROM).map(|change| change.delta).collect();
        match recent.len() {
            0 => 0.0,
            n => recent.iter().sum::<f32>() / n as f32,
        }
    }

    pub fn learned(&self) -> Vec<(String, Control, f32, usize)> {
        let mut seen: Vec<(String, Control)> = Vec::new();
        for change in &self.changes {
            if !seen.iter().any(|(light, control)| *light == change.light && *control == change.control) {
                seen.push((change.light.clone(), change.control));
            }
        }
        seen.into_iter()
            .map(|(light, control)| {
                let count = self.changes.iter().filter(|change| change.light == light && change.control == control).count();
                let offset = self.offset(&light, control);
                (light, control, offset, count)
            })
            .collect()
    }

    pub fn forget(&mut self, light: &str, control: Control) {
        self.changes.retain(|change| !(change.light == light && change.control == control));
    }
}

pub fn read_note(text: &str) -> Vec<(Control, f32)> {
    let text = format!(" {} ", text.to_lowercase());
    let too = text.contains(" te ") || text.contains(" too ") || text.contains(" veel te ");
    let has = |words: &[&str]| words.iter().any(|word| text.contains(word));
    let mut out: Vec<(Control, f32)> = Vec::new();
    let mut add = |control: Control, delta: f32| {
        if !out.iter().any(|(seen, _)| *seen == control) {
            out.push((control, delta));
        }
    };
    let flip = |delta: f32| if too { -delta } else { delta };
    if has(&["warm"]) {
        add(Control::Warmth, flip(300.0));
    } else if has(&["koel", "cool", "cold", "koud", "blauw", "blue"]) {
        add(Control::Warmth, flip(-300.0));
    }
    if has(&["licht", "light", "bright", "helder"]) && !has(&["highlight", "hooglicht"]) {
        add(Control::Exposure, flip(0.3));
    } else if has(&["donker", "dark", "dim"]) {
        add(Control::Exposure, flip(-0.3));
    }
    if has(&["meer contrast", "more contrast", "punch", "pittig"]) {
        add(Control::Contrast, 15.0);
    } else if has(&["minder contrast", "less contrast", "flat", "vlak", "zacht", "soft"]) {
        add(Control::Contrast, if too { 15.0 } else { -15.0 });
    }
    if has(&["highlight", "hooglicht", "uitgebeten", "blown", "lucht", "sky", "raam", "window"]) {
        add(Control::Highlights, -30.0);
    }
    if has(&["schaduw", "shadow"]) {
        add(Control::Shadows, 25.0);
    }
    if has(&["kleurrijk", "colourful", "colorful", "vivid", "meer kleur", "more colour", "more color"]) {
        add(Control::Vibrance, 15.0);
    } else if has(&["minder kleur", "less colour", "less color", "muted", "flets", "fel"]) {
        add(Control::Vibrance, if has(&["flets"]) { 15.0 } else { -15.0 });
    }
    if has(&["ruis", "noise", "korrel", "grain"]) {
        add(Control::Noise, 30.0);
    }
    out
}

pub const PAUSE: i64 = 300;

pub const MOST: usize = 48;

pub fn moments(times: &[i64]) -> Vec<Range<usize>> {
    let mut cut = Vec::new();
    let mut start = 0;
    for at in 1..times.len() {
        if times[at] - times[at - 1] > PAUSE {
            cut.push(start..at);
            start = at;
        }
    }
    if !times.is_empty() {
        cut.push(start..times.len());
    }
    let mut out = Vec::new();
    while let Some(run) = cut.pop() {
        if run.len() <= MOST {
            out.push(run);
            continue;
        }

        let at = (run.start + 1..run.end).max_by_key(|&at| (times[at] - times[at - 1], at)).filter(|&at| times[at] > times[at - 1]).unwrap_or(run.start + run.len() / 2);
        cut.push(run.start..at);
        cut.push(at..run.end);
    }
    out.sort_by_key(|run| run.start);
    out
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Frame {
    pub width: u32,
    pub height: u32,
    pub on: Placing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Placing {

    Subject,

    Faces,
}

impl Frame {

    pub const ASPECTS: [(u32, u32); 4] = [(4, 5), (1, 1), (3, 2), (16, 9)];

    pub fn ratio(self, frame: f32) -> f32 {
        let ratio = self.width as f32 / self.height as f32;
        if ratio > 1.0 && frame < 1.0 { 1.0 / ratio } else { ratio }
    }

    pub fn label(self) -> String {
        let on = match self.on {
            Placing::Subject => "the subject",
            Placing::Faces => "the faces",
        };
        format!("{}:{} on {on}", self.width, self.height)
    }
}

pub fn regions(width: usize, height: usize, cells: &[bool]) -> Vec<(usize, [f32; 4])> {
    let mut seen = vec![false; cells.len()];
    let mut found = Vec::new();
    for start in 0..cells.len().min(width * height) {
        if !cells[start] || seen[start] {
            continue;
        }
        let (mut size, mut bounds) = (0, [usize::MAX, usize::MAX, 0, 0]);
        let mut stack = vec![start];
        seen[start] = true;
        while let Some(cell) = stack.pop() {
            let (x, y) = (cell % width, cell / width);
            size += 1;
            bounds = [bounds[0].min(x), bounds[1].min(y), bounds[2].max(x), bounds[3].max(y)];
            let mut near = Vec::with_capacity(4);
            if x > 0 { near.push(cell - 1); }
            if x + 1 < width { near.push(cell + 1); }
            if y > 0 { near.push(cell - width); }
            if y + 1 < height { near.push(cell + width); }
            for next in near {
                if cells[next] && !seen[next] {
                    seen[next] = true;
                    stack.push(next);
                }
            }
        }
        let [x0, y0, x1, y1] = bounds;
        let (w, h) = (width as f32, height as f32);
        found.push((size, [x0 as f32 / w, y0 as f32 / h, (x1 + 1 - x0) as f32 / w, (y1 + 1 - y0) as f32 / h]));
    }
    found.sort_by(|a, b| b.0.cmp(&a.0));
    found
}

pub fn largest_on(frame: f32, ratio: f32, point: [f32; 2], at: [f32; 2]) -> [f32; 4] {

    let target = ratio / frame.max(1e-3);
    let (width, height) = if target <= 1.0 { (target, 1.0) } else { (1.0, 1.0 / target) };
    [(point[0] - width * at[0]).clamp(0.0, 1.0 - width), (point[1] - height * at[1]).clamp(0.0, 1.0 - height), width, height]
}

const ROOM: f32 = 0.06;

const AT_EDGE: f32 = 0.02;

pub const NO_SUBJECT: &str = "no one subject";
pub const NO_ROOM: &str = "no room round the subject";
pub const CUTS_SOMEONE: &str = "it would cut someone off";
pub const NO_FACE: &str = "no face found";

fn has_room([x, y, w, h]: [f32; 4], [sx, sy, sw, sh]: [f32; 4]) -> bool {
    let left = sx <= AT_EDGE || sx - x >= ROOM * w;
    let right = sx + sw >= 1.0 - AT_EDGE || (x + w) - (sx + sw) >= ROOM * w;
    let top = sy <= AT_EDGE || sy - y >= ROOM * h;
    let bottom = sy + sh >= 1.0 - AT_EDGE || (y + h) - (sy + sh) >= ROOM * h;
    left && right && top && bottom
}

fn cuts([x, y, w, h]: [f32; 4], [ox, oy, ow, oh]: [f32; 4]) -> bool {
    let across = ((x + w).min(ox + ow) - x.max(ox)).max(0.0);
    let down = ((y + h).min(oy + oh) - y.max(oy)).max(0.0);
    let inside = across * down / (ow * oh).max(1e-6);

    inside > 0.05 && inside < 0.85
}

pub fn framing(frame: f32, wanted: Frame, regions: &[(usize, [f32; 4])], faces: &[[f32; 4]]) -> Result<[f32; 4], &'static str> {
    let ratio = wanted.ratio(frame);
    let largest = faces.iter().copied().max_by(|a, b| (a[2] * a[3]).total_cmp(&(b[2] * b[3])));
    let body = |[x, y, w, h]: [f32; 4]| {
        let top = (y - h * 0.5).max(0.0);
        [(x + w / 2.0 - w * 1.5).max(0.0), top, (w * 3.0).min(1.0), (h * 7.0).min(1.0 - top)]
    };
    let size = |face: &[f32; 4]| face[2] * face[3];

    let people: Vec<[f32; 4]> = largest.map_or(Vec::new(), |largest| faces.iter().copied().filter(|face| size(face) >= size(&largest) * 0.4).collect());
    let union = |boxes: &[[f32; 4]]| {
        let x0 = boxes.iter().map(|b| b[0]).fold(1.0, f32::min);
        let y0 = boxes.iter().map(|b| b[1]).fold(1.0, f32::min);
        let x1 = boxes.iter().map(|b| b[0] + b[2]).fold(0.0, f32::max);
        let y1 = boxes.iter().map(|b| b[1] + b[3]).fold(0.0, f32::max);
        [x0, y0, x1 - x0, y1 - y0]
    };
    let subject = match wanted.on {
        Placing::Subject => match regions.first() {
            Some((_, region)) if region[2] * region[3] <= 0.6 => *region,

            _ if !people.is_empty() => union(&people.iter().map(|face| body(*face)).collect::<Vec<_>>()),
            _ => return Err(NO_SUBJECT),
        },
        Placing::Faces if people.is_empty() => return Err(NO_FACE),
        Placing::Faces => union(&people),
    };

    let [x, y, w, h] = subject;
    let across = x + w / 2.0;
    let thirds: [f32; 3] = match across {
        left if left < 0.45 => [1.0 / 3.0, 0.5, 2.0 / 3.0],
        right if right > 0.55 => [2.0 / 3.0, 0.5, 1.0 / 3.0],
        _ => [0.5, 1.0 / 3.0, 2.0 / 3.0],
    };
    let (down, at_down) = match (wanted.on, people.len()) {
        (Placing::Faces, 2..) => (y + h / 2.0, 0.4),
        (_, 1..) => {
            let eyes = largest.map_or(y + h / 2.0, |face| face[1] + face[3] * 0.4);
            (eyes, 1.0 / 3.0)
        }
        _ => (y + h / 2.0, 0.5),
    };

    let main = regions.first().map_or(0, |(size, _)| *size);
    let others: Vec<[f32; 4]> = regions.iter().skip(1).filter(|(size, _)| *size * 10 >= main * 3).map(|(_, other)| *other).collect();
    let mut why = NO_ROOM;
    for at_across in thirds {
        let rect = largest_on(frame, ratio, [across, down], [at_across, at_down]);
        if !has_room(rect, subject) {
            continue;
        }
        if others.iter().chain(&people).any(|other| cuts(rect, *other)) {
            why = CUTS_SOMEONE;
            continue;
        }
        return Ok(rect);
    }
    Err(why)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_pause_starts_a_moment_and_a_long_one_is_cut_where_it_pauses_longest() {
        assert!(moments(&[]).is_empty());
        assert_eq!(moments(&[0, 10, 20, 1000, 1010]), vec![0..3, 3..5]);

        let mut times: Vec<i64> = (0..60).collect();
        for t in times.iter_mut().skip(20) {
            *t += 100;
        }
        assert_eq!(moments(&times), vec![0..20, 20..60]);

        let still = vec![7; 100];
        let cut = moments(&still);
        assert!(cut.iter().all(|run| run.len() <= MOST) && cut.iter().map(|run| run.len()).sum::<usize>() == 100);
    }

    #[test]
    fn every_kind_has_four_or_five_things_and_reads_back_by_name() {
        for kind in Kind::ALL {
            assert_eq!(Kind::from_name(kind.name()), Some(kind));
            assert!((4..=5).contains(&Kind::controls(Some(kind)).len()), "{}", kind.name());
            assert_eq!(Kind::controls(Some(kind))[0], Control::Exposure, "exposure first, in every kind");
        }
    }

    #[test]
    fn a_control_sets_one_thing_and_warmth_keeps_the_tint() {
        let shot = WhiteBalance { temperature: 4800.0, tint: 7.0 };
        let mut document = Document::new("a.RAF".to_string());
        assert_eq!(Control::Warmth.get(&document, shot), 4800.0, "untouched is the camera's");
        Control::Warmth.set(&mut document, 5600.0, shot);
        assert_eq!(document.white_balance, Some(WhiteBalance { temperature: 5600.0, tint: 7.0 }));
        Control::Exposure.set(&mut document, 0.3, shot);
        assert_eq!(document.basic().tone.exposure, 0.3);
        assert_eq!(document.basic().tone.contrast, 0.0);
        Control::Exposure.reset(&mut document);
        Control::Warmth.reset(&mut document);
        assert_eq!(document.white_balance, None);
        assert!(document.basic().is_identity());
    }

    #[test]
    fn a_note_is_read_in_dutch_and_english_and_too_turns_it_round() {
        assert_eq!(read_note("te warm hier"), vec![(Control::Warmth, -300.0)]);
        assert_eq!(read_note("warmer please"), vec![(Control::Warmth, 300.0)]);
        assert_eq!(read_note("too dark"), vec![(Control::Exposure, 0.3)]);
        assert_eq!(read_note("iets lichter en meer contrast"), vec![(Control::Exposure, 0.3), (Control::Contrast, 15.0)]);
        assert_eq!(read_note("the windows are blown"), vec![(Control::Highlights, -30.0)]);
        assert!(read_note("mooi zo").is_empty(), "nothing understood is said, not guessed");
    }

    #[test]
    fn a_style_is_the_mean_of_its_changes_per_light_and_can_be_forgotten() {
        let mut style = Style::default();
        style.learn(1, "Warm light", Control::Warmth, 200.0, "moved by hand");
        style.learn(2, "Warm light", Control::Warmth, 400.0, "te koud");
        style.learn(3, "Daylight", Control::Exposure, 0.2, "moved by hand");
        style.learn(4, "Daylight", Control::Exposure, 0.0, "nothing");
        assert_eq!(style.offset("Warm light", Control::Warmth), 300.0);
        assert_eq!(style.offset("Daylight", Control::Warmth), 0.0, "per light");
        assert_eq!(style.learned().len(), 2);
        style.forget("Warm light", Control::Warmth);
        assert_eq!(style.offset("Warm light", Control::Warmth), 0.0);
        assert_eq!(light_of(3200.0), "Warm light");
        assert_eq!(light_of(5600.0), "Daylight");
    }

    #[test]
    fn every_kind_with_a_frame_frames_and_what_numa_did_says_its_parts() {
        for kind in Kind::ALL {
            assert_eq!(Kind::shape(Some(kind)).contains(&Part::Frame), Kind::frame(Some(kind)).is_some(), "{}", kind.name());
        }
        let did = Did { angle: Some(1.2), ..Default::default() };
        assert!(did.has(Part::Straighten) && !did.has(Part::Light));
        let back: Did = serde_json::from_str(&serde_json::to_string(&did).unwrap()).unwrap();
        assert_eq!(back, did);
    }

    #[test]
    fn a_part_switched_off_is_the_before_and_on_again_is_numas() {
        let before = Document::new("a.RAF".to_string());
        let mut did = Did { light: Some([0.4, 0.0, -20.0, 10.0, 0.0, 0.0]), angle: Some(1.5), size: Some((1.5, 1.0)), ..Default::default() };

        let mut current = did.put_on(&before, &before);
        assert_eq!(current.basic().tone.exposure, 0.4);
        assert_eq!(current.crop().map(|(_, angle)| angle), Some(1.5));
        let mut basic = current.basic();
        basic.presence.vibrance = 12.0;
        current.set_basic(basic);
        did.off.push(Part::Light);
        let without = did.put_on(&before, &current);
        assert_eq!(without.basic().tone.exposure, 0.0, "the light is the before's");
        assert_eq!(without.basic().presence.vibrance, 12.0, "the photographer's own stays");
        let mut moved = current.clone();
        let mut basic = moved.basic();
        basic.tone.shadows = 30.0;
        moved.set_basic(basic);
        let kept = did.put_on(&before, &moved).basic().tone;
        assert_eq!((kept.exposure, kept.highlights, kept.shadows), (0.0, 0.0, 30.0), "a slider moved since Numa stays");
        assert_eq!(without.crop().map(|(_, angle)| angle), Some(1.5), "straightening is its own switch");
        did.off.push(Part::Straighten);
        assert!(did.put_on(&before, &without).crop().is_none_or(|(_, angle)| angle == 0.0));
        did.off.clear();
        assert_eq!(did.put_on(&before, &without).basic().tone.exposure, 0.4);
    }

    #[test]
    fn a_burst_keeps_the_best_of_each_stretch_and_never_a_shut_eye() {
        let scores = [Some(0.5), Some(0.9), Some(0.8), None, Some(0.4), Some(0.7)];
        assert_eq!(keep_few(&scores, 2), vec![1, 5], "the best of each half");
        assert_eq!(keep_few(&scores, 1), vec![1]);
        assert_eq!(keep_few(&[None, None], 2), Vec::<usize>::new());
        assert_eq!(keep_few(&[Some(0.1), None, None, None], 2), vec![0], "no more than there are worth keeping");
    }

    #[test]
    fn moments_of_the_same_scene_are_one_group_in_order() {
        assert_eq!(alike(5, &[(0, 3), (3, 4)]), vec![vec![0, 3, 4], vec![1], vec![2]]);
        assert_eq!(alike(3, &[]), vec![vec![0], vec![1], vec![2]]);
        for kind in Kind::ALL {
            assert!(!Kind::chapters(Some(kind)).is_empty() && Kind::keep(Some(kind)) >= 1);
        }
    }

    #[test]
    fn a_moment_is_evened_by_what_the_camera_let_in() {

        let base = 1.0 / 125.0 * 400.0 / (2.8 * 2.8);
        let frames = [Some((base, false)), Some((base / 2.0, false)), Some((base * 2.0, true)), None, Some((base * 16.0, false)), Some((base, false))];
        let by = evened(&frames);
        assert_eq!(by[0], 0.0, "the middle stays");
        assert!((by[1] - 1.0).abs() < 1e-4, "a stop less light is given a stop");
        assert_eq!((by[2], by[3], by[4]), (0.0, 0.0, 0.0), "flash, nothing known and another light are left");
        assert_eq!(evened(&[None, None]), vec![0.0, 0.0]);
    }

    #[test]
    fn evening_switches_on_top_of_what_the_photographer_did() {
        let before = Document::new("a.RAF".to_string());
        let mut did = Did { light: Some([0.4, 0.0, 0.0, 0.0, 0.0, 0.0]), even: Some(1.0), ..Default::default() };
        let mut current = did.first(&before);
        assert!((current.basic().tone.exposure - 1.4).abs() < 1e-5, "Numa's light and the evening");

        let mut basic = current.basic();
        basic.tone.exposure += 0.3;
        current.set_basic(basic);
        let lit = did.switch(Part::Light, false, &before, &current);
        assert!((lit.basic().tone.exposure - 1.7).abs() < 1e-5, "moved since, so kept");
        let unlit = did.switch(Part::Light, true, &before, &did.first(&before));
        assert!((unlit.basic().tone.exposure - 1.4).abs() < 1e-5);
        let flat = did.switch(Part::Even, false, &before, &unlit);
        assert!((flat.basic().tone.exposure - 0.4).abs() < 1e-5, "evening off takes its stop off");
        let lights_off = did.switch(Part::Light, false, &before, &flat);
        assert!(lights_off.basic().tone.exposure.abs() < 1e-5, "and Numa's light off is the before");
        assert!((did.switch(Part::Even, true, &before, &lights_off).basic().tone.exposure - 1.0).abs() < 1e-5);
    }

    #[test]
    fn an_aspect_turns_with_the_photograph() {
        let four_five = Frame { width: 4, height: 5, on: Placing::Subject };
        assert_eq!(four_five.ratio(1.5), 0.8, "4:5 is what a feed asks for, whatever the frame");
        let wide = Frame { width: 16, height: 9, on: Placing::Faces };
        assert!(wide.ratio(2.0 / 3.0) < 1.0, "16:9 turns with a portrait frame");
    }

    #[test]
    fn one_player_is_found_and_the_others_are_kept_apart() {

        #[rustfmt::skip]
        let cells: Vec<bool> = [
            0,0,0,0,0,0,0,1,1,0,
            0,1,0,0,0,0,0,1,1,0,
            0,0,0,0,0,0,0,1,1,0,
            0,0,0,0,0,0,0,1,1,0,
        ].iter().map(|cell| *cell == 1).collect();
        let found = regions(10, 4, &cells);
        assert_eq!(found[0], (8, [0.7, 0.0, 0.2, 1.0]), "the larger one first");
        assert_eq!(found.len(), 2);
    }

    #[test]
    fn a_subject_keeps_room_round_it_and_nobody_is_cut_in_half() {
        let four_five = |on| Frame { width: 4, height: 5, on };

        let player = [0.6, 0.3, 0.1, 0.3];
        let rect = framing(1.5, four_five(Placing::Subject), &[(100, player)], &[]).unwrap();
        assert!(rect[0] < 0.6 && rect[0] + rect[2] > 0.7 && rect[1] < 0.3 && rect[1] + rect[3] > 0.6, "{rect:?}");

        let tall = [0.4, 0.03, 0.2, 0.94];
        assert_eq!(framing(1.5, four_five(Placing::Subject), &[(100, tall)], &[]), Err(NO_ROOM));

        let cut = [0.45, 0.2, 0.1, 0.8];
        assert!(framing(1.5, four_five(Placing::Subject), &[(100, cut)], &[]).is_ok());

        let man = [0.55, 0.15, 0.12, 0.7];
        let whole = |rect: [f32; 4], [ox, _, ow, _]: [f32; 4]| (rect[0] <= ox && rect[0] + rect[2] >= ox + ow) || rect[0] >= ox + ow || rect[0] + rect[2] <= ox;
        for woman in [[0.3, 0.2, 0.12, 0.7], [0.15, 0.2, 0.12, 0.7]] {
            let rect = framing(1.5, four_five(Placing::Subject), &[(100, man), (60, woman)], &[]).unwrap();
            assert!(whole(rect, woman), "{woman:?} halved by {rect:?}");
        }

        let squeezed = [[0.45, 0.1, 0.1, 0.8], [0.15, 0.2, 0.1, 0.7], [0.75, 0.2, 0.1, 0.7], [0.28, 0.2, 0.06, 0.7], [0.65, 0.2, 0.06, 0.7]];
        let found: Vec<(usize, [f32; 4])> = squeezed.iter().map(|b| (100, *b)).collect();
        assert_eq!(framing(1.5, four_five(Placing::Subject), &found, &[]), Err(CUTS_SOMEONE));

        let small = [0.7, 0.6, 0.03, 0.08];
        let rect = framing(1.5, four_five(Placing::Subject), &[(10, small)], &[]).unwrap();
        assert!((rect[2] - 0.8 / 1.5).abs() < 1e-4 && (rect[3] - 1.0).abs() < 1e-4, "{rect:?}");

        let far = [0.35, 0.5, 0.02, 0.05];
        assert!(framing(1.5, four_five(Placing::Subject), &[(100, man), (4, far)], &[]).is_ok());
    }

    #[test]
    fn the_subject_goes_on_a_third_and_the_eyes_on_the_upper_one() {
        let four_five = |on| Frame { width: 4, height: 5, on };

        let rect = framing(1.5, four_five(Placing::Subject), &[(100, [0.25, 0.3, 0.1, 0.4])], &[]).unwrap();
        let at = (0.3 - rect[0]) / rect[2];
        assert!((at - 1.0 / 3.0).abs() < 0.01, "{at} across {rect:?}");

        let rect = framing(1.5, four_five(Placing::Subject), &[(100, [0.45, 0.3, 0.1, 0.4])], &[]).unwrap();
        assert!(((0.5 - rect[0]) / rect[2] - 0.5).abs() < 0.01, "{rect:?}");

        let wide = Frame { width: 16, height: 9, on: Placing::Faces };
        let face = [0.45, 0.35, 0.1, 0.15];
        let rect = framing(1.5, wide, &[], &[face]).unwrap();
        let eyes = (0.35 + 0.15 * 0.4 - rect[1]) / rect[3];
        assert!((eyes - 1.0 / 3.0).abs() < 0.01, "{eyes} down {rect:?}");
    }

    #[test]
    fn a_crowd_is_not_a_subject_and_a_group_is_framed_together() {
        let four_five = |on| Frame { width: 4, height: 5, on };

        assert_eq!(framing(1.5, four_five(Placing::Subject), &[(900, [0.0, 0.3, 1.0, 0.7])], &[]), Err(NO_SUBJECT));

        let face = [0.49, 0.14, 0.06, 0.13];
        let rect = framing(1.5, four_five(Placing::Subject), &[(900, [0.0, 0.0, 1.0, 1.0])], &[face]).unwrap();
        assert!(rect[0] < 0.49 && rect[0] + rect[2] > 0.55 && rect[2] < 0.7, "{rect:?}");

        let faces = [[0.1, 0.3, 0.1, 0.2], [0.3, 0.25, 0.1, 0.2], [0.5, 0.2, 0.09, 0.18], [0.7, 0.3, 0.12, 0.24]];
        let wide = Frame { width: 16, height: 9, on: Placing::Faces };
        let rect = framing(1.5, wide, &[], &faces).unwrap();
        assert!(rect[0] <= 0.1 && rect[0] + rect[2] >= 0.82, "{rect:?}");

        let family = framing(1.5, four_five(Placing::Subject), &[(900, [0.05, 0.1, 0.9, 0.85])], &faces);
        assert!(matches!(family, Err(NO_ROOM) | Err(CUTS_SOMEONE)), "{family:?}");

        assert_eq!(framing(1.5, four_five(Placing::Faces), &[], &[]), Err(NO_FACE));
    }

}
