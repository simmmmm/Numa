use crate::pumps::BitPump;
use std::fmt;

const DECODE_CACHE_BITS: u32 = 12;

const CODE_ONLY: u32 = 1 << 31;

pub struct HuffTable {

  pub bits: [u32; 17],
  pub huffval: [u32; 256],

  pub shiftval: [u32; 256],

  pub dng_bug: bool,

  pub disable_cache: bool,

  pub nbits: u32,

  pub hufftable: Vec<(u8, u8, u8)>,

  decodecache: [u32; 1 << DECODE_CACHE_BITS],

  initialized: bool,
}

struct MockPump {
  bits: u64,
  nbits: u32,
}

impl MockPump {
  pub fn empty() -> Self {
    MockPump { bits: 0, nbits: 0 }
  }

  pub fn set(&mut self, bits: u32, nbits: u32) {
    self.bits = (bits as u64) << 32;
    self.nbits = nbits + 32;
  }

  pub fn validbits(&self) -> i32 {
    self.nbits as i32 - 32
  }
}

impl BitPump for MockPump {
  fn peek_bits(&mut self, num: u32) -> u32 {
    (self.bits >> (self.nbits - num)) as u32
  }

  fn consume_bits(&mut self, num: u32) {
    self.nbits -= num;
    self.bits &= (1 << self.nbits) - 1;
  }
}

impl HuffTable {
  pub fn empty() -> HuffTable {
    HuffTable {
      bits: [0; 17],
      huffval: [0; 256],
      shiftval: [0; 256],
      dng_bug: false,
      disable_cache: false,

      nbits: 0,
      hufftable: Vec::new(),
      decodecache: [0; 1 << DECODE_CACHE_BITS],
      initialized: false,
    }
  }

  pub fn new(bits: [u32; 17], huffval: [u32; 256], dng_bug: bool) -> Result<HuffTable, String> {
    let mut tbl = HuffTable {
      bits,
      huffval,
      shiftval: [0; 256],
      dng_bug,
      disable_cache: false,

      nbits: 0,
      hufftable: Vec::new(),
      decodecache: [0; 1 << DECODE_CACHE_BITS],
      initialized: false,
    };
    tbl.initialize()?;
    Ok(tbl)
  }

  pub fn same_code(&self, other: &HuffTable) -> bool {
    self.bits == other.bits
      && self.huffval == other.huffval
      && self.shiftval == other.shiftval
      && self.dng_bug == other.dng_bug
      && self.disable_cache == other.disable_cache
  }

  pub fn initialize(&mut self) -> Result<(), String> {

    self.nbits = 16;
    for i in 0..16 {
      if self.bits[16 - i] != 0 {
        break;
      }
      self.nbits -= 1;
    }
    self.hufftable = vec![(0, 0, 0); 1 << self.nbits];

    let mut h = 0;
    let mut pos = 0;
    for len in 0..self.nbits {

      for _ in 0..self.bits[len as usize + 1] {

        for _ in 0..(1 << (self.nbits - len - 1)) {
          self.hufftable[h] = (len as u8 + 1, self.huffval[pos] as u8, self.shiftval[pos] as u8);
          h += 1;
        }
        pos += 1;
      }
    }

    if !self.disable_cache {
      let mut pump = MockPump::empty();
      let mut i = 0;
      loop {
        pump.set(i, DECODE_CACHE_BITS);
        let (bits, decode) = self.huff_decode_slow(&mut pump);
        if pump.validbits() >= 0 {

          let consume = match (decode, self.dng_bug) {
            (-32768, false) => bits as u32 - 16,
            _ => bits as u32,
          };

          if consume > 0 {
            self.decodecache[i as usize] = (consume << 16) | (decode as i16 as u16 as u32);
          }
        }
        if self.decodecache[i as usize] == 0 {

          let code = if self.nbits >= DECODE_CACHE_BITS {
            (i as usize) << (self.nbits - DECODE_CACHE_BITS)
          } else {
            (i as usize) >> (DECODE_CACHE_BITS - self.nbits)
          };
          let (bits, len, shift) = self.hufftable[code];
          if bits > 0 && bits as u32 <= DECODE_CACHE_BITS {
            self.decodecache[i as usize] = CODE_ONLY | ((bits as u32) << 16) | ((shift as u32) << 8) | len as u32;
          }
        }
        i += 1;
        if i >= 1 << DECODE_CACHE_BITS {
          break;
        }
      }
    }

    self.initialized = true;
    Ok(())
  }

  #[inline(always)]
  pub fn huff_decode<P: BitPump + ?Sized>(&self, pump: &mut P) -> Result<i32, String> {
    let code = pump.peek_bits(DECODE_CACHE_BITS) as usize;
    let cached = self.decodecache[code];
    if (cached as i32) > 0 {
      pump.consume_bits(cached >> 16);
      Ok(cached as u16 as i16 as i32)
    } else if cached != 0 {
      pump.consume_bits((cached >> 16) & 0xff);
      Ok(self.huff_diff(pump, (0, cached as u8, (cached >> 8) as u8)))
    } else {
      let decode = self.huff_decode_slow(pump);
      Ok(decode.1)
    }
  }

  #[inline(always)]
  pub fn huff_decode_slow<P: BitPump + ?Sized>(&self, pump: &mut P) -> (u8, i32) {
    let len = self.huff_len(pump);
    (len.0 + len.1, self.huff_diff(pump, len))
  }

  #[inline(always)]
  pub fn huff_len<P: BitPump + ?Sized>(&self, pump: &mut P) -> (u8, u8, u8) {
    let code = pump.peek_bits(self.nbits) as usize;
    let (bits, len, shift) = self.hufftable[code];
    pump.consume_bits(bits as u32);
    (bits, len, shift)
  }

  #[inline(always)]
  pub fn huff_get_bits<P: BitPump + ?Sized>(&self, pump: &mut P) -> u32 {
    let code = pump.peek_bits(self.nbits) as usize;
    let (bits, len, _) = self.hufftable[code];
    pump.consume_bits(bits as u32);
    len as u32
  }

  #[inline(always)]
  pub fn huff_diff<P: BitPump + ?Sized>(&self, pump: &mut P, input: (u8, u8, u8)) -> i32 {
    let (_, len, shift) = input;

    match len {
      0 => 0,
      16 => {
        if self.dng_bug {
          pump.get_bits(16);
        }
        -32768
      }
      len => {

        let fulllen: i32 = len as i32 + shift as i32;
        let shift: i32 = shift as i32;
        let bits = pump.get_bits(len as u32) as i32;
        let mut diff: i32 = ((bits << 1) + 1) << shift >> 1;
        if (diff & (1 << (fulllen - 1))) == 0 {
          diff -= (1 << fulllen) - ((shift == 0) as i32);
        }
        diff
      }
    }
  }
}

impl fmt::Debug for HuffTable {
  fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
    if self.initialized {
      write!(f, "HuffTable {{ bits: {:?} huffval: {:?} }}", self.bits, &self.huffval[..])
    } else {
      write!(f, "HuffTable {{ uninitialized }}")
    }
  }
}
