use crate::mask::{Mask, Shape};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaskPart {
    Class(u16),
    Point(usize),

    Stroke { at: usize, run: usize },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PartRow {
    pub part: MaskPart,
    pub on: bool,
}

impl Mask {
    pub fn parts(&self) -> Vec<PartRow> {
        let mut rows = Vec::new();
        if let Shape::Segment { classes } = &self.shape {
            for class in classes {
                rows.push(PartRow { part: MaskPart::Class(*class), on: !self.muted.contains(class) });
            }
        }
        for (at, point) in self.points.iter().enumerate() {
            rows.push(PartRow { part: MaskPart::Point(at), on: point.enabled });
        }
        let mut at = 0;
        while at < self.strokes.len() {
            let first = &self.strokes[at];
            let same = |next: &&crate::mask::Stroke| {
                next.fill == first.fill && next.erase == first.erase && next.enabled == first.enabled
            };
            let run = self.strokes[at..].iter().take_while(same).count();
            rows.push(PartRow { part: MaskPart::Stroke { at, run }, on: first.enabled });
            at += run;
        }
        rows
    }

    pub fn toggle_part(&mut self, part: MaskPart) -> bool {
        match part {
            MaskPart::Class(class) => {
                if let Some(at) = self.muted.iter().position(|held| *held == class) {
                    self.muted.remove(at);
                    return true;
                }
                let Shape::Segment { classes } = &self.shape else { return false };
                if classes.iter().filter(|class| !self.muted.contains(class)).count() <= 1 {
                    return false;
                }
                self.muted.push(class);
            }
            MaskPart::Point(at) => {
                let Some(point) = self.points.get_mut(at) else { return false };
                point.enabled = !point.enabled;
            }
            MaskPart::Stroke { at, run } => {
                let Some(wanted) = self.strokes.get(at).map(|first| !first.enabled) else { return false };
                for stroke in self.strokes.iter_mut().skip(at).take(run) {
                    stroke.enabled = wanted;
                }
            }
        }
        true
    }

    pub fn remove_part(&mut self, part: MaskPart) -> bool {
        match part {
            MaskPart::Class(class) => {
                let Shape::Segment { classes } = &mut self.shape else { return false };
                if classes.len() <= 1 {
                    return false;
                }
                classes.retain(|held| *held != class);
            }
            MaskPart::Point(at) => {
                if at >= self.points.len() {
                    return false;
                }
                self.points.remove(at);
            }
            MaskPart::Stroke { at, run } => {
                if at >= self.strokes.len() {
                    return false;
                }
                self.strokes.drain(at..(at + run).min(self.strokes.len()));
            }
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mask::{RegionPoint, Stroke};

    fn stroke(erase: bool) -> Stroke {
        let mut stroke = Stroke::new(0.05, 0.5, erase);
        stroke.points = vec![[0.5, 0.5]];
        stroke
    }

    #[test]
    fn strokes_of_one_kind_are_one_row_and_go_together() {
        let mut mask = Mask::new(Shape::Painted);
        mask.strokes = vec![stroke(false), stroke(false), stroke(true)];
        let rows = mask.parts();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].part, MaskPart::Stroke { at: 0, run: 2 });

        assert!(mask.toggle_part(rows[0].part));
        assert!(!mask.strokes[0].enabled && !mask.strokes[1].enabled && mask.strokes[2].enabled);
        assert!(!mask.parts()[0].on);

        assert!(mask.remove_part(MaskPart::Stroke { at: 0, run: 2 }));
        assert_eq!(mask.strokes.len(), 1);
    }

    #[test]
    fn the_last_class_stays() {
        let mut mask = Mask::new(Shape::Segment { classes: vec![2, 12] });
        assert!(mask.toggle_part(MaskPart::Class(2)));
        assert!(!mask.toggle_part(MaskPart::Class(12)), "the last one live");
        assert!(mask.toggle_part(MaskPart::Class(2)), "back on");
        assert!(mask.remove_part(MaskPart::Class(2)));
        assert!(!mask.remove_part(MaskPart::Class(12)), "the last one there");
    }

    #[test]
    fn classes_come_first_then_clicks_then_strokes() {
        let mut mask = Mask::new(Shape::Segment { classes: vec![2] });
        mask.strokes = vec![stroke(false)];
        mask.points = vec![RegionPoint { at: [0.5, 0.5], subtract: false, enabled: false }];
        let rows = mask.parts();
        assert_eq!(rows[0], PartRow { part: MaskPart::Class(2), on: true });
        assert_eq!(rows[1], PartRow { part: MaskPart::Point(0), on: false });
        assert_eq!(rows[2].part, MaskPart::Stroke { at: 0, run: 1 });
    }
}
