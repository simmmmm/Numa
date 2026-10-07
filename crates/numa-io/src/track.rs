use std::path::Path;

use serde_json::Value;

use crate::capture::Capture;

pub const GAP: i64 = 15 * 60;

const STILL: f64 = 250.0;

const EDGE: i64 = 2 * 60;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Fix {

    pub at: i64,
    pub lat: f64,
    pub lon: f64,
}

#[derive(Debug, Clone, Default)]
pub struct Track {

    pub name: String,

    pub fixes: Vec<Fix>,
}

pub fn read(path: &Path) -> Result<Track, String> {
    let text = std::fs::read_to_string(path).map_err(|err| format!("could not read it: {err}"))?;
    let (name, mut fixes) = match text.trim_start().starts_with('<') {
        true => gpx(&text)?,
        false => (None, timeline(&text)?),
    };
    fixes.retain(|fix| fix.lat.abs() <= 90.0 && fix.lon.abs() <= 180.0 && (fix.lat, fix.lon) != (0.0, 0.0));
    fixes.sort_by_key(|fix| fix.at);
    fixes.dedup_by_key(|fix| fix.at);
    if fixes.is_empty() {
        return Err("it has no times and places in it".into());
    }
    let name = name.unwrap_or_else(|| path.file_stem().unwrap_or_default().to_string_lossy().into_owned());
    Ok(Track { name, fixes })
}

pub fn place(fixes: &[Fix], at: i64) -> Option<(f64, f64)> {
    let next = fixes.partition_point(|fix| fix.at < at);
    let before = next.checked_sub(1).map(|index| fixes[index]);
    match (before, fixes.get(next).copied()) {
        (_, Some(after)) if after.at == at => Some((after.lat, after.lon)),
        (Some(before), Some(after)) => {
            if after.at - before.at > GAP && metres(&before, &after) > STILL {
                return None;
            }
            let t = (at - before.at) as f64 / (after.at - before.at) as f64;

            Some((before.lat + (after.lat - before.lat) * t, before.lon + (after.lon - before.lon) * t))
        }
        (Some(last), None) if at - last.at <= EDGE => Some((last.lat, last.lon)),
        (None, Some(first)) if first.at - at <= EDGE => Some((first.lat, first.lon)),
        _ => None,
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Placing {

    pub placed: Vec<(usize, f64, f64)>,

    pub outside: usize,

    pub by_camera: usize,
}

pub fn place_all(track: &Track, captures: &[Capture], local: i64) -> Placing {
    let mut placing = Placing::default();
    for (index, capture) in captures.iter().enumerate() {
        if capture.placed {
            placing.by_camera += 1;
            continue;
        }
        match capture.utc(local).and_then(|at| place(&track.fixes, at)) {
            Some((lat, lon)) => placing.placed.push((index, lat, lon)),
            None => placing.outside += 1,
        }
    }
    placing
}

fn metres(one: &Fix, other: &Fix) -> f64 {
    const EARTH: f64 = 6_371_000.0;
    let mid = ((one.lat + other.lat) / 2.0).to_radians();
    let x = (other.lon - one.lon).to_radians() * mid.cos();
    let y = (other.lat - one.lat).to_radians();
    EARTH * x.hypot(y)
}

pub fn iso(text: &str) -> Option<i64> {
    let text = text.trim();
    let (wall, east) = match text.strip_suffix('Z') {
        Some(wall) => (wall, 0),
        None => {
            let time = text.find('T')?;
            match text[time..].rfind(['+', '-']) {
                Some(at) => (&text[..time + at], crate::capture::east(&text[time + at..])?),
                None => (text, 0),
            }
        }
    };

    Some(crate::exif::unix_seconds(wall)? - east)
}

fn gpx(text: &str) -> Result<(Option<String>, Vec<Fix>), String> {
    let document = roxmltree::Document::parse(text).map_err(|err| format!("it is not a GPX file: {err}"))?;
    let name = document
        .descendants()
        .find(|node| node.has_tag_name("name") && node.parent().is_some_and(|parent| parent.has_tag_name("trk") || parent.has_tag_name("metadata")))
        .and_then(|node| node.text())
        .map(|name| name.trim().to_string())
        .filter(|name| !name.is_empty());
    let fixes = document
        .descendants()
        .filter(|node| matches!(node.tag_name().name(), "trkpt" | "rtept" | "wpt"))
        .filter_map(|node| {
            let at = node.children().find(|child| child.has_tag_name("time"))?.text().and_then(iso)?;
            Some(Fix { at, lat: node.attribute("lat")?.trim().parse().ok()?, lon: node.attribute("lon")?.trim().parse().ok()? })
        })
        .collect();
    Ok((name, fixes))
}

fn timeline(text: &str) -> Result<Vec<Fix>, String> {
    let json: Value = serde_json::from_str(text).map_err(|err| format!("it is neither GPX nor a Timeline export: {err}"))?;
    let mut fixes = Vec::new();
    walk(&json, None, &mut fixes);
    Ok(fixes)
}

fn walk(value: &Value, start: Option<i64>, fixes: &mut Vec<Fix>) {
    match value {
        Value::Array(items) => items.iter().for_each(|item| walk(item, start, fixes)),
        Value::Object(map) => {
            let get = |key: &str| map.get(key);
            let time = |key: &str| get(key).and_then(Value::as_str).and_then(iso);

            let duration = get("duration");
            let from = time("startTime").or_else(|| duration.and_then(|d| d.get("startTimestamp")).and_then(Value::as_str).and_then(iso));
            let to = time("endTime").or_else(|| duration.and_then(|d| d.get("endTimestamp")).and_then(Value::as_str).and_then(iso));
            let mut push = |at: Option<i64>, place: Option<(f64, f64)>| {
                if let (Some(at), Some((lat, lon))) = (at, place) {
                    fixes.push(Fix { at, lat, lon });
                }
            };

            let own = time("timestamp")
                .or_else(|| time("time"))
                .or_else(|| get("timestampMs").and_then(|ms| ms.as_str().and_then(|text| text.parse().ok()).or_else(|| ms.as_i64())).map(|ms: i64| ms / 1000))
                .or_else(|| {
                    let minutes = get("durationMinutesOffsetFromStartTime")?;
                    let minutes = minutes.as_str().and_then(|text| text.parse::<f64>().ok()).or_else(|| minutes.as_f64())?;
                    Some(start? + (minutes * 60.0) as i64)
                });
            push(own, e7(map).or_else(|| ["point", "LatLng", "latLng"].iter().find_map(|key| get(key).and_then(lat_lng))));

            let visit = get("visit")
                .and_then(|visit| visit.get("topCandidate")?.get("placeLocation"))
                .and_then(lat_lng)
                .or_else(|| get("location").and_then(Value::as_object).and_then(e7));
            if let Some(place) = visit {
                push(from, Some(place));
                push(to, Some(place));
            }

            let ends = get("activity").unwrap_or(value);
            let end = |keys: [&str; 2]| keys.iter().find_map(|key| ends.get(key)).and_then(|end| lat_lng(end).or_else(|| end.as_object().and_then(e7)));
            push(from, end(["start", "startLocation"]));
            push(to, end(["end", "endLocation"]));
            for (key, child) in map {
                if key != "visit" {
                    walk(child, from.or(start), fixes);
                }
            }
        }
        _ => {}
    }
}

fn e7(map: &serde_json::Map<String, Value>) -> Option<(f64, f64)> {
    let number = |keys: [&str; 2]| keys.iter().find_map(|key| map.get(*key)?.as_f64());
    Some((number(["latitudeE7", "latE7"])? / 1e7, number(["longitudeE7", "lngE7"])? / 1e7))
}

fn lat_lng(value: &Value) -> Option<(f64, f64)> {
    let text = match value {
        Value::String(text) => text.as_str(),
        Value::Object(map) => map.get("latLng").or_else(|| map.get("LatLng"))?.as_str()?,
        _ => return None,
    };
    let (lat, lon) = text.trim().trim_start_matches("geo:").split_once(',')?;
    let number = |text: &str| text.trim().trim_end_matches('°').trim().parse::<f64>().ok();
    Some((number(lat)?, number(lon)?))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fix(at: i64, lat: f64, lon: f64) -> Fix {
        Fix { at, lat, lon }
    }

    #[test]
    fn iso_times_are_utc() {
        let noon = crate::exif::unix_seconds("2026-10-04 12:00:00").unwrap();
        assert_eq!(iso("2026-10-04T12:00:00Z"), Some(noon));
        assert_eq!(iso("2026-10-04T12:00:00.250Z"), Some(noon));
        assert_eq!(iso("2026-10-04T14:00:00+02:00"), Some(noon));
        assert_eq!(iso("2026-10-04T09:00:00.000-03:00"), Some(noon));
        assert_eq!(iso("yesterday"), None);
    }

    #[test]
    fn a_moment_is_placed_between_its_fixes() {
        let fixes = [fix(0, 52.0, 4.0), fix(100, 52.001, 4.001), fix(100 + 3600, 52.5, 4.5), fix(100 + 7200, 52.5005, 4.5)];
        assert_eq!(place(&fixes, 0), Some((52.0, 4.0)));
        let (lat, lon) = place(&fixes, 50).unwrap();
        assert!((lat - 52.0005).abs() < 1e-9 && (lon - 4.0005).abs() < 1e-9);
        assert_eq!(place(&fixes, 1000), None, "an hour between two places");
        assert!(place(&fixes, 100 + 5000).is_some(), "an hour in one place");
        assert_eq!(place(&fixes, -60), Some((52.0, 4.0)));
        assert_eq!(place(&fixes, -600), None);
        assert_eq!(place(&fixes, 100 + 7200 + 60), Some((52.5005, 4.5)));
        assert_eq!(place(&[], 0), None);
    }

    #[test]
    fn a_gpx_track_is_read_with_its_name() {
        let text = r#"<?xml version="1.0"?>
            <gpx version="1.1" xmlns="http://www.topografix.com/GPX/1/1">
              <metadata><time>2026-10-04T08:00:00Z</time></metadata>
              <trk><name>Phone track</name><trkseg>
                <trkpt lat="35.0116" lon="135.7681"><ele>40</ele><time>2026-10-04T08:00:00Z</time></trkpt>
                <trkpt lat="35.0120" lon="135.7690"><time>2026-10-04T08:00:10Z</time></trkpt>
                <trkpt lat="35.0" lon="135.0"></trkpt>
              </trkseg></trk>
            </gpx>"#;
        let (name, fixes) = gpx(text).unwrap();
        assert_eq!(name.as_deref(), Some("Phone track"));
        assert_eq!(fixes.len(), 2);
        assert_eq!(fixes[1], fix(iso("2026-10-04T08:00:10Z").unwrap(), 35.0120, 135.7690));
    }

    #[test]
    fn records_json_is_read() {
        let json = r#"{"locations": [
            {"latitudeE7": 520000000, "longitudeE7": 40000000, "timestamp": "2026-10-04T08:00:00.000Z"},
            {"latitudeE7": 520010000, "longitudeE7": 40010000, "timestampMs": "1791100000000"}]}"#;
        let fixes = timeline(json).unwrap();
        assert_eq!(fixes, vec![fix(iso("2026-10-04T08:00:00Z").unwrap(), 52.0, 4.0), fix(1_791_100_000, 52.001, 4.001)]);
    }

    #[test]
    fn the_new_timeline_is_read() {
        let json = r#"{"semanticSegments": [
            {"startTime": "2026-10-04T10:00:00.000+02:00", "endTime": "2026-10-04T12:00:00.000+02:00",
             "visit": {"topCandidate": {"placeLocation": {"latLng": "52.3676°, 4.9041°"}}}},
            {"startTime": "2026-10-04T12:00:00.000+02:00", "endTime": "2026-10-04T13:00:00.000+02:00",
             "timelinePath": [{"point": "52.37°, 4.91°", "time": "2026-10-04T12:30:00.000+02:00"}]}],
          "rawSignals": [{"position": {"LatLng": "52.38°, 4.92°", "timestamp": "2026-10-04T13:05:00.000+02:00"}}]}"#;
        let mut fixes = timeline(json).unwrap();
        fixes.sort_by_key(|fix| fix.at);
        let at = |text: &str| iso(text).unwrap();
        assert_eq!(
            fixes,
            vec![
                fix(at("2026-10-04T08:00:00Z"), 52.3676, 4.9041),
                fix(at("2026-10-04T10:00:00Z"), 52.3676, 4.9041),
                fix(at("2026-10-04T10:30:00Z"), 52.37, 4.91),
                fix(at("2026-10-04T11:05:00Z"), 52.38, 4.92),
            ]
        );
    }

    #[test]
    fn the_ios_timeline_is_read() {
        let json = r#"[
            {"startTime": "2026-10-04T10:00:00.000+02:00", "endTime": "2026-10-04T11:00:00.000+02:00",
             "visit": {"topCandidate": {"placeLocation": "geo:52.3676,4.9041"}}},
            {"startTime": "2026-10-04T11:00:00.000+02:00", "endTime": "2026-10-04T11:30:00.000+02:00",
             "activity": {"start": "geo:52.3676,4.9041", "end": "geo:52.40,4.95"}},
            {"startTime": "2026-10-04T11:00:00.000+02:00", "endTime": "2026-10-04T13:00:00.000+02:00",
             "timelinePath": [{"point": "geo:52.39,4.93", "durationMinutesOffsetFromStartTime": "15"}]}]"#;
        let fixes = timeline(json).unwrap();
        let at = |text: &str| iso(text).unwrap();
        assert!(fixes.contains(&fix(at("2026-10-04T09:30:00Z"), 52.40, 4.95)));
        assert!(fixes.contains(&fix(at("2026-10-04T09:15:00Z"), 52.39, 4.93)));
        assert!(fixes.contains(&fix(at("2026-10-04T08:00:00Z"), 52.3676, 4.9041)));
    }

    #[test]
    fn the_old_semantic_history_is_read() {
        let json = r#"{"timelineObjects": [
            {"placeVisit": {"location": {"latitudeE7": 523676000, "longitudeE7": 49041000},
                            "duration": {"startTimestamp": "2026-10-04T08:00:00Z", "endTimestamp": "2026-10-04T09:00:00Z"}}},
            {"activitySegment": {"startLocation": {"latitudeE7": 523676000, "longitudeE7": 49041000},
                                 "endLocation": {"latitudeE7": 524000000, "longitudeE7": 49500000},
                                 "duration": {"startTimestamp": "2026-10-04T09:00:00Z", "endTimestamp": "2026-10-04T09:20:00Z"}}}]}"#;
        let fixes = timeline(json).unwrap();
        let at = |text: &str| iso(text).unwrap();
        assert!(fixes.contains(&fix(at("2026-10-04T08:00:00Z"), 52.3676, 4.9041)));
        assert!(fixes.contains(&fix(at("2026-10-04T09:00:00Z"), 52.3676, 4.9041)));
        assert!(fixes.contains(&fix(at("2026-10-04T09:20:00Z"), 52.4, 4.95)));
    }

    #[test]
    fn photographs_are_placed_by_their_zone() {
        let wall = crate::exif::unix_seconds("2026-10-04 10:00:00").unwrap();
        let track = Track { name: "t".into(), fixes: vec![fix(wall - 7200, 52.0, 4.0), fix(wall - 7200 + 60, 52.0, 4.0)] };
        let shot = |zone, placed| Capture { taken: Some(wall), zone, placed, ..Capture::default() };
        let placing = place_all(&track, &[shot(Some(7200), false), shot(Some(7200), true), shot(None, false), Capture::default()], 0);
        assert_eq!(placing.placed, vec![(0, 52.0, 4.0)]);
        assert_eq!((placing.by_camera, placing.outside), (1, 2));

        assert_eq!(place_all(&track, &[shot(None, false)], 7200).placed.len(), 1);
    }
}
