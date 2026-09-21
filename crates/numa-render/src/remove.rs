use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use numa_core::retouch::Spot;
use numa_core::space::ColourSpace;
use numa_infer::Model;

pub const MODEL: &str = "lama_fp32.onnx";

const SIDE: usize = 512;

pub(crate) const WINDOW_RADII: f32 = 5.0;

pub fn is_installed() -> bool {
    numa_core::paths::model_file(&[MODEL]).is_some()
}

fn model() -> Option<&'static Model> {
    static MODEL_ONCE: OnceLock<Option<Model>> = OnceLock::new();
    MODEL_ONCE.get_or_init(|| Model::load(&numa_core::paths::model_file(&[MODEL])?)).as_ref()
}

struct Fill {
    left: f32,
    top: f32,
    span: f32,
    ratio: Vec<f32>,
}

type Key = [u32; 8];

fn fills() -> &'static Mutex<HashMap<Key, Arc<Fill>>> {
    static FILLS: OnceLock<Mutex<HashMap<Key, Arc<Fill>>>> = OnceLock::new();
    FILLS.get_or_init(Default::default)
}

pub(crate) fn apply(
    spot: &Spot,
    data: &mut [f32],
    width: usize,
    height: usize,
    radius: f32,
    ring: [f32; 3],
    render: [u32; 4],
) {
    let key = [spot.at[0].to_bits(), spot.at[1].to_bits(), spot.radius.to_bits(), width as u32, render[0], render[1], render[2], render[3]];
    let known = fills().lock().unwrap_or_else(|poisoned| poisoned.into_inner()).get(&key).cloned();
    let fill = match known {
        Some(fill) => fill,
        None => {
            let Some(fill) = inpaint(spot, data, width, height, radius, ring) else { return };
            let fill = Arc::new(fill);
            let mut cache = fills().lock().unwrap_or_else(|poisoned| poisoned.into_inner());

            if cache.len() > 48 {
                cache.clear();
            }
            cache.insert(key, fill.clone());
            fill
        }
    };

    let centre = [spot.at[0] * width as f32, spot.at[1] * height as f32];
    let scale = fill.span / SIDE as f32;
    let left = (centre[0] - radius).floor().max(0.0) as usize;
    let right = ((centre[0] + radius).ceil() as usize).min(width.saturating_sub(1));
    let top = (centre[1] - radius).floor().max(0.0) as usize;
    let bottom = ((centre[1] + radius).ceil() as usize).min(height.saturating_sub(1));
    for y in top..=bottom {
        for x in left..=right {
            let alpha = spot.coverage((x as f32 + 0.5 - centre[0]).hypot(y as f32 + 0.5 - centre[1]), radius);
            if alpha <= 0.0 {
                continue;
            }
            let u = ((x as f32 + 0.5 - fill.left) / scale - 0.5).clamp(0.0, (SIDE - 1) as f32);
            let v = ((y as f32 + 0.5 - fill.top) / scale - 0.5).clamp(0.0, (SIDE - 1) as f32);
            let ratio = bilinear(&fill.ratio, SIDE, u, v);
            let pixel = &mut data[(y * width + x) * 3..][..3];
            for channel in 0..3 {
                pixel[channel] += (ratio[channel] * ring[channel] - pixel[channel]) * alpha;
            }
        }
    }
}

fn inpaint(spot: &Spot, data: &[f32], width: usize, height: usize, radius: f32, ring: [f32; 3]) -> Option<Fill> {
    let model = model()?;
    let span = (radius * WINDOW_RADII).max(64.0).min(width.min(height) as f32);
    let centre = [spot.at[0] * width as f32, spot.at[1] * height as f32];
    let left = (centre[0] - span / 2.0).clamp(0.0, width as f32 - span);
    let top = (centre[1] - span / 2.0).clamp(0.0, height as f32 - span);
    let scale = span / SIDE as f32;

    let mut window = vec![0.0f32; 3 * SIDE * SIDE];
    for y in 0..SIDE {
        for x in 0..SIDE {
            let sample = bilinear(data, width, (left + (x as f32 + 0.5) * scale - 0.5).max(0.0), (top + (y as f32 + 0.5) * scale - 0.5).max(0.0));
            window[(y * SIDE + x) * 3..][..3].copy_from_slice(&sample);
        }
    }
    let mut brightness: Vec<f32> = window.chunks_exact(3).map(|p| p[0].max(p[1]).max(p[2])).collect();
    brightness.sort_by(f32::total_cmp);
    let norm = brightness[brightness.len() * 99 / 100].max(1e-3);

    let plane = SIDE * SIDE;
    let mut image = vec![0.0f32; 3 * plane];
    let mut mask = vec![0.0f32; plane];
    let hole = radius * 1.08 / scale;
    let middle = [(centre[0] - left) / scale, (centre[1] - top) / scale];
    for y in 0..SIDE {
        for x in 0..SIDE {
            let at = y * SIDE + x;
            for channel in 0..3 {
                image[channel * plane + at] = ColourSpace::Srgb.encode((window[at * 3 + channel] / norm).clamp(0.0, 1.0));
            }
            mask[at] = ((x as f32 + 0.5 - middle[0]).hypot(y as f32 + 0.5 - middle[1]) <= hole) as u8 as f32;
        }
    }
    let image = ndarray::Array4::from_shape_vec((1, 3, SIDE, SIDE), image).ok()?.into_dyn();
    let mask = ndarray::Array4::from_shape_vec((1, 1, SIDE, SIDE), mask).ok()?.into_dyn();
    let output = match model.run(vec![image.into(), mask.into()]) {
        Ok(output) => output.into_iter().next()?,
        Err(err) => {
            log::warn!("LaMa failed: {err}");
            return None;
        }
    };
    let output: Vec<f32> = output.iter().copied().collect();
    if output.len() != 3 * plane {
        return None;
    }
    let ratio = (0..plane)
        .flat_map(|at| {
            let rgb: [f32; 3] = std::array::from_fn(|channel| ColourSpace::Srgb.decode(output[channel * plane + at] / 255.0) * norm);
            (0..3).map(move |channel| rgb[channel] / ring[channel].max(1e-6))
        })
        .collect();
    Some(Fill { left, top, span, ratio })
}

fn bilinear(data: &[f32], width: usize, x: f32, y: f32) -> [f32; 3] {
    let height = data.len() / 3 / width;
    let x = x.clamp(0.0, (width - 1) as f32);
    let y = y.clamp(0.0, (height - 1) as f32);
    let (x0, y0) = (x.floor() as usize, y.floor() as usize);
    let (x1, y1) = ((x0 + 1).min(width - 1), (y0 + 1).min(height - 1));
    let (fx, fy) = (x - x0 as f32, y - y0 as f32);
    std::array::from_fn(|channel| {
        let at = |cx: usize, cy: usize| data[(cy * width + cx) * 3 + channel];
        let upper = at(x0, y0) + (at(x1, y0) - at(x0, y0)) * fx;
        let lower = at(x0, y1) + (at(x1, y1) - at(x0, y1)) * fx;
        upper + (lower - upper) * fy
    })
}

#[cfg(test)]
mod tests {
    use numa_core::retouch::{Kind, Retouch, Spot};

    #[test]
    fn a_disc_on_stripes_is_removed() {
        if !super::is_installed() {
            println!("LaMa is not installed here");
            return;
        }
        let (width, height) = (400usize, 300usize);
        let stripe = |x: usize| if (x / 10) % 2 == 0 { 0.5 } else { 0.1 };
        let mut data: Vec<f32> = (0..width * height)
            .flat_map(|at| {
                let (x, y) = (at % width, at / width);
                match (x as f32 - 200.0).hypot(y as f32 - 150.0) < 20.0 {
                    true => [0.8, 0.05, 0.05],
                    false => [stripe(x); 3],
                }
            })
            .collect();
        let before = data.clone();
        let spot = Spot { at: [0.5, 0.5], radius: 26.0 / 400.0, feather: 0.2, kind: Kind::Remove, ..Spot::default() };
        crate::retouch::apply(&Retouch { spots: vec![spot] }, &mut data, width, height, [0.0, 0.0, 1.0, 1.0]);
        let pixel = |x: usize, y: usize| &data[(y * width + x) * 3..][..3];
        for x in 185..215 {
            let [r, g, b] = [pixel(x, 150)[0], pixel(x, 150)[1], pixel(x, 150)[2]];
            assert!(r - g.min(b) < 0.1, "red left at {x}: {r} {g} {b}");
        }

        let across: Vec<f32> = (180..220).map(|x| pixel(x, 150)[1]).collect();
        let (low, high) = across.iter().fold((f32::MAX, f32::MIN), |(l, h), v| (l.min(*v), h.max(*v)));
        assert!(high - low > 0.2, "the stripes did not come through: {low}..{high}");
        assert_eq!(&data[..width * 3], &before[..width * 3], "the top row moved");
    }
}
