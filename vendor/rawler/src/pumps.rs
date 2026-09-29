use std::slice::Chunks;

use crate::bits::*;

#[derive(Debug, Clone)]
pub struct BitPumpLSB<'a> {
  buffer: Chunks<'a, u8>,
  bits: u64,
  nbits: u32,
}

impl<'a> BitPumpLSB<'a> {
  pub fn new(src: &'a [u8]) -> Self {
    Self {
      buffer: src.chunks(size_of::<u32>()),
      bits: 0,
      nbits: 0,
    }
  }

  fn refill(&mut self) -> (u32, u32) {
    if let Some(chunk) = self.buffer.next() {
      if chunk.len() == 4 {

        let bits: u32 = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        (bits, u32::BITS)
      } else {

        chunk
          .into_iter()
          .rev()
          .fold((0, 0), |(bits, bit_cnt), x| ((bits << 8) | *x as u32, bit_cnt + 8))
      }
    } else {
      panic!("Can't refill bitpump, buffer exhausted");
    }
  }
}

#[derive(Debug, Clone)]
pub struct BitPumpMSB<'a> {
  buffer: &'a [u8],
  pos: usize,

  bits: u64,
  nbits: u32,

  zeros: bool,

  pad: usize,
}

impl<'a> BitPumpMSB<'a> {
  pub fn new(src: &'a [u8]) -> Self {
    Self {
      buffer: src,
      pos: 0,
      bits: 0,
      nbits: 0,
      zeros: false,
      pad: 0,
    }
  }

  pub fn new_zero_padded(src: &'a [u8]) -> Self {
    Self { zeros: true, ..Self::new(src) }
  }

  #[inline(always)]
  pub fn bit_pos(&self) -> usize {
    self.pos * 8 + self.pad - self.nbits as usize
  }

  #[inline(always)]
  fn refill(&mut self, num: u32) {
    debug_assert!(self.nbits < num && num <= 32);
    if let Some(chunk) = self.buffer.get(self.pos..self.pos + 8) {
      let next = u64::from_be_bytes(chunk.try_into().unwrap());
      self.bits |= next >> self.nbits;
      let bytes = (63 - self.nbits) >> 3;
      self.pos += bytes as usize;
      self.nbits += bytes << 3;
    } else {
      self.refill_tail(num);
    }
  }

  #[cold]
  fn refill_tail(&mut self, num: u32) {

    self.bits &= !(u64::MAX.checked_shr(self.nbits).unwrap_or(0));
    if self.pos >= self.buffer.len() && !self.zeros {
      panic!("Can't refill bitpump, buffer exhausted");
    }
    while self.nbits <= 56 && self.pos < self.buffer.len() {
      self.bits |= (self.buffer[self.pos] as u64) << (56 - self.nbits);
      self.pos += 1;
      self.nbits += 8;
    }

    if self.nbits < num {
      self.pad += (num - self.nbits) as usize;
      self.nbits = num;
    }
  }
}

#[derive(Debug, Clone)]
pub struct BitPumpMSB32<'a> {
  buffer: Chunks<'a, u8>,
  pos: usize,
  bits: u64,
  nbits: u32,
}

impl<'a> BitPumpMSB32<'a> {
  pub fn new(src: &'a [u8]) -> Self {
    Self {
      buffer: src.chunks(size_of::<u32>()),
      pos: 0,
      bits: 0,
      nbits: 0,
    }
  }

  fn refill(&mut self) -> (u32, u32) {
    if let Some(chunk) = self.buffer.next() {
      if chunk.len() == 4 {

        let bits: u32 = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        (bits, u32::BITS)
      } else {

        chunk
          .into_iter()
          .rev()
          .fold((0, 0), |(bits, bit_cnt), x| ((bits << 8) | *x as u32, bit_cnt + 8))
      }
    } else {
      panic!("Can't refill bitpump, buffer exhausted");
    }
  }

  #[inline(always)]
  pub fn get_pos(&self) -> usize {
    self.pos - ((self.nbits >> 3) as usize)
  }
}

#[derive(Debug, Copy, Clone)]
pub struct BitPumpJPEG<'a> {
  buffer: &'a [u8],
  pos: usize,
  bits: u64,
  nbits: u32,
  finished: bool,
}

impl<'a> BitPumpJPEG<'a> {
  pub fn new(src: &'a [u8]) -> Self {
    Self {
      buffer: src,
      pos: 0,
      bits: 0,
      nbits: 0,
      finished: false,
    }
  }
}

pub trait BitPump {
  fn peek_bits(&mut self, num: u32) -> u32;
  fn consume_bits(&mut self, num: u32);

  #[inline(always)]
  fn get_bits(&mut self, num: u32) -> u32 {
    if num == 0 {
      return 0;
    }

    let val = self.peek_bits(num);
    self.consume_bits(num);

    val
  }

  #[inline(always)]
  fn peek_ibits(&mut self, num: u32) -> i32 {
    self.peek_bits(num) as i32
  }

  #[inline(always)]
  fn get_ibits(&mut self, num: u32) -> i32 {
    self.get_bits(num) as i32
  }

  #[inline(always)]
  fn get_ibits_sextended(&mut self, num: u32) -> i32 {
    let val = self.get_ibits(num);
    val.wrapping_shl(32 - num).wrapping_shr(32 - num)
  }

  #[inline(always)]
  fn consume_zerobits(&mut self) -> u32 {

    const BITS_PER_LOOP: u32 = u32::BITS - 1;
    let mut count = 0;

    loop {
      let batch: u32 = (self.peek_bits(BITS_PER_LOOP) << 1) | 0x1;
      let n = batch.leading_zeros();
      self.consume_bits(n);
      count += n;
      if n != BITS_PER_LOOP {
        break;
      }
    }
    count
  }
}

impl<'a> BitPump for BitPumpLSB<'a> {
  #[inline(always)]
  fn peek_bits(&mut self, num: u32) -> u32 {
    if num > self.nbits {
      let (inbits, bit_cnt) = self.refill();
      self.bits = (((inbits as u64) << 32) | (self.bits << (32 - self.nbits))) >> (32 - self.nbits);
      self.nbits += bit_cnt;
    }
    (self.bits & (0x0ffffffffu64 >> (32 - num))) as u32
  }

  #[inline(always)]
  fn consume_bits(&mut self, num: u32) {
    self.nbits -= num;
    self.bits >>= num;
  }
}

impl<'a> BitPump for BitPumpMSB<'a> {
  #[inline(always)]
  fn peek_bits(&mut self, num: u32) -> u32 {
    if num > self.nbits {
      self.refill(num);
    }

    (self.bits.checked_shr(64 - num).unwrap_or(0)) as u32
  }

  #[inline(always)]
  fn consume_bits(&mut self, num: u32) {
    self.nbits -= num;
    self.bits <<= num;
  }
}

impl<'a> BitPump for BitPumpMSB32<'a> {
  #[inline(always)]
  fn peek_bits(&mut self, num: u32) -> u32 {
    if num > self.nbits {
      let (inbits, bit_cnt) = self.refill();
      self.bits = (self.bits << 32) | inbits as u64;
      self.nbits += bit_cnt;
      self.pos += bit_cnt as usize / 8;
    }
    (self.bits >> (self.nbits - num)) as u32
  }

  #[inline(always)]
  fn consume_bits(&mut self, num: u32) {
    self.nbits -= num;
    self.bits &= (1 << self.nbits) - 1;
  }
}

impl<'a> BitPump for BitPumpJPEG<'a> {
  #[inline(always)]
  fn peek_bits(&mut self, num: u32) -> u32 {
    if num > self.nbits && !self.finished {
      if (self.buffer.len() >= 4)
        && self.pos < self.buffer.len() - 4
        && self.buffer[self.pos + 0] != 0xff
        && self.buffer[self.pos + 1] != 0xff
        && self.buffer[self.pos + 2] != 0xff
        && self.buffer[self.pos + 3] != 0xff
      {
        let inbits: u64 = BEu32(self.buffer, self.pos) as u64;
        self.bits = (self.bits << 32) | inbits;
        self.pos += 4;
        self.nbits += 32;
      } else {

        let mut read_bytes = 0;
        while read_bytes < 4 && !self.finished {
          let byte = {
            if self.pos >= self.buffer.len() {
              self.finished = true;
              0
            } else {
              let nextbyte = self.buffer[self.pos];
              if nextbyte != 0xff {
                nextbyte
              } else if self.buffer[self.pos + 1] == 0x00 {
                self.pos += 1;
                nextbyte
              } else {
                self.finished = true;
                0
              }
            }
          };
          self.bits = (self.bits << 8) | (byte as u64);
          self.pos += 1;
          self.nbits += 8;
          read_bytes += 1;
        }
      }
    }
    if num > self.nbits && self.finished {

      self.bits <<= 32;
      self.nbits += 32;
    }

    (self.bits >> (self.nbits - num)) as u32
  }

  #[inline(always)]
  fn consume_bits(&mut self, num: u32) {
    debug_assert!(num <= self.nbits);
    self.nbits -= num;
    self.bits &= (1 << self.nbits) - 1;
  }
}

#[derive(Debug, Copy, Clone)]
pub struct ByteStream<'a> {
  buffer: &'a [u8],
  pos: usize,
  endian: Endian,
}

impl<'a> ByteStream<'a> {
  pub fn new(src: &'a [u8], endian: Endian) -> Self {
    Self { buffer: src, pos: 0, endian }
  }

  #[inline(always)]
  pub fn remaining_bytes(&self) -> usize {
    self.buffer.len() - self.pos
  }

  #[inline(always)]
  pub fn get_pos(&self) -> usize {
    self.pos
  }

  #[inline(always)]
  pub fn peek_u8(&self) -> u8 {
    self.buffer[self.pos]
  }
  #[inline(always)]
  pub fn get_u8(&mut self) -> u8 {
    let val = self.peek_u8();
    self.pos += 1;
    val
  }

  #[inline(always)]
  pub fn peek_i8(&self) -> i8 {
    self.buffer[self.pos] as i8
  }
  #[inline(always)]
  pub fn get_i8(&mut self) -> i8 {
    let val = self.peek_i8();
    self.pos += 1;
    val
  }

  #[inline(always)]
  pub fn peek_u16(&self) -> u16 {
    self.endian.read_u16(self.buffer, self.pos)
  }
  #[inline(always)]
  pub fn get_u16(&mut self) -> u16 {
    let val = self.peek_u16();
    self.pos += 2;
    val
  }

  #[inline(always)]
  pub fn peek_i16(&self) -> i16 {
    self.endian.read_i16(self.buffer, self.pos)
  }
  #[inline(always)]
  pub fn get_i16(&mut self) -> i16 {
    let val = self.peek_i16();
    self.pos += 2;
    val
  }

  #[inline(always)]
  pub fn peek_u32(&self) -> u32 {
    self.endian.read_u32(self.buffer, self.pos)
  }

  #[inline(always)]
  pub fn get_u32(&mut self) -> u32 {
    let val = self.peek_u32();
    self.pos += 4;
    val
  }

  #[inline(always)]
  pub fn get_bytes(&mut self, n: usize) -> Vec<u8> {
    let mut val = Vec::with_capacity(n);
    val.extend_from_slice(&self.buffer[self.pos..self.pos + n]);
    self.pos += n;
    val
  }

  #[inline(always)]
  pub fn consume_bytes(&mut self, num: usize) {
    self.pos += num
  }

  #[inline(always)]
  pub fn skip_to_marker(&mut self) -> Result<usize, String> {
    let mut skip_count = 0;
    while !(self.buffer[self.pos] == 0xFF && self.buffer[self.pos + 1] != 0 && self.buffer[self.pos + 1] != 0xFF) {
      self.pos += 1;
      skip_count += 1;
      if self.pos >= self.buffer.len() {
        return Err("No marker found inside rest of buffer".to_string());
      }
    }
    self.pos += 1;
    Ok(skip_count + 1)
  }
}

#[derive(Debug, Copy, Clone)]
pub struct BitPumpReverseBitsMSB<'a> {
  buffer: &'a [u8],
  pos: usize,
  bits: u64,
  nbits: u32,
}

impl<'a> BitPumpReverseBitsMSB<'a> {
  pub fn new(src: &'a [u8]) -> Self {
    Self {
      buffer: src,
      pos: 0,
      bits: 0,
      nbits: 0,
    }
  }
}

impl<'a> BitPump for BitPumpReverseBitsMSB<'a> {
  #[inline(always)]
  fn peek_bits(&mut self, num: u32) -> u32 {
    debug_assert!(num <= 32);
    if num > self.nbits {
      let mut raw: [u8; 4] = BEu32(self.buffer, self.pos).to_ne_bytes();
      raw[0] = raw[0].reverse_bits();
      raw[1] = raw[1].reverse_bits();
      raw[2] = raw[2].reverse_bits();
      raw[3] = raw[3].reverse_bits();
      let inbits: u64 = u32::from_ne_bytes(raw) as u64;
      self.bits = (self.bits << 32) | inbits;
      self.pos += 4;
      self.nbits += 32;
    }
    (self.bits >> (self.nbits - num)) as u32
  }

  #[inline(always)]
  fn consume_bits(&mut self, num: u32) {
    self.nbits -= num;
    self.bits &= (1 << self.nbits) - 1;
  }
}
