use image::RgbImage;
use numa_core::color::invert;
use numa_core::profile::{multiply_matrix, Matrix3};
use numa_core::space::ColourSpace;
use rayon::prelude::*;

const STEPS: usize = 4096;

#[derive(Clone)]
pub struct Display {

    pub name: String,

    decode: [f32; 256],

    matrix: Matrix3,

    encode: [Vec<u8>; 3],
}

impl Display {

    pub fn from_icc(bytes: &[u8]) -> Result<Option<Display>, String> {
        let tags = Tags::read(bytes)?;
        let column = |name: &[u8; 4]| tags.xyz(name);
        let (red, green, blue) = (column(b"rXYZ")?, column(b"gXYZ")?, column(b"bXYZ")?);
        let screen: Matrix3 = std::array::from_fn(|row| [red[row], green[row], blue[row]]);
        let from_xyz = invert(&screen).ok_or("its primaries do not make a colour space")?;
        let matrix = multiply_matrix(&from_xyz, &ColourSpace::Srgb.to_xyz());
        let curves = [tags.curve(b"rTRC")?, tags.curve(b"gTRC")?, tags.curve(b"bTRC")?];

        let decode: [f32; 256] = std::array::from_fn(|code| ColourSpace::Srgb.decode(code as f32 / 255.0));
        let encode = curves.map(|curve| backwards(&curve));
        let display = Display { name: tags.name(), decode, matrix, encode };
        Ok((!display.is_srgb()).then_some(display))
    }

    fn is_srgb(&self) -> bool {
        (0..=255u8).all(|code| {
            [[code, code, code], [code, 0, 0], [0, code, 0], [0, 0, code]]
                .iter()
                .all(|&pixel| self.pixel(pixel).iter().zip(pixel).all(|(out, into)| out.abs_diff(into) <= 1))
        })
    }

    pub fn pixel(&self, rgb: [u8; 3]) -> [u8; 3] {
        let linear = rgb.map(|code| self.decode[code as usize]);
        std::array::from_fn(|channel| {
            let row = self.matrix[channel];
            let value = row[0] * linear[0] + row[1] * linear[1] + row[2] * linear[2];

            let step = (value.clamp(0.0, 1.0) * (STEPS - 1) as f32).round() as usize;
            self.encode[channel][step]
        })
    }

    pub fn apply(&self, image: &mut RgbImage) {
        image.par_chunks_exact_mut(3).for_each(|pixel| {
            let out = self.pixel([pixel[0], pixel[1], pixel[2]]);
            pixel.copy_from_slice(&out);
        });
    }
}

enum Curve {
    Table(Vec<f32>),

    Parametric(u16, [f32; 7]),
}

impl Curve {
    fn at(&self, code: f32) -> f32 {
        match self {
            Curve::Table(table) if table.is_empty() => code,
            Curve::Table(table) if table.len() == 1 => code.powf(table[0]),
            Curve::Table(table) => {
                let at = code * (table.len() - 1) as f32;
                let low = (at.floor() as usize).min(table.len() - 2);
                let t = at - low as f32;
                table[low] * (1.0 - t) + table[low + 1] * t
            }
            Curve::Parametric(kind, [g, a, b, c, d, e, f]) => match kind {
                0 => code.powf(*g),
                1 => if code >= -b / a { (a * code + b).powf(*g) } else { 0.0 },
                2 => if code >= -b / a { (a * code + b).powf(*g) + c } else { *c },
                3 => if code >= *d { (a * code + b).powf(*g) } else { c * code },
                _ => if code >= *d { (a * code + b).powf(*g) + e } else { c * code + f },
            },
        }
    }
}

fn backwards(curve: &Curve) -> Vec<u8> {
    const FINE: usize = 65536;
    let forward: Vec<f32> = (0..FINE).map(|at| curve.at(at as f32 / (FINE - 1) as f32)).collect();
    let mut code = 0;
    (0..STEPS)
        .map(|step| {
            let wanted = step as f32 / (STEPS - 1) as f32;
            while code + 1 < FINE && forward[code] < wanted {
                code += 1;
            }

            let nearer = match code > 0 && wanted - forward[code - 1] < forward[code] - wanted {
                true => code - 1,
                false => code,
            };
            (nearer as f32 / (FINE - 1) as f32 * 255.0).round() as u8
        })
        .collect()
}

struct Tags<'a> {
    bytes: &'a [u8],
    table: Vec<([u8; 4], usize, usize)>,
}

impl<'a> Tags<'a> {
    fn read(bytes: &'a [u8]) -> Result<Self, String> {
        if bytes.get(36..40) != Some(b"acsp") {
            return Err("not an ICC profile".to_string());
        }
        if bytes.get(16..20) != Some(b"RGB ") {
            return Err("not an RGB profile".to_string());
        }
        let count = u32_at(bytes, 128).ok_or("no tag table")? as usize;
        let table = (0..count.min(1024))
            .map(|index| {
                let at = 132 + index * 12;
                let name: [u8; 4] = bytes.get(at..at + 4)?.try_into().ok()?;
                Some((name, u32_at(bytes, at + 4)? as usize, u32_at(bytes, at + 8)? as usize))
            })
            .collect::<Option<Vec<_>>>()
            .ok_or("a tag table that runs off the end")?;
        Ok(Tags { bytes, table })
    }

    fn tag(&self, name: &[u8; 4]) -> Result<&'a [u8], String> {
        let &(_, at, size) = self
            .table
            .iter()
            .find(|(tag, _, _)| tag == name)
            .ok_or_else(|| format!("no {} — a profile made of lookup tables, which this does not read", String::from_utf8_lossy(name)))?;
        self.bytes.get(at..at + size).ok_or_else(|| format!("{} runs off the end", String::from_utf8_lossy(name)))
    }

    fn xyz(&self, name: &[u8; 4]) -> Result<[f32; 3], String> {
        let tag = self.tag(name)?;
        let value = |at: usize| u32_at(tag, at).map(|raw| raw as i32 as f32 / 65536.0);
        match (tag.get(..4), value(8), value(12), value(16)) {
            (Some(b"XYZ "), Some(x), Some(y), Some(z)) => Ok([x, y, z]),
            _ => Err(format!("{} is not an XYZ", String::from_utf8_lossy(name))),
        }
    }

    fn curve(&self, name: &[u8; 4]) -> Result<Curve, String> {
        let tag = self.tag(name)?;
        let short = |at: usize| tag.get(at..at + 2).map(|two| u16::from_be_bytes([two[0], two[1]]));
        let broken = || format!("{} is cut short", String::from_utf8_lossy(name));
        match tag.get(..4) {
            Some(b"curv") => {
                let count = u32_at(tag, 8).ok_or_else(broken)? as usize;
                let values: Option<Vec<u16>> = (0..count).map(|index| short(12 + index * 2)).collect();
                let values = values.ok_or_else(broken)?;
                Ok(Curve::Table(match count {

                    1 => vec![values[0] as f32 / 256.0],
                    _ => values.into_iter().map(|value| value as f32 / 65535.0).collect(),
                }))
            }
            Some(b"para") => {
                let kind = short(8).ok_or_else(broken)?;
                let count = [1, 3, 4, 5, 7].get(kind as usize).copied().ok_or("a parametric curve of an unknown kind")?;
                let mut numbers = [0.0; 7];
                for (index, number) in numbers.iter_mut().enumerate().take(count) {
                    *number = u32_at(tag, 12 + index * 4).ok_or_else(broken)? as i32 as f32 / 65536.0;
                }
                Ok(Curve::Parametric(kind, numbers))
            }
            _ => Err(format!("{} is not a curve", String::from_utf8_lossy(name))),
        }
    }

    fn name(&self) -> String {
        let Ok(tag) = self.tag(b"desc") else { return "unnamed profile".to_string() };
        let text = match tag.get(..4) {
            Some(b"desc") => {
                let length = u32_at(tag, 8).unwrap_or(0) as usize;
                tag.get(12..12 + length).map(|ascii| String::from_utf8_lossy(ascii).trim_end_matches('\0').to_string())
            }
            Some(b"mluc") => {

                let (length, at) = (u32_at(tag, 20).unwrap_or(0) as usize, u32_at(tag, 24).unwrap_or(0) as usize);
                tag.get(at..at + length).map(|utf16| {
                    let units: Vec<u16> = utf16.chunks_exact(2).map(|two| u16::from_be_bytes([two[0], two[1]])).collect();
                    String::from_utf16_lossy(&units)
                })
            }
            _ => None,
        };
        text.filter(|text| !text.is_empty()).unwrap_or_else(|| "unnamed profile".to_string())
    }
}

fn u32_at(bytes: &[u8], at: usize) -> Option<u32> {
    bytes.get(at..at + 4).map(|four| u32::from_be_bytes([four[0], four[1], four[2], four[3]]))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_parametric_curve_runs_backwards_to_its_own_encode() {
        let srgb = Curve::Parametric(3, [2.4, 1.0 / 1.055, 0.055 / 1.055, 1.0 / 12.92, 0.040_45, 0.0, 0.0]);
        let table = backwards(&srgb);
        for step in (0..STEPS).step_by(97) {
            let linear = step as f32 / (STEPS - 1) as f32;
            let expected = (ColourSpace::Srgb.encode(linear) * 255.0).round() as i32;
            assert!((table[step] as i32 - expected).abs() <= 1, "at {linear}: {} against {expected}", table[step]);
        }

        let table = backwards(&Curve::Parametric(0, [2.2, 0.0, 0.0, 0.0, 0.0, 0.0, 0.0]));
        assert_eq!(table[STEPS - 1], 255);
        assert_eq!(table[0], 0);
        let half = (0.5f32.powf(1.0 / 2.2) * 255.0).round() as i32;
        assert!((table[(STEPS - 1) / 2] as i32 - half).abs() <= 1);
    }

    #[test]
    #[ignore]
    fn a_real_display_profile() {
        let Ok(path) = std::env::var("NUMA_ICC") else { return };
        let display = Display::from_icc(&std::fs::read(path).unwrap()).unwrap();
        let Some(display) = display else { return println!("sRGB: nothing to convert") };
        println!("{}", display.name);
        for patch in [[255u8, 0, 0], [0, 255, 0], [0, 0, 255], [128, 128, 128], [255, 255, 255], [220, 170, 140]] {
            println!("{patch:?} -> {:?}", display.pixel(patch));
        }

        for (width, height) in [(2560, 1707), (1920, 1280)] {
            let mut frame = RgbImage::from_fn(width, height, |x, y| image::Rgb([x as u8, y as u8, (x ^ y) as u8]));
            let started = std::time::Instant::now();
            display.apply(&mut frame);
            println!("{width} × {height}: {:?}", started.elapsed());
        }
    }

    #[test]
    fn what_is_not_a_display_profile_is_said_so() {
        assert!(Display::from_icc(b"not a profile at all").is_err());
        assert!(Display::from_icc(&[0u8; 200]).is_err());
    }
}
