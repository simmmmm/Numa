use crate::decompressors::{Decompressor, LineIteratorMut};

pub struct JpegDecompressor {}

impl JpegDecompressor {
  pub fn new() -> Self {
    Self {}
  }
}

impl<'a> Decompressor<'a, u16> for JpegDecompressor {

  fn decompress(&self, src: &[u8], skip_rows: usize, lines: impl LineIteratorMut<'a, u16>, line_width: usize) -> std::result::Result<(), String> {
    let img = image::load_from_memory_with_format(src, image::ImageFormat::Jpeg).map_err(|err| format!("Lossy JPEG decompression failed: {:?}", err))?;
    match img {
      image::DynamicImage::ImageRgb8(image_buffer) => {
        for (dst, src) in lines.zip(image_buffer.chunks_exact(line_width).skip(skip_rows)) {
          for (dst, src) in dst.iter_mut().zip(src.iter()) {
            *dst = *src as u16;
          }
        }
        Ok(())
      }
      _ => Err(format!("JpegDecompressor: expected RGB-8 image")),
    }
  }

  fn can_skip_rows(&self) -> bool {
    false
  }
}
