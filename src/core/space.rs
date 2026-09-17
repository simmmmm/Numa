use serde::{Deserialize, Serialize};

use crate::core::profile::{multiply_matrix, Matrix3};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum ColourSpace {

    #[default]
    Srgb,

    DisplayP3,

    AdobeRgb,

    ProPhoto,
}

impl ColourSpace {
    pub const ALL: [ColourSpace; 4] =
        [ColourSpace::Srgb, ColourSpace::DisplayP3, ColourSpace::AdobeRgb, ColourSpace::ProPhoto];

    pub fn name(&self) -> &'static str {
        match self {
            ColourSpace::Srgb => "sRGB",
            ColourSpace::DisplayP3 => "Display P3",
            ColourSpace::AdobeRgb => "Adobe RGB",
            ColourSpace::ProPhoto => "ProPhoto RGB",
        }
    }

    pub fn note(&self) -> &'static str {
        match self {
            ColourSpace::Srgb => "Safe everywhere. The right answer unless you know otherwise",
            ColourSpace::DisplayP3 => "Wider reds and greens, and what modern screens show",
            ColourSpace::AdobeRgb => "Wider cyans and greens. What print workflows expect",
            ColourSpace::ProPhoto => "Larger than the eye — a working space, not a delivery one",
        }
    }

    pub fn to_xyz(&self) -> Matrix3 {
        match self {
            ColourSpace::Srgb => SRGB_TO_XYZ_D50,
            ColourSpace::DisplayP3 => P3_TO_XYZ_D50,
            ColourSpace::AdobeRgb => ADOBE_TO_XYZ_D50,
            ColourSpace::ProPhoto => PROPHOTO_TO_XYZ_D50,
        }
    }

    pub fn from_xyz(&self) -> Matrix3 {
        match self {
            ColourSpace::Srgb => XYZ_D50_TO_SRGB,
            ColourSpace::DisplayP3 => XYZ_D50_TO_P3,
            ColourSpace::AdobeRgb => XYZ_D50_TO_ADOBE,
            ColourSpace::ProPhoto => XYZ_D50_TO_PROPHOTO,
        }
    }

    pub fn convert_to(&self, other: ColourSpace) -> Option<Matrix3> {
        (*self != other).then(|| multiply_matrix(&other.from_xyz(), &self.to_xyz()))
    }

    pub fn luminance_weights(&self) -> [f32; 3] {
        match self {
            ColourSpace::Srgb => [0.2126, 0.7152, 0.0722],
            ColourSpace::DisplayP3 => [0.2290, 0.6917, 0.0793],
            ColourSpace::AdobeRgb => [0.2974, 0.6274, 0.0752],
            ColourSpace::ProPhoto => [0.2880, 0.7119, 0.0001],
        }
    }

    pub fn decode(&self, code: f32) -> f32 {
        let code = code.clamp(0.0, 1.0);
        match self {
            ColourSpace::Srgb | ColourSpace::DisplayP3 => match code <= 0.040_45 {
                true => code / 12.92,
                false => ((code + 0.055) / 1.055).powf(2.4),
            },
            other => code.powf(other.power()),
        }
    }

    pub fn encode(&self, linear: f32) -> f32 {
        let linear = linear.clamp(0.0, 1.0);
        match self {
            ColourSpace::Srgb | ColourSpace::DisplayP3 => match linear <= 0.003_130_8 {
                true => linear * 12.92,
                false => 1.055 * linear.powf(1.0 / 2.4) - 0.055,
            },
            other => linear.powf(1.0 / other.power()),
        }
    }

    fn power(&self) -> f32 {
        match self {
            ColourSpace::AdobeRgb => 563.0 / 256.0,
            ColourSpace::ProPhoto => 1.8,

            _ => 2.2,
        }
    }
}

const SRGB_TO_XYZ_D50: Matrix3 = [
    [0.436_074_7, 0.385_064_9, 0.143_080_4],
    [0.222_504_5, 0.716_878_6, 0.060_616_9],
    [0.013_932_2, 0.097_104_5, 0.714_173_3],
];

const XYZ_D50_TO_SRGB: Matrix3 = [
    [3.133_856_1, -1.616_866_7, -0.490_614_6],
    [-0.978_768_4, 1.916_141_5, 0.033_454_0],
    [0.071_945_3, -0.228_991_4, 1.405_242_7],
];

const P3_TO_XYZ_D50: Matrix3 = [
    [0.515_102_0, 0.291_965_0, 0.157_153_0],
    [0.241_182_0, 0.692_236_0, 0.066_582_0],
    [-0.001_050_0, 0.041_879_0, 0.784_378_0],
];

const XYZ_D50_TO_P3: Matrix3 = [
    [2.404_045_0, -0.990_265_0, -0.397_147_0],
    [-0.842_496_0, 1.799_090_0, 0.016_044_0],
    [0.048_223_0, -0.097_252_0, 1.273_827_0],
];

const ADOBE_TO_XYZ_D50: Matrix3 = [
    [0.609_738_0, 0.205_186_0, 0.149_187_0],
    [0.311_111_0, 0.625_681_0, 0.063_208_0],
    [0.019_469_0, 0.060_857_0, 0.744_568_0],
];

const XYZ_D50_TO_ADOBE: Matrix3 = [
    [1.962_517_0, -0.610_651_0, -0.341_384_0],
    [-0.978_749_0, 1.916_129_0, 0.033_454_0],
    [0.028_694_0, -0.140_946_0, 1.349_267_0],
];

const PROPHOTO_TO_XYZ_D50: Matrix3 = [
    [0.797_674_9, 0.135_191_7, 0.031_353_4],
    [0.288_040_2, 0.711_874_1, 0.000_059_9],
    [0.000_000_0, 0.000_000_0, 0.825_210_0],
];

const XYZ_D50_TO_PROPHOTO: Matrix3 = [
    [1.345_943_3, -0.255_607_5, -0.051_111_8],
    [-0.544_598_9, 1.508_167_3, 0.020_535_1],
    [0.000_000_0, 0.000_000_0, 1.211_812_8],
];

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::profile::multiply;

    #[test]
    fn every_space_comes_back_where_it_started() {
        for space in ColourSpace::ALL {
            for colour in [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0], [0.4, 0.6, 0.8]] {
                let there = multiply(&space.to_xyz(), colour);
                let back = multiply(&space.from_xyz(), there);
                for channel in 0..3 {
                    assert!(
                        (back[channel] - colour[channel]).abs() < 2e-3,
                        "{} lost {colour:?} — came back {back:?}",
                        space.name()
                    );
                }
            }
        }
    }

    #[test]
    fn white_stays_neutral_in_every_space() {
        for space in ColourSpace::ALL {
            let xyz = multiply(&space.to_xyz(), [1.0, 1.0, 1.0]);

            assert!((xyz[0] - 0.9642).abs() < 0.004, "{} white x: {xyz:?}", space.name());
            assert!((xyz[1] - 1.0).abs() < 0.004, "{} white y: {xyz:?}", space.name());
            assert!((xyz[2] - 0.8249).abs() < 0.004, "{} white z: {xyz:?}", space.name());
        }
    }

    #[test]
    fn a_space_converted_to_itself_is_no_conversion() {
        for space in ColourSpace::ALL {
            assert!(space.convert_to(space).is_none());
        }
        assert!(ColourSpace::Srgb.convert_to(ColourSpace::AdobeRgb).is_some());
    }

    #[test]
    fn a_wider_space_holds_what_a_narrower_one_clips() {
        let to_wide = ColourSpace::Srgb.convert_to(ColourSpace::ProPhoto).unwrap();
        let wide_red = multiply(&to_wide, [1.0, 0.0, 0.0]);
        assert!(
            wide_red.iter().all(|value| (0.0..=1.0).contains(value)),
            "sRGB's red fits inside ProPhoto: {wide_red:?}"
        );

        let to_narrow = ColourSpace::ProPhoto.convert_to(ColourSpace::Srgb).unwrap();
        let narrow_red = multiply(&to_narrow, [1.0, 0.0, 0.0]);
        assert!(
            narrow_red.iter().any(|value| *value < -0.01),
            "ProPhoto's red does not fit in sRGB: {narrow_red:?}"
        );
    }

    #[test]
    fn encoding_and_decoding_are_each_others_undoing() {
        for space in ColourSpace::ALL {
            for step in 0..=20 {
                let code = step as f32 / 20.0;
                let back = space.encode(space.decode(code));
                assert!(
                    (back - code).abs() < 1e-4,
                    "{} lost {code} — came back {back}",
                    space.name()
                );
            }
        }
    }

    #[test]
    fn luminance_weights_are_the_spaces_own() {
        let srgb = ColourSpace::Srgb.luminance_weights();
        assert!((srgb[0] - 0.2126).abs() < 0.002, "{srgb:?}");
        assert!((srgb[1] - 0.7152).abs() < 0.002, "{srgb:?}");
        assert!((srgb[2] - 0.0722).abs() < 0.002, "{srgb:?}");

        for space in ColourSpace::ALL {
            let weights = space.luminance_weights();
            let sum: f32 = weights.iter().sum();
            assert!((sum - 1.0).abs() < 1e-3, "{} weights sum to {sum}", space.name());
            assert!(weights[1] > weights[0] && weights[1] > weights[2], "green carries the most");
        }
    }
}
