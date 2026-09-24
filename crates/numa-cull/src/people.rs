use std::path::PathBuf;
use std::sync::OnceLock;

use image::RgbImage;

use super::faces::Face;
use numa_infer::Model;

const SIDE: usize = 112;

const TEMPLATE: [[f32; 2]; 5] = [
    [38.2946, 51.6963],
    [73.5318, 51.5014],
    [56.0252, 71.7366],
    [41.5493, 92.3655],
    [70.7299, 92.2041],
];

pub const LENGTH: usize = 128;

pub const SUGGEST: f32 = 0.5;

const MARGIN: f32 = 0.05;

pub fn model_path() -> PathBuf {
    super::faces::model_dir().join("face_recognition_sface_2021dec.onnx")
}

pub fn is_installed() -> bool {
    PLAN.get().map_or_else(|| model_path().exists(), Option::is_some)
}

static PLAN: OnceLock<Option<Model>> = OnceLock::new();

fn plan() -> Option<&'static Model> {
    PLAN.get_or_init(|| {
        let path = model_path();
        if !path.exists() {
            return None;
        }
        Model::load(&path)
    })
    .as_ref()
}

pub fn embed(image: &RgbImage, face: &Face) -> Option<[f32; LENGTH]> {
    let plan = plan()?;
    let aligned = align(image, face)?;

    let mut input = ndarray::Array4::<f32>::zeros((1, 3, SIDE, SIDE));
    for (x, y, pixel) in aligned.enumerate_pixels() {
        for channel in 0..3 {
            input[[0, channel, y as usize, x as usize]] = pixel[channel] as f32;
        }
    }

    let outputs = match plan.run(vec![input.into_dyn().into()]) {
        Ok(outputs) => outputs,
        Err(err) => {
            log::warn!("face recognition failed: {err}");
            return None;
        }
    };
    let features = outputs.first()?;
    if features.len() != LENGTH {
        log::warn!("the face model answered with {} numbers, not {LENGTH}", features.len());
        return None;
    }

    let mut out = [0.0f32; LENGTH];
    for (slot, value) in out.iter_mut().zip(features.iter()) {
        *slot = *value;
    }
    let norm = out.iter().map(|v| v * v).sum::<f32>().sqrt();
    if norm <= f32::EPSILON {
        return None;
    }
    out.iter_mut().for_each(|v| *v /= norm);
    Some(out)
}

pub fn recognise<'a>(
    face: &[f32; LENGTH],
    named: &'a [(String, [f32; LENGTH])],
) -> Option<(&'a str, f32)> {
    let mut best: Vec<(&str, f32)> = Vec::new();
    for (name, embedding) in named {
        let alike = likeness(face, embedding);
        match best.iter_mut().find(|(have, _)| *have == name.as_str()) {
            Some((_, score)) => *score = score.max(alike),
            None => best.push((name.as_str(), alike)),
        }
    }
    best.sort_by(|a, b| b.1.total_cmp(&a.1));
    let (name, score) = *best.first()?;
    let runner_up = best.get(1).map_or(f32::MIN, |(_, score)| *score);
    (score >= SUGGEST && score - runner_up >= MARGIN).then_some((name, score))
}

const GROUP: f32 = 0.45;

pub fn groups(faces: &[[f32; LENGTH]]) -> Vec<Vec<usize>> {
    let mut groups: Vec<Vec<usize>> = Vec::new();
    let mut sums: Vec<[f32; LENGTH]> = Vec::new();
    for (index, face) in faces.iter().enumerate() {
        let best = groups
            .iter()
            .zip(&sums)
            .enumerate()
            .map(|(group, (members, sum))| (group, likeness(sum, face) / members.len() as f32))
            .max_by(|a, b| a.1.total_cmp(&b.1));
        match best {
            Some((group, alike)) if alike >= GROUP => {
                groups[group].push(index);
                for (total, value) in sums[group].iter_mut().zip(face) {
                    *total += value;
                }
            }
            _ => {
                groups.push(vec![index]);
                sums.push(*face);
            }
        }
    }
    groups.sort_by_key(|group| std::cmp::Reverse(group.len()));
    groups
}

pub fn portrait_jpeg(image: &RgbImage, face: &Face) -> Option<Vec<u8>> {
    let aligned = align(image, face)?;
    let mut bytes = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut bytes, 85)
        .encode_image(&aligned)
        .ok()?;
    Some(bytes)
}

pub fn likeness(a: &[f32; LENGTH], b: &[f32; LENGTH]) -> f32 {
    a.iter().zip(b).map(|(a, b)| a * b).sum()
}

pub fn align(image: &RgbImage, face: &Face) -> Option<RgbImage> {
    let from: Vec<[f32; 2]> = face.landmarks.iter().map(|(x, y)| [*x, *y]).collect();
    let [a, b, tx, ty] = similarity(&from, &TEMPLATE)?;

    let det = a * a + b * b;
    if det <= f32::EPSILON {
        return None;
    }
    let (width, height) = (image.width() as f32, image.height() as f32);
    Some(RgbImage::from_fn(SIDE as u32, SIDE as u32, |u, v| {
        let (dx, dy) = (u as f32 + 0.5 - tx, v as f32 + 0.5 - ty);
        let x = (a * dx + b * dy) / det - 0.5;
        let y = (-b * dx + a * dy) / det - 0.5;
        if x < 0.0 || y < 0.0 || x > width - 1.0 || y > height - 1.0 {
            return image::Rgb([0, 0, 0]);
        }
        let (x0, y0) = (x.floor() as u32, y.floor() as u32);
        let (x1, y1) = ((x0 + 1).min(image.width() - 1), (y0 + 1).min(image.height() - 1));
        let (fx, fy) = (x - x0 as f32, y - y0 as f32);
        let mut out = [0u8; 3];
        for (channel, slot) in out.iter_mut().enumerate() {
            let at = |x, y| image.get_pixel(x, y)[channel] as f32;
            let top = at(x0, y0) * (1.0 - fx) + at(x1, y0) * fx;
            let bottom = at(x0, y1) * (1.0 - fx) + at(x1, y1) * fx;
            *slot = (top * (1.0 - fy) + bottom * fy).round().clamp(0.0, 255.0) as u8;
        }
        image::Rgb(out)
    }))
}

fn similarity(from: &[[f32; 2]], to: &[[f32; 2]]) -> Option<[f32; 4]> {
    let n = from.len().min(to.len()) as f32;
    if n < 2.0 {
        return None;
    }
    let mean = |points: &[[f32; 2]]| {
        let (x, y) = points.iter().fold((0.0, 0.0), |(x, y), p| (x + p[0], y + p[1]));
        [x / n, y / n]
    };
    let (mf, mt) = (mean(from), mean(to));

    let (mut dot, mut cross, mut spread) = (0.0f32, 0.0f32, 0.0f32);
    for (f, t) in from.iter().zip(to) {
        let (fx, fy) = (f[0] - mf[0], f[1] - mf[1]);
        let (gx, gy) = (t[0] - mt[0], t[1] - mt[1]);
        dot += fx * gx + fy * gy;
        cross += fx * gy - fy * gx;
        spread += fx * fx + fy * fy;
    }
    if spread <= f32::EPSILON {
        return None;
    }
    let (a, b) = (dot / spread, cross / spread);
    Some([a, b, mt[0] - (a * mf[0] - b * mf[1]), mt[1] - (b * mf[0] + a * mf[1])])
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_transform_found_is_the_one_that_was_applied() {

        let (angle, scale) = (30.0f32.to_radians(), 0.4f32);
        let (a, b) = (scale * angle.cos(), scale * angle.sin());
        let from: Vec<[f32; 2]> = TEMPLATE.iter().map(|p| [p[0] * 3.0 + 400.0, p[1] * 2.5 + 90.0]).collect();
        let to: Vec<[f32; 2]> =
            from.iter().map(|p| [a * p[0] - b * p[1] + 12.0, b * p[0] + a * p[1] - 7.0]).collect();
        let [fa, fb, tx, ty] = similarity(&from, &to).unwrap();
        for (got, want) in [(fa, a), (fb, b), (tx, 12.0), (ty, -7.0)] {
            assert!((got - want).abs() < 1e-2, "{got} against {want}");
        }
    }

    #[test]
    fn a_face_already_on_the_template_comes_out_where_it_was() {

        let mut image = RgbImage::from_pixel(SIDE as u32, SIDE as u32, image::Rgb([0, 0, 0]));
        for p in TEMPLATE {
            image.put_pixel(p[0] as u32, p[1] as u32, image::Rgb([255, 255, 255]));
        }
        let face = Face {
            x: 0.0,
            y: 0.0,
            width: SIDE as f32,
            height: SIDE as f32,
            score: 1.0,
            landmarks: TEMPLATE.map(|p| (p[0], p[1])),
        };
        let aligned = align(&image, &face).unwrap();
        for p in TEMPLATE {
            assert!(aligned.get_pixel(p[0] as u32, p[1] as u32)[0] > 100, "mark at {p:?} moved");
        }
    }

    fn towards(axis: usize, lean: f32) -> [f32; LENGTH] {
        let mut v = [0.0f32; LENGTH];
        v[axis] = 1.0;
        v[LENGTH - 1] = lean;
        let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
        v.map(|x| x / norm)
    }

    #[test]
    fn a_face_is_given_the_name_of_the_nearest_named_face() {
        let named = vec![
            ("Anna".to_string(), towards(0, 0.0)),
            ("Anna".to_string(), towards(1, 0.0)),
            ("Bram".to_string(), towards(2, 0.0)),
        ];

        assert_eq!(recognise(&towards(1, 0.3), &named).map(|(n, _)| n), Some("Anna"));

        assert_eq!(recognise(&towards(3, 0.3), &named), None);

        assert_eq!(recognise(&towards(0, 0.0), &[]), None);
    }

    #[test]
    fn a_tie_between_two_people_is_not_a_name() {
        let mut between = [0.0f32; LENGTH];
        between[0] = 0.7;
        between[2] = 0.7;
        let named = vec![("Anna".to_string(), towards(0, 0.0)), ("Bram".to_string(), towards(2, 0.0))];
        assert_eq!(recognise(&between, &named), None);
    }

    #[test]
    fn faces_of_one_person_are_one_group_and_the_largest_comes_first() {
        let faces = [towards(0, 0.0), towards(1, 0.0), towards(0, 0.3), towards(0, 0.2), towards(1, 0.2)];
        let groups = groups(&faces);
        assert_eq!(groups, vec![vec![0, 2, 3], vec![1, 4]]);
        assert!(super::groups(&[]).is_empty());
    }

    #[test]
    fn likeness_is_a_cosine() {
        let mut a = [0.0f32; LENGTH];
        let mut b = [0.0f32; LENGTH];
        a[0] = 1.0;
        b[1] = 1.0;
        assert_eq!(likeness(&a, &a), 1.0);
        assert_eq!(likeness(&a, &b), 0.0);
    }

    #[test]
    #[ignore]
    fn every_face_in_the_catalog() {
        let Ok(out) = std::env::var("OUT") else { return };
        let threshold: f32 =
            std::env::var("LIKENESS").ok().and_then(|v| v.parse().ok()).unwrap_or(0.45);
        let db = numa_core::paths::data_dir().join("catalog.db");
        let conn = rusqlite::Connection::open_with_flags(
            db,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY,
        )
        .unwrap();
        let paths: Vec<String> = conn
            .prepare("select p.path from analysis a join photos p on p.id = a.photo_id where a.faces > 0 order by p.path")
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .map(Result::unwrap)
            .collect();
        std::fs::create_dir_all(&out).unwrap();

        use rayon::prelude::*;
        let started = std::time::Instant::now();
        let faces: Vec<(String, RgbImage, [f32; LENGTH], f32)> = paths
            .par_iter()
            .flat_map_iter(|path| {
                let edge: u32 = std::env::var("EDGE").ok().and_then(|v| v.parse().ok()).unwrap_or(2400);
                let image = numa_io::raw::load_scaled(std::path::Path::new(path), edge).unwrap();
                let found = super::super::faces::detect(&image).unwrap_or_default();
                found
                    .into_iter()
                    .filter_map(|face| {
                        let embedding = embed(&image, &face)?;
                        Some((path.clone(), align(&image, &face)?, embedding, face.width))
                    })
                    .collect::<Vec<_>>()
            })
            .collect();
        println!("{} faces from {} photographs in {:?}", faces.len(), paths.len(), started.elapsed());

        let mut groups: Vec<Vec<usize>> = Vec::new();
        for (index, face) in faces.iter().enumerate() {
            let best = groups
                .iter()
                .enumerate()
                .map(|(g, members)| {
                    let mean = members.iter().map(|m| likeness(&faces[*m].2, &face.2)).sum::<f32>()
                        / members.len() as f32;
                    (g, mean)
                })
                .max_by(|a, b| a.1.total_cmp(&b.1));
            match best {
                Some((g, score)) if score >= threshold => groups[g].push(index),
                _ => groups.push(vec![index]),
            }
        }
        groups.sort_by_key(|g| std::cmp::Reverse(g.len()));
        for (g, members) in groups.iter().enumerate().filter(|(_, m)| m.len() > 1) {
            let columns = 10usize;
            let rows = members.len().div_ceil(columns);
            let mut sheet = RgbImage::new((columns * SIDE) as u32, (rows * SIDE) as u32);
            for (i, m) in members.iter().enumerate() {
                image::imageops::replace(
                    &mut sheet,
                    &faces[*m].1,
                    ((i % columns) * SIDE) as i64,
                    ((i / columns) * SIDE) as i64,
                );
            }
            sheet.save(format!("{out}/group-{g:02}-{}.png", members.len())).unwrap();
            let names: Vec<&str> = members
                .iter()
                .map(|m| faces[*m].0.rsplit('/').next().unwrap_or(""))
                .collect();
            println!("group {g}: {} faces — {}", members.len(), names.join(" "));
        }

        let big: Vec<&Vec<usize>> = groups.iter().filter(|g| g.len() >= 3).collect();
        for (i, a) in big.iter().enumerate() {
            let row: Vec<String> = big
                .iter()
                .map(|b| {
                    let max = a
                        .iter()
                        .flat_map(|x| b.iter().filter(move |y| *y != x).map(move |y| (x, y)))
                        .map(|(x, y)| likeness(&faces[*x].2, &faces[*y].2))
                        .fold(f32::MIN, f32::max);
                    format!("{max:5.2}")
                })
                .collect();
            println!("group {i:>2} ({:>2}): {}", a.len(), row.join(" "));
        }
        println!(
            "{} groups of one",
            groups.iter().filter(|g| g.len() == 1).count()
        );
    }
}
