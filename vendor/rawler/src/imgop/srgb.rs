use rayon::prelude::*;

const SRGB_GAMMA: f32 = 2.4;

const SRGB_CROSSOVER_POINT: f32 = 0.00304;

const LINEAR_GAIN: f32 = 12.92;

const SRGB_GAIN: f32 = 1.055;

const SRGB_OFFSET: f32 = SRGB_GAIN - 1.0;

pub fn srgb_apply_gamma(v: f32) -> f32 {
  if v <= SRGB_CROSSOVER_POINT {
    v * LINEAR_GAIN
  } else {
    v.powf(1.0 / SRGB_GAMMA) * SRGB_GAIN - SRGB_OFFSET
  }
}

pub fn srgb_apply_gamma_n<const N: usize>(v: [f32; N]) -> [f32; N] {
  v.map(srgb_apply_gamma)
}

pub fn srgb_invert_gamma(v: f32) -> f32 {
  if v <= SRGB_CROSSOVER_POINT * LINEAR_GAIN {
    v / LINEAR_GAIN
  } else {
    ((v + SRGB_OFFSET) / SRGB_GAIN).powf(SRGB_GAMMA)
  }
}

pub fn srgb_apply_gamma_inplace(pixels: &mut [f32]) {
  pixels.par_iter_mut().for_each(|p| *p = srgb_apply_gamma(*p));
}

pub fn srgb_invert_gamma_inplace(pixels: &mut [f32]) {
  pixels.par_iter_mut().for_each(|p| *p = srgb_invert_gamma(*p));
}
