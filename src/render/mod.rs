pub mod ai_denoise;
pub mod detail;
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
pub mod retouch;
pub mod sam;
pub mod segment;

use image::RgbImage;
use std::sync::Arc;
use rayon::prelude::*;

use crate::core::curve::{self, Curve};
use crate::core::point::PointColours;
use crate::core::profile::{multiply, Look, Rendering, Table};
use crate::core::tone::{self, MIDDLE_GREY};
use crate::core::document::{Basic, Document, Operation};
use crate::core::image::LinearImage;
use crate::core::mask::{Alpha, Mask, Pixels, Shape};
use crate::core::space::ColourSpace;

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

fn region_gain(amount: f32, mask: f32, stops: f32) -> f32 {
    if amount == 0.0 || mask == 0.0 {
        1.0
    } else {
        2.0f32.powf(amount / 100.0 * stops * mask)
    }
}

pub fn to_working_space(document: &Document, source: &LinearImage) -> LinearImage {

    let denoised = ai_denoise::for_render(document, source);
    let source = denoised.as_ref().unwrap_or(source);

    let balance = document.white_balance;
    let Some(profile) = &source.profile else {
        return source.clone();
    };

    let mut data = source.data.clone();

    let effective = balance.unwrap_or_else(|| profile.as_shot_white_balance());

    let resolved = profile_for(document, source)
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

    LinearImage::new(source.width, source.height, data)
        .with_film_mode(source.film_mode.clone())
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
) -> Option<std::sync::Arc<crate::core::profile::DngProfile>> {
    match document.colour_profile.as_deref() {
        Some(NO_COLOUR_PROFILE) => None,
        Some(name) => crate::io::dcp::by_name(name).or_else(|| source.rendering.clone()),
        None => source.rendering.clone(),
    }
}

pub const MASK_RASTER: usize = 2048;

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
        mask.unshaped = Pixels(Some(Arc::new(ranged)));
        mask.reshape_edge(width, height);
        return;
    }
    if let Shape::LuminanceRange { low, high, softness, picked } = mask.shape {
        let Some(frame) = frame else { return };
        let ranged = match picked {
            true => range::luminance(frame, low, high, softness, width, height),
            false => Alpha::new(width, height, vec![0.0; width * height]),
        };
        mask.unshaped = Pixels(Some(Arc::new(ranged)));
        mask.reshape_edge(width, height);
        return;
    }

    let mut matted = false;
    let base = match (&mask.shape, found) {
        (Shape::Segment { classes }, Some(segmentation)) => {

            let live: Vec<u16> =
                classes.iter().copied().filter(|class| !mask.muted.contains(class)).collect();
            let named = segmentation.alpha(&live);

            let empty = named.data.iter().map(|value| *value as f64).sum::<f64>()
                / named.data.len().max(1) as f64
                <= 0.001;
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

    let alpha = mask.rasterise(base.as_ref(), &regions, width, height);

    let photo = found.map(segment::Segmentation::photo).filter(|_| !matted);
    let alpha = search_edge(mask, alpha, |coarse| matte::refine(photo?, coarse));

    mask.unshaped = Pixels(Some(Arc::new(alpha)));
    mask.reshape_edge(width, height);
}

fn search_edge(mask: &Mask, mut alpha: Alpha, refine: impl FnOnce(&Alpha) -> Option<Alpha>) -> Alpha {
    if !mask.matte {
        return alpha;
    }
    mask.edge_to_search_from(&mut alpha);
    refine(&alpha).unwrap_or(alpha)
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

    let mut geometry = Document::new(document.source.path.clone());
    geometry.set_perspective(document.perspective());
    if let Some((rect, angle)) = document.crop() {
        geometry.set_crop(rect, angle);
    }
    geometry.set_rotation(document.rotation());
    geometry.set_mirrored(document.mirrored());

    let proxy = source.downscaled(2400).unwrap_or_else(|| source.clone());
    let working = to_working_space(&geometry, &proxy);
    let frame = apply_stack(&geometry, &working, 1.0);

    let (wants_model, wants_prompt) = models_needed(&masks);
    let found = wants_model.then(|| segment::of(&frame)).flatten();

    let clicked = wants_prompt.then(|| sam::encode(&frame)).flatten();

    let (width, height) = raster_size(frame.width(), frame.height(), MASK_RASTER);
    for mask in masks.iter_mut().filter(|mask| mask.wants_pixels()) {
        resolve_mask(mask, found.as_ref(), clicked.as_ref(), Some(&frame), width, height);
    }

    let mut resolved = document.clone();
    resolved.set_masks(masks);
    resolved
}

pub fn apply_stack(document: &Document, working: &LinearImage, detail_scale: f32) -> RgbImage {

    let geometry = geometry_of(document, working);

    apply_pixels(document, geometry.as_ref().unwrap_or(working), detail_scale, WHOLE_FRAME)
}

pub const WHOLE_FRAME: [f32; 4] = [0.0, 0.0, 1.0, 1.0];

pub fn apply_pixels(
    document: &Document,
    working: &LinearImage,
    detail_scale: f32,
    region: [f32; 4],
) -> RgbImage {
    let mut data = working.data.clone();

    retouch::apply(
        &document.retouch(),
        &mut data,
        working.width as usize,
        working.height as usize,
        region,
    );

    let faces = document.faces();
    if !faces.is_empty() {
        beautify::apply(
            &document.beautify(),
            &faces,
            &mut data,
            working.width as usize,
            working.height as usize,
            region,
        );
    }

    let basic = document.basic();
    let (width, height) = (working.width as usize, working.height as usize);
    detail::denoise(
        &mut data,
        width,
        height,
        basic.denoise_luma / 100.0,
        basic.denoise_detail / 100.0,
        basic.denoise_contrast / 100.0,
        basic.denoise_colour / 100.0,
        detail_scale,
    );
    detail::sharpen(
        &mut data,
        width,
        height,
        basic.sharpen / 100.0,
        basic.sharpen_radius,
        basic.sharpen_masking / 100.0,
        detail_scale,
    );

    detail::defringe(&mut data, width, height, basic.defringe / 100.0, detail_scale);

    detail::moire(&mut data, width, height, basic.moire / 100.0, detail_scale);

    let weights = document.working_space.luminance_weights();
    effects::calibrate(
        &mut data,
        [basic.red_hue, basic.green_hue, basic.blue_hue],
        [basic.red_saturation, basic.green_saturation, basic.blue_saturation],
        basic.shadow_tint,
        weights,
    );
    effects::dehaze(&mut data, width, height, basic.dehaze);

    let mut looks: Vec<Look> = Vec::new();
    let mixer = document.mixer();
    if !mixer.is_identity() {
        looks.push(Look::new(Table::resolve(&mixer.table(), 0.0)));
    }

    let points = document.point_colours();
    let colour_pass = |data: &mut [f32]| {
        apply_looks(&looks, data);
        apply_point_colours(&points, document.working_space, data);
    };
    let colour_work = !looks.is_empty() || !points.is_identity();

    for operation in &document.operations {
        apply(operation, &mut data);

        if colour_work && matches!(operation, Operation::Basic(_)) {
            colour_pass(&mut data);
        }

        if let Operation::Basic(basic) = operation {
            local::tone_map(
                &mut data,
                working.width as usize,
                working.height as usize,
                basic.hdr / 100.0,
                basic.clarity / 100.0,
                basic.texture / 100.0,
            );
        }
    }

    if colour_work
        && !document.operations.iter().any(|op| matches!(op, Operation::Basic(_)))
    {
        colour_pass(&mut data);
    }

    apply_masks(document, &mut data, width, height, region);

    let grading = document.grading();
    if !grading.is_identity() {
        data.par_chunks_exact_mut(3).for_each(|pixel| {
            let graded = grading.apply([pixel[0], pixel[1], pixel[2]]);
            pixel.copy_from_slice(&graded);
        });
    }

    let frame = [width as f32 / region[2].max(1e-6), height as f32 / region[3].max(1e-6)];
    effects::vignette(
        &mut data,
        width,
        height,
        region,
        frame,
        [basic.vignette, basic.vignette_midpoint, basic.vignette_roundness, basic.vignette_feather],
    );
    let full = frame.map(|edge| edge / detail_scale.max(1e-6));
    effects::grain(
        &mut data,
        width,
        height,
        region,
        full,
        [basic.grain, basic.grain_size, basic.grain_roughness],
        weights,
    );

    encode_srgb(
        working.width,
        working.height,
        &data,
        &document.curves(),
        document.working_space,
        document.output_space,
    )
}

fn apply_masks(
    document: &Document,
    data: &mut [f32],
    width: usize,
    height: usize,
    region: [f32; 4],
) {

    for mask in document.masks() {
        if mask.is_idle() {
            continue;
        }

        let field = mask.field(width, height, region);
        let mut local = data.to_vec();
        apply_basic(&mask.basic, &mut local);

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
    let basic = document.basic();
    basic.hdr == 0.0
        && basic.clarity == 0.0
        && basic.texture == 0.0

        && basic.dehaze == 0.0
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
        let margin = [margin[0] * spot.radius, margin[1] * spot.radius];
        [spot.at, spot.from].iter().all(|point| {
            point[0] - margin[0] >= region[0]
                && point[1] - margin[1] >= region[1]
                && point[0] + margin[0] <= region[0] + region[2]
                && point[1] + margin[1] <= region[1] + region[3]
        })
    })
}

pub fn develop(document: &Document, source: &LinearImage) -> RgbImage {
    let working = to_working_space(document, source);

    apply_stack(document, &working, 1.0)
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
        || basic.lens_distortion != 0.0
        || basic.lens_vignetting != 0.0
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

pub fn geometry_only(document: &Document, working: &LinearImage) -> LinearImage {
    geometry_of(document, working).unwrap_or_else(|| working.clone())
}

fn geometry_of(document: &Document, working: &LinearImage) -> Option<LinearImage> {
    let rotation = document.rotation();
    let crop = document.crop();
    let mirrored = document.mirrored();
    let basic = document.basic();
    let manual_lens = basic.lens_distortion != 0.0 || basic.lens_vignetting != 0.0;

    if rotation == 0.0 && crop.is_none() && !mirrored && !manual_lens {
        return None;
    }

    let corrected = manual_lens.then(|| {
        let profile = crate::io::raw::manual_lens_profile(basic.lens_distortion, basic.lens_vignetting);
        let mut image = working.clone();
        if basic.lens_vignetting != 0.0 {
            crate::io::raw::correct_vignetting(&mut image, &profile);
        }
        if basic.lens_distortion != 0.0 {
            image = crate::io::raw::correct_geometry(&image, &profile);
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

    let gain = 2.0f32.powf(basic.exposure);
    let slope = 1.0 + basic.contrast / 100.0;
    let contrasty = (slope - 1.0).abs() > f32::EPSILON;

    let toned = basic.shadows != 0.0
        || basic.highlights != 0.0
        || basic.blacks != 0.0
        || basic.whites != 0.0;

    data.par_chunks_exact_mut(3).for_each(|pixel| {

        for channel in pixel.iter_mut() {
            *channel = (*channel * gain).max(0.0);
        }

        if contrasty {
            for channel in pixel.iter_mut() {
                *channel = MIDDLE_GREY * (*channel / MIDDLE_GREY).powf(slope);
            }
        }

        if toned {
            let position = tone_position(luminance(pixel));
            let tone = region_gain(basic.shadows, ramp((0.5 - position) / 0.5), REGION_STOPS)
                * region_gain(basic.highlights, ramp((position - 0.5) / 0.5), REGION_STOPS)
                * region_gain(basic.blacks, ramp((0.25 - position) / 0.25), ENDPOINT_STOPS)
                * region_gain(basic.whites, ramp((position - 0.75) / 0.25), ENDPOINT_STOPS);

            if tone != 1.0 {
                for channel in pixel.iter_mut() {
                    *channel *= tone;
                }
            }
        }

        apply_saturation(basic, pixel);
    });
}

fn apply_saturation(basic: &Basic, pixel: &mut [f32]) {
    if basic.saturation == 0.0 && basic.vibrance == 0.0 {
        return;
    }

    let luma = luminance(pixel);
    let high = pixel[0].max(pixel[1]).max(pixel[2]);
    let low = pixel[0].min(pixel[1]).min(pixel[2]);

    let current = if high > 0.0 { (high - low) / high } else { 0.0 };

    let factor = 1.0 + basic.saturation / 100.0 + (basic.vibrance / 100.0) * (1.0 - current);

    for channel in pixel.iter_mut() {
        *channel = (luma + (*channel - luma) * factor).max(0.0);
    }
}

fn encode_srgb(
    width: u32,
    height: u32,
    data: &[f32],

    curves: &[Curve],
    working: ColourSpace,
    output: ColourSpace,
) -> RgbImage {
    let mut out = vec![0u8; data.len()];

    let lookups: Vec<Option<[f32; curve::LOOKUP]>> =
        curves.iter().map(|curve| (!curve.is_identity()).then(|| curve.lookup())).collect();
    let composite = lookups.first().cloned().flatten();
    let channel_lookup = |channel: usize| lookups.get(1 + channel).cloned().flatten();
    let channels = [channel_lookup(0), channel_lookup(1), channel_lookup(2)];

    let recode = (working != output || output != ColourSpace::Srgb)
        .then(|| (working.convert_to(output), output));

    let read = |lookup: &Option<[f32; curve::LOOKUP]>, display: f32| -> f32 {
        match lookup {

            Some(table) => {
                let at = display * (curve::LOOKUP - 1) as f32;
                let low = at.floor() as usize;
                let high = (low + 1).min(curve::LOOKUP - 1);
                let fraction = at - low as f32;
                table[low] * (1.0 - fraction) + table[high] * fraction
            }
            None => display,
        }
    };
    let shape = |channel: usize, display: f32| read(&channels[channel], read(&composite, display));

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
                    *byte = (target.encode(linear) * 255.0 + 0.5) as u8;
                }
            });
        return RgbImage::from_raw(width, height, out).expect("buffer matches dimensions");
    }

    out.par_chunks_exact_mut(3)
        .zip(data.par_chunks_exact(3))
        .for_each(|(bytes, pixel)| {
            for (channel, (byte, value)) in bytes.iter_mut().zip(pixel).enumerate() {
                let display = tone::curve(*value).clamp(0.0, 1.0);
                *byte = (shape(channel, display) * 255.0 + 0.5) as u8;
            }
        });

    RgbImage::from_raw(width, height, out).expect("buffer matches dimensions")
}

#[cfg(test)]
mod tests {
    use super::*;

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
        lifted.set_basic(Basic { lens_vignetting: 100.0, ..Default::default() });
        let out = geometry_of(&lifted, &image).expect("a correction to make");
        assert!(at(&out, 1, 1) > at(&image, 1, 1) * 1.2, "corners lifted");
        assert!((at(&out, 32, 24) - at(&image, 32, 24)).abs() < 0.02, "centre as it was");

        let mut straightened = Document::new(String::new());
        straightened.set_basic(Basic { lens_distortion: 100.0, ..Default::default() });
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
        let answer = search_edge(&mask, wide.clone(), |coarse| {
            shown = border(coarse);
            Some(coarse.clone())
        });
        assert!(shown < w / 2 - 4, "the model saw the edge pulled in: {shown}");

        mask.unshaped = Pixels(Some(Arc::new(answer)));
        assert!(mask.reshape_edge(w, h));
        let kept = border(mask.map.0.as_ref().unwrap());
        assert!(kept.abs_diff(shown) <= 1, "and it was not pulled in twice: {kept} against {shown}");

        mask.matte_edge = 0.0;
        search_edge(&mask, wide.clone(), |coarse| {
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
            .filter(|path| crate::io::raw::is_supported(path))
            .collect();
        paths.sort();
        let step = (paths.len() / 6).max(1);
        paths = paths.into_iter().step_by(step).take(6).collect();

        println!("{:<16} {:>10} {:>9} {:>10} {:>9}", "frame", "defringe", "worst", "moire", "worst");
        for path in paths {
            let Ok(linear) = crate::io::raw::decode_linear(&path) else { continue };
            let document = Document::new(path.display().to_string());
            let working = to_working_space(&document, &linear);
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
            fringed.set_basic(Basic { defringe: 100.0, ..Default::default() });
            let (fringe_share, fringe_worst) = changed(&apply_stack(&fringed, &working, 1.0));

            let mut moired = document.clone();
            moired.set_basic(Basic { moire: 100.0, ..Default::default() });
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
        use crate::core::space::ColourSpace;

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
    fn texture_and_faces_do_not_tile() {
        assert!(tiles_cleanly(&plain()));
        let mut textured = plain();
        textured.set_basic(Basic { texture: 20.0, ..Default::default() });
        assert!(!tiles_cleanly(&textured));
        let mut portrait = plain();
        portrait.set_beautify(crate::core::beautify::Beautify { skin: 40.0, ..Default::default() });
        assert!(!tiles_cleanly(&portrait));
    }

    #[test]
    fn a_spot_near_the_top_of_a_landscape_frame_needs_more_room_than_its_radius() {
        use crate::core::retouch::{Retouch, Spot};
        let mut document = plain();
        document.set_retouch(Retouch {
            spots: vec![Spot { at: [0.5, 0.062], from: [0.5, 0.5], radius: 0.01, ..Default::default() }],
        });

        assert!(!spots_within(&document, [0.0, 0.05, 1.0, 0.9], LANDSCAPE));
        assert!(spots_within(&document, WHOLE_FRAME, LANDSCAPE));
    }

    #[test]
    fn a_clicked_mask_asks_for_both_models() {
        use crate::core::mask::{Mask, RegionPoint, Shape};

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
        use crate::core::grading::{Grading, Range};

        let dark = rgb(0.02, 0.02, 0.02);
        let bright = rgb(1.5, 1.5, 1.5);

        let mut document = plain();
        document.set_grading(Grading {
            shadows: Range { hue: 240.0, saturation: 80.0, luminance: 0.0 },
            highlights: Range { hue: 40.0, saturation: 80.0, luminance: 0.0 },
            ..Default::default()
        });

        let shadow = develop(&document, &dark);
        let shadow = shadow.get_pixel(0, 0).0;
        assert!(shadow[2] > shadow[0], "the shadows should render blue: {shadow:?}");

        let highlight = develop(&document, &bright);
        let highlight = highlight.get_pixel(0, 0).0;
        assert!(
            highlight[0] >= highlight[2],
            "the highlights should render warm: {highlight:?}"
        );

        let plain = develop(&plain(), &dark).get_pixel(0, 0).0;
        assert_ne!(plain, shadow);
    }

    #[test]
    fn a_stored_mask_gets_its_pixels_before_it_is_rendered() {
        use crate::core::mask::{Mask, Shape, Stroke};

        let grey = LinearImage::new(32, 32, vec![0.5; 32 * 32 * 3]);
        let mut painted = Mask::new(Shape::Painted);
        painted.basic = Basic { exposure: -4.0, ..Default::default() };
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
        use crate::core::profile::DngProfile;

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
            profile_for(document, &source).map(|profile| profile.name.clone())
        };

        assert_eq!(name(&plain()).as_deref(), Some("Matched"));

        let mut none = plain();
        none.colour_profile = Some(NO_COLOUR_PROFILE.to_string());
        assert_eq!(name(&none), None);

        let mut missing = plain();
        missing.colour_profile = Some("Gone".to_string());
        assert_eq!(name(&missing).as_deref(), Some("Matched"));
    }

    #[test]
    fn a_mask_darkens_its_own_half_of_the_frame() {
        use crate::core::mask::{Mask, Shape};

        let flat = LinearImage::new(9, 1, vec![MIDDLE_GREY; 9 * 3]);
        let mut mask = Mask::new(Shape::Linear { from: [0.0, 0.5], to: [1.0, 0.5] });
        mask.basic.exposure = -2.0;

        let mut document = plain();
        document.set_masks(vec![mask]);
        let rendered = develop(&document, &flat);

        let left = rendered.get_pixel(0, 0)[0];
        let right = rendered.get_pixel(8, 0)[0];
        let untouched = develop(&plain(), &flat).get_pixel(0, 0)[0];

        assert!(left > untouched - 6, "the far end of the ramp was darkened: {left}");
        assert!(right < untouched / 2, "the near end was not: {right}");

        let middle = rendered.get_pixel(4, 0)[0];
        assert!(middle < left && middle > right, "{left} / {middle} / {right}");
    }

    #[test]
    fn a_mask_lands_in_the_same_place_on_a_tile() {
        use crate::core::mask::{Mask, Shape};

        let mut mask = Mask::new(Shape::Linear { from: [0.0, 0.5], to: [1.0, 0.5] });
        mask.basic.exposure = -2.0;
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
    fn a_spot_lands_in_the_same_place_on_a_tile() {
        use crate::core::retouch::{Retouch, Spot};

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
        use crate::core::retouch::{Retouch, Spot};

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
        use crate::core::mixer::Mixer;

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
        let before = develop(&plain(), &green).get_pixel(0, 0).0;
        let after = develop(&document, &green).get_pixel(0, 0).0;
        assert!(
            spread(after) < spread(before) / 2,
            "green kept its colour: {before:?} then {after:?}"
        );

        let red = rgb(0.4, 0.05, 0.05);
        let before = develop(&plain(), &red).get_pixel(0, 0).0;
        let after = develop(&document, &red).get_pixel(0, 0).0;
        assert!(
            (spread(after) - spread(before)).abs() < 8,
            "red followed the green band: {before:?} then {after:?}"
        );

        let mut clean = plain();
        clean.set_mixer(Mixer::default());
        assert!(clean.operations.is_empty());
    }

    #[test]
    fn point_colours_reach_the_render() {
        use crate::core::point::{PointColour, PointColours};

        let image = LinearImage::new(2, 1, vec![0.4, 0.05, 0.05, 0.05, 0.05, 0.4]);
        let untouched = develop(&plain(), &image);

        let mut empty = plain();
        empty.set_point_colours(PointColours::default());
        assert!(empty.operations.is_empty());
        assert_eq!(develop(&empty, &image).as_raw(), untouched.as_raw());

        for space in [ColourSpace::Srgb, ColourSpace::ProPhoto] {
            let mut document = plain();
            document.working_space = space;
            let before = develop(&document, &image);
            document.set_point_colours(PointColours {
                points: vec![PointColour { hue: 100.0, ..PointColour::picked([0.4, 0.05, 0.05]) }],
                highlight: None,
            });
            let after = develop(&document, &image);

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
        develop(document, image).get_pixel(0, 0)[0]
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
        let up = with(Basic { exposure: 1.0, ..Default::default() });
        assert_eq!(red(&up, &grey(0.18)), red(&plain(), &grey(0.36)));

        let down = with(Basic { exposure: -1.0, ..Default::default() });
        assert_eq!(red(&down, &grey(0.36)), red(&plain(), &grey(0.18)));
    }

    #[test]
    fn contrast_pivots_around_middle_grey() {
        let punchy = with(Basic { contrast: 50.0, ..Default::default() });

        assert_eq!(red(&punchy, &grey(MIDDLE_GREY)), 118, "the pivot must not move");
        assert!(red(&punchy, &grey(0.05)) < red(&plain(), &grey(0.05)));
        assert!(red(&punchy, &grey(0.5)) > red(&plain(), &grey(0.5)));
    }

    #[test]
    fn tone_sliders_stay_in_their_own_region() {
        let shadow = grey(0.02);
        let mid = grey(MIDDLE_GREY);
        let highlight = grey(0.75);

        let lifted = with(Basic { shadows: 100.0, ..Default::default() });
        assert!(red(&lifted, &shadow) > red(&plain(), &shadow), "shadows must lift");
        assert_eq!(red(&lifted, &highlight), red(&plain(), &highlight), "and leave highlights alone");

        let recovered = with(Basic { highlights: -100.0, ..Default::default() });
        assert!(red(&recovered, &highlight) < red(&plain(), &highlight), "highlights must recover");
        assert_eq!(red(&recovered, &shadow), red(&plain(), &shadow), "and leave shadows alone");

        for basic in [
            Basic { blacks: 100.0, ..Default::default() },
            Basic { whites: 100.0, ..Default::default() },
            Basic { shadows: 100.0, ..Default::default() },
            Basic { highlights: -100.0, ..Default::default() },
        ] {
            assert_eq!(red(&with(basic), &mid), 118, "middle grey moved: {basic:?}");
        }
    }

    #[test]
    fn saturation_moves_colour_but_not_grey() {
        let colour = rgb(0.4, 0.2, 0.1);

        let saturated = with(Basic { saturation: 50.0, ..Default::default() });
        let before = develop(&plain(), &colour);
        let after = develop(&saturated, &colour);
        let spread = |p: &image::Rgb<u8>| p[0] as i32 - p[2] as i32;
        assert!(spread(after.get_pixel(0, 0)) > spread(before.get_pixel(0, 0)));

        assert_eq!(red(&saturated, &grey(0.4)), red(&plain(), &grey(0.4)));

        let grey_out = develop(&with(Basic { saturation: -100.0, ..Default::default() }), &colour);
        let pixel = grey_out.get_pixel(0, 0);
        assert_eq!(pixel[0], pixel[1]);
        assert_eq!(pixel[1], pixel[2]);
    }

    #[test]
    fn vibrance_spares_already_saturated_colour() {
        let muted = rgb(0.30, 0.26, 0.24);
        let vivid = rgb(0.40, 0.02, 0.01);
        let vibrant = with(Basic { vibrance: 100.0, ..Default::default() });
        let spread = |image: &RgbImage| {
            let p = image.get_pixel(0, 0);
            p[0] as i32 - p[2] as i32
        };

        let muted_gain = spread(&develop(&vibrant, &muted)) - spread(&develop(&plain(), &muted));
        let vivid_gain = spread(&develop(&vibrant, &vivid)) - spread(&develop(&plain(), &vivid));
        assert!(
            muted_gain > vivid_gain,
            "vibrance must push muted colour harder: muted {muted_gain}, vivid {vivid_gain}"
        );
    }

    #[test]
    fn blown_highlights_desaturate_instead_of_shifting_hue() {

        let clipped = develop(&plain(), &rgb(4.0, 1.4, 1.1));
        let pixel = clipped.get_pixel(0, 0);
        let spread = pixel[0] as i32 - pixel[2] as i32;
        assert!(spread < 40, "blown highlight kept a {spread}/255 colour cast: {pixel:?}");
        assert!(pixel[0] > 200, "and it should still read as a highlight");

        let below = develop(&plain(), &rgb(0.4, 0.14, 0.11));
        let below_spread = below.get_pixel(0, 0)[0] as i32 - below.get_pixel(0, 0)[2] as i32;
        assert!(below_spread > spread, "a mid-tone must keep more colour than a blown one");
    }

    #[test]
    fn highlight_recovery_reaches_above_white() {

        let blown = grey(64.0);
        assert_eq!(red(&plain(), &blown), 255, "starts at display white");

        let recovered = with(Basic { highlights: -100.0, ..Default::default() });
        assert!(
            red(&recovered, &blown) < 255,
            "highlight recovery found nothing above white to pull back"
        );

        let bright = grey(2.0);
        assert!(red(&recovered, &bright) < red(&plain(), &bright));
    }

    #[test]
    fn negative_values_and_blowouts_stay_in_range() {
        assert_eq!(red(&with(Basic { exposure: -20.0, ..Default::default() }), &grey(1.0)), 0);
        assert_eq!(red(&with(Basic { exposure: 20.0, ..Default::default() }), &grey(1.0)), 255);
    }
}
