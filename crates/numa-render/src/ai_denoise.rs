use std::path::Path;
use std::sync::{Arc, Mutex};

use image::ImageBuffer;
use rayon::prelude::*;

use numa_core::document::Document;
use numa_core::denoise::{Denoised, MODEL, MODEL_HALF, SHARPEN_MODEL, WEIGHTS};
use numa_core::image::LinearImage;
use numa_core::profile::{multiply, Matrix3};
use numa_core::space::ColourSpace;
use numa_infer::Model;

pub const TILE: usize = 256;

fn tile_for(model: &Model) -> usize {
    match model.on_card() {
        true => 512,
        false => 320,
    }
}

fn load() -> Result<Model, String> {
    if let Some(half) = numa_core::paths::model_file(&[MODEL_HALF]) {
        if let Some(model) = Model::load(&half).filter(Model::on_card) {
            return Ok(model);
        }
    }
    let path = numa_core::paths::model_file(&[MODEL]).ok_or("the SCUNet model is not installed")?;
    Model::load(&path).ok_or_else(|| "the SCUNet model could not be loaded".to_string())
}

const OVERLAP: usize = 32;

pub fn is_installed() -> bool {
    numa_core::paths::model_file(&[MODEL]).is_some() && numa_core::paths::model_file(&[WEIGHTS]).is_some()
}

pub fn sharpen_installed() -> bool {
    numa_core::paths::model_file(&[SHARPEN_MODEL]).is_some()
}

pub fn sharpen(source: &LinearImage, progress: impl FnMut(usize, usize) -> bool) -> Result<Option<Denoised>, String> {
    let path = numa_core::paths::model_file(&[SHARPEN_MODEL]).ok_or("the AI sharpen model is not installed")?;
    let model = Model::load(&path).ok_or("the AI sharpen model could not be loaded")?;
    pass(source, &model, TILE, progress)
}

pub fn denoise(source: &LinearImage, progress: impl FnMut(usize, usize) -> bool) -> Result<Option<Denoised>, String> {

    let model = load()?;
    let tile = tile_for(&model);
    pass(source, &model, tile, progress)
}

fn pass(source: &LinearImage, model: &Model, tile: usize, progress: impl FnMut(usize, usize) -> bool) -> Result<Option<Denoised>, String> {
    let (width, height) = (source.width as usize, source.height as usize);
    let matrix = view_matrix(source);
    let data = &source.data;
    let pixel = |x: usize, y: usize| {
        let at = (y * width + x) * 3;
        multiply(&matrix, [data[at], data[at + 1], data[at + 2]]).map(|value| ColourSpace::Srgb.encode(value))
    };
    let run = |input: Vec<f32>| {
        let tensor = ndarray::Array4::from_shape_vec((1, 3, tile, tile), input)
            .map_err(|err| err.to_string())?
            .into_dyn();
        let outputs = model.run(vec![tensor.into()]).map_err(|err| err.to_string())?;
        let output = outputs.into_iter().next().ok_or("the model gave no output")?;
        Ok(output.iter().copied().collect())
    };

    let Some(codes) = tiled(width, height, tile, pixel, run, progress)? else {
        return Ok(None);
    };
    let codes = codes.par_iter().map(|code| to_twelve_bits(*code)).collect();
    Ok(ImageBuffer::from_raw(source.width, source.height, codes))
}

fn to_twelve_bits(code: f32) -> u16 {
    let twelve = (code.clamp(0.0, 1.0) * 4095.0).round() as u16;
    (twelve << 4) | (twelve >> 8)
}

pub fn tiled(
    width: usize,
    height: usize,
    tile: usize,
    pixel: impl Fn(usize, usize) -> [f32; 3],
    mut model: impl FnMut(Vec<f32>) -> Result<Vec<f32>, String>,
    mut progress: impl FnMut(usize, usize) -> bool,
) -> Result<Option<Vec<f32>>, String> {
    if width == 0 || height == 0 {
        return Ok(Some(Vec::new()));
    }
    let (across, down) = (origins(width, tile), origins(height, tile));
    let total = across.len() * down.len();
    let plane = tile * tile;

    let mut sum = vec![0.0f32; width * height * 3];
    let mut weight = vec![0.0f32; width * height];
    let tiles = down.iter().flat_map(|&top| across.iter().map(move |&left| (top, left)));
    for (done, (top, left)) in tiles.enumerate() {
        if !progress(done, total) {
            return Ok(None);
        }
        let mut input = vec![0.0f32; 3 * plane];
        for ty in 0..tile {
            for tx in 0..tile {
                let rgb = pixel((left + tx).min(width - 1), (top + ty).min(height - 1));
                for (channel, value) in rgb.into_iter().enumerate() {
                    input[channel * plane + ty * tile + tx] = value;
                }
            }
        }
        let output = model(input)?;
        if output.len() != 3 * plane {
            return Err(format!("the model answered with {} values for a {tile}-pixel tile", output.len()));
        }
        for ty in 0..tile.min(height - top) {
            for tx in 0..tile.min(width - left) {
                let share = ramp(tx, tile) * ramp(ty, tile);
                let at = (top + ty) * width + left + tx;
                weight[at] += share;
                for channel in 0..3 {
                    sum[at * 3 + channel] += share * output[channel * plane + ty * tile + tx];
                }
            }
        }
    }
    progress(total, total);

    sum.par_chunks_exact_mut(3).zip(weight.par_iter()).for_each(|(pixel, weight)| {
        pixel.iter_mut().for_each(|value| *value /= weight);
    });
    Ok(Some(sum))
}

fn origins(length: usize, tile: usize) -> Vec<usize> {
    if length <= tile {
        return vec![0];
    }
    let mut starts: Vec<usize> = (0..).map(|index| index * (tile - OVERLAP)).take_while(|start| start + tile < length).collect();
    starts.push(length - tile);
    starts
}

fn ramp(position: usize, tile: usize) -> f32 {
    let from_edge = position.min(tile - 1 - position) as f32 + 0.5;
    (from_edge / OVERLAP as f32).min(1.0)
}

pub fn view_matrix(source: &LinearImage) -> Matrix3 {
    source.profile.as_ref().map_or(IDENTITY, |profile| profile.transform(None))
}

const IDENTITY: Matrix3 = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];

fn inverse(m: &Matrix3) -> Option<Matrix3> {
    let cofactor = |r0: usize, r1: usize, c0: usize, c1: usize| m[r0][c0] * m[r1][c1] - m[r0][c1] * m[r1][c0];
    let adjugate = [
        [cofactor(1, 2, 1, 2), -cofactor(0, 2, 1, 2), cofactor(0, 1, 1, 2)],
        [-cofactor(1, 2, 0, 2), cofactor(0, 2, 0, 2), -cofactor(0, 1, 0, 2)],
        [cofactor(1, 2, 0, 1), -cofactor(0, 2, 0, 1), cofactor(0, 1, 0, 1)],
    ];
    let determinant = m[0][0] * adjugate[0][0] + m[0][1] * adjugate[1][0] + m[0][2] * adjugate[2][0];
    (determinant.abs() > 1e-9).then(|| adjugate.map(|row| row.map(|value| value / determinant)))
}

pub fn blend(source: &LinearImage, denoised: impl Fn(usize) -> [f32; 3] + Sync, amount: f32) -> LinearImage {
    let matrix = view_matrix(source);
    let back = inverse(&matrix).unwrap_or(IDENTITY);
    let mut out = source.clone();
    out.data.par_chunks_exact_mut(3).enumerate().for_each(|(index, pixel)| {
        let seen = multiply(&matrix, [pixel[0], pixel[1], pixel[2]]).map(|value| value.clamp(0.0, 1.0));
        let answer = denoised(index);
        let change = multiply(&back, [0, 1, 2].map(|c| (answer[c] - seen[c]) * amount));
        for (value, change) in pixel.iter_mut().zip(change) {
            *value += change;
        }
    });
    out
}

fn linear_at(stored: &Denoised, width: u32, height: u32) -> Option<Vec<f32>> {
    let linear: Vec<f32> = stored.as_raw().par_iter().map(|code| ColourSpace::Srgb.decode(*code as f32 / 65535.0)).collect();
    if (stored.width(), stored.height()) == (width, height) {
        return Some(linear);
    }
    let scaled = LinearImage::new(stored.width(), stored.height(), linear).downscaled(width.max(height))?;
    ((scaled.width, scaled.height) == (width, height)).then_some(scaled.data)
}

const MEMO_EDGE: u32 = 4096;

pub fn warm(photo: &Path, stored: &Denoised, width: u32, height: u32) {
    let _ = remembered(photo, stored, width, height);
}

fn remembered(photo: &Path, stored: &Denoised, width: u32, height: u32) -> Option<Arc<Vec<f32>>> {

    type Memo = Vec<(std::path::PathBuf, usize, u32, u32, Arc<Vec<f32>>)>;
    static MEMO: Mutex<Memo> = Mutex::new(Vec::new());

    let key = photo.to_path_buf();

    let which = stored.as_raw().as_ptr() as usize;
    let memo = MEMO.lock().unwrap_or_else(|poisoned| poisoned.into_inner()).clone();
    if let Some((_, _, _, _, answer)) = memo.iter().find(|(at, of, w, h, _)| *at == key && *of == which && (*w, *h) == (width, height)) {
        return Some(answer.clone());
    }
    let Some(answer) = linear_at(stored, width, height) else {
        log::warn!("{}: the denoised frame is {}x{}, not the photograph's shape", photo.display(), stored.width(), stored.height());
        return None;
    };
    let answer = Arc::new(answer);
    if width.max(height) <= MEMO_EDGE {
        let mut memo = MEMO.lock().unwrap_or_else(|poisoned| poisoned.into_inner());

        memo.retain(|(at, of, ..)| *at == key && *of != which);
        memo.truncate(1);
        memo.push((key, which, width, height, answer.clone()));
    }
    Some(answer)
}

pub fn for_render(document: &Document, source: &LinearImage, stored: Option<&Denoised>) -> Option<LinearImage> {
    mixed(document, source, stored, document.ai_denoise)
}

pub fn sharpen_for_render(document: &Document, source: &LinearImage, stored: Option<&Denoised>) -> Option<LinearImage> {
    mixed(document, source, stored, document.ai_sharpen)
}

fn mixed(document: &Document, source: &LinearImage, stored: Option<&Denoised>, percent: f32) -> Option<LinearImage> {
    let amount = (percent / 100.0).clamp(0.0, 1.0);
    if amount <= 0.0 {
        return None;
    }
    let answer = remembered(Path::new(&document.source.path), stored?, source.width, source.height)?;
    Some(blend(source, |index| [answer[index * 3], answer[index * 3 + 1], answer[index * 3 + 2]], amount))
}

#[cfg(test)]
mod tests {
    use image::Rgb;
    use super::*;
    use numa_core::color::CameraProfile;

    fn gradient(width: usize, height: usize) -> impl Fn(usize, usize) -> [f32; 3] {
        move |x, y| [x as f32 / width as f32, y as f32 / height as f32, ((x * 7 + y * 13) % 17) as f32 / 17.0]
    }

    #[test]
    fn a_smear_comes_back_sharp() {
        if !sharpen_installed() {
            println!("the AI sharpen model is not installed here");
            return;
        }
        let (width, height) = (320usize, 240usize);

        let mut seed = 11u32;
        let mut random = || {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (seed >> 8) as f32 / (1 << 24) as f32
        };
        let mut texture = vec![0.0f32; width * height];
        for (cell, weight) in [(40usize, 0.5f32), (16, 0.3), (6, 0.2)] {
            let grid: Vec<f32> = (0..(width / cell + 2) * (height / cell + 2)).map(|_| random()).collect();
            let across = width / cell + 2;
            for y in 0..height {
                for x in 0..width {
                    let (gx, gy) = (x as f32 / cell as f32, y as f32 / cell as f32);
                    let (ix, iy, fx, fy) = (gx as usize, gy as usize, gx.fract(), gy.fract());
                    let at = |cx: usize, cy: usize| grid[cy * across + cx];
                    let top = at(ix, iy) + (at(ix + 1, iy) - at(ix, iy)) * fx;
                    let bottom = at(ix, iy + 1) + (at(ix + 1, iy + 1) - at(ix, iy + 1)) * fx;
                    texture[y * width + x] += weight * (top + (bottom - top) * fy);
                }
            }
        }
        let smeared: Vec<f32> = (0..width * height)
            .map(|at| {
                let (x, y) = (at % width, at / width);
                (0..9).map(|d| texture[y * width + (x + d).saturating_sub(4).min(width - 1)]).sum::<f32>() / 9.0
            })
            .collect();
        let data = smeared.iter().flat_map(|code| [ColourSpace::Srgb.decode(*code); 3]).collect();
        let frame = LinearImage::new(width as u32, height as u32, data);
        let sharp = sharpen(&frame, |_, _| true).unwrap().unwrap();
        let apart = |value: &dyn Fn(usize, usize) -> f32| -> f32 {
            (0..height).flat_map(|y| (1..width).map(move |x| (x, y))).map(|(x, y)| (value(x, y) - texture[y * width + x]).powi(2)).sum()
        };
        let before = apart(&|x, y| smeared[y * width + x]);
        let after = apart(&|x, y| sharp.get_pixel(x as u32, y as u32).0[0] as f32 / 65535.0);
        assert!(after < before * 0.6, "not nearer the unsmeared texture: {before:.1} then {after:.1}");
    }

    #[test]
    fn identity_model_puts_the_frame_back_together() {

        for (width, height) in [(700, 529), (100, 40), (256, 257)] {
            let pixel = gradient(width, height);
            let mut calls = 0;
            let out = tiled(width, height, TILE, &pixel, |tile| {
                calls += 1;
                Ok(tile)
            }, |_, _| true)
            .unwrap()
            .unwrap();
            assert_eq!(calls, origins(width, TILE).len() * origins(height, TILE).len());
            for y in 0..height {
                for x in 0..width {
                    let want = pixel(x, y);
                    for c in 0..3 {
                        assert!((out[(y * width + x) * 3 + c] - want[c]).abs() < 1e-5, "{width}x{height} at {x},{y}");
                    }
                }
            }
        }
    }

    #[test]
    fn a_seam_is_feathered_not_cut() {

        let mut next = 0.0;
        let out = tiled(480, 1, TILE, |_, _| [0.0; 3], |tile| {
            next += 1.0;
            Ok(vec![next; tile.len()])
        }, |_, _| true)
        .unwrap()
        .unwrap();
        let steps: Vec<f32> = out.chunks(3).map(|p| p[0]).collect::<Vec<_>>().windows(2).map(|w| (w[1] - w[0]).abs()).collect();
        assert!(steps.iter().all(|step| *step < 0.1), "largest step {}", steps.iter().cloned().fold(0.0, f32::max));
    }

    #[test]
    fn stopping_stops() {
        let out = tiled(600, 600, TILE, |_, _| [0.5; 3], Ok, |done, _| done < 2).unwrap();
        assert!(out.is_none());
    }

    #[test]
    fn colour_goes_there_and_back() {
        let profile = CameraProfile {
            as_shot: [2.1, 1.0, 1.6],
            xyz_to_cam: [[0.9, 0.1, 0.0], [0.2, 0.8, 0.1], [0.0, 0.1, 0.9]],
            cam_to_srgb: [[1.7, -0.5, -0.2], [-0.2, 1.4, -0.2], [0.0, -0.4, 1.4]],
        };
        let data = vec![0.02, 0.05, 0.03, 0.2, 0.3, 0.1, 0.9, 1.4, 0.8, 0.0, 0.001, 0.4];
        let source = LinearImage::new(4, 1, data.clone()).with_profile(profile);
        let matrix = view_matrix(&source);

        let answer = |index: usize| {
            let at = index * 3;
            multiply(&matrix, [data[at], data[at + 1], data[at + 2]]).map(|value| {
                let code = to_twelve_bits(ColourSpace::Srgb.encode(value));
                ColourSpace::Srgb.decode(code as f32 / 65535.0)
            })
        };
        let back = blend(&source, answer, 1.0);
        for (got, want) in back.data.iter().zip(&data) {
            assert!((got - want).abs() < 1e-3 * want.max(0.01) + 2e-4, "{got} against {want}");
        }

        assert_eq!(blend(&source, |_| [0.7; 3], 0.0).data, data);
    }

    #[test]
    fn a_stored_answer_shrinks_to_the_proxy() {
        let stored = Denoised::from_fn(300, 200, |x, _| Rgb([(x * 200) as u16; 3]));
        let proxy = LinearImage::new(300, 200, vec![0.0; 300 * 200 * 3]).downscaled(150).unwrap();
        let scaled = linear_at(&stored, proxy.width, proxy.height).unwrap();
        assert_eq!(scaled.len(), proxy.data.len());
        assert!(linear_at(&stored, 149, 100).is_none(), "a different shape is not this photograph");
    }

    #[test]
    fn a_kept_answer_reaches_the_colour_stage() {
        numa_core::paths::use_test_cache(std::env::temp_dir().join("numa-thumbs-test-cache"));
        let dir = std::env::temp_dir().join(format!("numa-ai-denoise-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let photo = dir.join("frame.RAF");
        std::fs::write(&photo, b"a stand-in").unwrap();

        let source = LinearImage::new(40, 20, vec![0.3; 40 * 20 * 3]);
        let mut document = Document::new(photo.display().to_string());
        let code = to_twelve_bits(ColourSpace::Srgb.encode(0.2));
        let kept = Denoised::from_pixel(40, 20, Rgb([code; 3]));
        assert!(for_render(&document, &source, Some(&kept)).is_none(), "off");
        document.ai_denoise = 100.0;
        assert!(for_render(&document, &source, None).is_none(), "on, with nothing handed in");

        let near = |image: &LinearImage, want: f32| image.data.iter().all(|v| (v - want).abs() < 1e-3);
        assert!(near(&for_render(&document, &source, Some(&kept)).unwrap(), 0.2), "the full-size frame");
        assert!(near(&for_render(&document, &source.downscaled(20).unwrap(), Some(&kept)).unwrap(), 0.2), "and the proxy");
        document.ai_denoise = 50.0;
        assert!(near(&for_render(&document, &source, Some(&kept)).unwrap(), 0.25), "half of it");

        let _ = std::fs::remove_file(numa_io::denoised::cache_path(&photo).unwrap());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    #[ignore]
    fn whole_frame() {
        let Ok(path) = std::env::var("WHOLE") else { return };
        let photo = Path::new(&path);
        numa_core::paths::use_test_cache(std::env::temp_dir().join("numa-thumbs-test-cache"));
        let full = numa_io::raw::decode_linear_best(photo).unwrap();

        let started = std::time::Instant::now();
        let mut tiles = 0;
        assert!(numa_io::denoised::ensure(photo, &full, |_, total| {
            tiles = total;
            true
        })
        .unwrap());
        let kept = numa_io::denoised::cache_path(photo).unwrap();
        println!(
            "{}x{}: {tiles} tiles in {:?}, kept in {} MB",
            full.width,
            full.height,
            started.elapsed(),
            std::fs::metadata(&kept).unwrap().len() / 1_000_000
        );

        let mut document = Document::new(path.clone());
        document.ai_denoise = 100.0;
        let started = std::time::Instant::now();
        let stored = numa_io::denoised::load(Path::new(&path)).unwrap();
        let denoised = for_render(&document, &full, Some(&stored)).unwrap();
        println!("read back at full size in {:?}", started.elapsed());
        let proxy = full.downscaled(2400).unwrap();
        let started = std::time::Instant::now();
        for_render(&document, &proxy, Some(&stored)).unwrap();
        println!("at the proxy's size in {:?}, again in {:?}", started.elapsed(), {
            let again = std::time::Instant::now();
            for_render(&document, &proxy, Some(&stored)).unwrap();
            again.elapsed()
        });

        if let Ok(out) = std::env::var("OUT") {
            let plain = Document::new(path.clone());
            let before = crate::develop(&plain, &full, &Default::default());
            let after = crate::develop(&Document { ai_denoise: 0.0, ..plain.clone() }, &denoised, &Default::default());
            let (x, y) = (full.width / 2 - 400, full.height / 2 - 300);
            image::imageops::crop_imm(&before, x, y, 800, 600).to_image().save(format!("{out}-before.png")).unwrap();
            image::imageops::crop_imm(&after, x, y, 800, 600).to_image().save(format!("{out}-after.png")).unwrap();
        }
        let _ = std::fs::remove_file(kept);
    }

    #[test]
    #[ignore]
    fn real_model_on_a_crop() {
        if !is_installed() {
            println!("SCUNet is not installed; nothing to measure");
            return;
        }
        let crop = 720;
        let source = match std::env::var("NOISY") {
            Ok(path) => {
                let full = numa_io::raw::decode_linear_best(Path::new(&path)).unwrap();
                let (x0, y0) = ((full.width as usize - crop) / 2, (full.height as usize - crop) / 2);
                let mut data = Vec::with_capacity(crop * crop * 3);
                for y in y0..y0 + crop {
                    let at = (y * full.width as usize + x0) * 3;
                    data.extend_from_slice(&full.data[at..at + crop * 3]);
                }
                let mut image = LinearImage::new(crop as u32, crop as u32, data);
                image.profile = full.profile.clone();
                image
            }

            Err(_) => LinearImage::new(
                crop as u32,
                crop as u32,
                (0..crop * crop * 3).map(|i| 0.18 + ((i * 2_654_435_761) % 1000) as f32 / 1000.0 * 0.02 - 0.01).collect(),
            ),
        };

        let started = std::time::Instant::now();
        let mut first = None;
        let mut tiles = 0;
        let denoised = denoise(&source, |done, total| {
            tiles = total;
            if done == 1 {
                first = Some(std::time::Instant::now());
            }
            true
        })
        .unwrap()
        .unwrap();
        let first = first.unwrap_or(started);
        println!(
            "{tiles} tiles of {TILE} in {:?} with the model's loading; {:?} a tile after the first",
            started.elapsed(),
            first.elapsed() / (tiles.max(2) - 1) as u32
        );

        let spread = |values: &mut dyn Iterator<Item = f32>| {
            let values: Vec<f32> = values.collect();
            let mean = values.iter().sum::<f32>() / values.len() as f32;
            (values.iter().map(|v| (v - mean).powi(2)).sum::<f32>() / values.len() as f32).sqrt()
        };
        let matrix = view_matrix(&source);
        let before = spread(&mut source.data.chunks(3).map(|p| ColourSpace::Srgb.encode(multiply(&matrix, [p[0], p[1], p[2]])[1])));
        let after = spread(&mut denoised.as_raw().chunks(3).map(|p| p[1] as f32 / 65535.0));
        println!("green spread in sRGB codes: {before:.4} before, {after:.4} after");

        let mut png = Vec::new();
        let written = std::time::Instant::now();
        numa_io::denoised::encode(&denoised, &mut png).unwrap();
        println!("encoded in {:?}", written.elapsed());
        println!("kept as a 16-bit PNG: {} bytes, {:.1} bits a pixel", png.len(), png.len() as f32 * 8.0 / (crop * crop) as f32);
        if let Ok(out) = std::env::var("OUT") {
            std::fs::write(&out, &png).unwrap();
            let before = image::RgbImage::from_fn(crop as u32, crop as u32, |x, y| {
                let at = (y as usize * crop + x as usize) * 3;
                let d = &source.data;
                image::Rgb(multiply(&matrix, [d[at], d[at + 1], d[at + 2]]).map(|v| (ColourSpace::Srgb.encode(v) * 255.0).round() as u8))
            });
            before.save(format!("{out}.before.png")).unwrap();
        }
    }
}
