use jxl_oxide::JxlImage;

use crate::decompressors::{Decompressor, LineIteratorMut};

pub struct JpegXLDecompressor {

  bps: u32,
}

impl JpegXLDecompressor {

  pub fn new(bps: u32) -> Self {
    Self { bps }
  }
}

impl<'a> Decompressor<'a, u16> for JpegXLDecompressor {

  fn decompress(&self, src: &[u8], mut skip_rows: usize, lines: impl LineIteratorMut<'a, u16>, line_width: usize) -> std::result::Result<(), String> {
    let image = JxlImage::builder()
      .read(src)
      .map_err(|err| format!("Failed to read JPEG-XL image: {:?}", err))?;
    if let Some(header) = image.frame_header(0) {
      if header.bit_depth.bits_per_sample() != 8 && header.bit_depth.bits_per_sample() != 16 {

        unimplemented!("JPEG-XL bit-depth {} not supported yet", header.bit_depth.bits_per_sample());
      }

    }
    let frame = image.render_frame(0).map_err(|err| format!("Failed to render JPEG-XL image: {:?}", err))?;

    let mut stream = frame.stream_no_alpha();

    match self.bps {
      8 => {
        let mut tmp = vec![0_u8; line_width];

        for line in lines.skip(skip_rows) {
          while skip_rows > 0 {
            let written = stream.write_to_buffer(&mut tmp);
            assert_eq!(line.len(), written);
            skip_rows -= 1;
          }
          let written = stream.write_to_buffer(&mut tmp);
          assert_eq!(line.len(), written);
          for (p, x) in line.iter_mut().zip(tmp.iter()) {
            *p = *x as u16;
          }
        }
      }
      9..=16 => {
        for line in lines.skip(skip_rows) {
          while skip_rows > 0 {
            let written = stream.write_to_buffer(line);
            assert_eq!(line.len(), written);
            skip_rows -= 1;
          }
          let written = stream.write_to_buffer(line);
          assert_eq!(line.len(), written);
        }
      }
      _ => unimplemented!(),
    }

    Ok(())
  }

  fn can_skip_rows(&self) -> bool {
    false
  }
}
