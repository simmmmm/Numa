pub const MIDDLE_GREY: f32 = 0.18;

const CURVE_CONTRAST: f32 = 0.939;

const GREY_DISPLAY: f32 = 0.461_37;

fn grey_offset() -> f32 {
    -(1.0 / GREY_DISPLAY - 1.0).ln()
}

pub fn curve(value: f32) -> f32 {

    let stops = (value.max(1e-6) / MIDDLE_GREY).log2();
    1.0 / (1.0 + (-(CURVE_CONTRAST * stops + grey_offset())).exp())
}

pub fn scene_value_for(display: f32) -> f32 {

    let clamped = display.clamp(1e-4, 1.0 - 1e-4);
    let stops = ((1.0 / clamped - 1.0).ln().neg() - grey_offset()) / CURVE_CONTRAST;
    MIDDLE_GREY * stops.exp2()
}

use std::ops::Neg;

#[cfg(test)]
mod tests {
    use super::*;

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
}
