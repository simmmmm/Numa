use numa_core::mask::{Mask, Shape};
use numa_render::segment;

pub fn mask_name(mask: &Mask) -> String {
    mask_called(mask).to_string()
}

#[derive(Clone, Debug, PartialEq)]
pub enum MaskName {

    Own(String),

    Kind(&'static str),

    Found(segment::Named),
}

pub fn mask_called(mask: &Mask) -> MaskName {

    if let Some(name) = mask.name.as_ref().filter(|name| !name.trim().is_empty()) {
        return MaskName::Own(name.trim().to_string());
    }
    MaskName::Kind(match &mask.shape {
        Shape::Linear { .. } => "Linear",
        Shape::Radial { .. } => "Radial",
        Shape::Segment { classes } => return MaskName::Found(segment::named(classes)),

        Shape::Subject if mask.inverted => "Background",
        Shape::Subject => "Subject",

        Shape::Painted if mask.strokes.is_empty() && !mask.points.is_empty() => "Click",
        Shape::Painted => "Brush",
        Shape::ColourRange { .. } => "Colour range",
        Shape::LuminanceRange { .. } => "Luminance range",
    })
}

impl std::fmt::Display for MaskName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MaskName::Own(name) => f.write_str(name),
            MaskName::Kind(word) => f.write_str(word),
            MaskName::Found(named) => named.fmt(f),
        }
    }
}

pub fn mask_label(masks: &[Mask], index: usize) -> String {
    mask_label_parts(masks, index).to_string()
}

#[derive(Clone, Debug, PartialEq)]
pub struct MaskLabel {
    pub name: MaskName,
    pub number: Option<usize>,
}

pub fn mask_label_parts(masks: &[Mask], index: usize) -> MaskLabel {
    let name = mask_called(&masks[index]);

    let reads = name.to_string();
    let same: Vec<usize> =
        (0..masks.len()).filter(|other| mask_name(&masks[*other]) == reads).collect();
    let number = (same.len() > 1)
        .then(|| same.iter().position(|other| *other == index).unwrap_or(0) + 1);
    MaskLabel { name, number }
}

impl std::fmt::Display for MaskLabel {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.number {
            Some(number) => write!(f, "{} {number}", self.name),
            None => self.name.fmt(f),
        }
    }
}

#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Chips {

    pub groups: Vec<(String, Vec<u16>, f32)>,

    pub animal: Option<(String, f32)>,

    #[serde(skip)]
    pub grid: (usize, usize, Vec<u8>),
}

impl Chips {
    pub fn of(found: &segment::Segmentation) -> Self {
        let (width, height, winners) = found.winners();
        Chips {
            groups: found.found().into_iter().map(|thing| (thing.name, thing.classes, thing.share)).collect(),
            animal: None,

            grid: (width, height, winners.iter().map(|class| (*class).min(255) as u8).collect()),
        }
    }

    pub fn found(&self) -> Vec<segment::Found> {
        let groups = self.groups.iter().cloned();
        groups.map(|(name, classes, share)| segment::Found { name, classes, share }).collect()
    }

    pub fn coarse(&self, classes: &[u16]) -> (usize, usize, Vec<f32>) {
        let (width, height, cells) = &self.grid;
        (*width, *height, cells.iter().map(|class| classes.contains(&(*class as u16)) as u8 as f32).collect())
    }

    pub fn asked(framing: &str) -> String {
        format!("{framing}\0{}", segment::asked())
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

        assert_eq!(
            mask_label_parts(&three, 2),
            MaskLabel { name: MaskName::Own("Bird".into()), number: Some(2) }
        );
        assert_eq!(
            mask_label_parts(&plain, 0),
            MaskLabel { name: MaskName::Kind("Radial"), number: Some(1) }
        );
        let found = [Mask::new(Shape::Segment { classes: vec![12, 126] })];
        assert_eq!(
            mask_label_parts(&found, 0).name,
            MaskName::Found(segment::Named::Classes { labels: vec!["person", "animal"], more: 0 })
        );
    }
}
