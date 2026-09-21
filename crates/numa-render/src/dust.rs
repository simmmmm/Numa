use image::RgbImage;
use numa_core::plane::{blur, Plane};
use numa_core::retouch::{Kind, Spot};

const SCALES: [usize; 4] = [3, 5, 8, 12];

const SMOOTH: f32 = 0.04;

const DEEPEST: f32 = 0.3;

const ALONE: f32 = 0.25;

const MOST: usize = 60;

pub fn find(frame: &RgbImage) -> Vec<Spot> {
    let (width, height) = (frame.width() as usize, frame.height() as usize);
    if width < 32 || height < 32 {
        return Vec::new();
    }
    let long_edge = width.max(height) as f32;
    let stops = Plane::new(
        width,
        height,
        frame
            .pixels()
            .map(|pixel| {
                let [r, g, b] = pixel.0.map(|code| numa_core::tone::scene_value_for(code as f32 / 255.0));
                (0.2126 * r + 0.7152 * g + 0.0722 * b).max(1e-4).log2()
            })
            .collect(),
    );

    let fine = blur(&blur(&stops, 1), 1);
    let movement = Plane::new(width, height, stops.data.iter().zip(&fine.data).map(|(a, b)| (a - b).abs()).collect());
    let mut sorted = movement.data.clone();
    sorted.sort_by(f32::total_cmp);
    let grain = sorted[sorted.len() / 2].max(1e-3);

    let mut found: Vec<(f32, usize, usize, usize)> = Vec::new();
    for radius in SCALES {
        let inner = blur(&blur(&stops, radius), radius);
        let outer = blur(&blur(&stops, 3 * radius), 3 * radius);

        let own = Plane::new(width, height, stops.data.iter().zip(&inner.data).map(|(a, b)| (a - b).abs()).collect());
        let texture = blur(&blur(&own, 4 * radius), 4 * radius);
        let busy = blur(&blur(&movement, 4 * radius), 4 * radius);

        let difference = Plane::new(width, height, inner.data.iter().zip(&outer.data).map(|(a, b)| (a - b).abs()).collect());
        let around = blur(&blur(&difference, 4 * radius), 4 * radius);

        let floor = -(0.02f32).max(grain * 3.0 / (radius as f32).sqrt());
        let reach = 3 * radius;
        for y in reach..height - reach {
            for x in reach..width - reach {
                let at = y * width + x;
                let depth = inner.data[at] - outer.data[at];

                if depth > floor
                    || busy.data[at] > (grain * 1.6).min(SMOOTH)
                    || texture.data[at] > SMOOTH
                    || depth > -4.0 * texture.data[at]
                    || depth < -DEEPEST
                    || around.data[at] > -depth * ALONE
                    || outer.data[at] < -4.0
                {
                    continue;
                }

                let lowest = (y - radius..=y + radius)
                    .flat_map(|ny| (x - radius..=x + radius).map(move |nx| ny * width + nx))
                    .all(|near| inner.data[near] - outer.data[near] >= depth);

                let colour = frame.get_pixel(x as u32, y as u32).0.map(|code| code as f32 / 255.0);
                if lowest && crate::beautify::skin_like(colour) < 0.5 {
                    found.push((depth, x, y, radius));
                }
            }
        }
    }

    found.sort_by(|a, b| a.0.total_cmp(&b.0));
    let mut kept: Vec<(usize, usize, usize)> = Vec::new();
    for (_, x, y, radius) in found {
        let apart = |&(kx, ky, kr): &(usize, usize, usize)| {
            let gap = (x as f32 - kx as f32).hypot(y as f32 - ky as f32);
            gap > 2.0 * (radius.max(kr) as f32)
        };
        if kept.iter().all(apart) {
            kept.push((x, y, radius));
        }
        if kept.len() == MOST {
            break;
        }
    }

    kept.iter()
        .map(|&(x, y, radius)| {
            let size = 2.2 * radius as f32;
            let from = source(&movement, &kept, x, y, size);
            Spot {
                at: [(x as f32 + 0.5) / width as f32, (y as f32 + 0.5) / height as f32],
                from: [from.0 / width as f32, from.1 / height as f32],
                radius: size / long_edge,
                feather: 0.6,
                opacity: 1.0,
                heal: true,
                kind: Kind::Patch,
            }
        })
        .collect()
}

fn source(movement: &Plane, dust: &[(usize, usize, usize)], x: usize, y: usize, size: f32) -> (f32, f32) {
    let (width, height) = (movement.width as f32, movement.height as f32);
    let distance = 3.0 * size;
    (0..8)
        .map(|step| {
            let angle = step as f32 * std::f32::consts::FRAC_PI_4;
            (x as f32 + angle.cos() * distance, y as f32 + angle.sin() * distance)
        })
        .filter(|&(sx, sy)| sx >= size && sy >= size && sx < width - size && sy < height - size)
        .filter(|&(sx, sy)| dust.iter().all(|&(dx, dy, _)| (sx - dx as f32).hypot(sy - dy as f32) > 2.0 * size))
        .map(|(sx, sy)| {
            let reach = size as isize;
            let mut sum = 0.0;
            for oy in -reach..=reach {
                for ox in -reach..=reach {
                    let (px, py) = ((sx as isize + ox) as usize, (sy as isize + oy) as usize);
                    sum += movement.data[py * movement.width + px];
                }
            }
            (sum, (sx, sy))
        })
        .min_by(|a, b| a.0.total_cmp(&b.0))
        .map(|(_, at)| at)
        .unwrap_or((x as f32 + distance.min(width - 1.0 - x as f32), y as f32))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn noise(seed: &mut u32) -> f32 {
        *seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
        (*seed >> 8) as f32 / (1 << 24) as f32 - 0.5
    }

    #[test]
    fn specks_in_a_sky_are_found() {
        let (width, height) = (600u32, 400u32);
        let specks = [(100.0, 80.0, 6.0), (300.0, 200.0, 10.0), (500.0, 120.0, 4.0), (200.0, 320.0, 8.0), (450.0, 330.0, 5.0)];
        let mut seed = 7;
        let sky = RgbImage::from_fn(width, height, |x, y| {
            let base = 0.62 - 0.1 * y as f32 / height as f32;
            let shade: f32 = specks
                .iter()
                .map(|&(sx, sy, r)| {
                    let d = (x as f32 - sx).hypot(y as f32 - sy) / r;
                    1.0 - 0.06 * (-d * d).exp()
                })
                .product();
            let value = (base * shade + noise(&mut seed) * 0.008) * 255.0;
            image::Rgb([value as u8, (value * 1.02) as u8, (value * 1.1).min(255.0) as u8])
        });
        let spots = find(&sky);
        for &(sx, sy, _) in &specks {
            let near = spots.iter().any(|spot| (spot.at[0] * width as f32 - sx).hypot(spot.at[1] * height as f32 - sy) < 6.0);
            assert!(near, "the speck at {sx},{sy} was missed: {} found", spots.len());
        }
        assert!(spots.len() <= specks.len() + 1, "found {} in a sky with five", spots.len());
        for spot in &spots {
            assert!(spot.heal && (spot.from != spot.at), "a speck to heal, from beside it");
        }
    }

    #[test]
    fn texture_is_not_dust() {
        let mut seed = 3;
        let leaves = RgbImage::from_fn(400, 300, |_, _| {
            let value = 90.0 + noise(&mut seed) * 120.0;
            image::Rgb([value as u8 / 2, value as u8, value as u8 / 3])
        });
        let blurred = image::imageops::blur(&leaves, 1.5);
        assert!(find(&blurred).len() <= 1, "found {} specks in leaves", find(&blurred).len());
    }
}
