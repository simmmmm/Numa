use rayon::prelude::*;
use std::path::Path;

use image::{DynamicImage, RgbImage};
use rawler::decoders::RawDecodeParams;
use rawler::imgop::develop::{Intermediate, ProcessingStep, RawDevelop};

use std::sync::Arc;

use numa_core::color::CameraProfile;
use numa_core::lens::{correct_geometry, correct_vignetting, LensProfile};
use numa_core::profile::DngProfile;
use crate::dcp;
use numa_core::image::LinearImage;

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

pub fn embedded_preview(path: &Path) -> Result<Option<DynamicImage>, String> {
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
    let document = numa_core::document::Document::new(path.display().to_string());

    Ok(DynamicImage::ImageRgb8(numa_render::develop(&document, proxy, &Default::default())))
}

pub fn as_shot(path: &Path) -> Result<RgbImage, String> {
    if !is_raw(path) {
        return load_scaled(path, u32::MAX);
    }
    let linear = decode_linear_best(path)?;
    let document = numa_core::document::Document::new(path.display().to_string());
    Ok(numa_render::develop(&document, linear, &Default::default()))
}

fn open_heif(path: &Path) -> Result<DynamicImage, String> {
    let bytes = std::fs::read(path).map_err(|err| err.to_string())?;
    let context =
        heic_rs::context::Context::open(&bytes).map_err(|err: heic_rs::Error| err.to_string())?;
    let primary = context.meta.primary;

    let full = heif_item(&context, primary);
    if full.is_ok() {
        return full;
    }

    let transforms = context.props(primary).map(|props| props.transforms).unwrap_or_default();
    for preview in heif_previews(&context, primary) {
        if let Ok(image) = heif_preview(&context, preview, &transforms) {
            log::warn!(
                "{}: only its {}x{} preview — this file's full resolution needs HEVC                  Range Extensions, which the decoder here does not do",
                path.display(),
                image.width(),
                image.height(),
            );
            return Ok(image);
        }
    }
    full
}

fn heif_preview(
    context: &heic_rs::context::Context<'_>,
    item: u32,
    transforms: &[heic_rs::props::Transform],
) -> Result<DynamicImage, String> {
    let props = context.props(item).map_err(|err: heic_rs::Error| err.to_string())?;
    if props.hvcc.is_some() {
        return heif_item(context, item);
    }
    let data = context.item_data(item).map_err(|err: heic_rs::Error| err.to_string())?;
    let image = image::load_from_memory(&data).map_err(|err| err.to_string())?;
    let turns = match props.transforms.is_empty() {
        true => transforms,
        false => &props.transforms,
    };
    Ok(turn_heif(image, turns))
}

fn turn_heif(mut image: DynamicImage, transforms: &[heic_rs::props::Transform]) -> DynamicImage {
    use heic_rs::props::simple::{Mirror, Rotation};
    use heic_rs::props::Transform;

    for transform in transforms {
        image = match transform {

            Transform::Rotate(Rotation::Ccw90) => image.rotate270(),
            Transform::Rotate(Rotation::Ccw180) => image.rotate180(),
            Transform::Rotate(Rotation::Ccw270) => image.rotate90(),
            Transform::Rotate(Rotation::None) => image,
            Transform::Mirror(Mirror::LeftRight) => image.fliph(),
            Transform::Mirror(Mirror::TopBottom) => image.flipv(),

            Transform::Crop(_) => image,
        };
    }
    image
}

fn heif_previews(context: &heic_rs::context::Context<'_>, item: u32) -> Vec<u32> {
    use heic_rs::meta::iref::RefKind;

    let mut previews: Vec<(u32, u32)> = context
        .meta
        .refs
        .iter()
        .filter(|reference| reference.kind == RefKind::Thmb && reference.to.contains(&item))
        .map(|reference| {
            let area = context
                .props(reference.from)
                .ok()
                .and_then(|props| props.ispe)
                .map_or(0, |ispe| ispe.width * ispe.height);
            (reference.from, area)
        })
        .collect();
    previews.sort_by_key(|(_, area)| std::cmp::Reverse(*area));
    previews.into_iter().map(|(item, _)| item).collect()
}

fn heif_item(
    context: &heic_rs::context::Context<'_>,
    item: u32,
) -> Result<DynamicImage, String> {
    use heic_rs::props::colr::{MatrixCoefficients, Nclx, Range};

    let text = |err: heic_rs::Error| err.to_string();
    let props = context.props(item).map_err(text)?;
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

    let frame = match context.grid(item).map_err(text)? {
        Some((grid, tiles)) => {
            let frames = tiles.par_iter().map(|tile| decode(*tile)).collect::<Result<Vec<_>, _>>()?;
            heic_rs::grid::compose(&grid, &frames, limit).map_err(text)?
        }
        None => decode(item)?,
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
    fn the_median_network_agrees_with_a_sort() {
        let mut state = 0x2545_f491_4f6c_dd1du64;
        let mut next = || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state
        };

        for round in 0..20_000 {
            let mut window = [0f32; 9];
            for value in window.iter_mut() {

                *value = match round % 2 {
                    0 => (next() % 1_000_000) as f32 / 1000.0,
                    _ => (next() % 3) as f32,
                };
            }
            let mut sorted = window;
            sorted.sort_by(f32::total_cmp);
            assert_eq!(
                median_of_nine(window),
                sorted[4],
                "network disagrees on {window:?}"
            );
        }
    }

    #[test]
    fn the_false_colour_step_takes_speckle_and_keeps_an_edge() {
        use rawler::pixarray::Color2D;

        const LEFT: [f32; 3] = [0.30, 0.40, 0.20];
        const RIGHT: [f32; 3] = [0.50, 0.20, 0.45];
        let (width, height) = (40usize, 20usize);
        let edge = 20usize;

        let mut data: Vec<[f32; 3]> = (0..width * height)
            .map(|i| if i % width < edge { LEFT } else { RIGHT })
            .collect();

        let spikes = [(5usize, 5usize), (9, 12), (14, 7), (30, 14)];
        for (x, y) in spikes {
            data[y * width + x] = [0.9, 0.4, 0.05];
        }

        let mut rgb = Color2D::new_with(data, width, height);
        suppress_false_colour(rgb.data.as_flattened_mut(), width, height, BLACK_FLOOR);

        for (x, y) in spikes {
            let fixed = rgb.data[y * width + x];
            let want = if x < edge { LEFT } else { RIGHT };
            assert!(
                (fixed[0] / fixed[1] - want[0] / want[1]).abs() < 0.01
                    && (fixed[2] / fixed[1] - want[2] / want[1]).abs() < 0.01,
                "the spike at ({x}, {y}) survived: {fixed:?}"
            );

            assert_eq!(fixed[1], 0.4, "the median must not move green");
        }

        let same = |got: [f32; 3], want: [f32; 3]| {
            got.iter().zip(want).all(|(got, want)| (got - want).abs() < 1e-5)
        };
        for y in 1..height - 1 {
            let (before, after) = (rgb.data[y * width + edge - 1], rgb.data[y * width + edge]);
            assert!(same(before, LEFT), "the edge bled left at row {y}: {before:?}");
            assert!(same(after, RIGHT), "the edge bled right at row {y}: {after:?}");
        }
    }

    #[test]
    fn a_heif_rotation_turns_counter_clockwise() {
        use heic_rs::props::simple::{Mirror, Rotation};
        use heic_rs::props::Transform;

        let mut wide = RgbImage::new(2, 1);
        wide.put_pixel(0, 0, image::Rgb([0, 0, 0]));
        wide.put_pixel(1, 0, image::Rgb([255, 255, 255]));
        let wide = DynamicImage::ImageRgb8(wide);

        let turned = turn_heif(wide.clone(), &[Transform::Rotate(Rotation::Ccw90)]);
        assert_eq!((turned.width(), turned.height()), (1, 2));
        assert_eq!(turned.to_rgb8().get_pixel(0, 0), &image::Rgb([255, 255, 255]));
        assert_eq!(turned.to_rgb8().get_pixel(0, 1), &image::Rgb([0, 0, 0]));

        let back = turn_heif(wide.clone(), &[Transform::Rotate(Rotation::Ccw270)]);
        assert_eq!(back.to_rgb8().get_pixel(0, 0), &image::Rgb([0, 0, 0]));

        let flipped = turn_heif(wide.clone(), &[Transform::Mirror(Mirror::LeftRight)]);
        assert_eq!(flipped.to_rgb8().get_pixel(0, 0), &image::Rgb([255, 255, 255]));

        assert_eq!(turn_heif(wide, &[]).to_rgb8().get_pixel(0, 0), &image::Rgb([0, 0, 0]));
    }

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
    fn a_profile_that_corrects_nothing_is_not_a_profile_that_matched() {
        let nothing = flat_profile(0.0, 0.0, 0.0);
        assert!(!nothing.corrects_anything(), "zero tables correct nothing");

        let mut falloff = flat_profile(0.0, 0.0, 0.0);
        falloff.transmission = vec![1.0, 0.95, 0.8, 0.62];
        assert!(!falloff.bends_anything(), "falloff is not geometry");
        assert!(falloff.corrects_anything(), "but it is still a correction");

        assert!(flat_profile(-2.0, 0.0, 0.0).corrects_anything());
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

    #[test]
    fn an_aberration_table_survives_radii_that_are_not_the_falloff_tables() {

        let nine: Vec<f32> = (1..=9).map(|k| (k as f32 / 8.0).sqrt()).collect();
        let mut x_t5 = vec![515.9];
        x_t5.extend(&nine);
        x_t5.extend((1..=9).map(|k| k as f32 * 1e-5));
        x_t5.extend((1..=9).map(|k| k as f32 * -2e-5));
        x_t5.push(515.9);
        assert_eq!(x_t5.len(), 29);

        let (red, blue) = fuji_chromatic(Some(&x_t5), &nine);
        assert_eq!(red.len(), 9, "the falloff table's radii are what is stored");
        assert!((red[8] - 9e-5).abs() < 1e-9, "red at the last radius: {:?}", red[8]);
        assert!((blue[8] + 18e-5).abs() < 1e-9, "blue at the last radius: {:?}", blue[8]);

        let eleven: Vec<f32> = (0..=10).map(|k| k as f32 / 10.0).collect();
        let mut x_t20 = vec![327.7];
        x_t20.extend(eleven.iter().skip(1));
        x_t20.extend((1..=10).map(|k| k as f32 * 1e-5));
        x_t20.extend((1..=10).map(|k| k as f32 * -2e-5));
        assert_eq!(x_t20.len(), 31);

        let (red, blue) = fuji_chromatic(Some(&x_t20), &eleven);
        assert_eq!(red.len(), 11);
        assert!((red[10] - 10e-5).abs() < 1e-9, "red at 1.0: {:?}", red[10]);
        assert!((blue[10] + 20e-5).abs() < 1e-9, "blue at 1.0: {:?}", blue[10]);
        assert!((red[5] - 5e-5).abs() < 1e-9, "red at 0.5: {:?}", red[5]);

        assert!((red[0] - 1e-5).abs() < 1e-9, "red at the centre: {:?}", red[0]);

        let (red, _) = fuji_chromatic(Some(&x_t20), &nine);
        assert!((red[0] - 3.5355e-5).abs() < 1e-9, "red at sqrt(1/8): {:?}", red[0]);

        assert!(fuji_chromatic(None, &nine).0.is_empty());
        assert!(fuji_chromatic(Some(&[327.7, 0.5, 1.0]), &nine).0.is_empty());

        assert!(fuji_chromatic(Some(&[0.0; 31]), &eleven).0.is_empty());
        let mut wild = x_t20.clone();
        wild[12] = 0.4;
        assert!(fuji_chromatic(Some(&wild), &eleven).0.is_empty());
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

pub fn decode_for_editing(path: &Path) -> Result<LinearImage, String> {
    match is_raw(path) {
        true => decode_linear_best(path),
        false => decode_linear_any(path),
    }
}

pub fn decode_linear_best(path: &Path) -> Result<LinearImage, String> {
    decode_with(path, Demosaic::Best).map(|(image, _)| image)
}

pub fn decode_with_exposure(path: &Path) -> Result<(LinearImage, Option<f32>), String> {
    decode_with(path, Demosaic::Draft)
}

#[cfg(feature = "dump-switches")]
fn dump_without_vignetting() -> bool {
    std::env::var("NUMA_DUMP_NO_VIGNETTE").is_ok()
}

#[cfg(feature = "dump-switches")]
fn dump_without_rendering() -> bool {
    std::env::var("NUMA_DUMP_PROFILE").as_deref() == Ok("none")
}

#[cfg(not(feature = "dump-switches"))]
fn dump_without_vignetting() -> bool {
    false
}

#[cfg(not(feature = "dump-switches"))]
fn dump_without_rendering() -> bool {
    false
}

fn decode_with(path: &Path, demosaic: Demosaic) -> Result<(LinearImage, Option<f32>), String> {
    let fail = |err: String| format!("{}: {}", path.display(), err);

    let (developed, profile, rendering, flips, film_mode, exposure, markesteijn) = {
        let source = rawler::rawsource::RawSource::new(path).map_err(|e| fail(e.to_string()))?;
        let decoder = rawler::get_decoder(&source).map_err(|e| fail(e.to_string()))?;
        let raw = decoder
            .raw_image(&source, &RawDecodeParams::default(), false)
            .map_err(|e| fail(e.to_string()))?;

        let profile = camera_profile(&raw);
        let rendering = find_rendering(&raw).filter(|_| !dump_without_rendering());

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

        let demosaiced = match demosaic {
            Demosaic::Best => markesteijn_demosaic(&raw, path),
            Demosaic::Draft => None,
        };

        let markesteijn_ran = demosaiced.is_some();
        let developed = match demosaiced {
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

        (developed, profile, rendering, flips, film_mode, exposure, markesteijn_ran)
    };

    let dim = developed.dim();
    let baseline = 2.0f32.powf(BASELINE_EV);

    let mut scaled = developed.into_flatten();
    scaled.par_iter_mut().for_each(|v| *v *= baseline);
    let image = LinearImage::new(dim.w as u32, dim.h as u32, scaled);

    let mut image = undo_the_lens(image, path, markesteijn, baseline);

    let factor = match_camera_exposure(path, &image).unwrap_or(1.0);
    if factor != 1.0 {
        image.data.par_iter_mut().for_each(|value| *value *= factor);
    }

    let image = image.with_clip(baseline * factor);

    let image = image
        .into_oriented(flips.0, flips.1, flips.2)
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
    use rawler::rawimage::{RawImageData, RawPhotometricInterpretation};

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

    let (width, height) = (scaled.width, scaled.height);
    let (active_area, crop_area) = (scaled.active_area, scaled.crop_area);
    let floats = match std::mem::replace(&mut scaled.data, RawImageData::Integer(Vec::new())) {
        RawImageData::Float(floats) => floats,
        RawImageData::Integer(_) => return Ok(None),
    };
    drop(scaled);

    let pixels = PixF32::new_with(floats, width, height);

    let active = active_area.unwrap_or_else(|| pixels.rect());

    let rgb = XTransMarkesteijnDemosaic::new(1).demosaic(&pixels, &config.cfa, &config.colors, active);
    drop(pixels);

    Ok(Some(match crop_area {
        Some(crop) => {
            let adapted = crop.adapt(&active);
            if adapted.d == rgb.dim() { rgb } else { cropped_in_place(rgb, adapted) }
        }
        None => rgb,
    }))
}

fn undo_the_lens(
    mut image: LinearImage,
    path: &Path,
    markesteijn: bool,
    baseline: f32,
) -> LinearImage {
    let lens = lens_profile(path);
    if let Some(profile) = &lens {
        if !dump_without_vignetting() {
            correct_vignetting(&mut image, profile);
        }
    }
    let mut image = match &lens {
        Some(profile) if profile.bends_anything() => {

            let straight = correct_geometry(&image, profile);
            drop(image);
            straight
        }
        _ => image,
    };

    if markesteijn {
        let (w, h) = (image.width as usize, image.height as usize);
        suppress_false_colour(&mut image.data, w, h, BLACK_FLOOR * baseline);
    }
    image
}

const BLACK_FLOOR: f32 = 1e-4;

fn suppress_false_colour(data: &mut [f32], width: usize, height: usize, black: f32) {
    if width < 3 || height < 3 {
        return;
    }

    let mut ratio = vec![0f32; width * height];
    for channel in [0usize, 2] {
        ratio.par_iter_mut().zip(data.par_chunks(3)).for_each(|(ratio, pixel)| {

            *ratio = if pixel[1] > black { pixel[channel] / pixel[1] } else { 1.0 };
        });

        data.par_chunks_mut(width * 3).enumerate().for_each(|(y, row)| {

            let above = &ratio[y.saturating_sub(1) * width..][..width];
            let here = &ratio[y * width..][..width];
            let below = &ratio[(y + 1).min(height - 1) * width..][..width];

            for x in 0..width {
                let (left, right) = (x.saturating_sub(1), (x + 1).min(width - 1));
                let window = [
                    above[left], above[x], above[right],
                    here[left], here[x], here[right],
                    below[left], below[x], below[right],
                ];
                if row[x * 3 + 1] > black {
                    row[x * 3 + channel] = row[x * 3 + 1] * median_of_nine(window);
                }
            }
        });
    }
}

fn median_of_nine(mut w: [f32; 9]) -> f32 {
    macro_rules! sort2 {
        ($a:expr, $b:expr) => {{
            let (low, high) = (w[$a].min(w[$b]), w[$a].max(w[$b]));
            w[$a] = low;
            w[$b] = high;
        }};
    }
    sort2!(1, 2); sort2!(4, 5); sort2!(7, 8);
    sort2!(0, 1); sort2!(3, 4); sort2!(6, 7);
    sort2!(1, 2); sort2!(4, 5); sort2!(7, 8);
    sort2!(0, 3); sort2!(5, 8); sort2!(4, 7);
    sort2!(3, 6); sort2!(1, 4); sort2!(2, 5);
    sort2!(4, 7); sort2!(4, 2); sort2!(6, 4);
    sort2!(4, 2);
    w[4]
}

fn cropped_in_place(
    rgb: rawler::pixarray::Color2D<f32, 3>,
    area: rawler::imgop::Rect,
) -> rawler::pixarray::Color2D<f32, 3> {
    let width = rgb.width;
    let mut data = rgb.into_inner();
    for row in 0..area.d.h {
        let from = (area.p.y + row) * width + area.p.x;
        data.copy_within(from..from + area.d.w, row * area.d.w);
    }
    data.truncate(area.d.h * area.d.w);
    rawler::pixarray::Color2D::new_with(data, area.d.w, area.d.h)
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

pub(crate) fn is_raf(path: &Path) -> bool {
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

    let distortion = table(DISTORTION_PARAMS)
        .filter(|values| values.len() == 1 + samples * 2 && values[1..1 + samples] == radii[..])

        .filter(|values| values[1 + samples..].iter().all(|v| v.abs() < 20.0))
        .map(|values| values[1 + samples..].to_vec())
        .unwrap_or_default();

    let (red, blue) = fuji_chromatic(table(CHROMATIC_PARAMS).as_deref(), &radii);

    Some(LensProfile { radii, transmission, distortion, red, blue })
}

fn fuji_chromatic(values: Option<&[f32]>, radii: &[f32]) -> (Vec<f32>, Vec<f32>) {
    let nothing = (Vec::new(), Vec::new());
    let Some(values) = values.filter(|values| values.len() >= 4) else { return nothing };

    let samples = (values.len() - 1) / 3;
    let (own, red) = (&values[1..1 + samples], &values[1 + samples..1 + samples * 2]);
    let blue = &values[1 + samples * 2..1 + samples * 3];

    if own.windows(2).any(|pair| pair[1] <= pair[0]) {
        return nothing;
    }
    if red.iter().chain(blue).any(|v| v.abs() >= 0.05) {
        return nothing;
    }

    let onto = |table: &[f32]| radii.iter().map(|r| LensProfile::at(table, own, *r)).collect();
    (onto(red), onto(blue))
}

pub fn colour_setting(path: &Path) -> Option<u16> {
    use std::io::Read;
    let mut bytes = Vec::new();
    std::fs::File::open(path).ok()?.take(2 * 1024 * 1024).read_to_end(&mut bytes).ok()?;
    makernote_tag(&bytes, 0x1003).and_then(|values| values.first().map(|v| *v as u16))
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct AfPoint {
    pub x: f32,
    pub y: f32,

    pub zone: bool,
}

pub fn af_point(path: &Path) -> Option<AfPoint> {
    use std::io::Read;
    if !is_raf(path) {
        return None;
    }

    const SCAN: u64 = 2 * 1024 * 1024;
    let mut bytes = Vec::new();
    std::fs::File::open(path).ok()?.take(SCAN).read_to_end(&mut bytes).ok()?;

    let at = makernote_tag(&bytes, 0x1023)?;
    let (x, y) = (*at.first()?, *at.get(1)?);
    let single = makernote_tag(&bytes, 0x1022).and_then(|mode| mode.first().copied()) == Some(1.0);

    let offset = u32::from_be_bytes(bytes.get(84..88)?.try_into().ok()?) as usize;
    let jpeg = bytes.get(offset..)?;
    let (width, height) = jpeg_size(jpeg)?;
    let orientation = ::exif::Reader::new()
        .read_from_container(&mut std::io::Cursor::new(jpeg))
        .ok()
        .and_then(|exif| exif.get_field(::exif::Tag::Orientation, ::exif::In::PRIMARY)?.value.get_uint(0))
        .unwrap_or(1);

    let (u, v) = (x / width as f32, y / height as f32);
    if !(0.0..=1.0).contains(&u) || !(0.0..=1.0).contains(&v) {
        return None;
    }

    let (x, y) = match orientation {
        3 => (1.0 - u, 1.0 - v),
        6 => (1.0 - v, u),
        8 => (v, 1.0 - u),
        _ => (u, v),
    };
    Some(AfPoint { x, y, zone: !single })
}

pub fn shot(path: &Path) -> Option<(f32, f32)> {
    use std::io::Read;
    let exif = if is_raf(path) {
        let mut bytes = Vec::new();
        std::fs::File::open(path).ok()?.take(2 * 1024 * 1024).read_to_end(&mut bytes).ok()?;
        let offset = u32::from_be_bytes(bytes.get(84..88)?.try_into().ok()?) as usize;
        ::exif::Reader::new().read_from_container(&mut std::io::Cursor::new(bytes.get(offset..)?)).ok()?
    } else {
        let file = std::fs::File::open(path).ok()?;
        ::exif::Reader::new().read_from_container(&mut std::io::BufReader::new(file)).ok()?
    };
    let exposure = match &exif.get_field(::exif::Tag::ExposureTime, ::exif::In::PRIMARY)?.value {
        ::exif::Value::Rational(values) => values.first()?.to_f64() as f32,
        _ => return None,
    };
    let focal = exif.get_field(::exif::Tag::FocalLengthIn35mmFilm, ::exif::In::PRIMARY)?.value.get_uint(0)? as f32;
    (exposure > 0.0 && focal > 0.0).then_some((exposure, focal))
}

pub fn raw_levels(path: &Path) -> Option<(f32, f32)> {
    if !is_raw(path) {
        return None;
    }

    let raw = std::panic::catch_unwind(|| rawler::decode_file(path).ok()).ok().flatten()?;
    let rawler::RawImageData::Integer(data) = &raw.data else { return None };
    let white = *raw.whitelevel.0.first()? as f32;
    let black = raw.blacklevel.levels.first().map_or(0.0, |level| level.as_f32());
    let clip = (white - (white - black) * 0.01) as u16;
    let floor = (black + (white - black) * 0.01) as u16;

    const BLOCK: usize = 6;
    let (width, height) = (raw.width, raw.height);
    let (across, down) = (width / BLOCK, height / BLOCK);
    if across == 0 || down == 0 || data.len() < width * height {
        return None;
    }
    let (mut clipped, mut dark) = (0usize, 0usize);
    for by in 0..down {
        for bx in 0..across {
            let block = (0..BLOCK).flat_map(|y| {
                let row = (by * BLOCK + y) * width + bx * BLOCK;
                data[row..row + BLOCK].iter().copied()
            });
            let brightest = block.max().unwrap_or(0);
            clipped += usize::from(brightest >= clip);
            dark += usize::from(brightest <= floor);
        }
    }
    let blocks = (across * down) as f32;
    Some((clipped as f32 / blocks, dark as f32 / blocks))
}

fn jpeg_size(jpeg: &[u8]) -> Option<(u32, u32)> {
    let mut at = 2;
    while at + 9 < jpeg.len() {
        if jpeg[at] != 0xFF {
            return None;
        }
        let marker = jpeg[at + 1];
        let length = u16::from_be_bytes([jpeg[at + 2], jpeg[at + 3]]) as usize;
        if matches!(marker, 0xC0..=0xC3) {
            let height = u16::from_be_bytes([jpeg[at + 5], jpeg[at + 6]]) as u32;
            let width = u16::from_be_bytes([jpeg[at + 7], jpeg[at + 8]]) as u32;
            return (width > 0 && height > 0).then_some((width, height));
        }
        at += 2 + length;
    }
    None
}

fn find_film_mode_tag(bytes: &[u8]) -> Option<u16> {
    makernote_tag(bytes, 0x1401).and_then(|values| values.first().map(|v| *v as u16))
}

fn match_camera_exposure(path: &Path, image: &LinearImage) -> Option<f32> {

    const LIMIT_EV: f32 = 2.5;

    let preview = embedded_preview(path).ok()??.into_rgb8();
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

    let wanted = numa_core::tone::scene_value_for(median(&mut rendered));
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
