use crate::catalog::Photo;

pub fn cull_note(photo: &Photo, scale: &numa_cull::Scale) -> String {
    let frame = numa_cull::Frame {
        sharpness: photo.sharpness.unwrap_or_default(),
        blown: photo.blown.unwrap_or_default(),

        brightness: photo.brightness.unwrap_or(0.5),
        contrast: photo.contrast.unwrap_or(1.0),
        exposure: photo.exposure.unwrap_or_default(),
        focal35: photo.focal35.unwrap_or_default(),
        raw_clipped: photo.raw_clipped,
        ..Default::default()
    };

    let mut parts: Vec<String> = Vec::new();
    if let Some(suggested) = photo.suggested {
        parts.push(format!("~{suggested:.1}★"));
    }

    if let Some(note) = frame.blank_note() {
        parts.push(note.to_string());
        return parts.join(" · ");
    }
    if photo.best_of_burst {
        parts.push("best of burst".to_string());
    }

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
    parts.join(" · ")
}

pub fn cull_detail(photo: &Photo, scale: &numa_cull::Scale) -> String {
    let (Some(sharpness), Some(blown)) = (photo.sharpness, photo.blown) else {
        return "Not measured yet".to_string();
    };

    let mut lines = vec![
        format!("Sharpness {sharpness:.2} (soft below {:.2} in this library)", scale.soft),

        match photo.raw_clipped {
            Some(raw) => format!(
                "Blown {:.1} % in the raw, {:.1} % in the camera's JPEG{} (a lot above {:.0} %)",
                raw * 100.0,
                blown * 100.0,
                if blown > numa_cull::BLOWN && raw <= numa_cull::BLOWN { " — the raw holds it" } else { "" },
                numa_cull::BLOWN * 100.0
            ),
            None => format!("Blown {:.1} % (a lot above {:.0} %)", blown * 100.0, numa_cull::BLOWN * 100.0),
        },
    ];
    if let Some(dark) = photo.raw_dark.filter(|dark| *dark >= 0.01) {
        lines.push(format!("{:.0} % of the raw is deep shadow, within 1 % of black", dark * 100.0));
    }
    if let Some(contrast) = photo.contrast {
        lines.push(format!("Spread of tone {contrast:.3} (next to nothing in it below {:.2})", numa_cull::BLANK));
    }

    if let (Some(exposure), Some(focal35)) = (photo.exposure, photo.focal35) {
        let shutter = match exposure >= 1.0 {
            true => format!("{exposure:.1} s"),
            false => format!("1/{:.0} s", 1.0 / exposure),
        };
        let stops = (exposure * focal35).log2();
        let past = match stops > 0.0 {
            true => format!("{stops:.1} stops slower than 1/{focal35:.0}"),
            false => format!("faster than 1/{focal35:.0}"),
        };
        lines.push(format!(
            "{shutter} at {focal35:.0} mm full-frame — {past} (the shutter is named as a reason past {:.0})",
            numa_cull::HANDHELD.log2()
        ));
    }
    match (photo.faces, photo.face_sharpness) {
        (Some(0), _) => lines.push("No faces found".to_string()),
        (Some(_), _) if photo.eyes_closed == Some(true) => {
            lines.push("Both eyes of the largest face read as closed — a guess, check it".to_string())
        }
        (Some(count), Some(face)) => {
            lines.push(format!("{count} face(s), sharpest {face:.2} — this is what is scored"));
        }
        (Some(count), None) => lines.push(format!("{count} face(s), too small to judge")),
        (None, _) => lines.push("Faces not looked for".to_string()),
    }
    if let Some(suggested) = photo.suggested {
        lines.push(format!("Suggested {suggested:.1} of 5 — a suggestion, not a rating"));
    }
    lines.join("\n")
}
