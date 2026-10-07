use numa_core::plane::{self, Plane};

const EDGE_FLOOR: f32 = 0.12;

const ENOUGH: usize = 400;

pub(super) fn crowded_by(votes: &[(f32, f32)], agree: f32) -> Option<f32> {
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
    if smooth[peak] < agree * typical {
        return None;
    }
    Some(if angle.abs() >= 0.05 { angle } else { 0.0 })
}

pub(super) fn coherent_edges(luma: &Plane) -> Option<(Vec<(f32, f32, f32, f32, f32)>, (usize, usize))> {
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

    let mut edges: Vec<(f32, f32, f32, f32, f32)> = Vec::new();
    for y in 1..height - 1 {
        for x in 1..width - 1 {
            let at = |dx: isize, dy: isize| {
                smooth.data[(y as isize + dy) as usize * width + (x as isize + dx) as usize]
            };

            let (side, middle) = (3.0, 10.0);
            let gx = (side * at(1, -1) + middle * at(1, 0) + side * at(1, 1))
                - (side * at(-1, -1) + middle * at(-1, 0) + side * at(-1, 1));
            let gy = (side * at(-1, 1) + middle * at(0, 1) + side * at(1, 1))
                - (side * at(-1, -1) + middle * at(0, -1) + side * at(1, -1));
            let strength = gx.hypot(gy);
            if strength <= 0.0 {
                continue;
            }
            edges.push((x as f32, y as f32, gx, gy, strength));
        }
    }
    if edges.is_empty() {
        return Some((edges, (width, height)));
    }
    let mut strengths: Vec<f32> = edges.iter().map(|edge| edge.4).collect();
    let at = ((strengths.len() as f32 * 0.99) as usize).min(strengths.len() - 1);
    let floor = *strengths.select_nth_unstable_by(at, f32::total_cmp).1 * EDGE_FLOOR;
    edges.retain(|edge| edge.4 >= floor);
    Some((edges, (width, height)))
}
