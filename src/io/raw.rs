use rayon::prelude::*;
use std::path::Path;

use image::{DynamicImage, RgbImage};
use rawler::decoders::RawDecodeParams;
use rawler::imgop::develop::{Intermediate, ProcessingStep, RawDevelop};

use std::sync::Arc;

use crate::core::color::CameraProfile;
use crate::core::profile::DngProfile;
use crate::io::dcp;
use crate::core::image::LinearImage;

const BASELINE_EV: f32 = 1.241;

fn raw_extensions() -> &'static [&'static str] {
    rawler::decoders::supported_extensions()
}

const PLAIN_EXTENSIONS: &[&str] = &["jpg", "jpeg", "png", "webp", "tif", "tiff", "bmp"];

const HEIF_EXTENSIONS: &[&str] = &["heic", "heif", "hif"];

fn extension(path: &Path) -> Option<String> {
    let name = path.file_name()?.to_str()?;
    if name.starts_with("._") {
        return None;
    }
    Some(path.extension()?.to_str()?.to_ascii_lowercase())
}

pub fn is_raw(path: &Path) -> bool {

    extension(path)
        .is_some_and(|ext| raw_extensions().iter().any(|known| known.eq_ignore_ascii_case(&ext)))
}

pub fn is_supported(path: &Path) -> bool {
    is_raw(path)
        || extension(path).is_some_and(|ext| {
            PLAIN_EXTENSIONS.contains(&ext.as_str()) || HEIF_EXTENSIONS.contains(&ext.as_str())
        })
}

pub fn preview(path: &Path) -> Result<DynamicImage, String> {

    match embedded_preview(path)? {
        Some(image) => Ok(image),
        None => developed_preview(path).map_err(|err| format!("{}: {err}", path.display())),
    }
}

fn embedded_preview(path: &Path) -> Result<Option<DynamicImage>, String> {
    let fail = |err: String| format!("{}: {}", path.display(), err);
    let params = RawDecodeParams::default();

    let source = rawler::rawsource::RawSource::new(path).map_err(|e| fail(e.to_string()))?;
    let decoder = rawler::get_decoder(&source).map_err(|e| fail(e.to_string()))?;

    let embedded = |image: Option<DynamicImage>| image.filter(|image| image.width().max(image.height()) > 0);
    let preview = embedded(decoder.preview_image(&source, &params).ok().flatten());
    let image = match preview {
        Some(image) if image.width().max(image.height()) >= 1000 => Some(image),
        small => embedded(decoder.full_image(&source, &params).ok().flatten())
            .or(small)
            .or_else(|| embedded(decoder.thumbnail_image(&source, &params).ok().flatten())),
    };
    let Some(image) = image else { return Ok(None) };

    let orientation = decoder
        .raw_metadata(&source, &params)
        .ok()
        .and_then(|meta| meta.exif.orientation)
        .map_or(rawler::Orientation::Normal, rawler::Orientation::from_u16);

    Ok(Some(orient_image(image, orientation)))
}

fn developed_preview(path: &Path) -> Result<DynamicImage, String> {

    static ONE_AT_A_TIME: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _turn = ONE_AT_A_TIME.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    let linear = decode_linear(path)?;
    let proxy = linear.downscaled(1920).unwrap_or(linear);
    let document = crate::core::document::Document::new(path.display().to_string());
    Ok(DynamicImage::ImageRgb8(crate::render::develop(&document, &proxy)))
}

fn open_heif(path: &Path) -> Result<DynamicImage, String> {
    use heic_rs::props::colr::{MatrixCoefficients, Nclx, Range};

    let text = |err: heic_rs::Error| err.to_string();
    let bytes = std::fs::read(path).map_err(|err| err.to_string())?;
    let context = heic_rs::context::Context::open(&bytes).map_err(text)?;
    let primary = context.meta.primary;
    let props = context.props(primary).map_err(text)?;
    let limit = heic_rs::DEFAULT_MAX_PIXELS;

    let decode = |item: u32| -> Result<heic_rs::hevc::Frame, String> {
        let props = context.props(item).map_err(text)?;
        let config = props.hvcc.as_ref().ok_or("no HEVC configuration")?;
        let data = context.item_data(item).map_err(text)?;
        let slices = config.split_nals(&data).map_err(text)?;
        let frame = heic_rs::hevc::decode_still(&config.parameter_sets(), &slices).map_err(text)?;
        frame.validate().map_err(text)?;
        Ok(frame)
    };

    let frame = match context.grid(primary).map_err(text)? {
        Some((grid, tiles)) => {
            let frames = tiles.par_iter().map(|tile| decode(*tile)).collect::<Result<Vec<_>, _>>()?;
            heic_rs::grid::compose(&grid, &frames, limit).map_err(text)?
        }
        None => decode(primary)?,
    };

    let nclx = props.nclx.unwrap_or(Nclx {
        primaries: 1,
        transfer: 13,
        matrix: MatrixCoefficients::Bt601,
        matrix_code: 6,
        range: Range::Full,
    });
    let image = heic_rs::color::convert(&frame, None, nclx, heic_rs::PixelLayout::Rgb8, limit, None)
        .map_err(text)?;
    let image = heic_rs::transform::apply_all(image, &props.transforms).map_err(text)?;
    RgbImage::from_raw(image.width, image.height, image.data)
        .map(DynamicImage::ImageRgb8)
        .ok_or_else(|| "decoded buffer has the wrong size".to_string())
}

fn orient_image(mut image: DynamicImage, orientation: rawler::Orientation) -> DynamicImage {
    let (transpose, flip_x, flip_y) = orientation.to_flips();

    if flip_x {
        image = image.fliph();
    }
    if flip_y {
        image = image.flipv();
    }
    if transpose {
        image = image.rotate90().fliph();
    }

    image
}

fn open_upright(path: &Path) -> Result<image::DynamicImage, String> {
    use image::ImageDecoder;

    let fail = |err: String| format!("{}: {}", path.display(), err);
    if extension(path).is_some_and(|ext| HEIF_EXTENSIONS.contains(&ext.as_str())) {
        return open_heif(path).map_err(fail);
    }
    let mut decoder = image::ImageReader::open(path)
        .map_err(|err| fail(err.to_string()))?
        .with_guessed_format()
        .map_err(|err| fail(err.to_string()))?
        .into_decoder()
        .map_err(|err| fail(err.to_string()))?;

    let orientation = decoder.orientation().map_err(|err| fail(err.to_string()))?;
    let mut image =
        image::DynamicImage::from_decoder(decoder).map_err(|err| fail(err.to_string()))?;
    image.apply_orientation(orientation);
    Ok(image)
}

pub fn load_scaled(path: &Path, max_edge: u32) -> Result<RgbImage, String> {
    let image = if is_raw(path) {
        preview(path)?
    } else {
        open_upright(path)?
    };

    let image = if image.width().max(image.height()) > max_edge {
        image.thumbnail(max_edge, max_edge)
    } else {
        image
    };

    Ok(image.into_rgb8())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn everything_the_decoder_reads_is_something_the_library_looks_at() {
        for name in ["a.RAF", "b.cr2", "c.CR3", "d.nef", "e.arw", "f.rwl", "g.3fr", "h.iiq",
                     "i.x3f", "j.srw", "k.mrw", "l.dng", "m.orf", "n.pef", "o.mef", "p.nrw"] {
            assert!(is_raw(Path::new(name)), "{name} is not recognised as raw");
            assert!(is_supported(Path::new(name)));
        }

        assert!(is_raw(Path::new("SHOUTING.NEF")) && is_raw(Path::new("quiet.nef")));

        for name in ["photo.jpg", "photo.PNG", "scan.tif"] {
            assert!(!is_raw(Path::new(name)));
            assert!(is_supported(Path::new(name)), "{name} should still open");
        }
        for name in ["notes.txt", "movie.mp4", "catalog.db"] {
            assert!(!is_supported(Path::new(name)), "{name} should be ignored");
        }
    }

    fn flat_profile(distortion: f32, red: f32, blue: f32) -> LensProfile {
        let radii = vec![0.25, 0.5, 0.75, 1.0];
        LensProfile {
            transmission: vec![1.0; radii.len()],
            distortion: vec![distortion; radii.len()],
            red: vec![red; radii.len()],
            blue: vec![blue; radii.len()],
            radii,
        }
    }

    #[test]
    fn a_pincushion_lens_reads_from_further_out_and_a_barrel_one_from_closer_in() {
        let pincushion = flat_profile(3.0, 0.0, 0.0);
        assert!((pincushion.source_radius(0.5)[1] - 1.03).abs() < 1e-6);

        let barrel = flat_profile(-5.0, 0.0, 0.0);
        assert!((barrel.source_radius(0.5)[1] - 0.95).abs() < 1e-6);

        let fringing = flat_profile(0.0, -0.001, 0.002);
        let scales = fringing.source_radius(0.5);
        assert!((scales[0] - 0.999).abs() < 1e-6, "red {:?}", scales[0]);
        assert_eq!(scales[1], 1.0, "green is the reference and does not move");
        assert!((scales[2] - 1.002).abs() < 1e-6, "blue {:?}", scales[2]);

        assert!(!flat_profile(0.0, 0.0, 0.0).bends_anything());
        assert!(flat_profile(-1.0, 0.0, 0.0).bends_anything());
        assert!(flat_profile(0.0, 0.0, 0.0005).bends_anything());
    }

    #[test]
    fn correcting_barrel_moves_a_mark_outwards_by_the_stated_amount() {

        let (width, height) = (201usize, 201usize);
        let mut data = vec![0.0f32; width * height * 3];
        let mark = (167usize, 100usize);
        for channel in 0..3 {
            data[(mark.1 * width + mark.0) * 3 + channel] = 1.0;
        }
        let image = LinearImage::new(width as u32, height as u32, data);

        let corrected = correct_geometry(&image, &flat_profile(-10.0, 0.0, 0.0));
        let brightest = (0..width)
            .max_by(|a, b| {
                corrected.data[(100 * width + a) * 3]
                    .total_cmp(&corrected.data[(100 * width + b) * 3])
            })
            .unwrap();
        let moved = brightest as f32 - 100.5;
        assert!(
            (moved - 67.0 / 0.9).abs() < 1.5,
            "the mark went to {moved:.1} px out, not {:.1}",
            67.0 / 0.9
        );

        let pinched = correct_geometry(&image, &flat_profile(5.0, 0.0, 0.0));
        let corner: f32 = (0..3).map(|c| pinched.data[c]).sum();
        assert!(corner.is_finite(), "the corner sampled off the frame");
    }

    use std::path::PathBuf;

    #[test]
    fn a_heif_file_decodes_with_its_colours() {
        let path = Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/data/two-colours.heic"));
        assert!(is_supported(path) && !is_raw(path));
        let image = load_scaled(path, 1000).unwrap();
        assert_eq!((image.width(), image.height()), (64, 48));
        let near = |at: (u32, u32), want: [u8; 3]| {
            let got = image.get_pixel(at.0, at.1).0;
            assert!(got.iter().zip(want).all(|(g, w)| g.abs_diff(w) <= 12), "{got:?} is not {want:?}");
        };
        near((8, 24), [200, 40, 40]);
        near((56, 24), [40, 60, 200]);
    }

    #[test]
    fn load_scaled_downscales_and_keeps_rgb_layout() {
        let path = std::env::temp_dir().join("numa-test-scaled.png");
        image::RgbImage::from_fn(400, 200, |x, _| image::Rgb([x as u8, 1, 2]))
            .save(&path)
            .unwrap();

        let small = load_scaled(&path, 100).unwrap();
        assert_eq!(small.width(), 100, "long edge must hit the cap");
        assert_eq!(small.height(), 50, "aspect ratio must survive");
        assert_eq!(small.as_raw().len(), 100 * 50 * 3);

        let large = load_scaled(&path, 4000).unwrap();
        assert_eq!((large.width(), large.height()), (400, 200));

        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    fn classifies_extensions() {
        assert!(
            !is_supported(&PathBuf::from("/photos/._DSCF0001.RAF")),
            "an AppleDouble sidecar is not a photograph"
        );
        assert!(!is_raw(&PathBuf::from("/photos/._DSCF0001.RAF")));

        let raf = PathBuf::from("/photos/DSCF0001.RAF");
        assert!(is_raw(&raf), "uppercase RAF must be recognised");
        assert!(is_supported(&raf));

        let jpg = PathBuf::from("/photos/a.jpg");
        assert!(!is_raw(&jpg));
        assert!(is_supported(&jpg));

        assert!(!is_supported(&PathBuf::from("/photos/notes.txt")));
        assert!(!is_supported(&PathBuf::from("/photos/noext")));
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Demosaic {

    Draft,

    Best,
}

pub fn decode_linear(path: &Path) -> Result<LinearImage, String> {
    decode_with_exposure(path).map(|(image, _)| image)
}

pub fn decode_linear_best(path: &Path) -> Result<LinearImage, String> {
    decode_with(path, Demosaic::Best).map(|(image, _)| image)
}

pub fn decode_with_exposure(path: &Path) -> Result<(LinearImage, Option<f32>), String> {
    decode_with(path, Demosaic::Draft)
}

fn decode_with(path: &Path, demosaic: Demosaic) -> Result<(LinearImage, Option<f32>), String> {
    let fail = |err: String| format!("{}: {}", path.display(), err);

    let source = rawler::rawsource::RawSource::new(path).map_err(|e| fail(e.to_string()))?;
    let decoder = rawler::get_decoder(&source).map_err(|e| fail(e.to_string()))?;
    let raw = decoder
        .raw_image(&source, &RawDecodeParams::default(), false)
        .map_err(|e| fail(e.to_string()))?;

    let profile = camera_profile(&raw);
    let rendering = find_rendering(&raw);

    let metadata = decoder.raw_metadata(&source, &RawDecodeParams::default()).ok();

    let flips = metadata
        .as_ref()
        .and_then(|meta| meta.exif.orientation)
        .map_or(rawler::Orientation::Normal, rawler::Orientation::from_u16)
        .to_flips();

    let film_mode = film_mode(path).map(str::to_string);
    let exposure = metadata
        .as_ref()
        .and_then(|meta| relative_exposure(&meta.exif));

    let markesteijn = match demosaic {
        Demosaic::Best => markesteijn_demosaic(&raw, path),
        Demosaic::Draft => None,
    };

    let developed = match markesteijn {
        Some(pixels) => pixels,
        None => {
            let steps: Vec<ProcessingStep> = RawDevelop::default()
                .steps
                .into_iter()
                .filter(|step| {
                    !matches!(
                        step,
                        ProcessingStep::SRgb
                            | ProcessingStep::Calibrate
                            | ProcessingStep::WhiteBalance
                    )
                })
                .collect();

            match RawDevelop::new_with(&steps)
                .develop_intermediate(&raw)
                .map_err(|e| fail(e.to_string()))?
            {
                Intermediate::ThreeColor(pixels) => pixels,
                _ => return Err(fail("unsupported sensor colour layout".to_string())),
            }
        }
    };

    let dim = developed.dim();
    let baseline = 2.0f32.powf(BASELINE_EV);

    {
        {

            let mut scaled = developed.flatten();
            scaled.par_iter_mut().for_each(|v| *v *= baseline);
            let mut image = LinearImage::new(dim.w as u32, dim.h as u32, scaled);

            let lens = lens_profile(path);
            if let Some(profile) = &lens {
                correct_vignetting(&mut image, profile);
            }
            let mut image = match &lens {
                Some(profile) if profile.bends_anything() => correct_geometry(&image, profile),
                _ => image,
            };

            let factor = match_camera_exposure(path, &image).unwrap_or(1.0);
            if factor != 1.0 {
                image.data.par_iter_mut().for_each(|value| *value *= factor);
            }

            let image = image.with_clip(baseline * factor);

            let image = image
                .oriented(flips.0, flips.1, flips.2)
                .with_rendering(rendering)
                .with_film_mode(film_mode);
            Ok((
                match profile {
                    Some(profile) => image.with_profile(profile),
                    None => image,
                },
                exposure,
            ))
        }
    }
}

fn markesteijn_demosaic(
    raw: &rawler::RawImage,
    path: &Path,
) -> Option<rawler::pixarray::Color2D<f32, 3>> {
    let attempt = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        markesteijn_develop(raw)
    }));

    match attempt {
        Ok(Ok(pixels)) => pixels,
        Ok(Err(err)) => {
            log::warn!("{}: Markesteijn failed ({err}); using bilinear", path.display());
            None
        }
        Err(_) => {
            log::warn!(
                "{}: Markesteijn panicked inside rawler; using bilinear. \
                 Please report this file — it is the one needed to fix it.",
                path.display()
            );
            None
        }
    }
}

fn markesteijn_develop(
    raw: &rawler::RawImage,
) -> Result<Option<rawler::pixarray::Color2D<f32, 3>>, String> {
    use rawler::imgop::sensor::xtrans::markesteijn::XTransMarkesteijnDemosaic;
    use rawler::imgop::sensor::{Demosaic as _, SensorType};
    use rawler::pixarray::PixF32;
    use rawler::rawimage::RawPhotometricInterpretation;

    let RawPhotometricInterpretation::Cfa(config) = &raw.photometric else {
        return Ok(None);
    };
    if config.sensor != SensorType::Xtrans || !config.cfa.is_rgb() {
        return Ok(None);
    }

    if raw.fuji_rotation_width.is_some() {
        return Ok(None);
    }

    let mut scaled = raw.clone();
    scaled.apply_scaling().map_err(|err| err.to_string())?;

    let pixels = PixF32::new_with(
        scaled.data.as_f32().into_owned(),
        scaled.width,
        scaled.height,
    );

    let active = scaled.active_area.unwrap_or_else(|| pixels.rect());

    let rgb = XTransMarkesteijnDemosaic::new(1).demosaic(&pixels, &config.cfa, &config.colors, active);

    Ok(Some(match scaled.crop_area {
        Some(crop) => {
            let adapted = crop.adapt(&active);
            if adapted.d == rgb.dim() { rgb } else { rgb.crop(adapted) }
        }
        None => rgb,
    }))
}

fn camera_profile(raw: &rawler::RawImage) -> Option<CameraProfile> {
    use rawler::imgop::matrix::{multiply, normalize, pseudo_inverse};
    use rawler::imgop::xyz::SRGB_TO_XYZ_D65;

    use rawler::imgop::xyz::Illuminant;

    let daylight_first = [
        Illuminant::D65,
        Illuminant::D55,
        Illuminant::D50,
        Illuminant::D75,
        Illuminant::Daylight,
        Illuminant::FineWeather,
        Illuminant::Flash,
        Illuminant::CloudyWeather,
        Illuminant::Shade,
    ];
    let xyz_to_cam_4 = raw
        .color_matrix_find_first(daylight_first)
        .or_else(|| raw.color_matrix.iter().min_by_key(|(illuminant, _)| **illuminant as u16).map(|(illuminant, flat)| (*illuminant, flat.clone())))
        .and_then(|(_, flat)| {
            (flat.len() == 9).then(|| {
                let mut matrix = [[0.0f32; 3]; 4];
                for row in 0..3 {
                    matrix[row].copy_from_slice(&flat[row * 3..row * 3 + 3]);
                }
                matrix
            })
        })
        .unwrap_or(raw.xyz_to_cam);

    if xyz_to_cam_4[..3].iter().flatten().all(|value| *value == 0.0) {
        return None;
    }

    let cam_to_rgb = pseudo_inverse(normalize(multiply(&xyz_to_cam_4, &SRGB_TO_XYZ_D65)));

    let mut cam_to_srgb = [[0.0f32; 3]; 3];
    let mut xyz_to_cam = [[0.0f32; 3]; 3];
    for row in 0..3 {
        cam_to_srgb[row].copy_from_slice(&cam_to_rgb[row][..3]);
        xyz_to_cam[row] = xyz_to_cam_4[row];
    }

    let blue = if raw.wb_coeffs[3].is_finite() { 3 } else { 2 };
    let coefficients = if raw.wb_coeffs[0].is_nan()
        || raw.wb_coeffs[1].is_nan()
        || raw.wb_coeffs[blue].is_nan()
    {
        [1.0, 1.0, 1.0]
    } else {
        [raw.wb_coeffs[0], raw.wb_coeffs[1], raw.wb_coeffs[blue]]
    };

    Some(CameraProfile {
        as_shot: coefficients,
        xyz_to_cam,
        cam_to_srgb,
    })
}

pub fn decode_linear_any(path: &Path) -> Result<LinearImage, String> {
    if is_raw(path) {
        return decode_linear(path);
    }

    let rgb = open_upright(path)?.into_rgb8();

    let data = rgb
        .as_raw()
        .iter()
        .map(|byte| {
            let v = *byte as f32 / 255.0;
            if v <= 0.040_45 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        })
        .collect();

    Ok(LinearImage::new(rgb.width(), rgb.height(), data))
}

fn find_rendering(raw: &rawler::RawImage) -> Option<Arc<DngProfile>> {
    use std::collections::HashMap;
    use std::sync::{Mutex, OnceLock};

    static CACHE: OnceLock<Mutex<HashMap<String, Option<Arc<DngProfile>>>>> = OnceLock::new();
    let cache = CACHE.get_or_init(|| Mutex::new(HashMap::new()));

    let key = format!("{}|{}", raw.clean_make, raw.clean_model);
    let mut cache = cache.lock().ok()?;

    cache
        .entry(key)
        .or_insert_with(|| {
            let profile = dcp::find(&raw.clean_make, &raw.clean_model)?;
            Some(Arc::new(profile))
        })
        .clone()
}

fn relative_exposure(exif: &rawler::exif::Exif) -> Option<f32> {
    let ratio = |value: &rawler::formats::tiff::Rational| {
        (value.d != 0).then(|| value.n as f32 / value.d as f32)
    };

    let shutter = exif.exposure_time.as_ref().and_then(ratio)?;
    let aperture = exif.fnumber.as_ref().and_then(ratio)?;
    let iso = exif.iso_speed_ratings.map(u32::from).or(exif.iso_speed)? as f32;

    if shutter <= 0.0 || aperture <= 0.0 {
        return None;
    }

    Some(shutter * iso / (aperture * aperture))
}

pub fn film_mode(path: &Path) -> Option<&'static str> {
    use std::io::Read;

    const SCAN: usize = 2 * 1024 * 1024;

    let file = std::fs::File::open(path).ok()?;
    let mut bytes = Vec::new();
    file.take(SCAN as u64).read_to_end(&mut bytes).ok()?;

    let code = find_film_mode_tag(&bytes)?;

    Some(match code {
        0x0000 => "Provia",

        0x0100 | 0x0110 | 0x0120 | 0x0130 | 0x0300 => "Astia",
        0x0200 | 0x0400 => "Velvia",
        0x0500 => "Pro Neg. Std",
        0x0501 => "Pro Neg. Hi",
        0x0600 => "Classic Chrome",
        0x0700 => "Eterna",
        0x0800 => "Classic Negative",
        0x0900 => "Bleach Bypass",
        0x0a00 => "Nostalgic Neg",
        0x0b00 => "Reala ACE",
        _ => return None,
    })
}

fn makernote_tag(bytes: &[u8], wanted: u16) -> Option<Vec<f32>> {
    const MARKER: &[u8] = b"FUJIFILM";

    let u16_at = |at: usize| -> Option<u16> {
        Some(u16::from_le_bytes(bytes.get(at..at + 2)?.try_into().ok()?))
    };
    let u32_at = |at: usize| -> Option<u32> {
        Some(u32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?))
    };

    let mut from = MARKER.len();

    while let Some(offset) = bytes.get(from..)?
        .windows(MARKER.len())
        .position(|window| window == MARKER)
        .map(|found| from + found)
    {
        from = offset + 1;

        let Some(base) = u32_at(offset + 8).map(|relative| offset + relative as usize) else {
            continue;
        };
        let Some(count) = u16_at(base) else { continue };

        if count == 0 || count > 200 {
            continue;
        }

        for index in 0..count as usize {
            let entry = base + 2 + index * 12;
            if u16_at(entry) != Some(wanted) {
                continue;
            }

            let kind = u16_at(entry + 2)?;
            let values = u32_at(entry + 4)? as usize;
            let width = match kind {
                3 => 2,
                4 => 4,
                5 => 8,
                10 => 8,
                _ => 4,
            };

            let data = if values * width <= 4 {
                entry + 8
            } else {
                offset + u32_at(entry + 8)? as usize
            };

            if data.checked_add(values.checked_mul(width)?)? > bytes.len() {
                return None;
            }
            let mut out = Vec::with_capacity(values);
            for slot in 0..values {
                let at = data + slot * width;
                out.push(match kind {
                    3 => u16_at(at)? as f32,
                    4 => u32_at(at)? as f32,
                    5 => {
                        let (n, d) = (u32_at(at)?, u32_at(at + 4)?);
                        if d == 0 { 0.0 } else { n as f32 / d as f32 }
                    }
                    10 => {
                        let n = i32::from_le_bytes(bytes.get(at..at + 4)?.try_into().ok()?);
                        let d = i32::from_le_bytes(bytes.get(at + 4..at + 8)?.try_into().ok()?);
                        if d == 0 { 0.0 } else { n as f32 / d as f32 }
                    }
                    _ => u32_at(at)? as f32,
                });
            }
            return Some(out);
        }
    }

    None
}

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

    fn at(table: &[f32], radii: &[f32], radius: f32) -> f32 {
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

fn is_raf(path: &Path) -> bool {
    use std::io::Read;
    let Ok(mut file) = std::fs::File::open(path) else { return false };
    let mut magic = [0u8; 15];
    file.read_exact(&mut magic).is_ok() && &magic == b"FUJIFILMCCD-RAW"
}

pub fn lens_profile(path: &Path) -> Option<LensProfile> {
    fuji_lens_profile(path).or_else(|| lensfun_profile(path))
}

fn lensfun_profile(path: &Path) -> Option<LensProfile> {
    let summary = summary(path)?;
    let (width, height) = summary.sensor;
    super::lensfun::profile(
        &summary.make,
        &summary.model,
        summary.lens.as_deref()?,
        summary.focal_length?,

        summary.aperture.unwrap_or(8.0),
        width,
        height,
    )
}

fn fuji_lens_profile(path: &Path) -> Option<LensProfile> {
    if !is_raf(path) {
        return None;
    }

    use byteorder::{BigEndian, ReadBytesExt};
    use rawler::formats::tiff::IFD;
    use std::io::{Seek, SeekFrom};

    const SECOND_TIFF_POINTER: u64 = 100;

    const FUJI_IFD: u16 = 0xf000;
    const DISTORTION_PARAMS: u16 = 0xf00b;
    const CHROMATIC_PARAMS: u16 = 0xf00f;
    const VIGNETTING_PARAMS: u16 = 0xf010;

    let file = std::fs::File::open(path).ok()?;
    let mut reader = std::io::BufReader::new(file);

    reader.seek(SeekFrom::Start(SECOND_TIFF_POINTER)).ok()?;
    let offset = reader.read_u32::<BigEndian>().ok()?;
    let ifd = IFD::new_root_with_correction(&mut reader, 0, offset, 0, 10, &[FUJI_IFD]).ok()?;

    let table = |tag: u16| -> Option<Vec<f32>> {
        let entry = ifd.get_entry_recursive(tag)?;
        Some((0..entry.count() as usize).map(|index| entry.value.force_f32(index)).collect())
    };

    let falloff = table(VIGNETTING_PARAMS)?;
    if falloff.len() < 3 || (falloff.len() - 1) % 2 != 0 {
        return None;
    }
    let samples = (falloff.len() - 1) / 2;
    let radii: Vec<f32> = falloff[1..1 + samples].to_vec();
    let transmission: Vec<f32> = falloff[1 + samples..].iter().map(|v| v / 100.0).collect();

    if radii.windows(2).any(|pair| pair[1] < pair[0]) {
        return None;
    }
    if transmission.iter().any(|t| !(0.1..=1.5).contains(t)) {
        return None;
    }

    let same_radii = |values: &[f32]| values.len() > samples && values[1..1 + samples] == radii[..];

    let distortion = table(DISTORTION_PARAMS)
        .filter(|values| values.len() == 1 + samples * 2 && same_radii(values))

        .filter(|values| values[1 + samples..].iter().all(|v| v.abs() < 20.0))
        .map(|values| values[1 + samples..].to_vec())
        .unwrap_or_default();

    let chromatic = table(CHROMATIC_PARAMS)
        .filter(|values| values.len() >= 1 + samples * 3 && same_radii(values))
        .filter(|values| values[1 + samples..1 + samples * 3].iter().all(|v| v.abs() < 0.05));
    let (red, blue) = match chromatic {
        Some(values) => (
            values[1 + samples..1 + samples * 2].to_vec(),
            values[1 + samples * 2..1 + samples * 3].to_vec(),
        ),
        None => (Vec::new(), Vec::new()),
    };

    Some(LensProfile { radii, transmission, distortion, red, blue })
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

pub fn colour_setting(path: &Path) -> Option<u16> {
    use std::io::Read;
    let mut bytes = Vec::new();
    std::fs::File::open(path).ok()?.take(2 * 1024 * 1024).read_to_end(&mut bytes).ok()?;
    makernote_tag(&bytes, 0x1003).and_then(|values| values.first().map(|v| *v as u16))
}

fn find_film_mode_tag(bytes: &[u8]) -> Option<u16> {
    makernote_tag(bytes, 0x1401).and_then(|values| values.first().map(|v| *v as u16))
}

fn match_camera_exposure(path: &Path, image: &LinearImage) -> Option<f32> {

    const LIMIT_EV: f32 = 2.5;

    let preview = embedded_preview(path).ok()??.to_rgb8();
    if preview.width() < 16 || preview.height() < 16 {
        return None;
    }

    let mut rendered: Vec<f32> = preview
        .pixels()
        .step_by(37)
        .map(|p| {
            (0.2126 * p[0] as f32 + 0.7152 * p[1] as f32 + 0.0722 * p[2] as f32) / 255.0
        })
        .collect();
    let mut scene: Vec<f32> = image
        .data
        .chunks_exact(3)
        .step_by(37)
        .map(|p| 0.2126 * p[0] + 0.7152 * p[1] + 0.0722 * p[2])
        .collect();

    if rendered.is_empty() || scene.is_empty() {
        return None;
    }

    let median = |values: &mut Vec<f32>| {
        values.sort_by(f32::total_cmp);
        values[values.len() / 2]
    };

    let wanted = crate::core::tone::scene_value_for(median(&mut rendered));
    let have = median(&mut scene);

    if !(have > 1e-6) || !wanted.is_finite() {
        return None;
    }

    let factor = wanted / have;
    let stops = factor.log2();
    if !stops.is_finite() {
        return None;
    }

    Some(stops.clamp(-LIMIT_EV, LIMIT_EV).exp2())
}

#[derive(Debug, Clone, Default)]
pub struct Summary {
    pub camera: Option<String>,

    pub make: String,
    pub model: String,
    pub lens: Option<String>,
    pub focal_length: Option<f32>,
    pub aperture: Option<f32>,
    pub shutter: Option<f32>,
    pub iso: Option<u32>,
    pub exposure_bias: Option<f32>,
    pub taken: Option<String>,
    pub film_mode: Option<String>,

    pub sensor: (u32, u32),
    pub file_size: Option<u64>,
}

impl Summary {

    pub fn shutter_text(&self) -> Option<String> {
        let seconds = self.shutter?;
        Some(if seconds >= 1.0 {
            format!("{seconds:.1} s")
        } else {
            format!("1/{:.0} s", 1.0 / seconds.max(1e-6))
        })
    }

    pub fn megapixels(&self) -> f32 {
        (self.sensor.0 as f32 * self.sensor.1 as f32) / 1_000_000.0
    }
}

pub fn summary(path: &Path) -> Option<Summary> {
    std::panic::catch_unwind(|| read_summary(path)).ok().flatten()
}

fn read_summary(path: &Path) -> Option<Summary> {
    let params = RawDecodeParams::default();
    let source = rawler::rawsource::RawSource::new(path).ok()?;
    let decoder = rawler::get_decoder(&source).ok()?;
    let metadata = decoder.raw_metadata(&source, &params).ok()?;
    let exif = &metadata.exif;

    let ratio = |value: &Option<rawler::formats::tiff::Rational>| {
        value.as_ref().and_then(|v| (v.d != 0).then(|| v.n as f32 / v.d as f32))
    };
    let signed = |value: &Option<rawler::formats::tiff::SRational>| {
        value.as_ref().and_then(|v| (v.d != 0).then(|| v.n as f32 / v.d as f32))
    };

    let raw = decoder.raw_image(&source, &params, true).ok()?;
    let sensor = raw
        .crop_area
        .map_or((raw.width as u32, raw.height as u32), |crop| {
            (crop.width() as u32, crop.height() as u32)
        });

    Some(Summary {
        lens: metadata
            .lens
            .as_ref()
            .map(|lens| lens.lens_model.clone())
            .filter(|model| !model.is_empty())
            .or_else(|| lens_model(path)),
        camera: Some(format!("{} {}", metadata.make, metadata.model).trim().to_string())
            .filter(|text| !text.is_empty()),
        make: metadata.make.clone(),
        model: metadata.model.clone(),
        focal_length: ratio(&exif.focal_length),
        aperture: ratio(&exif.fnumber),
        shutter: ratio(&exif.exposure_time),
        iso: exif.iso_speed_ratings.map(u32::from).or(exif.iso_speed),
        exposure_bias: signed(&exif.exposure_bias),
        taken: exif.date_time_original.clone(),
        film_mode: film_mode(path).map(str::to_string),
        sensor,
        file_size: std::fs::metadata(path).ok().map(|meta| meta.len()),
    })
}

fn lens_model(path: &Path) -> Option<String> {
    if !is_raf(path) {
        return None;
    }

    use byteorder::{BigEndian, ReadBytesExt};
    use rawler::formats::tiff::IFD;
    use std::io::{Seek, SeekFrom};

    const MAIN_TIFF_POINTER: u64 = 84;
    const TIFF_HEADER_SKIP: u32 = 12;
    const EXIF_IFD: u16 = 0x8769;
    const LENS_MODEL: u16 = 0xa434;

    let file = std::fs::File::open(path).ok()?;
    let mut reader = std::io::BufReader::new(file);

    reader.seek(SeekFrom::Start(MAIN_TIFF_POINTER)).ok()?;
    let offset = reader.read_u32::<BigEndian>().ok()?;

    let ifd = IFD::new_root_with_correction(
        &mut reader,
        0,
        offset + TIFF_HEADER_SKIP,
        0,
        10,
        &[EXIF_IFD],
    )
    .ok()?;

    ifd.get_entry_recursive(LENS_MODEL)
        .and_then(|entry| entry.value.as_string().cloned())
        .map(|model| model.trim().to_string())
        .filter(|model| !model.is_empty())
}
