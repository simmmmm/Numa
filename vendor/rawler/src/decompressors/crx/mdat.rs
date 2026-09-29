use super::{Result, iquant::QStep};
use crate::decompressors::crx::CrxError;
use byteorder::{BigEndian, ReadBytesExt};
use std::io::{Cursor, Read};

#[derive(Debug, Clone)]
pub struct Tile {

  pub ind: u16,
  pub size: u16,
  pub tile_size: usize,
  pub flags: u32,
  pub qp_data: Option<TileQPData>,

  pub id: usize,
  pub counter: u32,
  pub tail_sign: u32,

  pub data_offset: usize,
  pub tile_width: usize,
  pub tile_height: usize,
  pub plane_width: usize,
  pub plane_height: usize,
  pub tiles_top: bool,
  pub tiles_bottom: bool,
  pub tiles_left: bool,
  pub tiles_right: bool,

  pub planes: Vec<Plane>,

  pub q_step: Option<Vec<QStep>>,
}

impl Tile {
  pub fn new<R: Read>(id: usize, hdr: &mut R, ind: u16, tile_offset: usize) -> Result<Self> {
    let size = hdr.read_u16::<BigEndian>()?;
    let tile_size = hdr.read_u32::<BigEndian>()? as usize;
    let flags = hdr.read_u32::<BigEndian>()?;

    let counter = (flags >> 16) & 0xF;
    let tail_sign = flags & 0xFFFF;
    let qp_data = if size == 16 {
      let mdat_qp_data_size = hdr.read_u32::<BigEndian>()?;
      let mdat_extra_size = hdr.read_u16::<BigEndian>()?;
      let terminator = hdr.read_u16::<BigEndian>()?;
      assert!(terminator == 0);
      Some(TileQPData {
        mdat_qp_data_size,
        mdat_extra_size,
        terminator,
      })
    } else {
      None
    };

    assert!((size == 8 && tail_sign == 0) || (size == 16 && tail_sign == 0x4000));

    Ok(Tile {
      id,
      ind,
      size,
      tile_size,
      flags,
      counter,
      tail_sign,
      data_offset: tile_offset,
      planes: vec![],
      tile_height: 0,
      tile_width: 0,
      plane_height: 0,
      plane_width: 0,
      qp_data,
      tiles_top: false,
      tiles_bottom: false,
      tiles_left: false,
      tiles_right: false,
      q_step: None,
    })
  }

  pub fn descriptor_line(&self) -> String {
    let extra_data = match self.qp_data.as_ref() {
      Some(qp_data) => {
        format!(
          " qp_data_size: {:#x} extra_size: {:#x}, terminator: {:#x}",
          qp_data.mdat_qp_data_size, qp_data.mdat_extra_size, qp_data.terminator
        )
      }
      None => String::from("NONE"),
    };
    format!(
      "Tile {:#x} size: {:#x} tile_size: {:#x} flags: {:#x} counter: {:#x} tail_sign: {:#x} extra: {}\n   top: {}, left: {}, bottom: {}, right: {}",
      self.ind,
      self.size,
      self.tile_size,
      self.flags,
      self.counter,
      self.tail_sign,
      extra_data,
      self.tiles_top,
      self.tiles_left,
      self.tiles_bottom,
      self.tiles_right,

    )
  }

  pub fn extra_size(&self) -> usize {
    match self.qp_data.as_ref() {
      Some(qp_data) => qp_data.mdat_qp_data_size as usize + qp_data.mdat_extra_size as usize,
      None => 0,
    }
  }
}

#[derive(Debug, Clone)]
pub struct TileQPData {

  pub mdat_qp_data_size: u32,

  pub mdat_extra_size: u16,

  pub terminator: u16,
}

#[derive(Debug, Clone)]
#[allow(unused)]
pub struct Plane {

  pub ind: u16,
  pub size: u16,
  pub plane_size: usize,
  pub flags: u32,

  pub id: usize,
  pub counter: u32,
  pub support_partial: bool,

  pub rounded_bits_mask: i32,
  pub data_offset: usize,
  pub parent_offset: usize,

  pub subbands: Vec<Subband>,
}

impl Plane {
  pub fn new<R: Read>(id: usize, hdr: &mut R, ind: u16, parent_offset: usize, plane_offset: usize) -> Result<Self> {
    let size = hdr.read_u16::<BigEndian>()?;
    let plane_size = hdr.read_u32::<BigEndian>()? as usize;
    let flags = hdr.read_u32::<BigEndian>()?;
    let counter = (flags >> 28) & 0xf;

    let support_partial: bool = (flags & 0x8000000) != 0;
    let mut rounded_bits_mask = ((flags >> 25) & 0x3) as i32;
    if rounded_bits_mask != 0 {
      rounded_bits_mask = 1 << (rounded_bits_mask - 1);
    }

    assert!(flags & 0x00FFFFFF == 0);
    Ok(Plane {
      id,
      ind,
      size,
      plane_size,
      flags,
      counter,
      support_partial,
      rounded_bits_mask,
      data_offset: plane_offset,
      parent_offset,
      subbands: vec![],
    })
  }

  pub fn descriptor_line(&self) -> String {
    format!(
      "  Plane {:#x} size: {:#x} plane_size: {:#x} flags: {:#x} counter: {:#x} support_partial: {} rounded_bits: {:#x}",
      self.ind, self.size, self.plane_size, self.flags, self.counter, self.support_partial, self.rounded_bits_mask
    )
  }
}

#[derive(Debug, Clone, Default)]
#[allow(unused)]
pub struct Subband {

  pub ind: u16,

  pub header_size: u16,

  pub subband_size: usize,

  pub flags: u32,

  pub q_step_base: i32,

  pub q_step_multi: u16,

  pub id: usize,

  pub counter: u32,

  pub support_partial: bool,

  pub q_param: u32,

  pub unused_bytes: u32,

  pub data_offset: usize,

  pub parent_offset: usize,

  pub data_size: usize,

  pub width: usize,

  pub height: usize,

  pub row_start_addon: usize,
  pub row_end_addon: usize,
  pub col_start_addon: usize,
  pub col_end_addon: usize,
  pub level_shift: i16,
}

impl Subband {
  pub fn new<R: Read>(id: usize, hdr: &mut R, ind: u16, parent_offset: usize, band_offset: usize) -> Result<Self> {
    let size = hdr.read_u16::<BigEndian>()?;
    assert!((size == 8 && ind == 0xFF03) || (size == 16 && ind == 0xFF13));
    let subband_size = hdr.read_u32::<BigEndian>()? as usize;
    match ind {
      0xFF03 => {
        let flags = hdr.read_u32::<BigEndian>()?;
        let counter = (flags >> 28) & 0xf;
        let support_partial: bool = (flags & 0x8000000) != 0;
        let q_param = (flags >> 19) & 0xFF;
        let unused_bytes = flags & 0x7FFFF;
        let data_size: usize = (subband_size as u32 - unused_bytes) as usize;
        let q_step_base = 0;
        let q_step_multi = 0;

        Ok(Subband {
          id,
          ind,
          header_size: size,
          subband_size,
          flags,
          counter,
          support_partial,
          q_param,
          q_step_base,
          q_step_multi,
          unused_bytes,
          data_offset: band_offset,
          parent_offset,
          data_size,
          ..Default::default()
        })
      }
      0xFF13 => {

        let q_param = 0;
        let support_partial = false;

        let flags = hdr.read_u16::<BigEndian>()? as u32;
        let q_step_multi = hdr.read_u16::<BigEndian>()?;
        let q_step_base = hdr.read_i32::<BigEndian>()?;
        let unused_bytes = hdr.read_u16::<BigEndian>()? as u32;
        let end_marker = hdr.read_u16::<BigEndian>()?;
        assert!(end_marker == 0);
        let counter = (flags >> 12) & 0xf;
        let data_size: usize = (subband_size as u32 - unused_bytes) as usize;

        Ok(Subband {
          id,
          ind,
          header_size: size,
          subband_size,
          flags,
          counter,
          support_partial,
          q_param,
          q_step_base,
          q_step_multi,
          unused_bytes,
          data_offset: band_offset,
          parent_offset,
          data_size,
          ..Default::default()
        })
      }
      _ => Err(CrxError::General(format!("Unknown subband header indicator: {:?}", ind))),
    }
  }

  pub fn descriptor_line(&self) -> String {
    format!(
      "    Subband {:#x} size: {:#x} subband_size: {:#x} flags: {:#x} counter: {:#x} support_partial: {} q_param: {:#x} unused_bytes: {:#x} qStepBase: {:#x} qStepMult: {:#x} ",
      self.ind,
      self.header_size,
      self.subband_size,
      self.flags,
      self.counter,
      self.support_partial,
      self.q_param,
      self.unused_bytes,
      self.q_step_base,
      self.q_step_multi
    )
  }

  pub(super) fn get_subband_row(&self, row: usize) -> usize {
    if row < self.row_start_addon {
      0
    } else if row < self.height - self.row_end_addon {
      row - self.row_end_addon
    } else {
      self.height - self.row_end_addon - self.row_start_addon - 1
    }
  }

  pub(super) fn setup_idx(
    &mut self,
    version: u16,
    level: usize,
    col_start_idx: usize,
    band_width_ex_coef: usize,
    row_start_idx: usize,
    band_height_ex_coef: usize,
  ) {

    if version == 0x200 {
      self.row_start_addon = row_start_idx;
      self.row_end_addon = band_height_ex_coef;
      self.col_start_addon = col_start_idx;
      self.col_end_addon = band_width_ex_coef;
      self.level_shift = 3 - level as i16;
    } else {
      self.row_start_addon = 0;
      self.row_end_addon = 0;
      self.col_start_addon = 0;
      self.col_end_addon = 0;
      self.level_shift = 0;
    }
  }
}

fn next_indicator<T: Read>(hdr: &mut T) -> Result<u16> {
  hdr
    .read_u16::<BigEndian>()
    .map_err(|_| CrxError::General("Header indicator read failed".into()))
}

#[allow(clippy::while_let_loop)]
pub(super) fn parse_header(mdat_hdr: &[u8]) -> Result<Vec<Tile>> {
  let mut hdr = Cursor::new(mdat_hdr);
  let mut tiles = Vec::new();
  let mut tile_offset: usize = 0;
  let mut plane_offset: usize = 0;
  let mut band_offset: usize = 0;

  let mut ind = next_indicator(&mut hdr)?;
  loop {
    match ind {
      0xff01 | 0xff11 => {
        let mut tile = Tile::new(tiles.len(), &mut hdr, ind, tile_offset)?;
        ind = next_indicator(&mut hdr)?;
        loop {
          match ind {
            0xff02 | 0xff12 => {
              let mut plane = Plane::new(tile.planes.len(), &mut hdr, ind, tile.data_offset, plane_offset)?;
              ind = next_indicator(&mut hdr)?;
              loop {
                match ind {
                  0xff03 | 0xff13 => {
                    let subband = Subband::new(plane.subbands.len(), &mut hdr, ind, tile.data_offset + plane.data_offset, band_offset)?;
                    band_offset += subband.subband_size;
                    plane.subbands.push(subband);

                    ind = next_indicator(&mut hdr).unwrap_or(0x0000);
                  }
                  _ => {
                    break;
                  }
                }
              }
              plane_offset += plane.plane_size as usize;
              band_offset = 0;
              tile.planes.push(plane);
            }
            _ => {
              break;
            }
          }
        }
        tile_offset += tile.tile_size;
        plane_offset = 0;
        tiles.push(tile);
      }
      0x0000 => {
        break;
      }
      _ => {
        return Err(CrxError::General(format!("Unexpected header record marker: {:x?}", ind)));
      }
    }
  }
  Ok(tiles)
}
