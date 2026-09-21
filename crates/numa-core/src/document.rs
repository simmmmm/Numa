use serde::{Deserialize, Serialize};

use crate::curve::Curve;
use crate::mask::Mask;
use crate::beautify::{Beautify, Portrait as Beautify_Portrait};
use crate::grading::Grading;
use crate::retouch::Retouch;
use crate::space::ColourSpace;
use crate::mixer::Mixer;
use crate::point::PointColours;

use crate::color::WhiteBalance;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Document {
    pub source: SourceImage,

    #[serde(default)]
    pub white_balance: Option<WhiteBalance>,

    #[serde(default)]
    pub film_simulation: Option<String>,

    #[serde(default)]
    pub colour_profile: Option<String>,

    #[serde(default)]
    pub working_space: ColourSpace,

    #[serde(default)]
    pub ai_denoise: f32,

    #[serde(default)]
    pub ai_sharpen: f32,

    #[serde(skip)]
    pub output_space: ColourSpace,
    pub operations: Vec<Operation>,

    #[serde(skip)]
    pub faces: Vec<Beautify_Portrait>,

    #[serde(skip)]
    pub masks_map: Option<[f32; 6]>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceImage {

    pub path: String,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Basic {
    #[serde(flatten)]
    pub tone: Tone,
    #[serde(flatten)]
    pub presence: Presence,
    #[serde(flatten)]
    pub detail: Detail,
    #[serde(flatten)]
    pub effects: Effects,
    #[serde(flatten)]
    pub calibration: Calibration,
    #[serde(flatten)]
    pub optics: Optics,
    #[serde(flatten)]
    pub balance: Balance,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Tone {
    pub exposure: f32,
    pub contrast: f32,
    pub highlights: f32,
    pub shadows: f32,
    pub whites: f32,
    pub blacks: f32,
}

impl Default for Tone {
    fn default() -> Self {
        Self {
            exposure: 0.0,
            contrast: 0.0,
            highlights: 0.0,
            shadows: 0.0,
            whites: 0.0,
            blacks: 0.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Presence {
    pub vibrance: f32,
    pub saturation: f32,

    pub hdr: f32,
    pub clarity: f32,

    #[serde(default)]
    pub texture: f32,
}

impl Default for Presence {
    fn default() -> Self {
        Self {
            vibrance: 0.0,
            saturation: 0.0,
            hdr: 0.0,
            clarity: 0.0,
            texture: 0.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Detail {

    pub sharpen: f32,

    pub sharpen_radius: f32,

    pub sharpen_masking: f32,

    pub denoise_luma: f32,

    pub denoise_detail: f32,

    pub denoise_contrast: f32,
    pub denoise_colour: f32,

    #[serde(default)]
    pub moire: f32,

    #[serde(default)]
    pub defringe: f32,
}

impl Default for Detail {
    fn default() -> Self {
        Self {

            sharpen: 25.0,

            sharpen_radius: 1.0,
            sharpen_masking: 0.0,
            denoise_luma: 0.0,

            denoise_detail: 50.0,
            denoise_contrast: 0.0,

            denoise_colour: 25.0,

            defringe: 0.0,

            moire: 0.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Effects {

    pub dehaze: f32,

    pub vignette: f32,
    pub vignette_midpoint: f32,
    pub vignette_roundness: f32,
    pub vignette_feather: f32,

    pub grain: f32,
    pub grain_size: f32,
    pub grain_roughness: f32,
}

impl Default for Effects {
    fn default() -> Self {
        Self {
            dehaze: 0.0,

            vignette: 0.0,
            vignette_midpoint: 50.0,
            vignette_roundness: 0.0,
            vignette_feather: 50.0,
            grain: 0.0,
            grain_size: 25.0,
            grain_roughness: 50.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Calibration {

    pub shadow_tint: f32,
    pub red_hue: f32,
    pub red_saturation: f32,
    pub green_hue: f32,
    pub green_saturation: f32,
    pub blue_hue: f32,
    pub blue_saturation: f32,
}

impl Default for Calibration {
    fn default() -> Self {
        Self {
            shadow_tint: 0.0,
            red_hue: 0.0,
            red_saturation: 0.0,
            green_hue: 0.0,
            green_saturation: 0.0,
            blue_hue: 0.0,
            blue_saturation: 0.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Optics {

    pub lens_distortion: f32,
    pub lens_vignetting: f32,
}

impl Default for Optics {
    fn default() -> Self {
        Self {
            lens_distortion: 0.0,
            lens_vignetting: 0.0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Balance {

    #[serde(default)]
    pub temperature: f32,
    #[serde(default)]
    pub tint: f32,
}

impl Default for Balance {
    fn default() -> Self {
        Self {
            temperature: 0.0,
            tint: 0.0,
        }
    }
}

impl Basic {

    pub fn with(change: impl FnOnce(&mut Self)) -> Self {
        let mut basic = Self::default();
        change(&mut basic);
        basic
    }

    pub fn is_identity(&self) -> bool {
        *self == Self::default()
    }

    pub fn local() -> Self {
        Self::with(|b| { b.detail.sharpen = 0.0; b.detail.denoise_colour = 0.0 })
    }

    pub fn is_local_identity(&self) -> bool {
        *self == Self::local()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct Perspective {
    #[serde(default)]
    pub vertical: f32,
    #[serde(default)]
    pub horizontal: f32,
    #[serde(default)]
    pub aspect: f32,
}

impl Perspective {
    pub fn is_identity(&self) -> bool {
        self.vertical == 0.0 && self.horizontal == 0.0 && self.aspect == 0.0
    }

    pub fn coefficients(&self) -> (f32, f32) {
        (self.vertical.clamp(-100.0, 100.0) / 200.0, self.horizontal.clamp(-100.0, 100.0) / 200.0)
    }

    pub fn stretch(&self) -> f32 {
        2.0f32.powf(self.aspect.clamp(-100.0, 100.0) / 100.0)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Operation {
    Basic(Basic),
    Crop {
        x: f32,
        y: f32,
        width: f32,
        height: f32,
        angle: f32,

        #[serde(default)]
        perspective: Perspective,
    },

    Rotate {
        degrees: f32,
        #[serde(default)]
        mirrored: bool,
    },
    Curve(Curve),

    ChannelCurve { channel: u8, curve: Curve },
    Mixer(Mixer),

    PointColours(PointColours),

    Grading(Grading),

    Retouch(Retouch),

    Beautify(Beautify),
    Mask(Mask),
}

impl Document {

    pub fn set_output_space(&mut self, space: ColourSpace) {
        self.output_space = space;
        if self.working_space == ColourSpace::Srgb {
            self.working_space = space;
        }
    }

    pub fn new(source_path: String) -> Self {
        Self {
            source: SourceImage { path: source_path },
            white_balance: None,
            film_simulation: None,
            colour_profile: None,
            working_space: ColourSpace::default(),
            ai_denoise: 0.0,
            ai_sharpen: 0.0,
            output_space: ColourSpace::default(),
            operations: Vec::new(),
            faces: Vec::new(),
            masks_map: None,
        }
    }

    pub fn crop(&self) -> Option<([f32; 4], f32)> {
        self.operations.iter().find_map(|operation| match operation {
            Operation::Crop { x, y, width, height, angle, .. } => {
                Some(([*x, *y, *width, *height], *angle))
            }
            _ => None,
        })
    }

    pub fn perspective(&self) -> Perspective {
        self.operations
            .iter()
            .find_map(|operation| match operation {
                Operation::Crop { perspective, .. } => Some(*perspective),
                _ => None,
            })
            .unwrap_or_default()
    }

    pub fn set_perspective(&mut self, perspective: Perspective) {
        let (rect, angle) = self.crop().unwrap_or(([0.0, 0.0, 1.0, 1.0], 0.0));
        self.write_crop(rect, angle, perspective);
    }

    pub fn set_crop(&mut self, rect: [f32; 4], angle: f32) {

        self.write_crop(rect, angle, self.perspective());
    }

    fn write_crop(&mut self, rect: [f32; 4], angle: f32, perspective: Perspective) {
        self.operations.retain(|operation| !matches!(operation, Operation::Crop { .. }));

        let whole =
            rect == [0.0, 0.0, 1.0, 1.0] && angle == 0.0 && perspective.is_identity();
        if !whole {
            self.operations.push(Operation::Crop {
                x: rect[0],
                y: rect[1],
                width: rect[2],
                height: rect[3],
                angle,
                perspective,
            });
        }
    }

    pub fn rotation(&self) -> f32 {
        self.operations
            .iter()
            .find_map(|operation| match operation {
                Operation::Rotate { degrees, .. } => Some(degrees.rem_euclid(360.0)),
                _ => None,
            })
            .unwrap_or(0.0)
    }

    pub fn set_rotation(&mut self, degrees: f32) {
        self.set_orientation(degrees, self.mirrored());
    }

    pub fn mirrored(&self) -> bool {
        self.operations.iter().any(|operation| matches!(operation, Operation::Rotate { mirrored: true, .. }))
    }

    pub fn set_mirrored(&mut self, mirrored: bool) {
        self.set_orientation(self.rotation(), mirrored);
    }

    fn set_orientation(&mut self, degrees: f32, mirrored: bool) {
        self.operations.retain(|operation| !matches!(operation, Operation::Rotate { .. }));
        let turns = degrees.rem_euclid(360.0);
        if turns != 0.0 || mirrored {
            self.operations.push(Operation::Rotate { degrees: turns, mirrored });
        }
    }

    pub fn curve(&self) -> Curve {
        self.operations
            .iter()
            .find_map(|operation| match operation {
                Operation::Curve(curve) => Some(curve.clone()),
                _ => None,
            })
            .unwrap_or_else(Curve::identity)
    }

    pub fn curves(&self) -> [Curve; 4] {
        let mut curves = [self.curve(), Curve::identity(), Curve::identity(), Curve::identity()];
        for operation in &self.operations {
            if let Operation::ChannelCurve { channel, curve } = operation {
                if let Some(slot) = curves.get_mut(1 + *channel as usize) {
                    *slot = curve.clone();
                }
            }
        }
        curves
    }

    pub fn set_curves(&mut self, curves: [Curve; 4]) {
        let [composite, channels @ ..] = curves;
        self.set_curve(composite);
        self.operations.retain(|operation| !matches!(operation, Operation::ChannelCurve { .. }));
        for (channel, curve) in channels.into_iter().enumerate() {
            if !curve.is_identity() {
                self.operations.push(Operation::ChannelCurve { channel: channel as u8, curve });
            }
        }
    }

    pub fn set_curve(&mut self, curve: Curve) {
        self.operations.retain(|operation| !matches!(operation, Operation::Curve(_)));
        if !curve.is_identity() {
            self.operations.push(Operation::Curve(curve));
        }
    }

    pub fn mixer(&self) -> Mixer {
        self.operations
            .iter()
            .find_map(|operation| match operation {
                Operation::Mixer(mixer) => Some(*mixer),
                _ => None,
            })
            .unwrap_or_default()
    }

    pub fn set_mixer(&mut self, mixer: Mixer) {
        self.operations.retain(|operation| !matches!(operation, Operation::Mixer(_)));
        if !mixer.is_identity() {
            self.operations.push(Operation::Mixer(mixer));
        }
    }

    pub fn point_colours(&self) -> PointColours {
        self.operations
            .iter()
            .find_map(|operation| match operation {
                Operation::PointColours(points) => Some(points.clone()),
                _ => None,
            })
            .unwrap_or_default()
    }

    pub fn set_point_colours(&mut self, points: PointColours) {
        self.operations.retain(|operation| !matches!(operation, Operation::PointColours(_)));
        if !points.points.is_empty() {
            self.operations.push(Operation::PointColours(points));
        }
    }

    pub fn grading(&self) -> Grading {
        self.operations
            .iter()
            .find_map(|operation| match operation {
                Operation::Grading(grading) => Some(*grading),
                _ => None,
            })
            .unwrap_or_default()
    }

    pub fn set_grading(&mut self, grading: Grading) {
        self.operations.retain(|operation| !matches!(operation, Operation::Grading(_)));

        if grading != Grading::default() {
            self.operations.push(Operation::Grading(grading));
        }
    }

    pub fn retouch(&self) -> Retouch {
        self.operations
            .iter()
            .find_map(|operation| match operation {
                Operation::Retouch(retouch) => Some(retouch.clone()),
                _ => None,
            })
            .unwrap_or_default()
    }

    pub fn faces(&self) -> Vec<Beautify_Portrait> {
        self.faces.clone()
    }

    pub fn beautify(&self) -> Beautify {
        self.operations
            .iter()
            .find_map(|operation| match operation {
                Operation::Beautify(beautify) => Some(*beautify),
                _ => None,
            })
            .unwrap_or_default()
    }

    pub fn set_beautify(&mut self, beautify: Beautify) {
        self.operations.retain(|operation| !matches!(operation, Operation::Beautify(_)));
        if !beautify.is_identity() {
            self.operations.push(Operation::Beautify(beautify));
        }
    }

    pub fn set_retouch(&mut self, retouch: Retouch) {
        self.operations.retain(|operation| !matches!(operation, Operation::Retouch(_)));

        if !retouch.is_identity() {
            self.operations.push(Operation::Retouch(retouch));
        }
    }

    pub fn masks(&self) -> Vec<Mask> {
        self.operations
            .iter()
            .filter_map(|operation| match operation {
                Operation::Mask(mask) => Some(mask.clone()),
                _ => None,
            })
            .collect()
    }

    pub fn mask_mut(&mut self, index: usize) -> Option<&mut Mask> {
        self.operations
            .iter_mut()
            .filter_map(|operation| match operation {
                Operation::Mask(mask) => Some(mask),
                _ => None,
            })
            .nth(index)
    }

    pub fn set_masks(&mut self, masks: Vec<Mask>) {
        self.operations.retain(|operation| !matches!(operation, Operation::Mask(_)));
        self.operations.extend(masks.into_iter().map(Operation::Mask));
    }

    pub fn basic(&self) -> Basic {
        self.operations
            .iter()
            .find_map(|operation| match operation {
                Operation::Basic(basic) => Some(*basic),
                _ => None,
            })
            .unwrap_or_default()
    }

    pub fn set_basic(&mut self, basic: Basic) {
        self.operations.retain(|operation| !matches!(operation, Operation::Basic(_)));
        if !basic.is_identity() {
            self.operations.insert(0, Operation::Basic(basic));
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct EditParts {
    pub white_balance: bool,

    pub tone: bool,

    pub colour: bool,
    pub curve: bool,

    pub detail: bool,

    pub geometry: bool,
}

impl Default for EditParts {

    fn default() -> Self {
        Self {
            white_balance: true,
            tone: true,
            colour: true,
            curve: true,
            detail: true,
            geometry: false,
        }
    }
}

impl EditParts {
    pub fn nothing() -> Self {
        Self {
            white_balance: false,
            tone: false,
            colour: false,
            curve: false,
            detail: false,
            geometry: false,
        }
    }

    pub fn any(&self) -> bool {
        *self != Self::nothing()
    }
}

impl Document {

    pub fn is_untouched(&self) -> bool {
        self.operations.is_empty()
            && self.white_balance.is_none()
            && self.film_simulation.is_none()
            && self.colour_profile.is_none()
            && self.ai_denoise == 0.0
            && self.ai_sharpen == 0.0
    }

    pub fn copy_from(&mut self, source: &Document, parts: EditParts) {
        if parts.white_balance {
            self.white_balance = source.white_balance;
        }
        if parts.colour {
            self.film_simulation = source.film_simulation.clone();
            self.colour_profile = source.colour_profile.clone();
            self.set_mixer(source.mixer());
            self.set_point_colours(source.point_colours());

            self.set_grading(source.grading());
        }
        if parts.curve {
            self.set_curves(source.curves());
        }

        if parts.geometry {

            self.set_perspective(source.perspective());
            let (rect, angle) = source.crop().unwrap_or(([0.0, 0.0, 1.0, 1.0], 0.0));
            self.set_crop(rect, angle);
            self.set_rotation(source.rotation());
            self.set_mirrored(source.mirrored());
        }

        if parts.tone || parts.colour || parts.detail {
            let from = source.basic();
            let mut basic = self.basic();
            if parts.tone {
                basic.tone.exposure = from.tone.exposure;
                basic.tone.contrast = from.tone.contrast;
                basic.tone.highlights = from.tone.highlights;
                basic.tone.shadows = from.tone.shadows;
                basic.tone.whites = from.tone.whites;
                basic.tone.blacks = from.tone.blacks;
                basic.presence.hdr = from.presence.hdr;
                basic.presence.clarity = from.presence.clarity;
                basic.presence.texture = from.presence.texture;
                basic.effects.dehaze = from.effects.dehaze;

                basic.effects.vignette = from.effects.vignette;
                basic.effects.vignette_midpoint = from.effects.vignette_midpoint;
                basic.effects.vignette_roundness = from.effects.vignette_roundness;
                basic.effects.vignette_feather = from.effects.vignette_feather;
                basic.effects.grain = from.effects.grain;
                basic.effects.grain_size = from.effects.grain_size;
                basic.effects.grain_roughness = from.effects.grain_roughness;
            }
            if parts.colour {
                basic.presence.vibrance = from.presence.vibrance;
                basic.presence.saturation = from.presence.saturation;
                basic.calibration.shadow_tint = from.calibration.shadow_tint;
                basic.calibration.red_hue = from.calibration.red_hue;
                basic.calibration.red_saturation = from.calibration.red_saturation;
                basic.calibration.green_hue = from.calibration.green_hue;
                basic.calibration.green_saturation = from.calibration.green_saturation;
                basic.calibration.blue_hue = from.calibration.blue_hue;
                basic.calibration.blue_saturation = from.calibration.blue_saturation;
            }
            if parts.detail {

                self.ai_denoise = source.ai_denoise;
                self.ai_sharpen = source.ai_sharpen;
                basic.detail.sharpen = from.detail.sharpen;
                basic.detail.sharpen_radius = from.detail.sharpen_radius;
                basic.detail.sharpen_masking = from.detail.sharpen_masking;
                basic.detail.denoise_luma = from.detail.denoise_luma;
                basic.detail.denoise_detail = from.detail.denoise_detail;
                basic.detail.denoise_contrast = from.detail.denoise_contrast;
                basic.detail.denoise_colour = from.detail.denoise_colour;
            }
            self.set_basic(basic);
        }
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn a_grade_keeps_a_hue_chosen_before_its_saturation() {
        use super::*;
        let mut document = Document::new(String::new());
        let mut grading = Grading::default();
        grading.shadows.hue = 210.0;
        grading.blending = 70.0;
        document.set_grading(grading);
        assert_eq!(document.grading(), grading);
        document.set_grading(Grading::default());
        assert!(!document.operations.iter().any(|operation| matches!(operation, Operation::Grading(_))));
    }

    #[test]
    fn a_fresh_mask_asks_for_no_sharpening() {
        use crate::mask::{Mask, Shape};
        let mask = Mask::new(Shape::linear());
        assert!(mask.is_idle(), "a mask nobody has touched does nothing");
        assert_eq!(mask.basic.detail.sharpen, 0.0);
        assert_eq!(mask.basic.detail.denoise_colour, 0.0);

        assert_eq!(Basic::default().detail.sharpen, 25.0);
        assert_eq!(Basic::default().detail.denoise_colour, 25.0);

        assert!(mask.curve.is_identity());
        let curved = r#"{"shape":{"type":"Linear","from":[0.0,0.0],"to":[0.0,1.0]},"curve":{"points":[[0.0,0.0],[0.5,0.8],[1.0,1.0]]}}"#;
        let read: Mask = serde_json::from_str(curved).expect("a mask with a curve parses");
        assert!(!read.curve.is_identity(), "the curve came back");
        assert!(!read.is_idle(), "a mask that only carries a curve still does something");

        let stored = r#"{"shape":{"type":"Linear","from":[0.0,0.0],"to":[0.0,1.0]}}"#;
        let read: Mask = serde_json::from_str(stored).expect("an old mask still parses");
        assert!(read.is_idle(), "a mask from before this still asks for nothing");
    }

    #[test]
    fn a_stack_from_before_b3_reads_back_unchanged() {
        let before = r#"{"source":{"path":"/tmp/x.RAF"},"white_balance":null,"operations":[{"type":"Basic","exposure":1.5,"contrast":-10.0,"hdr":20.0}]}"#;
        let document: Document = serde_json::from_str(before).expect("an old stack still parses");
        let basic = document.basic();
        assert_eq!(basic.tone.exposure, 1.5);
        assert_eq!(basic.balance.temperature, 0.0, "no white balance shift where none was written");
        assert_eq!(basic.balance.tint, 0.0);

        let written = serde_json::to_string(&document).expect("writes");
        let again: Document = serde_json::from_str(&written).expect("reads");
        assert_eq!(written, serde_json::to_string(&again).expect("writes again"));
    }

    use super::*;

    #[test]
    fn basic_json() {
        const DEFAULT: &str = r#"{"exposure":0.0,"contrast":0.0,"highlights":0.0,"shadows":0.0,"whites":0.0,"blacks":0.0,"vibrance":0.0,"saturation":0.0,"hdr":0.0,"clarity":0.0,"texture":0.0,"sharpen":25.0,"sharpen_radius":1.0,"sharpen_masking":0.0,"denoise_luma":0.0,"denoise_detail":50.0,"denoise_contrast":0.0,"denoise_colour":25.0,"moire":0.0,"defringe":0.0,"dehaze":0.0,"vignette":0.0,"vignette_midpoint":50.0,"vignette_roundness":0.0,"vignette_feather":50.0,"grain":0.0,"grain_size":25.0,"grain_roughness":50.0,"shadow_tint":0.0,"red_hue":0.0,"red_saturation":0.0,"green_hue":0.0,"green_saturation":0.0,"blue_hue":0.0,"blue_saturation":0.0,"lens_distortion":0.0,"lens_vignetting":0.0,"temperature":0.0,"tint":0.0}"#;
        const LOCAL: &str = r#"{"exposure":0.0,"contrast":0.0,"highlights":0.0,"shadows":0.0,"whites":0.0,"blacks":0.0,"vibrance":0.0,"saturation":0.0,"hdr":0.0,"clarity":0.0,"texture":0.0,"sharpen":0.0,"sharpen_radius":1.0,"sharpen_masking":0.0,"denoise_luma":0.0,"denoise_detail":50.0,"denoise_contrast":0.0,"denoise_colour":0.0,"moire":0.0,"defringe":0.0,"dehaze":0.0,"vignette":0.0,"vignette_midpoint":50.0,"vignette_roundness":0.0,"vignette_feather":50.0,"grain":0.0,"grain_size":25.0,"grain_roughness":50.0,"shadow_tint":0.0,"red_hue":0.0,"red_saturation":0.0,"green_hue":0.0,"green_saturation":0.0,"blue_hue":0.0,"blue_saturation":0.0,"lens_distortion":0.0,"lens_vignetting":0.0,"temperature":0.0,"tint":0.0}"#;
        const EVERY: &str = r#"{"exposure":1.5,"contrast":3.0,"highlights":4.5,"shadows":6.0,"whites":7.5,"blacks":9.0,"vibrance":10.5,"saturation":12.0,"hdr":13.5,"clarity":15.0,"texture":16.5,"sharpen":18.0,"sharpen_radius":19.5,"sharpen_masking":21.0,"denoise_luma":22.5,"denoise_detail":24.0,"denoise_contrast":25.5,"denoise_colour":27.0,"moire":28.5,"defringe":30.0,"dehaze":31.5,"vignette":33.0,"vignette_midpoint":34.5,"vignette_roundness":36.0,"vignette_feather":37.5,"grain":39.0,"grain_size":40.5,"grain_roughness":42.0,"shadow_tint":43.5,"red_hue":45.0,"red_saturation":46.5,"green_hue":48.0,"green_saturation":49.5,"blue_hue":51.0,"blue_saturation":52.5,"lens_distortion":54.0,"lens_vignetting":55.5,"temperature":57.0,"tint":58.5}"#;
        assert_eq!(serde_json::to_string(&Basic::default()).unwrap(), DEFAULT);
        assert_eq!(serde_json::to_string(&Basic::local()).unwrap(), LOCAL);
        for json in [DEFAULT, LOCAL, EVERY] {
            let read: Basic = serde_json::from_str(json).unwrap();
            assert_eq!(serde_json::to_string(&read).unwrap(), json, "a round trip changed it");
        }
        let every: Basic = serde_json::from_str(EVERY).unwrap();
        assert_eq!((every.tone.exposure, every.detail.defringe, every.balance.tint), (1.5, 30.0, 58.5));

        let old: Basic = serde_json::from_str(r#"{"exposure":1.0,"sharpen":40.0}"#).unwrap();
        assert_eq!(old.tone.exposure, 1.0);
        assert_eq!(old.detail.sharpen, 40.0);
        assert_eq!(old.detail.denoise_colour, Basic::default().detail.denoise_colour);
        assert_eq!(old.effects.vignette_midpoint, 50.0);
    }

    fn edited() -> Document {
        let mut document = Document::new("source.RAF".into());
        document.white_balance = Some(WhiteBalance { temperature: 7200.0, tint: 4.0 });
        document.film_simulation = Some("astia".into());
        document.ai_denoise = 80.0;
        document.set_basic(Basic::with(|b| { b.tone.exposure = 1.5; b.tone.shadows = 30.0; b.presence.saturation = 20.0; b.detail.sharpen = 70.0 }));
        document.set_curve(Curve::new([[0.0, 0.0], [0.5, 0.7], [1.0, 1.0]]));
        document.set_crop([0.1, 0.1, 0.5, 0.5], 2.0);
        document.set_rotation(90.0);
        document
    }

    #[test]
    fn masks_can_be_added_and_removed() {
        use crate::mask::{Mask, Shape};

        let mut document = Document::new("a.RAF".into());
        assert!(document.masks().is_empty());

        let mut first = Mask::new(Shape::linear());
        first.basic.tone.exposure = -1.0;
        let mut second = Mask::new(Shape::radial());
        second.basic.tone.exposure = 1.0;
        let third = Mask::new(Shape::linear());

        document.set_masks(vec![first, second, third]);
        assert_eq!(document.masks().len(), 3);

        let mut masks = document.masks();
        masks.remove(1);
        document.set_masks(masks);

        let left = document.masks();
        assert_eq!(left.len(), 2);
        assert_eq!(left[0].basic.tone.exposure, -1.0, "the first one moved or changed");
        assert!(left[1].is_idle(), "the third one is now the second");

        document.set_masks(Vec::new());
        assert!(document.masks().is_empty());
        assert!(document.is_untouched(), "an emptied stack is an untouched one");
    }

    #[test]
    fn copying_takes_the_settings_and_leaves_the_crop() {
        let source = edited();
        let mut target = Document::new("target.RAF".into());
        target.copy_from(&source, EditParts::default());

        assert_eq!(target.white_balance, source.white_balance);
        assert_eq!(target.film_simulation, source.film_simulation);
        assert_eq!(target.basic().tone.exposure, 1.5);
        assert_eq!(target.basic().presence.saturation, 20.0);
        assert_eq!(target.basic().detail.sharpen, 70.0);
        assert_eq!(target.ai_denoise, 80.0, "AI denoise goes with the detail");
        assert_eq!(target.curve(), source.curve());

        assert_eq!(target.crop(), None, "the crop stayed behind");
        assert_eq!(target.rotation(), 0.0, "and so did the turn");
    }

    #[test]
    fn each_group_travels_on_its_own() {
        let source = edited();

        let mut textured = Document::new("t.RAF".into());
        textured.set_basic(Basic::with(|b| b.presence.texture = -60.0));
        let mut plain = Document::new("p.RAF".into());
        plain.copy_from(&textured, EditParts { tone: true, ..EditParts::nothing() });
        assert_eq!(plain.basic().presence.texture, -60.0);

        let only_tone = EditParts { tone: true, ..EditParts::nothing() };
        let mut target = Document::new("t".into());
        target.copy_from(&source, only_tone);
        assert_eq!(target.basic().tone.exposure, 1.5);
        assert_eq!(target.basic().presence.saturation, 0.0, "colour is a different group");
        assert_eq!(target.basic().detail.sharpen, 25.0, "and so is detail");
        assert_eq!(target.white_balance, None);
        assert_eq!(target.curve(), Curve::identity());

        let only_geometry = EditParts { geometry: true, ..EditParts::nothing() };
        let mut target = Document::new("t".into());
        target.copy_from(&source, only_geometry);
        assert_eq!(target.crop(), Some(([0.1, 0.1, 0.5, 0.5], 2.0)));
        assert_eq!(target.rotation(), 90.0);
        assert_eq!(target.basic(), Basic::default(), "nothing else moved");
    }

    #[test]
    fn pasting_an_untouched_edit_clears_rather_than_merges() {
        let mut target = edited();
        let clean = Document::new("clean.RAF".into());

        target.copy_from(&clean, EditParts::default());
        assert_eq!(target.white_balance, None);
        assert_eq!(target.film_simulation, None);
        assert_eq!(target.basic(), Basic::default());
        assert_eq!(target.curve(), Curve::identity());
        assert!(target.operations.is_empty() || target.crop().is_some());

        assert_eq!(target.crop(), Some(([0.1, 0.1, 0.5, 0.5], 2.0)));
        assert_eq!(target.rotation(), 90.0);
    }

    #[test]
    fn copying_nothing_changes_nothing() {
        let source = edited();
        let mut target = Document::new("t".into());
        let before = serde_json::to_string(&target).unwrap();
        target.copy_from(&source, EditParts::nothing());
        assert_eq!(serde_json::to_string(&target).unwrap(), before);
        assert!(!EditParts::nothing().any());
        assert!(EditParts::default().any());
    }

    #[test]
    fn opening_a_photograph_is_not_editing_it() {
        let mut document = Document::new("a.RAF".into());
        assert!(document.is_untouched());

        document.set_basic(Basic::default());
        document.set_curve(Curve::identity());
        document.set_crop([0.0, 0.0, 1.0, 1.0], 0.0);
        document.set_rotation(0.0);
        assert!(document.is_untouched(), "storing the defaults marked it edited");

        for touch in [
            |d: &mut Document| d.set_basic(Basic::with(|b| b.tone.exposure = 0.1)),
            |d: &mut Document| d.set_rotation(90.0),
            |d: &mut Document| d.set_curve(Curve::new([[0.0, 0.0], [0.5, 0.6], [1.0, 1.0]])),
            |d: &mut Document| d.white_balance = Some(WhiteBalance { temperature: 6000.0, tint: 0.0 }),
            |d: &mut Document| d.film_simulation = Some("astia".into()),
        ] {
            let mut document = Document::new("a.RAF".into());
            touch(&mut document);
            assert!(!document.is_untouched());
        }
    }

    #[test]
    fn channel_curves_are_kept_and_straight_ones_are_not() {
        let mut document = Document::new("a.RAF".into());
        let red = Curve::new([[0.0, 0.0], [0.5, 0.6], [1.0, 1.0]]);
        document.set_curves([Curve::identity(), red.clone(), Curve::identity(), Curve::identity()]);
        assert!(!document.is_untouched());

        let back: Document = serde_json::from_str(&serde_json::to_string(&document).unwrap()).unwrap();
        assert_eq!(back.curves()[1], red);
        assert!(back.curves()[0].is_identity() && back.curves()[2].is_identity());

        document.set_curves(Default::default());
        assert!(document.is_untouched());
    }

    #[test]
    fn the_default_develop_is_still_the_empty_stack() {
        let mut document = Document::new("x".into());
        document.set_basic(Basic::default());
        assert!(document.operations.is_empty(), "the default must not be stored as an edit");
        assert!(Basic::default().is_identity());

        document.set_basic(Basic::with(|b| b.detail.sharpen = 0.0));
        assert_eq!(document.operations.len(), 1);
    }

    #[test]
    fn an_older_edit_stack_still_loads() {
        const BEFORE: &str = r#"{
            "source": { "path": "/photos/a.RAF" },
            "white_balance": null,
            "film_simulation": null,
            "operations": [
                { "type": "Basic", "exposure": 0.5, "contrast": 10.0, "highlights": 0.0,
                  "shadows": 20.0, "whites": 0.0, "blacks": 0.0, "vibrance": 0.0,
                  "saturation": 0.0, "hdr": 0.0, "clarity": 0.0 }
            ]
        }"#;

        let document: Document = serde_json::from_str(BEFORE).expect("an older stack must load");
        let basic = document.basic();
        assert_eq!(basic.tone.exposure, 0.5, "the edits that were there survive");
        assert_eq!(basic.tone.shadows, 20.0);
        assert_eq!(basic.detail.sharpen, 25.0, "and the new field takes its default");
        assert_eq!(basic.detail.sharpen_radius, 1.0);
    }

    #[test]
    fn basic_replaces_rather_than_stacks() {
        let mut document = Document::new("a.raf".into());
        assert_eq!(document.basic(), Basic::default());

        for exposure in [0.1, 0.2, 0.3] {
            document.set_basic(Basic::with(|b| b.tone.exposure = exposure));
        }
        assert_eq!(document.operations.len(), 1);
        assert_eq!(document.basic().tone.exposure, 0.3);

        document.operations.push(Operation::Rotate { degrees: 90.0, mirrored: false });
        document.set_basic(Basic::with(|b| b.tone.contrast = 20.0));
        assert_eq!(document.operations.len(), 2);
        assert_eq!(document.basic().tone.contrast, 20.0);

        document.set_basic(Basic::default());
        assert_eq!(document.operations.len(), 1);
        assert!(matches!(document.operations[0], Operation::Rotate { .. }));
    }

    #[test]
    fn geometry_replaces_rather_than_stacks() {
        let mut document = Document::new("a.raf".into());
        assert_eq!(document.crop(), None);
        assert_eq!(document.rotation(), 0.0);

        for width in [0.9, 0.8, 0.7] {
            document.set_crop([0.0, 0.0, width, 1.0], 0.0);
        }
        assert_eq!(document.operations.len(), 1);
        assert_eq!(document.crop().unwrap().0[2], 0.7);

        document.set_crop([0.0, 0.0, 1.0, 1.0], 0.0);
        assert_eq!(document.crop(), None);
        assert!(document.operations.is_empty());

        document.set_crop([0.0, 0.0, 1.0, 1.0], 1.5);
        assert_eq!(document.crop().unwrap().1, 1.5);

        document.set_rotation(450.0);
        assert_eq!(document.rotation(), 90.0);
        document.set_rotation(360.0);
        assert_eq!(document.rotation(), 0.0, "a full turn is no turn");
        assert!(document.crop().is_some(), "and it leaves the crop alone");
    }
}
