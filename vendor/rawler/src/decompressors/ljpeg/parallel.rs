use super::huffman::HuffTable;
use crate::pumps::BitPumpMSB;
use rayon::prelude::*;

const RECORD: usize = 1024;

const MIN_PART: usize = 512 * 1024;

const MAX_PARTS: usize = 4;

pub(crate) fn pool() -> Option<&'static rayon::ThreadPool> {
  static POOL: std::sync::OnceLock<Option<rayon::ThreadPool>> = std::sync::OnceLock::new();
  POOL
    .get_or_init(|| {
      let threads = std::thread::available_parallelism().map_or(1, |n| n.get()).min(MAX_PARTS);
      (threads > 1)
        .then(|| rayon::ThreadPoolBuilder::new().num_threads(threads).thread_name(|k| format!("rawler-{k}")).build().ok())
        .flatten()
    })
    .as_ref()
}

pub(crate) struct Diffs {
  parts: Vec<(Vec<i16>, usize, usize)>,
}

impl Diffs {

  pub(crate) fn pieces(&self, mut start: usize, mut len: usize) -> impl Iterator<Item = &[i16]> {
    self.parts.iter().filter_map(move |(diffs, from, to)| {
      let run = to - from;
      if len == 0 {
        return None;
      }
      if start >= run {
        start -= run;
        return None;
      }
      let take = len.min(run - start);
      let piece = &diffs[from + start..from + start + take];
      start = 0;
      len -= take;
      Some(piece)
    })
  }
}

struct Part<'a> {
  pump: BitPumpMSB<'a>,

  offset: usize,
  diffs: Vec<i16>,

  noted: Vec<usize>,

  join: Option<(usize, usize)>,

  stuck: bool,
}

impl<'a> Part<'a> {
  #[inline(always)]
  fn at(&self) -> usize {
    self.offset + self.pump.bit_pos()
  }

  #[inline(always)]
  fn read_to(&mut self, table: &HuffTable, next: usize, limit: usize) -> bool {
    let mut at = self.at();
    while at < next {
      if self.diffs.len() >= limit {
        return false;
      }
      let diff = table.huff_decode(&mut self.pump).unwrap_or(0);
      self.diffs.push(diff as i16);
      let now = self.at();
      if now == at {
        return false;
      }
      at = now;
    }
    true
  }
}

pub(crate) fn decode(bits: &[u8], end: usize, table: &HuffTable, total: usize) -> Option<Diffs> {
  let count = rayon::current_num_threads().min(end / MIN_PART).min(MAX_PARTS);
  debug_assert!(rayon::current_thread_index().is_some(), "read a stream inside pool()");
  if count < 2 || total == 0 {
    return None;
  }

  let share = total / count + 1;
  let limit = share * 2 + RECORD;

  let mut parts: Vec<Part> = (0..count)
    .map(|k| {
      let first = end * k / count;
      Part {
        pump: BitPumpMSB::new_zero_padded(&bits[first.min(bits.len())..]),
        offset: first * 8,
        diffs: Vec::with_capacity(share + share / 4 + RECORD),
        noted: Vec::with_capacity(RECORD),
        join: None,
        stuck: false,
      }
    })
    .collect();

  parts.par_iter_mut().skip(1).for_each(|part| {
    for _ in 0..RECORD {
      let at = part.at();
      part.noted.push(at);
      if !part.read_to(table, at + 1, RECORD) {
        part.stuck = true;
        return;
      }
    }
  });
  if parts.iter().any(|part| part.stuck) {
    return None;
  }

  let starts: Vec<(usize, Vec<usize>)> = parts.iter().map(|part| (part.offset, part.noted.clone())).collect();
  parts.par_iter_mut().enumerate().for_each(|(k, part)| {
    let last = k + 1 == count;
    let (next, noted) = match starts.get(k + 1) {
      Some((next, noted)) => (*next, noted.as_slice()),
      None => (end * 8, &[][..]),
    };

    if !part.read_to(table, next, limit) || last {
      part.stuck = !last;
      return;
    }
    let mut i = 0;
    loop {
      let at = part.at();
      while i < noted.len() && noted[i] < at {
        i += 1;
      }
      if i == noted.len() {

        part.stuck = true;
        return;
      }
      if noted[i] == at {
        part.join = Some((part.diffs.len(), i));
        return;
      }
      if !part.read_to(table, at + 1, limit) {
        part.stuck = true;
        return;
      }
    }
  });
  if parts.iter().any(|part| part.stuck) {
    return None;
  }

  let mut runs = Vec::with_capacity(count);
  let mut from = 0;
  let mut have = 0;
  let mut last = None;
  for (k, part) in parts.into_iter().enumerate() {
    let to = match part.join {
      Some((to, next_from)) => {
        if to < from {
          return None;
        }
        let run = (part.diffs, from, to);
        from = next_from;
        run
      }
      None if k + 1 == count => {
        last = Some(part.pump);
        let to = part.diffs.len();
        if to < from {
          return None;
        }
        (part.diffs, from, to)
      }
      None => return None,
    };
    have += to.2 - to.1;
    runs.push(to);
  }

  let (diffs, from, to) = runs.last_mut()?;
  if have < total {
    let mut pump = last?;
    diffs.truncate(*to);
    while have < total {
      diffs.push(table.huff_decode(&mut pump).unwrap_or(0) as i16);
      have += 1;
    }
    *to = diffs.len();
  } else {

    let over = have - total;
    if over > *to - *from {
      return None;
    }
    *to -= over;
  }
  debug_assert!(*from <= *to);
  Some(Diffs { parts: runs })
}

fn find_ff(bytes: &[u8]) -> Option<usize> {
  const ONES: u64 = 0x0101_0101_0101_0101;
  const HIGHS: u64 = 0x8080_8080_8080_8080;
  let mut words = bytes.chunks_exact(8);
  for (k, word) in words.by_ref().enumerate() {

    let inverse = !u64::from_le_bytes(word.try_into().unwrap());
    let flags = inverse.wrapping_sub(ONES) & !inverse & HIGHS;
    if flags != 0 {
      return Some(k * 8 + (flags.trailing_zeros() / 8) as usize);
    }
  }
  let tail = words.remainder();
  tail.iter().position(|&b| b == 0xFF).map(|at| bytes.len() - tail.len() + at)
}

pub(crate) fn unstuff(src: &[u8]) -> (Vec<u8>, usize) {

  fn walk(part: &[u8], mut put: impl FnMut(&[u8])) -> bool {
    let mut at = 0;
    while let Some(ff) = find_ff(&part[at..]) {
      let ff = at + ff;
      if part.get(ff + 1) != Some(&0) {

        put(&part[at..ff]);
        return true;
      }
      put(&part[at..=ff]);
      at = ff + 2;
    }
    put(&part[at..]);
    false
  }

  let count = rayon::current_num_threads().min(src.len() / MIN_PART).clamp(1, MAX_PARTS);
  let mut bounds: Vec<usize> = (0..=count).map(|k| src.len() * k / count).collect();
  for b in bounds[1..count].iter_mut() {

    if src[*b - 1] == 0xFF {
      *b += 1;
    }
  }
  let parts: Vec<&[u8]> = bounds.windows(2).map(|w| &src[w[0]..w[1].max(w[0])]).collect();

  let sizes: Vec<(usize, bool)> = parts
    .par_iter()
    .map(|part| {
      let mut size = 0;
      let end = walk(part, |run| size += run.len());
      (size, end)
    })
    .collect();
  let used = sizes.iter().position(|(_, end)| *end).map_or(count, |k| k + 1);
  let len: usize = sizes[..used].iter().map(|(size, _)| size).sum();

  let mut out = vec![0u8; len + 16];
  let mut rest = &mut out[..len];
  let mut targets = Vec::with_capacity(used);
  for (size, _) in &sizes[..used] {
    let (this, next) = std::mem::take(&mut rest).split_at_mut(*size);
    targets.push(this);
    rest = next;
  }
  parts[..used].par_iter().zip(targets.into_par_iter()).for_each(|(part, target)| {
    let mut at = 0;
    walk(part, |run| {
      target[at..at + run.len()].copy_from_slice(run);
      at += run.len();
    });
  });
  (out, len)
}

fn predictor_one(ljpeg: &super::LjpegDecompressor, width: usize, height: usize) -> Option<(Diffs, Vec<u16>)> {
  let ncomp = ljpeg.components();
  let table = ljpeg.sof.components.first()?.dc_tbl_num;
  if ljpeg.predictor != 1
    || ljpeg.sof.width * ncomp != width
    || ljpeg.sof.height < height
    || width <= ncomp
    || height == 0

    || ljpeg.sof.components.iter().any(|component| !ljpeg.dhts[component.dc_tbl_num].same_code(&ljpeg.dhts[table]))
  {
    return None;
  }
  let (bits, end) = unstuff(ljpeg.buffer);
  let diffs = decode(&bits, end, &ljpeg.dhts[table], width * height)?;
  drop(bits);

  let base = (1u32 << (ljpeg.sof.precision - ljpeg.point_transform - 1)) as u16;
  let mut firsts = vec![0u16; height * ncomp];
  for row in 0..height {
    let mut c = 0;
    for piece in diffs.pieces(row * width, ncomp) {
      for diff in piece {
        let above = if row == 0 { base } else { firsts[(row - 1) * ncomp + c] };
        firsts[row * ncomp + c] = above.wrapping_add(*diff as u16);
        c += 1;
      }
    }
  }
  Some((diffs, firsts))
}

#[inline(always)]
fn predict_row(line: &mut [u16], firsts: &[u16], diffs: &Diffs, row: usize) {
  let (width, ncomp) = (line.len(), firsts.len());
  line[..ncomp].copy_from_slice(firsts);
  let mut col = ncomp;
  for piece in diffs.pieces(row * width + ncomp, width - ncomp) {
    for diff in piece {
      line[col] = line[col - ncomp].wrapping_add(*diff as u16);
      col += 1;
    }
  }
}

pub(super) fn decode_ljpeg(ljpeg: &super::LjpegDecompressor, out: &mut [u16], x: usize, stripwidth: usize, width: usize, height: usize) -> Option<()> {
  if x != 0 || stripwidth != width || out.len() < width * height {
    return None;
  }
  pool()?.install(|| decode_ljpeg_in_pool(ljpeg, out, width, height))
}

fn decode_ljpeg_in_pool(ljpeg: &super::LjpegDecompressor, out: &mut [u16], width: usize, height: usize) -> Option<()> {
  let ncomp = ljpeg.components();
  let (diffs, firsts) = predictor_one(ljpeg, width, height)?;
  out[..width * height].par_chunks_exact_mut(width).enumerate().for_each(|(row, line)| {
    predict_row(line, &firsts[row * ncomp..(row + 1) * ncomp], &diffs, row);
  });
  Some(())
}

pub(crate) fn decode_cr2_fields(ljpeg: &super::LjpegDecompressor, fields: &[usize], width: usize, height: usize) -> Option<Vec<u16>> {
  if fields.iter().sum::<usize>() != width || fields.contains(&0) {
    return None;
  }
  pool()?.install(|| cr2_fields_in_pool(ljpeg, fields, width, height))
}

fn cr2_fields_in_pool(ljpeg: &super::LjpegDecompressor, fields: &[usize], width: usize, height: usize) -> Option<Vec<u16>> {
  let ncomp = ljpeg.components();
  let (diffs, firsts) = predictor_one(ljpeg, width, height)?;

  let mut starts = Vec::with_capacity(fields.len());
  let (mut at, mut column) = (0, 0);
  for &field in fields {
    starts.push((at, column, field));
    at += field * height;
    column += field;
  }

  let mut out = vec![0u16; width * height];
  let target = Target(out.as_mut_ptr());
  (0..height).into_par_iter().for_each_init(
    || vec![0u16; width],
    |line, row| {
      predict_row(line, &firsts[row * ncomp..(row + 1) * ncomp], &diffs, row);
      let mut col = 0;
      while col < width {
        let flat = row * width + col;

        let &(start, column, field) = starts.iter().rev().find(|(start, ..)| *start <= flat).unwrap();
        let (r, c) = ((flat - start) / field, (flat - start) % field);
        let run = (field - c).min(width - col);

        unsafe { std::ptr::copy_nonoverlapping(line[col..col + run].as_ptr(), target.at(r * width + column + c), run) };
        col += run;
      }
    },
  );
  Some(out)
}

#[derive(Clone, Copy)]
struct Target(*mut u16);
unsafe impl Send for Target {}
unsafe impl Sync for Target {}
impl Target {
  unsafe fn at(self, index: usize) -> *mut u16 {
    unsafe { self.0.add(index) }
  }
}
