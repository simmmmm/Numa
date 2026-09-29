use crate::{
  CFA,
  cfa::PlaneColor,
  imgop::Rect,
  pixarray::{Color2D, Pix2D, SubPixel},
};

pub mod bayer;
pub mod xtrans;

#[derive(PartialEq, Eq, PartialOrd, Ord, Debug, Clone, Copy)]
pub enum SensorType {

  Bayer,

  Xtrans,
}

impl SensorType {

  pub fn from_cfa(cfa: &CFA) -> Self {
    if cfa.width == 2 && cfa.height == 2 && cfa.unique_colors() >= 3 {
      Self::Bayer
    } else if cfa.width == 6 && cfa.height == 6 {
      Self::Xtrans
    } else {
      unimplemented!()
    }
  }
}

pub trait Demosaic<T: SubPixel, const N: usize> {

  fn demosaic(&self, pixels: &Pix2D<T>, cfa: &CFA, colors: &PlaneColor, roi: Rect) -> Color2D<T, N>;
}
