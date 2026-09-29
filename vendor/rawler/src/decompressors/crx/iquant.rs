use super::{
  BandParam, CodecParams, CrxError, Result,
  decoder::constrain,
  mdat::{Subband, Tile},
};
use crate::decompressors::crx::{decoder::error_code_signed, rice::RiceDecoder};
use bitstream_io::BitReader;
use log::warn;
use std::io::Cursor;

#[rustfmt::skip]
pub(super) const Q_STEP_TBL: [u32; 6] = [0x28, 0x2D, 0x33, 0x39, 0x40, 0x48];

#[derive(Clone, Debug)]
#[allow(unused)]
pub struct QStep {

  pub q_step_tbl: Vec<u32>,
  pub width: usize,
  pub height: usize,
}

impl QStep {
  pub fn new(width: usize, height: usize) -> Self {
    Self {
      q_step_tbl: Vec::with_capacity(width * height),
      width,
      height,
    }
  }
}

impl CodecParams {

  pub(super) fn update_q_param(_band: &Subband, param: &mut BandParam) -> Result<()> {
    warn!("Untested routine, please send in a sample file");
    let bit_code = param.rice.adaptive_rice_decode(true, 23, 8, 0)?;
    param.q_param = ((param.q_param as i32) + error_code_signed(bit_code)) as u32;
    if param.rice.k() > 7 {
      Err(CrxError::General(format!(
        "Overflow while updating Q parameter: K is out of range: {}",
        param.rice.k()
      )))
    } else {
      Ok(())
    }
  }

  pub(super) fn decode_line_with_iquantization(&self, band: &Subband, param: &mut BandParam, q_step: Option<&QStep>) -> super::Result<Vec<i32>> {
    if band.data_size == 0 {
      return Ok(Vec::new());
    }

    if band.support_partial && q_step.is_none() {
      debug_assert_eq!(1, 2);
      Self::update_q_param(band, param)?;
    }

    self.decode_line(param)?;

    match q_step {
      Some(q_step) => {

        let q_step_tbl_ptr = &q_step.q_step_tbl[(q_step.width * band.get_subband_row(param.cur_line - 1))..];

        for i in 0..band.col_start_addon {
          let quant_val = band.q_step_base + ((q_step_tbl_ptr[0] * band.q_step_multi as u32) >> 3) as i32;
          param.decoded_buf_mut()[i] *= constrain(quant_val, 1, 0x168000);
        }

        for i in band.col_start_addon..(band.width - band.col_end_addon) {
          let idx = (i - band.col_start_addon) >> band.level_shift;
          let quant_val = band.q_step_base + ((q_step_tbl_ptr[idx] * band.q_step_multi as u32) >> 3) as i32;

          param.decoded_buf_mut()[i] *= constrain(quant_val, 1, 0x168000);
        }

        let last_idx = (band.width - band.col_end_addon - band.col_start_addon - 1) >> band.level_shift;

        for i in (band.width - band.col_end_addon)..band.width {
          let quant_val = band.q_step_base + ((q_step_tbl_ptr[last_idx] * band.q_step_multi as u32) >> 3) as i32;
          param.decoded_buf_mut()[i] *= constrain(quant_val, 1, 0x168000);
        }
      }
      None => {

        let q_scale = if param.q_param / 6 >= 6 {
          Q_STEP_TBL[param.q_param as usize % 6] * (1 << (param.q_param / 6 + 26))
        } else {
          Q_STEP_TBL[param.q_param as usize % 6] >> (6 - param.q_param / 6)
        };

        if q_scale != 1 {

          for i in 0..band.width {
            param.decoded_buf_mut()[i] *= q_scale as i32;
          }
        }
      }
    }
    Ok(Vec::from(param.decoded_buf()))
  }
}

impl Tile {

  fn predict_qp_symbol(left: i32, top: i32, delta_h: i32, delta_v: i32) -> i32 {
    match ((delta_v < 0) ^ (delta_h < 0), (left < top) ^ (delta_h < 0)) {
      (false, false) | (false, true) => left + delta_h,
      (true, false) => left,
      (true, true) => top,
    }
  }

  fn make_qstep(&self, params: &CodecParams, qp_table: Vec<i32>) -> Result<Vec<QStep>> {
    debug_assert!(params.levels <= 3 && params.levels > 0);
    let qp_width = (self.plane_width >> 3) + if self.plane_width & 7 != 0 { 1 } else { 0 };
    let qp_height = (self.plane_height >> 1) + (self.plane_height & 1);
    let qp_height4 = (self.plane_height >> 2) + if self.plane_height & 3 > 0 { 1 } else { 0 };
    let qp_height8 = (self.plane_height >> 3) + if self.plane_height & 7 > 0 { 1 } else { 0 };

    let mut q_steps = Vec::with_capacity(params.levels as usize);

    let q_lookup = |quant_val: i32| -> u32 {

      if quant_val / 6 >= 6 {

        Q_STEP_TBL[quant_val as usize % 6].wrapping_shl(quant_val as u32 / 6 + 26)
      } else {
        Q_STEP_TBL[quant_val as usize % 6] >> (6 - quant_val / 6)
      }
    };

    for level in (1..=params.levels).rev() {
      match level {
        3 => {
          let mut q_step = QStep::new(qp_width, qp_height8);
          for qp_row in 0..qp_height8 {
            let mut row0_idx = qp_width * std::cmp::min(4 * qp_row + 0, qp_height - 1);
            let mut row1_idx = qp_width * std::cmp::min(4 * qp_row + 1, qp_height - 1);
            let mut row2_idx = qp_width * std::cmp::min(4 * qp_row + 2, qp_height - 1);
            let mut row3_idx = qp_width * std::cmp::min(4 * qp_row + 3, qp_height - 1);
            for _qp_col in 0..qp_width {
              let qp_sum = qp_table[row0_idx] + qp_table[row1_idx] + qp_table[row2_idx] + qp_table[row3_idx];
              let quant_val = if qp_sum.is_negative() {
                (qp_sum + 3) / 4
              } else {
                qp_sum / 4
              };

              let x = q_lookup(quant_val);

              q_step.q_step_tbl.push(x);
              row0_idx += 1;
              row1_idx += 1;
              row2_idx += 1;
              row3_idx += 1;
            }
          }
          debug_assert_eq!(q_step.q_step_tbl.len(), qp_width * qp_height8);

          q_steps.push(q_step);
        }
        2 => {
          let mut q_step = QStep::new(qp_width, qp_height4);
          for qp_row in 0..qp_height4 {
            let mut row0_idx = qp_width * std::cmp::min(2 * qp_row + 0, qp_height - 1);
            let mut row1_idx = qp_width * std::cmp::min(2 * qp_row + 1, qp_height - 1);
            for _qp_col in 0..qp_width {
              let quant_val = (qp_table[row0_idx] + qp_table[row1_idx]) / 2;
              let x = q_lookup(quant_val);

              q_step.q_step_tbl.push(x);
              row0_idx += 1;
              row1_idx += 1;
            }
          }
          debug_assert_eq!(q_step.q_step_tbl.len(), qp_width * qp_height4);

          q_steps.push(q_step);
        }
        1 => {

          let mut q_step = QStep::new(qp_width, qp_height);
          for qp_row in 0..qp_height {
            for qp_col in 0..qp_width {
              let quant_val = qp_table[(qp_row * qp_width) + qp_col];
              let x = q_lookup(quant_val);

              q_step.q_step_tbl.push(x);
            }
          }
          debug_assert_eq!(q_step.q_step_tbl.len(), qp_width * qp_height);
          q_steps.push(q_step);
        }
        _ => {
          return Err(CrxError::General(format!("Unsupported level while generating qstep data: {}", level)));
        }
      }
    }
    Ok(q_steps)
  }

  pub(super) fn generate_qstep_table(&mut self, params: &CodecParams, data: &[u8]) -> Result<()> {
    match self.qp_data.as_ref() {
      Some(qp_data) => {

        let mdat_qp = &data[self.data_offset..self.data_offset + qp_data.mdat_qp_data_size as usize];
        let bitpump = BitReader::endian(Cursor::new(mdat_qp), bitstream_io::BigEndian);
        let mut rice = RiceDecoder::new(bitpump);

        let qp_width = (self.plane_width >> 3) + if self.plane_width & 7 != 0 { 1 } else { 0 };
        let qp_height = (self.plane_height >> 1) + (self.plane_height & 1);
        let total_qp = qp_width * qp_height;

        let line_len = 1 + qp_width + 1;
        let mut line_buf = [vec![0; line_len], vec![0; line_len]];
        let mut qp_table = vec![0; total_qp];

        for qp_row in 0..qp_height {
          let mut line_pos = 1;
          if qp_row == 0 {

            line_buf[1][line_pos - 1] = 0;
            for _ in (0..qp_width).rev() {
              let x = line_buf[1][line_pos - 1];
              let qp = rice.adaptive_rice_decode(true, 23, 8, 7)?;
              line_buf[1][line_pos] = x + error_code_signed(qp);
              line_pos += 1;
            }
            line_buf[1][line_pos] = line_buf[1][line_pos - 1] + 1;
          } else {

            line_buf[1][line_pos - 1] = line_buf[0][line_pos];

            let mut delta_h = line_buf[0][line_pos] - line_buf[0][line_pos - 1];
            for width in (0..qp_width).rev() {
              let a = line_buf[1][line_pos - 1];
              let b = line_buf[0][line_pos];
              let c = line_buf[0][line_pos - 1];
              let d = line_buf[0][line_pos + 1];
              let x = Self::predict_qp_symbol(a, b, delta_h, c - a);
              let qp = rice.adaptive_rice_decode(false, 23, 8, 0)?;
              line_buf[1][line_pos] = x + error_code_signed(qp);
              if width > 0 {
                delta_h = d - b;
                rice.update_k_param((qp + 2 * delta_h.unsigned_abs()) >> 1, 7);
              } else {
                rice.update_k_param(qp, 7);
              }
              line_pos += 1;
            }
            line_buf[1][line_pos] = line_buf[1][line_pos - 1] + 1;
          }

          for qp_col in 0..qp_width {
            qp_table[(qp_row * qp_width) + qp_col] = line_buf[1][qp_col + 1] + 4;
          }
          line_buf.swap(0, 1);
        }

        self.q_step = Some(self.make_qstep(params, qp_table)?);
        Ok(())
      }
      None => Ok(()),
    }
  }
}
