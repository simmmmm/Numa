use serde::{Deserialize, Serialize};

const MAX_HUE_SHIFT: f32 = 30.0;

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PointColour {

    pub target: [f32; 3],

    pub hue: f32,
    pub saturation: f32,
    pub luminance: f32,

    pub range: f32,
}

impl Default for PointColour {
    fn default() -> Self {
        Self { target: [0.6, 0.12, 30.0], hue: 0.0, saturation: 0.0, luminance: 0.0, range: 50.0 }
    }
}

impl PointColour {
    pub fn picked(linear_srgb: [f32; 3]) -> Self {
        Self { target: oklch(linear_srgb), ..Default::default() }
    }

    fn is_idle(&self) -> bool {
        self.hue == 0.0 && self.saturation == 0.0 && self.luminance == 0.0
    }

    pub fn swatch(&self) -> [f32; 3] {
        from_oklch(self.target)
    }

    pub fn weight(&self, lch: [f32; 3]) -> f32 {
        let wide = self.range.clamp(0.0, 100.0) / 100.0;
        let [l, c, h] = self.target;

        let apart = (lch[2] - h).rem_euclid(360.0);
        let apart = apart.min(360.0 - apart) * (lch[1].min(c) / 0.04).min(1.0);

        let d_hue = apart / (15.0 + 45.0 * wide);
        let d_chroma = (lch[1] - c) / (0.05 + 0.15 * wide);

        let d_light = (lch[0] - l) / (0.2 + 0.4 * wide);
        let distance = (d_hue * d_hue + d_chroma * d_chroma + d_light * d_light).sqrt();

        let t = (1.5 - distance).clamp(0.0, 1.0);
        t * t * (3.0 - 2.0 * t)
    }
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct PointColours {
    pub points: Vec<PointColour>,

    #[serde(skip)]
    pub highlight: Option<usize>,
}

impl PointColours {

    pub fn is_identity(&self) -> bool {
        self.highlight.is_none() && self.points.iter().all(PointColour::is_idle)
    }

    pub fn apply(&self, rgb: [f32; 3]) -> [f32; 3] {
        let original = oklch(rgb);
        if original[0] <= 1e-4 {
            return rgb;
        }

        let mut lch = original;
        for point in self.points.iter().filter(|point| !point.is_idle()) {
            let weight = point.weight(original);
            if weight <= 0.0 {
                continue;
            }
            lch[0] *= 1.0 + weight * point.luminance / 100.0 * 0.5;
            lch[1] = (lch[1] * (1.0 + weight * point.saturation / 100.0)).max(0.0);
            lch[2] += weight * point.hue / 100.0 * MAX_HUE_SHIFT;
        }

        let mut out = if lch == original { rgb } else { from_oklch(lch) };

        if let Some(point) = self.highlight.and_then(|index| self.points.get(index)) {
            let weight = point.weight(original);
            let grey = 0.2126 * out[0] + 0.7152 * out[1] + 0.0722 * out[2];
            out = out.map(|value| grey + (value - grey) * weight);
        }

        out
    }
}

pub fn oklch(rgb: [f32; 3]) -> [f32; 3] {
    let [r, g, b] = rgb;
    let l = (0.412_221_47 * r + 0.536_332_55 * g + 0.051_445_99 * b).cbrt();
    let m = (0.211_903_5 * r + 0.680_699_5 * g + 0.107_396_96 * b).cbrt();
    let s = (0.088_302_46 * r + 0.281_718_85 * g + 0.629_978_7 * b).cbrt();
    let lightness = 0.210_454_26 * l + 0.793_617_8 * m - 0.004_072_047 * s;
    let a = 1.977_998_5 * l - 2.428_592_2 * m + 0.450_593_7 * s;
    let bb = 0.025_904_037 * l + 0.782_771_77 * m - 0.808_675_77 * s;
    [lightness, (a * a + bb * bb).sqrt(), bb.atan2(a).to_degrees().rem_euclid(360.0)]
}

pub fn from_oklch(lch: [f32; 3]) -> [f32; 3] {
    let [lightness, chroma, hue] = lch;
    let (a, b) = (chroma * hue.to_radians().cos(), chroma * hue.to_radians().sin());
    let l = lightness + 0.396_337_78 * a + 0.215_803_76 * b;
    let m = lightness - 0.105_561_346 * a - 0.063_854_17 * b;
    let s = lightness - 0.089_484_18 * a - 1.291_485_5 * b;
    let (l, m, s) = (l * l * l, m * m * m, s * s * s);
    [
        4.076_741_7 * l - 3.307_711_6 * m + 0.230_969_94 * s,
        -1.268_438 * l + 2.609_757_4 * m - 0.341_319_38 * s,
        -0.004_196_086_3 * l - 0.703_418_6 * m + 1.707_614_7 * s,
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    const RED: [f32; 3] = [0.6, 0.05, 0.04];
    const BLUE: [f32; 3] = [0.04, 0.08, 0.6];

    #[test]
    fn oklch_round_trips() {
        for rgb in [RED, BLUE, [0.18, 0.18, 0.18], [0.9, 0.7, 0.1]] {
            let back = from_oklch(oklch(rgb));
            for (a, b) in rgb.iter().zip(back) {
                assert!((a - b).abs() < 1e-4, "{rgb:?} -> {back:?}");
            }
        }
    }

    #[test]
    fn a_picked_red_turned_moves_red_and_leaves_blue() {
        let point = PointColour { hue: 100.0, ..PointColour::picked(RED) };
        let points = PointColours { points: vec![point], highlight: None };

        let red = points.apply(RED).map(|value| value.max(0.0));
        let turned = (oklch(red)[2] - oklch(RED)[2]).abs();

        assert!(turned > 15.0, "red turned only {turned} degrees");

        assert_eq!(points.apply(BLUE), BLUE);
    }

    #[test]
    fn the_range_falls_off_smoothly() {
        let point = PointColour::picked(RED);
        let [l, c, h] = point.target;
        let mut last = point.weight(point.target);
        assert_eq!(last, 1.0);
        for step in 1..=90 {
            let weight = point.weight([l, c, h + step as f32]);
            assert!(weight <= last, "weight rose at {step} degrees");
            assert!(last - weight < 0.1, "a step at {step} degrees: {last} -> {weight}");
            last = weight;
        }
        assert_eq!(last, 0.0, "ninety degrees away is outside the default range");

        let wide = PointColour { range: 100.0, ..point };
        assert!(wide.weight([l, c, h + 40.0]) > point.weight([l, c, h + 40.0]));
    }

    #[test]
    fn showing_the_range_greys_what_is_outside_it() {
        let points = PointColours { points: vec![PointColour::picked(RED)], highlight: Some(0) };
        assert!(!points.is_identity());
        let blue = points.apply(BLUE);
        assert!((blue[0] - blue[2]).abs() < 1e-4, "blue kept its colour: {blue:?}");
        let red = points.apply(RED);
        assert!((red[0] - RED[0]).abs() < 1e-3, "red lost its colour: {red:?}");
    }
}
