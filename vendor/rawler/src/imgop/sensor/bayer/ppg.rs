use multiversion::multiversion;
use rayon::prelude::*;
use std::{ops::Add, time::Instant};

use crate::{
  cfa::{CFA, CFA_COLOR_B, CFA_COLOR_G, CFA_COLOR_R, PlaneColor},
  imgop::{Rect, sensor::Demosaic},
  pixarray::{Color2D, PixF32, RgbF32},
};

#[derive(Default)]
pub struct PPGDemosaic {}

impl PPGDemosaic {
  pub fn new() -> Self {
    Self {}
  }
}

impl Demosaic<f32, 3> for PPGDemosaic {

  fn demosaic(&self, pixels: &PixF32, cfa: &CFA, _colors: &PlaneColor, roi: Rect) -> Color2D<f32, 3> {

    if !cfa.is_rgb() {
      panic!("CFA pattern '{}' is not a RGB pattern, can not demosaic with PPG", cfa);
    }

    let now = Instant::now();

    let cfa_roi = cfa.shift(roi.p.x, roi.p.y);

    let mut rgb = super::expand_bayer_rgb(pixels.pixels(), pixels.dim(), cfa, roi);

    interpolate_borders(&mut rgb, &cfa_roi);
    interpolate_green(&mut rgb, &cfa_roi);
    interpolate_rb_at_green(&mut rgb, &cfa_roi);
    interpolate_rb_at_non_green(&mut rgb, &cfa_roi);

    log::debug!("PPG total debayer time: {:.5}s", now.elapsed().as_secs_f32());
    rgb
  }
}

#[multiversion(targets("x86_64+avx+avx2", "x86+sse", "aarch64+neon"))]
fn interpolate_borders(input: &mut RgbF32, shifted: &CFA) {
  let w = input.width;
  let h = input.height;

  for row in 0..h {
    let mut col = 0;
    while col < w {

      if col == 3 && row >= 3 && row < h - 3 {
        col = w - 3
      }

      let mut sum = [(0.0, 0_usize); 3];

      for y in row.saturating_sub(1)..=row.add(1) {
        for x in col.saturating_sub(1)..=col.add(1) {

          if y < h && x < w {
            let ch = shifted.color_at(y, x);
            sum[ch].0 += input.at(y, x)[ch];
            sum[ch].1 += 1;
          }
        }
      }

      let ch = shifted.color_at(row, col);
      for (color, p) in input.at_mut(row, col).iter_mut().enumerate() {

        if color != ch && sum[color].1 > 0 {
          *p = sum[color].0 / sum[color].1 as f32;
        }
      }
      col += 1;
    }
  }
}

#[multiversion(targets("x86_64+avx+avx2", "x86+sse", "aarch64+neon"))]
fn interpolate_green(img: &mut RgbF32, shifted: &CFA) {
  let w = img.width;
  let h = img.height;

  let dataptr = img.data_ptr();

  img.pixels_mut().par_chunks_exact_mut(w).enumerate().skip(3).take(h - 6).for_each(|(row, buf)| {
    for (col, pixel) in buf.iter_mut().enumerate().skip(3).take(w - 6) {
      if shifted.color_at(row, col) != CFA_COLOR_G {
        let ch = shifted.color_at(row, col);
        let x = pixel[ch];
        let n_1 = unsafe { dataptr.at(row - 1, col)[CFA_COLOR_G] };
        let n_2 = unsafe { dataptr.at(row - 2, col)[ch] };
        let e_1 = unsafe { dataptr.at(row, col + 1)[CFA_COLOR_G] };
        let e_2 = unsafe { dataptr.at(row, col + 2)[ch] };
        let s_1 = unsafe { dataptr.at(row + 1, col)[CFA_COLOR_G] };
        let s_2 = unsafe { dataptr.at(row + 2, col)[ch] };
        let w_1 = unsafe { dataptr.at(row, col - 1)[CFA_COLOR_G] };
        let w_2 = unsafe { dataptr.at(row, col - 2)[ch] };

        let n = (x - n_2).abs() * 2.0 + (n_1 - s_1).abs();
        let e = (x - e_2).abs() * 2.0 + (w_1 - e_1).abs();
        let w = (x - w_2).abs() * 2.0 + (w_1 - e_1).abs();
        let s = (x - s_2).abs() * 2.0 + (n_1 - s_1).abs();

        let (mut min, mut p_green) = (n, (n_1 * 3.0 + s_1 + x - n_2) / 4.0);
        let east = (e_1 * 3.0 + w_1 + x - e_2) / 4.0;
        let west = (w_1 * 3.0 + e_1 + x - w_2) / 4.0;
        let south = (s_1 * 3.0 + n_1 + x - s_2) / 4.0;
        if e < min {
          (min, p_green) = (e, east);
        }
        if w < min {
          (min, p_green) = (w, west);
        }
        if s < min {
          p_green = south;
        }
        pixel[CFA_COLOR_G] = p_green;
      }
    }
  });
}

#[multiversion(targets("x86_64+avx+avx2", "x86+sse", "aarch64+neon"))]
fn interpolate_rb_at_green(img: &mut RgbF32, shifted: &CFA) {
  let w = img.width;
  let h = img.height;

  let dataptr = img.data_ptr();

  img.pixels_mut().par_chunks_exact_mut(w).enumerate().skip(3).take(h - 6).for_each(|(row, buf)| {
    for (col, pixel) in buf.iter_mut().enumerate().skip(3).take(w - 6) {
      if shifted.color_at(row, col) == CFA_COLOR_G {
        let h_ch = shifted.color_at(row, col + 1);
        let v_ch = shifted.color_at(row + 1, col);

        let g_x = pixel[CFA_COLOR_G];
        let g_w = unsafe { dataptr.at(row, col - 1)[CFA_COLOR_G] };
        let g_e = unsafe { dataptr.at(row, col + 1)[CFA_COLOR_G] };
        let g_n = unsafe { dataptr.at(row - 1, col)[CFA_COLOR_G] };
        let g_s = unsafe { dataptr.at(row + 1, col)[CFA_COLOR_G] };

        let h_w = unsafe { dataptr.at(row, col - 1).get_unchecked(h_ch) };
        let h_e = unsafe { dataptr.at(row, col + 1).get_unchecked(h_ch) };

        let v_n = unsafe { dataptr.at(row - 1, col).get_unchecked(v_ch) };
        let v_s = unsafe { dataptr.at(row + 1, col).get_unchecked(v_ch) };

        *unsafe { pixel.get_unchecked_mut(h_ch) } = hue_transit(g_w, g_x, g_e, *h_w, *h_e);
        *unsafe { pixel.get_unchecked_mut(v_ch) } = hue_transit(g_n, g_x, g_s, *v_n, *v_s);
      }
    }
  });
}

#[multiversion(targets("x86_64+avx+avx2", "x86+sse", "aarch64+neon"))]
fn interpolate_rb_at_non_green(img: &mut RgbF32, shifted: &CFA) {
  let w = img.width;
  let h = img.height;

  let dataptr = img.data_ptr();

  img.pixels_mut().par_chunks_exact_mut(w).enumerate().skip(3).take(h - 6).for_each(|(row, buf)| {
    for (col, pixel) in buf.iter_mut().enumerate().skip(3).take(w - 6) {
      if shifted.color_at(row, col) != CFA_COLOR_G {
        let x_ch = shifted.color_at(row, col);
        let y_ch = if x_ch == CFA_COLOR_R { CFA_COLOR_B } else { CFA_COLOR_R };

        let y_ne_1 = unsafe { dataptr.at(row - 1, col + 1)[y_ch] };
        let y_sw_1 = unsafe { dataptr.at(row + 1, col - 1)[y_ch] };
        let x_ne_2 = unsafe { dataptr.at(row - 2, col + 2)[x_ch] };
        let x_center = pixel[x_ch];
        let x_sw_2 = unsafe { dataptr.at(row + 2, col - 2)[x_ch] };
        let g_ne_1 = unsafe { dataptr.at(row - 1, col + 1)[CFA_COLOR_G] };
        let g_center = pixel[CFA_COLOR_G];
        let g_sw_1 = unsafe { dataptr.at(row + 1, col - 1)[CFA_COLOR_G] };
        let y_nw_1 = unsafe { dataptr.at(row - 1, col - 1)[y_ch] };
        let y_se_1 = unsafe { dataptr.at(row + 1, col + 1)[y_ch] };
        let x_nw_2 = unsafe { dataptr.at(row - 2, col - 2)[x_ch] };
        let x_se_2 = unsafe { dataptr.at(row + 2, col + 2)[x_ch] };
        let g_nw_1 = unsafe { dataptr.at(row - 1, col - 1)[CFA_COLOR_G] };
        let g_se_1 = unsafe { dataptr.at(row + 1, col + 1)[CFA_COLOR_G] };

        let ne = (y_ne_1 - y_sw_1).abs() + (x_ne_2 - x_center).abs() + (x_center - x_sw_2).abs() + (g_ne_1 - g_center).abs() + (g_center - g_sw_1).abs();

        let nw = (y_nw_1 - y_se_1).abs() + (x_nw_2 - x_center).abs() + (x_center - x_se_2).abs() + (g_nw_1 - g_center).abs() + (g_center - g_se_1).abs();

        let along_ne = hue_transit(g_ne_1, g_center, g_sw_1, y_ne_1, y_sw_1);
        let along_nw = hue_transit(g_nw_1, g_center, g_se_1, y_nw_1, y_se_1);
        pixel[y_ch] = if ne < nw { along_ne } else { along_nw };
      }
    }
  });
}

#[inline(always)]
fn hue_transit(l1: f32, l2: f32, l3: f32, v1: f32, v3: f32) -> f32 {

  let monotonic = (l1 < l2 && l2 < l3) || (l1 > l2 && l2 > l3);
  let along = v1 + (v3 - v1) * (l2 - l1) / (l3 - l1);
  let across = (v1 + v3) / 2.0 + (l2 * 2.0 - l1 - l3) / 4.0;
  if monotonic { along } else { across }
}
