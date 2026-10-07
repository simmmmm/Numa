use std::io::Read;
use std::path::Path;

use ::exif::{Exif, Tag};

const HEAD: u64 = 256 << 10;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct Capture {

    pub taken: Option<i64>,

    pub zone: Option<i64>,
    pub model: String,

    pub serial: String,

    pub software: String,

    pub placed: bool,
}

impl Capture {

    pub fn utc(&self, local: i64) -> Option<i64> {
        Some(self.taken? - self.zone.unwrap_or(local))
    }
}

pub fn read(path: &Path) -> Capture {
    let mut capture = std::panic::catch_unwind(|| head(path)).ok().flatten().unwrap_or_default();
    if capture.taken.is_none() {

        capture.taken = crate::exif::taken(path);
    }
    capture
}

fn head(path: &Path) -> Option<Capture> {
    let mut bytes = Vec::new();
    std::fs::File::open(path).ok()?.take(HEAD).read_to_end(&mut bytes).ok()?;
    let mut capture = Capture::default();
    for tiff in tiffs(&bytes) {
        if let Some(exif) = parse(tiff) {
            fill(&mut capture, &exif);
        }
    }
    Some(capture)
}

fn tiffs(head: &[u8]) -> Vec<Vec<u8>> {
    let embedded = || {
        let at = head.windows(6).position(|w| w == b"Exif\0\0")? + 6;
        matches!(head.get(at..at + 2), Some(b"II" | b"MM")).then(|| head[at..].to_vec())
    };
    if head.get(4..12) == Some(b"ftypcrx ") {
        return [b"CMT1", b"CMT2", b"CMT4"]
            .iter()
            .filter_map(|name| {
                let at = head.windows(4).position(|w| w == *name)?;
                let size = u32::from_be_bytes(head.get(at.checked_sub(4)?..at)?.try_into().ok()?) as usize;
                Some(head.get(at + 4..(at - 4).checked_add(size)?)?.to_vec())
            })
            .collect();
    }
    if matches!(head.get(..2), Some(b"II" | b"MM")) && head.len() > 8 {
        let mut tiff = head.to_vec();
        let magic: [u8; 2] = if tiff[0] == b'I' { [42, 0] } else { [0, 42] };
        tiff[2..4].copy_from_slice(&magic);
        return std::iter::once(tiff).chain(embedded()).collect();
    }
    embedded().into_iter().collect()
}

fn parse(tiff: Vec<u8>) -> Option<Exif> {
    match ::exif::Reader::new().continue_on_error(true).read_raw(tiff) {
        Ok(exif) => Some(exif),
        Err(::exif::Error::PartialResult(partial)) => Some(partial.into_inner().0),
        Err(_) => None,
    }
}

fn fill(capture: &mut Capture, exif: &Exif) {
    let text = |value: &::exif::Value| match value {
        ::exif::Value::Ascii(parts) => parts.first().map(|bytes| String::from_utf8_lossy(bytes).trim_matches(char::from(0)).trim().to_string()),
        _ => None,
    };
    for field in exif.fields() {
        let set = |slot: &mut String| {
            if slot.is_empty() {
                *slot = text(&field.value).unwrap_or_default();
            }
        };
        match field.tag {
            Tag::DateTimeOriginal if capture.taken.is_none() => {
                capture.taken = text(&field.value).and_then(|date| crate::exif::unix_seconds(&date));
            }
            Tag::OffsetTimeOriginal if capture.zone.is_none() => capture.zone = text(&field.value).and_then(|zone| east(&zone)),
            Tag::Model => set(&mut capture.model),
            Tag::BodySerialNumber => set(&mut capture.serial),
            Tag::Software => set(&mut capture.software),

            Tag::GPSLatitude => {
                capture.placed |= matches!(&field.value, ::exif::Value::Rational(parts) if parts.iter().any(|part| part.num != 0));
            }
            _ => {}
        }
    }
}

pub fn east(text: &str) -> Option<i64> {
    let text = text.trim();
    let sign = match text.chars().next()? {
        '+' => 1,
        '-' => -1,
        _ => return None,
    };
    let digits: String = text[1..].chars().filter(char::is_ascii_digit).collect();
    let (hours, minutes) = (digits.get(..2)?.parse::<i64>().ok()?, digits.get(2..4).unwrap_or("0").parse::<i64>().ok()?);
    (hours <= 14 && minutes < 60).then_some(sign * (hours * 3600 + minutes * 60))
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use ::exif::{Field, In, Rational, Value};

    pub(crate) fn tiff(fields: &[Field]) -> Vec<u8> {
        let mut writer = ::exif::experimental::Writer::new();
        for field in fields {
            writer.push_field(field);
        }
        let mut bytes = std::io::Cursor::new(Vec::new());
        writer.write(&mut bytes, true).unwrap();
        bytes.into_inner()
    }

    pub(crate) fn ascii(tag: Tag, text: &str) -> Field {
        Field { tag, ifd_num: In::PRIMARY, value: Value::Ascii(vec![text.as_bytes().to_vec()]) }
    }

    #[test]
    fn a_zone_is_seconds_east() {
        assert_eq!(east("+09:00"), Some(9 * 3600));
        assert_eq!(east("-03:00"), Some(-3 * 3600));
        assert_eq!(east("+0530"), Some(5 * 3600 + 30 * 60));
        assert_eq!(east("   :  "), None);
        assert_eq!(east("+25:00"), None);
    }

    #[test]
    fn a_jpegs_head_says_when_and_with_what() {
        let fields = [
            ascii(Tag::Model, "X-T5"),
            ascii(Tag::Software, "Digital Camera X-T5 Ver4.31"),
            ascii(Tag::DateTimeOriginal, "2026:10:04 18:30:05"),
            ascii(Tag::OffsetTimeOriginal, "+02:00"),
            ascii(Tag::BodySerialNumber, "2D005233"),
            Field { tag: Tag::GPSLatitude, ifd_num: In::PRIMARY, value: Value::Rational(vec![Rational::from((52, 1)), Rational::from((0, 1)), Rational::from((0, 1))]) },
        ];
        let mut jpeg = vec![0xFF, 0xD8, 0xFF, 0xE1, 0, 0];
        jpeg.extend(b"Exif\0\0");
        jpeg.extend(tiff(&fields));
        let path = std::env::temp_dir().join(format!("numa-capture-{}.jpg", std::process::id()));
        std::fs::write(&path, &jpeg).unwrap();
        let read = read(&path);
        std::fs::remove_file(&path).unwrap();
        assert_eq!(read.model, "X-T5");
        assert_eq!(read.serial, "2D005233");
        assert_eq!(read.software, "Digital Camera X-T5 Ver4.31");
        assert_eq!(read.zone, Some(7200));
        assert!(read.placed);
        let wall = crate::exif::unix_seconds("2026:10:04 18:30:05").unwrap();
        assert_eq!(read.taken, Some(wall));
        assert_eq!(read.utc(0), Some(wall - 7200));

        assert_eq!(Capture { zone: None, ..read }.utc(3600), Some(wall - 3600));
    }
}
