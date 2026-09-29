use multiversion::multiversion;

use crate::alloc_image_ok;
use crate::bits::*;
use crate::bits::{BEf16, BEf24, Endian, LEf16, LEf24};
use crate::decompressors::decompress_lines;
use crate::decompressors::{Decompressor, LineIteratorMut, unpack_lines_fn};
use crate::pixarray::PixU16;
use crate::pumps::{BitPump, BitPumpLSB, BitPumpMSB};

pub struct PackedDecompressor {
  bps: u32,
  endian: Endian,
}

impl PackedDecompressor {

  pub fn new(bps: u32, endian: Endian) -> Self {
    Self { bps, endian }
  }
}

impl<'a> Decompressor<'a, u16> for PackedDecompressor {

  fn decompress(&self, src: &[u8], skip_rows: usize, lines: impl LineIteratorMut<'a, u16>, line_width: usize) -> std::result::Result<(), String> {
    match (self.endian, self.bps) {

      (Endian::Big, 16) => unpack_16be(lines, src, skip_rows, line_width),
      (Endian::Little, 16) => unpack_16le(lines, src, skip_rows, line_width),

      (_, 12) => unpack_12be(lines, src, skip_rows, line_width),

      (_, 10) => unpack_10be(lines, src, skip_rows, line_width),

      (_, 8) => unpack_8bit(lines, src, skip_rows, line_width),

      (_, bps) if bps > 0 && bps < 16 => unpack_generic_msb(lines, src, skip_rows, line_width, self.bps),

      (_, bps) => return Err(format!("unsupported packed compression scheme: {bps} bps")),
    }
  }

  fn can_skip_rows(&self) -> bool {
    true
  }
}

#[multiversion(targets("x86_64+avx+avx2+fma", "x86+sse", "aarch64+neon"))]
fn unpack_16be<'a>(lines: impl LineIteratorMut<'a, u16>, src: &[u8], skip_rows: usize, width: usize) -> std::result::Result<(), String> {
  const BITS: usize = 16;
  let need = ((skip_rows + lines.len()) * width * BITS).div_ceil(u8::BITS as usize);
  if src.len() < need {
    return Err(format!("unpack_16be(): buffer too short ({} < {})", src.len(), need));
  }
  for (row, line) in lines.enumerate() {
    let inb = &src[((skip_rows + row) * width * 2)..];
    for (out, bytes) in line.into_iter().zip(inb.as_chunks::<2>().0.into_iter()) {
      *out = u16::from_be_bytes(*bytes);
    }
  }
  Ok(())
}

#[multiversion(targets("x86_64+avx+avx2+fma", "x86+sse", "aarch64+neon"))]
fn unpack_16le<'a>(lines: impl LineIteratorMut<'a, u16>, src: &[u8], skip_rows: usize, width: usize) -> std::result::Result<(), String> {
  const BITS: usize = 16;
  let need = ((skip_rows + lines.len()) * width * BITS).div_ceil(u8::BITS as usize);
  if src.len() < need {
    return Err(format!("unpack_16le(): buffer too short ({} < {})", src.len(), need));
  }
  for (row, line) in lines.enumerate() {
    let inb = &src[((skip_rows + row) * width * 2)..];
    for (out, bytes) in line.into_iter().zip(inb.as_chunks::<2>().0.into_iter()) {
      *out = u16::from_le_bytes(*bytes);
    }
  }
  Ok(())
}

#[multiversion(targets("x86_64+avx+avx2+fma", "x86+sse", "aarch64+neon"))]
fn unpack_12be<'a>(lines: impl LineIteratorMut<'a, u16>, src: &[u8], skip_rows: usize, width: usize) -> std::result::Result<(), String> {
  const BITS: usize = 12;
  let need = ((skip_rows + lines.len()) * width * BITS).div_ceil(u8::BITS as usize);
  if src.len() < need {
    return Err(format!("unpack_12be(): buffer too short ({} < {})", src.len(), need));
  }
  for (row, line) in lines.enumerate() {
    let inb = &src[((skip_rows + row) * width * BITS / 8)..];
    for (o, i) in line.as_chunks_mut::<2>().0.into_iter().zip(inb.as_chunks::<3>().0.into_iter()) {
      let g1: u16 = i[0] as u16;
      let g2: u16 = i[1] as u16;
      let g3: u16 = i[2] as u16;

      o[0] = (g1 << 4) | (g2 >> 4);
      o[1] = ((g2 & 0x0f) << 8) | g3;
    }
  }
  Ok(())
}

#[multiversion(targets("x86_64+avx+avx2+fma", "x86+sse", "aarch64+neon"))]
fn unpack_10be<'a>(lines: impl LineIteratorMut<'a, u16>, src: &[u8], skip_rows: usize, width: usize) -> std::result::Result<(), String> {
  const BITS: usize = 10;
  let need = ((skip_rows + lines.len()) * width * BITS).div_ceil(u8::BITS as usize);
  if src.len() < need {
    return Err(format!("unpack_10be(): buffer too short ({} < {})", src.len(), need));
  }
  for (row, line) in lines.enumerate() {
    let inb = &src[((skip_rows + row) * width * BITS / 8)..];

    for (o, i) in line.as_chunks_mut::<4>().0.into_iter().zip(inb.as_chunks::<5>().0) {
      let g1: u16 = i[0] as u16;
      let g2: u16 = i[1] as u16;
      let g3: u16 = i[2] as u16;
      let g4: u16 = i[3] as u16;
      let g5: u16 = i[4] as u16;

      o[0] = (g1 << 2) | (g2 >> 6);
      o[1] = ((g2 & 0x3f) << 4) | (g3 >> 4);
      o[2] = ((g3 & 0x0f) << 6) | (g4 >> 2);
      o[3] = ((g4 & 0x03) << 8) | g5;
    }
  }
  Ok(())
}

#[multiversion(targets("x86_64+avx+avx2+fma", "x86+sse", "aarch64+neon"))]
fn unpack_8bit<'a>(lines: impl LineIteratorMut<'a, u16>, src: &[u8], skip_rows: usize, width: usize) -> std::result::Result<(), String> {
  const BITS: usize = 8;
  let need = ((skip_rows + lines.len()) * width * BITS).div_ceil(u8::BITS as usize);
  if src.len() < need {
    return Err(format!("unpack_8bit(): buffer too short ({} < {})", src.len(), need));
  }
  for (row, line) in lines.enumerate() {
    let inb = &src[((skip_rows + row) * width)..];
    for (o, i) in line.iter_mut().zip(inb.iter()) {
      *o = *i as u16;
    }
  }
  Ok(())
}

#[multiversion(targets("x86_64+avx+avx2+fma", "x86+sse", "aarch64+neon"))]
fn unpack_generic_msb<'a>(lines: impl LineIteratorMut<'a, u16>, src: &[u8], skip_rows: usize, width: usize, bits: u32) -> std::result::Result<(), String> {
  let need = ((skip_rows + lines.len()) * width * bits as usize).div_ceil(u8::BITS as usize);
  if src.len() < need {
    return Err(format!("unpack_generic_msb(): buffer too short ({} < {})", src.len(), need));
  }
  assert!(bits <= 16);
  let skip_bits = skip_rows * width * bits as usize;
  let offset = skip_bits / 8;
  let bias = skip_bits % 8;
  let mut pump = BitPumpMSB::new(&src[offset..]);
  pump.consume_bits(bias as u32);
  for line in lines {
    for p in line {
      *p = pump.get_bits(bits) as u16;
    }
  }
  Ok(())
}

impl<'a> Decompressor<'a, f32> for PackedDecompressor {
  fn decompress(&self, src: &[u8], skip_rows: usize, lines: impl LineIteratorMut<'a, f32>, line_width: usize) -> std::result::Result<(), String> {
    match (self.endian, self.bps) {

      (Endian::Big, 32) => unpack_f32be(lines, src, skip_rows, line_width),
      (Endian::Little, 32) => unpack_f32le(lines, src, skip_rows, line_width),

      (Endian::Big, 24) => unpack_f24be(lines, src, skip_rows, line_width),
      (Endian::Little, 24) => unpack_f24le(lines, src, skip_rows, line_width),

      (Endian::Big, 16) => unpack_f16be(lines, src, skip_rows, line_width),
      (Endian::Little, 16) => unpack_f16le(lines, src, skip_rows, line_width),

      (_, bps) => return Err(format!("f32: unsupported packed compression scheme: {bps} bps")),
    }
  }

  fn can_skip_rows(&self) -> bool {
    true
  }
}

#[multiversion(targets("x86_64+avx+avx2+fma", "x86+sse", "aarch64+neon"))]
fn unpack_f32be<'a>(lines: impl LineIteratorMut<'a, f32>, src: &[u8], skip_rows: usize, width: usize) -> std::result::Result<(), String> {
  const BITS: usize = 32;
  let need = ((skip_rows + lines.len()) * width * BITS).div_ceil(u8::BITS as usize);
  if src.len() < need {
    return Err(format!("unpack_f32be(): buffer too short ({} < {})", src.len(), need));
  }
  for (row, line) in lines.enumerate() {
    let inb = &src[((skip_rows + row) * width * size_of::<f32>())..];

    for (p, bytes) in line.into_iter().zip(inb.as_chunks::<4>().0.into_iter()) {
      *p = f32::from_be_bytes(*bytes);
    }
  }
  Ok(())
}

#[multiversion(targets("x86_64+avx+avx2+fma", "x86+sse", "aarch64+neon"))]
fn unpack_f32le<'a>(lines: impl LineIteratorMut<'a, f32>, src: &[u8], skip_rows: usize, width: usize) -> std::result::Result<(), String> {
  const BITS: usize = 32;
  let need = ((skip_rows + lines.len()) * width * BITS).div_ceil(u8::BITS as usize);
  if src.len() < need {
    return Err(format!("unpack_f32le(): buffer too short ({} < {})", src.len(), need));
  }
  for (row, line) in lines.enumerate() {
    let inb = &src[((skip_rows + row) * width * size_of::<f32>())..];
    for (p, bytes) in line.into_iter().zip(inb.as_chunks::<4>().0.into_iter()) {
      *p = f32::from_le_bytes(*bytes);
    }
  }
  Ok(())
}

#[multiversion(targets("x86_64+avx+avx2+fma", "x86+sse", "aarch64+neon"))]
fn unpack_f24le<'a>(lines: impl LineIteratorMut<'a, f32>, src: &[u8], skip_rows: usize, width: usize) -> std::result::Result<(), String> {
  const BITS: usize = 24;
  let need = ((skip_rows + lines.len()) * width * BITS).div_ceil(u8::BITS as usize);
  if src.len() < need {
    return Err(format!("unpack_f24le(): buffer too short ({} < {})", src.len(), need));
  }
  const SIZEOF_FP24: usize = 3;
  for (row, line) in lines.enumerate() {
    let inb = &src[((skip_rows + row) * width * SIZEOF_FP24)..];
    for (i, bytes) in (0..width).zip(inb.chunks_exact(SIZEOF_FP24)) {
      line[i] = LEf24(bytes, 0);
    }
  }
  Ok(())
}

#[multiversion(targets("x86_64+avx+avx2+fma", "x86+sse", "aarch64+neon"))]
fn unpack_f24be<'a>(lines: impl LineIteratorMut<'a, f32>, src: &[u8], skip_rows: usize, width: usize) -> std::result::Result<(), String> {
  const BITS: usize = 24;
  let need = ((skip_rows + lines.len()) * width * BITS).div_ceil(u8::BITS as usize);
  if src.len() < need {
    return Err(format!("unpack_f24be(): buffer too short ({} < {})", src.len(), need));
  }
  const SIZEOF_FP24: usize = 3;
  for (row, line) in lines.enumerate() {
    let inb = &src[((skip_rows + row) * width * SIZEOF_FP24)..];
    for (i, bytes) in (0..width).zip(inb.chunks_exact(SIZEOF_FP24)) {
      line[i] = BEf24(bytes, 0);
    }
  }
  Ok(())
}

#[multiversion(targets("x86_64+avx+avx2+fma", "x86+sse", "aarch64+neon"))]
fn unpack_f16le<'a>(lines: impl LineIteratorMut<'a, f32>, src: &[u8], skip_rows: usize, width: usize) -> std::result::Result<(), String> {
  const BITS: usize = 16;
  let need = ((skip_rows + lines.len()) * width * BITS).div_ceil(u8::BITS as usize);
  if src.len() < need {
    return Err(format!("unpack_f16le(): buffer too short ({} < {})", src.len(), need));
  }
  const SIZEOF_FP16: usize = 2;
  for (row, line) in lines.enumerate() {
    let inb = &src[((skip_rows + row) * width * SIZEOF_FP16)..];
    for (i, bytes) in (0..width).zip(inb.chunks_exact(SIZEOF_FP16)) {
      line[i] = LEf16(bytes, 0);
    }
  }
  Ok(())
}

#[multiversion(targets("x86_64+avx+avx2+fma", "x86+sse", "aarch64+neon"))]
fn unpack_f16be<'a>(lines: impl LineIteratorMut<'a, f32>, src: &[u8], skip_rows: usize, width: usize) -> std::result::Result<(), String> {
  const BITS: usize = 16;
  let need = ((skip_rows + lines.len()) * width * BITS).div_ceil(u8::BITS as usize);
  if src.len() < need {
    return Err(format!("unpack_f16be(): buffer too short ({} < {})", src.len(), need));
  }
  const SIZEOF_FP16: usize = 2;
  for (row, line) in lines.enumerate() {
    let inb = &src[((skip_rows + row) * width * SIZEOF_FP16)..];
    for (i, bytes) in (0..width).zip(inb.chunks_exact(SIZEOF_FP16)) {
      line[i] = BEf16(bytes, 0);
    }
  }
  Ok(())
}

#[multiversion(targets("x86_64+avx+avx2+fma", "x86+sse", "aarch64+neon"))]
pub(crate) fn decompress_8bit_wtable(buf: &[u8], tbl: &LookupTable, width: usize, height: usize, dummy: bool) -> std::result::Result<PixU16, String> {
  let need = height * width;
  if buf.len() < need {
    return Err(format!("decompress_8bit_wtable(): buffer too short ({} < {})", buf.len(), need));
  }
  unpack_lines_fn(
    width,
    height,
    dummy,
    &(|out: &mut [u16], row| {
      let inb = &buf[(row * width)..];
      let mut random = LEu32(inb, 0);

      for (o, i) in out.into_iter().zip(inb.into_iter()) {
        *o = tbl.dither(*i as u16, &mut random);
      }
      Ok(())
    }),
  )
}

#[multiversion(targets("x86_64+avx+avx2+fma", "x86+sse", "aarch64+neon"))]
pub(crate) fn decompress_8bit(buf: &[u8], width: usize, height: usize, dummy: bool) -> std::result::Result<PixU16, String> {
  let need = height * width;
  if buf.len() < need {
    return Err(format!("decompress_8bit(): buffer too short ({} < {})", buf.len(), need));
  }
  decompress_lines(buf, width, height, dummy, PackedDecompressor::new(8, Endian::Little))
}

#[multiversion(targets("x86_64+avx+avx2+fma", "x86+sse", "aarch64+neon"))]
pub(crate) fn decompress_10le_lsb16(buf: &[u8], width: usize, height: usize, dummy: bool) -> std::result::Result<PixU16, String> {
  let need = (height * width * 10).div_ceil(8);
  if buf.len() < need {
    return Err(format!("decompress_10le_lsb16(): buffer too short ({} < {})", buf.len(), need));
  }
  unpack_lines_fn(
    width,
    height,
    dummy,
    &(|out: &mut [u16], row| {
      let inb = &buf[(row * width * 10 / 8)..];

      for (o, i) in out.as_chunks_mut::<8>().0.into_iter().zip(inb.as_chunks::<10>().0) {
        let g1: u16 = i[0] as u16;
        let g2: u16 = i[1] as u16;
        let g3: u16 = i[2] as u16;
        let g4: u16 = i[3] as u16;
        let g5: u16 = i[4] as u16;
        let g6: u16 = i[5] as u16;
        let g7: u16 = i[6] as u16;
        let g8: u16 = i[7] as u16;
        let g9: u16 = i[8] as u16;
        let g10: u16 = i[9] as u16;

        o[0] = (g2 << 2) | (g1 >> 6);
        o[1] = ((g1 & 0x3f) << 4) | (g4 >> 4);
        o[2] = ((g4 & 0x0f) << 6) | (g3 >> 2);
        o[3] = ((g3 & 0x03) << 8) | g6;
        o[4] = (g5 << 2) | (g8 >> 6);
        o[5] = ((g8 & 0x3f) << 4) | (g7 >> 4);
        o[6] = ((g7 & 0x0f) << 6) | (g10 >> 2);
        o[7] = ((g10 & 0x03) << 8) | g9;
      }
      Ok(())
    }),
  )
}

#[multiversion(targets("x86_64+avx+avx2+fma", "x86+sse", "aarch64+neon"))]
pub(crate) fn decompress_10be(buf: &[u8], width: usize, height: usize, dummy: bool) -> std::result::Result<PixU16, String> {
  let need = (height * width * 10).div_ceil(8);
  if buf.len() < need {
    return Err(format!("decompress_10be(): buffer too short ({} < {})", buf.len(), need));
  }
  decompress_lines(buf, width, height, dummy, PackedDecompressor::new(10, Endian::Big))
}

#[multiversion(targets("x86_64+avx+avx2+fma", "x86+sse", "aarch64+neon"))]
pub(crate) fn decompress_12be(buf: &[u8], width: usize, height: usize, dummy: bool) -> std::result::Result<PixU16, String> {
  decompress_lines(buf, width, height, dummy, PackedDecompressor::new(12, Endian::Big))
}

#[multiversion(targets("x86_64+avx+avx2+fma", "x86+sse", "aarch64+neon"))]
pub(crate) fn decompress_12be_msb16(buf: &[u8], width: usize, height: usize, dummy: bool) -> std::result::Result<PixU16, String> {
  let need = (height * width * 12).div_ceil(8);
  if buf.len() < need {
    return Err(format!("decompress_12be_msb16(): buffer too short ({} < {})", buf.len(), need));
  }
  let mut out = alloc_image_ok!(width, height, dummy);

  for (o, i) in out.pixels_mut().chunks_exact_mut(4).zip(buf.chunks_exact(6)) {
    let g1: u16 = i[0] as u16;
    let g2: u16 = i[1] as u16;
    let g3: u16 = i[2] as u16;
    let g4: u16 = i[3] as u16;
    let g5: u16 = i[4] as u16;
    let g6: u16 = i[5] as u16;

    o[0] = (g2 << 4) | (g1 >> 4);
    o[1] = ((g1 & 0x0f) << 8) | g4;
    o[2] = (g3 << 4) | (g6 >> 4);
    o[3] = ((g6 & 0x0f) << 8) | g5;
  }
  Ok(out)
}

#[multiversion(targets("x86_64+avx+avx2+fma", "x86+sse", "aarch64+neon"))]
pub(crate) fn decompress_12le_16bitaligned(buf: &[u8], width: usize, height: usize, dummy: bool) -> std::result::Result<PixU16, String> {
  let stride = ((width * 12 / 8 + 1) >> 1) << 1;
  let need = height * stride;
  if buf.len() < need {
    return Err(format!("decompress_12le_16bitaligned(): buffer too short ({} < {})", buf.len(), need));
  }
  unpack_lines_fn(
    width,
    height,
    dummy,
    &(|out: &mut [u16], row| {
      let inb = &buf[row * stride..];
      for (o, i) in out.as_chunks_mut::<2>().0.into_iter().zip(inb.as_chunks::<3>().0) {
        let g1: u16 = i[0] as u16;
        let g2: u16 = i[1] as u16;
        let g3: u16 = i[2] as u16;

        o[0] = (g1 << 4) | (g2 >> 4);
        o[1] = ((g2 & 0x0f) << 8) | g3;
      }
      Ok(())
    }),
  )
}

#[multiversion(targets("x86_64+avx+avx2+fma", "x86+sse", "aarch64+neon"))]
pub(crate) fn decompress_12be_msb32(buf: &[u8], width: usize, height: usize, dummy: bool) -> std::result::Result<PixU16, String> {
  let need = (height * width * 12).div_ceil(8);
  if buf.len() < need {
    return Err(format!("decompress_12be_msb32(): buffer too short ({} < {})", buf.len(), need));
  }
  let mut out = alloc_image_ok!(width, height, dummy);

  for (o, i) in out.pixels_mut().chunks_exact_mut(8).zip(buf.chunks_exact(12)) {
    let g1: u16 = i[0] as u16;
    let g2: u16 = i[1] as u16;
    let g3: u16 = i[2] as u16;
    let g4: u16 = i[3] as u16;
    let g5: u16 = i[4] as u16;
    let g6: u16 = i[5] as u16;
    let g7: u16 = i[6] as u16;
    let g8: u16 = i[7] as u16;
    let g9: u16 = i[8] as u16;
    let g10: u16 = i[9] as u16;
    let g11: u16 = i[10] as u16;
    let g12: u16 = i[11] as u16;

    o[0] = (g4 << 4) | (g3 >> 4);
    o[1] = ((g3 & 0x0f) << 8) | g2;
    o[2] = (g1 << 4) | (g8 >> 4);
    o[3] = ((g8 & 0x0f) << 8) | g7;
    o[4] = (g6 << 4) | (g5 >> 4);
    o[5] = ((g5 & 0x0f) << 8) | g12;
    o[6] = (g11 << 4) | (g10 >> 4);
    o[7] = ((g10 & 0x0f) << 8) | g9;
  }
  Ok(out)
}

#[multiversion(targets("x86_64+avx+avx2+fma", "x86+sse", "aarch64+neon"))]
pub(crate) fn decompress_14be_msb32(buf: &[u8], width: usize, height: usize, dummy: bool) -> std::result::Result<PixU16, String> {
  let need = (height * width * 14).div_ceil(8);
  if buf.len() < need {
    return Err(format!("decompress_14be_msb32(): buffer too short ({} < {})", buf.len(), need));
  }

  let mut out = alloc_image_ok!(width, height, dummy);

  for (o, i) in out.pixels_mut().chunks_exact_mut(16).zip(buf.chunks_exact(28)) {
    let g1: u16 = i[0] as u16;
    let g2: u16 = i[1] as u16;
    let g3: u16 = i[2] as u16;
    let g4: u16 = i[3] as u16;

    let g5: u16 = i[4] as u16;
    let g6: u16 = i[5] as u16;
    let g7: u16 = i[6] as u16;
    let g8: u16 = i[7] as u16;

    let g9: u16 = i[8] as u16;
    let g10: u16 = i[9] as u16;
    let g11: u16 = i[10] as u16;
    let g12: u16 = i[11] as u16;

    let g13: u16 = i[12] as u16;
    let g14: u16 = i[13] as u16;
    let g15: u16 = i[14] as u16;
    let g16: u16 = i[15] as u16;

    let g17: u16 = i[16] as u16;
    let g18: u16 = i[17] as u16;
    let g19: u16 = i[18] as u16;
    let g20: u16 = i[19] as u16;

    let g21: u16 = i[20] as u16;
    let g22: u16 = i[21] as u16;
    let g23: u16 = i[22] as u16;
    let g24: u16 = i[23] as u16;

    let g25: u16 = i[24] as u16;
    let g26: u16 = i[25] as u16;
    let g27: u16 = i[26] as u16;
    let g28: u16 = i[27] as u16;

    o[0] = (g4 << 6) | (g3 >> 2);
    o[1] = ((g3 & 0x3) << 12) | (g2 << 4) | (g1 >> 4);
    o[2] = ((g1 & 0xf) << 10) | (g8 << 2) | (g7 >> 6);

    o[3] = ((g7 & 0x3f) << 8) | g6;
    o[4] = (g5 << 6) | (g12 >> 2);

    o[5] = ((g12 & 0x3) << 12) | (g11 << 4) | (g10 >> 4);
    o[6] = ((g10 & 0xf) << 10) | (g9 << 2) | (g16 >> 6);

    o[7] = ((g16 & 0x3f) << 8) | g15;
    o[8] = (g14 << 6) | (g13 >> 2);

    o[9] = ((g13 & 0x3) << 12) | (g20 << 4) | (g19 >> 4);
    o[10] = ((g19 & 0xf) << 10) | (g18 << 2) | (g17 >> 6);

    o[11] = ((g17 & 0x3f) << 8) | g24;
    o[12] = (g23 << 6) | (g22 >> 2);

    o[13] = ((g22 & 0x3) << 12) | (g21 << 4) | (g28 >> 4);
    o[14] = ((g28 & 0xf) << 10) | (g27 << 2) | (g26 >> 6);

    o[15] = ((g26 & 0x3f) << 8) | g25;
  }
  Ok(out)
}

#[multiversion(targets("x86_64+avx+avx2+fma", "x86+sse", "aarch64+neon"))]
pub(crate) fn decompress_12le_wcontrol(buf: &[u8], width: usize, height: usize, dummy: bool) -> std::result::Result<PixU16, String> {

  let perline = width * 12 / 8 + ((width + 2) / 10);

  let need = height * perline;
  if buf.len() < need {
    return Err(format!("decompress_12le_wcontrol(): buffer too short ({} < {})", buf.len(), need));
  }

  unpack_lines_fn(
    width,
    height,
    dummy,
    &(|out: &mut [u16], row| {
      let inb = &buf[(row * perline)..];

      for (oc, ic) in out.chunks_exact_mut(10).zip(inb.chunks_exact(16)) {
        for (o, i) in oc.chunks_exact_mut(2).zip(ic.chunks_exact(3)) {
          let g1: u16 = i[0] as u16;
          let g2: u16 = i[1] as u16;
          let g3: u16 = i[2] as u16;

          o[0] = ((g2 & 0x0f) << 8) | g1;
          o[1] = (g3 << 4) | (g2 >> 4);
        }
      }
      Ok(())
    }),
  )
}

#[multiversion(targets("x86_64+avx+avx2+fma", "x86+sse", "aarch64+neon"))]
pub(crate) fn decompress_12be_wcontrol(buf: &[u8], width: usize, height: usize, dummy: bool) -> std::result::Result<PixU16, String> {

  let perline = width * 12 / 8 + ((width + 2) / 10);

  let need = height * perline;
  if buf.len() < need {
    return Err(format!("decompress_12be_wcontrol(): buffer too short ({} < {})", buf.len(), need));
  }

  unpack_lines_fn(
    width,
    height,
    dummy,
    &(|out: &mut [u16], row| {
      let inb = &buf[(row * perline)..];

      for (oc, ic) in out.chunks_exact_mut(10).zip(inb.chunks_exact(16)) {
        for (o, i) in oc.chunks_exact_mut(2).zip(ic.chunks_exact(3)) {
          let g1: u16 = i[0] as u16;
          let g2: u16 = i[1] as u16;
          let g3: u16 = i[2] as u16;

          o[0] = (g1 << 4) | (g2 >> 4);
          o[1] = ((g2 & 0x0f) << 8) | g3;
        }
      }
      Ok(())
    }),
  )
}

#[multiversion(targets("x86_64+avx+avx2+fma", "x86+sse", "aarch64+neon"))]
pub(crate) fn decompress_12be_interlaced(buf: &[u8], width: usize, height: usize, dummy: bool) -> std::result::Result<PixU16, String> {
  let half = (height + 1) >> 1;

  let second_field_offset = (((half * width * 3 / 2) >> 11) + 1) << 11;

  let need = second_field_offset + ((height - half) * width * 12).div_ceil(8);
  if buf.len() < need {
    return Err(format!("decompress_12be_interlaced(): buffer too short ({} < {})", buf.len(), need));
  }
  let second_field = &buf[second_field_offset..];

  unpack_lines_fn(
    width,
    height,
    dummy,
    &(|out: &mut [u16], row| {
      let off = row / 2 * width * 12 / 8;
      let inb = if (row % 2) == 0 { &buf[off..] } else { &second_field[off..] };

      for (o, i) in out.as_chunks_mut::<2>().0.into_iter().zip(inb.as_chunks::<3>().0) {
        let g1: u16 = i[0] as u16;
        let g2: u16 = i[1] as u16;
        let g3: u16 = i[2] as u16;

        o[0] = (g1 << 4) | (g2 >> 4);
        o[1] = ((g2 & 0x0f) << 8) | g3;
      }
      Ok(())
    }),
  )
}

#[multiversion(targets("x86_64+avx+avx2+fma", "x86+sse", "aarch64+neon"))]
pub(crate) fn decompress_12be_interlaced_unaligned(buf: &[u8], width: usize, height: usize, dummy: bool) -> std::result::Result<PixU16, String> {
  let need = (height * width * 12).div_ceil(8);
  if buf.len() < need {
    return Err(format!("decompress_12be_interlaced_unaligned(): buffer too short ({} < {})", buf.len(), need));
  }

  let half = (height + 1) >> 1;
  let second_field = &buf[half * width * 12 / 8..];

  unpack_lines_fn(
    width,
    height,
    dummy,
    &(|out: &mut [u16], row| {
      let off = row / 2 * width * 12 / 8;
      let inb = if (row % 2) == 0 { &buf[off..] } else { &second_field[off..] };

      for (o, i) in out.as_chunks_mut::<2>().0.into_iter().zip(inb.as_chunks::<3>().0) {
        let g1: u16 = i[0] as u16;
        let g2: u16 = i[1] as u16;
        let g3: u16 = i[2] as u16;

        o[0] = (g1 << 4) | (g2 >> 4);
        o[1] = ((g2 & 0x0f) << 8) | g3;
      }
      Ok(())
    }),
  )
}

#[multiversion(targets("x86_64+avx+avx2+fma", "x86+sse", "aarch64+neon"))]
pub(crate) fn decompress_12le(buf: &[u8], width: usize, height: usize, dummy: bool) -> std::result::Result<PixU16, String> {
  let need = (height * width * 12).div_ceil(8);
  if buf.len() < need {
    return Err(format!("decompress_12le(): buffer too short ({} < {})", buf.len(), need));
  }
  unpack_lines_fn(
    width,
    height,
    dummy,
    &(|out: &mut [u16], row| {
      let inb = &buf[(row * width * 12 / 8)..];

      for (o, i) in out.as_chunks_mut::<2>().0.into_iter().zip(inb.as_chunks::<3>().0) {
        let g1: u16 = i[0] as u16;
        let g2: u16 = i[1] as u16;
        let g3: u16 = i[2] as u16;

        o[0] = ((g2 & 0x0f) << 8) | g1;
        o[1] = (g3 << 4) | (g2 >> 4);
      }
      Ok(())
    }),
  )

}

#[multiversion(targets("x86_64+avx+avx2+fma", "x86+sse", "aarch64+neon"))]
pub(crate) fn decompress_12le_padded(buf: &[u8], width: usize, height: usize, stripesize: usize, dummy: bool) -> std::result::Result<PixU16, String> {
  let need = height * stripesize;
  if buf.len() < need {
    return Err(format!("decompress_12le_padded(): buffer too short ({} < {})", buf.len(), need));
  }
  unpack_lines_fn(
    width,
    height,
    dummy,
    &(|out: &mut [u16], row| {
      let inb = &buf[(row * stripesize)..];

      for (o, i) in out.as_chunks_mut::<2>().0.into_iter().zip(inb.as_chunks::<3>().0) {
        let g1: u16 = i[0] as u16;
        let g2: u16 = i[1] as u16;
        let g3: u16 = i[2] as u16;

        o[0] = ((g2 & 0x0f) << 8) | g1;
        o[1] = (g3 << 4) | (g2 >> 4);
      }
      Ok(())
    }),
  )
}

#[multiversion(targets("x86_64+avx+avx2+fma", "x86+sse", "aarch64+neon"))]
pub(crate) fn decompress_14le_padded(buf: &[u8], width: usize, height: usize, stripesize: usize, dummy: bool) -> std::result::Result<PixU16, String> {
  let need = height * stripesize;
  if buf.len() < need {
    return Err(format!("decompress_14le_padded(): buffer too short ({} < {})", buf.len(), need));
  }
  unpack_lines_fn(
    width,
    height,
    dummy,
    &(|out: &mut [u16], row| {
      let inb = &buf[(row * stripesize)..];

      for (o, i) in out.as_chunks_mut::<4>().0.into_iter().zip(inb.as_chunks::<7>().0) {
        let g1: u16 = i[0] as u16;
        let g2: u16 = i[1] as u16;
        let g3: u16 = i[2] as u16;
        let g4: u16 = i[3] as u16;
        let g5: u16 = i[4] as u16;
        let g6: u16 = i[5] as u16;
        let g7: u16 = i[6] as u16;
        o[0] = ((g2 & 0x3f) << 8) | g1;
        o[1] = ((g4 & 0xf) << 10) | (g3 << 2) | (g2 >> 6);
        o[2] = ((g6 & 0x3) << 12) | (g5 << 4) | (g4 >> 4);
        o[3] = (g7 << 6) | (g6 >> 2);
      }
      Ok(())
    }),
  )
}

#[multiversion(targets("x86_64+avx+avx2+fma", "x86+sse", "aarch64+neon"))]
pub(crate) fn decompress_12le_unpacked(buf: &[u8], width: usize, height: usize, dummy: bool) -> std::result::Result<PixU16, String> {
  let need = height * width * 2;
  if buf.len() < need {
    return Err(format!("decompress_12le_unpacked(): buffer too short ({} < {})", buf.len(), need));
  }
  unpack_lines_fn(
    width,
    height,
    dummy,
    &(|out: &mut [u16], row| {
      let inb = &buf[(row * width * 2)..];
      for (i, bytes) in (0..width).zip(inb.as_chunks::<2>().0) {
        out[i] = u16::from_le_bytes(*bytes) & 0x0fff;
      }
      Ok(())
    }),
  )
}

#[multiversion(targets("x86_64+avx+avx2+fma", "x86+sse", "aarch64+neon"))]
pub(crate) fn decompress_12be_unpacked(buf: &[u8], width: usize, height: usize, dummy: bool) -> std::result::Result<PixU16, String> {
  let need = height * width * 2;
  if buf.len() < need {
    return Err(format!("decompress_12be_unpacked(): buffer too short ({} < {})", buf.len(), need));
  }
  unpack_lines_fn(
    width,
    height,
    dummy,
    &(|out: &mut [u16], row| {
      let inb = &buf[(row * width * 2)..];

      for (i, bytes) in (0..width).zip(inb.as_chunks::<2>().0) {
        out[i] = u16::from_be_bytes(*bytes) & 0x0fff;
      }
      Ok(())
    }),
  )
}

#[multiversion(targets("x86_64+avx+avx2+fma", "x86+sse", "aarch64+neon"))]
pub(crate) fn decompress_12be_unpacked_left_aligned(buf: &[u8], width: usize, height: usize, dummy: bool) -> std::result::Result<PixU16, String> {
  let need = height * width * 2;
  if buf.len() < need {
    return Err(format!("decompress_12be_unpacked_left_aligned(): buffer too short ({} < {})", buf.len(), need));
  }
  unpack_lines_fn(
    width,
    height,
    dummy,
    &(|out: &mut [u16], row| {
      let inb = &buf[(row * width * 2)..];

      for (i, bytes) in (0..width).zip(inb.as_chunks::<2>().0) {
        out[i] = u16::from_be_bytes(*bytes) >> 4;
      }
      Ok(())
    }),
  )
}

#[multiversion(targets("x86_64+avx+avx2+fma", "x86+sse", "aarch64+neon"))]
pub(crate) fn decompress_12le_unpacked_left_aligned(buf: &[u8], width: usize, height: usize, dummy: bool) -> std::result::Result<PixU16, String> {
  let need = height * width * 2;
  if buf.len() < need {
    return Err(format!("decompress_12le_unpacked_left_aligned(): buffer too short ({} < {})", buf.len(), need));
  }
  unpack_lines_fn(
    width,
    height,
    dummy,
    &(|out: &mut [u16], row| {
      let inb = &buf[(row * width * 2)..];

      for (i, bytes) in (0..width).zip(inb.as_chunks::<2>().0) {
        out[i] = u16::from_le_bytes(*bytes) >> 4;
      }
      Ok(())
    }),
  )
}

#[multiversion(targets("x86_64+avx+avx2+fma", "x86+sse", "aarch64+neon"))]
pub(crate) fn decompress_14le_unpacked(buf: &[u8], width: usize, height: usize, dummy: bool) -> std::result::Result<PixU16, String> {
  let need = height * width * 2;
  if buf.len() < need {
    return Err(format!("decompress_14le_unpacked(): buffer too short ({} < {})", buf.len(), need));
  }
  unpack_lines_fn(
    width,
    height,
    dummy,
    &(|out: &mut [u16], row| {
      let inb = &buf[(row * width * 2)..];

      for (i, bytes) in (0..width).zip(inb.as_chunks::<2>().0) {
        out[i] = u16::from_le_bytes(*bytes) & 0x3fff;
      }
      Ok(())
    }),
  )
}

#[multiversion(targets("x86_64+avx+avx2+fma", "x86+sse", "aarch64+neon"))]
pub(crate) fn decompress_14le_unpacked_padded(buf: &[u8], width: usize, height: usize, stripsize: usize, dummy: bool) -> std::result::Result<PixU16, String> {
  let need = height * stripsize;
  if buf.len() < need {
    return Err(format!("decompress_14le_unpacked_padded(): buffer too short ({} < {})", buf.len(), need));
  }
  unpack_lines_fn(
    width,
    height,
    dummy,
    &(|out: &mut [u16], row| {
      let inb = &buf[(row * stripsize)..];

      for (i, bytes) in (0..width).zip(inb.as_chunks::<2>().0) {
        out[i] = u16::from_be_bytes(*bytes) & 0x3fff;
      }
      Ok(())
    }),
  )
}

#[multiversion(targets("x86_64+avx+avx2+fma", "x86+sse", "aarch64+neon"))]
pub(crate) fn decompress_14be_unpacked(buf: &[u8], width: usize, height: usize, dummy: bool) -> std::result::Result<PixU16, String> {
  let need = width * height * 2;
  if buf.len() < need {
    return Err(format!("decompress_14be_unpacked(): buffer too short ({} < {})", buf.len(), need));
  }
  unpack_lines_fn(
    width,
    height,
    dummy,
    &(|out: &mut [u16], row| {
      let inb = &buf[(row * width * 2)..];

      for (i, bytes) in (0..width).zip(inb.as_chunks::<2>().0) {
        out[i] = u16::from_be_bytes(*bytes) & 0x3fff;
      }
      Ok(())
    }),
  )
}

#[multiversion(targets("x86_64+avx+avx2+fma", "x86+sse", "aarch64+neon"))]
pub(crate) fn decompress_16le(buf: &[u8], width: usize, height: usize, dummy: bool) -> std::result::Result<PixU16, String> {
  let need = width * height * 2;
  if buf.len() < need {
    return Err(format!("decompress_16le(): buffer too short ({} < {})", buf.len(), need));
  }
  decompress_lines(buf, width, height, dummy, PackedDecompressor::new(16, Endian::Little))
}

#[multiversion(targets("x86_64+avx+avx2+fma", "x86+sse", "aarch64+neon"))]
pub(crate) fn decompress_16le_skiplines(buf: &[u8], width: usize, height: usize, dummy: bool) -> std::result::Result<PixU16, String> {
  let need = width * height * 4;
  if buf.len() < need {
    return Err(format!("decompress_16le_skiplines(): buffer too short ({} < {})", buf.len(), need));
  }
  unpack_lines_fn(
    width,
    height,
    dummy,
    &(|out: &mut [u16], row| {
      let inb = &buf[(row * width * 4)..];

      for (i, bytes) in (0..width).zip(inb.as_chunks::<2>().0) {
        out[i] = u16::from_le_bytes(*bytes);
      }
      Ok(())
    }),
  )
}

#[multiversion(targets("x86_64+avx+avx2+fma", "x86+sse", "aarch64+neon"))]
pub(crate) fn decompress_16be(buf: &[u8], width: usize, height: usize, dummy: bool) -> std::result::Result<PixU16, String> {
  let need = width * height * 2;
  if buf.len() < need {
    return Err(format!("decompress_16be(): buffer too short ({} < {})", buf.len(), need));
  }
  decompress_lines(buf, width, height, dummy, PackedDecompressor::new(16, Endian::Big))
}

#[multiversion(targets("x86_64+avx+avx2+fma", "x86+sse", "aarch64+neon"))]
pub(crate) fn decompress_generic_msb(buf: &[u8], width: usize, height: usize, bits: u32, dummy: bool) -> std::result::Result<PixU16, String> {
  assert!(bits <= 16);
  let need = (width * height * bits as usize).div_ceil(8);
  if buf.len() < need {
    return Err(format!("decompress_generic_msb(): buffer too short ({} < {})", buf.len(), need));
  }
  let mut pix: PixU16 = alloc_image_ok!(width, height, dummy);
  let mut pump = BitPumpMSB::new(buf);
  for p in pix.pixels_mut() {
    *p = pump.get_bits(bits) as u16;
  }
  Ok(pix)
}

#[multiversion(targets("x86_64+avx+avx2+fma", "x86+sse", "aarch64+neon"))]
pub(crate) fn decompress_generic_lsb(buf: &[u8], width: usize, height: usize, bits: u32, dummy: bool) -> std::result::Result<PixU16, String> {
  assert!(bits <= 16);
  let need = (width * height * bits as usize).div_ceil(8);
  if buf.len() < need {
    return Err(format!("decompress_generic_lsb(): buffer too short ({} < {})", buf.len(), need));
  }
  let mut pix: PixU16 = alloc_image_ok!(width, height, dummy);
  let mut pump = BitPumpLSB::new(buf);
  for p in pix.pixels_mut() {
    *p = pump.get_bits(bits) as u16;
  }
  Ok(pix)
}
