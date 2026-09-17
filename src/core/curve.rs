use serde::{Deserialize, Serialize};

pub const LOOKUP: usize = 256;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(from = "CurveWire")]
pub struct Curve {

    points: Vec<[f32; 2]>,
}

#[derive(Deserialize)]
struct CurveWire {
    points: Vec<[f32; 2]>,
}

impl From<CurveWire> for Curve {
    fn from(wire: CurveWire) -> Self {
        Self::new(wire.points)
    }
}

impl Default for Curve {
    fn default() -> Self {
        Self::identity()
    }
}

impl Curve {

    pub fn identity() -> Self {
        Self { points: vec![[0.0, 0.0], [1.0, 1.0]] }
    }

    pub fn new(points: impl IntoIterator<Item = [f32; 2]>) -> Self {
        let mut points: Vec<[f32; 2]> = points
            .into_iter()
            .map(|[x, y]| [x.clamp(0.0, 1.0), y.clamp(0.0, 1.0)])
            .collect();
        points.sort_by(|a, b| a[0].total_cmp(&b[0]));
        points.dedup_by(|a, b| (a[0] - b[0]).abs() < f32::EPSILON);

        if points.len() < 2 {
            return Self::identity();
        }
        Self { points }
    }

    pub fn points(&self) -> &[[f32; 2]] {
        &self.points
    }

    pub fn is_identity(&self) -> bool {
        self.points.iter().all(|[x, y]| (x - y).abs() < 1e-4)
    }

    pub fn place(&mut self, point: [f32; 2], grab: f32) -> usize {
        let [x, y] = [point[0].clamp(0.0, 1.0), point[1].clamp(0.0, 1.0)];

        let nearest = self
            .points
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| (a[0] - x).abs().total_cmp(&(b[0] - x).abs()));

        match nearest {
            Some((index, near)) if (near[0] - x).abs() <= grab => {

                if index == 0 || index == self.points.len() - 1 {
                    self.points[index][1] = y;
                } else {
                    self.points[index] = [x, y];
                }
                index
            }
            _ => {
                let index = self.points.partition_point(|p| p[0] < x);
                self.points.insert(index, [x, y]);
                index
            }
        }
    }

    pub fn move_point(&mut self, index: usize, to: [f32; 2]) {
        let last = self.points.len() - 1;
        let Some(point) = self.points.get_mut(index) else { return };

        let y = to[1].clamp(0.0, 1.0);
        if index == 0 || index == last {
            point[1] = y;
            return;
        }

        const GAP: f32 = 1e-3;
        let low = self.points[index - 1][0] + GAP;
        let high = self.points[index + 1][0] - GAP;
        self.points[index] = [to[0].clamp(low.min(high), high.max(low)), y];
    }

    pub fn remove(&mut self, index: usize) {
        if index > 0 && index + 1 < self.points.len() {
            self.points.remove(index);
        }
    }

    pub fn value_at(&self, x: f32) -> f32 {
        let x = x.clamp(0.0, 1.0);
        let slopes = self.slopes();

        let last = self.points.len() - 1;
        if x <= self.points[0][0] {
            return self.points[0][1];
        }
        if x >= self.points[last][0] {
            return self.points[last][1];
        }

        let index = self.points.partition_point(|p| p[0] <= x) - 1;
        let [x0, y0] = self.points[index];
        let [x1, y1] = self.points[index + 1];
        let width = x1 - x0;
        let t = (x - x0) / width;

        let (t2, t3) = (t * t, t * t * t);
        let h00 = 2.0 * t3 - 3.0 * t2 + 1.0;
        let h10 = t3 - 2.0 * t2 + t;
        let h01 = -2.0 * t3 + 3.0 * t2;
        let h11 = t3 - t2;

        (h00 * y0 + h10 * width * slopes[index] + h01 * y1 + h11 * width * slopes[index + 1])
            .clamp(0.0, 1.0)
    }

    pub fn lookup(&self) -> [f32; LOOKUP] {
        let mut table = [0.0f32; LOOKUP];
        for (index, entry) in table.iter_mut().enumerate() {
            *entry = self.value_at(index as f32 / (LOOKUP - 1) as f32);
        }
        table
    }

    fn slopes(&self) -> Vec<f32> {
        let count = self.points.len();
        let secants: Vec<f32> = self
            .points
            .windows(2)
            .map(|pair| (pair[1][1] - pair[0][1]) / (pair[1][0] - pair[0][0]))
            .collect();

        let mut slopes = vec![0.0f32; count];
        slopes[0] = secants[0];
        slopes[count - 1] = secants[count - 2];
        for index in 1..count - 1 {
            slopes[index] = (secants[index - 1] + secants[index]) / 2.0;
        }

        for (index, secant) in secants.iter().enumerate() {
            if *secant == 0.0 {

                slopes[index] = 0.0;
                slopes[index + 1] = 0.0;
                continue;
            }

            let a = slopes[index] / secant;
            let b = slopes[index + 1] / secant;
            let radius = a * a + b * b;
            if radius > 9.0 {
                let scale = 3.0 / radius.sqrt();
                slopes[index] = scale * a * secant;
                slopes[index + 1] = scale * b * secant;
            }
        }

        slopes
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_stored_curve_is_repaired_rather_than_trusted() {
        let empty: Curve = serde_json::from_str(r#"{"points":[]}"#).unwrap();
        assert_eq!(empty, Curve::identity());
        let one: Curve = serde_json::from_str(r#"{"points":[[0.5,0.5]]}"#).unwrap();
        assert_eq!(one, Curve::identity());
        let backwards: Curve =
            serde_json::from_str(r#"{"points":[[1.0,1.0],[0.5,0.7],[0.5,0.2],[0.0,0.0]]}"#).unwrap();
        assert_eq!(backwards.points(), &[[0.0, 0.0], [0.5, 0.7], [1.0, 1.0]]);
        let _ = backwards.lookup();
    }

    #[test]
    fn the_identity_leaves_every_value_alone() {
        let curve = Curve::identity();
        assert!(curve.is_identity());
        for step in 0..=10 {
            let value = step as f32 / 10.0;
            assert!((curve.value_at(value) - value).abs() < 1e-5, "at {value}");
        }
    }

    #[test]
    fn the_curve_passes_through_its_own_points() {

        let curve = Curve::new([[0.0, 0.0], [0.25, 0.4], [0.75, 0.6], [1.0, 1.0]]);
        for [x, y] in curve.points() {
            assert!((curve.value_at(*x) - y).abs() < 1e-4, "at {x}: want {y}");
        }
    }

    #[test]
    fn a_steep_segment_never_makes_the_curve_turn_back() {

        let curve = Curve::new([[0.0, 0.0], [0.45, 0.05], [0.55, 0.95], [1.0, 1.0]]);

        let mut previous = -1.0;
        for step in 0..=1000 {
            let x = step as f32 / 1000.0;
            let y = curve.value_at(x);
            assert!(
                y >= previous - 1e-5,
                "the curve turns back at {x}: {previous} then {y}"
            );
            assert!((0.0..=1.0).contains(&y), "out of range at {x}: {y}");
            previous = y;
        }
    }

    #[test]
    fn a_flat_segment_stays_flat() {

        let curve = Curve::new([[0.0, 0.0], [0.4, 0.8], [0.7, 0.8], [1.0, 1.0]]);
        for step in 0..=30 {
            let x = 0.4 + (0.7 - 0.4) * step as f32 / 30.0;
            assert!((curve.value_at(x) - 0.8).abs() < 1e-4, "bulge at {x}");
        }
    }

    #[test]
    fn points_are_sorted_and_never_share_an_input() {

        let curve = Curve::new([[0.8, 0.9], [0.0, 0.0], [0.8, 0.2], [1.0, 1.0]]);
        let inputs: Vec<f32> = curve.points().iter().map(|p| p[0]).collect();
        assert_eq!(inputs, vec![0.0, 0.8, 1.0]);

        assert_eq!(Curve::new([]), Curve::identity());
        assert_eq!(Curve::new([[0.5, 0.5]]), Curve::identity());
    }

    #[test]
    fn an_interior_point_stays_between_its_neighbours() {
        let mut curve = Curve::new([[0.0, 0.0], [0.5, 0.5], [1.0, 1.0]]);

        curve.move_point(1, [5.0, 0.7]);
        let moved = curve.points()[1];
        assert!(moved[0] < 1.0 && moved[0] > 0.0, "escaped to {moved:?}");
        assert_eq!(moved[1], 0.7);

        curve.move_point(0, [0.3, 0.2]);
        assert_eq!(curve.points()[0], [0.0, 0.2]);
        curve.move_point(2, [0.3, 0.9]);
        assert_eq!(curve.points()[2], [1.0, 0.9]);
    }

    #[test]
    fn placing_near_a_point_moves_it_rather_than_crowding_it() {
        let mut curve = Curve::new([[0.0, 0.0], [0.5, 0.5], [1.0, 1.0]]);

        let index = curve.place([0.52, 0.7], 0.05);
        assert_eq!(index, 1);
        assert_eq!(curve.points().len(), 3, "a near miss must not add a point");
        assert_eq!(curve.points()[1], [0.52, 0.7]);

        curve.place([0.2, 0.3], 0.05);
        assert_eq!(curve.points().len(), 4);
        assert_eq!(curve.points()[1], [0.2, 0.3]);
    }

    #[test]
    fn grabbing_an_end_moves_it_up_and_down_only() {
        let mut curve = Curve::new([[0.0, 0.0], [0.5, 0.5], [1.0, 1.0]]);

        let index = curve.place([0.03, 0.2], 0.05);
        assert_eq!(index, 0);
        assert_eq!(curve.points()[0], [0.0, 0.2], "black stayed at the left edge");

        let index = curve.place([0.98, 0.8], 0.05);
        assert_eq!(index, 2);
        assert_eq!(curve.points()[2], [1.0, 0.8], "white stayed at the right edge");

        assert_eq!(curve.value_at(0.0), 0.2);
        assert!(curve.value_at(0.02) > 0.2, "a shelf formed at the dark end");

        curve.place([0.52, 0.9], 0.05);
        assert_eq!(curve.points()[1], [0.52, 0.9]);
    }

    #[test]
    fn the_ends_cannot_be_removed() {
        let mut curve = Curve::new([[0.0, 0.0], [0.5, 0.8], [1.0, 1.0]]);
        curve.remove(0);
        curve.remove(2);
        assert_eq!(curve.points().len(), 3, "an end was removed");

        curve.remove(1);
        assert_eq!(curve.points(), Curve::identity().points());
    }

    #[test]
    fn the_lookup_matches_the_curve_it_came_from() {
        let curve = Curve::new([[0.0, 0.0], [0.3, 0.15], [0.7, 0.85], [1.0, 1.0]]);
        let table = curve.lookup();

        for (index, entry) in table.iter().enumerate() {
            let x = index as f32 / (LOOKUP - 1) as f32;
            assert!((entry - curve.value_at(x)).abs() < 1e-6, "entry {index}");
        }
        assert_eq!(table[0], 0.0);
        assert_eq!(table[LOOKUP - 1], 1.0);
    }
}
