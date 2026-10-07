use numa_core::color::WhiteBalance;
use numa_core::curve::Curve;
use numa_core::beautify::Beautify;
use numa_core::document::{AutoRecord, Basic, Document, Perspective};
use numa_core::grading::Grading;
use numa_core::mask::Mask;
use numa_core::mixer::Mixer;
use numa_core::point::PointColours;
use numa_core::retouch::Retouch;
use numa_core::space::ColourSpace;

use crate::masks::{mask_label_parts, MaskLabel};

#[derive(Debug, Clone, PartialEq)]
pub struct EditState {
    pub basic: Basic,
    pub white_balance: Option<WhiteBalance>,

    pub crop: Option<([f32; 4], f32)>,
    pub rotation: f32,

    pub mirrored: bool,
    pub film_simulation: Option<String>,

    pub colour_profile: Option<String>,

    pub curves: [Curve; 4],
    pub mixer: Mixer,

    pub point_colours: PointColours,

    pub grading: Grading,

    pub retouch: Retouch,

    pub perspective: Perspective,

    pub working_space: ColourSpace,

    pub beautify: Beautify,
    pub masks: Vec<Mask>,

    pub ai_denoise: f32,

    pub ai_sharpen: f32,

    pub lut: Option<numa_core::lut::LutChoice>,

    pub camera_look: Option<numa_core::camera_look::CameraLook>,

    pub auto: Option<AutoRecord>,
}

impl EditState {

    pub fn restore(&self, document: &mut Document) {
        let (rect, angle) = self.crop.unwrap_or(([0.0, 0.0, 1.0, 1.0], 0.0));
        document.working_space = self.working_space;
        document.set_perspective(self.perspective);
        document.set_crop(rect, angle);
        document.set_rotation(self.rotation);
        document.set_mirrored(self.mirrored);
        document.film_simulation = self.film_simulation.clone();
        document.colour_profile = self.colour_profile.clone();
        document.set_basic(self.basic);
        document.white_balance = self.white_balance;
        document.set_curves(self.curves.clone());
        document.set_mixer(self.mixer);
        document.set_point_colours(self.point_colours.clone());
        document.set_grading(self.grading);
        document.set_retouch(self.retouch.clone());
        document.set_beautify(self.beautify);
        document.set_masks(self.masks.clone());
        document.ai_denoise = self.ai_denoise;
        document.ai_sharpen = self.ai_sharpen;
        document.lut = self.lut.clone();
        document.camera_look = self.camera_look.clone();
        document.auto = self.auto;
    }

    pub fn untouched() -> Self {
        Self::of(&Document::new(String::new()))
    }

    pub fn of(document: &Document) -> Self {
        Self {
            basic: document.basic(),
            white_balance: document.white_balance,
            crop: document.crop(),
            rotation: document.rotation(),
            mirrored: document.mirrored(),
            film_simulation: document.film_simulation.clone(),
            colour_profile: document.colour_profile.clone(),
            curves: document.curves(),
            mixer: document.mixer(),
            point_colours: document.point_colours(),
            grading: document.grading(),
            retouch: document.retouch(),
            perspective: document.perspective(),
            working_space: document.working_space,
            beautify: document.beautify(),
            masks: document.masks(),
            ai_denoise: document.ai_denoise,
            ai_sharpen: document.ai_sharpen,
            lut: document.lut.clone(),
            camera_look: document.camera_look.clone(),
            auto: document.auto,
        }
    }
}

pub struct History {
    pub states: Vec<EditState>,

    pub position: usize,

    pub names: Vec<Option<String>>,
}

pub const HISTORY_KEPT: usize = 100;

impl History {
    pub fn new(initial: EditState) -> Self {
        Self { states: vec![initial], position: 0, names: vec![None] }
    }

    pub fn resumed(saved: Option<(Vec<EditState>, usize)>, now: EditState) -> Self {
        let mut history = match saved {
            Some((states, position)) if !states.is_empty() => {
                let position = position.min(states.len() - 1);
                Self { names: vec![None; states.len()], states, position }
            }
            _ if now != EditState::untouched() => Self::new(EditState::untouched()),
            _ => Self::new(now.clone()),
        };
        history.push(now);
        history
    }

    pub fn push(&mut self, state: EditState) {
        if self.states[self.position] == state {
            return;
        }

        self.states.truncate(self.position + 1);
        self.names.truncate(self.position + 1);
        self.states.push(state);
        self.names.push(None);
        if self.states.len() > HISTORY_KEPT {
            self.states.remove(0);
            self.names.remove(0);
        }
        self.position = self.states.len() - 1;
    }

    pub fn push_named(&mut self, state: EditState, name: &str) {
        if self.states[self.position] != state {
            self.push(state);
            self.names[self.position] = Some(name.to_string());
        }
    }

    pub fn undo(&mut self) -> Option<EditState> {
        if self.position == 0 {
            return None;
        }
        self.position -= 1;
        Some(self.states[self.position].clone())
    }

    pub fn redo(&mut self) -> Option<EditState> {
        if self.position + 1 >= self.states.len() {
            return None;
        }
        self.position += 1;
        Some(self.states[self.position].clone())
    }

    pub fn go_to(&mut self, position: usize) -> Option<EditState> {
        if position >= self.states.len() || position == self.position {
            return None;
        }
        self.position = position;
        Some(self.states[self.position].clone())
    }

    pub fn steps(&self) -> Vec<String> {
        self.step_parts().iter().map(Step::to_string).collect()
    }

    pub fn step_parts(&self) -> Vec<Step> {

        let first = if self.states[0] == EditState::untouched() { Step::Original } else { Step::Opened };
        let mut steps = vec![first];
        for (pair, name) in self.states.windows(2).zip(&self.names[1..]) {
            steps.push(match name {
                Some(name) => Step::Named(name.clone()),
                None => Step::Changed(pair[1].changes_from(&pair[0])),
            });
        }
        steps
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Step {
    Original,
    Opened,

    Named(String),

    Changed(Vec<Change>),
}

#[derive(Clone, Debug, PartialEq)]
pub enum Change {

    Word(&'static str),

    Mask(MaskDone, MaskLabel),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MaskDone {
    Added,
    Adjusted,
    Inverted,
    Showed,
    Hid,
    Renamed,
    SetStrength,
    DrewOn,
    PointedAt,
    Changed,
}

impl MaskDone {

    pub fn english(self) -> &'static str {
        match self {
            MaskDone::Added => "Added",
            MaskDone::Adjusted => "Adjusted",
            MaskDone::Inverted => "Inverted",
            MaskDone::Showed => "Showed",
            MaskDone::Hid => "Hid",
            MaskDone::Renamed => "Renamed",
            MaskDone::SetStrength => "Set the strength of",
            MaskDone::DrewOn => "Drew on",
            MaskDone::PointedAt => "Pointed at",
            MaskDone::Changed => "Changed",
        }
    }
}

impl std::fmt::Display for Change {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Change::Word(word) => f.write_str(word),
            Change::Mask(done, label) => write!(f, "{} {label}", done.english()),
        }
    }
}

impl std::fmt::Display for Step {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Step::Original => f.write_str("Original"),
            Step::Opened => f.write_str("Opened"),
            Step::Named(name) => f.write_str(name),
            Step::Changed(changed) => match changed.as_slice() {
                [] => f.write_str("No change"),
                [one] => one.fmt(f),
                [one, two] => write!(f, "{one} and {two}"),
                [one, rest @ ..] => write!(f, "{one} and {} more", rest.len()),
            },
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Fmt {

    Signed(usize),

    Positive,

    Radius,

    Middle,
}

pub struct Slider {
    pub name: &'static str,
    pub tab: &'static str,
    pub fmt: Fmt,
    pub at: fn(&mut Basic) -> &mut f32,
}

impl Slider {
    pub fn value(&self, basic: &Basic) -> f32 {
        let mut copy = *basic;
        *(self.at)(&mut copy)
    }
}

pub const SLIDERS: [Slider; 38] = [
    Slider { name: "Exposure", tab: "Light", fmt: Fmt::Signed(2), at: |b| &mut b.tone.exposure },
    Slider { name: "Contrast", tab: "Light", fmt: Fmt::Signed(0), at: |b| &mut b.tone.contrast },
    Slider { name: "Highlights", tab: "Light", fmt: Fmt::Signed(0), at: |b| &mut b.tone.highlights },
    Slider { name: "Shadows", tab: "Light", fmt: Fmt::Signed(0), at: |b| &mut b.tone.shadows },
    Slider { name: "Whites", tab: "Light", fmt: Fmt::Signed(0), at: |b| &mut b.tone.whites },
    Slider { name: "Blacks", tab: "Light", fmt: Fmt::Signed(0), at: |b| &mut b.tone.blacks },
    Slider { name: "Vibrance", tab: "Colour", fmt: Fmt::Signed(0), at: |b| &mut b.presence.vibrance },
    Slider { name: "Saturation", tab: "Colour", fmt: Fmt::Signed(0), at: |b| &mut b.presence.saturation },
    Slider { name: "HDR", tab: "Light", fmt: Fmt::Signed(0), at: |b| &mut b.presence.hdr },
    Slider { name: "Clarity", tab: "Effects", fmt: Fmt::Signed(0), at: |b| &mut b.presence.clarity },
    Slider { name: "Texture", tab: "Effects", fmt: Fmt::Signed(0), at: |b| &mut b.presence.texture },
    Slider { name: "Sharpening", tab: "Detail", fmt: Fmt::Positive, at: |b| &mut b.detail.sharpen },
    Slider { name: "Sharpening radius", tab: "Detail", fmt: Fmt::Radius, at: |b| &mut b.detail.sharpen_radius },
    Slider { name: "Sharpening mask", tab: "Detail", fmt: Fmt::Positive, at: |b| &mut b.detail.sharpen_masking },
    Slider { name: "Noise reduction", tab: "Detail", fmt: Fmt::Positive, at: |b| &mut b.detail.denoise_luma },
    Slider { name: "Noise detail", tab: "Detail", fmt: Fmt::Middle, at: |b| &mut b.detail.denoise_detail },
    Slider { name: "Noise contrast", tab: "Detail", fmt: Fmt::Positive, at: |b| &mut b.detail.denoise_contrast },
    Slider { name: "Colour noise", tab: "Detail", fmt: Fmt::Positive, at: |b| &mut b.detail.denoise_colour },
    Slider { name: "Defringe", tab: "Detail", fmt: Fmt::Positive, at: |b| &mut b.detail.defringe },
    Slider { name: "Moiré", tab: "Detail", fmt: Fmt::Positive, at: |b| &mut b.detail.moire },
    Slider { name: "Dehaze", tab: "Effects", fmt: Fmt::Signed(0), at: |b| &mut b.effects.dehaze },
    Slider { name: "Vignette", tab: "Effects", fmt: Fmt::Signed(0), at: |b| &mut b.effects.vignette },
    Slider { name: "Vignette midpoint", tab: "Effects", fmt: Fmt::Middle, at: |b| &mut b.effects.vignette_midpoint },
    Slider { name: "Vignette roundness", tab: "Effects", fmt: Fmt::Signed(0), at: |b| &mut b.effects.vignette_roundness },
    Slider { name: "Vignette feather", tab: "Effects", fmt: Fmt::Middle, at: |b| &mut b.effects.vignette_feather },
    Slider { name: "Grain", tab: "Effects", fmt: Fmt::Positive, at: |b| &mut b.effects.grain },
    Slider { name: "Grain size", tab: "Effects", fmt: Fmt::Positive, at: |b| &mut b.effects.grain_size },
    Slider { name: "Grain roughness", tab: "Effects", fmt: Fmt::Middle, at: |b| &mut b.effects.grain_roughness },
    Slider { name: "Mist", tab: "Effects", fmt: Fmt::Signed(0), at: |b| &mut b.effects.mist },
    Slider { name: "Shadows tint", tab: "Colour", fmt: Fmt::Signed(0), at: |b| &mut b.calibration.shadow_tint },
    Slider { name: "Red hue", tab: "Colour", fmt: Fmt::Signed(0), at: |b| &mut b.calibration.red_hue },
    Slider { name: "Red saturation", tab: "Colour", fmt: Fmt::Signed(0), at: |b| &mut b.calibration.red_saturation },
    Slider { name: "Green hue", tab: "Colour", fmt: Fmt::Signed(0), at: |b| &mut b.calibration.green_hue },
    Slider { name: "Green saturation", tab: "Colour", fmt: Fmt::Signed(0), at: |b| &mut b.calibration.green_saturation },
    Slider { name: "Blue hue", tab: "Colour", fmt: Fmt::Signed(0), at: |b| &mut b.calibration.blue_hue },
    Slider { name: "Blue saturation", tab: "Colour", fmt: Fmt::Signed(0), at: |b| &mut b.calibration.blue_saturation },
    Slider { name: "Lens distortion", tab: "Detail", fmt: Fmt::Signed(0), at: |b| &mut b.optics.lens_distortion },
    Slider { name: "Lens vignetting", tab: "Detail", fmt: Fmt::Signed(0), at: |b| &mut b.optics.lens_vignetting },
];

impl EditState {

    pub fn difference_from(&self, previous: &EditState) -> String {
        Step::Changed(self.changes_from(previous)).to_string()
    }

    pub fn changes_from(&self, previous: &EditState) -> Vec<Change> {
        let mut changed: Vec<Change> = SLIDERS
            .iter()
            .filter(|slider| slider.value(&self.basic) != slider.value(&previous.basic))
            .map(|slider| Change::Word(slider.name))
            .collect();

        if self.white_balance != previous.white_balance {
            changed.push(Change::Word("White balance"));
        }
        if self.crop != previous.crop {
            changed.push(Change::Word("Crop"));
        }
        if self.rotation != previous.rotation {
            changed.push(Change::Word("Rotation"));
        }
        if self.mirrored != previous.mirrored {
            changed.push(Change::Word("Flip"));
        }
        if self.curves != previous.curves {
            changed.push(Change::Word("Tone curve"));
        }
        if self.mixer != previous.mixer {
            changed.push(Change::Word("Colour mixer"));
        }
        if self.point_colours != previous.point_colours {
            changed.push(Change::Word("Point colour"));
        }
        if self.grading != previous.grading {
            changed.push(Change::Word("Colour grading"));
        }
        if self.retouch != previous.retouch {
            let (now, was) = (self.retouch.spots.len(), previous.retouch.spots.len());
            changed.push(Change::Word(match now.cmp(&was) {
                std::cmp::Ordering::Greater => "Retouched",
                std::cmp::Ordering::Less => "Removed a retouch",
                std::cmp::Ordering::Equal => "Moved a retouch",
            }));
        }
        if self.film_simulation != previous.film_simulation {
            changed.push(Change::Word("Film simulation"));
        }
        if self.colour_profile != previous.colour_profile {
            changed.push(Change::Word("Camera profile"));
        }
        if self.working_space != previous.working_space {
            changed.push(Change::Word("Colour space"));
        }
        if self.perspective != previous.perspective {
            changed.push(Change::Word("Perspective"));
        }
        if self.beautify != previous.beautify {
            changed.push(Change::Word("Face"));
        }
        if self.ai_denoise != previous.ai_denoise {
            changed.push(Change::Word("AI denoise"));
        }
        if self.ai_sharpen != previous.ai_sharpen {
            changed.push(Change::Word("AI sharpen"));
        }
        if self.lut != previous.lut {
            changed.push(Change::Word("LUT"));
        }

        if self.camera_look.as_ref().map(|look| look.strength) != previous.camera_look.as_ref().map(|look| look.strength) {
            changed.push(Change::Word("As Shot"));
        }
        if let Some(mask) = mask_change(&self.masks, &previous.masks) {
            changed.push(mask);
        }

        if changed.is_empty() && self.auto != previous.auto {
            changed.push(Change::Word("Auto"));
        }
        changed
    }
}

pub fn masks_changed(now: &[Mask], before: &[Mask]) -> Option<String> {
    mask_change(now, before).map(|change| change.to_string())
}

pub fn mask_change(now: &[Mask], before: &[Mask]) -> Option<Change> {
    if now.len() > before.len() {
        return Some(Change::Mask(MaskDone::Added, mask_label_parts(now, now.len() - 1)));
    }
    if now.len() < before.len() {
        return Some(Change::Word("Removed a mask"));
    }

    let (index, mask) = now
        .iter()
        .zip(before)
        .position(|(now, before)| now != before)
        .map(|index| (index, &now[index]))?;

    let was = &before[index];
    let done = if mask.basic != was.basic {
        MaskDone::Adjusted
    } else if mask.inverted != was.inverted {
        MaskDone::Inverted

    } else if mask.visible != was.visible {
        if mask.visible { MaskDone::Showed } else { MaskDone::Hid }
    } else if mask.name != was.name {
        MaskDone::Renamed
    } else if mask.opacity != was.opacity {
        MaskDone::SetStrength
    } else if mask.strokes.len() != was.strokes.len() {
        MaskDone::DrewOn
    } else if mask.points.len() != was.points.len() {
        MaskDone::PointedAt
    } else {
        MaskDone::Changed
    };
    Some(Change::Mask(done, mask_label_parts(now, index)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::masks::MaskName;
    use numa_core::mask::Shape;

    fn state(change: impl FnOnce(&mut Document)) -> EditState {
        let mut document = Document::new("x".into());
        change(&mut document);
        EditState::of(&document)
    }

    fn radial(change: impl FnOnce(&mut Mask)) -> Vec<Mask> {
        let mut mask = Mask::new(Shape::radial());
        change(&mut mask);
        vec![mask]
    }

    #[test]
    fn a_step_that_only_moves_autos_record_is_called_auto() {
        let before = state(|_| {});
        let after = state(|document| {
            document.auto = Some(numa_core::document::AutoRecord { vibrance: Some(10.0), ..Default::default() })
        });
        assert_eq!(after.changes_from(&before), vec![Change::Word("Auto")]);
    }

    #[test]
    fn a_step_in_parts_reads_as_the_step() {
        let before = EditState::untouched();
        let one = state(|d| d.set_basic(Basic::with(|b| b.tone.exposure = 1.0)));
        assert_eq!(one.changes_from(&before), [Change::Word("Exposure")]);
        assert_eq!(one.difference_from(&before), "Exposure");
        let two = state(|d| d.set_basic(Basic::with(|b| (b.tone.exposure, b.tone.contrast) = (1.0, 10.0))));
        assert_eq!(two.difference_from(&before), "Exposure and Contrast");
        let three = state(|d| {
            d.set_basic(Basic::with(|b| (b.tone.exposure, b.tone.contrast) = (1.0, 10.0)));
            d.set_crop([0.1, 0.1, 0.8, 0.8], 0.0);
        });
        assert_eq!(three.difference_from(&before), "Exposure and 2 more");
        assert_eq!(before.difference_from(&before), "No change");

        let added = state(|d| d.set_masks(radial(|_| {})));
        let radial_one = MaskLabel { name: MaskName::Kind("Radial"), number: None };
        assert_eq!(added.changes_from(&before), [Change::Mask(MaskDone::Added, radial_one.clone())]);
        assert_eq!(added.difference_from(&before), "Added Radial");
        let hidden = state(|d| d.set_masks(radial(|mask| mask.visible = false)));
        assert_eq!(hidden.difference_from(&added), "Hid Radial");
        let weaker = state(|d| d.set_masks(radial(|mask| mask.opacity = 0.5)));
        assert_eq!(weaker.changes_from(&added), [Change::Mask(MaskDone::SetStrength, radial_one)]);
        assert_eq!(weaker.difference_from(&added), "Set the strength of Radial");
        assert_eq!(before.difference_from(&added), "Removed a mask");
    }

    #[test]
    fn a_history_in_parts() {
        let mut history = History::new(EditState::untouched());
        history.push(state(|d| d.set_crop([0.1, 0.1, 0.8, 0.8], 0.0)));
        history.push_named(state(|d| d.set_basic(Basic::with(|b| b.tone.exposure = 0.7))), "Before the sky");
        assert_eq!(
            history.step_parts(),
            [Step::Original, Step::Changed(vec![Change::Word("Crop")]), Step::Named("Before the sky".into())]
        );
        assert_eq!(history.steps(), ["Original", "Crop", "Before the sky"]);
    }
}
