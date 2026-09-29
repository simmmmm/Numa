use super::IFD;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct TiffFile {

  pub chain: Vec<IFD>,

  pub base: u32,

  pub corr: i32,
}

impl TiffFile {
  pub fn new(base: u32, corr: i32) -> Self {
    Self { base, corr, chain: Vec::new() }
  }

  pub fn push_ifd(&mut self, ifd: IFD) {
    self.chain.push(ifd);
  }
}
