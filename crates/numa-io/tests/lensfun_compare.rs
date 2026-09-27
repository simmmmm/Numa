use numa_io::lensfun::{profile_in, Database};
use std::path::Path;

const RADII: [f32; 9] = [
    0.35352114, 0.5, 0.6126761, 0.7070423, 0.7908451, 0.86619717, 0.93521124, 1.0, 1.0605633,
];

fn dir() -> &'static Path {
    Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/../../data/lensfun"))
}

struct Old {
    model: String,
    crop: f32,
    distortion: Vec<f32>,
    red: Vec<f32>,
    blue: Vec<f32>,
    transmission: Vec<f32>,
}

fn old(db: &lensfun::Database, make: &str, camera: &str, model: &str, focal: f32, aperture: f32, w: u32, h: u32) -> Option<Old> {
    let body = db.find_cameras(Some(make), camera).into_iter().next()?;
    let lens = *db.find_lenses(Some(body), model).first()?;
    let mut modifier = lensfun::Modifier::new(lens, focal, lens.crop_factor, w, h, false);
    let bends = modifier.enable_distortion_correction(lens);
    let fringes = modifier.enable_tca_correction(lens);
    let darkens = modifier.enable_vignetting_correction(lens, aperture, 1000.0);
    if !bends && !fringes && !darkens {
        return None;
    }
    let (cx, cy) = ((w - 1) as f32 / 2.0, (h - 1) as f32 / 2.0);
    let radius = |x: f32, y: f32| (x - cx).hypot(y - cy);
    let mut out = Old { model: lens.model.clone(), crop: lens.crop_factor, distortion: vec![], red: vec![], blue: vec![], transmission: vec![] };
    for r in RADII {
        let (x, y) = (cx + r * cx, cy + r * cy);
        if bends {
            let mut c = [0.0f32; 2];
            modifier.apply_geometry_distortion(x, y, 1, 1, &mut c);
            out.distortion.push((radius(c[0], c[1]) / radius(x, y) - 1.0) * 100.0);
        }
        if fringes {
            let mut c = [0.0f32; 6];
            modifier.apply_subpixel_distortion(x, y, 1, 1, &mut c);
            let green = radius(c[2], c[3]);
            out.red.push(radius(c[0], c[1]) / green - 1.0);
            out.blue.push(radius(c[4], c[5]) / green - 1.0);
        }
        if darkens {
            let mut pixel = [1.0f32; 3];
            modifier.apply_color_modification_f32(&mut pixel, x, y, 1, 1, 3);
            out.transmission.push(1.0 / pixel[1].max(0.05));
        }
    }

    let wild = out.transmission.iter().any(|t| !(0.1..=1.5).contains(t))
        || out.distortion.iter().any(|d| d.abs() > 20.0)
        || out.red.iter().chain(&out.blue).any(|c| c.abs() > 0.05);
    (!wild).then_some(out)
}

fn identity(distortion: &[f32], red: &[f32], blue: &[f32], transmission: &[f32]) -> bool {
    distortion.iter().chain(red).chain(blue).all(|d| d.abs() < 1e-6) && transmission.iter().all(|t| (t - 1.0).abs() < 1e-6)
}

fn worst(a: &[f32], b: &[f32]) -> f32 {
    if a.is_empty() || b.is_empty() {
        return 0.0;
    }
    a.iter().zip(b).map(|(x, y)| (x - y).abs()).fold(0.0, f32::max)
}

#[test]
fn numa_agrees_with_lensfun_rs() {
    let theirs = lensfun::Database::load_dir(dir()).expect("lensfun-rs reads the database");
    let ours = Database::load_dir(dir()).expect("Numa reads the database");

    let (mut cases, mut other_lens, mut presence, mut both_valid) = (0, 0, 0, 0);
    let (mut dist, mut tca, mut vig): (Vec<(f32, String)>, Vec<(f32, String)>, Vec<(f32, String)>) = Default::default();
    let verbose = std::env::var_os("VERBOSE").is_some();

    for lens in &ours.lenses {
        let Some(body) = ours
            .cameras
            .iter()
            .find(|c| lens.mounts.contains(&c.mount) && c.crop >= lens.crop && c.crop < lens.crop * 1.1)
        else {
            continue;
        };
        let mut focals: Vec<f32> = lens.distortion.iter().map(|d| d.0).chain(lens.tca.iter().map(|t| t.0)).collect();
        focals.extend(lens.vignetting.iter().map(|v| v.0[0]));
        focals.sort_by(f32::total_cmp);
        focals.dedup();
        let between: Vec<f32> = focals.windows(2).map(|w| w[0] * 0.6 + w[1] * 0.4).collect();
        focals.extend(between);
        for focal in focals.into_iter().filter(|f| *f > 0.0) {
            for aperture in [1.4, 2.8, 4.0, 5.0, 8.0, 11.0] {
                for (w, h) in [(6000, 4000), (4000, 3000)] {
                    let a = old(&theirs, &body.maker, &body.model, &lens.model, focal, aperture, w, h);
                    let b = profile_in(&ours, &body.maker, &body.model, &lens.model, focal, aperture, w, h);
                    let ours_lens = ours.find_lenses(body, &lens.model).first().map(|l| (l.model.clone(), l.crop));
                    cases += 1;
                    let label = format!("{} / {} @ {focal} f/{aperture} {w}x{h}", body.model, lens.model);

                    let a = a.filter(|a| !identity(&a.distortion, &a.red, &a.blue, &a.transmission));
                    let b = b.filter(|b| !identity(&b.distortion, &b.red, &b.blue, &b.transmission));
                    let Some(a) = a else {
                        let unknown_to_them = theirs
                            .find_cameras(Some(&body.maker), &body.model)
                            .first()
                            .is_some_and(|c| theirs.find_lenses(Some(c), &lens.model).is_empty());
                        if b.is_some() && unknown_to_them {

                            both_valid += 1;
                        } else if b.is_some() {
                            presence += 1;
                            if verbose {
                                println!("only Numa: {label}");
                            }
                        }
                        continue;
                    };
                    if ours_lens.as_ref() != Some(&(a.model.clone(), a.crop)) {

                        let off = |crop: f32| (body.crop / crop).ln().abs();
                        match &ours_lens {
                            Some((model, crop)) if model.eq_ignore_ascii_case(&a.model) && off(*crop) <= off(a.crop) => both_valid += 1,

                            Some((model, _)) if model.eq_ignore_ascii_case(&lens.model) && !a.model.eq_ignore_ascii_case(&lens.model) => both_valid += 1,
                            _ => {
                                other_lens += 1;
                                if verbose {
                                    println!("lens: {label}: lensfun-rs {} @{} / Numa {:?}", a.model, a.crop, ours_lens);
                                }
                            }
                        }
                        continue;
                    }
                    let Some(b) = b else {
                        {
                            presence += 1;
                            if verbose {
                                println!("only lensfun-rs: {label}");
                            }
                        }
                        continue;
                    };
                    let note = |all: &mut Vec<(f32, String)>, err: f32, what: &str| all.push((err, format!("{label} {what}")));
                    note(&mut dist, worst(&a.distortion, &b.distortion), "distortion");
                    let t = worst(&a.red, &b.red).max(worst(&a.blue, &b.blue));
                    note(&mut tca, t, "tca");
                    let v = if a.transmission.is_empty() { 0.0 } else { worst(&a.transmission, &b.transmission) };
                    note(&mut vig, v, "vignetting");
                }
            }
        }
    }
    println!("{cases} cases; a different but valid choice in {both_valid}; another lens chosen in {other_lens}; one side without a profile in {presence}");
    for (name, all) in [("distortion (percentage points)", &mut dist), ("tca", &mut tca), ("transmission", &mut vig)] {
        all.sort_by(|a, b| b.0.total_cmp(&a.0));
        let over = |t: f32| all.iter().filter(|e| e.0 > t).count();
        println!("{name}: worst {:.2e} at {}; over 1e-3: {}, over 1e-4: {}", all[0].0, all[0].1, over(1e-3), over(1e-4));
        if verbose {
            for e in all.iter().take(15) {
                println!("   {:.2e} {}", e.0, e.1);
            }
        }
    }
    assert_eq!((other_lens, presence), (0, 0), "lens choices or profiles that differ");
    assert!(dist[0].0 < 1e-4, "distortion off by {} percentage points", dist[0].0);
    assert!(tca[0].0 < 1e-6, "TCA off by {}", tca[0].0);
    assert!(vig[0].0 < 1e-5, "transmission off by {}", vig[0].0);
}
