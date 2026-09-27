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
    let (low, t) = (at.floor() as usize, at.fract());
    if low + 1 >= TABLE_LEN {
        return CAMERAS[TABLE_LEN - 1];
    }
    CAMERAS[low] * (1.0 - t) + CAMERAS[low + 1] * t
}

pub fn scene_value_for(display: f32) -> f32 {
    let wanted = display.clamp(1e-4, 1.0 - 1e-4);
    let (mut low, mut high) = (-24.0f32, FIRST_STOP + TABLE_LEN as f32 / STEPS_PER_STOP);
    for _ in 0..48 {
        let middle = (low + high) / 2.0;
        if at_stops(middle) < wanted { low = middle } else { high = middle }
    }
    MIDDLE_GREY * high.exp2()
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
