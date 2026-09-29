use crate::{
  alloc_image_typed_ok,
  pixarray::{LineMut, Pix2D, SubPixel},
};

use rayon::prelude::*;

pub mod arw6;
pub mod crx;
pub mod deflate;
pub mod jpeg;
pub mod jpegxl;
pub mod ljpeg;
pub mod packed;
pub mod radc;

pub trait LineIteratorMut<'a, T>: Iterator<Item = &'a mut [T]> + ExactSizeIterator
where
  T: SubPixel + 'a,
{
}

impl<'a, T, I> LineIteratorMut<'a, T> for I
where
  I: ExactSizeIterator<Item = &'a mut [T]>,
  T: SubPixel + 'a,
{
}

#[allow(unused)]
pub(crate) trait LineIterator<'a, T>: Iterator<Item = &'a [T]> + ExactSizeIterator
where
  T: SubPixel + 'a,
{
}

pub trait Decompressor<'a, T>: Send + Sync
where
  T: SubPixel + 'a,
{

  fn decompress(&self, src: &[u8], skip_rows: usize, lines: impl LineIteratorMut<'a, T>, line_width: usize) -> std::result::Result<(), String>;

  fn can_skip_rows(&self) -> bool;
}

#[inline(always)]
pub fn decompress_lines_fn<T, F>(width: usize, height: usize, dummy: bool, closure: &F) -> std::result::Result<Pix2D<T>, String>
where
  F: Fn(&mut [T], usize) -> std::result::Result<(), String> + Sync,
  T: SubPixel,
{
  let mut out: Pix2D<T> = alloc_image_typed_ok!(T, width, height, dummy);
  out
    .pixels_mut()
    .par_chunks_mut(width)
    .enumerate()
    .try_for_each(|(row, line)| closure(line, row))?;
  Ok(out)
}

#[inline(always)]
pub fn decompress_lines<T, D>(src: &[u8], width: usize, height: usize, dummy: bool, decompressor: D) -> std::result::Result<Pix2D<T>, String>
where
  D: for<'a> Decompressor<'a, T>,
  T: SubPixel,
{
  let mut out: Pix2D<T> = alloc_image_typed_ok!(T, width, height, dummy);

  let rows = unpack_run(height);
  out
    .pixels_mut()
    .par_chunks_mut(width * rows)
    .enumerate()
    .try_for_each(|(run, lines)| decompressor.decompress(src, run * rows, lines.chunks_exact_mut(width), width))?;
  Ok(out)
}

pub fn unpack_lines_fn<T, F>(width: usize, height: usize, dummy: bool, closure: &F) -> std::result::Result<Pix2D<T>, String>
where
  F: Fn(&mut [T], usize) -> std::result::Result<(), String> + Sync,
  T: SubPixel,
{
  let mut out: Pix2D<T> = alloc_image_typed_ok!(T, width, height, dummy);
  let rows = unpack_run(height);
  out.pixels_mut().par_chunks_mut(width * rows).enumerate().try_for_each(|(run, lines)| {
    lines.chunks_exact_mut(width).enumerate().try_for_each(|(row, line)| closure(line, run * rows + row))
  })?;
  Ok(out)
}

fn unpack_run(height: usize) -> usize {
  height.div_ceil(2).max(1)
}

#[inline(always)]
pub fn decompress_strips_fn<T, F>(width: usize, height: usize, stripsize: usize, dummy: bool, closure: &F) -> std::result::Result<Pix2D<T>, String>
where
  F: Fn(&mut [T], usize, usize) -> std::result::Result<(), String> + Sync,
  T: SubPixel,
{
  let mut out: Pix2D<T> = alloc_image_typed_ok!(T, width, height, dummy);
  out
    .pixels_mut()
    .par_chunks_mut(width * stripsize)
    .enumerate()
    .try_for_each(|(strip, lines)| closure(lines, strip, strip * stripsize))?;
  Ok(out)
}

#[inline(always)]
pub fn decompress_strips<T, D>(src: &[u8], width: usize, height: usize, dummy: bool, stripsize: usize, decompressor: D) -> std::result::Result<Pix2D<T>, String>
where
  D: for<'a> Decompressor<'a, T>,
  T: SubPixel,
{
  let mut out: Pix2D<T> = alloc_image_typed_ok!(T, width, height, dummy);
  out
    .pixels_mut()
    .par_chunks_mut(width * stripsize)
    .enumerate()
    .try_for_each(|(strip, lines)| decompressor.decompress(src, strip * stripsize, lines.chunks_exact_mut(width), width))?;
  Ok(out)
}

#[inline(always)]
pub fn decompress_chunked_fn<T, F>(width: usize, height: usize, chunksize: usize, dummy: bool, closure: &F) -> std::result::Result<Pix2D<T>, String>
where
  F: Fn(&mut [T], usize) + Sync,
  T: SubPixel,
{
  let mut out: Pix2D<T> = alloc_image_typed_ok!(T, width, height, dummy);
  out.pixels_mut().par_chunks_mut(chunksize).enumerate().for_each(|(chunk_id, chunk)| {
    closure(chunk, chunk_id);
  });
  Ok(out)
}

pub struct FnLineDecompressor<T>(fn(&[u8], usize, LineMut<T>, usize) -> std::result::Result<(), String>);

impl<'a, T> Decompressor<'a, T> for FnLineDecompressor<T>
where
  T: SubPixel + 'a,
{
  fn decompress(&self, src: &[u8], skip_rows: usize, lines: impl LineIteratorMut<'a, T>, line_width: usize) -> std::result::Result<(), String> {
    for (row, line) in lines.enumerate() {
      (self.0)(src, skip_rows + row, line, line_width)?;
    }
    Ok(())
  }

  fn can_skip_rows(&self) -> bool {
    true
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  const WIDTH: usize = 1024;

  fn fill_row_col(src: &[u8], row: usize, line: &mut [u16], _line_width: usize) -> std::result::Result<(), String> {
    let _ = src;
    for (col, pixel) in line.iter_mut().enumerate() {
      *pixel = (row as u16).wrapping_add(col as u16);
    }
    Ok(())
  }

  fn always_err(_src: &[u8], _row: usize, _line: &mut [u16], _line_width: usize) -> std::result::Result<(), String> {
    Err("intentional error".into())
  }

  fn record_line_width(_src: &[u8], _row: usize, line: &mut [u16], line_width: usize) -> std::result::Result<(), String> {

    if !line.is_empty() {
      line[0] = line_width as u16;
    }
    Ok(())
  }

  #[test]
  fn test_single_line_pixels_filled() {
    let src = vec![0u8; 1];
    let mut line = vec![0u16; WIDTH];
    let decomp = FnLineDecompressor::<u16>(fill_row_col);
    decomp.decompress(&src, 0, std::iter::once(line.as_mut_slice()), WIDTH).unwrap();
    for (col, &pixel) in line.iter().enumerate() {
      assert_eq!(pixel, col as u16, "col {col}");
    }
  }

  #[test]
  fn test_skip_rows_forwarded() {
    let src = vec![0u8; 1];
    let mut line = vec![0u16; WIDTH];
    let decomp = FnLineDecompressor::<u16>(fill_row_col);

    decomp.decompress(&src, 7, std::iter::once(line.as_mut_slice()), WIDTH).unwrap();
    for (col, &pixel) in line.iter().enumerate() {
      assert_eq!(pixel, 7u16.wrapping_add(col as u16), "col {col}");
    }
  }

  #[test]
  fn test_multiple_lines_row_indices() {
    let src = vec![0u8; 1];
    let mut buf = vec![0u16; WIDTH * 4];
    let decomp = FnLineDecompressor::<u16>(fill_row_col);

    decomp.decompress(&src, 2, buf.chunks_exact_mut(WIDTH), WIDTH).unwrap();
    for row in 0..4usize {
      for col in 0..WIDTH {
        let expected = (2 + row as u16).wrapping_add(col as u16);
        assert_eq!(buf[row * WIDTH + col], expected, "row {row} col {col}");
      }
    }
  }

  #[test]
  fn test_line_width_forwarded() {
    let src = vec![0u8; 1];
    let mut line = vec![0u16; WIDTH];
    let decomp = FnLineDecompressor::<u16>(record_line_width);
    decomp.decompress(&src, 0, std::iter::once(line.as_mut_slice()), WIDTH).unwrap();
    assert_eq!(line[0], WIDTH as u16);
  }

  #[test]
  fn test_error_propagates() {
    let src = vec![0u8; 1];
    let mut line = vec![0u16; WIDTH];
    let decomp = FnLineDecompressor::<u16>(always_err);
    let result = decomp.decompress(&src, 0, std::iter::once(line.as_mut_slice()), WIDTH);
    assert_eq!(result.unwrap_err(), "intentional error");
  }

  #[test]
  fn test_error_stops_after_first_failing_line() {
    let src = vec![0u8; 1];
    let mut buf = vec![0u16; WIDTH * 3];
    let decomp = FnLineDecompressor::<u16>(always_err);

    let result = decomp.decompress(&src, 0, buf.chunks_exact_mut(WIDTH), WIDTH);
    assert!(result.is_err());
  }

  #[test]
  fn test_decompress_lines_pixel_values() {
    let src = vec![0u8; 1];
    let height = 4usize;
    let result = decompress_lines::<u16, _>(&src, WIDTH, height, false, FnLineDecompressor(fill_row_col)).unwrap();
    let pixels = result.pixels();
    for row in 0..height {
      for col in 0..WIDTH {
        let expected = (row as u16).wrapping_add(col as u16);
        assert_eq!(pixels[row * WIDTH + col], expected, "row {row} col {col}");
      }
    }
  }

  #[test]
  fn test_decompress_lines_output_dimensions() {
    let src = vec![0u8; 1];
    let height = 8usize;
    let result = decompress_lines::<u16, _>(&src, WIDTH, height, false, FnLineDecompressor(fill_row_col)).unwrap();
    assert_eq!(result.pixels().len(), WIDTH * height);
  }
}
