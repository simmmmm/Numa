pub fn apply_gamma(v: f32, gamma: f32) -> f32 {
  v.powf(1.0 / gamma)
}

pub fn invert_gamma(v: f32, gamma: f32) -> f32 {
  v.powf(gamma)
}
