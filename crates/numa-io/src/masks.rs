use numa_core::mask::{Mask, Shape};
use numa_render::segment;

pub fn mask_name(mask: &Mask) -> String {

    if let Some(name) = mask.name.as_ref().filter(|name| !name.trim().is_empty()) {
        return name.trim().to_string();
    }
    match &mask.shape {
        Shape::Linear { .. } => "Linear".to_string(),
        Shape::Radial { .. } => "Radial".to_string(),
        Shape::Segment { classes } => segment::name_for(classes),

        Shape::Subject if mask.inverted => "Background".to_string(),
        Shape::Subject => "Subject".to_string(),

        Shape::Painted if mask.strokes.is_empty() && !mask.points.is_empty() => "Click".to_string(),
        Shape::Painted => "Brush".to_string(),
        Shape::ColourRange { .. } => "Colour range".to_string(),
        Shape::LuminanceRange { .. } => "Luminance range".to_string(),
    }
}

pub fn mask_label(masks: &[Mask], index: usize) -> String {
    let name = mask_name(&masks[index]);
    let same: Vec<usize> =
        (0..masks.len()).filter(|other| mask_name(&masks[*other]) == name).collect();
    match same.len() > 1 {
        true => {
            format!("{name} {}", same.iter().position(|other| *other == index).unwrap_or(0) + 1)
        }
        false => name,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use numa_core::mask::{Mask, Shape};

    fn radial(name: Option<&str>) -> Mask {
        let mut mask = Mask::new(Shape::radial());
        mask.name = name.map(str::to_string);
        mask
    }

    #[test]
    fn subject_and_background_say_what_their_chips_say() {
        let mut subject = Mask::new(Shape::Subject);
        assert_eq!(mask_name(&subject), "Subject");
        subject.inverted = true;
        assert_eq!(mask_name(&subject), "Background");
        assert_eq!(numa_core::mask::SUBJECT_CLASSES, segment::MATTEABLE);
    }

    #[test]
    fn numbered_only_when_shared() {
        let one = [radial(Some("Bird"))];
        assert_eq!(mask_label(&one, 0), "Bird");

        let two = [radial(Some("Bird")), radial(Some("Sky"))];
        assert_eq!(mask_label(&two, 0), "Bird");
        assert_eq!(mask_label(&two, 1), "Sky");

        let three = [radial(Some("Bird")), radial(Some("Sky")), radial(Some("Bird"))];
        assert_eq!(mask_label(&three, 0), "Bird 1");
        assert_eq!(mask_label(&three, 1), "Sky");
        assert_eq!(mask_label(&three, 2), "Bird 2");

        let plain = [radial(None), radial(None)];
        assert_eq!(mask_label(&plain, 0), "Radial 1");
        assert_eq!(mask_label(&plain, 1), "Radial 2");
    }
}
