use std::path::Path;

pub fn taken(path: &Path) -> Option<i64> {
    let read = || from_container(path).or_else(|| from_raf(path)).or_else(|| from_raw(path));
    unix_seconds(&std::panic::catch_unwind(read).ok().flatten()?)
}

fn from_container(path: &Path) -> Option<String> {
    let file = std::fs::File::open(path).ok()?;
    let mut reader = std::io::BufReader::new(file);
    let exif = ::exif::Reader::new().read_from_container(&mut reader).ok()?;
    let field = exif.get_field(::exif::Tag::DateTimeOriginal, ::exif::In::PRIMARY)?;
    Some(field.display_value().to_string())
}

fn from_raf(path: &Path) -> Option<String> {
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

    ifd.get_entry_recursive(DATE_TIME_ORIGINAL)
        .and_then(|entry| entry.value.as_string().cloned())
        .map(|date| date.trim().to_string())
        .filter(|date| !date.is_empty())
}

fn from_raw(path: &Path) -> Option<String> {
    let source = rawler::rawsource::RawSource::new(path).ok()?;
    let decoder = rawler::get_decoder(&source).ok()?;
    let params = rawler::decoders::RawDecodeParams::default();
    let metadata = decoder.raw_metadata(&source, &params).ok()?;
    metadata.exif.date_time_original.clone()
}

fn unix_seconds(text: &str) -> Option<i64> {
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
            let (fast, slow) = (from_raf(&path), from_raw(&path));
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
