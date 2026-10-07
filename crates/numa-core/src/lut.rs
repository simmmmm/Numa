use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LutChoice {
    pub name: String,
    #[serde(default = "full")]
    pub amount: f32,
}

fn full() -> f32 {
    100.0
}

pub const MAX_SIZE: usize = 65;

#[derive(Debug, Clone, PartialEq)]
pub struct Lut {

    pub size: usize,

    pub cube: bool,
    domain_min: [f32; 3],
    domain_max: [f32; 3],

    table: Vec<[f32; 3]>,
}

impl Lut {

    pub fn parse(name: &str, text: &str) -> Result<Lut, String> {
        match name.rsplit('.').next().map(str::to_ascii_lowercase).as_deref() {
            Some("3dl") => parse_3dl(text),
            _ => parse_cube(text),
        }
    }

    pub fn from_fn(size: usize, f: impl Fn([f32; 3]) -> [f32; 3]) -> Lut {
        let step = |i: usize| i as f32 / (size - 1) as f32;
        let table = (0..size * size * size).map(|i| f([step(i % size), step((i / size) % size), step(i / (size * size))])).collect();
        Lut { size, cube: true, domain_min: [0.0; 3], domain_max: [1.0; 3], table }
    }

    pub fn apply(&self, rgb: [f32; 3]) -> [f32; 3] {

        let at: [f32; 3] = std::array::from_fn(|c| {
            let span = self.domain_max[c] - self.domain_min[c];
            ((rgb[c] - self.domain_min[c]) / if span > 0.0 { span } else { 1.0 }).clamp(0.0, 1.0)
        });
        match self.cube {
            true => self.tetrahedral(at),
            false => std::array::from_fn(|c| {
                let x = at[c] * (self.size - 1) as f32;
                let low = (x.floor() as usize).min(self.size - 2);
                let t = x - low as f32;
                self.table[low][c] * (1.0 - t) + self.table[low + 1][c] * t
            }),
        }
    }

    pub fn mix(&self, rgb: [f32; 3], amount: f32) -> [f32; 3] {
        let out = self.apply(rgb);
        std::array::from_fn(|c| rgb[c] + (out[c] - rgb[c]) * amount)
    }

    fn at(&self, r: usize, g: usize, b: usize) -> [f32; 3] {
        self.table[r + self.size * (g + self.size * b)]
    }

    fn tetrahedral(&self, at: [f32; 3]) -> [f32; 3] {
        let last = self.size - 1;
        let scaled = at.map(|v| v * last as f32);
        let base = scaled.map(|v| (v.floor() as usize).min(last.saturating_sub(1)));
        let [fr, fg, fb] = std::array::from_fn(|c| scaled[c] - base[c] as f32);
        let [r, g, b] = base;
        let (r1, g1, b1) = ((r + 1).min(last), (g + 1).min(last), (b + 1).min(last));
        let c000 = self.at(r, g, b);
        let c111 = self.at(r1, g1, b1);

        let (w, c1, c2) = if fr > fg {
            if fg > fb {
                ([fr, fg, fb], self.at(r1, g, b), self.at(r1, g1, b))
            } else if fr > fb {
                ([fr, fb, fg], self.at(r1, g, b), self.at(r1, g, b1))
            } else {
                ([fb, fr, fg], self.at(r, g, b1), self.at(r1, g, b1))
            }
        } else if fb > fg {
            ([fb, fg, fr], self.at(r, g, b1), self.at(r, g1, b1))
        } else if fb > fr {
            ([fg, fb, fr], self.at(r, g1, b), self.at(r, g1, b1))
        } else {
            ([fg, fr, fb], self.at(r, g1, b), self.at(r1, g1, b))
        };
        std::array::from_fn(|c| {
            (1.0 - w[0]) * c000[c] + (w[0] - w[1]) * c1[c] + (w[1] - w[2]) * c2[c] + w[2] * c111[c]
        })
    }
}

fn numbers(line: &str) -> Option<Vec<f32>> {
    line.split_whitespace().map(|word| word.parse::<f32>().ok()).collect()
}

pub fn parse_cube(text: &str) -> Result<Lut, String> {
    let (mut size, mut cube) = (0usize, true);
    let (mut domain_min, mut domain_max) = ([0.0f32; 3], [1.0f32; 3]);
    let mut table = Vec::new();
    for line in text.trim_start_matches('\u{feff}').lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut words = line.split_whitespace();
        let key = words.next().unwrap_or_default();
        let rest: Vec<&str> = words.collect();
        let value = |i: usize| rest.get(i).and_then(|word| word.parse::<f32>().ok());
        match key {
            "TITLE" => {}
            "LUT_3D_SIZE" | "LUT_1D_SIZE" => {
                size = value(0).ok_or("a size that is not a number")? as usize;
                cube = key == "LUT_3D_SIZE";
            }
            "DOMAIN_MIN" | "DOMAIN_MAX" => {
                let triple = [value(0), value(1), value(2)];
                let [Some(r), Some(g), Some(b)] = triple else { return Err(format!("{key} needs three numbers")) };
                match key {
                    "DOMAIN_MIN" => domain_min = [r, g, b],
                    _ => domain_max = [r, g, b],
                }
            }

            "LUT_3D_INPUT_RANGE" | "LUT_1D_INPUT_RANGE" => {
                let (Some(low), Some(high)) = (value(0), value(1)) else { return Err(format!("{key} needs two numbers")) };
                domain_min = [low; 3];
                domain_max = [high; 3];
            }
            _ => match numbers(line) {
                Some(row) if row.len() == 3 => table.push([row[0], row[1], row[2]]),

                Some(_) => return Err(format!("a row that is not three numbers: {line}")),
                None if key.chars().all(|c| c.is_ascii_uppercase() || c == '_' || c.is_ascii_digit()) => {}
                None => return Err(format!("not a .cube line: {line}")),
            },
        }
    }
    finish(size, cube, domain_min, domain_max, table)
}

pub fn parse_3dl(text: &str) -> Result<Lut, String> {
    let mut rows = text
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .filter_map(numbers)
        .filter(|row| !row.is_empty());
    let steps = rows.next().ok_or("an empty file")?;
    let size = steps.len();
    let values: Vec<[f32; 3]> = rows.filter(|row| row.len() == 3).map(|row| [row[0], row[1], row[2]]).collect();
    let top = values.iter().flatten().fold(0.0f32, |a, b| a.max(*b));
    let depth = [1023.0, 4095.0, 65535.0].into_iter().find(|depth| top <= *depth).unwrap_or(top.max(1.0));
    if values.len() != size.pow(3) {
        return Err(format!("{} rows where a {size}³ cube has {}", values.len(), size.pow(3)));
    }

    let mut table = vec![[0.0; 3]; values.len()];
    for (index, value) in values.iter().enumerate() {
        let (r, g, b) = (index / (size * size), (index / size) % size, index % size);
        table[r + size * (g + size * b)] = value.map(|v| v / depth);
    }
    finish(size, true, [0.0; 3], [1.0; 3], table)
}

fn finish(size: usize, cube: bool, domain_min: [f32; 3], domain_max: [f32; 3], table: Vec<[f32; 3]>) -> Result<Lut, String> {
    if !(2..=if cube { MAX_SIZE } else { 65536 }).contains(&size) {
        return Err(format!("a size of {size}, where 2 to {MAX_SIZE} is readable"));
    }
    let wanted = if cube { size.pow(3) } else { size };
    if table.len() != wanted {
        return Err(format!("{} rows where the size says {wanted}", table.len()));
    }
    Ok(Lut { size, cube, domain_min, domain_max, table })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn identity_cube(size: usize) -> String {
        let mut text = format!("TITLE \"identity\"\n# a comment\nLUT_3D_SIZE {size}\nDOMAIN_MIN 0 0 0\nDOMAIN_MAX 1 1 1\n");
        let step = |i: usize| i as f32 / (size - 1) as f32;
        for b in 0..size {
            for g in 0..size {
                for r in 0..size {
                    text += &format!("{} {} {}\n", step(r), step(g), step(b));
                }
            }
        }
        text
    }

    fn close(a: [f32; 3], b: [f32; 3]) -> bool {
        a.iter().zip(b).all(|(x, y)| (x - y).abs() < 1e-4)
    }

    #[test]
    fn a_cube_is_read_with_its_keywords() {
        let lut = parse_cube(&identity_cube(3)).unwrap();
        assert_eq!((lut.size, lut.cube, lut.table.len()), (3, true, 27));

        assert_eq!(lut.table[1], [0.5, 0.0, 0.0]);
        assert!(parse_cube("LUT_3D_SIZE 2\n0 0 0\n").is_err(), "too few rows");
        assert!(parse_cube("LUT_3D_SIZE 99\n").is_err(), "too large");
    }

    #[test]
    fn the_identity_changes_nothing() {
        let lut = parse_cube(&identity_cube(17)).unwrap();
        for rgb in [[0.0, 0.0, 0.0], [1.0, 1.0, 1.0], [0.2, 0.7, 0.4], [0.93, 0.05, 0.61], [0.5, 0.5, 0.5]] {
            assert!(close(lut.apply(rgb), rgb), "{rgb:?} → {:?}", lut.apply(rgb));
        }
    }

    #[test]
    fn a_known_cube_maps_what_it_should() {

        let mut text = String::from("LUT_3D_SIZE 2\n");
        for b in 0..2 {
            for g in 0..2 {
                for r in 0..2 {
                    text += &format!("{} {} {}\n", 1 - r, 1 - g, 1 - b);
                }
            }
        }
        let lut = parse_cube(&text).unwrap();
        assert!(close(lut.apply([0.2, 0.6, 0.9]), [0.8, 0.4, 0.1]));

        assert!(close(lut.mix([0.2, 0.6, 0.9], 0.5), [0.5, 0.5, 0.5]));

        let mut text = String::from("LUT_3D_SIZE 2\n");
        for b in 0..2 {
            for g in 0..2 {
                for r in 0..2 {
                    text += &format!("{b} {g} {r}\n");
                }
            }
        }
        let lut = parse_cube(&text).unwrap();
        assert!(close(lut.apply([0.9, 0.3, 0.1]), [0.1, 0.3, 0.9]));
    }

    #[test]
    fn a_1d_lut_and_a_domain() {

        let text = "LUT_1D_SIZE 3\nDOMAIN_MIN 0 0 0\nDOMAIN_MAX 2 2 2\n0 0 0\n0.25 0.25 0.25\n1 1 1\n";
        let lut = parse_cube(text).unwrap();
        assert!(!lut.cube);
        assert!(close(lut.apply([1.0, 2.0, 0.0]), [0.25, 1.0, 0.0]));
    }

    #[test]
    fn a_3dl_is_read_blue_fastest() {

        let mut text = String::from("0 1023\n");
        for r in 0..2 {
            for g in 0..2 {
                for b in 0..2 {
                    text += &format!("{} {} {}\n", b * 1023, g * 1023, r * 1023);
                }
            }
        }
        let lut = Lut::parse("look.3dl", &text).unwrap();
        assert!(close(lut.apply([1.0, 0.0, 0.0]), [0.0, 0.0, 1.0]));
        assert!(close(lut.apply([0.25, 0.5, 0.75]), [0.75, 0.5, 0.25]));
    }
}
