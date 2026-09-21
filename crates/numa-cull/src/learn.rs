use std::fmt;

use crate::{Frame, Scale};

pub const FEATURES: usize = 11;

pub const MIN_RATED: usize = 30;

const MIN_DISTINCT: usize = 3;

const HOLD_OUT: usize = 4;

const MIN_HELD_OUT: usize = 8;

const RIDGE: f64 = 0.05;

#[derive(Debug, Clone, Copy)]
pub struct Sample {
    pub features: [f64; FEATURES],

    pub rule: f32,

    pub label: Option<f32>,

    pub burst: usize,
}

impl Sample {

    #[allow(clippy::too_many_arguments)]
    pub fn new(
        scale: Scale,
        frame: &Frame,
        faces: Option<u32>,
        face_sharpness: Option<f32>,
        best: bool,
        burst: usize,
        rating: u8,
        rejected: bool,
    ) -> Self {
        let rule = scale.suggestion(frame, face_sharpness, best);

        let log = |value: f32| f64::from(value.max(0.0) + 0.02).ln();
        let brightness = f64::from(frame.brightness);
        Sample {
            features: [

                f64::from(rule),
                log(frame.sharpness),
                log(face_sharpness.unwrap_or(frame.sharpness)),
                f64::from(frame.blown),
                f64::from(faces.unwrap_or(0).min(5)),
                f64::from(u8::from(face_sharpness.is_some())),
                f64::from(u8::from(best)),
                brightness,

                brightness * brightness,
                f64::from(frame.contrast),
                f64::from(frame.colourfulness),
            ],
            rule,
            label: label(rating, rejected),
            burst,
        }
    }
}

pub fn label(rating: u8, rejected: bool) -> Option<f32> {
    match (rejected, rating) {
        (true, _) => Some(0.0),
        (false, 0) => None,
        (false, stars) => Some(f32::from(stars.min(5))),
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Outcome {

    TooFew { rated: usize },

    Rule { rated: usize, learned: f64, rule: f64 },

    Learned { rated: usize, learned: f64, rule: f64 },
}

impl fmt::Display for Outcome {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {

        match *self {
            Outcome::TooFew { rated } => write!(
                f,
                "suggestions follow the rule until {MIN_RATED} photos are rated with some spread ({rated} now)"
            ),
            Outcome::Rule { rated, learned, rule } => write!(
                f,
                "suggestions follow the rule — learned from {rated} ratings ρ {learned:.2}, rule ρ {rule:.2}"
            ),
            Outcome::Learned { rated, learned, rule } => write!(
                f,
                "suggestions learned from {rated} ratings — ρ {learned:.2} against the rule's {rule:.2}"
            ),
        }
    }
}

pub fn score(samples: &[Sample]) -> (Vec<f32>, Outcome) {
    let by_rule: Vec<f32> = samples.iter().map(|sample| sample.rule).collect();
    let rated: Vec<&Sample> = samples.iter().filter(|sample| sample.label.is_some()).collect();
    let (held, fitted): (Vec<&Sample>, Vec<&Sample>) =
        rated.iter().partition(|sample| sample.burst % HOLD_OUT == 0);

    if rated.len() < MIN_RATED
        || distinct(&rated) < MIN_DISTINCT
        || held.len() < MIN_HELD_OUT
        || distinct(&held) < 2
    {
        return (by_rule, Outcome::TooFew { rated: rated.len() });
    }

    let labels: Vec<f64> = held.iter().map(|sample| f64::from(sample.label.unwrap_or_default())).collect();
    let model = Model::fit(&fitted);
    let learned = spearman(&held.iter().map(|sample| model.predict(sample)).collect::<Vec<_>>(), &labels);
    let rule = spearman(&held.iter().map(|sample| f64::from(sample.rule)).collect::<Vec<_>>(), &labels);
    if learned <= rule {
        return (by_rule, Outcome::Rule { rated: rated.len(), learned, rule });
    }

    let model = Model::fit(&rated);
    let scores = samples.iter().map(|sample| model.predict(sample).clamp(0.0, 5.0) as f32).collect();
    (scores, Outcome::Learned { rated: rated.len(), learned, rule })
}

fn distinct(samples: &[&Sample]) -> usize {
    let mut seen = [false; 6];
    for sample in samples {
        if let Some(label) = sample.label {
            seen[label.round().clamp(0.0, 5.0) as usize] = true;
        }
    }
    seen.iter().filter(|seen| **seen).count()
}

struct Model {
    mean: [f64; FEATURES],
    scale: [f64; FEATURES],
    weights: [f64; FEATURES],
    intercept: f64,
}

impl Model {
    fn fit(samples: &[&Sample]) -> Model {
        let n = samples.len().max(1) as f64;
        let mut mean = [0.0; FEATURES];
        let mut scale = [0.0; FEATURES];
        for sample in samples {
            for (i, value) in sample.features.iter().enumerate() {
                mean[i] += value / n;
            }
        }
        for sample in samples {
            for (i, value) in sample.features.iter().enumerate() {
                scale[i] += (value - mean[i]).powi(2) / n;
            }
        }

        let scale = scale.map(|variance| if variance > 1e-12 { variance.sqrt() } else { 1.0 });

        let intercept = samples.iter().map(|sample| f64::from(sample.label.unwrap_or_default())).sum::<f64>() / n;
        let mut model = Model { mean, scale, weights: [0.0; FEATURES], intercept };

        let mut gram = [[0.0; FEATURES]; FEATURES];
        let mut moment = [0.0; FEATURES];
        for (i, row) in gram.iter_mut().enumerate() {
            row[i] = RIDGE * n;
        }
        for sample in samples {
            let z = model.standardised(sample);
            let y = f64::from(sample.label.unwrap_or_default()) - intercept;
            for i in 0..FEATURES {
                moment[i] += z[i] * y;
                for j in 0..FEATURES {
                    gram[i][j] += z[i] * z[j];
                }
            }
        }
        model.weights = solve(gram, moment);
        model
    }

    fn standardised(&self, sample: &Sample) -> [f64; FEATURES] {
        std::array::from_fn(|i| (sample.features[i] - self.mean[i]) / self.scale[i])
    }

    fn predict(&self, sample: &Sample) -> f64 {
        let z = self.standardised(sample);
        self.intercept + z.iter().zip(self.weights).map(|(z, w)| z * w).sum::<f64>()
    }
}

#[allow(clippy::needless_range_loop)]
fn solve(mut a: [[f64; FEATURES]; FEATURES], mut b: [f64; FEATURES]) -> [f64; FEATURES] {
    for k in 0..FEATURES {
        for i in k + 1..FEATURES {
            let factor = a[i][k] / a[k][k];
            for j in k..FEATURES {
                a[i][j] -= factor * a[k][j];
            }
            b[i] -= factor * b[k];
        }
    }
    let mut x = [0.0; FEATURES];
    for k in (0..FEATURES).rev() {
        let known: f64 = (k + 1..FEATURES).map(|j| a[k][j] * x[j]).sum();
        x[k] = (b[k] - known) / a[k][k];
    }
    x
}

fn spearman(a: &[f64], b: &[f64]) -> f64 {
    let (a, b) = (ranks(a), ranks(b));
    let n = a.len() as f64;
    let (mean_a, mean_b) = (a.iter().sum::<f64>() / n, b.iter().sum::<f64>() / n);
    let mut cross = 0.0;
    let (mut spread_a, mut spread_b) = (0.0, 0.0);
    for (x, y) in a.iter().zip(&b) {
        cross += (x - mean_a) * (y - mean_b);
        spread_a += (x - mean_a).powi(2);
        spread_b += (y - mean_b).powi(2);
    }

    if spread_a < 1e-12 || spread_b < 1e-12 {
        return 0.0;
    }
    cross / (spread_a * spread_b).sqrt()
}

fn ranks(values: &[f64]) -> Vec<f64> {
    let mut order: Vec<usize> = (0..values.len()).collect();
    order.sort_by(|a, b| values[*a].total_cmp(&values[*b]));
    let mut ranks = vec![0.0; values.len()];
    let mut start = 0;
    while start < order.len() {
        let mut end = start;
        while end + 1 < order.len() && values[order[end + 1]] == values[order[start]] {
            end += 1;
        }
        let shared = (start + end) as f64 / 2.0 + 1.0;
        for index in &order[start..=end] {
            ranks[*index] = shared;
        }
        start = end + 1;
    }
    ranks
}

#[cfg(test)]
mod tests {
    use super::*;

    fn noise(seed: &mut u64) -> f64 {
        *seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
        (*seed >> 11) as f64 / (1u64 << 53) as f64
    }

    fn samples(count: usize, rate: impl Fn(&[f64; FEATURES], f32) -> Option<f32>) -> Vec<Sample> {
        let mut seed = 7;
        (0..count)
            .map(|burst| {
                let features = std::array::from_fn(|_| noise(&mut seed));
                let rule = (noise(&mut seed) * 5.0) as f32;
                Sample { features, rule, label: rate(&features, rule), burst }
            })
            .collect()
    }

    #[test]
    fn a_taste_in_two_features_is_learned() {

        let taste = |f: &[f64; FEATURES], _| Some((1.0 + 2.5 * f[7] + 1.5 * f[10]).round() as f32);
        let samples = samples(240, taste);
        let (scores, outcome) = score(&samples);

        let Outcome::Learned { learned, rule, .. } = outcome else {
            panic!("the taste was not learned: {outcome:?}");
        };
        assert!(learned > 0.8 && rule < 0.3, "{outcome}");

        let labels: Vec<f32> = samples.iter().map(|sample| sample.label.unwrap()).collect();
        let mean = labels.iter().sum::<f32>() / labels.len() as f32;
        let error = |guess: &dyn Fn(usize) -> f32| {
            labels.iter().enumerate().map(|(i, label)| (guess(i) - label).abs()).sum::<f32>() / labels.len() as f32
        };
        let (fitted, constant) = (error(&|i| scores[i]), error(&|_| mean));
        assert!(fitted < constant / 2.0, "{fitted} against a constant's {constant}");
    }

    #[test]
    fn too_few_ratings_learn_nothing() {
        let taste = |f: &[f64; FEATURES], _| Some((1.0 + 4.0 * f[7]).round() as f32);

        let mut few = samples(100, taste);
        for sample in few.iter_mut().skip(29) {
            sample.label = None;
        }
        let (scores, outcome) = score(&few);
        assert_eq!(outcome, Outcome::TooFew { rated: 29 });
        assert!(scores.iter().zip(&few).all(|(score, sample)| *score == sample.rule));

        let (_, outcome) = score(&samples(100, |f, _| Some(if f[7] > 0.5 { 4.0 } else { 3.0 })));
        assert_eq!(outcome, Outcome::TooFew { rated: 100 });
    }

    #[test]
    fn the_rule_is_kept_when_the_model_ranks_worse() {

        let samples = samples(240, |_, rule| Some(rule.round().clamp(1.0, 5.0)));
        let (scores, outcome) = score(&samples);

        assert!(matches!(outcome, Outcome::Rule { .. }), "{outcome:?}");
        assert!(scores.iter().zip(&samples).all(|(score, sample)| *score == sample.rule));
    }

    #[test]
    fn a_reject_is_the_lowest_verdict_and_unrated_is_none() {
        assert_eq!(label(0, false), None);
        assert_eq!(label(4, false), Some(4.0));
        assert_eq!(label(4, true), Some(0.0));
        assert_eq!(label(0, true), Some(0.0));
    }

    #[test]
    fn ties_share_their_rank() {
        assert_eq!(ranks(&[3.0, 1.0, 3.0, 2.0]), vec![3.5, 1.0, 3.5, 2.0]);
        assert!((spearman(&[1.0, 2.0, 3.0], &[10.0, 20.0, 30.0]) - 1.0).abs() < 1e-12);
        assert_eq!(spearman(&[1.0, 1.0, 1.0], &[1.0, 2.0, 3.0]), 0.0);
    }
}
