pub const NEAR: i64 = 2;

pub const LONE: i64 = 3;

pub const LONG_BURST: usize = 8;

pub const PACE_MS: u32 = 167;
pub const LONE_MS: u32 = 500;
pub const BURST_MS: u32 = 100;

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
    fn review_holds_single_frames_and_hurries_through_long_bursts() {
        let times = [0, 60, 61, 62, 200, 200, 200, 200, 201, 201, 202, 202, 400];
        assert_eq!(pace_ms(&times, 0), LONE_MS);
        assert_eq!(pace_ms(&times, 2), PACE_MS);
        assert_eq!(pace_ms(&times, 6), BURST_MS);
        assert_eq!(pace_ms(&times, 12), LONE_MS);
        assert_eq!(pace_ms(&[5], 0), LONE_MS);
    }
}
