use std::path::Path;

pub fn taken(path: &Path) -> Option<i64> {
    stamp(path).taken
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Stamp {
    pub taken: Option<i64>,
    pub camera: String,
}

pub fn stamp(path: &Path) -> Stamp {
    let read = || from_head(path).or_else(|| from_container(path)).or_else(|| from_raf(path)).or_else(|| from_raw(path));
    match std::panic::catch_unwind(read).ok().flatten() {
        Some((date, camera)) => Stamp { taken: unix_seconds(&date), camera },
        None => Stamp::default(),
    }
}

pub fn camera_key(make: &str, model: &str, serial: &str) -> String {
    let clean = |text: &str| text.split_whitespace().collect::<Vec<_>>().join(" ");
    let (make, model) = (clean(make), clean(model));
    match make.is_empty() && model.is_empty() {
        true => String::new(),
        false => format!("{make}|{model}|{}", clean(serial)),
    }
}

fn ascii(exif: &::exif::Exif, number: u16) -> String {
    exif.fields()
        .find(|field| field.tag.number() == number)
        .and_then(|field| match &field.value {
            ::exif::Value::Ascii(parts) => parts.first().map(|bytes| String::from_utf8_lossy(bytes).trim_end_matches('\0').to_string()),
            _ => None,
        })
        .unwrap_or_default()
}

fn camera_in(exif: &::exif::Exif) -> String {
    let serial = Some(ascii(exif, 0xa431)).filter(|serial| !serial.trim().is_empty()).unwrap_or_else(|| ascii(exif, 0xc62f));
    camera_key(&ascii(exif, 0x010f), &ascii(exif, 0x0110), &serial)
}

const HEAD: u64 = 256 << 10;

fn from_head(path: &Path) -> Option<(String, String)> {
    use std::io::Read;
    let mut file = std::fs::File::open(path).ok()?;
    let mut head = vec![0u8; 12];
    file.read_exact(&mut head).ok()?;
    let cr3 = &head[4..12] == b"ftypcrx ";
    if !cr3 && !matches!(&head[..2], b"II" | b"MM") {
        return None;
    }
    file.take(HEAD).read_to_end(&mut head).ok()?;
    let tiff = if cr3 {

        let at = head.windows(4).position(|w| w == b"CMT2")?;
        let size = u32::from_be_bytes(head.get(at.checked_sub(4)?..at)?.try_into().ok()?) as usize;
        head.get(at + 4..(at - 4).checked_add(size)?)?.to_vec()
    } else {

        let magic: [u8; 2] = if head[0] == b'I' { [42, 0] } else { [0, 42] };
        head[2..4].copy_from_slice(&magic);
        head
    };
    date_in(&tiff).or_else(|| {

        let at = tiff.windows(6).position(|w| w == b"Exif\0\0")?;
        date_in(&tiff[at + 6..])
    })
}

fn date_in(tiff: &[u8]) -> Option<(String, String)> {
    let exif = directories(tiff)?;
    let field = exif.fields().find(|field| field.tag.number() == 0x9003)?;
    Some((field.display_value().to_string(), camera_in(&exif)))
}

fn directories(tiff: &[u8]) -> Option<::exif::Exif> {
    match ::exif::Reader::new().continue_on_error(true).read_raw(tiff.to_vec()) {
        Ok(exif) => Some(exif),
        Err(::exif::Error::PartialResult(partial)) => Some(partial.into_inner().0),
        Err(_) => None,
    }
}

pub fn rating(path: &Path) -> Option<u8> {
    use std::io::Read;
    let mut head = Vec::new();
    std::fs::File::open(path).ok()?.take(HEAD).read_to_end(&mut head).ok()?;
    let stars = xmp_rating(&head).filter(|stars| *stars > 0).or_else(|| exif_rating(&head))?;
    (1..=5).contains(&stars).then_some(stars as u8)
}

fn xmp_rating(head: &[u8]) -> Option<i64> {
    let at = head.windows(10).position(|w| w == b"xmp:Rating")? + 10;
    let value: String = head
        .get(at..(at + 8).min(head.len()))?
        .iter()
        .map(|&byte| byte as char)
        .skip_while(|c| matches!(c, '=' | '"' | '\'' | '>' | ' '))
        .take_while(|c| c.is_ascii_digit() || *c == '-')
        .collect();
    value.parse().ok()
}

fn exif_rating(head: &[u8]) -> Option<i64> {
    let tiff = if matches!(head.get(..2), Some(b"II" | b"MM")) {
        let mut tiff = head.to_vec();

        let magic: [u8; 2] = if tiff[0] == b'I' { [42, 0] } else { [0, 42] };
        tiff.get_mut(2..4)?.copy_from_slice(&magic);
        tiff
    } else {
        let at = head.windows(6).position(|w| w == b"Exif\0\0")? + 6;
        head[at..].to_vec()
    };
    let exif = directories(&tiff)?;
    let field = exif.fields().find(|field| field.tag.number() == 0x4746)?;
    field.value.get_uint(0).map(i64::from)
}

fn from_container(path: &Path) -> Option<(String, String)> {
    let file = std::fs::File::open(path).ok()?;
    let mut reader = std::io::BufReader::new(file);
    let exif = ::exif::Reader::new().read_from_container(&mut reader).ok()?;
    let field = exif.get_field(::exif::Tag::DateTimeOriginal, ::exif::In::PRIMARY)?;
    Some((field.display_value().to_string(), camera_in(&exif)))
}

fn from_raf(path: &Path) -> Option<(String, String)> {
    if !crate::raw::is_raf(path) {
        return None;
    }

    use byteorder::{BigEndian, ReadBytesExt};
    use rawler::formats::tiff::IFD;
    use std::io::{Seek, SeekFrom};

    const MAIN_TIFF_POINTER: u64 = 84;
    const TIFF_HEADER_SKIP: u32 = 12;
    const EXIF_IFD: u16 = 0x8769;
    const DATE_TIME_ORIGINAL: u16 = 0x9003;

    let file = std::fs::File::open(path).ok()?;
    let mut reader = std::io::BufReader::new(file);

    reader.seek(SeekFrom::Start(MAIN_TIFF_POINTER)).ok()?;
    let offset = reader.read_u32::<BigEndian>().ok()?;

    let ifd = IFD::new_root_with_correction(
        &mut reader,
        0,
        offset + TIFF_HEADER_SKIP,
        0,
        10,
        &[EXIF_IFD],
    )
    .ok()?;

    let text = |tag: u16| {
        ifd.get_entry_recursive(tag).and_then(|entry| entry.value.as_string().cloned()).unwrap_or_default().trim().to_string()
    };
    let date = Some(text(DATE_TIME_ORIGINAL)).filter(|date| !date.is_empty())?;

    Some((date, camera_key(&text(0x010f), &text(0x0110), &text(0xa431))))
}

fn from_raw(path: &Path) -> Option<(String, String)> {
    let source = rawler::rawsource::RawSource::new(path).ok()?;
    let decoder = rawler::get_decoder(&source).ok()?;
    let params = rawler::decoders::RawDecodeParams::default();
    let metadata = decoder.raw_metadata(&source, &params).ok()?;

    let serial = metadata.exif.serial_number.clone().unwrap_or_default();
    Some((metadata.exif.date_time_original.clone()?, camera_key(&metadata.make, &metadata.model, &serial)))
}

pub(crate) fn unix_seconds(text: &str) -> Option<i64> {
    let mut parts = text.split(|c: char| !c.is_ascii_digit()).filter(|p| !p.is_empty());
    let mut next = || parts.next()?.parse::<i64>().ok();
    let (year, month, day) = (next()?, next()?, next()?);
    let (hour, minute, second) = (next()?, next()?, next()?);

    if year < 1 || !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }

    Some(days_from_civil(year, month, day) * 86_400 + hour * 3600 + minute * 60 + second)
}

fn days_from_civil(year: i64, month: i64, day: i64) -> i64 {
    let year = year - i64::from(month <= 2);
    let era = year.div_euclid(400);
    let yoe = year - era * 400;
    let doy = (153 * (if month > 2 { month - 3 } else { month + 9 }) + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_date_string_becomes_the_second_it_names() {
        assert_eq!(unix_seconds("2022:07:17 12:14:32"), Some(19_190 * 86_400 + 44_072));
        assert_eq!(unix_seconds("1970:01:01 00:00:00"), Some(0));

        assert_eq!(unix_seconds("2022-07-17 12:14:32"), unix_seconds("2022:07:17 12:14:32"));

        assert_eq!(unix_seconds("0000:00:00 00:00:00"), None);
        assert_eq!(unix_seconds(""), None);
    }

    #[test]
    fn the_head_of_an_orf_says_when() {
        let mut tiff: Vec<u8> = b"IIRO".to_vec();
        tiff.extend(8u32.to_le_bytes());

        tiff.extend(1u16.to_le_bytes());
        tiff.extend([0x69, 0x87, 4, 0, 1, 0, 0, 0, 26, 0, 0, 0, 0, 0, 0, 0]);

        tiff.extend(1u16.to_le_bytes());
        tiff.extend([0x03, 0x90, 2, 0, 20, 0, 0, 0, 44, 0, 0, 0, 0, 0, 0, 0]);
        tiff.extend(b"2022:07:17 12:14:32\0");
        let path = std::env::temp_dir().join(format!("numa-head-{}.orf", std::process::id()));
        std::fs::write(&path, &tiff).unwrap();
        let found = from_head(&path);
        std::fs::remove_file(&path).unwrap();
        assert_eq!(unix_seconds(&found.expect("a date").0), unix_seconds("2022:07:17 12:14:32"));
    }

    #[test]
    fn a_body_is_its_make_model_and_serial() {
        assert_eq!(camera_key("FUJIFILM", "X-T5", "2D001165"), "FUJIFILM|X-T5|2D001165");
        assert_eq!(camera_key("SONY ", " ILCE-7M4", ""), "SONY|ILCE-7M4|");
        assert_eq!(camera_key("", "", "123"), "", "a file that names no camera");
    }

    #[test]
    fn the_cameras_stars_are_read_from_its_xmp() {
        assert_eq!(xmp_rating(b"<x xmp:Rating=\"4\" y>"), Some(4));
        assert_eq!(xmp_rating(b"..<xmp:Rating>3</xmp:Rating>.."), Some(3));
        assert_eq!(xmp_rating(b"<xmp:Rating>-1</xmp:Rating>"), Some(-1));
        assert_eq!(xmp_rating(b"no rating here"), None);
        let path = std::env::temp_dir().join(format!("numa-rating-{}.raf", std::process::id()));
        for (written, read) in [("5", Some(5)), ("0", None), ("-1", None)] {
            std::fs::write(&path, format!("FUJIFILMCCD-RAW <xmp:Rating>{written}</xmp:Rating>")).unwrap();
            assert_eq!(rating(&path), read, "{written}");
        }
        std::fs::remove_file(&path).unwrap();
    }

    #[test]
    #[ignore]
    fn the_head_agrees_with_the_whole_file() {
        let Some(list) = std::env::var_os("NUMA_DATE_FILES") else {
            println!("NUMA_DATE_FILES is not set");
            return;
        };
        let (mut same, mut left) = (0, 0);
        for line in std::fs::read_to_string(list).unwrap().lines().filter(|line| !line.is_empty()) {
            let path = Path::new(line);
            let whole = from_container(path).or_else(|| from_raf(path)).or_else(|| from_raw(path));
            match from_head(path) {
                Some(head) => {
                    assert_eq!(unix_seconds(&head.0), whole.and_then(|(date, _)| unix_seconds(&date)), "{line}");
                    same += 1;
                }
                None => left += 1,
            }
        }
        println!("{same} read the same from the head; {left} left to the whole file");
    }

    #[test]
    #[ignore]
    fn the_archives_own_photographs() {
        let Some(list) = std::env::var_os("NUMA_ARCHIVE_DATES") else {
            println!("NUMA_ARCHIVE_DATES is not set");
            return;
        };
        let text = std::fs::read_to_string(&list).expect("the list NUMA_ARCHIVE_DATES names");
        let expected: Vec<(&str, &str)> = text.lines().filter_map(|line| line.split_once('\t')).collect();
        assert!(!expected.is_empty(), "no `path<TAB>date` lines in the list");

        for (path, when) in expected {
            let path = Path::new(path);
            if !path.is_file() {
                println!("not here: {}", path.display());
                continue;
            }
            let started = std::time::Instant::now();
            let found = taken(path);
            println!("{:>7.1?}  {}", started.elapsed(), path.display());
            assert_eq!(found, unix_seconds(when), "{}", path.display());
        }
    }

    #[test]
    #[ignore]
    fn the_two_raf_paths_agree() {
        let Some(dir) = std::env::var_os("NUMA_RAF_DIR") else {
            println!("NUMA_RAF_DIR is not set");
            return;
        };
        let dir = Path::new(&dir);
        let Ok(entries) = std::fs::read_dir(dir) else {
            println!("not here: {}", dir.display());
            return;
        };

        let mut compared = 0;
        for path in entries.flatten().map(|entry| entry.path()) {
            if !crate::raw::is_raf(&path) {
                continue;
            }

            let (fast, slow) = (from_raf(&path).map(|(date, _)| date), from_raw(&path).map(|(date, _)| date));
            assert_eq!(fast, slow, "{}", path.display());
            assert!(fast.is_some(), "{} has a date and neither path found it", path.display());
            compared += 1;
            if compared == 20 {
                break;
            }
        }
        println!("{compared} RAFs read the same both ways");
        assert!(compared > 0, "no RAF to compare");
    }
}
