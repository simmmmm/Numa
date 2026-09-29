use std::borrow::Cow;
use std::collections::HashMap;

use itertools::Itertools;
use log::debug;
use serde::{Deserialize, Serialize};

use crate::Result;
use crate::cfa::PlaneColor;
use crate::imgop::raw::{correct_blacklevel, correct_blacklevel_cfa};
use crate::imgop::sensor::SensorType;
use crate::imgop::{convert_from_f32_scaled_u16, convert_to_f32_unscaled};
use crate::pixarray::SubPixel;
use crate::{
  CFA,
  decoders::*,
  formats::tiff::{Rational, Value},
  imgop::{
    Dim2, Point, Rect,
    raw::{ColorMatrix, DevelopParams},
    xyz::{FlatColorMatrix, Illuminant},
  },
  pixarray::PixU16,
  tags::TiffTag,
};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WhiteLevel(pub Vec<u32>);

impl Default for WhiteLevel {
  fn default() -> Self {
    Self(vec![u16::MAX as u32; 1])
  }
}

impl WhiteLevel {
  pub fn new(level: impl Into<Vec<u32>>) -> Self {
    Self(level.into())
  }

  pub fn new_bits(bits: u32, cpp: usize) -> Self {
    if bits > 32 {
      panic!("Whitelevel can only be calculated for max. 32 bits, but {} bits given", bits);
    }
    let level: u32 = ((1_u64 << bits) - 1) as u32;
    Self(vec![level; cpp])
  }

  pub fn as_vec(&self) -> Vec<f32> {
    self.0.iter().cloned().map(|x| x as f32).collect_vec()
  }

  pub fn as_bayer_array(&self) -> [f32; 4] {
    if self.0.len() == 4 {
      [self.0[0] as f32, self.0[1] as f32, self.0[2] as f32, self.0[3] as f32]
    } else {
      [self.0[0] as f32, self.0[0] as f32, self.0[0] as f32, self.0[0] as f32]
    }
  }
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
pub struct BlackLevel {
  pub levels: Vec<Rational>,
  pub cpp: usize,
  pub width: usize,
  pub height: usize,
}

impl Default for BlackLevel {
  fn default() -> Self {
    Self {
      levels: [Rational::from(0_u32)].into(),
      width: 1,
      height: 1,
      cpp: 1,
    }
  }
}

impl std::fmt::Debug for BlackLevel {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    let levels: Vec<f32> = self.levels.iter().map(|x| x.as_f32()).collect();
    f.write_fmt(format_args!("RepeatDim: {}:{}, cpp: {}, {:?}", self.height, self.width, self.cpp, levels))
  }
}

impl BlackLevel {
  pub fn new<T>(levels: &[T], width: usize, height: usize, cpp: usize) -> Self
  where
    T: Copy + Into<Rational>,
  {
    assert_eq!(levels.len(), width * height * cpp);
    Self {
      levels: levels.iter().map(|x| (*x).into()).collect(),
      width,
      height,
      cpp,
    }
  }

  pub fn zero(width: usize, height: usize, cpp: usize) -> Self {
    if width == 0 || height == 0 || cpp == 0 {
      panic!(
        "Blacklevel::zero() must not be called with zero value arguments: width: {}, height: {}, cpp: {}",
        width, height, cpp
      );
    }
    Self {
      levels: vec![0_u16; cpp * width * height].iter().map(|x| Rational::from(*x)).collect(),
      width,
      height,
      cpp,
    }
  }

  pub fn as_vec(&self) -> Vec<f32> {
    self.levels.iter().map(Rational::as_f32).collect_vec()
  }

  pub fn as_bayer_array(&self) -> [f32; 4] {
    if self.levels.len() == 4 {
      [
        self.levels[0].as_f32(),
        self.levels[1].as_f32(),
        self.levels[2].as_f32(),
        self.levels[3].as_f32(),
      ]
    } else {
      [
        self.levels[0].as_f32(),
        self.levels[0].as_f32(),
        self.levels[0].as_f32(),
        self.levels[0].as_f32(),
      ]
    }
  }

  pub fn sample_count(&self) -> usize {
    self.cpp * self.width * self.height
  }

  pub fn shift(&self, x: usize, y: usize) -> Self {
    if self.sample_count() == 1 {
      self.clone()
    } else {
      let mut trans = self.clone();
      let (w, h, cpp) = (trans.width, trans.height, trans.cpp);
      for yn in 0..h {
        for xn in 0..w {
          let ys = (yn + y) % h;
          let xs = (xn + x) % w;
          trans.levels[yn * w * cpp + xn * cpp..yn * w * cpp + xn * cpp + cpp]
            .copy_from_slice(&self.levels[ys * w * cpp + xs * cpp..ys * w * cpp + xs * cpp + cpp]);
        }
      }
      trans
    }
  }
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, Clone, PartialEq)]
pub enum RawPhotometricInterpretation {
  BlackIsZero,

  Cfa(CFAConfig),
  LinearRaw,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CFAConfig {
  pub cfa: CFA,
  pub colors: PlaneColor,
  pub sensor: SensorType,
}

impl CFAConfig {
  pub fn new(cfa: &CFA, colors: &PlaneColor) -> Self {
    Self {
      cfa: cfa.clone(),
      colors: colors.clone(),
      sensor: SensorType::from_cfa(cfa),
    }
  }

  pub fn new_from_camera(cam: &Camera) -> Self {
    Self {
      cfa: cam.cfa.clone(),
      colors: cam.plane_color.clone(),
      sensor: SensorType::from_cfa(&cam.cfa),
    }
  }
}

#[derive(Debug, Clone)]
pub struct RawImage {

  pub camera: Camera,

  pub make: String,

  pub model: String,

  pub clean_make: String,

  pub clean_model: String,

  pub width: usize,

  pub height: usize,

  pub cpp: usize,

  pub bps: usize,

  pub wb_coeffs: [f32; 4],

  pub whitelevel: WhiteLevel,

  pub blacklevel: BlackLevel,

  pub xyz_to_cam: [[f32; 3]; 4],

  pub photometric: RawPhotometricInterpretation,

  pub active_area: Option<Rect>,

  pub crop_area: Option<Rect>,

  pub blackareas: Vec<Rect>,

  pub orientation: Orientation,

  pub data: RawImageData,

  pub color_matrix: HashMap<Illuminant, FlatColorMatrix>,

  pub dng_tags: HashMap<u16, Value>,

  pub fuji_rotation_width: Option<usize>,
}

#[derive(Debug, Clone)]
pub enum RawImageData {

  Integer(Vec<u16>),

  Float(Vec<f32>),
}

impl RawImageData {
  pub fn as_f32<'a>(&'a self) -> Cow<'a, Vec<f32>> {
    match self {
      Self::Integer(data) => Cow::Owned(convert_to_f32_unscaled(data)),
      Self::Float(data) => Cow::Borrowed(data),
    }
  }

  pub fn force_integer(&mut self) {
    if let Self::Float(data) = self {
      *self = Self::Integer(convert_from_f32_scaled_u16(data, 0, u16::MAX));
    }
  }
}

impl RawImage {
  pub fn calc_black_levels<T>(cfa: &CFA, blackareas: &[Rect], width: usize, _height: usize, image: &[T]) -> Option<BlackLevel>
  where
    T: SubPixel,
  {
    let x = cfa.width * cfa.height;
    if x == 0 {
      return None;
    }
    assert!(!image.is_empty());

    #[derive(Clone, Copy)]
    struct Sample {
      avg: f32,
      count: usize,
    }

    if !blackareas.is_empty() {
      let mut samples = vec![Sample { avg: 0.0, count: 0 }; x];

      for area in blackareas {
        for row in area.p.y..area.p.y + area.d.h {
          for col in area.p.x..area.p.x + area.d.w {

            let color = (row % cfa.height) * cfa.width + (col % cfa.width);
            samples[color].avg += image[row * width + col].as_f32();
            samples[color].count += 1;
          }
        }
      }

      let blacklevels: Vec<f32> = samples.into_iter().map(|s| s.avg / s.count as f32).collect();

      debug!("Calculated blacklevels: {:?}", blacklevels);

      assert_eq!(cfa.width * cfa.height, 4);
      Some(BlackLevel::new(&[blacklevels[0], blacklevels[1], blacklevels[2], blacklevels[3]], 2, 2, 1))
    } else {
      None
    }
  }

  #[doc(hidden)]
  pub fn new(
    cam: Camera,
    image: PixU16,
    cpp: usize,
    wb_coeffs: [f32; 4],
    photometric: RawPhotometricInterpretation,
    blacklevel: Option<BlackLevel>,
    whitelevel: Option<WhiteLevel>,
    dummy: bool,
  ) -> RawImage {
    assert_eq!(image.width % cpp, 0);
    assert_eq!(dummy, !image.is_initialized());
    let sample_width = image.width;
    let pixel_width = image.width / cpp;

    let mut blackareas: Vec<Rect> = Vec::new();

    let active_area = cam.active_area.map(|area| Rect::new_with_borders(Dim2::new(pixel_width, image.height), &area));

    let blackarea_base = active_area.unwrap_or_else(|| Rect::new(Point::zero(), Dim2::new(sample_width, image.height)));

    if cpp == 1 {

      if let Some(ah) = cam.blackareah {
        blackareas.push(Rect::new_with_points(
          Point::new(blackarea_base.p.x, ah.0),
          Point::new(blackarea_base.p.x + blackarea_base.d.w, ah.0 + ah.1),
        ));
      }
      if let Some(av) = cam.blackareav {
        blackareas.push(Rect::new_with_points(
          Point::new(av.0, blackarea_base.p.y),
          Point::new(av.0 + av.1, blackarea_base.p.y + blackarea_base.d.h),
        ));
      }
    }

    let blacklevel = cam
      .make_blacklevel(cpp)
      .or_else(|| if cam.find_hint("invalid_blacklevel") { None } else { blacklevel })
      .or_else(|| {
        if dummy {
          Some(BlackLevel::default())
        } else {
          Self::calc_black_levels::<u16>(&cam.cfa, &blackareas, image.width, image.height, image.pixels())
        }
      })
      .unwrap_or_else(|| BlackLevel::zero(1, 1, cpp));

    let whitelevel = cam
      .make_whitelevel(cpp)
      .or(whitelevel)
      .unwrap_or_else(|| panic!("Need whitelevel in config: {}", cam.clean_model));

    let crop_area = cam.crop_area.map(|area| Rect::new_with_borders(Dim2::new(pixel_width, image.height), &area));

    RawImage {
      camera: cam.clone(),
      make: cam.make.clone(),
      model: cam.model.clone(),
      clean_make: cam.clean_make.clone(),
      clean_model: cam.clean_model.clone(),
      width: image.width / cpp,
      height: image.height,
      cpp,
      bps: cam.real_bps,
      wb_coeffs,
      data: RawImageData::Integer(image.into_inner()),
      blacklevel,
      whitelevel,
      xyz_to_cam: cam.xyz_to_cam,
      photometric,
      active_area,
      crop_area,
      blackareas,
      orientation: Orientation::Normal,
      color_matrix: cam.color_matrix,
      dng_tags: HashMap::new(),
      fuji_rotation_width: None,
    }
  }

  #[doc(hidden)]
  pub fn new_with_data(
    cam: Camera,
    image: RawImageData,
    sample_width: usize,
    height: usize,
    cpp: usize,
    wb_coeffs: [f32; 4],
    photometric: RawPhotometricInterpretation,
    blacklevel: Option<BlackLevel>,
    whitelevel: Option<WhiteLevel>,
    dummy: bool,
  ) -> RawImage {

    let pixel_width = sample_width / cpp;

    let mut blackareas: Vec<Rect> = Vec::new();

    let active_area = cam.active_area.map(|area| Rect::new_with_borders(Dim2::new(pixel_width, height), &area));

    let blackarea_base = active_area.unwrap_or_else(|| Rect::new(Point::zero(), Dim2::new(sample_width, height)));

    if cpp == 1 {

      if let Some(ah) = cam.blackareah {
        blackareas.push(Rect::new_with_points(
          Point::new(blackarea_base.p.x, ah.0),
          Point::new(blackarea_base.p.x + blackarea_base.d.w, ah.0 + ah.1),
        ));
      }
      if let Some(av) = cam.blackareav {
        blackareas.push(Rect::new_with_points(
          Point::new(av.0, blackarea_base.p.y),
          Point::new(av.0 + av.1, blackarea_base.p.y + blackarea_base.d.h),
        ));
      }
    }

    let blacklevel = cam
      .make_blacklevel(cpp)
      .or_else(|| if cam.find_hint("invalid_blacklevel") { None } else { blacklevel })
      .or_else(|| {
        if dummy {
          Some(BlackLevel::default())
        } else {
          match &image {
            RawImageData::Integer(pix) => Self::calc_black_levels(&cam.cfa, &blackareas, sample_width, height, pix),
            RawImageData::Float(pix) => Self::calc_black_levels(&cam.cfa, &blackareas, sample_width, height, pix),
          }
        }
      })
      .unwrap_or_else(|| BlackLevel::zero(1, 1, cpp));

    let whitelevel = cam
      .make_whitelevel(cpp)
      .or(whitelevel)
      .unwrap_or_else(|| panic!("Need whitelevel in config: {}", cam.clean_model));

    let crop_area = cam.crop_area.map(|area| Rect::new_with_borders(Dim2::new(pixel_width, height), &area));

    RawImage {
      camera: cam.clone(),
      make: cam.make.clone(),
      model: cam.model.clone(),
      clean_make: cam.clean_make.clone(),
      clean_model: cam.clean_model.clone(),
      width: pixel_width,
      height: height,
      cpp,
      bps: cam.real_bps,
      wb_coeffs,
      data: image,
      blacklevel,
      whitelevel,
      xyz_to_cam: cam.xyz_to_cam,
      photometric,
      active_area,
      crop_area,
      blackareas,
      orientation: Orientation::Normal,
      color_matrix: cam.color_matrix,
      dng_tags: HashMap::new(),
      fuji_rotation_width: None,
    }
  }

  pub fn dim(&self) -> Dim2 {
    Dim2::new(self.width, self.height)
  }

  pub fn pixels_u16(&self) -> &[u16] {
    if let RawImageData::Integer(data) = &self.data {
      data
    } else {
      panic!("Data is not u16");
    }
  }

  pub fn pixels_u16_mut(&mut self) -> &mut [u16] {
    if let RawImageData::Integer(data) = &mut self.data {
      data
    } else {
      panic!("Data is not u16");
    }
  }

  pub fn apply_scaling(&mut self) -> crate::Result<()> {
    let mut pixels = self.data.as_f32();
    match &self.photometric {
      RawPhotometricInterpretation::BlackIsZero => todo!(),
      RawPhotometricInterpretation::Cfa(_) => {
        correct_blacklevel_cfa(
          pixels.to_mut(),
          self.width,
          self.height,
          &self.blacklevel.as_bayer_array(),
          &self.whitelevel.as_bayer_array(),
        );
        self.data = RawImageData::Float(pixels.into_owned());
      }
      RawPhotometricInterpretation::LinearRaw => {
        correct_blacklevel(pixels.to_mut(), &self.blacklevel.as_vec(), &self.whitelevel.as_vec());
        self.data = RawImageData::Float(pixels.into_owned());
      }
    }
    self.blacklevel.levels.iter_mut().for_each(|x| *x = Rational::new(0, 1));
    self.whitelevel.0.iter_mut().for_each(|x| *x = 1);
    Ok(())
  }

  pub fn develop_params(&self) -> Result<DevelopParams> {
    let mut xyz2cam: [[f32; 3]; 4] = [[0.0; 3]; 4];

    let color_matrix = self
      .color_matrix
      .values()
      .next()
      .cloned()
      .unwrap_or(vec![1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0]);
    assert_eq!(color_matrix.len() % 3, 0);
    let components = color_matrix.len() / 3;
    for i in 0..components {
      for j in 0..3 {
        xyz2cam[i][j] = color_matrix[i * 3 + j];
      }
    }

    debug!("RAW developing active area: {:?}", self.active_area);

    let wb_coeff = if self.wb_coeffs[0].is_nan() { [1.0, 1.0, 1.0, 1.0] } else { self.wb_coeffs };

    let params = DevelopParams {
      width: self.width,
      height: self.height,
      color_matrices: vec![ColorMatrix {
        illuminant: Illuminant::D65,
        matrix: xyz2cam,
      }],
      whitelevel: self.whitelevel.clone(),
      blacklevel: self.blacklevel.clone(),

      photometric: self.photometric.clone(),
      wb_coeff,
      cpp: self.cpp,
      active_area: self.active_area,
      crop_area: self.crop_area,

    };

    Ok(params)
  }

  pub fn add_dng_tag<T: TiffTag, V: Into<Value>>(&mut self, tag: T, value: V) {
    let tag: u16 = tag.into();
    self.dng_tags.insert(tag, value.into());
  }

  pub fn linearize(&self) -> Result<Self> {
    todo!()
  }

  pub fn cam_to_xyz(&self) -> [[f32; 4]; 3] {
    self.pseudoinverse(self.xyz_to_cam)
  }

  pub fn cam_to_xyz_normalized(&self) -> [[f32; 4]; 3] {
    let mut xyz_to_cam = self.xyz_to_cam;

    for i in 0..4 {
      let mut num = 0.0;
      for j in 0..3 {
        num += xyz_to_cam[i][j];
      }
      for j in 0..3 {
        xyz_to_cam[i][j] = if num == 0.0 { 0.0 } else { xyz_to_cam[i][j] / num };
      }
    }

    self.pseudoinverse(xyz_to_cam)
  }

  pub fn neutralwb(&self) -> [f32; 4] {
    let rgb_to_xyz = [

      [0.412453, 0.357580, 0.180423],
      [0.212671, 0.715160, 0.072169],
      [0.019334, 0.119193, 0.950227],
    ];

    let mut rgb_to_cam = [[0.0; 3]; 4];
    for i in 0..4 {
      for j in 0..3 {
        rgb_to_cam[i][j] = 0.0;
        for k in 0..3 {
          rgb_to_cam[i][j] += self.xyz_to_cam[i][k] * rgb_to_xyz[k][j];
        }
      }
    }

    let mut neutralwb = [0 as f32; 4];
    for i in 0..4 {
      let mut num = 0.0;
      for j in 0..3 {
        num += rgb_to_cam[i][j];
      }
      neutralwb[i] = 1.0 / num;
    }

    [
      neutralwb[0] / neutralwb[1],
      neutralwb[1] / neutralwb[1],
      neutralwb[2] / neutralwb[1],
      neutralwb[3] / neutralwb[1],
    ]
  }

  fn pseudoinverse(&self, inm: [[f32; 3]; 4]) -> [[f32; 4]; 3] {
    let mut temp: [[f32; 6]; 3] = [[0.0; 6]; 3];

    for i in 0..3 {
      for j in 0..6 {
        temp[i][j] = if j == i + 3 { 1.0 } else { 0.0 };
      }
      for j in 0..3 {
        for k in 0..4 {
          temp[i][j] += inm[k][i] * inm[k][j];
        }
      }
    }

    for i in 0..3 {
      let mut num = temp[i][i];
      for j in 0..6 {
        temp[i][j] /= num;
      }
      for k in 0..3 {
        if k == i {
          continue;
        }
        num = temp[k][i];
        for j in 0..6 {
          temp[k][j] -= temp[i][j] * num;
        }
      }
    }

    let mut out: [[f32; 4]; 3] = [[0.0; 4]; 3];

    for i in 0..4 {
      for j in 0..3 {
        out[j][i] = 0.0;
        for k in 0..3 {
          out[j][i] += temp[j][k + 3] * inm[i][k];
        }
      }
    }

    out
  }

  pub fn cropped_cfa(&self) -> CFA {

    todo!()

  }

  pub fn is_monochrome(&self) -> bool {
    self.photometric == RawPhotometricInterpretation::BlackIsZero

  }

  pub fn color_matrix_find_first(&self, illuminants: impl IntoIterator<Item = Illuminant>) -> Option<(Illuminant, FlatColorMatrix)> {
    for illu in illuminants.into_iter() {
      if let Some(matrix) = self.color_matrix.get(&illu) {
        return Some((illu, matrix.clone()));
      }
    }
    None
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn blacklevel_shift() -> std::result::Result<(), Box<dyn std::error::Error>> {
    let black = BlackLevel::new(&[1_u16, 2, 3, 4], 2, 2, 1).shift(1, 1);
    assert_eq!(black.levels, vec![4_u16, 3, 2, 1].into_iter().map(Rational::from).collect::<Vec<Rational>>());

    let black = BlackLevel::new(&[1_u16, 1, 1, 2, 2, 2, 3, 3, 3, 4, 4, 4], 2, 2, 3).shift(3, 3);
    assert_eq!(
      black.levels,
      vec![4_u16, 4, 4, 3, 3, 3, 2, 2, 2, 1, 1, 1]
        .into_iter()
        .map(Rational::from)
        .collect::<Vec<Rational>>()
    );
    Ok(())
  }
}
