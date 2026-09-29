use self::{mdat::Tile, rice::RiceDecoder};
use crate::formats::bmff::ext_cr3::cmp1::Cmp1Box;
use bitstream_io::BitReader;
use log::debug;
use std::io::Cursor;
use thiserror::Error;

mod decoder;
mod idwt;
mod iquant;
mod mdat;
mod rice;
mod runlength;

#[rustfmt::skip]
const EX_COEF_NUM_TBL:[usize; 0x30*3] = [

    1, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 1, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0,
    1, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0, 1, 1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 0,

    1, 1, 1, 1, 0, 0, 1, 0, 1, 0, 0, 0, 1, 2, 2, 1, 0, 0, 1, 1, 1, 1, 0, 0,
    1, 1, 1, 1, 0, 0, 1, 0, 1, 0, 0, 0, 1, 2, 2, 1, 0, 0, 1, 1, 1, 1, 0, 0,

    1, 1, 1, 1, 1, 1, 1, 0, 1, 0, 1, 0, 1, 2, 2, 2, 2, 1, 1, 1, 1, 2, 2, 1,
    1, 1, 1, 2, 2, 1, 1, 0, 1, 1, 1, 1, 1, 2, 2, 1, 1, 1, 1, 1, 1, 1, 1, 1];

type BitPump<'a> = BitReader<Cursor<&'a [u8]>, bitstream_io::BigEndian>;

#[derive(Debug, Error)]
pub enum CrxError {

  #[error("Overflow error: {}", _0)]
  Overflow(String),

  #[error("General error: {}", _0)]
  General(String),

  #[error("Unsupported format: {}", _0)]
  Unsupp(String),

  #[error("I/O error")]
  Io(#[from] std::io::Error),
}

type Result<T> = std::result::Result<T, CrxError>;

#[derive(Default, Debug, Clone, Copy)]
pub struct CodecParams {
  #[allow(dead_code)]
  sample_precision: u8,
  image_width: usize,
  image_height: usize,
  plane_count: u8,

  #[allow(dead_code)]
  subband_count: u8,
  levels: usize,

  #[allow(dead_code)]
  n_bits: u8,

  median_bits: u8,
  enc_type: u8,
  tile_cols: usize,
  tile_rows: usize,
  tile_width: usize,
  tile_height: usize,
  mdat_hdr_size: u32,
  version: u16,
}

impl CodecParams {
  #[inline(always)]
  fn get_header<'a>(&self, mdat: &'a [u8]) -> &'a [u8] {
    &mdat[..self.mdat_hdr_size as usize]
  }

  #[inline(always)]
  fn get_data<'a>(&self, mdat: &'a [u8]) -> &'a [u8] {
    &mdat[self.mdat_hdr_size as usize..]
  }

  fn resolution(&self) -> usize {
    self.image_width * self.image_height
  }

  pub fn new(cmp1: &Cmp1Box) -> Result<Self> {
    const INCR_BIT_TABLE: [u8; 16] = [0, 0, 0, 0, 0, 0, 0, 0, 0, 1, 1, 0, 0, 0, 1, 0];

    if cmp1.n_planes != 4 {
      return Err(CrxError::General(format!("Plane configration {} is not supported", cmp1.n_planes)));
    }

    let tile_cols: usize = cmp1.f_width.div_ceil(cmp1.tile_width) as usize;
    let tile_rows: usize = cmp1.f_height.div_ceil(cmp1.tile_height) as usize;

    assert!(tile_cols > 0);
    assert!(tile_rows > 0);

    let params = Self {
      sample_precision: cmp1.n_bits as u8 + INCR_BIT_TABLE[4 * cmp1.enc_type as usize + 2] + 1,
      image_width: cmp1.f_width as usize,
      image_height: cmp1.f_height as usize,
      plane_count: cmp1.n_planes as u8,

      subband_count: 3 * cmp1.image_levels as u8 + 1,
      levels: cmp1.image_levels as usize,
      n_bits: cmp1.n_bits,
      median_bits: cmp1.median_bits,
      enc_type: cmp1.enc_type as u8,
      tile_cols,
      tile_rows,
      tile_width: cmp1.tile_width as usize,
      tile_height: cmp1.tile_height as usize,
      mdat_hdr_size: cmp1.mdat_hdr_size,
      version: cmp1.version,
    };

    if params.tile_cols > 0xff {
      return Err(CrxError::General(format!("Tile column count {} is not supported", tile_cols)));
    }
    if params.tile_rows > 0xff {
      return Err(CrxError::General(format!("Tile row count {} is not supported", tile_rows)));
    }

    Ok(params)
  }

  pub(super) fn process_tiles(&mut self, tiles: &mut Vec<Tile>) {
    let tile_count = tiles.len();

    for cur_tile in tiles.iter_mut() {
      if (cur_tile.id + 1) % self.tile_cols != 0 {

        cur_tile.tile_width = self.tile_width;
        cur_tile.plane_width = cur_tile.tile_width >> if self.plane_count == 4 { 1 } else { 0 };
        if self.tile_cols > 1 {
          cur_tile.tiles_right = true;
          if cur_tile.id % self.tile_cols != 0 {

            cur_tile.tiles_left = true;
          }
        }
      } else {

        cur_tile.tile_width = self.image_width - self.tile_width * (self.tile_cols - 1);
        cur_tile.plane_width = cur_tile.tile_width >> if self.plane_count == 4 { 1 } else { 0 };
        if self.tile_cols > 1 {
          cur_tile.tiles_left = true;
        }
      }
      if (cur_tile.id) < (tile_count - self.tile_cols) {

        cur_tile.tile_height = self.tile_height;
        cur_tile.plane_height = cur_tile.tile_height >> if self.plane_count == 4 { 1 } else { 0 };
        if self.tile_rows > 1 {
          cur_tile.tiles_bottom = true;
          if cur_tile.id >= self.tile_cols {
            cur_tile.tiles_top = true;
          }
        }
      } else {

        cur_tile.tile_height = self.image_height - self.tile_height * (self.tile_rows - 1);
        cur_tile.plane_height = cur_tile.tile_height >> if self.plane_count == 4 { 1 } else { 0 };
        if self.tile_rows > 1 {
          cur_tile.tiles_top = true;
        }
      }
    }

    for tile in tiles {
      debug!("{}", tile.descriptor_line());
      debug!("tile width: {}, tile height: {}", tile.tile_width, tile.tile_height);
      let mut plane_sizes = 0;
      self.process_subbands(tile);
      for plane in &mut tile.planes {
        debug!("{}", plane.descriptor_line());
        let mut band_sizes = 0;

        for band in &mut plane.subbands {
          debug!("{}", band.descriptor_line());
          assert!(band.subband_size != 0);
          assert_eq!(band.subband_size % 8, 0);
          band_sizes += band.subband_size;
        }
        assert_eq!(plane.plane_size, band_sizes);
        plane_sizes += plane.plane_size;
      }

      assert_eq!(tile.tile_size - tile.extra_size(), plane_sizes);
    }
  }

  pub(super) fn process_subbands(&self, tile: &mut Tile) {
    for plane in &mut tile.planes {
      let mut band_w = tile.plane_width;
      let mut band_h = tile.plane_height;
      let mut band_width_ex_coef = 0;
      let mut band_height_ex_coef = 0;
      if self.levels > 0 {
        let row_ex_coef = &EX_COEF_NUM_TBL[0x30 * (self.levels - 1) + 6 * (tile.plane_width & 7)..];
        let col_ex_coef = &EX_COEF_NUM_TBL[0x30 * (self.levels - 1) + 6 * (tile.plane_height & 7)..];

        for lev in 0..self.levels {
          let w_odd_pixel = band_w & 1;
          let h_odd_pixel = band_h & 1;

          band_w = (w_odd_pixel + band_w) >> 1;
          band_h = (h_odd_pixel + band_h) >> 1;

          let mut w_ex_coef0 = 0;
          let mut w_ex_coef1 = 0;
          let mut h_ex_coef0 = 0;
          let mut h_ex_coef1 = 0;
          let mut col_start = 0;
          let mut row_start = 0;
          if tile.tiles_right {
            w_ex_coef0 = row_ex_coef[2 * lev];
            w_ex_coef1 = row_ex_coef[2 * lev + 1];
          }
          if tile.tiles_left {
            w_ex_coef0 += 1;
            col_start = 1;
          }
          if tile.tiles_bottom {
            h_ex_coef0 = col_ex_coef[2 * lev];
            h_ex_coef1 = col_ex_coef[2 * lev + 1];
          }
          if tile.tiles_top {
            h_ex_coef0 += 1;
            row_start = 1;
          }

          let i = (self.levels - lev) * 3;
          plane.subbands[i - 0].width = band_w + w_ex_coef0 - w_odd_pixel;
          plane.subbands[i - 0].height = band_h + h_ex_coef0 - h_odd_pixel;
          plane.subbands[i - 0].setup_idx(self.version, lev + 1, col_start, w_ex_coef0 - col_start, row_start, h_ex_coef0 - row_start);

          plane.subbands[i - 1].width = band_w + w_ex_coef1;
          plane.subbands[i - 1].height = band_h + h_ex_coef0 - h_odd_pixel;
          plane.subbands[i - 1].setup_idx(self.version, lev + 1, 0, w_ex_coef1, row_start, h_ex_coef0 - row_start);

          plane.subbands[i - 2].width = band_w + w_ex_coef0 - w_odd_pixel;
          plane.subbands[i - 2].height = band_h + h_ex_coef1;
          plane.subbands[i - 2].setup_idx(self.version, lev + 1, col_start, w_ex_coef0 - col_start, 0, h_ex_coef1);
        }
        band_width_ex_coef = 0;
        band_height_ex_coef = 0;
        if tile.tiles_right {
          band_width_ex_coef = row_ex_coef[2 * self.levels - 1];
        }
        if tile.tiles_bottom {
          band_height_ex_coef = col_ex_coef[2 * self.levels - 1];
        }
      }

      plane.subbands[0].width = band_width_ex_coef + band_w;
      plane.subbands[0].height = band_height_ex_coef + band_h;
      if self.levels > 0 {
        plane.subbands[0].setup_idx(self.version, self.levels, 0, band_width_ex_coef, 0, band_height_ex_coef);
      }
    }
  }
}

struct BandParam<'mdat> {

  subband_width: usize,

  subband_height: usize,

  rounded_bits_mask: i32,

  #[allow(dead_code)]
  rounded_bits: i32,

  cur_line: usize,

  line_buf: [Vec<i32>; 2],

  line_k: Vec<u32>,

  line_pos: usize,

  #[allow(dead_code)]
  line_len: usize,

  s_param: u32,

  pub q_param: u32,

  supports_partial: bool,

  rice: RiceDecoder<'mdat>,
}

impl<'mdat> BandParam<'mdat> {

  fn coeff_a(&self) -> i32 {
    self.line_buf[1][self.line_pos - 1]
  }

  fn coeff_b(&self) -> i32 {
    self.line_buf[0][self.line_pos]
  }

  fn coeff_c(&self) -> i32 {
    self.line_buf[0][self.line_pos - 1]
  }

  fn coeff_d(&self) -> i32 {
    self.line_buf[0][self.line_pos + 1]
  }

  fn decoded_buf(&self) -> &[i32] {

    &self.line_buf[1][1..1 + self.subband_width]
  }

  fn decoded_buf_mut(&mut self) -> &mut [i32] {

    &mut self.line_buf[1][1..1 + self.subband_width]
  }
}

pub fn decompress_crx_image(buf: &[u8], cmp1: &Cmp1Box) -> Result<Vec<u16>> {
  let image = CodecParams::new(cmp1)?;
  debug!("CRX codec parameter: {:?}", image);
  image.decode(buf)
}
