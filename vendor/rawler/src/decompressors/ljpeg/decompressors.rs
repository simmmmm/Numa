use super::LjpegDecompressor;
use super::huffman::*;
use crate::pumps::BitPumpJPEG;
use crate::pumps::BitPumpMSB32;

pub fn decode_ljpeg(ljpeg: &LjpegDecompressor, out: &mut [u16], x: usize, stripwidth: usize, width: usize, height: usize) -> Result<(), String> {
  let ncomp: usize = ljpeg.components();
  if ljpeg.sof.width * ncomp < width || ljpeg.sof.height < height {
    return Err(format!(
      "ljpeg: trying to decode {}x{} into {}x{}",
      ljpeg.sof.width, ljpeg.sof.height, width, height
    ));
  }

  if super::parallel::decode_ljpeg(ljpeg, out, x, stripwidth, width, height).is_some() {
    return Ok(());
  }

  let htable = |index: usize| -> &HuffTable { &ljpeg.dhts[ljpeg.sof.components[index].dc_tbl_num] };
  let mut pump = BitPumpJPEG::new(ljpeg.buffer);
  let base_prediction = 1 << (ljpeg.sof.precision - ljpeg.point_transform - 1);

  for c in 0..ncomp {
    out[x + c] = (base_prediction + htable(c).huff_decode(&mut pump)?) as u16;
  }

  let skip_x = ljpeg.sof.width - width / ncomp;

  for row in 0..height {
    let startcol = if row == 0 { x + ncomp } else { x };
    for col in (startcol..(width + x)).step_by(ncomp) {
      for c in 0..ncomp {
        let p: i32 = if col == x {

          out[(row - 1) * stripwidth + x + c] as i32
        } else {

          match (row, ljpeg.predictor) {
            (row @ 0, _) | (row, 1) => {
              let a = out[row * stripwidth + (col - ncomp) + c] as i32;
              a
            }
            (row, 2) => {
              let b = out[(row - 1) * stripwidth + col + c] as i32;
              b
            }
            (row, 3) => {
              let c = out[(row - 1) * stripwidth + (col - ncomp) + c] as i32;
              c
            }
            (row, 4) => {
              let a = out[row * stripwidth + (col - ncomp) + c] as i32;
              let b = out[(row - 1) * stripwidth + col + c] as i32;
              let c = out[(row - 1) * stripwidth + (col - ncomp) + c] as i32;
              a + b - c
            }
            (row, 5) => {
              let a = out[row * stripwidth + (col - ncomp) + c] as i32;
              let b = out[(row - 1) * stripwidth + col + c] as i32;
              let c = out[(row - 1) * stripwidth + (col - ncomp) + c] as i32;
              a + ((b - c) >> 1)
            }
            (row, 6) => {
              let a = out[row * stripwidth + (col - ncomp) + c] as i32;
              let b = out[(row - 1) * stripwidth + col + c] as i32;
              let c = out[(row - 1) * stripwidth + (col - ncomp) + c] as i32;
              b + ((a - c) >> 1)
            }
            (row, 7) => {
              let a = out[row * stripwidth + (col - ncomp) + c] as i32;
              let b = out[(row - 1) * stripwidth + col + c] as i32;
              (a + b) >> 1
            }
            _ => {
              panic!("Unsupported prediction in LJPEG")
            }
          }
        };

        let diff = htable(c).huff_decode(&mut pump)?;
        out[row * stripwidth + col + c] = (p + diff) as u16;
      }
    }
    for _ in 0..skip_x {
      for c in 0..ncomp {

        htable(c).huff_decode(&mut pump)?;
      }
    }
  }

  Ok(())
}

fn set_yuv_420(out: &mut [u16], row: usize, col: usize, width: usize, y1: i32, y2: i32, y3: i32, y4: i32, cb: i32, cr: i32) {
  let pix1 = row * width + col;
  let pix2 = pix1 + 3;
  let pix3 = (row + 1) * width + col;
  let pix4 = pix3 + 3;

  debug_assert!(!y1.is_negative());
  debug_assert!(!y2.is_negative());
  debug_assert!(!y3.is_negative());
  debug_assert!(!y4.is_negative());
  debug_assert!(!cb.is_negative());
  debug_assert!(!cr.is_negative());

  out[pix1 + 0] = y1 as u16;
  out[pix1 + 1] = cb as u16;
  out[pix1 + 2] = cr as u16;
  out[pix2 + 0] = y2 as u16;
  out[pix2 + 1] = cb as u16;
  out[pix2 + 2] = cr as u16;
  out[pix3 + 0] = y3 as u16;
  out[pix3 + 1] = cb as u16;
  out[pix3 + 2] = cr as u16;
  out[pix4 + 0] = y4 as u16;
  out[pix4 + 1] = cb as u16;
  out[pix4 + 2] = cr as u16;
}

pub fn decode_sony_ljpeg_420(ljpeg: &LjpegDecompressor, out: &mut [u16], width: usize, height: usize) -> Result<(), String> {
  if ljpeg.sof.width * 3 != width || ljpeg.sof.height != height {
    return Err(format!(
      "ljpeg: trying to decode {}x{} into {}x{}",
      ljpeg.sof.width * 3,
      ljpeg.sof.height,
      width,
      height
    ));
  }

  debug_assert_eq!(width % 2, 0);
  debug_assert_eq!(width % 6, 0);
  debug_assert_eq!(height % 2, 0);

  let htable1 = &ljpeg.dhts[ljpeg.sof.components[0].dc_tbl_num];
  let htable2 = &ljpeg.dhts[ljpeg.sof.components[1].dc_tbl_num];
  let htable3 = &ljpeg.dhts[ljpeg.sof.components[2].dc_tbl_num];
  let mut pump = BitPumpJPEG::new(ljpeg.buffer);

  let base_prediction = 1 << (ljpeg.sof.precision - ljpeg.point_transform - 1);

  let y1 = base_prediction + htable1.huff_decode(&mut pump)?;
  let y2 = y1 + htable1.huff_decode(&mut pump)?;
  let y3 = y1 + htable1.huff_decode(&mut pump)?;
  let y4 = y3 + htable1.huff_decode(&mut pump)?;

  let cb = base_prediction + htable2.huff_decode(&mut pump)?;
  let cr = base_prediction + htable3.huff_decode(&mut pump)?;

  set_yuv_420(out, 0, 0, width, y1, y2, y3, y4, cb, cr);

  for row in (0..height).step_by(2) {
    let startcol = if row == 0 { 6 } else { 0 };
    for col in (startcol..width).step_by(6) {

      let (py1, py3, pcb, pcr) = if col == 0 {

        let pos = (row - 2) * width;
        (out[pos], 0, out[pos + 1], out[pos + 2])
      } else {
        let pos1 = row * width + col - 3;
        let pos3 = (row + 1) * width + col - 3;
        (out[pos1], out[pos3], out[pos1 + 1], out[pos1 + 2])
      };

      let y1 = (py1 as i32) + htable1.huff_decode(&mut pump)?;
      let y2 = (y1 as i32) + htable1.huff_decode(&mut pump)?;
      let y3 = if col == 0 {

        (y1 as i32) + htable1.huff_decode(&mut pump)?
      } else {

        (py3 as i32) + htable1.huff_decode(&mut pump)?
      };
      let y4 = (y3 as i32) + htable1.huff_decode(&mut pump)?;

      let cb = (pcb as i32) + htable2.huff_decode(&mut pump)?;
      let cr = (pcr as i32) + htable3.huff_decode(&mut pump)?;
      set_yuv_420(out, row, col, width, y1, y2, y3, y4, cb, cr);
    }
  }

  Ok(())
}

pub fn decode_ljpeg_420(ljpeg: &LjpegDecompressor, out: &mut [u16], width: usize, height: usize) -> Result<(), String> {
  if ljpeg.sof.width * 3 != width || ljpeg.sof.height != height {
    return Err(format!(
      "ljpeg: trying to decode {}x{} into {}x{}",
      ljpeg.sof.width * 3,
      ljpeg.sof.height,
      width,
      height
    ));
  }

  debug_assert_eq!(width % 2, 0);
  debug_assert_eq!(width % 6, 0);
  debug_assert_eq!(height % 2, 0);

  let htable1 = &ljpeg.dhts[ljpeg.sof.components[0].dc_tbl_num];
  let htable2 = &ljpeg.dhts[ljpeg.sof.components[1].dc_tbl_num];
  let htable3 = &ljpeg.dhts[ljpeg.sof.components[2].dc_tbl_num];
  let mut pump = BitPumpJPEG::new(ljpeg.buffer);

  let base_prediction = 1 << (ljpeg.sof.precision - ljpeg.point_transform - 1);
  let y1 = base_prediction + htable1.huff_decode(&mut pump)?;
  let y2 = y1 + htable1.huff_decode(&mut pump)?;
  let y3 = y2 + htable1.huff_decode(&mut pump)?;
  let y4 = y3 + htable1.huff_decode(&mut pump)?;
  let cb = base_prediction + htable2.huff_decode(&mut pump)?;
  let cr = base_prediction + htable3.huff_decode(&mut pump)?;
  set_yuv_420(out, 0, 0, width, y1, y2, y3, y4, cb, cr);

  for row in (0..height).step_by(2) {
    let startcol = if row == 0 { 6 } else { 0 };
    for col in (startcol..width).step_by(6) {
      let pos = if col == 0 {

        (row - 2) * width
      } else {

        (row + 1) * width + col - 3
      };
      let (py, pcb, pcr) = (out[pos], out[pos + 1], out[pos + 2]);

      let y1 = (py as i32) + htable1.huff_decode(&mut pump)?;
      let y2 = (y1 as i32) + htable1.huff_decode(&mut pump)?;
      let y3 = (y2 as i32) + htable1.huff_decode(&mut pump)?;
      let y4 = (y3 as i32) + htable1.huff_decode(&mut pump)?;
      let cb = (pcb as i32) + htable2.huff_decode(&mut pump)?;
      let cr = (pcr as i32) + htable3.huff_decode(&mut pump)?;
      set_yuv_420(out, row, col, width, y1, y2, y3, y4, cb, cr);
    }
  }

  Ok(())
}

fn set_yuv_422(out: &mut [u16], row: usize, col: usize, width: usize, y1: i32, y2: i32, cb: i32, cr: i32) {
  let pix1 = row * width + col;
  let pix2 = pix1 + 3;

  debug_assert!(!y1.is_negative());
  debug_assert!(!y2.is_negative());
  debug_assert!(!cb.is_negative());
  debug_assert!(!cr.is_negative());

  out[pix1 + 0] = y1 as u16;
  out[pix1 + 1] = cb as u16;
  out[pix1 + 2] = cr as u16;
  out[pix2 + 0] = y2 as u16;
  out[pix2 + 1] = cb as u16;
  out[pix2 + 2] = cr as u16;
}

pub fn decode_ljpeg_422(ljpeg: &LjpegDecompressor, out: &mut [u16], width: usize, height: usize) -> Result<(), String> {
  if ljpeg.sof.width * 3 != width || ljpeg.sof.height != height {
    return Err(format!(
      "ljpeg: trying to decode {}x{} into {}x{}",
      ljpeg.sof.width * 3,
      ljpeg.sof.height,
      width,
      height
    ));
  }
  let htable1 = &ljpeg.dhts[ljpeg.sof.components[0].dc_tbl_num];
  let htable2 = &ljpeg.dhts[ljpeg.sof.components[1].dc_tbl_num];
  let htable3 = &ljpeg.dhts[ljpeg.sof.components[2].dc_tbl_num];
  let mut pump = BitPumpJPEG::new(ljpeg.buffer);

  let base_prediction = 1 << (ljpeg.sof.precision - ljpeg.point_transform - 1);
  let y1 = base_prediction + htable1.huff_decode(&mut pump)?;
  let y2 = y1 + htable1.huff_decode(&mut pump)?;
  let cb = base_prediction + htable2.huff_decode(&mut pump)?;
  let cr = base_prediction + htable3.huff_decode(&mut pump)?;
  set_yuv_422(out, 0, 0, width, y1, y2, cb, cr);

  for row in 0..height {
    let startcol = if row == 0 { 6 } else { 0 };
    for col in (startcol..width).step_by(6) {
      let pos = if col == 0 {

        (row - 1) * width
      } else {

        row * width + col - 3
      };
      let (py, pcb, pcr) = (out[pos], out[pos + 1], out[pos + 2]);

      let y1 = (py as i32) + htable1.huff_decode(&mut pump)?;
      let y2 = (y1 as i32) + htable1.huff_decode(&mut pump)?;
      let cb = (pcb as i32) + htable2.huff_decode(&mut pump)?;
      let cr = (pcr as i32) + htable3.huff_decode(&mut pump)?;

      set_yuv_422(out, row, col, width, y1, y2, cb, cr);
    }
  }

  Ok(())
}

pub fn decode_hasselblad(ljpeg: &LjpegDecompressor, out: &mut [u16], width: usize) -> Result<(), String> {

  let mut pump = BitPumpMSB32::new(ljpeg.buffer);
  let htable = &ljpeg.dhts[ljpeg.sof.components[0].dc_tbl_num];

  for line in out.chunks_exact_mut(width) {
    let mut p1: i32 = 0x8000;
    let mut p2: i32 = 0x8000;
    for o in line.chunks_exact_mut(2) {
      let len1 = htable.huff_len(&mut pump);
      let len2 = htable.huff_len(&mut pump);
      p1 += htable.huff_diff(&mut pump, len1);
      p2 += htable.huff_diff(&mut pump, len2);
      o[0] = p1 as u16;
      o[1] = p2 as u16;
    }
  }

  Ok(())
}

pub fn decode_leaf_strip(src: &[u8], out: &mut [u16], width: usize, height: usize, htable1: &HuffTable, htable2: &HuffTable, bpred: i32) -> Result<(), String> {
  let mut pump = BitPumpJPEG::new(src);
  out[0] = (bpred + htable1.huff_decode(&mut pump)?) as u16;
  out[1] = (bpred + htable2.huff_decode(&mut pump)?) as u16;
  for row in 0..height {
    let startcol = if row == 0 { 2 } else { 0 };
    for col in (startcol..width).step_by(2) {
      let pos = if col == 0 {

        (row - 1) * width
      } else {

        row * width + col - 2
      };
      let (p1, p2) = (out[pos], out[pos + 1]);

      let diff1 = htable1.huff_decode(&mut pump)?;
      let diff2 = htable2.huff_decode(&mut pump)?;
      out[row * width + col] = ((p1 as i32) + diff1) as u16;
      out[row * width + col + 1] = ((p2 as i32) + diff2) as u16;
    }
  }

  Ok(())
}
