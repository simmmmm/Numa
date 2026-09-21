use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Spot {

    pub at: [f32; 2],

    pub from: [f32; 2],

    pub radius: f32,

    pub feather: f32,

    pub opacity: f32,

    pub heal: bool,

    #[serde(default)]
    pub kind: Kind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Kind {

    #[default]
    Patch,

    PetEye,

    Remove,
}

impl Default for Spot {
    fn default() -> Self {
        Self {
            at: [0.5, 0.5],
            from: [0.5, 0.5],
            radius: 0.03,
            feather: 0.5,
            opacity: 1.0,
            heal: true,
            kind: Kind::Patch,
        }
    }
}

impl Spot {

    pub fn is_idle(&self) -> bool {
        self.radius <= 0.0 || self.opacity <= 0.0
    }

    pub fn coverage(&self, distance: f32, radius: f32) -> f32 {
        if radius <= 0.0 || distance >= radius {
            return 0.0;
        }
        let inner = radius * (1.0 - self.feather.clamp(0.0, 1.0));
        if distance <= inner {
            return self.opacity.clamp(0.0, 1.0);
        }
        let t = ((distance - inner) / (radius - inner)).clamp(0.0, 1.0);
        (1.0 - t * t * (3.0 - 2.0 * t)) * self.opacity.clamp(0.0, 1.0)
    }
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct Retouch {
    pub spots: Vec<Spot>,
}

impl Retouch {

    pub fn is_identity(&self) -> bool {
        self.spots.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_spot_still_under_the_pointer_is_kept() {
        let spot = Spot { at: [0.3, 0.3], from: [0.3, 0.3], ..Default::default() };
        assert!(!spot.is_idle());
        assert!(!Retouch { spots: vec![spot] }.is_identity());
    }

    #[test]
    fn no_spots_is_no_operation() {
        assert!(Retouch::default().is_identity());
        assert!(Retouch { spots: Vec::new() }.is_identity());
    }

    #[test]
    fn a_spot_with_no_size_renders_nothing_but_is_not_forgotten() {
        let spot = Spot { radius: 0.0, ..Default::default() };
        assert!(spot.is_idle());
        assert!(!Retouch { spots: vec![spot] }.is_identity());
    }

    #[test]
    fn coverage_is_solid_in_the_middle_and_gone_at_the_edge() {
        let spot = Spot { feather: 0.5, opacity: 1.0, ..Default::default() };
        assert_eq!(spot.coverage(0.0, 10.0), 1.0);
        assert_eq!(spot.coverage(10.0, 10.0), 0.0);
        assert_eq!(spot.coverage(11.0, 10.0), 0.0);

        assert_eq!(spot.coverage(4.9, 10.0), 1.0);
        let edge = spot.coverage(7.5, 10.0);
        assert!(edge > 0.0 && edge < 1.0, "the feather should ramp: {edge}");
    }

    #[test]
    fn no_feather_is_a_hard_edge() {
        let spot = Spot { feather: 0.0, ..Default::default() };
        assert_eq!(spot.coverage(9.9, 10.0), 1.0);
        assert_eq!(spot.coverage(10.0, 10.0), 0.0);
    }

    #[test]
    fn opacity_scales_the_whole_patch() {
        let spot = Spot { opacity: 0.4, feather: 0.5, ..Default::default() };
        assert!((spot.coverage(0.0, 10.0) - 0.4).abs() < 1e-6);
        assert!(spot.coverage(7.5, 10.0) < 0.4);
    }
}
