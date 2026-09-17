use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct WhiteBalance {
    pub temperature: f32,
    pub tint: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct CameraProfile {

    pub as_shot: [f32; 3],

    pub xyz_to_cam: [[f32; 3]; 3],

    pub cam_to_srgb: [[f32; 3]; 3],
}

pub const MIN_KELVIN: f32 = 2000.0;
pub const MAX_KELVIN: f32 = 25000.0;
pub const MAX_TINT: f32 = 150.0;

const TINT_SCALE: f32 = 0.05 / MAX_TINT;

impl CameraProfile {

    pub fn multipliers(&self, balance: WhiteBalance) -> [f32; 3] {
        let (x, y) = white_point_xy(balance);

        let xyz = [x / y, 1.0, (1.0 - x - y) / y];
        let camera = multiply_vector(&self.xyz_to_cam, xyz);

        let green = camera[1];
        let safe = |value: f32| {
            if value.abs() < 1e-6 { 1.0 } else { green / value }
        };
        normalise_green([safe(camera[0]), 1.0, safe(camera[2])])
    }

    pub fn as_shot_white_balance(&self) -> WhiteBalance {
        self.white_balance_for(self.as_shot)
    }

    pub fn neutral_balance(&self, srgb: [f32; 3]) -> Option<WhiteBalance> {
        let balanced = multiply_vector(&invert(&self.cam_to_srgb)?, srgb);
        if balanced.iter().any(|value| *value <= 1e-6) {
            return None;
        }
        let shot = normalise_green(self.as_shot);
        Some(self.white_balance_for([shot[0] / balanced[0], shot[1] / balanced[1], shot[2] / balanced[2]]))
    }

    fn white_balance_for(&self, multipliers: [f32; 3]) -> WhiteBalance {
        let target = normalise_green(multipliers);
        let error = |balance: WhiteBalance| {
            let m = self.multipliers(balance);
            (m[0] - target[0]).powi(2) + (m[2] - target[2]).powi(2)
        };

        const STEPS: usize = 32;
        let mut kelvin = (MIN_KELVIN, MAX_KELVIN);
        let mut tint = (-MAX_TINT, MAX_TINT);
        let mut best = (f32::MAX, WhiteBalance { temperature: 5500.0, tint: 0.0 });

        for _ in 0..4 {
            for i in 0..=STEPS {
                let temperature = kelvin.0 + (kelvin.1 - kelvin.0) * i as f32 / STEPS as f32;
                for j in 0..=STEPS {
                    let candidate = WhiteBalance {
                        temperature,
                        tint: tint.0 + (tint.1 - tint.0) * j as f32 / STEPS as f32,
                    };
                    let score = error(candidate);
                    if score < best.0 {
                        best = (score, candidate);
                    }
                }
            }

            let kelvin_window = (kelvin.1 - kelvin.0) / STEPS as f32 * 2.0;
            let tint_window = (tint.1 - tint.0) / STEPS as f32 * 2.0;
            kelvin = (
                (best.1.temperature - kelvin_window).max(MIN_KELVIN),
                (best.1.temperature + kelvin_window).min(MAX_KELVIN),
            );
            tint = (
                (best.1.tint - tint_window).max(-MAX_TINT),
                (best.1.tint + tint_window).min(MAX_TINT),
            );
        }

        best.1
    }

    pub fn transform(&self, balance: Option<WhiteBalance>) -> [[f32; 3]; 3] {
        let multipliers = match balance {
            Some(balance) => self.multipliers(balance),
            None => normalise_green(self.as_shot),
        };

        let mut matrix = self.cam_to_srgb;
        for row in matrix.iter_mut() {
            for (column, value) in row.iter_mut().enumerate() {
                *value *= multipliers[column];
            }
        }
        matrix
    }
}

fn normalise_green(coefficients: [f32; 3]) -> [f32; 3] {
    let green = if coefficients[1].abs() < 1e-6 { 1.0 } else { coefficients[1] };
    [coefficients[0] / green, 1.0, coefficients[2] / green]
}

fn invert(m: &[[f32; 3]; 3]) -> Option<[[f32; 3]; 3]> {
    let cofactor = |r0: usize, r1: usize, c0: usize, c1: usize| m[r0][c0] * m[r1][c1] - m[r0][c1] * m[r1][c0];
    let det = m[0][0] * cofactor(1, 2, 1, 2) - m[0][1] * cofactor(1, 2, 0, 2) + m[0][2] * cofactor(1, 2, 0, 1);
    if det.abs() < 1e-9 {
        return None;
    }
    Some([
        [cofactor(1, 2, 1, 2) / det, -cofactor(0, 2, 1, 2) / det, cofactor(0, 1, 1, 2) / det],
        [-cofactor(1, 2, 0, 2) / det, cofactor(0, 2, 0, 2) / det, -cofactor(0, 1, 0, 2) / det],
        [cofactor(1, 2, 0, 1) / det, -cofactor(0, 2, 0, 1) / det, cofactor(0, 1, 0, 1) / det],
    ])
}

fn multiply_vector(matrix: &[[f32; 3]; 3], vector: [f32; 3]) -> [f32; 3] {
    let mut out = [0.0; 3];
    for (index, row) in matrix.iter().enumerate() {
        out[index] = row[0] * vector[0] + row[1] * vector[1] + row[2] * vector[2];
    }
    out
}

fn white_point_xy(balance: WhiteBalance) -> (f32, f32) {
    let kelvin = balance.temperature.clamp(MIN_KELVIN, MAX_KELVIN);
    let (x, y) = planckian_xy(kelvin);

    if balance.tint == 0.0 {
        return (x, y);
    }

    let (u, v) = xy_to_uv(x, y);

    let step = if kelvin + 100.0 <= MAX_KELVIN { 100.0 } else { -100.0 };
    let (u2, v2) = {
        let (nx, ny) = planckian_xy(kelvin + step);
        xy_to_uv(nx, ny)
    };

    let (du, dv) = ((u2 - u) * step.signum(), (v2 - v) * step.signum());
    let length = (du * du + dv * dv).sqrt().max(1e-9);

    let (nu, nv) = (-dv / length, du / length);

    let offset = balance.tint.clamp(-MAX_TINT, MAX_TINT) * TINT_SCALE;
    uv_to_xy(u + nu * offset, v + nv * offset)
}

fn planckian_xy(kelvin: f32) -> (f32, f32) {
    let t = kelvin as f64;
    let (t1, t2, t3) = (1.0e3 / t, 1.0e6 / (t * t), 1.0e9 / (t * t * t));

    let x = if t <= 4000.0 {
        -0.266_123_9 * t3 - 0.234_358_9 * t2 + 0.877_695_6 * t1 + 0.179_910
    } else {
        -3.025_846_9 * t3 + 2.107_037_9 * t2 + 0.222_634_7 * t1 + 0.240_390
    };

    let y = if t <= 2222.0 {
        -1.106_381_4 * x * x * x - 1.348_110_2 * x * x + 2.185_558_32 * x - 0.202_196_83
    } else if t <= 4000.0 {
        -0.954_947_6 * x * x * x - 1.374_185_93 * x * x + 2.091_370_15 * x - 0.167_488_67
    } else {
        3.081_758_0 * x * x * x - 5.873_386_7 * x * x + 3.751_129_97 * x - 0.370_014_83
    };

    (x as f32, y as f32)
}

fn xy_to_uv(x: f32, y: f32) -> (f32, f32) {
    let denominator = (-2.0 * x + 12.0 * y + 3.0).max(1e-9);
    (4.0 * x / denominator, 6.0 * y / denominator)
}

fn uv_to_xy(u: f32, v: f32) -> (f32, f32) {
    let denominator = (2.0 * u - 8.0 * v + 4.0).max(1e-9);
    (3.0 * u / denominator, 2.0 * v / denominator)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tint_still_moves_the_white_point_at_the_kelvin_ceiling() {
        let plain = white_point_xy(WhiteBalance { temperature: MAX_KELVIN, tint: 0.0 });
        let tinted = white_point_xy(WhiteBalance { temperature: MAX_KELVIN, tint: 50.0 });
        let moved = ((tinted.0 - plain.0).powi(2) + (tinted.1 - plain.1).powi(2)).sqrt();
        assert!(moved > 1e-4, "tint moved the white point by {moved}");

        let below = white_point_xy(WhiteBalance { temperature: MAX_KELVIN - 100.0, tint: 0.0 });
        let below_tinted = white_point_xy(WhiteBalance { temperature: MAX_KELVIN - 100.0, tint: 50.0 });
        let dot = (tinted.0 - plain.0) * (below_tinted.0 - below.0)
            + (tinted.1 - plain.1) * (below_tinted.1 - below.1);
        assert!(dot > 0.0, "tint flipped direction at the ceiling");
    }

    fn profile() -> CameraProfile {
        let xyz_to_cam = [
            [0.6058, -0.1889, -0.0645],
            [-0.4797, 1.2681, 0.2447],
            [-0.0665, 0.0873, 0.6779],
        ];
        CameraProfile {
            as_shot: [1.85, 1.0, 1.52],
            xyz_to_cam,
            cam_to_srgb: [[1.6, -0.5, -0.1], [-0.2, 1.5, -0.3], [0.0, -0.4, 1.4]],
        }
    }

    #[test]
    fn a_neutral_under_known_light_gives_that_light_back() {
        let profile = profile();
        for light in [
            WhiteBalance { temperature: 3200.0, tint: 0.0 },
            WhiteBalance { temperature: 7500.0, tint: 20.0 },
        ] {

            let gains = profile.multipliers(light);
            let raw = gains.map(|gain| 0.2 / gain);
            let shown = multiply_vector(&profile.transform(None), raw);
            let found = profile.neutral_balance(shown).expect("a measurable grey");
            assert!((found.temperature - light.temperature).abs() < light.temperature * 0.02, "{found:?} for {light:?}");
            assert!((found.tint - light.tint).abs() < 3.0, "{found:?} for {light:?}");
        }
        assert!(profile.neutral_balance([0.0, 0.0, 0.0]).is_none(), "black measures nothing");
    }

    #[test]
    fn warmer_light_needs_less_red() {
        let profile = profile();
        let warm = profile.multipliers(WhiteBalance { temperature: 2800.0, tint: 0.0 });
        let cool = profile.multipliers(WhiteBalance { temperature: 9000.0, tint: 0.0 });

        assert!(warm[0] < cool[0], "red multiplier: warm {} cool {}", warm[0], cool[0]);
        assert!(warm[2] > cool[2], "blue multiplier: warm {} cool {}", warm[2], cool[2]);

        assert_eq!(warm[1], 1.0);
        assert_eq!(cool[1], 1.0);
    }

    #[test]
    fn as_shot_round_trips() {
        let profile = profile();
        let balance = profile.as_shot_white_balance();

        assert!(
            (MIN_KELVIN..=MAX_KELVIN).contains(&balance.temperature),
            "temperature {} is outside the valid range",
            balance.temperature
        );

        let recovered = profile.multipliers(balance);
        let target = normalise_green(profile.as_shot);
        for channel in 0..3 {
            assert!(
                (recovered[channel] - target[channel]).abs() < 0.05,
                "channel {channel}: recovered {:?} vs as shot {:?}",
                recovered,
                target
            );
        }
    }

    #[test]
    fn tint_moves_green_against_the_others() {
        let profile = profile();
        let neutral = profile.multipliers(WhiteBalance { temperature: 5500.0, tint: 0.0 });
        let green = profile.multipliers(WhiteBalance { temperature: 5500.0, tint: -100.0 });
        let magenta = profile.multipliers(WhiteBalance { temperature: 5500.0, tint: 100.0 });

        assert!(green != neutral && magenta != neutral);
        let red_shift = (magenta[0] - neutral[0]).signum();
        let blue_shift = (magenta[2] - neutral[2]).signum();
        assert_eq!(red_shift, blue_shift, "tint must push red and blue together");
    }

    #[test]
    fn transform_without_a_balance_is_the_as_shot_transform() {
        let profile = profile();
        let as_shot = profile.transform(None);
        let explicit = profile.transform(Some(profile.as_shot_white_balance()));

        for row in 0..3 {
            for column in 0..3 {
                let (a, b) = (as_shot[row][column], explicit[row][column]);
                assert!(
                    (a - b).abs() <= 0.02 * a.abs().max(1.0),
                    "as shot {as_shot:?} vs recovered {explicit:?}"
                );
            }
        }
    }

    #[test]
    fn locus_stays_in_the_visible_range() {
        for kelvin in [MIN_KELVIN, 2800.0, 4000.0, 5500.0, 6500.0, 12000.0, MAX_KELVIN] {
            let (x, y) = planckian_xy(kelvin);
            assert!((0.2..0.6).contains(&x), "{kelvin}K gave x = {x}");
            assert!((0.2..0.45).contains(&y), "{kelvin}K gave y = {y}");

            let (u, v) = xy_to_uv(x, y);
            let (rx, ry) = uv_to_xy(u, v);
            assert!((rx - x).abs() < 1e-4 && (ry - y).abs() < 1e-4);
        }
    }
}
