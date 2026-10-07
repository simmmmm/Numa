use image::RgbImage;
use moxcms::{ColorProfile, DataColorSpace, Layout, ProfileClass, RenderingIntent, ProfileText, TransformOptions};
use numa_core::profile::{multiply_matrix, Matrix3};
use numa_core::space::ColourSpace;
use rayon::prelude::*;

use crate::Frame;

const GRID: usize = 33;

const LIMIT: f32 = 5.0;

const D50: [f64; 3] = [0.9642, 1.0, 0.8249];

pub struct About {
    pub name: String,

    pub rgb: bool,
}

pub fn describe(bytes: &[u8]) -> Result<About, String> {
    let profile = read(bytes)?;
    device(&profile)?;
    let name = match &profile.description {
        Some(ProfileText::PlainString(text)) => text.clone(),
        Some(ProfileText::Localizable(texts)) => {
            texts.iter().find(|text| text.language == "en").or(texts.first()).map(|text| text.value.clone()).unwrap_or_default()
        }
        Some(ProfileText::Description(text)) => text.ascii_string.clone(),
        None => String::new(),
    };
    let name = name.trim_end_matches('\0').trim().to_string();
    Ok(About { name: if name.is_empty() { "Unnamed profile".to_string() } else { name }, rgb: profile.color_space == DataColorSpace::Rgb })
}

fn read(bytes: &[u8]) -> Result<ColorProfile, String> {
    ColorProfile::new_from_slice(bytes).map_err(|err| format!("not a profile Numa can read: {err:?}"))
}

fn device(profile: &ColorProfile) -> Result<(Layout, usize), String> {
    match profile.color_space {
        DataColorSpace::Rgb => Ok((Layout::Rgb, 3)),
        DataColorSpace::Cmyk => Ok((Layout::Rgba, 4)),
        _ => Err("an RGB or CMYK printer's profile is needed".to_string()),
    }
}

fn options(intent: RenderingIntent) -> TransformOptions {
    TransformOptions { rendering_intent: intent, ..TransformOptions::default() }
}

fn intent(perceptual: bool) -> RenderingIntent {
    if perceptual { RenderingIntent::Perceptual } else { RenderingIntent::RelativeColorimetric }
}

pub struct Proof {

    table: Vec<[f32; 4]>,

    pub paper: bool,
}

impl Proof {

    pub fn new(bytes: &[u8], perceptual: bool) -> Result<Proof, String> {
        Self::of(read(bytes)?, perceptual)
    }

    pub fn srgb() -> Result<Proof, String> {
        Self::of(ColorProfile::new_srgb(), false)
    }

    fn of(printer: ColorProfile, perceptual: bool) -> Result<Proof, String> {
        let (layout, channels) = device(&printer)?;
        let srgb = ColorProfile::new_srgb();
        let into = |intent| srgb.create_transform_f32(Layout::Rgb, &printer, layout, options(intent));
        let fail = |err: moxcms::CmsError| format!("this profile cannot be proofed with: {err:?}");
        let chosen = match perceptual {
            true => into(RenderingIntent::Perceptual).or_else(|_| into(RenderingIntent::RelativeColorimetric)),
            false => into(RenderingIntent::RelativeColorimetric),
        }
        .map_err(fail)?;
        let colorimetric = into(RenderingIntent::RelativeColorimetric).map_err(fail)?;
        let back = printer.create_transform_f32(layout, &srgb, Layout::Rgb, options(RenderingIntent::RelativeColorimetric)).map_err(fail)?;

        let step = 1.0 / (GRID - 1) as f32;
        let points: Vec<f32> = (0..GRID * GRID * GRID)
            .flat_map(|at| [(at / (GRID * GRID)) as f32 * step, (at / GRID % GRID) as f32 * step, (at % GRID) as f32 * step])
            .collect();
        let through = |forward: &moxcms::TransformF32Executor| -> Result<Vec<f32>, String> {
            let mut ink = vec![0.0; points.len() / 3 * channels];
            forward.transform(&points, &mut ink).map_err(fail)?;

            ink.iter_mut().for_each(|value| *value = value.clamp(0.0, 1.0));
            let mut seen = vec![0.0; points.len()];
            back.transform(&ink, &mut seen).map_err(fail)?;
            Ok(seen)
        };
        let shown = through(&*chosen)?;
        let reached = through(&*colorimetric)?;

        let paper = paper(&printer);
        let white = apply(&paper, [1.0; 3]);

        let black = xyz(&reached[..3]);
        let table = points
            .chunks_exact(3)
            .zip(shown.chunks_exact(3))
            .zip(reached.chunks_exact(3))
            .map(|((point, shown), reached)| {
                let linear = [0, 1, 2].map(|channel| ColourSpace::Srgb.decode(shown[channel].clamp(0.0, 1.0)));
                let on_paper = apply(&paper, linear).map(|value| ColourSpace::Srgb.encode(value.clamp(0.0, 1.0)));
                let compensated: [f32; 3] = {
                    let wanted = xyz(point);
                    std::array::from_fn(|axis| black[axis] + wanted[axis] * (1.0 - black[axis] / D50[axis] as f32))
                };
                let missed = delta_e(lab(compensated), lab(xyz(reached))) > LIMIT;
                [on_paper[0], on_paper[1], on_paper[2], f32::from(u8::from(missed))]
            })
            .collect();
        Ok(Proof { table, paper: white.iter().any(|channel| *channel < 0.995) })
    }

    #[inline]
    fn pixel(&self, rgb: [u8; 3]) -> ([u8; 3], bool) {
        let place = |code: u8| {
            let at = code as f32 * ((GRID - 1) as f32 / 255.0);
            let low = (at as usize).min(GRID - 2);
            (low, at - low as f32)
        };
        let ((r, fr), (g, fg), (b, fb)) = (place(rgb[0]), place(rgb[1]), place(rgb[2]));
        let at = |dr: usize, dg: usize, db: usize| &self.table[((r + dr) * GRID + g + dg) * GRID + b + db];
        let (c000, c111) = (at(0, 0, 0), at(1, 1, 1));

        let (first, second, weights) = if fr >= fg {
            if fg >= fb {
                (at(1, 0, 0), at(1, 1, 0), [fr, fg, fb])
            } else if fr >= fb {
                (at(1, 0, 0), at(1, 0, 1), [fr, fb, fg])
            } else {
                (at(0, 0, 1), at(1, 0, 1), [fb, fr, fg])
            }
        } else if fb >= fg {
            (at(0, 0, 1), at(0, 1, 1), [fb, fg, fr])
        } else if fb >= fr {
            (at(0, 1, 0), at(0, 1, 1), [fg, fb, fr])
        } else {
            (at(0, 1, 0), at(1, 1, 0), [fg, fr, fb])
        };
        let value = |channel: usize| {
            c000[channel]
                + weights[0] * (first[channel] - c000[channel])
                + weights[1] * (second[channel] - first[channel])
                + weights[2] * (c111[channel] - second[channel])
        };
        ([0, 1, 2].map(|channel| (value(channel) * 255.0 + 0.5) as u8), value(3) > 0.5)
    }

    pub fn shown(&self, image: &RgbImage, out_of_range: bool) -> RgbImage {
        let width = image.width() as usize;
        let mut out = image.clone();
        out.par_chunks_exact_mut(width * 3).enumerate().for_each(|(y, row)| {
            for (x, pixel) in row.chunks_exact_mut(3).enumerate() {
                let (rgb, missed) = self.pixel([pixel[0], pixel[1], pixel[2]]);
                pixel.copy_from_slice(&if out_of_range && missed && hatched(x, y) { HATCH } else { rgb });
            }
        });
        out
    }

    pub fn apply_rgba(&self, bytes: &mut [u8], width: usize, stride: usize, out_of_range: bool) {
        bytes.par_chunks_mut(stride).enumerate().for_each(|(y, row)| {
            for (x, pixel) in row[..width * 4].chunks_exact_mut(4).enumerate() {
                let (rgb, missed) = self.pixel([pixel[0], pixel[1], pixel[2]]);
                pixel[..3].copy_from_slice(&if out_of_range && missed && hatched(x, y) { HATCH } else { rgb });
            }
        });
    }
}

const HATCH: [u8; 3] = [128, 128, 128];

#[inline]
fn hatched(x: usize, y: usize) -> bool {
    (x + y) % 8 < 3
}

fn paper(printer: &ColorProfile) -> Matrix3 {
    let white = match printer.profile_class {
        ProfileClass::OutputDevice => printer.media_white_point.map_or(D50, |white| [white.x, white.y, white.z]),
        _ => D50,
    };
    let scale: Matrix3 = std::array::from_fn(|row| {
        std::array::from_fn(|column| if row == column { (white[row] / D50[row]).min(1.0) as f32 } else { 0.0 })
    });
    multiply_matrix(&ColourSpace::Srgb.from_xyz(), &multiply_matrix(&scale, &ColourSpace::Srgb.to_xyz()))
}

fn apply(matrix: &Matrix3, value: [f32; 3]) -> [f32; 3] {
    matrix.map(|row| row[0] * value[0] + row[1] * value[1] + row[2] * value[2])
}

fn xyz(rgb: &[f32]) -> [f32; 3] {
    let linear = [0, 1, 2].map(|channel| ColourSpace::Srgb.decode(rgb[channel].clamp(0.0, 1.0)));
    apply(&ColourSpace::Srgb.to_xyz(), linear)
}

fn lab(xyz: [f32; 3]) -> [f32; 3] {
    let f = |value: f32| if value > 216.0 / 24389.0 { value.cbrt() } else { (24389.0 / 27.0 * value + 16.0) / 116.0 };
    let [x, y, z] = [0, 1, 2].map(|axis| f(xyz[axis] / D50[axis] as f32));
    [116.0 * y - 16.0, 500.0 * (x - y), 200.0 * (y - z)]
}

fn delta_e(a: [f32; 3], b: [f32; 3]) -> f32 {
    ((a[0] - b[0]).powi(2) + (a[1] - b[1]).powi(2) + (a[2] - b[2]).powi(2)).sqrt()
}

pub fn convert8(image: &RgbImage, bytes: &[u8], perceptual: bool) -> Result<RgbImage, String> {
    let printer = writable(bytes)?;
    let srgb = ColorProfile::new_srgb();
    let make = |intent| srgb.create_transform_8bit(Layout::Rgb, &printer, Layout::Rgb, options(intent));
    let transform = make(self::intent(perceptual)).or_else(|_| make(RenderingIntent::RelativeColorimetric)).map_err(|err| format!("{err:?}"))?;
    let mut out = image.clone();
    convert_rows(image.as_raw(), &mut out, image.width() as usize * 3, |from, to| transform.transform(from, to))?;
    Ok(out)
}

pub fn convert16(image: &Frame<u16>, bytes: &[u8], perceptual: bool) -> Result<Frame<u16>, String> {
    let printer = writable(bytes)?;
    let srgb = ColorProfile::new_srgb();
    let make = |intent| srgb.create_transform_16bit(Layout::Rgb, &printer, Layout::Rgb, options(intent));
    let transform = make(self::intent(perceptual)).or_else(|_| make(RenderingIntent::RelativeColorimetric)).map_err(|err| format!("{err:?}"))?;
    let mut out = image.clone();
    convert_rows(image.as_raw(), &mut out, image.width() as usize * 3, |from, to| transform.transform(from, to))?;
    Ok(out)
}

fn writable(bytes: &[u8]) -> Result<ColorProfile, String> {
    let printer = read(bytes)?;
    match printer.color_space {
        DataColorSpace::Rgb => Ok(printer),
        _ => Err("a CMYK profile is for proofing only: files for a lab are RGB".to_string()),
    }
}

fn convert_rows<T: Copy + Send + Sync>(
    from: &[T],
    into: &mut [T],
    row: usize,
    transform: impl Fn(&[T], &mut [T]) -> Result<(), moxcms::CmsError> + Sync,
) -> Result<(), String> {
    into.par_chunks_mut(row * 64)
        .zip(from.par_chunks(row * 64))
        .try_for_each(|(into, from)| transform(from, into))
        .map_err(|err| format!("{err:?}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use moxcms::Xyzd;

    fn narrow_paper() -> Vec<u8> {
        let mut profile = ColorProfile::new_srgb();
        profile.cicp = None;
        profile.profile_class = ProfileClass::OutputDevice;
        let grey = |c: Xyzd, white: [f64; 3]| Xyzd { x: c.x * 0.55 + white[0] * 0.15, y: c.y * 0.55 + white[1] * 0.15, z: c.z * 0.55 + white[2] * 0.15 };
        profile.red_colorant = grey(profile.red_colorant, D50);
        profile.green_colorant = grey(profile.green_colorant, D50);
        profile.blue_colorant = grey(profile.blue_colorant, D50);
        profile.media_white_point = Some(Xyzd { x: 0.88, y: 0.92, z: 0.70 });
        profile.description = Some(ProfileText::PlainString("Narrow Test Paper".to_string()));
        profile.encode().unwrap()
    }

    #[test]
    fn a_profile_is_named_and_known_for_rgb() {
        let about = describe(&narrow_paper()).unwrap();
        assert_eq!(about.name, "Narrow Test Paper");
        assert!(about.rgb);
        assert!(describe(b"not a profile").is_err());
    }

    #[test]
    fn the_narrow_paper_shows_what_it_cannot_print() {
        let proof = Proof::new(&narrow_paper(), false).unwrap();
        assert!(proof.pixel([255, 0, 0]).1, "pure red is beyond this paper");
        assert!(proof.pixel([0, 0, 255]).1);
        for grey in [0u8, 60, 128, 200] {
            assert!(!proof.pixel([grey, grey, grey]).1, "grey {grey} is printable");
        }
        let (white, _) = proof.pixel([255, 255, 255]);
        assert!(white[2] < white[0] && white[0] < 255, "warm and darker: {white:?}");

        assert!(proof.paper);
        let same = Proof::srgb().unwrap();
        assert!(!same.paper);
        for rgb in [[255u8, 0, 0], [12, 200, 90], [128, 128, 128], [255, 255, 255], [3, 7, 250]] {
            let (out, missed) = same.pixel(rgb);
            assert!(!missed, "{rgb:?} is in sRGB");
            assert!(out.iter().zip(rgb).all(|(a, b)| a.abs_diff(b) <= 3), "{rgb:?} -> {out:?}");
        }
    }

    #[test]
    fn the_hatch_and_both_frames_agree() {
        let proof = Proof::new(&narrow_paper(), true).unwrap();
        let frame = RgbImage::from_fn(40, 12, |x, _| if x < 20 { image::Rgb([255, 0, 0]) } else { image::Rgb([128, 128, 128]) });
        let plain = proof.shown(&frame, false);
        assert!(plain.pixels().all(|pixel| pixel.0 != HATCH));
        let hatched = proof.shown(&frame, true);
        assert!(hatched.get_pixel(0, 0).0 == HATCH && hatched.get_pixel(5, 0).0 != HATCH);
        assert!(hatched.enumerate_pixels().filter(|(x, ..)| *x >= 20).all(|(.., pixel)| *pixel == *plain.get_pixel(25, 0)));

        let stride = 40 * 4 + 16;
        let mut rgba = vec![0u8; stride * 12];
        for (y, row) in rgba.chunks_mut(stride).enumerate() {
            for x in 0..40 {
                row[x * 4..x * 4 + 3].copy_from_slice(&frame.get_pixel(x as u32, y as u32).0);
            }
        }
        proof.apply_rgba(&mut rgba, 40, stride, true);
        for (y, row) in rgba.chunks(stride).enumerate() {
            for x in 0..40 {
                assert_eq!(row[x * 4..x * 4 + 3], hatched.get_pixel(x as u32, y as u32).0);
            }
        }
    }

    #[test]
    fn a_frame_converts_to_an_rgb_paper() {
        let frame = RgbImage::from_fn(64, 64, |x, y| image::Rgb([(x * 4) as u8, (y * 4) as u8, 128]));
        let converted = convert8(&frame, &narrow_paper(), false).unwrap();
        let red = convert8(&RgbImage::from_pixel(1, 1, image::Rgb([255, 0, 0])), &narrow_paper(), false).unwrap();
        assert_eq!(red.get_pixel(0, 0).0[0], 255, "pure red is the paper's most red: {:?}", red.get_pixel(0, 0));
        let grey = convert8(&RgbImage::from_pixel(1, 1, image::Rgb([128, 128, 128])), &narrow_paper(), false).unwrap();
        let [r, g, b] = grey.get_pixel(0, 0).0;
        assert!(r.abs_diff(g) <= 2 && g.abs_diff(b) <= 2, "grey stays grey: {r} {g} {b}");
        let deep = Frame::<u16>::from_fn(64, 64, |x, y| image::Rgb([(x * 4) as u16 * 257, (y * 4) as u16 * 257, 128 * 257]));
        let deep = convert16(&deep, &narrow_paper(), false).unwrap();
        for (eight, sixteen) in converted.as_raw().iter().zip(deep.as_raw()) {
            assert!((*eight as i32 - (*sixteen / 257) as i32).abs() <= 2);
        }
    }

    #[test]
    #[ignore]
    fn a_real_printer_profile() {
        let Ok(path) = std::env::var("NUMA_PRINT_ICC") else { return };
        let bytes = std::fs::read(path).unwrap();
        let about = describe(&bytes).unwrap();
        for perceptual in [true, false] {
            let started = std::time::Instant::now();
            let proof = Proof::new(&bytes, perceptual).unwrap();
            println!("{} (rgb {}), perceptual {perceptual}: table in {:?}", about.name, about.rgb, started.elapsed());
            for patch in [[255u8, 255, 255], [255, 0, 0], [0, 255, 0], [0, 0, 255], [128, 128, 128], [220, 170, 140], [0, 0, 0]] {
                println!("  {patch:?} -> {:?}", proof.pixel(patch));
            }
            let frame = RgbImage::from_fn(2560, 1707, |x, y| image::Rgb([x as u8, y as u8, (x ^ y) as u8]));
            for _ in 0..3 {
                let started = std::time::Instant::now();
                let shown = proof.shown(&frame, true);
                println!("  2560 × 1707: {:?} ({} hatched)", started.elapsed(), shown.pixels().filter(|pixel| pixel.0 == HATCH).count());
            }
        }
    }
}
