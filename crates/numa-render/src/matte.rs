use std::path::PathBuf;
use std::sync::OnceLock;

use image::{imageops, RgbImage};
use numa_infer::Model;

use numa_core::mask::Alpha;
use crate::local::{self, Plane};

const EDGE: usize = 1024;

const PAD: f32 = 0.35;

const INSIDE: f32 = 0.5;

const CONFIDENT: f32 = 0.9;

const STAIR_RADIUS: f32 = 1.0 / 256.0;
const STAIR_EPSILON: f32 = 1e-5;

const SLACK: f32 = 0.08;

const SOFT: f32 = 0.1;

const OPEN: f32 = 0.02;

const TILE: usize = 1024;

fn model_path() -> Option<PathBuf> {
    numa_core::paths::model_file(&["isnet.onnx", "isnet-general-use.onnx"])
}

pub fn is_installed() -> bool {
    model_path().is_some()
}

fn plan() -> Option<&'static Model> {
    static PLAN: OnceLock<Option<Model>> = OnceLock::new();
    PLAN.get_or_init(|| {
        let path = model_path()?;
        Model::load(&path)
    })
    .as_ref()
}

fn vitmatte() -> Option<&'static Model> {
    static PLAN: OnceLock<Option<Model>> = OnceLock::new();
    PLAN.get_or_init(|| Model::load(&numa_core::paths::model_file(&["vitmatte_small.onnx"])?))
        .as_ref()
}

pub fn refine(photo: &RgbImage, coarse: &Alpha) -> Option<Alpha> {
    asked_about(photo, coarse, Ask::ThisSubject)
}

pub fn finer(frame: &RgbImage, matte: &Alpha) -> Option<Alpha> {
    let plan = vitmatte()?;
    let (width, height) = (matte.width, matte.height);
    let frame = match frame.width() as usize == width && frame.height() as usize == height {
        true => std::borrow::Cow::Borrowed(frame),
        false => std::borrow::Cow::Owned(imageops::resize(
            frame,
            width as u32,
            height as u32,
            imageops::FilterType::Triangle,
        )),
    };

    let subject = keep_largest(matte.clone());
    let solid: Vec<f32> = subject.data.iter().map(|v| if *v >= INSIDE { 1.0 } else { 0.0 }).collect();
    let (left, top, right, bottom) = box_of(&solid, width)?;
    let long = (right - left + 1).max(bottom - top + 1) as f32;
    let open = (long * OPEN).max(2.0) as usize;
    let depth = local::blur(&Plane::new(width, height, solid.clone()), open).data;
    let around = local::blur(&Plane::new(width, height, solid), 3 * open).data;

    let asked: Vec<bool> = (0..width * height)
        .map(|i| {
            let band = depth[i] > 0.001 && depth[i] < 0.999;
            let loose = around[i] > 0.001 && (SOFT..1.0 - SOFT).contains(&matte.data[i]);
            band || loose
        })
        .collect();
    let trimap: Vec<f32> = (0..width * height)
        .map(|i| match (asked[i], matte.data[i] >= INSIDE) {
            (true, _) => 0.5,
            (false, true) => 1.0,
            (false, false) => 0.0,
        })
        .collect();
    let marked: Vec<f32> = asked.iter().map(|a| *a as u8 as f32).collect();
    let (a_left, a_top, a_right, a_bottom) = box_of(&marked, width)?;

    let side_w = TILE.min(width / 32 * 32);
    let side_h = TILE.min(height / 32 * 32);
    let starts = |from: usize, to: usize, side: usize, room: usize| -> Vec<usize> {
        let span = to + 1 - from;
        let count = (span.saturating_sub(side) as f32 / (side as f32 * 0.75)).ceil() as usize + 1;
        (0..count)
            .map(|i| match count {
                1 => (from + span / 2).saturating_sub(side / 2),
                _ => from + (span.saturating_sub(side)) * i / (count - 1),
            })
            .map(|at| at.min(room - side))
            .collect()
    };
    let mut answers = Vec::new();
    for y in starts(a_top, a_bottom, side_h, height) {
        for x in starts(a_left, a_right, side_w, width) {
            if !(y..y + side_h).any(|row| asked[row * width + x..row * width + x + side_w].iter().any(|a| *a)) {
                continue;
            }
            let mut input = ndarray::Array4::<f32>::zeros((1, 4, side_h, side_w));
            for row in 0..side_h {
                for column in 0..side_w {
                    let pixel = frame.get_pixel((x + column) as u32, (y + row) as u32);
                    for channel in 0..3 {

                        input[[0, channel, row, column]] = (pixel[channel] as f32 / 255.0 - 0.5) / 0.5;
                    }
                    input[[0, 3, row, column]] = trimap[(y + row) * width + x + column];
                }
            }
            let outputs = match plan.run(vec![input.into_dyn().into()]) {
                Ok(outputs) => outputs,
                Err(err) => {
                    log::warn!("ViTMatte failed: {err}");
                    return None;
                }
            };
            let alpha = outputs.first()?;
            if alpha.shape() != [1, 1, side_h, side_w] {
                log::warn!("ViTMatte answered with {:?}", alpha.shape());
                return None;
            }
            let piece = Piece { x: x as f32, y: y as f32, width: side_w as f32, height: side_h as f32 };
            answers.push((piece, alpha.iter().cloned().collect::<Vec<f32>>()));
        }
    }
    log::info!("ViTMatte: {} tiles of {side_w}x{side_h}", answers.len());

    let mut out = matte.data.clone();
    for (index, value) in out.iter_mut().enumerate().filter(|(index, _)| asked[*index]) {
        let (x, y) = (index % width, index / width);
        let (mut sum, mut weight) = (0.0f32, 0.0f32);
        for (piece, alpha) in &answers {
            let Some(share) = piece.covers(x as f32 + 0.5, y as f32 + 0.5) else { continue };
            let (column, row) = (x - piece.x as usize, y - piece.y as usize);
            sum += share * alpha[row * side_w + column];
            weight += share;
        }
        if weight > 0.0 {
            *value = (sum / weight).clamp(0.0, 1.0);
        }
    }
    Some(Alpha::new(width, height, out))
}

fn box_of(plane: &[f32], width: usize) -> Option<(usize, usize, usize, usize)> {
    let (mut left, mut top, mut right, mut bottom) = (usize::MAX, usize::MAX, 0usize, 0usize);
    for (index, _) in plane.iter().enumerate().filter(|(_, v)| **v > 0.0) {
        let (x, y) = (index % width, index / width);
        (left, top, right, bottom) = (left.min(x), top.min(y), right.max(x), bottom.max(y));
    }
    (right > left && bottom > top).then_some((left, top, right, bottom))
}

#[derive(Clone, Copy, PartialEq)]
enum Ask {

    ThisSubject,

    WhateverStandsOut,
}

fn asked_about(photo: &RgbImage, coarse: &Alpha, question: Ask) -> Option<Alpha> {
    let plan = plan()?;
    let (left, top, right, bottom) = bounds(coarse)?;

    let scale_x = photo.width() as f32 / coarse.width as f32;
    let scale_y = photo.height() as f32 / coarse.height as f32;
    let crop_x = (left as f32 * scale_x) as u32;
    let crop_y = (top as f32 * scale_y) as u32;
    let crop_w = (((right - left + 1) as f32 * scale_x) as u32).max(8).min(photo.width() - crop_x);
    let crop_h = (((bottom - top + 1) as f32 * scale_y) as u32).max(8).min(photo.height() - crop_y);

    let pieces = match question {
        Ask::ThisSubject => pieces_of(crop_x, crop_y, crop_w, crop_h, photo.width(), photo.height()),
        Ask::WhateverStandsOut => {
            vec![Piece { x: crop_x as f32, y: crop_y as f32, width: crop_w as f32, height: crop_h as f32 }]
        }
    };

    let mut mattes = Vec::with_capacity(pieces.len());
    let mut strongest = 0.0f32;
    for piece in &pieces {
        let (answer, confidence) = ask(plan, photo, *piece)?;
        strongest = strongest.max(confidence);
        mattes.push(answer);
    }

    if strongest < CONFIDENT {
        return None;
    }

    let mut out = Alpha::new(coarse.width, coarse.height, vec![0.0; coarse.data.len()]);
    let slack = (right - left + 1).max(bottom - top + 1) as f32 * SLACK;
    let allowed = gate(coarse, slack);
    for y in top..=bottom {
        for x in left..=right {

            let px = (x as f32 + 0.5) * scale_x;
            let py = (y as f32 + 0.5) * scale_y;

            let (mut sum, mut weight) = (0.0f32, 0.0f32);
            for (piece, matte) in pieces.iter().zip(&mattes) {
                let Some(share) = piece.covers(px, py) else { continue };
                let u = (px - piece.x) / piece.width;
                let v = (py - piece.y) / piece.height;
                sum += share * sample(matte, u, v);
                weight += share;
            }
            let value = match weight > 0.0 {
                true => (sum / weight).clamp(0.0, 1.0),
                false => 0.0,
            };
            out.data[y * coarse.width + x] = value;
        }
    }

    let guide = luminance_of(photo, coarse.width, coarse.height);
    let radius = ((coarse.width.max(coarse.height) as f32 * STAIR_RADIUS) as usize).max(1);
    let smoothed = local::guided_by(&guide, &Plane::new(coarse.width, coarse.height, out.data), radius, STAIR_EPSILON);

    let out = Alpha::new(
        coarse.width,
        coarse.height,
        smoothed.data.iter().zip(&allowed).map(|(value, allow)| value.clamp(0.0, 1.0) * allow).collect(),
    );
    Some(out)
}

#[derive(Clone, Copy)]
struct Piece {
    x: f32,
    y: f32,
    width: f32,
    height: f32,
}

impl Piece {

    fn covers(&self, px: f32, py: f32) -> Option<f32> {
        let inside = |value: f32, start: f32, span: f32| {
            let offset = value - start;
            (offset >= 0.0 && offset <= span).then_some(offset.min(span - offset))
        };
        let (dx, dy) = (inside(px, self.x, self.width)?, inside(py, self.y, self.height)?);

        Some(dx.min(dy) + 1.0)
    }
}

fn pieces_of(x: u32, y: u32, width: u32, height: u32, frame_w: u32, frame_h: u32) -> Vec<Piece> {
    let (w, h) = (width as f32, height as f32);
    let (frame_w, frame_h) = (frame_w as f32, frame_h as f32);
    let (x, y) = (x as f32, y as f32);
    let whole = Piece { x, y, width: w, height: h };

    const ONE_PIECE: f32 = 1.3;
    let (long, short) = (w.max(h), w.min(h));
    if long <= short * ONE_PIECE {
        return vec![whole];
    }

    let side = (long * 0.6).max(short).min(frame_w).min(frame_h);
    let tall = h > w;
    let (along, across) = match tall {
        true => (y, x),
        false => (x, y),
    };
    let (span, room_along, room_across, other_span) = match tall {
        true => (h, frame_h, frame_w, w),
        false => (w, frame_w, frame_h, h),
    };
    let fixed = (across + other_span / 2.0 - side / 2.0).clamp(0.0, (room_across - side).max(0.0));
    let first = along.clamp(0.0, (room_along - side).max(0.0));
    let last = (along + span - side).clamp(0.0, (room_along - side).max(0.0));

    [first, last]
        .into_iter()
        .map(|start| match tall {
            true => Piece { x: fixed, y: start, width: side, height: side },
            false => Piece { x: start, y: fixed, width: side, height: side },
        })
        .collect()
}

fn ask(plan: &Model, photo: &RgbImage, piece: Piece) -> Option<(Vec<f32>, f32)> {
    let crop = imageops::crop_imm(
        photo,
        piece.x as u32,
        piece.y as u32,
        (piece.width as u32).max(1),
        (piece.height as u32).max(1),
    )
    .to_image();
    let square = imageops::resize(&crop, EDGE as u32, EDGE as u32, imageops::FilterType::Lanczos3);

    let mut input = ndarray::Array4::<f32>::zeros((1, 3, EDGE, EDGE));
    for (x, y, pixel) in square.enumerate_pixels() {
        for channel in 0..3 {

            input[[0, channel, y as usize, x as usize]] = pixel[channel] as f32 / 255.0 - 0.5;
        }
    }

    let outputs = match plan.run(vec![input.into_dyn().into()]) {
        Ok(outputs) => outputs,
        Err(err) => {
            log::warn!("matting failed: {err}");
            return None;
        }
    };
    let matte = outputs.first()?;
    if matte.shape() != [1, 1, EDGE, EDGE] {
        log::warn!("the matting model answered with {:?}", matte.shape());
        return None;
    }

    let strongest = matte.iter().cloned().fold(0.0f32, f32::max);

    let mut answer = vec![0.0f32; EDGE * EDGE];
    for (index, value) in matte.iter().enumerate() {
        answer[index] = *value;
    }
    Some((answer, strongest))
}

fn sample(matte: &[f32], u: f32, v: f32) -> f32 {
    let fx = u.clamp(0.0, 1.0) * (EDGE - 1) as f32;
    let fy = v.clamp(0.0, 1.0) * (EDGE - 1) as f32;
    let (x0, y0) = (fx.floor() as usize, fy.floor() as usize);
    let (x1, y1) = ((x0 + 1).min(EDGE - 1), (y0 + 1).min(EDGE - 1));
    let (tx, ty) = (fx - x0 as f32, fy - y0 as f32);
    let at = |sx: usize, sy: usize| matte[sy * EDGE + sx];
    let top = at(x0, y0) * (1.0 - tx) + at(x1, y0) * tx;
    let bottom = at(x0, y1) * (1.0 - tx) + at(x1, y1) * tx;
    top * (1.0 - ty) + bottom * ty
}

fn gate(coarse: &Alpha, slack: f32) -> Vec<f32> {
    let inside = coarse.data.iter().map(|v| if *v >= INSIDE { 1.0 } else { 0.0 }).collect();
    spread(inside, coarse.width, coarse.height, slack)
}

fn spread(inside: Vec<f32>, width: usize, height: usize, radius: f32) -> Vec<f32> {
    let radius = (radius.max(1.0).round() as usize).max(1);
    let plane = Plane::new(width, height, inside);

    let plane = local::blur(&plane, radius).data;

    plane
        .iter()
        .map(|v| {
            let t = (v * 2.0).clamp(0.0, 1.0);
            t * t * (3.0 - 2.0 * t)
        })
        .collect()
}

pub fn subject(photo: &RgbImage, width: usize, height: usize) -> Option<Alpha> {

    let everything = Alpha::new(width, height, vec![1.0; width * height]);
    let found = asked_about(photo, &everything, Ask::WhateverStandsOut)?;
    let largest = keep_largest(found);

    let covered: f64 =
        largest.data.iter().map(|value| *value as f64).sum::<f64>() / largest.data.len() as f64;

    (0.0005..0.85).contains(&covered).then_some(largest)
}

fn keep_largest(alpha: Alpha) -> Alpha {
    let (width, height) = (alpha.width, alpha.height);

    let reach = (width.max(height) / 100).max(2);
    let spread = local::blur(
        &Plane::new(
            width,
            height,
            alpha.data.iter().map(|value| (*value > INSIDE) as u8 as f32).collect(),
        ),
        reach,
    );
    let solid = |index: usize| spread.data[index] > 0.02;

    let mut label = vec![usize::MAX; width * height];
    let mut sizes: Vec<usize> = Vec::new();
    let mut stack: Vec<usize> = Vec::new();

    for start in 0..width * height {
        if !solid(start) || label[start] != usize::MAX {
            continue;
        }
        let this = sizes.len();
        sizes.push(0);
        stack.push(start);
        label[start] = this;
        while let Some(index) = stack.pop() {
            sizes[this] += 1;
            let (x, y) = (index % width, index / width);
            let mut visit = |nx: usize, ny: usize| {
                let at = ny * width + nx;
                if solid(at) && label[at] == usize::MAX {
                    label[at] = this;
                    stack.push(at);
                }
            };
            if x > 0 {
                visit(x - 1, y);
            }
            if x + 1 < width {
                visit(x + 1, y);
            }
            if y > 0 {
                visit(x, y - 1);
            }
            if y + 1 < height {
                visit(x, y + 1);
            }
        }
    }

    let Some(winner) = (0..sizes.len()).max_by_key(|index| sizes[*index]) else {
        return alpha;
    };

    let mut out = Alpha::new(width, height, vec![0.0; width * height]);
    for index in 0..width * height {
        if label[index] == winner {
            out.data[index] = alpha.data[index];
        }
    }
    out
}

fn bounds(coarse: &Alpha) -> Option<(usize, usize, usize, usize)> {
    let (mut left, mut top) = (usize::MAX, usize::MAX);
    let (mut right, mut bottom) = (0usize, 0usize);
    for (index, value) in coarse.data.iter().enumerate() {
        if *value < INSIDE {
            continue;
        }
        let (x, y) = (index % coarse.width, index / coarse.width);
        left = left.min(x);
        top = top.min(y);
        right = right.max(x);
        bottom = bottom.max(y);
    }
    if right <= left || bottom <= top {
        return None;
    }

    let pad = (((right - left + 1).max(bottom - top + 1)) as f32 * PAD) as usize;
    Some((
        left.saturating_sub(pad),
        top.saturating_sub(pad),
        (right + pad).min(coarse.width - 1),
        (bottom + pad).min(coarse.height - 1),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore]
    fn another_model_on_a_crop() {
        let (Ok(model), Ok(photo), Ok(out)) =
            (std::env::var("MODEL"), std::env::var("PHOTO"), std::env::var("OUT"))
        else {
            return;
        };
        let edge: usize = std::env::var("ISNET_EDGE").ok().and_then(|v| v.parse().ok()).unwrap_or(1024);
        let photo = image::open(photo).unwrap().to_rgb8();
        let started = std::time::Instant::now();
        let plan = Model::load(std::path::Path::new(&model)).unwrap();
        println!("planned in {:?}", started.elapsed());

        let boxes: Vec<(u32, u32, u32, u32)> = vec![
            (0, 0, photo.width(), photo.height()),
            (754, 173, 808, 712),
        ];
        for (x, y, w, h) in boxes {
            let crop = imageops::crop_imm(&photo, x, y, w, h).to_image();
            let square = imageops::resize(&crop, edge as u32, edge as u32, imageops::FilterType::Lanczos3);
            let mut input = ndarray::Array4::<f32>::zeros((1, 3, edge, edge));
            for (px, py, pixel) in square.enumerate_pixels() {
                for c in 0..3 {

                    let v = pixel[c] as f32 / 255.0;
                    input[[0, c, py as usize, px as usize]] = match std::env::var("NORM").as_deref() {
                        Ok("imagenet") => (v - [0.485, 0.456, 0.406][c]) / [0.229, 0.224, 0.225][c],
                        _ => v - 0.5,
                    };
                }
            }
            let started = std::time::Instant::now();
            let outputs = plan.run(vec![input.into_dyn().into()]).unwrap();
            println!("crop {w}x{h}: ran in {:?}, {} outputs, first {:?}", started.elapsed(), outputs.len(), outputs[0].shape());
            let pred = &outputs[0];
            let sigmoid = std::env::var("SIGMOID").is_ok();
            let (lo, hi) = pred.iter().fold((f32::MAX, f32::MIN), |(lo, hi), v| (lo.min(*v), hi.max(*v)));
            println!("  output range {lo:.3}..{hi:.3}");
            let image = image::GrayImage::from_fn(edge as u32, edge as u32, |px, py| {
                let raw = pred[[0, 0, py as usize, px as usize]];
                let v = if sigmoid { 1.0 / (1.0 + (-raw).exp()) } else { (raw - lo) / (hi - lo).max(1e-6) };
                image::Luma([(v.clamp(0.0, 1.0) * 255.0) as u8])
            });
            image.save(format!("{out}/{}-{w}.png", std::env::var("TAG").unwrap_or("model".into()))).unwrap();
        }
    }

    #[test]
    #[ignore]
    fn matte_alone() {
        let Ok(path) = std::env::var("FRAME") else { return };
        let path = std::path::PathBuf::from(path);
        let linear = numa_io::raw::decode_linear(&path).unwrap();
        let document = numa_core::document::Document::new(path.display().to_string());
        let working = crate::to_working_space(&document, &linear, &Default::default());
        let frame = crate::apply_stack(&document, &working, 1.0);

        let (w, h) = (512usize, 341usize);

        match subject(&frame, w, h) {
            Some(alpha) => {
                let share: f64 = alpha.data.iter().map(|v| *v as f64).sum::<f64>()
                    / alpha.data.len() as f64;
                let solid = alpha.data.iter().filter(|v| **v > 0.9).count();
                let edge = alpha.data.iter().filter(|v| (0.1..0.9).contains(*v)).count();
                println!(
                    "matte alleen: {:.2}% gedekt, {solid} vol, {edge} randcellen van {}",
                    share * 100.0,
                    alpha.data.len()
                );

                let (mut left, mut top, mut right, mut bottom) = (w, h, 0usize, 0usize);
                for y in 0..h {
                    for x in 0..w {
                        if alpha.data[y * w + x] > 0.5 {
                            left = left.min(x);
                            right = right.max(x);
                            top = top.min(y);
                            bottom = bottom.max(y);
                        }
                    }
                }
                if right >= left {
                    println!(
                        "  doos: u {:.2}..{:.2}  v {:.2}..{:.2}",
                        left as f32 / w as f32,
                        right as f32 / w as f32,
                        top as f32 / h as f32,
                        bottom as f32 / h as f32
                    );
                }

                for row in 0..34 {
                    let mut line = String::new();
                    for column in 0..72 {
                        let x = column * w / 72;
                        let y = row * h / 34;
                        let mut most = 0.0f32;
                        for dy in 0..(h / 34).max(1) {
                            for dx in 0..(w / 72).max(1) {
                                let at = (y + dy).min(h - 1) * w + (x + dx).min(w - 1);
                                most = most.max(alpha.data[at]);
                            }
                        }
                        line.push(match most {
                            m if m > 0.9 => '#',
                            m if m > 0.4 => '+',
                            m if m > 0.05 => '.',
                            _ => ' ',
                        });
                    }
                    println!("|{line}|");
                }
            }
            None => println!("de matte gaf niets terug — geen onderwerp gevonden"),
        }
    }

    fn box_alpha(width: usize, height: usize, rect: (usize, usize, usize, usize)) -> Alpha {
        let mut alpha = Alpha::new(width, height, vec![0.0; width * height]);
        for y in rect.1..=rect.3 {
            for x in rect.0..=rect.2 {
                alpha.data[y * width + x] = 1.0;
            }
        }
        alpha
    }

    #[test]
    fn the_box_is_padded_and_stays_inside_the_frame() {
        let alpha = box_alpha(100, 100, (40, 40, 59, 59));
        let (left, top, right, bottom) = bounds(&alpha).expect("a box");

        assert_eq!((left, top, right, bottom), (33, 33, 66, 66));

        let corner = box_alpha(100, 100, (0, 0, 9, 9));
        let (left, top, ..) = bounds(&corner).expect("a box");
        assert_eq!((left, top), (0, 0));
    }

    #[test]
    fn nothing_selected_has_no_box() {
        assert!(bounds(&Alpha::new(10, 10, vec![0.0; 100])).is_none());
    }

    #[test]
    fn the_gate_holds_the_matte_to_what_was_selected() {
        let alpha = box_alpha(64, 64, (10, 10, 30, 30));
        let g = gate(&alpha, 4.0);
        let at = |x: usize, y: usize| g[y * 64 + x];
        assert!(at(20, 20) > 0.99, "solid in the middle: {}", at(20, 20));
        assert!(at(32, 20) > 0.0, "just outside, within the slack: {}", at(32, 20));
        assert_eq!(at(55, 55), 0.0, "a second subject across the frame");
    }

    #[test]
    fn the_gate_stays_solid_where_the_mask_meets_the_frame() {
        let alpha = box_alpha(64, 64, (0, 0, 30, 30));
        let g = gate(&alpha, 8.0);
        let at = |x: usize, y: usize| g[y * 64 + x];
        assert!(at(0, 0) > 0.99, "solid in the corner: {}", at(0, 0));
        assert!(at(0, 15) > 0.99, "solid along the edge: {}", at(0, 15));
        assert!(at(15, 15) > 0.99, "solid in the middle: {}", at(15, 15));
    }

    #[test]
    fn the_gate_is_affordable_on_a_real_frame() {
        let alpha = box_alpha(2048, 1365, (400, 300, 1600, 1100));
        let started = std::time::Instant::now();
        let g = gate(&alpha, 96.0);
        let elapsed = started.elapsed();
        assert_eq!(g.len(), 2048 * 1365);
        assert!(elapsed.as_millis() < 500, "the gate took {elapsed:?}");
    }
}

fn luminance_of(photo: &RgbImage, width: usize, height: usize) -> Plane {
    let fitted = imageops::resize(photo, width as u32, height as u32, imageops::FilterType::Triangle);
    Plane::new(
        width,
        height,
        fitted
            .pixels()
            .map(|pixel| {
                (0.2126 * pixel[0] as f32 + 0.7152 * pixel[1] as f32 + 0.0722 * pixel[2] as f32) / 255.0
            })
            .collect(),
    )
}
