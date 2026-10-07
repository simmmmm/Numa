use super::*;
use numa_core::profile::Matrix3;

pub struct Plan {

    pub width: u32,
    pub height: u32,

    pub source: (u32, u32),

    pub geometry: Option<GeometryPlan>,

    pub camera: Option<Camera>,

    pub dehaze: Option<effects::HazeShape>,

    pub denoise_luma: Option<detail::LumaShape>,

    pub denoise_colour: Option<(f32, u32)>,

    pub tail: Tail,

    pub adjustments: Adjustments,

    pub local: Option<local::ToneShape>,

    pub masks: Vec<MaskPlan>,

    pub vignette: Option<[f32; 7]>,

    pub grain: Option<[f32; 3]>,

    pub full: [f32; 2],

    pub curves: [Option<[f32; curve::LOOKUP]>; 4],

    pub display_referred: bool,

    pub base_curve: [f32; BASE_STEPS],
    pub toe: f32,

    pub agx: bool,
}

#[derive(Debug)]
pub struct GeometryPlan {

    pub mirror: bool,

    pub turn: (bool, bool, bool),
    pub crop: Option<CropPlan>,
}

#[derive(Debug)]
pub struct CropPlan {

    pub size: (f32, f32),

    pub centre: (f32, f32),
    pub half: (f32, f32),

    pub sin: f32,
    pub cos: f32,

    pub keystone: (f32, f32),
    pub stretch: f32,
}

pub const BASE_STEPS: usize = 53;

pub struct Camera {
    pub multipliers: [f32; 3],

    pub lowest: f32,
    pub clip: Option<f32>,
    pub profile: CameraProfileStage,

    pub white_point: numa_core::color::WhiteBalance,
}

pub enum CameraProfileStage {

    Rendering { to_prophoto: Matrix3, map: Option<TablePlan>, look: Option<TablePlan>, to_srgb: Matrix3 },

    Matrix(Matrix3),
}

pub struct TablePlan {

    pub divisions: [u32; 3],

    pub srgb: bool,

    pub white: f32,
    pub entries: Vec<[f32; 3]>,
}

#[derive(Default)]
pub struct Adjustments {

    pub basic: Option<BasicPlan>,

    pub mixer: Option<LookPlan>,

    pub points: Option<PointsPlan>,

    pub monochrome: Option<[f32; 8]>,

    pub grading: Option<([[f32; 4]; 4], (f32, f32))>,
}

pub struct PointsPlan {
    pub points: Vec<[f32; 8]>,
    pub highlight: Option<[f32; 4]>,
}

#[derive(Default, Debug)]
pub struct Tail {
    pub sharpen: Option<detail::SharpenShape>,
    pub defringe: Option<(f32, u32)>,
    pub moire: Option<(f32, u32, u32)>,
}

impl Tail {
    fn of(detail: &numa_core::document::Detail, (width, height): (usize, usize), scale: f32) -> Self {

        let big = width >= 3 && height >= 3;
        let (near, far) = detail::moire_radii(scale);
        let amount = |slider: f32| Some((slider / 100.0).clamp(0.0, 1.0)).filter(|amount| *amount > 0.0 && big);
        Tail {
            sharpen: detail::SharpenShape::new(detail.sharpen / 100.0, detail.sharpen_radius, detail.sharpen_masking / 100.0, scale),
            defringe: amount(detail.defringe).map(|amount| (amount, detail::defringe_radius(scale) as u32)),
            moire: amount(detail.moire).map(|amount| (amount, near as u32, far as u32)),
        }
    }

    pub fn is_some(&self) -> bool {
        self.sharpen.is_some() || self.defringe.is_some() || self.moire.is_some()
    }
}

pub struct MaskPlan {

    pub key: u64,

    of: (std::sync::Arc<Vec<Mask>>, usize, (usize, usize), [f32; 6]),

    pub gains: Option<[f32; 3]>,

    pub dehaze: Option<effects::HazeShape>,

    pub denoise_luma: Option<detail::LumaShape>,

    pub denoise_colour: Option<(f32, u32)>,
    pub tail: Tail,
    pub adjustments: Adjustments,

    pub local: Option<local::ToneShape>,

    pub curves: Option<[Option<[f32; curve::LOOKUP]>; 4]>,

    pub tint: Option<([f32; 3], f32)>,

    pub grain: Option<[f32; 3]>,
}

const CARD_FRAME_NS: f32 = 1e8;

impl Plan {

    pub fn heavy(&self) -> bool {
        fn on(yes: bool, ns: f32) -> f32 {
            if yes { ns } else { 0.0 }
        }
        fn adjustments(a: &Adjustments) -> f32 {
            let basic = a.basic.as_ref().map_or(0.0, |b| on(b.tone.is_some(), 27.0) + on(b.slope.is_some(), 17.0));
            basic + on(a.mixer.is_some(), 61.0) + on(a.points.is_some(), 106.0) + on(a.monochrome.is_some(), 56.0) + on(a.grading.is_some(), 92.0)
        }
        fn tail(tail: &Tail) -> f32 {
            on(tail.sharpen.is_some(), 21.0) + on(tail.defringe.is_some(), 25.0) + on(tail.moire.is_some(), 43.0)
        }

        fn local(shape: &Option<local::ToneShape>) -> f32 {
            shape.as_ref().map_or(0.0, |shape| 24.0 + on(shape.texture, 29.0))
        }

        let masks: f32 = self
            .masks
            .iter()
            .map(|mask| {
                12.0 + adjustments(&mask.adjustments)
                    + on(mask.denoise_colour.is_some(), 40.0)
                    + on(mask.curves.is_some(), 320.0)
                    + local(&mask.local)

                    + on(mask.local.is_some() || mask.dehaze.is_some(), 12.0)
                    + on(mask.dehaze.is_some(), 19.0)
                    + tail(&mask.tail)
                    + on(mask.grain.is_some(), 53.0)
            })
            .sum();
        let ns = 18.0
            + adjustments(&self.adjustments)
            + masks
            + local(&self.local)
            + on(self.vignette.is_some(), 14.0)
            + on(self.grain.is_some(), 53.0);
        ns * (self.width * self.height) as f32 / 4.0 >= CARD_FRAME_NS
    }
}

impl Plan {

    pub fn prefix_kept(&self) -> bool {
        self.dehaze.is_some() || self.denoise_luma.is_some() || self.tail.is_some()
    }

    pub fn prefix_key(&self) -> Option<u64> {
        use std::hash::{Hash, Hasher};
        self.prefix_kept().then(|| {
            let mut hasher = std::collections::hash_map::DefaultHasher::new();

            format!("{:?}", (self.width, self.height, self.source, &self.geometry, &self.dehaze, &self.denoise_luma, self.denoise_colour, &self.tail)).hash(&mut hasher);
            hasher.finish()
        })
    }
}

impl MaskPlan {

    pub fn field(&self) -> Vec<f32> {
        let (masks, index, (width, height), map) = &self.of;
        numa_core::mask::field_among(masks, *index, *width, *height, WHOLE_FRAME, *map)
    }
}

pub struct BasicPlan {

    pub gain: f32,

    pub slope: Option<f32>,

    pub tone: Option<Vec<f32>>,
    pub saturation: f32,
    pub vibrance: f32,
}

pub const TONE_LOW: f32 = ToneCurve::LOW;
pub const TONE_STEP: f32 = ToneCurve::STEP;

pub struct LookPlan {
    pub into: Matrix3,
    pub table: TablePlan,
    pub out: Matrix3,
}

pub fn plan(document: &Document, source: &LinearImage, inputs: &RenderInputs, detail_scale: f32) -> Result<Plan, &'static str> {

    if document.ai_denoise > 0.0 || document.ai_sharpen > 0.0 {
        return Err("AI denoise");
    }

    if document.working_space != ColourSpace::Srgb || document.output_space != ColourSpace::Srgb {
        return Err("a colour space other than sRGB");
    }
    let camera = match source.profile.clone() {
        Some(profile) => Some(camera_stage(document, source, inputs, &profile)),

        None if document.white_balance.is_some() => return Err("a finished picture's white balance"),
        None => None,
    };

    let basic_now = document.basic();
    if basic_now.optics.lens_distortion != 0.0 || basic_now.optics.lens_vignetting != 0.0 {
        return Err("manual lens correction");
    }

    if basic_now.effects.mist != 0.0 {
        return Err("mist");
    }
    let (geometry, (width, height)) = geometry_stage(document, source.width, source.height);

    let mut basic = basic_now;
    if source.display_referred {
        let rest = numa_core::document::Detail::default();
        basic.detail.sharpen = (basic.detail.sharpen - rest.sharpen).max(0.0);
        basic.detail.denoise_colour = (basic.detail.denoise_colour - rest.denoise_colour).max(0.0);
    }
    if !document.retouch().is_identity() {
        return Err("spot removal");
    }
    if !document.faces().is_empty() && !document.beautify().is_identity() {
        return Err("face retouching");
    }
    let dehaze = effects::HazeShape::new(width as usize, height as usize, basic.effects.dehaze);
    let detail = &basic.detail;
    let denoise_luma = detail::LumaShape::new(detail.denoise_luma / 100.0, detail.denoise_detail / 100.0, detail.denoise_contrast / 100.0, detail_scale);
    let tail = Tail::of(detail, (width as usize, height as usize), detail_scale);
    let colour = (detail.denoise_colour / 100.0).clamp(0.0, 1.0);
    let denoise_colour = (colour > 0.0 && 4.0 * detail_scale >= 0.5).then(|| (colour, (4.0 * detail_scale).round().max(1.0) as u32));
    let calibration = &basic.calibration;
    if [calibration.red_hue, calibration.green_hue, calibration.blue_hue] != [0.0; 3]
        || [calibration.red_saturation, calibration.green_saturation, calibration.blue_saturation] != [0.0; 3]
        || calibration.shadow_tint != 0.0
    {
        return Err("calibration");
    }

    if document.operations.iter().filter(|operation| matches!(operation, Operation::Basic(_))).count() > 1 {
        return Err("more than one set of basic adjustments");
    }
    let presence = &basic_now.presence;
    let local = (presence.hdr != 0.0 || presence.clarity != 0.0 || presence.texture != 0.0)
        .then(|| local::ToneShape::new(width as usize, height as usize, [presence.hdr / 100.0, presence.clarity / 100.0, presence.texture / 100.0]));
    let mixer = document.mixer();
    let adjustments = Adjustments {
        basic: (!basic_now.is_identity()).then(|| basic_stage(&basic_now)),
        ..adjustments(&mixer, &document.point_colours(), &document.grading())
    };

    let white_point = camera.as_ref().map_or(FINISHED_WHITE, |camera| camera.white_point);
    let masks = masks_stage(document, (width as usize, height as usize), white_point, detail_scale);
    let vignette = effects::vignette_shape([width as f32, height as f32], [
        basic_now.effects.vignette,
        basic_now.effects.vignette_midpoint,
        basic_now.effects.vignette_roundness,
        basic_now.effects.vignette_feather,
    ]);

    if !looks_of(document).is_empty() {
        return Err("a LUT or As Shot");
    }
    let curves = document.curves().map(|curve| (!curve.is_identity()).then(|| curve.lookup()));
    let base_curve = std::array::from_fn(|k| tone::curve(MIDDLE_GREY * (k as f32 / 4.0 - 8.0).exp2()));

    let toe = -1.0 / (tone::curve(MIDDLE_GREY * (-9.0f32).exp2()) / base_curve[0]).log2();

    let grain = effects::grain_shape([basic_now.effects.grain, basic_now.effects.grain_size, basic_now.effects.grain_roughness]);
    let full = [width as f32, height as f32].map(|edge| edge / detail_scale.max(1e-6));

    Ok(Plan {
        width,
        height,
        source: (source.width, source.height),
        geometry,
        camera,
        dehaze,
        denoise_luma,
        denoise_colour,
        tail,
        local,
        adjustments,
        masks,
        vignette,
        grain,
        full,
        curves,
        display_referred: source.display_referred,
        base_curve,
        toe,
        agx: document.tone_mapping == tone::ToneMapping::Agx && !source.display_referred,
    })
}

fn adjustments(mixer: &numa_core::mixer::Mixer, points: &PointColours, grading: &numa_core::grading::Grading) -> Adjustments {
    Adjustments {
        basic: None,
        mixer: (!mixer.colour_is_identity()).then(|| {

            let look = Look::new(Table::resolve(&mixer.table(), 0.0)).in_space(ColourSpace::Srgb);
            let (table, into, out) = look.raw();
            LookPlan { into, table: table_plan(table, 1.0), out }
        }),
        points: (!points.is_identity()).then(|| PointsPlan {
            points: points
                .points
                .iter()
                .filter(|point| !point.is_idle())
                .map(|point| {
                    let [l, c, h] = point.target;
                    [l, c, h, point.range.clamp(0.0, 100.0) / 100.0, point.hue, point.saturation, point.luminance, 0.0]
                })
                .collect(),
            highlight: points.highlight.and_then(|at| points.points.get(at)).map(|point| {
                let [l, c, h] = point.target;
                [l, c, h, point.range.clamp(0.0, 100.0) / 100.0]
            }),
        }),
        monochrome: mixer.monochrome.then(|| mixer.grey.map(|grey| grey / 100.0 * numa_core::mixer::MAX_GREY_STOPS)),
        grading: (!grading.is_identity()).then(|| (grading.tints(), grading.shape())),
    }
}

fn masks_stage(document: &Document, (width, height): (usize, usize), from: numa_core::color::WhiteBalance, detail_scale: f32) -> Vec<MaskPlan> {
    let map = document.masks_map.unwrap_or([1.0, 0.0, 0.0, 0.0, 1.0, 0.0]);
    let masks = std::sync::Arc::new(document.masks());
    let mut plans = Vec::new();
    for (index, mask) in masks.iter().enumerate() {
        if mask.is_idle() || mask.covers_nothing() {
            continue;
        }
        let basic = &mask.basic;
        let (detail, presence) = (&basic.detail, &basic.presence);

        let key = {
            use std::hash::{Hash, Hasher};
            let mut hasher = std::collections::hash_map::DefaultHasher::new();
            (width, height, map.map(f32::to_bits), mask.field_key()).hash(&mut hasher);
            for id in &mask.minus_masks {
                masks.iter().filter(|other| other.id == *id).for_each(|other| other.field_key().hash(&mut hasher));
            }
            hasher.finish()
        };
        let gains = (basic.balance.temperature != 0.0 || basic.balance.tint != 0.0).then(|| {
            let to = numa_core::color::WhiteBalance { temperature: from.temperature + basic.balance.temperature, tint: from.tint + basic.balance.tint };
            numa_core::color::relative_gains(from, to, ColourSpace::Srgb.from_xyz())
        });
        let colour = (detail.denoise_colour / 100.0).clamp(0.0, 1.0);
        let curved = !mask.curve.is_identity() || mask.channel_curves.iter().any(|curve| !curve.is_identity());
        let [red, green, blue] = &mask.channel_curves;
        plans.push(MaskPlan {
            key,
            of: (masks.clone(), index, (width, height), map),
            gains,
            dehaze: effects::HazeShape::new(width, height, basic.effects.dehaze),
            denoise_luma: detail::LumaShape::new(detail.denoise_luma / 100.0, detail.denoise_detail / 100.0, detail.denoise_contrast / 100.0, detail_scale),
            denoise_colour: (colour > 0.0 && 4.0 * detail_scale >= 0.5).then(|| (colour, (4.0 * detail_scale).round().max(1.0) as u32)),
            tail: Tail::of(detail, (width, height), detail_scale),
            adjustments: Adjustments { basic: Some(basic_stage(basic)), ..adjustments(&mask.mixer, &mask.point_colours, &mask.grading) },
            local: (presence.hdr != 0.0 || presence.clarity != 0.0 || presence.texture != 0.0)
                .then(|| local::ToneShape::new(width, height, [presence.hdr / 100.0, presence.clarity / 100.0, presence.texture / 100.0])),
            curves: curved.then(|| [&mask.curve, red, green, blue].map(|curve| (!curve.is_identity()).then(|| curve.lookup()))),
            tint: (!mask.colour.is_identity()).then(|| tint_of(mask.colour, ColourSpace::Srgb.luminance_weights())),
            grain: (basic.effects.grain > 0.0).then(|| effects::grain_shape([basic.effects.grain, basic.effects.grain_size, basic.effects.grain_roughness])).flatten(),
        });
    }
    plans
}

fn camera_stage(document: &Document, source: &LinearImage, inputs: &RenderInputs, profile: &numa_core::color::CameraProfile) -> Camera {
    let balance = document.white_balance;
    let effective = balance.unwrap_or_else(|| profile.as_shot_white_balance());
    let multipliers = profile.multipliers(effective);
    let rendering = profile_for(document, source, inputs)
        .as_ref()
        .and_then(|dng| Rendering::resolve(dng, effective.temperature, source.clip.unwrap_or(1.0)));
    let stage = match rendering {
        Some(rendering) => {
            let white = |table: &Table| match table.raw().2 || rendering.sensor_white <= 0.0 {
                true => 1.0,
                false => rendering.sensor_white,
            };
            CameraProfileStage::Rendering {
                to_prophoto: rendering.to_prophoto,
                map: rendering.hue_sat_map.as_ref().map(|table| table_plan(table, white(table))),
                look: rendering.look_table.as_ref().map(|table| table_plan(table, white(table))),
                to_srgb: rendering.to_srgb,
            }
        }
        None => CameraProfileStage::Matrix(profile.transform(balance)),
    };
    Camera { multipliers, lowest: multipliers[0].min(multipliers[1]).min(multipliers[2]), clip: source.clip, profile: stage, white_point: effective }
}

fn geometry_stage(document: &Document, width: u32, height: u32) -> (Option<GeometryPlan>, (u32, u32)) {
    let mirror = document.mirrored();
    let turn = match document.rotation() as i32 {
        90 => (true, false, true),
        180 => (false, true, true),
        270 => (true, true, false),
        _ => (false, false, false),
    };
    let turned = if turn.0 { (height, width) } else { (width, height) };
    let crop = document.crop().map(|(rect, angle)| {
        let [x, y, w, h] = rect;
        let (source_width, source_height) = (turned.0 as f32, turned.1 as f32);
        let size = ((w * source_width).round().max(1.0), (h * source_height).round().max(1.0));
        let (sin, cos) = angle.to_radians().sin_cos();
        let perspective = document.perspective();
        CropPlan {
            size,
            centre: ((x + w / 2.0) * source_width, (y + h / 2.0) * source_height),
            half: (source_width / 2.0, source_height / 2.0),
            sin,
            cos,
            keystone: perspective.coefficients(),
            stretch: perspective.stretch(),
        }
    });
    if !mirror && turn == (false, false, false) && crop.is_none() {
        return (None, (width, height));
    }
    let out = crop.as_ref().map_or(turned, |crop| (crop.size.0 as u32, crop.size.1 as u32));
    (Some(GeometryPlan { mirror, turn, crop }), out)
}

fn table_plan(table: &Table, white: f32) -> TablePlan {
    let (divisions, srgb, _, entries) = table.raw();
    TablePlan { divisions: divisions.map(|d| d as u32), srgb, white, entries: entries.to_vec() }
}

fn basic_stage(basic: &Basic) -> BasicPlan {
    let contrast = basic.tone.contrast / 100.0;
    let slope = if contrast >= 0.0 { 1.0 + contrast } else { 1.0 / (1.0 - contrast) };
    let tone = &basic.tone;
    let toned = tone.shadows != 0.0 || tone.highlights != 0.0 || tone.blacks != 0.0 || tone.whites != 0.0;
    BasicPlan {
        gain: 2.0f32.powf(basic.tone.exposure),
        slope: ((slope - 1.0).abs() > f32::EPSILON).then_some(slope),
        tone: toned.then(|| ToneCurve::new(basic).stops),
        saturation: basic.presence.saturation,
        vibrance: basic.presence.vibrance,
    }
}
