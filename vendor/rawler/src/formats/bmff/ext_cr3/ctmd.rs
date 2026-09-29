use super::super::{BoxHeader, FourCC, ReadBox, Result};
use byteorder::{BigEndian, ReadBytesExt};
use serde::{Deserialize, Serialize};
use std::io::{Read, Seek, SeekFrom};

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct CtmdBox {
  pub header: BoxHeader,

  pub reserved: [u8; 6],
  pub data_ref_index: u16,
  pub rec_count: u32,
  pub records: Vec<CtmdRecord>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct CtmdRecord {
  pub unknown1: u8,
  pub unknown2: u8,
  pub rec_type: u16,
  pub rec_size: u32,
}

impl CtmdBox {
  pub const TYP: FourCC = FourCC::with(['C', 'T', 'M', 'D']);
}

impl<R: Read + Seek> ReadBox<&mut R> for CtmdBox {
  fn read_box(reader: &mut R, header: BoxHeader) -> Result<Self> {

    let mut reserved = [0_u8; 6];
    reader.read_exact(&mut reserved)?;
    let data_ref_index = reader.read_u16::<BigEndian>()?;
    let rec_count = reader.read_u32::<BigEndian>()?;

    let mut records = Vec::with_capacity(rec_count as usize);

    for _ in 0..rec_count {

      let record = CtmdRecord {
        unknown1: reader.read_u8()?,
        unknown2: reader.read_u8()?,
        rec_type: reader.read_u16::<BigEndian>()?,
        rec_size: reader.read_u32::<BigEndian>()?,
      };
      records.push(record);

    }

    assert!(reader.stream_position()? == header.end_offset());

    reader.seek(SeekFrom::Start(header.end_offset()))?;

    Ok(Self {
      header,

      reserved,
      data_ref_index,
      rec_count,
      records,
    })
  }
}
