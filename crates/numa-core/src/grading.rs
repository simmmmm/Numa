use serde::{Deserialize, Serialize};

use super::tone;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, Default)]
pub struct Range {

    pub hue: f32,

    pub saturation: f32,

    pub luminance: f32,
}

impl Range {
    fn is_identity(&self) -> bool {
        self.saturation.abs() < 1e-3 && self.luminance.abs() < 1e-3
    }

    fn direction(&self) -> [f32; 3] {
        let rgb = hue_to_rgb(self.hue);
        let luma = luminance(rgb);
        [rgb[0] - luma, rgb[1] - luma, rgb[2] - luma]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Grading {
    pub shadows: Range,
    pub midtones: Range,
    pub highlights: Range,

    pub global: Range,

    pub blending: f32,

    pub balance: f32,
}

impl Default for Grading {
    fn default() -> Self {
        Self {
            shadows: Range::default(),
            midtones: Range::default(),
            highlights: Range::default(),
            global: Range::default(),
            blending: 50.0,
            balance: 0.0,
        }
    }
}

impl Grading {

    pub fn is_identity(&self) -> bool {
        self.shadows.is_identity()
            && self.midtones.is_identity()
            && self.highlights.is_identity()
            && self.global.is_identity()
    }

    pub fn weights(&self, display: f32) -> [f32; 3] {
        let blend = (self.blending / 100.0).clamp(0.0, 1.0);
        let balance = (self.balance / 100.0).clamp(-1.0, 1.0);

        let shifted = display.clamp(0.0, 1.0).powf(2.0f32.powf(-balance));

        let reach = 4.0 - blend * 2.0;
        let shadow = (1.0 - shifted).powf(reach);
        let highlight = shifted.powf(reach);
        [shadow, (1.0 - shadow - highlight).max(0.0), highlight]
    }

    pub fn apply(&self, rgb: [f32; 3]) -> [f32; 3] {
        if self.is_identity() {
            return rgb;
        }

        let display = tone::curve(luminance(rgb).max(0.0));
        let [shadow, midtone, highlight] = self.weights(display);

        let mut gain = [1.0f32; 3];
        let mut stops = 0.0f32;
        for (range, weight) in [
            (&self.shadows, shadow),
            (&self.midtones, midtone),
            (&self.highlights, highlight),

            (&self.global, 1.0),
        ] {
            if weight <= 0.0 || range.is_identity() {
                continue;
            }
            let amount = weight * (range.saturation / 100.0);
            let direction = range.direction();
            for channel in 0..3 {
                gain[channel] *= 1.0 + direction[channel] * amount;
            }
            stops += weight * range.luminance / 100.0;
        }

        let lift = stops.exp2();
        [
            (rgb[0] * gain[0] * lift).max(0.0),
            (rgb[1] * gain[1] * lift).max(0.0),
            (rgb[2] * gain[2] * lift).max(0.0),
        ]
    }
}

fn luminance(rgb: [f32; 3]) -> f32 {
    0.2126 * rgb[0] + 0.7152 * rgb[1] + 0.0722 * rgb[2]
}

fn hue_to_rgb(degrees: f32) -> [f32; 3] {
    let hue = (degrees / 60.0).rem_euclid(6.0);
    let sector = hue.floor();
    let fraction = hue - sector;
    let rising = fraction;
    let falling = 1.0 - fraction;

    match sector as i32 {
        0 => [1.0, rising, 0.0],
        1 => [falling, 1.0, 0.0],
        2 => [0.0, 1.0, rising],
        3 => [0.0, falling, 1.0],
        4 => [rising, 0.0, 1.0],
        _ => [1.0, 0.0, falling],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn nothing_set_changes_nothing() {
        let grading = Grading::default();
        assert!(grading.is_identity());
        let pixel = [0.3, 0.2, 0.1];
        assert_eq!(grading.apply(pixel), pixel);
    }

    #[test]
    fn the_two_shaping_sliders_do_nothing_on_their_own() {
        let grading = Grading { blending: 0.0, balance: -80.0, ..Default::default() };
        assert!(grading.is_identity());
        assert_eq!(grading.apply([0.3, 0.2, 0.1]), [0.3, 0.2, 0.1]);
    }

    #[test]
    fn the_three_ranges_always_sum_to_one() {
        for blending in [0.0, 50.0, 100.0] {
            for balance in [-100.0, 0.0, 100.0] {
                let grading = Grading { blending, balance, ..Default::default() };
                for step in 0..=20 {
                    let display = step as f32 / 20.0;
                    let total: f32 = grading.weights(display).iter().sum();
                    assert!(
                        (total - 1.0).abs() < 1e-4,
                        "blending {blending}, balance {balance}, at {display}: {total}"
                    );
                }
            }
        }
    }

    #[test]
    fn the_ends_of_the_range_belong_to_one_range_each() {
        for balance in [-100.0, 0.0, 100.0] {
            let grading = Grading { balance, ..Default::default() };
            assert!(grading.weights(0.0)[0] > 0.999, "black is shadow");
            assert!(grading.weights(1.0)[2] > 0.999, "white is highlight");
        }
    }

    #[test]
    fn balance_moves_the_boundary() {
        let middle = 0.5;
        let towards_highlights = Grading { balance: 100.0, ..Default::default() };
        let towards_shadows = Grading { balance: -100.0, ..Default::default() };
        assert!(
            towards_highlights.weights(middle)[2] > towards_shadows.weights(middle)[2],
            "a positive balance should make a midtone read more as a highlight"
        );
    }

    #[test]
    fn a_tint_holds_the_brightness_it_was_given() {
        let grading = Grading {
            global: Range { hue: 240.0, saturation: 100.0, luminance: 0.0 },
            ..Default::default()
        };
        let pixel = [0.2, 0.2, 0.2];
        let graded = grading.apply(pixel);
        assert!(graded[2] > graded[0], "240 degrees should be blue");

        let before = luminance(pixel);
        let after = luminance(graded);

        assert!(
            (after / before).log2().abs() < 0.2,
            "a tint moved the brightness by {:+.2} EV",
            (after / before).log2()
        );
    }

    #[test]
    fn a_hundred_of_luminance_is_a_stop() {
        let grading = Grading {
            global: Range { hue: 0.0, saturation: 0.0, luminance: 100.0 },
            ..Default::default()
        };
        let graded = grading.apply([0.2, 0.2, 0.2]);
        assert!((graded[0] / 0.2 - 2.0).abs() < 1e-4, "{graded:?}");
    }

    #[test]
    fn warm_highlights_against_cool_shadows() {
        let grading = Grading {
            shadows: Range { hue: 240.0, saturation: 80.0, luminance: 0.0 },
            highlights: Range { hue: 40.0, saturation: 80.0, luminance: 0.0 },
            ..Default::default()
        };
        let dark = grading.apply([0.01, 0.01, 0.01]);
        let bright = grading.apply([2.0, 2.0, 2.0]);
        assert!(dark[2] > dark[0], "the shadows should have gone blue: {dark:?}");
        assert!(bright[0] > bright[2], "the highlights should have gone warm: {bright:?}");
    }
}
