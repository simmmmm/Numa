use multiversion::multiversion;
use rayon::prelude::*;
use std::time::Instant;

use crate::{
  cfa::{CFA, PlaneColor},
  imgop::{Rect, sensor::Demosaic},
  pixarray::{Color2D, PixF32, RgbF32},
};

#[derive(Default)]
pub struct XTransBilinearDemosaic {}

impl XTransBilinearDemosaic {
  pub fn new() -> Self {
    Self {}
  }
}

impl Demosaic<f32, 3> for XTransBilinearDemosaic {

  #[allow(unused)]
  fn demosaic(&self, pixels: &PixF32, cfa: &CFA, colors: &PlaneColor, roi: Rect) -> Color2D<f32, 3> {
    if !cfa.is_rgb() {
      panic!("CFA pattern '{}' is not a RGB pattern, can not demosaic", cfa);
    }
    let now = Instant::now();
    let rgb = interpolate_bilinear(pixels, cfa, roi);
    log::debug!("X-Trans bilinear demosaic total time: {:.5}s", now.elapsed().as_secs_f32());
    rgb
  }
}

#[multiversion(targets("x86_64+avx+avx2+fma", "x86+sse", "aarch64+neon"))]
fn interpolate_bilinear(input: &PixF32, cfa: &CFA, roi: Rect) -> Color2D<f32, 3> {
  let cfa_roi = cfa.shift(roi.p.x, roi.p.y);
  let mut output = RgbF32::new_with_default(roi.width(), roi.height(), f32::NAN);
  let width = output.width;
  let height = output.height;

  output.pixels_mut().par_chunks_exact_mut(width).enumerate().for_each(|(y, line)| {
    line.iter_mut().enumerate().for_each(|(x, pixel)| {
      let mut rgb = [0.0f32; 3];
      let mut count = [0u32; 3];

      let y_lo = y.saturating_sub(2);
      let y_hi = (y + 2).min(height - 1);
      let x_lo = x.saturating_sub(2);
      let x_hi = (x + 2).min(width - 1);

      for row in y_lo..=y_hi {
        for col in x_lo..=x_hi {
          let ch = cfa_roi.color_at(row, col) as usize;
          rgb[ch] += input.at(roi.p.y + row, roi.p.x + col);
          count[ch] += 1;
        }
      }

      for c in 0..3 {
        pixel[c] = if count[c] > 0 { rgb[c] / count[c] as f32 } else { 0.0 };
      }
    });
  });
  output
}
