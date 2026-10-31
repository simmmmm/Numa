use image::RgbImage;
use sha2::{Digest, Sha256};

pub const MAX_EDGE: u32 = 2048;

pub const TILE: u32 = 64;

pub struct Mulberry(u32);

impl Mulberry {
    pub fn new(seed: u32) -> Self {
        Self(seed)
    }

    pub fn next(&mut self) -> u32 {
        self.0 = self.0.wrapping_add(0x6d2b_79f5);
        let mut t = self.0;
        t = (t ^ (t >> 15)).wrapping_mul(t | 1);
        t ^= t.wrapping_add((t ^ (t >> 7)).wrapping_mul(t | 61));
        t ^ (t >> 14)
    }

    pub fn below(&mut self, n: usize) -> usize {
        ((self.next() as u64 * n as u64) >> 32) as usize
    }

    pub fn between(&mut self, low: f32, high: f32) -> f32 {
        low + (high - low) * (self.next() as f32 / u32::MAX as f32)
    }
}

pub fn order(seed: u32, tiles: usize) -> Vec<usize> {
    let mut order: Vec<usize> = (0..tiles).collect();
    let mut random = Mulberry::new(seed);
    for i in (1..tiles).rev() {
        order.swap(i, random.below(i + 1));
    }
    order
}

fn grid(width: u32, height: u32) -> (u32, u32) {
    (width.div_ceil(TILE), height.div_ceil(TILE))
}

pub fn scramble(picture: &RgbImage, seed: u32) -> RgbImage {
    let (columns, rows) = grid(picture.width(), picture.height());
    let order = order(seed, (columns * rows) as usize);
    let mut puzzle = RgbImage::new(columns * TILE, rows * TILE);
    for (from, &to) in order.iter().enumerate() {
        let (fx, fy) = ((from as u32 % columns) * TILE, (from as u32 / columns) * TILE);
        let (tx, ty) = ((to as u32 % columns) * TILE, (to as u32 / columns) * TILE);
        for y in 0..TILE {
            for x in 0..TILE {
                let source = picture.get_pixel((fx + x).min(picture.width() - 1), (fy + y).min(picture.height() - 1));
                puzzle.put_pixel(tx + x, ty + y, *source);
            }
        }
    }
    puzzle
}

#[cfg(test)]
pub fn unscramble(puzzle: &RgbImage, seed: u32, width: u32, height: u32) -> RgbImage {
    let (columns, rows) = grid(width, height);
    let order = order(seed, (columns * rows) as usize);
    RgbImage::from_fn(width, height, |x, y| {
        let to = order[((y / TILE) * columns + x / TILE) as usize] as u32;
        *puzzle.get_pixel((to % columns) * TILE + x % TILE, (to / columns) * TILE + y % TILE)
    })
}

pub fn jpeg(picture: &RgbImage, quality: u8) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    let mut encoder = jpeg_encoder::Encoder::new(&mut bytes, quality);
    encoder.set_sampling_factor(jpeg_encoder::SamplingFactor::F_1_1);
    if let Some(said) = numa_io::xmp::description(&numa_io::export::ExportSettings::default(), &numa_io::export::Description::default(), None) {

        encoder.add_app_segment(1, &numa_io::xmp::segment(&said)[4..]).map_err(|err| err.to_string())?;
    }
    let (width, height) = (picture.width() as u16, picture.height() as u16);
    encoder.encode(picture.as_raw(), width, height, jpeg_encoder::ColorType::Rgb).map_err(|err| err.to_string())?;
    Ok(bytes)
}

pub fn seed_of(parts: &[&[u8]]) -> u32 {
    let mut hash = Sha256::new();
    for part in parts {
        hash.update((part.len() as u32).to_le_bytes());
        hash.update(part);
    }
    u32::from_le_bytes(hash.finalize()[..4].try_into().expect("four bytes"))
}

struct Glyph {
    bitmap: Vec<u8>,
    width: i32,
    height: i32,
    left: i32,
    top: i32,
    advance: i32,
}

const BORDER: i32 = 3;

const EDGE: f32 = 191.25;
const PER_PIXEL: f32 = 255.0 / 8.0;

fn varint(bytes: &[u8], at: &mut usize) -> Option<u64> {
    let mut value = 0u64;
    for shift in (0..64).step_by(7) {
        let byte = *bytes.get(*at)?;
        *at += 1;
        value |= ((byte & 0x7f) as u64) << shift;
        if byte & 0x80 == 0 {
            return Some(value);
        }
    }
    None
}

fn fields(bytes: &[u8]) -> Vec<(u64, Result<u64, &[u8]>)> {
    let mut found = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        let Some(key) = varint(bytes, &mut at) else { break };
        let value = match key & 7 {
            0 => match varint(bytes, &mut at) {
                Some(value) => Ok(value),
                None => break,
            },
            2 => {
                let Some(length) = varint(bytes, &mut at) else { break };
                let end = at + length as usize;
                let Some(inside) = bytes.get(at..end) else { break };
                at = end;
                Err(inside)
            }
            _ => break,
        };
        found.push((key >> 3, value));
    }
    found
}

fn glyphs() -> &'static std::collections::HashMap<char, Glyph> {
    static GLYPHS: std::sync::OnceLock<std::collections::HashMap<char, Glyph>> = std::sync::OnceLock::new();
    GLYPHS.get_or_init(|| {
        let mut glyphs = std::collections::HashMap::new();
        for (_, file) in crate::pages::FONTS {
            for stack in fields(file).into_iter().filter_map(|(field, value)| (field == 1).then_some(value.err()).flatten()) {
                for one in fields(stack).into_iter().filter_map(|(field, value)| (field == 3).then_some(value.err()).flatten()) {
                    let (mut id, mut glyph) = (0, Glyph { bitmap: Vec::new(), width: 0, height: 0, left: 0, top: 0, advance: 0 });
                    let zigzag = |value: u64| ((value >> 1) as i64 ^ -((value & 1) as i64)) as i32;
                    for (field, value) in fields(one) {
                        match (field, value) {
                            (1, Ok(value)) => id = value as u32,
                            (2, Err(bitmap)) => glyph.bitmap = bitmap.to_vec(),
                            (3, Ok(value)) => glyph.width = value as i32,
                            (4, Ok(value)) => glyph.height = value as i32,
                            (5, Ok(value)) => glyph.left = zigzag(value),
                            (6, Ok(value)) => glyph.top = zigzag(value),
                            (7, Ok(value)) => glyph.advance = value as i32,
                            _ => {}
                        }
                    }
                    if let Some(c) = char::from_u32(id) {
                        glyphs.insert(c, glyph);
                    }
                }
            }
        }
        glyphs
    })
}

fn line(text: &str) -> (Vec<u8>, usize, usize) {
    let glyphs = glyphs();
    let pad = 8;
    let placed: Vec<(&Glyph, i32)> = text
        .chars()
        .filter_map(|c| glyphs.get(&c))
        .scan(0, |pen, glyph| {
            let at = *pen;
            *pen += glyph.advance;
            Some((glyph, at))
        })
        .collect();
    let width = placed.last().map_or(0, |(glyph, at)| at + glyph.advance) + 2 * pad;

    let high = placed.iter().map(|(glyph, _)| glyph.top + BORDER).max().unwrap_or(0);
    let low = placed.iter().map(|(glyph, _)| glyph.top - glyph.height - BORDER).min().unwrap_or(0);
    let height = high - low + 2 * pad;
    let mut field = vec![0u8; (width * height).max(0) as usize];
    for (glyph, at) in placed {
        let (w, h) = (glyph.width + 2 * BORDER, glyph.height + 2 * BORDER);
        if glyph.bitmap.len() < (w * h) as usize {
            continue;
        }
        let (x0, y0) = (pad + at + glyph.left - BORDER, pad + high - glyph.top - BORDER);
        for y in 0..h {
            for x in 0..w {
                let cell = &mut field[((y0 + y) * width + x0 + x) as usize];
                *cell = (*cell).max(glyph.bitmap[(y * w + x) as usize]);
            }
        }
    }
    (field, width as usize, height as usize)
}

pub fn watermark(picture: &mut RgbImage, text: &str, seed: u32) {
    let (field, field_width, field_height) = line(text.trim());
    if field_width == 0 || text.trim().is_empty() {
        return;
    }
    let mut random = Mulberry::new(seed);
    let (width, height) = picture.dimensions();
    let long = width.max(height) as f32;

    let em = long * random.between(0.032, 0.045);
    let scale = em / 24.0;
    let angle = random.between(20f32.to_radians(), 40f32.to_radians()) * if random.next() & 1 == 0 { 1.0 } else { -1.0 };
    let (sin, cos) = angle.sin_cos();
    let line_width = field_width as f32 * scale;
    let line_height = field_height as f32 * scale;
    let across = line_width + em * random.between(1.2, 2.4);
    let down = line_height + em * random.between(1.4, 2.6);
    let shear = random.between(0.25, 0.75);
    let (offset_u, offset_v) = (random.between(0.0, across), random.between(0.0, down));
    let opacity = random.between(0.30, 0.42);
    let jitter = em * 0.4;
    let row_seed = random.next();

    let soft = PER_PIXEL / scale * 0.5;
    let outline = EDGE - PER_PIXEL * 2.0;
    let sample = |u: f32, v: f32| -> f32 {
        let (u, v) = (u / scale, v / scale);
        if u < 0.0 || v < 0.0 || u >= (field_width - 1) as f32 || v >= (field_height - 1) as f32 {
            return 0.0;
        }
        let (x, y) = (u as usize, v as usize);
        let (fx, fy) = (u - x as f32, v - y as f32);
        let at = |x: usize, y: usize| field[y * field_width + x] as f32;
        let top = at(x, y) * (1.0 - fx) + at(x + 1, y) * fx;
        let bottom = at(x, y + 1) * (1.0 - fx) + at(x + 1, y + 1) * fx;
        top * (1.0 - fy) + bottom * fy
    };
    let smooth = |value: f32, edge: f32| ((value - edge + soft) / (2.0 * soft)).clamp(0.0, 1.0);
    for (x, y, pixel) in picture.enumerate_pixels_mut() {
        let (px, py) = (x as f32 - width as f32 / 2.0, y as f32 - height as f32 / 2.0);
        let v = -sin * px + cos * py + offset_v;
        let row = (v / down).floor();

        let nudge = Mulberry::new(row_seed ^ (row as i32 as u32).wrapping_mul(0x9e37_79b9)).between(-jitter, jitter);
        let u = cos * px + sin * py + offset_u + row * shear * across + nudge;
        let (cell_u, cell_v) = (u - (u / across).floor() * across, v - row * down);
        let value = sample(cell_u, cell_v);
        if value <= outline - soft {
            continue;
        }
        let ring = smooth(value, outline) * opacity * 0.55;
        let fill = smooth(value, EDGE) * opacity;
        for channel in pixel.0.iter_mut() {
            let darkened = *channel as f32 * (1.0 - ring);
            *channel = (darkened * (1.0 - fill) + 255.0 * fill).round() as u8;
        }
    }
}

pub fn proof(mut picture: RgbImage, watermark_text: &str, layout_seed: u32, mark_key: &[u8; 32], mark: u32) -> RgbImage {
    let (width, height) = picture.dimensions();
    if width.max(height) > MAX_EDGE {
        let ratio = MAX_EDGE as f32 / width.max(height) as f32;
        let (w, h) = (((width as f32 * ratio).round() as u32).max(1), ((height as f32 * ratio).round() as u32).max(1));
        picture = image::imageops::resize(&picture, w, h, image::imageops::FilterType::Lanczos3);
    }
    watermark(&mut picture, watermark_text, layout_seed);
    crate::mark::embed(&mut picture, mark, mark_key);
    picture
}

#[cfg(test)]
mod tests {
    use super::*;

    fn photograph(width: u32, height: u32) -> RgbImage {
        RgbImage::from_fn(width, height, |x, y| {
            let wave = ((x as f32 / 37.0).sin() * (y as f32 / 23.0).cos() * 60.0) as i32;
            image::Rgb([(x * 255 / width) as u8, (100 + wave).clamp(0, 255) as u8, (y * 255 / height) as u8])
        })
    }

    #[test]
    fn a_scrambled_proof_goes_back_together() {
        let picture = photograph(300, 197);
        let puzzle = scramble(&picture, 0xdead_beef);
        assert_eq!(puzzle.dimensions(), (320, 256));
        assert_eq!(unscramble(&puzzle, 0xdead_beef, 300, 197), picture);
        assert_ne!(unscramble(&puzzle, 0xdead_beee, 300, 197), picture, "another seed is another order");

        let decoded = |bytes: Vec<u8>| image::load_from_memory(&bytes).unwrap().to_rgb8();
        let back = unscramble(&decoded(jpeg(&puzzle, 90).unwrap()), 0xdead_beef, 300, 197);
        let plain = decoded(jpeg(&picture, 90).unwrap());
        let off = |a: &RgbImage| a.as_raw().iter().zip(picture.as_raw()).map(|(a, b)| (*a as f64 - *b as f64).abs()).sum::<f64>() / a.as_raw().len() as f64;
        assert!(off(&back) < off(&plain) * 1.15 + 0.2, "scrambled {} against plain {}", off(&back), off(&plain));
    }

    #[test]
    fn the_order_is_the_pages() {
        let mut random = Mulberry::new(1);
        assert_eq!([random.next(), random.next(), random.next()], [2_693_262_067, 11_749_833, 2_265_367_787]);
    }

    #[test]
    fn the_watermark_moves_between_photographs() {
        let plain = photograph(800, 533);
        let stamped = |seed: u32| {
            let mut picture = plain.clone();
            watermark(&mut picture, "© Studio Sam", seed);
            plain.as_raw().iter().zip(picture.as_raw()).map(|(a, b)| a != b).collect::<Vec<bool>>()
        };
        let (one, two) = (stamped(seed_of(&[b"share", b"1"])), stamped(seed_of(&[b"share", b"2"])));
        let covered = one.iter().filter(|&&on| on).count();
        assert!(covered > one.len() / 20, "the text covers the whole picture, not a corner: {covered}");
        let both = one.iter().zip(&two).filter(|(a, b)| **a && **b).count();
        assert!(both < covered / 2, "the same places on both: {both} of {covered}");
    }

    #[test]
    fn a_proofs_file_reserves_its_rights_and_names_nothing() {
        let bytes = jpeg(&photograph(640, 480), 85).unwrap();
        let text = String::from_utf8_lossy(&bytes);
        assert!(text.contains("DMI-PROHIBITED-AIMLTRAINING"));
        for nothing in ["Exif", "dc:creator", "dc:rights", "dc:subject", "PersonInImage", "xmp:Rating"] {
            assert!(!text.contains(nothing), "{nothing}");
        }
        assert_eq!(image::load_from_memory(&bytes).unwrap().to_rgb8().dimensions(), (640, 480));
    }

    #[test]
    fn a_proof_is_never_larger_than_the_cap() {
        let proof = proof(photograph(3000, 2000), "© Sam", 7, &[3; 32], 42);
        assert_eq!(proof.dimensions(), (2048, 1365));
    }
}
