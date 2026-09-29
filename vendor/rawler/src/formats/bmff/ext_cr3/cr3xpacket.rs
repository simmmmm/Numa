use super::super::{BoxHeader, ReadBox, Result};
use serde::{Deserialize, Serialize};
use std::io::{Read, Seek, SeekFrom};

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Cr3XpacketBox {
  pub header: BoxHeader,
}

impl Cr3XpacketBox {

  pub const UUID: [u8; 16] = [0xbe, 0x7a, 0xcf, 0xcb, 0x97, 0xa9, 0x42, 0xe8, 0x9c, 0x71, 0x99, 0x94, 0x91, 0xe3, 0xaf, 0xac];
}

impl<R: Read + Seek> ReadBox<&mut R> for Cr3XpacketBox {
  fn read_box(reader: &mut R, header: BoxHeader) -> Result<Self> {
    reader.seek(SeekFrom::Start(header.end_offset()))?;

    Ok(Self { header })
  }
}
