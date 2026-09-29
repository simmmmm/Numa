use super::{
  BandParam, CodecParams, CrxError, Result,
  mdat::{Plane, Tile},
};
use crate::decompressors::crx::{idwt::WaveletTransform, mdat::parse_header, rice::RiceDecoder};
use bitstream_io::BitReader;
use itertools::izip;
use log::debug;
use rayon::prelude::*;
use std::{convert::TryInto, io::Cursor, time::Instant};

pub(super) const PREDICT_K_MAX: u32 = 15;
pub(super) const PREDICT_K_ESCAPE: u32 = 41;
pub(super) const PREDICT_K_ESCBITS: u32 = 21;

struct PlaneLineIter<'a> {
  tile: &'a Tile,
  plane: &'a Plane,
  codec: CodecParams,
  params: Vec<BandParam<'a>>,
  iwt_transforms: Vec<WaveletTransform>,

  next_row: usize,
}

impl<'a> PlaneLineIter<'a> {

  fn new(codec: CodecParams, tile: &'a Tile, plane: &'a Plane, mdat: &'a [u8]) -> Result<Self> {

    assert!(tile.plane_height > 0);
    assert!(tile.plane_width > 0);

    let data = codec.get_data(mdat);

    let plane_mdat_offset =
      tile.data_offset + tile.qp_data.as_ref().map(|qp| qp.mdat_qp_data_size + qp.mdat_extra_size as u32).unwrap_or(0) as usize + plane.data_offset;

    let mut params = Vec::with_capacity(plane.subbands.len());
    for (band_id, band) in plane.subbands.iter().enumerate() {
      let band_mdat_offset = plane_mdat_offset + band.data_offset;
      debug!("Band {} has MDAT offset: {}", band_id, band_mdat_offset);
      let band_buf = &data[band_mdat_offset..band_mdat_offset + band.data_size];

      let line_len = 1 + band.width + 1;
      let bitpump = BitReader::endian(Cursor::new(band_buf), bitstream_io::BigEndian);

      let param = BandParam {
        subband_width: band.width,
        subband_height: band.height,
        rounded_bits_mask: if plane.support_partial && band_id == 0 { plane.rounded_bits_mask } else { 0 },
        rounded_bits: 0,
        cur_line: 0,
        line_buf: [vec![0; line_len], vec![0; line_len]],
        line_k: vec![0; line_len],
        line_pos: 0,
        line_len,
        s_param: 0,
        q_param: band.q_param,
        supports_partial: plane.support_partial && band_id == 0,
        rice: RiceDecoder::new(bitpump),
      };
      params.push(param);
    }

    let mut iwt_transforms = Vec::with_capacity(codec.levels);

    if codec.levels > 0 {

      for level in 0..codec.levels {
        let band = 3 * level + 1;
        let (height, width) = if level >= codec.levels - 1 {
          (tile.plane_height, tile.plane_width)
        } else {
          (plane.subbands[band + 3].height, plane.subbands[band + 4].width)
        };
        iwt_transforms.push(WaveletTransform::new(height, width));
      }
      codec.idwt_53_filter_init(tile, plane, &mut params, &mut iwt_transforms, codec.levels)?;
    }

    Ok(Self {
      params,
      tile,
      plane,
      codec,
      iwt_transforms,
      next_row: 0,
    })
  }

  fn decode_plane_line(&mut self) -> Result<&[i32]> {
    if self.next_row < self.tile.plane_height {
      self.next_row += 1;
      if self.codec.levels > 0 {
        self
          .codec
          .idwt_53_filter_decode(self.tile, self.plane, &mut self.params, &mut self.iwt_transforms, self.codec.levels - 1)?;
        self
          .codec
          .idwt_53_filter_transform(self.tile, self.plane, &mut self.params, &mut self.iwt_transforms, self.codec.levels - 1)?;
        let line_data = self.iwt_transforms[self.codec.levels - 1].getline();
        debug_assert_eq!(line_data.len(), self.tile.plane_width);
        Ok(line_data)
      } else {
        debug_assert_eq!(self.plane.subbands.len(), 1);
        let param = &mut self.params[0];
        self.codec.decode_line(param)?;
        let line_data = param.decoded_buf();
        debug_assert_eq!(line_data.len(), param.subband_width as usize);
        debug_assert_eq!(line_data.len(), self.tile.plane_width);
        Ok(line_data)
      }
    } else {
      Err(CrxError::General("All rows processed, can't decode more".to_string()))
    }
  }
}

impl<'a> Iterator for PlaneLineIter<'a> {
  type Item = Result<PlaneLine>;

  fn next(&mut self) -> Option<Self::Item> {
    if self.next_row < self.tile.plane_height {
      match self.decode_plane_line() {
        Ok(line) => Some(Ok(line.into())),
        Err(e) => Some(Err(e)),
      }
    } else {
      None
    }
  }

  fn size_hint(&self) -> (usize, Option<usize>) {
    (0, Some(self.tile.plane_height))
  }
}

type PlaneLine = Vec<i32>;

fn decode_full_plane(codec: &CodecParams, tile: &Tile, plane: &Plane, mdat: &[u8]) -> Result<Vec<PlaneLine>> {

  let line_decoder = PlaneLineIter::new(*codec, tile, plane, mdat)?;
  line_decoder.collect()
}

impl CodecParams {

  pub fn decode(mut self, mdat: &[u8]) -> Result<Vec<u16>> {
    let instant = Instant::now();
    debug!("Tile configuration: rows: {}, columns: {}", self.tile_rows, self.tile_cols);

    let mut tiles = parse_header(self.get_header(mdat))?;
    self.process_tiles(&mut tiles);
    for tile in tiles.iter_mut() {
      tile.generate_qstep_table(&self, self.get_data(mdat))?;
    }

    let mut cfa: Vec<u16> = vec![0; self.resolution()];

    let plane_bufs: Result<Vec<Vec<Vec<PlaneLine>>>> = tiles
      .par_iter()
      .map(|tile| tile.planes.par_iter().map(move |plane| decode_full_plane(&self, tile, plane, mdat)).collect())
      .collect();

    match plane_bufs {
      Ok(bufs) => {
        for (tile_id, tile) in bufs.into_iter().enumerate() {
          let plane_count = tile.len();
          debug_assert_eq!(plane_count, 4);

          let planes: [Vec<PlaneLine>; 4] = tile
            .try_into()
            .map_err(|_| CrxError::General(format!("Invalid plane count {} (expected 4) for tile {}", plane_count, tile_id)))?;

          let (p0, p1, p2, p3) = (&planes[0], &planes[1], &planes[2], &planes[3]);

          for (plane_row, (l0, l1, l2, l3)) in izip!(p0, p1, p2, p3).enumerate() {
            let (c0, c1, c2, c3) = convert_plane_line(&self, l0, l1, l2, l3)?;
            integrate_cfa(&self, &tiles, &mut cfa, tile_id, 0, plane_row, &c0)?;
            integrate_cfa(&self, &tiles, &mut cfa, tile_id, 1, plane_row, &c1)?;
            integrate_cfa(&self, &tiles, &mut cfa, tile_id, 2, plane_row, &c2)?;
            integrate_cfa(&self, &tiles, &mut cfa, tile_id, 3, plane_row, &c3)?;
          }
        }
      }
      Err(e) => {
        return Err(e);
      }
    }
    debug!("MDAT decoding and CFA build: {} s", instant.elapsed().as_secs_f32());
    Ok(cfa)
  }

  fn decode_top_line_no_ref_prev_line(&self, p: &mut BandParam) -> Result<()> {
    debug_assert_eq!(p.line_pos, 1);
    let mut remaining = p.subband_width as u32;

    p.line_buf[0][p.line_pos - 1] = 0;
    p.line_buf[1][p.line_pos - 1] = 0;
    while remaining > 1 {

      if p.coeff_a() != 0 {

        let bit_code = p.rice.adaptive_rice_decode(true, PREDICT_K_ESCAPE, PREDICT_K_ESCBITS, PREDICT_K_MAX)?;
        p.line_buf[1][p.line_pos] = error_code_signed(bit_code);
      } else {

        if p.rice.bitstream_get_bits(1)? == 1 {
          let n_syms = self.symbol_run_count(p, remaining)?;

          remaining = remaining.saturating_sub(n_syms);

          for _ in 0..n_syms {

            p.line_buf[1][p.line_pos] = 0;
            p.line_k[p.line_pos - 1] = 0;
            p.line_pos += 1;
          }

          if remaining == 0 {
            break;
          }
        }

        let bit_code = p.rice.adaptive_rice_decode(true, PREDICT_K_ESCAPE, PREDICT_K_ESCBITS, PREDICT_K_MAX)?;
        p.line_buf[1][p.line_pos] = error_code_signed(bit_code + 1);

      }
      p.line_k[p.line_pos - 1] = p.rice.k();
      p.line_pos += 1;
      remaining = remaining.saturating_sub(1);
    }

    if remaining == 1 {
      let bit_code = p.rice.adaptive_rice_decode(true, PREDICT_K_ESCAPE, PREDICT_K_ESCBITS, PREDICT_K_MAX)?;
      p.line_buf[1][p.line_pos] = error_code_signed(bit_code);
      p.line_k[p.line_pos - 1] = p.rice.k();
      p.line_pos += 1;
    }
    debug_assert!(p.line_pos < p.line_buf[1].len());
    p.line_buf[1][p.line_pos] = 0;
    Ok(())
  }

  fn decode_nontop_line_no_ref_prev_line(&self, p: &mut BandParam) -> Result<()> {

    debug_assert_eq!(p.line_pos, 1);
    let mut remaining = p.subband_width as u32;
    while remaining > 1 {

      if (p.coeff_d() | p.coeff_b() | p.coeff_a()) != 0 {
        let bit_code = p.rice.adaptive_rice_decode(true, PREDICT_K_ESCAPE, PREDICT_K_ESCBITS, 0)?;
        p.line_buf[1][p.line_pos] = error_code_signed(bit_code);
        if p.line_k[p.line_pos].saturating_sub(p.rice.k()) <= 1 {
          if p.rice.k() >= 15 {
            p.rice.set_k(15);
          }
        } else {
          p.rice.set_k(p.rice.k() + 1);
        }
      } else {
        if p.rice.bitstream_get_bits(1)? == 1 {
          debug_assert!(remaining != 1);
          let n_syms = self.symbol_run_count(p, remaining)?;

          remaining = remaining.saturating_sub(n_syms);

          for _ in 0..n_syms {

            p.line_buf[1][p.line_pos] = 0;
            p.line_k[p.line_pos - 1] = 0;
            p.line_pos += 1;
          }
        }

        if remaining <= 1 {
          if remaining == 1 {
            let bit_code = p.rice.adaptive_rice_decode(true, PREDICT_K_ESCAPE, PREDICT_K_ESCBITS, PREDICT_K_MAX)?;
            p.line_buf[1][p.line_pos] = error_code_signed(bit_code + 1);
            p.line_k[p.line_pos - 1] = p.rice.k();
            p.line_pos += 1;
            remaining = remaining.saturating_sub(1);
          }
          break;
        } else {
          let bit_code = p.rice.adaptive_rice_decode(true, PREDICT_K_ESCAPE, PREDICT_K_ESCBITS, 0)?;
          p.line_buf[1][p.line_pos] = error_code_signed(bit_code + 1);
          if p.line_k[p.line_pos].saturating_sub(p.rice.k()) <= 1 {
            if p.rice.k() >= 15 {
              p.rice.set_k(15);
            }
          } else {
            p.rice.set_k(p.rice.k() + 1);
          }
        }
      }
      p.line_k[p.line_pos - 1] = p.rice.k();
      p.line_pos += 1;
      remaining = remaining.saturating_sub(1);
    }

    if remaining == 1 {
      let bit_code = p.rice.adaptive_rice_decode(true, PREDICT_K_ESCAPE, PREDICT_K_ESCBITS, PREDICT_K_MAX)?;
      p.line_buf[1][p.line_pos] = error_code_signed(bit_code);
      p.line_k[p.line_pos - 1] = p.rice.k();
      p.line_pos += 1;
    }
    debug_assert!(p.line_pos < p.line_buf[1].len());
    Ok(())
  }

  fn decode_top_line(&self, p: &mut BandParam) -> Result<()> {
    debug_assert_eq!(p.line_pos, 1);
    let mut remaining = p.subband_width as u32;

    p.line_buf[1][p.line_pos - 1] = 0;
    while remaining > 1 {

      if p.coeff_a() != 0 {
        p.line_buf[1][p.line_pos] = p.coeff_a();
      } else {
        if p.rice.bitstream_get_bits(1)? == 1 {
          let n_syms = self.symbol_run_count(p, remaining)?;
          remaining = remaining.saturating_sub(n_syms);

          for _ in 0..n_syms {
            p.line_buf[1][p.line_pos] = p.coeff_a();
            p.line_pos += 1;
          }
          if remaining == 0 {
            break;
          }
        }
        p.line_buf[1][p.line_pos] = 0;
      }
      let bit_code = p.rice.adaptive_rice_decode(true, PREDICT_K_ESCAPE, PREDICT_K_ESCBITS, PREDICT_K_MAX)?;
      p.line_buf[1][p.line_pos] += error_code_signed(bit_code);
      p.line_pos += 1;
      remaining = remaining.saturating_sub(1);
    }

    if remaining == 1 {
      let x = p.coeff_a();
      let bit_code = p.rice.adaptive_rice_decode(true, PREDICT_K_ESCAPE, PREDICT_K_ESCBITS, PREDICT_K_MAX)?;
      p.line_buf[1][p.line_pos] = x + error_code_signed(bit_code);
      p.line_pos += 1;
    }
    debug_assert!(p.line_pos < p.line_buf[1].len());
    p.line_buf[1][p.line_pos] = p.coeff_a() + 1;
    Ok(())
  }

  fn decode_nontop_line(&self, p: &mut BandParam) -> Result<()> {
    debug_assert_eq!(p.line_pos, 1);
    let mut remaining = p.subband_width as u32;

    p.line_buf[1][p.line_pos - 1] = p.coeff_b();

    while remaining > 1 {
      let mut x = 0;

      if p.coeff_a() == p.coeff_b() && p.coeff_a() == p.coeff_d() {

        if p.rice.bitstream_get_bits(1)? == 1 {
          let n_syms = self.symbol_run_count(p, remaining)?;
          remaining = remaining.saturating_sub(n_syms);

          for _ in 0..n_syms {
            p.line_buf[1][p.line_pos] = p.coeff_a();
            p.line_pos += 1;
          }
        }
        if remaining > 0 {
          x = p.coeff_b();
        }
      } else {

        x = med(p.coeff_a(), p.coeff_b(), p.coeff_c());
      }
      if remaining > 0 {
        let mut bit_code = p.rice.adaptive_rice_decode(false, PREDICT_K_ESCAPE, PREDICT_K_ESCBITS, PREDICT_K_MAX)?;

        p.line_buf[1][p.line_pos] = x + error_code_signed(bit_code);

        if remaining > 1 {
          let delta: i32 = (p.coeff_d() - p.coeff_b()) << 1;
          bit_code = (bit_code + delta.unsigned_abs()) >> 1;
        }
        p.rice.update_k_param(bit_code, PREDICT_K_MAX);
        p.line_pos += 1;
      }
      remaining = remaining.saturating_sub(1);
    }

    if remaining == 1 {
      let x = med(p.coeff_a(), p.coeff_b(), p.coeff_c());
      let bit_code = p.rice.adaptive_rice_decode(true, PREDICT_K_ESCAPE, PREDICT_K_ESCBITS, PREDICT_K_MAX)?;

      p.line_buf[1][p.line_pos] = x + error_code_signed(bit_code);
      p.line_pos += 1;
    }
    debug_assert!(p.line_pos < p.line_buf[1].len());
    p.line_buf[1][p.line_pos] = p.coeff_a() + 1;
    Ok(())
  }

  fn decode_symbol_rounded(&self, p: &mut BandParam, use_med: bool, not_eol: bool) -> Result<()> {
    let sym = if use_med { med(p.coeff_a(), p.coeff_b(), p.coeff_c()) } else { p.coeff_b() };
    let bit_code = p.rice.adaptive_rice_decode(false, PREDICT_K_ESCAPE, PREDICT_K_ESCBITS, PREDICT_K_MAX)?;
    let mut code = error_code_signed(bit_code);
    let x = p.rounded_bits_mask * 2 * code + (code >> 31);
    p.line_buf[1][p.line_pos] = x + sym;

    if not_eol {
      if p.coeff_d() > p.coeff_b() {
        code = (p.coeff_d() - p.coeff_b() + p.rounded_bits_mask - 1) >> p.rounded_bits;
      } else {
        code = -((p.coeff_b() - p.coeff_d() + p.rounded_bits_mask) >> p.rounded_bits);
      }
      p.rice.update_k_param((bit_code + 2 * code.unsigned_abs()) >> 1, PREDICT_K_MAX);
    } else {
      p.rice.update_k_param(bit_code, PREDICT_K_MAX);
    }

    p.line_pos += 1;
    Ok(())
  }

  fn decode_top_line_rounded(&self, p: &mut BandParam) -> Result<()> {
    debug_assert_eq!(p.line_pos, 1);
    let mut remaining = p.subband_width as u32;

    p.line_buf[1][p.line_pos - 1] = 0;
    while remaining > 1 {

      if p.coeff_a().abs() > p.rounded_bits_mask {
        p.line_buf[1][p.line_pos] = p.coeff_a();
      } else {
        if p.rice.bitstream_get_bits(1)? == 1 {
          let n_syms = self.symbol_run_count(p, remaining)?;
          remaining = remaining.saturating_sub(n_syms);

          for _ in 0..n_syms {
            p.line_buf[1][p.line_pos] = p.coeff_a();
            p.line_pos += 1;
          }
          if remaining == 0 {
            break;
          }
        }
        p.line_buf[1][p.line_pos] = 0;
      }
      let bit_code = p.rice.adaptive_rice_decode(true, PREDICT_K_ESCAPE, PREDICT_K_ESCBITS, PREDICT_K_MAX)?;
      let code = error_code_signed(bit_code);
      p.line_buf[1][p.line_pos] += p.rounded_bits_mask * 2 * code + (code >> 31);
      p.line_pos += 1;
      remaining = remaining.saturating_sub(1);
    }

    if remaining == 1 {
      let bit_code = p.rice.adaptive_rice_decode(true, PREDICT_K_ESCAPE, PREDICT_K_ESCBITS, PREDICT_K_MAX)?;
      let code = error_code_signed(bit_code);
      p.line_buf[1][p.line_pos] += p.rounded_bits_mask * 2 * code + (code >> 31);
      p.line_pos += 1;
    }
    debug_assert!(p.line_pos < p.line_buf[1].len());
    p.line_buf[1][p.line_pos] = p.coeff_a() + 1;
    Ok(())
  }

  #[allow(clippy::comparison_chain)]
  fn decode_nontop_line_rounded(&self, p: &mut BandParam) -> Result<()> {
    debug_assert_eq!(p.line_pos, 1);
    let mut remaining = p.subband_width as u32;
    let mut value_reached = false;
    p.line_buf[0][p.line_pos - 1] = p.coeff_b();
    p.line_buf[1][p.line_pos - 1] = p.coeff_b();

    while remaining > 1 {
      if (p.coeff_d() - p.coeff_b()).abs() > p.rounded_bits_mask {
        self.decode_symbol_rounded(p, true, true)?;
        value_reached = true;
      } else if value_reached || (p.coeff_c() - p.coeff_a()).abs() > p.rounded_bits_mask {
        self.decode_symbol_rounded(p, true, true)?;
        value_reached = false;
      } else {
        if p.rice.bitstream_get_bits(1)? == 1 {
          let n_syms = self.symbol_run_count(p, remaining)?;
          remaining = remaining.saturating_sub(n_syms);

          for _ in 0..n_syms {
            p.line_buf[1][p.line_pos] = p.coeff_a();
            p.line_pos += 1;
          }
        }
        if remaining > 1 {
          self.decode_symbol_rounded(p, false, true)?;
          value_reached = (p.coeff_b() - p.coeff_c()).abs() > p.rounded_bits_mask;
        } else if remaining == 1 {
          self.decode_symbol_rounded(p, false, false)?;
        }
      }
      remaining = remaining.saturating_sub(1);
    }

    if remaining == 1 {
      self.decode_symbol_rounded(p, true, false)?;
    }
    debug_assert!(p.line_pos < p.line_buf[1].len());
    p.line_buf[1][p.line_pos] = p.coeff_a() + 1;
    Ok(())
  }

  pub(super) fn decode_line(&self, param: &mut BandParam) -> Result<()> {
    debug_assert!(param.cur_line < param.subband_height);

    param.line_pos = 1;
    if param.cur_line == 0 {
      param.s_param = 0;
      param.rice.set_k(0);
      if param.supports_partial {
        if param.rounded_bits_mask <= 0 {
          self.decode_top_line(param)?;
        } else {
          param.rounded_bits = 1;
          if (param.rounded_bits_mask & !1) != 0 {
            while param.rounded_bits_mask >> param.rounded_bits != 0 {
              param.rounded_bits += 1;
            }
          }
          self.decode_top_line_rounded(param)?;
        }
      } else {
        self.decode_top_line_no_ref_prev_line(param)?;
      }
    } else if !param.supports_partial {

      param.line_buf.swap(0, 1);
      self.decode_nontop_line_no_ref_prev_line(param)?;
    } else if param.rounded_bits_mask <= 0 {

      param.line_buf.swap(0, 1);
      self.decode_nontop_line(param)?;
    } else {

      param.line_buf.swap(0, 1);
      self.decode_nontop_line_rounded(param)?;
    }
    param.cur_line += 1;
    Ok(())
  }
}

pub(super) fn constrain(value: i32, min: i32, max: i32) -> i32 {
  std::cmp::min(std::cmp::max(value, min), max)

}

pub(super) fn error_code_signed(bit_code: u32) -> i32 {
  -((bit_code & 1) as i32) ^ (bit_code >> 1) as i32
}

pub(super) fn med(a: i32, b: i32, c: i32) -> i32 {
  if c >= std::cmp::max(a, b) {
    std::cmp::min(a, b)
  } else if c <= std::cmp::min(a, b) {
    std::cmp::max(a, b)
  } else {
    a + b - c
  }
}

#[allow(clippy::type_complexity)]
fn convert_plane_line(codec: &CodecParams, l0: &[i32], l1: &[i32], l2: &[i32], l3: &[i32]) -> Result<(Vec<u16>, Vec<u16>, Vec<u16>, Vec<u16>)> {
  let mut p0 = vec![0; l0.len()];
  let mut p1 = vec![0; l1.len()];
  let mut p2 = vec![0; l2.len()];
  let mut p3 = vec![0; l3.len()];

  match codec.enc_type {
    0 => {
      let median: i32 = 1 << (codec.median_bits - 1);
      let max_val: i32 = (1 << codec.median_bits) - 1;

      izip!(l0, l1, l2, l3).enumerate().for_each(|(i, (v0, v1, v2, v3))| {
        p0[i] = constrain(median + v0, 0, max_val) as u16;
        p1[i] = constrain(median + v1, 0, max_val) as u16;
        p2[i] = constrain(median + v2, 0, max_val) as u16;
        p3[i] = constrain(median + v3, 0, max_val) as u16;
      });
    }
    3 => {
      let median: i32 = 1 << (codec.median_bits - 1) << 10;
      let max_val: i32 = (1 << codec.median_bits) - 1;

      izip!(l0, l1, l2, l3).enumerate().for_each(|(i, (v0, v1, v2, v3))| {
        let mut gr: i32 = median + (v0 << 10) - 168 * v1 - 585 * v3;
        if gr < 0 {
          gr = -(((gr.abs() + 512) >> 9) & !1);
        } else {
          gr = ((gr.abs() + 512) >> 9) & !1;
        }
        p0[i] = constrain((median + (v0 << 10) + 1510 * v3 + 512) >> 10, 0, max_val) as u16;
        p1[i] = constrain((v2 + gr + 1) >> 1, 0, max_val) as u16;
        p2[i] = constrain((gr - v2 + 1) >> 1, 0, max_val) as u16;
        p3[i] = constrain((median + (v0 << 10) + 1927 * v1 + 512) >> 10, 0, max_val) as u16;
      });
    }
    enc_type => {
      return Err(CrxError::General(format!("Unsupported encoding type {}", enc_type)));
    }
  }

  Ok((p0, p1, p2, p3))
}

fn integrate_cfa(codec: &CodecParams, tiles: &[Tile], cfa_buf: &mut [u16], tile_id: usize, plane_id: usize, plane_row: usize, plane_buf: &[u16]) -> Result<()> {

  const CFA_DIM: usize = 2;

  debug_assert_ne!(plane_buf.len(), 0);
  debug_assert_ne!(cfa_buf.len(), 0);
  debug_assert!(codec.tile_cols > 0);
  debug_assert!(codec.tile_rows > 0);

  if plane_id > 3 {
    return Err(CrxError::Overflow(format!(
      "More then 4 planes detected, unable to process plane_id {}",
      plane_id
    )));
  }

  let tile_row_idx = tile_id / codec.tile_cols;
  let tile_col_idx = tile_id % codec.tile_cols;

  let row_offset = tile_row_idx * codec.tile_width;

  let col_offset = tile_col_idx * codec.tile_width;
  let (row_shift, col_shift) = match plane_id {
    0 => (0, 0),
    1 => (0, 1),
    2 => (1, 0),
    3 => (1, 1),
    _ => {
      return Err(CrxError::General("Invalid plane id".to_string()));
    }
  };

  let row_idx = row_offset + (plane_row * CFA_DIM) + row_shift;
  for plane_col in 0..tiles[tile_id].plane_width {

    let col_idx = col_offset + (plane_col * CFA_DIM) + col_shift;

    cfa_buf[(row_idx * codec.image_width) + col_idx] = plane_buf[plane_col];
  }
  Ok(())
}
