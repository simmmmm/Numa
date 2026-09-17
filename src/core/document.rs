use serde::{Deserialize, Serialize};

use crate::core::curve::Curve;
use crate::core::mask::Mask;
use crate::core::beautify::{Beautify, Portrait as Beautify_Portrait};
use crate::core::grading::Grading;
use crate::core::retouch::Retouch;
use crate::core::space::ColourSpace;
use crate::core::mixer::Mixer;
use crate::core::point::PointColours;

use crate::core::color::WhiteBalance;

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

    #[serde(skip)]
    pub output_space: ColourSpace,
    pub operations: Vec<Operation>,

    #[serde(skip)]
    pub faces: Vec<Beautify_Portrait>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SourceImage {

    pub path: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Basic {
    pub exposure: f32,
    pub contrast: f32,
    pub highlights: f32,
    pub shadows: f32,
    pub whites: f32,
    pub blacks: f32,
    pub vibrance: f32,
    pub saturation: f32,

    pub hdr: f32,
    pub clarity: f32,

    #[serde(default)]
    pub texture: f32,

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

    pub dehaze: f32,

    pub vignette: f32,
    pub vignette_midpoint: f32,
    pub vignette_roundness: f32,
    pub vignette_feather: f32,

    pub grain: f32,
    pub grain_size: f32,
    pub grain_roughness: f32,

    pub shadow_tint: f32,
    pub red_hue: f32,
    pub red_saturation: f32,
    pub green_hue: f32,
    pub green_saturation: f32,
    pub blue_hue: f32,
    pub blue_saturation: f32,

    pub lens_distortion: f32,
    pub lens_vignetting: f32,
}

impl Default for Basic {
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
            exposure: 0.0,
            contrast: 0.0,
            highlights: 0.0,
            shadows: 0.0,
            whites: 0.0,
            blacks: 0.0,
            vibrance: 0.0,
            saturation: 0.0,
            hdr: 0.0,
            clarity: 0.0,
            texture: 0.0,
            dehaze: 0.0,

            vignette: 0.0,
            vignette_midpoint: 50.0,
            vignette_roundness: 0.0,
            vignette_feather: 50.0,
            grain: 0.0,
            grain_size: 25.0,
            grain_roughness: 50.0,
            shadow_tint: 0.0,
            red_hue: 0.0,
            red_saturation: 0.0,
            green_hue: 0.0,
            green_saturation: 0.0,
            blue_hue: 0.0,
            blue_saturation: 0.0,
            lens_distortion: 0.0,
            lens_vignetting: 0.0,
        }
    }
}

impl Basic {

    pub fn is_identity(&self) -> bool {
        *self == Self::default()
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
    pub fn new(source_path: String) -> Self {
        Self {
            source: SourceImage { path: source_path },
            white_balance: None,
            film_simulation: None,
            colour_profile: None,
            working_space: ColourSpace::default(),
            ai_denoise: 0.0,
            output_space: ColourSpace::default(),
            operations: Vec::new(),
            faces: Vec::new(),
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
        if !grading.is_identity() {
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
                basic.exposure = from.exposure;
                basic.contrast = from.contrast;
                basic.highlights = from.highlights;
                basic.shadows = from.shadows;
                basic.whites = from.whites;
                basic.blacks = from.blacks;
                basic.hdr = from.hdr;
                basic.clarity = from.clarity;
                basic.texture = from.texture;
                basic.dehaze = from.dehaze;

                basic.vignette = from.vignette;
                basic.vignette_midpoint = from.vignette_midpoint;
                basic.vignette_roundness = from.vignette_roundness;
                basic.vignette_feather = from.vignette_feather;
                basic.grain = from.grain;
                basic.grain_size = from.grain_size;
                basic.grain_roughness = from.grain_roughness;
            }
            if parts.colour {
                basic.vibrance = from.vibrance;
                basic.saturation = from.saturation;
                basic.shadow_tint = from.shadow_tint;
                basic.red_hue = from.red_hue;
                basic.red_saturation = from.red_saturation;
                basic.green_hue = from.green_hue;
                basic.green_saturation = from.green_saturation;
                basic.blue_hue = from.blue_hue;
                basic.blue_saturation = from.blue_saturation;
            }
            if parts.detail {

                self.ai_denoise = source.ai_denoise;
                basic.sharpen = from.sharpen;
                basic.sharpen_radius = from.sharpen_radius;
                basic.sharpen_masking = from.sharpen_masking;
                basic.denoise_luma = from.denoise_luma;
                basic.denoise_detail = from.denoise_detail;
                basic.denoise_contrast = from.denoise_contrast;
                basic.denoise_colour = from.denoise_colour;
            }
            self.set_basic(basic);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn edited() -> Document {
        let mut document = Document::new("source.RAF".into());
        document.white_balance = Some(WhiteBalance { temperature: 7200.0, tint: 4.0 });
        document.film_simulation = Some("astia".into());
        document.ai_denoise = 80.0;
        document.set_basic(Basic {
            exposure: 1.5,
            shadows: 30.0,
            saturation: 20.0,
            sharpen: 70.0,
            ..Default::default()
        });
        document.set_curve(Curve::new([[0.0, 0.0], [0.5, 0.7], [1.0, 1.0]]));
        document.set_crop([0.1, 0.1, 0.5, 0.5], 2.0);
        document.set_rotation(90.0);
        document
    }

    #[test]
    fn masks_can_be_added_and_removed() {
        use crate::core::mask::{Mask, Shape};

        let mut document = Document::new("a.RAF".into());
        assert!(document.masks().is_empty());

        let mut first = Mask::new(Shape::linear());
        first.basic.exposure = -1.0;
        let mut second = Mask::new(Shape::radial());
        second.basic.exposure = 1.0;
        let third = Mask::new(Shape::linear());

        document.set_masks(vec![first, second, third]);
        assert_eq!(document.masks().len(), 3);

        let mut masks = document.masks();
        masks.remove(1);
        document.set_masks(masks);

        let left = document.masks();
        assert_eq!(left.len(), 2);
        assert_eq!(left[0].basic.exposure, -1.0, "the first one moved or changed");
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
        assert_eq!(target.basic().exposure, 1.5);
        assert_eq!(target.basic().saturation, 20.0);
        assert_eq!(target.basic().sharpen, 70.0);
        assert_eq!(target.ai_denoise, 80.0, "AI denoise goes with the detail");
        assert_eq!(target.curve(), source.curve());

        assert_eq!(target.crop(), None, "the crop stayed behind");
        assert_eq!(target.rotation(), 0.0, "and so did the turn");
    }

    #[test]
    fn each_group_travels_on_its_own() {
        let source = edited();

        let mut textured = Document::new("t.RAF".into());
        textured.set_basic(Basic { texture: -60.0, ..Default::default() });
        let mut plain = Document::new("p.RAF".into());
        plain.copy_from(&textured, EditParts { tone: true, ..EditParts::nothing() });
        assert_eq!(plain.basic().texture, -60.0);

        let only_tone = EditParts { tone: true, ..EditParts::nothing() };
        let mut target = Document::new("t".into());
        target.copy_from(&source, only_tone);
        assert_eq!(target.basic().exposure, 1.5);
        assert_eq!(target.basic().saturation, 0.0, "colour is a different group");
        assert_eq!(target.basic().sharpen, 25.0, "and so is detail");
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
            |d: &mut Document| d.set_basic(Basic { exposure: 0.1, ..Default::default() }),
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

        document.set_basic(Basic { sharpen: 0.0, ..Default::default() });
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
        assert_eq!(basic.exposure, 0.5, "the edits that were there survive");
        assert_eq!(basic.shadows, 20.0);
        assert_eq!(basic.sharpen, 25.0, "and the new field takes its default");
        assert_eq!(basic.sharpen_radius, 1.0);
    }

    #[test]
    fn basic_replaces_rather_than_stacks() {
        let mut document = Document::new("a.raf".into());
        assert_eq!(document.basic(), Basic::default());

        for exposure in [0.1, 0.2, 0.3] {
            document.set_basic(Basic { exposure, ..Default::default() });
        }
        assert_eq!(document.operations.len(), 1);
        assert_eq!(document.basic().exposure, 0.3);

        document.operations.push(Operation::Rotate { degrees: 90.0, mirrored: false });
        document.set_basic(Basic { contrast: 20.0, ..Default::default() });
        assert_eq!(document.operations.len(), 2);
        assert_eq!(document.basic().contrast, 20.0);

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
