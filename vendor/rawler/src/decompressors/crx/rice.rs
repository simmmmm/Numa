use super::BitPump;
use super::Result;
use bitstream_io::BitRead;

pub(super) struct RiceDecoder<'mdat> {

  bitpump: BitPump<'mdat>,
  k_param: u32,
}

impl<'mdat> RiceDecoder<'mdat> {

  pub(super) fn new(bitpump: BitPump<'mdat>) -> Self {
    Self { bitpump, k_param: 0 }
  }

  #[inline(always)]
  pub(super) fn k(&self) -> u32 {
    self.k_param
  }

  #[inline(always)]
  pub(super) fn set_k(&mut self, k: u32) {
    self.k_param = k;
  }

  #[inline(always)]
  pub(super) fn bitstream_zeros(&mut self) -> Result<u32> {
    Ok(self.bitpump.read_unary::<1>()?)
  }

  #[inline(always)]
  pub(super) fn bitstream_get_bits(&mut self, bits: u32) -> Result<u32> {
    debug_assert!(bits <= 32);
    Ok(self.bitpump.read_var(bits)?)
  }

  fn rice_decode(&mut self, escape: u32, esc_bits: u32) -> Result<u32> {

    let prefix = self.bitstream_zeros()?;
    if prefix >= escape {

      Ok(self.bitstream_get_bits(esc_bits)?)
    } else if self.k_param > 0 {

      Ok((prefix << self.k_param) | self.bitstream_get_bits(self.k_param)?)
    } else {

      Ok(prefix)
    }
  }

  pub(super) fn adaptive_rice_decode(&mut self, adapt_k: bool, escape: u32, esc_bits: u32, k_max: u32) -> Result<u32> {
    let val = self.rice_decode(escape, esc_bits)?;
    if adapt_k {
      self.k_param = Self::predict_k_param_max(self.k_param, val, k_max);
    }
    Ok(val)
  }

  pub(super) fn update_k_param(&mut self, bit_code: u32, k_max: u32) {
    self.k_param = Self::predict_k_param_max(self.k_param, bit_code, k_max);
  }

  fn predict_k_param_max(prev_k: u32, value: u32, k_max: u32) -> u32 {
    let mut new_k = prev_k;
    if value >> prev_k > 2 {
      new_k += 1;
    }
    if value >> prev_k > 5 {
      new_k += 1;
    }
    if value < ((1 << prev_k) >> 1) {
      new_k -= 1;
    }

    if k_max > 0 { std::cmp::min(new_k, k_max) } else { new_k }
  }
}
