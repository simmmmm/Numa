use super::*;

#[derive(Clone)]
pub(super) struct Sliders {
    pub(super) balance: BalanceSliders,
    pub(super) tone: ToneSliders,
    pub(super) presence: PresenceSliders,
    pub(super) detail: DetailSliders,
    pub(super) effects: EffectsSliders,
    pub(super) calibration: CalibrationSliders,
    pub(super) optics: OpticsSliders,
}

#[derive(Clone)]
pub(super) struct BalanceSliders {
    pub(super) temperature: gtk::Scale,
    pub(super) tint: gtk::Scale,
}

#[derive(Clone)]
pub(super) struct ToneSliders {
    pub(super) exposure: gtk::Scale,
    pub(super) contrast: gtk::Scale,
    pub(super) highlights: gtk::Scale,
    pub(super) shadows: gtk::Scale,
    pub(super) whites: gtk::Scale,
    pub(super) blacks: gtk::Scale,
}

#[derive(Clone)]
pub(super) struct PresenceSliders {
    pub(super) vibrance: gtk::Scale,
    pub(super) saturation: gtk::Scale,
    pub(super) hdr: gtk::Scale,
    pub(super) clarity: gtk::Scale,
    pub(super) texture: gtk::Scale,
}

#[derive(Clone)]
pub(super) struct DetailSliders {
    pub(super) sharpen: gtk::Scale,
    pub(super) sharpen_radius: gtk::Scale,
    pub(super) sharpen_masking: gtk::Scale,
    pub(super) denoise_luma: gtk::Scale,

    pub(super) denoise_detail: gtk::Scale,
    pub(super) denoise_contrast: gtk::Scale,
    pub(super) denoise_colour: gtk::Scale,

    pub(super) moire: gtk::Scale,

    pub(super) defringe: gtk::Scale,
}

#[derive(Clone)]
pub(super) struct EffectsSliders {

    pub(super) dehaze: gtk::Scale,
    pub(super) vignette: gtk::Scale,
    pub(super) vignette_midpoint: gtk::Scale,
    pub(super) vignette_roundness: gtk::Scale,
    pub(super) vignette_feather: gtk::Scale,
    pub(super) grain: gtk::Scale,
    pub(super) grain_size: gtk::Scale,
    pub(super) grain_roughness: gtk::Scale,
}

#[derive(Clone)]
pub(super) struct CalibrationSliders {
    pub(super) shadow_tint: gtk::Scale,
    pub(super) red_hue: gtk::Scale,
    pub(super) red_saturation: gtk::Scale,
    pub(super) green_hue: gtk::Scale,
    pub(super) green_saturation: gtk::Scale,
    pub(super) blue_hue: gtk::Scale,
    pub(super) blue_saturation: gtk::Scale,
}

#[derive(Clone)]
pub(super) struct OpticsSliders {

    pub(super) lens_distortion: gtk::Scale,
    pub(super) lens_vignetting: gtk::Scale,
}

impl Sliders {
    pub(super) fn new() -> Self {
        let amount = || gtk::Scale::with_range(gtk::Orientation::Horizontal, -100.0, 100.0, 1.0);
        let positive = || gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.0, 100.0, 1.0);
        Self {
            balance: BalanceSliders {

                temperature: gtk::Scale::with_range(gtk::Orientation::Horizontal, 2000.0, 15000.0, 10.0),
                tint: gtk::Scale::with_range(
                    gtk::Orientation::Horizontal,
                    -color::MAX_TINT as f64,
                    color::MAX_TINT as f64,
                    1.0,
                ),
            },
            tone: ToneSliders {

                exposure: gtk::Scale::with_range(gtk::Orientation::Horizontal, -5.0, 5.0, 0.05),
                contrast: amount(),
                highlights: amount(),
                shadows: amount(),
                whites: amount(),
                blacks: amount(),
            },
            presence: PresenceSliders {
                vibrance: amount(),
                saturation: amount(),
                hdr: amount(),
                clarity: amount(),
                texture: amount(),
            },
            detail: DetailSliders {

                sharpen: positive(),
                sharpen_radius: gtk::Scale::with_range(gtk::Orientation::Horizontal, 0.5, 3.0, 0.1),
                sharpen_masking: positive(),
                denoise_luma: positive(),
                denoise_detail: positive(),
                denoise_contrast: positive(),
                denoise_colour: positive(),
                moire: positive(),
                defringe: positive(),
            },
            effects: EffectsSliders {
                dehaze: amount(),
                vignette: amount(),
                vignette_midpoint: positive(),
                vignette_roundness: amount(),
                vignette_feather: positive(),
                grain: positive(),
                grain_size: positive(),
                grain_roughness: positive(),
            },
            calibration: CalibrationSliders {
                shadow_tint: amount(),
                red_hue: amount(),
                red_saturation: amount(),
                green_hue: amount(),
                green_saturation: amount(),
                blue_hue: amount(),
                blue_saturation: amount(),
            },
            optics: OpticsSliders { lens_distortion: amount(), lens_vignetting: amount() },
        }
    }

    pub(super) fn white_balance(&self) -> WhiteBalance {
        WhiteBalance {
            temperature: self.balance.temperature.value() as f32,

            tint: -(self.balance.tint.value() as f32),
        }
    }

    pub(super) fn write_white_balance(&self, balance: WhiteBalance) {
        self.balance.temperature.set_value(balance.temperature as f64);
        self.balance.tint.set_value(-balance.tint as f64);
    }

    pub(super) fn white_balance_at_rest(&self, balance: WhiteBalance) {
        set_neutral(&self.balance.temperature, balance.temperature as f64);
        set_neutral(&self.balance.tint, -balance.tint as f64);
    }

    pub(super) fn read(&self) -> Basic {
        let (tone, presence, detail) = (&self.tone, &self.presence, &self.detail);
        let (effects, calibration, optics) = (&self.effects, &self.calibration, &self.optics);
        let at = |scale: &gtk::Scale| scale.value() as f32;
        Basic {
            tone: Tone {
                exposure: at(&tone.exposure),
                contrast: at(&tone.contrast),
                highlights: at(&tone.highlights),
                shadows: at(&tone.shadows),
                whites: at(&tone.whites),
                blacks: at(&tone.blacks),
            },
            presence: Presence {
                vibrance: at(&presence.vibrance),
                saturation: at(&presence.saturation),
                hdr: at(&presence.hdr),
                clarity: at(&presence.clarity),
                texture: at(&presence.texture),
            },
            detail: Detail {
                sharpen: at(&detail.sharpen),
                sharpen_radius: at(&detail.sharpen_radius),
                sharpen_masking: at(&detail.sharpen_masking),
                denoise_luma: at(&detail.denoise_luma),
                denoise_detail: at(&detail.denoise_detail),
                denoise_contrast: at(&detail.denoise_contrast),
                denoise_colour: at(&detail.denoise_colour),
                moire: at(&detail.moire),
                defringe: at(&detail.defringe),
            },
            effects: Effects {
                dehaze: at(&effects.dehaze),
                vignette: at(&effects.vignette),
                vignette_midpoint: at(&effects.vignette_midpoint),
                vignette_roundness: at(&effects.vignette_roundness),
                vignette_feather: at(&effects.vignette_feather),
                grain: at(&effects.grain),
                grain_size: at(&effects.grain_size),
                grain_roughness: at(&effects.grain_roughness),
            },
            calibration: Calibration {
                shadow_tint: at(&calibration.shadow_tint),
                red_hue: at(&calibration.red_hue),
                red_saturation: at(&calibration.red_saturation),
                green_hue: at(&calibration.green_hue),
                green_saturation: at(&calibration.green_saturation),
                blue_hue: at(&calibration.blue_hue),
                blue_saturation: at(&calibration.blue_saturation),
            },
            optics: Optics {
                lens_distortion: at(&optics.lens_distortion),
                lens_vignetting: at(&optics.lens_vignetting),
            },

            balance: Balance { temperature: 0.0, tint: 0.0 },
        }
    }

    pub(super) fn at_rest(&self, as_shot: WhiteBalance) -> [bool; SLIDER_COUNT] {
        at_rest_of(self.read(), self.white_balance(), as_shot)
    }

    pub(super) fn write(&self, basic: Basic) {
        const _: () = assert!(
            SLIDER_COUNT == 39,
            "a slider was added to `each`; add it to `write` and `at_rest_of` too"
        );
        let put = |scale: &gtk::Scale, value: f32| scale.set_value(value as f64);

        let (tone, set) = (&self.tone, basic.tone);
        put(&tone.exposure, set.exposure);
        put(&tone.contrast, set.contrast);
        put(&tone.highlights, set.highlights);
        put(&tone.shadows, set.shadows);
        put(&tone.whites, set.whites);
        put(&tone.blacks, set.blacks);

        let (presence, set) = (&self.presence, basic.presence);
        put(&presence.vibrance, set.vibrance);
        put(&presence.saturation, set.saturation);
        put(&presence.hdr, set.hdr);
        put(&presence.clarity, set.clarity);

        put(&presence.texture, set.texture);

        let (detail, set) = (&self.detail, basic.detail);
        put(&detail.sharpen, set.sharpen);
        put(&detail.sharpen_radius, set.sharpen_radius);
        put(&detail.sharpen_masking, set.sharpen_masking);
        put(&detail.denoise_luma, set.denoise_luma);
        put(&detail.denoise_detail, set.denoise_detail);
        put(&detail.denoise_contrast, set.denoise_contrast);
        put(&detail.denoise_colour, set.denoise_colour);
        put(&detail.defringe, set.defringe);
        put(&detail.moire, set.moire);

        let (effects, set) = (&self.effects, basic.effects);
        put(&effects.dehaze, set.dehaze);
        put(&effects.vignette, set.vignette);
        put(&effects.vignette_midpoint, set.vignette_midpoint);
        put(&effects.vignette_roundness, set.vignette_roundness);
        put(&effects.vignette_feather, set.vignette_feather);
        put(&effects.grain, set.grain);
        put(&effects.grain_size, set.grain_size);
        put(&effects.grain_roughness, set.grain_roughness);

        let (calibration, set) = (&self.calibration, basic.calibration);
        put(&calibration.shadow_tint, set.shadow_tint);
        put(&calibration.red_hue, set.red_hue);
        put(&calibration.red_saturation, set.red_saturation);
        put(&calibration.green_hue, set.green_hue);
        put(&calibration.green_saturation, set.green_saturation);
        put(&calibration.blue_hue, set.blue_hue);
        put(&calibration.blue_saturation, set.blue_saturation);

        put(&self.optics.lens_distortion, basic.optics.lens_distortion);
        put(&self.optics.lens_vignetting, basic.optics.lens_vignetting);
    }

    pub(super) fn each(&self) -> [(&'static str, &gtk::Scale, Readout); SLIDER_COUNT] {
        let (balance, tone, presence, detail) = (&self.balance, &self.tone, &self.presence, &self.detail);
        let (effects, calibration, optics) = (&self.effects, &self.calibration, &self.optics);
        [
            ("Temperature", &balance.temperature, Readout::Kelvin),
            ("Tint", &balance.tint, Readout::Signed(0)),
            ("Exposure", &tone.exposure, Readout::Signed(2)),
            ("Contrast", &tone.contrast, Readout::Signed(0)),
            ("Highlights", &tone.highlights, Readout::Signed(0)),
            ("Shadows", &tone.shadows, Readout::Signed(0)),
            ("Whites", &tone.whites, Readout::Signed(0)),
            ("Blacks", &tone.blacks, Readout::Signed(0)),
            ("HDR", &presence.hdr, Readout::Signed(0)),
            ("Vibrance", &presence.vibrance, Readout::Signed(0)),
            ("Saturation", &presence.saturation, Readout::Signed(0)),
            ("Clarity", &presence.clarity, Readout::Signed(0)),
            ("Texture", &presence.texture, Readout::Signed(0)),
            ("Sharpening", &detail.sharpen, Readout::Positive(0)),
            ("Radius", &detail.sharpen_radius, Readout::Radius),
            ("Masking", &detail.sharpen_masking, Readout::Positive(0)),
            ("Noise reduction", &detail.denoise_luma, Readout::Positive(0)),
            ("Detail", &detail.denoise_detail, Readout::Middle),
            ("Contrast", &detail.denoise_contrast, Readout::Positive(0)),
            ("Colour noise", &detail.denoise_colour, Readout::Positive(0)),
            ("Defringe", &detail.defringe, Readout::Positive(0)),
            ("Moiré", &detail.moire, Readout::Positive(0)),
            ("Dehaze", &effects.dehaze, Readout::Signed(0)),
            ("Amount", &effects.vignette, Readout::Signed(0)),
            ("Midpoint", &effects.vignette_midpoint, Readout::Middle),
            ("Roundness", &effects.vignette_roundness, Readout::Signed(0)),
            ("Feather", &effects.vignette_feather, Readout::Middle),
            ("Amount", &effects.grain, Readout::Positive(0)),
            ("Size", &effects.grain_size, Readout::Positive(0)),
            ("Roughness", &effects.grain_roughness, Readout::Middle),
            ("Shadows tint", &calibration.shadow_tint, Readout::Signed(0)),
            ("Red hue", &calibration.red_hue, Readout::Signed(0)),
            ("Red saturation", &calibration.red_saturation, Readout::Signed(0)),
            ("Green hue", &calibration.green_hue, Readout::Signed(0)),
            ("Green saturation", &calibration.green_saturation, Readout::Signed(0)),
            ("Blue hue", &calibration.blue_hue, Readout::Signed(0)),
            ("Blue saturation", &calibration.blue_saturation, Readout::Signed(0)),
            ("Distortion", &optics.lens_distortion, Readout::Signed(0)),
            ("Vignetting", &optics.lens_vignetting, Readout::Signed(0)),
        ]
    }
}

#[derive(Clone, Copy, PartialEq)]
pub(super) enum Readout {

    Signed(usize),

    Kelvin,

    OffsetKelvin,

    Positive(usize),

    Radius,

    Degrees,

    Middle,

    BrushSize,
}

impl Readout {
    pub(super) fn format(self, value: f64) -> String {
        match self {

            Readout::Kelvin => {
                let kelvin = format!("{value:.0}");
                let (head, tail) = kelvin.split_at(kelvin.len().saturating_sub(3));
                match head.is_empty() {
                    true => format!("{tail} K"),
                    false => format!("{head}\u{2009}{tail} K"),
                }
            }
            Readout::OffsetKelvin if value == 0.0 => "0 K".to_string(),
            Readout::OffsetKelvin => {
                let sign = if value > 0.0 { '+' } else { '\u{2212}' };
                format!("{sign}{} K", Readout::Kelvin.format(value.abs()).trim_end_matches(" K"))
            }
            Readout::Signed(_) if value == 0.0 => "0".to_string(),
            Readout::Signed(decimals) => {

                let sign = if value > 0.0 { '+' } else { '\u{2212}' };
                format!("{sign}{:.*}", decimals, value.abs())
            }
            Readout::Positive(decimals) => format!("{:.*}", decimals, value),
            Readout::Radius => format!("{value:.1} px"),
            Readout::Middle => format!("{value:.0}"),
            Readout::Degrees => format!("{value:.0}\u{00b0}"),
            Readout::BrushSize => match brush_size(value) as f64 * 100.0 {
                small if small < 1.0 => format!("{small:.2} %"),
                middle if middle < 10.0 => format!("{middle:.1} %"),
                large => format!("{large:.0} %"),
            },
        }
    }
}
