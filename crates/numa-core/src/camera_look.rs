use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CameraLook {
    #[serde(default = "full")]
    pub strength: f32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fit: Option<LookFit>,
}

fn full() -> f32 {
    100.0
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LookFit {
    pub terms: Vec<f32>,
    pub knee: f32,
}

pub const TERMS: usize = 20;

fn features(rgb: [f32; 3]) -> [f64; TERMS] {
    let [r, g, b] = rgb.map(f64::from);
    [1.0, r, g, b, r * r, g * g, b * b, r * g, r * b, g * b, r * r * r, g * g * g, b * b * b, r * g * b, r * r * g, r * r * b, g * g * r, g * g * b, b * b * r, b * b * g]
}

impl LookFit {

    pub fn identity() -> Self {
        let mut terms = vec![0.0; 3 * TERMS];
        for channel in 0..3 {
            terms[channel * TERMS + 1 + channel] = 1.0;
        }
        Self { terms, knee: 1.0 }
    }

    fn polynomial(&self, rgb: [f32; 3]) -> [f32; 3] {
        let f = features(rgb);
        std::array::from_fn(|channel| {
            let row = &self.terms[channel * TERMS..][..TERMS];
            row.iter().zip(f).map(|(beta, x)| *beta as f64 * x).sum::<f64>() as f32
        })
    }

    pub fn apply(&self, rgb: [f32; 3]) -> [f32; 3] {
        if self.terms.len() != 3 * TERMS {
            return rgb;
        }
        let top = rgb[0].max(rgb[1]).max(rgb[2]);
        let knee = self.knee.clamp(0.05, 1.0);
        let out = if top <= knee {
            self.polynomial(rgb)
        } else {

            let at = rgb.map(|value| value * knee / top);
            let there = self.polynomial(at).map(|value| value.clamp(0.0, 1.0));
            let fade = ((1.0 - top) / (1.0 - knee).max(1e-6)).clamp(0.0, 1.0);
            std::array::from_fn(|channel| rgb[channel] + (there[channel] - at[channel]) * fade)
        };
        out.map(|value| value.clamp(0.0, 1.0))
    }

    pub fn bake(&self) -> crate::lut::Lut {
        crate::lut::Lut::from_fn(33, |rgb| self.apply(rgb))
    }
}

pub const ANCHOR_SHARE: f64 = 0.005;

pub const KNEE_FLOOR: f32 = 0.05;

pub fn fit(pairs: &[([f32; 3], [f32; 3])]) -> Option<LookFit> {
    fit_with(pairs, ANCHOR_SHARE)
}

pub fn fit_with(pairs: &[([f32; 3], [f32; 3])], anchor_share: f64) -> Option<LookFit> {
    if pairs.len() < 200 {
        return None;
    }
    let mut tops: Vec<f32> = pairs.iter().map(|(ours, _)| ours[0].max(ours[1]).max(ours[2])).collect();
    let at = ((tops.len() as f32 * 0.995) as usize).min(tops.len() - 1);
    let knee = *tops.select_nth_unstable_by(at, f32::total_cmp).1;

    let mut normal = [[0.0f64; TERMS]; TERMS];
    let mut right = [[0.0f64; 3]; TERMS];
    let mut add = |rgb: [f32; 3], target: [f32; 3], weight: f64| {
        let f = features(rgb);
        for i in 0..TERMS {
            let wi = weight * f[i];
            for j in i..TERMS {
                normal[i][j] += wi * f[j];
            }
            for channel in 0..3 {
                right[i][channel] += wi * target[channel] as f64;
            }
        }
    };
    for (ours, camera) in pairs {
        add(*ours, *camera, 1.0);
    }
    const GRID: usize = 6;
    let anchor = anchor_share * pairs.len() as f64 / (GRID * GRID * GRID) as f64;
    for index in 0..GRID * GRID * GRID {
        let rgb = [index % GRID, (index / GRID) % GRID, index / (GRID * GRID)].map(|step| step as f32 / (GRID - 1) as f32);
        add(rgb, rgb, anchor);
    }
    for i in 0..TERMS {
        for j in 0..i {
            normal[i][j] = normal[j][i];
        }
    }

    let trace: f64 = (0..TERMS).map(|i| normal[i][i]).sum();
    for (i, row) in normal.iter_mut().enumerate() {
        row[i] += 1e-9 * trace;
    }
    let beta = solve(normal, right)?;
    let terms = (0..3).flat_map(|channel| (0..TERMS).map(move |i| (channel, i))).map(|(channel, i)| beta[i][channel] as f32).collect();
    Some(LookFit { terms, knee: knee.clamp(KNEE_FLOOR, 1.0) })
}

fn solve(mut a: [[f64; TERMS]; TERMS], b: [[f64; 3]; TERMS]) -> Option<[[f64; 3]; TERMS]> {
    for j in 0..TERMS {
        let mut d = a[j][j];
        for k in 0..j {
            d -= a[j][k] * a[j][k];
        }
        if !(d > 0.0) {
            return None;
        }
        a[j][j] = d.sqrt();
        for i in j + 1..TERMS {
            let mut s = a[i][j];
            for k in 0..j {
                s -= a[i][k] * a[j][k];
            }
            a[i][j] = s / a[j][j];
        }
    }
    let mut x = b;
    for channel in 0..3 {
        for i in 0..TERMS {
            let mut s = x[i][channel];
            for k in 0..i {
                s -= a[i][k] * x[k][channel];
            }
            x[i][channel] = s / a[i][i];
        }
        for i in (0..TERMS).rev() {
            let mut s = x[i][channel];
            for k in i + 1..TERMS {
                s -= a[k][i] * x[k][channel];
            }
            x[i][channel] = s / a[i][i];
        }
    }
    Some(x)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cube(steps: usize) -> Vec<[f32; 3]> {
        (0..steps * steps * steps).map(|i| [i % steps, (i / steps) % steps, i / (steps * steps)].map(|s| s as f32 / (steps - 1) as f32)).collect()
    }

    #[test]
    fn a_smooth_look_is_found_again() {
        let look = |p: [f32; 3]| [0.03 + 0.95 * p[0] + 0.02 * p[1] * p[1], 0.02 + 0.9 * p[1] + 0.05 * p[0] * p[2], 0.01 + 0.85 * p[2]];
        let pairs: Vec<_> = cube(17).into_iter().map(|p| (p, look(p))).collect();
        let fit = fit_with(&pairs, 0.0).unwrap();
        for (ours, camera) in pairs.iter().step_by(97) {
            let got = fit.apply(*ours);
            assert!((0..3).all(|c| (got[c] - camera[c]).abs() < 2e-3), "{ours:?} → {got:?}, wanted {camera:?}");
        }
    }

    #[test]
    fn a_grey_frame_leaves_colours_alone() {
        let pairs: Vec<_> = (0..2000).map(|i| {
            let v = 0.05 + 0.85 * i as f32 / 2000.0;
            ([v; 3], [(v * 1.1).min(1.0); 3])
        }).collect();
        let fit = fit(&pairs).unwrap();
        for colour in [[0.9, 0.1, 0.1], [0.1, 0.8, 0.2], [0.2, 0.2, 0.9], [0.9, 0.8, 0.1]] {
            let got = fit.apply(colour);
            let moved = (0..3).map(|c| (got[c] - colour[c]).abs()).fold(0.0, f32::max);
            assert!(moved < 0.15, "{colour:?} → {got:?}");
        }

        let grey = fit.apply([0.5; 3]);
        assert!((grey[1] - 0.55).abs() < 0.02, "{grey:?}");
    }

    #[test]
    fn highlights_past_the_knee_stay_in_order() {
        let mut fit = LookFit::identity();

        fit.terms[1] = 1.15;
        fit.terms[TERMS + 2] = 1.15;
        fit.terms[2 * TERMS + 3] = 1.15;
        fit.knee = 0.8;
        let mut last = 0.0;
        for step in 0..=100 {
            let v = step as f32 / 100.0;
            let out = fit.apply([v, v * 0.9, v * 0.8])[0];
            assert!(out + 1e-6 >= last, "at {v}: {out} after {last}");
            last = out;
        }
        assert!((fit.apply([1.0; 3])[0] - 1.0).abs() < 1e-6);
    }

    #[test]
    fn identity_changes_nothing() {
        let fit = LookFit::identity();
        for p in cube(5) {
            assert_eq!(fit.apply(p), p);
        }
    }
}
