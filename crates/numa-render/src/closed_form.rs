use rayon::prelude::*;

const EPS: f64 = 1e-5;

const TILE: usize = 384;
const OVERLAP: usize = 32;

const TOLERANCE: f64 = 1e-4;
const ITERATIONS: usize = 600;

pub fn solve(rgb: &[[f32; 3]], width: usize, height: usize, alpha: &[f32], unknown: &[bool], prior: &[f32], pull: f32) -> Vec<f32> {
    let mut out = alpha.to_vec();
    let Some((left, top, right, bottom)) = extent(unknown, width) else { return out };
    let starts = |from: usize, to: usize, room: usize| -> Vec<usize> {
        let side = TILE.min(room);
        let mut at = from.saturating_sub(OVERLAP).min(room - side);
        let mut all = vec![at];
        while at + side < (to + OVERLAP + 1).min(room) {
            at = (at + side - OVERLAP).min(room - side);
            all.push(at);
        }
        all
    };
    let (tw, th) = (TILE.min(width), TILE.min(height));
    let squares: Vec<(usize, usize)> = starts(top, bottom, height)
        .into_iter()
        .flat_map(|y| starts(left, right, width).into_iter().map(move |x| (x, y)))
        .filter(|(x, y)| (*y..y + th).any(|row| unknown[row * width + x..row * width + x + tw].iter().any(|u| *u)))
        .collect();
    let answers: Vec<((usize, usize), Vec<f64>)> = squares
        .par_iter()
        .map(|&(x0, y0)| {
            let take = |plane: &dyn Fn(usize) -> f64| -> Vec<f64> {
                (0..tw * th).map(|i| plane((y0 + i / tw) * width + x0 + i % tw)).collect()
            };
            let colours: Vec<[f64; 3]> = (0..tw * th)
                .map(|i| rgb[(y0 + i / tw) * width + x0 + i % tw].map(f64::from))
                .collect();
            let asked: Vec<bool> = (0..tw * th).map(|i| unknown[(y0 + i / tw) * width + x0 + i % tw]).collect();
            let known = take(&|i| alpha[i] as f64);
            let start = take(&|i| prior[i] as f64);
            ((x0, y0), square(&colours, tw, th, &known, &asked, &start, pull as f64))
        })
        .collect();

    let mut sum = vec![0.0f64; width * height];
    let mut weight = vec![0.0f64; width * height];
    for ((x0, y0), solved) in &answers {
        for i in 0..tw * th {
            let (x, y) = (i % tw, i / tw);
            let at = (y0 + y) * width + x0 + x;
            if !unknown[at] {
                continue;
            }
            let near = x.min(tw - 1 - x).min(y).min(th - 1 - y).min(OVERLAP) as f64 + 1.0;
            sum[at] += near * solved[i];
            weight[at] += near;
        }
    }
    for (i, value) in out.iter_mut().enumerate() {
        if unknown[i] && weight[i] > 0.0 {
            *value = (sum[i] / weight[i]).clamp(0.0, 1.0) as f32;
        }
    }
    out
}

fn extent(set: &[bool], width: usize) -> Option<(usize, usize, usize, usize)> {
    let mut found = None::<(usize, usize, usize, usize)>;
    for (i, _) in set.iter().enumerate().filter(|(_, s)| **s) {
        let (x, y) = (i % width, i / width);
        found = Some(match found {
            None => (x, y, x, y),
            Some((l, t, r, b)) => (l.min(x), t.min(y), r.max(x), b.max(y)),
        });
    }
    found
}

struct Window {
    centre: usize,
    mean: [f64; 3],
    inverse: [f64; 6],
}

impl Window {
    fn times(&self, v: [f64; 3]) -> [f64; 3] {
        let m = &self.inverse;
        [
            m[0] * v[0] + m[1] * v[1] + m[2] * v[2],
            m[1] * v[0] + m[3] * v[1] + m[4] * v[2],
            m[2] * v[0] + m[4] * v[1] + m[5] * v[2],
        ]
    }
}

fn square(colours: &[[f64; 3]], w: usize, h: usize, known: &[f64], asked: &[bool], start: &[f64], pull: f64) -> Vec<f64> {
    let offsets: [isize; 9] = {
        let w = w as isize;
        [-w - 1, -w, -w + 1, -1, 0, 1, w - 1, w, w + 1]
    };
    let at = |centre: usize, k: usize| (centre as isize + offsets[k]) as usize;

    let mut windows = Vec::new();
    for y in 1..h.saturating_sub(1) {
        for x in 1..w.saturating_sub(1) {
            let centre = y * w + x;
            if !(0..9).any(|k| asked[at(centre, k)]) {
                continue;
            }
            let mut mean = [0.0; 3];
            for k in 0..9 {
                for c in 0..3 {
                    mean[c] += colours[at(centre, k)][c] / 9.0;
                }
            }
            let mut cov = [0.0; 6];
            for k in 0..9 {
                let d = colours[at(centre, k)];
                let d = [d[0] - mean[0], d[1] - mean[1], d[2] - mean[2]];
                for (slot, (a, b)) in [(0, 0), (0, 1), (0, 2), (1, 1), (1, 2), (2, 2)].iter().enumerate() {
                    cov[slot] += d[*a] * d[*b] / 9.0;
                }
            }
            for slot in [0, 3, 5] {
                cov[slot] += EPS / 9.0;
            }

            let [a, b, c, d, e, f] = cov;
            let (ca, cb, cc) = (d * f - e * e, c * e - b * f, b * e - c * d);
            let det = a * ca + b * cb + c * cc;
            let inverse = [ca / det, cb / det, cc / det, (a * f - c * c) / det, (b * c - a * e) / det, (a * d - b * b) / det];
            windows.push(Window { centre, mean, inverse });
        }
    }

    let laplacian = |x: &[f64], y: &mut [f64]| {
        y.iter_mut().for_each(|v| *v = 0.0);
        for window in &windows {
            let (mut total, mut v) = (0.0, [0.0; 3]);
            for k in 0..9 {
                let i = at(window.centre, k);
                total += x[i];
                for c in 0..3 {
                    v[c] += (colours[i][c] - window.mean[c]) * x[i];
                }
            }
            let mv = window.times(v);
            for k in 0..9 {
                let i = at(window.centre, k);
                let d = [0, 1, 2].map(|c| colours[i][c] - window.mean[c]);
                y[i] += x[i] - (total + d[0] * mv[0] + d[1] * mv[1] + d[2] * mv[2]) / 9.0;
            }
        }
    };
    let mut diagonal = vec![pull; w * h];
    for window in &windows {
        for k in 0..9 {
            let i = at(window.centre, k);
            let d = [0, 1, 2].map(|c| colours[i][c] - window.mean[c]);
            let md = window.times(d);
            diagonal[i] += 1.0 - (1.0 + d[0] * md[0] + d[1] * md[1] + d[2] * md[2]) / 9.0;
        }
    }

    let mut x: Vec<f64> = (0..w * h).map(|i| if asked[i] { start[i] } else { known[i] }).collect();
    let mut scratch = vec![0.0; w * h];
    laplacian(&x, &mut scratch);
    let mut r: Vec<f64> = (0..w * h).map(|i| if asked[i] { -scratch[i] - pull * (x[i] - start[i]) } else { 0.0 }).collect();
    let mut z: Vec<f64> = (0..w * h).map(|i| r[i] / diagonal[i]).collect();
    let mut p = z.clone();
    let mut rz: f64 = r.iter().zip(&z).map(|(a, b)| a * b).sum();
    let first = r.iter().map(|v| v * v).sum::<f64>().sqrt();
    for _ in 0..ITERATIONS {
        laplacian(&p, &mut scratch);
        for i in 0..w * h {
            scratch[i] = if asked[i] { scratch[i] + pull * p[i] } else { 0.0 };
        }
        let pap: f64 = p.iter().zip(&scratch).map(|(a, b)| a * b).sum();
        if pap <= 0.0 {
            break;
        }
        let step = rz / pap;
        for i in 0..w * h {
            x[i] += step * p[i];
            r[i] -= step * scratch[i];
        }
        if r.iter().map(|v| v * v).sum::<f64>().sqrt() <= TOLERANCE * first {
            break;
        }
        for i in 0..w * h {
            z[i] = r[i] / diagonal[i];
        }
        let next: f64 = r.iter().zip(&z).map(|(a, b)| a * b).sum();
        let beta = next / rz;
        rz = next;
        for i in 0..w * h {
            p[i] = z[i] + beta * p[i];
        }
    }
    x
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_thin_strand_is_its_coverage() {
        let (w, h) = (40usize, 30usize);
        let (dark, sky) = ([0.2f32, 0.12, 0.08], [0.4f32, 0.6, 0.85]);
        let cover = |i: usize| match (i % w, i / w) {
            (0..=9, _) => 1.0,
            (10..=25, 15) => 2.0 / 3.0,
            _ => 0.0,
        };
        let rgb: Vec<[f32; 3]> = (0..w * h)
            .map(|i| {
                let a = cover(i);

                let sky = [sky[0], sky[1], sky[2] - (i / w) as f32 * 0.003];
                [0, 1, 2].map(|c| a * dark[c] + (1.0 - a) * sky[c])
            })
            .collect();
        let unknown: Vec<bool> = (0..w * h).map(|i| (8..30).contains(&(i % w))).collect();
        let alpha: Vec<f32> = (0..w * h).map(cover).collect();
        let prior = vec![0.5f32; w * h];
        let solved = solve(&rgb, w, h, &alpha.iter().map(|a| a.round()).collect::<Vec<_>>(), &unknown, &prior, 1e-4);
        let row = 15 * w;
        assert!((solved[row + 20] - 2.0 / 3.0).abs() < 0.05, "strand {}", solved[row + 20]);
        assert!(solved[row + 27] < 0.05 && solved[row - 2 * w + 20] < 0.05, "sky {} {}", solved[row + 27], solved[row - 2 * w + 20]);
        assert!(solved[row + 9] > 0.95, "block {}", solved[row + 9]);
    }
}
