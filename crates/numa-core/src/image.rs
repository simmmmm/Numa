use rayon::prelude::*;

use std::sync::Arc;

use crate::color::CameraProfile;
use crate::document::Perspective;
use crate::profile::DngProfile;

impl<'a> From<&'a LinearImage> for std::borrow::Cow<'a, LinearImage> {
    fn from(image: &'a LinearImage) -> Self {
        Self::Borrowed(image)
    }
}

impl From<LinearImage> for std::borrow::Cow<'_, LinearImage> {
    fn from(image: LinearImage) -> Self {
        Self::Owned(image)
    }
}

#[derive(Debug, Clone)]
pub struct LinearImage {
    pub width: u32,
    pub height: u32,

    pub data: Vec<f32>,

    pub profile: Option<CameraProfile>,

    pub clip: Option<f32>,

    pub rendering: Option<Arc<DngProfile>>,

    pub film_mode: Option<String>,

    pub white_point: Option<crate::color::WhiteBalance>,
}

impl LinearImage {
    pub fn new(width: u32, height: u32, data: Vec<f32>) -> Self {
        debug_assert_eq!(data.len(), (width as usize) * (height as usize) * 3);
        Self {
            width,
            height,
            data,
            profile: None,
            clip: None,
            rendering: None,
            film_mode: None,
            white_point: None,
        }
    }

    pub fn with_profile(mut self, profile: CameraProfile) -> Self {
        self.profile = Some(profile);
        self
    }

    pub fn with_clip(mut self, clip: f32) -> Self {
        self.clip = Some(clip);
        self
    }

    pub fn with_rendering(mut self, rendering: Option<Arc<DngProfile>>) -> Self {
        self.rendering = rendering;
        self
    }

    pub fn with_film_mode(mut self, film_mode: Option<String>) -> Self {
        self.film_mode = film_mode;
        self
    }

    pub fn pixel_count(&self) -> usize {
        (self.width as usize) * (self.height as usize)
    }

    pub fn oriented(&self, transpose: bool, flip_x: bool, flip_y: bool) -> LinearImage {
        self.clone().into_oriented(transpose, flip_x, flip_y)
    }

    pub fn into_oriented(self, transpose: bool, flip_x: bool, flip_y: bool) -> LinearImage {
        if !transpose && !flip_x && !flip_y {
            return self;
        }

        let (width, height) = (self.width as usize, self.height as usize);
        let (out_width, out_height) = if transpose {
            (height, width)
        } else {
            (width, height)
        };

        let mut data = vec![0.0f32; out_width * out_height * 3];

        data.par_chunks_mut(out_width * 3)
            .enumerate()
            .for_each(|(y, row)| {
                for x in 0..out_width {

                    let (fx, fy) = if transpose { (y, x) } else { (x, y) };
                    let sx = if flip_x { width - 1 - fx } else { fx };
                    let sy = if flip_y { height - 1 - fy } else { fy };

                    let from = (sy * width + sx) * 3;
                    row[x * 3..x * 3 + 3].copy_from_slice(&self.data[from..from + 3]);
                }
            });

        LinearImage {
            width: out_width as u32,
            height: out_height as u32,
            data,
            profile: self.profile,
            clip: self.clip,
            rendering: self.rendering,
            film_mode: self.film_mode,
            white_point: None,
        }
    }

    pub fn cropped(&self, rect: [f32; 4], angle: f32, perspective: Perspective) -> LinearImage {
        let [_, _, width, height] = rect;
        let (source_width, source_height) = (self.width as f32, self.height as f32);

        let crop_width = (width * source_width).round().max(1.0);
        let crop_height = (height * source_height).round().max(1.0);
        let (out_width, out_height) = (crop_width as usize, crop_height as usize);

        let source_of = source_map(source_width, source_height, rect, angle, perspective);

        let mut data = vec![0.0f32; out_width * out_height * 3];

        data.par_chunks_mut(out_width * 3)
            .enumerate()
            .for_each(|(row_index, row)| {
                let dy = row_index as f32 - crop_height / 2.0 + 0.5;

                for column in 0..out_width {
                    let dx = column as f32 - crop_width / 2.0 + 0.5;
                    let (sx, sy) = source_of(dx, dy);
                    let pixel = self.sample(sx - 0.5, sy - 0.5);
                    row[column * 3..column * 3 + 3].copy_from_slice(&pixel);
                }
            });

        LinearImage {
            width: out_width as u32,
            height: out_height as u32,
            data,
            profile: self.profile,
            clip: self.clip,
            rendering: self.rendering.clone(),
            film_mode: self.film_mode.clone(),
            white_point: None,
        }
    }

    fn sample(&self, x: f32, y: f32) -> [f32; 3] {
        let (width, height) = (self.width as isize, self.height as isize);
        if x < -1.0 || y < -1.0 || x > self.width as f32 || y > self.height as f32 {
            return [0.0; 3];
        }

        let x0 = x.floor();
        let y0 = y.floor();
        let tx = x - x0;
        let ty = y - y0;

        let at = |px: isize, py: isize| -> [f32; 3] {
            let cx = px.clamp(0, width - 1) as usize;
            let cy = py.clamp(0, height - 1) as usize;
            let offset = (cy * self.width as usize + cx) * 3;
            [self.data[offset], self.data[offset + 1], self.data[offset + 2]]
        };

        let (ix, iy) = (x0 as isize, y0 as isize);
        let (a, b, c, d) = (
            at(ix, iy),
            at(ix + 1, iy),
            at(ix, iy + 1),
            at(ix + 1, iy + 1),
        );

        let mut out = [0.0f32; 3];
        for channel in 0..3 {
            let top = a[channel] + (b[channel] - a[channel]) * tx;
            let bottom = c[channel] + (d[channel] - c[channel]) * tx;
            out[channel] = top + (bottom - top) * ty;
        }
        out
    }

    pub fn downscaled(&self, max_edge: u32) -> Option<LinearImage> {
        let longest = self.width.max(self.height);
        if longest <= max_edge || longest == 0 {
            return None;
        }

        let scale = max_edge as f64 / longest as f64;
        let width = ((self.width as f64 * scale).round() as u32).max(1);
        let height = ((self.height as f64 * scale).round() as u32).max(1);

        let x_ratio = self.width as f64 / width as f64;
        let y_ratio = self.height as f64 / height as f64;

        let mut data = vec![0.0f32; (width as usize) * (height as usize) * 3];

        data.par_chunks_mut(width as usize * 3)
            .enumerate()
            .for_each(|(y, row)| {
                let y0 = (y as f64 * y_ratio) as u32;
                let y1 = (((y as f64 + 1.0) * y_ratio).ceil() as u32).min(self.height).max(y0 + 1);

                for x in 0..width as usize {
                    let x0 = (x as f64 * x_ratio) as u32;
                    let x1 = (((x as f64 + 1.0) * x_ratio).ceil() as u32).min(self.width).max(x0 + 1);

                    let mut sum = [0.0f64; 3];
                    let mut count = 0.0f64;

                    for sy in y0..y1 {
                        let base = (sy as usize * self.width as usize) * 3;
                        for sx in x0..x1 {
                            let offset = base + sx as usize * 3;
                            sum[0] += self.data[offset] as f64;
                            sum[1] += self.data[offset + 1] as f64;
                            sum[2] += self.data[offset + 2] as f64;
                            count += 1.0;
                        }
                    }

                    for channel in 0..3 {
                        row[x * 3 + channel] = (sum[channel] / count) as f32;
                    }
                }
            });

        Some(LinearImage {
            width,
            height,
            data,
            profile: self.profile,
            clip: self.clip,
            rendering: self.rendering.clone(),
            film_mode: self.film_mode.clone(),
            white_point: None,
        })
    }
}

pub(crate) fn source_map(
    width: f32,
    height: f32,
    rect: [f32; 4],
    angle: f32,
    perspective: Perspective,
) -> impl Fn(f32, f32) -> (f32, f32) {
    let [x, y, w, h] = rect;
    let (centre_x, centre_y) = ((x + w / 2.0) * width, (y + h / 2.0) * height);
    let (half_width, half_height) = (width / 2.0, height / 2.0);
    let (sin, cos) = angle.to_radians().sin_cos();
    let (vertical, horizontal) = perspective.coefficients();
    let stretch = perspective.stretch();

    move |dx: f32, dy: f32| {
        let rx = (dx * cos - dy * sin) / stretch;
        let ry = (dx * sin + dy * cos) * stretch;
        let (qx, qy) = (centre_x + rx - half_width, centre_y + ry - half_height);

        let depth = 1.0 - vertical * (qy / half_height) - horizontal * (qx / half_width);
        let reach = 1.0 / depth.max(0.05);
        (half_width + qx * reach, half_height + qy * reach)
    }
}

pub fn crop_in_view(width: f32, height: f32, rect: [f32; 4], angle: f32, perspective: Perspective) -> [f32; 4] {
    let [x, y, w, h] = rect;

    let (cx, cy) = ((x + w / 2.0 - 0.5) * width, (y + h / 2.0 - 0.5) * height);
    let stretch = perspective.stretch();
    let (sx, sy) = (cx * stretch, cy / stretch);
    let (sin, cos) = angle.to_radians().sin_cos();
    let (vx, vy) = (sx * cos + sy * sin, sy * cos - sx * sin);
    [0.5 + vx / width - w / 2.0, 0.5 + vy / height - h / 2.0, w, h]
}

pub fn view_to_crop(width: f32, height: f32, rect: [f32; 4], rect_angle: f32, view_angle: f32, perspective: Perspective) -> [f32; 6] {
    let [x, y, w, h] = rect;
    let stretch = perspective.stretch();
    let (view_sin, view_cos) = view_angle.to_radians().sin_cos();
    let (sin, cos) = rect_angle.to_radians().sin_cos();
    let (cx, cy) = ((x + w / 2.0 - 0.5) * width, (y + h / 2.0 - 0.5) * height);

    let through = |u: f32, v: f32| {
        let (dx, dy) = ((u - 0.5) * width, (v - 0.5) * height);
        let (qx, qy) = ((dx * view_cos - dy * view_sin) / stretch, (dx * view_sin + dy * view_cos) * stretch);
        let (ex, ey) = ((qx - cx) * stretch, (qy - cy) / stretch);
        let (ox, oy) = (ex * cos + ey * sin, ey * cos - ex * sin);
        (0.5 + ox / (w * width), 0.5 + oy / (h * height))
    };
    let (origin, across, down) = (through(0.0, 0.0), through(1.0, 0.0), through(0.0, 1.0));
    [
        across.0 - origin.0, down.0 - origin.0, origin.0,
        across.1 - origin.1, down.1 - origin.1, origin.1,
    ]
}

pub fn crop_from_view(width: f32, height: f32, view: [f32; 4], angle: f32, perspective: Perspective) -> [f32; 4] {
    let [x, y, w, h] = view;
    let (ex, ey) = ((x + w / 2.0 - 0.5) * width, (y + h / 2.0 - 0.5) * height);
    let stretch = perspective.stretch();
    let (sin, cos) = angle.to_radians().sin_cos();
    let (cx, cy) = ((ex * cos - ey * sin) / stretch, (ex * sin + ey * cos) * stretch);
    [0.5 + cx / width - w / 2.0, 0.5 + cy / height - h / 2.0, w, h]
}

pub fn crop_fits(rect: [f32; 4], angle: f32, perspective: Perspective, width: f32, height: f32) -> bool {
    let source_of = source_map(width, height, rect, angle, perspective);

    let (hw, hh) = ((rect[2] * width - 1.0).max(0.0) / 2.0, (rect[3] * height - 1.0).max(0.0) / 2.0);
    [(-hw, -hh), (hw, -hh), (hw, hh), (-hw, hh)].iter().all(|(dx, dy)| {
        let (sx, sy) = source_of(*dx, *dy);
        (-0.01..=width + 0.01).contains(&sx) && (-0.01..=height + 0.01).contains(&sy)
    })
}

pub fn crop_inside(rect: [f32; 4], angle: f32, perspective: Perspective, width: f32, height: f32) -> [f32; 4] {
    if crop_fits(rect, angle, perspective, width, height) {
        return rect;
    }
    fit(rect, angle, perspective, width, height, 1.0)
}

fn fit(rect: [f32; 4], angle: f32, perspective: Perspective, width: f32, height: f32, largest: f32) -> [f32; 4] {
    let [x, y, w, h] = rect;
    let (half_width, half_height) = (width / 2.0, height / 2.0);
    let (vertical, horizontal) = perspective.coefficients();
    let stretch = perspective.stretch();
    let (sin, cos) = angle.to_radians().sin_cos();

    let a = [horizontal / half_width, vertical / half_height];
    let covered: Vec<[f32; 2]> = [[0.0, 0.0], [width - 1.0, 0.0], [width - 1.0, height - 1.0], [0.0, height - 1.0]]
        .iter()
        .map(|[sx, sy]| {
            let u = [sx - half_width, sy - half_height];
            let scale = 1.0 / (1.0 + a[0] * u[0] + a[1] * u[1]).max(0.05);
            [u[0] * scale, u[1] * scale]
        })
        .collect();

    let corner = |zoom: f32, sx: f32, sy: f32| {
        let (dx, dy) = (sx * zoom * w * half_width, sy * zoom * h * half_height);
        [(dx * cos - dy * sin) / stretch, (dx * sin + dy * cos) * stretch]
    };

    let centres = |zoom: f32| {
        let mut polygon = covered.clone();
        for (sx, sy) in [(-1.0, -1.0), (1.0, -1.0), (1.0, 1.0), (-1.0, 1.0)] {
            let [ox, oy] = corner(zoom, sx, sy);
            let moved: Vec<[f32; 2]> = covered.iter().map(|[px, py]| [px - ox, py - oy]).collect();
            polygon = clip(&polygon, &moved);
            if polygon.is_empty() {
                break;
            }
        }
        let (reach_x, reach_y) = (half_width * (1.0 - zoom * w), half_height * (1.0 - zoom * h));
        let frame = [[-reach_x, -reach_y], [reach_x, -reach_y], [reach_x, reach_y], [-reach_x, reach_y]];
        if reach_x < 0.0 || reach_y < 0.0 {
            return Vec::new();
        }
        clip(&polygon, &frame)
    };

    let (mut small, mut large) = (0.05f32, largest);
    if !centres(small).is_empty() {
        if !centres(large).is_empty() {
            small = large;
        } else {
            for _ in 0..24 {
                let middle = (small + large) / 2.0;
                if centres(middle).is_empty() {
                    large = middle;
                } else {
                    small = middle;
                }
            }
        }
    }

    let wanted = [(x + w / 2.0) * width - half_width, (y + h / 2.0) * height - half_height];
    let place = |zoom: f32| -> [f32; 4] {
        let centre = nearest_in(&centres(zoom), wanted).unwrap_or(wanted);
        let (cw, ch) = (w * zoom, h * zoom);
        [(centre[0] + half_width) / width - cw / 2.0, (centre[1] + half_height) / height - ch / 2.0, cw, ch]
    };

    let inside = |fitted: [f32; 4]| crop_fits(fitted, angle, perspective, width, height);
    let mut zoom = small;
    let mut fitted = place(zoom);
    for _ in 0..40 {
        if inside(fitted) {
            break;
        }
        zoom *= 0.99;
        fitted = place(zoom);
    }
    fitted
}

fn clip(subject: &[[f32; 2]], window: &[[f32; 2]]) -> Vec<[f32; 2]> {

    let area: f32 = (0..window.len())
        .map(|i| {
            let (p, q) = (window[i], window[(i + 1) % window.len()]);
            p[0] * q[1] - q[0] * p[1]
        })
        .sum();
    let sign = if area >= 0.0 { 1.0 } else { -1.0 };
    let side = |p: [f32; 2], a: [f32; 2], b: [f32; 2]| sign * ((b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0]));

    let mut output = subject.to_vec();
    for i in 0..window.len() {
        let (a, b) = (window[i], window[(i + 1) % window.len()]);
        let input = std::mem::take(&mut output);
        for j in 0..input.len() {
            let (current, previous) = (input[j], input[(j + input.len() - 1) % input.len()]);
            let (now, before) = (side(current, a, b), side(previous, a, b));
            if now >= 0.0 {
                if before < 0.0 {
                    output.push(crossing(previous, current, before, now));
                }
                output.push(current);
            } else if before >= 0.0 {
                output.push(crossing(previous, current, before, now));
            }
        }
        if output.is_empty() {
            break;
        }
    }
    output
}

fn crossing(p: [f32; 2], q: [f32; 2], at_p: f32, at_q: f32) -> [f32; 2] {
    let t = at_p / (at_p - at_q);
    [p[0] + (q[0] - p[0]) * t, p[1] + (q[1] - p[1]) * t]
}

fn nearest_in(polygon: &[[f32; 2]], target: [f32; 2]) -> Option<[f32; 2]> {
    if polygon.is_empty() {
        return None;
    }
    if polygon.len() >= 3 && !clip(&[target], polygon).is_empty() {
        return Some(target);
    }
    (0..polygon.len())
        .map(|i| {
            let (a, b) = (polygon[i], polygon[(i + 1) % polygon.len()]);
            let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
            let length = dx * dx + dy * dy;
            let t = if length > 0.0 {
                (((target[0] - a[0]) * dx + (target[1] - a[1]) * dy) / length).clamp(0.0, 1.0)
            } else {
                0.0
            };
            [a[0] + dx * t, a[1] + dy * t]
        })
        .min_by(|p, q| {
            let d = |r: &[f32; 2]| (r[0] - target[0]).powi(2) + (r[1] - target[1]).powi(2);
            d(p).total_cmp(&d(q))
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn coordinates() -> LinearImage {
        let mut data = Vec::new();
        for y in 0..2 {
            for x in 0..3 {
                data.extend_from_slice(&[x as f32, y as f32, 0.0]);
            }
        }
        LinearImage::new(3, 2, data)
    }

    #[test]
    fn a_crop_is_the_part_of_the_view_crop_in_view_names() {

        let (width, height) = (300usize, 200usize);
        let mut data = Vec::with_capacity(width * height * 3);
        for y in 0..height {
            for x in 0..width {
                data.extend_from_slice(&[x as f32, y as f32, 0.0]);
            }
        }
        let image = LinearImage::new(width as u32, height as u32, data);
        let rect = [0.08, 0.3, 0.4, 0.5];
        for (angle, perspective) in [
            (0.0, Perspective::default()),
            (8.0, Perspective::default()),
            (-6.0, Perspective { vertical: 20.0, horizontal: -10.0, aspect: 15.0 }),
        ] {
            let view = image.cropped([0.0, 0.0, 1.0, 1.0], angle, perspective);
            let crop = image.cropped(rect, angle, perspective);
            let [vx, vy, vw, vh] = crop_in_view(width as f32, height as f32, rect, angle, perspective);
            for (s, t) in [(0.2, 0.2), (0.5, 0.5), (0.8, 0.3), (0.3, 0.9)] {
                let at = |image: &LinearImage, u: f32, v: f32| {
                    let (x, y) = ((u * image.width as f32) as usize, (v * image.height as f32) as usize);
                    let i = (y * image.width as usize + x) * 3;
                    [image.data[i], image.data[i + 1]]
                };
                let (here, there) = (at(&crop, s, t), at(&view, vx + s * vw, vy + t * vh));
                let off = (here[0] - there[0]).hypot(here[1] - there[1]);
                assert!(off < 1.5, "angle {angle}: ({s}, {t}) reads {here:?} in the crop, {there:?} in the view");
            }
        }
    }

    #[test]
    fn the_view_finds_its_points_in_an_older_crop() {
        let (width, height) = (400.0, 300.0);
        let perspective = Perspective { vertical: 0.0, horizontal: 0.0, aspect: 12.0 };
        let rect = [0.15, 0.1, 0.6, 0.5];
        for view_angle in [4.0, 9.5] {
            let map = view_to_crop(width, height, rect, 4.0, view_angle, perspective);
            let view = source_map(width, height, [0.0, 0.0, 1.0, 1.0], view_angle, perspective);
            let crop = source_map(width, height, rect, 4.0, perspective);
            for (u, v) in [(0.3, 0.3), (0.5, 0.6), (0.7, 0.25)] {
                let (s, t) = (map[0] * u + map[1] * v + map[2], map[3] * u + map[4] * v + map[5]);
                let a = view((u - 0.5) * width, (v - 0.5) * height);
                let b = crop((s - 0.5) * rect[2] * width, (t - 0.5) * rect[3] * height);
                assert!((a.0 - b.0).hypot(a.1 - b.1) < 0.01, "view {view_angle}° at ({u}, {v}): {a:?} against {b:?}");
            }
        }
    }

    #[test]
    fn the_view_and_the_crop_go_back_and_forth() {
        let perspective = Perspective { vertical: 12.0, horizontal: -8.0, aspect: 5.0 };
        for rect in [[0.1, 0.2, 0.5, 0.4], [0.4, 0.05, 0.55, 0.9]] {
            let view = crop_in_view(400.0, 300.0, rect, 7.0, perspective);
            let back = crop_from_view(400.0, 300.0, view, 7.0, perspective);
            for (a, b) in rect.iter().zip(back) {
                assert!((a - b).abs() < 1e-5, "{rect:?} came back as {back:?}");
            }
        }
    }

    #[test]
    fn a_straightened_crop_stays_on_the_photograph() {
        let (width, height) = (300.0, 200.0);
        let whole = [0.0, 0.0, 1.0, 1.0];
        let kept = crop_inside(whole, 5.0, Perspective::default(), width, height);
        assert!(crop_fits(kept, 5.0, Perspective::default(), width, height), "{kept:?}");
        assert!(kept[2] < 1.0 && kept[2] > 0.8, "shrunk, and only as far as it had to: {kept:?}");
        assert!((kept[2] / kept[3] - 1.0).abs() < 1e-4, "the same shape: {kept:?}");

        let small = [0.3, 0.3, 0.3, 0.3];
        assert_eq!(crop_inside(small, 5.0, Perspective::default(), width, height), small);
        assert_eq!(crop_inside(kept, 0.0, Perspective::default(), width, height), kept, "no growing back");
    }

    #[test]
    fn crop_takes_the_region_asked_for() {

        let mut data = Vec::new();
        for y in 0..4 {
            for x in 0..4 {
                data.extend_from_slice(&[(x + 4 * y) as f32, 0.0, 0.0]);
            }
        }
        let image = LinearImage::new(4, 4, data);

        let whole = image.cropped([0.0, 0.0, 1.0, 1.0], 0.0, Default::default());
        assert_eq!((whole.width, whole.height), (4, 4));
        assert_eq!(whole.data, image.data);

        let corner = image.cropped([0.5, 0.5, 0.5, 0.5], 0.0, Default::default());
        assert_eq!((corner.width, corner.height), (2, 2));
        assert_eq!(corner.data[0], 10.0, "top-left of the crop is source (2,2)");
        assert_eq!(corner.data[3], 11.0);
        assert_eq!(corner.data[6], 14.0);

        let wide = image.cropped([0.0, 0.25, 1.0, 0.5], 0.0, Default::default());
        assert_eq!((wide.width, wide.height), (4, 2));
    }

    #[test]
    fn a_vertical_keystone_moves_a_line_one_way_at_the_top_and_the_other_below() {
        use crate::document::Perspective;

        let (width, height) = (129usize, 129usize);
        let mut data = vec![0.0f32; width * height * 3];
        const LINE: usize = 96;
        for y in 0..height {
            for channel in 0..3 {
                data[(y * width + LINE) * 3 + channel] = 1.0;
            }
        }
        let image = LinearImage::new(width as u32, height as u32, data);

        let perspective = Perspective { vertical: 100.0, ..Default::default() };
        let rect = crop_inside([0.0, 0.0, 1.0, 1.0], 0.0, perspective, width as f32, height as f32);
        let keyed = image.cropped(rect, 0.0, perspective);

        let scale = keyed.width as f32 / width as f32;
        let height = keyed.height as usize;

        let line_at = |image: &LinearImage, row: usize| {
            let width = image.width as usize;
            (0..width)
                .max_by(|a, b| {
                    image.data[(row * width + a) * 3]
                        .total_cmp(&image.data[(row * width + b) * 3])
                })
                .unwrap()
        };

        let middle = height / 2;
        let pivot = line_at(&keyed, middle);
        let top = line_at(&keyed, 1);
        let bottom = line_at(&keyed, height - 2);
        assert!(top as f32 > pivot as f32 + 6.0 * scale, "the top reaches less far, so the line is further out: {top} against {pivot}");
        assert!((bottom as f32) < pivot as f32 - 6.0 * scale, "the bottom reaches further, so the line is closer in: {bottom} against {pivot}");

        {
            let both = Perspective { vertical: -40.0, horizontal: 40.0, ..Default::default() };
            let (w, h) = (image.width as f32, image.height as f32);
            let map = source_map(w, h, [0.0, 0.0, 1.0, 1.0], 0.0, both);

            let points = [(-50.0, -60.0), (0.0, 0.0), (50.0, 60.0)].map(|(x, y)| map(x, y));
            let [(ax, ay), (bx, by), (cx, cy)] = points;
            let cross = (bx - ax) * (cy - ay) - (by - ay) * (cx - ax);
            assert!(cross.abs() < 1e-2, "a straight line bent: {points:?}");
        }

        let perspective = Perspective { horizontal: 100.0, ..Default::default() };
        let rect = crop_inside([0.0, 0.0, 1.0, 1.0], 0.0, perspective, image.width as f32, image.height as f32);
        let sideways = image.cropped(rect, 0.0, perspective);
        assert_eq!(
            line_at(&sideways, 1),
            line_at(&sideways, sideways.height as usize - 2),
            "a horizontal keystone leaves a vertical line vertical"
        );

        let same = image.cropped([0.1, 0.1, 0.5, 0.5], 3.0, Perspective::default());
        let reference = image.cropped([0.1, 0.1, 0.5, 0.5], 3.0, Perspective::default());
        assert!(same.data.iter().zip(&reference.data).all(|(a, b)| a.to_bits() == b.to_bits()));
    }

    #[test]
    fn a_corrected_frame_has_no_smeared_edge() {
        use crate::document::Perspective;

        let (width, height) = (128usize, 128usize);
        let mut data = Vec::with_capacity(width * height * 3);
        for y in 0..height {
            for x in 0..width {

                let value = (x * 7 + y * 13) as f32 / (width * 7 + height * 13) as f32;
                data.extend_from_slice(&[value, value, value]);
            }
        }
        let image = LinearImage::new(width as u32, height as u32, data);

        let longest_run = |image: &LinearImage| {
            let stride = image.width as usize;
            (0..image.height as usize)
                .map(|row| {
                    let at = |x: usize| image.data[(row * stride + x) * 3];
                    let (mut longest, mut run) = (1, 1);
                    for x in 1..stride {
                        if (at(x) - at(x - 1)).abs() < 1e-7 {
                            run += 1;
                            longest = longest.max(run);
                        } else {
                            run = 1;
                        }
                    }
                    longest
                })
                .max()
                .unwrap_or(0)
        };

        for perspective in [
            Perspective { vertical: 100.0, ..Default::default() },
            Perspective { vertical: -60.0, horizontal: 45.0, ..Default::default() },
            Perspective { horizontal: -100.0, aspect: 40.0, ..Default::default() },
        ] {
            let fitted = crop_inside([0.0, 0.0, 1.0, 1.0], 0.0, perspective, 128.0, 128.0);
            assert!(fitted[2] < 1.0 && fitted[3] < 1.0, "{perspective:?} was not pulled in");
            let keyed = image.cropped(fitted, 0.0, perspective);
            let run = longest_run(&keyed);
            assert!(run < 6, "{perspective:?} left a smear {run} px long");
        }

        let whole = [0.0, 0.0, 1.0, 1.0];
        assert_eq!(crop_inside(whole, 0.0, Perspective::default(), 128.0, 128.0), whole);
        let plain = image.cropped(whole, 0.0, Perspective::default());
        assert!(plain
            .data
            .iter()
            .zip(&image.data)
            .all(|(a, b)| (a - b).abs() < 1e-6));
    }

    #[test]
    fn a_fitted_crop_is_as_large_as_the_photograph_allows() {
        use crate::document::Perspective;

        let (width, height, angle) = (600.0, 800.0, -4.1);
        let perspective = Perspective { vertical: -40.0, horizontal: 40.0, ..Default::default() };
        let whole = [0.0, 0.0, 1.0, 1.0];
        let fitted = crop_inside(whole, angle, perspective, width, height);

        let inside = |rect: [f32; 4]| {
            let source_of = source_map(width, height, rect, angle, perspective);
            let (hw, hh) = (rect[2] * width / 2.0, rect[3] * height / 2.0);
            [(-hw, -hh), (hw, -hh), (hw, hh), (-hw, hh)].iter().all(|(dx, dy)| {
                let (sx, sy) = source_of(*dx, *dy);
                (-0.01..=width - 0.99).contains(&sx) && (-0.01..=height - 0.99).contains(&sy)
            })
        };
        assert!(inside(fitted), "{fitted:?} reaches past the photograph");
        assert!((fitted[2] / fitted[3] - 1.0).abs() < 1e-3, "the shape is kept: {fitted:?}");
        assert!(fitted[0] >= -1e-4 && fitted[1] >= -1e-4 && fitted[0] + fitted[2] <= 1.0001 && fitted[1] + fitted[3] <= 1.0001);

        let (mut small, mut large) = (0.05f32, 1.0f32);
        for _ in 0..30 {
            let middle = (small + large) / 2.0;
            let centred = [(1.0 - middle) / 2.0, (1.0 - middle) / 2.0, middle, middle];
            if inside(centred) { small = middle } else { large = middle }
        }
        assert!(fitted[2] > small * 1.1, "not larger than the centred crop: {} against {small}", fitted[2]);

        let grown = fitted[2] * 1.03;
        let nudges = [-0.02f32, 0.0, 0.02];
        let fits_larger = nudges.iter().any(|nx| nudges.iter().any(|ny| {
            let rect = [fitted[0] + nx - (grown - fitted[2]) / 2.0, fitted[1] + ny - (grown - fitted[3]) / 2.0, grown, grown];
            inside(rect) && rect[0] >= 0.0 && rect[1] >= 0.0 && rect[0] + grown <= 1.0 && rect[1] + grown <= 1.0
        }));
        assert!(!fits_larger, "a larger crop fits beside {fitted:?}");
    }

    #[test]
    fn straighten_rotates_about_the_crop_centre() {

        let mut data = Vec::new();
        for _ in 0..32 {
            for x in 0..32 {
                let value = if x < 16 { 0.0 } else { 1.0 };
                data.extend_from_slice(&[value; 3]);
            }
        }
        let image = LinearImage::new(32, 32, data);

        let straight = image.cropped([0.25, 0.25, 0.5, 0.5], 0.0, Default::default());
        let turned = image.cropped([0.25, 0.25, 0.5, 0.5], 90.0, Default::default());
        assert_eq!((turned.width, turned.height), (straight.width, straight.height));

        let at = |img: &LinearImage, x: usize, y: usize| {
            img.data[(y * img.width as usize + x) * 3]
        };

        assert_eq!(at(&straight, 2, 8), 0.0);
        assert_eq!(at(&straight, 13, 8), 1.0);

        assert!(
            (at(&turned, 8, 2) - at(&turned, 8, 13)).abs() > 0.5,
            "a 90 degree turn should put the edge horizontal"
        );
        assert!(
            (at(&turned, 2, 8) - at(&turned, 13, 8)).abs() < 0.5,
            "and take it out of the vertical"
        );

        let nudged = image.cropped([0.25, 0.25, 0.5, 0.5], 2.0, Default::default());
        assert_eq!(at(&nudged, 2, 8), 0.0, "a 2 degree tilt should not reach the edge");
    }

    #[test]
    fn sampling_outside_the_frame_is_black_and_the_frame_is_not() {

        let image = LinearImage::new(4, 4, vec![0.5; 48]);
        let beyond = image.cropped([-1.0, -1.0, 3.0, 3.0], 0.0, Default::default());
        assert_eq!((beyond.width, beyond.height), (12, 12));
        let at = |x: usize, y: usize| beyond.data[(y * 12 + x) * 3];
        assert_eq!(at(0, 0), 0.0, "far outside is nothing, not stretched edge");
        assert_eq!(at(11, 6), 0.0);
        assert!((at(4, 4) - 0.5).abs() < 1e-5 && (at(7, 7) - 0.5).abs() < 1e-5, "the frame's own edge keeps its colour");

        let same = image.cropped([0.0, 0.0, 1.0, 1.0], 0.0, Default::default());
        assert!(same.data.iter().all(|value| (*value - 0.5).abs() < 1e-5));
    }

    #[test]
    fn orientation_moves_pixels_where_the_tag_says() {
        let image = coordinates();
        let at = |img: &LinearImage, x: usize, y: usize| {
            let o = (y * img.width as usize + x) * 3;
            (img.data[o], img.data[o + 1])
        };

        let same = image.oriented(false, false, false);
        assert_eq!((same.width, same.height), (3, 2));
        assert_eq!(at(&same, 2, 1), (2.0, 1.0));

        let swapped = image.oriented(true, false, false);
        assert_eq!((swapped.width, swapped.height), (2, 3));
        assert_eq!(at(&swapped, 1, 2), (2.0, 1.0), "(x,y) must land at (y,x)");

        let mirrored = image.oriented(false, true, false);
        assert_eq!((mirrored.width, mirrored.height), (3, 2));
        assert_eq!(at(&mirrored, 0, 1), (2.0, 1.0));

        let rotated = image.oriented(true, true, false);
        assert_eq!((rotated.width, rotated.height), (2, 3));

        assert_eq!(at(&rotated, 0, 0), (2.0, 0.0));

        for image in [swapped, mirrored, rotated] {
            assert_eq!(
                image.data.len(),
                image.pixel_count() * 3,
                "buffer does not match the new dimensions"
            );
        }
    }

    #[test]
    fn orientation_carries_the_profile_through() {
        let image = coordinates().oriented(true, true, false);
        assert!(image.profile.is_none());

        let small = image.downscaled(1).unwrap();
        assert_eq!((small.width, small.height), (1, 1));
    }

    #[test]
    fn downscale_averages_and_keeps_aspect() {

        let mut data = Vec::new();
        for _ in 0..2 {
            for x in 0..4 {
                let value = if x < 2 { 0.0 } else { 1.0 };
                data.extend_from_slice(&[value; 3]);
            }
        }
        let image = LinearImage::new(4, 2, data);

        let half = image.downscaled(2).unwrap();
        assert_eq!((half.width, half.height), (2, 1));

        assert_eq!(half.data[0], 0.0);
        assert_eq!(half.data[3], 1.0);

        let one = image.downscaled(1).unwrap();
        assert_eq!((one.width, one.height), (1, 1));
        assert!((one.data[0] - 0.5).abs() < 1e-6, "got {}", one.data[0]);

        assert!(image.downscaled(4).is_none(), "already small enough");
        assert!(image.downscaled(99).is_none());
    }
}
