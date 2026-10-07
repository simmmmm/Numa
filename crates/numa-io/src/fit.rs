use std::path::{Path, PathBuf};
use std::sync::Arc;

use numa_core::color::CameraProfile;
use numa_core::document::Document;
use numa_core::image::LinearImage;
use numa_core::profile::{
    fit_hue_sat_map, multiply, multiply_matrix, rgb_to_hsv, DngProfile, HsvSample, Matrix3, Rendering,
    SRGB_TO_XYZ_D50, XYZ_D50_TO_PROPHOTO,
};
use numa_core::tone::scene_value_for;

use crate::style::{self, Style};
use crate::{dcp, raw};

pub const STOPPED: &str = "stopped";

pub struct Frame {
    pub name: String,
    pub proxy: LinearImage,
    pub camera: image::RgbImage,
    pub samples: Vec<HsvSample>,

    pub taken: Option<i64>,
}

pub fn srgb_encode(v: f32) -> f32 {
    let v = v.clamp(0.0, 1.0);
    if v <= 0.003_130_8 { v * 12.92 } else { 1.055 * v.powf(1.0 / 2.4) - 0.055 }
}

pub fn srgb_decode(v: f32) -> f32 {
    if v <= 0.040_45 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
}

pub fn luminance(p: [f32; 3]) -> f32 {
    0.2126 * p[0] + 0.7152 * p[1] + 0.0722 * p[2]
}

pub fn forward_profile(camera: &CameraProfile, name: &str, body: &str) -> DngProfile {
    let forward: Matrix3 = multiply_matrix(&SRGB_TO_XYZ_D50, &camera.cam_to_srgb);
    DngProfile {
        name: name.to_string(),
        camera: Some(body.to_string()),
        color_matrix: [Some(camera.xyz_to_cam), None],
        forward_matrix: [Some(forward), None],

        illuminant: [Some(21), None],
        hue_sat_map: None,
        look_table: None,
        tone_curve: None,
    }
}

pub fn load(path: &Path) -> Result<Frame, String> {
    let name = path.file_name().map(|name| name.to_string_lossy().to_string()).unwrap_or_default();

    let sidecar = path.with_file_name(format!("{name}.jpg"));
    let preview = match image::open(&sidecar) {
        Ok(image) => image,
        Err(_) => match raw::embedded_preview(path) {
            Ok(Some(image)) => image,
            _ => return Err("no camera JPEG".into()),
        },
    };
    let linear = raw::decode_linear(path).map_err(|_| "no decode".to_string())?;
    let proxy = linear.downscaled(900).ok_or("no decode")?;
    drop(linear);
    let jpeg = preview.to_rgb8();
    drop(preview);
    if proxy.profile.is_none() {
        return Err("no camera matrix".into());
    }

    let spread = chroma_spread(&jpeg);
    if spread < 0.03 {
        return Err(format!("monochrome JPEG (spread {spread:.3})"));
    }
    let taken = raw::summary(path).and_then(|s| s.taken).and_then(|t| minutes(&t));
    let frame_of = |proxy: LinearImage, jpeg: &image::RgbImage| {
        let samples = samples(&proxy, jpeg);
        let camera = image::imageops::resize(jpeg, proxy.width, proxy.height, image::imageops::FilterType::Triangle);
        Frame { name: name.clone(), proxy, camera, samples, taken }
    };
    let mut frame = frame_of(proxy.clone(), &jpeg);
    if frame.samples.is_empty() {
        return Err("no pairs (the JPEG's shape is not the frame's, or nothing unclipped)".into());
    }
    let mut matrix = chroma_error(&[&frame], |_| None);

    if adobe_rgb(path) {
        let mut converted = jpeg.clone();
        adobe_to_srgb(&mut converted);
        let other = frame_of(proxy, &converted);
        let error = chroma_error(&[&other], |_| None);
        log::info!("{name}: Adobe RGB set; matrix {matrix:.4} as sRGB, {error:.4} as Adobe RGB");
        if error < matrix {
            (frame, matrix) = (other, error);
        }
    }

    if matrix > 0.3 {
        return Err(format!("decode and JPEG disagree (matrix {matrix:.3})"));
    }
    Ok(frame)
}

fn chroma_spread(jpeg: &image::RgbImage) -> f64 {
    let small = image::imageops::thumbnail(jpeg, 64, 64);
    let (mut n, mut sum, mut squares) = (0.0f64, [0.0f64; 3], [0.0f64; 3]);
    for p in small.pixels() {
        let l = luminance(p.0.map(f32::from)) as f64;
        if l < 20.0 {
            continue;
        }
        for c in 0..3 {
            let u = p[c] as f64 / l;
            sum[c] += u;
            squares[c] += u * u;
        }
        n += 1.0;
    }
    (0..3).map(|c| (squares[c] / n.max(1.0) - (sum[c] / n.max(1.0)).powi(2)).max(0.0).sqrt()).fold(0.0, f64::max)
}

fn adobe_rgb(path: &Path) -> bool {
    let Ok(out) = std::process::Command::new("exiftool")
        .args(["-s3", "-ExifIFD:ColorSpace", "-MakerNotes:ColorSpace", "-InteropIndex"])
        .arg(path)
        .output()
    else {
        return false;
    };
    let text = String::from_utf8_lossy(&out.stdout);
    ["Uncalibrated", "Adobe", "R03"].iter().any(|mark| text.contains(mark))
}

fn adobe_to_srgb(jpeg: &mut image::RgbImage) {
    const TO_SRGB: Matrix3 = [[1.3983, -0.3983, 0.0], [0.0, 1.0, 0.0], [0.0, -0.0429, 1.0429]];
    let decode: Vec<f32> = (0..256).map(|v| (v as f32 / 255.0).powf(563.0 / 256.0)).collect();
    for p in jpeg.pixels_mut() {
        let linear = multiply(&TO_SRGB, p.0.map(|v| decode[v as usize]));
        p.0 = linear.map(|v| (srgb_encode(v) * 255.0).round() as u8);
    }
}

fn minutes(taken: &str) -> Option<i64> {
    let n: Vec<i64> = taken.split(|c: char| !c.is_ascii_digit()).filter(|s| !s.is_empty()).map(|s| s.parse().ok()).collect::<Option<_>>()?;
    let [year, month, day, hour, minute, ..] = n[..] else { return None };
    Some((((year * 12 + month) * 31 + day) * 24 + hour) * 60 + minute)
}

fn samples(proxy: &LinearImage, jpeg: &image::RgbImage) -> Vec<HsvSample> {
    let Some(camera) = proxy.profile else { return Vec::new() };
    let ours = proxy.downscaled(300).unwrap_or_else(|| proxy.clone());

    let linear: Vec<f32> = jpeg.as_raw().iter().map(|b| srgb_decode(*b as f32 / 255.0)).collect();
    let theirs = LinearImage::new(jpeg.width(), jpeg.height(), linear).downscaled(300);
    let Some(theirs) = theirs else { return Vec::new() };
    if (ours.width, ours.height) != (theirs.width, theirs.height) {
        return Vec::new();
    }

    let balance = camera.as_shot_white_balance();
    let multipliers = camera.multipliers(balance);
    let to_prophoto = Rendering::new(multiply_matrix(&SRGB_TO_XYZ_D50, &camera.cam_to_srgb), None, None).to_prophoto;
    let srgb_to_prophoto = multiply_matrix(&XYZ_D50_TO_PROPHOTO, &SRGB_TO_XYZ_D50);

    let (w, h) = (ours.width as usize, ours.height as usize);
    let display = |x: usize, y: usize| -> [f32; 3] {
        let i = (y * w + x) * 3;
        [srgb_encode(theirs.data[i]), srgb_encode(theirs.data[i + 1]), srgb_encode(theirs.data[i + 2])]
    };
    let border = w.max(h) * 8 / 100;

    let mut pairs = Vec::new();
    for y in border..h - border {
        for x in border..w - border {
            let i = (y * w + x) * 3;
            let c = [ours.data[i], ours.data[i + 1], ours.data[i + 2]];
            if proxy.clip.is_some_and(|clip| c.iter().any(|v| *v >= clip * 0.9)) {
                continue;
            }
            let d = display(x, y);
            if d.iter().any(|v| *v > 0.96 || *v < 0.02) || luminance(d) < 0.06 {
                continue;
            }

            let here = luminance(d);
            let edge = [(x - 1, y), (x + 1, y), (x, y - 1), (x, y + 1)]
                .iter()
                .any(|(nx, ny)| (luminance(display(*nx, *ny)) - here).abs() > 0.06);
            if edge {
                continue;
            }

            let balanced = [c[0] * multipliers[0], c[1] * multipliers[1], c[2] * multipliers[2]];
            let from = rgb_to_hsv(multiply(&to_prophoto, balanced));
            let target = d.map(scene_value_for);
            let to = rgb_to_hsv(multiply(&srgb_to_prophoto, target));
            if from[2] > 1e-4 {
                pairs.push(HsvSample { from, to });
            }
        }
    }

    let mut ratios: Vec<f32> = pairs.iter().map(|p| p.to[2] / p.from[2]).collect();
    if ratios.is_empty() {
        return pairs;
    }
    ratios.sort_by(f32::total_cmp);
    let gain = ratios[ratios.len() / 2];
    for pair in &mut pairs {
        pair.to[2] /= gain;
    }
    pairs
}

pub fn chroma_error(frames: &[&Frame], rendering: impl Fn(&Frame) -> Option<Arc<DngProfile>>) -> f64 {
    let mut sum = 0.0;
    for frame in frames {
        let mut source = frame.proxy.clone();
        source.rendering = rendering(frame);
        let ours = numa_render::develop(&Document::new(frame.name.clone()), &source, &Default::default());
        let unit = |p: &image::Rgb<u8>| {
            let l = (0.2126 * p[0] as f32 + 0.7152 * p[1] as f32 + 0.0722 * p[2] as f32).max(1.0);
            [p[0] as f32 / l, p[1] as f32 / l, p[2] as f32 / l]
        };
        let mut total = 0.0f64;
        for (a, b) in ours.pixels().zip(frame.camera.pixels()) {
            let (x, y) = (unit(a), unit(b));
            total += (0..3).map(|c| (x[c] - y[c]).abs() as f64).sum::<f64>();
        }
        sum += total / (ours.pixels().len() * 3) as f64;
    }
    sum / frames.len().max(1) as f64
}

pub fn tidy(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub const FRAMES: usize = 24;
pub const FEWEST: usize = 8;

pub struct Camera {
    pub make: String,
    pub model: String,

    pub like: PathBuf,

    pub asked: usize,
}

pub fn cameras(paths: &[PathBuf]) -> Vec<Camera> {
    let mut formats: std::collections::BTreeMap<Option<std::ffi::OsString>, Vec<&PathBuf>> = Default::default();
    for path in paths.iter().filter(|path| raw::is_raw(path)) {
        formats.entry(path.extension().map(|ext| ext.to_ascii_lowercase())).or_default().push(path);
    }
    let mut found: Vec<Camera> = Vec::new();
    for files in formats.values() {
        let mut order: Vec<usize> = (0..files.len()).collect();
        order.sort_by_key(|i| (*i as u32).reverse_bits());
        for i in order.into_iter().take(200) {
            let Some(summary) = raw::summary(files[i]) else { continue };
            let (make, model) = (tidy(&summary.make), tidy(&summary.model));
            match found.iter_mut().find(|camera| camera.make == make && camera.model == model) {
                Some(camera) => camera.asked += 1,
                None => found.push(Camera { make, model, like: files[i].clone(), asked: 1 }),
            }
        }
    }
    found.sort_by(|a, b| b.asked.cmp(&a.asked));
    found
}

pub struct Found {

    pub picked: Vec<PathBuf>,
    pub neutral: usize,

    pub days: usize,

    pub other: usize,
    pub commonest: Option<String>,
}

pub fn find_frames(paths: &[PathBuf], like: &Path, progress: &dyn Fn(usize, usize) -> bool) -> Result<Found, String> {
    let want = raw::summary(like).ok_or("Numa cannot read which camera took this photograph")?;
    let readable = style::of(like) != Style::Unknown;
    let extension = |path: &Path| path.extension().map(|ext| ext.to_ascii_lowercase());
    let same_format: Vec<&PathBuf> = paths.iter().filter(|path| extension(path) == extension(like)).collect();
    let mut order: Vec<usize> = (0..same_format.len()).collect();
    order.sort_by_key(|i| (*i as u32).reverse_bits());

    let mut neutral: Vec<(&PathBuf, Option<i64>)> = Vec::new();
    let mut others: std::collections::HashMap<String, usize> = Default::default();
    for (asked, i) in order.into_iter().enumerate() {
        if neutral.len() >= 4 * FRAMES {
            break;
        }
        if !progress(asked, same_format.len()) {
            return Err(STOPPED.into());
        }
        let path = same_format[i];
        let style = style::of(path);
        let usable = style == Style::Neutral || (!readable && style == Style::Unknown);

        if !usable && !matches!(style, Style::Other(_)) {
            continue;
        }
        let Some(summary) = raw::summary(path).filter(|s| s.make == want.make && s.model == want.model) else { continue };
        match style {
            Style::Other(name) => *others.entry(name).or_default() += 1,
            _ => neutral.push((path, summary.taken.and_then(|taken| minutes(&taken)))),
        }
    }

    neutral.sort_by_key(|(_, taken)| *taken);
    let days = neutral.iter().filter_map(|(_, taken)| taken.map(|t| t / (24 * 60))).collect::<std::collections::HashSet<_>>().len();
    let count = FRAMES.min(neutral.len());
    let step = neutral.len() as f64 / count.max(1) as f64;
    let picked = (0..count).map(|i| neutral[(i as f64 * step) as usize].0.clone()).collect();
    let commonest = others.iter().max_by_key(|(_, n)| **n).map(|(name, _)| name.clone());
    Ok(Found { picked, neutral: neutral.len(), days, other: others.values().sum(), commonest })
}

pub struct Made {
    pub name: String,

    pub file: Option<PathBuf>,
    pub fitted_on: usize,
    pub held_out: usize,

    pub before: f64,
    pub fitted: f64,
}

pub fn own_profile(paths: &[PathBuf], progress: &dyn Fn(usize, usize) -> bool) -> Result<Made, String> {
    let summary = paths.iter().find_map(|path| raw::summary(path)).ok_or("no raw the reader knows")?;
    let model = tidy(&summary.model);
    let body = format!("{} {model}", tidy(&summary.make));
    let name = format!("Your {model}");

    let candidates: Vec<([usize; 3], f32)> =
        [[36, 8, 4], [18, 6, 3]].into_iter().flat_map(|d| [3.0f32, 10.0, 30.0].map(|s| (d, s))).collect();
    let total = paths.len() + candidates.len() + 1;
    let mut frames = Vec::new();
    for (i, path) in paths.iter().enumerate() {
        if !progress(i, total) {
            return Err(STOPPED.into());
        }
        match load(path) {
            Ok(frame) => frames.push(frame),
            Err(why) => log::info!("{}: not fitted from, {why}", path.display()),
        }
    }
    if frames.len() < FEWEST {
        return Err(format!("Only {} of {} photographs could be read for colour; a profile takes {FEWEST}", frames.len(), paths.len()));
    }

    frames.sort_by_key(|frame| frame.taken);
    let mut sessions: Vec<Vec<Frame>> = Vec::new();
    for frame in frames {
        match sessions.last_mut() {
            Some(session) if session.last().and_then(|f| f.taken).zip(frame.taken).is_none_or(|(a, b)| b - a <= 30) => {
                session.push(frame)
            }
            _ => sessions.push(vec![frame]),
        }
    }
    let count: usize = sessions.iter().map(Vec::len).sum();
    sessions.sort_by_key(Vec::len);
    let (mut held, mut fitting) = (Vec::new(), Vec::new());
    for session in sessions {
        if held.len() < count / 4 && held.len() + session.len() <= count / 2 {
            held.extend(session);
        } else {
            fitting.extend(session);
        }
    }
    if held.is_empty() {
        return Err("These photographs are all from one session; photographs from another day are needed to test a profile".into());
    }
    let held: Vec<&Frame> = held.iter().collect();
    let fitting: Vec<&Frame> = fitting.iter().collect();

    let base = |frame: &Frame| forward_profile(frame.proxy.profile.as_ref().expect("load keeps only frames with a matrix"), &name, &body);
    let pooled = |set: &[&Frame]| set.iter().flat_map(|frame| frame.samples.iter().copied()).collect::<Vec<_>>();

    let every = |keep: fn(usize) -> bool| -> Vec<&Frame> {
        fitting.iter().enumerate().filter(|(i, _)| keep(*i)).map(|(_, frame)| *frame).collect()
    };
    let (validation, training) = (every(|i| i % 4 == 3), every(|i| i % 4 != 3));
    let mut best = (f64::MAX, [18, 6, 3], 30.0f32);
    for (step, (divisions, smoothness)) in candidates.into_iter().enumerate() {
        if !progress(paths.len() + step, total) {
            return Err(STOPPED.into());
        }
        let table = fit_hue_sat_map(&pooled(&training), divisions, smoothness);
        let error = chroma_error(&validation, |frame| {
            Some(Arc::new(DngProfile { hue_sat_map: Some(table.clone()), ..base(frame) }))
        });
        if error < best.0 {
            best = (error, divisions, smoothness);
        }
    }
    let profile = DngProfile { hue_sat_map: Some(fit_hue_sat_map(&pooled(&fitting), best.1, best.2)), ..base(fitting[0]) };

    let scratch = std::env::temp_dir().join(format!("numa-own-profile-{}.dcp", std::process::id()));
    dcp::write(&scratch, &profile).map_err(|err| err.to_string())?;
    let read_back = dcp::read(&scratch).map(Arc::new);
    let _ = std::fs::remove_file(&scratch);
    let read_back = read_back?;

    let before = chroma_error(&held, |frame| frame.proxy.rendering.clone());
    let fitted = chroma_error(&held, |_| Some(read_back.clone()));
    progress(total, total);
    let file = if fitted < before {
        let dir = dcp::profiles_dir().ok_or("no folder for your own profiles")?;
        std::fs::create_dir_all(&dir).map_err(|err| err.to_string())?;
        let file = dir.join(format!("{name}.dcp"));
        dcp::write(&file, &profile).map_err(|err| err.to_string())?;
        dcp::forget();
        Some(file)
    } else {
        None
    };
    Ok(Made { name, file, fitted_on: fitting.len(), held_out: held.len(), before, fitted })
}
