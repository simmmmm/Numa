use numa_core::document::Perspective;
use numa_core::plane::Plane;
use serde::{Deserialize, Serialize};

pub(crate) const BUILDINGS: [u16; 9] = [0, 1, 8, 14, 25, 42, 48, 79, 84];

const NEVER: [u16; 18] = [16, 68, 46, 29, 13, 34, 94, 6, 11, 52, 140, 32, 38, 95, 53, 59, 12, 126];

const BUILT_UP: f32 = 0.08;

const LEAN: f32 = 0.6;
const STEEP: f32 = 0.27;

const ENOUGH: usize = 400;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Source {
    Sea,

    Water,
    Buildings,
    Verticals,
}

impl Source {
    pub fn name(self) -> &'static str {
        match self {
            Source::Sea => "the sea",
            Source::Water => "the water",
            Source::Buildings => "the buildings",
            Source::Verticals => "the verticals",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum Reading {

    Line([f32; 4]),

    Lean(f32),
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Evidence {
    pub source: Source,
    pub reading: Reading,

    pub sigma: f32,
}

impl Evidence {

    pub fn angle(&self, perspective: Perspective, width: f32, height: f32) -> f32 {
        let stretch = perspective.stretch();
        match self.reading {
            Reading::Lean(lean) => -(lean * stretch * stretch).atan().to_degrees(),
            Reading::Line([u0, v0, u1, v1]) => {
                let (vertical, horizontal) = perspective.coefficients();
                let (a, b) = (vertical / (height / 2.0), horizontal / (width / 2.0));

                let corrected = |u: f32, v: f32| {
                    let (sx, sy) = ((u - 0.5) * width, (v - 0.5) * height);
                    let depth = 1.0 + a * sy + b * sx;
                    (sx / depth, sy / depth)
                };
                let ((x0, y0), (x1, y1)) = (corrected(u0, v0), corrected(u1, v1));
                ((y1 - y0) / stretch).atan2((x1 - x0) * stretch).to_degrees()
            }
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Lean {
    pub lean: f32,
    pub reach: f32,

    pub sigma: f32,
}

impl Lean {

    pub fn vertical(&self) -> f32 {
        (-200.0 * self.reach).clamp(-100.0, 100.0)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Vote {
    pub x: f32,
    pub y: f32,
    pub gx: f32,
    pub gy: f32,
    pub weight: f32,
    pub class: u16,

    pub building: f32,
}

pub fn votes(luma: &Plane, class_at: impl Fn(f32, f32) -> (u16, f32)) -> Vec<Vote> {
    let Some((edges, (width, height))) = super::level::coherent_edges(luma) else { return Vec::new() };
    edges
        .into_iter()
        .map(|(x, y, gx, gy, weight)| {
            let (class, building) = class_at(x / width as f32, y / height as f32);
            Vote { x, y, gx, gy, weight, class, building }
        })
        .collect()
}

pub fn vanishing(votes: &[Vote], width: usize, height: usize, built: f32) -> Option<Lean> {
    if built < BUILT_UP {
        return None;
    }
    let rows = rows_of(votes, width, height, |vote| vote.building >= 0.5 && vote.gy.abs() < vote.gx.abs() * LEAN);
    point_of(&rows, width, height)
}

fn rows_of(votes: &[Vote], width: usize, height: usize, keep: impl Fn(&Vote) -> bool) -> Vec<Row> {
    let half = height as f32 / 2.0;
    votes
        .iter()
        .filter(|vote| keep(vote))
        .map(|vote| {
            let (x, y) = ((vote.x - width as f32 / 2.0) / half, (vote.y - half) / half);
            let lean = -vote.gy / vote.gx;
            Row { lean, across: lean * y - x, weight: vote.weight, x }
        })
        .collect()
}

fn point_of(rows: &[Row], width: usize, height: usize) -> Option<Lean> {
    let half = height as f32 / 2.0;
    if rows.len() < ENOUGH {
        return None;
    }
    let total: f32 = rows.iter().map(|row| row.weight).sum();
    let mean = rows.iter().map(|row| row.x * row.weight).sum::<f32>() / total;
    let spread = (rows.iter().map(|row| (row.x - mean).powi(2) * row.weight).sum::<f32>() / total).sqrt();
    if spread < 0.15 * width as f32 / half {
        return None;
    }

    let start = (median(rows), 0.0);
    let ((lean, reach), kept) = irls(rows, start, |_| 1.0, 10)?;
    if kept < total * 0.3 {
        return None;
    }

    const STRIPS: usize = 16;
    let strip = |row: &Row| (((row.x * half + width as f32 / 2.0) / width as f32 * STRIPS as f32) as usize).min(STRIPS - 1);
    let mut seed = 0x2545_f491u32;
    let draws: Vec<f32> = (0..48)
        .filter_map(|_| {
            let mut counts = [0.0f32; STRIPS];
            for _ in 0..STRIPS {
                seed ^= seed << 13;
                seed ^= seed >> 17;
                seed ^= seed << 5;
                counts[seed as usize % STRIPS] += 1.0;
            }
            irls(rows, (lean, reach), |row| counts[strip(row)], 4).map(|((lean, _), _)| lean)
        })
        .collect();
    let average = draws.iter().sum::<f32>() / draws.len().max(1) as f32;
    let deviation = (draws.iter().map(|d| (d - average).powi(2)).sum::<f32>() / draws.len().max(1) as f32).sqrt();
    let sigma = (deviation.atan().to_degrees()).max(0.1);
    Some(Lean { lean, reach, sigma })
}

struct Row {
    lean: f32,
    across: f32,
    weight: f32,
    x: f32,
}

fn median(rows: &[Row]) -> f32 {
    let mut sorted: Vec<(f32, f32)> = rows.iter().map(|row| (row.lean, row.weight)).collect();
    sorted.sort_by(|a, b| a.0.total_cmp(&b.0));
    let half = sorted.iter().map(|s| s.1).sum::<f32>() / 2.0;
    let mut sum = 0.0;
    sorted.iter().find(|s| { sum += s.1; sum >= half }).map_or(0.0, |s| s.0)
}

fn irls(rows: &[Row], start: (f32, f32), times: impl Fn(&Row) -> f32, rounds: usize) -> Option<((f32, f32), f32)> {
    let (mut t, mut p) = start;
    let mut kept = 0.0;
    for _ in 0..rounds {
        let residual = |row: &Row| row.lean - t - p * row.across;
        let mut absolute: Vec<f32> = rows.iter().map(|row| residual(row).abs()).collect();
        let middle = absolute.len() / 2;
        let scale = (4.685 * 1.4826 * *absolute.select_nth_unstable_by(middle, f32::total_cmp).1).max(0.01);
        let (mut s, mut sa, mut saa, mut sl, mut sal) = (0.0f64, 0.0f64, 0.0f64, 0.0f64, 0.0f64);
        kept = 0.0;
        for row in rows {
            let r = residual(row) / scale;
            let w = (row.weight * times(row) * (1.0 - r * r).max(0.0).powi(2)) as f64;
            if r.abs() <= 1.0 {
                kept += row.weight * times(row);
            }
            let (a, l) = (row.across as f64, row.lean as f64);
            s += w;
            sa += w * a;
            saa += w * a * a;
            sl += w * l;
            sal += w * a * l;
        }
        let det = s * saa - sa * sa;
        if s <= 0.0 || det.abs() < 1e-9 * s * s {
            return None;
        }
        t = ((saa * sl - sa * sal) / det) as f32;
        p = ((s * sal - sa * sl) / det) as f32;
    }
    Some(((t, p), kept))
}

pub fn verticals(votes: &[Vote], width: usize, height: usize) -> Option<Lean> {
    let middle = width as f32 / 2.0;
    let rows = rows_of(votes, width, height, |vote| {
        !NEVER.contains(&vote.class) && (vote.x - middle).abs() <= width as f32 * 0.4 && vote.gy.abs() < vote.gx.abs() * STEEP
    });
    let lean = point_of(&rows, width, height)?;

    let at_centre: Vec<(f32, f32)> =
        rows.iter().map(|row| ((-(row.lean - lean.reach * row.across).atan()).to_degrees(), row.weight)).collect();
    let peak = super::level::crowded_by(&at_centre, 3.5)?;
    if (peak + lean.lean.atan().to_degrees()).abs() > 0.3 {
        return None;
    }

    const STRIPS: usize = 32;
    let half = height as f32 / 2.0;
    let mut strips = [0.0f32; STRIPS];
    for row in rows.iter().filter(|row| (row.lean - lean.lean - lean.reach * row.across).abs() <= 0.01) {
        let at = ((-row.across * half + middle) / width as f32 * STRIPS as f32).clamp(0.0, (STRIPS - 1) as f32) as usize;
        strips[at] += row.weight;
    }
    let total: f32 = strips.iter().sum();
    let mut structures = 0;
    let mut open = false;
    for weight in strips {
        let counts = weight >= total * 0.02;
        structures += (counts && !open) as usize;
        open = counts;
    }
    (structures >= 3).then_some(Lean { sigma: lean.sigma.max(0.2), ..lean })
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Verdict {
    Level { angle: f32, by: Source },

    Offer { angle: f32, by: Source },
    Refused(Refusal),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Refusal {

    NothingToGoBy,

    Disagree,

    Deliberate(f32),
    AlreadyLevel,
}

pub struct Frame {
    pub width: f32,
    pub height: f32,
    pub perspective: Perspective,
    pub current: f32,
    pub pitch: Option<(Lean, f32)>,
}

pub fn decide(evidence: &[Evidence], frame: &Frame) -> Verdict {
    let readings: Vec<(Evidence, f32)> =
        evidence.iter().map(|e| (*e, e.angle(frame.perspective, frame.width, frame.height))).collect();
    let agree = |a: &(Evidence, f32), b: &(Evidence, f32)| (a.1 - b.1).abs() <= 3.0 * a.0.sigma.hypot(b.0.sigma);

    let kept: Vec<(Evidence, f32)> = readings
        .iter()
        .filter(|r| r.0.source != Source::Water || readings.iter().any(|o| o.0.source != Source::Water && agree(r, o)))
        .copied()
        .collect();
    if kept.is_empty() {
        return Verdict::Refused(Refusal::NothingToGoBy);
    }
    let consistent = kept.iter().all(|a| kept.iter().all(|b| agree(a, b)));
    let used: Vec<(Evidence, f32)> = match consistent {
        true => kept,

        false => match kept.iter().find(|r| r.0.source == Source::Sea && where_the_pitch_says(&r.0, frame)) {
            Some(sea) => vec![*sea],
            None => return Verdict::Refused(Refusal::Disagree),
        },
    };
    let best = used.iter().min_by(|a, b| a.0.sigma.total_cmp(&b.0.sigma)).expect("not empty");
    if best.0.sigma > 0.5 {
        return Verdict::Refused(Refusal::NothingToGoBy);
    }
    let weight: f32 = used.iter().map(|r| r.0.sigma.powi(-2)).sum();
    let angle = used.iter().map(|r| r.1 * r.0.sigma.powi(-2)).sum::<f32>() / weight;
    let by = best.0.source;

    if (angle - frame.current).abs() < 0.15 {
        return Verdict::Refused(Refusal::AlreadyLevel);
    }
    if angle.abs() > 8.0 {
        let sea = used.iter().any(|r| r.0.source == Source::Sea);
        return match sea && angle.abs() <= 15.0 {
            true => Verdict::Offer { angle, by },
            false => Verdict::Refused(Refusal::Deliberate(angle)),
        };
    }

    if used.iter().all(|r| r.0.source == Source::Verticals) {
        return Verdict::Offer { angle, by };
    }
    Verdict::Level { angle, by }
}

fn where_the_pitch_says(sea: &Evidence, frame: &Frame) -> bool {
    let (Reading::Line([_, v0, _, v1]), Some((lean, focal))) = (sea.reading, frame.pitch) else { return false };
    if lean.reach.abs() < 1e-4 {
        return false;
    }
    let half = frame.height / 2.0;
    let point = half / lean.reach;
    let horizon = 0.5 + (-focal * focal / point) / frame.height;
    ((v0 + v1) / 2.0 - horizon).abs() <= 0.02
}
