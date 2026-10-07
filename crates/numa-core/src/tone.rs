use crate::profile::{multiply, multiply_matrix, Matrix3};
use crate::space::ColourSpace;

pub const MIDDLE_GREY: f32 = 0.18;

const CAMERAS: [f32; TABLE_LEN] = [
    0.00772, 0.00854, 0.00945, 0.01045, 0.01156, 0.01279, 0.01415, 0.01565,
    0.01732, 0.01916, 0.02119, 0.02344, 0.02597, 0.02912, 0.03387, 0.04034,
    0.04817, 0.05723, 0.06744, 0.07886, 0.09166, 0.10660, 0.12398, 0.14376,
    0.16643, 0.19251, 0.22177, 0.25442, 0.29093, 0.33042, 0.37222, 0.41609,
    0.46137, 0.50807, 0.55549, 0.60278, 0.64992, 0.69667, 0.74153, 0.78407,
    0.82428, 0.86073, 0.89175, 0.91676, 0.93468, 0.94930, 0.96408, 0.97631,
    0.98601, 0.99317, 0.99779, 0.99987, 1.00000,
];
const TABLE_LEN: usize = 53;

const FIRST_STOP: f32 = -8.0;
const STEPS_PER_STOP: f32 = 4.0;

const TOE: f32 = 1.715;

#[cfg(test)]
const GREY_DISPLAY: f32 = 0.461_37;

pub fn curve(value: f32) -> f32 {

    at_stops((value.max(1e-6) / MIDDLE_GREY).log2())
}

fn at_stops(stops: f32) -> f32 {
    let at = (stops - FIRST_STOP) * STEPS_PER_STOP;
    if at <= 0.0 {
        return CAMERAS[0] * (at / STEPS_PER_STOP / TOE).exp2();
    }

    if at >= (TABLE_LEN - 1) as f32 {
        return CAMERAS[TABLE_LEN - 1];
    }

    let low = at as usize;
    let t = at - low as f32;
    CAMERAS[low] * (1.0 - t) + CAMERAS[low + 1] * t
}

pub fn scene_value_for(display: f32) -> f32 {
    let wanted = display.clamp(1e-4, 1.0 - 1e-4);
    let stops = if wanted <= CAMERAS[STEEP] {
        near(wanted)
    } else if wanted <= 1.0 {

        let margin = 8.0 * f32::EPSILON;
        halved(wanted, (guess(wanted - margin), guess(wanted + margin)))
    } else {

        halved(wanted, (f32::NEG_INFINITY, f32::INFINITY))
    };
    MIDDLE_GREY * stops.exp2()
}

const STEEP: usize = 48;

fn halved(wanted: f32, (under, over): (f32, f32)) -> f32 {
    let (mut low, mut high) = (-24.0f32, FIRST_STOP + TABLE_LEN as f32 / STEPS_PER_STOP);
    for _ in 0..48 {
        let middle = (low + high) / 2.0;
        let short = middle < under || (middle <= over && at_stops(middle) < wanted);
        low = if short { middle } else { low };
        high = if short { high } else { middle };
    }
    high
}

fn guess(wanted: f32) -> f32 {
    if wanted <= CAMERAS[0] {
        return FIRST_STOP + (wanted / CAMERAS[0]).log2() * TOE;
    }
    let low = CAMERAS.partition_point(|&value| value < wanted) - 1;
    let t = (wanted - CAMERAS[low]) / (CAMERAS[low + 1] - CAMERAS[low]);
    FIRST_STOP + (low as f32 + t) / STEPS_PER_STOP
}

fn near(wanted: f32) -> f32 {

    let order = |x: f32| match x.to_bits() {
        bits if bits >> 31 == 1 => !bits,
        bits => bits | 1 << 31,
    };
    let float = |at: u32| f32::from_bits(if at >> 31 == 1 { at & !(1 << 31) } else { !at });
    let short = |at: u32| at_stops(float(at)) < wanted;
    let guess = order(guess(wanted));
    for reach in [4, 64, 1 << 10, 1 << 16, 1 << 24] {
        let (mut low, mut high) = (guess - reach, guess + reach);
        if short(low) && !short(high) {

            while high - low > 1 {
                let middle = low + (high - low) / 2;
                let under = short(middle);
                low = if under { middle } else { low };
                high = if under { high } else { middle };
            }
            return float(high);
        }
    }
    halved(wanted, (f32::NEG_INFINITY, f32::INFINITY))
}

pub fn shown(value: f32, display_referred: bool) -> f32 {
    match display_referred {
        true => crate::space::ColourSpace::Srgb.encode(value),
        false => curve(value),
    }
}

pub fn scene_for(display: f32, display_referred: bool) -> f32 {
    match display_referred {
        true => crate::space::ColourSpace::Srgb.decode(display),
        false => scene_value_for(display),
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum ToneMapping {
    #[default]
    Camera,
    Agx,
}

impl ToneMapping {
    pub fn is_camera(&self) -> bool {
        *self == ToneMapping::Camera
    }
}

pub struct Agx {

    into: Matrix3,

    back: Matrix3,
}

const SRGB_TO_REC2020: Matrix3 = [
    [0.627_403_9, 0.329_283_04, 0.043_313_066],
    [0.069_097_29, 0.919_540_4, 0.011_362_316],
    [0.016_391_439, 0.088_013_31, 0.895_595_3],
];
const REC2020_TO_SRGB: Matrix3 = [
    [1.660_491, -0.587_641_14, -0.072_849_865],
    [-0.124_550_48, 1.132_899_9, -0.008_349_423],
    [-0.018_150_763, -0.100_578_9, 1.118_729_7],
];
const AGX_INSET: Matrix3 = [
    [0.856_627_15, 0.095_121_24, 0.048_251_606],
    [0.137_318_97, 0.761_242, 0.101_439_04],
    [0.111_898_21, 0.076_799_42, 0.811_302_4],
];
const AGX_OUTSET: Matrix3 = [
    [1.127_100_6, -0.110_606_64, -0.016_493_939],
    [-0.141_329_76, 1.157_823_7, -0.016_493_939],
    [-0.141_329_76, -0.110_606_64, 1.251_936_4],
];

const AGX_MIN_EV: f32 = -12.473_93;
const AGX_MAX_EV: f32 = 4.026_069;

impl Agx {
    pub fn new(working: ColourSpace) -> Agx {
        let into = multiply_matrix(&AGX_INSET, &SRGB_TO_REC2020);
        Agx {
            into: working.convert_to(ColourSpace::Srgb).map_or(into, |to| multiply_matrix(&into, &to)),
            back: ColourSpace::Srgb.convert_to(working).map_or(REC2020_TO_SRGB, |from| multiply_matrix(&from, &REC2020_TO_SRGB)),
        }
    }

    pub fn shown(&self, rgb: [f32; 3]) -> [f32; 3] {
        let sigmoid = multiply(&self.into, rgb).map(|value| {
            agx_contrast(((value.max(1e-10).log2() - AGX_MIN_EV) / (AGX_MAX_EV - AGX_MIN_EV)).clamp(0.0, 1.0))
        });
        let linear = multiply(&AGX_OUTSET, sigmoid).map(|value| value.max(0.0).powf(2.2));
        multiply(&self.back, linear).map(|value| ColourSpace::Srgb.encode(value))
    }
}

fn agx_contrast(x: f32) -> f32 {
    let (x2, x4) = (x * x, x * x * x * x);
    15.5 * x4 * x2 - 40.14 * x4 * x + 31.96 * x4 - 6.868 * x2 * x + 0.4298 * x2 + 0.1191 * x - 0.002_32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_infinity_is_white() {
        assert_eq!(curve(f32::INFINITY), 1.0);
        assert_eq!(curve(f32::MAX), 1.0);

        assert_eq!(curve(f32::NAN), curve(0.0));
    }

    #[test]
    fn middle_grey_is_pinned() {
        assert!((curve(MIDDLE_GREY) - GREY_DISPLAY).abs() < 1e-5);
        assert_eq!((curve(MIDDLE_GREY) * 255.0 + 0.5) as u8, 118);
    }

    #[test]
    fn the_curve_is_monotonic_and_bounded() {
        let mut previous = 0.0;
        for step in 0..60 {
            let value = curve(0.001 * 1.3f32.powi(step));
            assert!(value >= previous, "went backwards at step {step}");
            assert!((0.0..=1.0).contains(&value));
            previous = value;
        }

        assert!(curve(0.0) < 0.001, "black");
        assert!(curve(1000.0) > 0.999, "very bright");

        assert!((0.85..0.99).contains(&curve(1.0)), "got {}", curve(1.0));
    }

    #[test]
    fn the_inverse_undoes_the_curve() {
        for scene in [0.001f32, 0.05, MIDDLE_GREY, 0.5, 1.0, 4.0] {
            let back = scene_value_for(curve(scene));
            assert!(
                (back / scene - 1.0).abs() < 0.01,
                "{scene} became {back} after a round trip"
            );
        }

        for display in [0.05f32, 0.2, GREY_DISPLAY, 0.8, 0.95] {
            let back = curve(scene_value_for(display));
            assert!((back - display).abs() < 1e-3, "{display} became {back}");
        }
    }

    #[test]
    fn the_inverse_refuses_the_unreachable() {

        assert!(scene_value_for(0.0).is_finite());
        assert!(scene_value_for(1.0).is_finite());
        assert!(scene_value_for(0.0) > 0.0);
    }

    #[test]
    fn agx_takes_a_bright_colour_toward_white() {
        let agx = Agx::new(ColourSpace::Srgb);
        let grey = agx.shown([MIDDLE_GREY; 3]);
        assert!(grey.iter().all(|value| (value - grey[0]).abs() < 1e-3), "{grey:?}");
        assert!((0.45..0.55).contains(&grey[0]), "{grey:?}");
        assert!(agx.shown([0.0; 3])[0] < 0.01);
        let mut previous = 0.0;
        for step in 0..60 {
            let value = agx.shown([0.001 * 1.3f32.powi(step); 3])[1];
            assert!(value >= previous, "went backwards at step {step}");
            previous = value;
        }

        let spread = |rgb: [f32; 3]| rgb.iter().fold(f32::MIN, |a, &b| a.max(b)) - rgb.iter().fold(f32::MAX, |a, &b| a.min(b));
        let blue = |stops: f32| [0.05, 0.1, 1.0].map(|value: f32| value * MIDDLE_GREY * stops.exp2());
        assert!(spread(agx.shown(blue(3.0))) < spread(blue(3.0).map(curve)) - 0.1);
        assert!(agx.shown(blue(10.0)).iter().all(|&value| value > 0.9), "{:?}", agx.shown(blue(10.0)));
    }

    #[test]
    fn agx_is_three_js() {
        let agx = Agx::new(ColourSpace::Srgb);
        for (light, theirs) in [
            ([0.18, 0.18, 0.18], [0.5005, 0.5005, 0.5005]),
            ([0.3, 0.12, 0.05], [0.6024, 0.4277, 0.3101]),
            ([0.072, 0.144, 1.44], [0.4377, 0.5817, 0.8934]),
            ([4.0, 1.0, 0.2], [0.9722, 0.8112, 0.6807]),
        ] {
            let ours = agx.shown(light);
            assert!(ours.iter().zip(theirs).all(|(a, b)| (a - b).abs() < 2e-3), "{light:?}: {ours:?} against {theirs:?}");
        }
    }

    #[test]
    fn agx_is_the_same_in_another_working_space() {
        let colour = [0.3f32, 0.12, 0.05];
        let srgb = Agx::new(ColourSpace::Srgb).shown(colour);
        let into = ColourSpace::Srgb.convert_to(ColourSpace::DisplayP3).unwrap();
        let back = ColourSpace::DisplayP3.convert_to(ColourSpace::Srgb).unwrap();
        let p3 = Agx::new(ColourSpace::DisplayP3).shown(multiply(&into, colour));
        let again = multiply(&back, p3.map(|value| ColourSpace::Srgb.decode(value))).map(|value| ColourSpace::Srgb.encode(value));
        assert!(srgb.iter().zip(again).all(|(a, b)| (a - b).abs() < 1e-3), "{srgb:?} {again:?}");
    }

    fn original_at_stops(stops: f32) -> f32 {
        let at = (stops - FIRST_STOP) * STEPS_PER_STOP;
        if at <= 0.0 {
            return CAMERAS[0] * (at / STEPS_PER_STOP / TOE).exp2();
        }
        if at >= (TABLE_LEN - 1) as f32 {
            return CAMERAS[TABLE_LEN - 1];
        }
        let (low, t) = (at.floor() as usize, at.fract());
        CAMERAS[low] * (1.0 - t) + CAMERAS[low + 1] * t
    }

    fn original_scene_value_for(display: f32) -> f32 {
        let wanted = display.clamp(1e-4, 1.0 - 1e-4);
        let (mut low, mut high) = (-24.0f32, FIRST_STOP + TABLE_LEN as f32 / STEPS_PER_STOP);
        for _ in 0..48 {
            let middle = (low + high) / 2.0;
            if original_at_stops(middle) < wanted { low = middle } else { high = middle }
        }
        MIDDLE_GREY * high.exp2()
    }

    fn agrees_with_the_halving(step: usize) {
        use rayon::prelude::*;
        let halving = original_scene_value_for;
        let (from, to) = (1e-4f32.to_bits(), (1.0f32 - 1e-4).to_bits());
        let differ: Vec<f32> = (0..=(to - from) / step as u32)
            .into_par_iter()
            .map(|at| f32::from_bits(from + at * step as u32))
            .filter(|&display| scene_value_for(display).to_bits() != halving(display).to_bits())
            .collect();
        assert!(differ.is_empty(), "{} differ, first {:?}", differ.len(), &differ[..differ.len().min(5)]);
        assert_eq!(scene_value_for(f32::NAN).to_bits(), halving(f32::NAN).to_bits());
    }

    #[test]
    fn the_inverse_is_the_halvings_float_for_float() {
        agrees_with_the_halving(61);
    }

    #[test]
    #[ignore]
    fn the_inverse_is_the_halvings_on_every_float() {
        use rayon::prelude::*;
        agrees_with_the_halving(1);
        let (from, to) = (FIRST_STOP - 1.0, FIRST_STOP + TABLE_LEN as f32 / STEPS_PER_STOP);
        let differ = (0..=u32::MAX)
            .into_par_iter()
            .map(f32::from_bits)
            .filter(|stops| (from..to).contains(stops) && at_stops(*stops).to_bits() != original_at_stops(*stops).to_bits())
            .count();
        assert_eq!(differ, 0);
    }
}
