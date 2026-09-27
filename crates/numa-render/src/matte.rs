use image::{imageops, RgbImage};
use numa_infer::Model;

use numa_core::mask::Alpha;
use crate::local::{self, Plane};

const EDGE: usize = 1024;

const PAD: f32 = 0.35;

const INSIDE: f32 = 0.5;

const SCRAP: f32 = 0.05;

const CONFIDENT: f32 = 0.9;

const SLACK: f32 = 0.08;

const SOFT: f32 = 0.1;

const OPEN: f32 = 0.02;

const INWARD: usize = 8;

const TILE: usize = 1024;

#[cfg(not(target_os = "ios"))]
const BIREFNET: &str = "birefnet.onnx";
#[cfg(target_os = "ios")]
const BIREFNET: &str = "birefnet_lite_512.onnx";

const ISNET: [&str; 2] = ["isnet.onnx", "isnet-general-use.onnx"];

const ISNET_ALLOWED: bool = !cfg!(target_vendor = "apple");

const ROOM: u64 = 1_730_000_000 + 250_000_000;

const ISNET_ROOM: u64 = 2_040_000_000 + 250_000_000;

#[derive(Clone, Copy, PartialEq, Debug)]
enum Subject {
    BiRefNet,
    IsNet,
}

impl Subject {
    fn file(self) -> &'static str {
        match self {
            Subject::BiRefNet => BIREFNET,
            Subject::IsNet => ISNET[0],
        }
    }
}

fn choose(birefnet: bool, isnet: bool, room: Option<u64>) -> Option<Subject> {
    let fits = |need: u64| room.is_none_or(|free| free >= need);
    if birefnet && fits(ROOM) {
        Some(Subject::BiRefNet)
    } else if isnet && fits(ISNET_ROOM) {
        Some(Subject::IsNet)
    } else {
        None
    }
}

fn after_declined(subject: Subject, isnet: bool, room: Option<u64>) -> Option<Subject> {
    (subject == Subject::BiRefNet && isnet && room.is_none_or(|free| free >= ISNET_ROOM)).then_some(Subject::IsNet)
}

#[cfg(target_os = "ios")]
fn room() -> Option<u64> {

    extern "C" {
        fn os_proc_available_memory() -> usize;
    }
    Some(unsafe { os_proc_available_memory() } as u64)
}

#[cfg(not(target_os = "ios"))]
fn room() -> Option<u64> {
    None
}

fn isnet_here() -> bool {
    isnet_kept(ISNET_ALLOWED, numa_core::paths::model_file(&ISNET).is_some())
}

fn isnet_kept(allowed: bool, on_disk: bool) -> bool {
    allowed && on_disk
}

fn chosen() -> Option<Subject> {
    choose(numa_core::paths::model_file(&[BIREFNET]).is_some(), isnet_here(), room())
}

pub fn is_installed() -> bool {
    numa_core::paths::model_file(&[BIREFNET]).is_some() || isnet_here()
}

pub fn answering() -> String {
    named(chosen(), isnet_here())
}

fn named(chosen: Option<Subject>, isnet: bool) -> String {
    match chosen {
        Some(Subject::BiRefNet) if isnet => format!("{BIREFNET}+{}", ISNET[0]),
        Some(subject) => subject.file().to_string(),
        None => String::new(),
    }
}

fn load(subject: Subject) -> Option<std::sync::Arc<Model>> {
    static BIREFNET_PLAN: numa_infer::Kept = numa_infer::Kept::new();
    static ISNET_PLAN: numa_infer::Kept = numa_infer::Kept::new();
    match subject {
        Subject::BiRefNet => {
            let path = numa_core::paths::model_file(&[BIREFNET])?;
            BIREFNET_PLAN.get_or_init(|| {

                if let Err(err) = numa_infer::rewrite::prepare(&path) {
                    log::warn!("{}: not rewritten for the card: {err}", path.display());
                }
                let model = Model::load(&path)?;

                for old in ISNET.iter().filter_map(|name| numa_core::paths::model_file(&[name])) {
                    let _ = std::fs::remove_file(old);
                }
                Some(model)
            })
        }
        Subject::IsNet => {
            let path = numa_core::paths::model_file(&ISNET)?;
            ISNET_PLAN.get_or_init(|| Model::load(&path))
        }
    }
}

const VITMATTE: &str = "vitmatte_small.onnx";

#[cfg_attr(target_vendor = "apple", allow(dead_code))]
fn vitmatte() -> Option<std::sync::Arc<Model>> {
    static PLAN: numa_infer::Kept = numa_infer::Kept::new();
    PLAN.get_or_init(|| {
        let model = Model::load(&numa_core::paths::model_file(&[VITMATTE])?)?;

        let _ = std::fs::remove_file(numa_core::paths::models_dir().join("birefnet_lite_matting.onnx"));
        Some(model)
    })
}

pub fn refine(photo: &RgbImage, coarse: &Alpha) -> Option<Alpha> {
    asked_about(photo, coarse, Ask::ThisSubject)
}

pub fn finer(frame: &RgbImage, matte: &Alpha) -> Option<Alpha> {
    #[cfg(not(target_vendor = "apple"))]
    {
        let plan = vitmatte()?;
        by_vitmatte(&plan, &Band::of(frame, matte)?)
    }
    #[cfg(target_vendor = "apple")]
    {

        static SWEPT: std::sync::Once = std::sync::Once::new();
        SWEPT.call_once(|| {
            for old in [VITMATTE, "birefnet_lite_matting_512.onnx"] {
                let _ = std::fs::remove_file(numa_core::paths::models_dir().join(old));
            }
        });
        Some(by_closed_form(&Band::of(frame, matte)?))
    }
}

struct Band<'a> {

    frame: std::borrow::Cow<'a, RgbImage>,
    matte: &'a Alpha,
    width: usize,
    height: usize,

    asked: Vec<bool>,
    area: (usize, usize, usize, usize),
}

impl<'a> Band<'a> {

    fn of(frame: &'a RgbImage, matte: &'a Alpha) -> Option<Band<'a>> {
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

        let subject = keep_subjects(matte.clone());
        let solid: Vec<f32> = subject.data.iter().map(|v| if *v >= INSIDE { 1.0 } else { 0.0 }).collect();
        let (left, top, right, bottom) = box_of(&solid, width)?;
        let long = (right - left + 1).max(bottom - top + 1) as f32;
        let open = (long * OPEN).max(2.0) as usize;
        let depth = local::blur(&Plane::new(width, height, solid.clone()), open).data;
        let inner = local::blur(&Plane::new(width, height, solid.clone()), (open / INWARD).max(1)).data;
        let around = local::blur(&Plane::new(width, height, solid.clone()), 3 * open).data;

        let asked: Vec<bool> = (0..width * height)
            .map(|i| {
                let band = match solid[i] > 0.5 {
                    true => inner[i] < 0.999,
                    false => depth[i] > 0.001,
                };
                let loose = around[i] > 0.001 && (SOFT..1.0 - SOFT).contains(&matte.data[i]);
                band || loose
            })
            .collect();
        let marked: Vec<f32> = asked.iter().map(|a| *a as u8 as f32).collect();
        let area = box_of(&marked, width)?;
        Some(Band { frame, matte, width, height, asked, area })
    }

    fn trimap(&self) -> Vec<f32> {
        (0..self.width * self.height)
            .map(|i| match (self.asked[i], self.matte.data[i] >= INSIDE) {
                (true, _) => 0.5,
                (false, true) => 1.0,
                (false, false) => 0.0,
            })
            .collect()
    }

    fn starts(from: usize, to: usize, side: usize, room: usize) -> Vec<usize> {
        let span = to + 1 - from;
        let count = (span.saturating_sub(side) as f32 / (side as f32 * 0.75)).ceil() as usize + 1;
        (0..count)
            .map(|i| match count {
                1 => (from + span / 2).saturating_sub(side / 2),
                _ => from + (span.saturating_sub(side)) * i / (count - 1),
            })
            .map(|at| at.min(room - side))
            .collect()
    }

    fn tiles(&self, side_w: usize, side_h: usize) -> Vec<(usize, usize)> {
        let (left, top, right, bottom) = self.area;
        let width = self.width;
        let mut tiles = Vec::new();
        for y in Self::starts(top, bottom, side_h, self.height) {
            for x in Self::starts(left, right, side_w, width) {
                if (y..y + side_h).any(|row| self.asked[row * width + x..row * width + x + side_w].iter().any(|a| *a)) {
                    tiles.push((x, y));
                }
            }
        }
        tiles
    }
}

#[cfg_attr(target_vendor = "apple", allow(dead_code))]
fn by_vitmatte(plan: &Model, band: &Band) -> Option<Alpha> {
    let (width, height) = (band.width, band.height);
    let trimap = band.trimap();
    let side_w = TILE.min(width / 32 * 32);
    let side_h = TILE.min(height / 32 * 32);
    let mut answers = Vec::new();
    for (x, y) in band.tiles(side_w, side_h) {
        let mut input = ndarray::Array4::<f32>::zeros((1, 4, side_h, side_w));
        for row in 0..side_h {
            for column in 0..side_w {
                let pixel = band.frame.get_pixel((x + column) as u32, (y + row) as u32);
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
    log::info!("ViTMatte: {} tiles of {side_w}x{side_h}", answers.len());

    let mut out = band.matte.data.clone();
    for (index, value) in out.iter_mut().enumerate().filter(|(index, _)| band.asked[*index]) {
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

#[cfg_attr(not(target_vendor = "apple"), allow(dead_code))]
fn by_closed_form(band: &Band) -> Alpha {
    const PULL: f32 = 0.01;
    let rgb: Vec<[f32; 3]> = band.frame.pixels().map(|p| p.0.map(|c| c as f32 / 255.0)).collect();
    let out = crate::closed_form::solve(&rgb, band.width, band.height, &band.trimap(), &band.asked, &band.matte.data, PULL);
    Alpha::new(band.width, band.height, out)
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
    const KEEP: usize = 3;
    let chosen = chosen()?;
    type Kept = Vec<(u64, Option<Alpha>)>;
    static KEPT: std::sync::Mutex<Kept> = std::sync::Mutex::new(Vec::new());

    let key = crate::content_hash_bytes(photo.as_raw())
        ^ crate::content_hash(&coarse.data).rotate_left(7)
        ^ ((coarse.width as u64) << 32 | coarse.height as u64).rotate_left(29)
        ^ u64::from(question == Ask::ThisSubject)

        ^ crate::content_hash_bytes(named(Some(chosen), isnet_here()).as_bytes()).rotate_left(13);
    let found = KEPT.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).iter().find(|(at, _)| *at == key).map(|(_, answer)| answer.clone());
    if let Some(answer) = found {
        return answer;
    }
    let answer = asked_about_now(photo, coarse, question, chosen);
    let mut kept = KEPT.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
    if kept.len() >= KEEP {
        kept.remove(0);
    }
    kept.push((key, answer.clone()));
    answer
}

fn asked_about_now(photo: &RgbImage, coarse: &Alpha, question: Ask, chosen: Subject) -> Option<Alpha> {
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

    let mut subject = chosen;
    let (mattes, edge) = loop {
        let Some(plan) = load(subject) else {
            subject = after_declined(subject, isnet_here(), room())?;
            continue;
        };
        let edge = plan.side().unwrap_or(EDGE);
        let mut mattes = Vec::with_capacity(pieces.len());
        let mut strongest = 0.0f32;
        for piece in &pieces {
            let (answer, confidence) = ask(&plan, subject, photo, *piece, edge)?;
            strongest = strongest.max(confidence);
            mattes.push(answer);
        }
        if strongest >= CONFIDENT {
            log::info!("subject: answered by {}", subject.file());
            break (mattes, edge);
        }
        let next = after_declined(subject, isnet_here(), room());
        log::info!("subject: {} found none, {}", subject.file(), next.map_or("nothing else to ask", Subject::file));
        subject = next?;
    };

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
                sum += share * sample(matte, edge, u, v);
                weight += share;
            }
            let value = match weight > 0.0 {
                true => (sum / weight).clamp(0.0, 1.0),
                false => 0.0,
            };
            out.data[y * coarse.width + x] = value;
        }
    }

    for (value, allow) in out.data.iter_mut().zip(&allowed) {
        *value *= allow;
    }
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

fn ask(plan: &Model, subject: Subject, photo: &RgbImage, piece: Piece, edge: usize) -> Option<(Vec<f32>, f32)> {
    let crop = imageops::crop_imm(
        photo,
        piece.x as u32,
        piece.y as u32,
        (piece.width as u32).max(1),
        (piece.height as u32).max(1),
    )
    .to_image();
    let square = imageops::resize(&crop, edge as u32, edge as u32, imageops::FilterType::Lanczos3);

    let mut input = ndarray::Array4::<f32>::zeros((1, 3, edge, edge));
    for (x, y, pixel) in square.enumerate_pixels() {
        for channel in 0..3 {

            let value = pixel[channel] as f32 / 255.0;
            input[[0, channel, y as usize, x as usize]] = match subject {
                Subject::BiRefNet => (value - [0.485, 0.456, 0.406][channel]) / [0.229, 0.224, 0.225][channel],
                Subject::IsNet => value - 0.5,
            };
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
    if matte.shape() != [1, 1, edge, edge] {
        log::warn!("the matting model answered with {:?}", matte.shape());
        return None;
    }

    let answer: Vec<f32> = match subject {
        Subject::BiRefNet => matte.iter().map(|value| 1.0 / (1.0 + (-value).exp())).collect(),
        Subject::IsNet => matte.iter().copied().collect(),
    };
    let strongest = answer.iter().copied().fold(0.0f32, f32::max);
    Some((answer, strongest))
}

fn sample(matte: &[f32], edge: usize, u: f32, v: f32) -> f32 {
    let fx = u.clamp(0.0, 1.0) * (edge - 1) as f32;
    let fy = v.clamp(0.0, 1.0) * (edge - 1) as f32;
    let (x0, y0) = (fx.floor() as usize, fy.floor() as usize);
    let (x1, y1) = ((x0 + 1).min(edge - 1), (y0 + 1).min(edge - 1));
    let (tx, ty) = (fx - x0 as f32, fy - y0 as f32);
    let at = |sx: usize, sy: usize| matte[sy * edge + sx];
    let top = at(x0, y0) * (1.0 - tx) + at(x1, y0) * tx;
    let bottom = at(x0, y1) * (1.0 - tx) + at(x1, y1) * tx;
    top * (1.0 - ty) + bottom * ty
}

fn gate(coarse: &Alpha, slack: f32) -> Vec<f32> {
    let inside = coarse.data.iter().map(|v| if *v >= INSIDE { 1.0 } else { 0.0 }).collect();
    spread(inside, coarse.width, coarse.height, slack)
}

fn spread(inside: Vec<f32>, width: usize, height: usize, radius: f32) -> Vec<f32> {
    let reach = ((radius.max(1.0) / 2.0).round() as usize).max(1);

    let touched = local::blur(&Plane::new(width, height, inside), reach);
    let grown = touched.data.iter().map(|v| (*v > 1e-3) as u8 as f32).collect();
    local::blur(&Plane::new(width, height, grown), reach)
        .data
        .iter()
        .map(|v| {
            let t = v.clamp(0.0, 1.0);
            t * t * (3.0 - 2.0 * t)
        })
        .collect()
}

pub fn subject(photo: &RgbImage, width: usize, height: usize) -> Option<Alpha> {

    let everything = Alpha::new(width, height, vec![1.0; width * height]);
    let found = asked_about(photo, &everything, Ask::WhateverStandsOut)?;
    let largest = keep_subjects(found);

    let covered: f64 =
        largest.data.iter().map(|value| *value as f64).sum::<f64>() / largest.data.len() as f64;

    (0.0005..0.85).contains(&covered).then_some(largest)
}

fn keep_subjects(alpha: Alpha) -> Alpha {
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
    let kept: Vec<bool> = sizes.iter().map(|size| *size as f32 >= sizes[winner] as f32 * SCRAP).collect();

    let mut out = Alpha::new(width, height, vec![0.0; width * height]);
    for index in 0..width * height {
        if label[index] != usize::MAX && kept[label[index]] {
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

    #[test]
    #[ignore]
    fn finer_on_a_frame() {
        let (Ok(path), Ok(out)) = (std::env::var("FRAME"), std::env::var("OUT")) else { return };
        let path = std::path::PathBuf::from(path);
        let out = std::path::PathBuf::from(out);
        let linear = numa_io::raw::decode_linear(&path).unwrap();
        let document = numa_core::document::Document::new(path.display().to_string());
        let working = crate::to_working_space(&document, &linear, &Default::default());
        let frame = crate::apply_stack(&document, &working, 1.0);

        let (w, h) = match frame.width() > frame.height() {
            true => (512usize, 512 * frame.height() as usize / frame.width() as usize),
            false => (512 * frame.width() as usize / frame.height() as usize, 512usize),
        };
        let coarse = subject(&frame, w, h).expect("no subject");
        let write = |name: &str, alpha: &Alpha| {
            let mut image = image::GrayImage::new(alpha.width as u32, alpha.height as u32);
            for (index, value) in alpha.data.iter().enumerate() {
                image.put_pixel(
                    (index % alpha.width) as u32,
                    (index / alpha.width) as u32,
                    image::Luma([(value.clamp(0.0, 1.0) * 255.0) as u8]),
                );
            }
            image.save(out.join(name)).unwrap();
            let solid = alpha.data.iter().filter(|v| **v > 0.9).count();
            let edge = alpha.data.iter().filter(|v| (0.1..0.9).contains(*v)).count();
            println!("{name}: {solid} vol, {edge} randcellen van {}", alpha.data.len());
        };
        write("coarse.png", &coarse);
        match finer(&frame, &coarse) {
            Some(closer) => write("finer.png", &closer),
            None => println!("finer gaf niets terug"),
        }
    }

    #[test]
    #[ignore]
    fn subject_timed() {
        let Ok(list) = std::env::var("PHOTOS") else { return };
        if let Ok(dir) = std::env::var("GPU_MODELS") {
            numa_infer::enable_gpu(std::path::Path::new(&dir), &std::env::temp_dir().join("numa-subject-guard"));
        }
        let started = std::time::Instant::now();
        let which = chosen().expect("a subject model");
        let plan = load(which).expect("loads");
        println!("ready in {:.1?}, {}, on the card {}", started.elapsed(), which.file(), plan.on_card());
        for path in std::env::split_paths(&list) {
            let photo = image::open(&path).unwrap().to_rgb8();
            let started = std::time::Instant::now();
            let found = subject(&photo, (photo.width() / 4) as usize, (photo.height() / 4) as usize);
            let covered = found.map(|alpha| alpha.data.iter().sum::<f32>() / alpha.data.len() as f32);
            println!("{}: {covered:?} in {:.0?}", path.display(), started.elapsed());
        }
    }

    #[test]
    #[ignore]
    fn refine_edge_candidates() {
        let (Ok(frames), Ok(method)) = (std::env::var("FRAMES"), std::env::var("METHOD")) else { return };
        let plan = (method == "vitmatte")
            .then(|| Model::load(std::path::Path::new(&std::env::var("MODEL").expect("MODEL"))).expect("loads"));
        for path in frames.split(':') {
            let frame = image::open(path).unwrap().to_rgb8();
            let started = std::time::Instant::now();
            let answer = match method.as_str() {
                "coarse" => subject(&frame, frame.width() as usize, frame.height() as usize).expect("a subject"),
                _ => {
                    let grey = image::open(path.replace("-frame.png", "-coarse.png")).unwrap().to_luma8();
                    let coarse = Alpha::new(grey.width() as usize, grey.height() as usize, grey.pixels().map(|p| p[0] as f32 / 255.0).collect());
                    let band = Band::of(&frame, &coarse).expect("a band");
                    match &plan {
                        Some(plan) => by_vitmatte(plan, &band).expect("an answer"),
                        None => by_closed_form(&band),
                    }
                }
            };
            println!("{path}: {method} {:.2?}", started.elapsed());
            let mut image = image::GrayImage::new(answer.width as u32, answer.height as u32);
            for (pixel, value) in image.pixels_mut().zip(&answer.data) {
                *pixel = image::Luma([(value.clamp(0.0, 1.0) * 255.0).round() as u8]);
            }
            image.save(path.replace("-frame.png", &format!("-{method}.png"))).unwrap();
        }
    }

    #[test]
    fn finer_finds_strands_in_the_band() {
        let (side, radius) = (512usize, 150.0f32);
        let centre = side as f32 / 2.0;
        let strand = |x: f32, y: f32| {
            let (dx, dy) = (x - centre, y - centre);
            let (distance, angle) = ((dx * dx + dy * dy).sqrt(), dy.atan2(dx));

            let nearest = (angle / std::f32::consts::TAU * 60.0).round() * std::f32::consts::TAU / 60.0;
            distance < radius + 60.0 && (angle - nearest).abs() * distance < 0.75
        };
        let frame = RgbImage::from_fn(side as u32, side as u32, |x, y| {
            let (x, y) = (x as f32 + 0.5, y as f32 + 0.5);
            let inside = (x - centre).hypot(y - centre) < radius;
            match inside || strand(x, y) {
                true => image::Rgb([60, 40, 25]),
                false => image::Rgb([110, 160, (220.0 - y / 8.0) as u8]),
            }
        });
        let coarse = Alpha::new(side, side, (0..side * side)
            .map(|i| (((i % side) as f32 + 0.5 - centre).hypot((i / side) as f32 + 0.5 - centre) < radius) as u8 as f32)
            .collect());
        let band = Band::of(&frame, &coarse).expect("a band");
        let mut answers = vec![("closed form", by_closed_form(&band))];
        match vitmatte() {
            Some(plan) => answers.push(("ViTMatte", by_vitmatte(&plan, &band).expect("an answer"))),
            None => eprintln!("no {VITMATTE}: only the closed form"),
        }
        for (way, finer) in answers {

            let (mut strands, mut sky) = (Vec::new(), Vec::new());
            for i in 0..side * side {
                let (x, y) = ((i % side) as f32 + 0.5, (i / side) as f32 + 0.5);
                let distance = (x - centre).hypot(y - centre);
                if distance > radius + 60.0 || distance < radius - 40.0 {
                    assert_eq!(finer.data[i], coarse.data[i], "{way} moved outside the band at {x},{y}");
                } else if distance > radius + 1.0 && finer.data[i] != coarse.data[i] {
                    match strand(x, y) {
                        true => strands.push(finer.data[i]),
                        false => sky.push(finer.data[i]),
                    }
                }
            }
            let mean = |v: &[f32]| v.iter().sum::<f32>() / v.len().max(1) as f32;
            assert!(!strands.is_empty() && !sky.is_empty(), "{way}: the band did not reach the strands");

            println!("{way}: strands {} against sky {}", mean(&strands), mean(&sky));
            assert!(mean(&strands) > 0.05 && mean(&strands) > 10.0 * mean(&sky), "{way}: strands {} against sky {}", mean(&strands), mean(&sky));
        }
    }

    #[test]
    fn birefnet_where_it_is_here_and_fits() {
        assert_eq!(choose(true, true, None), Some(Subject::BiRefNet));
        assert_eq!(choose(true, false, Some(ROOM)), Some(Subject::BiRefNet));
    }

    #[test]
    fn nothing_when_memory_is_short() {

        assert_eq!(choose(true, true, Some(ROOM - 1)), None);
        assert_eq!(choose(true, false, Some(ROOM - 1)), None);

        assert_eq!(choose(false, true, Some(ISNET_ROOM)), Some(Subject::IsNet));
        assert_eq!(choose(false, true, Some(ISNET_ROOM - 1)), None);
    }

    #[test]
    fn isnet_when_birefnet_is_missing() {
        assert_eq!(choose(false, true, Some(u64::MAX)), Some(Subject::IsNet));
        assert_eq!(choose(false, true, None), Some(Subject::IsNet));
        assert_eq!(choose(false, false, None), None);
    }

    #[test]
    fn isnet_when_birefnet_declines() {
        assert_eq!(after_declined(Subject::BiRefNet, true, None), Some(Subject::IsNet));
        assert_eq!(after_declined(Subject::BiRefNet, true, Some(ISNET_ROOM)), Some(Subject::IsNet));
        assert_eq!(after_declined(Subject::BiRefNet, true, Some(ISNET_ROOM - 1)), None);
        assert_eq!(after_declined(Subject::BiRefNet, false, None), None);
        assert_eq!(after_declined(Subject::IsNet, true, None), None);
    }

    #[test]
    fn isnet_is_never_asked_where_numa_is_sold() {
        let isnet = isnet_kept(false, true);
        assert!(!isnet);
        assert_eq!(choose(true, isnet, Some(ROOM - 1)), None);
        assert_eq!(choose(false, isnet, None), None);
        assert_eq!(after_declined(Subject::BiRefNet, isnet, None), None);
        assert_eq!(named(Some(Subject::BiRefNet), isnet), BIREFNET);

        assert!(isnet_kept(true, true));
        assert_eq!(ISNET_ALLOWED, !cfg!(target_vendor = "apple"));
    }

    #[test]
    fn the_key_names_who_can_answer() {
        assert_eq!(named(Some(Subject::BiRefNet), false), BIREFNET);
        assert_eq!(named(Some(Subject::BiRefNet), true), format!("{BIREFNET}+isnet.onnx"));
        assert_eq!(named(Some(Subject::IsNet), true), "isnet.onnx");
        assert_eq!(named(None, false), "");
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
    fn the_gate_keeps_what_is_narrower_than_its_slack() {
        let mut alpha = box_alpha(128, 128, (20, 60, 100, 120));
        for y in 30..60 {
            for x in 58..62 {
                alpha.data[y * 128 + x] = 1.0;
            }
        }
        let g = gate(&alpha, 32.0);
        assert!(g[31 * 128 + 60] > 0.99, "the top of a four-cell column: {}", g[31 * 128 + 60]);
        assert!(g[25 * 128 + 60] > 0.5, "just above it, inside the slack: {}", g[25 * 128 + 60]);
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
