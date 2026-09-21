use crate::image::LinearImage;

pub fn manual_lens_profile(distortion: f32, vignetting: f32) -> LensProfile {
    let radii: Vec<f32> = (1..=9).map(|step| step as f32 * 0.125).collect();
    LensProfile {
        transmission: radii.iter().map(|r| 1.0 - 0.5 * (vignetting / 100.0) * r * r).collect(),
        distortion: radii.iter().map(|r| -10.0 * (distortion / 100.0) * r * r).collect(),
        red: vec![0.0; radii.len()],
        blue: vec![0.0; radii.len()],
        radii,
    }
}

#[derive(Debug, Clone)]
pub struct LensProfile {

    pub radii: Vec<f32>,

    pub transmission: Vec<f32>,

    pub distortion: Vec<f32>,

    pub red: Vec<f32>,
    pub blue: Vec<f32>,
}

impl LensProfile {

    pub fn at(table: &[f32], radii: &[f32], radius: f32) -> f32 {
        if table.is_empty() {
            return 0.0;
        }
        if radius <= radii[0] {
            return table[0];
        }
        for window in 1..radii.len().min(table.len()) {
            if radius <= radii[window] {
                let (r0, r1) = (radii[window - 1], radii[window]);
                let fraction = ((radius - r0) / (r1 - r0).max(1e-6)).clamp(0.0, 1.0);
                return table[window - 1] + (table[window] - table[window - 1]) * fraction;
            }
        }
        table[table.len() - 1]
    }

    pub fn source_radius(&self, radius: f32) -> [f32; 3] {
        let distortion = 1.0 + Self::at(&self.distortion, &self.radii, radius) / 100.0;
        [
            distortion * (1.0 + Self::at(&self.red, &self.radii, radius)),
            distortion,
            distortion * (1.0 + Self::at(&self.blue, &self.radii, radius)),
        ]
    }

    pub fn bends_anything(&self) -> bool {
        self.distortion.iter().any(|value| value.abs() > 0.01)
            || self.red.iter().chain(self.blue.iter()).any(|value| value.abs() > 1e-5)
    }

    pub fn corrects_anything(&self) -> bool {

        self.bends_anything()
            || self.transmission.iter().any(|value| (value - 1.0).abs() > 1e-3)
    }

    pub fn gain(&self, radius: f32) -> f32 {
        if self.radii.is_empty() {
            return 1.0;
        }

        if radius <= 0.0 {
            return 1.0;
        }

        if radius <= self.radii[0] {
            let fraction = (radius / self.radii[0]).clamp(0.0, 1.0);
            let transmission = 1.0 + (self.transmission[0] - 1.0) * fraction;
            return 1.0 / transmission.max(0.05);
        }

        for window in 1..self.radii.len() {
            if radius <= self.radii[window] {
                let (r0, r1) = (self.radii[window - 1], self.radii[window]);
                let (t0, t1) = (self.transmission[window - 1], self.transmission[window]);
                let fraction = ((radius - r0) / (r1 - r0).max(1e-6)).clamp(0.0, 1.0);
                return 1.0 / (t0 + (t1 - t0) * fraction).max(0.05);
            }
        }

        1.0 / self.transmission[self.transmission.len() - 1].max(0.05)
    }
}

pub fn correct_vignetting(image: &mut LinearImage, profile: &LensProfile) {
    use rayon::prelude::*;

    let (width, height) = (image.width as f32, image.height as f32);
    let (centre_x, centre_y) = (width / 2.0, height / 2.0);
    let half_diagonal = (centre_x * centre_x + centre_y * centre_y).sqrt().max(1.0);

    let row_width = image.width as usize * 3;
    image
        .data
        .par_chunks_mut(row_width)
        .enumerate()
        .for_each(|(y, row)| {
            let dy = y as f32 + 0.5 - centre_y;

            for x in 0..width as usize {
                let dx = x as f32 + 0.5 - centre_x;
                let gain = profile.gain((dx * dx + dy * dy).sqrt() / half_diagonal);

                for channel in &mut row[x * 3..x * 3 + 3] {
                    *channel *= gain;
                }
            }
        });
}

pub fn correct_geometry(image: &LinearImage, profile: &LensProfile) -> LinearImage {
    use rayon::prelude::*;

    let (width, height) = (image.width as usize, image.height as usize);
    if width < 2 || height < 2 {
        return image.clone();
    }

    let (centre_x, centre_y) = (width as f32 / 2.0, height as f32 / 2.0);
    let half_diagonal = (centre_x * centre_x + centre_y * centre_y).sqrt().max(1.0);

    let overshoot = (0..=32)
        .map(|step| step as f32 / 32.0)
        .map(|radius| {
            let scales = profile.source_radius(radius);
            radius * scales[0].max(scales[1]).max(scales[2])
        })
        .fold(1.0f32, f32::max);
    let fit = 1.0 / overshoot.max(1.0);

    let mut data = vec![0.0f32; width * height * 3];
    data.par_chunks_mut(width * 3)
        .enumerate()
        .for_each(|(y, row)| {
            let dy = (y as f32 + 0.5 - centre_y) * fit;
            for x in 0..width {
                let dx = (x as f32 + 0.5 - centre_x) * fit;
                let radius = (dx * dx + dy * dy).sqrt() / half_diagonal;
                let scales = profile.source_radius(radius);

                for (channel, scale) in scales.iter().enumerate() {
                    let sx = centre_x + dx * scale - 0.5;
                    let sy = centre_y + dy * scale - 0.5;
                    row[x * 3 + channel] = sample(image, sx, sy, channel);
                }
            }
        });

    LinearImage {
        white_point: image.white_point,
        width: image.width,
        height: image.height,
        data,
        profile: image.profile.clone(),
        clip: image.clip,
        rendering: image.rendering.clone(),
        film_mode: image.film_mode.clone(),
    }
}

fn sample(image: &LinearImage, x: f32, y: f32, channel: usize) -> f32 {
    let (width, height) = (image.width as isize, image.height as isize);
    let (x0, y0) = (x.floor(), y.floor());
    let (fx, fy) = (x - x0, y - y0);

    let at = |x: isize, y: isize| {
        let x = x.clamp(0, width - 1) as usize;
        let y = y.clamp(0, height - 1) as usize;
        image.data[(y * width as usize + x) * 3 + channel]
    };

    let (x0, y0) = (x0 as isize, y0 as isize);
    let top = at(x0, y0) + (at(x0 + 1, y0) - at(x0, y0)) * fx;
    let bottom = at(x0, y0 + 1) + (at(x0 + 1, y0 + 1) - at(x0, y0 + 1)) * fx;
    top + (bottom - top) * fy
}
