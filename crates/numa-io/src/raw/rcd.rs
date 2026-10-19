use rawler::cfa::CFA;
use rawler::imgop::Rect;
use rawler::pixarray::Color2D;
use rayon::prelude::*;

const EPS: f32 = 1e-5;
const EPSSQ: f32 = 1e-10;

const NEED_DIRECTION: usize = 4;
const NEED_GREEN: usize = 4;
const NEED_COLOUR: usize = 3;

const TILE_ROWS: usize = 128;
const TILE_COLS: usize = 512;
const MARGIN: usize = 10;

pub fn develop(floats: &[f32], width: usize, cfa: &CFA, roi: Rect, balance: [f32; 3]) -> Color2D<f32, 3> {
    let (w, h) = (roi.d.w, roi.d.h);
    let pattern = [0, 1].map(|y| [0, 1].map(|x| cfa.color_at(roi.p.y + y, roi.p.x + x) as u8));
    let frame = Frame { floats, width, x0: roi.p.x, y0: roi.p.y, w, h, pattern, balance };
    let mut out = vec![[0f32; 3]; w * h];
    out.par_chunks_mut(w * TILE_ROWS).enumerate().for_each(|(band, rows)| {
        let mut tile = Tile::default();
        let y0 = band * TILE_ROWS;
        for x0 in (0..w).step_by(TILE_COLS) {
            tile.develop(&frame, x0, y0, (x0 + TILE_COLS).min(w), (y0 + TILE_ROWS).min(h), rows);
        }
    });
    Color2D::new_with(out, w, h)
}

struct Frame<'a> {
    floats: &'a [f32],
    width: usize,
    x0: usize,
    y0: usize,
    w: usize,
    h: usize,

    pattern: [[u8; 2]; 2],
    balance: [f32; 3],
}

impl Frame<'_> {
    fn colour(&self, x: usize, y: usize) -> usize {
        self.pattern[y & 1][x & 1] as usize
    }

    fn edge(&self, x: usize, y: usize) -> usize {
        x.min(y).min(self.w - 1 - x).min(self.h - 1 - y)
    }
}

#[derive(Default)]
struct Tile {
    bx: usize,
    by: usize,
    bw: usize,
    bh: usize,
    colour: Vec<u8>,

    m: Vec<f32>,

    hv: Vec<f32>,
    hh: Vec<f32>,

    vh: Vec<f32>,

    lpf: Vec<f32>,
    green: Vec<f32>,

    hp: Vec<f32>,
    hq: Vec<f32>,

    other: Vec<f32>,
}

fn high(m: &[f32], i: usize, s: usize) -> f32 {
    let v = (m[i - 3 * s] - m[i - s] - m[i + s] + m[i + 3 * s]) - 3.0 * (m[i - 2 * s] + m[i + 2 * s]) + 6.0 * m[i];
    v * v
}

fn refined(dir: &[f32], i: usize, s: usize) -> f32 {
    let central = dir[i];
    let around = 0.25 * (dir[i - s - 1] + dir[i - s + 1] + dir[i + s - 1] + dir[i + s + 1]);
    if (0.5 - central).abs() < (0.5 - around).abs() { around } else { central }
}

fn ratio(value: f32, here: f32, there: f32) -> f32 {
    value * (1.0 + (here - there) / (EPS + here + there))
}

impl Tile {
    fn develop(&mut self, f: &Frame, x0: usize, y0: usize, x1: usize, y1: usize, out: &mut [[f32; 3]]) {
        self.load(f, x0, y0, x1, y1);
        self.directions(f);
        self.greens(f);
        self.colours(f);
        self.finish(f, x0, y0, x1, y1, out);
    }

    fn sites(&self) -> impl Iterator<Item = (usize, usize, usize, usize)> + use<> {
        let (bx, by, bw, bh) = (self.bx, self.by, self.bw, self.bh);
        (0..bh).flat_map(move |ly| (0..bw).map(move |lx| (ly * bw + lx, bx + lx, by + ly, lx.min(ly).min(bw - 1 - lx).min(bh - 1 - ly))))
    }

    fn border(&self, i: usize, c: u8) -> f32 {
        let (lx, ly) = (i % self.bw, i / self.bw);
        let (mut sum, mut count) = (0.0f32, 0u32);
        for yy in ly.saturating_sub(1)..=(ly + 1).min(self.bh - 1) {
            for xx in lx.saturating_sub(1)..=(lx + 1).min(self.bw - 1) {
                if self.colour[yy * self.bw + xx] == c {
                    sum += self.m[yy * self.bw + xx];
                    count += 1;
                }
            }
        }
        if count == 0 { 0.0 } else { sum / count as f32 }
    }

    fn load(&mut self, f: &Frame, x0: usize, y0: usize, x1: usize, y1: usize) {
        self.bx = x0.saturating_sub(MARGIN);
        self.by = y0.saturating_sub(MARGIN);
        self.bw = (x1 + MARGIN).min(f.w) - self.bx;
        self.bh = (y1 + MARGIN).min(f.h) - self.by;
        let n = self.bw * self.bh;
        for plane in [&mut self.m, &mut self.hv, &mut self.hh, &mut self.vh, &mut self.lpf, &mut self.green, &mut self.hp, &mut self.hq, &mut self.other] {
            plane.clear();
            plane.resize(n, 0.0);
        }
        self.colour.clear();
        self.colour.resize(n, 0);
        for (i, x, y, _) in self.sites() {
            let c = f.colour(x, y);
            self.colour[i] = c as u8;
            self.m[i] = f.floats[(f.y0 + y) * f.width + f.x0 + x] * f.balance[c];
        }
    }

    fn directions(&mut self, f: &Frame) {
        let (bw, m) = (self.bw, &self.m);
        for (i, _, _, local) in self.sites() {
            if local >= 3 {
                self.hv[i] = high(m, i, bw);
                self.hh[i] = high(m, i, 1);
            }
        }
        for (i, x, y, local) in self.sites() {
            self.vh[i] = match (f.edge(x, y) >= NEED_DIRECTION, local >= NEED_DIRECTION) {
                (false, _) => 0.5,
                (true, false) => 0.0,
                (true, true) => {
                    let v = (self.hv[i - bw] + self.hv[i] + self.hv[i + bw]).max(EPSSQ);
                    let hz = (self.hh[i - 1] + self.hh[i] + self.hh[i + 1]).max(EPSSQ);
                    v / (v + hz)
                }
            };
        }
        for (i, _, _, local) in self.sites() {
            if local >= 1 && self.colour[i] != 1 {
                self.lpf[i] = m[i] + 0.5 * (m[i - bw] + m[i + bw] + m[i - 1] + m[i + 1])
                    + 0.25 * (m[i - bw - 1] + m[i - bw + 1] + m[i + bw - 1] + m[i + bw + 1]);
            }
        }
    }

    fn greens(&mut self, f: &Frame) {
        let mut green = std::mem::take(&mut self.green);
        let (s, m, lpf) = (self.bw, &self.m, &self.lpf);
        for (i, x, y, local) in self.sites() {
            green[i] = match (self.colour[i] == 1, f.edge(x, y) >= NEED_GREEN, local >= NEED_GREEN) {
                (true, _, _) => m[i],
                (false, false, _) => self.border(i, 1),
                (false, true, false) => 0.0,
                (false, true, true) => {
                    let disc = refined(&self.vh, i, s);
                    let n_grad = EPS + (m[i - s] - m[i + s]).abs() + (m[i] - m[i - 2 * s]).abs() + (m[i - s] - m[i - 3 * s]).abs() + (m[i - 2 * s] - m[i - 4 * s]).abs();
                    let s_grad = EPS + (m[i + s] - m[i - s]).abs() + (m[i] - m[i + 2 * s]).abs() + (m[i + s] - m[i + 3 * s]).abs() + (m[i + 2 * s] - m[i + 4 * s]).abs();
                    let w_grad = EPS + (m[i - 1] - m[i + 1]).abs() + (m[i] - m[i - 2]).abs() + (m[i - 1] - m[i - 3]).abs() + (m[i - 2] - m[i - 4]).abs();
                    let e_grad = EPS + (m[i + 1] - m[i - 1]).abs() + (m[i] - m[i + 2]).abs() + (m[i + 1] - m[i + 3]).abs() + (m[i + 2] - m[i + 4]).abs();
                    let n_est = ratio(m[i - s], lpf[i], lpf[i - 2 * s]);
                    let s_est = ratio(m[i + s], lpf[i], lpf[i + 2 * s]);
                    let w_est = ratio(m[i - 1], lpf[i], lpf[i - 2]);
                    let e_est = ratio(m[i + 1], lpf[i], lpf[i + 2]);
                    let v_est = (s_grad * n_est + n_grad * s_est) / (n_grad + s_grad);
                    let h_est = (e_grad * w_est + w_grad * e_est) / (e_grad + w_grad);
                    (disc * h_est + (1.0 - disc) * v_est).max(0.0)
                }
            };
        }
        self.green = green;
    }

    fn colours(&mut self, f: &Frame) {
        let (bw, m) = (self.bw, &self.m);
        for (i, _, _, local) in self.sites() {
            if local >= 3 && self.colour[i] != 1 {
                self.hp[i] = high(m, i, bw + 1);
                self.hq[i] = high(m, i, bw - 1);
            }
        }

        let (hp, hq) = (&self.hp, &self.hq);
        let pq = |i: usize, x: usize, y: usize| match f.edge(x, y) >= NEED_DIRECTION {
            false => 0.5,
            true => {
                let p = (hp[i - bw - 1] + hp[i] + hp[i + bw + 1]).max(EPSSQ);
                let q = (hq[i - bw + 1] + hq[i] + hq[i + bw - 1]).max(EPSSQ);
                p / (p + q)
            }
        };
        let mut other = std::mem::take(&mut self.other);
        let (s, g) = (bw, &self.green);
        for (i, x, y, local) in self.sites() {
            if self.colour[i] == 1 {
                continue;
            }
            other[i] = match (f.edge(x, y) >= NEED_COLOUR, local >= NEED_COLOUR) {
                (false, _) => self.border(i, 2 - self.colour[i]),
                (true, false) => 0.0,
                (true, true) => {
                    let central = pq(i, x, y);
                    let around = 0.25 * (pq(i - s - 1, x - 1, y - 1) + pq(i - s + 1, x + 1, y - 1) + pq(i + s - 1, x - 1, y + 1) + pq(i + s + 1, x + 1, y + 1));
                    let disc = if (0.5 - central).abs() < (0.5 - around).abs() { around } else { central };
                    let nw_grad = EPS + (m[i - s - 1] - m[i + s + 1]).abs() + (m[i - s - 1] - m[i - 3 * s - 3]).abs() + (g[i] - g[i - 2 * s - 2]).abs();
                    let ne_grad = EPS + (m[i - s + 1] - m[i + s - 1]).abs() + (m[i - s + 1] - m[i - 3 * s + 3]).abs() + (g[i] - g[i - 2 * s + 2]).abs();
                    let sw_grad = EPS + (m[i + s - 1] - m[i - s + 1]).abs() + (m[i + s - 1] - m[i + 3 * s - 3]).abs() + (g[i] - g[i + 2 * s - 2]).abs();
                    let se_grad = EPS + (m[i + s + 1] - m[i - s - 1]).abs() + (m[i + s + 1] - m[i + 3 * s + 3]).abs() + (g[i] - g[i + 2 * s + 2]).abs();
                    let nw = m[i - s - 1] - g[i - s - 1];
                    let ne = m[i - s + 1] - g[i - s + 1];
                    let sw = m[i + s - 1] - g[i + s - 1];
                    let se = m[i + s + 1] - g[i + s + 1];
                    let p_est = (nw_grad * se + se_grad * nw) / (nw_grad + se_grad);
                    let q_est = (ne_grad * sw + sw_grad * ne) / (ne_grad + sw_grad);
                    (g[i] + (1.0 - disc) * p_est + disc * q_est).max(0.0)
                }
            };
        }
        self.other = other;
    }

    fn finish(&self, f: &Frame, x0: usize, y0: usize, x1: usize, y1: usize, out: &mut [[f32; 3]]) {
        let unbalance = [1.0 / f.balance[0], 1.0, 1.0 / f.balance[2]];
        let (s, m, g, colour) = (self.bw, &self.m, &self.green, &self.colour);

        let known = |i: usize, c: u8| if colour[i] == c { m[i] } else { self.other[i] };
        for y in y0..y1 {
            for x in x0..x1 {
                let i = (y - self.by) * s + (x - self.bx);
                let mut rgb = [0f32; 3];
                rgb[1] = g[i];
                if colour[i] != 1 {
                    rgb[colour[i] as usize] = m[i];
                    rgb[2 - colour[i] as usize] = self.other[i];
                }
                for c in [0u8, 2].into_iter().filter(|_| colour[i] == 1) {
                    rgb[c as usize] = match f.edge(x, y) >= NEED_COLOUR {
                        false => self.border(i, c),
                        true => {
                            let disc = refined(&self.vh, i, s);
                            let n_grad = EPS + (g[i] - g[i - 2 * s]).abs() + (known(i - s, c) - known(i + s, c)).abs() + (known(i - s, c) - known(i - 3 * s, c)).abs();
                            let s_grad = EPS + (g[i] - g[i + 2 * s]).abs() + (known(i + s, c) - known(i - s, c)).abs() + (known(i + s, c) - known(i + 3 * s, c)).abs();
                            let w_grad = EPS + (g[i] - g[i - 2]).abs() + (known(i - 1, c) - known(i + 1, c)).abs() + (known(i - 1, c) - known(i - 3, c)).abs();
                            let e_grad = EPS + (g[i] - g[i + 2]).abs() + (known(i + 1, c) - known(i - 1, c)).abs() + (known(i + 1, c) - known(i + 3, c)).abs();
                            let n = known(i - s, c) - g[i - s];
                            let so = known(i + s, c) - g[i + s];
                            let we = known(i - 1, c) - g[i - 1];
                            let e = known(i + 1, c) - g[i + 1];
                            let v_est = (n_grad * so + s_grad * n) / (n_grad + s_grad);
                            let h_est = (e_grad * we + w_grad * e) / (e_grad + w_grad);
                            (g[i] + (1.0 - disc) * v_est + disc * h_est).max(0.0)
                        }
                    };
                }
                out[(y - y0) * f.w + x] = [rgb[0] * unbalance[0], rgb[1], rgb[2] * unbalance[2]];
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rawler::imgop::{Dim2, Point};

    fn mosaic(width: usize, height: usize) -> Vec<f32> {
        (0..width * height)
            .map(|i| {
                let (x, y) = ((i % width) as f32, (i / width) as f32);
                let scene = 0.3 + 0.2 * (x * 0.37).sin() * (y * 0.23).cos() + 0.1 * ((x + y) * 0.9).sin();
                let gain = [[0.5, 1.0], [1.0, 0.7]][(i / width) % 2][i % 2];
                scene * gain
            })
            .collect()
    }

    #[test]
    fn tiles_join_without_a_seam() {
        let (width, height) = (1100, 300);
        let floats = mosaic(width, height);
        let roi = Rect::new(Point::new(4, 2), Dim2::new(1090, 290));
        let cfa = CFA::new("RGGB");
        let tiled = develop(&floats, width, &cfa, roi, [2.0, 1.0, 1.4286]);

        let pattern = [0, 1].map(|y| [0, 1].map(|x| cfa.color_at(roi.p.y + y, roi.p.x + x) as u8));
        let frame = Frame { floats: &floats, width, x0: 4, y0: 2, w: 1090, h: 290, pattern, balance: [2.0, 1.0, 1.4286] };
        let mut whole = vec![[0f32; 3]; 1090 * 290];
        Tile::default().develop(&frame, 0, 0, 1090, 290, &mut whole);
        assert!(tiled.data == whole, "the tiled frame differs from the whole one");
    }

    #[test]
    fn a_flat_grey_stays_flat() {
        let (width, height) = (300, 200);
        let floats: Vec<f32> = (0..width * height).map(|i| [[0.25, 0.5], [0.5, 0.35]][(i / width) % 2][i % 2]).collect();
        let roi = Rect::new(Point::new(0, 0), Dim2::new(width, height));
        let out = develop(&floats, width, &CFA::new("RGGB"), roi, [2.0, 1.0, 0.5 / 0.35]);
        for (i, rgb) in out.data.iter().enumerate() {
            for (c, want) in [0.25, 0.5, 0.35].into_iter().enumerate() {
                assert!((rgb[c] - want).abs() < 1e-5, "{i} {c}: {} for {want}", rgb[c]);
            }
        }
    }
}
