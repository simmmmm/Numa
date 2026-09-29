use std::fmt;

use itertools::Itertools;

use crate::formats::tiff::{self, Value};

pub const CFA_COLOR_R: usize = 0;
pub const CFA_COLOR_G: usize = 1;
pub const CFA_COLOR_B: usize = 2;

use num_enum::TryFromPrimitive;

#[derive(Clone, Copy, Debug, Eq, PartialEq, PartialOrd, Ord, TryFromPrimitive)]
#[repr(u8)]
#[allow(non_camel_case_types)]
pub enum CFAColor {

  RED = 0,
  GREEN = 1,
  BLUE = 2,
  CYAN = 3,
  MAGENTA = 4,
  YELLOW = 5,
  WHITE = 6,
  FUJI_GREEN = 7,
  END,
  UNKNOWN = 255,
}

impl Default for CFAColor {
  fn default() -> Self {
    Self::UNKNOWN
  }
}

impl TryFrom<char> for CFAColor {
  type Error = String;
  fn try_from(value: char) -> Result<Self, Self::Error> {
    Ok(match value {
      'R' => Self::RED,
      'G' => Self::GREEN,
      'B' => Self::BLUE,
      'E' => Self::CYAN,
      'M' => Self::MAGENTA,
      'Y' => Self::YELLOW,
      'C' => Self::CYAN,
      _ => {
        return Err(format!("Unknown CFA color \"{}\"", value));
      }
    })
  }
}

#[derive(Clone, Eq, PartialEq, PartialOrd, Ord)]
pub struct CFA {

  pub name: String,

  pub width: usize,

  pub height: usize,

  pattern: [[u8; 48]; 48],
}

impl Default for CFA {
  fn default() -> Self {
    Self::new("")
  }
}

impl CFA {
  #[doc(hidden)]
  pub fn new_from_tag(pat: &Value) -> CFA {
    let mut patname = String::new();
    for i in 0..pat.count() {
      patname.push(match pat.force_u32(i as usize) {
        0 => 'R',
        1 => 'G',
        2 => 'B',
        3 => 'C',
        4 => 'M',
        5 => 'Y',
        _ => 'U',
      });
    }
    CFA::new(&patname)
  }

  pub fn new(patname: &str) -> CFA {
    let (width, height) = match patname.len() {
      0 => (0, 0),
      4 => (2, 2),
      36 => (6, 6),
      16 => (2, 8),
      144 => (12, 12),
      _ => panic!("Unknown CFA size \"{}\"", patname),
    };
    let mut pattern: [[u8; 48]; 48] = [[0; 48]; 48];

    if width > 0 {

      for (i, c) in patname.chars().enumerate() {
        pattern[i / width][i % width] = CFAColor::try_from(c).expect("Invalid CFA pattern") as u8;
      }

      for row in 0..48 {
        for col in 0..48 {
          pattern[row][col] = pattern[row % height][col % width];
        }
      }
    }

    CFA {
      name: patname.to_string(),
      pattern,
      width,
      height,
    }
  }

  pub fn map_colors<F>(&self, op: F) -> Self
  where
    F: Fn(usize, usize, u8) -> u8,
  {
    let mut copy = self.clone();
    for row in 0..48 {
      for col in 0..48 {
        copy.pattern[row][col] = op(row % self.height, col % self.width, copy.pattern[row % self.height][col % self.width]);
      }
    }
    copy
  }

  pub fn color_at(&self, row: usize, col: usize) -> usize {
    self.pattern[(row + 48) % 48][(col + 48) % 48] as usize
  }

  pub fn cfa_color_at(&self, row: usize, col: usize) -> CFAColor {
    (self.pattern[(row + 48) % 48][(col + 48) % 48])
      .try_into()
      .expect("invalid CFA color value in pattern")
  }

  pub fn flat_pattern(&self) -> Vec<u8> {
    self
      .pattern
      .iter()
      .take(self.height)
      .flat_map(|v| v.iter().take(self.width))
      .cloned()
      .map(|v| v as u8)
      .collect()
  }

  pub fn unique_colors(&self) -> usize {
    self.pattern.iter().flatten().unique().count()
  }

  pub fn is_rgb(&self) -> bool {
    self.name.chars().filter(|ch| !['R', 'G', 'B'].contains(ch)).count() == 0 && self.name.contains('R') && self.name.contains('G') && self.name.contains('B')
  }

  pub fn is_rgbe(&self) -> bool {
    self.name.chars().filter(|ch| !['R', 'G', 'B', 'E'].contains(ch)).count() == 0
      && self.name.contains('R')
      && self.name.contains('G')
      && self.name.contains('B')
      && self.name.contains('E')
  }

  pub fn is_cygm(&self) -> bool {
    self.name.chars().filter(|ch| !['C', 'Y', 'G', 'M'].contains(ch)).count() == 0
      && self.name.contains('C')
      && self.name.contains('Y')
      && self.name.contains('G')
      && self.name.contains('M')
  }

  pub fn shift(&self, x: usize, y: usize) -> CFA {
    let mut pattern: [[u8; 48]; 48] = [[0; 48]; 48];
    for row in 0..48 {
      for col in 0..48 {
        pattern[row][col] = self.color_at(row + y, col + x) as u8;
      }
    }

    let mut name = "".to_string();
    for row in 0..self.height {
      for col in 0..self.width {
        name.push_str(match pattern[row][col] {
          0 => "R",
          1 => "G",
          2 => "B",
          3 => "C",
          4 => "M",
          5 => "Y",
          x => panic!("Unknown CFA color \"{}\"", x),
        });
      }
    }

    CFA {
      name,
      pattern,
      width: self.width,
      height: self.height,
    }
  }

  pub fn is_valid(&self) -> bool {
    self.width != 0 && self.height != 0
  }
}

impl fmt::Display for CFA {

  fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
    f.write_str(&self.name)
  }
}

impl fmt::Debug for CFA {
  fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
    write!(f, "CFA {{ {} }}", self.name)
  }
}

#[derive(Clone, Eq, PartialEq)]
pub struct PlaneColor {
  pub colors: Vec<CFAColor>,
}

impl PlaneColor {
  pub fn new(patname: &str) -> Self {
    let mut colors = vec![CFAColor::default(); patname.len()];
    for (i, c) in patname.chars().enumerate() {
      colors[i] = CFAColor::try_from(c).expect("Invalid CFA color");
    }
    Self { colors }
  }

  pub fn plane_colors<const N: usize>(&self) -> [CFAColor; N] {
    self.colors.clone().try_into().expect("PlaneColor has invalid length")
  }

  pub fn plane_lookup_table(&self) -> [usize; 256] {
    let mut map = [255; 256];
    self.colors.iter().enumerate().for_each(|(plane, color)| {
      map[*color as usize] = plane;
    });
    map
  }

  pub fn plane_count(&self) -> usize {
    self.colors.len()
  }

  pub fn cfa_index(cfa: &CFA, color: CFAColor) -> usize {
    for row in 0..cfa.height {
      for col in 0..cfa.width {
        if cfa.cfa_color_at(row, col) == color {
          return row * cfa.width + col;
        }
      }
    }
    panic!("CFAColor {:?} is not included in CFA {:?}", color, cfa);
  }
}

impl From<&PlaneColor> for tiff::Value {
  fn from(value: &PlaneColor) -> Self {
    Self::Byte(value.colors.iter().map(|x| *x as u8).collect())
  }
}

impl<T> From<T> for PlaneColor
where
  T: Into<Vec<CFAColor>>,
{
  fn from(value: T) -> Self {
    Self { colors: value.into() }
  }
}

impl Default for PlaneColor {
  fn default() -> Self {
    Self::from([CFAColor::RED, CFAColor::GREEN, CFAColor::BLUE])
  }
}

impl fmt::Debug for PlaneColor {
  fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
    write!(f, "PlaneColors {{ {:?} }}", self.colors)
  }
}
