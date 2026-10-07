use numa_core::document::Perspective;
use numa_core::image::{into_crop, levelled};

use super::scene::Scene;

const MOST: f32 = 0.05;

const SUBJECTS: [u16; 2] = [12, 126];

const JOINTS: [(f32, f32); 4] = [(0.0, 1.0), (1.6, 2.7), (4.7, 5.7), (6.8, 8.1)];

const UNKNOWN_POSTURE: f32 = JOINTS[1].0;

const STANDING: f32 = 6.5;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Kept {
    pub rect: [f32; 4],
    pub share: f32,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Cut {
    Face,
    Subject,
    Horizon,
    TooMuch,
}

pub fn level(scene: &Scene, rect: [f32; 4], from: f32, to: f32, perspective: Perspective) -> Result<Kept, (Cut, Kept)> {
    let (width, height) = (scene.size[0] as f32, scene.size[1] as f32);
    let area = |r: [f32; 4]| r[2] * r[3] / (rect[2] * rect[3]);
    let base = levelled(rect, from, to, perspective, width, height);
    let largest = Kept { rect: base, share: area(base) };
    if largest.share < 1.0 - MOST {
        return Err((Cut::TooMuch, largest));
    }
    let Some(cut) = cuts(scene, (rect, from), (base, to), perspective, None) else { return Ok(largest) };

    let mut best: Option<Kept> = None;
    for step in 1..=10 {
        let scale = 1.0 - step as f32 * 0.0025;
        let share = largest.share * scale * scale;
        if share < 1.0 - MOST {
            break;
        }
        let [x, y, w, h] = base;
        let (small_w, small_h) = (w * scale, h * scale);
        for i in 0..=6 {
            for j in 0..=6 {
                let left = x + (w - small_w) * i as f32 / 6.0;
                let top = y + (h - small_h) * j as f32 / 6.0;
                let candidate = [left, top, small_w, small_h];

                if cuts(scene, (rect, from), (candidate, to), perspective, None).is_none()
                    && best.is_none_or(|kept| distance(candidate, base) < distance(kept.rect, base))
                {
                    best = Some(Kept { rect: candidate, share });
                }
            }
        }
        if best.is_some() {
            return Ok(best.expect("just found"));
        }
    }
    Err((cut, largest))
}

fn distance(a: [f32; 4], b: [f32; 4]) -> f32 {
    (a[0] + a[2] / 2.0 - b[0] - b[2] / 2.0).hypot(a[1] + a[3] / 2.0 - b[1] - b[3] / 2.0)
}

pub(super) struct Crowd {
    pub(super) parts: Vec<Vec<(usize, usize, bool)>>,

    pub(super) found: Vec<bool>,
    pub(super) subject: Vec<usize>,
}

pub(super) fn cuts(scene: &Scene, old: ([f32; 4], f32), new: ([f32; 4], f32), perspective: Perspective, crowd: Option<&Crowd>) -> Option<Cut> {
    let through = crowd.is_some();
    let (width, height) = (scene.size[0] as f32, scene.size[1] as f32);
    let at = |frame: ([f32; 4], f32), point: [f32; 2]| into_crop(width, height, frame.0, frame.1, perspective, point);
    let inside = |p: [f32; 2]| (0.0..=1.0).contains(&p[0]) && (0.0..=1.0).contains(&p[1]);

    for &[x, y, w, h] in &scene.faces {
        let corners = [[x, y], [x + w, y], [x, y + h], [x + w, y + h]];
        let was = corners.iter().filter(|c| inside(at(old, **c))).count();
        let now = corners.iter().filter(|c| inside(at(new, **c))).count();
        if now < was {
            return Some(Cut::Face);
        }
    }

    let (cells_wide, cells_high, classes) = &scene.classes;
    let (cw, ch) = (*cells_wide, *cells_high);
    let class = |x: usize, y: usize| classes[y * cw + x] as u16;
    let centre = |x: usize, y: usize| [(x as f32 + 0.5) / cw as f32, (y as f32 + 0.5) / ch as f32];

    let depth = |frame: ([f32; 4], f32), p: [f32; 2]| {
        (p[0].min(1.0 - p[0]) * frame.0[2] * cw as f32).min(p[1].min(1.0 - p[1]) * frame.0[3] * ch as f32)
    };
    let joined;
    let parts = match crowd {
        Some(crowd) => &crowd.parts,
        None => {
            joined = subjects(cw, ch, &class);
            &joined
        }
    };
    let largest = parts.iter().map(Vec::len).max().unwrap_or(0);

    let subject = |i: usize| crowd.map_or(parts[i].len() == largest, |crowd| crowd.subject.contains(&i));
    for (i, part) in parts.iter().enumerate() {
        let cut_already = part.iter().filter(|cell| !cell.2).any(|&(x, y, _)| {
            let p = at(old, centre(x, y));
            !inside(p) || depth(old, p) < 0.75
        });

        let person = through && part.iter().any(|&(x, y, added)| !added && class(x, y) == 12);

        let feet = part.iter().filter(|cell| !cell.2).map(|&(x, y, _)| centre(x, y)).max_by(|a, b| a[1].total_cmp(&b[1]));
        let feet = feet.filter(|p| inside(at(old, *p)) && depth(old, at(old, *p)) >= 0.75).map(|p| p[1] + 0.5 / ch as f32);

        let joint_rule = |p: [f32; 2]| {
            let now = at(new, p);
            if !through || !(0.0..=1.0).contains(&now[0]) || now[1] <= 1.0 {
                return None;
            }
            let &[fx, fy, fw, fh] = scene.faces.iter().find(|&&[fx, fy, fw, fh]| p[1] > fy + fh && (p[0] - fx - fw / 2.0).abs() <= 2.0 * fw)?;
            let (column, chin) = (fx + fw / 2.0, fy + fh);
            let standing = feet.is_some_and(|feet| (feet - chin) / fh >= STANDING);
            let edge = |below: f32| at(new, [column, chin + below * fh])[1];
            Some(edge(0.0) < 1.0 && (standing || edge(UNKNOWN_POSTURE) > 1.0) && JOINTS.iter().all(|&(a, b)| !(edge(a) < 1.0 && edge(b) > 1.0)))
        };

        if (cut_already || person) && !subject(i) && part.iter().all(|&(x, y, _)| !inside(at(new, centre(x, y)))) {
            continue;
        }
        for &(x, y, _) in part {
            let (was, now) = (at(old, centre(x, y)), at(new, centre(x, y)));
            let lost = match joint_rule(centre(x, y)) {
                Some(between) => !between,
                None => !cut_already || depth(old, was) > 1.5,
            };
            if inside(was) && !inside(now) && lost {
                return Some(Cut::Subject);
            }
        }
    }

    if let Some([u0, v0, u1, v1]) = scene.horizon() {
        let row = |frame| {
            let (a, b) = (at(frame, [u0, v0]), at(frame, [u1, v1]));
            let t = (0.5 - a[0]) / (b[0] - a[0]);
            a[1] + (b[1] - a[1]) * t
        };
        let near_an_edge = |v: f32| (0.0..0.03).contains(&v) || (0.97..=1.0).contains(&v);
        if near_an_edge(row(new)) && !near_an_edge(row(old)) {
            return Some(Cut::Horizon);
        }
    }
    None
}

pub(super) fn subjects(width: usize, height: usize, class: &dyn Fn(usize, usize) -> u16) -> Vec<Vec<(usize, usize, bool)>> {
    let mut what = vec![0u8; width * height];
    for y in 0..height {
        for x in 0..width {
            match class(x, y) {
                12 => what[y * width + x] = 1,

                126 => {
                    what[y * width + x] = 2;
                    for (dx, dy) in (-2isize..=2).flat_map(|dy| (-2isize..=2).map(move |dx| (dx, dy))) {
                        let (nx, ny) = (x as isize + dx, y as isize + dy);
                        if !(0..width as isize).contains(&nx) || !(0..height as isize).contains(&ny) {
                            continue;
                        }
                        let (nx, ny) = (nx as usize, ny as usize);
                        if what[ny * width + nx] == 0 && !SUBJECTS.contains(&class(nx, ny)) {
                            what[ny * width + nx] = 3;
                        }
                    }
                }
                _ => {}
            }
        }
    }
    let mut parts = Vec::new();
    for start in 0..what.len() {
        if what[start] == 0 {
            continue;
        }
        let mut part = Vec::new();
        let mut stack = vec![start];
        let kind = what[start];
        while let Some(at) = stack.pop() {
            if what[at] == 0 {
                continue;
            }
            part.push((at % width, at / width, what[at] == 3));
            what[at] = 0;
            stack.extend(neighbours(at % width, at / width, width, height).map(|(x, y)| y * width + x).filter(|&n| what[n] != 0 && (what[n] == 1) == (kind == 1)));
        }
        parts.push(part);
    }
    parts
}

pub(super) fn people(scene: &Scene) -> Crowd {
    let (cw, ch, classes) = &scene.classes;
    let (cw, ch) = (*cw, *ch);
    let mut crowd = Crowd { parts: Vec::new(), found: Vec::new(), subject: Vec::new() };
    if classes.is_empty() {
        return crowd;
    }
    let class = |x: usize, y: usize| classes[y * cw + x] as u16;
    let held = |b: &[f32; 4], [u, v]: [f32; 2]| (b[0]..=b[0] + b[2]).contains(&u) && (b[1]..=b[1] + b[3]).contains(&v);
    let area = |b: &[f32; 4]| b[2] * b[3];
    let shared = |a: &[f32; 4], b: &[f32; 4]| ((a[0] + a[2]).min(b[0] + b[2]) - a[0].max(b[0])).max(0.0) * ((a[1] + a[3]).min(b[1] + b[3]) - a[1].max(b[1])).max(0.0);
    let boxes: Vec<[f32; 4]> = scene.objects.iter().filter(|(class, _)| *class == 0).map(|(_, b)| *b).collect();
    let faced = |b: &[f32; 4]| scene.faces.iter().any(|[x, y, w, h]| held(b, [x + w / 2.0, y + h / 2.0]));

    let mut figures: Vec<[f32; 4]> = boxes
        .iter()
        .filter(|a| !boxes.iter().any(|b| area(b) > area(a) && shared(a, b) >= 0.7 * area(a) && area(a) >= 0.2 * area(b)) || faced(a))
        .copied()
        .collect();
    for &[x, y, w, h] in &scene.faces {
        if !figures.iter().any(|b| held(b, [x + w / 2.0, y + h / 2.0])) {

            figures.push([x - w, y - 0.3 * h, 3.0 * w, 9.3 * h]);
        }
    }
    let centre = |x: usize, y: usize| [(x as f32 + 0.5) / cw as f32, (y as f32 + 0.5) / ch as f32];
    for part in subjects(cw, ch, &class) {
        let person = part.iter().any(|&(x, y, added)| !added && class(x, y) == 12);
        let owner = |&(x, y, _): &(usize, usize, bool)| {
            (0..figures.len()).filter(|&f| held(&figures[f], centre(x, y))).min_by(|&a, &b| (figures[a][2] * figures[a][3]).total_cmp(&(figures[b][2] * figures[b][3])))
        };
        let mut owners: Vec<usize> = part.iter().filter_map(owner).collect();
        owners.sort_unstable();
        owners.dedup();
        if !person || owners.len() < 2 {
            crowd.parts.push(part);
            crowd.found.push(true);
            continue;
        }
        let mut theirs: Vec<Vec<(usize, usize, bool)>> = vec![Vec::new(); figures.len()];
        let mut rest = Vec::new();
        for cell in part {
            match owner(&cell) {
                Some(f) => theirs[f].push(cell),
                None => rest.push(cell),
            }
        }
        for cells in theirs.into_iter().filter(|cells| !cells.is_empty()) {
            crowd.parts.push(cells);
            crowd.found.push(true);
        }
        if !rest.is_empty() {
            crowd.parts.push(rest);
            crowd.found.push(false);
        }
    }
    crowd
}

fn neighbours(x: usize, y: usize, width: usize, height: usize) -> impl Iterator<Item = (usize, usize)> {
    (-1isize..=1).flat_map(move |dy| (-1isize..=1).map(move |dx| (x as isize + dx, y as isize + dy)))
        .filter(move |&(nx, ny)| (nx, ny) != (x as isize, y as isize) && (0..width as isize).contains(&nx) && (0..height as isize).contains(&ny))
        .map(|(nx, ny)| (nx as usize, ny as usize))
}
