use numa::core::document::Document;
use numa::core::curve::Curve;
use numa::core::mask::{Mask, Shape};
use numa::render;
use std::time::Instant;

fn cpu() -> f64 {
    let mut t = libc::timespec { tv_sec: 0, tv_nsec: 0 };
    unsafe { libc::clock_gettime(libc::CLOCK_PROCESS_CPUTIME_ID, &mut t) };
    t.tv_sec as f64 + t.tv_nsec as f64 * 1e-9
}

fn stage<T>(n: usize, mut f: impl FnMut(usize) -> T) -> (f64, f64) {
    std::hint::black_box(f(0));
    let (w0, c0) = (Instant::now(), cpu());
    for i in 0..n {
        std::hint::black_box(f(i + 1));
    }
    let wall = w0.elapsed().as_secs_f64();
    (wall * 1000.0 / n as f64, (cpu() - c0) / wall)
}

fn hash(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf2_9ce4_8422_2325u64, |hash, byte| (hash ^ u64::from(*byte)).wrapping_mul(0x100_0000_01b3))
}

fn edited(path: &str) -> Document {
    let mut document = Document::new(path.to_string());
    let mut basic = document.basic();
    basic.tone.exposure = 0.3;
    basic.tone.contrast = 15.0;
    basic.tone.highlights = -40.0;
    basic.tone.shadows = 30.0;
    basic.presence.vibrance = 15.0;
    basic.presence.clarity = 20.0;
    basic.effects.vignette = -15.0;
    document.set_basic(basic);
    let mut sky = Mask::new(Shape::Linear { from: [0.5, 0.0], to: [0.5, 0.45] });
    sky.basic.tone.exposure = -0.5;
    document.set_masks(vec![sky]);
    document
}

fn curved(path: &str) -> Document {
    let mut document = edited(path);
    let mut masks = document.masks();
    masks[0].curve = Curve::new([[0.0, 0.05], [0.3, 0.22], [0.7, 0.8], [1.0, 0.95]]);
    masks[0].channel_curves[0] = Curve::new([[0.0, 0.0], [0.5, 0.58], [1.0, 1.0]]);
    document.set_masks(masks);
    document
}

fn with_exposure(document: &Document, tick: usize) -> Document {
    let mut document = document.clone();
    let mut basic = document.basic();
    basic.tone.exposure += 0.01 * tick as f32;
    document.set_basic(basic);
    document
}

fn with_vignette(document: &Document, tick: usize) -> Document {
    let mut document = document.clone();
    let mut basic = document.basic();
    basic.effects.vignette -= 0.5 * tick as f32;
    document.set_basic(basic);
    document
}

fn main() {
    env_logger::init();
    let n: usize = std::env::var("N").ok().and_then(|v| v.parse().ok()).unwrap_or(20);
    let edge: u32 = std::env::var("EDGE").ok().and_then(|v| v.parse().ok()).unwrap_or(1920);
    let looping: Option<f64> = std::env::var("LOOP").ok().and_then(|v| v.parse().ok());
    let icc = std::env::var("ICC").ok().and_then(|path| render::display::Display::from_icc(&std::fs::read(path).ok()?).ok().flatten());
    let paths: Vec<String> = std::env::args().skip(1).collect();

    if std::env::var_os("RECOLOUR").is_some() {
        recolour(&paths);
        return;
    }

    let mut photos = Vec::new();
    for path in &paths {
        let (proxy, full) = numa::io::raw::proxy_from_mosaic(std::path::Path::new(path), edge).expect("decodes");
        let document = Document::new(path.clone());
        let inputs = numa::io::inputs::render_inputs(&document);
        let scale = proxy.width.max(proxy.height) as f32 / full.0.max(full.1).max(1) as f32;
        photos.push((path.clone(), proxy, document, inputs, scale));
    }
    std::thread::sleep(std::time::Duration::from_millis(500));

    if let Some(seconds) = looping {

        std::thread::sleep(std::time::Duration::from_secs(3));
        let now = || std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs_f64();
        let (a, c0, started) = (now(), cpu(), Instant::now());
        let mut steps = 0;

        if std::env::var_os("CURVED").is_some() {
            let drafts: Vec<_> = photos.iter().map(|(path, proxy, document, inputs, scale)| {
                let working = render::to_working_space(document, proxy, inputs);
                let draft = std::sync::Arc::new(working.downscaled(working.width.max(working.height) / 2).expect("halves"));
                (render::with_masks_resolved(&curved(path), &working), *scale * draft.width as f32 / working.width as f32, draft)
            }).collect();
            while started.elapsed().as_secs_f64() < seconds {
                for (curves, scale, draft) in &drafts {
                    std::hint::black_box(render::apply_stack_kept(&with_exposure(curves, steps + 1), draft, *scale));
                    steps += 1;
                }
            }
        }
        while started.elapsed().as_secs_f64() < seconds {
            for (_, proxy, document, inputs, scale) in &photos {
                let working = std::sync::Arc::new(render::to_working_space(document, proxy, inputs));

                let rendered = render::apply_stack_kept(document, &working, *scale * (1.0 + 1e-6 * (steps + 1) as f32));
                std::hint::black_box(render::histogram::of(&rendered));
                steps += 1;
            }
        }
        let b = now();
        println!("MODE cpu-step start {a:.3} end {b:.3} wall {:.3} s cpu {:.3} s jumps {steps}", b - a, cpu() - c0);
        return;
    }

    if std::env::var_os("HASH").is_some() {
        for (path, proxy, document, inputs, scale) in &photos {
            let name = std::path::Path::new(path).file_name().unwrap().to_string_lossy().to_string();
            let working = std::sync::Arc::new(render::to_working_space(document, proxy, inputs));
            let half = working.width.max(working.height) / 2;
            let draft = std::sync::Arc::new(working.downscaled(half).expect("halves"));
            let draft_scale = *scale * draft.width as f32 / working.width as f32;
            let heavy = render::with_masks_resolved(&edited(path), &working);
            let mut warm = document.clone();
            warm.white_balance = Some(numa::core::color::WhiteBalance { temperature: 7200.0, tint: 12.0 });
            let warm_working = std::sync::Arc::new(render::to_working_space(&warm, proxy, inputs));

            let kept = render::apply_stack_kept;
            let frames = [
                ("plain", kept(document, &working, *scale)),
                ("plain-draft", kept(document, &draft, draft_scale)),
                ("edited", kept(&heavy, &working, *scale)),
                ("edited-draft", kept(&heavy, &draft, draft_scale)),
                ("warm", kept(&warm, &warm_working, *scale)),

                ("edited-late", kept(&with_vignette(&heavy, 7), &working, *scale)),
                ("edited-late2", kept(&with_vignette(&heavy, 9), &working, *scale)),
                ("edited-exposure", kept(&with_exposure(&with_vignette(&heavy, 9), 5), &working, *scale)),
                ("edited-again", kept(&heavy, &working, *scale)),
                ("curved-draft", kept(&render::with_masks_resolved(&curved(path), &working), &draft, draft_scale)),
            ];

            let bits = |frame: &numa::core::image::LinearImage| hash(&frame.data.iter().flat_map(|value| value.to_bits().to_le_bytes()).collect::<Vec<u8>>());
            println!("HASH {name} working-floats {:016x} warm {:016x}", bits(&working), bits(&warm_working));
            for (label, frame) in &frames {
                let histogram = render::histogram::of(frame);
                println!("HASH {name} {label} {}x{} {:016x} histogram {:016x}", frame.width(), frame.height(), hash(frame.as_raw()), hash(format!("{histogram:?}").as_bytes()));
                if let Some(display) = &icc {
                    let mut shown = frame.clone();
                    display.apply(&mut shown);
                    println!("HASH {name} {label}-icc {:016x}", hash(shown.as_raw()));
                    println!("HASH {name} {label}-bgra-icc {:016x}", hash(&render::display::to_bgra(frame, Some(display))));
                }
            }
        }
        return;
    }

    println!("{:14} {:>10} {:>34}", "photo", "size", "stage: ms per call / cores");
    for (path, proxy, document, inputs, scale) in &photos {
        let name = std::path::Path::new(path).file_name().unwrap().to_string_lossy().to_string();
        let mut rows: Vec<(&str, (f64, f64))> = Vec::new();

        rows.push(("colour", stage(n, |_| render::to_working_space(document, proxy, inputs))));
        let working = std::sync::Arc::new(render::to_working_space(document, proxy, inputs));
        let kept = render::apply_stack_kept;

        rows.push(("first render", stage(n, |i| kept(document, &working, *scale * (1.0 + 1e-5 * (i + 1) as f32)))));
        let rendered = kept(document, &working, *scale);
        rows.push(("histogram", stage(n, |_| render::histogram::of(&rendered))));
        rows.push(("scope", stage(n, |_| render::scope::of(&rendered, render::scope::Kind::Histogram))));
        if let Some(display) = &icc {
            rows.push(("display icc", stage(n, |_| {
                let mut frame = rendered.clone();
                display.apply(&mut frame);
                frame
            })));
            rows.push(("rgb clone", stage(n, |_| rendered.clone())));

            rows.push(("bgra icc", stage(n, |_| render::display::to_bgra(&rendered, Some(display)))));
            rows.push(("bgra", stage(n, |_| render::display::to_bgra(&rendered, None))));
        }

        rows.push(("tick sharp", stage(n, |i| kept(&with_exposure(document, i), &working, *scale))));
        let half = working.width.max(working.height) / 2;
        let draft = std::sync::Arc::new(working.downscaled(half).expect("halves"));
        let draft_scale = *scale * draft.width as f32 / working.width as f32;
        rows.push(("tick draft", stage(n, |i| kept(&with_exposure(document, i), &draft, draft_scale))));

        let heavy = render::with_masks_resolved(&edited(path), &working);
        rows.push(("edited sharp", stage(n, |i| kept(&with_exposure(&heavy, i), &working, *scale))));
        rows.push(("edited draft", stage(n, |i| kept(&with_exposure(&heavy, i), &draft, draft_scale))));
        let curves = render::with_masks_resolved(&curved(path), &working);
        rows.push(("curved draft", stage(n, |i| kept(&with_exposure(&curves, i), &draft, draft_scale))));

        rows.push(("late sharp", stage(n, |i| kept(&with_vignette(&heavy, i), &working, *scale))));

        let cells: Vec<String> = rows.iter().map(|(label, (ms, cores))| format!("{label} {ms:.1}/{cores:.1}")).collect();
        println!("{name:14} {:>4}x{:<5} {}", working.width, working.height, cells.join(" | "));
    }
}

fn recolour(paths: &[String]) {
    for path in paths {
        let name = std::path::Path::new(path).file_name().unwrap().to_string_lossy().to_string();
        let regions = numa::io::raw::region::Regions::open(std::path::Path::new(path)).expect("opens");
        let (w, h) = regions.size();
        let camera = regions.region([w / 2 - 700, h / 2 - 450, 1400, 900]);
        for temperature in [None, Some(7200.0)] {
            let mut document = Document::new(path.clone());
            document.white_balance = temperature.map(|temperature| numa::core::color::WhiteBalance { temperature, tint: 12.0 });
            let inputs = numa::io::inputs::render_inputs(&document);
            let settled = render::to_working_space(&document, &camera, &inputs).downscaled(700).expect("reduces");
            let moving = render::to_working_space(&document, camera.downscaled(700).expect("reduces"), &inputs);
            let [a, b] = [settled, moving].map(|frame| render::apply_pixels(&document, &frame, 0.5, render::WHOLE_FRAME));
            let diffs: Vec<u8> = a.as_raw().iter().zip(b.as_raw()).map(|(x, y)| x.abs_diff(*y)).collect();
            let over = |limit: u8| diffs.iter().filter(|d| **d > limit).count() as f64 / diffs.len() as f64 * 100.0;
            let mean = diffs.iter().map(|d| *d as f64).sum::<f64>() / diffs.len() as f64;
            println!("RECOLOUR {name} {:>6} max {} mean {mean:.3} over 1: {:.2} % over 3: {:.3} %", temperature.map_or("shot".to_string(), |t| format!("{t}K")), diffs.iter().max().unwrap(), over(1), over(3));
        }

        let (proxy, full) = numa::io::raw::proxy_from_mosaic(std::path::Path::new(path), 1920).expect("decodes");
        let document = Document::new(path.clone());
        let inputs = numa::io::inputs::render_inputs(&document);
        let long = proxy.width.max(proxy.height);
        let scale = long as f32 / full.0.max(full.1) as f32;
        let shares = [1, 2, 4].map(|by| {
            let small = proxy.downscaled(long / by).unwrap_or_else(|| proxy.clone());
            let working = render::to_working_space(&document, small, &inputs);
            let histogram = render::histogram::of(&render::apply_stack(&document, &working, scale / by as f32));
            histogram.channels.map(|bins| bins.map(|count| count as f64 / histogram.total.max(1) as f64))
        });
        let moved = |a: usize, b: usize| (0..3).map(|channel| (0..256).map(|bin| (shares[a][channel][bin] - shares[b][channel][bin]).abs()).sum::<f64>() / 2.0).fold(0.0, f64::max) * 100.0;
        println!("HISTOGRAM {name} of the counts in another bin (worst channel): half against the proxy {:.2} %, a quarter against half {:.2} %", moved(1, 0), moved(2, 1));
    }
}
