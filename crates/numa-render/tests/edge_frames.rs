use std::panic::{catch_unwind, AssertUnwindSafe};

use numa_core::document::{Basic, Document, Perspective};
use numa_core::image::LinearImage;
use numa_core::mask::{Mask, Shape};

const SIZES: [(u32, u32); 8] = [(1, 1), (1, 9), (9, 1), (2, 2), (3, 5), (17, 11), (64, 3), (2, 300)];

fn frame(w: u32, h: u32) -> LinearImage {
    let data = (0..(w * h * 3) as usize).map(|i| ((i * 7919) % 997) as f32 / 997.0 * 0.9).collect();
    LinearImage::new(w, h, data)
}

fn basic_from(fields: &serde_json::Map<String, serde_json::Value>) -> Basic {
    serde_json::from_value(serde_json::Value::Object(fields.clone())).unwrap()
}

fn extremes(key: &str) -> [f64; 2] {
    match key {
        "exposure" => [-5.0, 5.0],
        "sharpen_radius" => [0.5, 3.0],
        _ => [-100.0, 100.0],
    }
}

fn run(label: &str, document: &Document, failures: &mut Vec<String>) {
    for (w, h) in SIZES {
        let source = frame(w, h);
        let out = catch_unwind(AssertUnwindSafe(|| numa_render::apply_stack(document, &source, 1.0)));
        match out {
            Err(cause) => {
                let why = cause.downcast_ref::<String>().cloned()
                    .or_else(|| cause.downcast_ref::<&str>().map(|s| s.to_string()))
                    .unwrap_or_default();
                failures.push(format!("{label} @ {w}x{h}: panicked: {why}"));
            }
            Ok(image) if image.width() == 0 || image.height() == 0 => {
                failures.push(format!("{label} @ {w}x{h}: empty {}x{}", image.width(), image.height()));
            }
            Ok(_) => {}
        }
    }
}

#[test]
fn sliders_at_their_ends_on_awkward_frames() {
    std::panic::set_hook(Box::new(|_| {}));
    let mut failures = Vec::new();
    let default = serde_json::to_value(Basic::default()).unwrap();
    let default = default.as_object().unwrap().clone();

    for key in default.keys() {
        for value in extremes(key) {
            let mut fields = default.clone();
            fields.insert(key.clone(), value.into());
            let mut document = Document::new("a.RAF".into());
            document.set_basic(basic_from(&fields));
            run(&format!("{key}={value}"), &document, &mut failures);
        }
    }
    for end in 0..2 {
        let mut fields = default.clone();
        for key in default.keys() {
            fields.insert(key.clone(), extremes(key)[end].into());
        }
        let mut document = Document::new("a.RAF".into());
        document.set_basic(basic_from(&fields));
        run(&format!("all at end {end}"), &document, &mut failures);
    }

    let geometry: [(&str, Box<dyn Fn(&mut Document)>); 6] = [
        ("tiny crop", Box::new(|d| d.set_crop([0.4, 0.4, 0.01, 0.01], 0.0))),
        ("crop at 45°", Box::new(|d| d.set_crop([0.0, 0.0, 1.0, 1.0], 45.0))),
        ("quarter turn", Box::new(|d| d.set_rotation(90.0))),
        ("mirrored", Box::new(|d| d.set_mirrored(true))),
        ("perspective", Box::new(|d| d.set_perspective(Perspective { vertical: 100.0, horizontal: -100.0, aspect: 100.0 }))),
        ("crop at edge", Box::new(|d| d.set_crop([0.99, 0.99, 0.01, 0.01], 0.0))),
    ];
    for (label, apply) in geometry {
        let mut document = Document::new("a.RAF".into());
        apply(&mut document);
        run(label, &document, &mut failures);
    }

    let masks = [
        ("linear from == to", Shape::Linear { from: [0.5, 0.5], to: [0.5, 0.5] }),
        ("radial of no size", Shape::Radial { centre: [0.5, 0.5], radius: [0.0, 0.0], feather: 0.0 }),
        ("radial off the frame", Shape::Radial { centre: [5.0, -5.0], radius: [0.1, 0.1], feather: 1.0 }),
    ];
    for (label, shape) in masks {
        let mut mask = Mask::new(shape);
        mask.basic.tone.exposure = 1.0;
        mask.basic.presence.clarity = 50.0;
        let mut document = Document::new("a.RAF".into());
        document.set_masks(vec![mask]);
        run(label, &document, &mut failures);
    }

    let _ = std::panic::take_hook();
    assert!(failures.is_empty(), "{} failures:\n{}", failures.len(), failures.join("\n"));
}

#[test]
fn a_crop_is_never_more_than_twice_the_frame() {
    let source = frame(17, 11);
    let mut failures = Vec::new();
    for rect in [[0.0, 0.0, 1000.0, 1000.0], [0.0, 0.0, 50.0, 50.0], [-0.5, -0.5, 2.0, 2.0], [f32::NAN, 0.0, f32::INFINITY, 1.0]] {
        let mut document = Document::new("a.RAF".into());
        document.set_crop(rect, 0.0);
        let out = numa_render::apply_stack(&document, &source, 1.0);
        if out.width() > 2 * source.width || out.height() > 2 * source.height {
            failures.push(format!("{rect:?}: {}x{} from 17x11", out.width(), out.height()));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
