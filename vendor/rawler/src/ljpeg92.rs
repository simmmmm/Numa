use byteorder::{BigEndian, WriteBytesExt};
use multiversion::multiversion;
use std::{
  cmp::min,
  io::{Cursor, Write},
};
use thiserror::Error;

use crate::{bitarray::BitArray16, inspector};

const NUM_BITS_TBL: [u16; 256] = build_num_bits_tbl();

const fn build_num_bits_tbl() -> [u16; 256] {
  let mut tbl = [0; 256];
  let mut i = 1;
  loop {
    if i < 256 {
      let mut nbits = 1;
      let mut tmp = i;
      loop {
        tmp >>= 1;
        if tmp != 0 {
          nbits += 1;
        } else {
          break;
        }
      }
      tbl[i] = nbits;
      i += 1;
    } else {
      break;
    }
  }
  tbl
}

fn lookup_ssss(diff: i16) -> u16 {
  let diff_abs = (diff as i32).unsigned_abs() as usize;
  if diff_abs >= 256 {
    NUM_BITS_TBL[(diff_abs >> 8) & 0xFF] + 8
  } else {
    NUM_BITS_TBL[diff_abs & 0xFF]
  }

}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd)]
pub enum Predictor {
  P1 = 1,
  P2 = 2,
  P3 = 3,
  P4 = 4,
  P5 = 5,
  P6 = 6,
  P7 = 7,
}

impl Predictor {
  fn as_u8(&self) -> u8 {
    *self as u8
  }
}

impl From<u8> for Predictor {
  fn from(v: u8) -> Self {
    match v {
      1 => Self::P1,
      2 => Self::P2,
      3 => Self::P3,
      4 => Self::P4,
      5 => Self::P5,
      6 => Self::P6,
      7 => Self::P7,
      mode => panic!("Invalid predictor mode: {}", mode),
    }
  }
}

#[derive(Debug, Error)]
pub enum CompressorError {

  #[error("Overflow error: {}", _0)]
  Overflow(String),

  #[error("I/O error")]
  Io(#[from] std::io::Error),
}

type Result<T> = std::result::Result<T, CompressorError>;

pub struct LjpegCompressor<'a> {

  image: &'a [u16],

  width: usize,

  height: usize,

  components: usize,

  bitdepth: u8,

  point_transform: u8,

  predictor: Predictor,

  padding: usize,

  comp_state: Vec<ComponentState>,

  cache: Vec<i16>,
}

#[derive(Debug, Default, Clone)]
struct HuffCode {
  enc: u16,
  bits: u16,
}

#[derive(Default, Debug)]
struct HuffTableBuilder {

  freq: [u64; Self::CLASSES + 1],

  codesize: [usize; Self::CLASSES + 1],

  others: [Option<usize>; Self::CLASSES + 1],

  bits: Vec<u8>,

  huffval: [Option<u8>; Self::CLASSES],

  huffcode: [HuffCode; Self::CLASSES + 1],

  huffsym: [Option<usize>; Self::CLASSES],
}

impl HuffTableBuilder {

  const CLASSES: usize = 17;

  fn new(histogram: [usize; Self::CLASSES]) -> Self {
    let mut ins = Self::default();
    ins.bits.resize(33, 0);
    for (i, f) in histogram.iter().enumerate() {
      ins.freq[i] = *f as u64;
    }
    ins.freq[Self::CLASSES] = 1;
    ins
  }

  fn gen_codesizes(&mut self) {
    loop {

      let mut v1freq: u64 = u64::MAX;
      let mut v2freq: u64 = u64::MAX;
      let mut v1: Option<usize> = None;
      let mut v2: Option<usize> = None;

      for (i, f) in self.freq.iter().enumerate().filter(|(_i, f)| **f > 0) {
        if *f <= v1freq {
          v1freq = *f;
          v1 = Some(i);
        }
      }

      for (i, f) in self.freq.iter().enumerate().filter(|(i, f)| **f > 0 && Some(*i) != v1) {
        if *f <= v2freq {
          v2freq = *f;
          v2 = Some(i);
        }
      }

      inspector!("V1: {:?}, V2: {:?}", v1, v2);

      match (&mut v1, &mut v2) {
        (Some(v1), Some(v2)) => {

          self.freq[*v1] += self.freq[*v2];
          self.freq[*v2] = 0;

          loop {
            self.codesize[*v1] += 1;
            if let Some(other) = self.others[*v1] {
              *v1 = other
            } else {
              break;
            }
          }
          self.others[*v1] = Some(*v2);
          loop {
            self.codesize[*v2] += 1;
            if let Some(other) = self.others[*v2] {
              *v2 = other;
            } else {
              break;
            }
          }
        }
        _ => {
          break;
        }
      }
    }
    #[cfg(feature = "inspector")]
    for (i, codesize) in self.codesize.iter().enumerate() {
      inspector!("codesize[{}]={}", i, codesize);
    }
  }

  fn count_bits(&mut self) {

    for i in 0..18 {
      if self.codesize[i] > 0 {
        self.bits[self.codesize[i] as usize] += 1;
      }
    }

    self.adjust_bits();

    #[cfg(feature = "inspector")]
    for (i, bit) in self.bits.iter().enumerate() {
      inspector!("bits[{}]={}", i, bit);
    }
  }

  fn sort_input(&mut self) {
    let mut k = 0;
    for i in 1..=32 {
      for j in 0..=16 {

        if self.codesize[j] == i {
          self.huffval[k] = Some(j as u8);
          k += 1;
        }
      }
    }
  }

  fn gen_size_table(&mut self) -> usize {
    let mut k = 0;
    let mut i = 1;

    while i <= 16 {
      let mut j = 1;
      while j <= self.bits[i] {
        self.huffcode[k].bits = i as u16;
        j += 1;
        k += 1;
      }
      i += 1;
    }
    self.huffcode[k].bits = 0;
    k
  }

  fn gen_code_table(&mut self) {
    let mut k = 0;
    let mut code = 0;
    let mut si = self.huffcode[0].bits;
    loop {
      loop {
        self.huffcode[k].enc = code;
        code += 1;
        k += 1;
        if self.huffcode[k].bits != si {
          break;
        }
      }
      if self.huffcode[k].bits == 0 {
        break;
      }
      loop {
        code <<= 1;
        si += 1;
        if self.huffcode[k].bits == si {
          break;
        }
      }
    }
  }

  fn order_codes(&mut self, _lastk: usize) {
    for (i, ssss) in self.huffval.iter().enumerate() {
      if let Some(ssss) = ssss {
        self.huffsym[*ssss as usize] = Some(i);
      }
    }
  }

  fn adjust_bits(&mut self) {
    let mut i = 32;

    while i > 16 {
      if self.bits[i] > 0 {
        let mut j = i - 2;
        while self.bits[j] == 0 {
          j -= 1;
        }
        self.bits[i] -= 2;
        self.bits[i - 1] += 1;
        self.bits[j + 1] += 2;
        self.bits[j] -= 1;
      } else {
        i -= 1;
      }
    }

    while self.bits[i] == 0 {
      i -= 1;
    }
    self.bits[i] -= 1;
  }

  fn build(mut self) -> [BitArray16; HuffTableBuilder::CLASSES] {
    inspector!("Start building table");
    self.gen_codesizes();
    self.count_bits();
    self.sort_input();
    let lastk = self.gen_size_table();
    self.gen_code_table();
    self.order_codes(lastk);
    let mut table = [BitArray16::default(); HuffTableBuilder::CLASSES];

    for ssss in 0..=16 {
      if let Some(code) = self.huffsym[ssss] {
        let enc = &self.huffcode[code];
        inspector!("ssss: {}, enc.bits: {}", ssss, enc.bits);
        table[ssss as usize] = BitArray16::from_lsb(enc.bits as usize, enc.enc);
      }
    }

    #[cfg(feature = "inspector")]
    {
      for (class, val) in self.huffcode.iter().enumerate() {
        inspector!("huffcode: idx: {}, bitlen: {}, {:b}", class, val.bits, val.enc);
      }

      for (class, val) in self.huffval.iter().enumerate() {
        inspector!("huffval: idx: {}, ssss: {:?}", class, val);
      }

      for (idx, bitlen) in self.bits.iter().enumerate() {
        inspector!("bits: codelen: {}, value_count: {}", idx, bitlen);
      }

      for (class, val) in self.huffsym.iter().enumerate() {
        inspector!("hufsym: ssss: {}, code_idx: {:?}", class, val);
      }
    }
    table
  }

  fn _generic_table(histogram: [usize; Self::CLASSES], _resolution: f32) -> [BitArray16; HuffTableBuilder::CLASSES] {
    let mut dist: Vec<(usize, usize)> = histogram.iter().enumerate().map(|(a, b)| (a, *b)).collect();
    dist.sort_by(|a, b| b.1.cmp(&a.1));
    #[cfg(feature = "inspector")]
    for (i, f) in &dist {
      inspector!("Freq: {}: {}, {}", i, f, *f as f32 / _resolution);
    }
    let mut table = [BitArray16::default(); HuffTableBuilder::CLASSES];
    table[dist[0].0] = BitArray16::from_lsb(2, 0b00);
    table[dist[1].0] = BitArray16::from_lsb(3, 0b010);
    table[dist[2].0] = BitArray16::from_lsb(3, 0b011);
    table[dist[3].0] = BitArray16::from_lsb(3, 0b100);
    table[dist[4].0] = BitArray16::from_lsb(3, 0b101);
    table[dist[5].0] = BitArray16::from_lsb(3, 0b110);
    table[dist[6].0] = BitArray16::from_lsb(4, 0b1110);
    table[dist[7].0] = BitArray16::from_lsb(5, 0b11110);
    table[dist[8].0] = BitArray16::from_lsb(6, 0b111110);
    table[dist[9].0] = BitArray16::from_lsb(7, 0b1111110);
    table[dist[10].0] = BitArray16::from_lsb(8, 0b11111110);
    table[dist[11].0] = BitArray16::from_lsb(9, 0b111111110);
    table[dist[12].0] = BitArray16::from_lsb(10, 0b1111111110);
    table[dist[13].0] = BitArray16::from_lsb(11, 0b11111111110);
    table[dist[14].0] = BitArray16::from_lsb(12, 0b111111111110);
    table[dist[15].0] = BitArray16::from_lsb(13, 0b1111111111110);
    table[dist[16].0] = BitArray16::from_lsb(14, 0b11111111111110);

    table
  }
}

#[derive(Default, Clone, Debug)]
struct ComponentState {

  histogram: [usize; 17],

  hufftable: [BitArray16; HuffTableBuilder::CLASSES],
}

pub struct BitstreamJPEG<'a> {
  inner: &'a mut dyn Write,
  next: u8,
  used: usize,
}

impl<'a> BitstreamJPEG<'a> {
  pub fn new(inner: &'a mut dyn Write) -> Self {
    Self { inner, next: 0, used: 0 }
  }

  pub fn write_bit(&mut self, value: bool) -> std::io::Result<()> {
    self.write(1, if value { 1 } else { 0 })
  }

  pub fn write(&mut self, mut bits: usize, value: u64) -> std::io::Result<()> {
    while bits > 0 {

      if self.used == 8 {
        self.internal_flush()?;
      }

      let free = 8 - self.used;

      let take = min(bits, free);

      let peek = ((value >> (bits - take)) & ((1 << take) - 1)) as u8;

      self.next |= peek << (free - take);

      bits -= take;
      self.used += take;
    }
    Ok(())
  }

  fn internal_flush(&mut self) -> std::io::Result<()> {
    self.inner.write_u8(self.next)?;
    if self.next == 0xFF {

      self.inner.write_u8(0x00)?;
    }
    self.used = 0;
    self.next = 0;
    Ok(())
  }

  pub fn flush(&mut self) -> std::io::Result<()> {
    if self.used > 0 {
      self.internal_flush()?;
    }
    Ok(())
  }
}

impl<'a> LjpegCompressor<'a> {

  pub fn new(
    image: &'a [u16],
    width: usize,
    height: usize,
    components: usize,
    bitdepth: u8,
    predictor: u8,
    point_transform: u8,
    padding: usize,
  ) -> Result<Self> {
    if !(1..=7).contains(&predictor) {
      return Err(CompressorError::Overflow(format!("Unsupported predictor: {}", predictor)));
    }
    if image.len() < height * ((width + padding) * components) {
      return Err(CompressorError::Overflow(
        "Image input buffer is not large enough for given dimensions".to_string(),
      ));
    }
    if !(2..=16).contains(&bitdepth) {
      return Err(CompressorError::Overflow(format!(
        "Overflow for bit depth {}, only 2 >= bp <= 16 is supported",
        bitdepth
      )));
    }
    if height > 65_535 {
      return Err(CompressorError::Overflow(format!(
        "Overflow for height {}, only h <= 65.535 is supported",
        height
      )));
    }
    if width > 65_535 {
      return Err(CompressorError::Overflow(format!(
        "Overflow for width {}, only h <= 65.535 is supported",
        width
      )));
    }
    Ok(Self {
      image,
      width,
      height,
      components,
      bitdepth,
      point_transform,
      predictor: Predictor::from(predictor),
      padding,
      comp_state: vec![ComponentState::default(); components],
      cache: Vec::default(),
    })
  }

  fn component_range(&self) -> std::ops::Range<usize> {
    0..self.components
  }

  pub fn encode(mut self) -> Result<Vec<u8>> {
    let mut encoded = Cursor::new(Vec::with_capacity(self.resolution() * self.components));
    self.scan_frequency()?;
    for comp in self.component_range() {
      self.build_hufftable(comp);

    }

    self.write_header(&mut encoded)?;
    self.write_body(&mut encoded)?;
    self.write_post(&mut encoded)?;
    Ok(encoded.into_inner())
  }

  #[inline(always)]
  fn resolution(&self) -> usize {
    self.height * self.width
  }

  fn scan_frequency(&mut self) -> Result<()> {
    let mut cache = vec![0; self.resolution() * self.components];

    let rowsize = self.width * self.components;
    let linesize = (self.width + self.padding) * self.components;
    let mut row_prev = &self.image[0..];
    let mut row_curr = &self.image[0..];
    let mut diffs = vec![0_i16; linesize];

    macro_rules! match_predictor {
      ($comp:expr, $pred:expr) => {
        match $pred {
          Predictor::P1 => ljpeg92_diff::<$comp, 1>(row_prev, row_curr, &mut diffs, linesize, self.point_transform, self.bitdepth),
          Predictor::P2 => ljpeg92_diff::<$comp, 2>(row_prev, row_curr, &mut diffs, linesize, self.point_transform, self.bitdepth),
          Predictor::P3 => ljpeg92_diff::<$comp, 3>(row_prev, row_curr, &mut diffs, linesize, self.point_transform, self.bitdepth),
          Predictor::P4 => ljpeg92_diff::<$comp, 4>(row_prev, row_curr, &mut diffs, linesize, self.point_transform, self.bitdepth),
          Predictor::P5 => ljpeg92_diff::<$comp, 5>(row_prev, row_curr, &mut diffs, linesize, self.point_transform, self.bitdepth),
          Predictor::P6 => ljpeg92_diff::<$comp, 6>(row_prev, row_curr, &mut diffs, linesize, self.point_transform, self.bitdepth),
          Predictor::P7 => ljpeg92_diff::<$comp, 7>(row_prev, row_curr, &mut diffs, linesize, self.point_transform, self.bitdepth),
        }
      };
    }

    for row in 0..self.height {
      match self.components {
        1 => match_predictor!(1, self.predictor),
        2 => match_predictor!(2, self.predictor),
        3 => match_predictor!(3, self.predictor),
        4 => match_predictor!(4, self.predictor),
        _ => unreachable!(),
      }

      cache[row * rowsize..row * rowsize + rowsize].copy_from_slice(&diffs[..rowsize]);

      for (i, diff) in diffs.iter().take(rowsize).enumerate() {
        let comp = i % self.components;
        let ssss = lookup_ssss(*diff);
        self.comp_state[comp].histogram[ssss as usize] += 1;
      }

      row_prev = row_curr;
      row_curr = &row_curr[linesize..];
    }

    #[cfg(feature = "inspector")]
    for comp in 0..self.components {
      inspector!("Huffman table {}", comp);
      for i in 0..17 {
        inspector!("scan: self.huffman[{}].hist[{}]={}", comp, i, self.comp_state[comp].histogram[i]);
      }
    }
    self.cache = cache;
    Ok(())
  }

  fn build_hufftable(&mut self, comp: usize) {
    #[cfg(feature = "inspector")]
    {
      let distibution: Vec<(u16, usize)> = self.comp_state[comp].histogram.iter().enumerate().map(|(a, b)| (a as u16, b.clone())).collect();
      for (i, f) in &distibution {
        inspector!("unsorted freq: {}: {}, {}", i, f, *f as f32 / (self.resolution() as f32));
      }
    }
    let huffgen = HuffTableBuilder::new(self.comp_state[comp].histogram);
    let table = huffgen.build();

    #[cfg(feature = "inspector")]
    for (i, code) in table.iter().enumerate() {
      inspector!("table[{}]={}", i, code);
    }
    self.comp_state[comp].hufftable = table;
  }

  fn write_header(&mut self, encoded: &mut dyn Write) -> Result<()> {
    encoded.write_u16::<BigEndian>(0xffd8)?;
    encoded.write_u16::<BigEndian>(0xffc3)?;

    encoded.write_u16::<BigEndian>(2 + 6 + self.components as u16 * 3)?;
    encoded.write_u8(self.bitdepth)?;
    encoded.write_u16::<BigEndian>(self.height as u16)?;
    encoded.write_u16::<BigEndian>(self.width as u16)?;

    encoded.write_u8(self.components as u8)?;
    for c in self.component_range() {
      encoded.write_u8(c as u8)?;
      encoded.write_u8(0x11)?;
      encoded.write_u8(0)?;
    }

    for comp in self.component_range() {

      encoded.write_u16::<BigEndian>(0xffc4)?;

      let bit_sum: u16 = self.comp_state[comp].hufftable.iter().filter(|e| !e.is_empty()).count() as u16;
      inspector!("Bitsum: {}", bit_sum);

      encoded.write_u16::<BigEndian>(2 + (1 + 16) + bit_sum)?;
      encoded.write_u8(comp as u8)?;

      for bit_len in 1..=16 {
        let count = self.comp_state[comp].hufftable.iter().filter(|entry| entry.len() == bit_len).count();
        inspector!("COUNT: {}={}", bit_len, count);
        encoded.write_u8(count as u8)?;
      }

      for bit_len in 1..=16 {
        let mut codes: Vec<(u16, BitArray16)> = self.comp_state[comp]
          .hufftable
          .iter()
          .enumerate()
          .filter(|(_, code)| code.len() == bit_len)
          .map(|(ssss, code)| (ssss as u16, *code))
          .collect();
        codes.sort_by(|a, b| a.1.cmp(&b.1));
        for (ssss, _) in codes.iter() {
          encoded.write_u8(*ssss as u8)?;
          inspector!("VAL: {}", ssss);
        }
      }
    }

    encoded.write_u16::<BigEndian>(0xffda)?;
    encoded.write_u16::<BigEndian>(0x0006 + (self.components as u16 * 2))?;
    encoded.write_u8(self.components as u8)?;
    for c in self.component_range() {
      encoded.write_u8(c as u8)?;
      encoded.write_u8((c as u8) << 4)?;
    }
    encoded.write_u8(self.predictor.as_u8())?;
    encoded.write_u8(0)?;
    debug_assert!(self.point_transform <= 15);
    encoded.write_u8(0x00 | (self.point_transform & 0xF))?;
    Ok(())
  }

  fn write_post(&mut self, encoded: &mut dyn Write) -> Result<()> {
    encoded.write_u16::<BigEndian>(0xffd9)?;
    Ok(())
  }

  fn write_body(&mut self, encoded: &mut dyn Write) -> Result<()> {
    let mut bitstream = BitstreamJPEG::new(encoded);
    for (i, diff) in self.cache.iter().enumerate() {
      let comp = i % self.components;
      let ssss = lookup_ssss(*diff);
      let enc = self.comp_state[comp].hufftable[ssss as usize];
      let (bits, value) = (enc.len(), enc.get_lsb() as u64);
      debug_assert!(bits > 0);
      bitstream.write(bits, value)?;

      debug_assert!(ssss <= 16);
      if (ssss & 15) != 0 {

        let diff = if *diff < 0 { *diff as i32 - 1 } else { *diff as i32 };
        bitstream.write(ssss as usize, (diff & (0x0FFFF >> (16 - ssss))) as u64)?;
      }
    }

    bitstream.flush()?;
    Ok(())
  }
}

#[multiversion(targets("x86_64+avx+avx2+fma+bmi1+bmi2", "x86_64+avx+avx2", "x86+sse", "aarch64+neon"))]

fn ljpeg92_diff<const NCOMP: usize, const PX: u8>(
  row_prev: &[u16],
  row_curr: &[u16],
  diffs: &mut [i16],
  linesize: usize,
  point_transform: u8,
  bitdepth: u8,
) {
  debug_assert_eq!(linesize % NCOMP, 0);
  let pixels = linesize / NCOMP;
  let samplecnt = pixels * NCOMP;
  let row_prev = &row_prev[..samplecnt];
  let row_curr = &row_curr[..samplecnt];
  let diffs = &mut diffs[..samplecnt];

  #[cfg(debug_assertions)]
  row_curr.iter().for_each(|sample| {
    let max_value = ((1u32 << (bitdepth - point_transform)) - 1) as u16;
    if (*sample >> point_transform) > max_value {
      panic!("Sample overflow, sample is {:#x} but max value is {:#x}", sample, max_value);
    }
  });

  if row_curr.as_ptr() == row_prev.as_ptr() {
    for comp in 0..NCOMP {
      let px = (1u16 << (bitdepth - point_transform - 1)) as i32;
      let sample = pred_x::<NCOMP>(row_prev, row_curr, comp, point_transform);
      diffs[0 + comp] = (sample - px) as i16;
    }

    for idx in NCOMP..samplecnt {
      let px = pred_a::<NCOMP>(row_prev, row_curr, idx, point_transform);
      let sample = pred_x::<NCOMP>(row_prev, row_curr, idx, point_transform);
      diffs[idx] = (sample - px) as i16;
    }
  } else {

    for comp in 0..NCOMP {
      let px = pred_b::<NCOMP>(row_prev, row_curr, 0 + comp, point_transform);
      let sample = pred_x::<NCOMP>(row_prev, row_curr, comp, point_transform);
      diffs[0 + comp] = (sample - px) as i16;
    }
    let predictor = match PX {
      1 => pred_a::<NCOMP>,
      2 => pred_b::<NCOMP>,
      3 => pred_c::<NCOMP>,
      4 => |prev: &[u16], curr: &[u16], idx: usize, pt: u8| -> i32 {
        let ra = pred_a::<NCOMP>(prev, curr, idx, pt);
        let rb = pred_b::<NCOMP>(prev, curr, idx, pt);
        let rc = pred_c::<NCOMP>(prev, curr, idx, pt);
        ra + rb - rc
      },
      5 => |prev: &[u16], curr: &[u16], idx: usize, pt: u8| -> i32 {
        let ra = pred_a::<NCOMP>(prev, curr, idx, pt);
        let rb = pred_b::<NCOMP>(prev, curr, idx, pt);
        let rc = pred_c::<NCOMP>(prev, curr, idx, pt);
        ra + ((rb - rc) >> 1)
      },
      6 => |prev: &[u16], curr: &[u16], idx: usize, pt: u8| -> i32 {
        let ra = pred_a::<NCOMP>(prev, curr, idx, pt);
        let rb = pred_b::<NCOMP>(prev, curr, idx, pt);
        let rc = pred_c::<NCOMP>(prev, curr, idx, pt);
        rb + ((ra - rc) >> 1)
      },
      7 => |prev: &[u16], curr: &[u16], idx: usize, pt: u8| -> i32 {
        let ra = pred_a::<NCOMP>(prev, curr, idx, pt);
        let rb = pred_b::<NCOMP>(prev, curr, idx, pt);
        (ra + rb) >> 1
      },

      _ => unreachable!(),
    };

    for idx in NCOMP..samplecnt {
      let px = predictor(row_prev, row_curr, idx, point_transform);
      let sample = pred_x::<NCOMP>(row_prev, row_curr, idx, point_transform);

      diffs[idx] = (sample - px) as i16;
    }
  }
}

#[inline(always)]
fn pred_x<const NCOMP: usize>(_prev: &[u16], curr: &[u16], idx: usize, point_transform: u8) -> i32 {
  unsafe { (curr.get_unchecked(idx) >> point_transform) as i32 }
}

#[inline(always)]
fn pred_a<const NCOMP: usize>(_prev: &[u16], curr: &[u16], idx: usize, point_transform: u8) -> i32 {
  unsafe { (curr.get_unchecked(idx - NCOMP) >> point_transform) as i32 }
}

#[inline(always)]
fn pred_b<const NCOMP: usize>(prev: &[u16], _curr: &[u16], idx: usize, point_transform: u8) -> i32 {
  unsafe { (prev.get_unchecked(idx) >> point_transform) as i32 }
}

#[inline(always)]
fn pred_c<const NCOMP: usize>(prev: &[u16], _curr: &[u16], idx: usize, point_transform: u8) -> i32 {
  unsafe { (prev.get_unchecked(idx - NCOMP) >> point_transform) as i32 }
}

#[cfg(test)]
mod tests {
  use super::*;

  use crate::decompressors::ljpeg::LjpegDecompressor;

  #[test]
  fn bitstream_test1() -> std::result::Result<(), Box<dyn std::error::Error>> {
    crate::init_test_logger();
    let mut buf = Vec::new();
    let mut bs = BitstreamJPEG::new(&mut buf);
    bs.write(1, 0b1)?;
    bs.flush()?;
    bs.write(1, 0b0)?;
    bs.write(3, 0b101)?;
    bs.write(4, 0b11111101)?;
    bs.write(2, 0b101)?;
    bs.flush()?;
    bs.write(16, 0b1111111111111111)?;
    bs.flush()?;
    bs.write(16, 0b0)?;
    bs.flush()?;
    assert_eq!(buf[0], 0b10000000);
    assert_eq!(buf[1], 0b01011101);
    assert_eq!(buf[2], 0b01000000);
    assert_eq!(buf[3], 0xFF);
    assert_eq!(buf[4], 0x00);
    assert_eq!(buf[5], 0xFF);
    assert_eq!(buf[6], 0x00);
    assert_eq!(buf[7], 0x00);
    Ok(())
  }

  #[test]
  fn encode_16x16_16bit_black_decode_single() -> std::result::Result<(), Box<dyn std::error::Error>> {
    crate::init_test_logger();
    let h = 16;
    let w = 16;
    let c = 1;
    let input_image = vec![0x0000; w * h * c];
    let enc = LjpegCompressor::new(&input_image, w, h, c, 16, 1, 0, 0)?;
    let result = enc.encode();
    assert!(result.is_ok());
    let jpeg = result?;
    let dec = LjpegDecompressor::new(&jpeg)?;
    let mut outbuf = vec![0; h * w];
    dec.decode(&mut outbuf, 0, w * c, w * c, h, false)?;
    assert_eq!(outbuf[0], input_image[0]);
    assert_eq!(outbuf[1], input_image[1]);
    for i in 0..outbuf.len() {
      assert_eq!(outbuf[i], input_image[i]);
    }
    Ok(())
  }

  #[test]
  fn encode_16x16_16bit_black_decode_2component() -> std::result::Result<(), Box<dyn std::error::Error>> {
    crate::init_test_logger();
    let h = 2;
    let w = 2;
    let c = 2;
    let input_image = vec![0x0000; w * h * c];
    let enc = LjpegCompressor::new(&input_image, w, h, c, 16, 1, 0, 0)?;
    let result = enc.encode();
    assert!(result.is_ok());
    let jpeg = result?;
    let dec = LjpegDecompressor::new_full(&jpeg, false, false)?;
    let mut outbuf = vec![0; w * h * c];
    dec.decode(&mut outbuf, 0, w * c, w * c, h, false)?;
    assert_eq!(outbuf[0], input_image[0]);
    assert_eq!(outbuf[1], input_image[1]);
    for i in 0..outbuf.len() {
      assert_eq!(outbuf[i], input_image[i]);
    }
    Ok(())
  }

  #[test]
  fn encode_16x16_16bit_black() -> std::result::Result<(), Box<dyn std::error::Error>> {
    crate::init_test_logger();
    let h = 16;
    let w = 16;
    let c = 1;
    let input_image = vec![0x0000; w * h * c];
    let enc = LjpegCompressor::new(&input_image, w, h, c, 16, 1, 0, 0)?;
    let result = enc.encode();
    assert!(result.is_ok());
    Ok(())
  }

  #[test]
  fn encode_16x16_16bit_white() -> std::result::Result<(), Box<dyn std::error::Error>> {
    crate::init_test_logger();
    let h = 16;
    let w = 16;
    let c = 1;
    let input_image = vec![0xffff; w * h * c];
    let enc = LjpegCompressor::new(&input_image, w, h, c, 16, 1, 0, 0)?;
    let result = enc.encode();
    assert!(result.is_ok());
    Ok(())
  }

  #[cfg(debug_assertions)]
  #[test]
  #[should_panic(expected = "Sample overflow, sample is 0xfffe but max value is 0x3fff")]
  fn encode_16x16_bitdepth_error() {
    crate::init_test_logger();
    let h = 16;
    let w = 16;
    let c = 1;
    let input_image = vec![0xfffe; w * h * c];
    let enc = LjpegCompressor::new(&input_image, w, h, c, 14, 1, 0, 0).expect("Compressor failed");
    let _ = enc.encode();
  }

  #[test]
  fn encode_16x16_short_buffer_error() -> std::result::Result<(), Box<dyn std::error::Error>> {
    crate::init_test_logger();
    let h = 16;
    let w = 16;
    let c = 1;
    let input_image = vec![0x9999_u16; (w * h * c) - 1];
    let result = LjpegCompressor::new(&input_image, w, h, c, 16, 1, 0, 0);
    assert!(result.is_err());
    Ok(())
  }

  #[test]
  fn encode_16x16_16bit_incr() -> std::result::Result<(), Box<dyn std::error::Error>> {
    crate::init_test_logger();
    let h = 16;
    let w = 16;
    let c = 2;
    let mut input_image = vec![0; h * w * c];
    for i in 0..input_image.len() {
      input_image[i] = i as u16;
    }
    let enc = LjpegCompressor::new(&input_image, w, h, c, 16, 1, 0, 0)?;
    let result = enc.encode();
    assert!(result.is_ok());
    let jpeg = result?;
    let dec = LjpegDecompressor::new_full(&jpeg, false, false)?;
    let mut outbuf = vec![0; h * w * c];
    dec.decode(&mut outbuf, 0, w * c, w * c, h, false)?;

    for i in 0..outbuf.len() {
      assert_eq!(outbuf[i], input_image[i]);
    }
    Ok(())
  }

  #[test]
  fn encode_all_differences() -> std::result::Result<(), Box<dyn std::error::Error>> {
    crate::init_test_logger();

    let input_image = vec![
      0, 0, 1, 0, 2, 0, 4, 0, 8, 0, 16, 0, 32, 0, 64, 0, 128, 0, 256, 0, 512, 0, 1024, 0, 2048, 0, 4096, 0, 8192, 0, 16384, 0, 32768,
    ];
    let h = 1;
    let w = input_image.len();
    let c = 1;

    let enc = LjpegCompressor::new(&input_image, w, h, c, 16, 1, 0, 0)?;
    let result = enc.encode();
    assert!(result.is_ok());
    let jpeg = result?;
    let dec = LjpegDecompressor::new_full(&jpeg, false, false)?;
    let mut outbuf = vec![0; h * w * c];
    dec.decode(&mut outbuf, 0, w * c, w * c, h, false)?;
    for i in 0..outbuf.len() {
      assert_eq!(outbuf[i], input_image[i]);
    }
    Ok(())
  }

  #[test]
  fn encode_ssss_16() -> std::result::Result<(), Box<dyn std::error::Error>> {
    crate::init_test_logger();

    let input_image = vec![0, 0, 0, 32768];
    let h = 1;
    let w = input_image.len();
    let c = 1;

    let enc = LjpegCompressor::new(&input_image, w, h, c, 16, 1, 0, 0)?;
    let result = enc.encode();
    assert!(result.is_ok());
    let jpeg = result?;
    let dec = LjpegDecompressor::new_full(&jpeg, false, false)?;
    let mut outbuf = vec![0; h * w * c];
    dec.decode(&mut outbuf, 0, w * c, w * c, h, false)?;
    for i in 0..outbuf.len() {
      assert_eq!(outbuf[i], input_image[i]);
    }
    Ok(())
  }

  #[test]
  fn encode_difference_above_32768() -> std::result::Result<(), Box<dyn std::error::Error>> {
    crate::init_test_logger();

    let input_image = vec![0, 0, 0, 32768 + 1, 0, 0, 0, u16::MAX, u16::MAX, 1, u16::MAX, 1, 0];
    let h = 1;
    let w = input_image.len();
    let c = 1;

    let enc = LjpegCompressor::new(&input_image, w, h, c, 16, 1, 0, 0)?;
    let result = enc.encode();
    assert!(result.is_ok());
    let jpeg = result?;
    let dec = LjpegDecompressor::new_full(&jpeg, false, false)?;
    let mut outbuf = vec![0; h * w * c];
    dec.decode(&mut outbuf, 0, w * c, w * c, h, false)?;

    for i in 0..outbuf.len() {
      assert_eq!(outbuf[i], input_image[i]);
    }
    Ok(())
  }

  #[test]
  fn encode_predictor2_1comp() -> std::result::Result<(), Box<dyn std::error::Error>> {
    crate::init_test_logger();
    let input_image = vec![0, 0, u16::MAX, 0, 0, 0, u16::MAX - 5, 0];
    let h = 2;
    let w = 4;
    let c = 1;

    let enc = LjpegCompressor::new(&input_image, w, h, c, 16, 2, 0, 0)?;
    let result = enc.encode();
    assert!(result.is_ok());
    let jpeg = result?;

    let dec = LjpegDecompressor::new_full(&jpeg, false, false)?;
    let mut outbuf = vec![0; h * w * c];
    dec.decode(&mut outbuf, 0, w * c, w * c, h, false)?;

    for i in 0..outbuf.len() {
      assert_eq!(outbuf[i], input_image[i]);
    }

    Ok(())
  }

  #[test]
  fn encode_predictor1_2comp_padding() -> std::result::Result<(), Box<dyn std::error::Error>> {
    crate::init_test_logger();
    let input_image = vec![1, 2, 5, 6, 3, 7, 0, 0, 7, 8, 3, 4, 6, 2, 0, 0];
    let expected_image = [1, 2, 5, 6, 3, 7, 7, 8, 3, 4, 6, 2];
    let h = 2;
    let w = 3;
    let c = 2;
    let padding = 1;

    let enc = LjpegCompressor::new(&input_image, w, h, c, 16, 1, 0, padding)?;
    let result = enc.encode();
    assert!(result.is_ok());
    let jpeg = result?;

    let dec = LjpegDecompressor::new_full(&jpeg, false, false)?;
    let mut outbuf = vec![0; h * w * c];
    dec.decode(&mut outbuf, 0, w * c, w * c, h, false)?;

    for i in 0..outbuf.len() {
      assert_eq!(outbuf[i], expected_image[i]);
    }

    Ok(())
  }

  #[test]
  fn encode_predictor3_1comp() -> std::result::Result<(), Box<dyn std::error::Error>> {
    crate::init_test_logger();
    let input_image = vec![0, u16::MAX, 0, 0, 0, 0, u16::MAX - 5, 0];
    let h = 2;
    let w = 4;
    let c = 1;

    let enc = LjpegCompressor::new(&input_image, w, h, c, 16, 3, 0, 0)?;
    let result = enc.encode();
    assert!(result.is_ok());
    let jpeg = result?;

    let dec = LjpegDecompressor::new_full(&jpeg, false, false)?;
    let mut outbuf = vec![0; h * w * c];
    dec.decode(&mut outbuf, 0, w * c, w * c, h, false)?;

    for i in 0..outbuf.len() {
      assert_eq!(outbuf[i], input_image[i]);
    }

    Ok(())
  }

  #[test]
  fn encode_predictor4_1comp() -> std::result::Result<(), Box<dyn std::error::Error>> {
    crate::init_test_logger();
    let input_image = vec![0, 0, u16::MAX, 0, 0, u16::MAX, 0, 0];
    let h = 2;
    let w = 4;
    let c = 1;

    let enc = LjpegCompressor::new(&input_image, w, h, c, 16, 4, 0, 0)?;
    let result = enc.encode();
    assert!(result.is_ok());
    let jpeg = result?;

    let dec = LjpegDecompressor::new_full(&jpeg, false, false)?;
    let mut outbuf = vec![0; h * w * c];
    dec.decode(&mut outbuf, 0, w * c, w * c, h, false)?;

    for i in 0..outbuf.len() {
      assert_eq!(outbuf[i], input_image[i]);
    }

    Ok(())
  }

  #[test]
  fn encode_predictor5_1comp() -> std::result::Result<(), Box<dyn std::error::Error>> {
    crate::init_test_logger();
    let input_image = vec![0, u16::MAX, u16::MAX, 0, 0, u16::MAX, 0, 0];
    let h = 2;
    let w = 4;
    let c = 1;

    let enc = LjpegCompressor::new(&input_image, w, h, c, 16, 5, 0, 0)?;
    let result = enc.encode();
    assert!(result.is_ok());
    let jpeg = result?;

    let dec = LjpegDecompressor::new_full(&jpeg, false, false)?;
    let mut outbuf = vec![0; h * w * c];
    dec.decode(&mut outbuf, 0, w * c, w * c, h, false)?;

    for i in 0..outbuf.len() {
      assert_eq!(outbuf[i], input_image[i]);
    }

    Ok(())
  }

  #[test]
  fn encode_predictor6_1comp() -> std::result::Result<(), Box<dyn std::error::Error>> {
    crate::init_test_logger();
    let input_image = vec![0, u16::MAX, u16::MAX, 0, 0, u16::MAX, 0, 0];
    let h = 2;
    let w = 4;
    let c = 1;

    let enc = LjpegCompressor::new(&input_image, w, h, c, 16, 6, 0, 0)?;
    let result = enc.encode();
    assert!(result.is_ok());
    let jpeg = result?;

    let dec = LjpegDecompressor::new_full(&jpeg, false, false)?;
    let mut outbuf = vec![0; h * w * c];
    dec.decode(&mut outbuf, 0, w * c, w * c, h, false)?;

    for i in 0..outbuf.len() {
      assert_eq!(outbuf[i], input_image[i]);
    }

    Ok(())
  }

  #[test]
  fn encode_predictor6_3comp() -> std::result::Result<(), Box<dyn std::error::Error>> {
    crate::init_test_logger();
    let input_image = vec![56543, 45, 65000, 0, 0, 35632];
    let h = 2;
    let w = 1;
    let c = 3;

    let enc = LjpegCompressor::new(&input_image, w, h, c, 16, 6, 0, 0)?;
    let result = enc.encode();
    assert!(result.is_ok());
    let jpeg = result?;

    let dec = LjpegDecompressor::new_full(&jpeg, false, false)?;
    let mut outbuf = vec![0; h * w * c];
    dec.decode(&mut outbuf, 0, w * c, w * c, h, false)?;

    for i in 0..outbuf.len() {
      assert_eq!(outbuf[i], input_image[i]);
    }

    Ok(())
  }

  #[test]
  fn encode_predictor6_3comp_ssss16() -> std::result::Result<(), Box<dyn std::error::Error>> {
    crate::init_test_logger();
    let input_image = vec![0, 0, 0, 0, 0, 0, 0, 0, 0, 32768, 32768, 32768, 0, 0, 0, 0, 0, 0];
    let h = 3;
    let w = 2;
    let c = 3;

    let enc = LjpegCompressor::new(&input_image, w, h, c, 16, 6, 0, 0)?;
    let result = enc.encode();
    assert!(result.is_ok());
    let jpeg = result?;

    let dec = LjpegDecompressor::new_full(&jpeg, false, false)?;
    let mut outbuf = vec![0; h * w * c];
    dec.decode(&mut outbuf, 0, w * c, w * c, h, false)?;

    for i in 0..outbuf.len() {
      assert_eq!(outbuf[i], input_image[i]);
    }

    Ok(())
  }

  #[test]
  fn encode_predictor7_1comp() -> std::result::Result<(), Box<dyn std::error::Error>> {
    crate::init_test_logger();
    let input_image = vec![0, 0, u16::MAX, 0, 0, u16::MAX, 0, 0];
    let h = 2;
    let w = 4;
    let c = 1;

    let enc = LjpegCompressor::new(&input_image, w, h, c, 16, 7, 0, 0)?;
    let result = enc.encode();
    assert!(result.is_ok());
    let jpeg = result?;

    let dec = LjpegDecompressor::new_full(&jpeg, false, false)?;
    let mut outbuf = vec![0; h * w * c];
    dec.decode(&mut outbuf, 0, w * c, w * c, h, false)?;

    for i in 0..outbuf.len() {
      assert_eq!(outbuf[i], input_image[i]);
    }

    Ok(())
  }

  #[test]
  fn encode_predictor1_ljpeg_width_larger_than_output() -> std::result::Result<(), Box<dyn std::error::Error>> {
    crate::init_test_logger();
    let input_image = vec![100, 105, 200, 207, 50, 48, 34, 45, 50, 45, 23, 100, 34, 76, 23, 99];
    let expected_output = vec![100, 105, 200, 207, 50, 45, 23, 100];
    let h = 2;
    let w = 4;
    let c = 2;

    let enc = LjpegCompressor::new(&input_image, w, h, c, 16, 1, 0, 0)?;
    let result = enc.encode();
    assert!(result.is_ok());
    let jpeg = result?;

    let w = w / 2;
    let dec = LjpegDecompressor::new_full(&jpeg, false, false)?;
    let mut outbuf = vec![0; h * w * c];
    dec.decode(&mut outbuf, 0, w * c, w * c, h, false)?;

    assert_eq!(outbuf, expected_output);

    Ok(())
  }

  #[test]
  fn encode_predictor4_trigger_minus1_prediction() -> std::result::Result<(), Box<dyn std::error::Error>> {
    crate::init_test_logger();
    let input_image = vec![0, 0, 5, 2, 0, 0, 0, 0, 2, 9, 0, 0];
    let h = 2;
    let w = 6;
    let c = 1;

    let enc = LjpegCompressor::new(&input_image, w, h, c, 16, 4, 0, 0)?;
    let result = enc.encode();
    assert!(result.is_ok());
    let jpeg = result?;

    let dec = LjpegDecompressor::new_full(&jpeg, false, false)?;
    let mut outbuf = vec![0; h * w * c];
    dec.decode(&mut outbuf, 0, w * c, w * c, h, false)?;

    for i in 0..outbuf.len() {
      assert_eq!(outbuf[i], input_image[i]);
    }

    Ok(())
  }

  #[test]
  fn encode_predictor5_trigger_minus1_prediction() -> std::result::Result<(), Box<dyn std::error::Error>> {
    crate::init_test_logger();
    let input_image = vec![0, 0, 2, 1, 0, 0, 1, 9];
    let h = 2;
    let w = 4;
    let c = 1;

    let enc = LjpegCompressor::new(&input_image, w, h, c, 16, 5, 0, 0)?;
    let result = enc.encode();
    assert!(result.is_ok());
    let jpeg = result?;

    let dec = LjpegDecompressor::new_full(&jpeg, false, false)?;
    let mut outbuf = vec![0; h * w * c];
    dec.decode(&mut outbuf, 0, w * c, w * c, h, false)?;

    for i in 0..outbuf.len() {
      assert_eq!(outbuf[i], input_image[i]);
    }

    Ok(())
  }
}
