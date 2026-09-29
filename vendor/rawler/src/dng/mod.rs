pub mod convert;
pub mod original;
pub mod writer;

use crate::imgop::Rect;

pub const DNG_VERSION_V1_0: [u8; 4] = [1, 0, 0, 0];
pub const DNG_VERSION_V1_1: [u8; 4] = [1, 1, 0, 0];
pub const DNG_VERSION_V1_2: [u8; 4] = [1, 2, 0, 0];
pub const DNG_VERSION_V1_3: [u8; 4] = [1, 3, 0, 0];
pub const DNG_VERSION_V1_4: [u8; 4] = [1, 4, 0, 0];
pub const DNG_VERSION_V1_5: [u8; 4] = [1, 5, 0, 0];
pub const DNG_VERSION_V1_6: [u8; 4] = [1, 6, 0, 0];

pub fn rect_to_dng_area(area: &Rect) -> [u16; 4] {
  [
    area.p.y as u16,
    area.p.x as u16,
    area.p.y as u16 + area.d.h as u16,
    area.p.x as u16 + area.d.w as u16,
  ]

}

#[cfg(feature = "clap")]
impl clap::ValueEnum for DngCompression {
  fn value_variants<'a>() -> &'a [Self] {
    &[Self::Lossless, Self::Uncompressed]
  }

  fn to_possible_value(&self) -> Option<clap::builder::PossibleValue> {
    Some(match self {
      Self::Uncompressed => clap::builder::PossibleValue::new("uncompressed"),
      Self::Lossless => clap::builder::PossibleValue::new("lossless"),
    })
  }
}

#[derive(Clone, Copy, Debug)]
pub enum DngPhotometricConversion {
  Original,
  Linear,
}

impl Default for DngPhotometricConversion {
  fn default() -> Self {
    Self::Original
  }
}

#[derive(Clone, Copy, Debug)]
pub enum CropMode {
  Best,
  ActiveArea,
  None,
}

#[cfg(feature = "clap")]
impl clap::ValueEnum for CropMode {
  fn value_variants<'a>() -> &'a [Self] {
    &[Self::Best, Self::ActiveArea, Self::None]
  }

  fn to_possible_value(&self) -> Option<clap::builder::PossibleValue> {
    Some(match self {
      Self::Best => clap::builder::PossibleValue::new("best"),
      Self::ActiveArea => clap::builder::PossibleValue::new("activearea"),
      Self::None => clap::builder::PossibleValue::new("none"),
    })
  }
}

const PREVIEW_JPEG_QUALITY: f32 = 0.75;
#[derive(Clone, Copy, Debug, Eq, PartialEq)]

pub enum DngCompression {

  Uncompressed,

  Lossless,

}
