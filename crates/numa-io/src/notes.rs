use crate::catalog::Photo;

pub fn cull_note(photo: &Photo, scale: &numa_cull::Scale) -> String {
    let mut parts: Vec<String> = Vec::new();
    if let Some(suggested) = photo.suggested {
        parts.push(format!("~{suggested:.1}★"));
    }

    if let Some(note) = frame_of(photo).blank_note() {
        parts.push(note.to_string());
        return parts.join(" · ");
    }
    if photo.best_of_burst {
        parts.push("best of burst".to_string());
    }
    parts.extend(issues(photo, scale));
    parts.join(" · ")
}

pub fn issues(photo: &Photo, scale: &numa_cull::Scale) -> Vec<String> {
    let frame = frame_of(photo);
    if let Some(note) = frame.blank_note() {
        return vec![note.to_string()];
    }
    let mut parts: Vec<String> = Vec::new();

    if photo.eyes_closed == Some(true) {
        parts.push("eyes closed?".to_string());
    }

    match (photo.face_sharpness, photo.sharpness) {
        (Some(face), Some(_)) if face < scale.soft => parts.push("soft face".to_string()),

        _ if photo.sharpness.is_some() && scale.is_soft(&frame) && frame.is_slow() => {
            parts.push("soft · slow shutter".to_string())
        }
        _ if photo.sharpness.is_some() && scale.is_soft(&frame) => parts.push("soft".to_string()),
        _ => {}
    }
    if photo.blown.is_some() && frame.is_blown() {
        parts.push("blown".to_string());
    }
    parts
}

fn frame_of(photo: &Photo) -> numa_cull::Frame {
    numa_cull::Frame {
        sharpness: photo.sharpness.unwrap_or_default(),
        blown: photo.blown.unwrap_or_default(),

        brightness: photo.brightness.unwrap_or(0.5),
        contrast: photo.contrast.unwrap_or(1.0),
        exposure: photo.exposure.unwrap_or_default(),
        focal35: photo.focal35.unwrap_or_default(),
        raw_clipped: photo.raw_clipped,
        ..Default::default()
    }
}

pub fn cull_detail(photo: &Photo, scale: &numa_cull::Scale) -> String {
    match cull_detail_lines(photo, scale) {
        Some(lines) => lines.iter().map(DetailLine::to_string).collect::<Vec<_>>().join("\n"),
        None => "Not measured yet".to_string(),
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum DetailLine {

    Sharpness { sharpness: f32, soft_below: f32 },

    Blown { blown: f32, raw: Option<f32>, holds: bool, a_lot: f32 },

    DeepShadow { share: f32 },

    Spread { contrast: f32, blank: f32 },

    Shutter { seconds: f32, focal35: f32, stops: f32, named_past: f32 },
    NoFaces,
    EyesClosed,

    Faces { count: u32, sharpest: Option<f32> },
    FacesNotLookedFor,
    Suggested { stars: f32 },
}

pub fn cull_detail_lines(photo: &Photo, scale: &numa_cull::Scale) -> Option<Vec<DetailLine>> {
    let (Some(sharpness), Some(blown)) = (photo.sharpness, photo.blown) else {
        return None;
    };
    let a_lot = numa_cull::BLOWN * 100.0;
    let mut lines = vec![
        DetailLine::Sharpness { sharpness, soft_below: scale.soft },

        DetailLine::Blown {
            blown: blown * 100.0,
            raw: photo.raw_clipped.map(|raw| raw * 100.0),
            holds: photo.raw_clipped.is_some_and(|raw| blown > numa_cull::BLOWN && raw <= numa_cull::BLOWN),
            a_lot,
        },
    ];
    if let Some(dark) = photo.raw_dark.filter(|dark| *dark >= 0.01) {
        lines.push(DetailLine::DeepShadow { share: dark * 100.0 });
    }
    if let Some(contrast) = photo.contrast {
        lines.push(DetailLine::Spread { contrast, blank: numa_cull::BLANK });
    }

    if let (Some(seconds), Some(focal35)) = (photo.exposure, photo.focal35) {
        let stops = (seconds * focal35).log2();
        lines.push(DetailLine::Shutter { seconds, focal35, stops, named_past: numa_cull::HANDHELD.log2() });
    }
    lines.push(match (photo.faces, photo.face_sharpness) {
        (Some(0), _) => DetailLine::NoFaces,
        (Some(_), _) if photo.eyes_closed == Some(true) => DetailLine::EyesClosed,
        (Some(count), sharpest) => DetailLine::Faces { count, sharpest },
        (None, _) => DetailLine::FacesNotLookedFor,
    });
    if let Some(stars) = photo.suggested {
        lines.push(DetailLine::Suggested { stars });
    }
    Some(lines)
}

impl std::fmt::Display for DetailLine {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match *self {
            DetailLine::Sharpness { sharpness, soft_below } => {
                write!(f, "Sharpness {sharpness:.2} (soft below {soft_below:.2} in this library)")
            }
            DetailLine::Blown { blown, raw: Some(raw), holds, a_lot } => write!(
                f,
                "Blown {raw:.1} % in the raw, {blown:.1} % in the camera's JPEG{} (a lot above {a_lot:.0} %)",
                if holds { " — the raw holds it" } else { "" }
            ),
            DetailLine::Blown { blown, raw: None, a_lot, .. } => {
                write!(f, "Blown {blown:.1} % (a lot above {a_lot:.0} %)")
            }
            DetailLine::DeepShadow { share } => {
                write!(f, "{share:.0} % of the raw is deep shadow, within 1 % of black")
            }
            DetailLine::Spread { contrast, blank } => {
                write!(f, "Spread of tone {contrast:.3} (next to nothing in it below {blank:.2})")
            }
            DetailLine::Shutter { seconds, focal35, stops, named_past } => {
                match seconds >= 1.0 {
                    true => write!(f, "{seconds:.1} s")?,
                    false => write!(f, "1/{:.0} s", 1.0 / seconds)?,
                }
                write!(f, " at {focal35:.0} mm full-frame — ")?;
                match stops > 0.0 {
                    true => write!(f, "{stops:.1} stops slower than 1/{focal35:.0}")?,
                    false => write!(f, "faster than 1/{focal35:.0}")?,
                }
                write!(f, " (the shutter is named as a reason past {named_past:.0})")
            }
            DetailLine::NoFaces => f.write_str("No faces found"),
            DetailLine::EyesClosed => f.write_str("Both eyes of the largest face read as closed — a guess, check it"),
            DetailLine::Faces { count, sharpest: Some(face) } => {
                write!(f, "{count} face(s), sharpest {face:.2} — this is what is scored")
            }
            DetailLine::Faces { count, sharpest: None } => write!(f, "{count} face(s), too small to judge"),
            DetailLine::FacesNotLookedFor => f.write_str("Faces not looked for"),
            DetailLine::Suggested { stars } => write!(f, "Suggested {stars:.1} of 5 — a suggestion, not a rating"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::Flag;
    use std::path::PathBuf;

    fn measured() -> Photo {
        Photo {
            id: 1,
            path: PathBuf::from("/1.raf"),
            mtime: 0,
            taken: None,
            rating: 0,
            flag: Flag::None,
            sharpness: Some(0.42),
            blown: Some(0.12),
            best_of_burst: false,
            burst: None,
            echo: None,
            exposure: Some(1.0 / 8.0),
            focal35: Some(50.0),
            raw_clipped: Some(0.05),
            raw_dark: Some(0.03),
            eyes_closed: None,
            faces: Some(2),
            face_sharpness: Some(0.51),
            brightness: None,
            contrast: Some(0.25),
            suggested: Some(3.4),
            edited: false,
            aspect: None,
        }
    }

    #[test]
    fn the_detail_in_parts_reads_as_the_tooltip() {
        let scale = numa_cull::Scale { dull: 0.1, crisp: 0.9, soft: 0.3 };
        assert_eq!(
            cull_detail(&measured(), &scale),
            "Sharpness 0.42 (soft below 0.30 in this library)\n\
             Blown 5.0 % in the raw, 12.0 % in the camera's JPEG — the raw holds it (a lot above 10 %)\n\
             3 % of the raw is deep shadow, within 1 % of black\n\
             Spread of tone 0.250 (next to nothing in it below 0.01)\n\
             1/8 s at 50 mm full-frame — 2.6 stops slower than 1/50 (the shutter is named as a reason past 5)\n\
             2 face(s), sharpest 0.51 — this is what is scored\n\
             Suggested 3.4 of 5 — a suggestion, not a rating"
        );
        let lines = cull_detail_lines(&measured(), &scale).unwrap();
        assert_eq!(lines[5], DetailLine::Faces { count: 2, sharpest: Some(0.51) });
        let unmeasured = Photo { sharpness: None, ..measured() };
        assert_eq!(cull_detail_lines(&unmeasured, &scale), None);
        assert_eq!(cull_detail(&unmeasured, &scale), "Not measured yet");
    }
}
