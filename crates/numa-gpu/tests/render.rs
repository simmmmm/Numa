use numa_core::color::WhiteBalance;
use numa_core::curve::Curve;
use numa_core::document::{Basic, Document};
use numa_core::image::LinearImage;
use numa_core::grading::{Grading, Range};
use numa_core::mask::{Mask, Shape, Tint};
use numa_core::mixer::Mixer;
use numa_core::point::{PointColour, PointColours};
use std::path::{Path, PathBuf};

fn ask_for_the_render() {
    std::env::set_var("NUMA_GPU_RENDER", "1");
}

fn frames() -> Vec<PathBuf> {
    if let Some(folder) = std::env::var_os("NUMA_GPU_RAWS") {
        let mut found: Vec<PathBuf> = std::fs::read_dir(&folder).into_iter().flatten().flatten().map(|entry| entry.path()).filter(|path| numa_io::raw::is_raw(path)).collect();
        found.sort();
        return found;
    }
    let corpus = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../tests/corpus");
    ["Olympus/E-M1MarkII.ORF", "Fujifilm/X-T5.RAF", "Sony/ILCE-6000.ARW"].iter().map(|name| corpus.join(name)).filter(|path| path.is_file()).collect()
}

fn edits(path: &str) -> Vec<(&'static str, Document)> {
    let with = |change: &dyn Fn(&mut Document)| {
        let mut document = Document::new(path.to_string());
        change(&mut document);
        document
    };
    let basic = |change: fn(&mut Basic)| move |document: &mut Document| document.set_basic(Basic::with(change));
    let curves = |document: &mut Document| {
        document.set_curves([
            Curve::new([[0.0, 0.0], [0.25, 0.18], [0.75, 0.85], [1.0, 1.0]]),
            Curve::new([[0.0, 0.04], [1.0, 0.96]]),
            Curve::identity(),
            Curve::new([[0.0, 0.0], [0.5, 0.56], [1.0, 1.0]]),
        ])
    };
    let mixer = |document: &mut Document| {
        let mut mixer = Mixer::default();
        mixer.bands[1] = [20.0, 30.0, -20.0];
        mixer.bands[3] = [-15.0, -40.0, 25.0];
        mixer.bands[5] = [10.0, 50.0, -30.0];
        document.set_mixer(mixer)
    };
    let everything = |document: &mut Document| {
        document.set_basic(Basic::with(|b| {
            b.tone.exposure = 0.4;
            b.tone.contrast = 25.0;
            b.tone.highlights = -50.0;
            b.tone.shadows = 40.0;
            b.tone.whites = 15.0;
            b.tone.blacks = -20.0;
            b.presence.vibrance = 25.0;
            b.presence.saturation = 10.0;
            b.detail.denoise_colour = 60.0;
        }));
        document.white_balance = Some(WhiteBalance { temperature: 4300.0, tint: 12.0 });
        curves(document);
        mixer(document);
    };
    vec![
        ("untouched", with(&|_| {})),
        ("exposure +1.5", with(&basic(|b| b.tone.exposure = 1.5))),
        ("exposure -2", with(&basic(|b| b.tone.exposure = -2.0))),
        ("contrast +60", with(&basic(|b| b.tone.contrast = 60.0))),
        ("contrast -60", with(&basic(|b| b.tone.contrast = -60.0))),
        ("highlights -80 shadows +70", with(&basic(|b| {
            b.tone.highlights = -80.0;
            b.tone.shadows = 70.0;
        }))),
        ("whites +40 blacks -60", with(&basic(|b| {
            b.tone.whites = 40.0;
            b.tone.blacks = -60.0;
        }))),
        ("saturation +40", with(&basic(|b| b.presence.saturation = 40.0))),
        ("vibrance +60", with(&basic(|b| b.presence.vibrance = 60.0))),
        ("no colour noise reduction", with(&basic(|b| b.detail.denoise_colour = 0.0))),
        ("white balance 3200 K", with(&|d| d.white_balance = Some(WhiteBalance { temperature: 3200.0, tint: -8.0 }))),
        ("no camera profile", with(&|d| d.colour_profile = Some(numa_render::NO_COLOUR_PROFILE.to_string()))),
        ("curves", with(&curves)),
        ("mixer", with(&mixer)),
        ("turned 90", with(&|d| d.set_rotation(90.0))),
        ("mirrored, turned 270", with(&|d| {
            d.set_mirrored(true);
            d.set_rotation(270.0);
        })),
        ("cropped", with(&|d| d.set_crop([0.1, 0.15, 0.6, 0.55], 0.0))),
        ("straightened 3.7°", with(&|d| d.set_crop([0.05, 0.05, 0.9, 0.9], 3.7))),
        ("perspective, mirrored, turned 180", with(&|d| {
            d.set_mirrored(true);
            d.set_rotation(180.0);
            d.set_perspective(numa_core::document::Perspective { vertical: 25.0, horizontal: -10.0, aspect: 8.0 });
            d.set_crop([0.08, 0.1, 0.8, 0.75], -2.2);
        })),
        ("grading", with(&|d| d.set_grading(grading()))),
        ("point colour", with(&|d| d.set_point_colours(points(None)))),
        ("point colour, shown", with(&|d| d.set_point_colours(points(Some(2))))),
        ("black and white", with(&|d| d.set_mixer(monochrome()))),
        ("vignette", with(&basic(|b| {
            b.effects.vignette = -50.0;
            b.effects.vignette_midpoint = 40.0;
            b.effects.vignette_roundness = 20.0;
            b.effects.vignette_feather = 60.0;
        }))),
        ("vignette, squared", with(&basic(|b| {
            b.effects.vignette = 30.0;
            b.effects.vignette_roundness = -70.0;
        }))),

        ("luminance NR 40", with(&basic(|b| b.detail.denoise_luma = 40.0))),
        ("luminance NR 70, detail 20, contrast 60", with(&basic(|b| {
            b.detail.denoise_luma = 70.0;
            b.detail.denoise_detail = 20.0;
            b.detail.denoise_contrast = 60.0;
        }))),
        ("HDR +60", with(&basic(|b| b.presence.hdr = 60.0))),
        ("HDR -40", with(&basic(|b| b.presence.hdr = -40.0))),
        ("Clarity +50", with(&basic(|b| b.presence.clarity = 50.0))),
        ("Clarity -40", with(&basic(|b| b.presence.clarity = -40.0))),
        ("Texture +60", with(&basic(|b| b.presence.texture = 60.0))),
        ("Texture -50", with(&basic(|b| b.presence.texture = -50.0))),
        ("HDR, Clarity, Texture, luminance NR", with(&basic(|b| {
            b.presence.hdr = 40.0;
            b.presence.clarity = 30.0;
            b.presence.texture = 25.0;
            b.detail.denoise_luma = 30.0;
            b.tone.exposure = 0.3;
        }))),
        ("mask: gradient", with(&|d| d.set_masks(vec![gradient()]))),
        ("mask: radial, warmth", with(&|d| d.set_masks(vec![radial()]))),
        ("mask: colour NR, curves", with(&|d| d.set_masks(vec![curved()]))),
        ("mask: black and white, grade, Color", with(&|d| d.set_masks(vec![toned()]))),
        ("mask: mixer, point colour", with(&|d| d.set_masks(vec![coloured()]))),
        ("masks, five, turned and cropped", with(&|d| {
            d.set_masks(vec![gradient(), radial(), curved(), toned(), coloured()]);
            d.set_rotation(90.0);
            d.set_crop([0.1, 0.05, 0.8, 0.85], 2.0);
        })),
        ("everything", with(&|d| {
            everything(d);
            d.set_rotation(90.0);
            d.set_crop([0.05, 0.05, 0.9, 0.9], 1.5);
        })),
        ("everything, and grading, point colour, vignette and masks", with(&|d| {
            everything(d);
            d.set_grading(grading());
            d.set_point_colours(points(None));
            let mut basic = d.basic();
            basic.effects.vignette = -30.0;
            basic.presence.clarity = 25.0;
            basic.presence.texture = -20.0;
            basic.detail.denoise_luma = 25.0;
            d.set_basic(basic);
            d.set_masks(vec![gradient(), curved(), coloured()]);
        })),
    ]
}

fn grading() -> Grading {
    Grading {
        shadows: Range { hue: 220.0, saturation: 40.0, luminance: -20.0 },
        highlights: Range { hue: 40.0, saturation: 30.0, luminance: 10.0 },
        global: Range { hue: 100.0, saturation: 10.0, luminance: 0.0 },
        blending: 70.0,
        balance: -30.0,
        ..Grading::default()
    }
}

fn points(highlight: Option<usize>) -> PointColours {
    PointColours {
        points: vec![
            PointColour { hue: 60.0, saturation: 40.0, luminance: -30.0, ..PointColour::picked([0.5, 0.25, 0.1]) },
            PointColour { hue: -40.0, saturation: -50.0, luminance: 20.0, range: 80.0, ..PointColour::picked([0.1, 0.2, 0.5]) },
            PointColour::picked([0.1, 0.4, 0.1]),
        ],
        highlight,
    }
}

fn monochrome() -> Mixer {
    Mixer { monochrome: true, grey: [40.0, 20.0, -10.0, -30.0, 10.0, -50.0, 0.0, 30.0], ..Mixer::default() }
}

fn gradient() -> Mask {
    let mut mask = Mask::new(Shape::Linear { from: [0.5, 0.6], to: [0.5, 0.1] });
    mask.basic.tone.exposure = -0.8;
    mask.basic.tone.contrast = 20.0;
    mask.basic.tone.shadows = 30.0;
    mask.basic.tone.highlights = -40.0;
    mask
}

fn radial() -> Mask {
    let mut mask = Mask::new(Shape::Radial { centre: [0.4, 0.55], radius: [0.3, 0.25], feather: 0.6 });
    mask.basic.balance.temperature = 800.0;
    mask.basic.balance.tint = 10.0;
    mask.basic.presence.saturation = 30.0;
    mask.basic.presence.vibrance = 20.0;
    mask
}

fn curved() -> Mask {
    let mut mask = Mask::new(Shape::Radial { centre: [0.6, 0.4], radius: [0.35, 0.3], feather: 0.4 });
    mask.inverted = true;
    mask.basic.detail.denoise_colour = 25.0;
    mask.curve = Curve::new([[0.0, 0.05], [0.3, 0.22], [0.7, 0.8], [1.0, 0.95]]);
    mask.channel_curves[0] = Curve::new([[0.0, 0.0], [0.5, 0.58], [1.0, 1.0]]);
    mask
}

fn toned() -> Mask {
    let mut mask = Mask::new(Shape::Linear { from: [0.2, 0.9], to: [0.6, 0.5] });
    mask.mixer = monochrome();
    mask.grading = grading();
    mask.colour = Tint { hue: 200.0, saturation: 50.0 };
    mask.opacity = 0.7;
    mask
}

fn coloured() -> Mask {
    let mut mask = Mask::new(Shape::Radial { centre: [0.5, 0.5], radius: [0.4, 0.4], feather: 0.8 });
    mask.mixer.bands[3] = [-15.0, -40.0, 25.0];
    mask.mixer.bands[5] = [10.0, 50.0, -30.0];
    mask.point_colours = points(None);
    mask
}

fn difference(cpu: &[u8], card_rgba: &[u8], width: usize, stride: usize) -> (u8, f64) {
    let (mut most, mut moved) = (0u8, 0usize);
    for (y, row) in cpu.chunks_exact(width * 3).enumerate() {
        let card = &card_rgba[y * stride..y * stride + width * 4];
        for (x, pixel) in row.chunks_exact(3).enumerate() {
            for c in 0..3 {
                let step = pixel[c].abs_diff(card[x * 4 + c]);
                most = most.max(step);
                moved += (step > 0) as usize;
            }
        }
    }
    (most, moved as f64 / cpu.len() as f64)
}

fn finished_picture(proxy: &LinearImage, path: &str) -> LinearImage {
    let document = Document::new(path.to_string());
    let mut image = numa_render::to_working_space(&document, proxy, &numa_io::inputs::render_inputs(&document));
    image.profile = None;
    image.clip = None;
    image.rendering = None;
    image.white_point = None;
    image.display_referred = true;
    image
}

#[test]
fn the_card_renders_what_the_processor_does() {
    let _ = env_logger::builder().is_test(true).try_init();
    let frames = frames();
    let frugal = std::env::var_os("NUMA_GPU_LOW").is_some();
    ask_for_the_render();
    if frames.is_empty() || !numa_gpu::open_now(frugal) {
        assert!(!numa_gpu::broken(), "the develop did not build on this card");
        println!("skipped: no corpus or no card");
        return;
    }
    for path in frames {
        let name = path.file_name().unwrap().to_string_lossy().to_string();
        let (raw, full) = numa_io::raw::proxy_from_mosaic(&path, 2400).expect("decodes");
        let scale = raw.width.max(raw.height) as f32 / full.0.max(full.1).max(1) as f32;
        let finished = finished_picture(&raw, &path.to_string_lossy());
        for (source, kind) in [(&raw, "raw"), (&finished, "finished")] {
            for (what, document) in edits(&path.to_string_lossy()) {
                if kind == "finished" && !matches!(what, "untouched" | "exposure +1.5" | "curves" | "mixer" | "vibrance +60" | "straightened 3.7°" | "grading" | "mask: gradient" | "mask: radial, warmth" | "mask: colour NR, curves" | "HDR +60" | "luminance NR 40" | "Clarity -40") {
                    continue;
                }
                let inputs = numa_io::inputs::render_inputs(&document);
                let plan = numa_render::card::plan(&document, source, &inputs, scale).unwrap_or_else(|stage| panic!("{what}: the card lacks {stage}"));
                let working = numa_render::to_working_space(&document, source, &inputs);
                let cpu = numa_render::apply_stack(&document, &working, scale);
                let histogram = numa_render::histogram::of(&cpu);
                let card = numa_gpu::render(source, &plan, 0, frugal, numa_gpu::Output::ReadBack).expect("the card renders");
                assert_eq!((card.width, card.height), cpu.dimensions());
                let numa_gpu::Pixels::Rgba { bytes, stride } = &card.pixels else { panic!("read back") };
                let (most, moved) = difference(cpu.as_raw(), bytes, card.width as usize, *stride);
                let bins: [u32; 770] = std::array::from_fn(|k| match k {
                    768 => histogram.shadow_clipped,
                    769 => histogram.highlight_clipped,
                    _ => histogram.channels[k / 256][k % 256],
                });
                let off: u64 = bins.iter().zip(&card.histogram).map(|(a, b)| u64::from(a.abs_diff(*b))).sum();
                println!("{name} {kind} {what}: 8-bit max {most}, moved {:.4}%, histogram {off} counts off of {}", moved * 100.0, histogram.total * 3);
                assert!(most <= 2, "{name} {kind} {what}: {most} levels apart");
                assert!(moved < 5e-3, "{name} {kind} {what}: {:.3}% of values moved", moved * 100.0);
                assert!((off as f64) < histogram.total as f64 * 3.0 * 1e-2, "{name} {kind} {what}: the histogram is {off} counts off");

                let width = card.width as usize;
                let mut own = image::RgbImage::from_fn(card.width, card.height, |x, y| {
                    let at = y as usize * stride + x as usize * 4;
                    image::Rgb([bytes[at], bytes[at + 1], bytes[at + 2]])
                });
                numa_render::histogram::mark_clipping(&mut own, numa_render::histogram::ClippingOverlay { shadows: true, highlights: true });
                let marked = numa_gpu::render(source, &plan, 3, frugal, numa_gpu::Output::ReadBack).expect("the card renders");
                let numa_gpu::Pixels::Rgba { bytes, stride } = &marked.pixels else { panic!("read back") };
                assert_eq!(difference(own.as_raw(), bytes, width, *stride), (0, 0.0), "{name} {kind} {what}: the overlay");
            }
        }
    }
}

#[test]
fn a_stage_the_card_lacks_is_named() {
    let image = LinearImage::new(4, 4, vec![0.2; 48]);
    let mut document = Document::new(String::new());
    document.set_basic(Basic::with(|b| b.effects.dehaze = 30.0));
    let plan = numa_render::card::plan(&document, &image, &Default::default(), 0.3);
    assert_eq!(plan.err(), Some("dehaze"));
    let document = {
        let mut document = Document::new(String::new());
        document.set_basic(Basic::with(|b| b.optics.lens_distortion = 10.0));
        document
    };
    assert_eq!(numa_render::card::plan(&document, &image, &Default::default(), 0.3).err(), Some("manual lens correction"));
}

#[test]
fn a_discrete_card_takes_the_heavy_edits() {
    let image = LinearImage::new(2400, 1600, vec![0.2; 2400 * 1600 * 3]);
    let heavy = |change: &dyn Fn(&mut Document)| {
        let mut document = Document::new(String::new());
        change(&mut document);
        numa_render::card::plan(&document, &image, &Default::default(), 0.3).expect("the card has it").heavy()
    };
    assert!(!heavy(&|_| {}), "untouched");
    assert!(!heavy(&|d| d.set_basic(Basic::with(|b| {
        b.tone.exposure = 1.0;
        b.tone.contrast = 20.0;
        b.tone.shadows = 30.0;
    }))), "basic sliders");
    assert!(!heavy(&|d| d.set_masks(vec![gradient()])), "a mask");
    assert!(heavy(&|d| d.set_grading(grading())), "a grade");
    assert!(heavy(&|d| d.set_masks(vec![curved()])), "a mask's curves");
    assert!(heavy(&|d| {
        d.set_masks(vec![gradient(), radial()]);
        d.set_basic(Basic::with(|b| b.presence.hdr = 18.0));
        d.set_grading(grading());
    }), "his DSCF3204's kind: masks, HDR and a grade");
}

#[test]
fn a_small_frame_renders() {
    ask_for_the_render();
    for frugal in [false, true] {
        if !numa_gpu::open_now(frugal) {
            assert!(!numa_gpu::broken(), "the develop did not build on this card");
            println!("skipped: no {} card", if frugal { "integrated" } else { "suitable" });
            continue;
        }
        small_frame(frugal);
    }
}

fn small_frame(frugal: bool) {
    let data: Vec<f32> = (0..64 * 48 * 3).map(|i| ((i * 37) % 101) as f32 / 90.0).collect();
    let mut image = LinearImage::new(64, 48, data);
    image.display_referred = true;
    let mut document = Document::new(String::new());
    document.set_basic(Basic::with(|b| b.tone.exposure = 0.5));
    document.set_rotation(90.0);
    document.set_crop([0.1, 0.1, 0.8, 0.8], 5.0);

    document.set_grading(Grading { global: Range { hue: 30.0, saturation: 30.0, luminance: 10.0 }, ..Grading::default() });
    let mut mask = Mask::new(Shape::Radial { centre: [0.5, 0.5], radius: [0.3, 0.3], feather: 0.5 });
    mask.basic.tone.exposure = 0.7;
    mask.basic.detail.denoise_colour = 25.0;
    document.set_masks(vec![mask]);
    let mut basic = document.basic();
    basic.detail.denoise_luma = 40.0;
    basic.detail.denoise_contrast = 50.0;
    basic.presence.clarity = -30.0;
    basic.presence.texture = 40.0;
    basic.presence.hdr = 50.0;
    document.set_basic(basic);
    let plan = numa_render::card::plan(&document, &image, &Default::default(), 1.0).expect("the card has these");
    let cpu = numa_render::apply_stack(&document, &image, 1.0);
    let card = numa_gpu::render(&image, &plan, 0, frugal, numa_gpu::Output::ReadBack).expect("the card renders");
    let numa_gpu::Pixels::Rgba { bytes, stride } = &card.pixels else { panic!("read back") };
    assert_eq!((card.width, card.height), cpu.dimensions());
    let (most, moved) = difference(cpu.as_raw(), bytes, card.width as usize, *stride);
    assert!(most <= 1 && moved < 0.01, "{most} levels apart on {:.2}%", moved * 100.0);
}

#[test]
fn a_masks_temperature_moves_a_turned_cropped_frame() {
    let data: Vec<f32> = (0..64 * 48 * 3).map(|i| 0.1 + ((i * 37) % 101) as f32 / 150.0).collect();
    let mut image = LinearImage::new(64, 48, data);
    image.display_referred = true;
    let with = |temperature: f32| {
        let mut document = Document::new(String::new());
        document.set_mirrored(true);
        document.set_rotation(90.0);
        document.set_crop([0.1, 0.1, 0.8, 0.8], 3.0);
        let mut mask = Mask::new(Shape::Radial { centre: [0.5, 0.5], radius: [0.4, 0.4], feather: 0.5 });
        mask.basic.balance.temperature = temperature;
        document.set_masks(vec![mask]);
        document
    };
    let (plain, warm) = (with(0.0), with(1500.0));

    let warmth = |pixels: &mut dyn Iterator<Item = &[u8]>| pixels.map(|p| f64::from(p[0]) - f64::from(p[2])).sum::<f64>();
    let processor = |document: &Document| numa_render::apply_stack(document, &numa_render::to_working_space(document, &image, &Default::default()), 1.0);
    let (cpu_plain, cpu_warm) = (processor(&plain), processor(&warm));
    let (before, after) = (warmth(&mut cpu_plain.as_raw().chunks_exact(3)), warmth(&mut cpu_warm.as_raw().chunks_exact(3)));
    assert!(after > before + 1000.0, "the processor: red over blue {before} → {after}");

    ask_for_the_render();
    if !numa_gpu::open_now(false) {
        assert!(!numa_gpu::broken(), "the develop did not build on this card");
        println!("skipped the card: none here");
        return;
    }
    let card = |document: &Document| {
        let plan = numa_render::card::plan(document, &image, &Default::default(), 1.0).expect("the card has these");
        let rendered = numa_gpu::render(&image, &plan, 0, false, numa_gpu::Output::ReadBack).expect("the card renders");
        let numa_gpu::Pixels::Rgba { bytes, stride } = rendered.pixels else { panic!("read back") };
        let rows: Vec<u8> = bytes.chunks_exact(stride).flat_map(|row| row[..rendered.width as usize * 4].to_vec()).collect();
        (rows, rendered.width as usize)
    };
    let ((card_plain, width), (card_warm, _)) = (card(&plain), card(&warm));
    let (before, after) = (warmth(&mut card_plain.chunks_exact(4)), warmth(&mut card_warm.chunks_exact(4)));
    assert!(after > before + 1000.0, "the card: red over blue {before} → {after}");
    let (most, moved) = difference(cpu_warm.as_raw(), &card_warm, width, width * 4);
    assert!(most <= 1 && moved < 0.01, "the card against the processor: {most} levels apart on {:.2}%", moved * 100.0);
}

fn painted(width: u32, height: u32) -> Mask {
    let mut mask = Mask::new(Shape::Painted);
    let mut lasso = numa_core::mask::Stroke::soft_lasso(0.02, 0.5, false);
    lasso.points = vec![[0.2, 0.3], [0.7, 0.25], [0.8, 0.7], [0.3, 0.8]];
    mask.strokes.push(lasso);
    let (w, h) = numa_render::raster_size(width, height, numa_render::MASK_RASTER);
    numa_render::resolve_mask(&mut mask, None, None, None, w, h);
    mask.basic.tone.exposure = 0.5;
    mask.basic.tone.shadows = 20.0;
    mask
}

fn power() -> (f64, f64) {
    let mut card = 0.0;
    let mut socket = 0.0;
    for entry in std::fs::read_dir("/sys/class/drm").into_iter().flatten().flatten() {
        let device = entry.path().join("device");
        for hwmon in std::fs::read_dir(device.join("hwmon")).into_iter().flatten().flatten() {
            if let Ok(text) = std::fs::read_to_string(hwmon.path().join("power1_average")) {
                card += text.trim().parse::<f64>().unwrap_or(0.0) / 1e6;
            }
        }

        if let Ok(metrics) = std::fs::read(device.join("gpu_metrics")) {
            if metrics.len() > 42 && metrics[2] == 2 {
                socket = f64::from(u16::from_le_bytes([metrics[40], metrics[41]]));
            }
        }
    }
    (card, socket)
}

fn cpu_seconds() -> f64 {

    let usage = unsafe {
        let mut usage: libc::rusage = std::mem::zeroed();
        libc::getrusage(libc::RUSAGE_SELF, &mut usage);
        usage
    };
    let seconds = |t: libc::timeval| t.tv_sec as f64 + t.tv_usec as f64 / 1e6;
    seconds(usage.ru_utime) + seconds(usage.ru_stime)
}

fn drag(frames: usize, idle: (f64, f64), mut frame: impl FnMut()) -> (f64, f64, f64, f64) {

    std::thread::sleep(std::time::Duration::from_secs(3));
    let samples = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let stop = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let sampler = {
        let (samples, stop) = (samples.clone(), stop.clone());
        std::thread::spawn(move || {
            while !stop.load(std::sync::atomic::Ordering::Relaxed) {
                samples.lock().unwrap().push(power());
                std::thread::sleep(std::time::Duration::from_millis(50));
            }
        })
    };
    let (started, cpu) = (std::time::Instant::now(), cpu_seconds());
    let mut times = Vec::new();
    for k in 0..frames {
        let tick = started + std::time::Duration::from_micros(16_667 * k as u64);
        if let Some(wait) = tick.checked_duration_since(std::time::Instant::now()) {
            std::thread::sleep(wait);
        }
        let at = std::time::Instant::now();
        frame();
        times.push(at.elapsed().as_secs_f64() * 1e3);
    }
    let (seconds, cpu) = (started.elapsed().as_secs_f64(), cpu_seconds() - cpu);
    stop.store(true, std::sync::atomic::Ordering::Relaxed);
    sampler.join().unwrap();
    let samples = samples.lock().unwrap();
    let mean = |pick: fn(&(f64, f64)) -> f64| samples.iter().map(pick).sum::<f64>() / samples.len().max(1) as f64;
    let watts = (mean(|s| s.0) - idle.0).max(0.0) + (mean(|s| s.1) - idle.1).max(0.0);
    times.sort_by(f64::total_cmp);
    if std::env::var_os("NUMA_BENCH_TRACE").is_some() {
        println!("  card W: {:?}", samples.iter().map(|s| s.0.round() as i32).collect::<Vec<_>>());
        println!("  package W: {:?}", samples.iter().map(|s| s.1.round() as i32).collect::<Vec<_>>());
    }
    (times[times.len() / 2], cpu / frames as f64, watts * seconds / frames as f64, (mean(|s| s.0) - idle.0).max(0.0) * seconds / frames as f64)
}

#[test]
#[ignore]
fn what_a_drag_costs() {
    ask_for_the_render();
    let frames = frames();
    if frames.is_empty() || !numa_gpu::open_now(false) {
        println!("skipped: no corpus or no card");
        return;
    }
    let path = frames.iter().find(|path| path.to_string_lossy().contains("X-T5")).unwrap_or(&frames[0]);
    let (raw, full) = numa_io::raw::proxy_from_mosaic(path, 2400).expect("decodes");
    let scale = raw.width.max(raw.height) as f32 / full.0.max(full.1).max(1) as f32;
    let name = path.to_string_lossy().to_string();
    let with = |change: &dyn Fn(&mut Document)| {
        let mut document = Document::new(name.clone());
        change(&mut document);
        document
    };
    let (w, h) = (raw.width, raw.height);
    let cases = [
        ("untouched", with(&|_| {})),
        ("basic sliders, curves, mixer", with(&|d| {
            d.set_basic(Basic::with(|b| {
                b.tone.contrast = 20.0;
                b.tone.highlights = -40.0;
                b.tone.shadows = 30.0;
                b.presence.vibrance = 20.0;
            }));
            d.white_balance = Some(WhiteBalance { temperature: 4800.0, tint: 5.0 });
            d.set_curves([Curve::new([[0.0, 0.0], [0.25, 0.2], [0.75, 0.82], [1.0, 1.0]]), Curve::identity(), Curve::identity(), Curve::identity()]);
            d.set_mixer({
                let mut mixer = Mixer::default();
                mixer.bands[3] = [-15.0, -40.0, 25.0];
                mixer
            });
        })),
        ("black and white, vignette", with(&|d| {
            d.set_mixer(monochrome());
            d.set_basic(Basic::with(|b| b.effects.vignette = -30.0));
        })),
        ("grading", with(&|d| d.set_grading(grading()))),
        ("point colour", with(&|d| d.set_point_colours(points(None)))),
        ("mask: gradient", with(&|d| d.set_masks(vec![gradient()]))),
        ("mask: painted", with(&|d| d.set_masks(vec![painted(raw.width, raw.height)]))),
        ("mask: colour NR, curves", with(&|d| d.set_masks(vec![curved()]))),
        ("masks: painted, gradient", with(&|d| d.set_masks(vec![painted(w, h), gradient()]))),
        ("masks: painted, gradient, colour NR", with(&|d| d.set_masks(vec![painted(w, h), gradient(), curved()]))),
        ("luminance NR 30", with(&|d| d.set_basic(Basic::with(|b| b.detail.denoise_luma = 30.0)))),
        ("HDR +50", with(&|d| d.set_basic(Basic::with(|b| b.presence.hdr = 50.0)))),
        ("Clarity +30, Texture +20", with(&|d| d.set_basic(Basic::with(|b| {
            b.presence.clarity = 30.0;
            b.presence.texture = 20.0;
        })))),

        ("two masks, HDR 18, luminance NR 48, grading", with(&|d| {
            d.set_masks(vec![painted(w, h), gradient()]);
            d.set_basic(Basic::with(|b| {
                b.presence.hdr = 18.0;
                b.detail.denoise_luma = 48.0;
            }));
            d.set_grading(grading());
        })),
    ];
    std::thread::sleep(std::time::Duration::from_secs(2));
    let idle = {
        let samples: Vec<(f64, f64)> = (0..40).map(|_| (power(), std::thread::sleep(std::time::Duration::from_millis(50))).0).collect();
        (samples.iter().map(|s| s.0).sum::<f64>() / 40.0, samples.iter().map(|s| s.1).sum::<f64>() / 40.0)
    };
    println!("idle: card {:.1} W, package {:.1} W; {} at {}×{}", idle.0, idle.1, path.display(), raw.width, raw.height);

    let ticked = |document: &Document, tick: &mut f32| {
        *tick += 0.01;
        let mut document = document.clone();
        let mut basic = document.basic();
        basic.tone.exposure += *tick;
        document.set_basic(basic);
        document
    };
    for (what, document) in &cases {
        let inputs = numa_io::inputs::render_inputs(document);

        let working = numa_render::to_working_space(document, &raw, &inputs);
        let draft = std::sync::Arc::new(working.downscaled(working.width.max(working.height) / 2).expect("smaller"));
        let draft_scale = scale * draft.width as f32 / working.width as f32;
        let mut tick = 0.0;
        let cpu = drag(120, idle, || {
            let frame = numa_render::apply_stack_kept(&ticked(document, &mut tick), &draft, draft_scale);
            std::hint::black_box(numa_render::histogram::of(&frame));
        });
        numa_render::forget_kept();

        let mut tick = 0.0;
        let card = drag(120, idle, || {
            let plan = numa_render::card::plan(&ticked(document, &mut tick), &raw, &inputs, scale).expect("the card has it");
            std::hint::black_box(numa_gpu::render(&raw, &plan, 0, false, numa_gpu::Output::ReadBack).expect("renders"));
        });

        let low = numa_gpu::open_now(true).then(|| {
            drag(120, idle, || {
                let plan = numa_render::card::plan(document, &raw, &inputs, scale).expect("the card has it");
                std::hint::black_box(numa_gpu::render(&raw, &plan, 0, true, numa_gpu::Output::ReadBack).expect("renders"));
            })
        });
        if let Some(low) = low {
            println!("{what}: integrated GPU {:.1} ms, {:.3} CPU-s, {:.2} J a frame ({:.2} J the discrete card's)", low.0, low.1, low.2, low.3);
        }
        let plan_ms = {
            let at = std::time::Instant::now();
            for _ in 0..10 {
                std::hint::black_box(numa_render::card::plan(document, &raw, &inputs, scale).unwrap());
            }
            at.elapsed().as_secs_f64() * 100.0
        };
        println!(
            "{what}: processor (half draft) {:.1} ms, {:.3} CPU-s, {:.2} J a frame | card {:.1} ms (plan {plan_ms:.1}), {:.3} CPU-s, {:.2} J ({:.2} J the card's)",
            cpu.0, cpu.1, cpu.2, card.0, card.1, card.2, card.3
        );
    }
}
