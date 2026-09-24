pub mod eyes;
pub mod faces;
pub mod learn;
pub mod people;
pub mod measure;

pub const VERSION: i64 = 4;

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Frame {

    pub sharpness: f32,

    pub blown: f32,

    pub hash: u64,

    pub shape: u64,

    pub exposure: f32,
    pub focal35: f32,

    pub raw_clipped: Option<f32>,
    pub raw_dark: Option<f32>,

    pub eyes_closed: Option<bool>,

    pub brightness: f32,

    pub contrast: f32,

    pub colourfulness: f32,
}

pub const SOFT: f32 = 0.25;

const SOFT_FRACTION: f32 = 1.0 / 12.0;

pub const BLOWN: f32 = 0.10;

pub const HANDHELD: f32 = 32.0;

pub const BLANK: f32 = 0.01;

impl Frame {

    pub fn blown_fraction(&self) -> f32 {
        self.raw_clipped.unwrap_or(self.blown)
    }

    pub fn is_blown(&self) -> bool {
        self.blown_fraction() > BLOWN
    }

    pub fn stops_past_handheld(&self) -> Option<f32> {
        (self.exposure > 0.0 && self.focal35 > 0.0).then(|| (self.exposure * self.focal35).log2())
    }

    pub fn is_slow(&self) -> bool {
        self.stops_past_handheld().is_some_and(|stops| stops > HANDHELD.log2())
    }

    pub fn is_blank(&self) -> bool {
        self.contrast < BLANK
    }

    pub fn blank_note(&self) -> Option<&'static str> {
        self.is_blank().then(|| match self.brightness {
            brightness if brightness < 0.15 => "nearly black",
            brightness if brightness > 0.85 => "nearly white",
            _ => "nearly blank",
        })
    }
}

pub fn distance(a: u64, b: u64) -> u32 {
    (a ^ b).count_ones()
}

pub fn bursts(hashes: &[u64], taken: &[i64], tolerance: u32) -> Vec<usize> {
    let mut groups = Vec::with_capacity(hashes.len());
    let mut current = 0usize;
    let mut anchor = match hashes.first() {
        Some(first) => *first,
        None => return groups,
    };

    for (index, hash) in hashes.iter().enumerate() {
        let follows = index > 0
            && (taken[index] - taken[index - 1]).abs() <= BURST_GAP
            && distance(hashes[index - 1], *hash) <= BURST_STEP;
        if index > 0 && distance(anchor, *hash) > tolerance && !follows {
            current += 1;
            anchor = *hash;
        }
        groups.push(current);
    }

    groups
}

pub const BURST_GAP: i64 = 2;

pub const BURST_STEP: u32 = 12;

pub const ECHO: u32 = 24;

pub const ECHO_GAP: i64 = 120;

pub fn echoes(frames: &[Frame], groups: &[usize], taken: &[i64]) -> Vec<Option<usize>> {
    let mut nearest: Vec<Option<(u32, usize)>> = vec![None; frames.len()];
    for a in 0..frames.len() {
        if frames[a].is_blank() {
            continue;
        }
        for b in a + 1..frames.len() {
            if groups[a] == groups[b] || frames[b].is_blank() || (taken[b] - taken[a]).abs() < ECHO_GAP {
                continue;
            }
            let apart = distance(frames[a].hash, frames[b].hash) + distance(frames[a].shape, frames[b].shape);
            if apart > ECHO {
                continue;
            }
            for (from, to) in [(a, b), (b, a)] {
                if nearest[from].is_none_or(|(best, _)| apart < best) {
                    nearest[from] = Some((apart, to));
                }
            }
        }
    }
    nearest.into_iter().map(|found| found.map(|(_, index)| index)).collect()
}

pub const BURST_TOLERANCE: u32 = 8;

pub fn best_of_each(frames: &[Frame], faces: &[Option<f32>], groups: &[usize]) -> Vec<usize> {
    let mut best: Vec<Option<usize>> = Vec::new();
    let mut counts: Vec<usize> = Vec::new();
    let sharpness =
        |index: usize| faces.get(index).copied().flatten().unwrap_or(frames[index].sharpness);

    for (index, group) in groups.iter().enumerate() {
        if best.len() <= *group {
            best.resize(*group + 1, None);
            counts.resize(*group + 1, 0);
        }
        counts[*group] += 1;

        match best[*group] {
            None => best[*group] = Some(index),
            Some(current) => {
                let (a, b) = (sharpness(index), sharpness(current));
                let better = a > b || (a == b && frames[index].blown_fraction() < frames[current].blown_fraction());
                if better {
                    best[*group] = Some(index);
                }
            }
        }
    }

    best.into_iter()
        .zip(counts)
        .filter_map(|(index, count)| index.filter(|_| count > 1))
        .collect()
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Scale {
    pub dull: f32,
    pub crisp: f32,

    pub soft: f32,
}

impl Default for Scale {
    fn default() -> Self {
        Scale { dull: 0.15, crisp: 1.15, soft: SOFT }
    }
}

const SCALE_MIN_FRAMES: usize = 50;

const SCALE_MIN_SPREAD: f32 = 0.10;

impl Scale {

    pub fn of(sharpness: impl IntoIterator<Item = f32>) -> Scale {
        let mut values: Vec<f32> = sharpness.into_iter().filter(|value| value.is_finite()).collect();
        if values.len() < SCALE_MIN_FRAMES {
            return Scale::default();
        }
        values.sort_by(f32::total_cmp);

        let at = |fraction: f32| values[((values.len() - 1) as f32 * fraction).round() as usize];
        let scale = Scale { dull: at(0.05), crisp: at(0.95), soft: at(SOFT_FRACTION) };
        if scale.crisp - scale.dull < SCALE_MIN_SPREAD {
            return Scale::default();
        }
        scale
    }

    pub fn is_soft(&self, frame: &Frame) -> bool {
        frame.sharpness < self.soft
    }

    pub fn suggestion(&self, frame: &Frame, face_sharpness: Option<f32>, best_of_burst: bool) -> f32 {

        if frame.is_blank() {
            return 0.0;
        }
        let sharpness = face_sharpness.unwrap_or(frame.sharpness);
        let mut score = 5.0 * ((sharpness - self.dull) / (self.crisp - self.dull)).clamp(0.0, 1.0);

        if frame.is_blown() {
            let past = ((frame.blown_fraction() - BLOWN) / (0.40 - BLOWN)).clamp(0.0, 1.0);
            score -= 2.0 * past;
        }

        if best_of_burst {
            score += 0.4;
        }

        score.clamp(0.0, 5.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn suggestion(frame: &Frame, face_sharpness: Option<f32>, best_of_burst: bool) -> f32 {
        Scale::default().suggestion(frame, face_sharpness, best_of_burst)
    }

    fn frame(sharpness: f32, blown: f32, hash: u64) -> Frame {
        Frame { sharpness, blown, hash, contrast: 0.2, brightness: 0.4, ..Default::default() }
    }

    #[test]
    fn a_blank_frame_says_what_it_looks_like_and_scores_nothing() {
        let blank = |brightness: f32| Frame { sharpness: 3.0, contrast: 0.002, brightness, ..Default::default() };
        assert_eq!(blank(0.01).blank_note(), Some("nearly black"));
        assert_eq!(blank(0.99).blank_note(), Some("nearly white"));
        assert_eq!(blank(0.5).blank_note(), Some("nearly blank"));
        assert_eq!(suggestion(&blank(0.01), None, true), 0.0, "noise is not detail");

        let dim = Frame { contrast: 0.012, brightness: 0.03, ..frame(0.5, 0.0, 0) };
        assert_eq!(dim.blank_note(), None);
        assert!(suggestion(&dim, None, false) > 0.0);
    }

    #[test]
    fn a_run_of_the_same_scene_is_one_group() {

        let hashes = [0b0000, 0b0001, 0b0011, 0xFFFF_FFFF, 0xFFFF_FFFE];
        assert_eq!(bursts(&hashes, &apart(5), 4), vec![0, 0, 0, 1, 1]);

        assert_eq!(bursts(&hashes, &apart(5), 0), vec![0, 1, 2, 3, 4]);
        assert!(bursts(&[], &[], 4).is_empty());
    }

    fn apart(count: usize) -> Vec<i64> {
        (0..count as i64).map(|index| index * 60).collect()
    }

    #[test]
    fn a_burst_that_moves_is_still_one_burst() {

        let hashes: Vec<u64> = (0..7).map(|step| (1u64 << (step * 2)) - 1).collect();
        let quick: Vec<i64> = (0..7).collect();
        let groups = bursts(&hashes, &quick, 4);
        assert!(groups.iter().all(|group| *group == 0), "{groups:?}");

        assert_eq!(bursts(&[0, u64::MAX], &[0, 1], 4), vec![0, 1]);
    }

    #[test]
    fn a_slow_pan_does_not_drift_one_burst_into_the_next() {

        let hashes: Vec<u64> = (0..7).map(|step| (1u64 << (step * 2)) - 1).collect();
        let groups = bursts(&hashes, &apart(7), 4);

        assert_ne!(
            groups.first(),
            groups.last(),
            "comparing to the neighbour would have made this all one burst: {groups:?}"
        );
    }

    #[test]
    fn the_best_of_a_run_is_the_sharpest() {
        let frames = [
            frame(0.4, 0.0, 0),
            frame(0.9, 0.0, 0),
            frame(0.6, 0.0, 0),
            frame(0.5, 0.0, 1),
        ];
        let groups = vec![0, 0, 0, 1];
        assert_eq!(best_of_each(&frames, &[], &groups), vec![1], "the run of one has no best");

        let frames = [frame(0.8, 0.30, 0), frame(0.8, 0.02, 0)];
        assert_eq!(best_of_each(&frames, &[], &[0, 0]), vec![1]);

        let singles = [frame(0.4, 0.0, 0), frame(0.9, 0.0, 1), frame(0.6, 0.0, 2)];
        assert!(best_of_each(&singles, &[], &[0, 1, 2]).is_empty(), "a run of one is not a burst");

        let portraits = [frame(1.1, 0.0, 0), frame(0.4, 0.0, 0)];
        let faces = [Some(0.2), Some(1.0)];
        assert_eq!(best_of_each(&portraits, &faces, &[0, 0]), vec![1], "the face has to win here too");
    }

    #[test]
    fn the_verdicts_are_the_only_place_a_threshold_lives() {
        let reference = Scale::default();
        assert!(reference.is_soft(&frame(SOFT - 0.01, 0.0, 0)));
        assert!(!reference.is_soft(&frame(SOFT + 0.01, 0.0, 0)));
        assert!(frame(1.0, BLOWN + 0.001, 0).is_blown());
        assert!(!frame(1.0, BLOWN - 0.001, 0).is_blown());
    }

    #[test]
    fn soft_is_a_fraction_of_the_library_it_is_asked_about() {

        let measured: Vec<f32> = (0..200).map(|step| 0.26 + step as f32 * 0.002).collect();
        let scale = Scale::of(measured.iter().copied());

        let flagged = measured.iter().filter(|value| scale.is_soft(&frame(**value, 0.0, 0))).count();
        assert!(
            (12..=22).contains(&flagged),
            "meant to be about one in twelve of 200, got {flagged}"
        );

        assert_eq!(measured.iter().filter(|value| **value < SOFT).count(), 0);
    }

    #[test]
    fn a_sharp_face_beats_a_sharp_background() {

        let crisp_frame = frame(1.1, 0.0, 0);
        assert!(
            suggestion(&crisp_frame, Some(0.2), false) < suggestion(&crisp_frame, None, false) / 2.0,
            "the face has to win"
        );

        let soft_frame = frame(0.2, 0.0, 0);
        assert!(suggestion(&soft_frame, Some(1.1), false) > 4.0);
    }

    #[test]
    fn the_suggestion_stays_inside_the_star_scale() {

        for sharpness in [0.0, 0.15, 0.6, 1.15, 99.0] {
            for blown in [0.0, 0.05, 0.4, 1.0] {
                for best in [false, true] {
                    let score = suggestion(&frame(sharpness, blown, 0), None, best);
                    assert!((0.0..=5.0).contains(&score), "{sharpness}/{blown}/{best} -> {score}");
                }
            }
        }
    }

    #[test]
    fn blown_highlights_cost_stars_and_the_pick_of_a_run_gains_one() {
        let clean = suggestion(&frame(0.9, 0.0, 0), None, false);
        let ruined = suggestion(&frame(0.9, 0.45, 0), None, false);
        assert!(clean - ruined > 1.5, "{clean} against {ruined}");

        assert_eq!(suggestion(&frame(0.9, BLOWN - 0.01, 0), None, false), clean);

        assert!(suggestion(&frame(0.9, 0.0, 0), None, true) > clean);
    }

    #[test]
    fn a_library_is_scored_against_its_own_ends() {

        let measured: Vec<f32> = (0..100).map(|step| 0.10 + step as f32 * 0.005).collect();
        let scale = Scale::of(measured.iter().copied());
        assert_ne!(scale, Scale::default(), "a hundred frames speak for themselves");

        let best = frame(0.59, 0.0, 0);
        assert!(suggestion(&best, None, false) < 2.5, "the old yardstick, for contrast");
        assert!(
            scale.suggestion(&best, None, false) > 4.0,
            "the sharpest frame of the library has to read as one: {}",
            scale.suggestion(&best, None, false)
        );

        assert!(scale.suggestion(&best, None, false) > scale.suggestion(&frame(0.11, 0.0, 0), None, false));

        assert_eq!(Scale::of(measured.iter().copied().take(20)), Scale::default());
        assert_eq!(Scale::of(std::iter::repeat(0.4).take(200)), Scale::default());
    }

    #[test]
    fn the_same_scene_later_is_an_echo() {
        let at = |hash: u64, shape: u64| Frame { hash, shape, ..frame(0.5, 0.0, 0) };
        let frames = [at(0, 0), at(0b111, 0b111), at(u64::MAX, u64::MAX), at(0b1, 0b1), at(0, 0)];
        let groups = [0, 1, 2, 3, 4];

        let taken = [0, 600, 4200, 4230, 9000];
        let echoes = echoes(&frames, &groups, &taken);
        assert_eq!(echoes[0], Some(4), "the same scene much later, nearest of the two");
        assert_eq!(echoes[2], None, "a different scene");
        assert_eq!(echoes[3], Some(0), "the nearest look, not the nearest in time");

        let echoes_close = super::echoes(&frames[2..4], &[0, 1], &[0, 30]);
        assert_eq!(echoes_close, vec![None, None]);

        let blank = Frame { contrast: 0.0, ..at(0, 0) };
        assert_eq!(super::echoes(&[blank, blank], &[0, 1], &[0, 9000]), vec![None, None], "blanks all hash alike");
    }

    #[test]
    fn slow_is_five_stops_past_one_over_the_focal_length() {
        let at = |exposure: f32, focal35: f32| Frame { exposure, focal35, ..frame(0.5, 0.0, 0) };
        assert!(!at(1.0 / 30.0, 450.0).is_slow(), "four stops past, which stabilisation held");
        assert!(at(0.25, 450.0).is_slow(), "a quarter second at 450 mm");
        assert!(!at(0.25, 0.0).is_slow() && at(0.25, 0.0).stops_past_handheld().is_none(), "no focal length, no opinion");
        let stops = at(0.25, 450.0).stops_past_handheld().unwrap();
        assert!((stops - 6.81).abs() < 0.01, "{stops}");
    }

    #[test]
    fn blown_is_the_raw_where_there_is_one() {
        let sky = Frame { blown: 0.36, raw_clipped: Some(0.0), ..frame(0.9, 0.0, 0) };
        assert!(!sky.is_blown(), "white in the JPEG, whole in the raw");
        assert_eq!(suggestion(&sky, None, false), suggestion(&frame(0.9, 0.0, 0), None, false));

        let gone = Frame { blown: 0.05, raw_clipped: Some(0.3), ..frame(0.9, 0.0, 0) };
        assert!(gone.is_blown());
        assert!(frame(0.9, 0.36, 0).is_blown(), "no raw: the preview is all there is");
    }

    #[test]
    fn distance_counts_differing_bits() {
        assert_eq!(distance(0, 0), 0);
        assert_eq!(distance(0b1011, 0b0001), 2);
        assert_eq!(distance(u64::MAX, 0), 64);
    }
}
