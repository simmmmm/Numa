use numa_core::document::{Basic, Document, Perspective};
use numa_core::mask::{Mask, Shape};
use numa_core::mask::Alpha;
use numa_core::image::LinearImage;
use numa_core::plane::{self, Plane};
use numa_core::tone;

use super::ToneCurve;

const MOST_EXPOSURE: f32 = 0.75;

const BLOWN: f32 = 0.988;
const DIM: f32 = 0.74;

const BLACK_POINT: f32 = 0.035;
const WHITE_POINT: f32 = 0.94;

const CLOSE_ENOUGH: f32 = 0.012;

const WORTH_MOVING: f32 = 0.006;

const SUBJECT_TARGET: f32 = 0.55;
const SUBJECT_LOW: f32 = 0.40;

const MOST_SUBJECT_EXPOSURE: f32 = 2.0;
const LEAST_SUBJECT_EXPOSURE: f32 = 0.33;

const MOST_HDR: f32 = 40.0;
const MOST_VIBRANCE: f32 = 22.0;

const COLOUR_TARGET: f32 = 0.30;

const LOW: f32 = 0.005;
const HIGH: f32 = 0.995;

pub struct Auto {
    pub basic: Basic,

    pub subject: Option<Basic>,
}

pub const SUBJECT_MASK: &str = "Subject";

impl Auto {

    pub fn apply(&self, document: &mut Document) -> Option<usize> {
        let mut basic = document.basic();
        basic.tone.exposure = self.basic.tone.exposure;
        basic.tone.whites = self.basic.tone.whites;
        basic.tone.blacks = self.basic.tone.blacks;
        basic.tone.highlights = self.basic.tone.highlights;
        basic.presence.hdr = self.basic.presence.hdr;
        basic.presence.vibrance = self.basic.presence.vibrance;
        document.set_basic(basic);

        let lift = self.subject?;
        let mut masks = document.masks();
        masks.retain(|mask| mask.name.as_deref() != Some(SUBJECT_MASK));
        let mut mask = Mask::new(Shape::Segment { classes: super::segment::MATTEABLE.to_vec() });
        mask.set_matte(true);
        mask.basic = lift;
        mask.name = Some(SUBJECT_MASK.to_string());
        masks.push(mask);
        document.set_masks(masks);
        Some(document.masks().len() - 1)
    }
}

pub fn subject_mask(
    found: &crate::segment::Segmentation,
    frame: &image::RgbImage,
    width: usize,
    height: usize,
) -> Option<Mask> {
    let classes = crate::segment::MATTEABLE.to_vec();
    if !crate::segment::named_something(&found.alpha(&classes)) {
        return None;
    }

    let mut mask = Mask::new(Shape::Segment { classes });
    mask.matte = true;
    crate::resolve_mask(&mut mask, Some(found), None, Some(frame), width, height);
    Some(mask)
}

pub fn tone(image: &LinearImage, subject: Option<&Alpha>) -> Auto {
    let mut basic = Basic::default();
    let Some(sorted) = luminances(image, None) else {
        return Auto { basic, subject: None };
    };

    let inside = subject.and_then(|alpha| luminances(image, Some(alpha)));

    let at = |fraction: f32| {
        let index = ((sorted.len() - 1) as f32 * fraction).round() as usize;
        sorted[index.min(sorted.len() - 1)].max(1e-6)
    };

    let brightest = tone::curve(at(HIGH));
    let wanted = match brightest {
        above if above > BLOWN => Some(tone::scene_value_for(WHITE_POINT)),
        below if below < DIM => Some(tone::scene_value_for(WHITE_POINT)),
        _ => None,
    };
    basic.tone.exposure = wanted
        .map(|target| (target / at(HIGH)).log2().clamp(-MOST_EXPOSURE, MOST_EXPOSURE))
        .unwrap_or(0.0);
    let gain = 2.0f32.powf(basic.tone.exposure);

    basic.tone.whites = solve(|amount| display_at(at(HIGH) * gain, 0.0, amount), WHITE_POINT);
    basic.tone.blacks = solve(|amount| display_at(at(LOW) * gain, amount, 0.0), BLACK_POINT);

    let highest = display_at(at(HIGH) * gain, basic.tone.blacks, basic.tone.whites);
    if highest > WHITE_POINT + 0.005 {
        basic.tone.highlights = solve(
            |amount| {
                let lit = at(HIGH) * gain;
                let asked = Basic::with(|b| {
                    b.tone.highlights = amount;
                    b.tone.whites = basic.tone.whites;
                });
                tone::curve(lit * ToneCurve::new(&asked).gain(lit))
            },
            WHITE_POINT,
        )
        .min(0.0);
    }

    let subject = inside.and_then(|values| {
        let middle = values[values.len() / 2].max(1e-6) * gain;
        let short = (tone::scene_value_for(SUBJECT_TARGET) / middle).log2();
        let worth_it = tone::curve(middle) < SUBJECT_LOW && short >= LEAST_SUBJECT_EXPOSURE;
        worth_it.then(|| Basic::with(|b| b.tone.exposure = short.min(MOST_SUBJECT_EXPOSURE)))
    });

    if let Some(lift) = &subject {
        basic.presence.hdr = (lift.tone.exposure / MOST_SUBJECT_EXPOSURE * MOST_HDR).clamp(0.0, MOST_HDR).round();
    }

    let colour = colourfulness(image);
    basic.presence.vibrance = (((COLOUR_TARGET - colour) / COLOUR_TARGET) * 100.0)
        .clamp(0.0, MOST_VIBRANCE)
        .round();

    Auto { basic, subject }
}

fn colourfulness(image: &LinearImage) -> f32 {
    let pixels = image.pixel_count();
    if pixels == 0 {
        return COLOUR_TARGET;
    }
    let step = (pixels / 50_000).max(1);
    let mut values: Vec<f32> = image
        .data
        .chunks_exact(3)
        .step_by(step)
        .filter_map(|pixel| {
            let high = pixel[0].max(pixel[1]).max(pixel[2]);
            let low = pixel[0].min(pixel[1]).min(pixel[2]);

            (high > 0.004).then(|| ((high - low) / high).clamp(0.0, 1.0))
        })
        .collect();
    if values.len() < 64 {
        return COLOUR_TARGET;
    }
    values.sort_by(f32::total_cmp);
    values[values.len() / 2]
}

fn display_at(lit: f32, blacks: f32, whites: f32) -> f32 {
    let asked = Basic::with(|b| {
        b.tone.blacks = blacks;
        b.tone.whites = whites;
    });
    tone::curve(lit * ToneCurve::new(&asked).gain(lit))
}

fn solve(measure: impl Fn(f32) -> f32, wanted: f32) -> f32 {
    let resting = measure(0.0);
    if (resting - wanted).abs() < CLOSE_ENOUGH {
        return 0.0;
    }

    let (mut low, mut high) = (-100.0f32, 100.0f32);
    let amount = if measure(low) > wanted {
        low
    } else if measure(high) < wanted {
        high
    } else {
        for _ in 0..16 {
            let middle = (low + high) / 2.0;
            if measure(middle) < wanted {
                low = middle;
            } else {
                high = middle;
            }
        }
        (low + high) / 2.0
    };

    if (measure(amount) - resting).abs() < WORTH_MOVING || amount.abs() < 2.0 {
        return 0.0;
    }
    amount
}

fn luminances(image: &LinearImage, only: Option<&Alpha>) -> Option<Vec<f32>> {
    let pixels = image.pixel_count();
    if pixels == 0 {
        return None;
    }

    let step = (pixels / 100_000).max(1);
    let (width, height) = (image.width as usize, image.height as usize);

    let mut values: Vec<f32> = image
        .data
        .chunks_exact(3)
        .enumerate()
        .step_by(step)
        .filter(|(index, _)| match only {

            Some(alpha) => {
                let (x, y) = (index % width, index / width);
                let ax = (x * alpha.width / width.max(1)).min(alpha.width.saturating_sub(1));
                let ay = (y * alpha.height / height.max(1)).min(alpha.height.saturating_sub(1));
                alpha.data.get(ay * alpha.width + ax).copied().unwrap_or(0.0) > 0.6
            }
            None => true,
        })
        .map(|(_, pixel)| 0.2126 * pixel[0] + 0.7152 * pixel[1] + 0.0722 * pixel[2])
        .filter(|value| value.is_finite())
        .collect();

    if values.len() < 64 {
        return None;
    }
    values.sort_by(f32::total_cmp);
    Some(values)
}

const EDGE_FLOOR: f32 = 0.12;

const LEAN: f32 = 0.6;

const ENOUGH: usize = 400;

const WORTH_IT: f32 = 2.0;

pub fn perspective(luma: &Plane) -> Option<Perspective> {
    let (edges, (width, height)) = coherent_edges(luma)?;
    let centre_x = width as f32 / 2.0;

    let upright: Vec<(f32, f32, f32)> = edges
        .into_iter()

        .filter(|&(_, _, gx, gy, _)| gy.abs() < gx.abs() * LEAN)
        .map(|(x, _, gx, gy, strength)| (x - centre_x, -gy / gx, strength))
        .collect();

    let slope = robust_slope(&upright, width as f32)?;
    let vertical = worth_it((slope * (height as f32 / 2.0) * 200.0).clamp(-60.0, 60.0));
    Some(Perspective { vertical, ..Perspective::default() })
}

fn robust_slope(votes: &[(f32, f32, f32)], across: f32) -> Option<f32> {
    let total: f32 = votes.iter().map(|vote| vote.2).sum();
    let mut fit = Fit::default();
    for &(at, lean, weight) in votes {
        fit.add(at, lean, weight);
    }
    let (mut intercept, mut slope) = fit.solve(ENOUGH)?;
    let mut kept = total;
    for band in [0.15, 0.08] {
        let mut fit = Fit::default();
        for &(at, lean, weight) in votes {
            if (lean - (intercept + slope * at)).abs() <= band {
                fit.add(at, lean, weight);
            }
        }
        (intercept, slope) = fit.solve(ENOUGH)?;
        kept = fit.weight as f32;

        let mean = fit.x / fit.weight;
        let spread = (fit.xx / fit.weight - mean * mean).max(0.0).sqrt() as f32;
        if spread < across * SPREAD {
            return None;
        }
    }
    (kept >= total * SHARE).then_some(slope)
}

const SHARE: f32 = 0.3;

const SPREAD: f32 = 0.15;

pub fn level(luma: &Plane) -> Option<f32> {
    let (edges, _) = coherent_edges(luma)?;
    let votes: Vec<(f32, f32)> = edges
        .into_iter()
        .filter_map(|(_, _, gx, gy, strength)| {

            let turned = if gy.abs() < gx.abs() * LEVEL_LEAN {
                (gy / gx).atan()
            } else if gx.abs() < gy.abs() * LEVEL_LEAN {
                -(gx / gy).atan()
            } else {
                return None;
            };
            Some((turned.to_degrees(), strength))
        })
        .collect();
    crowded(&votes)
}

fn crowded(votes: &[(f32, f32)]) -> Option<f32> {
    if votes.len() < ENOUGH {
        return None;
    }

    const BINS: usize = 301;
    let bin = |angle: f32| ((angle + 15.0) * 10.0).round() as usize;
    let mut histogram = [0.0f32; BINS];
    for &(angle, weight) in votes.iter().filter(|vote| vote.0.abs() <= 15.0) {
        histogram[bin(angle)] += weight;
    }
    let smooth: Vec<f32> = (0..BINS)
        .map(|at| {
            (-10i32..=10)
                .filter_map(|offset| histogram.get((at as i32 + offset) as usize).map(|v| v * (11 - offset.abs()) as f32))
                .sum()
        })
        .collect();
    let peak = (0..BINS).max_by(|a, b| smooth[*a].total_cmp(&smooth[*b])).unwrap_or(0);
    let mut sorted = smooth.clone();
    sorted.sort_by(f32::total_cmp);
    let typical = sorted[BINS / 2].max(f32::EPSILON);

    let mut angle = peak as f32 / 10.0 - 15.0;
    for _ in 0..4 {
        let (sum, weight) = votes
            .iter()
            .filter(|vote| (vote.0 - angle).abs() <= 1.5)
            .fold((0.0, 0.0), |(sum, total), vote| (sum + vote.0 * vote.1, total + vote.1));
        if weight > 0.0 {
            angle = sum / weight;
        }
    }
    if smooth[peak] < AGREE * typical {
        return None;
    }
    Some(if angle.abs() >= 0.05 { angle } else { 0.0 })
}

const AGREE: f32 = 2.5;

const LEVEL_LEAN: f32 = 0.27;

fn coherent_edges(luma: &Plane) -> Option<(Vec<(f32, f32, f32, f32, f32)>, (usize, usize))> {
    let (edges, (width, height)) = strong_edges(luma)?;
    let mut products = [vec![0.0f32; width * height], vec![0.0f32; width * height], vec![0.0f32; width * height]];
    for &(x, y, gx, gy, _) in &edges {
        let at = y as usize * width + x as usize;
        products[0][at] = gx * gx;
        products[1][at] = gy * gy;
        products[2][at] = gx * gy;
    }
    let reach = (width.max(height) / 300).max(2);
    let [xx, yy, xy] = products.map(|data| plane::blur(&Plane::new(width, height, data), reach));
    let coherent = edges
        .into_iter()
        .filter_map(|(x, y, gx, gy, strength)| {
            let at = y as usize * width + x as usize;
            let (a, b, c) = (xx.data[at], yy.data[at], xy.data[at]);
            let coherence = ((a - b) * (a - b) + 4.0 * c * c).sqrt() / (a + b).max(f32::EPSILON);
            (coherence >= 0.7).then(|| (x, y, gx, gy, strength * coherence.powi(4)))
        })
        .collect();
    Some((coherent, (width, height)))
}

fn strong_edges(luma: &Plane) -> Option<(Vec<(f32, f32, f32, f32, f32)>, (usize, usize))> {
    let (width, height) = (luma.width, luma.height);
    if width < 32 || height < 32 {
        return None;
    }

    let smooth = plane::blur(luma, (width.max(height) / 400).max(1));

    let mut strongest = 0.0f32;
    let mut edges: Vec<(f32, f32, f32, f32, f32)> = Vec::new();
    for y in 1..height - 1 {
        for x in 1..width - 1 {
            let at = |dx: isize, dy: isize| {
                smooth.data[(y as isize + dy) as usize * width + (x as isize + dx) as usize]
            };

            let gx = (at(1, -1) + 2.0 * at(1, 0) + at(1, 1))
                - (at(-1, -1) + 2.0 * at(-1, 0) + at(-1, 1));
            let gy = (at(-1, 1) + 2.0 * at(0, 1) + at(1, 1))
                - (at(-1, -1) + 2.0 * at(0, -1) + at(1, -1));
            let strength = gx.hypot(gy);
            if strength <= 0.0 {
                continue;
            }
            strongest = strongest.max(strength);
            edges.push((x as f32, y as f32, gx, gy, strength));
        }
    }
    let floor = strongest * EDGE_FLOOR;
    edges.retain(|edge| edge.4 >= floor);
    Some((edges, (width, height)))
}

fn worth_it(amount: f32) -> f32 {
    if amount.abs() >= WORTH_IT {
        amount
    } else {
        0.0
    }
}

#[derive(Default)]
struct Fit {
    weight: f64,
    x: f64,
    y: f64,
    xx: f64,
    xy: f64,
    count: usize,
}

impl Fit {
    fn add(&mut self, x: f32, y: f32, weight: f32) {
        let (x, y, w) = (x as f64, y as f64, weight as f64);
        self.weight += w;
        self.x += w * x;
        self.y += w * y;
        self.xx += w * x * x;
        self.xy += w * x * y;
        self.count += 1;
    }

    fn solve(&self, enough: usize) -> Option<(f32, f32)> {
        if self.count < enough || self.weight <= 0.0 {
            return None;
        }
        let mean_x = self.x / self.weight;
        let mean_y = self.y / self.weight;
        let variance = self.xx / self.weight - mean_x * mean_x;
        if variance < 1e-6 {
            return None;
        }
        let covariance = self.xy / self.weight - mean_x * mean_y;
        let slope = covariance / variance;
        Some(((mean_y - slope * mean_x) as f32, slope as f32))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
        let working = crate::to_working_space(&document, &linear, &Default::default());

        let before = crate::apply_stack(&document, &working, 1.0);
        let (mut after, auto, _) = run(&mut document, &working, &before);
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
        let paths = match std::env::var("FRAME") {
            Ok(one) => vec![std::path::PathBuf::from(one)],
            Err(_) => {
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
            let working = crate::to_working_space(&document, &linear, &Default::default());
            let before = crate::apply_stack(&document, &working, 1.0);
            let (after, auto, subject) = run(&mut document, &working, &before);

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

    fn run(
        document: &mut Document,
        working: &LinearImage,
        before: &image::RgbImage,
    ) -> (image::RgbImage, Auto, Option<Alpha>) {
        let (rw, rh) = crate::raster_size(before.width(), before.height(), crate::MASK_RASTER);
        let found = crate::segment::of(before);
        let resolve = |mask: &mut Mask| {
            crate::resolve_mask(mask, found.as_ref(), None, Some(before), rw, rh)
        };

        let mut probe = Mask::new(Shape::Segment { classes: crate::segment::MATTEABLE.to_vec() });
        probe.matte = true;
        resolve(&mut probe);
        let subject = probe.map.0.as_deref().map(|kept| kept.to_alpha());

        let auto = tone(working, subject.as_ref());
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
            match &auto.subject {
                Some(lift) => format!("{:+.2} EV in een masker", lift.tone.exposure),
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
    fn converging_verticals_are_found_and_upright_ones_are_not_touched() {
        let (w, h) = (256usize, 256usize);

        let draw = |keystone: f32| {
            let mut data = vec![0.2f32; w * h];
            for line in [-0.3, -0.1, 0.1, 0.3] {
                let base = w as f32 / 2.0 + line * w as f32;
                for y in 0..h {
                    let dy = y as f32 - h as f32 / 2.0;
                    let x = base + (base - w as f32 / 2.0) * keystone * dy / (h as f32 / 2.0);
                    let x = x.round() as isize;
                    for width in -1..=1 {
                        let at = x + width;
                        if (0..w as isize).contains(&at) {
                            data[y * w + at as usize] = 0.9;
                        }
                    }
                }
            }
            Plane::new(w, h, data)
        };

        let straight = perspective(&draw(0.0)).unwrap_or_default();
        assert!(
            straight.vertical.abs() < 6.0,
            "upright lines need no correction: {}",
            straight.vertical
        );

        let keyed = perspective(&draw(0.35)).unwrap_or_default();
        assert!(keyed.vertical.abs() > 12.0, "a keystone is found: {}", keyed.vertical);

        let other = perspective(&draw(-0.35)).unwrap_or_default();
        assert!(
            other.vertical * keyed.vertical < 0.0,
            "opposite keystones, opposite corrections: {} and {}",
            keyed.vertical,
            other.vertical
        );
    }

    #[test]
    fn a_frame_with_no_lines_is_left_alone() {
        let (w, h) = (128usize, 128usize);
        let data = (0..w * h)
            .map(|index| ((index * 2654435761usize) % 997) as f32 / 997.0)
            .collect();
        let plane = Plane::new(w, h, data);
        assert_eq!(perspective(&plane).unwrap_or_default(), Perspective::default());

        let mut seed = 0x9e3779b9u32;
        let noise = (0..w * h)
            .map(|_| {
                seed ^= seed << 13;
                seed ^= seed >> 17;
                seed ^= seed << 5;
                (seed % 1000) as f32 / 1000.0
            })
            .collect();
        assert_eq!(level(&Plane::new(w, h, noise)), None);
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

        let unreachable = solve(|amount| display_at(0.0004, amount, 0.0), 0.5);
        assert_eq!(unreachable, 0.0, "a target it cannot reach is not an answer");

        let resting = display_at(0.9, 0.0, 0.0);
        assert_eq!(solve(|amount| display_at(0.9, 0.0, amount), resting), 0.0);

        let wanted = display_at(0.9, 0.0, 40.0);
        let found = solve(|amount| display_at(0.9, 0.0, amount), wanted);
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

        let high_key = tone(&flat(0.55, tone::scene_value_for(0.93)), None).basic;
        assert_eq!(high_key.tone.exposure, 0.0, "a bright scene is not a mistake");

        let blown = tone(&flat(0.4, tone::scene_value_for(0.9995)), None).basic;
        assert!(blown.tone.exposure < -0.05, "a blown frame comes down: {}", blown.tone.exposure);

        let dark = tone(&flat(0.01, 0.03), None).basic;
        assert!(dark.tone.exposure > 0.05, "a dark frame goes up: {}", dark.tone.exposure);

        for basic in [blown, dark] {
            assert!(basic.tone.exposure.abs() <= MOST_EXPOSURE + 1e-4);
        }
    }

    #[test]
    fn a_subject_in_shadow_gets_a_mask_and_the_frame_keeps_its_exposure() {

        let (w, h) = (64u32, 64u32);
        let bright = tone::scene_value_for(0.9);
        let data = (0..w * h)
            .flat_map(|index| {
                let (x, y) = (index % w, index / w);
                let dark = (16..48).contains(&x) && (16..48).contains(&y);
                let value = if dark { bright / 32.0 } else { bright };
                [value, value, value]
            })
            .collect();
        let image = LinearImage::new(w, h, data);

        let alpha = Alpha::new(
            w as usize,
            h as usize,
            (0..w * h)
                .map(|index| {
                    let (x, y) = (index % w, index / w);
                    match (16..48).contains(&x) && (16..48).contains(&y) {
                        true => 1.0,
                        false => 0.0,
                    }
                })
                .collect(),
        );

        let auto = tone(&image, Some(&alpha));
        let lift = auto.subject.expect("a subject five stops down is one to lift");
        assert!(lift.tone.exposure > 1.0, "and lifted by a real amount: {}", lift.tone.exposure);
        assert!(lift.tone.exposure <= MOST_SUBJECT_EXPOSURE + 1e-4, "but not past its limit");
        assert!(auto.basic.presence.hdr > 0.0, "with a hand under everything else down there");
        assert_eq!(auto.basic.tone.shadows, 0.0, "and never the slider that cannot reach");

        assert_eq!(auto.basic.tone.exposure, 0.0, "the frame keeps its exposure");

        let flat = LinearImage::new(w, h, vec![tone::scene_value_for(0.55); (w * h * 3) as usize]);
        let everything = Alpha::new(w as usize, h as usize, vec![1.0; (w * h) as usize]);
        let lit = tone(&flat, Some(&everything));
        assert!(lit.subject.is_none(), "a lit subject needs no mask");
        assert_eq!(lit.basic.presence.hdr, 0.0, "and no local tone mapping either");
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

        let flat = tone(&of(0.20, 0.21, 0.22), None).basic;
        assert!(flat.presence.vibrance > 0.0, "a flat frame gets some: {}", flat.presence.vibrance);
        assert!(flat.presence.vibrance <= MOST_VIBRANCE, "and never more than a fifth of the slider");

        let vivid = tone(&of(0.40, 0.10, 0.05), None).basic;
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
        let basic = tone(&LinearImage::new(w, h, data), None).basic;

        assert_eq!(basic.tone.contrast, 0.0);
        assert_eq!(basic.presence.saturation, 0.0);
        assert_eq!(basic.tone.shadows, 0.0);
        assert_eq!(basic.presence.clarity, 0.0);
        assert_eq!(basic.presence.texture, 0.0);
    }
}
