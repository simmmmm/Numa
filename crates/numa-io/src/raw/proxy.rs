use rawler::imgop::sensor::SensorType;
use rawler::pixarray::Color2D;
use rawler::rawimage::{RawImageData, RawPhotometricInterpretation};
use rayon::prelude::*;
use std::path::Path;

use numa_core::lens::{geometry_fit, Centre, LensProfile};

use super::{dump_without_vignetting, lens_profile, Demosaic, Laps, BASELINE_EV, EXPOSURE_STRIDE};

pub(super) type Small = (Color2D<f32, 3>, (usize, usize), Vec<f32>);

pub(super) fn develop(raw: rawler::RawImage, path: &Path, demosaic: Demosaic, laps: &mut Laps) -> Result<Small, rawler::RawImage> {
    let Demosaic::Proxy(edge) = demosaic else { return Err(raw) };
    let Some(frame) = Frame::of(&raw, edge) else { return Err(raw) };

    let lens = lens_profile(path);
    laps.lap("lens lookup");
    let pixels = match &raw.data {
        RawImageData::Integer(mosaic) => frame.shrink(mosaic, lens.as_ref()),
        RawImageData::Float(mosaic) => frame.shrink(mosaic, lens.as_ref()),
    };
    laps.lap("develop small");
    let samples = match &raw.data {
        RawImageData::Integer(mosaic) => frame.samples(mosaic, lens.as_ref()),
        RawImageData::Float(mosaic) => frame.samples(mosaic, lens.as_ref()),
    };
    laps.lap("exposure samples");
    Ok((Color2D::new_with(pixels, frame.out.0, frame.out.1), frame.size, samples))
}

struct Frame {

    stride: usize,

    at: (usize, usize),
    size: (usize, usize),

    out: (usize, usize),

    period: (usize, usize),
    sites: Vec<Vec<usize>>,

    colours: Vec<u8>,

    black: [f32; 4],
    scale: [f32; 4],
}

const TENT: [f32; 3] = [2.0, 1.0, 2.0];

const MOST_COLUMNS: usize = 32;

impl Frame {
    fn of(raw: &rawler::RawImage, edge: u32) -> Option<Self> {
        let RawPhotometricInterpretation::Cfa(config) = &raw.photometric else { return None };
        if !matches!(config.sensor, SensorType::Bayer | SensorType::Xtrans) || !config.cfa.is_rgb() || raw.fuji_rotation_width.is_some() || raw.cpp != 1 {
            return None;
        }

        let crop = raw.crop_area.or(raw.active_area);
        let (at, size) = crop.map_or(((0, 0), (raw.width, raw.height)), |rect| ((rect.p.x, rect.p.y), (rect.d.w, rect.d.h)));
        if at.0 + size.0 > raw.width || at.1 + size.1 > raw.height {
            return None;
        }

        let longest = size.0.max(size.1);
        if longest <= edge as usize || size.0.min(size.1) == 0 || longest > edge as usize * 12 {
            return None;
        }
        let scale = edge as f64 / longest as f64;
        let out = (
            ((size.0 as f64 * scale).round() as usize).max(1),
            ((size.1 as f64 * scale).round() as usize).max(1),
        );

        let cfa = &config.cfa;
        let period = (cfa.width, cfa.height);

        if period.0 % 2 != 0 || period.1 % 2 != 0 {
            return None;
        }
        let sites: Vec<Vec<usize>> = (0..period.1 * 3)
            .map(|at| (0..period.0).filter(|col| cfa.color_at(at / 3, *col) == at % 3).collect())
            .collect();

        let (black, white) = (raw.blacklevel.as_bayer_array(), raw.whitelevel.as_bayer_array());
        let scale = [0, 1, 2, 3].map(|i| 1.0 / (white[i] - black[i]).max(f32::MIN_POSITIVE));
        let colours = (0..period.1).flat_map(|row| (0..period.0).map(move |col| cfa.color_at(row, col) as u8)).collect();
        Some(Self { stride: raw.width, at, size, out, period, sites, colours, black, scale })
    }

    fn shrink<T: Copy + Sync + Into<f32>>(&self, mosaic: &[T], lens: Option<&LensProfile>) -> Vec<[f32; 3]> {
        let (width, height) = (self.size.0 as f32, self.size.1 as f32);
        let (centre_x, centre_y) = (width / 2.0, height / 2.0);
        let half_diagonal = (centre_x * centre_x + centre_y * centre_y).sqrt().max(1.0);
        let bends = lens.filter(|profile| profile.bends_anything());
        let fit = bends.map_or(1.0, geometry_fit);
        let falloff = lens.filter(|_| !dump_without_vignetting());
        let ratio = (self.size.0 as f64 / self.out.0 as f64, self.size.1 as f64 / self.out.1 as f64);

        let reach = (
            0.5 * ratio.0.min(ratio.1).ceil() as f32 * fit * 0.9,
            0.5 * (ratio.0.max(ratio.1).ceil() as f32 + 1.0) * fit * 1.1,
        );
        let table = Table::new(reach);

        let mut pixels = vec![[0.0f32; 3]; self.out.0 * self.out.1];
        pixels.par_chunks_mut(self.out.0).enumerate().for_each(|(y, row)| {
            let (top, bottom) = footprint(y, ratio.1, self.size.1);
            let dy = ((top + bottom) / 2.0 - centre_y) * fit;
            for (x, pixel) in row.iter_mut().enumerate() {
                let (left, right) = footprint(x, ratio.0, self.size.0);
                let dx = ((left + right) / 2.0 - centre_x) * fit;
                let radius = (dx * dx + dy * dy).sqrt() / half_diagonal;
                let scales = bends.map_or([1.0; 3], |profile| profile.source_radius(radius));
                for (channel, scale) in scales.into_iter().enumerate() {
                    let centre = (centre_x + dx * scale, centre_y + dy * scale);
                    let half = ((right - left) * fit * scale / 2.0, (bottom - top) * fit * scale / 2.0);
                    let gain = falloff.map_or(1.0, |profile| profile.gain(radius * scale));
                    pixel[channel] = self.mean(mosaic, &table, channel, centre, half) * gain;
                }
            }
        });
        pixels
    }

    fn samples<T: Copy + Sync + Into<f32>>(&self, mosaic: &[T], lens: Option<&LensProfile>) -> Vec<f32> {
        let (width, height) = self.size;
        let centre = Centre::of(width, height);
        let bends = lens.filter(|profile| profile.bends_anything());
        let fit = bends.map_or(1.0, geometry_fit);
        let falloff = lens.filter(|_| !dump_without_vignetting());
        let baseline = 2.0f32.powf(BASELINE_EV);
        let nearest = |at: f32, size: usize| (at.round().max(0.0) as usize).min(size - 1);
        (0..(width * height).div_ceil(EXPOSURE_STRIDE))
            .into_par_iter()
            .flat_map_iter(|at| {
                let (x, y) = ((at * EXPOSURE_STRIDE) % width, (at * EXPOSURE_STRIDE) / width);
                let sources = match bends {
                    Some(profile) => centre.source(profile, fit, x, y).map(|(sx, sy)| (nearest(sx, width), nearest(sy, height))),
                    None => [(x, y); 3],
                };
                let (gx, gy) = sources[1];
                let gain = baseline * falloff.map_or(1.0, |profile| centre.gain(profile, gx, gy));
                let pixel: [f32; 3] = [0, 1, 2].map(|channel| self.demosaicked(mosaic, sources[channel].0, sources[channel].1, channel) * gain);
                pixel
            })
            .collect()
    }

    fn demosaicked<T: Copy + Into<f32>>(&self, mosaic: &[T], x: usize, y: usize, channel: usize) -> f32 {
        let scaled = |x: usize, y: usize| -> Option<f32> {
            let (row, col) = (self.at.1 + y, self.at.0 + x);
            (self.colours[(row % self.period.1) * self.period.0 + col % self.period.0] as usize == channel).then(|| {
                let level = (row & 1) * 2 + (col & 1);
                (mosaic[row * self.stride + col].into() - self.black[level]).max(0.0) * self.scale[level]
            })
        };
        if let Some(value) = scaled(x, y) {
            return value;
        }
        for reach in [1usize, 2] {
            let (mut sum, mut count) = (0.0f32, 0usize);
            for ny in y.saturating_sub(reach)..(y + reach + 1).min(self.size.1) {
                for nx in x.saturating_sub(reach)..(x + reach + 1).min(self.size.0) {
                    if let Some(value) = scaled(nx, ny) {
                        sum += value;
                        count += 1;
                    }
                }
            }
            if count > 0 {
                return sum / count as f32;
            }
        }
        0.0
    }

    fn mean<T: Copy + Into<f32>>(&self, mosaic: &[T], table: &Table, channel: usize, centre: (f32, f32), half: (f32, f32)) -> f32 {
        let (width, height) = (self.size.0 as f32, self.size.1 as f32);
        let centre = (centre.0.clamp(0.0, width), centre.1.clamp(0.0, height));
        let tent = TENT[channel];
        let mut grow = 1.0;
        loop {
            let (left, right) = ((centre.0 - half.0 * grow).max(0.0).min(width), (centre.0 + half.0 * grow).min(width).max(0.0));
            let (top, bottom) = ((centre.1 - half.1 * grow).max(0.0).min(height), (centre.1 + half.1 * grow).min(height).max(0.0));
            let whole = grow == 1.0;
            let (first, across) = table.weigh(left, right, channel, whole && centre.0 - half.0 >= 0.0 && centre.0 + half.0 <= width);
            let (start, downs) = table.weigh(top, bottom, channel, whole && centre.1 - half.1 >= 0.0 && centre.1 + half.1 <= height);
            let last = ((right + tent).ceil() as usize).min(self.size.0).min(first + MOST_COLUMNS);
            let rows = start..((bottom + tent).ceil() as usize).min(self.size.1).min(start + MOST_COLUMNS);

            let (period, columns) = (self.period.0, self.period.0 - (self.at.0 + first) % self.period.0);
            let mut phase = (self.at.1 + rows.start) % self.period.1;
            let (mut sum, mut weight) = (0.0f32, 0.0f32);
            for y in rows.clone() {
                let sites = &self.sites[phase * 3 + channel];
                phase = if phase + 1 == self.period.1 { 0 } else { phase + 1 };
                let down = downs[y - rows.start];
                let row = self.at.1 + y;
                let line = &mosaic[row * self.stride + self.at.0 + first..row * self.stride + self.at.0 + last];
                for &site in sites {

                    let level = (row & 1) * 2 + (site & 1);
                    let (black, scale) = (self.black[level], self.scale[level]);

                    let mut x = site + columns;
                    if x >= period {
                        x -= period;
                    }
                    while x < line.len() {
                        let w = down * across[x];
                        sum += w * (line[x].into() - black).max(0.0) * scale;
                        weight += w;
                        x += period;
                    }
                }
            }
            if weight > 0.0 || grow > 8.0 {
                return if weight > 0.0 { sum / weight } else { 0.0 };
            }
            grow *= 2.0;
        }
    }
}

fn footprint(at: usize, ratio: f64, size: usize) -> (f32, f32) {
    let start = (at as f64 * ratio) as usize;
    let end = (((at as f64 + 1.0) * ratio).ceil() as usize).min(size).max(start + 1);
    (start as f32, end as f32)
}

struct Table {

    least: f32,
    widths: usize,
    rows: Vec<[f32; MOST_COLUMNS]>,
}

const STEPS: f32 = 16.0;

impl Table {
    fn new((least, most): (f32, f32)) -> Self {
        let widths = ((most - least) * STEPS).ceil().max(0.0) as usize + 1;
        let mut rows = Vec::with_capacity(TENT.len() * widths * STEPS as usize);
        for tent in TENT {
            for width in 0..widths {
                let half = least + width as f32 / STEPS;
                for phase in 0..STEPS as usize {
                    let from = phase as f32 / STEPS + tent;
                    rows.push(weights(from, from + 2.0 * half, 0, tent));
                }
            }
        }
        Self { least, widths, rows }
    }

    fn weigh(&self, from: f32, to: f32, channel: usize, inside: bool) -> (usize, [f32; MOST_COLUMNS]) {
        let tent = TENT[channel];
        let reach = from - tent;
        let width = ((to - from) / 2.0 - self.least) * STEPS;
        if inside && reach >= 0.0 && width >= -0.5 && width < self.widths as f32 - 0.5 {
            let (first, phase) = (reach.floor(), ((reach - reach.floor()) * STEPS).round());
            let (first, phase) = if phase >= STEPS { (first + 1.0, 0.0) } else { (first, phase) };
            let at = (channel * self.widths + width.round() as usize) * STEPS as usize + phase as usize;
            return (first as usize, self.rows[at]);
        }
        let first = reach.floor().max(0.0) as usize;
        (first, weights(from, to, first, tent))
    }
}

fn weights(from: f32, to: f32, start: usize, tent: f32) -> [f32; MOST_COLUMNS] {
    let mut weights = [0.0f32; MOST_COLUMNS];
    let first = start as f32 + 0.5;
    for (at, weight) in weights.iter_mut().enumerate() {
        *weight = covered(from, to, first + at as f32, tent);
    }
    weights
}

fn covered(from: f32, to: f32, at: f32, tent: f32) -> f32 {
    let area = |u: f32| {
        let a = u.clamp(-1.0, 1.0);
        0.5 + a - 0.5 * a * a.abs()
    };
    let reach = 1.0 / tent;
    (area((to - at) * reach) - area((from - at) * reach)).max(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flat(pattern: &[[u8; 6]; 6], period: usize, rgb: [f32; 3], width: usize, height: usize) -> (Frame, Vec<u16>) {
        let mosaic = (0..width * height)
            .map(|at| 100 + (rgb[pattern[(at / width) % period][(at % width) % period] as usize] * 1000.0) as u16)
            .collect();
        let sites = (0..period * 3).map(|at| (0..period).filter(|col| pattern[at / 3][*col] as usize == at % 3).collect()).collect::<Vec<Vec<usize>>>();
        let frame = Frame {
            stride: width,
            at: (2, 4),
            size: (width - 6, height - 8),
            out: ((width - 6) / 5, (height - 8) / 5),
            period: (period, period),
            sites,
            colours: pattern.iter().take(period).flat_map(|row| row[..period].to_vec()).collect(),
            black: [100.0; 4],
            scale: [1.0 / 1000.0; 4],
        };
        (frame, mosaic)
    }

    #[test]
    fn a_flat_field_stays_flat_and_keeps_its_colour() {
        let bayer = [[0, 1, 0, 1, 0, 1], [1, 2, 1, 2, 1, 2], [0; 6], [0; 6], [0; 6], [0; 6]];
        let xtrans = [[1, 1, 0, 1, 1, 2], [1, 1, 2, 1, 1, 0], [2, 0, 1, 0, 2, 1], [1, 1, 2, 1, 1, 0], [1, 1, 0, 1, 1, 2], [0, 2, 1, 2, 0, 1]];
        for (pattern, period) in [(&bayer, 2), (&xtrans, 6)] {
            let rgb = [0.2, 0.5, 0.8];
            let (frame, mosaic) = flat(pattern, period, rgb, 126, 88);
            let pixels = frame.shrink(&mosaic, None);
            assert_eq!(pixels.len(), frame.out.0 * frame.out.1);
            for pixel in pixels {
                for channel in 0..3 {
                    assert!((pixel[channel] - rgb[channel]).abs() < 1e-3, "period {period}: {pixel:?}");
                }
            }
        }
    }

    #[test]
    fn a_box_past_the_edge_grows_back_to_it() {
        let bayer = [[0, 1, 0, 1, 0, 1], [1, 2, 1, 2, 1, 2], [0; 6], [0; 6], [0; 6], [0; 6]];
        let (frame, mosaic) = flat(&bayer, 2, [0.2, 0.5, 0.8], 126, 88);
        let (width, height) = (frame.size.0 as f32, frame.size.1 as f32);
        let green = frame.mean(&mosaic, &Table::new((1.0, 2.0)), 1, (width + 4.2, height / 2.0), (1.5, 1.0));
        assert!((green - 0.5).abs() < 1e-3, "{green}");
    }

    #[test]
    fn a_photosite_weighs_what_of_its_tent_is_in_the_box() {
        assert!((covered(0.0, 10.0, 5.0, 2.0) - 1.0).abs() < 1e-6);
        assert_eq!(covered(0.0, 10.0, 13.0, 2.0), 0.0);
        assert!((covered(0.0, 10.0, 10.0, 2.0) - 0.5).abs() < 1e-6);
        assert!((covered(0.0, 10.0, 0.0, 1.0) - 0.5).abs() < 1e-6);
    }

    #[test]
    fn a_box_outside_the_frame_holds_its_edge() {
        let bayer = [[0, 1, 0, 1, 0, 1], [1, 2, 1, 2, 1, 2], [0; 6], [0; 6], [0; 6], [0; 6]];
        let rgb = [0.2, 0.5, 0.8];
        let (frame, mosaic) = flat(&bayer, 2, rgb, 126, 88);
        let table = Table::new((0.5, 3.0));
        let (width, height) = (frame.size.0 as f32, frame.size.1 as f32);
        for centre in [(width + 9.0, 40.0), (-9.0, 40.0), (60.0, height + 9.0), (60.0, -9.0), (width + 30.0, height + 30.0)] {
            for channel in 0..3 {
                let value = frame.mean(&mosaic, &table, channel, centre, (0.77, 0.77));
                assert!((value - rgb[channel]).abs() < 1e-3, "{centre:?} channel {channel}: {value}");
            }
        }
    }

    #[test]
    fn the_frames_that_panicked_take_the_mosaics_route() {
        let raws = Path::new("/mnt/data-games/dev/raws-cc0");
        for (file, full) in [
            ("Panasonic/DC-GH5S/2603-Panasonic - DC-GH5S - 4:3.RW2", (3680, 2760)),
            ("Panasonic/DC-GH5S/2607-Panasonic - DC-GH5S - 4:3.RW2", (3680, 2760)),
            ("Nikon/Z 9/5146-Nikon - Z 9 - 8bit 8bit compressed (3:2).NEF", (8256, 5504)),
        ] {
            let path = raws.join(file);
            if !path.exists() {
                eprintln!("skipped: {} is not on this computer", path.display());
                continue;
            }
            for edge in [2400, 1920, 1024] {
                let (proxy, size) = super::super::proxy_from_mosaic(&path, edge).unwrap();
                assert_eq!((proxy.width.max(proxy.height), size), (edge, full), "{file}");
                assert!(proxy.data.iter().all(|value| value.is_finite()), "{file}");
            }
        }
        let fx2 = raws.join("Sony/ILME-FX2/8807-Sony - ILME-FX2 - 14bit uncompressed (3:2).ARW");
        if fx2.exists() {
            let (proxy, size) = super::super::proxy_from_mosaic(&fx2, 2400).unwrap();
            assert_eq!((proxy.width, size), (2400, (7008, 4672)));
        }
    }

    #[test]
    #[ignore]
    fn every_frame_in_a_list_makes_a_proxy() {
        let Ok(list) = std::env::var("NUMA_PROXY_SWEEP") else { return };
        numa_core::power::set_frugal(true);
        let mut panics = Vec::new();
        for path in std::fs::read_to_string(list).unwrap().lines().filter(|line| !line.is_empty()) {
            for edge in [2400, 1920, 1024] {
                let started = std::time::Instant::now();
                match std::panic::catch_unwind(|| super::super::editor_proxy(Path::new(path), edge)) {
                    Ok(Ok((proxy, full))) => println!("ok\t{edge}\t{}x{}\t{full:?}\t{:.2}s\t{path}", proxy.width, proxy.height, started.elapsed().as_secs_f32()),
                    Ok(Err(err)) => println!("err\t{edge}\t{err}"),
                    Err(_) => {
                        println!("PANIC\t{edge}\t{path}");
                        panics.push(format!("{edge} {path}"));
                    }
                }
            }
        }
        assert!(panics.is_empty(), "{panics:#?}");
    }
}
