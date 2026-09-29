use std::{iter, ops::Range};

use crate::pixarray::{LineMut, SubPixel};

#[derive(Debug)]
pub struct ErrorNotTileable;

pub struct ImageTiler<'a, T> {
  data: &'a [T],
  width: usize,
  #[allow(dead_code)]
  height: usize,
  cpp: usize,
  tiles: Range<usize>,
  tw: usize,
  th: usize,
  tcols: usize,
  trows: usize,
}

impl<'a, T> ImageTiler<'a, T> {
  pub fn new(data: &'a [T], width: usize, height: usize, cpp: usize, tw: usize, th: usize) -> Self {
    assert!(data.len() >= height * width * cpp);
    let tcols = width.div_ceil(tw);
    let trows = height.div_ceil(th);
    Self {
      data,
      width,
      height,
      cpp,
      tiles: Range { start: 0, end: trows * tcols },
      tw,
      th,
      tcols,
      trows,
    }
  }

  pub fn tile_cols(&self) -> usize {
    self.tcols
  }

  pub fn tile_rows(&self) -> usize {
    self.trows
  }

  pub fn tile_count(&self) -> usize {
    self.tile_rows() * self.tile_cols()
  }

  fn needs_padding(&self) -> bool {
    self.width % self.tw > 0
  }
}

impl<'a, T> ImageTiler<'a, T>
where
  T: Copy + Default,
{

  pub fn build_tile(&self, idx: usize) -> Vec<T> {
    let mut buf = Vec::with_capacity(self.th * self.tw * self.cpp);

    let tile_row = idx / self.tcols;
    let tile_col = idx % self.tcols;

    for row in 0..self.th {
      let off_row = (tile_row * self.th) + row;
      let offset = off_row * self.width * self.cpp + (tile_col * self.tw * self.cpp);

      if offset < self.data.len() {
        if tile_col < self.tcols - 1 || !self.needs_padding() {
          let sub = &self.data[offset..offset + self.tw * self.cpp];
          buf.extend_from_slice(sub);
        } else {
          buf.extend_from_slice(&self.data[offset..offset + (self.width % self.tw) * self.cpp]);
          let last_pix = buf.last().copied().unwrap_or_default();
          buf.extend(iter::repeat(last_pix).take((self.tw - (self.width % self.tw)) * self.cpp));
        };
      } else {
        buf.extend_from_within((row - 1) * self.tw * self.cpp..((row - 1) * self.tw * self.cpp) + self.tw * self.cpp);
      }
    }
    buf
  }
}

impl<'a, T> Iterator for ImageTiler<'a, T>
where
  T: Copy + Default,
{
  type Item = Vec<T>;

  fn next(&mut self) -> Option<Self::Item> {
    let i = self.tiles.next()?;
    Some(self.build_tile(i))
  }

  fn size_hint(&self) -> (usize, Option<usize>) {
    (self.tile_count(), Some(self.tile_count()))
  }
}

pub trait TilesMut<'a, T>
where
  T: SubPixel,
{
  fn into_tiles_iter_mut(self, width: usize, cpp: usize, tile_width: usize, tile_height: usize) -> std::result::Result<IntoTilesIter<'a, T>, ErrorNotTileable>;
}

impl<'a, T> TilesMut<'a, T> for &'a mut [T]
where
  T: SubPixel,
{
  fn into_tiles_iter_mut(self, width: usize, cpp: usize, tile_width: usize, tile_height: usize) -> std::result::Result<IntoTilesIter<'a, T>, ErrorNotTileable> {
    assert!(width > 0, "Width and height must be greater than zero");
    assert!(tile_width * tile_height > 0, "Tile width and height must be greater than zero");
    assert!(cpp > 0, "cpp must be greater than zero");
    if !self.len().is_multiple_of(tile_width * cpp * tile_height) {
      return Err(ErrorNotTileable);
    }
    Ok(IntoTilesIter {
      count: 0,
      width,
      cpp,
      tile_width,
      tile_height,
      original: self,
    })
  }
}

pub struct IntoTilesIter<'a, T> {
  count: usize,
  width: usize,
  cpp: usize,
  tile_width: usize,
  tile_height: usize,
  original: &'a mut [T],
}

impl<'a, T> IntoTilesIter<'a, T> {

  fn tile_count(&self) -> usize {
    self.original.len() / self.cpp / (self.tile_height * self.tile_width)
  }
}

impl<'a, T> ExactSizeIterator for IntoTilesIter<'a, T> where T: Send {}

impl<'a, T> Iterator for IntoTilesIter<'a, T> {
  type Item = Tile<'a, T>;

  fn next(&mut self) -> Option<Self::Item> {
    let tile_cols = self.width / self.tile_width;
    let _tile_rows = self.original.len() / self.cpp / (self.tile_height * self.width);

    let tile_x = self.count % tile_cols;
    let tile_y = self.count / tile_cols;

    assert!(self.count <= self.tile_count());

    let start_index = tile_x * (self.tile_width * self.cpp) + tile_y * (self.width * self.cpp) * self.tile_height;
    self.count += 1;

    if start_index >= self.original.len() {
      return None;
    } else {

      let next_line_distance = self.width * self.cpp;
      let first_line_begin = &mut self.original[start_index..];
      if first_line_begin.len() < self.tile_height * next_line_distance - (tile_x * self.tile_width * self.cpp) {

        panic!("Tile buffer too small.")
      }
      let first_line = &mut first_line_begin[..self.tile_width * self.cpp];

      Some(Tile {
        first_line,
        tile_height: self.tile_height,
        width: self.width,
        cpp: self.cpp,
        _phantom: std::marker::PhantomData,
      })
    }
  }

  fn size_hint(&self) -> (usize, Option<usize>) {
    (self.tile_count() - self.count, Some(self.tile_count() - self.count))
  }
}

pub struct Tile<'a, T> {

  first_line: *mut [T],
  tile_height: usize,
  width: usize,
  cpp: usize,
  _phantom: std::marker::PhantomData<&'a [T]>,
}

impl<'a, T> Tile<'a, T> {
  pub fn into_iter_mut(self) -> TileIterMut<'a, T> {
    TileIterMut { tile: self, current_line: 0 }
  }
}

unsafe impl<T: Send> Send for Tile<'_, T> {}

pub struct TileIterMut<'a, T> {
  tile: Tile<'a, T>,
  current_line: usize,
}

impl<'a, T> Iterator for TileIterMut<'a, T>
where
  T: SubPixel + 'a,
{
  type Item = LineMut<'a, T>;

  fn next(&mut self) -> Option<Self::Item> {
    if self.current_line >= self.tile.tile_height {
      return None;
    }

    let next_line_distance = self.tile.width * self.tile.cpp;
    let line_ptr = unsafe { (self.tile.first_line as *mut T).offset((self.current_line * next_line_distance) as isize) };
    self.current_line += 1;

    Some(unsafe { std::slice::from_raw_parts_mut(line_ptr, self.tile.first_line.len()) })
  }

  fn size_hint(&self) -> (usize, Option<usize>) {
    (self.tile.tile_height - self.current_line, Some(self.tile.tile_height - self.current_line))
  }
}

impl<'a, T> ExactSizeIterator for TileIterMut<'a, T> where T: SubPixel + 'a {}

#[cfg(test)]
mod tests {
  use super::*;

  use rayon::prelude::*;

  #[test]
  fn tile_1x1() -> std::result::Result<(), Box<dyn std::error::Error>> {
    crate::init_test_logger();
    let w = 1;
    let h = 1;
    let c = 3;
    let buf = vec![0_u16; w * h * c];

    let tiles: Vec<Vec<u16>> = ImageTiler::new(&buf, w, h, c, 20, 20).collect();

    assert_eq!(tiles.len(), 1);
    assert_eq!(tiles[0].len(), c * 20 * 20);

    Ok(())
  }

  #[test]
  fn test_par_bridge() {
    #[rustfmt::skip]
    let mut vec = vec![
      1,  2,    3,  4,   5,  6,
      7,  8,    9, 10,  11, 12,

      13, 14,  15, 16,  17, 18,
      19, 20,  21, 22,  23, 24,
    ];
    let cpp = 1;
    let tiles = vec.into_tiles_iter_mut(6, cpp, 2, 2);
    assert!(tiles.is_ok());
    let tiles = tiles.expect("Tiling failed");

    tiles.par_bridge().for_each(|tile| {
      tile.into_iter_mut().for_each(|line| {
        for p in line {
          *p *= 2;
        }
      });
    });
  }

  #[test]
  fn test_par_bridge_without_collect() {
    #[rustfmt::skip]
    let mut vec = vec![
      1,  2,    3,  4,   5,  6,
      7,  8,    9, 10,  11, 12,

      13, 14,  15, 16,  17, 18,
      19, 20,  21, 22,  23, 24,
    ];
    let expected_vec: Vec<u16> = vec.iter().map(|p| p * 2).collect();
    let cpp = 1;
    let tiles = vec.into_tiles_iter_mut(6, cpp, 2, 2);
    assert!(tiles.is_ok());

    let tiles = tiles.expect("Tiling failed");

    tiles.par_bridge().for_each(|tile| {
      tile.into_iter_mut().for_each(|line| {
        for p in line {
          *p *= 2;
        }
      });
    });

    assert_eq!(vec, expected_vec);
  }

  #[test]
  fn test_tiles_mut() {
    #[rustfmt::skip]
    let mut vec = vec![
      1,  2,    3,  4,   5,  6,
      7,  8,    9, 10,  11, 12,

      13, 14,  15, 16,  17, 18,
      19, 20,  21, 22,  23, 24,
    ];
    let cpp = 1;
    let tiles = vec.into_tiles_iter_mut(6, cpp, 2, 2);
    assert!(tiles.is_ok());
    let tiles = tiles.expect("Tiling failed");

    let tiles: Vec<Tile<u16>> = tiles.collect();

    assert_eq!(tiles.len(), 6);
  }
}
