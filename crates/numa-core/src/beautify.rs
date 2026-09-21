use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Portrait {

    pub at: [f32; 4],

    pub points: [[f32; 2]; 5],
}

#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Beautify {

    #[serde(default)]
    pub spots: f32,

    pub skin: f32,

    pub evenness: f32,

    pub red_eye: f32,

    pub teeth: f32,
}

impl Beautify {
    pub fn is_identity(&self) -> bool {
        self.spots <= 0.0
            && self.skin <= 0.0
            && self.evenness <= 0.0
            && self.red_eye <= 0.0
            && self.teeth <= 0.0
    }

    pub fn wants_skin(&self) -> bool {
        self.spots > 0.0 || self.skin > 0.0 || self.evenness > 0.0
    }
}

pub fn skin_at(face: &Portrait, u: f32, v: f32) -> f32 {
    let inside = ellipse(face.at, u, v, 0.92);
    if inside <= 0.0 {
        return 0.0;
    }

    let scale = face.at[2].max(face.at[3]);
    let mut open = 1.0f32;
    for point in [face.points[0], face.points[1]] {
        open = open.min(1.0 - disc(point, u, v, scale * 0.16, 0.6));
    }

    open = open.min(1.0 - mouth_at(face, u, v));

    let brow = (face.points[0][1] + face.points[1][1]) * 0.5 - scale * 0.13;
    let across = (face.points[0][0] + face.points[1][0]) * 0.5;
    open = open.min(1.0 - disc([across, brow], u, v, scale * 0.3, 0.8) * 0.8);

    inside * open.max(0.0)
}

pub fn eyes_at(face: &Portrait, u: f32, v: f32) -> f32 {
    let scale = face.at[2].max(face.at[3]);
    let right = disc(face.points[0], u, v, scale * 0.10, 0.5);
    let left = disc(face.points[1], u, v, scale * 0.10, 0.5);
    right.max(left)
}

pub fn mouth_at(face: &Portrait, u: f32, v: f32) -> f32 {
    let (right, left) = (face.points[3], face.points[4]);
    let centre = [(right[0] + left[0]) * 0.5, (right[1] + left[1]) * 0.5];

    let half_width = ((right[0] - left[0]).abs() * 0.78).max(1e-4);

    let half_height = (half_width * 0.45).max(1e-4);

    ellipse(
        [centre[0] - half_width, centre[1] - half_height, half_width * 2.0, half_height * 2.0],
        u,
        v,
        MOUTH_FILL,
    )
}

const MOUTH_FILL: f32 = 0.6;

fn ellipse(at: [f32; 4], u: f32, v: f32, fill: f32) -> f32 {
    let (half_w, half_h) = (at[2] * 0.5, at[3] * 0.5);
    if half_w <= 0.0 || half_h <= 0.0 {
        return 0.0;
    }
    let dx = (u - (at[0] + half_w)) / half_w;
    let dy = (v - (at[1] + half_h)) / half_h;
    ramp(dx.hypot(dy), fill)
}

fn disc(centre: [f32; 2], u: f32, v: f32, radius: f32, fill: f32) -> f32 {
    if radius <= 0.0 {
        return 0.0;
    }
    ramp((u - centre[0]).hypot(v - centre[1]) / radius, fill)
}

fn ramp(distance: f32, fill: f32) -> f32 {
    let fill = fill.clamp(0.0, 0.99);
    if distance <= fill {
        return 1.0;
    }
    if distance >= 1.0 {
        return 0.0;
    }
    let t = (distance - fill) / (1.0 - fill);
    1.0 - t * t * (3.0 - 2.0 * t)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn portrait() -> Portrait {
        Portrait {
            at: [0.3, 0.2, 0.4, 0.5],
            points: [
                [0.40, 0.36],
                [0.60, 0.36],
                [0.50, 0.46],
                [0.43, 0.58],
                [0.57, 0.58],
            ],
        }
    }

    #[test]
    fn nothing_is_cut_out_of_the_skin_with_a_hard_edge() {
        let face = portrait();

        const ACROSS: usize = 2048;
        let step = 1.0 / ACROSS as f32;

        let worst = |v: f32| {
            (1..ACROSS)
                .map(|x| {
                    let u = x as f32 * step;
                    (skin_at(&face, u, v) - skin_at(&face, u - step, v)).abs()
                })
                .fold(0.0f32, f32::max)
        };

        for (name, v) in [("eyes", 0.36), ("nose", 0.46), ("mouth", 0.58)] {
            let jump = worst(v);
            println!("{name}: {jump:.4} per pixel");
            assert!(jump < 0.08, "a step of {jump} across one pixel at the {name}");
        }

        let down = (1..ACROSS)
            .map(|y| {
                let v = y as f32 * step;
                (skin_at(&face, 0.5, v) - skin_at(&face, 0.5, v - step)).abs()
            })
            .fold(0.0f32, f32::max);
        println!("down the face: {down:.4} per pixel");
        assert!(down < 0.08, "a step of {down} across one pixel down the face");
    }

    #[test]
    fn the_cheek_is_skin_and_the_wall_is_not() {
        let face = portrait();
        assert!(skin_at(&face, 0.37, 0.50) > 0.8, "a cheek");
        assert_eq!(skin_at(&face, 0.05, 0.05), 0.0, "the corner of the frame");
        assert_eq!(skin_at(&face, 0.9, 0.9), 0.0, "and the other corner");
    }

    #[test]
    fn the_features_are_left_alone() {
        let face = portrait();
        for (name, at) in [
            ("right eye", face.points[0]),
            ("left eye", face.points[1]),
            ("mouth", [(face.points[3][0] + face.points[4][0]) * 0.5, face.points[3][1]]),
        ] {
            let skin = skin_at(&face, at[0], at[1]);
            assert!(skin < 0.1, "{name} reads as skin at {skin}");
        }
    }

    #[test]
    fn the_eyes_are_where_the_eyes_are() {
        let face = portrait();
        assert!(eyes_at(&face, 0.40, 0.36) > 0.9, "on the right eye");
        assert!(eyes_at(&face, 0.60, 0.36) > 0.9, "on the left eye");
        assert_eq!(eyes_at(&face, 0.50, 0.46), 0.0, "the nose between them");
    }

    #[test]
    fn the_mouth_is_wider_than_it_is_tall() {
        let face = portrait();
        let centre = [0.50, 0.58];
        assert!(mouth_at(&face, centre[0], centre[1]) > 0.9);

        assert!(mouth_at(&face, centre[0] + 0.05, centre[1]) > 0.0);
        assert_eq!(mouth_at(&face, centre[0], centre[1] + 0.05), 0.0);
    }

    #[test]
    fn nothing_set_is_nothing_to_do() {
        assert!(Beautify::default().is_identity());
        assert!(!Beautify::default().wants_skin());
        assert!(Beautify { evenness: 20.0, ..Default::default() }.wants_skin());
        assert!(!Beautify { teeth: 20.0, ..Default::default() }.wants_skin());
    }
}
