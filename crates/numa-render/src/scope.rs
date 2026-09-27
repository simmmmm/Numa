use image::RgbImage;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Kind {
    #[default]
    Histogram,
    Waveform,
    Parade,
}

impl Kind {
    pub const ALL: [Kind; 3] = [Kind::Histogram, Kind::Waveform, Kind::Parade];

    pub fn name(self) -> &'static str {
        match self {
            Kind::Histogram => "Histogram",
            Kind::Waveform => "Waveform",
            Kind::Parade => "Parade",
        }
    }

    pub fn from_name(name: &str) -> Kind {
        Kind::ALL.into_iter().find(|kind| kind.name().eq_ignore_ascii_case(name)).unwrap_or_default()
    }
}

pub const COLUMNS: usize = 360;

pub const LEVELS: usize = 128;

#[derive(Debug, Clone)]
pub struct Scope {
    pub width: usize,
    pub height: usize,
    pub rgba: Vec<u8>,
}

pub fn counts(image: &RgbImage, parade: bool) -> Vec<[u32; 3]> {
    let mut counts = vec![[0u32; 3]; COLUMNS * LEVELS];
    let (width, height) = (image.width() as usize, image.height() as usize);
    if width == 0 || height == 0 {
        return counts;
    }

    let panel = if parade { COLUMNS / 3 } else { COLUMNS };
    let row = |level: u8| (LEVELS - 1) - level as usize * LEVELS / 256;

    let across = width.max(panel);
    for y in (0..height).step_by(2) {
        for step in 0..across {
            let pixel = image.get_pixel((step * width / across) as u32, y as u32).0;
            let column = step * panel / across;
            if parade {
                for channel in 0..3 {
                    counts[row(pixel[channel]) * COLUMNS + channel * panel + column][channel] += 1;
                }
            } else {
                let luma = 0.2126 * pixel[0] as f32 + 0.7152 * pixel[1] as f32 + 0.0722 * pixel[2] as f32;
                let cell = &mut counts[row(luma.round() as u8) * COLUMNS + column];
                for count in cell.iter_mut() {
                    *count += 1;
                }
            }
        }
    }
    counts
}

pub fn of(image: &RgbImage, kind: Kind) -> Option<Scope> {
    if kind == Kind::Histogram {
        return None;
    }
    let parade = kind == Kind::Parade;
    let counts = counts(image, parade);

    let columns = if parade { COLUMNS / 3 } else { COLUMNS };
    let samples = (image.width() as f32 * image.height().div_ceil(2) as f32 / columns as f32).max(1.0);
    let typical = (samples / LEVELS as f32 * 4.0).max(1.0);
    let mut rgba = Vec::with_capacity(COLUMNS * LEVELS * 4);
    for (index, cell) in counts.iter().enumerate() {
        let level = LEVELS - 1 - index / COLUMNS;
        let graticule = [0, 25, 50, 75, 100].iter().any(|ire| level == ire * (LEVELS - 1) / 100);
        let base = if graticule { 60.0 } else { 18.0 };
        let glow = cell.map(|count| 1.0 - (-(count as f32) / typical).exp());
        let out: [f32; 3] = match parade {
            true => std::array::from_fn(|channel| glow[channel] * 255.0 + base * (1.0 - glow[channel])),
            false => [glow[0] * 235.0 + base; 3],
        };
        rgba.extend(out.map(|value| value.min(255.0) as u8));
        rgba.push(255);
    }
    Some(Scope { width: COLUMNS, height: LEVELS, rgba })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn column_levels(counts: &[[u32; 3]], column: usize, channel: usize) -> Vec<usize> {
        (0..LEVELS).filter(|row| counts[row * COLUMNS + column][channel] > 0).collect()
    }

    #[test]
    fn a_flat_grey_is_one_line() {
        let grey = RgbImage::from_pixel(100, 10, image::Rgb([128, 128, 128]));
        let counts = counts(&grey, false);
        let rows: Vec<usize> = (0..LEVELS).filter(|row| counts[row * COLUMNS..(row + 1) * COLUMNS].iter().any(|c| c[0] > 0)).collect();
        assert_eq!(rows, vec![LEVELS - 1 - 64], "one level, halfway up");

        assert!((0..COLUMNS).all(|column| counts[rows[0] * COLUMNS + column][0] > 0));
    }

    #[test]
    fn across_is_the_photographs_own_columns() {

        let image = RgbImage::from_fn(200, 4, |x, _| if x < 100 { image::Rgb([0, 0, 0]) } else { image::Rgb([255, 255, 255]) });
        let counts = counts(&image, false);
        assert_eq!(column_levels(&counts, 0, 0), vec![LEVELS - 1], "the left at the bottom");
        assert_eq!(column_levels(&counts, COLUMNS - 1, 0), vec![0], "the right at the top");
    }

    #[test]
    fn the_parade_splits_the_channels() {
        let red = RgbImage::from_pixel(90, 4, image::Rgb([255, 0, 0]));
        let counts = counts(&red, true);
        let third = COLUMNS / 3;
        assert_eq!(column_levels(&counts, 5, 0), vec![0], "red at the top of its panel");
        assert_eq!(column_levels(&counts, third + 5, 1), vec![LEVELS - 1], "green at the bottom of its");
        assert_eq!(column_levels(&counts, 2 * third + 5, 2), vec![LEVELS - 1], "and blue");

        assert!(column_levels(&counts, third + 5, 0).is_empty());
    }

    #[test]
    fn the_picture_is_the_size_it_says() {
        let image = RgbImage::from_pixel(64, 48, image::Rgb([30, 140, 220]));
        assert!(of(&image, Kind::Histogram).is_none());
        let scope = of(&image, Kind::Parade).unwrap();
        assert_eq!(scope.rgba.len(), scope.width * scope.height * 4);
        assert_eq!(Kind::from_name("parade"), Kind::Parade);
        assert_eq!(Kind::from_name("nonsense"), Kind::Histogram);
    }
}
