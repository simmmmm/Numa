#![deny(

    unstable_features,

  )]

#![allow(
  clippy::needless_doctest_main,
  clippy::identity_op,
  clippy::too_many_arguments,
  clippy::bool_assert_comparison,
  clippy::upper_case_acronyms,
  clippy::eq_op,
  clippy::needless_range_loop,
  clippy::manual_range_patterns,
  clippy::unnecessary_cast,
  clippy::get_first,
  clippy::vec_init_then_push,
  clippy::only_used_in_recursion,

  clippy::needless_lifetimes,
  clippy::type_complexity,

)]

use decoders::Camera;
use decoders::Decoder;
use decoders::RawDecodeParams;
use formats::jfif::JfifError;
use lazy_static::lazy_static;

pub mod analyze;
pub mod bitarray;
pub mod bits;
pub mod buffer;
pub mod cfa;
pub mod decoders;
pub mod decompressors;
pub mod devtools;
pub mod dng;
pub(crate) mod envparams;
pub mod exif;
pub mod formats;
pub mod imgop;
pub mod lens;
pub mod ljpeg92;
pub mod pixarray;
pub mod pumps;
pub mod rawimage;
pub mod rawsource;
pub mod tags;
pub mod tiles;

#[doc(hidden)]
pub use cfa::CFA;
pub use decoders::Orientation;
#[doc(hidden)]
pub use decoders::RawLoader;
use formats::tiff::TiffError;
pub use rawimage::RawImage;
pub use rawimage::RawImageData;
use rawsource::RawSource;

lazy_static! {
  static ref LOADER: RawLoader = decoders::RawLoader::new();
}

use std::io::Read;
use std::io::Seek;
use std::path::Path;
use thiserror::Error;

pub(crate) const ISSUE_HINT: &str = "Please open an issue at https://github.com/dnglab/dnglab/issues and provide this message (optionally the RAW file, if you can license it under CC0-license).";

pub trait ReadTrait: Read + Seek {}

impl<T: Read + Seek> ReadTrait for T {}

#[derive(Error, Debug)]
pub enum RawlerError {
  #[error("Error: {}, model '{}', make: '{}', mode: '{}'", what, model, make, mode)]
  Unsupported { what: String, model: String, make: String, mode: String },

  #[error("Failed to decode image, possibly corrupt image: {}", _0)]
  DecoderFailed(String),
}

pub type Result<T> = std::result::Result<T, RawlerError>;

impl RawlerError {
  pub fn unsupported(camera: &Camera, what: impl AsRef<str>) -> Self {
    Self::Unsupported {
      what: what.as_ref().to_string(),
      model: camera.model.clone(),
      make: camera.make.clone(),
      mode: camera.mode.clone(),
    }
  }

  pub fn with_io_error(context: impl AsRef<str>, path: impl AsRef<Path>, error: std::io::Error) -> Self {
    Self::DecoderFailed(format!(
      "I/O error in context '{}', {} on file: {}",
      context.as_ref(),
      error,
      path.as_ref().display()
    ))
  }
}

impl From<std::io::Error> for RawlerError {
  fn from(err: std::io::Error) -> Self {
    log::error!("I/O error: {}", err.to_string());
    log::error!("Backtrace:\n{:?}", backtrace::Backtrace::new());
    Self::DecoderFailed(format!("I/O Error without context: {}", err))
  }
}

impl From<&String> for RawlerError {
  fn from(str: &String) -> Self {
    Self::DecoderFailed(str.clone())
  }
}

impl From<&str> for RawlerError {
  fn from(str: &str) -> Self {
    Self::DecoderFailed(str.to_string())
  }
}

impl From<std::fmt::Arguments<'_>> for RawlerError {
  fn from(fmt: std::fmt::Arguments) -> Self {
    Self::DecoderFailed(fmt.to_string())
  }
}

impl From<String> for RawlerError {
  fn from(str: String) -> Self {
    Self::DecoderFailed(str)
  }
}

impl From<TiffError> for RawlerError {
  fn from(err: TiffError) -> Self {
    Self::DecoderFailed(err.to_string())
  }
}

impl From<JfifError> for RawlerError {
  fn from(err: JfifError) -> Self {
    Self::DecoderFailed(err.to_string())
  }
}

pub fn decode_file<P: AsRef<Path>>(path: P) -> Result<RawImage> {
  LOADER.decode_file(path.as_ref())
}

pub fn decode(rawfile: &RawSource, params: &RawDecodeParams) -> Result<RawImage> {
  LOADER.decode(rawfile, params, false)
}

#[doc(hidden)]
pub fn force_initialization() {
  lazy_static::initialize(&LOADER);
}

#[doc(hidden)]
pub fn decode_unwrapped(rawfile: &RawSource) -> Result<RawImageData> {
  LOADER.decode_unwrapped(rawfile)
}

#[doc(hidden)]
pub fn decode_dummy(rawfile: &RawSource) -> Result<RawImage> {
  LOADER.decode(rawfile, &RawDecodeParams::default(), true)
}

pub fn get_decoder(rawfile: &RawSource) -> Result<Box<dyn Decoder>> {
  LOADER.get_decoder(rawfile)
}

pub fn raw_image_count_file<P: AsRef<Path>>(path: P) -> Result<usize> {
  LOADER.raw_image_count_file(path.as_ref())
}

pub fn global_loader() -> &'static RawLoader {
  &LOADER
}

#[cfg(test)]
pub(crate) fn init_test_logger() {
  let _ = env_logger::builder().is_test(true).filter_level(log::LevelFilter::Debug).try_init();
}
