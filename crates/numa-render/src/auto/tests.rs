use super::*;
use numa_core::document::Perspective;
use numa_core::plane::Plane;
use numa_core::curve::Curve;
use numa_core::mask::AUTO_SUBJECT;

fn fresh() -> Document {
    Document::new("frame".to_string())
}

#[test]
#[ignore]
fn auto_before_and_after() {
    let (Ok(path), Ok(out)) = (std::env::var("FRAME"), std::env::var("OUT")) else {
        println!("set FRAME and OUT");
        return;
    };
    let path = std::path::PathBuf::from(path);
    let stem = path.file_stem().unwrap().to_string_lossy().to_string();
    let linear = numa_io::raw::decode_linear(&path).unwrap();
    let mut document = Document::new(path.display().to_string());
    framing_from_env(&mut document);
    let working = crate::to_working_space(&document, &linear, &Default::default());

    let before = crate::apply_stack(&document, &working, 1.0);
    let (mut after, auto, _) = run(&mut document, &working);
    report(&stem, &auto);

    let mut basic = document.basic();
    if let Ok(hdr) = std::env::var("HDR") {
        basic.presence.hdr = hdr.parse().unwrap();
        document.set_basic(basic);
        after = crate::apply_stack(&document, &working, 1.0);
    }
    if std::env::var("MASK").as_deref() == Ok("0") {
        document.set_masks(Vec::new());
        after = crate::apply_stack(&document, &working, 1.0);
    }

    let shrink = |image: &image::RgbImage| {
        let scale = 1400.0 / image.width().max(image.height()) as f32;
        image::imageops::resize(
            image,
            (image.width() as f32 * scale) as u32,
            (image.height() as f32 * scale) as u32,
            image::imageops::FilterType::Lanczos3,
        )
    };
    for (name, image) in [("voor", &before), ("na", &after)] {
        let file = format!("{out}/{stem}-{name}.jpg");
        shrink(image).save(&file).unwrap();
        println!("  {file}");
    }
}

#[test]
#[ignore]
fn what_auto_now_does() {
    let listed = std::env::var("FRAMES_LIST")
        .ok()
        .map(|list| std::fs::read_to_string(&list).unwrap_or_else(|error| panic!("FRAMES_LIST {list}: {error}")));
    let paths = match (listed, std::env::var("FRAME")) {
        (Some(list), _) => list.lines().map(str::trim).filter(|line| !line.is_empty()).map(std::path::PathBuf::from).collect(),
        (None, Ok(one)) => vec![std::path::PathBuf::from(one)],
        (None, Err(_)) => {
            let Ok(dir) = std::env::var("RAF_DIR") else {
                println!("set RAF_DIR or FRAME to run this");
                return;
            };
            let mut found: Vec<_> = std::fs::read_dir(&dir)
                .unwrap()
                .flatten()
                .map(|entry| entry.path())
                .filter(|path| numa_io::raw::is_supported(path))
                .collect();
            found.sort();
            let count: usize =
                std::env::var("FRAMES").ok().and_then(|n| n.parse().ok()).unwrap_or(8);
            let step = (found.len() / count.max(1)).max(1);
            found.into_iter().step_by(step).take(count).collect()
        }
    };

    for path in paths {
        let Ok(full) = numa_io::raw::decode_linear(&path) else { continue };

        let linear = full.downscaled(2000).unwrap_or(full);
        let mut document = Document::new(path.display().to_string());
        framing_from_env(&mut document);
        let working = crate::to_working_space(&document, &linear, &Default::default());
        let before = crate::apply_stack(&document, &working, 1.0);
        let (after, auto, subject) = run(&mut document, &working);

        let stem = path.file_stem().unwrap().to_string_lossy().to_string();
        report(&stem, &auto);
        let (rw, rh) = crate::raster_size(before.width(), before.height(), crate::MASK_RASTER);
        let read = |image: &image::RgbImage| measure(image, subject.as_ref(), rw, rh);
        let (was_subject, was_low, was_median, was_high) = read(&before);
        let (is_subject, is_low, is_median, is_high) = read(&after);

        let code = |v: f32| v * 255.0;
        println!(
            "    onderwerp {:.0} -> {:.0}   p1 {:.0} -> {:.0}   \
p50 {:.0} -> {:.0}   p99 {:.0} -> {:.0}",
            code(was_subject), code(is_subject),
            code(was_low), code(is_low),
            code(was_median), code(is_median),
            code(was_high), code(is_high),
        );

        if let Ok(csv) = std::env::var("OUT_CSV") {
            use std::io::Write;
            let new = !std::path::Path::new(&csv).exists();
            let mut file = std::fs::OpenOptions::new().create(true).append(true).open(&csv).unwrap();
            if new {
                writeln!(
                    file,
                    "frame,framing,exposure,highlights,whites,blacks,hdr,vibrance,lift,\
subject_before,subject_after,p1_before,p1_after,p50_before,p50_after,p99_before,p99_after"
                )
                .unwrap();
            }
            let basic = &auto.basic;
            let framing = serde_json::to_string(&crate::mask_geometry(&document).operations).unwrap_or_default();
            writeln!(
                file,
                "{stem},\"{}\",{:.3},{:.1},{:.1},{:.1},{:.1},{:.1},{:.3},{:.1},{:.1},{:.1},{:.1},{:.1},{:.1},{:.1},{:.1}",
                framing.replace('"', "\"\""),
                basic.tone.exposure,
                basic.tone.highlights,
                basic.tone.whites,
                basic.tone.blacks,
                basic.presence.hdr,
                basic.presence.vibrance,
                auto.subject.unwrap_or(0.0),
                code(was_subject), code(is_subject),
                code(was_low), code(is_low),
                code(was_median), code(is_median),
                code(was_high), code(is_high),
            )
            .unwrap();
        }
    }
}

#[test]
#[ignore]
fn the_subject_mask_as_a_picture() {
    let (Ok(path), Ok(out)) = (std::env::var("FRAME"), std::env::var("OUT")) else { return };
    let path = std::path::PathBuf::from(path);
    let full = numa_io::raw::decode_linear(&path).unwrap();

    let proxy = match std::env::var("FULL").is_ok() {
        true => full,
        false => full.downscaled(2400).unwrap_or(full),
    };
    let document = Document::new(path.display().to_string());
    let working = crate::to_working_space(&document, &proxy, &Default::default());
    let frame = crate::apply_stack(&document, &working, 1.0);
    let (rw, rh) = crate::raster_size(frame.width(), frame.height(), crate::MASK_RASTER);
    let found = crate::segment::of(&frame).expect("model installed");
    println!("segmentation photo {}x{}, raster {rw}x{rh}", found.photo().width(), found.photo().height());

    if std::env::var("SAVE_PHOTO").is_ok() {
        let stem = path.file_stem().unwrap().to_string_lossy().to_string();
        found.photo().save(format!("{out}/photo-{stem}.png")).unwrap();
    }
    for (class, share) in found.present() {
        println!("  class {class} {:?} {:.2}%", crate::segment::label(class), share * 100.0);
    }

    let classes: Vec<u16> = std::env::var("CLASSES")
        .ok()
        .map(|list| list.split(',').filter_map(|c| c.parse().ok()).collect())
        .unwrap_or_else(|| crate::segment::MATTEABLE.to_vec());
    let mut mask = Mask::new(Shape::Segment { classes: classes.clone() });
    mask.matte = classes.iter().all(|class| crate::segment::MATTEABLE.contains(class));
    crate::resolve_mask(&mut mask, Some(&found), None, Some(&frame), rw, rh);
    for (name, pixels) in [("unshaped", &mask.unshaped), ("map", &mask.map)] {
        let Some(alpha) = pixels.0.as_deref() else { println!("{name}: none"); continue };
        let share: f64 = alpha.values().map(|v| v as f64).sum::<f64>() / alpha.len() as f64;
        println!("{name}: {:.2}% of the frame, feather {} shift {}", share * 100.0, mask.feather, mask.shift);
        let image = image::GrayImage::from_fn(rw as u32, rh as u32, |x, y| {
            image::Luma([(alpha.at(y as usize * rw + x as usize).clamp(0.0, 1.0) * 255.0) as u8])
        });
        let file = format!("{out}/mask-{name}.png");
        image.save(&file).unwrap();
        println!("  {file}");
    }
}

fn framing_from_env(document: &mut Document) {
    let number = |name: &str| std::env::var(name).ok().and_then(|value| value.trim().parse::<f32>().ok());
    if let Ok(crop) = std::env::var("CROP") {
        let values: Vec<f32> = crop.split(',').filter_map(|value| value.trim().parse().ok()).collect();
        if let [x, y, width, height, rest @ ..] = values.as_slice() {
            document.set_crop([*x, *y, *width, *height], rest.first().copied().unwrap_or(0.0));
        }
    }
    if let Some(turn) = number("TURN") {
        document.set_rotation(turn);
    }
    if std::env::var("MIRROR").as_deref() == Ok("1") {
        document.set_mirrored(true);
    }
    if let Some(vertical) = number("KEYSTONE") {
        document.set_perspective(Perspective { vertical, ..document.perspective() });
    }
}

fn run(document: &mut Document, working: &LinearImage) -> (image::RgbImage, Auto, Option<Alpha>) {
    run_meant(document, working, None)
}

fn run_meant(document: &mut Document, working: &LinearImage, compensation: Option<f32>) -> (image::RgbImage, Auto, Option<Alpha>) {
    let frame = crate::apply_stack(&crate::mask_geometry(document), working, 1.0);
    let (rw, rh) = crate::raster_size(frame.width(), frame.height(), crate::MASK_RASTER);
    let found = crate::segment::of(&frame);
    let resolve = |mask: &mut Mask| {
        crate::resolve_mask(mask, found.as_ref(), None, Some(&frame), rw, rh)
    };

    let subject = found
        .as_ref()
        .and_then(|found| subject_mask(found, &frame, rw, rh))
        .and_then(|mask| mask.map.0)
        .map(|kept| kept.to_alpha());

    let faces = faces_in(&frame);
    let animal = found.as_ref().is_some_and(|found| found.coarse(&[126]).data.iter().any(|share| *share > 0.5));

    let focus = numa_io::raw::af_point(std::path::Path::new(&document.source.path))
        .filter(|point| !point.zone && document.crop().is_none() && document.rotation() == 0.0 && !document.mirrored())
        .map(|point| [point.x, point.y]);
    let lit = subject.and_then(|subject| lit_part(&subject, &faces, animal, focus));
    let lights = found.as_ref().map(|found| found.alpha(&LIGHTS));
    let path = std::path::Path::new(&document.source.path);
    let intent = Intent::of(
        compensation,
        numa_io::raw::dynamic_range_mode(path),
        numa_io::raw::colour_setting(path),
    );
    let told = Told {
        subject: lit.as_ref().map(|lit| &lit.alpha),
        lights: lights.as_ref(),
        faces: faces.iter().map(|[_, _, w, h]| w * h).sum(),
        intent,
    };
    let auto = tone_told(&framed(document, working), document, &told);
    let subject = lit.map(|lit| lit.alpha);
    auto.apply(document);

    let mut masks = document.masks();
    masks.iter_mut().filter(|mask| mask.is_pending()).for_each(resolve);
    if let (Ok(out), Some(mask)) = (std::env::var("OUT"), masks.last()) {
        for (name, pixels) in [("run-unshaped", &mask.unshaped), ("run-map", &mask.map)] {
            if let Some(alpha) = pixels.0.as_deref() {
                let image = image::GrayImage::from_fn(rw as u32, rh as u32, |x, y| {
                    image::Luma([(alpha.at(y as usize * rw + x as usize).clamp(0.0, 1.0) * 255.0) as u8])
                });
                image.save(format!("{out}/{name}.png")).unwrap();
            }
        }
    }
    document.set_masks(masks);

    (crate::apply_stack(document, working, 1.0), auto, subject)
}

fn report(stem: &str, auto: &Auto) {
    let basic = &auto.basic;
    println!(
        "{stem}: exposure {:+.2}  blacks {:.0}  whites {:.0}  highlights {:.0}  hdr {:.0}  vibrance {:.0}  onderwerp {}",
        basic.tone.exposure,
        basic.tone.blacks,
        basic.tone.whites,
        basic.tone.highlights,
        basic.presence.hdr,
        basic.presence.vibrance,
        match auto.subject {
            Some(stops) => format!("belichting {stops:+.2} EV op het onderwerp"),
            None => "geen".to_string(),
        }
    );
}

fn measure(
    image: &image::RgbImage,
    subject: Option<&Alpha>,
    rw: usize,
    rh: usize,
) -> (f32, f32, f32, f32) {
    let (mut inside, mut all) = (Vec::new(), Vec::new());
    for (x, y, pixel) in image.enumerate_pixels() {
        let luma = (0.2126 * pixel[0] as f32
            + 0.7152 * pixel[1] as f32
            + 0.0722 * pixel[2] as f32)
            / 255.0;
        all.push(luma);
        if let Some(alpha) = subject {
            let ax = (x as usize * rw / image.width() as usize).min(rw - 1);
            let ay = (y as usize * rh / image.height() as usize).min(rh - 1);
            if alpha.data[ay * rw + ax] > 0.6 {
                inside.push(luma);
            }
        }
    }
    all.sort_by(f32::total_cmp);
    inside.sort_by(f32::total_cmp);
    (
        inside.get(inside.len() / 2).copied().unwrap_or(-1.0),
        all[all.len() / 100],
        all[all.len() / 2],
        all[all.len() * 99 / 100],
    )
}

#[test]
fn a_frame_with_no_lines_is_left_alone() {
    let (w, h) = (128usize, 128usize);
    let mut seed = 0x9e3779b9u32;
    let noise = (0..w * h)
        .map(|_| {
            seed ^= seed << 13;
            seed ^= seed >> 17;
            seed ^= seed << 5;
            (seed % 1000) as f32 / 1000.0
        })
        .collect();
    let plane = Plane::new(w, h, noise);
    assert_eq!(level(&plane), None);
    let votes = evidence::votes(&plane, |_, _| (1, 1.0));
    assert!(evidence::vanishing(&votes, w, h, 1.0).is_none());
    assert!(evidence::verticals(&votes, w, h).is_none());
}

fn level(luma: &Plane) -> Option<f32> {
    let (edges, _) = super::level::coherent_edges(luma)?;
    let votes: Vec<(f32, f32)> = edges
        .into_iter()
        .filter_map(|(_, _, gx, gy, strength)| {
            let turned = if gy.abs() < gx.abs() * 0.27 {
                (gy / gx).atan()
            } else if gx.abs() < gy.abs() * 0.27 {
                -(gx / gy).atan()
            } else {
                return None;
            };
            Some((turned.to_degrees(), strength))
        })
        .collect();
    super::level::crowded_by(&votes, 2.5)
}

#[test]
fn a_crooked_frame_is_levelled_by_its_horizon_or_its_verticals() {
    let (w, h) = (480usize, 360usize);
    let horizon = |x: usize, y: usize| -> f32 { if y < h / 2 { 0.8 } else if (x / 7) % 2 == 0 { 0.15 } else { 0.1 } };
    let building = |x: usize, _: usize| -> f32 { if (x / 40) % 2 == 0 { 0.7 } else { 0.2 } };
    let luma = |image: &LinearImage| {
        Plane::new(
            image.width as usize,
            image.height as usize,
            image.data.chunks_exact(3).map(|pixel| pixel[1]).collect(),
        )
    };
    let centre = [0.2, 0.2, 0.6, 0.6];

    for (name, scene) in [("horizon", &horizon as &dyn Fn(usize, usize) -> f32), ("building", &building)] {
        let flat = LinearImage::new(
            w as u32,
            h as u32,
            (0..w * h).flat_map(|index| [scene(index % w, index / w); 3]).collect(),
        );
        for tilt in [-4.0f32, 2.5] {
            let shot = flat.cropped([0.0, 0.0, 1.0, 1.0], tilt, Perspective::default());
            let found = level(&luma(&shot.cropped(centre, 0.0, Perspective::default()))).unwrap_or(0.0);
            assert!((found + tilt).abs() < 0.3, "{name} shot at {tilt}°: levelled by {found}°");

            let levelled = shot.cropped(centre, found, Perspective::default());
            let again = level(&luma(&levelled)).unwrap_or(99.0);
            assert!(again.abs() < 0.3, "{name} at {tilt}°, levelled, still asks for {again}°");
        }
    }
}

#[test]
fn an_endpoint_that_cannot_reach_its_target_stays_put() {
    let look = Look { curve: None, finished: false };
    let blacks = |lit: f32, amount: f32| look.display(lit, &Basic::with(|b| b.tone.blacks = amount));
    let whites = |lit: f32, amount: f32| look.display(lit, &Basic::with(|b| b.tone.whites = amount));

    let unreachable = solve(|amount| blacks(0.0004, amount), 0.5);
    assert_eq!(unreachable, 0.0, "a target it cannot reach is not an answer");

    let resting = whites(0.9, 0.0);
    assert_eq!(solve(|amount| whites(0.9, amount), resting), 0.0);

    let wanted = whites(0.9, 40.0);
    let found = solve(|amount| whites(0.9, amount), wanted);
    assert!((found - 40.0).abs() < 1.0, "solved to {found}, wanted 40");
}

#[test]
fn the_exposure_answers_to_the_ends_and_not_to_the_middle() {
    let flat = |value: f32, brightest: f32| {
        let (w, h) = (64u32, 64u32);
        let count = (w * h) as usize;
        let data = (0..count)
            .flat_map(|index| {

                let level = if index >= count - count / 150 { brightest } else { value };
                [level, level, level]
            })
            .collect();
        LinearImage::new(w, h, data)
    };

    let high_key = tone(&flat(0.55, tone::scene_value_for(0.93)), None, &fresh()).basic;
    assert_eq!(high_key.tone.exposure, 0.0, "a bright scene is not a mistake");

    let blown = tone(&flat(0.4, tone::scene_value_for(0.9995)), None, &fresh()).basic;
    assert_eq!(blown.tone.exposure, 0.0, "a blown top end keeps the exposure");
    assert!(blown.tone.whites < 0.0 || blown.tone.highlights < 0.0, "and is brought in: {:?}", blown.tone);

    let dark = tone(&flat(tone::scene_value_for(0.3), tone::scene_value_for(0.6)), None, &fresh()).basic;
    assert!(dark.tone.exposure > 0.05, "a dim frame goes up: {}", dark.tone.exposure);

    let night = tone(&flat(0.01, 0.03), None, &fresh()).basic;
    assert_eq!(night.tone.exposure, 0.0, "a night is not brought up");

    for basic in [blown, dark] {
        assert!(basic.tone.exposure.abs() <= MOST_EXPOSURE + 1e-4);
    }
}

#[test]
fn a_subject_in_shadow_is_lit_by_the_exposure_and_no_mask() {
    let (image, alpha) = subject_in_shadow();
    let auto = tone(&image, Some(&alpha), &fresh());
    let stops = auto.subject.expect("a subject five stops down is one to light");
    assert_eq!(auto.basic.tone.exposure, stops, "the frame's own exposure");
    assert!((stops - MOST_SUBJECT_EXPOSURE).abs() < 1e-4, "as far as it goes: {stops}");
    assert_eq!(auto.basic.presence.hdr, 0.0, "and no HDR look under it");
    assert_eq!(auto.basic.tone.shadows, 0.0, "nor the slider that cannot reach");

    let mut document = fresh();
    let applied = auto.apply(&mut document);
    assert!(document.masks().is_empty(), "no mask");
    assert_eq!(applied.subject, Some(stops));

    let (w, h) = (64u32, 64u32);
    let dark = LinearImage::new(w, h, vec![tone::scene_value_for(0.12); (w * h * 3) as usize]);
    assert!(tone(&dark, Some(&alpha), &fresh()).subject.is_none(), "not lit in a dark room");

    let flat = LinearImage::new(w, h, vec![tone::scene_value_for(0.75); (w * h * 3) as usize]);
    let everything = Alpha::new(w as usize, h as usize, vec![1.0; (w * h) as usize]);
    assert!(tone(&flat, Some(&everything), &fresh()).subject.is_none(), "a lit subject is left");
}

#[test]
fn a_person_is_lit_by_their_face_and_a_back_not_at_all() {
    let (w, h) = (100usize, 100usize);
    let body = Alpha::new(w, h, (0..w * h).map(|i| if (30..70).contains(&(i % w)) && i / w >= 20 { 1.0 } else { 0.0 }).collect());
    let face = [0.4, 0.2, 0.2, 0.2];
    let lit = lit_part(&body, &[face], false, None).expect("a face on the subject");
    assert!(lit.alpha.data[25 * w + 50] > 0.5 && lit.alpha.data[80 * w + 50] == 0.0, "the face, not the coat");
    assert_eq!(lit.faces, 1);
    assert!(lit_part(&body, &[], false, None).is_none(), "a back turned");
    assert!(lit_part(&body, &[[0.0, 0.0, 0.1, 0.1]], false, None).is_none(), "a face elsewhere is someone else's");
    assert!(lit_part(&body, &[], true, None).is_some_and(|lit| lit.faces == 0), "an animal whole");
    let speck = Alpha::new(w, h, (0..w * h).map(|i| if i % w < 10 && i / w < 10 { 1.0 } else { 0.0 }).collect());
    assert!(lit_part(&speck, &[[0.0, 0.0, 0.1, 0.1]], true, None).is_none(), "1 % of the frame");

    assert!(lit_part(&body, &[face], false, Some([0.1, 0.5])).is_none(), "focused on something else");
    let other = [0.42, 0.75, 0.16, 0.16];
    let chosen = lit_part(&body, &[face, other], false, Some([0.5, 0.82])).expect("the face focused on");
    assert!(chosen.alpha.data[82 * w + 50] > 0.5 && chosen.alpha.data[30 * w + 50] == 0.0, "the one at the point, not the other");
}

#[test]
fn a_low_key_frame_keeps_its_dark() {
    let night = ends(0.01, 0.05);
    let auto = tone(&night, None, &fresh());
    assert!(auto.basic.tone.exposure <= 0.0, "{}", auto.basic.tone.exposure);
    assert!(auto.basic.tone.blacks <= 0.0, "{}", auto.basic.tone.blacks);

    let (w, h) = (64u32, 64u32);
    let facade = LinearImage::new(w, h, (0..w * h).flat_map(|i| {
        let v = if i / w < 16 { 0.0005 } else { tone::scene_value_for(0.5) };
        [v, v, v]
    }).collect());
    let auto = tone(&facade, None, &fresh());
    assert!(auto.basic.tone.exposure <= 0.0 && auto.basic.tone.blacks <= 0.0, "{:?}", auto.basic.tone);
}

#[test]
fn a_burst_is_held_to_one_light() {
    let frame = |scale: f32| {
        let mut image = ends(tone::scene_value_for(0.35) * scale, tone::scene_value_for(0.8) * scale);
        image.data.iter_mut().enumerate().for_each(|(i, v)| *v *= 1.0 + (i % 7) as f32 * 0.05);
        image
    };
    let (reference, other) = (frame(1.0), frame(0.7));
    let mut first = fresh();
    tone(&reference, None, &first).apply(&mut first);
    let wanted = key(&reference, &first).unwrap();

    let mut second = fresh();
    match_light(&other, &second, &first.basic(), wanted).apply(&mut second);
    let got = key(&other, &second).unwrap();
    let spread = (tone::scene_for(got, false) / tone::scene_for(wanted, false)).log2().abs();
    assert!(spread < 0.05, "{got} against {wanted}: {spread} EV");
    assert!(second.basic().tone.exposure > first.basic().tone.exposure, "the darker frame gets more");

    let mut theirs = fresh();
    theirs.set_basic(Basic::with(|b| b.tone.exposure = 0.4));
    match_light(&other, &theirs, &first.basic(), wanted).apply(&mut theirs);
    assert_eq!(theirs.basic().tone.exposure, 0.4, "their exposure stays");
}

#[test]
fn a_frame_shot_a_stop_down_stays_low_key() {
    let (image, alpha) = subject_in_shadow();
    let with = |intent: Intent| tone_told(&image, &fresh(), &Told { subject: Some(&alpha), intent, ..Default::default() });
    let meant = with(Intent::of(Some(-1.0), None, None));
    assert_eq!(meant.basic.tone.exposure.max(0.0), 0.0, "not brightened");
    assert!(meant.subject.is_none() && meant.held_low == Some(-1.0));
    let habit = with(Intent::of(Some(-0.33), None, None));
    assert!(habit.subject.is_some() && habit.held_low.is_none());
    let up = with(Intent::of(Some(1.0), None, None));
    assert!(up.subject.is_some() && up.held_low.is_none(), "+1 EV wanted it lighter, and is not held");
    let dark = tone_told(&ends(0.01, 0.03), &fresh(), &Told { intent: Intent::of(Some(-1.3), None, None), ..Default::default() });
    assert!(dark.basic.tone.exposure <= 0.0, "the ends do not bring it up either");
}

#[test]
fn what_the_file_says_limits_whites_and_colour() {
    let dim = ends(0.4, tone::scene_value_for(0.88));
    let plain = tone(&dim, None, &fresh()).basic;
    assert!(plain.tone.whites > 0.0, "the top end is pushed out without DR: {}", plain.tone.whites);
    let dr = tone_told(&dim, &fresh(), &Told { intent: Intent::of(None, Some(400), None), ..Default::default() }).basic;
    assert!(dr.tone.whites <= 0.0, "DR400 keeps its headroom: {}", dr.tone.whites);

    let (w, h) = (64u32, 64u32);
    let grey = LinearImage::new(w, h, (0..w * h).flat_map(|_| [0.20, 0.21, 0.22]).collect());
    let colour = tone(&grey, None, &fresh()).basic.presence.vibrance;
    assert!(colour > MOST_VIBRANCE_ON_FACES, "a flat frame gets colour: {colour}");
    let acros = tone_told(&grey, &fresh(), &Told { intent: Intent::of(None, None, Some(0x500)), ..Default::default() });
    assert_eq!(acros.basic.presence.vibrance, 0.0, "Acros");
    let faces = tone_told(&grey, &fresh(), &Told { faces: 0.1, ..Default::default() });
    assert_eq!(faces.basic.presence.vibrance, MOST_VIBRANCE_ON_FACES, "skin");
}

#[test]
fn a_lamp_is_not_the_top_end() {
    let (w, h) = (256u32, 170u32);
    let lamp = |x: u32, y: u32| (120..136).contains(&x) && (40..56).contains(&y);

    let spot = |x: u32, y: u32| (100..103).contains(&y) && x % 8 < 3 && x < 240;
    let room = |lit: &dyn Fn(u32, u32) -> bool| {
        LinearImage::new(w, h, (0..w * h).flat_map(|i| {
            let (x, y) = (i % w, i / w);

            let v = if lit(x, y) { tone::scene_value_for(0.9995) } else { tone::scene_value_for(0.25 + 0.6 * (x as f32 / w as f32)) };
            [v, v, v]
        }).collect())
    };
    let named = Alpha::new(w as usize, h as usize, (0..w * h).map(|i| if lamp(i % w, i / w) { 1.0 } else { 0.0 }).collect());
    let with_lamp = room(&lamp);
    let blind = tone(&with_lamp, None, &fresh()).basic;
    let told = tone_told(&with_lamp, &fresh(), &Told { lights: Some(&named), ..Default::default() }).basic;
    let without = tone(&room(&|_, _| false), None, &fresh()).basic;
    assert!(blind.tone.whites < 0.0 || blind.tone.highlights < 0.0, "unnamed, the lamp is the top end: {:?}", blind.tone);
    let same = |a: &Basic, b: &Basic| (a.tone.exposure - b.tone.exposure).abs() < 0.03 && (a.tone.whites - b.tone.whites).abs() <= 2.0 && a.tone.highlights == b.tone.highlights;
    assert!(same(&told, &without), "named, it is left out: {:?} against {:?}", told.tone, without.tone);

    let with_spots = tone(&room(&spot), None, &fresh()).basic;
    assert!(!same(&with_spots, &without), "by day, points are the top end: {:?}", with_spots.tone);

    let night = |lit: &dyn Fn(u32, u32) -> bool| {
        LinearImage::new(w, h, (0..w * h).flat_map(|i| {
            let (x, y) = (i % w, i / w);
            let v = if lit(x, y) { tone::scene_value_for(0.9999) * 4.0 } else { tone::scene_value_for(0.04 + 0.6 * (x as f32 / w as f32).powi(4)) };
            [v, v, v]
        }).collect())
    };
    let (dark, lit) = (tone(&night(&|_, _| false), None, &fresh()).basic, tone(&night(&spot), None, &fresh()).basic);
    assert!(same(&lit, &dark), "a street of points at night: {:?} against {:?}", lit.tone, dark.tone);
}

#[test]
fn fog_keeps_its_flatness() {
    let (w, h) = (64u32, 64u32);
    let fog = LinearImage::new(w, h, (0..w * h).flat_map(|i| {
        let v = tone::scene_value_for(0.55 + 0.25 * (i as f32 / (w * h) as f32));
        [v, v, v]
    }).collect());
    assert_eq!(tone(&fog, None, &fresh()).basic.tone.blacks, 0.0, "fog");
}

#[test]
fn a_silhouette_against_a_sunset_stays_one() {
    let (w, h) = (64u32, 64u32);
    let inside = |i: u32| (16..48).contains(&(i % w)) && (24..64).contains(&(i / w));
    let frame = |sky: [f32; 3]| LinearImage::new(w, h, (0..w * h).flat_map(|i| if inside(i) { [0.0006; 3] } else { sky }).collect());
    let alpha = Alpha::new(w as usize, h as usize, (0..w * h).map(|i| if inside(i) { 1.0 } else { 0.0 }).collect());
    let sunset = tone(&frame([0.9, 0.45, 0.2]), Some(&alpha), &fresh());
    assert!(sunset.silhouette && sunset.subject.is_none(), "kept: {:?}", sunset.subject);
    let noon = tone(&frame([0.4, 0.5, 0.6]), Some(&alpha), &fresh());
    assert!(!noon.silhouette && noon.subject.is_some_and(|stops| stops > 0.0), "backlit at noon is lit");
    let meant = tone_told(&frame([0.4, 0.5, 0.6]), &fresh(), &Told { subject: Some(&alpha), intent: Intent::of(Some(-1.0), None, None), ..Default::default() });
    assert!(meant.silhouette, "a stop down dialled in says it was meant");
}

#[test]
fn a_subject_is_lit_no_further_than_its_own_white() {
    let (w, h) = (64u32, 64u32);
    let inside = |i: u32| (16..48).contains(&(i % w)) && (16..48).contains(&(i / w));
    let breast = |i: u32| inside(i) && (i % w) < 20;
    let bright = tone::scene_value_for(0.9);
    let data = (0..w * h)
        .flat_map(|i| {
            let v = if breast(i) { tone::scene_value_for(0.5) } else if inside(i) { bright / 32.0 } else { bright };
            [v, v, v]
        })
        .collect();
    let alpha = Alpha::new(w as usize, h as usize, (0..w * h).map(|i| if inside(i) { 1.0 } else { 0.0 }).collect());
    let image = LinearImage::new(w, h, data);
    let stops = tone(&image, Some(&alpha), &fresh()).subject.unwrap_or(0.0);
    let white = (tone::scene_for(WHITE_POINT, false) / tone::scene_value_for(0.5)).log2();
    assert!(stops <= white + 1e-3, "{stops} lifts the white breast past white ({white})");
}

#[test]
fn a_burnt_out_subject_brings_the_exposure_down() {
    let (w, h) = (64u32, 64u32);
    let inside = |index: u32| (16..48).contains(&(index % w)) && (16..48).contains(&(index / w));
    let data = (0..w * h)
        .flat_map(|index| {
            let value = if inside(index) { tone::scene_value_for(0.999) * 1.6 } else { tone::scene_value_for(0.2) };
            [value, value, value]
        })
        .collect();
    let alpha = Alpha::new(w as usize, h as usize, (0..w * h).map(|index| if inside(index) { 1.0 } else { 0.0 }).collect());
    let auto = tone(&LinearImage::new(w, h, data), Some(&alpha), &fresh());
    let stops = auto.subject.expect("a subject past white comes down");
    assert!(stops < -0.33 && stops >= -MOST_SUBJECT_DOWN - 1e-4, "{stops}");
}

#[test]
fn colour_is_added_to_a_flat_frame_and_not_to_a_colourful_one() {
    let (w, h) = (64u32, 64u32);
    let of = |red: f32, green: f32, blue: f32| {
        LinearImage::new(
            w,
            h,
            (0..w * h).flat_map(|_| [red, green, blue]).collect::<Vec<_>>(),
        )
    };

    let flat = tone(&of(0.20, 0.21, 0.22), None, &fresh()).basic;
    assert!(flat.presence.vibrance > 0.0, "a flat frame gets some: {}", flat.presence.vibrance);
    assert!(flat.presence.vibrance <= MOST_VIBRANCE, "and never more than a fifth of the slider");

    let vivid = tone(&of(0.40, 0.10, 0.05), None, &fresh()).basic;
    assert_eq!(vivid.presence.vibrance, 0.0, "a colourful frame is left alone");
    assert_eq!(vivid.presence.saturation, 0.0, "and never by the blunt slider");
}

#[test]
fn auto_refuses_everything_that_is_a_matter_of_taste() {
    let (w, h) = (64u32, 64u32);
    let data = (0..w * h)
        .flat_map(|index| {
            let value = (index % (w * h)) as f32 / (w * h) as f32 * 1.5;
            [value, value, value]
        })
        .collect();
    let basic = tone(&LinearImage::new(w, h, data), None, &fresh()).basic;

    assert_eq!(basic.tone.contrast, 0.0);
    assert_eq!(basic.presence.saturation, 0.0);
    assert_eq!(basic.tone.shadows, 0.0);
    assert_eq!(basic.presence.clarity, 0.0);
    assert_eq!(basic.presence.texture, 0.0);
}

fn subject_in_shadow() -> (LinearImage, Alpha) {
    let (w, h) = (64u32, 64u32);
    let inside = |index: u32| (16..48).contains(&(index % w)) && (16..48).contains(&(index / w));
    let bright = tone::scene_value_for(0.9);
    let data = (0..w * h)
        .flat_map(|index| {
            let value = if inside(index) { bright / 32.0 } else { bright };
            [value, value, value]
        })
        .collect();
    let alpha = (0..w * h).map(|index| if inside(index) { 1.0 } else { 0.0 }).collect();
    (LinearImage::new(w, h, data), Alpha::new(w as usize, h as usize, alpha))
}

fn ends(value: f32, brightest: f32) -> LinearImage {
    let (w, h) = (64u32, 64u32);
    let count = (w * h) as usize;
    let data = (0..count)
        .flat_map(|index| {
            let level = if index >= count - count / 150 { brightest } else { value };
            [level, level, level]
        })
        .collect();
    LinearImage::new(w, h, data)
}

#[test]
fn auto_leaves_what_the_photographer_set() {
    let blown = ends(0.4, tone::scene_value_for(0.9995));
    let mut document = fresh();
    document.set_basic(Basic::with(|b| {
        b.tone.exposure = 0.4;
        b.presence.hdr = 50.0;
    }));

    let applied = tone(&blown, None, &document).apply(&mut document);
    let basic = document.basic();
    assert_eq!(basic.tone.exposure, 0.4, "the photographer's exposure stays");
    assert_eq!(basic.presence.hdr, 50.0, "and so does a merge's HDR");
    assert_eq!(applied.kept, vec!["exposure"], "and the toast can say so");
    assert_eq!(document.auto.and_then(|record| record.exposure), None, "Auto does not claim it");
}

#[test]
fn auto_replaces_what_it_set_itself() {
    let mut document = fresh();
    let dim = ends(tone::scene_value_for(0.3), tone::scene_value_for(0.6));
    tone(&dim, None, &document).apply(&mut document);
    let first = document.basic().tone.exposure;
    assert!(first > 0.05, "a dim frame goes up: {first}");

    let applied = tone(&ends(0.4, tone::scene_value_for(0.9995)), None, &document).apply(&mut document);
    let second = document.basic().tone.exposure;
    assert_eq!(second, 0.0, "measured again, its own answer is replaced: {second}");
    assert!(applied.kept.is_empty(), "nothing of the photographer's was in the way");
}

#[test]
fn pressing_twice_gives_the_same_document() {
    let (image, alpha) = subject_in_shadow();
    let mut document = fresh();
    tone(&image, Some(&alpha), &document).apply(&mut document);
    let once = serde_json::to_string(&document).unwrap();

    let applied = tone(&image, Some(&alpha), &document).apply(&mut document);
    assert_eq!(serde_json::to_string(&document).unwrap(), once, "replaced, not compounded");
    assert!(applied.subject.is_some() && applied.kept.is_empty());
}

#[test]
fn the_photographers_subject_mask_is_kept_and_not_lifted_again() {
    let (image, alpha) = subject_in_shadow();
    let mut document = fresh();
    let mut theirs = Mask::new(Shape::Subject);
    theirs.basic.tone.exposure = 0.3;
    document.set_masks(vec![theirs]);

    let applied = tone(&image, Some(&alpha), &document).apply(&mut document);
    let masks = document.masks();
    assert_eq!(masks.len(), 1, "theirs kept");
    assert_eq!(masks[0].basic.tone.exposure, 0.3, "and untouched");
    assert!(applied.subject.is_none(), "they lit it: the ends set the exposure");
}

#[test]
fn an_auto_mask_once_tuned_is_the_photographers() {
    let (image, alpha) = subject_in_shadow();
    let mut document = fresh();
    let mut tuned = autos_mask();
    assert!(tuned.as_auto_left_it());
    assert_eq!(tuned.name.as_deref(), Some(AUTO_SUBJECT));
    tuned.basic.tone.exposure += 0.25;
    document.set_masks(vec![tuned.clone()]);

    tone(&image, Some(&alpha), &document).apply(&mut document);
    let masks = document.masks();
    assert_eq!(masks.len(), 1);
    assert_eq!(masks[0].basic.tone.exposure, tuned.basic.tone.exposure, "a tuned mask is left alone");
}

fn autos_mask() -> Mask {
    Mask { id: 1, ..Mask::auto_subject(changed(&Basic::local(), |b| b.tone.exposure = 1.2)) }
}

#[test]
fn the_ends_are_solved_through_the_photographers_look() {
    let mut document = fresh();
    document.set_basic(Basic::with(|b| b.tone.contrast = 40.0));
    let look = Look::of(&document, false);
    let rest = document.basic();

    let (mut low, mut high) = (0.01f32, 2.0f32);
    for _ in 0..30 {
        let middle = (low + high) / 2.0;
        if look.display(middle, &rest) < 0.89 {
            low = middle;
        } else {
            high = middle;
        }
    }
    let top = (low + high) / 2.0;

    let basic = tone(&ends(0.05, top), None, &document).basic;
    assert_eq!(basic.tone.contrast, 40.0, "the look stays");
    assert!(basic.tone.whites > 0.0, "Whites carries the top end: {}", basic.tone.whites);
    let gain = basic.tone.exposure.exp2();
    let landed = look.display(top * gain, &basic);
    assert!((landed - WHITE_POINT).abs() < CLOSE_ENOUGH, "the top end lands at {landed} through the look");

    let plain = Look { curve: None, finished: false };
    let blind = solve(|amount| plain.display(top, &Basic::with(|b| b.tone.whites = amount)), WHITE_POINT);
    let missed = look.display(top, &changed(&rest, |b| b.tone.whites = blind));
    assert!((missed - WHITE_POINT).abs() >= CLOSE_ENOUGH, "solved without the look it lands at {missed}");
}

#[test]
fn the_subject_is_measured_on_the_frame_it_was_found_in() {
    let (w, h) = (96u32, 64u32);
    let bright = tone::scene_value_for(0.8);
    let working = LinearImage::new(
        w,
        h,
        (0..w * h)
            .flat_map(|index| {
                let (x, y) = (index % w, index / w);
                let value = if (20..44).contains(&x) && (20..38).contains(&y) { bright / 32.0 } else { bright };
                [value, value, value]
            })
            .collect(),
    );
    let framings: [(&str, fn(&mut Document)); 5] = [
        ("cropped", |d| d.set_crop([0.0, 0.0, 0.5, 0.6], 0.0)),
        ("straightened", |d| d.set_crop([0.05, 0.05, 0.6, 0.8], 4.0)),
        ("turned", |d| d.set_rotation(90.0)),
        ("mirrored", |d| d.set_mirrored(true)),
        ("keystoned", |d| d.set_perspective(Perspective { vertical: 20.0, ..Perspective::default() })),
    ];
    for (name, framing) in framings {
        let mut document = fresh();
        framing(&mut document);

        let frame = crate::apply_stack(&crate::mask_geometry(&document), &working, 1.0);
        let alpha = Alpha::new(
            frame.width() as usize,
            frame.height() as usize,
            frame.pixels().map(|pixel| if pixel[1] < 100 { 1.0 } else { 0.0 }).collect(),
        );
        let lift = tone(&framed(&document, &working), Some(&alpha), &document).subject.unwrap_or(0.0);
        assert!(lift > 1.0, "{name}: the dark subject is the one measured, lit {lift}");
    }
}

#[test]
fn a_lamp_cropped_away_no_longer_sets_the_exposure() {
    let (w, h) = (96u32, 64u32);
    let lamp = tone::scene_value_for(0.9995);
    let middle = tone::scene_value_for(0.85);
    let working = LinearImage::new(
        w,
        h,
        (0..w * h)
            .flat_map(|index| {
                let value = if index % w < 24 { lamp } else { middle };
                [value, value, value]
            })
            .collect(),
    );
    let mut document = fresh();
    document.set_crop([0.5, 0.0, 0.5, 1.0], 0.0);

    let whole = tone(&working, None, &document).basic.tone;
    let shown = tone(&framed(&document, &working), None, &document).basic.tone;
    assert!(whole.whites < 0.0 || whole.highlights < 0.0, "the uncropped frame's top is pulled in by the lamp: {whole:?}");
    assert!(shown.whites >= 0.0 && shown.highlights == 0.0, "the frame as shown has no lamp in it: {shown:?}");
}

#[test]
fn blacks_leave_a_dark_end_where_the_camera_renders_it() {
    let blacks = |display: f32| {
        let darkest = tone::scene_value_for(display);
        let (w, h) = (64u32, 64u32);
        let count = (w * h) as usize;
        let data = (0..count)
            .flat_map(|index| {

                let level = match index {
                    low if low < count / 100 => darkest,
                    high if high >= count - count / 100 => tone::scene_value_for(0.93),
                    _ => tone::scene_value_for(0.25 + 0.6 * index as f32 / count as f32),
                };
                [level, level, level]
            })
            .collect();
        tone(&LinearImage::new(w, h, data), None, &fresh()).basic.tone.blacks
    };
    for display in [0.05, 0.07, 0.085] {
        assert_eq!(blacks(display), 0.0, "a dark end at {display} is where this camera puts it");
    }
    assert!(blacks(0.02) > 0.0, "a crushed one is lifted: {}", blacks(0.02));
    assert!(blacks(0.15) < 0.0, "a lifted one comes down: {}", blacks(0.15));
}

#[test]
fn auto_does_not_undo_the_photographers_highlight_recovery() {
    let mut document = fresh();
    document.set_basic(Basic::with(|b| b.tone.highlights = -100.0));
    let applied = tone(&ends(0.4, tone::scene_value_for(0.93)), None, &document).apply(&mut document);
    let basic = document.basic();
    assert_eq!(basic.tone.highlights, -100.0);
    assert_eq!(basic.tone.exposure, 0.0, "the frame is not brightened to put the sky back");
    assert_eq!(basic.tone.whites, 0.0, "nor Whites pushed to undo it");
    assert_eq!(applied.kept, vec!["highlights"]);
}

#[test]
fn the_photographers_contrast_does_not_move_the_exposure() {
    let mut document = fresh();
    document.set_basic(Basic::with(|b| b.tone.contrast = 45.0));
    let basic = tone(&ends(0.4, tone::scene_value_for(0.93)), None, &document).basic;
    assert_eq!(basic.tone.exposure, 0.0, "the exposure answers to the capture, not the look");
    assert_eq!(basic.tone.contrast, 45.0);
}

#[test]
fn a_curve_that_fades_the_blacks_keeps_its_fade() {
    let darkest = tone::scene_value_for(0.15);
    let (w, h) = (64u32, 64u32);
    let count = (w * h) as usize;
    let data = (0..count)
        .flat_map(|index| {
            let level = match index {
                low if low < count / 100 => darkest,
                high if high >= count - count / 100 => tone::scene_value_for(0.93),
                _ => tone::scene_value_for(0.25 + 0.6 * index as f32 / count as f32),
            };
            [level, level, level]
        })
        .collect();
    let frame = LinearImage::new(w, h, data);
    assert!(tone(&frame, None, &fresh()).basic.tone.blacks < 0.0, "without the curve the end comes down");

    let mut document = fresh();
    document.set_curve(Curve::new([[0.0, 0.12], [0.5, 0.55], [1.0, 1.0]]));
    assert_eq!(tone(&frame, None, &document).basic.tone.blacks, 0.0, "a faded black is the photographer's");
}

#[test]
fn autos_old_mask_goes_with_the_next_press() {
    let (image, alpha) = subject_in_shadow();
    let mut document = fresh();
    let mut radial = Mask::new(Shape::radial());
    radial.id = 2;
    document.set_masks(vec![autos_mask(), radial]);

    tone(&image, Some(&alpha), &document).apply(&mut document);
    let after = document.masks();
    assert_eq!(after.len(), 1, "Auto's own mask gone");
    assert_eq!(after[0].id, 2, "the photographer's radial kept");
}

#[test]
fn a_mask_that_subtracts_autos_makes_it_the_photographers() {
    let (image, alpha) = subject_in_shadow();
    let mut document = fresh();
    let mut sky = Mask::new(Shape::radial());
    sky.id = 2;
    sky.minus_masks = vec![1];
    document.set_masks(vec![autos_mask(), sky]);
    let before = serde_json::to_string(&document.masks()).unwrap();

    tone(&image, Some(&alpha), &document).apply(&mut document);
    assert_eq!(serde_json::to_string(&document.masks()).unwrap(), before, "both left as they were");
}

#[test]
fn renaming_autos_mask_or_softening_it_makes_it_the_photographers() {
    let (image, alpha) = subject_in_shadow();
    let changes: [fn(&mut Mask); 3] =
        [|mask| mask.name = Some("Heron".to_string()), |mask| mask.feather = 30.0, |mask| mask.shift = -20.0];
    for change in changes {
        let mut document = fresh();
        let mut mask = autos_mask();
        change(&mut mask);
        assert!(!mask.as_auto_left_it());
        document.set_masks(vec![mask]);

        tone(&image, Some(&alpha), &document).apply(&mut document);
        assert_eq!(document.masks().len(), 1, "kept");
    }
}

#[test]
fn a_tuned_lift_keeps_the_hdr_under_it_and_autos_own_hdr_goes() {
    let (image, alpha) = subject_in_shadow();
    let mut document = fresh();
    let mut tuned = autos_mask();
    tuned.basic.tone.exposure += 0.25;
    document.set_masks(vec![tuned]);
    document.set_basic(Basic::with(|b| b.presence.hdr = 30.0));
    document.auto = Some(AutoRecord { hdr: Some(30.0), ..AutoRecord::default() });
    tone(&image, Some(&alpha), &document).apply(&mut document);
    assert_eq!(document.basic().presence.hdr, 30.0, "the hand under their lift stays");

    document.set_masks(Vec::new());
    tone(&image, Some(&alpha), &document).apply(&mut document);
    assert_eq!(document.basic().presence.hdr, 0.0, "Auto's own HDR goes back to rest");
}

#[test]
fn a_slider_reset_while_auto_measures_stays_reset() {
    let mut document = fresh();
    document.set_basic(Basic::with(|b| b.tone.exposure = 0.4));
    let measured = tone(&ends(0.4, tone::scene_value_for(0.9995)), None, &document);

    document.set_basic(Basic::default());
    let applied = measured.apply(&mut document);
    assert_eq!(document.basic().tone.exposure, 0.0, "not the old value back");
    assert_eq!(document.auto.and_then(|record| record.exposure), None, "and not claimed");
    assert!(applied.kept.is_empty());
}

#[test]
fn the_lift_the_old_auto_left_is_removed() {
    let (image, alpha) = subject_in_shadow();
    let mut document = fresh();
    let mut old = Mask::new(Shape::Subject);
    old.set_matte(true);
    old.basic = Basic::with(|b| b.tone.exposure = 1.7);
    assert!(old.from_old_auto());
    document.set_masks(vec![old]);

    tone(&image, Some(&alpha), &document).apply(&mut document);
    assert!(document.masks().is_empty(), "the old Auto's lift goes too");

    let mut chip = Mask::new(Shape::Subject);
    chip.set_matte(true);
    chip.basic.tone.exposure = 0.5;
    assert!(!chip.from_old_auto(), "a mask from the Subject chip is the photographer's");
}

#[test]
fn a_press_that_changes_nothing_leaves_no_record() {
    let mut frame = ends(0.4, tone::scene_value_for(0.93));
    frame.data.chunks_exact_mut(3).for_each(|pixel| {
        pixel[0] *= 1.3;
        pixel[2] *= 0.4;
    });
    let mut document = fresh();
    tone(&frame, None, &document).apply(&mut document);
    assert_eq!(document.basic(), Basic::default(), "a frame that needs nothing gets nothing");
    assert_eq!(document.auto, None, "and no record of it");
}

#[test]
fn autos_own_end_stays_when_the_photographer_shapes_it() {
    let mut document = fresh();
    document.set_basic(Basic::with(|b| {
        b.tone.whites = -25.0;
        b.tone.highlights = -50.0;
    }));
    document.auto = Some(AutoRecord { whites: Some(-25.0), ..AutoRecord::default() });

    tone(&ends(0.4, tone::scene_value_for(0.9995)), None, &document).apply(&mut document);
    let basic = document.basic();
    assert_eq!(basic.tone.highlights, -50.0, "their recovery");
    assert_eq!(basic.tone.whites, -25.0, "and what Auto had under it, both left");
}

use super::evidence::{self, Evidence, Frame, Reading, Refusal, Source, Verdict, Vote};
use super::sea;

fn seascape(left: f32, right: f32, bend: f32, land: bool) -> (Vec<Plane>, Plane) {
    let (cells, (gw, gh)) = (64usize, (900usize, 600usize));
    let horizon = |u: f32| left + (right - left) * u + bend * 4.0 * u * (1.0 - u);
    let grid = |f: &dyn Fn(f32, f32) -> f32| {
        Plane::new(cells, cells, (0..cells * cells).map(|i| f(((i % cells) as f32 + 0.5) / cells as f32, ((i / cells) as f32 + 0.5) / cells as f32)).collect())
    };

    let below = |u: f32, v: f32| ((v - horizon(u)) * cells as f32 + 0.5).clamp(0.0, 1.0);
    let sky = grid(&|u, v| 1.0 - below(u, v));
    let sea = grid(&|u, v| if land { 0.0 } else { below(u, v) });
    let other = grid(&|_, _| 0.0);
    let in_the_way = grid(&|u, v| if land { below(u, v) } else { 0.0 });
    let guide = Plane::new(
        gw,
        gh,
        (0..gw * gh)
            .map(|i| {
                let (x, y) = (i % gw, i / gw);
                let v = y as f32 / gh as f32;

                let wave = if (x / 9 + y / 4) % 7 == 0 { 0.06 } else { 0.0 };
                if v < horizon(x as f32 / gw as f32) { 0.75 } else { 0.35 - wave }
            })
            .collect(),
    );
    (vec![sky, sea, other, in_the_way], guide)
}

fn read_sea(planes: &[Plane], guide: &Plane) -> Option<sea::Horizon> {
    sea::horizon(&sea::Planes { sky: &planes[0], sea: &planes[1], other_water: &planes[2], in_the_way: &planes[3], guide })
}

fn sea_reading(horizon: sea::Horizon) -> Evidence {
    Evidence { source: Source::Sea, reading: Reading::Line(horizon.line), sigma: horizon.sigma }
}

#[test]
fn the_sea_is_found_where_sky_meets_it() {
    for (left, right) in [(0.40, 0.47), (0.55, 0.50), (0.45, 0.45)] {
        let (planes, guide) = seascape(left, right, 0.0, false);
        let found = read_sea(&planes, &guide).expect("a straight sea horizon");
        let wanted = ((right - left) * 600.0 / 900.0).atan().to_degrees();
        let angle = sea_reading(found).angle(Perspective::default(), 900.0, 600.0);
        assert!((angle - wanted).abs() < 0.05, "{left}→{right}: {angle}° against {wanted}°");
        assert!(found.sigma < 0.3 && found.sea, "{found:?}");
    }
}

#[test]
fn a_hill_or_a_curved_shore_is_not_the_sea() {
    let (planes, guide) = seascape(0.40, 0.47, 0.0, true);
    assert!(read_sea(&planes, &guide).is_none(), "sky on a hill");
    let (planes, guide) = seascape(0.45, 0.45, 0.06, false);
    assert!(read_sea(&planes, &guide).is_none(), "a bay's curve");
}

#[test]
fn the_horizon_is_levelled_through_the_keystone() {
    let (w, h) = (480usize, 360usize);

    let (left, right) = (0.30f32, 0.36f32);

    let scene = |x: usize, y: usize| -> f32 {
        let edge = (left + (right - left) * (x as f32 + 0.5) / w as f32) * h as f32;
        let sea = (y as f32 + 0.5 - edge + 0.5).clamp(0.0, 1.0);
        0.8 * (1.0 - sea) + 0.15 * sea
    };
    let flat = LinearImage::new(w as u32, h as u32, (0..w * h).flat_map(|i| [scene(i % w, i / w); 3]).collect());
    let reading = Evidence { source: Source::Sea, reading: Reading::Line([0.0, left, 1.0, right]), sigma: 0.05 };
    let luma = |image: &LinearImage| {
        Plane::new(image.width as usize, image.height as usize, image.data.chunks_exact(3).map(|p| p[1]).collect())
    };
    for vertical in [0.0f32, 40.0, -40.0] {
        let perspective = Perspective { vertical, ..Perspective::default() };
        let angle = reading.angle(perspective, w as f32, h as f32);
        let levelled = flat.cropped([0.25, 0.1, 0.5, 0.4], angle, perspective);
        let left_over = level(&luma(&levelled)).unwrap_or(99.0);
        assert!(left_over.abs() < 0.25, "keystone {vertical}: levelled by {angle}°, still {left_over}°");
    }
}

fn converging(point: (f32, f32), roll_free: bool, (w, h): (usize, usize)) -> Vec<Vote> {
    let mut votes = Vec::new();
    for column in 0..12 {
        let base_x = w as f32 * (0.08 + 0.07 * column as f32);

        let base_y = if point.1 > 0.0 { 0.0 } else { h as f32 - 1.0 };
        let (vx, vy) = (w as f32 / 2.0 + point.0, h as f32 / 2.0 + point.1);
        let (dx, dy) = (vx - base_x, vy - base_y);
        for step in 0..60 {
            let t = step as f32 / 60.0 * 0.8;
            let (x, y) = (base_x + dx * t * (h as f32 / dy.abs()), base_y + dy.signum() * t * h as f32);
            if !(0.0..w as f32).contains(&x) || !(0.0..h as f32).contains(&y) {
                continue;
            }

            let length = dx.hypot(dy);
            let (gx, gy) = (dy / length, -dx / length);
            votes.push(Vote { x, y, gx, gy, weight: 1.0, class: 1, building: if roll_free { 1.0 } else { 0.9 } });
        }
    }
    votes
}

#[test]
fn the_buildings_vanishing_point_gives_roll_and_keystone() {
    let size = (600usize, 400usize);
    let half = size.1 as f32 / 2.0;
    for (vx, vy) in [(0.0f32, -900.0f32), (40.0, -900.0), (-30.0, 1200.0), (25.0, -1.0e7)] {
        let lean = evidence::vanishing(&converging((vx, vy), true, size), size.0, size.1, 0.5).expect("a fit");
        let wanted = vx / vy;
        assert!((lean.lean - wanted).abs() < 0.004, "({vx}, {vy}): lean {} against {wanted}", lean.lean);
        assert!((lean.reach - half / vy).abs() < 0.01, "({vx}, {vy}): reach {} against {}", lean.reach, half / vy);
    }

    assert!(evidence::vanishing(&converging((0.0, -900.0), true, size), size.0, size.1, 0.02).is_none());
}

#[test]
fn the_vanishing_points_keystone_is_the_drawings() {
    let (w, h) = (256usize, 256usize);
    let mut data = vec![0.2f32; w * h];
    for line in [-0.3, -0.1, 0.1, 0.3] {
        let base = w as f32 / 2.0 + line * w as f32;
        for y in 0..h {
            let dy = y as f32 + 0.5 - h as f32 / 2.0;
            let x = base + (base - w as f32 / 2.0) * 0.35 * dy / (h as f32 / 2.0);
            for at in (x as isize - 3)..=(x as isize + 3) {
                if (0..w as isize).contains(&at) {
                    let cover = (2.0 - (at as f32 + 0.5 - x).abs()).clamp(0.0, 1.0);
                    data[y * w + at as usize] += 0.7 * cover;
                }
            }
        }
    }
    let plane = Plane::new(w, h, data);
    let votes = evidence::votes(&plane, |_, _| (1, 1.0));
    let new = evidence::vanishing(&votes, w, h, 1.0).expect("a fit").vertical();

    assert!((new - 70.0).abs() < 2.5, "{new}");
}

#[test]
fn verticals_need_three_structures() {
    let (w, h) = (600usize, 400usize);
    let tilt = 1.5f32.to_radians();
    let trunk = |x0: f32| {
        (0..300).map(move |step| {
            let y = 50.0 + step as f32;
            Vote { x: x0 + (y - 200.0) * tilt.tan(), y, gx: tilt.cos(), gy: -tilt.sin(), weight: 1.0, class: 4, building: 0.0 }
        })
    };
    let three: Vec<Vote> = [130.0, 300.0, 460.0].into_iter().flat_map(trunk).collect();
    let lean = evidence::verticals(&three, w, h).expect("three trunks");
    let angle = Evidence { source: Source::Verticals, reading: Reading::Lean(lean.lean), sigma: lean.sigma }.angle(Perspective::default(), w as f32, h as f32);
    assert!((angle.abs() - 1.5).abs() < 0.1, "{angle}");
    let two: Vec<Vote> = [130.0, 460.0].into_iter().flat_map(trunk).collect();
    assert!(evidence::verticals(&two, w, h).is_none());
    let hill: Vec<Vote> = three.iter().map(|vote| Vote { class: 68, ..*vote }).collect();
    assert!(evidence::verticals(&hill, w, h).is_none());
}

#[test]
fn verticals_on_one_side_under_a_keystone_are_not_a_roll() {
    let size = (600usize, 400usize);
    let posts: Vec<Vote> = converging((0.0, 1500.0), true, size)
        .into_iter()
        .filter(|vote| vote.x > 150.0)
        .map(|vote| Vote { class: 4, building: 0.0, ..vote })
        .collect();
    let lean = evidence::verticals(&posts, size.0, size.1).expect("posts enough");
    let angle = -(lean.lean.atan().to_degrees());
    assert!(angle.abs() < 0.2, "a keystone read as a roll of {angle}°");
}

fn frame() -> Frame {
    Frame { width: 900.0, height: 600.0, perspective: Perspective::default(), current: 0.0, pitch: None }
}

fn lean_at(angle: f32, source: Source, sigma: f32) -> Evidence {
    Evidence { source, reading: Reading::Lean(-angle.to_radians().tan()), sigma }
}

#[test]
fn a_lake_needs_a_second_reading() {
    let lake = Evidence { source: Source::Water, reading: Reading::Line([0.0, 0.5, 1.0, 0.5 + 900.0 / 600.0 * 1.2f32.to_radians().tan()]), sigma: 0.05 };
    assert_eq!(evidence::decide(&[lake], &frame()), Verdict::Refused(Refusal::NothingToGoBy));
    match evidence::decide(&[lake, lean_at(1.1, Source::Verticals, 0.3)], &frame()) {
        Verdict::Level { angle, .. } => assert!((angle - 1.2).abs() < 0.05, "{angle}"),
        other => panic!("{other:?}"),
    }

    assert!(matches!(evidence::decide(&[lean_at(1.1, Source::Verticals, 0.3)], &frame()), Verdict::Offer { by: Source::Verticals, .. }));
}

#[test]
fn disagreeing_readings_are_refused_unless_the_sea_is_where_the_pitch_says() {
    let sea = |row: f32| Evidence { source: Source::Sea, reading: Reading::Line([0.0, row, 1.0, row + 900.0 / 600.0 * 1.0f32.to_radians().tan()]), sigma: 0.05 };
    let buildings = lean_at(-2.0, Source::Buildings, 0.2);
    assert_eq!(evidence::decide(&[sea(0.5), buildings], &frame()), Verdict::Refused(Refusal::Disagree));

    let lean = evidence::Lean { lean: 0.0, reach: -300.0 / 1500.0, sigma: 0.2 };
    let pitched = Frame { pitch: Some((lean, 900.0)), ..frame() };
    let row = 0.5 + 540.0 / 600.0 - 0.0087;
    match evidence::decide(&[sea(row), buildings], &pitched) {
        Verdict::Level { by: Source::Sea, angle } => assert!((angle - 1.0).abs() < 0.05),
        other => panic!("{other:?}"),
    }
    assert_eq!(evidence::decide(&[sea(0.5), buildings], &pitched), Verdict::Refused(Refusal::Disagree));
}

#[test]
fn a_dutch_angle_is_left_and_a_tilted_sea_is_offered() {
    assert_eq!(
        evidence::decide(&[lean_at(12.0, Source::Buildings, 0.2)], &frame()),
        Verdict::Refused(Refusal::Deliberate(12.0f32.to_radians().tan().atan().to_degrees()))
    );
    let tilted = Evidence { source: Source::Sea, reading: Reading::Line([0.0, 0.3, 1.0, 0.3 + 900.0 / 600.0 * 10.0f32.to_radians().tan()]), sigma: 0.05 };
    assert!(matches!(evidence::decide(&[tilted], &frame()), Verdict::Offer { by: Source::Sea, .. }));
    let level_already = Frame { current: 1.1, ..frame() };
    assert_eq!(evidence::decide(&[lean_at(1.2, Source::Buildings, 0.2)], &level_already), Verdict::Refused(Refusal::AlreadyLevel));
    assert_eq!(evidence::decide(&[lean_at(1.2, Source::Verticals, 0.7)], &frame()), Verdict::Refused(Refusal::NothingToGoBy));
}

fn empty_scene() -> super::scene::Scene {
    super::scene::Scene { size: [900, 600], classes: (90, 60, vec![4; 90 * 60]), ..Default::default() }
}

const WHOLE: [f32; 4] = [0.0, 0.0, 1.0, 1.0];

#[test]
fn a_level_costs_its_corners_and_no_more_than_five_per_cent() {
    use super::corners::{level, Cut};
    let scene = empty_scene();
    let kept = level(&scene, WHOLE, 0.0, 0.7, Perspective::default()).expect("0.7° is cheap");
    assert!(kept.share > 0.95 && kept.share < 0.99, "{kept:?}");
    assert!(matches!(level(&scene, WHOLE, 0.0, 2.0, Perspective::default()), Err((Cut::TooMuch, _))));
}

#[test]
fn a_level_keeps_a_face_whole() {
    use numa_core::image::into_crop;
    use super::corners::{level, Cut};
    let face = [0.955, 0.40, 0.04, 0.08];
    let scene = super::scene::Scene { faces: vec![face], ..empty_scene() };
    match level(&scene, WHOLE, 0.0, 0.6, Perspective::default()) {
        Ok(kept) => {
            for corner in [[face[0], face[1]], [face[0] + face[2], face[1] + face[3]], [face[0] + face[2], face[1]], [face[0], face[1] + face[3]]] {
                let [u, v] = into_crop(900.0, 600.0, kept.rect, 0.6, Perspective::default(), corner);
                assert!((0.0..=1.0).contains(&u) && (0.0..=1.0).contains(&v), "{kept:?} cuts the face at {u}, {v}");
            }
            assert!(kept.share >= 0.95);
        }
        Err((cut, _)) => assert_eq!(cut, Cut::Face),
    }

    let flush = super::scene::Scene { faces: vec![[0.955, 0.40, 0.045, 0.08]], ..empty_scene() };
    assert!(matches!(level(&flush, WHOLE, 0.0, 0.6, Perspective::default()), Err((Cut::Face, _))));
}

#[test]
fn a_level_cuts_no_person_deeper_and_keeps_the_horizon_off_the_edge() {
    use super::corners::{level, Cut};
    let at_rows = |rows: &[usize], class: u8| {
        let mut classes = vec![4u8; 90 * 60];
        for &row in rows {
            for x in 30..60 {
                classes[row * 90 + x] = class;
            }
        }
        super::scene::Scene { classes: (90, 60, classes), ..empty_scene() }
    };

    assert!(level(&at_rows(&[58, 59], 12), WHOLE, 0.0, 0.6, Perspective::default()).is_ok());

    assert!(level(&at_rows(&[58], 12), WHOLE, 0.0, 0.9, Perspective::default()).is_ok());

    assert!(matches!(level(&at_rows(&[58], 126), WHOLE, 0.0, 0.9, Perspective::default()), Err((Cut::Subject, _))));

    let sea = |row: f32| super::scene::Scene {
        level: vec![Evidence { source: Source::Sea, reading: Reading::Line([0.0, row, 1.0, row + 0.01]), sigma: 0.05 }],
        ..empty_scene()
    };

    assert!(matches!(level(&sea(0.025), WHOLE, 0.0, 0.6, Perspective::default()), Err((Cut::Horizon, _))));
    assert!(level(&sea(0.045), WHOLE, 0.0, 0.6, Perspective::default()).is_ok());
    assert!(level(&sea(0.5), WHOLE, 0.0, 0.6, Perspective::default()).is_ok());
}

#[test]
#[ignore]
fn the_level_on_real_frames() {
    use numa_core::image::crop_inside;
    let Ok(list) = std::env::var("FRAMES_LIST") else {
        println!("set FRAMES_LIST");
        return;
    };
    let turns: Vec<f32> = std::env::var("ROTATE")
        .map(|list| list.split(',').filter_map(|t| t.trim().parse().ok()).collect())
        .unwrap_or_default();
    let csv = std::env::var("OUT_CSV").ok();
    let mut rows = vec!["file,turn,readings,upright,verdict,angle,error,corners".to_string()];
    let verdict = |scene: &super::scene::Scene| evidence::decide(&scene.level, &scene.frame(Perspective::default(), 0.0));
    let angle_of = |v: &Verdict| match v {
        Verdict::Level { angle, .. } | Verdict::Offer { angle, .. } => Some(*angle),
        Verdict::Refused(_) => None,
    };
    for path in std::fs::read_to_string(&list).unwrap().lines().map(str::trim).filter(|l| !l.is_empty()) {
        let path = std::path::PathBuf::from(path);

        let Ok(Ok(full)) = std::panic::catch_unwind(|| numa_io::raw::decode_linear_any(&path)) else { continue };
        let linear = full.downscaled(2400).unwrap_or(full);
        let document = Document::new(path.display().to_string());
        let working = crate::to_working_space(&document, &linear, &Default::default());
        let camera = super::scene::Camera { focal35: numa_io::raw::shot(&path).map(|(_, focal)| focal), ..Default::default() };
        let started = std::time::Instant::now();
        let scene = super::scene::read(&document, &working, None, camera.clone());
        let took = started.elapsed().as_millis();
        let first = verdict(&scene);
        let stem = path.file_name().unwrap().to_string_lossy().to_string();
        let readings = |scene: &super::scene::Scene| {
            scene.level.iter().map(|e| format!("{:?} {:+.2}±{:.2}", e.source, e.angle(Perspective::default(), scene.size[0] as f32, scene.size[1] as f32), e.sigma)).collect::<Vec<_>>().join(" ")
        };
        let corners = match first {
            Verdict::Level { angle, .. } => format!("{:?}", super::corners::level(&scene, [0.0, 0.0, 1.0, 1.0], 0.0, angle, Perspective::default()).map(|k| k.share)),
            _ => String::new(),
        };
        let upright = scene.upright.map(|lean| format!("{:+.1}", lean.vertical())).unwrap_or_default();
        println!("{stem}  {took} ms  [{}]  upright {upright}  -> {first:?} {corners}", readings(&scene));
        rows.push(format!("{stem},0,{},{upright},{first:?},{},,{corners}", readings(&scene), angle_of(&first).map_or(String::new(), |a| format!("{a:.3}"))));

        for &turn in &turns {
            let (w, h) = (working.width as f32, working.height as f32);
            let inside = crop_inside([0.0, 0.0, 1.0, 1.0], turn, Perspective::default(), w, h);
            let turned = working.cropped(inside, turn, Perspective::default());
            let scene = super::scene::read(&document, &turned, None, camera.clone());
            let now = verdict(&scene);
            let error = match (angle_of(&first), angle_of(&now)) {
                (Some(was), Some(now)) => format!("{:.3}", now - (was - turn)),
                (None, Some(now)) => format!("{:.3}", now + turn),
                _ => String::new(),
            };
            println!("    turned {turn:+}: [{}] -> {now:?} error {error}", readings(&scene));
            rows.push(format!("{stem},{turn},{},,{now:?},{},{error},", readings(&scene), angle_of(&now).map_or(String::new(), |a| format!("{a:.3}"))));
        }
    }
    if let Some(csv) = csv {
        std::fs::write(&csv, rows.join("\n") + "\n").unwrap();
    }
}

#[test]
fn autos_level_is_its_own_and_the_photographers_is_left() {
    use super::plan::{self, Leave, Level};
    let sea = Evidence { source: Source::Sea, reading: Reading::Line([0.0, 0.5, 1.0, 0.5 + 900.0 / 600.0 * 0.6f32.to_radians().tan()]), sigma: 0.05 };
    let scene = super::scene::Scene { level: vec![sea], ..empty_scene() };
    let mut document = fresh();
    let Level::Apply { rect, angle, by: Source::Sea, keeps } = plan::level(&scene, &document) else { panic!("{:?}", plan::level(&scene, &document)) };
    assert!((angle - 0.6).abs() < 0.01 && keeps > 0.95);
    assert!(plan::level(&scene, &document).says().starts_with("0.6° by the sea · keeps 9"));
    plan::apply(&mut document, rect, angle);
    assert_eq!(plan::level(&scene, &document), Level::Leave(Leave::Refused(Refusal::AlreadyLevel)));

    let posts = Evidence { source: Source::Verticals, reading: Reading::Lean(-(2.0f32.to_radians().tan())), sigma: 0.3 };
    let level_camera = super::scene::Scene { level: vec![posts], camera: super::scene::Camera { roll: Some(90.0), ..Default::default() }, ..empty_scene() };
    assert_eq!(plan::level(&level_camera, &fresh()), Level::Leave(Leave::Refused(Refusal::Disagree)));
    let tilted_camera = super::scene::Scene { camera: super::scene::Camera { roll: Some(2.1), ..Default::default() }, ..level_camera };
    assert!(matches!(plan::level(&tilted_camera, &fresh()), Level::Offer { .. }));

    let mut theirs = fresh();
    theirs.set_crop([0.0, 0.0, 1.0, 1.0], 1.0);
    assert_eq!(plan::level(&scene, &theirs), Level::Leave(Leave::Theirs(1.0)));
    assert_eq!(plan::level(&scene, &theirs).says(), "Straighten is yours (+1.0°) — left");
}

#[test]
#[ignore]
fn auto_blind_renders() {
    let (Ok(list), Ok(out)) = (std::env::var("FRAMES_LIST"), std::env::var("OUT")) else {
        println!("set FRAMES_LIST and OUT");
        return;
    };
    let shrink = |image: &image::RgbImage| {
        let scale = 1400.0 / image.width().max(image.height()) as f32;
        let (w, h) = ((image.width() as f32 * scale) as u32, (image.height() as f32 * scale) as u32);
        image::imageops::resize(image, w, h, image::imageops::FilterType::Lanczos3)
    };
    let mut rows = vec!["file,level,angle,exposure,highlights,whites,blacks,hdr,vibrance,subject,compensation,held".to_string()];
    for path in std::fs::read_to_string(&list).unwrap().lines().map(str::trim).filter(|l| !l.is_empty()) {
        let path = std::path::PathBuf::from(path);
        let stem = path.file_stem().unwrap().to_string_lossy().to_string();
        let Ok(Ok(linear)) = std::panic::catch_unwind(|| numa_io::raw::decode_linear(&path)) else { continue };
        let mut document = Document::new(path.display().to_string());
        let working = crate::to_working_space(&document, &linear, &Default::default());
        shrink(&crate::apply_stack(&document, &working, 1.0)).save(format!("{out}/{stem}-shot.jpg")).unwrap();

        let proxy = linear.downscaled(2400).unwrap_or_else(|| linear.clone());
        let proxy = crate::to_working_space(&document, &proxy, &Default::default());
        let camera = super::scene::Camera { focal35: numa_io::raw::shot(&path).map(|(_, focal)| focal), ..Default::default() };
        let scene = super::scene::read(&document, &proxy, None, camera);
        let level = super::plan::level(&scene, &document);
        if let super::plan::Level::Apply { rect, angle, .. } = level {
            super::plan::apply(&mut document, rect, angle);
        }
        let compensation = numa_io::raw::summary(&path).and_then(|summary| summary.exposure_bias);
        let (after, auto, lit) = run_meant(&mut document, &working, compensation);
        shrink(&after).save(format!("{out}/{stem}-new.jpg")).unwrap();
        let b = auto.basic;
        let angle = document.crop().map_or(0.0, |(_, angle)| angle);
        println!("{stem}: {}  angle {angle:+.2}", level.says());
        rows.push(format!(
            "{stem},\"{}\",{angle:.2},{:.2},{:.0},{:.0},{:.0},{:.0},{:.0},{},{},{}",

            level.says(), b.tone.exposure, b.tone.highlights, b.tone.whites, b.tone.blacks, b.presence.hdr, b.presence.vibrance, lit.is_some(),
            compensation.unwrap_or(0.0), auto.held_low.is_some()
        ));
    }
    std::fs::write(format!("{out}/auto.csv"), rows.join("\n") + "\n").unwrap();
}

#[test]
#[ignore]
fn the_cameras_roll_against_the_read() {
    let (Ok(list), Ok(csv)) = (std::env::var("FRAMES_LIST"), std::env::var("OUT_CSV")) else {
        println!("set FRAMES_LIST and OUT_CSV");
        return;
    };
    let mut rows = vec!["file,roll,source,angle,sigma".to_string()];
    for path in std::fs::read_to_string(&list).unwrap().lines().map(str::trim).filter(|l| !l.is_empty()) {
        let path = std::path::PathBuf::from(path);
        let Some(roll) = numa_io::raw::roll_angle(&path) else { continue };
        let Ok(Ok(full)) = std::panic::catch_unwind(|| numa_io::raw::decode_linear_any(&path)) else { continue };
        let linear = full.downscaled(2400).unwrap_or(full);
        let document = Document::new(path.display().to_string());
        let working = crate::to_working_space(&document, &linear, &Default::default());
        let scene = super::scene::read(&document, &working, None, Default::default());
        let (w, h) = (scene.size[0] as f32, scene.size[1] as f32);
        let stem = path.file_name().unwrap().to_string_lossy().to_string();
        let sure = scene.level.iter().filter(|e| e.sigma <= 0.3 && matches!(e.source, Source::Sea | Source::Buildings)).min_by(|a, b| a.sigma.total_cmp(&b.sigma));
        let row = match sure {
            Some(e) => format!("{stem},{roll},{:?},{:.3},{:.3}", e.source, e.angle(Perspective::default(), w, h), e.sigma),
            None => format!("{stem},{roll},,,"),
        };
        println!("{row}");
        rows.push(row);
    }
    std::fs::write(csv, rows.join("\n") + "\n").unwrap();
}

#[test]
fn crops_are_offered_round_the_subject_and_never_through_it() {
    use super::crop::proposals;
    use numa_core::image::into_crop;
    let mut classes = vec![4u8; 90 * 60];
    for y in 12..50 {
        for x in 12..22 {
            classes[y * 90 + x] = 126;
        }
    }
    let heron = super::scene::Scene { classes: (90, 60, classes), ..empty_scene() };
    let (found, _) = proposals(&heron, WHOLE, 0.0, Perspective::default(), (7728, 5152));
    assert!(!found.is_empty(), "{found:?}");
    assert!(found.iter().any(|p| p.aspect == "Portrait 4:5"), "the birder's card: {found:?}");
    for proposal in &found {
        assert!(proposal.why.contains(" MP") && proposal.megapixels >= 8.0, "{proposal:?}");
        for (x, y) in [(12usize, 12usize), (21, 49)] {
            let [u, v] = into_crop(900.0, 600.0, proposal.rect, 0.0, Perspective::default(), [(x as f32 + 0.5) / 90.0, (y as f32 + 0.5) / 60.0]);
            assert!((0.0..=1.0).contains(&u) && (0.0..=1.0).contains(&v), "{proposal:?} cuts the heron");
        }
    }
    let (_, beats) = proposals(&empty_scene(), WHOLE, 0.0, Perspective::default(), (6000, 4000));
    assert!(!beats, "nothing in the frame asks for a crop");

    let sea = super::scene::Scene {
        level: vec![Evidence { source: Source::Sea, reading: Reading::Line([0.0, 0.5, 1.0, 0.5]), sigma: 0.05 }],
        classes: (90, 60, (0..90 * 60).map(|i| if i / 90 < 30 { 2 } else { 26 }).collect()),
        ..empty_scene()
    };
    let (found, beats) = proposals(&sea, WHOLE, 0.0, Perspective::default(), (6000, 4000));
    assert!(beats && found[0].why.contains("horizon on a third"), "{found:?}");
}

#[test]
fn crops_keep_what_the_detector_found_whole() {
    use super::crop::proposals;
    use numa_core::image::into_crop;
    let inside = |rect: [f32; 4], [x, y, w, h]: [f32; 4]| {
        [[x, y], [x + w, y + h]].iter().all(|p| {
            let [u, v] = into_crop(900.0, 600.0, rect, 0.0, Perspective::default(), *p);
            (0.0..=1.0).contains(&u) && (0.0..=1.0).contains(&v)
        })
    };

    let mut classes = vec![4u8; 90 * 60];
    for y in 20..50 {
        for x in 40..52 {
            classes[y * 90 + x] = 12;
        }
    }
    let gull = [0.30, 0.25, 0.32, 0.30];
    let held = super::scene::Scene { classes: (90, 60, classes), objects: vec![(14, gull)], ..empty_scene() };
    let (found, _) = proposals(&held, WHOLE, 0.0, Perspective::default(), (7728, 5152));
    assert!(!found.is_empty());
    assert!(found.iter().all(|p| inside(p.rect, gull)), "{found:?} cut the gull");

    let train = [0.05, 0.45, 0.45, 0.25];
    let rails = super::scene::Scene { objects: vec![(6, train)], ..empty_scene() };
    let (found, _) = proposals(&rails, WHOLE, 0.0, Perspective::default(), (7728, 5152));
    assert!(!found.is_empty());
    assert!(found.iter().all(|p| inside(p.rect, [0.27, 0.57, 0.01, 0.01])), "{found:?} lost the train");
}

#[test]
fn crops_are_placed_round_what_stands_out_where_nothing_is_named() {
    use super::crop::proposals;
    use numa_core::image::into_crop;
    let mut salient = vec![0u8; 48 * 32];
    for y in 6..18 {
        for x in 30..34 {
            salient[y * 48 + x] = 255;
        }
    }
    let pagoda = super::scene::Scene { salient: (48, 32, salient), ..empty_scene() };
    let (found, _) = proposals(&pagoda, WHOLE, 0.0, Perspective::default(), (7728, 5152));
    assert!(!found.is_empty());
    for proposal in &found {
        let [u, v] = into_crop(900.0, 600.0, proposal.rect, 0.0, Perspective::default(), [32.0 / 48.0, 12.0 / 32.0]);
        assert!((0.0..=1.0).contains(&u) && (0.0..=1.0).contains(&v), "{proposal:?} lost the pagoda");
    }
    assert!(found[0].why.contains("subject on a third"), "{found:?}");
}

#[test]
fn a_crop_passes_through_a_person_between_the_joints_only() {
    use super::corners::cuts;

    let figure = |feet_row: usize| {
        let mut classes = vec![4u8; 90 * 60];
        for y in 8..=feet_row {
            for x in 40..50 {
                classes[y * 90 + x] = 12;
            }
        }
        super::scene::Scene { classes: (90, 60, classes), faces: vec![[0.43, 0.15, 0.053, 0.08]], ..empty_scene() }
    };
    let standing = figure(50);
    let crop = |scene: &super::scene::Scene, bottom: f32, through: bool| {
        let crowd = through.then(|| {
            let mut crowd = super::corners::people(scene);
            crowd.subject = super::crop::choose(scene, &crowd).unwrap();
            crowd
        });
        cuts(scene, (WHOLE, 0.0), ([0.2, 0.0, 0.6, bottom], 0.0), Perspective::default(), crowd.as_ref()).is_none()
    };
    for (bottom, passes, at) in [
        (0.27, false, "the neck"),
        (0.40, false, "the waist"),
        (0.50, true, "the hips"),
        (0.55, true, "mid-thigh"),
        (0.65, false, "the knees"),
        (0.72, true, "mid-calf"),
        (0.82, false, "the ankles"),
        (0.95, true, "below the feet"),
    ] {
        assert_eq!(crop(&standing, bottom, true), passes, "a crop at {at}");
    }
    assert!(!crop(&standing, 0.50, false), "a level keeps the person whole");
    let seated = figure(35);
    assert!(crop(&seated, 0.33, true), "at the chest");
    assert!(!crop(&seated, 0.50, true), "at a seated person's lap");

    let mut classes = vec![4u8; 90 * 60];
    for y in 47..60 {
        for x in 40..50 {
            classes[y * 90 + x] = 12;
        }
    }
    let bust = super::scene::Scene { classes: (90, 60, classes), faces: vec![[0.43, 0.80, 0.053, 0.12]], ..empty_scene() };
    assert!(!crop(&bust, 0.98, true), "a trim through the neck");
    assert!(crop(&bust, 0.98, false), "a level may trim it");
}

#[test]
fn a_face_looking_aside_gets_room_in_front_and_the_mirror_mirrors_it() {
    use super::crop::proposals;
    let person = |mirror: bool| {
        let flip = |u: f32, w: f32| if mirror { 1.0 - u - w } else { u };
        let mut classes = vec![4u8; 90 * 60];
        for y in 20..44 {
            for x in 42..50 {
                classes[y * 90 + if mirror { 89 - x } else { x }] = 12;
            }
        }

        let points = [[0.48, 0.37], [0.50, 0.37], [0.498, 0.40], [0.483, 0.43], [0.497, 0.43]].map(|[u, v]| [flip(u, 0.0), v]);
        super::scene::Scene {
            classes: (90, 60, classes),
            faces: vec![[flip(0.47, 0.04), 0.34, 0.04, 0.10]],
            landmarks: vec![points],
            ..empty_scene()
        }
    };
    let (right, _) = proposals(&person(false), WHOLE, 0.0, Perspective::default(), (7728, 5152));
    let (left, _) = proposals(&person(true), WHOLE, 0.0, Perspective::default(), (7728, 5152));
    let [x, _, w, _] = right[0].rect;
    assert!(x + w / 2.0 > 0.49 + 0.05, "the room is in front, on the right: {right:?}");
    assert!(right[0].why.contains("looks") || right[0].why.contains("third"), "{right:?}");
    let [mx, _, mw, _] = left[0].rect;
    assert!((mx - (1.0 - x - w)).abs() < 0.011 && (mw - w).abs() < 1e-4, "{right:?} against {left:?}");
}

#[test]
fn crops_leave_no_sliver_or_bright_mass_at_their_edge() {
    use super::crop::proposals;
    use numa_core::image::into_crop;
    let mut classes = vec![4u8; 90 * 60];
    for y in 12..50 {
        for x in 12..22 {
            classes[y * 90 + x] = 126;
        }
    }
    let heron = super::scene::Scene { classes: (90, 60, classes), ..empty_scene() };
    let passer_by = [0.88, 0.30, 0.10, 0.50];
    let street = super::scene::Scene { objects: vec![(0, passer_by)], ..empty_scene() };
    let (found, _) = proposals(&street, WHOLE, 0.0, Perspective::default(), (7728, 5152));
    assert!(!found.is_empty());
    for proposal in &found {
        let [x, y, w, h] = passer_by;
        let kept = (0..10)
            .flat_map(|i| (0..10).map(move |j| [x + w * (i as f32 + 0.5) / 10.0, y + h * (j as f32 + 0.5) / 10.0]))
            .filter(|p| {
                let [u, v] = into_crop(900.0, 600.0, proposal.rect, 0.0, Perspective::default(), *p);
                (0.0..=1.0).contains(&u) && (0.0..=1.0).contains(&v)
            })
            .count();
        assert!(kept == 0 || kept == 100, "{proposal:?} cuts the passer-by, keeping {kept} %");
    }

    let mut bright = vec![0u8; 48 * 32];
    for y in 0..32 {
        bright[y * 48] = 255;
        bright[y * 48 + 1] = 255;
    }
    let lamp = super::scene::Scene { bright: (48, 32, bright), ..heron };
    let (found, _) = proposals(&lamp, WHOLE, 0.0, Perspective::default(), (7728, 5152));
    assert!(!found.is_empty() && found.iter().all(|p| p.rect[0] > 0.03), "{found:?} keep the bright edge");
}

#[test]
fn a_crowd_is_split_into_its_people_and_the_af_point_chooses() {
    use super::corners::people;
    use super::crop::{choose, proposals};
    use numa_core::image::into_crop;

    let mut classes = vec![4u8; 90 * 60];
    for y in 15..55 {
        for x in 2..88 {
            classes[y * 90 + x] = 12;
        }
    }
    let walkers = [[0.0, 0.25, 0.20, 0.67], [0.78, 0.25, 0.21, 0.67]];
    let man = [0.42, 0.30, 0.16, 0.45];
    let street = super::scene::Scene {
        classes: (90, 60, classes),
        objects: vec![(0, walkers[0]), (0, man), (0, walkers[1])],
        camera: super::scene::Camera { focus: Some([0.50, 0.45]), ..Default::default() },
        ..empty_scene()
    };
    let mut crowd = people(&street);
    assert!(crowd.parts.len() >= 3, "{} parts", crowd.parts.len());
    let chosen = choose(&street, &crowd).expect("the AF point stands one out");
    let parts = crowd.parts.clone();
    assert_eq!(chosen.len(), 1);
    let [u0, v0] = [(parts[chosen[0]][0].0 as f32 + 0.5) / 90.0, (parts[chosen[0]][0].1 as f32 + 0.5) / 60.0];
    assert!((0.42..0.58).contains(&u0) && (0.30..0.75).contains(&v0), "the man in the middle, not {u0},{v0}");
    let (found, _) = proposals(&street, WHOLE, 0.0, Perspective::default(), (7728, 5152));
    assert!(!found.is_empty(), "a crop round the man");
    let kept = |rect: [f32; 4], [x, y, w, h]: [f32; 4]| {
        (0..10)
            .flat_map(|i| (0..10).map(move |j| [x + w * (i as f32 + 0.5) / 10.0, y + h * (j as f32 + 0.5) / 10.0]))
            .filter(|p| {
                let [u, v] = into_crop(900.0, 600.0, rect, 0.0, Perspective::default(), *p);
                (0.0..=1.0).contains(&u) && (0.0..=1.0).contains(&v)
            })
            .count()
    };
    assert!(found.iter().all(|p| kept(p.rect, man) == 100), "{found:?} cut the man");

    for (x, y) in [(10usize, 30usize), (80, 30)] {
        let walker = parts.iter().find(|part| part.iter().any(|c| (c.0, c.1) == (x, y))).unwrap();
        for p in &found {
            let inside = walker
                .iter()
                .filter(|&&(x, y, _)| {
                    let [u, v] = into_crop(900.0, 600.0, p.rect, 0.0, Perspective::default(), [(x as f32 + 0.5) / 90.0, (y as f32 + 0.5) / 60.0]);
                    (0.0..=1.0).contains(&u) && (0.0..=1.0).contains(&v)
                })
                .count();
            assert!(inside == 0 || inside == walker.len(), "{p:?} slices a walker");
        }
    }

    crowd.subject = chosen;
    let gate = |rect: [f32; 4]| super::corners::cuts(&street, (WHOLE, 0.0), (rect, 0.0), Perspective::default(), Some(&crowd));
    assert_eq!(gate([0.2, 0.2, 0.575, 0.78]), None);
    assert!(gate([0.2, 0.2, 0.7, 0.78]).is_some());

    let unsure = super::scene::Scene { camera: Default::default(), objects: vec![(0, walkers[0]), (0, [0.42, 0.25, 0.20, 0.67]), (0, walkers[1])], ..street };
    assert!(choose(&unsure, &people(&unsure)).is_none());
    assert!(proposals(&unsure, WHOLE, 0.0, Perspective::default(), (7728, 5152)).0.is_empty());

    let mut classes = vec![4u8; 90 * 60];
    for y in 10..58 {
        for x in 30..60 {
            classes[y * 90 + x] = 12;
        }
    }
    let close = super::scene::Scene {
        classes: (90, 60, classes),
        objects: vec![(0, [0.32, 0.15, 0.36, 0.83]), (0, [0.33, 0.55, 0.30, 0.42])],
        faces: vec![[0.45, 0.20, 0.10, 0.12]],
        ..empty_scene()
    };
    assert_eq!(people(&close).parts.len(), 1);
}

#[test]
#[ignore]
fn the_crops_against_the_photographers() {
    let (Ok(list), Ok(csv)) = (std::env::var("FRAMES_LIST"), std::env::var("OUT_CSV")) else {
        println!("set FRAMES_LIST and OUT_CSV");
        return;
    };
    let iou = |a: [f32; 4], b: [f32; 4]| {
        let x = (a[0] + a[2]).min(b[0] + b[2]) - a[0].max(b[0]);
        let y = (a[1] + a[3]).min(b[1] + b[3]) - a[1].max(b[1]);
        let inter = x.max(0.0) * y.max(0.0);
        inter / (a[2] * a[3] + b[2] * b[3] - inter)
    };
    let mut rows = vec!["file,theirs,beats,best_iou,proposals".to_string()];
    for line in std::fs::read_to_string(&list).unwrap().lines().filter(|l| !l.trim().is_empty()) {
        let Some((path, theirs)) = line.split_once('\t') else { continue };
        let theirs: Vec<f32> = theirs.split(',').filter_map(|v| v.trim().parse().ok()).collect();
        let [tx, ty, tw, th] = theirs[..] else { continue };
        let path = std::path::PathBuf::from(path);
        let Ok(Ok(full)) = std::panic::catch_unwind(|| numa_io::raw::decode_linear_any(&path)) else { continue };
        let size = (full.width, full.height);
        let linear = full.downscaled(2400).unwrap_or(full);
        let document = Document::new(path.display().to_string());
        let working = crate::to_working_space(&document, &linear, &Default::default());
        let focus = numa_io::raw::af_point(&path).filter(|point| !point.zone).map(|point| [point.x, point.y]);
        let scene = super::scene::read(&document, &working, None, super::scene::Camera { focus, ..Default::default() });
        let (found, beats) = super::crop::proposals(&scene, WHOLE, 0.0, Perspective::default(), size);
        let crowd = super::corners::people(&scene);
        println!("people {} subject {:?} focus {focus:?}", crowd.parts.len(), super::crop::choose(&scene, &crowd));
        let best = found.iter().map(|p| iou(p.rect, [tx, ty, tw, th])).fold(0.0f32, f32::max);
        let yaw: Vec<f32> = scene.faces.iter().zip(&scene.landmarks).map(|(f, [r, l, n, ..])| (n[0] - (r[0] + l[0]) / 2.0) / f[2]).collect();
        println!("{} objects {:?} faces {:?} yaw {yaw:?}", path.display(), scene.objects, scene.faces);
        let said = found.iter().map(|p| format!("{:?} {}", p.rect.map(|v| (v * 1000.0).round() / 1000.0), p.why)).collect::<Vec<_>>().join(" | ");
        let row = format!("{},\"{tx},{ty},{tw},{th}\",{beats},{best:.3},\"{said}\"", path.file_name().unwrap().to_string_lossy());
        println!("{row}");
        rows.push(row);
    }
    std::fs::write(csv, rows.join("\n") + "\n").unwrap();
}

#[test]
#[ignore]
fn animals_in_frames() {
    let (Ok(list), Ok(out)) = (std::env::var("FRAMES_LIST"), std::env::var("OUT_TSV")) else {
        println!("set FRAMES_LIST and OUT_TSV");
        return;
    };
    let mut rows = Vec::new();
    for path in std::fs::read_to_string(&list).unwrap().lines().map(str::trim).filter(|l| !l.is_empty()) {
        let path = std::path::PathBuf::from(path);
        let Ok(Ok((proxy, _))) = std::panic::catch_unwind(|| numa_io::raw::proxy_from_mosaic(&path, 800)) else { continue };
        let document = Document::new(path.display().to_string());
        let frame = crate::apply_stack(&document, &crate::to_working_space(&document, &proxy, &Default::default()), 1.0);
        let area = (frame.width() * frame.height()) as f32;

        for (class, [l, t, r, b]) in crate::distractions::objects(&frame).unwrap_or_default().into_iter().filter(|(c, _)| (14..=23).contains(c)) {
            rows.push(format!("{}\t{class}\t{:.4}", path.display(), (r - l) * (b - t) / area));
        }
    }
    std::fs::write(out, rows.join("\n") + "\n").unwrap();
}
