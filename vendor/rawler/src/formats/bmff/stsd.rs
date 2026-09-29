use super::{
  BoxHeader, FourCC, ReadBox, Result,
  ext_cr3::{craw::CrawBox, ctmd::CtmdBox},
  read_box_header_ext,
  vendor::VendorBox,
};
use byteorder::{BigEndian, ReadBytesExt};
use log::debug;
use serde::{Deserialize, Serialize};
use std::io::{Read, Seek, SeekFrom};

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct StsdBox {
  pub header: BoxHeader,
  pub version: u8,
  pub flags: u32,
  pub craw: Option<CrawBox>,
  pub ctmd: Option<CtmdBox>,
  pub vendor: Vec<VendorBox>,
}

impl StsdBox {
  pub const TYP: FourCC = FourCC::with(['s', 't', 's', 'd']);
}

impl<R: Read + Seek> ReadBox<&mut R> for StsdBox {
  fn read_box(mut reader: &mut R, header: BoxHeader) -> Result<Self> {
    let (version, flags) = read_box_header_ext(reader)?;

    let mut craw = None;
    let mut ctmd = None;

    let mut vendors = Vec::new();

    reader.read_u32::<BigEndian>()?;

    let mut current = reader.stream_position()?;

    while current < header.end_offset() {

      let header = BoxHeader::parse(&mut reader)?;

      match header.typ {
        CrawBox::TYP => {
          assert_eq!(craw, None, "Found second CRAW box");
          craw = Some(CrawBox::read_box(&mut reader, header)?);
        }
        CtmdBox::TYP => {
          assert_eq!(ctmd, None, "Found second CTMD box");
          ctmd = Some(CtmdBox::read_box(&mut reader, header)?);
        }

        _ => {
          debug!("Vendor box found in stsd: {:?}", header.typ);
          let vendor = VendorBox::read_box(&mut reader, header)?;
          vendors.push(vendor);
        }
      }

      current = reader.stream_position()?;
    }

    reader.seek(SeekFrom::Start(header.end_offset()))?;

    Ok(Self {
      header,
      version,
      flags,
      craw,
      ctmd,
      vendor: vendors,
    })
  }
}
