use super::super::{BoxHeader, FourCC, ReadBox, Result};
use serde::{Deserialize, Serialize};
use std::io::{Read, Seek, SeekFrom};

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct ThmbBox {
  pub header: BoxHeader,
}

impl ThmbBox {
  pub const TYP: FourCC = FourCC::with(['T', 'H', 'M', 'B']);
}

impl<R: Read + Seek> ReadBox<&mut R> for ThmbBox {
  fn read_box(reader: &mut R, header: BoxHeader) -> Result<Self> {

    reader.seek(SeekFrom::Start(header.end_offset()))?;

    Ok(Self { header })
  }
}
