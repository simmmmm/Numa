use image::RgbImage;
use numa_core::color::{CameraProfile, WhiteBalance};
use numa_core::curve::Curve;
use numa_core::document::{Basic, Document};
use numa_core::image::LinearImage;
use numa_core::mask::{Mask, Shape};
use numa_core::mixer::Mixer;
use numa_core::point::{PointColour, PointColours};
use numa_core::tone::{scene_value_for, MIDDLE_GREY};
use numa_render::{apply_stack, develop, to_working_space, RenderInputs};

const RAMP_WIDTH: u32 = 64;
const RAMP_STOPS: (f32, f32) = (-8.0, 4.0);

fn ramp_value(x: u32) -> f32 {
    let along = x as f32 / (RAMP_WIDTH - 1) as f32;
    MIDDLE_GREY * 2f32.powf(RAMP_STOPS.0 + (RAMP_STOPS.1 - RAMP_STOPS.0) * along)
}

fn ramp() -> LinearImage {
    let data = (0..RAMP_WIDTH * 4).flat_map(|i| [ramp_value(i % RAMP_WIDTH); 3]).collect();
    LinearImage::new(RAMP_WIDTH, 4, data)
}

fn column(value: f32) -> u32 {
    let off = |x: u32| (ramp_value(x) / value).log2().abs();
    (0..RAMP_WIDTH).min_by(|a, b| off(*a).total_cmp(&off(*b))).expect("a column")
}

fn level(image: &RgbImage, x: u32) -> f32 {
    let pixel = image.get_pixel(x, 1).0;
    pixel.iter().map(|value| *value as f32).sum::<f32>() / 3.0
}

const PATCH: u32 = 16;
const HUES: [(&str, f32); 8] = [
    ("red", 0.0),
    ("orange", 30.0),
    ("yellow", 60.0),
    ("green", 120.0),
    ("aqua", 180.0),
    ("blue", 240.0),
    ("purple", 270.0),
    ("magenta", 300.0),
];
const SKIN: usize = 8;
const GREY: usize = 9;
const PATCHES: usize = 10;

fn decode(value: f32) -> f32 {
    if value <= 0.040_45 { value / 12.92 } else { ((value + 0.055) / 1.055).powf(2.4) }
}

fn from_hsv(hue: f32, saturation: f32, value: f32) -> [f32; 3] {
    let sixths = hue.rem_euclid(360.0) / 60.0;
    let fraction = sixths - sixths.floor();
    let (p, q, t) = (1.0 - saturation, 1.0 - saturation * fraction, 1.0 - saturation * (1.0 - fraction));
    let rgb = match sixths as u32 {
        0 => [1.0, t, p],
        1 => [q, 1.0, p],
        2 => [p, 1.0, t],
        3 => [p, q, 1.0],
        4 => [t, p, 1.0],
        _ => [1.0, p, q],
    };
    rgb.map(|channel| decode(channel * value))
}

fn patch_colour(index: usize) -> [f32; 3] {
    match index {
        SKIN => from_hsv(25.0, 0.35, 0.85),
        GREY => [MIDDLE_GREY; 3],
        hue => from_hsv(HUES[hue].1, 0.5, 0.8),
    }
}

fn patches() -> LinearImage {
    let width = PATCH * PATCHES as u32;
    let data = (0..width * 8).flat_map(|i| patch_colour(((i % width) / PATCH) as usize)).collect();
    LinearImage::new(width, 8, data)
}

fn patch(image: &RgbImage, index: usize) -> [f32; 3] {
    let mut sum = [0.0f32; 3];
    for y in 2..6 {
        for x in 6..10 {
            let pixel = image.get_pixel(index as u32 * PATCH + x, y).0;
            for channel in 0..3 {
                sum[channel] += pixel[channel] as f32 / 16.0;
            }
        }
    }
    sum
}

fn hsv(rgb: [f32; 3]) -> (f32, f32, f32) {
    let high = rgb[0].max(rgb[1]).max(rgb[2]);
    let low = rgb[0].min(rgb[1]).min(rgb[2]);
    let range = high - low;
    let saturation = if high > 0.0 { range / high } else { 0.0 };
    if range <= 0.0 {
        return (0.0, saturation, high);
    }
    let hue = if high == rgb[0] {
        (rgb[1] - rgb[2]) / range
    } else if high == rgb[1] {
        2.0 + (rgb[2] - rgb[0]) / range
    } else {
        4.0 + (rgb[0] - rgb[1]) / range
    };
    ((hue * 60.0).rem_euclid(360.0), saturation, high)
}

fn turn(from: f32, to: f32) -> f32 {
    (to - from + 540.0).rem_euclid(360.0) - 180.0
}

fn luma(rgb: [f32; 3]) -> f32 {
    0.2126 * rgb[0] + 0.7152 * rgb[1] + 0.0722 * rgb[2]
}

fn plain() -> Document {
    Document::new(String::new())
}

fn with(basic: Basic) -> Document {
    let mut document = plain();
    document.set_basic(basic);
    document
}

fn render(document: &Document, frame: &LinearImage) -> RgbImage {
    apply_stack(document, frame, 1.0)
}

fn biggest_change(a: &RgbImage, b: &RgbImage) -> u8 {
    a.as_raw().iter().zip(b.as_raw()).map(|(a, b)| a.abs_diff(*b)).max().unwrap_or(0)
}

fn in_order(image: &RgbImage) -> Result<(), String> {
    for channel in 0..3 {
        for x in 1..image.width() {
            let (before, now) = (image.get_pixel(x - 1, 1)[channel], image.get_pixel(x, 1)[channel]);
            if now as i32 + 1 < before as i32 {
                return Err(format!("channel {channel} steps down {before} → {now} at column {x}"));
            }
        }
    }
    Ok(())
}

fn has_a_picture(image: &RgbImage) -> bool {
    let middle = (0..image.width()).filter(|x| (3.0..252.0).contains(&level(image, *x))).count();
    middle >= image.width() as usize / 4
}

fn patches_survive(image: &RgbImage) -> Result<(), String> {
    for index in 0..PATCHES {
        let colour = patch(image, index);
        let (_, _, value) = hsv(colour);
        if value < 3.0 || colour.iter().all(|channel| *channel > 252.0) {
            return Err(format!("patch {index} collapsed to {colour:?}"));
        }
    }
    Ok(())
}

fn no_jump(label: &str, hair: u8, ten_hairs: u8) {
    assert!(hair <= 3 || ten_hairs >= hair * 2, "{label}: a hair moved {hair} codes, ten hairs {ten_hairs}");
}

fn neutral_is_untouched_and_continuous(frame: &LinearImage, set: fn(&mut Basic, f32), hair: f32) {
    let untouched = render(&plain(), frame);
    assert_eq!(render(&with(Basic::with(|b| set(b, 0.0))), frame), untouched, "neutral is not the untouched frame");
    for side in [-hair, hair] {
        let basic = Basic::with(|b| set(b, side));
        assert!(!basic.is_identity(), "the slider writes nothing at {side}");
        let moved = |value: f32| biggest_change(&render(&with(Basic::with(|b| set(b, value))), frame), &untouched);
        no_jump(&format!("at {side}"), moved(side), moved(side * 10.0));
    }
}

fn ends_are_usable(set: fn(&mut Basic, f32), low: f32, high: f32) {
    let frame = ramp();
    let untouched = render(&plain(), &frame);
    for (value, least) in [(low, 8), (high, 8), (low / 2.0, 3), (high / 2.0, 3)] {
        let out = render(&with(Basic::with(|b| set(b, value))), &frame);
        if let Err(problem) = in_order(&out) {
            panic!("at {value}: {problem}");
        }
        assert!(has_a_picture(&out), "at {value} the ramp is clipped nearly whole");
        let moved = biggest_change(&out, &untouched);
        assert!(moved >= least, "at {value} nothing moved more than {moved} codes");
    }
}

fn exposure(b: &mut Basic, value: f32) {
    b.tone.exposure = value;
}
fn contrast(b: &mut Basic, value: f32) {
    b.tone.contrast = value;
}
fn highlights(b: &mut Basic, value: f32) {
    b.tone.highlights = value;
}
fn shadows(b: &mut Basic, value: f32) {
    b.tone.shadows = value;
}
fn whites(b: &mut Basic, value: f32) {
    b.tone.whites = value;
}
fn blacks(b: &mut Basic, value: f32) {
    b.tone.blacks = value;
}

fn scene_at(image: &RgbImage, x: u32) -> f32 {
    scene_value_for(level(image, x) / 255.0)
}

fn moves(set: fn(&mut Basic, f32), value: f32, scene: f32) -> f32 {
    let frame = ramp();
    let x = column(scene);
    level(&render(&with(Basic::with(|b| set(b, value))), &frame), x) - level(&render(&plain(), &frame), x)
}

#[test]
fn exposure_is_a_stop_per_step_everywhere_on_the_frame() {
    neutral_is_untouched_and_continuous(&ramp(), exposure, 0.05);

    let frame = ramp();
    let grey = column(MIDDLE_GREY);
    let base = scene_at(&render(&plain(), &frame), grey);
    for (stops, ratio) in [(1.0, 2.0), (-1.0, 0.5)] {
        let out = render(&with(Basic::with(|b| b.tone.exposure = stops)), &frame);
        let got = scene_at(&out, grey) / base;
        assert!((got / ratio - 1.0).abs() < 0.1, "{stops:+} EV gave ×{got:.3}");

        for x in 0..RAMP_WIDTH {
            let (now, was) = (level(&out, x), level(&render(&plain(), &frame), x));
            assert!(if stops > 0.0 { now >= was } else { now <= was }, "column {x} went the wrong way");
            let pixel = out.get_pixel(x, 1).0;
            assert!(pixel[0].abs_diff(pixel[1]) <= 1 && pixel[1].abs_diff(pixel[2]) <= 1, "grey tinted: {pixel:?}");
        }
    }

    ends_are_usable(exposure, -5.0, 5.0);
}

#[test]
fn contrast_spreads_the_tones_about_middle_grey() {
    neutral_is_untouched_and_continuous(&ramp(), contrast, 1.0);

    let (dark, grey, bright) = (0.02, MIDDLE_GREY, 0.75);

    assert!(moves(contrast, 50.0, dark) <= -8.0 && moves(contrast, 50.0, bright) >= 8.0);
    assert!(moves(contrast, -50.0, dark) >= 8.0 && moves(contrast, -50.0, bright) <= -8.0);
    for value in [-50.0, 50.0, 100.0] {
        assert!(moves(contrast, value, grey).abs() <= 1.0, "middle grey moved at {value}");
    }

    ends_are_usable(contrast, -50.0, 100.0);
}

#[test]
fn contrast_at_its_lowest_still_leaves_a_picture() {
    let frame = ramp();
    let out = render(&with(Basic::with(|b| b.tone.contrast = -100.0)), &frame);
    let span = level(&out, RAMP_WIDTH - 1) - level(&out, 0);

    assert!(span >= 32.0, "twelve stops of ramp span {span} codes at −100");
    let colours = render(&with(Basic::with(|b| b.tone.contrast = -100.0)), &patches());
    assert!(hsv(patch(&colours, 0)).1 > 0.1, "the red patch went grey: {:?}", patch(&colours, 0));
}

#[test]
fn highlights_move_the_bright_tones_and_leave_the_shadows() {
    neutral_is_untouched_and_continuous(&ramp(), highlights, 1.0);

    assert!(moves(highlights, -100.0, 0.75) <= -8.0, "−100 did not pull a bright tone down");
    assert!(moves(highlights, 100.0, 0.75) >= 8.0, "+100 did not lift a bright tone");

    assert!(moves(highlights, -100.0, 2.5) <= -8.0, "nothing recovered above white");
    for value in [-100.0, 100.0] {
        for scene in [0.005, 0.02, MIDDLE_GREY] {
            assert_eq!(moves(highlights, value, scene), 0.0, "{value} reached {scene}");
        }
    }

    ends_are_usable(highlights, -50.0, 100.0);
}

#[test]
fn highlights_pulled_all_the_way_down_keep_brighter_tones_brighter() {
    ends_are_usable(highlights, -100.0, 100.0);
}

#[test]
fn shadows_move_the_dark_tones_and_leave_the_highlights() {
    neutral_is_untouched_and_continuous(&ramp(), shadows, 1.0);

    assert!(moves(shadows, 100.0, 0.02) >= 8.0, "+100 did not lift a shadow");
    assert!(moves(shadows, -100.0, 0.02) <= -3.0, "−100 did not deepen a shadow");
    for value in [-100.0, 100.0] {
        for scene in [0.3, 0.75, 2.5] {
            assert_eq!(moves(shadows, value, scene), 0.0, "{value} reached {scene}");
        }
    }

    ends_are_usable(shadows, -100.0, 100.0);
}

#[test]
fn whites_move_the_top_of_the_range_and_leave_the_middle() {
    neutral_is_untouched_and_continuous(&ramp(), whites, 1.0);

    assert!(moves(whites, 100.0, 0.9) >= 8.0, "+100 did not lift the whites");
    assert!(moves(whites, -100.0, 0.9) <= -8.0, "−100 did not pull the whites down");
    for value in [-100.0, 100.0] {
        for scene in [0.02, MIDDLE_GREY, 0.4] {
            assert_eq!(moves(whites, value, scene), 0.0, "{value} reached {scene}");
        }
    }

    ends_are_usable(whites, -50.0, 100.0);
}

#[test]
fn whites_pulled_all_the_way_down_keep_brighter_tones_brighter() {
    ends_are_usable(whites, -100.0, 100.0);
}

#[test]
fn blacks_move_the_bottom_of_the_range_and_leave_the_middle() {
    neutral_is_untouched_and_continuous(&ramp(), blacks, 1.0);

    assert!(moves(blacks, 100.0, 0.01) > 0.0, "+100 did not lift the blacks");
    assert!(moves(blacks, -100.0, 0.01) < 0.0, "−100 did not deepen the blacks");
    for value in [-100.0, 100.0] {
        for scene in [0.1, MIDDLE_GREY, 0.75] {
            assert_eq!(moves(blacks, value, scene), 0.0, "{value} reached {scene}");
        }
    }
    for value in [-100.0, 100.0] {
        let out = render(&with(Basic::with(|b| b.tone.blacks = value)), &ramp());
        in_order(&out).unwrap_or_else(|problem| panic!("at {value}: {problem}"));
    }
}

#[test]
fn blacks_at_full_travel_visibly_moves_the_darkest_tones() {
    ends_are_usable(blacks, -100.0, 100.0);
}

#[test]
fn auto_brightens_a_dim_frame_without_clipping_it() {
    let mut dim = ramp();
    dim.data.iter_mut().for_each(|value| *value *= 0.125);
    let auto = numa_render::auto::tone(&dim, None);
    assert!(auto.basic.tone.exposure > 0.0, "{:?}", auto.basic.tone);

    let mut document = plain();
    assert_eq!(auto.apply(&mut document), None, "no subject, so no mask");
    let (before, after) = (render(&plain(), &dim), render(&document, &dim));
    assert!(level(&after, RAMP_WIDTH - 1) > level(&before, RAMP_WIDTH - 1) + 8.0, "the top did not come up");
    in_order(&after).unwrap();
    assert!(has_a_picture(&after));
}

fn with_curves(curves: [Curve; 4]) -> Document {
    let mut document = plain();
    document.set_curves(curves);
    document
}

fn lift(at: f32, to: f32) -> Curve {
    Curve::new([[0.0, 0.0], [at, to], [1.0, 1.0]])
}

#[test]
fn the_tone_curve_bends_the_tones_it_is_drawn_through_and_pins_its_ends() {
    let frame = ramp();
    let untouched = render(&plain(), &frame);
    let identity = || [Curve::identity(), Curve::identity(), Curve::identity(), Curve::identity()];
    assert_eq!(render(&with_curves(identity()), &frame), untouched, "a straight curve changed the frame");

    let hair = with_curves([lift(0.5, 0.505), Curve::identity(), Curve::identity(), Curve::identity()]);
    assert!(biggest_change(&render(&hair, &frame), &untouched) <= 3);

    let lifted = render(&with_curves([lift(0.5, 0.62), Curve::identity(), Curve::identity(), Curve::identity()]), &frame);
    let grey = column(MIDDLE_GREY);
    let raised = level(&lifted, grey) - level(&untouched, grey);
    assert!((20.0..40.0).contains(&raised), "middle grey moved {raised} codes for a 0.12 lift");
    assert!((level(&lifted, 0) - level(&untouched, 0)).abs() <= 1.0, "black moved");
    let top = RAMP_WIDTH - 1;
    assert!((level(&lifted, top) - level(&untouched, top)).abs() <= 3.0, "white moved");

    let lowered = render(&with_curves([lift(0.5, 0.38), Curve::identity(), Curve::identity(), Curve::identity()]), &frame);
    assert!(level(&lowered, grey) < level(&untouched, grey) - 20.0);

    let colours = render(&with_curves([lift(0.5, 0.62), Curve::identity(), Curve::identity(), Curve::identity()]), &patches());
    let [r, g, b] = patch(&colours, GREY);
    assert!((r - g).abs() <= 1.0 && (g - b).abs() <= 1.0, "grey tinted: {r} {g} {b}");

    for curve in [Curve::new([[0.0, 0.3], [1.0, 0.7]]), Curve::new([[0.0, 0.0], [0.2, 1.0], [1.0, 1.0]])] {
        let out = render(&with_curves([curve.clone(), Curve::identity(), Curve::identity(), Curve::identity()]), &frame);
        in_order(&out).unwrap_or_else(|problem| panic!("{curve:?}: {problem}"));
        assert!(out.as_raw().iter().any(|code| *code < 255), "{curve:?} whited the frame out");
    }
}

#[test]
fn each_channel_curve_moves_its_own_channel_and_no_other() {
    let frame = patches();
    let untouched = patch(&render(&plain(), &frame), GREY);
    for channel in 0..3 {
        let mut curves = [Curve::identity(), Curve::identity(), Curve::identity(), Curve::identity()];
        curves[1 + channel] = lift(0.5, 0.62);
        let grey = patch(&render(&with_curves(curves.clone()), &frame), GREY);
        for other in 0..3 {
            let moved = grey[other] - untouched[other];
            if other == channel {
                assert!(moved >= 20.0, "curve {channel} raised its own channel by {moved}");
            } else {
                assert!(moved.abs() <= 1.0, "curve {channel} moved channel {other} by {moved}");
            }
        }

        curves[1 + channel] = lift(0.5, 0.38);
        let grey = patch(&render(&with_curves(curves), &frame), GREY);
        assert!(grey[channel] < untouched[channel] - 20.0, "curve {channel} down did not lower it");
    }
}

fn camera() -> CameraProfile {
    let mut camera = CameraProfile {
        as_shot: [1.0; 3],
        xyz_to_cam: [[0.6058, -0.1889, -0.0645], [-0.4797, 1.2681, 0.2447], [-0.0665, 0.0873, 0.6779]],
        cam_to_srgb: [[1.6, -0.5, -0.1], [-0.2, 1.5, -0.3], [0.0, -0.4, 1.4]],
    };
    camera.as_shot = camera.multipliers(WhiteBalance { temperature: 5500.0, tint: 0.0 });
    camera
}

fn camera_ramp() -> LinearImage {
    let gains = camera().as_shot;
    let mut frame = ramp();
    frame.data.chunks_exact_mut(3).for_each(|pixel| (0..3).for_each(|c| pixel[c] /= gains[c]));
    frame.with_profile(camera())
}

fn balanced(balance: Option<WhiteBalance>) -> RgbImage {
    let mut document = plain();
    document.white_balance = balance;
    develop(&document, &camera_ramp(), &RenderInputs::default())
}

fn warmth(image: &RgbImage) -> f32 {
    let pixel = image.get_pixel(column(MIDDLE_GREY), 1).0;
    pixel[0] as f32 / pixel[2].max(1) as f32
}

fn greenness(image: &RgbImage) -> f32 {
    let pixel = image.get_pixel(column(MIDDLE_GREY), 1).0.map(|value| value as f32);
    pixel[1] / ((pixel[0] + pixel[2]) / 2.0).max(1.0)
}

#[test]
fn temperature_warms_upward_and_cools_downward_across_its_range() {
    let shot = camera().as_shot_white_balance();

    let untouched = balanced(None);
    let named = balanced(Some(shot));
    assert!(biggest_change(&named, &untouched) <= 2, "as shot by name moved {}", biggest_change(&named, &untouched));
    let at = |kelvin: f32| balanced(Some(WhiteBalance { temperature: kelvin, ..shot }));
    let moved = |step: f32| biggest_change(&at(shot.temperature + step), &untouched);
    no_jump("one step of the slider", moved(10.0), moved(100.0));

    assert!(warmth(&at(shot.temperature + 1000.0)) > warmth(&untouched) * 1.05, "+1000 K is not warmer");
    assert!(warmth(&at(shot.temperature - 1000.0)) < warmth(&untouched) / 1.05, "−1000 K is not cooler");

    let mut previous = 0.0;
    for kelvin in (2000..=15000).step_by(500) {
        let out = at(kelvin as f32);
        let now = warmth(&out);
        assert!(now >= previous, "{kelvin} K is cooler than the step before: {now} < {previous}");
        previous = now;
        assert!(has_a_picture(&out), "{kelvin} K clipped the ramp");
        in_order(&out).unwrap_or_else(|problem| panic!("{kelvin} K: {problem}"));
    }
}

#[test]
fn tint_moves_green_against_magenta_steadily_across_its_range() {
    let shot = camera().as_shot_white_balance();
    let untouched = balanced(None);
    let at = |tint: f32| balanced(Some(WhiteBalance { tint, ..shot }));
    let moved = |step: f32| biggest_change(&at(shot.tint + step), &untouched);
    no_jump("tint", moved(1.0), moved(10.0));
    let (up, down) = (greenness(&at(shot.tint + 30.0)), greenness(&at(shot.tint - 30.0)));
    let base = greenness(&untouched);

    assert!((up - base) * (down - base) < 0.0 && (up - base).abs() > 0.03, "±30 tint: {down} / {base} / {up}");

    let sign = (up - base).signum();
    let mut previous = greenness(&at(-150.0)) - sign;
    for tint in (-150..=150).step_by(15) {
        let out = at(tint as f32);
        let now = greenness(&out);
        assert!((now - previous) * sign >= 0.0, "tint {tint} turned back: {previous} → {now}");
        previous = now;
        assert!(has_a_picture(&out), "tint {tint} clipped the ramp");
    }
}

#[test]
fn the_stored_tint_is_greener_upward() {
    let shot = camera().as_shot_white_balance();
    let untouched = balanced(None);
    let at = |tint: f32| balanced(Some(WhiteBalance { tint, ..shot }));
    let (up, down, base) = (greenness(&at(shot.tint + 30.0)), greenness(&at(shot.tint - 30.0)), greenness(&untouched));
    assert!(up > base && down < base, "−30 / 0 / +30 greenness: {down} / {base} / {up}");
}

fn grey_card_under(light: WhiteBalance) -> LinearImage {
    let gains = camera().multipliers(light);
    LinearImage::new(8, 8, (0..64).flat_map(|_| gains.map(|gain| MIDDLE_GREY / gain)).collect()).with_profile(camera())
}

fn pick_a_neutral_under(light: WhiteBalance) -> (WhiteBalance, [u8; 3]) {
    let card = grey_card_under(light);
    let shown = develop(&plain(), &card, &RenderInputs::default());

    let picked = shown.get_pixel(4, 4).0.map(|code| scene_value_for(code as f32 / 255.0));
    let found = camera().neutral_balance(picked).expect("a measurable grey");
    let mut document = plain();
    document.white_balance = Some(found);
    (found, develop(&document, &card, &RenderInputs::default()).get_pixel(4, 4).0)
}

const LIGHTS: [WhiteBalance; 2] =
    [WhiteBalance { temperature: 3200.0, tint: 0.0 }, WhiteBalance { temperature: 8000.0, tint: 20.0 }];

#[test]
fn pick_a_neutral_moves_a_tinted_grey_toward_grey() {
    for light in LIGHTS {
        let card = grey_card_under(light);
        let before = develop(&plain(), &card, &RenderInputs::default()).get_pixel(4, 4).0;
        let (_, after) = pick_a_neutral_under(light);
        let spread = |p: [u8; 3]| p.iter().max().unwrap() - p.iter().min().unwrap();
        assert!(spread(after) < spread(before), "{light:?}: {before:?} → {after:?}");

        let linear = to_working_space(&plain(), &card, &RenderInputs::default());
        let found = camera().neutral_balance([linear.data[0], linear.data[1], linear.data[2]]).expect("a grey");
        assert!((found.temperature / light.temperature - 1.0).abs() < 0.02, "{light:?}: found {found:?}");
    }
}

#[test]
fn pick_a_neutral_makes_the_grey_it_was_pointed_at_grey() {
    for light in LIGHTS {
        let (found, after) = pick_a_neutral_under(light);
        let spread = after.iter().max().unwrap() - after.iter().min().unwrap();
        assert!(spread <= 3, "{light:?} picked as {found:?} renders {after:?}");
    }
}

#[test]
fn a_masks_white_balance_goes_the_way_the_panels_does() {
    let shot = camera().as_shot_white_balance();
    let working = to_working_space(&plain(), &camera_ramp(), &RenderInputs::default());
    let masked = |temperature: f32, tint: f32| {

        let mut mask = Mask::new(Shape::Linear { from: [-2.0, 0.5], to: [-1.0, 0.5] });
        mask.basic.balance.temperature = temperature;
        mask.basic.balance.tint = tint;
        let mut document = plain();
        document.set_masks(vec![mask]);
        render(&document, &working)
    };
    let untouched = render(&plain(), &working);
    assert_eq!(masked(0.0, 0.0), untouched, "a mask asking for nothing changed the frame");

    let panel = |step: f32, tint: f32| {
        balanced(Some(WhiteBalance { temperature: shot.temperature + step, tint: shot.tint + tint }))
    };
    let base = (warmth(&untouched), greenness(&untouched));
    for step in [-800.0, 800.0] {
        let (local, global) = (warmth(&masked(step, 0.0)) / base.0, warmth(&panel(step, 0.0)) / base.0);
        assert!((local - 1.0) * (global - 1.0) > 0.0, "{step} K: mask ×{local}, panel ×{global}");
        assert!((local.ln() / global.ln() - 1.0).abs() < 0.5, "{step} K: mask ×{local}, panel ×{global}");
    }
    assert!(warmth(&masked(800.0, 0.0)) > base.0, "a mask's +800 K is not warmer");
    for tint in [-30.0, 30.0] {
        let (local, global) = (greenness(&masked(0.0, tint)) / base.1, greenness(&panel(0.0, tint)) / base.1);
        assert!((local - 1.0) * (global - 1.0) > 0.0, "tint {tint}: mask ×{local}, panel ×{global}");
    }
    for (temperature, tint) in [(-2000.0, -150.0), (2000.0, 150.0)] {
        assert!(has_a_picture(&masked(temperature, tint)), "{temperature} K {tint} clipped the ramp");
    }
}

fn saturation(b: &mut Basic, value: f32) {
    b.presence.saturation = value;
}
fn vibrance(b: &mut Basic, value: f32) {
    b.presence.vibrance = value;
}

fn saturations(set: fn(&mut Basic, f32), value: f32) -> Vec<(f32, f32)> {
    let frame = patches();
    let (before, after) = (render(&plain(), &frame), render(&with(Basic::with(|b| set(b, value))), &frame));
    (0..PATCHES).map(|index| (hsv(patch(&before, index)).1, hsv(patch(&after, index)).1)).collect()
}

#[test]
fn saturation_moves_every_colour_toward_or_away_from_grey_and_not_its_hue() {
    neutral_is_untouched_and_continuous(&patches(), saturation, 1.0);
    let frame = patches();
    let untouched = render(&plain(), &frame);

    let greyed = saturations(saturation, -100.0);
    for (index, (_, after)) in greyed.iter().enumerate() {
        assert!(*after < 0.03, "patch {index} kept saturation {after} at −100");
    }
    for (index, (before, after)) in saturations(saturation, 50.0).iter().enumerate().take(SKIN + 1) {
        assert!(after > before, "patch {index} did not gain colour at +50: {before} → {after}");
    }
    let lifted = render(&with(Basic::with(|b| b.presence.saturation = 50.0)), &frame);
    for index in 0..=SKIN {
        let (was, now) = (hsv(patch(&untouched, index)).0, hsv(patch(&lifted, index)).0);
        assert!(turn(was, now).abs() < 8.0, "patch {index} turned from {was} to {now}");
    }
    assert_eq!(patch(&lifted, GREY), patch(&untouched, GREY), "grey took colour");

    for value in [-100.0, 100.0, -50.0, 50.0] {
        let out = render(&with(Basic::with(|b| b.presence.saturation = value)), &frame);
        patches_survive(&out).unwrap_or_else(|problem| panic!("at {value}: {problem}"));
        assert!(biggest_change(&out, &untouched) >= 8, "{value} barely moved");
    }
}

#[test]
fn vibrance_moves_muted_colours_more_than_strong_ones() {
    neutral_is_untouched_and_continuous(&patches(), vibrance, 1.0);
    let frame = patches();
    let untouched = render(&plain(), &frame);

    let gain = |values: &[(f32, f32)], index: usize| values[index].1 / values[index].0;
    let up = saturations(vibrance, 50.0);
    assert!(gain(&up, SKIN) > 1.02 && gain(&up, SKIN) > gain(&up, 0), "skin ×{} red ×{}", gain(&up, SKIN), gain(&up, 0));
    let down = saturations(vibrance, -50.0);
    assert!(gain(&down, SKIN) < 0.98 && gain(&down, SKIN) < gain(&down, 0), "skin ×{} red ×{}", gain(&down, SKIN), gain(&down, 0));
    let out = render(&with(Basic::with(|b| b.presence.vibrance = 100.0)), &frame);
    assert_eq!(patch(&out, GREY), patch(&untouched, GREY), "grey took colour");

    for value in [-100.0, 100.0, -50.0, 50.0] {
        let out = render(&with(Basic::with(|b| b.presence.vibrance = value)), &frame);
        patches_survive(&out).unwrap_or_else(|problem| panic!("at {value}: {problem}"));
        assert!(biggest_change(&out, &untouched) >= 8, "{value} barely moved");
    }
}

fn with_mixer(band: usize, channel: usize, value: f32) -> Document {
    let mut mixer = Mixer::default();
    mixer.bands[band][channel] = value;
    let mut document = plain();
    document.set_mixer(mixer);
    document
}

fn mixer_band_check(channel: usize, measure: fn([f32; 3], [f32; 3]) -> f32, least: f32) {
    let frame = patches();
    let untouched = render(&plain(), &frame);
    assert_eq!(render(&with_mixer(0, channel, 0.0), &frame), untouched);
    for band in 0..8 {
        let name = HUES[band].0;
        let moved = |value: f32| biggest_change(&render(&with_mixer(band, channel, value), &frame), &untouched);
        no_jump(name, moved(1.0), moved(10.0));
        for sign in [-1.0, 1.0] {
            let out = render(&with_mixer(band, channel, 100.0 * sign), &frame);
            let own = measure(patch(&untouched, band), patch(&out, band)) * sign;
            assert!(own >= least, "{name} {sign:+}: its own patch moved {own}");
            let half = render(&with_mixer(band, channel, 50.0 * sign), &frame);
            let middle = measure(patch(&untouched, band), patch(&half, band)) * sign;
            assert!(middle >= least / 3.0, "{name} {sign:+}50: its own patch moved {middle}");
            let far = (band + 4) % 8;
            let leak = measure(patch(&untouched, far), patch(&out, far));
            assert!(leak.abs() <= least / 5.0, "{name} {sign:+}: {} moved {leak}", HUES[far].0);
            patches_survive(&out).unwrap_or_else(|problem| panic!("{name} {sign:+}: {problem}"));
        }
    }
}

fn mixer_leaves_grey(channel: usize) {
    let frame = LinearImage::new(8, 8, vec![MIDDLE_GREY; 8 * 8 * 3]);
    let untouched = render(&plain(), &frame);
    let worst = (0..8)
        .flat_map(|band| [-100.0, 100.0].map(|value| (band, value)))
        .map(|(band, value)| {
            let out = render(&with_mixer(band, channel, value), &frame);
            (biggest_change(&out, &untouched), HUES[band].0, value, out.get_pixel(4, 4).0)
        })
        .max_by_key(|found| found.0)
        .expect("sixteen renders");
    assert!(worst.0 <= 1, "{} {}: grey moved {} codes, to {:?}", worst.1, worst.2, worst.0, worst.3);
}

#[test]
fn a_mixer_hue_turns_its_own_colour_the_way_the_wheel_goes() {

    mixer_band_check(0, |before, after| turn(hsv(before).0, hsv(after).0), 6.0);
    mixer_leaves_grey(0);
}

#[test]
fn a_mixer_saturation_moves_its_own_colour_toward_or_away_from_grey() {
    mixer_band_check(1, |before, after| (hsv(after).1 - hsv(before).1) * 100.0, 10.0);
    mixer_leaves_grey(1);
}

#[test]
fn a_mixer_luminance_lightens_or_darkens_its_own_colour() {
    mixer_band_check(2, |before, after| luma(after) - luma(before), 8.0);
}

#[test]
fn a_mixer_luminance_leaves_neutral_grey_alone() {
    mixer_leaves_grey(2);
}

fn with_point(change: impl FnOnce(&mut PointColour)) -> Document {
    let mut point = PointColour::picked(patch_colour(1));
    change(&mut point);
    let mut document = plain();
    document.set_point_colours(PointColours { points: vec![point], highlight: None });
    document
}

#[test]
fn a_point_colour_moves_the_colour_it_was_picked_from_and_not_the_far_side_of_the_wheel() {
    let frame = patches();
    let untouched = render(&plain(), &frame);
    assert_eq!(render(&with_point(|_| ()), &frame), untouched, "a point just picked changed the frame");
    let (orange, blue) = (1, 5);

    for (name, set) in POINT_SLIDERS {
        let moved = |value: f32| biggest_change(&render(&with_point(|p| set(p, value)), &frame), &untouched);
        no_jump(name, moved(1.0), moved(10.0));
        for value in [-100.0, -50.0, 50.0, 100.0] {
            let out = render(&with_point(|p| set(p, value)), &frame);
            let (was, now) = (patch(&untouched, orange), patch(&out, orange));
            let moved = match name {
                "hue" => turn(hsv(was).0, hsv(now).0),
                "saturation" => (hsv(now).1 - hsv(was).1) * 100.0,
                _ => luma(now) - luma(was),
            } * value.signum();
            let least = if value.abs() == 100.0 { 6.0 } else { 2.0 };
            assert!(moved >= least, "{name} {value}: orange moved {moved}");
            let off = biggest_change_of(patch(&untouched, blue), patch(&out, blue));
            assert!(off <= 1.0, "{name} {value}: blue moved {off}");
            patches_survive(&out).unwrap_or_else(|problem| panic!("{name} {value}: {problem}"));
        }
    }
}

const POINT_SLIDERS: [(&str, fn(&mut PointColour, f32)); 3] = [
    ("hue", |p, v| p.hue = v),
    ("saturation", |p, v| p.saturation = v),
    ("luminance", |p, v| p.luminance = v),
];

fn biggest_change_of(a: [f32; 3], b: [f32; 3]) -> f32 {
    (0..3).map(|c| (a[c] - b[c]).abs()).fold(0.0, f32::max)
}

#[test]
fn a_point_colour_leaves_neutral_grey_alone() {
    let frame = patches();
    let untouched = patch(&render(&plain(), &frame), GREY);
    for (name, set) in POINT_SLIDERS {
        for value in [-100.0, 100.0] {
            let grey = patch(&render(&with_point(|p| set(p, value)), &frame), GREY);
            let moved = biggest_change_of(grey, untouched);
            assert!(moved <= 1.0, "{name} {value}: grey moved {moved} codes, {untouched:?} → {grey:?}");
        }
    }
}

#[test]
fn a_point_colours_range_widens_what_it_reaches() {
    let frame = patches();
    let untouched = render(&plain(), &frame);
    let greyed = |range: f32| render(&with_point(|p| { p.saturation = -100.0; p.range = range }), &frame);

    let loss = |out: &RgbImage, index: usize| hsv(patch(&untouched, index)).1 - hsv(patch(out, index)).1;
    let (narrow, middle, wide) = (greyed(0.0), greyed(50.0), greyed(100.0));
    for out in [&narrow, &middle, &wide] {
        assert!(loss(out, 1) > 0.3, "orange itself kept its colour");
    }
    let reach = |out: &RgbImage| loss(out, 0) + loss(out, 2) + loss(out, SKIN);
    assert!(reach(&narrow) < reach(&middle) && reach(&middle) < reach(&wide),
        "reach {} / {} / {}", reach(&narrow), reach(&middle), reach(&wide));

    assert!(loss(&wide, 5) <= 0.01 && patch(&wide, GREY) == patch(&untouched, GREY), "blue or grey reached");
}

#[test]
fn show_affected_area_greys_out_what_the_point_does_not_reach() {
    let frame = patches();
    let mut document = with_point(|p| p.saturation = 20.0);
    let mut points = document.point_colours();
    points.highlight = Some(0);
    document.set_point_colours(points);
    let shown = render(&document, &frame);
    assert!(hsv(patch(&shown, 1)).1 > 0.3, "orange was greyed: {:?}", patch(&shown, 1));
    for far in [3, 4, 5] {
        assert!(hsv(patch(&shown, far)).1 < 0.05, "{} kept its colour", HUES[far].0);
    }
}

fn shadow_tint(b: &mut Basic, value: f32) {
    b.calibration.shadow_tint = value;
}

#[test]
fn shadow_tint_goes_magenta_upward_in_the_shadows_only() {
    neutral_is_untouched_and_continuous(&ramp(), shadow_tint, 1.0);
    let frame = ramp();
    let dark = column(0.03);
    let pixel = |value: f32, x: u32| render(&with(Basic::with(|b| b.calibration.shadow_tint = value)), &frame).get_pixel(x, 1).0;
    let untouched = render(&plain(), &frame);
    for value in [50.0, 100.0] {
        let [r, g, b] = pixel(value, dark).map(|c| c as i32);
        assert!(g < r && g < b, "+{value}: shadow not magenta: {r} {g} {b}");
        let [r, g, b] = pixel(-value, dark).map(|c| c as i32);
        assert!(g > r && g > b, "−{value}: shadow not green: {r} {g} {b}");
    }
    for value in [-100.0, 100.0] {
        for scene in [0.5, 1.0] {
            let x = column(scene);
            assert_eq!(pixel(value, x), untouched.get_pixel(x, 1).0, "{value} reached {scene}");
        }
        let out = render(&with(Basic::with(|b| b.calibration.shadow_tint = value)), &frame);
        assert!(has_a_picture(&out));
    }
}

fn calibrated(primary: usize, hue: bool, value: f32) -> RgbImage {
    let basic = Basic::with(|b| {
        let c = &mut b.calibration;
        let slot = match (primary, hue) {
            (0, true) => &mut c.red_hue,
            (0, false) => &mut c.red_saturation,
            (1, true) => &mut c.green_hue,
            (1, false) => &mut c.green_saturation,
            (2, true) => &mut c.blue_hue,
            _ => &mut c.blue_saturation,
        };
        *slot = value;
    });
    render(&with(basic), &patches())
}

const PRIMARY: [(usize, usize); 3] = [(0, 4), (3, 7), (5, 2)];

fn calibration_move(untouched: &RgbImage, out: &RgbImage, index: usize, hue: bool) -> f32 {
    let (was, now) = (hsv(patch(untouched, index)), hsv(patch(out, index)));
    if hue { turn(was.0, now.0) } else { (now.1 - was.1) * 100.0 }
}

fn primary_check(primary: usize, hue: bool) {
    let untouched = render(&plain(), &patches());
    let own = PRIMARY[primary].0;
    assert_eq!(calibrated(primary, hue, 0.0), untouched);
    let moved = |value: f32| biggest_change(&calibrated(primary, hue, value), &untouched);
    no_jump("calibration", moved(1.0), moved(10.0));
    for value in [-100.0, -50.0, 50.0, 100.0] {
        let out = calibrated(primary, hue, value);
        let moved = calibration_move(&untouched, &out, own, hue) * value.signum();
        let least = if value.abs() == 100.0 { 6.0 } else { 2.0 };
        assert!(moved >= least, "{value}: patch {own} moved {moved}");
        let grey = patch(&out, GREY);
        assert!(biggest_change_of(grey, patch(&untouched, GREY)) <= 1.0, "{value}: grey moved to {grey:?}");
        patches_survive(&out).unwrap_or_else(|problem| panic!("{value}: {problem}"));
    }
}

fn primary_stays_its_own(primary: usize, hue: bool) {
    let untouched = render(&plain(), &patches());
    let (own, far) = PRIMARY[primary];
    for value in [-100.0, -50.0, 50.0, 100.0] {
        let out = calibrated(primary, hue, value);
        let moved = calibration_move(&untouched, &out, own, hue);
        let leak = calibration_move(&untouched, &out, far, hue);
        assert!(leak.abs() <= moved.abs() / 3.0, "{value}: patch {far} moved {leak} against patch {own}'s {moved}");
    }
}

#[test]
fn red_primary_hue_turns_red_toward_yellow_and_leaves_grey() {
    primary_check(0, true);
    primary_stays_its_own(0, true);
}

#[test]
fn red_primary_saturation_moves_red_and_leaves_grey() {
    primary_check(0, false);
}

#[test]
fn red_primary_saturation_leaves_its_complement_alone() {
    primary_stays_its_own(0, false);
}

#[test]
fn green_primary_hue_turns_green_toward_cyan_and_leaves_grey() {
    primary_check(1, true);
    primary_stays_its_own(1, true);
}

#[test]
fn green_primary_saturation_moves_green_and_leaves_grey() {
    primary_check(1, false);
}

#[test]
fn green_primary_saturation_leaves_its_complement_alone() {
    primary_stays_its_own(1, false);
}

#[test]
fn blue_primary_hue_turns_blue_toward_purple_and_leaves_grey() {
    primary_check(2, true);
    primary_stays_its_own(2, true);
}

#[test]
fn blue_primary_saturation_moves_blue_and_leaves_grey() {
    primary_check(2, false);
}

#[test]
fn blue_primary_saturation_leaves_its_complement_alone() {
    primary_stays_its_own(2, false);
}
