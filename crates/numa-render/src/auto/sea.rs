use numa_core::plane::Plane;

pub(crate) const IN_THE_WAY: [u16; 12] = [16, 68, 29, 46, 13, 34, 94, 12, 126, 76, 103, 140];

const DECIDED: f32 = 0.3;
const WITHIN: usize = 3;

const SPAN: f32 = 0.4;
const ON_THE_LINE: f32 = 0.6;
const STRAIGHT: f32 = 0.0025;

const FLOOR: f32 = 0.05;

pub struct Planes<'a> {
    pub sky: &'a Plane,
    pub sea: &'a Plane,

    pub other_water: &'a Plane,
    pub in_the_way: &'a Plane,

    pub guide: &'a Plane,
}

#[derive(Debug, Clone, Copy)]
pub struct Horizon {
    pub line: [f32; 4],
    pub sigma: f32,
    pub sea: bool,
}

pub fn horizon(planes: &Planes) -> Option<Horizon> {
    let crossings = crossings(planes);
    let (cells_wide, guide) = (planes.sky.width, planes.guide);
    if (crossings.len() as f32) < cells_wide as f32 * SPAN {
        return None;
    }
    let log: Vec<f32> = guide.data.iter().map(|value| (value + 1.0 / 255.0).ln()).collect();
    let points = refined(&crossings, &log, guide, planes.sky.height, cells_wide);
    let (normal, offset, inliers) = fit(&points, guide.height as f32 * STRAIGHT * 2.0)?;

    let on: Vec<&Point> = inliers.iter().map(|&at| &points[at]).collect();
    let (left, right) = on.iter().fold((f32::MAX, f32::MIN), |(l, r), p| (l.min(p.x), r.max(p.x)));
    let rms = (on.iter().map(|p| (normal[0] * p.x + normal[1] * p.y - offset).powi(2)).sum::<f32>() / on.len() as f32).sqrt();
    if right - left < guide.width as f32 * SPAN
        || (on.len() as f32) < crossings.len() as f32 * ON_THE_LINE
        || rms > guide.height as f32 * STRAIGHT
        || normal[1].abs() < 1e-3
    {
        return None;
    }

    let at = |x: f32| (offset - normal[0] * x) / normal[1] / guide.height as f32;
    let mean = on.iter().map(|p| p.x).sum::<f32>() / on.len() as f32;
    let spread = on.iter().map(|p| (p.x - mean).powi(2)).sum::<f32>().sqrt().max(1.0);
    let sea = on.iter().map(|p| p.sea).sum::<f32>() >= on.len() as f32 / 2.0;
    Some(Horizon {
        line: [0.0, at(0.0), 1.0, at(guide.width as f32)],
        sigma: (rms / spread).atan().to_degrees().max(FLOOR),
        sea,
    })
}

struct Crossing {
    column: usize,

    row: f32,
    steep: f32,

    sea: f32,
}

fn crossings(planes: &Planes) -> Vec<Crossing> {
    let (width, height) = (planes.sky.width, planes.sky.height);
    let at = |plane: &Plane, row: usize, column: usize| plane.data[row * width + column];
    let water = |row, column| at(planes.sea, row, column) + at(planes.other_water, row, column);
    let lean = |row, column| at(planes.sky, row, column) - water(row, column);
    (0..width)
        .filter_map(|column| {

            let mut best: Option<(usize, f32, f32)> = None;
            for top in 0..height {
                if lean(top, column) <= DECIDED {
                    continue;
                }
                let Some(bottom) = (top + 1..(top + WITHIN + 1).min(height)).find(|&row| lean(row, column) < -DECIDED) else {
                    continue;
                };
                let steep = (lean(top, column) - lean(bottom, column)) / (bottom - top) as f32;
                let cross = (top..bottom).find(|&row| lean(row + 1, column) < 0.0).unwrap_or(top);
                let (above, below) = (lean(cross, column), lean(cross + 1, column));
                let row = cross as f32 + above / (above - below).max(f32::EPSILON);
                if best.is_none_or(|(_, _, was)| steep > was) {
                    best = Some((bottom, row, steep));
                }
            }
            let (bottom, row, steep) = best?;

            let near = row.round() as isize;
            let blocked = (near - 2..=near + 2)
                .filter(|&row| (0..height as isize).contains(&row))
                .any(|row| at(planes.in_the_way, row as usize, column) > 0.5);
            if blocked {
                return None;
            }
            let below = bottom.min(height - 1);
            let sea = (at(planes.sea, below, column) >= at(planes.other_water, below, column)) as u8 as f32;
            Some(Crossing { column, row, steep, sea })
        })
        .collect()
}

struct Point {
    x: f32,
    y: f32,
    weight: f32,
    sea: f32,
}

fn refined(crossings: &[Crossing], log: &[f32], guide: &Plane, cells_high: usize, cells_wide: usize) -> Vec<Point> {
    let (width, height) = (guide.width, guide.height);
    let cell = (width as f32 / cells_wide as f32, height as f32 / cells_high as f32);
    let reach = (2.0 * cell.1).ceil() as isize;

    let columns: Vec<(isize, Vec<f32>)> = crossings
        .iter()
        .map(|crossing| {
            let from = (crossing.column as f32 * cell.0) as usize;
            let to = (((crossing.column + 1) as f32 * cell.0) as usize).clamp(from + 1, width);
            let centre = ((crossing.row + 0.5) * cell.1) as isize;
            let first = (centre - reach).max(1);
            let rows = (first..=(centre + reach).min(height as isize - 2))
                .map(|y| {
                    let y = y as usize;
                    let change: f32 = (from..to).map(|x| log[(y + 1) * width + x] - log[(y - 1) * width + x]).sum();
                    (change / (to - from) as f32 / 2.0).abs()
                })
                .collect();
            (first, rows)
        })
        .collect();

    let path = continuous(crossings, &columns, cell.0);

    let ends = cells_wide as f32 * 0.1;
    crossings
        .iter()
        .zip(&columns)
        .zip(path)
        .filter_map(|((crossing, (first, edge)), chosen)| {
            let peak = edge[chosen];
            let mut sorted = edge.clone();
            sorted.sort_by(f32::total_cmp);
            if peak < 0.004 || peak < 3.0 * sorted[sorted.len() / 2] {
                return None;
            }

            let (before, after) = (edge.get(chosen.wrapping_sub(1)).copied(), edge.get(chosen + 1).copied());
            let shift = match (before, after) {
                (Some(b), Some(a)) if b - 2.0 * peak + a < 0.0 => (0.5 * (b - a) / (b - 2.0 * peak + a)).clamp(-0.5, 0.5),
                _ => 0.0,
            };
            let at_end = (crossing.column as f32) < ends || (crossing.column as f32) >= cells_wide as f32 - ends;
            Some(Point {
                x: (crossing.column as f32 + 0.5) * cell.0,

                y: (*first + chosen as isize) as f32 + shift,
                weight: crossing.steep * if at_end { 0.5 } else { 1.0 },
                sea: crossing.sea,
            })
        })
        .collect()
}

fn continuous(crossings: &[Crossing], columns: &[(isize, Vec<f32>)], cell_width: f32) -> Vec<usize> {
    const PENALTY: f32 = 0.01;
    let normal = |edge: &[f32]| {
        let most = edge.iter().copied().fold(f32::EPSILON, f32::max);
        edge.iter().map(|value| value / most).collect::<Vec<f32>>()
    };
    let mut score: Vec<f32> = columns.first().map(|(_, edge)| normal(edge)).unwrap_or_default();
    let mut from: Vec<Vec<usize>> = vec![vec![]];
    for at in 1..columns.len() {
        let (was_first, _) = columns[at - 1];
        let (first, ref edge) = columns[at];
        let slack = 0.3 * cell_width * (crossings[at].column - crossings[at - 1].column) as f32;
        let gain = normal(edge);
        let mut next = vec![0.0; edge.len()];
        let mut back = vec![0; edge.len()];
        for (row, value) in gain.iter().enumerate() {
            let (best, score) = score
                .iter()
                .enumerate()
                .map(|(was, total)| {
                    let jump = ((first + row as isize) - (was_first + was as isize)).abs() as f32;
                    (was, total - PENALTY * (jump - slack).max(0.0).powi(2))
                })
                .max_by(|a, b| a.1.total_cmp(&b.1))
                .unwrap_or((0, 0.0));
            next[row] = score + value;
            back[row] = best;
        }
        score = next;
        from.push(back);
    }
    let mut row = (0..score.len()).max_by(|a, b| score[*a].total_cmp(&score[*b])).unwrap_or(0);
    let mut path = vec![0; columns.len()];
    for at in (0..columns.len()).rev() {
        path[at] = row;
        if at > 0 {
            row = from[at][row];
        }
    }
    path
}

fn fit(points: &[Point], near: f32) -> Option<([f32; 2], f32, Vec<usize>)> {
    if points.len() < 2 {
        return None;
    }
    let step = (points.len() as f32 / 64.0).max(1.0);
    let tried: Vec<usize> = (0..points.len().min(64)).map(|at| (at as f32 * step) as usize).collect();
    let line_through = |a: &Point, b: &Point| {
        let (dx, dy) = (b.x - a.x, b.y - a.y);
        let length = dx.hypot(dy).max(f32::EPSILON);
        let normal = [-dy / length, dx / length];
        (normal, normal[0] * a.x + normal[1] * a.y)
    };
    let support = |(normal, offset): ([f32; 2], f32)| -> f32 {
        points.iter().filter(|p| (normal[0] * p.x + normal[1] * p.y - offset).abs() <= near).map(|p| p.weight).sum()
    };
    let mut best = None;
    let mut most = 0.0;
    for (index, &a) in tried.iter().enumerate() {
        for &b in &tried[index + 1..] {
            let line = line_through(&points[a], &points[b]);
            let weight = support(line);
            if weight > most {
                (most, best) = (weight, Some(line));
            }
        }
    }
    let (mut normal, mut offset) = best?;

    for _ in 0..10 {
        let residuals: Vec<f32> = points.iter().map(|p| normal[0] * p.x + normal[1] * p.y - offset).collect();
        let mut absolute: Vec<f32> = residuals.iter().map(|r| r.abs()).collect();
        absolute.sort_by(f32::total_cmp);
        let scale = (4.685 * 1.4826 * absolute[absolute.len() / 2]).max(near);
        let weights: Vec<f32> = points
            .iter()
            .zip(&residuals)
            .map(|(p, r)| p.weight * (1.0 - (r / scale).powi(2)).max(0.0).powi(2))
            .collect();
        let total: f32 = weights.iter().sum();
        if total <= 0.0 {
            return None;
        }
        let mx = points.iter().zip(&weights).map(|(p, w)| p.x * w).sum::<f32>() / total;
        let my = points.iter().zip(&weights).map(|(p, w)| p.y * w).sum::<f32>() / total;
        let (mut xx, mut yy, mut xy) = (0.0f32, 0.0f32, 0.0f32);
        for (p, w) in points.iter().zip(&weights) {
            let (dx, dy) = (p.x - mx, p.y - my);
            xx += w * dx * dx;
            yy += w * dy * dy;
            xy += w * dx * dy;
        }

        let angle = 0.5 * (2.0 * xy).atan2(xx - yy);
        normal = [-angle.sin(), angle.cos()];
        offset = normal[0] * mx + normal[1] * my;
    }
    let inliers = (0..points.len())
        .filter(|&at| (normal[0] * points[at].x + normal[1] * points[at].y - offset).abs() <= near)
        .collect();
    Some((normal, offset, inliers))
}
