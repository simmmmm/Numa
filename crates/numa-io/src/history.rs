use numa_core::color::WhiteBalance;
use numa_core::curve::Curve;
use numa_core::beautify::Beautify;
use numa_core::document::{Basic, Document, Perspective};
use numa_core::grading::Grading;
use numa_core::mask::Mask;
use numa_core::mixer::Mixer;
use numa_core::point::PointColours;
use numa_core::retouch::Retouch;
use numa_core::space::ColourSpace;

use crate::masks::mask_label;

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

        let first = if self.states[0] == EditState::untouched() { "Original" } else { "Opened" };
        let mut steps = vec![first.to_string()];
        for (pair, name) in self.states.windows(2).zip(&self.names[1..]) {
            steps.push(name.clone().unwrap_or_else(|| pair[1].difference_from(&pair[0])));
        }
        steps
    }
}

impl EditState {

    pub fn difference_from(&self, previous: &EditState) -> String {
        let sliders: [(&str, fn(&Basic) -> f32); 37] = [
            ("Exposure", |b| b.tone.exposure),
            ("Contrast", |b| b.tone.contrast),
            ("Highlights", |b| b.tone.highlights),
            ("Shadows", |b| b.tone.shadows),
            ("Whites", |b| b.tone.whites),
            ("Blacks", |b| b.tone.blacks),
            ("Vibrance", |b| b.presence.vibrance),
            ("Saturation", |b| b.presence.saturation),
            ("HDR", |b| b.presence.hdr),
            ("Clarity", |b| b.presence.clarity),
            ("Texture", |b| b.presence.texture),
            ("Sharpening", |b| b.detail.sharpen),
            ("Sharpening radius", |b| b.detail.sharpen_radius),
            ("Sharpening mask", |b| b.detail.sharpen_masking),
            ("Noise reduction", |b| b.detail.denoise_luma),
            ("Noise detail", |b| b.detail.denoise_detail),
            ("Noise contrast", |b| b.detail.denoise_contrast),
            ("Colour noise", |b| b.detail.denoise_colour),
            ("Defringe", |b| b.detail.defringe),
            ("Moiré", |b| b.detail.moire),
            ("Dehaze", |b| b.effects.dehaze),
            ("Vignette", |b| b.effects.vignette),
            ("Vignette midpoint", |b| b.effects.vignette_midpoint),
            ("Vignette roundness", |b| b.effects.vignette_roundness),
            ("Vignette feather", |b| b.effects.vignette_feather),
            ("Grain", |b| b.effects.grain),
            ("Grain size", |b| b.effects.grain_size),
            ("Grain roughness", |b| b.effects.grain_roughness),
            ("Shadows tint", |b| b.calibration.shadow_tint),
            ("Red hue", |b| b.calibration.red_hue),
            ("Red saturation", |b| b.calibration.red_saturation),
            ("Green hue", |b| b.calibration.green_hue),
            ("Green saturation", |b| b.calibration.green_saturation),
            ("Blue hue", |b| b.calibration.blue_hue),
            ("Blue saturation", |b| b.calibration.blue_saturation),
            ("Lens distortion", |b| b.optics.lens_distortion),
            ("Lens vignetting", |b| b.optics.lens_vignetting),
        ];

        let mut changed: Vec<String> = sliders
            .iter()
            .filter(|(_, read)| read(&self.basic) != read(&previous.basic))
            .map(|(name, _)| name.to_string())
            .collect();

        if self.white_balance != previous.white_balance {
            changed.push("White balance".to_string());
        }
        if self.crop != previous.crop {
            changed.push("Crop".to_string());
        }
        if self.rotation != previous.rotation {
            changed.push("Rotation".to_string());
        }
        if self.mirrored != previous.mirrored {
            changed.push("Flip".to_string());
        }
        if self.curves != previous.curves {
            changed.push("Tone curve".to_string());
        }
        if self.mixer != previous.mixer {
            changed.push("Colour mixer".to_string());
        }
        if self.point_colours != previous.point_colours {
            changed.push("Point colour".to_string());
        }
        if self.grading != previous.grading {
            changed.push("Colour grading".to_string());
        }
        if self.retouch != previous.retouch {
            let (now, was) = (self.retouch.spots.len(), previous.retouch.spots.len());
            changed.push(match now.cmp(&was) {
                std::cmp::Ordering::Greater => "Retouched".to_string(),
                std::cmp::Ordering::Less => "Removed a retouch".to_string(),
                std::cmp::Ordering::Equal => "Moved a retouch".to_string(),
            });
        }
        if self.film_simulation != previous.film_simulation {
            changed.push("Film simulation".to_string());
        }
        if self.colour_profile != previous.colour_profile {
            changed.push("Camera profile".to_string());
        }
        if self.working_space != previous.working_space {
            changed.push("Colour space".to_string());
        }
        if self.perspective != previous.perspective {
            changed.push("Perspective".to_string());
        }
        if self.beautify != previous.beautify {
            changed.push("Face".to_string());
        }
        if self.ai_denoise != previous.ai_denoise {
            changed.push("AI denoise".to_string());
        }
        if self.ai_sharpen != previous.ai_sharpen {
            changed.push("AI sharpen".to_string());
        }
        if self.lut != previous.lut {
            changed.push("LUT".to_string());
        }
        if let Some(mask) = masks_changed(&self.masks, &previous.masks) {
            changed.push(mask);
        }

        match changed.len() {
            0 => "No change".to_string(),
            1 => changed.remove(0),
            2 => changed.join(" and "),
            many => format!("{} and {} more", changed.remove(0), many - 1),
        }
    }
}

pub fn masks_changed(now: &[Mask], before: &[Mask]) -> Option<String> {
    if now.len() > before.len() {
        return Some(format!("Added {}", mask_label(now, now.len() - 1)));
    }
    if now.len() < before.len() {
        return Some("Removed a mask".to_string());
    }

    let (index, mask) = now
        .iter()
        .zip(before)
        .position(|(now, before)| now != before)
        .map(|index| (index, &now[index]))?;

    let was = &before[index];
    let what = if mask.basic != was.basic {
        "Adjusted"
    } else if mask.inverted != was.inverted {
        "Inverted"

    } else if mask.visible != was.visible {
        if mask.visible { "Showed" } else { "Hid" }
    } else if mask.name != was.name {
        "Renamed"
    } else if mask.opacity != was.opacity {
        "Set the strength of"
    } else if mask.strokes.len() != was.strokes.len() {
        "Drew on"
    } else if mask.points.len() != was.points.len() {
        "Pointed at"
    } else {
        "Changed"
    };
    Some(format!("{what} {}", mask_label(now, index)))
}
