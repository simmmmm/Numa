pub mod ai_denoise;
pub mod detail;
pub mod dust;
pub mod display;
pub mod distractions;
pub mod effects;
pub mod align;
pub mod beautify;
pub mod bracket;
pub mod classify;
pub mod histogram;
pub mod local;
pub mod auto;
pub mod matte;
pub mod range;
pub mod remove;
pub mod retouch;
pub mod sam;
pub mod segment;

use image::RgbImage;
use std::borrow::Cow;
use rayon::prelude::*;

use numa_core::curve::{self, Curve};
use numa_core::point::PointColours;
use numa_core::profile::{multiply, Look, Rendering, Table};
use numa_core::tone::{self, MIDDLE_GREY};
use numa_core::document::{Basic, Document, Operation};
use numa_core::image::LinearImage;
use numa_core::mask::{Alpha, Mask, Pixels, Shape};
use numa_core::space::ColourSpace;

pub const NO_COLOUR_PROFILE: &str = "";

const REGION_STOPS: f32 = 2.0;
const ENDPOINT_STOPS: f32 = 1.0;

fn luminance(pixel: &[f32]) -> f32 {
    0.2126 * pixel[0] + 0.7152 * pixel[1] + 0.0722 * pixel[2]
}

fn tone_position(luma: f32) -> f32 {
    luma.max(0.0).powf(1.0 / 2.2).min(1.0)
}

fn ramp(value: f32) -> f32 {
    let clamped = value.clamp(0.0, 1.0);
    clamped * clamped
}

#[derive(Default, Clone)]
pub struct RenderInputs {

    pub profile: Option<std::sync::Arc<numa_core::profile::DngProfile>>,

    pub denoised: Option<std::sync::Arc<numa_core::denoise::Denoised>>,

    pub sharpened: Option<std::sync::Arc<numa_core::denoise::Denoised>>,
}

pub fn to_working_space<'a>(
    document: &Document,
    source: impl Into<Cow<'a, LinearImage>>,
    inputs: &RenderInputs,
) -> LinearImage {
    let source = source.into();

    let source = match ai_denoise::for_render(document, &source, inputs.denoised.as_deref()) {
        Some(denoised) => Cow::Owned(denoised),
        None => source,
    };

    let mut source = match ai_denoise::sharpen_for_render(document, &source, inputs.sharpened.as_deref()) {
        Some(sharpened) => Cow::Owned(sharpened),
        None => source,
    };

    let balance = document.white_balance;

    let Some(profile) = source.profile.clone() else {
        return source.into_owned();
    };

    let mut data = match &mut source {
        Cow::Borrowed(frame) => frame.data.clone(),
        Cow::Owned(frame) => std::mem::take(&mut frame.data),
    };
    let source = &*source;

    let effective = balance.unwrap_or_else(|| profile.as_shot_white_balance());

    let resolved = profile_for(document, source, inputs)
        .as_ref()
        .and_then(|dng| Rendering::resolve(dng, effective.temperature));

    match resolved {
        Some(rendering) => {

            let multipliers = profile.multipliers(effective);
            let clip = source.clip;
            let lowest = multipliers[0].min(multipliers[1]).min(multipliers[2]);

            data.par_chunks_exact_mut(3).for_each(|pixel| {
                let balanced = [
                    pixel[0] * multipliers[0],
                    pixel[1] * multipliers[1],
                    pixel[2] * multipliers[2],
                ];
                let balanced = neutralise_clipping(balanced, pixel, clip, lowest);
                pixel.copy_from_slice(&rendering.render(balanced));
            });
        }
        None => {
            let matrix = profile.transform(balance);

            let multipliers = profile.multipliers(effective);
            let clip = source.clip;
            let lowest = multipliers[0].min(multipliers[1]).min(multipliers[2]);
            data.par_chunks_exact_mut(3).for_each(|pixel| {
                let balanced = [
                    pixel[0] * multipliers[0],
                    pixel[1] * multipliers[1],
                    pixel[2] * multipliers[2],
                ];
                let fixed = neutralise_clipping(balanced, pixel, clip, lowest);

                let source_pixel = [
                    fixed[0] / multipliers[0],
                    fixed[1] / multipliers[1],
                    fixed[2] / multipliers[2],
                ];
                let (r, g, b) = (source_pixel[0], source_pixel[1], source_pixel[2]);
                for (channel, row) in pixel.iter_mut().zip(matrix.iter()) {

                    *channel = row[0] * r + row[1] * g + row[2] * b;
                }
            });
        }
    }

    match ColourSpace::Srgb.convert_to(document.working_space) {
        Some(matrix) => data.par_chunks_exact_mut(3).for_each(|pixel| {
            let (r, g, b) = (pixel[0], pixel[1], pixel[2]);
            for (channel, row) in pixel.iter_mut().zip(matrix.iter()) {
                *channel = (row[0] * r + row[1] * g + row[2] * b).max(0.0);
            }
        }),
        None => data.par_iter_mut().for_each(|value| *value = value.max(0.0)),
    }

    let mut working = LinearImage::new(source.width, source.height, data)
        .with_film_mode(source.film_mode.clone());
    working.white_point = Some(effective);
    working
}

fn neutralise_clipping(balanced: [f32; 3], camera: &[f32], clip: Option<f32>, floor: f32) -> [f32; 3] {
    let Some(clip) = clip else { return balanced };

    let highest = camera[0].max(camera[1]).max(camera[2]);
    let blend = ((highest - clip * 0.9) / (clip * 0.1)).clamp(0.0, 1.0);
    if blend <= 0.0 {
        return balanced;
    }

    let ceiling = clip * floor;
    let mut out = balanced;
    for channel in 0..3 {
        let held = out[channel].min(ceiling);
        out[channel] += (held - out[channel]) * blend;
    }
    out
}

fn profile_for(
    document: &Document,
    source: &LinearImage,
    inputs: &RenderInputs,
) -> Option<std::sync::Arc<numa_core::profile::DngProfile>> {
    match document.colour_profile.as_deref() {

        Some(NO_COLOUR_PROFILE) => None,
        Some(_) => inputs.profile.clone().or_else(|| source.rendering.clone()),
        None => source.rendering.clone(),
    }
}

pub const MASK_RASTER: usize = 4096;

pub fn raster_size(width: u32, height: u32, long_edge: usize) -> (usize, usize) {
    let long = width.max(height).max(1) as f32;
    (
        ((width as f32 / long * long_edge as f32).round() as usize).max(1),
        ((height as f32 / long * long_edge as f32).round() as usize).max(1),
    )
}

pub fn resolve_mask(
    mask: &mut Mask,
    found: Option<&segment::Segmentation>,
    clicked: Option<&sam::Embedding>,
    frame: Option<&image::RgbImage>,
    width: usize,
    height: usize,
) {

    if let Shape::ColourRange { hue, spread, saturation, picked } = mask.shape {
        let Some(frame) = frame else { return };

        let ranged = match picked {
            true => range::colour(frame, hue, spread, saturation, width, height),
            false => Alpha::new(width, height, vec![0.0; width * height]),
        };
        mask.unshaped = Pixels::of(&ranged);
        mask.reshape_edge(width, height);
        return;
    }
    if let Shape::LuminanceRange { low, high, softness, picked } = mask.shape {
        let Some(frame) = frame else { return };
        let ranged = match picked {
            true => range::luminance(frame, low, high, softness, width, height),
            false => Alpha::new(width, height, vec![0.0; width * height]),
        };
        mask.unshaped = Pixels::of(&ranged);
        mask.reshape_edge(width, height);
        return;
    }

    let mut matted = false;
    let base = match (&mask.shape, found) {
        (Shape::Segment { classes }, Some(segmentation)) => {

            let live: Vec<u16> =
                classes.iter().copied().filter(|class| !mask.muted.contains(class)).collect();
            let named = segmentation.alpha(&live);

            let empty = !segment::named_something(&named);
            let matteable = live.iter().any(|class| segment::MATTEABLE.contains(class));
            match empty && matteable {
                true => match matte::subject(segmentation.photo(), width, height) {
                    Some(subject) => {

                        matted = true;
                        Some(subject)
                    }
                    None => Some(named),
                },
                false => Some(named),
            }
        }

        (Shape::Segment { .. }, None) => return,
        _ => None,
    };

    let regions: Vec<(Alpha, bool)> = mask
        .points
        .iter()
        .filter(|point| point.enabled)
        .filter_map(|point| {
            let region = clicked
                .and_then(|embedding| embedding.at(point.at[0], point.at[1]))
                .or_else(|| {
                    found.map(|segmentation| {
                        segmentation.region_at(point.at[0], point.at[1]).1
                    })
                })?;
            Some((region, point.subtract))
        })
        .collect();

    let named = base.as_ref().map(share_of);
    let alpha = mask.rasterise(base.as_ref(), &regions, width, height);

    let photo = found.map(segment::Segmentation::photo).filter(|_| !matted);
    let (alpha, from_matte) = search_edge(mask, alpha, named, |coarse| matte::refine(photo?, coarse));

    mask.matted = from_matte || matted;

    mask.unshaped = Pixels::of(&alpha);
    mask.reshape_edge(width, height);
}

fn share_of(alpha: &Alpha) -> f64 {
    alpha.data.iter().map(|value| *value as f64).sum::<f64>() / alpha.data.len().max(1) as f64
}

fn search_edge(
    mask: &Mask,
    mut alpha: Alpha,
    named: Option<f64>,
    refine: impl FnOnce(&Alpha) -> Option<Alpha>,
) -> (Alpha, bool) {
    if !mask.matte {
        return (alpha, false);
    }
    mask.edge_to_search_from(&mut alpha);
    let Some(refined) = refine(&alpha) else { return (alpha, false) };

    let (Some(before), after) = (named, share_of(&refined)) else {
        return (refined, true);
    };
    if after < before * 0.5 {

        log::info!(
            "matting kept {:.2}% of a {:.2}% mask; keeping the one that was named",
            after * 100.0,
            before * 100.0
        );
        return (alpha, false);
    }
    (refined, true)
}

fn mask_geometry(document: &Document) -> Document {
    let mut geometry = Document::new(document.source.path.clone());
    geometry.set_perspective(document.perspective());
    if let Some((rect, angle)) = document.crop() {
        geometry.set_crop(rect, angle);
    }
    geometry.set_rotation(document.rotation());
    geometry.set_mirrored(document.mirrored());
    geometry
}

pub fn fine_frame(document: &Document, source: &LinearImage, inputs: &RenderInputs) -> RgbImage {
    let geometry = mask_geometry(document);
    let smaller = source.downscaled(MASK_RASTER as u32).map_or(Cow::Borrowed(source), Cow::Owned);
    let working = to_working_space(&geometry, smaller, inputs);
    apply_stack(&geometry, working, 1.0)
}

pub fn refine_finely(mask: &mut Mask, frame: &RgbImage) -> bool {
    let Some(unshaped) = mask.unshaped.0.as_deref().filter(|_| mask.fine && mask.matted) else {
        return false;
    };
    let (width, height) = (unshaped.width, unshaped.height);
    let Some(closer) = matte::finer(frame, &unshaped.to_alpha()) else { return false };
    mask.unshaped = Pixels::of(&closer);
    mask.reshape_edge(width, height);
    true
}

fn models_needed(masks: &[Mask]) -> (bool, bool) {
    let semantic = masks
        .iter()
        .any(|mask| matches!(mask.shape, Shape::Segment { .. }) || !mask.points.is_empty());
    let prompt = masks.iter().any(|mask| !mask.points.is_empty());
    (semantic, prompt)
}

pub fn with_masks_resolved(document: &Document, source: &LinearImage) -> Document {
    let mut masks = document.masks();
    if !masks.iter().any(Mask::wants_pixels) {
        return document.clone();
    }

    let geometry = mask_geometry(document);

    let proxy = source.downscaled(2400).unwrap_or_else(|| source.clone());
    let working = to_working_space(&geometry, proxy, &RenderInputs::default());
    let frame = apply_stack(&geometry, working, 1.0);

    let (wants_model, wants_prompt) = models_needed(&masks);
    let found = wants_model.then(|| segment::of(&frame)).flatten();

    let clicked = wants_prompt.then(|| sam::encode(&frame)).flatten();

    let (width, height) = raster_size(frame.width(), frame.height(), MASK_RASTER);

    let mut fine = None;
    for mask in masks.iter_mut().filter(|mask| mask.wants_pixels()) {
        resolve_mask(mask, found.as_ref(), clicked.as_ref(), Some(&frame), width, height);
        if mask.fine && mask.matted && source.width.max(source.height) > frame.width().max(frame.height()) {
            let fine = fine.get_or_insert_with(|| fine_frame(document, source, &RenderInputs::default()));
            refine_finely(mask, fine);
        }
    }

    let mut resolved = document.clone();
    resolved.set_masks(masks);
    resolved
}

pub fn apply_stack<'a>(
    document: &Document,
    working: impl Into<Cow<'a, LinearImage>>,
    detail_scale: f32,
) -> RgbImage {
    stack(document, working, detail_scale, local::Tone::Own)
}

pub fn apply_stack_recording<'a>(
    document: &Document,
    working: impl Into<Cow<'a, LinearImage>>,
    detail_scale: f32,
) -> (RgbImage, Option<local::ToneGuide>) {
    let measured = std::cell::RefCell::new(None);
    let image = stack(document, working, detail_scale, local::Tone::Record(&measured));
    (image, measured.into_inner())
}

fn stack<'a, T: Sample>(
    document: &Document,
    working: impl Into<Cow<'a, LinearImage>>,
    detail_scale: f32,
    tone: local::Tone,
) -> Frame<T>
where
    image::Rgb<T>: image::Pixel<Subpixel = T>,
{
    let working = working.into();

    match geometry_of(document, &working) {

        Some(geometry) => {
            drop(working);
            pixels(document, geometry, detail_scale, WHOLE_FRAME, tone)
        }
        None => pixels(document, working, detail_scale, WHOLE_FRAME, tone),
    }
}

pub const WHOLE_FRAME: [f32; 4] = [0.0, 0.0, 1.0, 1.0];

pub fn apply_pixels<'a>(
    document: &Document,
    working: impl Into<Cow<'a, LinearImage>>,
    detail_scale: f32,
    region: [f32; 4],
) -> RgbImage {
    pixels(document, working, detail_scale, region, local::Tone::Own)
}

pub fn measure_tone<'a>(document: &Document, working: impl Into<Cow<'a, LinearImage>>) -> Option<local::ToneGuide> {
    let basic = document.basic();
    if basic.presence.hdr == 0.0 && basic.presence.clarity == 0.0 && basic.presence.texture == 0.0 {
        return None;
    }

    let measured = std::cell::RefCell::new(None);
    let working = working.into();
    match geometry_of(document, &working) {
        Some(framed) => finished(document, framed, 1.0, WHOLE_FRAME, local::Tone::Measure(&measured)),
        None => finished(document, working, 1.0, WHOLE_FRAME, local::Tone::Measure(&measured)),
    };
    measured.into_inner()
}

pub fn apply_pixels_guided<'a>(
    document: &Document,
    working: impl Into<Cow<'a, LinearImage>>,
    detail_scale: f32,
    region: [f32; 4],
    guide: &local::ToneGuide,
) -> RgbImage {
    pixels(document, working, detail_scale, region, local::Tone::Guided(guide, region))
}

fn pixels<'a, T: Sample>(
    document: &Document,
    working: impl Into<Cow<'a, LinearImage>>,
    detail_scale: f32,
    region: [f32; 4],
    tone: local::Tone,
) -> Frame<T>
where
    image::Rgb<T>: image::Pixel<Subpixel = T>,
{
    let (data, width, height) = finished(document, working, detail_scale, region, tone);
    encode(width, height, &data, &document.curves(), document.working_space, document.output_space)
}

struct Passes {
    started: Option<std::time::Instant>,
    costs: Vec<(&'static str, f32)>,
}

impl Passes {
    fn new() -> Self {
        static ON: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        let on = *ON.get_or_init(|| std::env::var_os("NUMA_TIMING").is_some());
        Self { started: on.then(std::time::Instant::now), costs: Vec::new() }
    }

    fn mark(&mut self, name: &'static str) {
        if let Some(started) = self.started {
            let cost = started.elapsed().as_secs_f32() * 1000.0;
            self.costs.push((name, cost));
            self.started = Some(std::time::Instant::now());
        }
    }

    fn report(mut self, width: usize, height: usize) {
        if self.started.is_none() {
            return;
        }
        self.costs.sort_by(|a, b| b.1.total_cmp(&a.1));
        let total: f32 = self.costs.iter().map(|(_, cost)| cost).sum();
        let worst: Vec<String> = self
            .costs
            .iter()
            .filter(|(_, cost)| *cost >= 1.0)
            .map(|(name, cost)| format!("{name} {cost:.0}"))
            .collect();
        log::info!("stack {width}x{height} in {total:.0} ms — {}", worst.join(", "));
    }
}

fn finished<'a>(
    document: &Document,
    working: impl Into<Cow<'a, LinearImage>>,
    detail_scale: f32,
    region: [f32; 4],
    tone: local::Tone,
) -> (Vec<f32>, u32, u32) {
    let mut passes = Passes::new();

    let measuring = matches!(tone, local::Tone::Measure(_));
    let mut working = working.into();
    let mut data = take_pixels(&mut working);
    let working = &*working;

    let basic = document.basic();
    let (width, height) = (working.width as usize, working.height as usize);
    let weights = document.working_space.luminance_weights();
    match measuring {
        true => prefix(document, &basic, &mut data, (width, height), detail_scale, region, measuring, &mut passes),
        false => prefix_kept(document, &basic, &mut data, (width, height), detail_scale, region, &mut passes),
    }

    run_operations(document, &mut data, working, tone);

    if measuring {
        return (data, working.width, working.height);
    }

    passes.mark("operations");

    let (point, space) = (working.white_point, document.working_space);
    apply_masks(document, &mut data, width, height, region, point, space, detail_scale);

    passes.mark("masks");

    let mixer = document.mixer();
    if mixer.monochrome {
        data.par_chunks_exact_mut(3).for_each(|pixel| {
            let rgb = [pixel[0], pixel[1], pixel[2]];
            let luminance = rgb[0] * weights[0] + rgb[1] * weights[1] + rgb[2] * weights[2];
            pixel.fill(luminance * mixer.grey_gain(rgb));
        });
    }

    passes.mark("monochrome");

    let grading = document.grading();
    if !grading.is_identity() {
        data.par_chunks_exact_mut(3).for_each(|pixel| {
            let graded = grading.apply([pixel[0], pixel[1], pixel[2]]);
            pixel.copy_from_slice(&graded);
        });
    }

    passes.mark("grade");

    vignette_and_grain(&mut data, width, height, region, &basic, weights, detail_scale);
    passes.mark("vignette");
    passes.report(width, height);
    (data, working.width, working.height)
}

#[allow(clippy::too_many_arguments)]
fn prefix(
    document: &Document,
    basic: &Basic,
    data: &mut [f32],
    (width, height): (usize, usize),
    detail_scale: f32,
    region: [f32; 4],
    measuring: bool,
    passes: &mut Passes,
) {

    if !measuring {
        retouch::apply(
            &document.retouch(),
            data,
            width,
            height,
            region,
        );
    }

    passes.mark("retouch");
    let faces = document.faces();
    if !faces.is_empty() {
        beautify::apply(
            &document.beautify(),
            &faces,
            data,
            width,
            height,
            region,
        );
    }

    passes.mark("faces");

    effects::dehaze(data, width, height, basic.effects.dehaze);
    passes.mark("dehaze");

    if !measuring {
        detail::passes(data, width, height, basic, detail_scale);
    }

    passes.mark("detail");

    let weights = document.working_space.luminance_weights();
    effects::calibrate(
        data,
        [basic.calibration.red_hue, basic.calibration.green_hue, basic.calibration.blue_hue],
        [basic.calibration.red_saturation, basic.calibration.green_saturation, basic.calibration.blue_saturation],
        basic.calibration.shadow_tint,
        weights,
    );

    passes.mark("calibrate");
}

fn prefix_kept(
    document: &Document,
    basic: &Basic,
    data: &mut Vec<f32>,
    size: (usize, usize),
    detail_scale: f32,
    region: [f32; 4],
    passes: &mut Passes,
) {
    const KEEP: usize = 4;
    const LARGEST: usize = 8_000_000 * 3;
    type Kept = Vec<(u64, std::sync::Arc<Vec<f32>>)>;
    static KEPT: std::sync::Mutex<Kept> = std::sync::Mutex::new(Vec::new());

    #[cfg(test)]
    let off = KEEP_OFF.load(std::sync::atomic::Ordering::Relaxed);
    #[cfg(not(test))]
    let off = false;
    if off || data.len() > LARGEST {
        prefix(document, basic, data, size, detail_scale, region, false, passes);
        return;
    }
    let settings = format!(
        "{:?}",
        (
            document.retouch(),
            document.faces(),
            document.beautify(),
            basic.effects.dehaze,
            basic.detail,
            basic.calibration,
            document.working_space,
            detail_scale,
            region,
            size,
        )
    );
    let key = content_hash(data) ^ content_hash_bytes(settings.as_bytes()).rotate_left(1);
    let found = KEPT.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).iter().find(|(at, _)| *at == key).map(|(_, kept)| kept.clone());
    if let Some(kept) = found {
        data.copy_from_slice(&kept);
        passes.mark("prefix kept");
        return;
    }
    prefix(document, basic, data, size, detail_scale, region, false, passes);
    let mut kept = KEPT.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    kept.retain(|(at, _)| *at != key);
    if kept.len() >= KEEP {
        kept.remove(0);
    }
    kept.push((key, std::sync::Arc::new(data.clone())));
}

#[cfg(test)]
static KEEP_OFF: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

pub(crate) fn content_hash(data: &[f32]) -> u64 {
    let chunks: Vec<u64> = data
        .par_chunks(1 << 16)
        .map(|chunk| chunk.iter().fold(0xcbf2_9ce4_8422_2325u64, |hash, value| (hash ^ u64::from(value.to_bits())).wrapping_mul(0x100_0000_01b3)))
        .collect();
    chunks.iter().fold(data.len() as u64, |hash, chunk| (hash ^ chunk).wrapping_mul(0x100_0000_01b3).rotate_left(17))
}

pub(crate) fn content_hash_bytes(bytes: &[u8]) -> u64 {
    let chunks: Vec<u64> = bytes
        .par_chunks(1 << 18)
        .map(|chunk| chunk.iter().fold(0xcbf2_9ce4_8422_2325u64, |hash, byte| (hash ^ u64::from(*byte)).wrapping_mul(0x100_0000_01b3)))
        .collect();
    chunks.iter().fold(bytes.len() as u64, |hash, chunk| (hash ^ chunk).wrapping_mul(0x100_0000_01b3).rotate_left(17))
}

pub const HDR_STOPS: f32 = 3.0;

pub fn develop_hdr<'a>(
    document: &Document,
    source: impl Into<Cow<'a, LinearImage>>,
    inputs: &RenderInputs,
    detail_scale: f32,
) -> (RgbImage, Vec<f32>) {
    let working = to_working_space(document, source, inputs);
    let (data, width, height) = match geometry_of(document, &working) {
        Some(geometry) => {
            drop(working);
            finished(document, geometry, detail_scale, WHOLE_FRAME, local::Tone::Own)
        }
        None => finished(document, working, detail_scale, WHOLE_FRAME, local::Tone::Own),
    };
    let curves = document.curves();
    let frame = encode(width, height, &data, &curves, document.working_space, document.output_space);
    let shape = shaper(&curves);
    let weights = document.working_space.luminance_weights();
    let ceiling = HDR_STOPS.exp2();
    let gain = data
        .par_chunks_exact(3)
        .map(|pixel| {
            let scene = pixel[0] * weights[0] + pixel[1] * weights[1] + pixel[2] * weights[2];
            if scene <= tone::MIDDLE_GREY {
                return 1.0;
            }
            let sdr: f32 = (0..3)
                .map(|channel| weights[channel] * ColourSpace::Srgb.decode(shape(channel, tone::curve(pixel[channel]).clamp(0.0, 1.0))))
                .sum();
            (scene.min(ceiling) / sdr.max(1e-6)).max(1.0)
        })
        .collect();
    (frame, gain)
}

fn take_pixels(working: &mut Cow<'_, LinearImage>) -> Vec<f32> {

    match working {
        Cow::Borrowed(frame) => frame.data.clone(),
        Cow::Owned(frame) => std::mem::take(&mut frame.data),
    }
}

fn run_operations(document: &Document, data: &mut [f32], working: &LinearImage, tone: local::Tone) {

    let mut looks: Vec<Look> = Vec::new();
    let mixer = document.mixer();
    if !mixer.colour_is_identity() {
        looks.push(Look::new(Table::resolve(&mixer.table(), 0.0)).in_space(document.working_space));
    }

    let points = document.point_colours();
    let colour_pass = |data: &mut [f32]| {
        apply_looks(&looks, data);
        apply_point_colours(&points, document.working_space, data);
    };
    let colour_work = !looks.is_empty() || !points.is_identity();

    for operation in &document.operations {
        apply(operation, data);

        if colour_work && matches!(operation, Operation::Basic(_)) {
            colour_pass(data);
        }

        if let Operation::Basic(basic) = operation {
            local::tone_map(
                data,
                working.width as usize,
                working.height as usize,
                basic.presence.hdr / 100.0,
                basic.presence.clarity / 100.0,
                basic.presence.texture / 100.0,
                tone,
            );
        }
    }

    if colour_work
        && !document.operations.iter().any(|op| matches!(op, Operation::Basic(_)))
    {
        colour_pass(data);
    }
}

fn vignette_and_grain(
    data: &mut [f32],
    width: usize,
    height: usize,
    region: [f32; 4],
    basic: &Basic,
    weights: [f32; 3],
    detail_scale: f32,
) {
    let frame = [width as f32 / region[2].max(1e-6), height as f32 / region[3].max(1e-6)];
    effects::vignette(
        data,
        width,
        height,
        region,
        frame,
        [basic.effects.vignette, basic.effects.vignette_midpoint, basic.effects.vignette_roundness, basic.effects.vignette_feather],
    );
    let full = frame.map(|edge| edge / detail_scale.max(1e-6));
    effects::grain(
        data,
        width,
        height,
        region,
        full,
        [basic.effects.grain, basic.effects.grain_size, basic.effects.grain_roughness],
        weights,
    );
}

fn through(table: &[f32; curve::LOOKUP], display: f32) -> f32 {
    let at = display.clamp(0.0, 1.0) * (curve::LOOKUP - 1) as f32;
    let low = at.floor() as usize;
    let high = (low + 1).min(curve::LOOKUP - 1);
    let fraction = at - low as f32;
    table[low] * (1.0 - fraction) + table[high] * fraction
}

#[allow(clippy::too_many_arguments)]
fn apply_masks(
    document: &Document,
    data: &mut [f32],
    width: usize,
    height: usize,
    region: [f32; 4],
    white_point: Option<numa_core::color::WhiteBalance>,
    space: ColourSpace,
    detail_scale: f32,
) {

    let map = document.masks_map.unwrap_or([1.0, 0.0, 0.0, 0.0, 1.0, 0.0]);
    for mask in document.masks() {
        if mask.is_idle() || mask.covers_nothing() {
            continue;
        }

        let field = mask.field_through(width, height, region, map);

        let Some((top, bottom)) = rows_to_work(&mask.basic, &field, width, height) else { continue };
        let height = bottom + 1 - top;
        let (start, end) = (top * width * 3, (bottom + 1) * width * 3);
        let field = &field[top * width..(bottom + 1) * width];
        let data = &mut data[start..end];
        let mut local = data.to_vec();

        if let Some(from) = white_point {
            if mask.basic.balance.temperature != 0.0 || mask.basic.balance.tint != 0.0 {
                let to = numa_core::color::WhiteBalance {
                    temperature: from.temperature + mask.basic.balance.temperature,
                    tint: from.tint + mask.basic.balance.tint,
                };
                let gains = numa_core::color::relative_gains(from, to, space.from_xyz());
                local.par_chunks_exact_mut(3).for_each(|pixel| {
                    for (channel, gain) in pixel.iter_mut().zip(gains) {
                        *channel = (*channel * gain).max(0.0);
                    }
                });
            }
        }

        effects::dehaze(&mut local, width, height, mask.basic.effects.dehaze);
        detail::denoise(
            &mut local,
            width,
            height,
            mask.basic.detail.denoise_luma / 100.0,
            mask.basic.detail.denoise_detail / 100.0,
            mask.basic.detail.denoise_contrast / 100.0,
            detail_scale,
        );
        detail::denoise_colour(
            &mut local,
            width,
            height,
            mask.basic.detail.denoise_colour / 100.0,
            detail_scale,
        );
        detail::sharpen(
            &mut local,
            width,
            height,
            mask.basic.detail.sharpen / 100.0,
            mask.basic.detail.sharpen_radius,
            mask.basic.detail.sharpen_masking / 100.0,
            detail_scale,
        );
        detail::defringe(&mut local, width, height, mask.basic.detail.defringe / 100.0, detail_scale);
        detail::moire(&mut local, width, height, mask.basic.detail.moire / 100.0, detail_scale);

        apply_basic(&mask.basic, &mut local);
        local::tone_map(
            &mut local,
            width,
            height,
            mask.basic.presence.hdr / 100.0,
            mask.basic.presence.clarity / 100.0,
            mask.basic.presence.texture / 100.0,
            local::Tone::Own,
        );

        if !mask.curve.is_identity() {
            let table = mask.curve.lookup();
            local.par_chunks_exact_mut(3).for_each(|pixel| {
                for channel in pixel.iter_mut() {
                    let display = numa_core::tone::curve(*channel).clamp(0.0, 1.0);
                    *channel = numa_core::tone::scene_value_for(through(&table, display));
                }
            });
        }

        data.par_chunks_exact_mut(3).enumerate().for_each(|(index, pixel)| {
            let weight = field[index];
            if weight <= 0.0 {
                return;
            }
            for (channel, value) in pixel.iter_mut().enumerate() {
                let edited = local[index * 3 + channel];
                *value += (edited - *value) * weight;
            }
        });
    }
}

fn rows_to_work(basic: &Basic, field: &[f32], width: usize, height: usize) -> Option<(usize, usize)> {
    const MARGIN: usize = 64;
    let (first, last) = covered_rows(field, width)?;
    if measures_frame(basic) {
        return Some((0, height.saturating_sub(1)));
    }
    Some((first.saturating_sub(MARGIN), (last + MARGIN).min(height.saturating_sub(1))))
}

fn covered_rows(field: &[f32], width: usize) -> Option<(usize, usize)> {
    let covered = |row: &[f32]| row.iter().any(|weight| *weight > 0.0);
    let rows: Vec<bool> = field.par_chunks(width).map(covered).collect();
    let first = rows.iter().position(|covered| *covered)?;
    let last = rows.iter().rposition(|covered| *covered)?;
    Some((first, last))
}

fn apply_point_colours(points: &PointColours, space: ColourSpace, data: &mut [f32]) {
    if points.is_identity() {
        return;
    }
    let (into, back) = (space.convert_to(ColourSpace::Srgb), ColourSpace::Srgb.convert_to(space));
    data.par_chunks_exact_mut(3).for_each(|pixel| {
        let mut colour = [pixel[0], pixel[1], pixel[2]];
        if let Some(matrix) = &into {
            colour = multiply(matrix, colour);
        }
        colour = points.apply(colour);
        if let Some(matrix) = &back {
            colour = multiply(matrix, colour);
        }
        pixel.copy_from_slice(&colour.map(|value| value.max(0.0)));
    });
}

fn apply_looks(looks: &[Look], data: &mut [f32]) {
    if looks.is_empty() {
        return;
    }
    data.par_chunks_exact_mut(3).for_each(|pixel| {
        let mut colour = [pixel[0], pixel[1], pixel[2]];
        for look in looks {
            colour = look.apply(colour);
        }
        pixel.copy_from_slice(&colour);
    });
}

pub fn tiles_cleanly(document: &Document) -> bool {
    !measures_frame(&document.basic()) && tiles_with_guide(document)
}

fn measures_frame(basic: &Basic) -> bool {
    basic.presence.hdr != 0.0
        || basic.presence.clarity != 0.0
        || basic.presence.texture != 0.0
        || basic.effects.dehaze != 0.0
}

pub fn tiles_with_guide(document: &Document) -> bool {
    document.basic().effects.dehaze == 0.0
        && document.masks().iter().all(|mask| mask.is_idle() || mask.covers_nothing() || !measures_frame(&mask.basic))
        && document.beautify().is_identity()
}

pub fn spots_within(document: &Document, region: [f32; 4], frame: [f32; 2]) -> bool {
    let retouch = document.retouch();
    if retouch.is_identity() {
        return true;
    }
    if region[2] <= 0.0 || region[3] <= 0.0 || frame[0] <= 0.0 || frame[1] <= 0.0 {
        return false;
    }

    let long_edge = frame[0].max(frame[1]);
    let margin = [retouch::RING * long_edge / frame[0], retouch::RING * long_edge / frame[1]];
    retouch.spots.iter().filter(|spot| !spot.is_idle()).all(|spot| {

        let reach = match spot.kind {
            numa_core::retouch::Kind::Remove => remove::WINDOW_RADII / 2.0 / retouch::RING,
            _ => 1.0,
        };
        let margin = [margin[0] * spot.radius * reach, margin[1] * spot.radius * reach];
        [spot.at, spot.from].iter().all(|point| {
            point[0] - margin[0] >= region[0]
                && point[1] - margin[1] >= region[1]
                && point[0] + margin[0] <= region[0] + region[2]
                && point[1] + margin[1] <= region[1] + region[3]
        })
    })
}

pub fn develop<'a>(
    document: &Document,
    source: impl Into<Cow<'a, LinearImage>>,
    inputs: &RenderInputs,
) -> RgbImage {

    develop_at(document, source, inputs, 1.0)
}

pub fn develop_at<'a>(
    document: &Document,
    source: impl Into<Cow<'a, LinearImage>>,
    inputs: &RenderInputs,
    detail_scale: f32,
) -> RgbImage {
    apply_stack(document, to_working_space(document, source, inputs), detail_scale)
}

pub fn develop16<'a>(
    document: &Document,
    source: impl Into<Cow<'a, LinearImage>>,
    inputs: &RenderInputs,
) -> Frame<u16> {
    develop16_at(document, source, inputs, 1.0)
}

pub fn develop16_at<'a>(
    document: &Document,
    source: impl Into<Cow<'a, LinearImage>>,
    inputs: &RenderInputs,
    detail_scale: f32,
) -> Frame<u16> {
    stack(document, to_working_space(document, source, inputs), detail_scale, local::Tone::Own)
}

pub type Frame<T> = image::ImageBuffer<image::Rgb<T>, Vec<T>>;

pub trait Sample: image::Primitive + Send + Sync + 'static {

    fn quantise(unit: f32) -> Self;
}

impl Sample for u8 {
    fn quantise(unit: f32) -> u8 {
        (unit * 255.0 + 0.5) as u8
    }
}

impl Sample for u16 {
    fn quantise(unit: f32) -> u16 {
        (unit * 65535.0 + 0.5) as u16
    }
}

fn apply(operation: &Operation, data: &mut [f32]) {
    match operation {
        Operation::Basic(basic) => apply_basic(basic, data),

        Operation::Crop { .. } | Operation::Rotate { .. } => {}

        Operation::Mixer(_) => {}

        Operation::PointColours(_) => {}

        Operation::Mask(_) => {}

        Operation::Grading(_) => {}

        Operation::Retouch(_) => {}

        Operation::Beautify(_) => {}

        Operation::Curve(_) | Operation::ChannelCurve { .. } => {}
    }
}

pub fn tile_in_source(document: &Document, tile: [f32; 4]) -> Option<[f32; 4]> {

    let basic = document.basic();
    if document.rotation() != 0.0
        || document.mirrored()
        || !document.perspective().is_identity()
        || basic.optics.lens_distortion != 0.0
        || basic.optics.lens_vignetting != 0.0
    {
        return None;
    }

    match document.crop() {
        None => Some(tile),
        Some((_, angle)) if angle != 0.0 => None,
        Some(([crop_x, crop_y, crop_width, crop_height], _)) => Some([
            crop_x + tile[0] * crop_width,
            crop_y + tile[1] * crop_height,
            tile[2] * crop_width,
            tile[3] * crop_height,
        ]),
    }
}

pub fn cut_turned_tile(document: &Document, full: &LinearImage, region: [f32; 4]) -> Option<LinearImage> {
    let basic = document.basic();
    if !document.perspective().is_identity()
        || basic.optics.lens_distortion != 0.0
        || basic.optics.lens_vignetting != 0.0
    {
        return None;
    }
    let quarter = match document.rotation() as i32 {
        0 => 0,
        90 => 1,
        180 => 2,
        270 => 3,
        _ => return None,
    };
    let mirrored = document.mirrored();
    let (width, height) = (full.width as f32, full.height as f32);

    let (turned_w, turned_h) = if quarter % 2 == 1 { (height, width) } else { (width, height) };
    let (crop, angle) = document.crop().unwrap_or(([0.0, 0.0, 1.0, 1.0], 0.0));

    let crop_w = (crop[2] * turned_w).round().max(1.0);
    let crop_h = (crop[3] * turned_h).round().max(1.0);
    let centre = ((crop[0] + crop[2] / 2.0) * turned_w, (crop[1] + crop[3] / 2.0) * turned_h);
    let (sin, cos) = angle.to_radians().sin_cos();
    let (dx, dy) = ((region[0] + region[2] / 2.0 - 0.5) * crop_w, (region[1] + region[3] / 2.0 - 0.5) * crop_h);
    let middle = (centre.0 + dx * cos - dy * sin, centre.1 + dx * sin + dy * cos);
    let (tile_w, tile_h) = (region[2] * crop_w, region[3] * crop_h);

    let reach_x = (tile_w * cos.abs() + tile_h * sin.abs()) / 2.0 + 2.0;
    let reach_y = (tile_w * sin.abs() + tile_h * cos.abs()) / 2.0 + 2.0;
    let turned_box = [
        (middle.0 - reach_x).max(0.0),
        (middle.1 - reach_y).max(0.0),
        (middle.0 + reach_x).min(turned_w),
        (middle.1 + reach_y).min(turned_h),
    ];
    if turned_box[2] <= turned_box[0] || turned_box[3] <= turned_box[1] {
        return None;
    }

    let back = |x: f32, y: f32| -> (f32, f32) {
        let (mx, my) = match quarter {
            1 => (y, height - x),
            2 => (width - x, height - y),
            3 => (width - y, x),
            _ => (x, y),
        };
        if mirrored { (width - mx, my) } else { (mx, my) }
    };
    let corners = [
        back(turned_box[0], turned_box[1]),
        back(turned_box[2], turned_box[1]),
        back(turned_box[0], turned_box[3]),
        back(turned_box[2], turned_box[3]),
    ];
    let left = corners.iter().map(|c| c.0).fold(f32::MAX, f32::min).floor().max(0.0);
    let top = corners.iter().map(|c| c.1).fold(f32::MAX, f32::min).floor().max(0.0);
    let right = corners.iter().map(|c| c.0).fold(f32::MIN, f32::max).ceil().min(width);
    let bottom = corners.iter().map(|c| c.1).fold(f32::MIN, f32::max).ceil().min(height);
    let cut = full.cropped([left / width, top / height, (right - left) / width, (bottom - top) / height], 0.0, Default::default());

    let flipped = if mirrored { cut.into_oriented(false, true, false) } else { cut };
    let small = match quarter {
        1 => flipped.into_oriented(true, false, true),
        2 => flipped.into_oriented(false, true, true),
        3 => flipped.into_oriented(true, true, false),
        _ => flipped,
    };
    let forth = |x: f32, y: f32| -> (f32, f32) {
        let (mx, my) = if mirrored { (width - x, y) } else { (x, y) };
        match quarter {
            1 => (height - my, mx),
            2 => (width - mx, height - my),
            3 => (my, width - mx),
            _ => (mx, my),
        }
    };
    let (a, b) = (forth(left, top), forth(right, bottom));
    let origin = (a.0.min(b.0), a.1.min(b.1));
    let (small_w, small_h) = (small.width as f32, small.height as f32);

    let sub_w = tile_w / small_w;
    let sub_h = tile_h / small_h;
    let sub = [
        (middle.0 - origin.0) / small_w - sub_w / 2.0,
        (middle.1 - origin.1) / small_h - sub_h / 2.0,
        sub_w,
        sub_h,
    ];
    Some(small.cropped(sub, angle, Default::default()))
}

pub fn geometry_only(document: &Document, working: &LinearImage) -> LinearImage {
    geometry_of(document, working).unwrap_or_else(|| working.clone())
}

fn geometry_of(document: &Document, working: &LinearImage) -> Option<LinearImage> {
    let rotation = document.rotation();
    let crop = document.crop();
    let mirrored = document.mirrored();
    let basic = document.basic();
    let manual_lens = basic.optics.lens_distortion != 0.0 || basic.optics.lens_vignetting != 0.0;

    if rotation == 0.0 && crop.is_none() && !mirrored && !manual_lens {
        return None;
    }

    let corrected = manual_lens.then(|| {
        let profile = numa_core::lens::manual_lens_profile(basic.optics.lens_distortion, basic.optics.lens_vignetting);
        let mut image = working.clone();
        if basic.optics.lens_vignetting != 0.0 {
            numa_core::lens::correct_vignetting(&mut image, &profile);
        }
        if basic.optics.lens_distortion != 0.0 {
            image = numa_core::lens::correct_geometry(&image, &profile);
        }
        image
    });
    let working = corrected.as_ref().unwrap_or(working);

    let flipped = mirrored.then(|| working.oriented(false, true, false));
    let working = flipped.as_ref().unwrap_or(working);

    let turned = match rotation as i32 {
        90 => Some(working.oriented(true, false, true)),
        180 => Some(working.oriented(false, true, true)),
        270 => Some(working.oriented(true, true, false)),
        _ => None,
    };
    let source = turned.as_ref().unwrap_or(working);

    match crop {
        Some((rect, angle)) => Some(source.cropped(rect, angle, document.perspective())),
        None => turned.or(flipped).or(corrected),
    }
}

fn apply_basic(basic: &Basic, data: &mut [f32]) {
    if basic.is_identity() {
        return;
    }

    let gain = 2.0f32.powf(basic.tone.exposure);

    let contrast = basic.tone.contrast / 100.0;
    let slope = if contrast >= 0.0 { 1.0 + contrast } else { 1.0 / (1.0 - contrast) };
    let contrasty = (slope - 1.0).abs() > f32::EPSILON;

    let toned = basic.tone.shadows != 0.0
        || basic.tone.highlights != 0.0
        || basic.tone.blacks != 0.0
        || basic.tone.whites != 0.0;
    let curve = toned.then(|| ToneCurve::new(basic));

    data.par_chunks_exact_mut(3).for_each(|pixel| {

        for channel in pixel.iter_mut() {
            *channel = (*channel * gain).max(0.0);
        }

        if contrasty {
            for channel in pixel.iter_mut() {
                *channel = MIDDLE_GREY * (*channel / MIDDLE_GREY).powf(slope);
            }
        }

        if let Some(curve) = &curve {
            let tone = curve.gain(luminance(pixel));
            for channel in pixel.iter_mut() {
                *channel *= tone;
            }
        }

        apply_saturation(basic, pixel);
    });
}

pub(crate) struct ToneCurve {

    stops: Vec<f32>,
}

impl ToneCurve {
    const LOW: f32 = -20.0;
    const HIGH: f32 = 8.0;
    const STEP: f32 = 0.01;

    const LEAST_SLOPE: f32 = 0.25;

    pub(crate) fn new(basic: &Basic) -> Self {
        let tone = &basic.tone;
        let count = ((Self::HIGH - Self::LOW) / Self::STEP) as usize + 1;
        let mut stops = Vec::with_capacity(count);
        let mut floor = f32::NEG_INFINITY;
        for index in 0..count {
            let level = Self::LOW + index as f32 * Self::STEP;
            let position = tone_position(level.exp2());
            let asked = region_stops(tone.shadows, ramp((0.5 - position) / 0.5), REGION_STOPS)
                + region_stops(tone.highlights, ramp((position - 0.5) / 0.5), REGION_STOPS)
                + region_stops(tone.whites, ramp((position - 0.75) / 0.25), ENDPOINT_STOPS);
            let out = Self::blacks(tone.blacks, level + asked, position).max(floor);
            floor = out + Self::LEAST_SLOPE * Self::STEP;
            stops.push(out - level);
        }
        Self { stops }
    }

    fn blacks(amount: f32, level: f32, position: f32) -> f32 {
        const LIFT_STOPS: f32 = 2.0;
        const LIFT_REACH: f32 = 0.35;

        const DEEPEST: f32 = 0.03;

        const FADE: f32 = 0.1;
        let amount = amount / 100.0;
        if amount > 0.0 {
            level + amount * LIFT_STOPS * (1.0 - position / LIFT_REACH).max(0.0)
        } else if amount < 0.0 {
            let value = level.exp2();
            let t = (value / FADE).clamp(0.0, 1.0);
            let held = t * t * (3.0 - 2.0 * t);
            (value + amount * DEEPEST * (1.0 - held)).max(1e-12).log2()
        } else {
            level
        }
    }

    pub(crate) fn gain(&self, luma: f32) -> f32 {
        let at = ((luma.max(1e-12).log2() - Self::LOW) / Self::STEP).clamp(0.0, (self.stops.len() - 1) as f32);
        let below = at as usize;
        let above = (below + 1).min(self.stops.len() - 1);
        let t = at - below as f32;
        (self.stops[below] + (self.stops[above] - self.stops[below]) * t).exp2()
    }
}

fn region_stops(amount: f32, mask: f32, stops: f32) -> f32 {
    amount / 100.0 * stops * mask
}

fn apply_saturation(basic: &Basic, pixel: &mut [f32]) {
    if basic.presence.saturation == 0.0 && basic.presence.vibrance == 0.0 {
        return;
    }

    let luma = luminance(pixel);
    let high = pixel[0].max(pixel[1]).max(pixel[2]);
    let low = pixel[0].min(pixel[1]).min(pixel[2]);

    let current = if high > 0.0 { (high - low) / high } else { 0.0 };

    let factor = 1.0 + basic.presence.saturation / 100.0 + (basic.presence.vibrance / 100.0) * (1.0 - current);

    for channel in pixel.iter_mut() {
        *channel = (luma + (*channel - luma) * factor).max(0.0);
    }
}

#[cfg(test)]
fn encode_srgb(
    width: u32,
    height: u32,
    data: &[f32],
    curves: &[Curve],
    working: ColourSpace,
    output: ColourSpace,
) -> RgbImage {
    encode(width, height, data, curves, working, output)
}

fn shaper(curves: &[Curve]) -> impl Fn(usize, f32) -> f32 + Sync {
    let lookups: Vec<Option<[f32; curve::LOOKUP]>> =
        curves.iter().map(|curve| (!curve.is_identity()).then(|| curve.lookup())).collect();
    let composite = lookups.first().cloned().flatten();
    let channel_lookup = |channel: usize| lookups.get(1 + channel).cloned().flatten();
    let channels = [channel_lookup(0), channel_lookup(1), channel_lookup(2)];
    let read = |lookup: &Option<[f32; curve::LOOKUP]>, display: f32| -> f32 {
        match lookup {
            Some(table) => through(table, display),
            None => display,
        }
    };
    move |channel: usize, display: f32| read(&channels[channel], read(&composite, display))
}

fn encode<T: Sample>(
    width: u32,
    height: u32,
    data: &[f32],

    curves: &[Curve],
    working: ColourSpace,
    output: ColourSpace,
) -> Frame<T>
where
    image::Rgb<T>: image::Pixel<Subpixel = T>,
{
    let mut out = vec![T::quantise(0.0); data.len()];

    let shape = shaper(curves);

    let recode = (working != output || output != ColourSpace::Srgb)
        .then(|| (working.convert_to(output), output));

    if let Some((matrix, target)) = recode {
        out.par_chunks_exact_mut(3)
            .zip(data.par_chunks_exact(3))
            .for_each(|(bytes, pixel)| {
                let shaped: [f32; 3] = std::array::from_fn(|channel| {
                    ColourSpace::Srgb.decode(shape(channel, tone::curve(pixel[channel]).clamp(0.0, 1.0)))
                });
                let turned = match &matrix {
                    Some(matrix) => std::array::from_fn(|channel| {
                        let row = matrix[channel];
                        row[0] * shaped[0] + row[1] * shaped[1] + row[2] * shaped[2]
                    }),
                    None => shaped,
                };
                for (byte, linear) in bytes.iter_mut().zip(turned) {
                    *byte = T::quantise(target.encode(linear));
                }
            });
        return Frame::from_raw(width, height, out).expect("buffer matches dimensions");
    }

    out.par_chunks_exact_mut(3)
        .zip(data.par_chunks_exact(3))
        .for_each(|(bytes, pixel)| {
            for (channel, (byte, value)) in bytes.iter_mut().zip(pixel).enumerate() {
                let display = tone::curve(*value).clamp(0.0, 1.0);
                *byte = T::quantise(shape(channel, display));
            }
        });

    Frame::from_raw(width, height, out).expect("buffer matches dimensions")
}

#[cfg(test)]
mod tests {

    #[test]
    fn a_kept_prefix_is_the_prefix() {
        use numa_core::document::{Basic, Document};
        let (w, h) = (96usize, 64usize);
        let data: Vec<f32> = (0..w * h * 3).map(|i| ((i * 7919) % 997) as f32 / 997.0 * 0.6).collect();
        let frame = numa_core::image::LinearImage::new(w as u32, h as u32, data);
        let with = |exposure: f32| {
            let mut document = Document::new("a.RAF".into());
            document.set_basic(Basic::with(|b| {
                b.tone.exposure = exposure;
                b.detail.sharpen = 60.0;
                b.detail.denoise_colour = 40.0;
            }));
            document
        };
        super::KEEP_OFF.store(true, std::sync::atomic::Ordering::Relaxed);
        let fresh = super::apply_stack(&with(1.0), &frame, 1.0);
        super::KEEP_OFF.store(false, std::sync::atomic::Ordering::Relaxed);
        let _ = super::apply_stack(&with(0.0), &frame, 1.0);
        let kept = super::apply_stack(&with(1.0), &frame, 1.0);
        assert_eq!(fresh.as_raw(), kept.as_raw());
    }

    use super::*;

    #[test]
    fn sixteen_bits_are_the_eight_with_more_steps() {
        let data: Vec<f32> = (0..512).flat_map(|i| {
            let t = i as f32 / 256.0;
            [t, t * 0.6, t * 0.3]
        }).collect();
        let working = LinearImage::new(512, 1, data);
        for space in [ColourSpace::Srgb, ColourSpace::DisplayP3] {
            let mut document = Document::new(String::new());
            document.set_basic(Basic::with(|b| b.tone.exposure = 0.3));
            document.output_space = space;
            let eight = apply_stack(&document, &working, 1.0);
            let sixteen: Frame<u16> = stack(&document, &working, 1.0, local::Tone::Own);
            for (at, (low, high)) in eight.iter().zip(sixteen.iter()).enumerate() {
                let brought_down = (*high as f32 / 257.0).round();
                assert!((*low as f32 - brought_down).abs() <= 1.0, "{space:?} subpixel {at}: {low} against {high}");
            }
            let distinct = |values: Vec<u32>| values.into_iter().collect::<std::collections::BTreeSet<_>>().len();
            let (few, many) = (distinct(eight.iter().map(|v| *v as u32).collect()), distinct(sixteen.iter().map(|v| *v as u32).collect()));
            assert!(many > few * 2, "{space:?}: {many} values at sixteen bits, {few} at eight");
        }
    }

    #[test]
    fn a_mask_stays_on_its_pixels_while_the_crop_tool_shows_the_whole_frame() {
        let (w, h) = (240u32, 160u32);
        let data: Vec<f32> = (0..w * h).flat_map(|i| {
            let (x, y) = ((i % w) as f32 / w as f32, (i / w) as f32 / h as f32);
            [0.05 + 0.1 * x, 0.05 + 0.1 * y, 0.08]
        }).collect();
        let image = LinearImage::new(w, h, data);
        let rect = [0.1, 0.25, 0.5, 0.6];
        let (angle, perspective) = (6.0, numa_core::document::Perspective { vertical: 15.0, horizontal: 0.0, aspect: 10.0 });
        let mut mask = Mask::new(Shape::Radial { centre: [0.35, 0.4], radius: [0.2, 0.25], feather: 0.3 });
        mask.basic.tone.exposure = 2.5;

        let mut cropped = Document::new(String::new());
        cropped.set_perspective(perspective);
        cropped.set_crop(rect, angle);
        cropped.set_masks(vec![mask]);
        let mut shown = cropped.clone();
        shown.set_crop([0.0, 0.0, 1.0, 1.0], angle);
        let lost = develop(&shown, &image, &Default::default());
        shown.masks_map = Some(numa_core::image::view_to_crop(w as f32, h as f32, rect, angle, angle, perspective));

        let crop = develop(&cropped, &image, &Default::default());
        let view = develop(&shown, &image, &Default::default());
        let [vx, vy, vw, vh] = numa_core::image::crop_in_view(w as f32, h as f32, rect, angle, perspective);
        let at = |frame: &RgbImage, u: f32, v: f32| {
            frame.get_pixel((u * frame.width() as f32) as u32, (v * frame.height() as f32) as u32)[0] as i32
        };

        let around = |frame: &RgbImage, u: f32, v: f32| {
            let (x, y) = ((u * frame.width() as f32) as i32, (v * frame.height() as f32) as i32);
            let values: Vec<i32> = (-1..=1)
                .flat_map(|dy| (-1..=1).map(move |dx| (x + dx, y + dy)))
                .map(|(x, y)| frame.get_pixel(x as u32, y as u32)[0] as i32)
                .collect();
            (*values.iter().min().unwrap(), *values.iter().max().unwrap())
        };
        let mut worst_before = 0;

        for s in [0.15, 0.2, 0.25, 0.35, 0.45, 0.5, 0.55] {
            for t in [0.2, 0.4, 0.6] {
                let here = at(&crop, s, t);
                let (low, high) = around(&view, vx + s * vw, vy + t * vh);
                assert!((low - 2..=high + 2).contains(&here), "({s}, {t}): {here} cropped, {low}..{high} in the crop tool");
                worst_before = worst_before.max((here - at(&lost, vx + s * vw, vy + t * vh)).abs());
            }
        }
        assert!(worst_before > 20, "the stretched mask should have missed by more than this: {worst_before}");
    }

    #[test]
    fn manual_lens_corrections_reach_the_edges_not_the_centre() {
        let (w, h) = (64u32, 48u32);
        let data: Vec<f32> = (0..w * h).flat_map(|i| {
            let (x, y) = (i % w, i / w);
            let v = if (x / 8 + y / 8) % 2 == 0 { 0.2 } else { 0.6 };
            [v, v, v]
        }).collect();
        let image = LinearImage::new(w, h, data);
        let at = |img: &LinearImage, x: u32, y: u32| img.data[((y * img.width + x) * 3) as usize];

        let mut lifted = Document::new(String::new());
        lifted.set_basic(Basic::with(|b| b.optics.lens_vignetting = 100.0));
        let out = geometry_of(&lifted, &image).expect("a correction to make");
        assert!(at(&out, 1, 1) > at(&image, 1, 1) * 1.2, "corners lifted");
        assert!((at(&out, 32, 24) - at(&image, 32, 24)).abs() < 0.02, "centre as it was");

        let mut straightened = Document::new(String::new());
        straightened.set_basic(Basic::with(|b| b.optics.lens_distortion = 100.0));
        let bent = geometry_of(&straightened, &image).expect("a correction to make");
        assert!((at(&bent, 32, 24) - at(&image, 32, 24)).abs() < 0.01, "the centre does not move");
        assert!((0..w).any(|x| at(&bent, x, 2) != at(&image, x, 2)), "an edge row moved");

        assert!(geometry_of(&Document::new(String::new()), &image).is_none());
    }

    #[test]
    fn a_flip_mirrors_the_frame_and_undoes_itself() {

        let data: Vec<f32> = (0..6).flat_map(|i| [i as f32, 0.0, 0.0]).collect();
        let image = LinearImage::new(3, 2, data);
        let red = |document: &Document| -> Vec<f32> {
            geometry_of(document, &image).map_or(image.data.clone(), |out| out.data).chunks(3).map(|p| p[0]).collect()
        };

        let mut document = Document::new(String::new());
        document.set_mirrored(true);
        assert_eq!(red(&document), [2.0, 1.0, 0.0, 5.0, 4.0, 3.0], "left to right");

        let mut vertical = Document::new(String::new());
        vertical.set_mirrored(true);
        vertical.set_rotation(180.0);
        assert_eq!(red(&vertical), [3.0, 4.0, 5.0, 0.0, 1.0, 2.0], "top to bottom");

        document.set_mirrored(false);
        assert_eq!(red(&document), [0.0, 1.0, 2.0, 3.0, 4.0, 5.0]);
        assert!(document.operations.is_empty(), "no flip and no turn store nothing");
    }

    #[test]
    fn the_matte_searches_from_the_edge_that_was_set() {
        let (w, h) = (512usize, 16usize);
        let border = |alpha: &Alpha| (0..w).find(|x| alpha.data[h / 2 * w + x] < 0.5).unwrap_or(w);
        let wide = Alpha::new(w, h, (0..w * h).map(|i| if i % w < w / 2 { 1.0 } else { 0.0 }).collect());

        let mut mask = Mask::new(Shape::Painted);
        mask.matte = true;
        mask.feather = 0.0;
        mask.shift = -60.0;
        mask.matte_edge = -60.0;
        let mut shown = w;
        let (answer, from_matte) = search_edge(&mask, wide.clone(), None, |coarse| {
            shown = border(coarse);
            Some(coarse.clone())
        });
        assert!(shown < w / 2 - 4, "the model saw the edge pulled in: {shown}");
        assert!(from_matte, "and its answer is a matte, so the border is not decided again");

        mask.matted = from_matte;
        mask.unshaped = Pixels::of(&answer);
        assert!(mask.reshape_edge(w, h));
        let kept = border(&mask.map.0.as_ref().unwrap().to_alpha());
        assert!(kept.abs_diff(shown) <= 1, "and it was not pulled in twice: {kept} against {shown}");

        mask.matte_edge = 0.0;
        search_edge(&mask, wide.clone(), None, |coarse| {
            shown = border(coarse);
            None
        });
        assert_eq!(shown, w / 2, "an old refined mask is searched from where it was found");
    }

    const LANDSCAPE: [f32; 2] = [6000.0, 4000.0];

    #[test]
    fn a_steep_curve_fills_every_code_it_covers() {

        let curve = Curve::new([[0.0, 0.0], [0.15, 0.55], [0.5, 0.85], [1.0, 1.0]]);

        let count = 4096;
        let data: Vec<f32> = (0..count)
            .flat_map(|index| {
                let value = index as f32 / (count - 1) as f32 * 0.25;
                [value, value, value]
            })
            .collect();

        let shown =
            encode_srgb(count as u32, 1, &data, std::slice::from_ref(&curve), ColourSpace::Srgb, ColourSpace::Srgb);
        let codes: std::collections::BTreeSet<u8> =
            shown.as_raw().iter().step_by(3).copied().collect();

        let (lowest, highest) = (*codes.iter().next().unwrap(), *codes.iter().last().unwrap());
        let covered = highest as usize - lowest as usize + 1;
        assert!(
            codes.len() * 20 >= covered * 19,
            "{} of the {covered} codes between {lowest} and {highest} are used",
            codes.len(),
        );
    }

    fn grey(value: f32) -> LinearImage {
        LinearImage::new(1, 1, vec![value; 3])
    }

    fn rgb(r: f32, g: f32, b: f32) -> LinearImage {
        LinearImage::new(1, 1, vec![r, g, b])
    }

    fn with(basic: Basic) -> Document {
        let mut document = Document::new("x".into());
        document.set_basic(basic);
        document
    }

    #[test]
    #[ignore]
    fn detail_passes_on_real_frames() {
        let Ok(dir) = std::env::var("RAF_DIR") else {
            println!("set RAF_DIR to run this");
            return;
        };
        let mut paths: Vec<_> = std::fs::read_dir(&dir)
            .unwrap()
            .flatten()
            .map(|entry| entry.path())
            .filter(|path| numa_io::raw::is_supported(path))
            .collect();
        paths.sort();
        let step = (paths.len() / 6).max(1);
        paths = paths.into_iter().step_by(step).take(6).collect();

        println!("{:<16} {:>10} {:>9} {:>10} {:>9}", "frame", "defringe", "worst", "moire", "worst");
        for path in paths {
            let Ok(linear) = numa_io::raw::decode_linear(&path) else { continue };
            let document = Document::new(path.display().to_string());
            let working = to_working_space(&document, &linear, &Default::default());
            let plain = apply_stack(&document, &working, 1.0);

            let changed = |other: &RgbImage| {
                let (mut count, mut worst) = (0usize, 0u8);
                for (a, b) in plain.as_raw().iter().zip(other.as_raw()) {
                    let apart = a.abs_diff(*b);
                    if apart > 0 {
                        count += 1;
                    }
                    worst = worst.max(apart);
                }
                (count as f64 / plain.as_raw().len() as f64 * 100.0, worst)
            };

            let mut fringed = document.clone();
            fringed.set_basic(Basic::with(|b| b.detail.defringe = 100.0));
            let (fringe_share, fringe_worst) = changed(&apply_stack(&fringed, &working, 1.0));

            let mut moired = document.clone();
            moired.set_basic(Basic::with(|b| b.detail.moire = 100.0));
            let (moire_share, moire_worst) = changed(&apply_stack(&moired, &working, 1.0));

            println!(
                "{:<16} {:>9.2}% {:>9} {:>9.2}% {:>9}",
                path.file_name().unwrap().to_string_lossy(),
                fringe_share,
                fringe_worst,
                moire_share,
                moire_worst,
            );
        }
    }

    #[test]
    fn a_channel_curve_touches_only_its_channel() {
        let data = vec![0.18f32; 3];
        let lift = Curve::new([[0.0, 0.0], [0.5, 0.8], [1.0, 1.0]]);
        let plain = encode_srgb(1, 1, &data, &[], ColourSpace::Srgb, ColourSpace::Srgb);
        let red = encode_srgb(1, 1, &data, &[Curve::identity(), lift], ColourSpace::Srgb, ColourSpace::Srgb);
        assert!(red.as_raw()[0] > plain.as_raw()[0]);
        assert_eq!(red.as_raw()[1..], plain.as_raw()[1..]);
    }

    #[test]
    fn the_default_space_changes_nothing_at_all() {
        use numa_core::space::ColourSpace;

        let count = 300usize;
        let data: Vec<f32> = (0..count * 3).map(|i| (i % 97) as f32 / 96.0 * 2.0).collect();
        let curve = Curve::new([[0.0, 0.0], [0.4, 0.55], [1.0, 1.0]]);

        let plain = encode_srgb(count as u32, 1, &data, std::slice::from_ref(&curve), ColourSpace::Srgb, ColourSpace::Srgb);

        let identity = encode_srgb(
            count as u32,
            1,
            &data,
            std::slice::from_ref(&curve),
            ColourSpace::Srgb,
            ColourSpace::Srgb,
        );
        assert_eq!(plain.as_raw(), identity.as_raw());

        let greens: Vec<f32> = vec![0.05, 0.85, 0.08, 0.02, 0.5, 0.05, 0.6, 0.9, 0.1];
        let here = encode_srgb(3, 1, &greens, &[], ColourSpace::Srgb, ColourSpace::Srgb);
        for space in [ColourSpace::AdobeRgb, ColourSpace::DisplayP3, ColourSpace::ProPhoto] {
            let there =
                encode_srgb(3, 1, &greens, &[], ColourSpace::Srgb, space);
            let moved = here
                .as_raw()
                .iter()
                .zip(there.as_raw())
                .map(|(a, b)| a.abs_diff(*b) as u32)
                .max()
                .unwrap_or(0);
            assert!(moved > 8, "{} writes a green differently: {moved}", space.name());
        }
    }

    fn plain() -> Document {
        Document::new("x".into())
    }

    #[test]
    fn a_guided_tile_matches_the_whole_frame() {

        let (width, height) = (800u32, 600u32);
        let mut data = Vec::with_capacity((width * height * 3) as usize);
        for y in 0..height {
            for x in 0..width {
                let sky = 0.5 + 0.4 * (x as f32 / width as f32);
                let weave = 0.03 * (((x / 3) + (y / 3)) % 2) as f32;
                let value = if y > 260 && x > 180 && x < 620 { 0.04 + weave } else { sky };
                data.extend([value * 0.9, value, value * 1.1]);
            }
        }
        let full = LinearImage::new(width, height, data);

        for (hdr, clarity, texture) in [(40.0, 0.0, 0.0), (0.0, 60.0, 0.0), (0.0, -50.0, 0.0), (0.0, 0.0, 70.0), (30.0, 40.0, 50.0)] {
            let mut document = plain();
            let mut basic = document.basic();
            basic.presence.hdr = hdr;
            basic.presence.clarity = clarity;
            basic.presence.texture = texture;
            document.set_basic(basic);

            let (whole, measured) = apply_stack_recording(&document, &full, 1.0);
            let from_frame = measured.expect("something was measured");
            let quickly = measure_tone(&document, &full).expect("measured quickly");
            let (_, proxy_measured) = apply_stack_recording(&document, full.downscaled(400).unwrap(), 0.5);
            let from_proxy = proxy_measured.expect("the proxy was measured");

            let (x, y, w, h) = (240u32, 180u32, 240u32, 180u32);
            let region = [x as f32 / width as f32, y as f32 / height as f32, w as f32 / width as f32, h as f32 / height as f32];
            let tile = full.cropped(region, 0.0, Default::default());

            let compare = |rendered: &RgbImage| -> (u8, f32) {
                let (mut worst, mut sum) = (0u8, 0.0f32);
                for row in 0..h {
                    for column in 0..w {
                        let (a, b) = (rendered.get_pixel(column, row), whole.get_pixel(x + column, y + row));
                        for channel in 0..3 {
                            let off = a[channel].abs_diff(b[channel]);
                            worst = worst.max(off);
                            sum += off as f32;
                        }
                    }
                }
                (worst, sum / (w * h * 3) as f32)
            };
            let own = compare(&apply_pixels(&document, &tile, 1.0, region));
            let same = compare(&apply_pixels_guided(&document, &tile, 1.0, region, &from_frame));
            let quick = compare(&apply_pixels_guided(&document, &tile, 1.0, region, &quickly));
            let proxy = compare(&apply_pixels_guided(&document, &tile, 1.0, region, &from_proxy));
            println!(
                "hdr {hdr} clarity {clarity} texture {texture}: tile alone {own:?}, measured on the frame {same:?}, measured quickly {quick:?}, on the proxy {proxy:?}"
            );

            assert!(same.1 < 0.05, "the frame's own measurement gives its pixels: {same:?}");

            assert!(quick.1 < 0.05, "the quick measurement is as good: {quick:?}");

            assert!(proxy.1 < 1.5, "the proxy's is within a level on average: {proxy:?}");

        }
    }

    #[test]
    fn a_turned_tile_is_the_whole_frame_cut() {

        let (width, height) = (120u32, 80u32);
        let mut data = Vec::with_capacity((width * height * 3) as usize);
        for y in 0..height {
            for x in 0..width {
                let (u, v) = (x as f32 / width as f32, y as f32 / height as f32);
                data.extend([0.1 + 0.6 * u, 0.1 + 0.5 * v, 0.2 + 0.3 * u * v]);
            }
        }
        let full = LinearImage::new(width, height, data);

        for rotation in [0.0, 90.0, 180.0, 270.0] {
            for mirrored in [false, true] {
                for angle in [0.0, 3.0] {
                    let mut document = plain();
                    document.set_rotation(rotation);
                    document.set_mirrored(mirrored);
                    document.set_crop([0.1, 0.15, 0.8, 0.7], angle);
                    let whole = geometry_of(&document, &full).expect("there is geometry");

                    let (x, y, w, h) = (7u32, 5u32, 20u32, 14u32);
                    let (fw, fh) = (whole.width as f32, whole.height as f32);
                    let region = [x as f32 / fw, y as f32 / fh, w as f32 / fw, h as f32 / fh];
                    let tile = cut_turned_tile(&document, &full, region).expect("a tile");
                    assert_eq!((tile.width, tile.height), (w, h), "{rotation}° {mirrored} {angle}°");

                    let mut worst = 0.0f32;
                    for row in 0..h {
                        for column in 0..w {
                            for channel in 0..3 {
                                let got = tile.data[((row * w + column) * 3 + channel) as usize];
                                let want = whole.data[(((y + row) * whole.width + x + column) * 3 + channel) as usize];
                                worst = worst.max((got - want).abs());
                            }
                        }
                    }
                    assert!(worst < 1e-4, "{rotation}° mirrored {mirrored} at {angle}°: off by {worst}");
                }
            }
        }
    }

    #[test]
    fn a_wide_export_keeps_what_srgb_cannot_hold() {
        use numa_core::space::ColourSpace;

        let to_srgb = ColourSpace::DisplayP3.convert_to(ColourSpace::Srgb).unwrap();
        let green = to_srgb.map(|row| row[1] * 0.3);
        let frame = rgb(green[0], green[1], green[2]);

        let mut wide = plain();
        wide.set_output_space(ColourSpace::DisplayP3);
        assert_eq!(wide.working_space, ColourSpace::DisplayP3);
        let mut clipped = plain();
        clipped.output_space = ColourSpace::DisplayP3;
        let kept = develop16(&wide, &frame, &Default::default()).get_pixel(0, 0).0;
        let lost = develop16(&clipped, &frame, &Default::default()).get_pixel(0, 0).0;
        assert!(kept[0] + 3000 < lost[0] && kept[2] + 3000 < lost[2], "no purer a green: {kept:?} against {lost:?}");

        let mut stored = plain();
        stored.working_space = ColourSpace::AdobeRgb;
        stored.set_output_space(ColourSpace::DisplayP3);
        assert_eq!(stored.working_space, ColourSpace::AdobeRgb);
    }

    #[test]
    fn the_mixer_does_the_same_in_a_wider_space() {
        use numa_core::mixer::Mixer;
        use numa_core::space::ColourSpace;

        let mut mixer = Mixer::default();

        mixer.bands[3] = [60.0, -30.0, -20.0];
        mixer.bands[5] = [-60.0, -30.0, 20.0];
        let table = || Table::resolve(&mixer.table(), 0.0);
        let to_p3 = ColourSpace::Srgb.convert_to(ColourSpace::DisplayP3).unwrap();
        let turn = |rgb: [f32; 3]| to_p3.map(|row| row[0] * rgb[0] + row[1] * rgb[1] + row[2] * rgb[2]);
        for srgb in [[0.08, 0.3, 0.06], [0.06, 0.1, 0.35], [0.1, 0.25, 0.2]] {
            let expected = turn(Look::new(table()).apply(srgb));
            let got = Look::new(table()).in_space(ColourSpace::DisplayP3).apply(turn(srgb));
            for (a, b) in got.iter().zip(expected) {
                assert!((a - b).abs() < 2e-3, "{srgb:?}: {got:?} in P3, {expected:?} by way of sRGB");
            }
        }
    }

    #[test]
    fn texture_and_faces_do_not_tile() {
        assert!(tiles_cleanly(&plain()));
        let mut textured = plain();
        textured.set_basic(Basic::with(|b| b.presence.texture = 20.0));
        assert!(!tiles_cleanly(&textured));
        let mut portrait = plain();
        portrait.set_beautify(numa_core::beautify::Beautify { skin: 40.0, ..Default::default() });
        assert!(!tiles_cleanly(&portrait));
    }

    #[test]
    fn a_spot_near_the_top_of_a_landscape_frame_needs_more_room_than_its_radius() {
        use numa_core::retouch::{Retouch, Spot};
        let mut document = plain();
        document.set_retouch(Retouch {
            spots: vec![Spot { at: [0.5, 0.062], from: [0.5, 0.5], radius: 0.01, ..Default::default() }],
        });

        assert!(!spots_within(&document, [0.0, 0.05, 1.0, 0.9], LANDSCAPE));
        assert!(spots_within(&document, WHOLE_FRAME, LANDSCAPE));
    }

    #[test]
    fn a_clicked_mask_asks_for_both_models() {
        use numa_core::mask::{Mask, RegionPoint, Shape};

        let clicked = {
            let mut mask = Mask::new(Shape::Painted);
            mask.points.push(RegionPoint { at: [0.5, 0.5], subtract: false, enabled: true });
            mask
        };
        assert_eq!(models_needed(&[clicked]), (true, true), "a click needs both");

        let named = Mask::new(Shape::Segment { classes: vec![2] });
        assert_eq!(models_needed(&[named]), (true, false), "a named thing needs the semantic one");

        let gradient = Mask::new(Shape::Linear { from: [0.0, 0.0], to: [1.0, 1.0] });
        assert_eq!(models_needed(&[gradient]), (false, false), "a gradient needs neither");

        assert_eq!(models_needed(&[]), (false, false));
    }

    #[test]
    fn a_grade_reaches_the_rendered_pixels() {
        use numa_core::grading::{Grading, Range};

        let dark = rgb(0.02, 0.02, 0.02);
        let bright = rgb(1.5, 1.5, 1.5);

        let mut document = plain();
        document.set_grading(Grading {
            shadows: Range { hue: 240.0, saturation: 80.0, luminance: 0.0 },
            highlights: Range { hue: 40.0, saturation: 80.0, luminance: 0.0 },
            ..Default::default()
        });

        let shadow = develop(&document, &dark, &Default::default());
        let shadow = shadow.get_pixel(0, 0).0;
        assert!(shadow[2] > shadow[0], "the shadows should render blue: {shadow:?}");

        let highlight = develop(&document, &bright, &Default::default());
        let highlight = highlight.get_pixel(0, 0).0;
        assert!(
            highlight[0] >= highlight[2],
            "the highlights should render warm: {highlight:?}"
        );

        let plain = develop(&plain(), &dark, &Default::default()).get_pixel(0, 0).0;
        assert_ne!(plain, shadow);
    }

    #[test]
    fn a_stored_mask_gets_its_pixels_before_it_is_rendered() {
        use numa_core::mask::{Mask, Shape, Stroke};

        let grey = LinearImage::new(32, 32, vec![0.5; 32 * 32 * 3]);
        let mut painted = Mask::new(Shape::Painted);
        painted.basic = Basic::with(|b| b.tone.exposure = -4.0);
        let mut stroke = Stroke::lasso(false);
        stroke.points = vec![[0.1, 0.1], [0.9, 0.1], [0.9, 0.9], [0.1, 0.9]];
        painted.strokes.push(stroke);

        let mut document = Document::new("x".into());
        document.set_masks(vec![painted]);
        assert!(document.masks()[0].is_pending(), "the fixture is already resolved");

        let resolved = with_masks_resolved(&document, &grey);
        assert!(!resolved.masks()[0].is_pending(), "the mask was left without pixels");

        let before = apply_stack(&document, &grey, 1.0);
        let after = apply_stack(&resolved, &grey, 1.0);
        assert_eq!(before.get_pixel(16, 16)[0], 197, "the fixture is not what it was");
        assert!(
            after.get_pixel(16, 16)[0] < 100,
            "the lasso did nothing: {:?}",
            after.get_pixel(16, 16)
        );

        assert_eq!(after.get_pixel(0, 0)[0], 197);

        let plain = Document::new("x".into());
        assert!(with_masks_resolved(&plain, &grey).masks().is_empty());
    }

    #[test]
    fn a_blown_highlight_renders_neutral_and_a_red_one_stays_red() {
        const CLIP: f32 = 2.4;
        let multipliers = [1.79, 1.0, 1.89];
        let lowest = 1.0;
        let balance = |camera: [f32; 3]| {
            [
                camera[0] * multipliers[0],
                camera[1] * multipliers[1],
                camera[2] * multipliers[2],
            ]
        };

        let blown = [CLIP, CLIP, CLIP];
        let out = neutralise_clipping(balance(blown), &blown, Some(CLIP), lowest);
        assert!(
            (out[0] - out[1]).abs() < 1e-3 && (out[1] - out[2]).abs() < 1e-3,
            "a blown white should stay white, got {out:?}"
        );

        let mostly = [CLIP * 0.8, CLIP * 0.95, CLIP];
        let spread = |pixel: [f32; 3]| {
            pixel[0].max(pixel[1]).max(pixel[2]) / pixel[0].min(pixel[1]).min(pixel[2])
        };
        let out = neutralise_clipping(balance(mostly), &mostly, Some(CLIP), lowest);
        assert!(spread(balance(mostly)) > 1.9, "the test case is not coloured to begin with");
        assert!(spread(out) < 1.15, "a blown sky kept its cast: {out:?}");

        let red = [CLIP, 0.2, 0.15];
        let out = neutralise_clipping(balance(red), &red, Some(CLIP), lowest);
        assert!(out[0] > out[1] * 3.0 && out[0] > out[2] * 3.0, "{out:?}");

        let ordinary = [0.5, 0.6, 0.55];
        assert_eq!(
            neutralise_clipping(balance(ordinary), &ordinary, Some(CLIP), lowest),
            balance(ordinary)
        );
    }

    #[test]
    fn the_document_decides_which_camera_profile_renders_it() {
        use numa_core::profile::DngProfile;

        let matched = std::sync::Arc::new(DngProfile {
            name: "Matched".into(),
            camera: None,
            color_matrix: [None, None],
            forward_matrix: [None, None],
            illuminant: [None, None],
            hue_sat_map: None,
            look_table: None,
            tone_curve: None,
        });
        let source = grey(0.5).with_rendering(Some(matched));

        let name = |document: &Document| {
            profile_for(document, &source, &Default::default()).map(|profile| profile.name.clone())
        };

        assert_eq!(name(&plain()).as_deref(), Some("Matched"));

        let mut none = plain();
        none.colour_profile = Some(NO_COLOUR_PROFILE.to_string());
        assert_eq!(name(&none), None);

        let mut missing = plain();
        missing.colour_profile = Some("Gone".to_string());
        assert_eq!(name(&missing).as_deref(), Some("Matched"));

        let chosen = std::sync::Arc::new(DngProfile {
            name: "Chosen".into(),
            camera: None,
            color_matrix: [None, None],
            forward_matrix: [None, None],
            illuminant: [None, None],
            hue_sat_map: None,
            look_table: None,
            tone_curve: None,
        });
        let mut named = plain();
        named.colour_profile = Some("Chosen".to_string());
        let handed = RenderInputs { profile: Some(chosen), ..Default::default() };
        assert_eq!(
            profile_for(&named, &source, &handed).map(|profile| profile.name.clone()).as_deref(),
            Some("Chosen"),
        );
    }

    #[test]
    fn a_render_handed_nothing_is_the_undenoised_one() {
        let source = grey(0.5);
        let mut asked = plain();
        asked.ai_denoise = 100.0;
        assert_eq!(
            develop(&asked, &source, &Default::default()).into_raw(),
            develop(&plain(), &source, &Default::default()).into_raw(),
            "a document asking for denoise, handed none, must render the plain picture",
        );
    }

    #[test]
    fn a_mask_darkens_its_own_half_of_the_frame() {
        use numa_core::mask::{Mask, Shape};

        let flat = LinearImage::new(9, 1, vec![MIDDLE_GREY; 9 * 3]);
        let mut mask = Mask::new(Shape::Linear { from: [0.0, 0.5], to: [1.0, 0.5] });
        mask.basic.tone.exposure = -2.0;

        let mut document = plain();
        document.set_masks(vec![mask]);
        let rendered = develop(&document, &flat, &Default::default());

        let left = rendered.get_pixel(0, 0)[0];
        let right = rendered.get_pixel(8, 0)[0];
        let untouched = develop(&plain(), &flat, &Default::default()).get_pixel(0, 0)[0];

        assert!(left > untouched - 6, "the far end of the ramp was darkened: {left}");
        assert!(right < untouched / 2, "the near end was not: {right}");

        let middle = rendered.get_pixel(4, 0)[0];
        assert!(middle < left && middle > right, "{left} / {middle} / {right}");
    }

    #[test]
    fn a_mask_that_measures_the_frame_gets_all_of_it() {
        let (width, height) = (4usize, 400usize);
        let mut field = vec![0.0f32; width * height];
        field[300 * width..310 * width].fill(1.0);

        let plain = Basic::local();
        assert_eq!(rows_to_work(&plain, &field, width, height), Some((236, 373)), "its rows and the margin");

        for pass in ["hdr", "clarity", "texture", "dehaze"] {
            let mut basic = Basic::local();
            match pass {
                "hdr" => basic.presence.hdr = 30.0,
                "clarity" => basic.presence.clarity = 30.0,
                "texture" => basic.presence.texture = 30.0,
                _ => basic.effects.dehaze = 30.0,
            }
            assert_eq!(rows_to_work(&basic, &field, width, height), Some((0, height - 1)), "{pass}");
        }
        assert_eq!(rows_to_work(&plain, &vec![0.0; width * height], width, height), None, "nothing covered");
    }

    #[test]
    fn a_mask_on_a_few_rows_leaves_the_rest_alone() {
        use numa_core::mask::{Mask, Shape};

        let flat = LinearImage::new(8, 400, vec![MIDDLE_GREY; 8 * 400 * 3]);

        let mut mask = Mask::new(Shape::Radial { centre: [0.5, 0.9], radius: [0.2, 0.05], feather: 0.0 });
        mask.basic.tone.exposure = -3.0;

        let mut document = plain();
        document.set_masks(vec![mask]);
        let rendered = develop(&document, &flat, &Default::default());
        let untouched = develop(&plain(), &flat, &Default::default());

        assert!(
            rendered.get_pixel(4, 360)[0] < untouched.get_pixel(4, 360)[0] / 2,
            "inside the circle is darkened"
        );
        for y in [0u32, 100, 200] {
            assert_eq!(
                rendered.get_pixel(4, y)[0],
                untouched.get_pixel(4, y)[0],
                "row {y} is outside the mask and untouched"
            );
        }
    }

    #[test]
    fn a_feathered_radial_has_no_ring() {
        use numa_core::mask::{Mask, Shape};

        let flat = LinearImage::new(400, 400, vec![MIDDLE_GREY; 400 * 400 * 3]);
        let mut mask = Mask::new(Shape::Radial { centre: [0.5, 0.5], radius: [0.3, 0.3], feather: 0.5 });
        mask.basic.tone.exposure = -2.0;
        let mut document = plain();
        let mut basic = document.basic();
        basic.presence.hdr = 50.0;
        basic.presence.clarity = 30.0;
        document.set_basic(basic);
        document.set_masks(vec![mask]);
        let out = develop(&document, &flat, &Default::default());

        let row: Vec<u8> = (200..400).map(|x| out.get_pixel(x, 200)[0]).collect();
        assert!(row[0] < row[199] / 2, "the centre is darkened: {row:?}");
        for pair in row.windows(2) {
            assert!(pair[1] >= pair[0], "brightness falls going outwards: {row:?}");
        }
    }

    #[test]
    fn a_mask_lands_in_the_same_place_on_a_tile() {
        use numa_core::mask::{Mask, Shape};

        let mut mask = Mask::new(Shape::Linear { from: [0.0, 0.5], to: [1.0, 0.5] });
        mask.basic.tone.exposure = -2.0;
        let mut document = plain();
        document.set_masks(vec![mask]);

        let whole = LinearImage::new(8, 1, vec![MIDDLE_GREY; 8 * 3]);
        let half = LinearImage::new(4, 1, vec![MIDDLE_GREY; 4 * 3]);

        let full = apply_pixels(&document, &whole, 1.0, WHOLE_FRAME);
        let tile = apply_pixels(&document, &half, 1.0, [0.5, 0.0, 0.5, 1.0]);

        for x in 0..4 {
            let from_full = full.get_pixel(x + 4, 0)[0] as i32;
            let from_tile = tile.get_pixel(x, 0)[0] as i32;
            assert!(
                (from_full - from_tile).abs() <= 2,
                "column {x}: whole frame {from_full}, tile {from_tile}"
            );
        }
    }

    #[test]
    fn a_mask_carrying_the_frame_local_family_refuses_to_tile() {
        use numa_core::mask::{Mask, Shape};

        for (name, set) in [
            ("dehaze", (|basic: &mut Basic| basic.effects.dehaze = 10.0) as fn(&mut Basic)),
            ("clarity", |basic: &mut Basic| basic.presence.clarity = 20.0),
            ("texture", |basic: &mut Basic| basic.presence.texture = 20.0),
            ("hdr", |basic: &mut Basic| basic.presence.hdr = 20.0),
        ] {
            let mut mask = Mask::new(Shape::Linear { from: [0.0, 0.5], to: [1.0, 0.5] });
            set(&mut mask.basic);
            let mut document = plain();
            assert!(tiles_cleanly(&document), "a plain document tiles");
            document.set_masks(vec![mask]);
            assert!(
                !tiles_cleanly(&document),
                "a mask carrying {name} must send the renderer to the whole frame"
            );
        }

        let mut mask = Mask::new(Shape::Linear { from: [0.0, 0.5], to: [1.0, 0.5] });
        mask.basic.tone.exposure = -2.0;
        mask.basic.presence.saturation = 30.0;
        let mut document = plain();
        document.set_masks(vec![mask]);
        assert!(tiles_cleanly(&document), "exposure and saturation in a mask still tile");
    }

    #[test]
    fn the_roadmap_example_matches_between_preview_and_export() {
        use numa_core::mask::{Mask, Shape};

        let (w, h) = (48usize, 32usize);
        let data: Vec<f32> = (0..w * h)
            .flat_map(|index| {
                let scene = if (index / w) < h / 2 { 0.5 } else { 0.15 };
                let hazy = scene * 0.6 + 0.5 * 0.4;
                [hazy * 0.95, hazy, hazy * 1.1]
            })
            .collect();
        let mut frame = LinearImage::new(w as u32, h as u32, data);
        frame.white_point = Some(numa_core::color::WhiteBalance { temperature: 5500.0, tint: 0.0 });

        let mut sky = Mask::new(Shape::Linear { from: [0.5, 0.48], to: [0.5, 0.52] });
        sky.basic.effects.dehaze = 10.0;
        sky.basic.balance.temperature = -5.0;
        let mut document = plain();
        document.set_masks(vec![sky]);

        assert!(!tiles_cleanly(&document), "dehaze in a mask sends this to the whole frame");

        let export = apply_pixels(&document, &frame, 1.0, WHOLE_FRAME);
        let preview = develop(&document, &frame, &Default::default());
        for y in 0..h as u32 {
            for x in 0..w as u32 {
                let (a, b) = (export.get_pixel(x, y), preview.get_pixel(x, y));
                assert_eq!(a, b, "preview and export disagree at {x},{y}");
            }
        }

        let plain_render = apply_pixels(&plain(), &frame, 1.0, WHOLE_FRAME);
        let moved = (0..h as u32)
            .flat_map(|y| (0..w as u32).map(move |x| (x, y)))
            .filter(|(x, y)| plain_render.get_pixel(*x, *y) != export.get_pixel(*x, *y))
            .count();
        assert!(moved > w * h / 4, "the sky mask changed {moved} of {} pixels", w * h);
    }

    #[test]
    fn a_mask_carries_dehaze_over_its_own_half() {
        use numa_core::mask::{Mask, Shape};

        let (w, h) = (64usize, 64usize);
        let data: Vec<f32> = (0..w * h)
            .flat_map(|index| {
                let scene = if (index % w) < w / 2 { 0.1 } else { 0.5 };
                let hazy = scene * 0.5 + 0.6 * 0.5;
                [hazy; 3]
            })
            .collect();
        let frame = LinearImage::new(w as u32, h as u32, data);

        let plain_render = apply_pixels(&plain(), &frame, 1.0, WHOLE_FRAME);

        let mut mask = Mask::new(Shape::Linear { from: [0.49, 0.5], to: [0.51, 0.5] });
        mask.basic.effects.dehaze = 80.0;
        let mut document = plain();
        document.set_masks(vec![mask]);
        let masked = apply_pixels(&document, &frame, 1.0, WHOLE_FRAME);

        let at = |image: &RgbImage, x: u32| image.get_pixel(x, (h / 2) as u32)[0] as i32;
        let left = (at(&plain_render, 2) - at(&masked, 2)).abs();
        let right = (at(&plain_render, w as u32 - 3) - at(&masked, w as u32 - 3)).abs();
        assert!(left <= 1, "outside the mask nothing moved: {left}");
        assert!(right > 2, "inside it the dehaze landed: {right}");
    }

    #[test]
    fn a_spot_lands_in_the_same_place_on_a_tile() {
        use numa_core::retouch::{Retouch, Spot};

        let columns = 64;
        let mut data = vec![0.0f32; columns * 3];
        for x in 0..columns {
            let value = if x % 4 == 0 { MIDDLE_GREY } else { MIDDLE_GREY * 4.0 };
            data[x * 3..][..3].copy_from_slice(&[value; 3]);
        }
        let whole = LinearImage::new(columns as u32, 1, data.clone());
        let half = LinearImage::new(columns as u32 / 2, 1, data[columns / 2 * 3..].to_vec());

        let mut document = plain();
        document.set_retouch(Retouch {
            spots: vec![Spot {

                at: [0.75, 0.5],
                from: [0.90, 0.5],
                radius: 0.03,
                feather: 0.0,
                opacity: 1.0,
                heal: false,
                kind: numa_core::retouch::Kind::Patch,
            }],
        });

        let full = apply_pixels(&document, &whole, 1.0, WHOLE_FRAME);
        let tile = apply_pixels(&document, &half, 1.0, [0.5, 0.0, 0.5, 1.0]);

        for x in 4..columns / 2 {
            let from_full = full.get_pixel((x + columns / 2) as u32, 0)[0] as i32;
            let from_tile = tile.get_pixel(x as u32, 0)[0] as i32;
            assert!(
                (from_full - from_tile).abs() <= 2,
                "column {x}: whole frame {from_full}, tile {from_tile}"
            );
        }
    }

    #[test]
    fn a_spot_reaching_outside_a_tile_refuses_it() {
        use numa_core::retouch::{Retouch, Spot};

        let mut document = plain();
        document.set_retouch(Retouch {
            spots: vec![Spot {
                at: [0.75, 0.5],

                from: [0.20, 0.5],
                radius: 0.03,
                ..Default::default()
            }],
        });

        let right = [0.5, 0.0, 0.5, 1.0];
        assert!(!spots_within(&document, right, LANDSCAPE), "the source is not in this tile");
        assert!(spots_within(&document, WHOLE_FRAME, LANDSCAPE), "but it is in the frame");

        document.set_retouch(Retouch {
            spots: vec![Spot {
                at: [0.75, 0.5],
                from: [0.9, 0.5],
                radius: 0.03,
                ..Default::default()
            }],
        });
        assert!(spots_within(&document, right, LANDSCAPE));

        document.set_retouch(Retouch {
            spots: vec![Spot {
                at: [0.75, 0.5],

                from: [0.995, 0.5],
                radius: 0.03,
                ..Default::default()
            }],
        });
        assert!(!spots_within(&document, right, LANDSCAPE), "the patch hangs over the edge");
    }

    #[test]
    fn no_spots_never_refuses_a_tile() {
        assert!(spots_within(&plain(), [0.5, 0.0, 0.25, 0.25], LANDSCAPE));
    }

    #[test]
    fn the_colour_mixer_reaches_the_render() {
        use numa_core::mixer::Mixer;

        let mut mixer = Mixer::default();

        mixer.bands[3][1] = -100.0;
        let mut document = plain();
        document.set_mixer(mixer);

        let spread = |pixel: [u8; 3]| {
            let high = pixel.iter().copied().max().unwrap() as i32;
            let low = pixel.iter().copied().min().unwrap() as i32;
            high - low
        };

        let green = rgb(0.05, 0.4, 0.05);
        let before = develop(&plain(), &green, &Default::default()).get_pixel(0, 0).0;
        let after = develop(&document, &green, &Default::default()).get_pixel(0, 0).0;
        assert!(
            spread(after) < spread(before) / 2,
            "green kept its colour: {before:?} then {after:?}"
        );

        let red = rgb(0.4, 0.05, 0.05);
        let before = develop(&plain(), &red, &Default::default()).get_pixel(0, 0).0;
        let after = develop(&document, &red, &Default::default()).get_pixel(0, 0).0;
        assert!(
            (spread(after) - spread(before)).abs() < 8,
            "red followed the green band: {before:?} then {after:?}"
        );

        let mut clean = plain();
        clean.set_mixer(Mixer::default());
        assert!(clean.operations.is_empty());
    }

    #[test]
    fn black_and_white_is_grey_and_the_mix_and_the_grade_reach_it() {
        use numa_core::grading::{Grading, Range};
        use numa_core::mixer::Mixer;

        let mut document = plain();
        document.set_mixer(Mixer { monochrome: true, ..Mixer::default() });
        let blue = rgb(0.05, 0.1, 0.4);
        let red = rgb(0.4, 0.05, 0.05);
        let grey_of = |document: &Document, frame: &LinearImage| {
            let pixel = develop(document, frame, &Default::default()).get_pixel(0, 0).0;
            assert!(pixel[0] == pixel[1] && pixel[1] == pixel[2], "not grey: {pixel:?}");
            pixel[0]
        };
        let (blue_grey, red_grey) = (grey_of(&document, &blue), grey_of(&document, &red));

        let mut mixer = document.mixer();
        mixer.grey[5] = -100.0;
        document.set_mixer(mixer);
        assert!(grey_of(&document, &blue) + 20 < blue_grey, "Blue −100 left the blue as bright");
        assert_eq!(grey_of(&document, &red), red_grey, "Blue −100 moved the red");

        let mut toned = document.clone();
        toned.set_grading(Grading { global: Range { hue: 40.0, saturation: 60.0, ..Range::default() }, ..Grading::default() });
        let pixel = develop(&toned, &red, &Default::default()).get_pixel(0, 0).0;
        assert!(pixel[0] > pixel[2] + 5, "the grade did not tone the grey: {pixel:?}");
    }

    #[test]
    fn point_colours_reach_the_render() {
        use numa_core::point::{PointColour, PointColours};

        let image = LinearImage::new(2, 1, vec![0.4, 0.05, 0.05, 0.05, 0.05, 0.4]);
        let untouched = develop(&plain(), &image, &Default::default());

        let mut empty = plain();
        empty.set_point_colours(PointColours::default());
        assert!(empty.operations.is_empty());
        assert_eq!(develop(&empty, &image, &Default::default()).as_raw(), untouched.as_raw());

        for space in [ColourSpace::Srgb, ColourSpace::ProPhoto] {
            let mut document = plain();
            document.working_space = space;
            let before = develop(&document, &image, &Default::default());
            document.set_point_colours(PointColours {
                points: vec![PointColour { hue: 100.0, ..PointColour::picked([0.4, 0.05, 0.05]) }],
                highlight: None,
            });
            let after = develop(&document, &image, &Default::default());

            let red = |image: &RgbImage| image.get_pixel(0, 0).0;
            assert!(
                red(&before).iter().zip(red(&after)).any(|(a, b)| (*a as i32 - b as i32).abs() > 10),
                "{space:?}: red did not move: {:?} -> {:?}",
                red(&before),
                red(&after)
            );
            let blue = |image: &RgbImage| image.get_pixel(1, 0).0;
            for (a, b) in blue(&before).iter().zip(blue(&after)) {
                assert!((*a as i32 - b as i32).abs() <= 1, "{space:?}: blue followed red");
            }
        }
    }

    #[test]
    fn the_curve_is_measured_against_the_displayed_value() {

        let mut document = plain();
        document.set_curve(Curve::new([[0.0, 0.0], [0.5, 0.0], [1.0, 1.0]]));

        let just_below = tone::scene_value_for(0.45);
        assert_eq!(red(&document, &grey(just_below)), 0, "should have been crushed");

        assert!(just_below < 0.2, "scene value {just_below} is not display 0.45");

        let white = tone::scene_value_for(0.999);
        assert!(red(&document, &grey(white)) > 250);

        let bright = tone::scene_value_for(0.75);
        assert!(red(&document, &grey(bright)) < red(&plain(), &grey(bright)));
    }

    #[test]
    fn a_straight_curve_changes_nothing_and_is_not_stored() {
        let mut document = plain();
        document.set_curve(Curve::identity());
        assert!(document.operations.is_empty(), "the identity must not be stored");

        document.set_curve(Curve::new([[0.0, 0.0], [0.5, 0.7], [1.0, 1.0]]));
        assert_eq!(document.operations.len(), 1);
        let lifted = red(&document, &grey(tone::scene_value_for(0.5)));
        assert!(lifted > 150, "a lifted midtone should be brighter: {lifted}");

        document.set_curve(Curve::identity());
        assert!(document.operations.is_empty());
        assert_eq!(
            red(&document, &grey(MIDDLE_GREY)),
            red(&plain(), &grey(MIDDLE_GREY))
        );
    }

    #[test]
    fn a_tile_of_a_cropped_frame_comes_from_the_right_place() {

        let mut data = vec![0.0f32; 8 * 8 * 3];
        for index in 0..64 {
            data[index * 3] = index as f32;
        }
        let source = LinearImage::new(8, 8, data);

        let mut document = Document::new("x".into());
        document.set_crop([0.25, 0.25, 0.5, 0.5], 0.0);

        let tile = [0.5, 0.5, 0.5, 0.5];
        let composed = tile_in_source(&document, tile).expect("an unrotated crop composes");

        let long_way = geometry_of(&document, &source).unwrap().cropped(tile, 0.0, Default::default());
        let short_way = source.cropped(composed, 0.0, Default::default());

        assert_eq!((long_way.width, long_way.height), (short_way.width, short_way.height));
        assert_eq!(long_way.data, short_way.data, "the tile came from the wrong place");

        document.set_rotation(90.0);
        assert!(tile_in_source(&document, tile).is_none());

        let mut straightened = Document::new("x".into());
        straightened.set_crop([0.25, 0.25, 0.5, 0.5], 3.0);
        assert!(tile_in_source(&straightened, tile).is_none());

        assert_eq!(tile_in_source(&Document::new("x".into()), tile), Some(tile));
    }

    #[test]
    fn ninety_degrees_turns_the_picture_clockwise() {

        let mut data = vec![0.0f32; 3 * 2 * 3];
        for index in 0..6 {
            data[index * 3] = index as f32;
        }
        let image = LinearImage::new(3, 2, data);
        let at = |turned: &LinearImage, x: u32, y: u32| {
            turned.data[((y * turned.width + x) * 3) as usize] as i32
        };

        let mut document = Document::new("x".into());
        document.set_rotation(90.0);
        let turned = geometry_of(&document, &image).expect("a turn is a change");
        assert_eq!((turned.width, turned.height), (2, 3));

        assert_eq!(
            [at(&turned, 0, 0), at(&turned, 1, 0), at(&turned, 0, 2)],
            [3, 0, 5],
            "+90 must turn the photograph right"
        );

        document.set_rotation(270.0);
        let turned = geometry_of(&document, &image).expect("a turn is a change");
        assert_eq!(
            [at(&turned, 0, 0), at(&turned, 1, 0), at(&turned, 0, 2)],
            [2, 5, 0],
            "-90 must turn it left"
        );

        document.set_rotation(180.0);
        let turned = geometry_of(&document, &image).expect("a turn is a change");
        assert_eq!((turned.width, turned.height), (3, 2));
        assert_eq!(at(&turned, 0, 0), 5, "half a turn puts the last pixel first");
    }

    fn red(document: &Document, image: &LinearImage) -> u8 {
        develop(document, image, &Default::default()).get_pixel(0, 0)[0]
    }

    #[test]
    fn base_curve_pins_middle_grey_and_leaves_headroom() {

        assert_eq!(red(&plain(), &grey(MIDDLE_GREY)), 118);
        assert_eq!(red(&plain(), &grey(0.0)), 0, "black stays black");

        let sensor_white = red(&plain(), &grey(1.0));
        assert!(
            (200..255).contains(&sensor_white),
            "sensor white should read bright but leave headroom, got {sensor_white}"
        );

        assert_eq!(red(&plain(), &grey(64.0)), 255);

        let mut previous = 0;
        for step in 0..40 {
            let value = red(&plain(), &grey(0.01 * 1.25f32.powi(step)));
            assert!(value >= previous, "curve is not monotonic at step {step}");
            previous = value;
        }
    }

    #[test]
    fn one_stop_of_exposure_doubles_the_light() {
        let up = with(Basic::with(|b| b.tone.exposure = 1.0));
        assert_eq!(red(&up, &grey(0.18)), red(&plain(), &grey(0.36)));

        let down = with(Basic::with(|b| b.tone.exposure = -1.0));
        assert_eq!(red(&down, &grey(0.36)), red(&plain(), &grey(0.18)));
    }

    #[test]
    fn contrast_pivots_around_middle_grey() {
        let punchy = with(Basic::with(|b| b.tone.contrast = 50.0));

        assert_eq!(red(&punchy, &grey(MIDDLE_GREY)), 118, "the pivot must not move");
        assert!(red(&punchy, &grey(0.05)) < red(&plain(), &grey(0.05)));
        assert!(red(&punchy, &grey(0.5)) > red(&plain(), &grey(0.5)));
    }

    #[test]
    fn tone_sliders_stay_in_their_own_region() {
        let shadow = grey(0.02);
        let mid = grey(MIDDLE_GREY);
        let highlight = grey(0.75);

        let lifted = with(Basic::with(|b| b.tone.shadows = 100.0));
        assert!(red(&lifted, &shadow) > red(&plain(), &shadow), "shadows must lift");
        assert_eq!(red(&lifted, &highlight), red(&plain(), &highlight), "and leave highlights alone");

        let recovered = with(Basic::with(|b| b.tone.highlights = -100.0));
        assert!(red(&recovered, &highlight) < red(&plain(), &highlight), "highlights must recover");
        assert_eq!(red(&recovered, &shadow), red(&plain(), &shadow), "and leave shadows alone");

        for basic in [
            Basic::with(|b| b.tone.blacks = 100.0),
            Basic::with(|b| b.tone.whites = 100.0),
            Basic::with(|b| b.tone.shadows = 100.0),
            Basic::with(|b| b.tone.highlights = -100.0),
        ] {
            assert_eq!(red(&with(basic), &mid), 118, "middle grey moved: {basic:?}");
        }
    }

    #[test]
    fn saturation_moves_colour_but_not_grey() {
        let colour = rgb(0.4, 0.2, 0.1);

        let saturated = with(Basic::with(|b| b.presence.saturation = 50.0));
        let before = develop(&plain(), &colour, &Default::default());
        let after = develop(&saturated, &colour, &Default::default());
        let spread = |p: &image::Rgb<u8>| p[0] as i32 - p[2] as i32;
        assert!(spread(after.get_pixel(0, 0)) > spread(before.get_pixel(0, 0)));

        assert_eq!(red(&saturated, &grey(0.4)), red(&plain(), &grey(0.4)));

        let grey_out = develop(
            &with(Basic::with(|b| b.presence.saturation = -100.0)),
            &colour,
            &Default::default(),
        );
        let pixel = grey_out.get_pixel(0, 0);
        assert_eq!(pixel[0], pixel[1]);
        assert_eq!(pixel[1], pixel[2]);
    }

    #[test]
    fn vibrance_spares_already_saturated_colour() {
        let muted = rgb(0.30, 0.26, 0.24);
        let vivid = rgb(0.40, 0.02, 0.01);
        let vibrant = with(Basic::with(|b| b.presence.vibrance = 100.0));
        let spread = |image: &RgbImage| {
            let p = image.get_pixel(0, 0);
            p[0] as i32 - p[2] as i32
        };

        let muted_gain = spread(&develop(&vibrant, &muted, &Default::default())) - spread(&develop(&plain(), &muted, &Default::default()));
        let vivid_gain = spread(&develop(&vibrant, &vivid, &Default::default())) - spread(&develop(&plain(), &vivid, &Default::default()));
        assert!(
            muted_gain > vivid_gain,
            "vibrance must push muted colour harder: muted {muted_gain}, vivid {vivid_gain}"
        );
    }

    #[test]
    fn blown_highlights_desaturate_instead_of_shifting_hue() {

        let clipped = develop(&plain(), &rgb(4.0, 1.4, 1.1), &Default::default());
        let pixel = clipped.get_pixel(0, 0);
        let spread = pixel[0] as i32 - pixel[2] as i32;
        assert!(spread < 40, "blown highlight kept a {spread}/255 colour cast: {pixel:?}");
        assert!(pixel[0] > 200, "and it should still read as a highlight");

        let below = develop(&plain(), &rgb(0.4, 0.14, 0.11), &Default::default());
        let below_spread = below.get_pixel(0, 0)[0] as i32 - below.get_pixel(0, 0)[2] as i32;
        assert!(below_spread > spread, "a mid-tone must keep more colour than a blown one");
    }

    #[test]
    fn highlight_recovery_reaches_above_white() {

        let blown = grey(64.0);
        assert_eq!(red(&plain(), &blown), 255, "starts at display white");

        let recovered = with(Basic::with(|b| b.tone.highlights = -100.0));
        assert!(
            red(&recovered, &blown) < 255,
            "highlight recovery found nothing above white to pull back"
        );

        let bright = grey(2.0);
        assert!(red(&recovered, &bright) < red(&plain(), &bright));
    }

    #[test]
    fn negative_values_and_blowouts_stay_in_range() {
        assert_eq!(red(&with(Basic::with(|b| b.tone.exposure = -20.0)), &grey(1.0)), 0);
        assert_eq!(red(&with(Basic::with(|b| b.tone.exposure = 20.0)), &grey(1.0)), 255);
    }
}
