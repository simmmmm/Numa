use std::io::Read;
use std::sync::OnceLock;

pub struct Place {
    pub name: String,
    pub latitude: f32,
    pub longitude: f32,
    pub region: String,
    pub country: String,
    pub people: u64,
}

fn places() -> &'static [Place] {
    static PLACES: OnceLock<Vec<Place>> = OnceLock::new();
    PLACES.get_or_init(|| {
        let mut text = String::new();
        let packed: &[u8] = include_bytes!("../../../data/places/places.tsv.gz");
        if let Err(err) = flate2::read::GzDecoder::new(packed).read_to_string(&mut text) {
            log::warn!("places: {err}");
        }
        text.lines()
            .filter_map(|line| {
                let mut parts = line.split('\t');
                Some(Place {
                    name: parts.next()?.to_string(),
                    latitude: parts.next()?.parse().ok()?,
                    longitude: parts.next()?.parse().ok()?,
                    region: parts.next()?.to_string(),
                    country: parts.next()?.to_string(),
                    people: parts.next()?.parse().ok()?,
                })
            })
            .collect()
    })
}

fn distance((a_lat, a_lon): (f32, f32), (b_lat, b_lon): (f32, f32)) -> f32 {
    let x = (b_lon - a_lon).to_radians() * ((a_lat + b_lat) / 2.0).to_radians().cos();
    let y = (b_lat - a_lat).to_radians();
    6371.0 * (x * x + y * y).sqrt()
}

pub fn nearest(at: (f32, f32)) -> Option<&'static Place> {
    let (mut any, mut town): (Option<(f32, &Place)>, Option<&Place>) = (None, None);
    for place in places() {
        let far = distance(at, (place.latitude, place.longitude));
        if any.is_none_or(|(best, _)| far < best) {
            any = Some((far, place));
        }

        if town.is_none() && place.people >= 15_000 && far <= 15.0 {
            town = Some(place);
        }
    }
    town.or(any.filter(|(far, _)| *far <= 50.0).map(|(_, place)| place))
}

fn joined(names: &[&str]) -> String {
    match names {
        [] => String::new(),
        [one] => one.to_string(),
        [rest @ .., last] => format!("{} & {last}", rest.join(", ")),
    }
}

pub fn from_positions(positions: &[(f64, f64)]) -> Vec<String> {
    let found: Vec<&Place> = positions.iter().filter_map(|&(lat, lon)| nearest((lat as f32, lon as f32))).collect();
    if found.is_empty() {
        return Vec::new();
    }
    let n = found.len() as f32;
    let tally = |key: fn(&Place) -> &str| {
        let mut counts: Vec<(&str, usize)> = Vec::new();
        for place in &found {
            match counts.iter_mut().find(|(name, _)| *name == key(place)) {
                Some((_, count)) => *count += 1,
                None => counts.push((key(place), 1)),
            }
        }
        counts.sort_by_key(|(_, count)| std::cmp::Reverse(*count));
        counts
    };
    let (towns, regions, countries) = (tally(|place| &place.name), tally(|place| &place.region), tally(|place| &place.country));
    let share = |count: usize| count as f32 / n;
    let mut names = Vec::new();
    if share(towns[0].1) >= 0.6 {
        names.push(towns[0].0.to_string());
    } else {
        let main: Vec<&str> = towns.iter().filter(|(_, count)| share(*count) >= 0.15).take(3).map(|(name, _)| *name).collect();
        if !main.is_empty() {
            names.push(joined(&main));
        }
    }
    if share(regions[0].1) >= 0.8 && !regions[0].0.is_empty() {
        names.push(regions[0].0.to_string());
    }
    let lands: Vec<&str> = countries.iter().filter(|(_, count)| share(*count) >= 0.2).take(3).map(|(name, _)| *name).collect();
    if !lands.is_empty() {
        names.push(joined(&lands));
    }
    names.dedup();
    names.truncate(3);
    names
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_trip_is_named_after_its_towns_and_country() {

        let japan = [(35.690, 139.692), (35.680, 139.760), (35.011, 135.768), (35.003, 135.778), (34.694, 135.502)];
        assert_eq!(from_positions(&japan), ["Tokyo, Kyoto & Osaka", "Japan"]);

        let maastricht = [(50.851, 5.691), (50.849, 5.688)];
        assert_eq!(from_positions(&maastricht), ["Maastricht", "Limburg", "The Netherlands"]);

        let patagonia = [(-49.331, -72.886), (-49.331, -72.886), (-50.338, -72.264), (-51.729, -72.506)];
        let named = from_positions(&patagonia);
        assert_eq!(named.last().map(String::as_str), Some("Argentina & Chile"), "{named:?}");

        assert!(from_positions(&[(0.0, -30.0)]).is_empty());
        assert!(from_positions(&[]).is_empty());
    }
}
