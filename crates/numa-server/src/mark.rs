use image::{ImageBuffer, Luma, RgbImage};
use sha2::{Digest, Sha256};

pub const BITS: usize = 48;
const GRID: u32 = 512;
const BLOCKS: usize = (GRID as usize / 8) * (GRID as usize / 8);

const SLOTS: [(usize, usize); 5] = [(1, 1), (0, 2), (2, 0), (1, 2), (2, 1)];

const MARGIN: f32 = 5.0;

struct Stream {
    key: [u8; 32],
    counter: u64,
    left: Vec<u32>,
}

impl Stream {
    fn next(&mut self) -> u32 {
        if self.left.is_empty() {
            let block = Sha256::new().chain_update(self.key).chain_update(self.counter.to_le_bytes()).finalize();
            self.counter += 1;
            self.left = block.chunks(4).map(|four| u32::from_le_bytes(four.try_into().expect("four bytes"))).collect();
        }
        self.left.pop().expect("eight to a block")
    }
}

fn plan(key: &[u8; 32]) -> (Vec<usize>, Vec<f32>) {
    let mut random = Stream { key: *key, counter: 0, left: Vec::new() };
    let chips = BLOCKS * SLOTS.len();
    let mut bit: Vec<usize> = (0..chips).map(|chip| chip % BITS).collect();
    for i in (1..chips).rev() {
        bit.swap(i, (random.next() as u64 * (i as u64 + 1) >> 32) as usize);
    }
    let sign = (0..chips).map(|_| if random.next() & 1 == 0 { 1.0 } else { -1.0 }).collect();
    (bit, sign)
}

fn basis() -> [[f32; 8]; 8] {
    let mut basis = [[0.0; 8]; 8];
    for (frequency, row) in basis.iter_mut().enumerate() {
        let scale = if frequency == 0 { (1.0f32 / 8.0).sqrt() } else { (2.0f32 / 8.0).sqrt() };
        for (position, value) in row.iter_mut().enumerate() {
            *value = scale * ((2 * position + 1) as f32 * frequency as f32 * std::f32::consts::PI / 16.0).cos();
        }
    }
    basis
}

fn check(id: u32) -> u16 {
    (id.wrapping_mul(0x9e37_79b1).rotate_left(13) ^ 0x5bd1_e995) as u16
}

fn payload(id: u32) -> [f32; BITS] {
    let word = (id as u64) | ((check(id) as u64) << 32);
    std::array::from_fn(|bit| if word >> bit & 1 == 1 { 1.0 } else { -1.0 })
}

fn measure(picture: &RgbImage, bit: &[usize], sign: &[f32], basis: &[[f32; 8]; 8]) -> [f32; BITS] {

    let luma: ImageBuffer<Luma<f32>, Vec<f32>> = ImageBuffer::from_fn(picture.width(), picture.height(), |x, y| {
        let [r, g, b] = picture.get_pixel(x, y).0.map(f32::from);
        Luma([(0.299 * r + 0.587 * g + 0.114 * b) / 255.0])
    });
    let grid = image::imageops::resize(&luma, GRID, GRID, image::imageops::FilterType::Triangle);
    let (mut sum, mut count) = ([0.0f32; BITS], [0u32; BITS]);
    let across = GRID as usize / 8;
    for block in 0..BLOCKS {
        let (bx, by) = ((block % across) * 8, (block / across) * 8);
        for (slot, &(u, v)) in SLOTS.iter().enumerate() {
            let mut coefficient = 0.0;
            for y in 0..8 {
                for x in 0..8 {
                    coefficient += 255.0 * grid.get_pixel((bx + x) as u32, (by + y) as u32).0[0] * basis[u][x] * basis[v][y];
                }
            }
            let chip = block * SLOTS.len() + slot;
            sum[bit[chip]] += sign[chip] * coefficient;
            count[bit[chip]] += 1;
        }
    }
    std::array::from_fn(|b| sum[b] / count[b].max(1) as f32)
}

pub fn embed(picture: &mut RgbImage, id: u32, key: &[u8; 32]) {
    let (bit, sign) = plan(key);
    let basis = basis();
    let wanted = payload(id);
    let (width, height) = picture.dimensions();
    let across = GRID as usize / 8;

    for _ in 0..4 {
        let now = measure(picture, &bit, &sign, &basis);
        let short: [f32; BITS] = std::array::from_fn(|b| (MARGIN * 1.15 - wanted[b] * now[b]).max(0.0) * wanted[b]);
        if short.iter().all(|&s| s == 0.0) {
            break;
        }
        let mut delta = vec![0.0f32; (GRID * GRID) as usize];
        for block in 0..BLOCKS {
            let (bx, by) = ((block % across) * 8, (block / across) * 8);
            for (slot, &(u, v)) in SLOTS.iter().enumerate() {
                let chip = block * SLOTS.len() + slot;
                let amount = sign[chip] * short[bit[chip]];
                if amount == 0.0 {
                    continue;
                }
                for y in 0..8 {
                    for x in 0..8 {
                        delta[(by + y) * GRID as usize + bx + x] += amount * basis[u][x] * basis[v][y];
                    }
                }
            }
        }

        let at = |gx: f32, gy: f32| {
            let (gx, gy) = (gx.clamp(0.0, (GRID - 1) as f32), gy.clamp(0.0, (GRID - 1) as f32));
            let (x0, y0) = (gx as usize, gy as usize);
            let (x1, y1) = ((x0 + 1).min(GRID as usize - 1), (y0 + 1).min(GRID as usize - 1));
            let (fx, fy) = (gx - x0 as f32, gy - y0 as f32);
            let row = |y: usize| delta[y * GRID as usize + x0] * (1.0 - fx) + delta[y * GRID as usize + x1] * fx;
            row(y0) * (1.0 - fy) + row(y1) * fy
        };
        for (x, y, pixel) in picture.enumerate_pixels_mut() {
            let change = at((x as f32 + 0.5) * GRID as f32 / width as f32 - 0.5, (y as f32 + 0.5) * GRID as f32 / height as f32 - 0.5);
            for channel in pixel.0.iter_mut() {
                *channel = (*channel as f32 + change).round().clamp(0.0, 255.0) as u8;
            }
        }
    }
}

pub fn read(picture: &RgbImage, key: &[u8; 32]) -> Option<(u32, f32)> {
    let (bit, sign) = plan(key);
    let now = measure(picture, &bit, &sign, &basis());
    let word = now.iter().enumerate().fold(0u64, |word, (b, &value)| word | ((value > 0.0) as u64) << b);
    let id = word as u32;
    let clearly = now.iter().map(|value| value.abs()).fold(f32::MAX, f32::min) / MARGIN;
    (check(id) as u64 == word >> 32).then_some((id, clearly))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn photograph(width: u32, height: u32) -> RgbImage {
        let mut random = crate::proof::Mulberry::new(9);
        RgbImage::from_fn(width, height, |x, y| {
            let (fx, fy) = (x as f32 / width as f32, y as f32 / height as f32);
            let sky = 200.0 - 120.0 * fy;
            let hill = if fy > 0.55 + 0.1 * (fx * 9.0).sin() { -90.0 } else { 0.0 };
            let detail = 25.0 * ((fx * 61.0).sin() * (fy * 47.0).cos());
            let grain = random.between(-12.0, 12.0);
            let value = |tint: f32| (sky + hill + detail + grain + tint).clamp(0.0, 255.0) as u8;
            image::Rgb([value(10.0), value(0.0), value(-25.0)])
        })
    }

    #[test]
    fn the_mark_survives_jpeg_and_halving() {
        let key = [7u8; 32];
        let plain = photograph(2048, 1365);
        let mut marked = plain.clone();
        embed(&mut marked, 0xC0FF_EE42, &key);
        let squared = marked.as_raw().iter().zip(plain.as_raw()).map(|(a, b)| (*a as f64 - *b as f64).powi(2)).sum::<f64>();
        let psnr = 10.0 * (255.0f64.powi(2) / (squared / plain.as_raw().len() as f64)).log10();
        assert!(psnr > 38.0, "the mark is unseen: PSNR {psnr:.1} dB");

        let halved = image::imageops::resize(&marked, 1024, 683, image::imageops::FilterType::Triangle);
        let jpeg = crate::proof::jpeg(&halved, 75).unwrap();
        let leaked = image::load_from_memory(&jpeg).unwrap().to_rgb8();
        let (id, clearly) = read(&leaked, &key).expect("the mark is read back");
        assert_eq!(id, 0xC0FF_EE42);
        eprintln!("PSNR {psnr:.1} dB; after JPEG 75 and a halving the weakest bit is at {clearly:.2} of its margin");
        assert!(read(&plain, &key).is_none(), "an unmarked picture carries no share");
        assert!(read(&leaked, &[8u8; 32]).is_none(), "another server's key reads nothing");
    }

    #[test]
    #[ignore]
    fn marks_on_photographs() {
        let folder = std::env::var("NUMA_MARK_SAMPLES").expect("NUMA_MARK_SAMPLES");
        let key = [7u8; 32];
        let (mut read_back, mut harder, mut psnrs, mut weakest) = (0, 0, Vec::new(), f32::MAX);
        let files: Vec<_> = std::fs::read_dir(folder).unwrap().flatten().map(|entry| entry.path()).take(40).collect();
        for (n, path) in files.iter().enumerate() {
            let Ok(picture) = image::open(path) else { continue };
            let plain = picture.to_rgb8();
            let mut marked = plain.clone();
            embed(&mut marked, n as u32 * 7919, &key);
            let squared = marked.as_raw().iter().zip(plain.as_raw()).map(|(a, b)| (*a as f64 - *b as f64).powi(2)).sum::<f64>();
            psnrs.push(10.0 * (255.0f64.powi(2) / (squared / plain.as_raw().len() as f64)).log10());
            let (w, h) = marked.dimensions();
            let halved = image::imageops::resize(&marked, w / 2, h / 2, image::imageops::FilterType::Triangle);
            let leaked = image::load_from_memory(&crate::proof::jpeg(&halved, 75).unwrap()).unwrap().to_rgb8();
            if let Some((id, clearly)) = read(&leaked, &key) {
                read_back += (id == n as u32 * 7919) as usize;
                weakest = weakest.min(clearly);
            }

            let small = image::imageops::resize(&marked, w / 3, h / 3, image::imageops::FilterType::Triangle);
            let worse = image::load_from_memory(&crate::proof::jpeg(&small, 50).unwrap()).unwrap().to_rgb8();
            harder += read(&worse, &key).is_some_and(|(id, _)| id == n as u32 * 7919) as usize;
        }
        psnrs.sort_by(f64::total_cmp);
        eprintln!("{read_back} of {} read back after JPEG 75 and a halving; weakest bit {weakest:.2} of its margin; {harder} after a third and JPEG 50; PSNR median {:.1}, lowest {:.1} dB", psnrs.len(), psnrs[psnrs.len() / 2], psnrs[0]);
    }
}
