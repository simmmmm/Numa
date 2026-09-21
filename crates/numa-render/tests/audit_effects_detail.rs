use std::f32::consts::TAU;
use std::ops::Range;

use image::RgbImage;
use numa_core::document::{Basic, Document};
use numa_core::grading::{self, Grading};
use numa_core::image::LinearImage;
use numa_core::tone;
use numa_render::apply_stack;

const W: u32 = 128;
const H: u32 = 96;

fn render(basic: Basic, frame: &LinearImage) -> RgbImage {
    let mut document = Document::new(String::new());
    document.set_basic(basic);
    apply_stack(&document, frame, 1.0)
}

fn untouched(frame: &LinearImage) -> RgbImage {
    apply_stack(&Document::new(String::new()), frame, 1.0)
}

fn graded(set: impl FnOnce(&mut Grading)) -> RgbImage {
    let mut grade = Grading::default();
    set(&mut grade);
    let mut document = Document::new(String::new());
    document.set_grading(grade);
    apply_stack(&document, &ramp(), 1.0)
}

fn frame(width: u32, height: u32, pixel: impl Fn(u32, u32) -> [f32; 3]) -> LinearImage {
    LinearImage::new(width, height, (0..width * height).flat_map(|i| pixel(i % width, i / width)).collect())
}

fn grey(value: f32) -> LinearImage {
    frame(W, H, |_, _| [value; 3])
}

fn noise(x: u32, y: u32, salt: u64) -> f32 {
    let mut h = ((x as u64) << 32 | y as u64) ^ salt.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    h = (h ^ (h >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    h = (h ^ (h >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    h ^= h >> 31;
    (h >> 40) as f32 / (1u64 << 23) as f32 - 1.0
}

fn grating(period: f32, stops: f32) -> LinearImage {
    frame(W, H, |x, _| [0.18 * (stops * (x as f32 * TAU / period).sin()).exp2(); 3])
}

fn step_edge(dark: f32, bright: f32) -> LinearImage {
    frame(W, H, |x, _| [if x < W / 2 { dark } else { bright }; 3])
}

fn ramp() -> LinearImage {
    frame(W, 8, |x, _| [tone::scene_value_for(0.02 + 0.96 * x as f32 / (W - 1) as f32); 3])
}

const SHADOWS: Range<u32> = 4..20;
const MIDTONES: Range<u32> = 56..72;
const HIGHLIGHTS: Range<u32> = 108..124;

fn luma(image: &RgbImage, x: u32, y: u32) -> f32 {
    let [r, g, b] = image.get_pixel(x, y).0;
    0.2126 * r as f32 + 0.7152 * g as f32 + 0.0722 * b as f32
}

fn lumas(image: &RgbImage, xs: Range<u32>, ys: Range<u32>) -> Vec<f32> {
    ys.flat_map(|y| xs.clone().map(move |x| luma(image, x, y))).collect()
}

fn mean(values: &[f32]) -> f32 {
    values.iter().sum::<f32>() / values.len() as f32
}

fn spread(values: &[f32]) -> f32 {
    let centre = mean(values);
    (values.iter().map(|v| (v - centre).powi(2)).sum::<f32>() / values.len() as f32).sqrt()
}

fn colour(image: &RgbImage, xs: Range<u32>, ys: Range<u32>) -> [f32; 3] {
    let count = (xs.len() * ys.len()) as f32;
    let mut sum = [0.0f32; 3];
    for y in ys {
        for x in xs.clone() {
            for (total, value) in sum.iter_mut().zip(image.get_pixel(x, y).0) {
                *total += value as f32;
            }
        }
    }
    sum.map(|total| total / count)
}

fn tint(image: &RgbImage, xs: Range<u32>) -> f32 {
    let [r, g, b] = colour(image, xs, 0..8);
    r.max(g).max(b) - r.min(g).min(b)
}

fn level(image: &RgbImage, xs: Range<u32>) -> f32 {
    mean(&lumas(image, xs, 0..8))
}

fn worst_in(a: &RgbImage, b: &RgbImage, xs: Range<u32>, ys: Range<u32>) -> u8 {
    ys.flat_map(|y| xs.clone().map(move |x| (x, y)))
        .flat_map(|(x, y)| a.get_pixel(x, y).0.into_iter().zip(b.get_pixel(x, y).0))
        .map(|(p, q)| p.abs_diff(q))
        .max()
        .unwrap_or(0)
}

fn worst(a: &RgbImage, b: &RgbImage) -> u8 {
    a.as_raw().iter().zip(b.as_raw()).map(|(p, q)| p.abs_diff(*q)).max().unwrap_or(0)
}

fn corners(image: &RgbImage) -> f32 {
    let (w, h) = image.dimensions();
    let areas = [(0..4, 0..4), (w - 4..w, 0..4), (0..4, h - 4..h), (w - 4..w, h - 4..h)];
    mean(&areas.into_iter().flat_map(|(xs, ys)| lumas(image, xs, ys)).collect::<Vec<_>>())
}

fn centre(image: &RgbImage) -> f32 {
    let (w, h) = image.dimensions();
    mean(&lumas(image, w / 2 - 4..w / 2 + 4, h / 2 - 4..h / 2 + 4))
}

fn columns(image: &RgbImage) -> Vec<f32> {
    let (w, h) = image.dimensions();
    (0..w).map(|x| mean(&lumas(image, x..x + 1, h / 4..h * 3 / 4))).collect()
}

fn steepest(image: &RgbImage, xs: Range<u32>) -> f32 {
    let columns = columns(image);
    columns[xs.start as usize..xs.end as usize].windows(2).map(|pair| (pair[1] - pair[0]).abs()).fold(0.0, f32::max)
}

fn acutance(image: &RgbImage) -> f32 {
    columns(image).windows(2).map(|pair| (pair[1] - pair[0]).powi(2)).sum()
}

fn light(image: &RgbImage, xs: Range<u32>, ys: Range<u32>) -> f32 {
    let (weights, pixels) = ([0.2126, 0.7152, 0.0722], colour(image, xs, ys));
    pixels.iter().zip(weights).map(|(code, weight)| weight * tone::scene_value_for(code / 255.0)).sum()
}

fn coherence(image: &RgbImage) -> f32 {
    let (w, h) = image.dimensions();
    let values = lumas(image, 0..w, 0..h);
    let centre = mean(&values);
    let (mut together, mut alone) = (0.0, 0.0);
    for row in values.chunks(w as usize) {
        for pair in row.windows(2) {
            together += (pair[0] - centre) * (pair[1] - centre);
            alone += (pair[0] - centre).powi(2);
        }
    }
    together / alone
}

fn assert_rises(what: &str, points: &[(f32, f32)]) {
    println!("{what}: {}", points.iter().map(|(at, m)| format!("{at}→{m:.2}")).collect::<Vec<_>>().join("  "));
    let total = points[points.len() - 1].1 - points[0].1;
    assert!(total > 0.0, "{what}: no rise across the whole travel");
    for pair in points.windows(2) {
        let ((from, a), (to, b)) = (pair[0], pair[1]);
        assert!(b - a >= total * 0.05, "{what}: {from} → {to} moves {:.3} of a total {total:.3}", b - a);
    }
}

fn assert_falls(what: &str, points: &[(f32, f32)]) {
    assert_rises(what, &points.iter().map(|(at, m)| (*at, -m)).collect::<Vec<_>>());
}

fn assert_no_jump(what: &str, neutral: &RgbImage, one_step: &RgbImage) {
    let moved = worst(neutral, one_step);
    assert!(moved <= 3, "{what}: one step off neutral moved a pixel by {moved} codes");
}

fn assert_usable(what: &str, neutral: &RgbImage, image: &RgbImage) {
    let holes = image.enumerate_pixels().filter(|(x, y, _)| luma(neutral, *x, *y) >= 10.0 && luma(image, *x, *y) < 0.5).count();
    assert_eq!(holes, 0, "{what}: {holes} pixels went black that were not");
    let clipped = image.pixels().filter(|p| p.0 == [0; 3] || p.0 == [255; 3]).count();
    assert!(clipped * 2 < image.as_raw().len() / 3, "{what}: {clipped} pixels clipped");
}

fn assert_home(what: &str, home: f32, away: &[f32]) {
    println!("{what}: home {home:.2}, away {away:.2?}");
    assert!(home.abs() >= 4.0, "{what}: {home:.2} codes at home is nothing");
    for elsewhere in away {
        assert!(elsewhere.abs() <= home.abs() * 0.25, "{what}: {elsewhere:.2} codes away against {home:.2} at home");
    }
}

#[test]
fn clarity_deepens_a_soft_grating_and_negative_softens_it_while_flat_grey_stays() {
    let grating = grating(16.0, 0.15);
    let flat = grey(0.18);
    let at = |clarity: f32, frame: &LinearImage| render(Basic::with(|b| b.presence.clarity = clarity), frame);
    let depth = |image: &RgbImage| spread(&lumas(image, 16..W - 16, 16..H - 16));

    assert_eq!(at(0.0, &grating), untouched(&grating));
    assert_no_jump("clarity +1", &untouched(&grating), &at(1.0, &grating));
    assert_rises("clarity: grating depth", &[-100.0, -50.0, 0.0, 50.0, 100.0].map(|v| (v, depth(&at(v, &grating)))));
    for end in [-100.0, 100.0] {
        assert_usable("clarity", &untouched(&grating), &at(end, &grating));
        assert!(worst(&at(end, &flat), &untouched(&flat)) <= 1, "clarity {end} moved a flat field");
    }
}

#[test]
fn texture_works_on_fine_detail_not_clarity_s_scale_and_leaves_flat_grey() {
    let (fine, coarse, flat) = (grating(4.0, 0.15), grating(16.0, 0.15), grey(0.18));
    let at = |texture: f32, frame: &LinearImage| render(Basic::with(|b| b.presence.texture = texture), frame);
    let depth = |image: &RgbImage| spread(&lumas(image, 16..W - 16, 16..H - 16));

    assert_eq!(at(0.0, &fine), untouched(&fine));
    assert_no_jump("texture +1", &untouched(&fine), &at(1.0, &fine));
    assert_rises("texture: fine grating depth", &[-100.0, -50.0, 0.0, 50.0, 100.0].map(|v| (v, depth(&at(v, &fine)))));

    let gain = |frame: &LinearImage| depth(&at(100.0, frame)) / depth(&untouched(frame)) - 1.0;
    let (on_fine, on_coarse) = (gain(&fine), gain(&coarse));
    println!("texture +100: fine depth {on_fine:+.2}, coarse {on_coarse:+.2}");
    assert!(on_coarse.abs() < on_fine / 3.0, "texture reached clarity's scale: {on_coarse:.2} against {on_fine:.2}");

    for end in [-100.0, 100.0] {
        assert_usable("texture", &untouched(&fine), &at(end, &fine));
        assert!(worst(&at(end, &flat), &untouched(&flat)) <= 1, "texture {end} moved a flat field");
    }
}

#[test]
fn dehaze_clears_a_hazy_frame_and_negative_adds_haze_while_a_clear_one_stays() {

    let hazy = frame(W, H, |x, _| {
        let scene = if x < W / 2 { 0.1 } else { 0.5 };
        [scene * 0.5 + 0.3, scene * 0.5 + 0.3, scene * 0.5 + 0.32]
    });

    let clear = frame(W, H, |x, y| match (x < W / 2, y < H / 2) {
        (true, true) => [0.5, 0.002, 0.002],
        (false, true) => [0.002, 0.4, 0.002],
        (true, false) => [0.002, 0.002, 0.6],
        _ => [0.3, 0.3, 0.002],
    });
    let at = |amount: f32, frame: &LinearImage| render(Basic::with(|b| b.effects.dehaze = amount), frame);
    let contrast = |image: &RgbImage| mean(&lumas(image, W * 3 / 4..W - 4, 4..H - 4)) - mean(&lumas(image, 4..W / 4, 4..H - 4));

    assert_eq!(at(0.0, &hazy), untouched(&hazy));
    assert_no_jump("dehaze +1", &untouched(&hazy), &at(1.0, &hazy));
    assert_rises("dehaze: contrast through the haze", &[-100.0, -50.0, 0.0, 50.0, 100.0].map(|v| (v, contrast(&at(v, &hazy)))));
    let dark = |image: &RgbImage| mean(&lumas(image, 4..W / 4, 4..H - 4));
    assert!(dark(&at(100.0, &hazy)) < dark(&untouched(&hazy)) - 5.0, "the dark side should deepen");

    for end in [-100.0, 100.0] {
        assert_usable("dehaze", &untouched(&hazy), &at(end, &hazy));
    }
    let moved = worst(&at(100.0, &clear), &untouched(&clear));
    println!("dehaze +100 on a clear frame: worst {moved} codes");
    assert!(moved <= 2, "dehaze +100 moved a frame with no haze by {moved} codes");
}

fn vignette(set: impl FnOnce(&mut Basic), frame: &LinearImage) -> RgbImage {
    render(Basic::with(set), frame)
}

#[test]
fn vignette_amount_darkens_the_corners_when_negative_and_lightens_them_when_positive_never_the_centre() {
    let flat = grey(0.18);
    let neutral = untouched(&flat);
    let at = |amount: f32| vignette(|b| b.effects.vignette = amount, &flat);

    assert_eq!(at(0.0), neutral);
    assert_no_jump("vignette −1", &neutral, &at(-1.0));
    assert_rises("vignette: corner brightness", &[-100.0, -50.0, 0.0, 50.0, 100.0].map(|v| (v, corners(&at(v)))));
    assert!(corners(&at(-100.0)) < corners(&neutral) - 30.0, "−100 should darken the corners by stops");
    for end in [-100.0, 100.0] {
        let out = at(end);
        assert!((centre(&out) - centre(&neutral)).abs() <= 1.0, "vignette {end} moved the centre");
        assert_usable("vignette", &neutral, &out);
    }
}

#[test]
fn vignette_midpoint_moves_how_far_in_the_darkening_starts() {
    let flat = grey(0.18);
    let neutral = untouched(&flat);
    let at = |midpoint: f32| vignette(|b| { b.effects.vignette = -100.0; b.effects.vignette_midpoint = midpoint }, &flat);

    for midpoint in [0.0, 100.0] {
        let alone = vignette(|b| b.effects.vignette_midpoint = midpoint, &flat);
        assert_eq!(alone, neutral, "midpoint {midpoint} with no amount");
    }

    let whole = |image: &RgbImage| mean(&lumas(image, 0..W, 0..H));
    assert_rises("vignette midpoint: frame brightness", &[0.0, 25.0, 50.0, 75.0, 100.0].map(|m| (m, whole(&at(m)))));
    assert!(corners(&at(100.0)) < corners(&neutral) - 30.0, "at 100 the corners are still the vignette's");
}

#[test]
fn vignette_roundness_turns_it_to_a_circle_when_positive_and_a_rectangle_when_negative() {

    let wide = frame(160, 80, |_, _| [0.18; 3]);
    let neutral = untouched(&wide);
    let at = |roundness: f32| vignette(|b| { b.effects.vignette = -100.0; b.effects.vignette_roundness = roundness }, &wide);
    assert_eq!(vignette(|b| b.effects.vignette_roundness = -100.0, &wide), neutral, "roundness with no amount");

    let reach = |image: &RgbImage, (x, y): (f32, f32)| {
        (0..=100).map(|step| step as f32 / 100.0).find(|f| {
            let (px, py) = ((80.0 + (x - 80.0) * f).min(159.0) as u32, (40.0 + (y - 40.0) * f).min(79.0) as u32);
            luma(&neutral, px, py) - luma(image, px, py) >= 30.0
        })
        .unwrap_or(1.0)
    };
    let (side, top, corner) = ((0.0, 40.0), (80.0, 0.0), (0.0, 0.0));
    let (ellipse, circle, rectangle) = (at(0.0), at(100.0), at(-100.0));

    let across = |image: &RgbImage| reach(image, side) / reach(image, top);
    println!("roundness: side/top reach at 0 {:.2}, at +100 {:.2}", across(&ellipse), across(&circle));
    assert!((across(&ellipse) - 1.0).abs() <= 0.05, "at 0 the vignette is the frame's own ellipse");
    assert!(across(&circle) < 0.75, "+100 should make it a circle");

    let square = |image: &RgbImage| reach(image, corner) / reach(image, side);
    println!("roundness: corner/side reach at 0 {:.2}, at −100 {:.2}", square(&ellipse), square(&rectangle));
    assert!(square(&rectangle) > square(&ellipse) + 0.1, "−100 should square the vignette off");
    for image in [&circle, &rectangle] {
        assert_usable("roundness", &neutral, image);
    }
}

#[test]
fn vignette_roundness_reshapes_without_weakening_the_corners() {

    let flat = grey(0.18);
    let neutral = corners(&untouched(&flat));
    let darkening = |midpoint: f32, roundness: f32| {
        neutral - corners(&vignette(|b| {
            b.effects.vignette = -100.0;
            b.effects.vignette_midpoint = midpoint;
            b.effects.vignette_roundness = roundness;
        }, &flat))
    };
    let weak: Vec<String> = [50.0, 75.0, 100.0]
        .into_iter()
        .flat_map(|midpoint| [-100.0, -50.0, 100.0].map(|roundness| (midpoint, roundness)))
        .filter(|(midpoint, roundness)| darkening(*midpoint, *roundness) < darkening(*midpoint, 0.0) * 0.8)
        .map(|(midpoint, roundness)| {
            let (got, full) = (darkening(midpoint, roundness), darkening(midpoint, 0.0));
            format!("midpoint {midpoint} roundness {roundness}: {got:.1} of {full:.1}")
        })
        .collect();
    println!("roundness, corners short of the amount: {weak:?}");
    assert!(weak.is_empty(), "{weak:?}");
}

#[test]
fn vignette_feather_widens_the_transition() {
    let flat = grey(0.18);
    let neutral = untouched(&flat);
    let at = |feather: f32| vignette(|b| { b.effects.vignette = -100.0; b.effects.vignette_feather = feather }, &flat);
    for feather in [0.0, 100.0] {
        assert_eq!(vignette(|b| b.effects.vignette_feather = feather, &flat), neutral, "feather {feather} with no amount");
    }

    let base = luma(&neutral, W / 2, H / 2);
    let width = |image: &RgbImage| {
        let fall: Vec<f32> = (W / 2..W).map(|x| base - luma(image, x, H / 2)).collect();
        let full = fall.iter().cloned().fold(0.0, f32::max);
        fall.iter().filter(|d| **d > full * 0.1 && **d < full * 0.9).count() as f32
    };
    assert_rises("vignette feather: transition width in pixels", &[0.0, 25.0, 50.0, 75.0, 100.0].map(|f| (f, width(&at(f)))));
    assert_usable("feather 0", &neutral, &at(0.0));
}

fn grain(set: impl FnOnce(&mut Basic), frame: &LinearImage) -> RgbImage {
    render(Basic::with(|b| { b.effects.grain = 100.0; set(b) }), frame)
}

#[test]
fn grain_amount_adds_repeatable_grain_to_the_midtones_and_spares_black_and_white() {
    let flat = grey(0.18);
    let neutral = untouched(&flat);
    let at = |amount: f32| render(Basic::with(|b| b.effects.grain = amount), &flat);
    let roughness = |image: &RgbImage| spread(&lumas(image, 0..W, 0..H));

    assert_eq!(at(0.0), neutral);
    assert_no_jump("grain 1", &neutral, &at(1.0));
    assert_rises("grain: spread on flat grey", &[0.0, 25.0, 50.0, 75.0, 100.0].map(|a| (a, roughness(&at(a)))));
    assert_eq!(at(100.0), at(100.0), "the same frame twice is the same grain twice");
    let shift = mean(&lumas(&at(100.0), 0..W, 0..H)) - mean(&lumas(&neutral, 0..W, 0..H));
    println!("grain 100: mean moved {shift:+.2} codes");
    assert!(shift.abs() <= 4.0, "grain 100 moved the mean by {shift:.2} codes");
    assert_usable("grain 100", &neutral, &at(100.0));

    for (name, value) in [("black", 0.0), ("white", 4.0)] {
        let frame = grey(value);
        assert_eq!(grain(|_| {}, &frame), untouched(&frame), "grain on {name}");
    }
}

#[test]
fn grain_size_makes_the_grain_coarser() {
    let flat = grey(0.18);
    for size in [0.0, 100.0] {
        let alone = render(Basic::with(|b| b.effects.grain_size = size), &flat);
        assert_eq!(alone, untouched(&flat), "size {size} with no amount");
    }
    let at = |size: f32| grain(|b| b.effects.grain_size = size, &flat);
    assert_rises("grain size: neighbour likeness", &[0.0, 25.0, 50.0, 75.0, 100.0].map(|s| (s, coherence(&at(s)))));
}

#[test]
fn grain_roughness_makes_the_grain_less_even() {
    let flat = grey(0.18);
    for roughness in [0.0, 100.0] {
        let alone = render(Basic::with(|b| b.effects.grain_roughness = roughness), &flat);
        assert_eq!(alone, untouched(&flat), "roughness {roughness} with no amount");
    }

    let at = |roughness: f32| grain(|b| b.effects.grain_roughness = roughness, &flat);
    assert_falls("grain roughness: neighbour likeness", &[0.0, 50.0, 100.0].map(|r| (r, coherence(&at(r)))));
}

type Which = fn(&mut Grading) -> &mut grading::Range;

const RANGES: [(&str, Which, Range<u32>); 4] = [
    ("shadows", |g| &mut g.shadows, SHADOWS),
    ("midtones", |g| &mut g.midtones, MIDTONES),
    ("highlights", |g| &mut g.highlights, HIGHLIGHTS),
    ("global", |g| &mut g.global, MIDTONES),
];

fn range(hue: f32, saturation: f32, luminance: f32) -> grading::Range {
    grading::Range { hue, saturation, luminance }
}

fn assert_stays_home(which: Which, hue: f32, home: Range<u32>, away: &[Range<u32>], name: &str) {
    let neutral = untouched(&ramp());
    let tinted = graded(|g| *which(g) = range(hue, 80.0, 0.0));
    let tints: Vec<f32> = away.iter().map(|xs| tint(&tinted, xs.clone())).collect();
    assert_home(&format!("{name} tint"), tint(&tinted, home.clone()), &tints);

    let lifted = graded(|g| which(g).luminance = 100.0);
    let lift = |xs: Range<u32>| level(&lifted, xs.clone()) - level(&neutral, xs);
    let lifts: Vec<f32> = away.iter().map(|xs| lift(xs.clone())).collect();
    assert_home(&format!("{name} brightness"), lift(home), &lifts);
}

#[test]
fn shadows_grading_tints_and_brightens_the_shadows_not_the_highlights() {
    assert_stays_home(|g| &mut g.shadows, 240.0, SHADOWS, &[HIGHLIGHTS], "shadows");
}

#[test]
fn midtones_grading_tints_and_brightens_the_midtones_not_the_ends() {
    assert_stays_home(|g| &mut g.midtones, 120.0, MIDTONES, &[SHADOWS, HIGHLIGHTS], "midtones");
}

#[test]
fn highlights_grading_tints_and_brightens_the_highlights_not_the_shadows() {
    assert_stays_home(|g| &mut g.highlights, 40.0, HIGHLIGHTS, &[SHADOWS], "highlights");
}

#[test]
fn global_grading_tints_every_part_of_the_range() {
    let tinted = graded(|g| g.global = range(200.0, 60.0, 0.0));
    let tints = [SHADOWS, MIDTONES, HIGHLIGHTS].map(|xs| tint(&tinted, xs));
    println!("global tint: {tints:.2?}");
    assert!(tints.iter().all(|t| *t >= 3.0), "global should reach all three: {tints:.2?}");
}

#[test]
fn grading_hue_turns_each_range_s_tint_around_the_wheel_and_does_nothing_alone() {
    for (name, which, home) in RANGES {
        assert_eq!(graded(|g| which(g).hue = 200.0), untouched(&ramp()), "{name}: a hue with no saturation");
        for (hue, channel) in [(0.0, 0), (120.0, 1), (240.0, 2)] {
            let [r, g, b] = colour(&graded(|grade| *which(grade) = range(hue, 100.0, 0.0)), home.clone(), 0..8);
            let dominant = [r, g, b].iter().enumerate().max_by(|a, b| a.1.total_cmp(b.1)).map(|(i, _)| i);
            assert_eq!(dominant, Some(channel), "{name} at {hue}°: {r:.1} {g:.1} {b:.1}");
        }
    }
}

#[test]
fn grading_saturation_sets_how_strong_the_tint_is() {
    for (name, which, home) in RANGES {
        let points = [0.0, 25.0, 50.0, 75.0, 100.0].map(|s| (s, tint(&graded(|g| *which(g) = range(30.0, s, 0.0)), home.clone())));
        assert_rises(&format!("{name} saturation: tint"), &points);
        assert_no_jump(name, &untouched(&ramp()), &graded(|g| *which(g) = range(30.0, 1.0, 0.0)));
    }
}

#[test]
fn grading_brightness_lifts_when_positive_and_lowers_when_negative_in_every_range() {
    let neutral = untouched(&ramp());
    for (name, which, home) in RANGES {
        let at = |luminance: f32| graded(|g| which(g).luminance = luminance);
        let points = [-100.0, -50.0, 0.0, 50.0, 100.0].map(|v| (v, level(&at(v), home.clone())));
        assert_rises(&format!("{name} brightness: level at home"), &points);
        assert_no_jump(name, &neutral, &at(1.0));
        for end in [-100.0, 100.0] {
            assert_usable(name, &neutral, &at(end));
        }
    }
}

#[test]
fn grading_blending_carries_a_tint_further_into_the_next_range_and_does_nothing_alone() {
    for blending in [0.0, 100.0] {
        assert_eq!(graded(|g| g.blending = blending), untouched(&ramp()), "blending {blending} on its own");
    }
    let at = |blending: f32| graded(|g| { g.shadows = range(240.0, 100.0, 0.0); g.blending = blending });
    assert_rises("blending: shadows' tint in the midtones", &[0.0, 25.0, 50.0, 75.0, 100.0].map(|v| (v, tint(&at(v), MIDTONES))));
}

#[test]
fn grading_balance_moves_the_line_between_shadows_and_highlights_and_does_nothing_alone() {
    for balance in [-100.0, 100.0] {
        assert_eq!(graded(|g| g.balance = balance), untouched(&ramp()), "balance {balance} on its own");
    }
    let at = |balance: f32| graded(|g| { g.highlights = range(40.0, 100.0, 0.0); g.balance = balance });
    assert_rises("balance: highlights' tint in the midtones", &[-100.0, -50.0, 0.0, 50.0, 100.0].map(|v| (v, tint(&at(v), MIDTONES))));
}

fn soft_edge() -> LinearImage {
    frame(W, H, |x, _| {
        let t = 1.0 / (1.0 + (-(x as f32 - W as f32 / 2.0) / 1.5).exp());
        [0.09 * 4f32.powf(t); 3]
    })
}

fn sharpened(set: impl FnOnce(&mut Basic), frame: &LinearImage) -> RgbImage {
    render(Basic::with(|b| { b.detail.sharpen = 100.0; set(b) }), frame)
}

#[test]
fn sharpening_steepens_an_edge_across_its_whole_travel_and_leaves_flat_areas() {
    let edge = soft_edge();
    let neutral = untouched(&edge);
    let at = |amount: f32| render(Basic::with(|b| b.detail.sharpen = amount), &edge);

    assert_eq!(at(25.0), neutral);
    assert_no_jump("sharpening 26", &neutral, &at(26.0));
    assert_rises("sharpening: acutance", &[0.0, 25.0, 50.0, 75.0, 100.0].map(|a| (a, acutance(&at(a)))));
    for amount in [0.0, 100.0] {
        assert_eq!(worst_in(&at(amount), &neutral, 0..W / 4, 0..H), 0, "sharpening {amount} moved flat grey");
    }
    assert_usable("sharpening 100", &neutral, &at(100.0));
}

#[test]
fn sharpening_radius_widens_the_band_around_an_edge() {
    let edge = step_edge(0.09, 0.36);
    let plain = render(Basic::with(|b| b.detail.sharpen = 0.0), &edge);
    for radius in [0.5, 3.0] {
        let alone = render(Basic::with(|b| { b.detail.sharpen = 0.0; b.detail.sharpen_radius = radius }), &edge);
        assert_eq!(alone, plain, "radius {radius} with no sharpening");
    }

    let band = |radius: f32| {
        let out = sharpened(|b| b.detail.sharpen_radius = radius, &edge);
        (0..W).filter(|x| worst_in(&out, &plain, *x..x + 1, 0..H) >= 2).count() as f32
    };
    assert_rises("radius: columns touched", &[1.0, 2.0, 3.0].map(|r| (r, band(r))));
}

#[test]
fn every_step_of_sharpening_radius_changes_the_render() {
    let edge = soft_edge();
    let renders: Vec<(f32, Vec<u16>)> = (5..=30)
        .map(|tenths| tenths as f32 / 10.0)
        .map(|r| {
            let mut document = Document::new(String::new());
            document.set_basic(Basic::with(|b| {
                b.detail.sharpen = 100.0;
                b.detail.sharpen_radius = r;
            }));
            (r, numa_render::develop16(&document, &edge, &Default::default()).into_raw())
        })
        .collect();
    let dead: Vec<String> = renders
        .windows(2)
        .filter(|pair| pair[0].1 == pair[1].1)
        .map(|pair| format!("{}→{}", pair[0].0, pair[1].0))
        .collect();
    assert!(dead.is_empty(), "{} of {} radius steps change nothing: {}", dead.len(), renders.len() - 1, dead.join(" "));
}

#[test]
fn sharpening_masking_holds_it_off_noise_and_keeps_it_on_edges() {

    let scene = frame(W, H, |x, y| {
        let v = match x {
            x if x < W / 2 => 0.18 * (0.2 * noise(x, y, 1)).exp2(),
            x if x < W * 3 / 4 => 0.05,
            _ => 0.6,
        };
        [v; 3]
    });
    let plain = render(Basic::with(|b| b.detail.sharpen = 0.0), &scene);
    assert_eq!(render(Basic::with(|b| { b.detail.sharpen = 0.0; b.detail.sharpen_masking = 100.0 }), &scene), plain);

    let at = |masking: f32| sharpened(|b| b.detail.sharpen_masking = masking, &scene);
    let grain = |image: &RgbImage| spread(&lumas(image, 8..W / 2 - 8, 8..H - 8));

    let edge = |image: &RgbImage| {
        let added: Vec<f32> = (W * 3 / 4 - 4..W * 3 / 4 + 4).map(|x| (luma(image, x, H / 2) - luma(&plain, x, H / 2)).abs()).collect();
        mean(&added)
    };
    assert_falls("masking: noise under sharpening 100", &[0.0, 25.0, 50.0, 100.0].map(|m| (m, grain(&at(m)))));
    let (held, raw, before) = (grain(&at(100.0)), grain(&at(0.0)), grain(&plain));
    assert!(held - before <= (raw - before) * 0.25, "masking 100 let {:.2} of {:.2} through", held - before, raw - before);
    println!("masking: halo at the edge {:.1} at 0, {:.1} at 100", edge(&at(0.0)), edge(&at(100.0)));
    assert!(edge(&at(0.0)) >= 4.0, "sharpening 100 should show at a clean edge");
    assert!(edge(&at(100.0)) >= edge(&at(0.0)) * 0.8, "masking 100 took the sharpening off the edge too");
}

fn noisy_step() -> LinearImage {
    frame(W, H, |x, y| [if x < W * 3 / 4 { 0.18 } else { 0.72 } * (0.1 * noise(x, y, 2)).exp2(); 3])
}

fn noise_left(image: &RgbImage) -> f32 {
    spread(&lumas(image, 8..W * 3 / 4 - 8, 8..H - 8))
}

#[test]
fn noise_reduction_smooths_noise_and_keeps_an_edge() {
    let noisy = noisy_step();
    let neutral = untouched(&noisy);
    let at = |amount: f32| render(Basic::with(|b| b.detail.denoise_luma = amount), &noisy);

    assert_eq!(at(0.0), neutral);
    assert_no_jump("noise reduction 1", &neutral, &at(1.0));
    assert_falls("noise reduction: noise", &[0.0, 25.0, 50.0, 75.0, 100.0].map(|a| (a, noise_left(&at(a)))));
    let edge = |image: &RgbImage| steepest(image, W * 3 / 4 - 4..W * 3 / 4 + 4);
    println!("noise reduction: edge {:.1} → {:.1}", edge(&neutral), edge(&at(100.0)));
    assert!(edge(&at(100.0)) >= edge(&neutral) * 0.8, "noise reduction 100 softened the edge");
    assert_usable("noise reduction 100", &neutral, &at(100.0));
}

#[test]
fn noise_detail_keeps_more_texture_when_raised_and_does_nothing_alone() {
    let noisy = noisy_step();
    for detail in [0.0, 100.0] {
        let alone = render(Basic::with(|b| b.detail.denoise_detail = detail), &noisy);
        assert_eq!(alone, untouched(&noisy), "detail {detail} with no noise reduction");
    }
    let at = |detail: f32| render(Basic::with(|b| { b.detail.denoise_luma = 60.0; b.detail.denoise_detail = detail }), &noisy);
    assert_rises("noise detail: noise left at reduction 60", &[0.0, 25.0, 50.0, 75.0, 100.0].map(|d| (d, noise_left(&at(d)))));
}

#[test]
fn colour_noise_takes_out_speckle_not_brightness_or_real_colour() {

    let speckled = frame(W, H, |x, y| {
        let (r, b) = (0.18 * (1.0 + 0.3 * noise(x, y, 3)), 0.18 * (1.0 + 0.3 * noise(x, y, 4)));
        [r, (0.18 - 0.2126 * r - 0.0722 * b) / 0.7152, b]
    });
    let neutral = untouched(&speckled);
    let at = |amount: f32, frame: &LinearImage| render(Basic::with(|b| b.detail.denoise_colour = amount), frame);
    let speckle = |image: &RgbImage| {
        let pixels: Vec<[f32; 3]> = image.pixels().map(|p| p.0.map(f32::from)).collect();
        let red: Vec<f32> = pixels.iter().map(|p| p[0] - p[1]).collect();
        let blue: Vec<f32> = pixels.iter().map(|p| p[2] - p[1]).collect();
        spread(&red) + spread(&blue)
    };

    assert_eq!(at(25.0, &speckled), neutral);
    assert_no_jump("colour noise 26", &neutral, &at(26.0, &speckled));
    assert_falls("colour noise: speckle", &[0.0, 25.0, 50.0, 75.0, 100.0].map(|a| (a, speckle(&at(a, &speckled)))));
    let brightness = |image: &RgbImage| lumas(image, 0..W, 0..H);
    let moved = mean(&brightness(&at(100.0, &speckled)).iter().zip(brightness(&neutral)).map(|(a, b)| (a - b).abs()).collect::<Vec<_>>());
    println!("colour noise 100: brightness moved {moved:.2} codes a pixel");
    assert!(moved <= 2.0, "colour noise 100 moved brightness by {moved:.2} codes a pixel");

    let fields = frame(W, H, |x, _| if x < W / 2 { [0.3, 0.08, 0.06] } else { [0.06, 0.25, 0.08] });
    let (none, full) = (at(0.0, &fields), at(100.0, &fields));
    let inside = worst_in(&none, &full, 0..W / 2 - 8, 0..H).max(worst_in(&none, &full, W / 2 + 8..W, 0..H));
    assert!(inside <= 1, "colour noise moved a flat colour field by {inside} codes");
}

#[test]
fn defringe_takes_purple_and_green_off_a_hard_edge_and_leaves_a_purple_object() {

    let fringed = frame(W, H, |x, y| match x {
        x if x < W / 2 => [0.03; 3],
        x if x < W / 2 + 2 && y < H / 2 => [0.6, 0.25, 0.6],
        x if x < W / 2 + 2 => [0.35, 0.6, 0.35],
        _ => [0.6; 3],
    });
    let neutral = untouched(&fringed);
    let at = |amount: f32, frame: &LinearImage| render(Basic::with(|b| b.detail.defringe = amount), frame);
    let cast = |image: &RgbImage| {
        let [r, g, b] = colour(image, W / 2..W / 2 + 2, 4..H / 2 - 4);
        let purple = (r + b) / 2.0 - g;
        let [r, g, b] = colour(image, W / 2..W / 2 + 2, H / 2 + 4..H - 4);
        purple + g - (r + b) / 2.0
    };

    assert_eq!(at(0.0, &fringed), neutral);
    assert_no_jump("defringe 1", &neutral, &at(1.0, &fringed));
    assert_falls("defringe: purple plus green on the fringe", &[0.0, 25.0, 50.0, 75.0, 100.0].map(|a| (a, cast(&at(a, &fringed)))));
    assert!(cast(&at(100.0, &fringed)) < cast(&neutral) * 0.5, "defringe 100 left most of the fringe");
    for (half, ys) in [("purple", 4..H / 2 - 4), ("green", H / 2 + 4..H - 4)] {
        let stops = (light(&at(100.0, &fringed), W / 2..W / 2 + 2, ys.clone()) / light(&neutral, W / 2..W / 2 + 2, ys)).log2();
        println!("defringe 100: {half} rim's light moved {stops:+.3} stops");
        assert!(stops.abs() <= 0.1, "defringe moved the {half} rim's light by {stops:+.3} stops");
    }

    let object = frame(W, H, |_, _| [0.3, 0.12, 0.3]);
    assert_eq!(at(100.0, &object), untouched(&object), "defringe touched a purple field");
}

#[test]
fn moire_takes_false_colour_off_fine_stripes_and_leaves_a_real_colour_edge() {

    let stripes = frame(W, H, |x, _| if x % 2 == 0 { [0.28, 0.1402, 0.28] } else { [0.10, 0.2119, 0.10] });
    let neutral = untouched(&stripes);
    let at = |amount: f32, frame: &LinearImage| render(Basic::with(|b| b.detail.moire = amount), frame);
    let false_colour = |image: &RgbImage| {
        let red: Vec<f32> = (8..H - 8).flat_map(|y| (8..W - 8).map(move |x| (x, y))).map(|(x, y)| {
            let [r, g, _] = image.get_pixel(x, y).0;
            r as f32 - g as f32
        }).collect();
        spread(&red)
    };

    assert_eq!(at(0.0, &stripes), neutral);
    assert_no_jump("moiré 1", &neutral, &at(1.0, &stripes));
    assert_falls("moiré: false colour", &[0.0, 25.0, 50.0, 75.0, 100.0].map(|a| (a, false_colour(&at(a, &stripes)))));
    let stops = (light(&at(100.0, &stripes), 8..W - 8, 8..H - 8) / light(&neutral, 8..W - 8, 8..H - 8)).log2();
    println!("moiré 100: light moved {stops:+.3} stops");
    assert!(stops.abs() <= 0.1, "moiré moved the light by {stops:+.3} stops");

    let edge = colour_edge(1.5);
    let moved = worst(&at(100.0, &edge), &untouched(&edge));
    println!("moiré 100 on a soft colour edge: worst {moved} codes");
    assert!(moved <= 2, "moiré 100 moved a real colour edge by {moved} codes");
}

fn colour_edge(softness: f32) -> LinearImage {
    let (red, green) = ([0.3, 0.14, 0.1], [0.1, 0.22, 0.12]);
    frame(W, H, |x, _| {
        let offset = x as f32 + 0.5 - W as f32 / 2.0;
        let t = match softness > 0.0 {
            true => 1.0 / (1.0 + (-offset / softness).exp()),
            false => f32::from(offset > 0.0),
        };
        [0, 1, 2].map(|i| red[i] + (green[i] - red[i]) * t)
    })
}

#[test]
#[ignore = "Ruled not a fault (FT-028 #14): Moiré greys a hard one-pixel colour step (up to 43 codes), which a demosaiced photograph does not have; a 1.5 px soft seam is untouched. Lightroom has moiré only as a local adjustment."]
fn moire_leaves_even_a_hard_colour_step() {
    let edge = colour_edge(0.0);
    let out = render(Basic::with(|b| b.detail.moire = 100.0), &edge);
    let neutral = untouched(&edge);
    let moved: Vec<(u32, u8)> = (0..W).map(|x| (x, worst_in(&out, &neutral, x..x + 1, 0..H))).filter(|(_, d)| *d > 2).collect();
    println!("moiré 100 on a hard colour step: columns moved {moved:?}");
    assert!(moved.is_empty(), "moiré 100 moved a real colour step: {moved:?}");
}

#[test]
fn lens_distortion_pushes_the_edges_out_when_positive_and_in_when_negative_never_the_centre() {

    let lined = frame(W, H, |_, y| [if (15..17).contains(&y) { 0.8 } else { 0.1 }; 3]);
    let at = |amount: f32, frame: &LinearImage| render(Basic::with(|b| b.optics.lens_distortion = amount), frame);
    let background = luma(&untouched(&lined), W / 2, H / 2);

    let height = |image: &RgbImage, x: u32| {
        let weights: Vec<(f32, f32)> = (0..H / 2).map(|y| ((luma(image, x, y) - background).max(0.0), y as f32 + 0.5)).collect();
        H as f32 / 2.0 - weights.iter().map(|(w, y)| w * y).sum::<f32>() / weights.iter().map(|(w, _)| w).sum::<f32>()
    };

    let bow = |image: &RgbImage| (height(image, 8) + height(image, W - 9)) / 2.0 - height(image, W / 2);

    assert_eq!(at(0.0, &lined), untouched(&lined));

    assert!(bow(&at(1.0, &lined)).abs() <= bow(&at(100.0, &lined)) * 0.05, "one step of distortion bowed the line");
    assert_rises("distortion: bow of a straight line (px)", &[-100.0, -50.0, 0.0, 50.0, 100.0].map(|v| (v, bow(&at(v, &lined)))));
    assert!(bow(&at(100.0, &lined)) > 0.8, "+100 should push the line's ends outwards");

    let flat = grey(0.18);
    for end in [-100.0, 100.0] {
        let out = at(end, &flat);
        assert!(worst_in(&out, &untouched(&flat), W / 2 - 4..W / 2 + 4, H / 2 - 4..H / 2 + 4) <= 1, "distortion {end} moved the centre");
        assert_usable("distortion", &untouched(&flat), &out);
    }
}

#[test]
fn lens_vignetting_lifts_the_corners_when_positive_and_lowers_them_when_negative_never_the_centre() {
    let flat = grey(0.18);
    let neutral = untouched(&flat);
    let at = |amount: f32| render(Basic::with(|b| b.optics.lens_vignetting = amount), &flat);

    assert_eq!(at(0.0), neutral);
    assert_no_jump("vignetting +1", &neutral, &at(1.0));
    assert_rises("lens vignetting: corner brightness", &[-100.0, -50.0, 0.0, 50.0, 100.0].map(|v| (v, corners(&at(v)))));
    assert!(corners(&at(100.0)) > corners(&neutral) + 20.0, "+100 should lift the corners by about a stop");
    for end in [-100.0, 100.0] {
        let out = at(end);
        assert!((centre(&out) - centre(&neutral)).abs() <= 1.0, "vignetting {end} moved the centre");
        assert_usable("vignetting", &neutral, &out);
    }
}
