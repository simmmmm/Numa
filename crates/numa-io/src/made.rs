use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use numa_core::document::Document;

use crate::history::{Change, EditState, Fmt, SLIDERS};
use crate::presets::Notes;

const MANY: usize = 8;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AppliedLook {
    pub name: String,
    pub notes: Notes,
}

impl AppliedLook {

    pub fn line(&self) -> String {
        match &self.notes.by {
            Some(by) => format!("Look “{}” · by {by}", self.name),
            None => format!("Look “{}”", self.name),
        }
    }
}

#[derive(Debug, Clone)]
pub struct MadeStep {

    pub key: String,

    pub what: String,

    pub tab: &'static str,

    pub why: Option<String>,

    pub look: Option<usize>,
    pub state: EditState,
}

#[derive(Debug, Clone)]
pub struct Made {

    pub start: EditState,
    pub steps: Vec<MadeStep>,
    pub looks: Vec<AppliedLook>,
}

pub fn digest(state: &EditState) -> String {
    let mut document = Document::new(String::new());
    state.restore(&mut document);
    let json = serde_json::to_vec(&document).unwrap_or_default();
    let hash = json.iter().fold(0xcbf2_9ce4_8422_2325u64, |hash, byte| (hash ^ *byte as u64).wrapping_mul(0x0100_0000_01b3));
    format!("{hash:016x}")
}

pub fn how_it_was_made(states: &[EditState], applied: &HashMap<String, AppliedLook>) -> Made {
    let mut made = Made { start: states.first().cloned().unwrap_or_else(EditState::untouched), steps: Vec::new(), looks: Vec::new() };

    let mut open = false;
    for pair in states.windows(2) {
        let (before, after) = (&pair[0], &pair[1]);
        let mut parts = parts_of(before, after);
        if parts.is_empty() {
            continue;
        }
        let look = applied.get(&digest(after));
        if look.is_none() && parts.len() > MANY {
            parts = vec![Part { key: String::new(), what: format!("{} settings", parts.len()), tab: "", take: Take::All }];
        }
        if look.is_none() && parts.len() == 1 && open {
            if let Some(last) = made.steps.last_mut().filter(|last| last.key == parts[0].key) {
                last.what = parts.remove(0).what;
                last.state = after.clone();
                continue;
            }
        }
        open = look.is_none() && parts.len() == 1;
        let look_at = look.map(|look| {
            made.looks.push(look.clone());
            made.looks.len() - 1
        });
        let mut now = before.clone();
        for part in parts {
            part.take.apply(&mut now, after);
            let why = look.and_then(|look| look.notes.notes.get(&part.key)).cloned();
            made.steps.push(MadeStep { key: part.key, what: part.what, tab: part.tab, why, look: look_at, state: now.clone() });
        }

        if let Some(last) = made.steps.last_mut() {
            last.state = after.clone();
        }
    }
    made
}

struct Part {
    key: String,
    what: String,
    tab: &'static str,
    take: Take,
}

enum Take {
    Slider(usize),
    Field(fn(&mut EditState, &EditState)),
    All,
}

impl Take {
    fn apply(&self, onto: &mut EditState, from: &EditState) {
        match self {
            Take::Slider(at) => {
                let mut copy = from.basic;
                let value = *(SLIDERS[*at].at)(&mut copy);
                *(SLIDERS[*at].at)(&mut onto.basic) = value;
            }
            Take::Field(copy) => copy(onto, from),
            Take::All => *onto = from.clone(),
        }
    }
}

fn parts_of(before: &EditState, after: &EditState) -> Vec<Part> {

    let fields: [(&str, &str, fn(&mut EditState, &EditState)); 20] = [
        ("White balance", "Colour", |c, n| c.white_balance = n.white_balance),
        ("Crop", "Crop", |c, n| c.crop = n.crop),
        ("Rotation", "Crop", |c, n| c.rotation = n.rotation),
        ("Flip", "Crop", |c, n| c.mirrored = n.mirrored),
        ("Tone curve", "Light", |c, n| c.curves = n.curves.clone()),
        ("Colour mixer", "Colour", |c, n| c.mixer = n.mixer),
        ("Point colour", "Colour", |c, n| c.point_colours = n.point_colours.clone()),
        ("Colour grading", "Grade", |c, n| c.grading = n.grading),
        ("Retouched", "Retouch", |c, n| c.retouch = n.retouch.clone()),
        ("Removed a retouch", "Retouch", |c, n| c.retouch = n.retouch.clone()),
        ("Moved a retouch", "Retouch", |c, n| c.retouch = n.retouch.clone()),
        ("Film simulation", "Looks", |c, n| c.film_simulation = n.film_simulation.clone()),
        ("Camera profile", "Looks", |c, n| c.colour_profile = n.colour_profile.clone()),
        ("Tone mapping", "Looks", |c, n| c.tone_mapping = n.tone_mapping),
        ("Colour space", "Colour", |c, n| c.working_space = n.working_space),
        ("Perspective", "Crop", |c, n| c.perspective = n.perspective),
        ("Face", "Retouch", |c, n| c.beautify = n.beautify),
        ("AI denoise", "Detail", |c, n| c.ai_denoise = n.ai_denoise),
        ("AI sharpen", "Detail", |c, n| c.ai_sharpen = n.ai_sharpen),
        ("LUT", "Looks", |c, n| c.lut = n.lut.clone()),
    ];
    after
        .changes_from(before)
        .into_iter()
        .filter_map(|change| {
            let word = match &change {
                Change::Word(word) => *word,
                Change::Mask(_, label) => {
                    let key = label.to_string();
                    return Some(Part { key, what: change.to_string(), tab: "Masks", take: Take::Field(|c, n| c.masks = n.masks.clone()) });
                }
            };
            if let Some(at) = SLIDERS.iter().position(|slider| slider.name == word) {
                let slider = &SLIDERS[at];
                let what = format!("{word} {}", number(slider.fmt, slider.value(&after.basic)));
                return Some(Part { key: word.to_string(), what, tab: slider.tab, take: Take::Slider(at) });
            }
            let (_, tab, copy) = fields.iter().find(|(name, ..)| *name == word)?;
            let what = match detail(word, before, after) {
                Some(detail) => format!("{word} {detail}"),
                None => word.to_string(),
            };
            Some(Part { key: word.to_string(), what, tab, take: Take::Field(*copy) })
        })
        .collect()
}

fn detail(word: &str, before: &EditState, after: &EditState) -> Option<String> {
    let name = |text: &str| text.rsplit_once('.').map_or(text, |(stem, _)| stem).to_string();
    match word {
        "White balance" => match (before.white_balance, after.white_balance) {
            (_, None) => Some("as shot".into()),
            (Some(was), Some(now)) if was.temperature == now.temperature => Some(format!("tint {}", number(Fmt::Signed(0), now.tint))),
            (_, Some(now)) => Some(kelvin(now.temperature)),
        },
        "Film simulation" => Some(after.film_simulation.as_deref().map_or("off".into(), name)),
        "Camera profile" => Some(after.colour_profile.as_deref().map_or("as shot".into(), name)),
        "Tone mapping" => Some(match after.tone_mapping {
            numa_core::tone::ToneMapping::Camera => "camera".into(),
            numa_core::tone::ToneMapping::Agx => "AgX".into(),
        }),
        "LUT" => Some(after.lut.as_ref().map_or("off".into(), |lut| name(&lut.name))),
        "AI denoise" => Some(number(Fmt::Positive, after.ai_denoise)),
        "AI sharpen" => Some(number(Fmt::Positive, after.ai_sharpen)),
        _ => None,
    }
}

pub fn number(fmt: Fmt, value: f32) -> String {
    match fmt {
        Fmt::Signed(decimals) => {
            let digits = format!("{:.*}", decimals, value.abs());
            match digits.parse::<f32>() {
                Ok(0.0) | Err(_) => "0".to_string(),
                Ok(_) => format!("{}{digits}", if value > 0.0 { '+' } else { '\u{2212}' }),
            }
        }
        Fmt::Positive | Fmt::Middle => format!("{value:.0}"),
        Fmt::Radius => format!("{value:.1} px"),
    }
}

fn kelvin(value: f32) -> String {
    let digits = format!("{value:.0}");
    let (head, tail) = digits.split_at(digits.len().saturating_sub(3));
    match head.is_empty() {
        true => format!("{tail} K"),
        false => format!("{head}\u{2009}{tail} K"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use numa_core::color::WhiteBalance;
    use numa_core::document::Basic;

    fn state(change: impl FnOnce(&mut Document)) -> EditState {
        let mut document = Document::new(String::new());
        change(&mut document);
        EditState::of(&document)
    }

    fn basic(change: impl FnOnce(&mut Basic)) -> EditState {
        state(|d| d.set_basic(Basic::with(change)))
    }

    #[test]
    fn numbers_are_written_as_the_panel_writes_them() {
        assert_eq!(number(Fmt::Signed(2), 0.35), "+0.35");
        assert_eq!(number(Fmt::Signed(0), -41.0), "\u{2212}41");
        assert_eq!(number(Fmt::Signed(0), 0.2), "0", "rounds to nothing");
        assert_eq!(number(Fmt::Positive, 40.0), "40");
        assert_eq!(number(Fmt::Radius, 1.0), "1.0 px");
        assert_eq!(kelvin(5300.0), "5\u{2009}300 K");
        assert_eq!(kelvin(950.0), "950 K");
    }

    #[test]
    fn a_history_plays_as_steps() {
        let start = EditState::untouched();
        let a = basic(|b| b.tone.exposure = 0.1);
        let b = basic(|b| b.tone.exposure = 0.35);
        let c = state(|d| {
            d.set_basic(Basic::with(|b| {
                b.tone.exposure = 0.35;
                b.tone.highlights = -15.0;
                b.presence.vibrance = 15.0;
            }));
            d.white_balance = Some(WhiteBalance { temperature: 5300.0, tint: 0.0 });
        });
        let d = state(|d| {
            d.set_basic(Basic::with(|b| {
                b.tone.exposure = 0.35;
                b.tone.highlights = -15.0;
                b.presence.vibrance = 15.0;
                b.tone.contrast = 10.0;
            }));
            d.white_balance = Some(WhiteBalance { temperature: 5300.0, tint: 0.0 });
        });
        let mut notes = Notes::default();
        notes.by = Some("Numa".into());
        notes.notes.insert("Highlights".into(), "Keeps the sky".into());
        let look = AppliedLook { name: "Golden Hour".into(), notes };
        let applied = HashMap::from([(digest(&c), look.clone())]);

        let made = how_it_was_made(&[start.clone(), a, b.clone(), c.clone(), d.clone()], &applied);
        let words: Vec<_> = made.steps.iter().map(|step| step.what.as_str()).collect();
        assert_eq!(words, ["Exposure +0.35", "Highlights \u{2212}15", "Vibrance +15", "White balance 5\u{2009}300 K", "Contrast +10"]);
        assert_eq!(made.steps[0].state, b, "the two moves of Exposure are one step");
        assert_eq!(made.steps[1].why.as_deref(), Some("Keeps the sky"));
        assert_eq!(made.steps[2].why, None, "no note for it");
        assert_eq!(made.steps[1].look, Some(0));
        assert_eq!(made.steps[4].look, None);
        assert_eq!(made.looks[0].line(), "Look “Golden Hour” · by Numa");

        assert_eq!(made.steps[1].state.basic.tone.highlights, -15.0);
        assert_eq!(made.steps[1].state.basic.presence.vibrance, 0.0);
        assert_eq!(made.steps[3].state, c);
        assert_eq!(made.steps.last().unwrap().state, d);
        assert_eq!(made.start, start);
    }

    #[cfg(numa_looks)]
    #[test]
    fn every_look_says_why_of_every_setting_it_changes() {
        for (name, json) in crate::presets::LOOKS {
            let look: crate::presets::Preset = serde_json::from_str(json).unwrap();
            let notes: Notes = serde_json::from_str(json).unwrap();
            assert_eq!(notes.by.as_deref(), Some("Numa"), "{name}");
            let mut document = Document::new(String::new());
            document.copy_from(&look.document, look.parts);
            let after = EditState::of(&document);
            let applied = HashMap::from([(digest(&after), AppliedLook { name: name.to_string(), notes: notes.clone() })]);
            let made = how_it_was_made(&[EditState::untouched(), after], &applied);
            let said: Vec<&str> = made.steps.iter().filter(|step| step.why.is_none()).map(|step| step.key.as_str()).collect();
            assert!(said.is_empty(), "{name} has no note for {said:?}");
            let keys: Vec<&str> = made.steps.iter().map(|step| step.key.as_str()).collect();
            let loose: Vec<&String> = notes.notes.keys().filter(|key| !keys.contains(&key.as_str())).collect();
            assert!(loose.is_empty(), "{name} has notes for {loose:?}, which it does not change");
            assert!(made.steps.iter().all(|step| step.why.as_deref().is_some_and(|why| why.len() < 64 && !why.contains('!'))));
        }
    }

    #[test]
    fn a_reset_is_one_row_and_a_digest_is_stable() {
        let busy = basic(|b| {
            b.tone.exposure = 1.0;
            b.tone.contrast = 1.0;
            b.tone.highlights = 1.0;
            b.tone.shadows = 1.0;
            b.tone.whites = 1.0;
            b.tone.blacks = 1.0;
            b.presence.vibrance = 1.0;
            b.presence.saturation = 1.0;
            b.presence.clarity = 1.0;
        });
        let made = how_it_was_made(&[EditState::untouched(), busy.clone()], &HashMap::new());
        assert_eq!(made.steps.len(), 1);
        assert_eq!(made.steps[0].what, "9 settings");
        assert_eq!(made.steps[0].state, busy);
        assert_eq!(digest(&busy), digest(&busy.clone()));
        assert_ne!(digest(&busy), digest(&EditState::untouched()));
        assert_eq!(digest(&EditState::untouched()).len(), 16);
    }
}
