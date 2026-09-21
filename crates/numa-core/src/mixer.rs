use serde::{Deserialize, Serialize};

use crate::profile::{prophoto_hue_of_srgb, HsvTable, ValueEncoding};

pub const BANDS: [(&str, f32); 8] = [
    ("Red", 0.0),
    ("Orange", 30.0),
    ("Yellow", 60.0),
    ("Green", 120.0),
    ("Aqua", 180.0),
    ("Blue", 240.0),
    ("Purple", 270.0),
    ("Magenta", 300.0),
];

const MAX_HUE_SHIFT: f32 = 30.0;

const DIVISIONS: usize = 120;

const MAX_GREY_STOPS: f32 = 2.0;

#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Mixer {

    pub bands: [[f32; 3]; 8],

    pub monochrome: bool,

    pub grey: [f32; 8],
}

impl Mixer {
    pub fn is_identity(&self) -> bool {
        !self.monochrome && self.colour_is_identity() && self.grey.iter().all(|value| *value == 0.0)
    }

    pub fn colour_is_identity(&self) -> bool {
        self.monochrome || self.bands.iter().flatten().all(|value| *value == 0.0)
    }

    pub fn grey_gain(&self, rgb: [f32; 3]) -> f32 {
        let [r, g, b] = rgb.map(|value| value.max(0.0).powf(1.0 / 2.2));
        let (high, low) = (r.max(g).max(b), r.min(g).min(b));
        if high <= 0.0 || high == low {
            return 1.0;
        }
        let chroma = high - low;
        let hue = 60.0
            * if high == r {
                (g - b) / chroma
            } else if high == g {
                (b - r) / chroma + 2.0
            } else {
                (r - g) / chroma + 4.0
            };
        let (below, above, blend) = surrounding(&BANDS.map(|(_, hue)| hue), hue);
        let value = self.grey[below] + (self.grey[above] - self.grey[below]) * blend;
        (value / 100.0 * MAX_GREY_STOPS * chroma / high).exp2()
    }

    fn centres() -> [f32; 8] {
        let mut centres = [0.0f32; 8];
        for (index, (_, srgb)) in BANDS.iter().enumerate() {
            centres[index] = prophoto_hue_of_srgb(*srgb);
        }
        centres
    }

    pub fn table(&self) -> HsvTable {
        let centres = Self::centres();
        let mut entries = Vec::with_capacity(DIVISIONS);

        for division in 0..DIVISIONS {
            let hue = division as f32 * 360.0 / DIVISIONS as f32;
            let (low, high, blend) = surrounding(&centres, hue);

            let mix = |channel: usize| {
                let a = self.bands[low][channel];
                let b = self.bands[high][channel];
                a + (b - a) * blend
            };

            let (hue, saturation) = (mix(0) / 100.0 * MAX_HUE_SHIFT, (1.0 + mix(1) / 100.0).max(0.0));

            entries.push([hue, saturation, 1.0]);
            entries.push([hue, saturation, (1.0 + mix(2) / 100.0).max(0.0)]);
        }

        HsvTable {

            value_encoding: ValueEncoding::Srgb,
            hue_divisions: DIVISIONS,
            sat_divisions: 2,
            val_divisions: 1,
            entries,
            second: None,
        }
    }
}

fn surrounding(centres: &[f32; 8], hue: f32) -> (usize, usize, f32) {
    let hue = hue.rem_euclid(360.0);

    for index in 0..centres.len() {
        let low = centres[index];
        let next = (index + 1) % centres.len();

        let span = (centres[next] - low).rem_euclid(360.0);
        let along = (hue - low).rem_euclid(360.0);

        if along < span {
            return (index, next, if span > 0.0 { along / span } else { 0.0 });
        }
    }

    (0, 0, 0.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::{Look, Table};

    fn with(band: usize, values: [f32; 3]) -> Mixer {
        let mut mixer = Mixer::default();
        mixer.bands[band] = values;
        mixer
    }

    fn look_of(mixer: &Mixer) -> Look {
        Look::new(Table::resolve(&mixer.table(), 0.0))
    }

    fn colour(degrees: f32) -> [f32; 3] {
        let sixths = degrees.rem_euclid(360.0) / 60.0;
        let sector = sixths.floor();
        let fraction = sixths - sector;
        let (a, b) = (0.0, 1.0);
        let up = a + (b - a) * fraction;
        let down = b - (b - a) * fraction;
        let encoded = match sector as i32 {
            0 => [1.0, up, 0.0],
            1 => [down, 1.0, 0.0],
            2 => [0.0, 1.0, up],
            3 => [0.0, down, 1.0],
            4 => [up, 0.0, 1.0],
            _ => [1.0, 0.0, down],
        };
        encoded.map(|v: f32| {
            if v <= 0.04045 { v / 12.92 } else { ((v + 0.055) / 1.055).powf(2.4) }
        })
    }

    fn saturation(rgb: [f32; 3]) -> f32 {
        let high = rgb[0].max(rgb[1]).max(rgb[2]);
        let low = rgb[0].min(rgb[1]).min(rgb[2]);
        if high <= 0.0 { 0.0 } else { (high - low) / high }
    }

    #[test]
    fn the_grey_mix_moves_its_own_colour() {
        let mut mixer = Mixer { monochrome: true, ..Mixer::default() };
        assert!(!mixer.is_identity() && mixer.colour_is_identity());
        mixer.grey[5] = -100.0;
        let blue = mixer.grey_gain(colour(240.0));
        assert!((blue - 0.25).abs() < 0.01, "full Blue −100 is two stops down: {blue}");
        assert_eq!(mixer.grey_gain([0.2, 0.2, 0.2]), 1.0, "a grey has no band");
        for (name, hue) in [("red", 0.0), ("yellow", 60.0), ("green", 120.0)] {
            assert_eq!(mixer.grey_gain(colour(hue)), 1.0, "{name} moved with blue");
        }
        mixer.grey[1] = 100.0;
        let orange = mixer.grey_gain(colour(30.0));
        assert!((orange - 4.0).abs() < 0.05, "orange is the Orange band: {orange}");
    }

    #[test]
    fn an_untouched_mixer_changes_nothing() {
        let mixer = Mixer::default();
        assert!(mixer.is_identity());

        let table = mixer.table();
        assert_eq!(table.entries.len(), DIVISIONS * 2, "a grey and a full row per hue");
        for entry in &table.entries {
            assert_eq!(*entry, [0.0, 1.0, 1.0]);
        }
    }

    #[test]
    fn a_band_moves_its_own_colour_and_leaves_the_others() {

        let look = look_of(&with(3, [0.0, -100.0, 0.0]));

        let green = look.apply(colour(120.0));
        assert!(saturation(green) < 0.15, "green should be nearly grey: {green:?}");

        for (name, hue) in [("red", 0.0), ("blue", 240.0), ("magenta", 300.0)] {
            let other = look.apply(colour(hue));
            assert!(
                saturation(other) > 0.7,
                "{name} lost saturation to the green band: {other:?}"
            );
        }
    }

    #[test]
    fn the_wedge_between_magenta_and_red_belongs_to_them() {

        let look = look_of(&with(0, [0.0, -100.0, 0.0]));
        let nearly_red = look.apply(colour(350.0));
        assert!(
            saturation(nearly_red) < 0.6,
            "the wrap-around wedge was left untouched: {nearly_red:?}"
        );
    }

    #[test]
    fn a_hue_shift_turns_the_colour_it_names() {
        let look = look_of(&with(5, [100.0, 0.0, 0.0]));
        let before = colour(240.0);
        let after = look.apply(before);
        assert_ne!(before, after, "blue did not move");

        let yellow = colour(60.0);
        let after = look.apply(yellow);
        for (before, now) in yellow.iter().zip(after.iter()) {
            assert!((before - now).abs() < 1e-4, "yellow followed blue: {yellow:?} -> {after:?}");
        }
    }

    #[test]
    fn bands_land_on_the_prophoto_hue_their_srgb_name_maps_to() {
        let centres = Mixer::centres();
        for (index, (name, srgb)) in BANDS.iter().enumerate() {
            let expected = prophoto_hue_of_srgb(*srgb);
            assert!((centres[index] - expected).abs() < 1e-3, "{name}");
        }

        for pair in centres.windows(2) {
            assert!(pair[0] < pair[1], "band centres are out of order: {centres:?}");
        }
    }

    #[test]
    fn surrounding_walks_the_whole_circle() {
        let centres = Mixer::centres();
        for step in 0..360 {
            let (low, high, blend) = surrounding(&centres, step as f32);
            assert!(low < 8 && high < 8);
            assert!((0.0..1.0).contains(&blend), "at {step}: {blend}");
        }

        let (low, _, blend) = surrounding(&centres, centres[4]);
        assert_eq!(low, 4);
        assert!(blend.abs() < 1e-5);
    }
}
