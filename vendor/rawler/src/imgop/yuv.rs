use crate::bits::clampbits;
use rayon::prelude::*;

pub fn ycbcr_to_rgb(buf: &mut [u16]) {
  let cpp = 3;
  assert_eq!(buf.len() % cpp, 0, "pixel buffer must contains 3 samples/pixel");

  let corr: i32 = 16383;

  buf.par_chunks_exact_mut(cpp).for_each(|pix| {
    let y = pix[0] as f32;
    let cb = (pix[1] as i32 - corr) as f32;
    let cr = (pix[2] as i32 - corr) as f32;

    let r = y + 1.4 * cr;
    let g = y + (-0.343 * cb) + (-0.711 * cr);
    let b = y + 1.765 * cb;
    pix[0] = clampbits(r as i32, 16);
    pix[1] = clampbits(g as i32, 16);
    pix[2] = clampbits(b as i32, 16);
  })
}

pub fn interpolate_yuv(super_h: usize, super_v: usize, width: usize, _height: usize, image: &mut [u16]) {
  if super_h == 1 && super_v == 1 {
    return;
  }

  image.par_chunks_mut(width * 3).for_each(|slice| {

    if super_h == 2 {
      debug_assert_eq!(slice.len() % width, 0);
      for row in 0..(slice.len() / width) {
        for col in (6..width).step_by(6) {
          let pix1 = row * width + col - 6;
          let pix2 = pix1 + 3;
          let pix3 = row * width + col;
          slice[pix2 + 1] = ((slice[pix1 + 1] as i32 + slice[pix3 + 1] as i32 + 1) / 2) as u16;
          slice[pix2 + 2] = ((slice[pix1 + 2] as i32 + slice[pix3 + 2] as i32 + 1) / 2) as u16;
        }
      }
    }

    if super_v == 2 && slice.len() == width * 3 {
      for col in (0..width).step_by(3) {
        let pix1 = col;
        let pix2 = width + col;
        let pix3 = 2 * width + col;
        slice[pix2 + 1] = ((slice[pix1 + 1] as i32 + slice[pix3 + 1] as i32 + 1) / 2) as u16;
        slice[pix2 + 2] = ((slice[pix1 + 2] as i32 + slice[pix3 + 2] as i32 + 1) / 2) as u16;
      }
    }
  });
}
