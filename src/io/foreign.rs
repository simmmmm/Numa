use std::collections::HashMap;
use std::path::Path;

use crate::core::curve::Curve;
use crate::core::document::{Basic, Document, EditParts};
use crate::core::grading::{Grading, Range};
use crate::core::mixer::Mixer;
use crate::core::color::WhiteBalance;
use crate::io::presets::Preset;

#[derive(Debug)]
pub struct Translated {
    pub name: String,
    pub preset: Preset,

    pub ignored: Vec<&'static str>,
}

#[derive(Debug, Clone, PartialEq)]
enum Value {
    Number(f32),
    Text(String),
    List(Vec<f32>),
}

type Settings = HashMap<String, Value>;

pub fn is_foreign(path: &Path) -> bool {
    matches!(extension(path).as_str(), "lrtemplate" | "xmp" | "costyle")
}

fn extension(path: &Path) -> String {
    path.extension().map(|ext| ext.to_string_lossy().to_ascii_lowercase()).unwrap_or_default()
}

pub fn translate(text: &str, kind: &str, name: &str) -> Result<Translated, String> {
    let (settings, title, local) = match kind {
        "lrtemplate" => read_lua(text)?,
        "xmp" => read_xmp(text)?,
        "costyle" => (read_costyle(text), None, false),
        _ => return Err(format!("not a preset Numa can read: .{kind}")),
    };
    let title = title.filter(|title| !title.trim().is_empty()).unwrap_or_else(|| name.to_string());
    if settings.is_empty() {
        return Err(format!("“{title}” has no settings in it"));
    }
    let mut translated = match kind {
        "costyle" => from_capture_one(&settings),
        _ => from_lightroom(&settings),
    };
    if local {
        translated.ignored.push("local adjustments");
    }
    translated.name = title.trim().to_string();
    if !translated.preset.parts.any() {
        return Err(format!("nothing in “{}” is something Numa has", translated.name));
    }
    Ok(translated)
}

pub fn translate_file(path: &Path) -> Result<Translated, String> {
    let bytes = std::fs::read(path).map_err(|err| format!("{}: {err}", path.display()))?;
    let text = String::from_utf8_lossy(&bytes);
    let name = path.file_stem().map(|stem| stem.to_string_lossy().into_owned()).unwrap_or_default();
    translate(&text, &extension(path), &name).map_err(|err| format!("{}: {err}", path.display()))
}

fn read_lua(text: &str) -> Result<(Settings, Option<String>, bool), String> {
    let tokens = lua_tokens(text);
    let mut settings = Settings::new();
    let mut title = None;
    let mut local = false;

    let mut path: Vec<Option<String>> = Vec::new();
    let mut index = 0;
    while index < tokens.len() {
        match &tokens[index] {
            Token::Open => path.push(None),
            Token::Close => {
                path.pop();
            }
            Token::Word(key) if tokens.get(index + 1) == Some(&Token::Equals) => {
                let in_settings = path.last().is_some_and(|open| open.as_deref() == Some("settings"));
                match tokens.get(index + 2) {
                    Some(Token::Open) => {
                        if in_settings {
                            if key.ends_with("Corrections") && lua_table_has_content(&tokens, index + 2) {
                                local = true;
                            }
                            if let Some(list) = lua_number_list(&tokens, index + 2) {
                                settings.insert(key.clone(), Value::List(list));
                            }
                        }
                        path.push(Some(key.clone()));
                        index += 3;
                        continue;
                    }
                    Some(value) => {
                        if key == "title" && path.len() == 1 {
                            if let Token::Text(text) = value {
                                title = Some(text.clone());
                            }
                        }
                        if in_settings {
                            match value {
                                Token::Number(number) => {
                                    settings.insert(key.clone(), Value::Number(*number));
                                }
                                Token::Text(text) | Token::Word(text) => {
                                    settings.insert(key.clone(), Value::Text(text.clone()));
                                }
                                _ => {}
                            }
                        }
                        index += 3;
                        continue;
                    }
                    None => {}
                }
            }
            _ => {}
        }
        index += 1;
    }
    if path.len() > 1 {
        return Err("the preset ends before its tables do".to_string());
    }
    Ok((settings, title, local))
}

#[derive(Debug, Clone, PartialEq)]
enum Token {
    Open,
    Close,
    Equals,
    Word(String),
    Text(String),
    Number(f32),
}

fn lua_tokens(text: &str) -> Vec<Token> {
    let chars: Vec<char> = text.chars().collect();
    let mut tokens = Vec::new();
    let mut at = 0;
    while at < chars.len() {
        let c = chars[at];
        match c {
            '{' => tokens.push(Token::Open),
            '}' => tokens.push(Token::Close),
            '=' => tokens.push(Token::Equals),
            '"' => {
                let mut value = String::new();
                at += 1;
                while at < chars.len() && chars[at] != '"' {
                    if chars[at] == '\\' && at + 1 < chars.len() {
                        at += 1;
                    }
                    value.push(chars[at]);
                    at += 1;
                }
                tokens.push(Token::Text(value));
            }
            c if c == '-' || c == '+' || c == '.' || c.is_ascii_digit() => {
                let start = at;
                while at + 1 < chars.len() && matches!(chars[at + 1], '0'..='9' | '.' | 'e' | 'E' | '-' | '+') {
                    at += 1;
                }
                let number: String = chars[start..=at].iter().collect();
                if let Ok(number) = number.parse::<f32>() {
                    tokens.push(Token::Number(number));
                }
            }
            c if c.is_alphabetic() || c == '_' => {
                let start = at;
                while at + 1 < chars.len() && (chars[at + 1].is_alphanumeric() || chars[at + 1] == '_') {
                    at += 1;
                }
                tokens.push(Token::Word(chars[start..=at].iter().collect()));
            }
            _ => {}
        }
        at += 1;
    }
    tokens
}

fn lua_number_list(tokens: &[Token], open: usize) -> Option<Vec<f32>> {
    let mut list = Vec::new();
    for token in &tokens[open + 1..] {
        match token {
            Token::Number(number) => list.push(*number),
            Token::Close => return Some(list),
            _ => return None,
        }
    }
    None
}

fn lua_table_has_content(tokens: &[Token], open: usize) -> bool {
    !matches!(tokens.get(open + 1), Some(Token::Close))
}

fn read_xmp(text: &str) -> Result<(Settings, Option<String>, bool), String> {
    if !text.contains("camera-raw-settings") {
        return Err("not a Camera Raw preset".to_string());
    }
    let mut settings = Settings::new();
    let mut rest = text;
    while let Some(at) = rest.find("crs:") {
        rest = &rest[at + 4..];
        let key: String = rest.chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();

        let after = rest[key.len()..].trim_start();
        if let Some(value) = after.strip_prefix('=').map(str::trim_start).and_then(|v| v.strip_prefix('"')) {
            let value = &value[..value.find('"').unwrap_or(0)];
            settings.insert(key, parse_value(value));
        }
    }

    let mut name = None;
    let mut local = false;
    let mut rest = text;
    while let Some(at) = rest.find("<crs:") {
        rest = &rest[at + 5..];
        let key: String = rest.chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
        let close = format!("</crs:{key}>");
        let Some(end) = rest.find(&close) else { continue };
        let body = &rest[..end];
        if key.ends_with("Corrections") && body.contains("<rdf:li") {
            local = true;
        }
        let items: Vec<&str> = body
            .split("<rdf:li")
            .skip(1)
            .filter_map(|item| Some(item.split_once('>')?.1.split('<').next()?.trim()))
            .collect();
        if key == "Name" {
            name = items.first().map(|item| item.to_string());
        } else if items.is_empty() {

            let body = body.split_once('>').map_or("", |(_, value)| value).trim();
            if !body.contains('<') && !body.is_empty() {
                settings.entry(key).or_insert_with(|| parse_value(body));
            }
        } else if key.starts_with("ToneCurve") {
            let numbers: Vec<f32> =
                items.iter().flat_map(|item| item.split(',')).filter_map(|n| n.trim().parse().ok()).collect();
            settings.insert(key, Value::List(numbers));
        }
    }
    Ok((settings, name, local))
}

fn read_costyle(text: &str) -> Settings {
    let mut settings = Settings::new();
    for element in text.split("<E ").skip(1) {
        let field = |name: &str| -> Option<&str> {
            let start = element.find(&format!("{name}=\""))? + name.len() + 2;
            let end = element[start..].find('"')?;
            Some(&element[start..start + end])
        };
        if let (Some(key), Some(value)) = (field("K"), field("V")) {
            settings.insert(key.to_string(), parse_value(value));
        }
    }
    settings
}

fn parse_value(value: &str) -> Value {
    match value.trim().trim_start_matches('+').parse::<f32>() {
        Ok(number) => Value::Number(number),
        Err(_) => Value::Text(value.to_string()),
    }
}

struct Builder<'a> {
    settings: &'a Settings,
    document: Document,
    basic: Basic,
    parts: EditParts,
    ignored: Vec<&'static str>,
}

impl<'a> Builder<'a> {
    fn new(settings: &'a Settings) -> Self {
        Self {
            settings,
            document: Document::new(String::new()),
            basic: Basic::default(),
            parts: EditParts::nothing(),
            ignored: Vec::new(),
        }
    }

    fn number(&self, key: &str) -> Option<f32> {
        match self.settings.get(key)? {
            Value::Number(number) => Some(*number),
            _ => None,
        }
    }

    fn text(&self, key: &str) -> Option<&str> {
        match self.settings.get(key)? {
            Value::Text(text) => Some(text),
            _ => None,
        }
    }

    fn take(&mut self, key: &str, part: fn(&mut EditParts), set: impl FnOnce(&mut Basic, f32)) -> Option<f32> {
        let value = self.number(key)?;
        set(&mut self.basic, value);
        part(&mut self.parts);
        Some(value)
    }

    fn ignore_if(&mut self, what: &'static str, present: bool) {
        if present && !self.ignored.contains(&what) {
            self.ignored.push(what);
        }
    }

    fn nonzero(&self, keys: &[&str]) -> bool {
        keys.iter().any(|key| self.number(key).is_some_and(|value| value.abs() > 1e-3))
    }

    fn finish(mut self) -> Translated {
        self.document.set_basic(self.basic);

        let mut document = Document::new(String::new());
        document.copy_from(&self.document, self.parts);
        Translated { name: String::new(), preset: Preset { parts: self.parts, document }, ignored: self.ignored }
    }
}

fn tone(parts: &mut EditParts) {
    parts.tone = true;
}
fn colour(parts: &mut EditParts) {
    parts.colour = true;
}
fn detail(parts: &mut EditParts) {
    parts.detail = true;
}

const LIGHTROOM_BANDS: [&str; 8] = ["Red", "Orange", "Yellow", "Green", "Aqua", "Blue", "Purple", "Magenta"];

fn from_lightroom(settings: &Settings) -> Translated {
    let mut b = Builder::new(settings);

    for (key, set) in [
        ("Exposure2012", (|basic: &mut Basic, v: f32| basic.exposure = v) as fn(&mut Basic, f32)),
        ("Contrast2012", |basic, v| basic.contrast = v),
        ("Highlights2012", |basic, v| basic.highlights = v),
        ("Shadows2012", |basic, v| basic.shadows = v),
        ("Whites2012", |basic, v| basic.whites = v),
        ("Blacks2012", |basic, v| basic.blacks = v),
        ("Clarity2012", |basic, v| basic.clarity = v),
        ("Texture", |basic, v| basic.texture = v),
        ("Dehaze", |basic, v| basic.dehaze = v),
        ("PostCropVignetteAmount", |basic, v| basic.vignette = v),
        ("PostCropVignetteMidpoint", |basic, v| basic.vignette_midpoint = v),
        ("PostCropVignetteRoundness", |basic, v| basic.vignette_roundness = v),
        ("PostCropVignetteFeather", |basic, v| basic.vignette_feather = v),
        ("GrainAmount", |basic, v| basic.grain = v),
        ("GrainSize", |basic, v| basic.grain_size = v),
        ("GrainFrequency", |basic, v| basic.grain_roughness = v),
    ] {
        b.take(key, tone, set);
    }
    let old_process = b.number("Exposure").is_some() || b.number("Brightness").is_some_and(|v| v != 0.0);
    b.ignore_if("tone from before process 2012", old_process && b.number("Exposure2012").is_none());

    for (key, set) in [
        ("Vibrance", (|basic: &mut Basic, v: f32| basic.vibrance = v) as fn(&mut Basic, f32)),
        ("Saturation", |basic, v| basic.saturation = v),
        ("ShadowTint", |basic, v| basic.shadow_tint = v),
        ("RedHue", |basic, v| basic.red_hue = v),
        ("RedSaturation", |basic, v| basic.red_saturation = v),
        ("GreenHue", |basic, v| basic.green_hue = v),
        ("GreenSaturation", |basic, v| basic.green_saturation = v),
        ("BlueHue", |basic, v| basic.blue_hue = v),
        ("BlueSaturation", |basic, v| basic.blue_saturation = v),
    ] {
        b.take(key, colour, set);
    }
    if b.text("ConvertToGrayscale").is_some_and(|v| v.eq_ignore_ascii_case("true")) {
        b.basic.saturation = -100.0;
        b.parts.colour = true;
        b.ignore_if("black-and-white mix", true);
    }

    let mut mixer = Mixer::default();
    let mut mixed = false;
    for (band, name) in LIGHTROOM_BANDS.iter().enumerate() {
        for (channel, prefix) in ["HueAdjustment", "SaturationAdjustment", "LuminanceAdjustment"].iter().enumerate() {
            if let Some(value) = b.number(&format!("{prefix}{name}")) {
                mixer.bands[band][channel] = value;
                mixed = true;
            }
        }
    }
    if mixed {
        b.document.set_mixer(mixer);
        b.parts.colour = true;
    }

    let range = |b: &Builder, which: &str| Range {
        hue: b.number(&format!("ColorGrade{which}Hue")).unwrap_or(0.0),
        saturation: b.number(&format!("ColorGrade{which}Sat")).unwrap_or(0.0),
        luminance: b.number(&format!("ColorGrade{which}Lum")).unwrap_or(0.0),
    };
    let mut grading = Grading {
        shadows: range(&b, "Shadow"),
        midtones: range(&b, "Midtone"),
        highlights: range(&b, "Highlight"),
        global: range(&b, "Global"),
        blending: b.number("ColorGradeBlending").unwrap_or(50.0),
        balance: b.number("SplitToningBalance").unwrap_or(0.0),
    };
    let graded = b.nonzero(&["ColorGradeShadowSat", "ColorGradeMidtoneSat", "ColorGradeHighlightSat", "ColorGradeGlobalSat"]);
    if !graded && b.nonzero(&["SplitToningShadowSaturation", "SplitToningHighlightSaturation"]) {
        grading.shadows.hue = b.number("SplitToningShadowHue").unwrap_or(0.0);
        grading.shadows.saturation = b.number("SplitToningShadowSaturation").unwrap_or(0.0);
        grading.highlights.hue = b.number("SplitToningHighlightHue").unwrap_or(0.0);
        grading.highlights.saturation = b.number("SplitToningHighlightSaturation").unwrap_or(0.0);
    }
    if grading != Grading::default() {
        b.document.set_grading(grading);
        b.parts.colour = true;
    }

    if b.text("WhiteBalance").is_some_and(|v| v == "Custom") {
        if let Some(temperature) = b.number("Temperature") {
            b.document.white_balance =
                Some(WhiteBalance { temperature, tint: b.number("Tint").unwrap_or(0.0) });
            b.parts.white_balance = true;
        }
        b.ignore_if(
            "white balance for JPEGs",
            b.nonzero(&["IncrementalTemperature", "IncrementalTint"]),
        );
    }

    for (key, set) in [
        ("Sharpness", (|basic: &mut Basic, v: f32| basic.sharpen = v.min(100.0)) as fn(&mut Basic, f32)),
        ("SharpenRadius", |basic, v| basic.sharpen_radius = v.clamp(0.5, 3.0)),
        ("SharpenEdgeMasking", |basic, v| basic.sharpen_masking = v),
        ("LuminanceSmoothing", |basic, v| basic.denoise_luma = v),
        ("LuminanceNoiseReductionDetail", |basic, v| basic.denoise_detail = v),
        ("LuminanceNoiseReductionContrast", |basic, v| basic.denoise_contrast = v),
        ("ColorNoiseReduction", |basic, v| basic.denoise_colour = v),
    ] {
        b.take(key, detail, set);
    }

    let curve = |b: &Builder, key: &str| -> Option<Curve> {
        let Some(Value::List(numbers)) = b.settings.get(key) else { return None };
        let points: Vec<[f32; 2]> = numbers.chunks_exact(2).map(|p| [p[0] / 255.0, p[1] / 255.0]).collect();
        (points.len() >= 2).then(|| Curve::new(points))
    };
    let mut curves = [

        curve(&b, "ToneCurvePV2012").or_else(|| curve(&b, "ToneCurve")),
        curve(&b, "ToneCurvePV2012Red"),
        curve(&b, "ToneCurvePV2012Green"),
        curve(&b, "ToneCurvePV2012Blue"),
    ];

    let regions = ["ParametricShadows", "ParametricDarks", "ParametricLights", "ParametricHighlights"];
    if b.nonzero(&regions) {
        let amounts = regions.map(|key| b.number(key).unwrap_or(0.0) / 100.0);
        let splits = [
            b.number("ParametricShadowSplit").unwrap_or(25.0) / 100.0,
            b.number("ParametricMidtoneSplit").unwrap_or(50.0) / 100.0,
            b.number("ParametricHighlightSplit").unwrap_or(75.0) / 100.0,
        ];
        let base = curves[0].clone().unwrap_or_else(Curve::identity);
        curves[0] = Some(Curve::new((0..=16).map(|i| {
            let x = i as f32 / 16.0;
            [x, parametric(base.value_at(x), amounts, splits)]
        })));
    }
    if curves.iter().any(|c| c.as_ref().is_some_and(|c| !c.is_identity())) {
        b.document.set_curves(curves.map(|c| c.unwrap_or_else(Curve::identity)));
        b.parts.curve = true;
    }

    let profile = b.text("CameraProfile").is_some_and(|p| !matches!(p, "Adobe Standard" | "Embedded" | ""));
    b.ignore_if("camera profile", profile);
    b.ignore_if("lens vignetting", b.nonzero(&["VignetteAmount"]));
    b.ignore_if(
        "lens corrections",
        b.nonzero(&["LensManualDistortionAmount", "PerspectiveVertical", "PerspectiveHorizontal", "DefringePurpleAmount", "DefringeGreenAmount"]),
    );
    b.finish()
}

fn from_capture_one(settings: &Settings) -> Translated {
    let mut b = Builder::new(settings);

    b.take("Exposure", tone, |basic, v| basic.exposure = v);

    b.take("Contrast", tone, |basic, v| basic.contrast = (v * 2.0).clamp(-100.0, 100.0));
    b.take("HighlightRecovery", tone, |basic, v| basic.highlights = -v);
    b.take("ShadowRecovery", tone, |basic, v| basic.shadows = v);
    b.take("WhiteRecovery", tone, |basic, v| basic.whites = -v);
    b.take("BlackRecovery", tone, |basic, v| basic.blacks = v);
    b.take("Clarity", tone, |basic, v| basic.clarity = v);
    b.take("FilmGrainAmount", tone, |basic, v| basic.grain = v);
    b.take("FilmGrainGranularity", tone, |basic, v| basic.grain_size = v);

    b.take("Vignetting", tone, |basic, v| basic.vignette = (v * 50.0).clamp(-100.0, 100.0));
    b.take("Saturation", colour, |basic, v| basic.saturation = v);
    b.ignore_if("brightness", b.nonzero(&["Brightness"]));

    if b.number("BwEnabled").is_some_and(|v| v != 0.0) {
        b.basic.saturation = -100.0;
        b.parts.colour = true;
        b.ignore_if("black-and-white mix", true);
    }

    let balance = |b: &Builder, key: &str| -> Option<Range> {
        let Some(Value::Text(text)) = b.settings.get(key) else { return None };
        let gain: Vec<f32> = text.split(';').filter_map(|v| v.trim().parse().ok()).collect();
        let [r, g, bl] = gain[..] else { return None };
        range_from_gain([r, g, bl])
    };
    let grading = Grading {
        global: balance(&b, "ColorBalance").unwrap_or_default(),
        shadows: balance(&b, "ColorBalanceShadow").unwrap_or_default(),
        midtones: balance(&b, "ColorBalanceMidtone").unwrap_or_default(),
        highlights: balance(&b, "ColorBalanceHighlight").unwrap_or_default(),
        ..Grading::default()
    };
    if grading != Grading::default() {
        b.document.set_grading(grading);
        b.parts.colour = true;
    }

    let curve = |b: &Builder, key: &str| -> Option<Curve> {
        let Some(Value::Text(text)) = b.settings.get(key) else { return None };
        let points: Vec<[f32; 2]> = text
            .split(';')
            .filter_map(|pair| {
                let (x, y) = pair.split_once(',')?;
                Some([x.trim().parse().ok()?, y.trim().parse().ok()?])
            })
            .collect();
        (points.len() >= 2).then(|| Curve::new(points))
    };
    let curves = [
        curve(&b, "GradationCurve"),
        curve(&b, "GradationCurveRed"),
        curve(&b, "GradationCurveGreen"),
        curve(&b, "GradationCurveBlue"),
    ];
    if curves.iter().any(|c| c.as_ref().is_some_and(|c| !c.is_identity())) {
        b.document.set_curves(curves.map(|c| c.unwrap_or_else(Curve::identity)));
        b.parts.curve = true;
    }
    b.ignore_if("luma curve", curve(&b, "GradationCurveY").is_some_and(|c| !c.is_identity()));
    b.ignore_if("advanced colour editor", b.settings.contains_key("ColorCorrections"));
    b.finish()
}

fn parametric(x: f32, amounts: [f32; 4], splits: [f32; 3]) -> f32 {
    let edges = [0.0, splits[0], splits[1], splits[2], 1.0];
    let mut y = x;
    for region in 0..4 {
        let (low, high) = (edges[region], edges[region + 1]);
        let centre = (low + high) / 2.0;

        let reach_low = if region == 0 { 0.0 } else { (edges[region - 1] + low) / 2.0 };
        let reach_high = if region == 3 { 1.0 } else { (high + edges[region + 2]) / 2.0 };
        let weight = if x <= centre {
            ((x - reach_low) / (centre - reach_low).max(1e-4)).clamp(0.0, 1.0)
        } else {
            ((reach_high - x) / (reach_high - centre).max(1e-4)).clamp(0.0, 1.0)
        };
        let weight = weight * weight * (3.0 - 2.0 * weight);
        y += amounts[region] * 0.2 * weight;
    }
    y.clamp(0.0, 1.0)
}

fn range_from_gain(gain: [f32; 3]) -> Option<Range> {
    let luma = 0.2126 * gain[0] + 0.7152 * gain[1] + 0.0722 * gain[2];
    if luma <= 0.0 {
        return None;
    }
    let offset = gain.map(|g| g / luma - 1.0);
    let (max, min) = (gain[0].max(gain[1]).max(gain[2]), gain[0].min(gain[1]).min(gain[2]));
    if max - min < 1e-3 {
        return None;
    }
    let hue = hue_of(gain);
    let direction = {
        let rgb = hue_rgb(hue);
        let l = 0.2126 * rgb[0] + 0.7152 * rgb[1] + 0.0722 * rgb[2];
        rgb.map(|c| c - l)
    };
    let dot: f32 = (0..3).map(|i| direction[i] * offset[i]).sum();
    let norm: f32 = direction.iter().map(|d| d * d).sum();
    let amount = (dot / norm.max(1e-6) * 100.0).clamp(0.0, 100.0);
    Some(Range { hue, saturation: amount, luminance: 0.0 })
}

fn hue_of(rgb: [f32; 3]) -> f32 {
    let (max, min) = (rgb[0].max(rgb[1]).max(rgb[2]), rgb[0].min(rgb[1]).min(rgb[2]));
    let delta = max - min;
    let hue = if max == rgb[0] {
        60.0 * ((rgb[1] - rgb[2]) / delta)
    } else if max == rgb[1] {
        60.0 * ((rgb[2] - rgb[0]) / delta + 2.0)
    } else {
        60.0 * ((rgb[0] - rgb[1]) / delta + 4.0)
    };
    hue.rem_euclid(360.0)
}

fn hue_rgb(degrees: f32) -> [f32; 3] {
    let h = (degrees / 60.0).rem_euclid(6.0);
    let f = h - h.floor();
    match h as i32 {
        0 => [1.0, f, 0.0],
        1 => [1.0 - f, 1.0, 0.0],
        2 => [0.0, 1.0, f],
        3 => [0.0, 1.0 - f, 1.0],
        4 => [f, 0.0, 1.0],
        _ => [1.0, 0.0, 1.0 - f],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_lightroom_classic_template_translates() {
        let text = "s = {\r\n\tid = \"X\",\r\n\ttitle = \"Warm Film\",\r\n\tvalue = {\r\n\t\tsettings = {\r\n\
            Exposure2012 = 0.35, Contrast2012 = -20, Highlights2012 = -100, GrainAmount = 30,\r\n\
            HueAdjustmentAqua = 20, SaturationAdjustmentBlue = -15, RedHue = 10, ShadowTint = -5,\r\n\
            SplitToningHighlightHue = 45, SplitToningHighlightSaturation = 20,\r\n\
            ParametricShadows = 10, CameraProfile = \"Camera Neutral\", WhiteBalance = \"As Shot\",\r\n\
            ToneCurvePV2012 = { 0, 20, 128, 128, 255, 240, },\r\n\
            PaintBasedCorrections = { { What = \"Correction\", }, },\r\n\t\t},\r\n\t},\r\n\tversion = 0,\r\n}";
        let t = translate(text, "lrtemplate", "file").unwrap();
        assert_eq!(t.name, "Warm Film");
        let basic = t.preset.document.basic();
        assert_eq!((basic.exposure, basic.contrast, basic.highlights, basic.grain), (0.35, -20.0, -100.0, 30.0));
        assert_eq!((basic.red_hue, basic.shadow_tint), (10.0, -5.0));
        let mixer = t.preset.document.mixer();
        assert_eq!((mixer.bands[4][0], mixer.bands[5][1]), (20.0, -15.0));
        assert_eq!(t.preset.document.grading().highlights.saturation, 20.0);

        let curve = t.preset.document.curve();
        assert!(curve.value_at(0.1) > (20.0 + (128.0 - 20.0) * 0.1 / 0.502) / 255.0, "{}", curve.value_at(0.1));
        let parts = t.preset.parts;
        assert!(parts.tone && parts.colour && parts.curve && !parts.white_balance && !parts.detail);
        for what in ["camera profile", "local adjustments"] {
            assert!(t.ignored.contains(&what), "{what} in {:?}", t.ignored);
        }
    }

    #[test]
    fn a_lightroom_xmp_translates() {
        let text = r#"<x:xmpmeta><rdf:RDF><rdf:Description xmlns:crs="http://ns.adobe.com/camera-raw-settings/1.0/"
            crs:Vibrance="+10" crs:Sharpness = "80" crs:PostCropVignetteAmount="-25"
            crs:WhiteBalance="Custom" crs:Temperature="6200" crs:Tint="+8">
            <crs:Name><rdf:Alt><rdf:li xml:lang="x-default">Airy</rdf:li></rdf:Alt></crs:Name>
            <crs:ToneCurvePV2012><rdf:Seq><rdf:li>0, 10</rdf:li><rdf:li>255, 255</rdf:li></rdf:Seq></crs:ToneCurvePV2012>
            </rdf:Description></rdf:RDF></x:xmpmeta>"#;
        let t = translate(text, "xmp", "file").unwrap();
        assert_eq!(t.name, "Airy");
        let basic = t.preset.document.basic();
        assert_eq!((basic.vibrance, basic.sharpen, basic.vignette), (10.0, 80.0, -25.0));
        assert_eq!(t.preset.document.white_balance.map(|wb| wb.temperature), Some(6200.0));
        let parts = t.preset.parts;
        assert!(parts.white_balance && parts.detail && parts.curve && parts.colour && parts.tone);
    }

    #[test]
    fn a_capture_one_style_translates() {
        let text = r#"<?xml version="1.0"?><SL Engine="1000">
            <E K="ColorBalanceShadow" V="1;0.95;0.99" /><E K="FilmGrainAmount" V="40" />
            <E K="GradationCurve" V="0.05,0;0.5,0.55;1,1" /><E K="BwEnabled" V="1" />
            <E K="ColorCorrections" V="1,1,1" /><E K="Name" V="BW-01" /></SL>"#;
        let t = translate(text, "costyle", "BW-01").unwrap();
        let basic = t.preset.document.basic();
        assert_eq!((basic.grain, basic.saturation), (40.0, -100.0));
        let shadows = t.preset.document.grading().shadows;
        assert!(shadows.saturation > 0.0 && (shadows.hue > 300.0 || shadows.hue < 30.0), "{shadows:?}");
        assert!(t.ignored.contains(&"advanced colour editor"));
    }

    #[test]
    fn a_file_with_nothing_numa_has_is_refused() {
        assert!(translate("s = { title = \"x\", value = { settings = { CameraProfile = \"Foo\", }, }, }", "lrtemplate", "x").is_err());
        assert!(translate("<x:xmpmeta/>", "xmp", "x").is_err());
    }
}
