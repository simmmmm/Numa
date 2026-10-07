use numa_core::document::{AutoRecord, Document, Perspective};
use numa_core::image::levelled;

use super::corners::{self, Cut};
use super::evidence::{self, Refusal, Source, Verdict};
use super::scene::Scene;

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Level {

    Apply { rect: [f32; 4], angle: f32, by: Source, keeps: f32 },

    Offer { rect: [f32; 4], angle: f32, by: Source, why: Offer },
    Leave(Leave),
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Offer {
    Cuts(Cut),

    Tilted,

    Unsure,
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Leave {
    Refused(Refusal),

    Theirs(f32),

    Spots,
}

pub fn level(scene: &Scene, document: &Document) -> Level {
    if !document.retouch().is_identity() {
        return Level::Leave(Leave::Spots);
    }
    let (rect, current) = document.crop().unwrap_or(([0.0, 0.0, 1.0, 1.0], 0.0));
    let record = document.auto.unwrap_or_default();
    if !AutoRecord::may_set(current, record.angle) {
        return Level::Leave(Leave::Theirs(current));
    }
    let perspective = document.perspective();
    match evidence::decide(&scene.level, &scene.frame(perspective, current)) {
        Verdict::Refused(refusal) => Level::Leave(Leave::Refused(refusal)),

        Verdict::Offer { angle, by: Source::Verticals } if angle.abs() > 1.5 && camera_says_level(scene) => {
            Level::Leave(Leave::Refused(Refusal::Disagree))
        }
        Verdict::Offer { angle, by } => {
            let (width, height) = (scene.size[0] as f32, scene.size[1] as f32);
            let why = if by == Source::Sea { Offer::Tilted } else { Offer::Unsure };
            Level::Offer { rect: levelled(rect, current, angle, perspective, width, height), angle, by, why }
        }
        Verdict::Level { angle, by } => match corners::level(scene, rect, current, angle, perspective) {
            Ok(kept) => Level::Apply { rect: kept.rect, angle, by, keeps: kept.share },
            Err((cut, kept)) => Level::Offer { rect: kept.rect, angle, by, why: Offer::Cuts(cut) },
        },
    }
}

fn camera_says_level(scene: &Scene) -> bool {
    scene.camera.roll.is_some_and(|roll| (roll - 90.0 * (roll / 90.0).round()).abs() < 0.95)
}

pub fn apply(document: &mut Document, rect: [f32; 4], angle: f32) {
    document.set_crop(rect, angle);
    let mut record = document.auto.unwrap_or_default();
    record.angle = (angle != 0.0).then_some(angle);
    document.auto = Some(record);
}

pub fn upright(scene: &Scene, perspective: Perspective) -> Option<f32> {
    let vertical = scene.upright.filter(|lean| lean.sigma <= 0.5)?.vertical();
    (vertical.abs() >= 2.0 && (vertical - perspective.vertical).abs() >= 2.0).then_some(vertical)
}

impl Level {

    pub fn says(&self) -> String {
        match *self {
            Level::Apply { angle, by, keeps, .. } => {
                format!("{:.1}° by {} · keeps {:.0} %", angle.abs(), by.name(), keeps * 100.0)
            }
            Level::Offer { angle, why: Offer::Tilted, .. } => format!("The sea is tilted {:.0}° — level it?", angle.abs()),
            Level::Offer { angle, by, why: Offer::Unsure, .. } => format!("{:.1}° by {} alone — level it?", angle.abs(), by.name()),
            Level::Offer { angle, by, why: Offer::Cuts(cut), .. } => {
                let cost = match cut {
                    Cut::Face => "would cut a face at the edge".to_string(),
                    Cut::Subject => "would cut into someone at the edge".to_string(),
                    Cut::Horizon => "would put the horizon at the edge".to_string(),
                    Cut::TooMuch => "would crop more than 5 % away".to_string(),
                };
                format!("{:.1}° by {} {cost} — level it?", angle.abs(), by.name())
            }
            Level::Leave(Leave::Spots) => "No level: healed spots would move".into(),
            Level::Leave(Leave::Theirs(angle)) => format!("Straighten is yours ({angle:+.1}°) — left"),
            Level::Leave(Leave::Refused(refusal)) => match refusal {
                Refusal::NothingToGoBy => "No horizon or upright lines clear enough to level by".into(),
                Refusal::Disagree => "The lines disagree — left alone".into(),
                Refusal::Deliberate(angle) => format!("A tilt of {:.0}° looks deliberate — left alone", angle.abs()),
                Refusal::AlreadyLevel => "Already level".into(),
            },
        }
    }
}
