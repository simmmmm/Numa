#[derive(Debug, Clone)]
pub struct DngProfile {
    pub name: String,

    pub camera: Option<String>,

    pub color_matrix: [Option<Matrix3>; 2],

    pub forward_matrix: [Option<Matrix3>; 2],

    pub illuminant: [Option<u16>; 2],

    pub hue_sat_map: Option<HsvTable>,

    pub look_table: Option<HsvTable>,

    pub tone_curve: Option<Vec<(f32, f32)>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ValueEncoding {

    #[default]
    Linear,

    Srgb,
}

impl ValueEncoding {
    fn encode(self, value: f32) -> f32 {
        match self {
            ValueEncoding::Linear => value,
            ValueEncoding::Srgb => {
                let v = value.clamp(0.0, 1.0);
                if v <= 0.003_130_8 {
                    v * 12.92
                } else {
                    1.055 * v.powf(1.0 / 2.4) - 0.055
                }
            }
        }
    }
}

pub type Matrix3 = [[f32; 3]; 3];

#[derive(Debug, Clone)]
pub struct HsvTable {

    pub value_encoding: ValueEncoding,
    pub hue_divisions: usize,
    pub sat_divisions: usize,
    pub val_divisions: usize,

    pub entries: Vec<[f32; 3]>,

    pub second: Option<Vec<[f32; 3]>>,
}

impl HsvTable {
    pub fn len(&self) -> usize {
        self.hue_divisions * self.sat_divisions * self.val_divisions
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

const PROPHOTO_TO_XYZ_D50: Matrix3 = [
    [0.797_674_9, 0.135_191_7, 0.031_353_4],
    [0.288_040_2, 0.711_874_1, 0.000_059_9],
    [0.000_000_0, 0.000_000_0, 0.825_210_0],
];

pub const XYZ_D50_TO_PROPHOTO: Matrix3 = [
    [1.345_943_3, -0.255_607_5, -0.051_111_8],
    [-0.544_598_9, 1.508_167_3, 0.020_535_1],
    [0.000_000_0, 0.000_000_0, 1.211_812_8],
];

pub const SRGB_TO_XYZ_D50: Matrix3 = [
    [0.436_074_7, 0.385_064_9, 0.143_080_4],
    [0.222_504_5, 0.716_878_6, 0.060_616_9],
    [0.013_932_2, 0.097_104_5, 0.714_173_3],
];

const XYZ_D50_TO_SRGB: Matrix3 = [
    [3.133_856_1, -1.616_866_7, -0.490_614_6],
    [-0.978_768_4, 1.916_141_5, 0.033_454_0],
    [0.071_945_3, -0.228_991_4, 1.405_242_7],
];

pub fn multiply(matrix: &Matrix3, vector: [f32; 3]) -> [f32; 3] {
    let mut out = [0.0; 3];
    for (index, row) in matrix.iter().enumerate() {
        out[index] = row[0] * vector[0] + row[1] * vector[1] + row[2] * vector[2];
    }
    out
}

pub fn multiply_matrix(a: &Matrix3, b: &Matrix3) -> Matrix3 {
    let mut out = [[0.0f32; 3]; 3];
    for row in 0..3 {
        for column in 0..3 {
            out[row][column] = (0..3).map(|k| a[row][k] * b[k][column]).sum();
        }
    }
    out
}

pub fn prophoto_hue_of_srgb(degrees: f32) -> f32 {

    let sixths = degrees.rem_euclid(360.0) / 60.0;
    let encoded = hsv_to_rgb([sixths, 1.0, 1.0]);
    let linear = encoded.map(|value| {
        if value <= 0.04045 {
            value / 12.92
        } else {
            ((value + 0.055) / 1.055).powf(2.4)
        }
    });

    let to_prophoto = multiply_matrix(&XYZ_D50_TO_PROPHOTO, &SRGB_TO_XYZ_D50);
    let wide = multiply(&to_prophoto, linear).map(|value| value.max(0.0));

    rgb_to_hsv(wide)[0] * 60.0
}

pub fn rgb_to_hsv(rgb: [f32; 3]) -> [f32; 3] {
    let max = rgb[0].max(rgb[1]).max(rgb[2]);
    let min = rgb[0].min(rgb[1]).min(rgb[2]);
    let range = max - min;

    if range <= 0.0 || max <= 0.0 {
        return [0.0, 0.0, max];
    }

    let hue = if max == rgb[0] {
        (rgb[1] - rgb[2]) / range
    } else if max == rgb[1] {
        2.0 + (rgb[2] - rgb[0]) / range
    } else {
        4.0 + (rgb[0] - rgb[1]) / range
    };

    [hue.rem_euclid(6.0), range / max, max]
}

fn hsv_to_rgb(hsv: [f32; 3]) -> [f32; 3] {
    let [hue, saturation, value] = hsv;
    if saturation <= 0.0 {
        return [value; 3];
    }

    let hue = hue.rem_euclid(6.0);
    let sector = hue.floor();
    let fraction = hue - sector;

    let p = value * (1.0 - saturation);
    let q = value * (1.0 - saturation * fraction);
    let t = value * (1.0 - saturation * (1.0 - fraction));

    match sector as i32 {
        0 => [value, t, p],
        1 => [q, value, p],
        2 => [p, value, t],
        3 => [p, q, value],
        4 => [t, p, value],
        _ => [value, p, q],
    }
}

#[derive(Debug, Clone)]
pub struct Table {
    value_encoding: ValueEncoding,
    hue_divisions: usize,
    sat_divisions: usize,
    val_divisions: usize,
    entries: Vec<[f32; 3]>,
}

impl Table {

    pub fn resolve(source: &HsvTable, mix: f32) -> Self {
        let entries = match (&source.second, mix) {
            (Some(second), mix) if mix > 0.0 => source
                .entries
                .iter()
                .zip(second.iter())
                .map(|(a, b)| {
                    [
                        a[0] + (b[0] - a[0]) * mix,
                        a[1] + (b[1] - a[1]) * mix,
                        a[2] + (b[2] - a[2]) * mix,
                    ]
                })
                .collect(),
            _ => source.entries.clone(),
        };

        Self {
            value_encoding: source.value_encoding,
            hue_divisions: source.hue_divisions,
            sat_divisions: source.sat_divisions,
            val_divisions: source.val_divisions,
            entries,
        }
    }

    fn at(&self, hue: usize, saturation: usize, value: usize) -> [f32; 3] {

        self.entries[(value * self.hue_divisions + hue) * self.sat_divisions + saturation]
    }

    fn lookup(&self, hsv: [f32; 3]) -> [f32; 3] {
        let hue = hsv[0] * self.hue_divisions as f32 / 6.0;
        let hue_floor = hue.floor();
        let hue_fraction = hue - hue_floor;
        let hue0 = (hue_floor as isize).rem_euclid(self.hue_divisions as isize) as usize;
        let hue1 = (hue0 + 1) % self.hue_divisions;

        let saturation = (hsv[1] * (self.sat_divisions - 1) as f32)
            .clamp(0.0, (self.sat_divisions - 1) as f32);
        let sat0 = saturation.floor() as usize;
        let sat1 = (sat0 + 1).min(self.sat_divisions - 1);
        let sat_fraction = saturation - sat0 as f32;

        let (val0, val1, val_fraction) = if self.val_divisions > 1 {
            let encoded = self.value_encoding.encode(hsv[2]);
            let value = (encoded * (self.val_divisions - 1) as f32)
                .clamp(0.0, (self.val_divisions - 1) as f32);
            let floor = value.floor() as usize;
            (floor, (floor + 1).min(self.val_divisions - 1), value - floor as f32)
        } else {
            (0, 0, 0.0)
        };

        let blend = |a: [f32; 3], b: [f32; 3], t: f32| {
            [
                a[0] + (b[0] - a[0]) * t,
                a[1] + (b[1] - a[1]) * t,
                a[2] + (b[2] - a[2]) * t,
            ]
        };

        let plane = |value: usize| {
            blend(
                blend(self.at(hue0, sat0, value), self.at(hue0, sat1, value), sat_fraction),
                blend(self.at(hue1, sat0, value), self.at(hue1, sat1, value), sat_fraction),
                hue_fraction,
            )
        };

        if self.val_divisions > 1 {
            blend(plane(val0), plane(val1), val_fraction)
        } else {
            plane(0)
        }
    }

    pub fn apply(&self, rgb: [f32; 3]) -> [f32; 3] {
        let mut hsv = rgb_to_hsv(rgb);
        if hsv[2] <= 0.0 {
            return rgb;
        }

        let [hue_shift, sat_scale, val_scale] = self.lookup(hsv);

        hsv[0] = (hsv[0] + hue_shift / 60.0).rem_euclid(6.0);
        hsv[1] = (hsv[1] * sat_scale).clamp(0.0, 1.0);
        hsv[2] *= val_scale;

        hsv_to_rgb(hsv)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct HsvSample {
    pub from: [f32; 3],
    pub to: [f32; 3],
}

pub fn fit_hue_sat_map(samples: &[HsvSample], divisions: [usize; 3], smoothness: f32) -> HsvTable {
    const IDENTITY_PULL: f64 = 0.02;
    let [hues, sats, vals] = divisions;
    let n = hues * sats * vals;
    let index = |h: usize, s: usize, v: usize| (v * hues + h) * sats + s;
    let encoding = ValueEncoding::Srgb;

    let weight = |sample: &HsvSample| (sample.from[1] / 0.2).min(1.0) as f64;
    let total: f64 = samples.iter().map(weight).sum();

    let scale = if total > 0.0 { n as f64 / total } else { 0.0 };

    let mut normal = vec![0.0f64; n * n];
    let mut rhs = vec![[0.0f64; 3]; n];

    for sample in samples {
        let w = weight(sample) * scale;
        if w <= 0.0 {
            continue;
        }
        let [h, s, v] = sample.from;

        let hue = h * hues as f32 / 6.0;
        let h0f = hue.floor();
        let (h0, hf) = ((h0f as isize).rem_euclid(hues as isize) as usize, (hue - h0f) as f64);
        let h1 = (h0 + 1) % hues;
        let sat = (s * (sats - 1) as f32).clamp(0.0, (sats - 1) as f32);
        let (s0, sf) = (sat.floor() as usize, (sat - sat.floor()) as f64);
        let s1 = (s0 + 1).min(sats - 1);
        let (v0, v1, vf) = if vals > 1 {
            let value = (encoding.encode(v) * (vals - 1) as f32).clamp(0.0, (vals - 1) as f32);
            let floor = value.floor() as usize;
            (floor, (floor + 1).min(vals - 1), (value - floor as f32) as f64)
        } else {
            (0, 0, 0.0)
        };

        let mut corners = [(0usize, 0.0f64); 8];
        let mut k = 0;
        for (vi, vw) in [(v0, 1.0 - vf), (v1, vf)] {
            for (hi, hw) in [(h0, 1.0 - hf), (h1, hf)] {
                for (si, sw) in [(s0, 1.0 - sf), (s1, sf)] {
                    corners[k] = (index(hi, si, vi), vw * hw * sw);
                    k += 1;
                }
            }
        }

        let shift = ((sample.to[0] - h + 3.0).rem_euclid(6.0) - 3.0) * 60.0;
        let target = [
            shift.clamp(-45.0, 45.0) as f64,
            (sample.to[1] / s.max(1e-3)).clamp(0.25, 4.0) as f64,
            (sample.to[2] / v.max(1e-6)).clamp(0.25, 4.0) as f64,
        ];

        for &(a, wa) in &corners {
            if wa == 0.0 {
                continue;
            }
            for c in 0..3 {
                rhs[a][c] += w * wa * target[c];
            }
            for &(b, wb) in &corners {
                normal[a * n + b] += w * wa * wb;
            }
        }
    }

    let identity = [0.0, 1.0, 1.0];
    let smooth = smoothness as f64;
    let mut tie = |a: usize, b: usize, strength: f64| {
        normal[a * n + a] += strength;
        normal[b * n + b] += strength;
        normal[a * n + b] -= strength;
        normal[b * n + a] -= strength;
    };
    for v in 0..vals {
        for h in 0..hues {
            for s in 0..sats {
                let here = index(h, s, v);
                if hues > 1 {
                    tie(here, index((h + 1) % hues, s, v), smooth);
                }
                if s + 1 < sats {
                    tie(here, index(h, s + 1, v), smooth);
                }
                if v + 1 < vals {
                    tie(here, index(h, s, v + 1), smooth);
                }
            }
        }
    }
    for here in 0..n {
        normal[here * n + here] += IDENTITY_PULL;
        for c in 0..3 {
            rhs[here][c] += IDENTITY_PULL * identity[c];
        }
    }

    let mut pinned = normal.clone();
    let mut pinned_rhs = rhs.clone();
    for v in 0..vals {
        for h in 0..hues {
            let here = index(h, 0, v);
            pinned[here * n + here] += 1e6;
            pinned_rhs[here][2] += 1e6;
        }
    }
    let free = solve_symmetric(normal, rhs, n);
    let value = solve_symmetric(pinned, pinned_rhs, n);

    HsvTable {
        value_encoding: encoding,
        hue_divisions: hues,
        sat_divisions: sats,
        val_divisions: vals,
        entries: free
            .into_iter()
            .zip(value)
            .map(|([h, s, _], [_, _, v])| [h as f32, (s as f32).max(0.0), (v as f32).max(0.01)])
            .collect(),
        second: None,
    }
}

fn solve_symmetric(mut a: Vec<f64>, mut b: Vec<[f64; 3]>, n: usize) -> Vec<[f64; 3]> {
    for j in 0..n {
        let mut diagonal = a[j * n + j];
        for k in 0..j {
            diagonal -= a[j * n + k] * a[j * n + k];
        }
        let diagonal = diagonal.max(1e-12).sqrt();
        a[j * n + j] = diagonal;
        for i in j + 1..n {
            let mut value = a[i * n + j];
            for k in 0..j {
                value -= a[i * n + k] * a[j * n + k];
            }
            a[i * n + j] = value / diagonal;
        }
    }
    for c in 0..3 {
        for i in 0..n {
            let mut value = b[i][c];
            for k in 0..i {
                value -= a[i * n + k] * b[k][c];
            }
            b[i][c] = value / a[i * n + i];
        }
        for i in (0..n).rev() {
            let mut value = b[i][c];
            for k in i + 1..n {
                value -= a[k * n + i] * b[k][c];
            }
            b[i][c] = value / a[i * n + i];
        }
    }
    b
}

pub struct Look {
    table: Table,
    into_prophoto: Matrix3,
    out_of_prophoto: Matrix3,
}

impl Look {
    pub fn new(table: Table) -> Self {
        Self {
            table,
            into_prophoto: multiply_matrix(&XYZ_D50_TO_PROPHOTO, &SRGB_TO_XYZ_D50),
            out_of_prophoto: multiply_matrix(&XYZ_D50_TO_SRGB, &PROPHOTO_TO_XYZ_D50),
        }
    }

    pub fn in_space(self, space: crate::space::ColourSpace) -> Self {
        Self {
            into_prophoto: multiply_matrix(&XYZ_D50_TO_PROPHOTO, &space.to_xyz()),
            out_of_prophoto: multiply_matrix(&space.from_xyz(), &PROPHOTO_TO_XYZ_D50),
            ..self
        }
    }

    pub fn apply(&self, pixel: [f32; 3]) -> [f32; 3] {
        let wide = multiply(&self.into_prophoto, pixel);
        let looked = self.table.apply(wide);
        let out = multiply(&self.out_of_prophoto, looked);
        [out[0].max(0.0), out[1].max(0.0), out[2].max(0.0)]
    }
}

#[derive(Debug, Clone)]
pub struct Rendering {

    pub to_prophoto: Matrix3,

    pub to_srgb: Matrix3,
    pub hue_sat_map: Option<Table>,
    pub look_table: Option<Table>,
}

fn illuminant_temperature(code: u16) -> f32 {
    match code {
        1 => 5500.0,
        2 => 4200.0,
        3 | 17 => 2856.0,
        18 => 4874.0,
        19 => 6774.0,
        20 => 5503.0,
        21 => 6504.0,
        22 => 7504.0,
        24 => 3200.0,
        _ => 5003.0,
    }
}

pub fn illuminant_mix(profile: &DngProfile, kelvin: f32) -> f32 {
    let (Some(first), Some(second)) = (profile.illuminant[0], profile.illuminant[1]) else {
        return 0.0;
    };
    if profile.color_matrix[1].is_none() && profile.forward_matrix[1].is_none() {
        return 0.0;
    }

    let t1 = illuminant_temperature(first);
    let t2 = illuminant_temperature(second);
    if (t1 - t2).abs() < 1.0 {
        return 0.0;
    }

    let inverse = 1.0 / kelvin.max(1.0);

    let first_weight = (inverse - 1.0 / t2) / (1.0 / t1 - 1.0 / t2);
    (1.0 - first_weight).clamp(0.0, 1.0)
}

pub fn mix_matrix(first: Option<Matrix3>, second: Option<Matrix3>, mix: f32) -> Option<Matrix3> {
    match (first, second) {
        (Some(a), Some(b)) => {
            let mut out = [[0.0f32; 3]; 3];
            for row in 0..3 {
                for column in 0..3 {
                    out[row][column] = a[row][column] + (b[row][column] - a[row][column]) * mix;
                }
            }
            Some(out)
        }
        (Some(only), None) | (None, Some(only)) => Some(only),
        (None, None) => None,
    }
}

impl Rendering {

    pub fn resolve(profile: &DngProfile, kelvin: f32) -> Option<Self> {
        let mix = illuminant_mix(profile, kelvin);
        let forward = mix_matrix(profile.forward_matrix[0], profile.forward_matrix[1], mix)?;

        Some(Self::new(
            forward,
            profile.hue_sat_map.as_ref().map(|table| Table::resolve(table, mix)),
            profile.look_table.as_ref().map(|table| Table::resolve(table, mix)),
        ))
    }

    pub fn new(forward: Matrix3, hue_sat_map: Option<Table>, look_table: Option<Table>) -> Self {
        Self {
            to_prophoto: multiply_matrix(&XYZ_D50_TO_PROPHOTO, &forward),
            to_srgb: multiply_matrix(&XYZ_D50_TO_SRGB, &PROPHOTO_TO_XYZ_D50),
            hue_sat_map,
            look_table,
        }
    }

    pub fn render(&self, camera: [f32; 3]) -> [f32; 3] {
        let mut rgb = multiply(&self.to_prophoto, camera);

        if let Some(map) = &self.hue_sat_map {
            rgb = map.apply(rgb);
        }
        if let Some(look) = &self.look_table {
            rgb = look.apply(rgb);
        }

        let out = multiply(&self.to_srgb, rgb);
        [out[0].max(0.0), out[1].max(0.0), out[2].max(0.0)]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn srgb_hues_move_when_seen_in_prophoto() {

        for (srgb, prophoto) in [(0.0, 9.5), (30.0, 26.2), (60.0, 68.1), (120.0, 103.1),
                                 (180.0, 189.5), (240.0, 248.1), (300.0, 283.1)] {
            let got = prophoto_hue_of_srgb(srgb);
            assert!(
                (got - prophoto).abs() < 1.0,
                "sRGB {srgb} should land near {prophoto} in ProPhoto, got {got}"
            );
        }

        assert!((prophoto_hue_of_srgb(360.0) - prophoto_hue_of_srgb(0.0)).abs() < 1e-3);
        for degrees in [0.0, 45.0, 137.0, 300.0, 359.0] {
            let hue = prophoto_hue_of_srgb(degrees);
            assert!((0.0..360.0).contains(&hue), "{degrees} produced {hue}");
        }
    }

    fn table(hue_divisions: usize, sat_divisions: usize, fill: [f32; 3]) -> Table {
        Table {
            value_encoding: ValueEncoding::Linear,
            hue_divisions,
            sat_divisions,
            val_divisions: 1,
            entries: vec![fill; hue_divisions * sat_divisions],
        }
    }

    #[test]
    fn hsv_round_trips() {
        for rgb in [
            [0.5, 0.2, 0.1],
            [0.1, 0.9, 0.3],
            [0.2, 0.2, 0.8],
            [0.4, 0.4, 0.4],
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
        ] {
            let back = hsv_to_rgb(rgb_to_hsv(rgb));
            for channel in 0..3 {
                assert!(
                    (back[channel] - rgb[channel]).abs() < 1e-5,
                    "{rgb:?} came back as {back:?}"
                );
            }
        }
    }

    #[test]
    fn an_identity_table_changes_nothing() {
        let identity = table(12, 6, [0.0, 1.0, 1.0]);
        let colour = [0.6, 0.25, 0.1];
        let out = identity.apply(colour);
        for channel in 0..3 {
            assert!((out[channel] - colour[channel]).abs() < 1e-5, "{out:?}");
        }
    }

    #[test]
    fn a_neutral_colour_is_never_moved() {

        let aggressive = table(12, 6, [90.0, 2.0, 1.0]);
        let grey = [0.42, 0.42, 0.42];
        let out = aggressive.apply(grey);
        for channel in 0..3 {
            assert!((out[channel] - grey[channel]).abs() < 1e-5, "grey moved to {out:?}");
        }
    }

    #[test]
    fn a_hue_shift_rotates_colour() {

        let rotate = table(12, 6, [120.0, 1.0, 1.0]);
        let out = rotate.apply([1.0, 0.0, 0.0]);
        assert!(out[1] > 0.9 && out[0] < 0.1 && out[2] < 0.1, "red did not become green: {out:?}");
    }

    #[test]
    fn scales_apply_to_saturation_and_value() {
        let desaturate = table(12, 6, [0.0, 0.5, 1.0]);
        let before = rgb_to_hsv([0.8, 0.2, 0.2]);
        let after = rgb_to_hsv(desaturate.apply([0.8, 0.2, 0.2]));
        assert!((after[1] - before[1] * 0.5).abs() < 1e-4, "{} vs {}", after[1], before[1]);

        let darken = table(12, 6, [0.0, 1.0, 0.5]);
        let after = rgb_to_hsv(darken.apply([0.8, 0.2, 0.2]));
        assert!((after[2] - before[2] * 0.5).abs() < 1e-4);
    }

    #[test]
    fn hue_interpolation_wraps_around_the_circle() {

        let mut wrapping = table(2, 2, [0.0, 1.0, 1.0]);
        wrapping.entries[2] = [60.0, 1.0, 1.0];
        wrapping.entries[3] = [60.0, 1.0, 1.0];

        let near_seam = wrapping.lookup([5.9, 1.0, 1.0])[0];
        let just_past = wrapping.lookup([0.1, 1.0, 1.0])[0];
        assert!(
            (near_seam - just_past).abs() < 15.0,
            "the seam is a discontinuity: {near_seam} vs {just_past}"
        );
    }

    #[test]
    fn resolve_blends_the_two_illuminants() {
        let source = HsvTable {
            value_encoding: ValueEncoding::Linear,
            hue_divisions: 2,
            sat_divisions: 2,
            val_divisions: 1,
            entries: vec![[0.0, 1.0, 1.0]; 4],
            second: Some(vec![[10.0, 2.0, 1.0]; 4]),
        };

        assert_eq!(Table::resolve(&source, 0.0).at(0, 0, 0), [0.0, 1.0, 1.0]);
        assert_eq!(Table::resolve(&source, 1.0).at(0, 0, 0), [10.0, 2.0, 1.0]);

        let half = Table::resolve(&source, 0.5).at(0, 0, 0);
        assert!((half[0] - 5.0).abs() < 1e-5 && (half[1] - 1.5).abs() < 1e-5, "{half:?}");
    }

    #[test]
    fn illuminant_blend_runs_the_right_way_and_clamps() {
        let profile = DngProfile {
            name: "test".into(),
            camera: Some("Test Camera".into()),
            color_matrix: [Some([[1.0; 3]; 3]), Some([[2.0; 3]; 3])],
            forward_matrix: [Some([[1.0; 3]; 3]), Some([[2.0; 3]; 3])],

            illuminant: [Some(17), Some(21)],
            hue_sat_map: None,
            look_table: None,
            tone_curve: None,
        };

        assert!(illuminant_mix(&profile, 2856.0) < 0.01, "warm end should be all illuminant 1");
        assert!(illuminant_mix(&profile, 6504.0) > 0.99, "cool end should be all illuminant 2");

        let middle = illuminant_mix(&profile, 4000.0);
        assert!((0.0..1.0).contains(&middle));
        assert_eq!(illuminant_mix(&profile, 1500.0), 0.0);
        assert_eq!(illuminant_mix(&profile, 20000.0), 1.0);

        let single = DngProfile { illuminant: [Some(21), None], ..profile.clone() };
        assert_eq!(illuminant_mix(&single, 3000.0), 0.0);
    }

    #[test]
    fn a_look_round_trips_through_prophoto() {

        let identity = Look::new(table(12, 6, [0.0, 1.0, 1.0]));

        for colour in [[0.5, 0.2, 0.1], [0.18, 0.18, 0.18], [0.02, 0.05, 0.9]] {
            let out = identity.apply(colour);
            for channel in 0..3 {
                assert!(
                    (out[channel] - colour[channel]).abs() < 1e-4,
                    "{colour:?} came back as {out:?}"
                );
            }
        }
    }

    #[test]
    fn a_look_sees_the_brightness_it_is_given() {

        let mut swings_in_the_dark = table(4, 4, [0.0, 1.0, 1.0]);
        swings_in_the_dark.val_divisions = 8;
        swings_in_the_dark.entries = vec![[0.0, 1.0, 1.0]; 4 * 4 * 8];

        for entry in swings_in_the_dark.entries.iter_mut().take(16) {
            *entry = [120.0, 1.0, 1.0];
        }
        let look = Look::new(swings_in_the_dark);

        let dark = look.apply([0.02, 0.004, 0.004]);
        let bright = look.apply([0.9, 0.18, 0.18]);

        let hue_shift = |a: [f32; 3], b: [f32; 3]| {
            let difference = (rgb_to_hsv(a)[0] - rgb_to_hsv(b)[0]).rem_euclid(6.0);
            difference.min(6.0 - difference)
        };
        assert!(
            hue_shift(dark, [0.02, 0.004, 0.004]) > 0.5,
            "the dark row should have moved this pixel's hue"
        );
        assert!(
            hue_shift(bright, [0.9, 0.18, 0.18]) < 0.2,
            "the bright row should have left it alone"
        );
    }

    #[test]
    fn a_fit_recovers_a_known_hue_shift() {

        let mut state = 0x2545_f491_4f6c_dd1du64;
        let mut random = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            (state >> 11) as f32 / (1u64 << 53) as f32
        };

        let samples: Vec<HsvSample> = (0..40_000)
            .map(|_| {
                let from = [random() * 6.0, 0.1 + random() * 0.8, 0.05 + random() * 0.9];

                let noise = (random() - 0.5) * 0.1;
                let to = [(from[0] + 0.2 + noise).rem_euclid(6.0), from[1] * 1.2, from[2]];
                HsvSample { from, to }
            })
            .collect();

        let fitted = fit_hue_sat_map(&samples, [18, 6, 3], 1.0);
        assert_eq!(fitted.len(), 18 * 6 * 3);
        let table = Table::resolve(&fitted, 0.0);

        for rgb in [[0.6, 0.35, 0.25], [0.2, 0.5, 0.35], [0.3, 0.35, 0.6]] {
            let before = rgb_to_hsv(rgb);
            let after = rgb_to_hsv(table.apply(rgb));
            let shift = ((after[0] - before[0] + 3.0).rem_euclid(6.0) - 3.0) * 60.0;
            assert!((shift - 12.0).abs() < 1.0, "{rgb:?} shifted {shift} degrees");
            assert!((after[1] / before[1] - 1.2).abs() < 0.03, "{rgb:?} saturation {}", after[1] / before[1]);
            assert!((after[2] / before[2] - 1.0).abs() < 0.02, "{rgb:?} value {}", after[2] / before[2]);
        }

        let grey = table.apply([0.3, 0.3, 0.3]);
        assert!(grey.iter().all(|c| (c - 0.3).abs() < 1e-4), "{grey:?}");
    }

    #[test]
    fn rendering_without_tables_is_just_the_matrices() {

        let identity = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];
        let rendering = Rendering::new(identity, None, None);

        let d50_white = [0.9642, 1.0, 0.8249];
        let out = rendering.render(d50_white);
        assert!(
            (out[0] - out[1]).abs() < 0.02 && (out[1] - out[2]).abs() < 0.02,
            "D50 white should render neutral, got {out:?}"
        );
    }
}
