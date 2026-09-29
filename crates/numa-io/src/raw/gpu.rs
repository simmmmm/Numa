use super::*;

pub(super) fn develop(
    raw: &rawler::RawImage,
    path: &Path,
    demosaic: Demosaic,
    flips: (bool, bool, bool),
    profile: Option<&CameraProfile>,
    wanted: &mut dyn FnMut() -> Option<f32>,
    laps: &mut Laps,
    may_round: bool,
) -> Option<(LinearImage, (u32, u32))> {
    if !numa_gpu::enabled() {
        return None;
    }
    let proxy = match demosaic {
        Demosaic::Proxy(edge) => Some(edge),
        _ => None,
    };
    let mosaic = mosaic(raw, demosaic)?;
    let xtrans = mosaic.xtrans;
    let lens = lens_profile(path);
    laps.lap("lens lookup");
    let baseline = 2.0f32.powf(BASELINE_EV);
    let job = numa_gpu::Job {
        mosaic,
        baseline,
        lens: lens.as_ref(),
        vignetting: !dump_without_vignetting(),
        false_colour: xtrans.then_some(BLACK_FLOOR * baseline),
        flips,
        proxy,
        stride: EXPOSURE_STRIDE,
        may_round,
    };
    let exposure = |samples: &[f32]| {
        wanted().and_then(|wanted| match_camera_exposure(wanted, samples, 1, profile)).unwrap_or(1.0)
    };

    static ONE: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _one = ONE.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
    let Some(_card) = numa_infer::card_within(std::time::Duration::from_millis(1500)) else {
        log::info!("{}: a model is still on the card; decoding on the processor", path.display());
        return None;
    };
    match numa_gpu::develop(&job, frugal(), exposure) {
        Ok(done) => {
            laps.lap("gpu");
            let image = LinearImage::new(done.width, done.height, done.data).with_clip(baseline * done.factor);
            Some((image, done.full))
        }
        Err(err) => {
            log::info!("{}: {err}; decoding on the processor", path.display());
            None
        }
    }
}

fn mosaic(raw: &rawler::RawImage, demosaic: Demosaic) -> Option<numa_gpu::Mosaic<'_>> {
    use rawler::imgop::sensor::SensorType;
    use rawler::rawimage::{RawImageData, RawPhotometricInterpretation};

    let RawPhotometricInterpretation::Cfa(config) = &raw.photometric else { return None };
    let RawImageData::Integer(pixels) = &raw.data else { return None };

    if demosaic == Demosaic::Draft {
        return None;
    }
    let xtrans = matches!(config.sensor, SensorType::Xtrans);
    if !config.cfa.is_rgb() || raw.fuji_rotation_width.is_some() || raw.cpp != 1 {
        return None;
    }

    let whole = rawler::imgop::Rect::new(rawler::imgop::Point::new(0, 0), rawler::imgop::Dim2::new(raw.width, raw.height));
    let active = raw.active_area.unwrap_or(whole);
    let crop = match xtrans {
        true => raw.crop_area,
        false => raw.crop_area.or(raw.active_area),
    };
    let crop = match crop {
        Some(crop) if raw.active_area.is_some() || xtrans => within(crop, active)?,
        Some(crop) => crop,
        None => rawler::imgop::Rect::new(rawler::imgop::Point::new(0, 0), active.d),
    };
    let crop = if crop.d == active.d { rawler::imgop::Rect::new(rawler::imgop::Point::new(0, 0), active.d) } else { crop };
    let shifted = config.cfa.shift(active.p.x, active.p.y);
    let cfa = std::array::from_fn(|row| std::array::from_fn(|col| shifted.color_at(row, col) as u8));

    Some(numa_gpu::Mosaic {
        pixels,
        width: raw.width,
        height: raw.height,
        black: raw.blacklevel.as_bayer_array(),
        white: raw.whitelevel.as_bayer_array(),
        cfa,
        xtrans,
        active: [active.p.x, active.p.y, active.d.w, active.d.h],
        crop: [crop.p.x, crop.p.y, crop.d.w, crop.d.h],
    })
}

fn within(crop: rawler::imgop::Rect, active: rawler::imgop::Rect) -> Option<rawler::imgop::Rect> {
    let fits = crop.p.x >= active.p.x && crop.p.y >= active.p.y && crop.d.w <= active.d.w && crop.d.h <= active.d.h;
    fits.then(|| crop.adapt(&active))
}

static MARKER: std::sync::OnceLock<Option<std::path::PathBuf>> = std::sync::OnceLock::new();

pub(super) fn warm_up(marker: Option<std::path::PathBuf>) {
    let marker = MARKER.get_or_init(|| marker);
    numa_gpu::warm_up(frugal(), marker.clone());
}

pub(super) fn rewarm() {
    if let Some(marker) = MARKER.get() {
        numa_gpu::warm_up(frugal(), marker.clone());
    }
}

pub(super) fn open() -> Option<String> {
    numa_gpu::open_now(frugal()).then(|| numa_gpu::limits(frugal())).flatten()
}

pub(super) fn release() {
    numa_gpu::release();
}

pub(super) fn ready() -> bool {
    numa_gpu::describe(frugal()).is_some()
}

pub(super) fn frugal() -> bool {
    numa_core::power::frugal() || std::env::var("NUMA_GPU").as_deref() == Ok("low")
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[derive(Clone, Copy, PartialEq, PartialOrd, Debug)]
    enum Stage {
        Demosaic,
        Vignetting,
        Geometry,
        FalseColour,
        Turned,
        Proxy,
    }

    const EDGE: u32 = 2400;

    fn frames() -> Vec<PathBuf> {
        if let Some(folder) = std::env::var_os("NUMA_GPU_RAWS") {
            let entries = |dir: &Path| std::fs::read_dir(dir).into_iter().flatten().flatten().map(|entry| entry.path()).collect::<Vec<_>>();
            let mut found: Vec<PathBuf> =
                entries(Path::new(&folder)).into_iter().flat_map(|path| if path.is_dir() { entries(&path) } else { vec![path] }).filter(|path| is_raw(path)).collect();
            found.sort();
            return found;
        }
        let corpus = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/corpus");
        ["Olympus/E-M1MarkII.ORF", "Fujifilm/X-T5.RAF"].iter().map(|name| corpus.join(name)).filter(|path| path.is_file()).collect()
    }

    fn decoded(path: &Path) -> (rawler::RawImage, (bool, bool, bool)) {
        let source = rawler::rawsource::RawSource::new(path).unwrap();
        let decoder = rawler::get_decoder(&source).unwrap();
        let raw = decoder.raw_image(&source, &RawDecodeParams::default(), false).unwrap();
        let metadata = decoder.raw_metadata(&source, &RawDecodeParams::default()).ok();
        let flips = metadata
            .as_ref()
            .and_then(|meta| meta.exif.orientation)
            .map_or(rawler::Orientation::Normal, rawler::Orientation::from_u16)
            .to_flips();
        (raw, flips)
    }

    fn lens_to(lens: Option<&LensProfile>, stage: Stage) -> Option<LensProfile> {
        let mut lens = lens.filter(|_| stage >= Stage::Vignetting)?.clone();
        if stage == Stage::Vignetting {
            for table in [&mut lens.distortion, &mut lens.red, &mut lens.blue] {
                table.iter_mut().for_each(|value| *value = 0.0);
            }
        }
        Some(lens)
    }

    fn on_processor(raw: &rawler::RawImage, path: &Path, lens: Option<&LensProfile>, stage: Stage, flips: (bool, bool, bool)) -> LinearImage {
        let (developed, markesteijn) = demosaic_on_processor(raw.clone(), path, Demosaic::Best).unwrap();
        let baseline = 2.0f32.powf(BASELINE_EV);
        let dim = developed.dim();
        let mut data = flat(developed);
        data.par_iter_mut().for_each(|v| *v *= baseline);
        let mut image = LinearImage::new(dim.w as u32, dim.h as u32, data);
        if let Some(lens) = lens {
            correct_vignetting(&mut image, lens);
            if lens.bends_anything() {
                image = correct_geometry(&image, lens);
            }
        }
        if markesteijn && stage >= Stage::FalseColour {
            let (w, h) = (image.width as usize, image.height as usize);
            suppress_false_colour(&mut image.data, w, h, BLACK_FLOOR * baseline);
        }
        if stage >= Stage::Turned {
            image = image.into_oriented(flips.0, flips.1, flips.2);
        }
        match stage {
            Stage::Proxy => image.downscaled(EDGE).unwrap_or(image),
            _ => image,
        }
    }

    fn on_card(raw: &rawler::RawImage, lens: Option<&LensProfile>, stage: Stage, flips: (bool, bool, bool)) -> LinearImage {
        let mosaic = mosaic(raw, Demosaic::Best).expect("a frame the card develops");
        let baseline = 2.0f32.powf(BASELINE_EV);
        let job = numa_gpu::Job {
            false_colour: (mosaic.xtrans && stage >= Stage::FalseColour).then_some(BLACK_FLOOR * baseline),
            mosaic,
            baseline,
            lens,
            vignetting: true,
            flips: if stage >= Stage::Turned { flips } else { (false, false, false) },
            proxy: (stage == Stage::Proxy).then_some(EDGE),
            stride: EXPOSURE_STRIDE,
            may_round: false,
        };
        let done = numa_gpu::develop(&job, false, |_| 1.0).unwrap();
        LinearImage::new(done.width, done.height, done.data)
    }

    fn linear_difference(a: &LinearImage, b: &LinearImage) -> (f32, (u32, u32), f64, f64) {
        assert_eq!((a.width, a.height), (b.width, b.height));
        let (mut most, mut at, mut sum, mut far) = (0.0f32, 0usize, 0.0f64, 0usize);
        for (i, (x, y)) in a.data.iter().zip(&b.data).enumerate() {
            let d = (x - y).abs();
            sum += d as f64;
            far += (d > 1e-3) as usize;
            if d > most {
                (most, at) = (d, i);
            }
        }
        let pixel = (at / 3) as u32;
        let n = a.data.len() as f64;
        (most, (pixel % a.width, pixel / a.width), sum / n, far as f64 / n)
    }

    fn rendered_difference(a: &LinearImage, b: &LinearImage, profile: Option<CameraProfile>) -> (u8, f64) {
        let inputs = numa_render::RenderInputs { profile: None, denoised: None, sharpened: None };
        let document = numa_core::document::Document::new(String::new());
        let render = |image: &LinearImage| {
            let mut image = image.clone();
            image.profile = profile;
            numa_render::develop(&document, image, &inputs)
        };
        let (a, b) = (render(a), render(b));
        let steps: Vec<u8> = a.as_raw().iter().zip(b.as_raw()).map(|(x, y)| x.abs_diff(*y)).collect();
        (steps.iter().copied().max().unwrap_or(0), steps.iter().filter(|step| **step > 0).count() as f64 / steps.len() as f64)
    }

    #[test]
    fn the_card_develops_what_the_processor_does() {
        let _ = env_logger::builder().is_test(true).try_init();
        let frames = frames();
        if frames.is_empty() || !numa_gpu::open_now(false) {
            assert!(!numa_gpu::broken(), "the develop did not build on this card");
            println!("skipped: no corpus or no card");
            return;
        }
        let half = std::env::var("NUMA_GPU_HALF").as_deref() == Ok("1");
        let stages = match std::env::var_os("NUMA_GPU_RAWS") {
            Some(_) => vec![Stage::Demosaic, Stage::Vignetting, Stage::Geometry, Stage::FalseColour, Stage::Turned, Stage::Proxy],
            None => vec![Stage::FalseColour, Stage::Proxy],
        };
        for path in frames {
            let (raw, flips) = decoded(&path);
            if mosaic(&raw, Demosaic::Best).is_none() {
                println!("{}: not a frame the card develops; the processor does", path.display());
                continue;
            }
            let lens = lens_profile(&path);
            let profile = camera_profile(&raw);
            for &stage in &stages {
                let lens = lens_to(lens.as_ref(), stage);
                let cpu = on_processor(&raw, &path, lens.as_ref(), stage, flips);
                let gpu = on_card(&raw, lens.as_ref(), stage, flips);
                let (most, at, mean, far) = linear_difference(&cpu, &gpu);
                let rendered = (stage == Stage::Proxy).then(|| rendered_difference(&cpu, &gpu, profile));
                println!(
                    "{} {stage:?}: max {most:.2e} at {at:?}, mean {mean:.2e}, >1e-3 {:.4}%{}",
                    path.file_name().unwrap().to_string_lossy(),
                    far * 100.0,
                    rendered.map_or(String::new(), |(step, moved)| format!(", 8-bit max {step}, moved {:.4}%", moved * 100.0)),
                );

                let (most_mean, most_far, most_moved) = match half {
                    true => (1.5e-4, 1e-2, 1.5e-2),
                    false => (1e-5, 1e-3, 5e-3),
                };
                assert!(mean < most_mean, "{stage:?}: the mean difference {mean:.2e} is over {most_mean:.0e}");
                assert!(far < most_far, "{stage:?}: {:.3}% of values more than 1e-3 apart", far * 100.0);
                if let Some((step, moved)) = rendered {
                    assert!(step <= 2, "{stage:?}: the rendered proxies are {step} levels apart");
                    assert!(moved < most_moved, "{stage:?}: {:.2}% of the rendered values moved", moved * 100.0);
                }
            }
        }
    }
}
