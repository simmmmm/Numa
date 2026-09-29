use rawler::decoders::RawDecodeParams;
use rayon::prelude::*;
use std::path::Path;
use std::sync::Arc;

use numa_core::color::CameraProfile;
use numa_core::image::LinearImage;
use numa_core::lens::{bilinear, geometry_fit, Centre, LensProfile};
use numa_core::profile::DngProfile;

use super::{
    camera_midtone, camera_profile, canon_multi_exposure, demosaic_on_processor, dump_without_rendering,
    dump_without_vignetting, film_mode, find_rendering, flat, lens_profile, match_camera_exposure, median_of_nine,
    Demosaic, Laps, BASELINE_EV, BLACK_FLOOR, EXPOSURE_STRIDE,
};

pub struct Regions {

    developed: Vec<f32>,
    width: usize,
    height: usize,
    centre: Centre,
    lens: Option<LensProfile>,

    falloff: bool,
    bends: bool,
    fit: f32,
    markesteijn: bool,
    baseline: f32,
    factor: f32,

    clip: f32,
    flips: (bool, bool, bool),
    profile: Option<CameraProfile>,
    rendering: Option<Arc<DngProfile>>,
    film_mode: Option<String>,
}

type Rect = [usize; 4];

impl Regions {

    pub fn open(path: &Path) -> Result<Regions, String> {
        std::thread::scope(|scope| {
            let midtone = scope.spawn(|| camera_midtone(path));
            Self::open_beside(path, || midtone.join().ok().flatten())
        })
    }

    fn open_beside(path: &Path, camera_midtone: impl FnOnce() -> Option<f32>) -> Result<Regions, String> {
        let fail = |err: String| format!("{}: {}", path.display(), err);
        let mut laps = Laps::start();

        let (mut camera_midtone, mut asked) = (Some(camera_midtone), None);
        let mut wanted = || *asked.get_or_insert_with(|| camera_midtone.take().and_then(|midtone| midtone()));

        let (developed, (width, height), profile, rendering, flips, film_mode, markesteijn) = {
            let source = rawler::rawsource::RawSource::new(path).map_err(|e| fail(e.to_string()))?;
            let decoder = rawler::get_decoder(&source).map_err(|e| fail(e.to_string()))?;
            laps.lap("read");
            let raw = decoder.raw_image(&source, &RawDecodeParams::default(), false).map_err(|e| fail(e.to_string()))?;
            laps.lap("decode");
            let mut profile = camera_profile(&raw);
            if raw.camera.make == "Canon" && canon_multi_exposure(path) == Some(true) {
                if let Some(profile) = profile.as_mut() {
                    profile.as_shot = [1.0, 1.0, 1.0];
                }
            }
            let rendering = find_rendering(&raw).filter(|_| !dump_without_rendering());
            laps.lap("profile");
            let metadata = decoder.raw_metadata(&source, &RawDecodeParams::default()).ok();
            let flips = metadata
                .as_ref()
                .and_then(|meta| meta.exif.orientation)
                .map_or(rawler::Orientation::Normal, rawler::Orientation::from_u16)
                .to_flips();
            let film_mode = film_mode(path).map(str::to_string);
            laps.lap("metadata");

            #[cfg(feature = "gpu")]
            if let Some((image, _)) = super::gpu::develop(&raw, path, Demosaic::Best, flips, profile.as_ref(), &mut wanted, &mut laps, true) {
                laps.report("regions", path);
                return Ok(Self::cut_from(image, profile, rendering, film_mode));
            }
            let (developed, markesteijn) = demosaic_on_processor(raw, path, Demosaic::Best)?;
            laps.lap("demosaic");
            let dim = developed.dim();
            (flat(developed), (dim.w, dim.h), profile, rendering, flips, film_mode, markesteijn)
        };
        let lens = lens_profile(path);
        laps.lap("lens lookup");
        let falloff = lens.is_some() && !dump_without_vignetting();
        let bends = lens.as_ref().is_some_and(LensProfile::bends_anything);
        let fit = lens.as_ref().filter(|_| bends).map_or(1.0, geometry_fit);
        let mut regions = Regions {
            developed,
            width,
            height,
            centre: Centre::of(width, height),
            lens,
            falloff,
            bends,
            fit,
            markesteijn,
            baseline: 2.0f32.powf(BASELINE_EV),
            factor: 1.0,
            clip: 0.0,
            flips,
            profile,
            rendering,
            film_mode,
        };
        regions.factor = wanted().and_then(|wanted| regions.exposure_factor(wanted)).unwrap_or(1.0);
        regions.clip = regions.baseline * regions.factor;
        laps.lap("exposure match");
        laps.report("regions", path);
        Ok(regions)
    }

    fn cut_from(image: LinearImage, profile: Option<CameraProfile>, rendering: Option<Arc<DngProfile>>, film_mode: Option<String>) -> Regions {
        let (width, height) = (image.width as usize, image.height as usize);
        Regions {
            clip: image.clip.unwrap_or(f32::INFINITY),
            developed: image.data,
            width,
            height,
            centre: Centre::of(width, height),
            lens: None,
            falloff: false,
            bends: false,
            fit: 1.0,
            markesteijn: false,
            baseline: 1.0,
            factor: 1.0,
            flips: (false, false, false),
            profile,
            rendering,
            film_mode,
        }
    }

    pub fn size(&self) -> (u32, u32) {
        match self.flips.0 {
            true => (self.height as u32, self.width as u32),
            false => (self.width as u32, self.height as u32),
        }
    }

    pub fn region(&self, rect: [u32; 4]) -> LinearImage {
        let rect = self.unturned(rect.map(|value| value as usize));

        let around = match self.markesteijn {
            true => self.grown(rect, 1),
            false => rect,
        };
        let lensed = self.lensed(around);
        let mut data = match self.markesteijn {
            true => self.without_false_colour(rect, around, &lensed),
            false => lensed,
        };
        if self.factor != 1.0 {
            data.par_iter_mut().for_each(|value| *value *= self.factor);
        }
        let image = LinearImage::new(rect[2] as u32, rect[3] as u32, data)
            .with_clip(self.clip)
            .into_oriented(self.flips.0, self.flips.1, self.flips.2)
            .with_rendering(self.rendering.clone())
            .with_film_mode(self.film_mode.clone());
        match self.profile {
            Some(profile) => image.with_profile(profile),
            None => image,
        }
    }

    fn unturned(&self, [x, y, width, height]: Rect) -> Rect {
        let (transpose, flip_x, flip_y) = self.flips;
        let (x, y, width, height) = if transpose { (y, x, height, width) } else { (x, y, width, height) };
        let x = if flip_x { self.width - (x + width) } else { x };
        let y = if flip_y { self.height - (y + height) } else { y };
        [x, y, width, height]
    }

    fn grown(&self, [x, y, width, height]: Rect, by: usize) -> Rect {
        let (left, top) = (x.saturating_sub(by), y.saturating_sub(by));
        let (right, bottom) = ((x + width + by).min(self.width), (y + height + by).min(self.height));
        [left, top, right - left, bottom - top]
    }

    fn vignetted(&self, x: usize, y: usize, channel: usize) -> f32 {
        let value = self.developed[(y * self.width + x) * 3 + channel] * self.baseline;
        match self.lens.as_ref().filter(|_| self.falloff) {
            Some(profile) => value * self.centre.gain(profile, x, y),
            None => value,
        }
    }

    fn lensed(&self, rect: Rect) -> Vec<f32> {
        let [x0, y0, width, height] = rect;
        let bent = self.lens.as_ref().filter(|_| self.bends);
        let reach = match bent {
            Some(profile) => self.reached(rect, profile),
            None => rect,
        };
        let [rx, ry, rw, rh] = reach;
        let mut read = vec![0.0f32; rw * rh * 3];
        read.par_chunks_mut(rw * 3).enumerate().for_each(|(y, row)| {
            for x in 0..rw {
                for channel in 0..3 {
                    row[x * 3 + channel] = self.vignetted(rx + x, ry + y, channel);
                }
            }
        });
        let Some(profile) = bent else { return read };

        let mut out = vec![0.0f32; width * height * 3];
        out.par_chunks_mut(width * 3).enumerate().for_each(|(y, row)| {
            for x in 0..width {
                for (channel, (sx, sy)) in self.centre.source(profile, self.fit, x0 + x, y0 + y).into_iter().enumerate() {
                    row[x * 3 + channel] = bilinear(self.width, self.height, sx, sy, |x, y| read[((y - ry) * rw + x - rx) * 3 + channel]);
                }
            }
        });
        out
    }

    fn reached(&self, [x0, y0, width, height]: Rect, profile: &LensProfile) -> Rect {
        let (last_x, last_y) = (self.width as isize - 1, self.height as isize - 1);
        let (left, top, right, bottom) = (y0..y0 + height)
            .into_par_iter()
            .map(|y| {
                let mut span = (isize::MAX, isize::MAX, isize::MIN, isize::MIN);
                for x in x0..x0 + width {
                    for (sx, sy) in self.centre.source(profile, self.fit, x, y) {
                        let (fx, fy) = (sx.floor() as isize, sy.floor() as isize);
                        span.0 = span.0.min(fx.clamp(0, last_x));
                        span.1 = span.1.min(fy.clamp(0, last_y));
                        span.2 = span.2.max((fx + 1).clamp(0, last_x));
                        span.3 = span.3.max((fy + 1).clamp(0, last_y));
                    }
                }
                span
            })
            .reduce(
                || (isize::MAX, isize::MAX, isize::MIN, isize::MIN),
                |a, b| (a.0.min(b.0), a.1.min(b.1), a.2.max(b.2), a.3.max(b.3)),
            );
        [left as usize, top as usize, (right - left + 1) as usize, (bottom - top + 1) as usize]
    }

    fn without_false_colour(&self, rect: Rect, around: Rect, lensed: &[f32]) -> Vec<f32> {
        let [x0, y0, width, height] = rect;
        let [ax, ay, aw, _] = around;
        let at = |x: usize, y: usize| {
            let from = ((y - ay) * aw + x - ax) * 3;
            [lensed[from], lensed[from + 1], lensed[from + 2]]
        };
        let mut out = vec![0.0f32; width * height * 3];
        out.par_chunks_mut(width * 3).enumerate().for_each(|(y, row)| {
            for x in 0..width {
                row[x * 3..x * 3 + 3].copy_from_slice(&self.false_colour(x0 + x, y0 + y, at));
            }
        });
        out
    }

    fn false_colour(&self, x: usize, y: usize, at: impl Fn(usize, usize) -> [f32; 3]) -> [f32; 3] {
        let mut pixel = at(x, y);
        let black = BLACK_FLOOR * self.baseline;
        if self.width < 3 || self.height < 3 || !(pixel[1] > black) {
            return pixel;
        }
        let (left, right) = (x.saturating_sub(1), (x + 1).min(self.width - 1));
        let (above, below) = (y.saturating_sub(1), (y + 1).min(self.height - 1));
        let around = [
            at(left, above), at(x, above), at(right, above),
            at(left, y), pixel, at(right, y),
            at(left, below), at(x, below), at(right, below),
        ];
        let green = pixel[1];
        for channel in [0usize, 2] {
            let ratios = around.map(|near| if near[1] > black { near[channel] / near[1] } else { 1.0 });
            pixel[channel] = green * median_of_nine(ratios);
        }
        pixel
    }

    fn exposure_factor(&self, wanted: f32) -> Option<f32> {
        let count = (self.width * self.height).div_ceil(EXPOSURE_STRIDE);
        let pixels: Vec<f32> = (0..count)
            .into_par_iter()
            .flat_map_iter(|at| {
                let at = at * EXPOSURE_STRIDE;
                self.finished(at % self.width, at / self.width)
            })
            .collect();
        match_camera_exposure(wanted, &pixels, 1, self.profile.as_ref())
    }

    fn finished(&self, x: usize, y: usize) -> [f32; 3] {
        let lensed = |x: usize, y: usize| -> [f32; 3] {
            match self.lens.as_ref().filter(|_| self.bends) {
                Some(profile) => {
                    let sources = self.centre.source(profile, self.fit, x, y);
                    [0, 1, 2].map(|channel| {
                        let (sx, sy) = sources[channel];
                        bilinear(self.width, self.height, sx, sy, |x, y| self.vignetted(x, y, channel))
                    })
                }
                None => [0, 1, 2].map(|channel| self.vignetted(x, y, channel)),
            }
        };
        match self.markesteijn {
            true => self.false_colour(x, y, lensed),
            false => lensed(x, y),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_region_of_a_whole_frame_is_cut_as_it_is() {
        let (w, h) = (7u32, 5u32);
        let data: Vec<f32> = (0..w * h * 3).map(|v| v as f32 * 0.37).collect();
        let regions = Regions::cut_from(LinearImage::new(w, h, data.clone()).with_clip(2.5), None, None, None);
        assert_eq!(regions.size(), (w, h));
        let part = regions.region([2, 1, 3, 2]);
        assert_eq!((part.width, part.height, part.clip), (3, 2, Some(2.5)));
        for (at, value) in part.data.iter().enumerate() {
            let (row, col, channel) = (at as u32 / 9, at as u32 / 3 % 3, at as u32 % 3);
            assert_eq!(value.to_bits(), data[(((1 + row) * w + 2 + col) * 3 + channel) as usize].to_bits(), "{at}");
        }
    }

    #[test]
    fn a_region_is_the_full_develop_cut() {
        for file in ["Olympus/E-M1MarkII.ORF", "Fujifilm/X-T5.RAF"] {
            let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/corpus").join(file);
            if !path.is_file() {
                println!("skipped {file}: run dev/fetch-corpus.sh");
                continue;
            }
            let full = super::super::decode_linear_best(&path).unwrap();
            let regions = Regions::open(&path).unwrap();
            let (width, height) = regions.size();
            assert_eq!((width, height), (full.width, full.height), "{file}");
            for [x, y, w, h] in [[width / 3, height / 3, 401, 299], [0, 0, 64, 48], [width - 65, height - 49, 65, 49], [0, 0, width, height]] {
                let region = regions.region([x, y, w, h]);
                let same = (0..h).all(|row| {
                    let from = ((y + row) * width + x) as usize * 3;
                    let want = &full.data[from..from + w as usize * 3];
                    let got = &region.data[(row * w) as usize * 3..][..w as usize * 3];
                    want.iter().zip(got).all(|(a, b)| a.to_bits() == b.to_bits())
                });
                assert!(same, "{file}: region {:?}", [x, y, w, h]);
                assert_eq!(region.clip.map(f32::to_bits), full.clip.map(f32::to_bits), "{file}: the ceiling");
            }
        }
    }
}
