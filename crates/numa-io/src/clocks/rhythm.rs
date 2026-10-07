use std::collections::{HashMap, HashSet};

use super::{Moment, WINDOW};

const SMOOTH: i64 = 2;

const SHOULDER: i64 = 15;

pub const STANDS_OUT: f64 = 2.0;

const COINCIDE: usize = 10;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rhythm {

    pub lag: i64,

    pub over_next: f64,
    pub over_now: f64,

    pub coincide: usize,
    pub pair: (i64, i64),
}

impl Rhythm {

    pub fn clear(&self) -> bool {
        self.over_next >= STANDS_OUT && self.over_now >= STANDS_OUT && self.coincide >= COINCIDE
    }
}

pub fn rhythm(ours: &[&Moment], theirs: &[&Moment]) -> Option<Rhythm> {
    let mut ours = ours.to_vec();
    ours.sort_by_key(|moment| moment.taken);
    let ours = ours.as_slice();
    let day = |moment: &&Moment| moment.taken.div_euclid(86_400);
    let shared: HashSet<i64> = ours.iter().map(day).collect::<HashSet<_>>().intersection(&theirs.iter().map(day).collect()).copied().collect();
    let theirs: Vec<&Moment> = theirs.iter().copied().filter(|moment| shared.contains(&day(moment))).collect();
    if theirs.is_empty() {
        return None;
    }

    let reach = WINDOW + SMOOTH;
    let mut raw = vec![0u32; (2 * reach + 1) as usize];
    for theirs in &theirs {
        let from = ours.partition_point(|moment| moment.taken < theirs.taken - reach);
        for moment in ours[from..].iter().take_while(|moment| moment.taken <= theirs.taken + reach) {
            raw[(moment.taken - theirs.taken + reach) as usize] += 1;
        }
    }
    let at = |lag: i64| raw[(lag + reach) as usize];
    let smooth: Vec<f64> = (-WINDOW..=WINDOW)
        .map(|lag| (-SMOOTH..=SMOOTH).map(|k| f64::from(at(lag + k)) * (SMOOTH + 1 - k.abs()) as f64).sum())
        .collect();
    let score = |lag: i64| smooth[(lag + WINDOW) as usize];

    let mut sorted = smooth.clone();
    sorted.sort_by(f64::total_cmp);
    let usual = sorted[sorted.len() / 2];
    let lag = (-WINDOW..=WINDOW).max_by(|a, b| score(*a).total_cmp(&score(*b)).then(b.abs().cmp(&a.abs())))?;
    let next = (-WINDOW..=WINDOW).filter(|other| (other - lag).abs() > SHOULDER).map(score).fold(f64::MIN, f64::max);
    let over = |other: f64| match other - usual {
        below if below <= 0.0 => f64::INFINITY,
        above => (score(lag) - usual) / above,
    };
    let coincide = theirs
        .iter()
        .filter(|moment| {
            let from = ours.partition_point(|ours| ours.taken < moment.taken + lag - SMOOTH);
            ours.get(from).is_some_and(|ours| ours.taken <= moment.taken + lag + SMOOTH)
        })
        .count();
    Some(Rhythm { lag, over_next: over(next), over_now: over(score(0)), coincide, pair: busiest(ours, &theirs, lag) })
}

fn busiest(ours: &[&Moment], theirs: &[&Moment], lag: i64) -> (i64, i64) {
    let near = |moment: &Moment| {
        let at = moment.taken + lag;
        let from = ours.partition_point(|ours| ours.taken < at - SMOOTH);
        ours[from..].iter().take_while(|ours| ours.taken <= at + SMOOTH).copied().collect::<Vec<_>>()
    };
    let mut counted: HashMap<i64, (usize, i64)> = HashMap::new();
    for moment in theirs {
        let close = near(moment);
        if let Some(closest) = close.iter().min_by_key(|ours| (ours.taken - moment.taken - lag).abs()) {
            counted.insert(moment.id, (close.len(), closest.id));
        }
    }
    counted
        .into_iter()
        .max_by_key(|(id, (count, _))| (*count, -id))
        .map_or((0, 0), |(id, (_, closest))| (closest, id))
}
