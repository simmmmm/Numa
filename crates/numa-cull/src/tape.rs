pub const NEAR: i64 = 2;

pub const LONE: i64 = 3;

pub const LONG_BURST: usize = 8;

pub const PACE_MS: u32 = 167;
pub const LONE_MS: u32 = 500;
pub const BURST_MS: u32 = 100;

pub const LABEL_GAP: i64 = 5 * 60;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Layout {

    pub x: Vec<f64>,

    pub width: f64,

    pub labels: Vec<usize>,
}

pub fn layout(times: &[i64], pitch: f64, widest: f64, room: f64) -> Layout {
    let doublings = (LABEL_GAP as f64 / NEAR as f64).log2();
    let mut x = Vec::with_capacity(times.len());
    let mut labels = Vec::new();
    let mut at = 0.0;
    let mut last_label = f64::NEG_INFINITY;
    for (index, &time) in times.iter().enumerate() {
        let jump = index.checked_sub(1).map_or(LABEL_GAP, |before| (time - times[before]).abs());
        if index > 0 {
            at += pitch;
            if jump > NEAR {
                at += (widest * (jump as f64 / NEAR as f64).log2() / doublings).min(widest);
            }
        }
        if jump >= LABEL_GAP && at - last_label >= room {
            labels.push(index);
            last_label = at;
        }
        x.push(at);
    }
    let width = if times.is_empty() { 0.0 } else { at + pitch };
    Layout { x, width, labels }
}

pub fn pace_ms(times: &[i64], at: usize) -> u32 {
    let apart = |a: usize, b: usize| (times[b] - times[a]).abs();
    let before = at.checked_sub(1).map(|previous| apart(previous, at));
    let after = (at + 1 < times.len()).then(|| apart(at, at + 1));
    if before.into_iter().chain(after).min().is_none_or(|nearest| nearest > LONE) {
        return LONE_MS;
    }
    let behind = (1..=at).rev().take_while(|&index| apart(index - 1, index) <= NEAR).count();
    let ahead = (at + 1..times.len()).take_while(|&index| apart(index - 1, index) <= NEAR).count();
    match behind + 1 + ahead >= LONG_BURST {
        true => BURST_MS,
        false => PACE_MS,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn frames_close_together_sit_a_pitch_apart_and_gaps_grow_to_a_cap() {

        let times = [0, 0, 1, 11, 71, 7271];
        let tape = layout(&times, 7.0, 30.0, 40.0);
        assert_eq!(&tape.x[..3], &[0.0, 7.0, 14.0]);
        let gap = |index: usize| tape.x[index] - tape.x[index - 1] - 7.0;
        assert!(gap(3) > 0.0 && gap(3) < gap(4), "ten seconds narrower than a minute");
        assert_eq!(gap(5), 30.0, "two hours no wider than five minutes");
        assert_eq!(tape.width, tape.x[5] + 7.0);

        assert_eq!(tape.labels, vec![0, 5]);
    }

    #[test]
    fn a_label_too_close_to_the_last_is_left_out() {
        let times = [0, 600, 1200];
        let tape = layout(&times, 7.0, 30.0, 70.0);

        assert_eq!(tape.labels, vec![0, 2]);
        assert!(layout(&[], 7.0, 30.0, 80.0).x.is_empty());
    }

    #[test]
    fn review_holds_single_frames_and_hurries_through_long_bursts() {
        let times = [0, 60, 61, 62, 200, 200, 200, 200, 201, 201, 202, 202, 400];
        assert_eq!(pace_ms(&times, 0), LONE_MS);
        assert_eq!(pace_ms(&times, 2), PACE_MS);
        assert_eq!(pace_ms(&times, 6), BURST_MS);
        assert_eq!(pace_ms(&times, 12), LONE_MS);
        assert_eq!(pace_ms(&[5], 0), LONE_MS);
    }
}
