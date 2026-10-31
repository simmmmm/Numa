use crate::color::WhiteBalance;
use crate::document::{Basic, Document};
use crate::grading::Grading;
use crate::mask::Mask;

pub const LIGHT_SOURCES: [(&str, f32, f32); 6] = [
    ("Daylight", 5500.0, 10.0),
    ("Cloudy", 6500.0, 10.0),
    ("Shade", 7500.0, 10.0),
    ("Tungsten", 2850.0, 0.0),
    ("Fluorescent", 3800.0, 21.0),
    ("Flash", 5500.0, 0.0),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LightSource {
    AsShot,
    Preset(&'static str),
    Custom,
}

pub fn light_source(kelvin: f32, tint: f32, as_shot: Option<(f32, f32)>, raw: bool) -> LightSource {
    let at = |(k, t): (f32, f32)| (kelvin - k).abs() < 0.5 && (tint - t).abs() < 0.5;
    if as_shot.is_some_and(at) {
        return LightSource::AsShot;
    }
    if !raw {
        return LightSource::Custom;
    }
    LIGHT_SOURCES
        .iter()
        .find(|(_, k, t)| at((*k, *t)))
        .map_or(LightSource::Custom, |(name, _, _)| LightSource::Preset(name))
}

fn off(balance: Option<WhiteBalance>, as_shot: Option<WhiteBalance>) -> bool {
    match (balance, as_shot) {
        (None, _) => false,
        (Some(_), None) => true,
        (Some(set), Some(shot)) => {
            (set.temperature - shot.temperature).abs() >= 0.5 || (set.tint - shot.tint).abs() >= 0.5
        }
    }
}

fn presence_colour(basic: &Basic, rest: &Basic) -> bool {
    basic.presence.vibrance != rest.presence.vibrance
        || basic.presence.saturation != rest.presence.saturation
}

fn presence_effects(basic: &Basic, rest: &Basic) -> bool {
    basic.presence.clarity != rest.presence.clarity || basic.presence.texture != rest.presence.texture
}

impl Document {

    pub fn tab_touched(&self, tab: &str, as_shot: Option<WhiteBalance>) -> bool {
        let (basic, rest) = (self.basic(), Basic::default());
        match tab {
            "presets" | "looks" => self.lut.is_some(),
            "light" => basic.tone != rest.tone || self.curves().iter().any(|curve| !curve.is_identity()),
            "colour" => {
                presence_colour(&basic, &rest)
                    || !self.mixer().is_identity()
                    || !self.point_colours().is_identity()
                    || off(self.white_balance, as_shot)
                    || self.colour_profile.is_some()
                    || !self.tone_mapping.is_camera()
                    || self.camera_look.is_some()
            }
            "effects" => presence_effects(&basic, &rest) || basic.effects != rest.effects,
            "grade" => self.grading() != Grading::default(),
            "detail" => {
                basic.detail != rest.detail
                    || basic.optics != rest.optics
                    || self.ai_denoise > 0.0
                    || self.ai_sharpen > 0.0
            }
            "masks" => !self.masks().is_empty(),
            "retouch" => !self.retouch().spots.is_empty() || !self.beautify().is_identity(),
            "crop" => {
                self.crop().is_some_and(|(rect, angle)| rect != [0.0, 0.0, 1.0, 1.0] || angle != 0.0)
                    || self.perspective() != Default::default()
                    || self.rotation() != 0.0
                    || self.mirrored()
            }
            _ => false,
        }
    }
}

impl Mask {

    pub fn tab_touched(&self, tab: &str) -> bool {
        let rest = Basic::local();
        let basic = &self.basic;
        match tab {
            "light" => {
                basic.tone != rest.tone
                    || !self.curve.is_identity()
                    || self.channel_curves.iter().any(|curve| !curve.is_identity())
            }
            "colour" => {
                presence_colour(basic, &rest)
                    || basic.balance != rest.balance
                    || !self.mixer.is_identity()
                    || !self.point_colours.is_identity()
                    || !self.colour.is_identity()
            }
            "effects" => presence_effects(basic, &rest) || basic.effects != rest.effects,
            "grade" => self.grading != Grading::default(),
            "detail" => basic.detail != rest.detail,
            _ => false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PassState {

    Idle,

    Missing,

    Busy,

    Running(Option<f32>),

    Stopping,

    On,

    Failed,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct PassFacts {
    pub on: bool,
    pub installed: bool,
    pub running: bool,

    pub fraction: Option<f32>,
    pub stopping: bool,
    pub other_running: bool,
    pub failed: bool,
}

impl PassState {
    pub fn of(facts: PassFacts) -> Self {
        match facts {
            PassFacts { running: true, stopping: true, .. } => Self::Stopping,
            PassFacts { running: true, fraction, .. } => Self::Running(fraction),
            PassFacts { on: true, .. } => Self::On,
            PassFacts { installed: false, .. } => Self::Missing,
            PassFacts { other_running: true, .. } => Self::Busy,
            PassFacts { failed: true, .. } => Self::Failed,
            _ => Self::Idle,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::document::{Basic, Document};
    use crate::lut::LutChoice;

    #[test]
    fn the_light_source_is_named_as_shot_first_and_a_jpeg_has_only_that() {
        let shot = Some((5500.0, 10.0));
        assert_eq!(light_source(5500.0, 10.0, shot, true), LightSource::AsShot);
        assert_eq!(light_source(2850.0, 0.0, shot, true), LightSource::Preset("Tungsten"));
        assert_eq!(light_source(5000.0, 3.0, shot, true), LightSource::Custom);
        assert_eq!(light_source(2850.0, 0.0, shot, false), LightSource::Custom);
    }

    const TABS: [&str; 9] =
        ["presets", "light", "colour", "effects", "grade", "detail", "masks", "retouch", "crop"];

    fn lit(document: &Document) -> Vec<&'static str> {
        TABS.into_iter().filter(|tab| document.tab_touched(tab, None)).collect()
    }

    #[test]
    fn an_untouched_photograph_lights_nothing() {
        assert!(lit(&Document::new("a.raf".into())).is_empty());
    }

    #[test]
    fn each_thing_lights_its_own_tab_and_no_other() {
        let mut lut = Document::new("a.raf".into());
        lut.lut = Some(LutChoice { name: "warm.cube".into(), amount: 100.0 });
        assert_eq!(lit(&lut), ["presets"]);

        let mut ai = Document::new("a.raf".into());
        ai.ai_sharpen = 50.0;
        assert_eq!(lit(&ai), ["detail"]);

        let mut look = Document::new("a.raf".into());
        look.camera_look = Some(crate::camera_look::CameraLook { strength: 100.0, fit: None });
        assert_eq!(lit(&look), ["colour"]);

        let mut tone = Document::new("a.raf".into());
        tone.set_basic(Basic::with(|basic| basic.tone.exposure = 1.0));
        assert_eq!(lit(&tone), ["light"]);

        let mut clarity = Document::new("a.raf".into());
        clarity.set_basic(Basic::with(|basic| basic.presence.clarity = 10.0));
        assert_eq!(lit(&clarity), ["effects"]);

        let mut mask = Document::new("a.raf".into());
        mask.set_masks(vec![Mask::new(crate::mask::Shape::linear())]);
        assert_eq!(lit(&mask), ["masks"]);
    }

    #[test]
    fn a_white_balance_is_off_what_the_camera_chose() {
        let shot = WhiteBalance { temperature: 5300.0, tint: 2.0 };
        let mut document = Document::new("a.raf".into());
        document.white_balance = Some(shot);
        assert!(!document.tab_touched("colour", Some(shot)));
        document.white_balance = Some(WhiteBalance { temperature: 6000.0, tint: 2.0 });
        assert!(document.tab_touched("colour", Some(shot)));
    }

    #[test]
    fn inside_a_mask_its_own_rest_counts() {
        let mut mask = Mask::new(crate::mask::Shape::linear());
        assert!(["light", "colour", "effects", "grade", "detail"].iter().all(|tab| !mask.tab_touched(tab)));
        mask.basic.detail.sharpen = 25.0;
        assert!(mask.tab_touched("detail"), "the photograph's rest is more than a mask's");
        assert!(!mask.tab_touched("light"));
    }

    #[test]
    fn a_pass_says_one_thing_at_a_time() {
        let of = |facts| PassState::of(facts);
        let ready = PassFacts { installed: true, ..Default::default() };
        assert_eq!(of(ready), PassState::Idle);
        assert_eq!(of(PassFacts::default()), PassState::Missing);
        assert_eq!(of(PassFacts { other_running: true, ..ready }), PassState::Busy);
        assert_eq!(of(PassFacts { failed: true, ..ready }), PassState::Failed);
        assert_eq!(of(PassFacts { on: true, ..ready }), PassState::On);
        assert_eq!(of(PassFacts { running: true, ..ready }), PassState::Running(None));
        assert_eq!(
            of(PassFacts { running: true, fraction: Some(0.4), on: true, ..ready }),
            PassState::Running(Some(0.4)),
            "a pass switched on and running is running"
        );
        assert_eq!(of(PassFacts { running: true, stopping: true, ..ready }), PassState::Stopping);
    }
}
