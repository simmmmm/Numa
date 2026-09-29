pub mod bilinear;
pub mod ppg;
pub mod superpixel;

use multiversion::multiversion;
use rayon::prelude::*;

use crate::{
  cfa::CFA,
  imgop::{Dim2, Rect},
  pixarray::RgbF32,
};

#[multiversion(targets("x86_64+avx+avx2", "x86+sse", "aarch64+neon"))]
fn expand_bayer_rgb(raw: &[f32], dim: Dim2, cfa: &CFA, roi: Rect) -> RgbF32 {

  let cfa_roi = cfa.shift(roi.x(), roi.y());
  let mut out = RgbF32::new(roi.width(), roi.height());
  out.pixels_mut().par_chunks_exact_mut(roi.width()).enumerate().for_each(|(row_out, buf)| {
    let row_in = roi.p.y + row_out;
    let start_in = row_in * dim.w + roi.p.x;
    let line = &raw[start_in..start_in + roi.width()];
    for (col, (p_out, p_in)) in buf.iter_mut().zip(line.iter()).enumerate() {
      p_out[cfa_roi.color_at(row_out, col)] = *p_in;
    }
  });
  out
}

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum RgbBayerPattern {
  RGGB,
  BGGR,
  GBRG,
  GRBG,

}
