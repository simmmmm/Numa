use super::raw::LensProfile;
use std::sync::OnceLock;

const RADII: [f32; 9] = [
    0.35352114, 0.5, 0.6126761, 0.7070423, 0.7908451, 0.86619717, 0.93521124, 1.0, 1.0605633,
];

const DISTANCE: f32 = 1000.0;

fn database() -> Option<&'static lensfun::Database> {
    static DB: OnceLock<Option<lensfun::Database>> = OnceLock::new();
    DB.get_or_init(|| match lensfun::Database::load_bundled() {
        Ok(db) => Some(db),
        Err(err) => {
            log::warn!("lensfun database unavailable: {err}");
            None
        }
    })
    .as_ref()
}

pub fn profile(
    make: &str,
    camera: &str,
    model: &str,
    focal: f32,
    aperture: f32,
    width: u32,
    height: u32,
) -> Option<LensProfile> {
    if width < 2 || height < 2 || !(focal > 0.0) {
        return None;
    }

    let db = database()?;

    let body = db.find_cameras(Some(make), camera).into_iter().next()?;
    let lens = *db.find_lenses(Some(body), model).first()?;

    let crop = lens.crop_factor;
    let mut modifier =
        lensfun::Modifier::new(lens, focal, crop, width, height, false);

    let bends = modifier.enable_distortion_correction(lens);
    let fringes = modifier.enable_tca_correction(lens);
    let darkens = modifier.enable_vignetting_correction(lens, aperture, DISTANCE);
    if !bends && !fringes && !darkens {
        return None;
    }

    let (cx, cy) = ((width - 1) as f32 / 2.0, (height - 1) as f32 / 2.0);
    let point = |r: f32| (cx + r * cx, cy + r * cy);

    let mut distortion = Vec::new();
    let mut red = Vec::new();
    let mut blue = Vec::new();
    let mut transmission = Vec::new();

    for r in RADII {
        let (x, y) = point(r);

        if bends {
            let mut coords = [0.0f32; 2];
            if !modifier.apply_geometry_distortion(x, y, 1, 1, &mut coords) {
                return None;
            }
            let source = radius(coords[0], coords[1], cx, cy);
            distortion.push((source / radius(x, y, cx, cy) - 1.0) * 100.0);
        }

        if fringes {
            let mut coords = [0.0f32; 6];
            if !modifier.apply_subpixel_distortion(x, y, 1, 1, &mut coords) {
                return None;
            }
            let green = radius(coords[2], coords[3], cx, cy);
            if green <= 0.0 {
                return None;
            }
            red.push(radius(coords[0], coords[1], cx, cy) / green - 1.0);
            blue.push(radius(coords[4], coords[5], cx, cy) / green - 1.0);
        }

        if darkens {

            let mut pixel = [1.0f32; 3];
            if !modifier.apply_color_modification_f32(&mut pixel, x, y, 1, 1, 3) {
                return None;
            }
            transmission.push(1.0 / pixel[1].max(0.05));
        }
    }

    if transmission.iter().any(|t| !(0.1..=1.5).contains(t)) {
        return None;
    }
    if distortion.iter().any(|d| d.abs() > 20.0) {
        return None;
    }
    if red.iter().chain(blue.iter()).any(|c| c.abs() > 0.05) {
        return None;
    }

    if transmission.is_empty() {
        transmission = vec![1.0; RADII.len()];
    }

    Some(LensProfile { radii: RADII.to_vec(), transmission, distortion, red, blue })
}

fn radius(x: f32, y: f32, cx: f32, cy: f32) -> f32 {
    (x - cx).hypot(y - cy)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FRAME: (u32, u32) = (7728, 5152);

    #[test]
    fn the_corner_of_the_frame_is_the_corner_of_the_model() {

        let expected = 1.0 - 0.9412 + 0.1158 + 0.1433;
        let corner = RADII.iter().position(|r| *r == 1.0).expect("a sample on the corner");

        for frame in [FRAME, (6000, 4000), (3000, 2000)] {
            let profile = profile("Fujifilm", "X-T5", "XF27mmF2.8", 27.0, 2.8, frame.0, frame.1)
                .expect("a lens the database holds");
            let difference = (profile.transmission[corner] - expected).abs();
            assert!(
                difference < 0.005,
                "{:?}: corner transmission {} is not the model's {expected}",
                frame,
                profile.transmission[corner]
            );
        }
    }

    #[test]
    fn a_zoom_bends_both_ways_along_its_range() {
        let wide = profile("Fujifilm", "X-T5", "XF16-80mmF4 R OIS WR", 16.0, 4.0, FRAME.0, FRAME.1)
            .expect("the wide end");
        let long = profile("Fujifilm", "X-T5", "XF16-80mmF4 R OIS WR", 80.0, 4.0, FRAME.0, FRAME.1)
            .expect("the long end");

        assert!(wide.distortion[7] < -5.0, "16 mm should barrel: {:?}", wide.distortion);
        assert!(long.distortion[7] > 4.0, "80 mm should pincushion: {:?}", long.distortion);

        for profile in [&wide, &long] {
            assert!(profile.transmission[7] < profile.transmission[0]);
        }
    }

    #[test]
    fn a_lens_nobody_has_measured_is_left_alone() {
        assert!(profile("Fujifilm", "X-T5", "Some Glass 50mm", 50.0, 2.0, FRAME.0, FRAME.1).is_none());

        assert!(profile("Fujifilm", "X-T5", "AF 23/1.4 XF", 23.0, 1.4, FRAME.0, FRAME.1).is_none());
    }
}
