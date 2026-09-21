use super::document::Perspective;
use super::image::source_map;

pub type Guide = [[f32; 2]; 2];

const MOST_ANGLE: f64 = 15.0;

const TRUE: f64 = 3.0e-6;

pub fn to_source(point: [f32; 2], width: f32, height: f32, angle: f32, perspective: Perspective) -> [f32; 2] {
    let map = source_map(width, height, [0.0, 0.0, 1.0, 1.0], angle, perspective);
    let (x, y) = map(point[0] * width - width / 2.0, point[1] * height - height / 2.0);
    [x / width, y / height]
}

pub fn to_frame(point: [f32; 2], width: f32, height: f32, angle: f32, perspective: Perspective) -> Option<[f32; 2]> {
    let (w, h) = (width as f64, height as f64);
    let map = forward(w, h, angle as f64, perspective);
    let (dx, dy) = map(point[0] as f64 * w, point[1] as f64 * h)?;
    Some([((dx + w / 2.0) / w) as f32, ((dy + h / 2.0) / h) as f32])
}

fn forward(width: f64, height: f64, angle: f64, perspective: Perspective) -> impl Fn(f64, f64) -> Option<(f64, f64)> {
    let (half_width, half_height) = (width / 2.0, height / 2.0);
    let (vertical, horizontal) = perspective.coefficients();
    let a = (horizontal as f64 / half_width, vertical as f64 / half_height);
    let stretch = perspective.stretch() as f64;
    let (sin, cos) = angle.to_radians().sin_cos();
    move |sx, sy| {
        let (ux, uy) = (sx - half_width, sy - half_height);

        let depth = 1.0 + a.0 * ux + a.1 * uy;
        if !(1e-6..20.0).contains(&depth) {
            return None;
        }
        let (x, y) = (ux / depth * stretch, uy / depth / stretch);
        Some((x * cos + y * sin, -x * sin + y * cos))
    }
}

pub fn solve(guides: &[Guide], width: f32, height: f32, angle: f32, perspective: Perspective) -> (Perspective, f32) {
    if guides.is_empty() {
        return (perspective, angle);
    }
    let (w, h) = (width as f64, height as f64);
    let lines: Vec<[(f64, f64); 2]> = guides
        .iter()
        .map(|ends| ends.map(|[x, y]| (x as f64 * w, y as f64 * h)))
        .collect();

    let shown = forward(w, h, angle as f64, perspective);
    let upright: Vec<bool> = lines
        .iter()
        .map(|[p, q]| match (shown(p.0, p.1), shown(q.0, q.1)) {
            (Some(p), Some(q)) => (q.1 - p.1).abs() >= (q.0 - p.0).abs(),
            _ => true,
        })
        .collect();

    let lean = |[vertical, horizontal, angle]: [f64; 3]| -> f64 {
        let trial = Perspective { vertical: vertical as f32, horizontal: horizontal as f32, aspect: perspective.aspect };
        let map = forward(w, h, angle, trial);
        lines
            .iter()
            .zip(&upright)
            .map(|([p, q], upright)| match (map(p.0, p.1), map(q.0, q.1)) {
                (Some(p), Some(q)) => {
                    let (dx, dy) = (q.0 - p.0, q.1 - p.1);
                    let off = if *upright { dx } else { dy };
                    off * off / (dx * dx + dy * dy).max(1e-12)
                }
                _ => 1.0,
            })
            .sum()
    };

    let now = [perspective.vertical as f64, perspective.horizontal as f64, angle as f64];
    let verticals = upright.iter().any(|&up| up);
    let levels = upright.iter().any(|&up| !up);

    let found = if guides.len() == 1 {
        let keystone = search(&lean, now, [verticals, levels, false]);
        if lean(keystone) <= TRUE {
            keystone
        } else {
            search(&lean, now, [false, false, true])
        }
    } else {
        search(&lean, now, [verticals, levels, true])
    };

    let [vertical, horizontal, angle] = found;
    (
        Perspective { vertical: vertical as f32, horizontal: horizontal as f32, aspect: perspective.aspect },
        angle as f32,
    )
}

fn search(lean: &impl Fn([f64; 3]) -> f64, start: [f64; 3], free: [bool; 3]) -> [f64; 3] {
    const STEPS: i32 = 5;
    let limits = [100.0, 100.0, MOST_ANGLE];
    let cost = |p: [f64; 3]| {
        lean(p) + 1e-9 * (0..3).map(|i| if free[i] { (p[i] / limits[i]).powi(2) } else { 0.0 }).sum::<f64>()
    };

    let mut centre = [0, 1, 2].map(|i| if free[i] { 0.0 } else { start[i] });
    let mut reach = [0, 1, 2].map(|i| if free[i] { limits[i] } else { 0.0 });
    let span = |i: usize| if free[i] { -STEPS..=STEPS } else { 0..=0 };

    for _ in 0..28 {
        let mut best = (cost(centre), centre);
        for i in span(0) {
            for j in span(1) {
                for k in span(2) {
                    let p = [(0, i), (1, j), (2, k)]
                        .map(|(axis, step)| (centre[axis] + reach[axis] * step as f64 / STEPS as f64).clamp(-limits[axis], limits[axis]));
                    let c = cost(p);
                    if c < best.0 {
                        best = (c, p);
                    }
                }
            }
        }
        centre = best.1;
        reach = reach.map(|r| r * 2.0 / STEPS as f64);
    }
    centre
}

#[cfg(test)]
mod tests {
    use super::*;

    const W: f32 = 3000.0;
    const H: f32 = 2000.0;

    fn degrees_off(guide: Guide, upright: bool, angle: f32, perspective: Perspective) -> f32 {
        let [p, q] = guide.map(|end| to_frame(end, W, H, angle, perspective).unwrap());
        let (dx, dy) = ((q[0] - p[0]) * W, (q[1] - p[1]) * H);
        let (off, along) = if upright { (dx, dy) } else { (dy, dx) };
        (off / along).atan().to_degrees().abs()
    }

    fn keystoned(lines: &[Guide], angle: f32, perspective: Perspective) -> Vec<Guide> {
        lines.iter().map(|line| line.map(|end| to_source(end, W, H, angle, perspective))).collect()
    }

    #[test]
    fn the_inverse_undoes_source_map() {
        let perspective = Perspective { vertical: -35.0, horizontal: 20.0, aspect: 15.0 };
        let angle = 4.0;
        for x in [0.05, 0.3, 0.5, 0.71, 0.95] {
            for y in [0.02, 0.4, 0.5, 0.88] {
                let source = to_source([x, y], W, H, angle, perspective);
                let back = to_frame(source, W, H, angle, perspective).unwrap();
                assert!((back[0] - x).abs() * W < 0.01 && (back[1] - y).abs() * H < 0.01, "{x},{y} came back as {back:?}");
            }
        }
    }

    #[test]
    fn four_guides_recover_the_correction() {
        let truth = Perspective { vertical: -30.0, horizontal: 18.0, aspect: 0.0 };
        let angle = 3.0;
        let lines = [
            [[0.2, 0.2], [0.2, 0.8]],
            [[0.85, 0.15], [0.85, 0.7]],
            [[0.2, 0.25], [0.8, 0.25]],
            [[0.1, 0.8], [0.7, 0.8]],
        ];
        let guides = keystoned(&lines, angle, truth);

        let (found, found_angle) = solve(&guides, W, H, 0.0, Perspective::default());
        assert!((found.vertical - truth.vertical).abs() < 0.2, "{found:?}");
        assert!((found.horizontal - truth.horizontal).abs() < 0.2, "{found:?}");
        assert!((found_angle - angle).abs() < 0.02, "{found_angle}");
        for (index, guide) in guides.iter().enumerate() {
            let off = degrees_off(*guide, index < 2, found_angle, found);
            assert!(off < 0.02, "guide {index} still {off}° off");
        }

        let (again, again_angle) = solve(&guides, W, H, -6.0, Perspective { vertical: 40.0, horizontal: -10.0, aspect: 0.0 });
        assert!((again.vertical - truth.vertical).abs() < 0.2 && (again_angle - angle).abs() < 0.02, "{again:?} {again_angle}");
    }

    #[test]
    fn two_uprights_set_the_keystone_and_the_angle() {
        let truth = Perspective { vertical: 25.0, ..Default::default() };
        let guides = keystoned(&[[[0.15, 0.1], [0.15, 0.9]], [[0.7, 0.3], [0.7, 0.9]]], -2.5, truth);
        let (found, angle) = solve(&guides, W, H, 0.0, Perspective::default());
        assert!((found.vertical - 25.0).abs() < 0.2 && (angle + 2.5).abs() < 0.02, "{found:?} {angle}");
        assert_eq!(found.horizontal, 0.0, "no level guide, so the horizontal keystone is left alone");
    }

    #[test]
    fn one_guide_sets_its_keystone_and_nothing_else() {
        let truth = Perspective { vertical: -25.0, ..Default::default() };
        let guides = keystoned(&[[[0.1, 0.2], [0.1, 0.9]]], 0.0, truth);
        let (found, angle) = solve(&guides, W, H, 0.0, Perspective { aspect: 10.0, ..Default::default() });
        assert!((found.vertical + 25.0).abs() < 0.3, "{found:?}");
        assert_eq!((found.horizontal, found.aspect, angle), (0.0, 10.0, 0.0));
    }

    #[test]
    fn one_guide_through_the_middle_is_a_tilt() {

        let guides = keystoned(&[[[0.5, 0.1], [0.5, 0.9]]], 5.0, Perspective::default());
        let (found, angle) = solve(&guides, W, H, 0.0, Perspective::default());
        assert!(found.is_identity(), "{found:?}");
        assert!((angle - 5.0).abs() < 0.02, "{angle}");
    }

    #[test]
    fn one_upright_and_one_level_are_squared() {
        let truth = Perspective { vertical: 20.0, horizontal: -15.0, aspect: 0.0 };
        let lines = [[[0.2, 0.2], [0.2, 0.8]], [[0.3, 0.85], [0.9, 0.85]]];
        let guides = keystoned(&lines, 1.0, truth);
        let (found, angle) = solve(&guides, W, H, 0.0, Perspective::default());
        assert!(degrees_off(guides[0], true, angle, found) < 0.02, "{found:?} {angle}");
        assert!(degrees_off(guides[1], false, angle, found) < 0.02, "{found:?} {angle}");
    }
}
