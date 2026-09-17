pub mod faces;
pub mod learn;
pub mod people;
pub mod measure;

pub const VERSION: i64 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct Frame {

    pub sharpness: f32,

    pub blown: f32,

    pub hash: u64,

    pub brightness: f32,

    pub contrast: f32,

    pub colourfulness: f32,
}

pub const SOFT: f32 = 0.25;

pub const BLOWN: f32 = 0.10;

impl Frame {

    pub fn is_soft(&self) -> bool {
        self.sharpness < SOFT
    }

    pub fn is_blown(&self) -> bool {
        self.blown > BLOWN
    }
}

pub fn distance(a: u64, b: u64) -> u32 {
    (a ^ b).count_ones()
}

pub fn bursts(hashes: &[u64], tolerance: u32) -> Vec<usize> {
    let mut groups = Vec::with_capacity(hashes.len());
    let mut current = 0usize;
    let mut anchor = match hashes.first() {
        Some(first) => *first,
        None => return groups,
    };

    for (index, hash) in hashes.iter().enumerate() {
        if index > 0 && distance(anchor, *hash) > tolerance {
            current += 1;
            anchor = *hash;
        }
        groups.push(current);
    }

    groups
}

pub const BURST_TOLERANCE: u32 = 8;

pub fn best_of_each(frames: &[Frame], groups: &[usize]) -> Vec<usize> {
    let mut best: Vec<Option<usize>> = Vec::new();
    let mut counts: Vec<usize> = Vec::new();

    for (index, group) in groups.iter().enumerate() {
        if best.len() <= *group {
            best.resize(*group + 1, None);
            counts.resize(*group + 1, 0);
        }
        counts[*group] += 1;

        match best[*group] {
            None => best[*group] = Some(index),
            Some(current) => {
                let (a, b) = (&frames[index], &frames[current]);
                let better = a.sharpness > b.sharpness
                    || (a.sharpness == b.sharpness && a.blown < b.blown);
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

pub fn suggestion(frame: &Frame, face_sharpness: Option<f32>, best_of_burst: bool) -> f32 {

    const DULL: f32 = 0.15;
    const CRISP: f32 = 1.15;

    let sharpness = face_sharpness.unwrap_or(frame.sharpness);
    let mut score = 5.0 * ((sharpness - DULL) / (CRISP - DULL)).clamp(0.0, 1.0);

    if frame.blown > BLOWN {
        let past = ((frame.blown - BLOWN) / (0.40 - BLOWN)).clamp(0.0, 1.0);
        score -= 2.0 * past;
    }

    if best_of_burst {
        score += 0.4;
    }

    score.clamp(0.0, 5.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn frame(sharpness: f32, blown: f32, hash: u64) -> Frame {
        Frame { sharpness, blown, hash, ..Default::default() }
    }

    #[test]
    fn a_run_of_the_same_scene_is_one_group() {

        let hashes = [0b0000, 0b0001, 0b0011, 0xFFFF_FFFF, 0xFFFF_FFFE];
        assert_eq!(bursts(&hashes, 4), vec![0, 0, 0, 1, 1]);

        assert_eq!(bursts(&hashes, 0), vec![0, 1, 2, 3, 4]);
        assert!(bursts(&[], 4).is_empty());
    }

    #[test]
    fn a_slow_pan_does_not_drift_one_burst_into_the_next() {

        let hashes: Vec<u64> = (0..7).map(|step| (1u64 << (step * 2)) - 1).collect();
        let groups = bursts(&hashes, 4);

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
        assert_eq!(best_of_each(&frames, &groups), vec![1], "the run of one has no best");

        let frames = [frame(0.8, 0.30, 0), frame(0.8, 0.02, 0)];
        assert_eq!(best_of_each(&frames, &[0, 0]), vec![1]);

        let singles = [frame(0.4, 0.0, 0), frame(0.9, 0.0, 1), frame(0.6, 0.0, 2)];
        assert!(best_of_each(&singles, &[0, 1, 2]).is_empty(), "a run of one is not a burst");
    }

    #[test]
    fn the_verdicts_are_the_only_place_a_threshold_lives() {
        assert!(frame(SOFT - 0.01, 0.0, 0).is_soft());
        assert!(!frame(SOFT + 0.01, 0.0, 0).is_soft());
        assert!(frame(1.0, BLOWN + 0.001, 0).is_blown());
        assert!(!frame(1.0, BLOWN - 0.001, 0).is_blown());
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
    fn distance_counts_differing_bits() {
        assert_eq!(distance(0, 0), 0);
        assert_eq!(distance(0b1011, 0b0001), 2);
        assert_eq!(distance(u64::MAX, 0), 64);
    }
}
