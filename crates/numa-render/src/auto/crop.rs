use numa_core::document::Perspective;
use numa_core::image::{crop_fits, into_crop};

use super::corners::{cuts, people, subjects, Crowd};
use super::scene::Scene;

const BEAT: f32 = 0.08;

const FLOOR_MP: f32 = 8.0;

const ANIMALS: std::ops::RangeInclusive<u8> = 14..=23;

const THINGS: [u8; 6] = [1, 3, 4, 5, 6, 8];

const THING_SHARE: f32 = 0.01;

const INTRUSION: f32 = 0.2;

const FACING: f32 = 0.1;

#[derive(Debug, Clone, PartialEq)]
pub struct Proposal {
    pub rect: [f32; 4],
    pub aspect: &'static str,
    pub score: f32,
    pub megapixels: f32,
    pub why: String,
}

struct Scored {
    score: f32,

    terms: Vec<(&'static str, f32)>,
}

pub fn proposals(scene: &Scene, rect: [f32; 4], angle: f32, perspective: Perspective, size: (u32, u32)) -> (Vec<Proposal>, bool) {
    let (width, height) = (scene.size[0] as f32, scene.size[1] as f32);
    let mut crowd = people(scene);

    let Some(chosen) = choose(scene, &crowd) else { return (Vec::new(), false) };
    let subject = Subject::of(scene, &crowd.parts, &chosen);
    crowd.subject = chosen;
    let animals = whole_boxes(scene);
    let frame_aspect = rect[2] * width / (rect[3] * height);
    let total_mp = size.0 as f32 * size.1 as f32 / 1e6;
    let floor = (FLOOR_MP / total_mp).clamp(0.05, 0.6);
    let shot = score(scene, &subject, rect, angle, perspective, rect);

    let same = match frame_aspect.max(1.0 / frame_aspect) {
        r if (r - 1.5).abs() < 0.03 => if frame_aspect >= 1.0 { "3:2" } else { "2:3" },
        r if (r - 4.0 / 3.0).abs() < 0.03 => if frame_aspect >= 1.0 { "4:3" } else { "3:4" },
        _ => "Same shape",
    };
    let mut aspects: Vec<(&'static str, f32)> = vec![(same, frame_aspect)];
    let landscape = frame_aspect >= 1.0;
    aspects.push(if landscape { ("5:4", 1.25) } else { ("4:5", 0.8) });
    if let Some(b) = subject.as_ref().map(|s| s.aspect(width, height)) {
        if (0.7..=1.4).contains(&b) {
            aspects.push(("1:1", 1.0));
        }
        if landscape && b <= 1.0 / 1.6 {
            aspects.push(("Portrait 4:5", 0.8));
        }
        if !landscape && b >= 1.6 {
            aspects.push(("Landscape 5:4", 1.25));
        }
    }
    if scene.horizon().is_some() && landscape {
        aspects.push(("16:9", 16.0 / 9.0));
    }

    let mut all: Vec<Proposal> = Vec::new();
    for (name, aspect) in aspects {

        let (mut w, mut h) = if aspect * height >= frame_aspect * height {
            (rect[2], rect[2] * width / aspect / height)
        } else {
            (rect[3] * height * aspect / width, rect[3])
        };
        if h > rect[3] {
            let k = rect[3] / h;
            (w, h) = (w * k, h * k);
        }
        if w > rect[2] {
            let k = rect[2] / w;
            (w, h) = (w * k, h * k);
        }
        let full = w * h;
        let mut share = 1.0f32;
        while share * full / (rect[2] * rect[3]) >= floor {

            if share * full / (rect[2] * rect[3]) > 0.9 {
                share -= 0.04;
                continue;
            }
            let (cw, ch) = (w * share.sqrt(), h * share.sqrt());
            for i in 0..=10 {
                for j in 0..=10 {
                    let candidate = [rect[0] + (rect[2] - cw) * i as f32 / 10.0, rect[1] + (rect[3] - ch) * j as f32 / 10.0, cw, ch];
                    if !crop_fits(candidate, angle, perspective, width, height) || cuts(scene, (rect, angle), (candidate, angle), perspective, Some(&crowd)).is_some() {
                        continue;
                    }
                    if subject.as_ref().is_some_and(|s| !s.headroom(scene, candidate, angle, perspective)) {
                        continue;
                    }
                    if !animals_whole(&animals, scene, (rect, candidate), angle, perspective) {
                        continue;
                    }
                    let scored = score(scene, &subject, candidate, angle, perspective, rect);
                    let megapixels = total_mp * cw * ch;
                    all.push(Proposal { rect: candidate, aspect: name, score: scored.score, megapixels, why: why(name, &scored, &shot, megapixels) });
                }
            }
            share -= 0.04;
        }
    }
    all.sort_by(|a, b| b.score.total_cmp(&a.score));

    let turned = all.iter().find(|p| p.aspect.starts_with("Portrait") || p.aspect.starts_with("Landscape")).cloned();

    let mut kept: Vec<Proposal> = Vec::new();
    for proposal in all {
        if kept.len() == 3 {
            break;
        }
        if kept.iter().any(|k| iou(k.rect, proposal.rect) > 0.7) || proposal.rect == rect {
            continue;
        }
        if kept.len() == 2 && kept.iter().all(|k| k.aspect == proposal.aspect) && kept[0].aspect == kept[1].aspect {
            continue;
        }
        kept.push(proposal);
    }
    if let Some(turned) = turned.filter(|t| !kept.iter().any(|k| k.aspect == t.aspect)) {
        if kept.len() == 3 {
            kept.pop();
        }
        kept.push(turned);
    }
    let beats = kept.first().is_some_and(|best| best.score >= shot.score + BEAT);
    (kept, beats)
}

pub(super) fn named(scene: &Scene) -> bool {
    !admitted(&people(scene)).is_empty() || Subject::thing(scene).is_some()
}

fn admitted(crowd: &Crowd) -> Vec<usize> {
    let size = |i: usize| crowd.parts[i].len();
    let largest = (0..crowd.parts.len()).map(size).max().unwrap_or(0);
    (0..crowd.parts.len()).filter(|&i| crowd.found[i] && size(i) >= 6 && size(i) * 20 >= largest).collect()
}

pub(super) fn choose(scene: &Scene, crowd: &Crowd) -> Option<Vec<usize>> {
    let (cw, ch, _) = &scene.classes;
    let (cw, ch) = (*cw, *ch);
    let admitted = admitted(crowd);
    let parts = &crowd.parts;
    let cells = |i: usize| parts[i].iter().filter(|c| !c.2).map(|&(x, y, _)| (x, y));
    let box_of = |i: usize| {
        let (mut l, mut t, mut r, mut b) = (usize::MAX, usize::MAX, 0, 0);
        for (x, y) in cells(i) {
            (l, t, r, b) = (l.min(x), t.min(y), r.max(x + 1), b.max(y + 1));
        }
        [l as f32 / cw as f32, t as f32 / ch as f32, r as f32 / cw as f32, b as f32 / ch as f32]
    };

    let on = |i: usize| {
        let Some([u, v]) = scene.camera.focus else { return false };
        let [l, t, r, b] = box_of(i);
        let animal = parts[i].iter().any(|&(x, y, added)| !added && scene.classes.2[y * cw + x] == 126);
        parts[i].iter().any(|&(x, y, _)| ((x as f32 + 0.5) / cw as f32 - u).abs() <= 0.02 + 0.5 / cw as f32 && ((y as f32 + 0.5) / ch as f32 - v).abs() <= 0.02 + 0.5 / ch as f32)
            || animal && scene.objects.iter().any(|(class, [x, y, w, h])| ANIMALS.contains(class) && x < &r && x + w > l && y < &b && y + h > t && (*x..=x + w).contains(&u) && (*y..=y + h).contains(&v))
    };
    let score = |i: usize| {
        let [l, t, r, b] = box_of(i);
        let mut score = if on(i) { 2 } else { 0 };
        let near = |u: f32, v: f32| cells(i).any(|(x, y)| ((x as f32 + 0.5) / cw as f32 - u).abs() * cw as f32 <= 1.0 && ((y as f32 + 0.5) / ch as f32 - v).abs() * ch as f32 <= 1.0);
        if scene.faces.iter().any(|[x, y, w, h]| near(x + w / 2.0, y + h / 2.0)) {
            score += 2;
        }
        let edges = [cells(i).any(|(x, _)| x == 0), cells(i).any(|(x, _)| x + 1 == cw), cells(i).any(|(_, y)| y == 0), cells(i).any(|(_, y)| y + 1 == ch)];
        if edges.iter().filter(|e| **e).count() >= 2 || (r - l) * (b - t) > 0.6 {
            score -= 2;
        }
        score
    };
    if admitted.is_empty() || scene.camera.focus.is_some() && !admitted.iter().any(|&i| on(i)) {
        return Some(Vec::new());
    }
    let size = |i: usize| parts[i].len();
    let scores: Vec<i32> = admitted.iter().map(|&i| score(i)).collect();
    let (best, &top) = admitted.iter().zip(&scores).max_by_key(|(i, s)| (**s, size(**i)))?;
    let together: Vec<usize> = admitted.iter().zip(&scores).filter(|(i, s)| **s >= top - 1 && size(**i) * 3 >= size(*best) * 2).map(|(i, _)| *i).collect();
    let [l, t, r, b] = together.iter().map(|&i| box_of(i)).fold([1.0f32, 1.0, 0.0, 0.0], |a, b| [a[0].min(b[0]), a[1].min(b[1]), a[2].max(b[2]), a[3].max(b[3])]);
    (together.len() == 1 || (r - l) * (b - t) <= 0.4).then_some(together)
}

struct Subject {
    cells: Vec<[f32; 2]>,
    box_: [f32; 4],
    faces: Vec<[f32; 4]>,

    facing: f32,
}

impl Subject {
    fn of(scene: &Scene, parts: &[Vec<(usize, usize, bool)>], chosen: &[usize]) -> Option<Self> {
        Self::of_grid(scene, parts, chosen).or_else(|| Self::thing(scene)).or_else(|| Self::salient(scene))
    }

    fn salient(scene: &Scene) -> Option<Self> {
        let (cw, ch, cells) = &scene.salient;
        let cells: Vec<[f32; 2]> = cells
            .iter()
            .enumerate()
            .filter(|(_, a)| **a >= 128)
            .map(|(i, _)| [((i % cw) as f32 + 0.5) / *cw as f32, ((i / cw) as f32 + 0.5) / *ch as f32])
            .collect();
        Some(Self { box_: bounds(&cells)?, cells, faces: Vec::new(), facing: 0.0 })
    }

    fn thing(scene: &Scene) -> Option<Self> {
        let area = |b: &[f32; 4]| b[2] * b[3];
        let &(_, box_) = scene
            .objects
            .iter()
            .filter(|(class, b)| (ANIMALS.contains(class) || THINGS.contains(class)) && area(b) >= THING_SHARE)
            .max_by(|a, b| area(&a.1).total_cmp(&area(&b.1)))?;
        let cells = (0..8).flat_map(|i| (0..8).map(move |j| [box_[0] + box_[2] * (i as f32 + 0.5) / 8.0, box_[1] + box_[3] * (j as f32 + 0.5) / 8.0])).collect();
        Some(Self { cells, box_, faces: Vec::new(), facing: 0.0 })
    }

    fn of_grid(scene: &Scene, parts: &[Vec<(usize, usize, bool)>], chosen: &[usize]) -> Option<Self> {
        let (cw, ch, _) = &scene.classes;
        let cells: Vec<[f32; 2]> = chosen
            .iter()
            .flat_map(|&i| parts[i].iter().filter(|c| !c.2).map(|&(x, y, _)| [(x as f32 + 0.5) / *cw as f32, (y as f32 + 0.5) / *ch as f32]))
            .collect();
        let box_ = bounds(&cells)?;
        let [l, t, r, b] = [box_[0], box_[1], box_[0] + box_[2], box_[1] + box_[3]];
        let theirs: Vec<usize> = (0..scene.faces.len())
            .filter(|&i| {
                let [x, y, w, h] = scene.faces[i];
                let (u, v) = (x + w / 2.0, y + h / 2.0);
                u >= l && u <= r && v >= t && v <= b
            })
            .collect();
        let faces = theirs.iter().map(|&i| scene.faces[i]).collect();

        let facing = theirs.first().and_then(|&i| {
            let [right_eye, left_eye, nose, ..] = *scene.landmarks.get(i)?;
            Some((nose[0] - (right_eye[0] + left_eye[0]) / 2.0) / scene.faces[i][2].max(1e-6))
        });
        Some(Self { cells, box_, faces, facing: facing.unwrap_or(0.0) })
    }

    fn aspect(&self, width: f32, height: f32) -> f32 {
        self.box_[2] * width / (self.box_[3] * height).max(1e-6)
    }

    fn headroom(&self, scene: &Scene, candidate: [f32; 4], angle: f32, perspective: Perspective) -> bool {
        let (width, height) = (scene.size[0] as f32, scene.size[1] as f32);
        self.faces.iter().all(|[x, y, w, h]| {
            let top = into_crop(width, height, candidate, angle, perspective, [x + w / 2.0, y - 0.3 * h]);
            top[1] >= 0.0
        })
    }
}

fn bounds(cells: &[[f32; 2]]) -> Option<[f32; 4]> {
    let (mut l, mut t, mut r, mut b) = (1.0f32, 1.0f32, 0.0f32, 0.0f32);
    for [u, v] in cells {
        (l, t, r, b) = (l.min(*u), t.min(*v), r.max(*u), b.max(*v));
    }
    (!cells.is_empty()).then_some([l, t, r - l, b - t])
}

fn score(scene: &Scene, subject: &Option<Subject>, candidate: [f32; 4], angle: f32, perspective: Perspective, shot: [f32; 4]) -> Scored {
    let (width, height) = (scene.size[0] as f32, scene.size[1] as f32);
    let at = |p: [f32; 2]| into_crop(width, height, candidate, angle, perspective, p);
    let mut terms: Vec<(&'static str, f32, f32)> = Vec::new();

    if let Some(subject) = subject {
        let inside: Vec<[f32; 2]> = subject.cells.iter().map(|p| at(*p)).filter(|p| (0.0..=1.0).contains(&p[0]) && (0.0..=1.0).contains(&p[1])).collect();
        if !inside.is_empty() {
            let n = inside.len() as f32;

            let centre = match subject.faces.first() {
                Some([x, y, w, h]) => at([x + w / 2.0, y + h / 2.0]),
                None => [inside.iter().map(|p| p[0]).sum::<f32>() / n, inside.iter().map(|p| p[1]).sum::<f32>() / n],
            };
            let thirds = [1.0 / 3.0, 2.0 / 3.0];
            let d_thirds = thirds.iter().flat_map(|x| thirds.iter().map(move |y| (centre[0] - x).hypot(centre[1] - y))).fold(f32::MAX, f32::min);
            let d_centre = (centre[0] - 0.5).hypot(centre[1] - 0.5);
            let gauss = |d: f32| (-(d * d) / (2.0 * 0.1 * 0.1)).exp();
            terms.push(("placement", 0.35, gauss(d_thirds).max(0.8 * gauss(d_centre))));

            let sides = |frame: [f32; 4], reach: f32| {
                let all: Vec<[f32; 2]> = subject.cells.iter().map(|p| into_crop(width, height, frame, angle, perspective, *p)).collect();
                [all.iter().any(|p| p[0] < reach), all.iter().any(|p| p[0] > 1.0 - reach), all.iter().any(|p| p[1] < reach), all.iter().any(|p| p[1] > 1.0 - reach)]
            };
            let (touched, through) = (sides(shot, 0.02), sides(candidate, 0.0));
            let past: Vec<bool> = touched.iter().zip(through).map(|(a, b)| *a || b).collect();
            let margin = inside
                .iter()
                .flat_map(|p| [p[0], 1.0 - p[0], p[1], 1.0 - p[1]].into_iter().zip(&past).filter(|(_, past)| !**past).map(|(d, _)| d))
                .fold(f32::MAX, f32::min);
            let smooth = ((margin - 0.02) / 0.04).clamp(0.0, 1.0);
            terms.push(("margins", 0.10, smooth * smooth * (3.0 - 2.0 * smooth)));
        }
    }

    if let Some(subject) = subject.as_ref().filter(|s| s.facing.abs() >= FACING) {
        if let Some(&[x, y, w, h]) = subject.faces.first() {
            let [u, _] = at([x + w / 2.0, y + h / 2.0]);
            let (front, behind) = if subject.facing > 0.0 { (1.0 - u, u) } else { (u, 1.0 - u) };
            if front > 0.0 && behind > 0.0 {
                terms.push(("look", 0.15, (-(front / behind / 2.0).ln().powi(2) / 0.5).exp()));
            }
        }
    }
    if let Some([u0, v0, u1, v1]) = scene.horizon() {
        let (a, b) = (at([u0, v0]), at([u1, v1]));
        let row = a[1] + (b[1] - a[1]) * ((0.5 - a[0]) / (b[0] - a[0]));
        if (0.0..=1.0).contains(&row) {
            let near = |target: f32| (-((row - target).powi(2)) / (2.0 * 0.05 * 0.05)).exp();
            terms.push(("horizon", 0.25, near(1.0 / 3.0).max(near(2.0 / 3.0)).max(0.6 * near(0.5))));
        }
    }

    let inside = |p: [f32; 2]| (0.0..=1.0).contains(&p[0]) && (0.0..=1.0).contains(&p[1]);
    let intrusions: Vec<&[f32; 4]> = scene
        .objects
        .iter()
        .map(|(_, b)| b)

        .filter(|b| b[2] * b[3] <= INTRUSION && !subject.as_ref().is_some_and(|s| overlap(s.box_, **b) > 0.3 * (b[2] * b[3]).min(s.box_[2] * s.box_[3])))
        .collect();
    let (bw, bh, bright) = &scene.bright;
    let cell = |i: usize| [((i % bw) as f32 + 0.5) / *bw as f32, ((i / bw) as f32 + 0.5) / *bh as f32];
    let lit = |i: usize| bright[i] >= 128 && scene.class_at(cell(i)[0], cell(i)[1]) != Some(2);
    if !intrusions.is_empty() || (0..bright.len()).any(lit) {
        let mut cost = 0.0f32;
        for b in intrusions {
            let kept = (0..5).flat_map(|i| (0..5).map(move |j| [b[0] + b[2] * (i as f32 + 0.5) / 5.0, b[1] + b[3] * (j as f32 + 0.5) / 5.0])).filter(|p| inside(at(*p))).count() as f32 / 25.0;
            if kept > 0.0 && kept < 1.0 {
                cost += (1.0 - kept) * (b[2] * b[3] / THING_SHARE).min(1.0);
            }
        }
        let band: Vec<usize> = (0..bright.len()).filter(|&i| {
            let p = at(cell(i));
            inside(p) && (p[0] < 0.05 || p[0] > 0.95 || p[1] < 0.05 || p[1] > 0.95)
        }).collect();
        if !band.is_empty() {
            cost += 3.0 * band.iter().filter(|&&i| lit(i)).count() as f32 / band.len() as f32;
        }
        terms.push(("edges", 0.20, (1.0 - cost).max(0.0)));
    }
    terms.push(("area", 0.15, (candidate[2] * candidate[3] / (shot[2] * shot[3])).sqrt().min(1.0)));

    let (cw, ch, classes) = &scene.classes;
    if !classes.is_empty() {
        let (mut kept, mut all) = (0usize, 0usize);
        for (i, class) in classes.iter().enumerate().step_by(2) {
            if *class == 2 {
                continue;
            }
            all += 1;
            let p = at([((i % cw) as f32 + 0.5) / *cw as f32, ((i / cw) as f32 + 0.5) / *ch as f32]);
            kept += ((0.0..=1.0).contains(&p[0]) && (0.0..=1.0).contains(&p[1])) as usize;
        }
        if all > 0 {
            terms.push(("content", 0.15, kept as f32 / all as f32));
        }
    }

    let total: f32 = terms.iter().map(|t| t.1).sum();
    let score = terms.iter().map(|t| t.1 * t.2).sum::<f32>() / total;
    Scored { score, terms: terms.into_iter().map(|t| (t.0, t.1 * t.2 / total)).collect() }
}

fn why(aspect: &str, scored: &Scored, shot: &Scored, megapixels: f32) -> String {
    let gain = |name: &str| {
        let was = shot.terms.iter().find(|t| t.0 == name).map_or(0.0, |t| t.1);
        scored.terms.iter().find(|t| t.0 == name).map_or(0.0, |t| t.1) - was
    };
    let best = ["placement", "look", "horizon", "margins", "content", "edges"].into_iter().max_by(|a, b| gain(a).total_cmp(&gain(b))).filter(|name| gain(name) > 0.01);
    let reason = match best {
        Some("placement") => "the subject on a third",
        Some("horizon") => "the horizon on a third",
        Some("margins") => "room round the subject",
        Some("content") => "the empty part left out",
        Some("look") => "room where the face looks",
        Some("edges") => "less cut at the edges",
        _ => "tighter round what matters",
    };
    format!("{aspect} · {reason} · {megapixels:.0} MP")
}

fn whole_boxes(scene: &Scene) -> Vec<[f32; 4]> {
    let (width, height) = (scene.size[0] as f32, scene.size[1] as f32);
    let short = 0.015 * width.min(height);
    let (gx, gy) = (short / width, short / height);
    let detected = scene
        .objects
        .iter()

        .filter(|(class, b)| ANIMALS.contains(class) && b[2] * b[3] >= THING_SHARE && b[0] > 0.005 && b[1] > 0.005 && b[0] + b[2] < 0.995 && b[1] + b[3] < 0.995)
        .map(|(_, [x, y, w, h])| {
            let (x, y) = ((x - gx).max(0.0), (y - gy).max(0.0));
            [x, y, (w + 2.0 * gx).min(1.0 - x), (h + 2.0 * gy).min(1.0 - y)]
        });
    let (cw, ch, classes) = &scene.classes;
    if classes.is_empty() {
        return detected.collect();
    }
    let class = |x: usize, y: usize| classes[y * cw + x] as u16;
    subjects(*cw, *ch, &class)
        .into_iter()
        .filter(|part| part.iter().any(|&(x, y, added)| !added && class(x, y) == 126))
        .map(|part| {
            let (mut l, mut t, mut r, mut b) = (usize::MAX, usize::MAX, 0, 0);
            for &(x, y, _) in part.iter().filter(|c| !c.2) {
                (l, t, r, b) = (l.min(x), t.min(y), r.max(x + 1), b.max(y + 1));
            }
            let (w, h) = ((r - l) as f32 / *cw as f32, (b - t) as f32 / *ch as f32);
            let (x, y) = (l as f32 / *cw as f32 - w / 4.0, t as f32 / *ch as f32 - h / 10.0);
            [x.max(0.0), y.max(0.0), (w * 1.5).min(1.0 - x.max(0.0)), (h * 1.2).min(1.0 - y.max(0.0))]
        })
        .chain(detected)
        .collect()
}

fn animals_whole(boxes: &[[f32; 4]], scene: &Scene, (shot, candidate): ([f32; 4], [f32; 4]), angle: f32, perspective: Perspective) -> bool {
    let (width, height) = (scene.size[0] as f32, scene.size[1] as f32);
    let inside = |frame: [f32; 4], p: [f32; 2]| {
        let q = into_crop(width, height, frame, angle, perspective, p);
        (0.0..=1.0).contains(&q[0]) && (0.0..=1.0).contains(&q[1])
    };
    boxes.iter().all(|[x, y, w, h]| {
        let corners = [[*x, *y], [x + w, *y], [*x, y + h], [x + w, y + h]];
        !corners.iter().all(|c| inside(shot, *c)) || corners.iter().all(|c| inside(candidate, *c))
    })
}

fn iou(a: [f32; 4], b: [f32; 4]) -> f32 {
    let inter = overlap(a, b);
    inter / (a[2] * a[3] + b[2] * b[3] - inter)
}

fn overlap(a: [f32; 4], b: [f32; 4]) -> f32 {
    let x = (a[0] + a[2]).min(b[0] + b[2]) - a[0].max(b[0]);
    let y = (a[1] + a[3]).min(b[1] + b[3]) - a[1].max(b[1]);
    x.max(0.0) * y.max(0.0)
}
