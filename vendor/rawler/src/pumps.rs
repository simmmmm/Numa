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

  /// Refill internal bit buffer - Little-Endian
  ///
  /// For fast refill, we can simply take a whole u32 value out.
  /// For slow refill, there may be 1, 2 or 3 bytes left in buffer. We need
  /// to collect them manually.
  fn refill(&mut self) -> (u32, u32) {
    if let Some(chunk) = self.buffer.next() {
      if chunk.len() == 4 {
        // Fast refill
        let bits: u32 = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        (bits, u32::BITS)
      } else {
        // Slow refill
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

/// Numa (PERF-021): the bits are kept at the top of a 64-bit word and refilled
/// with as many whole bytes as fit, eight read at once, rather than one
/// 32-bit chunk at a time through an iterator. The same bits come out in the
/// same order; a refill with nothing left to read panics as before.
#[derive(Debug, Clone)]
pub struct BitPumpMSB<'a> {
  buffer: &'a [u8],
  pos: usize,
  /// The next `nbits` bits of the stream, most significant first. Below them
  /// may sit the start of the bytes not yet counted, which the next refill
  /// writes over with the same bits.
  bits: u64,
  nbits: u32,
  /// Past the end, zeros rather than a panic.
  zeros: bool,
  /// Zero bits made up past the end, so the position keeps counting.
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

  /// Numa (PERF-022): a pump that reads zeros past the end of `src`, as
  /// [`BitPumpJPEG`] does after the last marker.
  pub fn new_zero_padded(src: &'a [u8]) -> Self {
    Self { zeros: true, ..Self::new(src) }
  }

  /// How many bits have been consumed.
  #[inline(always)]
  pub fn bit_pos(&self) -> usize {
    self.pos * 8 + self.pad - self.nbits as usize
  }

  /// Refill internal bit buffer - Big-Endian
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

  /// The last few bytes, one at a time.
  #[cold]
  fn refill_tail(&mut self, num: u32) {
    // Only the counted bits are kept; the rest is filled byte by byte.
    self.bits &= !(u64::MAX.checked_shr(self.nbits).unwrap_or(0));
    if self.pos >= self.buffer.len() && !self.zeros {
      panic!("Can't refill bitpump, buffer exhausted");
    }
    while self.nbits <= 56 && self.pos < self.buffer.len() {
      self.bits |= (self.buffer[self.pos] as u64) << (56 - self.nbits);
      self.pos += 1;
      self.nbits += 8;
    }
    // Short at the very end: what is missing reads as zeros.
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

  /// Refill internal bit buffer - MSB32 (bytes read in little-endian order)
  ///
  /// For fast refill, we can simply take a whole u32 value out.
  /// For slow refill, there may be 1, 2 or 3 bytes left in buffer. We need
  /// to collect them manually.
  fn refill(&mut self) -> (u32, u32) {
    if let Some(chunk) = self.buffer.next() {
      if chunk.len() == 4 {
        // Fast refill
        let bits: u32 = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
        (bits, u32::BITS)
      } else {
        // Slow refill
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

  // Sign extend ibits
  #[inline(always)]
  fn get_ibits_sextended(&mut self, num: u32) -> i32 {
    let val = self.get_ibits(num);
    val.wrapping_shl(32 - num).wrapping_shr(32 - num)
  }

  /// Count the leading zeroes block-wise in 31 bits
  /// per block and returns the count.
  /// All zero bits are consumed.
  #[inline(always)]
  fn consume_zerobits(&mut self) -> u32 {
    // Take one bit less because leading_zeros() is undefined
    // when all bits in register are zero.
    const BITS_PER_LOOP: u32 = u32::BITS - 1;
    let mut count = 0;
    // Count-and-skip all the leading `0`s.
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
    // `num` is at most 32, so the shift is 32 or more and never 64 but for 0.
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
        // Read 32 bits the hard way
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
                self.pos += 1; // Skip the extra byte used to mark 255
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
      // Stuff with zeroes to not fail to read
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

  //  #[inline(always)]
  //  pub fn peek_u32(&self) -> u32 { self.endian.ru32(self.buffer, self.pos) }
  //  #[inline(always)]
  //  pub fn get_u32(&mut self) -> u32 {
  //    let val = self.peek_u32();
  //    self.pos += 4;
  //    val
  //  }

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
    self.pos += 1; // Make the next byte the marker
    Ok(skip_count + 1)
  }
}

/// This pump is for bitstreams where values are stored in LSB bit order.
/// During refill, bits are converted from LSB to MSB so peaking
/// is done by reading in MSB mode.
///
/// Input bitstream is:     1011 0101 0010 1110...
/// Output for peek(10) is: 1010 1101 01
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
