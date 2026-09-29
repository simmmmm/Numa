use multiversion::multiversion;
use rayon::prelude::*;
use std::time::Instant;

use crate::{
  CFA,
  cfa::{CFA_COLOR_B, CFA_COLOR_G, CFA_COLOR_R, PlaneColor},
  imgop::{
    Rect,
    cielab::XYZ_to_lab,
    matrix::multiply_row1,
    sensor::Demosaic,
    xyz::{CIE_1931_TRISTIMULUS_D65, SRGB_TO_XYZ_D65},
  },
  pixarray::{Color2D, PixF32, SharedColor2D},
};

#[derive(Default)]
pub struct XTransMarkesteijnDemosaic {

  passes: usize,
}

impl XTransMarkesteijnDemosaic {

  pub fn new(passes: usize) -> Self {
    Self { passes }
  }

  pub fn new_pass_1() -> Self {
    Self::new(1)
  }

  pub fn new_pass_3() -> Self {
    Self::new(3)
  }
}

impl Demosaic<f32, 3> for XTransMarkesteijnDemosaic {

  fn demosaic(&self, pixels: &PixF32, cfa: &CFA, _colors: &PlaneColor, roi: Rect) -> Color2D<f32, 3> {
    if !cfa.is_rgb() {
      panic!("CFA pattern '{}' is not a RGB pattern, can not demosaic with Markesteijn", cfa);
    }

    let now = Instant::now();
    let input = pixels.crop(roi);

    let cfa_roi = cfa.shift(roi.p.x, roi.p.y);

    let result = demosaic_impl(&input.data, roi.width(), roi.height(), &cfa_roi, self.passes);
    log::debug!("X-Trans Markesteijn ({}-pass) debayer time: {:.5}s", self.passes, now.elapsed().as_secs_f32());
    result
  }
}

const TS: usize = 128;

fn demosaic_impl(input: &[f32], width: usize, height: usize, cfa: &CFA, passes: usize) -> Color2D<f32, 3> {
  let mut output = Color2D::<f32, 3>::new(width, height);
  let ndir: usize = if passes > 1 { 8 } else { 4 };

  let pad_lab: usize = if passes == 1 { 8 } else { 13 };
  let pad_drv: usize = pad_lab + 1;
  let pad_homo: usize = pad_drv + 1;
  let pad_tile: usize = pad_homo + 2;
  let overlap: usize = pad_tile * 2;

  let allhex = build_hex_lut(cfa);
  let green_bounds = compute_green_bounds(input, width, height, cfa, &allhex);

  border_interpolate(input, width, height, cfa, &mut output, pad_tile);

  let stride = TS - overlap;
  let tiles: Vec<(usize, usize, usize, usize)> = (0..height)
    .step_by(stride)
    .flat_map(|top| {
      (0..width).step_by(stride).filter_map(move |left| {
        let tile_h = TS.min(height - top);
        let tile_w = TS.min(width - left);
        if tile_h > overlap && tile_w > overlap {
          Some((top, left, tile_h, tile_w))
        } else {
          None
        }
      })
    })
    .collect();

  let shared_output = SharedColor2D::<f32, 3>::new(output);

  tiles.par_iter().for_each(|&(top, left, tile_h, tile_w)| {
    process_tile(
      input,
      width,
      height,
      cfa,
      &allhex,
      &green_bounds,
      top,
      left,
      tile_h,
      tile_w,
      passes,
      ndir,
      pad_lab,
      pad_drv,
      pad_homo,
      pad_tile,
      &shared_output,
    );
  });

  shared_output.into_inner()
}

struct HexLut {
  hex: [[[[(i32, i32); 8]; 2]; 3]; 3],

  sgrow: usize,

  sgcol: usize,
}

#[multiversion(targets("x86_64+avx512f+avx512bw+avx512vl", "x86_64+avx+avx2+fma", "x86_64+sse4.1", "aarch64+neon"))]
fn build_hex_lut(cfa: &CFA) -> HexLut {

  const ORTH: [i32; 12] = [1, 0, 0, 1, -1, 0, 0, -1, 1, 0, 0, 1];
  const PATT: [[i32; 16]; 2] = [
    [0, 1, 0, -1, 2, 0, -1, 0, 1, 1, 1, -1, 0, 0, 0, 0],
    [0, 1, 0, -2, 1, 0, -2, 0, 1, 1, -2, -2, 1, -1, -1, 1],
  ];

  let mut lut = HexLut {
    hex: [[[[(0i32, 0i32); 8]; 2]; 3]; 3],
    sgrow: 0,
    sgcol: 0,
  };

  for row in 0..3i32 {
    for col in 0..3i32 {
      let mut ng: i32 = 0;
      let mut d: usize = 0;
      while d < 10 {

        let g: usize = if cfa.color_at(row as usize, col as usize) == CFA_COLOR_G { 1 } else { 0 };

        if cfa.color_at((row + ORTH[d]) as usize, (col + ORTH[d + 2]) as usize) == CFA_COLOR_G {
          ng = 0;
        } else {
          ng += 1;
        }

        if ng == 4 {
          lut.sgrow = row as usize;
          lut.sgcol = col as usize;
        }

        if ng == g as i32 + 1 {
          for c in 0..8 {

            let v = ORTH[d] * PATT[g][c * 2] + ORTH[d + 1] * PATT[g][c * 2 + 1];
            let h = ORTH[d + 2] * PATT[g][c * 2] + ORTH[d + 3] * PATT[g][c * 2 + 1];

            let slot = c ^ (g * 2 & d);
            lut.hex[row as usize][col as usize][0][slot] = (v, h);
            lut.hex[row as usize][col as usize][1][slot] = (v, h);
          }
        }
        d += 2;
      }
    }
  }

  lut
}

#[multiversion(targets("x86_64+avx512f+avx512bw+avx512vl", "x86_64+avx+avx2+fma", "x86_64+sse4.1", "aarch64+neon"))]
fn compute_green_bounds(input: &[f32], width: usize, height: usize, cfa: &CFA, allhex: &HexLut) -> Vec<[f32; 2]> {
  let npix = width * height;
  let mut bounds = vec![[0.0f32; 2]; npix];

  bounds.par_chunks_mut(width).enumerate().skip(2).take(height.saturating_sub(4)).for_each(|(y, bounds)| {
    for x in 2..width - 2 {
      let idx = y * width + x;
      if cfa.color_at(y, x) == CFA_COLOR_G {
        bounds[x] = [input[idx], input[idx]];
        continue;
      }

      let hex = &allhex.hex[y % 3][x % 3][0];
      let mut lo = f32::MAX;
      let mut hi = f32::MIN;

      for i in 0..6 {
        let (dy, dx) = hex[i];
        if dy == 0 && dx == 0 {
          continue;
        }
        let ny = (y as i32 + dy) as usize;
        let nx = (x as i32 + dx) as usize;
        if ny < height && nx < width {
          let v = input[ny * width + nx];
          lo = lo.min(v);
          hi = hi.max(v);
        }
      }

      bounds[x] = [lo, hi];
    }
  });
  bounds
}

fn border_interpolate(input: &[f32], width: usize, height: usize, cfa: &CFA, output: &mut Color2D<f32, 3>, border: usize) {
  let hb = height.min(border);
  let wb = width.min(border);

  for y in (0..hb).chain(height.saturating_sub(border)..height) {
    for x in 0..width {
      interpolate_border_pixel(input, width, height, cfa, output, y, x);
    }
  }

  for y in hb..height.saturating_sub(border) {
    for x in (0..wb).chain(width.saturating_sub(border)..width) {
      interpolate_border_pixel(input, width, height, cfa, output, y, x);
    }
  }
}

#[inline]
#[multiversion(targets("x86_64+avx512f+avx512bw+avx512vl", "x86_64+avx+avx2+fma", "x86_64+sse4.1", "aarch64+neon"))]
fn interpolate_border_pixel(input: &[f32], width: usize, height: usize, cfa: &CFA, output: &mut Color2D<f32, 3>, y: usize, x: usize) {
  let mut rgb = [0.0f32; 3];
  let mut count = [0u32; 3];

  let y_lo = y.saturating_sub(1);
  let y_hi = (y + 1).min(height - 1);
  let x_lo = x.saturating_sub(1);
  let x_hi = (x + 1).min(width - 1);

  for ny in y_lo..=y_hi {
    for nx in x_lo..=x_hi {
      let ch = cfa.color_at(ny, nx);
      rgb[ch] += input[ny * width + nx];
      count[ch] += 1;
    }
  }

  *output.at_mut(y, x) = [
    if count[0] > 0 { rgb[0] / count[0] as f32 } else { 0.0 },
    if count[1] > 0 { rgb[1] / count[1] as f32 } else { 0.0 },
    if count[2] > 0 { rgb[2] / count[2] as f32 } else { 0.0 },
  ];
}

#[allow(clippy::too_many_arguments)]
#[multiversion(targets("x86_64+avx512f+avx512bw+avx512vl", "x86_64+avx+avx2+fma", "x86_64+sse4.1", "aarch64+neon"))]
fn process_tile(
  input: &[f32],
  width: usize,
  height: usize,
  cfa: &CFA,
  allhex: &HexLut,
  green_bounds: &[[f32; 2]],
  top: usize,
  left: usize,
  tile_h: usize,
  tile_w: usize,
  passes: usize,
  ndir: usize,
  pad_lab: usize,
  pad_drv: usize,
  pad_homo: usize,
  pad_tile: usize,
  output: &SharedColor2D<f32, 3>,
) {
  let tpix = tile_h * tile_w;
  let mut rgb = vec![[0.0f32; 3]; ndir * tpix];
  let mut lab = vec![[0.0f32; 3]; tpix];
  let mut drv = vec![0.0f32; ndir * tpix];
  let mut homo = vec![0u8; ndir * tpix];

  green_interpolation(input, width, height, cfa, allhex, green_bounds, top, left, tile_h, tile_w, &mut rgb);

  for pass in 0..passes {
    let dir_base = if pass == 0 { 0 } else { 4 };

    if pass == 1 {

      for d in 0..4 {
        for i in 0..tpix {
          rgb[(4 + d) * tpix + i] = rgb[d * tpix + i];
        }
      }
    }

    if pass > 0 {
      green_recalculation(width, cfa, allhex, green_bounds, top, left, tile_h, tile_w, dir_base, &mut rgb);
    }

    rb_interpolation(input, width, height, cfa, allhex, top, left, tile_h, tile_w, dir_base, passes, &mut rgb);
  }

  let dir_offsets: [i32; 4] = [1, tile_w as i32, tile_w as i32 + 1, tile_w as i32 - 1];

  for d in 0..ndir {
    let base = d * tpix;

    for ty in pad_lab..tile_h.saturating_sub(pad_lab) {
      for tx in pad_lab..tile_w.saturating_sub(pad_lab) {
        let ti = ty * tile_w + tx;
        lab[ti] = rgb_to_lab(&rgb[base + ti]);
      }
    }

    let f = dir_offsets[d & 3];
    for ty in pad_drv..tile_h.saturating_sub(pad_drv) {
      for tx in pad_drv..tile_w.saturating_sub(pad_drv) {
        let ti = ty * tile_w + tx;
        let lix = lab[ti];
        let plus = lab[(ti as i32 + f) as usize];
        let minus = lab[(ti as i32 - f) as usize];

        let g = 2.0 * lix[0] - plus[0] - minus[0];
        let a_diff = 2.0 * lix[1] - plus[1] - minus[1] + g * (500.0 / 232.0);
        let b_diff = 2.0 * lix[2] - plus[2] - minus[2] - g * (500.0 / 580.0);

        drv[base + ti] = g * g + a_diff * a_diff + b_diff * b_diff;
      }
    }
  }

  for ty in pad_homo..tile_h.saturating_sub(pad_homo) {
    for tx in pad_homo..tile_w.saturating_sub(pad_homo) {
      let ti = ty * tile_w + tx;

      let mut tr = f32::MAX;
      for d in 0..ndir {
        let val = drv[d * tpix + ti];
        if tr > val {
          tr = val;
        }
      }
      tr *= 8.0;

      for d in 0..ndir {
        let base = d * tpix;
        let mut votes = 0u8;
        for v in -1i32..=1 {
          for h in -1i32..=1 {
            let ni = (ti as i32 + v * tile_w as i32 + h) as usize;
            if drv[base + ni] <= tr {
              votes += 1;
            }
          }
        }
        homo[base + ti] = votes;
      }
    }
  }

  for ty in pad_tile..tile_h.saturating_sub(pad_tile) {
    let iy = top + ty;
    if iy >= height {
      break;
    }

    for tx in pad_tile..tile_w.saturating_sub(pad_tile) {
      let ix = left + tx;
      if ix >= width {
        break;
      }
      let ti = ty * tile_w + tx;

      let mut hm = [0u16; 8];
      for d in 0..ndir {
        let base = d * tpix;
        for v in -2i32..=2 {
          for h in -2i32..=2 {
            let ni = (ti as i32 + v * tile_w as i32 + h) as usize;
            hm[d] += homo[base + ni] as u16;
          }
        }
      }

      for d in 0..ndir.saturating_sub(4) {
        if hm[d] < hm[d + 4] {
          hm[d] = 0;
        } else if hm[d] > hm[d + 4] {
          hm[d + 4] = 0;
        }
      }

      let mut max_hm = hm[0];
      for d in 1..ndir {
        if max_hm < hm[d] {
          max_hm = hm[d];
        }
      }
      let threshold = max_hm - (max_hm >> 3);

      let mut sum = [0.0f32; 3];
      let mut cnt = 0u32;
      for d in 0..ndir {
        if hm[d] >= threshold {
          let v = rgb[d * tpix + ti];
          sum[0] += v[0];
          sum[1] += v[1];
          sum[2] += v[2];
          cnt += 1;
        }
      }

      if cnt > 0 {
        let inv = 1.0 / cnt as f32;

        unsafe {
          *output.inner_mut().at_mut(iy, ix) = [
            sum[0] * inv,
            sum[1] * inv,
            sum[2] * inv,
          ];
        }
      }
    }
  }
}

#[multiversion(targets("x86_64+avx512f+avx512bw+avx512vl", "x86_64+avx+avx2+fma", "x86_64+sse4.1", "aarch64+neon"))]
fn green_interpolation(
  input: &[f32],
  width: usize,
  height: usize,
  cfa: &CFA,
  allhex: &HexLut,
  green_bounds: &[[f32; 2]],
  top: usize,
  left: usize,
  tile_h: usize,
  tile_w: usize,
  rgb: &mut [[f32; 3]],
) {
  let tpix = tile_h * tile_w;
  let sgrow = allhex.sgrow as i32;

  let pix = |row: i32, col: i32| -> f32 { input[row as usize * width + col as usize] };

  for ty in 0..tile_h {
    let iy = top + ty;
    if iy >= height {
      break;
    }

    for tx in 0..tile_w {
      let ix = left + tx;
      if ix >= width {
        break;
      }

      let ti = ty * tile_w + tx;
      let img_idx = iy * width + ix;
      let f = cfa.color_at(iy, ix);

      let v = input[img_idx];
      for d in 0..4 {
        rgb[d * tpix + ti][f as usize] = v;
      }

      if f == 1 {

        for d in 0..4 {
          rgb[d * tpix + ti][1] = v;
        }
        continue;
      }

      if iy >= 3 && iy + 3 < height && ix >= 3 && ix + 3 < width {
        let hex = &allhex.hex[iy % 3][ix % 3][0];
        let [lo, hi] = green_bounds[img_idx];
        let row = iy as i32;
        let col = ix as i32;

        let (h0v, h0h) = hex[0];
        let (h1v, h1h) = hex[1];
        let c0 = (174.0 * (pix(row + h1v, col + h1h) + pix(row + h0v, col + h0h))
                - 46.0 * (pix(row + 2 * h1v, col + 2 * h1h) + pix(row + 2 * h0v, col + 2 * h0h)))
                * (1.0 / 256.0);

        let (h2v, h2h) = hex[2];
        let (h3v, h3h) = hex[3];
        let c1 = (223.0 * pix(row + h3v, col + h3h)
                + 33.0 * pix(row + h2v, col + h2h)
                + 92.0 * (v - pix(row - h2v, col - h2h)))
                * (1.0 / 256.0);

        let (h4v, h4h) = hex[4];
        let c2 = (164.0 * pix(row + h4v, col + h4h)
                + 92.0 * pix(row - 2 * h4v, col - 2 * h4h)
                + 33.0 * (2.0 * v - pix(row + 3 * h4v, col + 3 * h4h)
                                   - pix(row - 3 * h4v, col - 3 * h4h)))
                * (1.0 / 256.0);

        let (h5v, h5h) = hex[5];
        let c3 = (164.0 * pix(row + h5v, col + h5h)
                + 92.0 * pix(row - 2 * h5v, col - 2 * h5h)
                + 33.0 * (2.0 * v - pix(row + 3 * h5v, col + 3 * h5h)
                                   - pix(row - 3 * h5v, col - 3 * h5h)))
                * (1.0 / 256.0);

        let xor_val = if (row - sgrow).rem_euclid(3) == 0 { 1usize } else { 0usize };
        let color = [c0, c1, c2, c3];
        for c in 0..4usize {
          let d = c ^ xor_val;
          rgb[d * tpix + ti][1] = color[c].max(lo).min(hi);
        }
      } else {

        for d in 0..4 {
          rgb[d * tpix + ti][1] = v;
        }
      }
    }
  }
}

#[allow(clippy::too_many_arguments)]
#[multiversion(targets("x86_64+avx512f+avx512bw+avx512vl", "x86_64+avx+avx2+fma", "x86_64+sse4.1", "aarch64+neon"))]
fn green_recalculation(
  width: usize,
  cfa: &CFA,
  allhex: &HexLut,
  green_bounds: &[[f32; 2]],
  top: usize,
  left: usize,
  tile_h: usize,
  tile_w: usize,
  dir_base: usize,
  rgb: &mut [[f32; 3]],
) {
  let tpix = tile_h * tile_w;
  let tw = tile_w as i32;
  let sgrow = allhex.sgrow as i32;
  const PAD_G_RECALC: usize = 6;

  for ty in PAD_G_RECALC..tile_h.saturating_sub(PAD_G_RECALC) {
    let iy = top + ty;
    let row = iy as i32;
    for tx in PAD_G_RECALC..tile_w.saturating_sub(PAD_G_RECALC) {
      let ix = left + tx;
      let f = cfa.color_at(iy, ix);
      if f == CFA_COLOR_G {
        continue;
      }
      let f = f as usize;
      let ti = (ty * tile_w + tx) as i32;
      let img_idx = iy * width + ix;
      let [lo, hi] = green_bounds[img_idx];

      let hex = &allhex.hex[iy % 3][ix % 3][1];
      let xor_val: usize = if (row - sgrow).rem_euclid(3) == 0 { 1 } else { 0 };

      for d in 3..6usize {
        let dir_idx = (d - 2) ^ xor_val;
        let base = (dir_base + dir_idx) * tpix;

        let (dy, dx) = hex[d];
        let hex_off = dy * tw + dx;

        let rix_center = rgb[(base as i32 + ti) as usize];
        let rix_plus = rgb[(base as i32 + ti + hex_off) as usize];
        let rix_minus2 = rgb[(base as i32 + ti - 2 * hex_off) as usize];

        let val = rix_minus2[1] + 2.0 * rix_plus[1]
                - rix_minus2[f] - 2.0 * rix_plus[f]
                + 3.0 * rix_center[f];

        rgb[(base as i32 + ti) as usize][1] = (val / 3.0).max(lo).min(hi);
      }
    }
  }
}

#[multiversion(targets("x86_64+avx512f+avx512bw+avx512vl", "x86_64+avx+avx2+fma", "x86_64+sse4.1", "aarch64+neon"))]
fn rb_interpolation(
  _input: &[f32],
  _width: usize,
  _height: usize,
  cfa: &CFA,
  allhex: &HexLut,
  top: usize,
  left: usize,
  tile_h: usize,
  tile_w: usize,
  dir_base: usize,
  passes: usize,
  rgb: &mut [[f32; 3]],
) {
  let tpix = tile_h * tile_w;
  let tw = tile_w as i32;
  let sgrow = allhex.sgrow as i32;
  let sgcol = allhex.sgcol as i32;

  let pad_rb_g: usize = if passes == 1 { 6 } else { 5 };
  let pad_rb_br: usize = if passes == 1 { 6 } else { 5 };
  let pad_g2x2: usize = if passes == 1 { 8 } else { 4 };

  {

    let row_start = ((top as i32 - sgrow + pad_rb_g as i32 + 2) / 3 * 3 + sgrow) as usize - top;
    let col_start = ((left as i32 - sgcol + pad_rb_g as i32 + 2) / 3 * 3 + sgcol) as usize - left;

    let mut ty = row_start;
    while ty + pad_rb_g < tile_h {
      let iy = (top + ty) as i32;
      let mut tx = col_start;
      while tx + pad_rb_g < tile_w {
        let ix = (left + tx) as i32;
        let ti = (ty * tile_w + tx) as i32;

        if cfa.color_at(iy as usize, ix as usize) != CFA_COLOR_G {
          tx += 3;
          continue;
        }

        let mut h = cfa.color_at(iy as usize, (ix + 1) as usize) as usize;
        if h == CFA_COLOR_G {
          tx += 3;
          continue;
        }

        let mut color_est = [[0.0f32; 6]; 3];
        let mut diff = [0.0f32; 6];
        let mut i: i32 = 1;
        let mut buf_idx = dir_base;

        for d in 0..6usize {
          for c in 0..2usize {
            let step = i << c;
            let base = buf_idx * tpix;
            let center_g = rgb[(base as i32 + ti) as usize][1];
            let plus_g = rgb[(base as i32 + ti + step) as usize][1];
            let minus_g = rgb[(base as i32 + ti - step) as usize][1];
            let plus_h = rgb[(base as i32 + ti + step) as usize][h];
            let minus_h = rgb[(base as i32 + ti - step) as usize][h];

            let g = 2.0 * center_g - plus_g - minus_g;
            color_est[h][d] = g + plus_h + minus_h;

            if d > 1 {
              diff[d] += (plus_g - minus_g - plus_h + minus_h).powi(2) + g * g;
            }
            h ^= 2;
          }

          if d > 1 && (d & 1) != 0 {
            if diff[d - 1] < diff[d] {
              for c in 0..2usize {
                color_est[c * 2][d] = color_est[c * 2][d - 1];
              }
            }
          }

          if d < 2 || (d & 1) != 0 {
            let base = buf_idx * tpix;
            for c in 0..2usize {
              rgb[(base as i32 + ti) as usize][c * 2] = (color_est[c * 2][d] * 0.5).max(0.0);
            }
            buf_idx += 1;
          }
          i ^= tw ^ 1;
          h ^= 2;
        }

        tx += 3;
      }
      ty += 3;
    }
  }

  for ty in pad_rb_br..tile_h.saturating_sub(pad_rb_br) {
    let iy = (top + ty) as i32;
    for tx in pad_rb_br..tile_w.saturating_sub(pad_rb_br) {
      let ix = (left + tx) as i32;
      let ti = ty * tile_w + tx;

      let fc = cfa.color_at(iy as usize, ix as usize);
      let f = match fc {
        CFA_COLOR_R => 2usize,
        CFA_COLOR_B => 0usize,
        _ => continue,
      };

      let c: i32 = if ((iy - sgrow).rem_euclid(3)) != 0 { tw } else { 1 };
      let h: i32 = 3 * (c ^ tw ^ 1);

      for d in 0..4 {
        let base = (dir_base + d) * tpix;
        let rix = |off: i32| -> &[f32; 3] { &rgb[(base as i32 + ti as i32 + off) as usize] };

        let dir = if d > 1 || ((d as i32 ^ c) & 1) != 0
          || ((rix(0)[1] - rix(c)[1]).abs() + (rix(0)[1] - rix(-c)[1]).abs())
            < 2.0 * ((rix(0)[1] - rix(h)[1]).abs() + (rix(0)[1] - rix(-h)[1]).abs())

        {
          c
        } else {
          h
        };

        let val = (rix(dir)[f] + rix(-dir)[f]
          + 2.0 * rix(0)[1]
          - rix(dir)[1]
          - rix(-dir)[1])
          * 0.5;
        rgb[base + ti][f] = val.max(0.0);
      }
    }
  }

  for ty in pad_g2x2..tile_h.saturating_sub(pad_g2x2) {
    let iy = (top + ty) as i32;
    if ((iy - sgrow).rem_euclid(3)) == 0 {
      continue;
    }
    for tx in pad_g2x2..tile_w.saturating_sub(pad_g2x2) {
      let ix = (left + tx) as i32;
      if ((ix - sgcol).rem_euclid(3)) == 0 {
        continue;
      }
      let ti = (ty * tile_w + tx) as i32;
      let hex = &allhex.hex[iy as usize % 3][ix as usize % 3][1];

      for dd in 0..4usize {
        let hd = dd * 2;
        let base = ((dir_base + dd) * tpix) as i32;
        let (h0v, h0h) = hex[hd];
        let (h1v, h1h) = hex[hd + 1];
        let off0 = h0v * tw + h0h;
        let off1 = h1v * tw + h1h;

        if off0 + off1 != 0 {

          let g = 3.0 * rgb[(base + ti) as usize][1]
                - 2.0 * rgb[(base + ti + off0) as usize][1]
                - rgb[(base + ti + off1) as usize][1];
          for ch in (0..3).step_by(2) {
            rgb[(base + ti) as usize][ch] = ((g
              + 2.0 * rgb[(base + ti + off0) as usize][ch]
              + rgb[(base + ti + off1) as usize][ch])
              / 3.0)
              .max(0.0);
          }
        } else {

          let g = 2.0 * rgb[(base + ti) as usize][1]
                - rgb[(base + ti + off0) as usize][1]
                - rgb[(base + ti + off1) as usize][1];
          for ch in (0..3).step_by(2) {
            rgb[(base + ti) as usize][ch] = ((g
              + rgb[(base + ti + off0) as usize][ch]
              + rgb[(base + ti + off1) as usize][ch])
              * 0.5)
              .max(0.0);
          }
        }
      }
    }
  }
}

#[inline(always)]
pub fn rgb_to_lab(rgb: &[f32; 3]) -> [f32; 3] {
  let xyz = multiply_row1(&SRGB_TO_XYZ_D65, &rgb);
  XYZ_to_lab(&xyz, &CIE_1931_TRISTIMULUS_D65)
}
