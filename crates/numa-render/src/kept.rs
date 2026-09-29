use super::*;
use std::sync::{Arc, Mutex, Weak};

struct Point {
    frame: Weak<LinearImage>,
    key: String,

    _masks: Vec<Weak<numa_core::mask::Stored>>,
    size: (usize, usize),
    white_point: Option<numa_core::color::WhiteBalance>,
    data: Arc<Vec<f32>>,
}

struct Points {
    kept: Vec<Point>,

    last: Option<(Weak<LinearImage>, String)>,
}

static EARLY: Mutex<Points> = Mutex::new(Points { kept: Vec::new(), last: None });

static LATE: Mutex<Points> = Mutex::new(Points { kept: Vec::new(), last: None });

const LARGEST: usize = 8_000_000 * 3;

static SPARE: Mutex<Option<Vec<f32>>> = Mutex::new(None);

fn buffer_from(pixels: &[f32]) -> Vec<f32> {
    let mut buffer = SPARE.lock().unwrap_or_else(std::sync::PoisonError::into_inner).take().unwrap_or_default();
    buffer.clear();
    buffer.extend_from_slice(pixels);
    buffer
}

pub(super) fn recycle(buffer: Vec<f32>) {
    if buffer.capacity() <= LARGEST {
        let mut spare = SPARE.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if spare.as_ref().is_none_or(|kept| kept.capacity() < buffer.capacity()) {
            *spare = Some(buffer);
        }
    }
}

type Found = (Vec<f32>, (usize, usize), Option<numa_core::color::WhiteBalance>);

impl Points {
    fn lock(points: &Mutex<Points>) -> std::sync::MutexGuard<'_, Points> {
        points.lock().unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    fn find(points: &Mutex<Points>, frame: &Arc<LinearImage>, key: &str) -> Option<Found> {
        let found = Self::lock(points)
            .kept
            .iter()
            .find(|point| std::ptr::eq(point.frame.as_ptr(), Arc::as_ptr(frame)) && point.key == key)
            .map(|point| (point.data.clone(), point.size, point.white_point))?;

        Some((buffer_from(&found.0), found.1, found.2))
    }

    fn asked_again(points: &Mutex<Points>, frame: &Arc<LinearImage>, key: &str) -> bool {
        let mut points = Self::lock(points);
        let again = points.last.as_ref().is_some_and(|(was, said)| std::ptr::eq(was.as_ptr(), Arc::as_ptr(frame)) && said == key);
        points.last = Some((Arc::downgrade(frame), key.to_string()));
        again
    }

    #[allow(clippy::too_many_arguments)]
    fn keep(
        points: &Mutex<Points>,
        room: usize,
        frame: &Arc<LinearImage>,
        key: String,
        masks: Vec<Weak<numa_core::mask::Stored>>,
        data: &[f32],
        size: (usize, usize),
        white_point: Option<numa_core::color::WhiteBalance>,
    ) {
        if data.len() > LARGEST {
            return;
        }
        let data = Arc::new(data.to_vec());
        let mut points = Self::lock(points);

        points.kept.retain(|point| point.frame.strong_count() > 0 && !(std::ptr::eq(point.frame.as_ptr(), Arc::as_ptr(frame)) && point.key == key));
        if points.kept.len() >= room {
            points.kept.remove(0);
        }
        points.kept.push(Point { frame: Arc::downgrade(frame), key, _masks: masks, size, white_point, data });
    }
}

pub(super) fn finished(
    document: &Document,
    source: &Arc<LinearImage>,
    detail_scale: f32,
    region: [f32; 4],
    whole: bool,
    tone: local::Tone,
) -> (Vec<f32>, u32, u32) {
    let mut passes = Passes::new();
    let basic = stack_basic(document, source.display_referred);
    let early = early_key(document, &basic, detail_scale, region, whole);

    let late = matches!(tone, local::Tone::Own).then(|| late_key(document, &early));
    let again = late.as_ref().is_some_and(|(key, _)| Points::asked_again(&LATE, source, key));
    let found = late.as_ref().and_then(|(key, _)| Points::find(&LATE, source, key));
    let (mut data, size) = match found {
        Some((data, size, _)) => {
            passes.mark("kept to the grade");
            (data, size)
        }
        None => {
            let (mut data, size, white_point) = early_stage(document, &basic, source, &early, detail_scale, region, whole, &mut passes);
            run_operations(document, &mut data, size, tone);
            passes.mark("operations");
            to_the_grade(document, &mut data, size, region, (white_point, source.display_referred), detail_scale, &mut passes);
            if let Some((key, masks)) = late.filter(|_| again) {
                Points::keep(&LATE, 2, source, key, masks, &data, size, white_point);
            }
            (data, size)
        }
    };
    from_the_grade(document, &basic, &mut data, size, region, detail_scale, &mut passes);
    passes.report(size.0, size.1);
    (data, size.0 as u32, size.1 as u32)
}

#[allow(clippy::too_many_arguments)]
fn early_stage(
    document: &Document,
    basic: &Basic,
    source: &Arc<LinearImage>,
    key: &str,
    detail_scale: f32,
    region: [f32; 4],
    whole: bool,
    passes: &mut Passes,
) -> Found {
    if let Some(found) = Points::find(&EARLY, source, key) {
        passes.mark("prefix kept");
        return found;
    }
    let framed = if whole { geometry_of(document, source) } else { None };
    let frame = framed.as_ref().unwrap_or(source);

    let (size, white_point) = ((frame.width as usize, frame.height as usize), frame.white_point);
    let mut data = match framed {
        Some(framed) => framed.data,
        None => buffer_from(&source.data),
    };
    prefix(document, basic, &mut data, size, detail_scale, region, false, passes);
    Points::keep(&EARLY, 4, source, key.to_string(), Vec::new(), &data, size, white_point);
    (data, size, white_point)
}

fn early_key(document: &Document, basic: &Basic, detail_scale: f32, region: [f32; 4], whole: bool) -> String {
    let geometry = whole.then(|| (document.rotation(), document.crop(), document.mirrored(), document.perspective(), basic.optics));
    format!(
        "{:?}",
        (
            document.retouch(),
            document.faces(),
            document.beautify(),
            basic.effects.dehaze,
            basic.detail,
            basic.calibration,
            document.working_space,
            detail_scale,
            region,
            geometry,
        )
    )
}

fn late_key(document: &Document, early: &str) -> (String, Vec<Weak<numa_core::mask::Stored>>) {
    let mut before = document.clone();
    before.lut = None;
    before.operations.retain(|operation| !matches!(operation, Operation::Grading(_) | Operation::Curve(_) | Operation::ChannelCurve { .. }));
    for operation in &mut before.operations {
        if let Operation::Basic(basic) = operation {
            let effects = &mut basic.effects;
            (effects.vignette, effects.vignette_midpoint, effects.vignette_roundness, effects.vignette_feather) = (0.0, 0.0, 0.0, 0.0);
            (effects.grain, effects.grain_size, effects.grain_roughness) = (0.0, 0.0, 0.0);
        }
    }
    let pixels: Vec<&Arc<numa_core::mask::Stored>> = before
        .operations
        .iter()
        .filter_map(|operation| match operation {
            Operation::Mask(mask) => Some([&mask.map, &mask.unshaped, &mask.cut]),
            _ => None,
        })
        .flatten()
        .filter_map(|pixels| pixels.0.as_ref())
        .collect();
    let addresses: Vec<*const numa_core::mask::Stored> = pixels.iter().map(|arc| Arc::as_ptr(arc)).collect();
    let text = serde_json::to_string(&before).unwrap_or_default();
    let key = format!("{early}|{text}|{:?}|{addresses:?}", document.masks_map);
    (key, pixels.into_iter().map(Arc::downgrade).collect())
}

pub(super) fn forget() {
    SPARE.lock().unwrap_or_else(std::sync::PoisonError::into_inner).take();
    for points in [&EARLY, &LATE] {
        let mut points = Points::lock(points);
        points.kept.clear();
        points.last = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use numa_core::mask::{Mask, Shape};

    fn frame(seed: usize) -> Arc<LinearImage> {
        let (w, h) = (96usize, 64usize);
        let data: Vec<f32> = (0..w * h * 3).map(|i| ((i * 7919 + seed * 131) % 997) as f32 / 997.0 * 0.9).collect();
        Arc::new(LinearImage::new(w as u32, h as u32, data))
    }

    fn edited(exposure: f32, vignette: f32) -> Document {
        let mut document = Document::new("a.RAF".into());
        document.set_basic(Basic::with(|b| {
            b.tone.exposure = exposure;
            b.tone.contrast = 15.0;
            b.presence.clarity = 20.0;
            b.presence.vibrance = 15.0;
            b.detail.sharpen = 60.0;
            b.effects.vignette = vignette;
            b.effects.grain = 10.0;
        }));
        let mut sky = Mask::new(Shape::Linear { from: [0.5, 0.0], to: [0.5, 0.45] });
        sky.basic.tone.exposure = -0.5;
        document.set_masks(vec![sky]);
        document
    }

    #[test]
    fn a_kept_stage_is_the_stage() {
        forget();
        let (one, two) = (frame(1), frame(2));
        let same = |document: &Document, source: &Arc<LinearImage>| {
            let kept = apply_stack_kept(document, source, 0.5);
            assert_eq!(kept.as_raw(), apply_stack(document, &**source, 0.5).as_raw());
            kept
        };
        let mut documents: Vec<Document> = (0..4).map(|tick| edited(0.3, -15.0 - tick as f32)).collect();
        documents.push(edited(0.5, -18.0));
        let mut curved = edited(0.5, -18.0);
        curved.operations.push(Operation::Curve(Curve::new([[0.0, 0.0], [0.5, 0.6], [1.0, 1.0]])));
        documents.push(curved);
        let mut cropped = edited(0.5, -18.0);
        cropped.set_crop([0.1, 0.2, 0.7, 0.6], 0.0);
        documents.push(cropped.clone());
        documents.push(cropped);
        for document in &documents {
            same(document, &one);
            same(document, &one);
            same(document, &two);
        }
        let kept = |points: &Mutex<Points>| Points::lock(points).kept.iter().filter(|point| std::ptr::eq(point.frame.as_ptr(), Arc::as_ptr(&one))).count();
        assert!(kept(&EARLY) > 0 && kept(&LATE) > 0, "both points were kept, so both were read");
        assert_ne!(same(&documents[0], &one).as_raw(), same(&documents[0], &two).as_raw(), "two frames, two pictures");

        let region = [0.25, 0.25, 0.5, 0.5];
        for _ in 0..3 {
            let kept = apply_pixels_kept(&documents[1], &one, 0.5, region, None);
            assert_eq!(kept.as_raw(), apply_pixels(&documents[1], &*one, 0.5, region).as_raw());
        }

        let painted = |value: f32| {
            let mut mask = Mask::new(Shape::Painted);
            mask.basic.tone.exposure = 1.0;
            mask.map = numa_core::mask::Pixels::of(&numa_core::mask::Alpha::new(8, 8, (0..64).map(|i| (i % 8) as f32 / 7.0 * value).collect()));
            let mut document = edited(0.3, -15.0);
            document.set_masks(vec![mask]);
            document
        };
        let (a, b) = (painted(1.0), painted(0.2));
        for _ in 0..3 {
            same(&a, &one);
        }
        for _ in 0..3 {
            same(&b, &one);
        }
    }
}
