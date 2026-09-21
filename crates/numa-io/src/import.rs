use std::collections::HashMap;
use std::ffi::OsString;
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use crate::catalog::{copy_whole, Copied, Library};
use crate::raw;

const GAP: i64 = 2 * 24 * 3600;

const NEAR: i64 = 2 * 24 * 3600;

const ENDS: u64 = 64 * 1024;

#[derive(Debug, Clone, PartialEq)]
pub struct Found {
    pub path: PathBuf,
    pub size: u64,

    pub when: i64,
}

pub fn scan(source: &Path) -> Vec<Found> {
    let camera = |dir: &Path| dir.join("DCIM").is_dir().then(|| dir.join("DCIM"));
    let roots: Vec<PathBuf> = match camera(source) {
        Some(dcim) => vec![dcim],
        None => {
            let slots: Vec<PathBuf> = std::fs::read_dir(source)
                .map(|entries| entries.flatten().filter_map(|entry| camera(&entry.path())).collect())
                .unwrap_or_default();
            if slots.is_empty() { vec![source.to_path_buf()] } else { slots }
        }
    };

    let mut found = Vec::new();
    let mut pending = roots;
    while let Some(dir) = pending.pop() {
        let Ok(entries) = std::fs::read_dir(&dir) else { continue };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(meta) = entry.metadata() else { continue };
            if meta.is_dir() {
                pending.push(path);
            } else if raw::is_supported(&path) {
                let when = meta
                    .modified()
                    .ok()
                    .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
                    .map_or(0, |since| since.as_secs() as i64);
                found.push(Found { path, size: meta.len(), when });
            }
        }
    }
    found.sort_by(|a, b| a.when.cmp(&b.when).then_with(|| a.path.cmp(&b.path)));
    found
}

pub fn by_name(paths: impl IntoIterator<Item = PathBuf>) -> HashMap<OsString, Vec<PathBuf>> {
    let mut index: HashMap<OsString, Vec<PathBuf>> = HashMap::new();
    for path in paths {
        if let Some(name) = path.file_name() {
            index.entry(name.to_os_string()).or_default().push(path);
        }
    }
    index
}

pub fn already(found: &[Found], known: &HashMap<OsString, Vec<PathBuf>>) -> Vec<Option<PathBuf>> {
    found
        .iter()
        .map(|photo| {
            let name = photo.path.file_name()?;
            known.get(name)?.iter().find(|there| same_file(&photo.path, photo.size, there)).cloned()
        })
        .collect()
}

fn same_file(one: &Path, size: u64, other: &Path) -> bool {
    if std::fs::metadata(other).map(|meta| meta.len()).ok() != Some(size) {
        return false;
    }
    let ends = |path: &Path| -> Option<Vec<u8>> {
        let mut file = std::fs::File::open(path).ok()?;
        let mut bytes = Vec::with_capacity(2 * ENDS as usize);
        (&mut file).take(ENDS).read_to_end(&mut bytes).ok()?;
        if size > ENDS {
            file.seek(SeekFrom::Start(size.saturating_sub(ENDS).max(ENDS))).ok()?;
            file.take(ENDS).read_to_end(&mut bytes).ok()?;
        }
        Some(bytes)
    };
    matches!((ends(one), ends(other)), (Some(a), Some(b)) if a == b)
}

#[derive(Debug, Clone, PartialEq)]
pub struct Shoot {
    pub first: i64,
    pub last: i64,
    pub photos: Vec<usize>,
}

pub fn shoots(found: &[Found], photos: &[usize]) -> Vec<Shoot> {
    let mut out: Vec<Shoot> = Vec::new();
    for &at in photos {
        let when = found[at].when;
        match out.last_mut() {
            Some(shoot) if when - shoot.last < GAP => {
                shoot.last = shoot.last.max(when);
                shoot.photos.push(at);
            }
            _ => out.push(Shoot { first: when, last: when, photos: vec![at] }),
        }
    }
    out
}

#[derive(Debug, Clone, PartialEq)]
pub enum Place {

    Library(Library),

    New(PathBuf),
}

pub type Span = (Library, Option<i64>, Option<i64>);

pub fn suggest(shoot: &Shoot, libraries: &[Span], offset: i64) -> Place {
    let beside = libraries
        .iter()
        .filter_map(|(library, first, last)| {
            let (first, last) = ((*first)?, (*last)?);
            let distance = (first - NEAR - shoot.last).max(shoot.first - last - NEAR);
            (distance <= 0).then_some((library, (shoot.first - first).abs()))
        })
        .min_by_key(|(_, apart)| *apart);
    match beside {
        Some((library, _)) => Place::Library(library.clone()),
        None => Place::New(home_for(shoot.first, libraries, offset).join(day(shoot.first, offset))),
    }
}

fn home_for(when: i64, libraries: &[Span], offset: i64) -> PathBuf {
    let parents: Vec<&Path> = libraries.iter().filter_map(|(library, _, _)| library.path.parent()).collect();
    let is_year = |dir: &&Path| {
        dir.file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.len() == 4 && name.chars().all(|c| c.is_ascii_digit()))
    };
    let years: Vec<&Path> = parents.iter().copied().filter(is_year).collect();
    if !years.is_empty() && years.len() * 2 >= parents.len() {
        if let Some(root) = most_common(years.iter().filter_map(|year| year.parent())) {
            return root.join(civil(when + offset).0.to_string());
        }
    }
    most_common(parents.iter().copied())
        .map(Path::to_path_buf)
        .or_else(dirs::picture_dir)
        .unwrap_or_else(|| dirs::home_dir().unwrap_or_default().join("Pictures"))
}

fn most_common<'a>(paths: impl Iterator<Item = &'a Path>) -> Option<&'a Path> {
    let mut counts: HashMap<&Path, usize> = HashMap::new();
    for path in paths {
        *counts.entry(path).or_default() += 1;
    }
    counts.into_iter().max_by(|a, b| a.1.cmp(&b.1).then_with(|| b.0.cmp(a.0))).map(|(path, _)| path)
}

pub fn day(when: i64, offset: i64) -> String {
    let (year, month, day) = civil(when + offset);
    format!("{year}-{month:02}-{day:02}")
}

fn civil(seconds: i64) -> (i64, u32, u32) {
    let days = seconds.div_euclid(86_400) + 719_468;
    let era = days.div_euclid(146_097);
    let of_era = days - era * 146_097;
    let year_of_era = (of_era - of_era / 1460 + of_era / 36_524 - of_era / 146_096) / 365;
    let of_year = of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted = (5 * of_year + 2) / 153;
    let day = (of_year - (153 * shifted + 2) / 5 + 1) as u32;
    let month = if shifted < 10 { shifted + 3 } else { shifted - 9 } as u32;
    (year_of_era + era * 400 + i64::from(month <= 2), month, day)
}

pub fn named(pattern: &str, photo: &Found, n: usize, offset: i64) -> OsString {
    let original = photo.path.file_name().unwrap_or_default().to_os_string();
    if pattern.trim().is_empty() {
        return original;
    }
    let stem = photo.path.file_stem().and_then(|stem| stem.to_str()).unwrap_or("photo");
    let local = photo.when + offset;
    let time = local.rem_euclid(86_400);
    let stem = pattern
        .replace("{name}", stem)
        .replace("{date}", &day(photo.when, offset))
        .replace("{time}", &format!("{:02}{:02}{:02}", time / 3600, time / 60 % 60, time % 60))
        .replace("{n}", &format!("{n:04}"))
        .replace(['/', '\0'], "-");
    match photo.path.extension() {
        Some(extension) => format!("{stem}.{}", extension.to_string_lossy()).into(),
        None => stem.into(),
    }
}

pub fn copy(
    photos: &[(&Found, OsString)],
    folder: &Path,
    stop: impl Fn() -> bool,
    mut done: impl FnMut(usize),
) -> Copied {
    let mut copied = Copied::default();
    for (at, (photo, name)) in photos.iter().enumerate() {
        if stop() {
            break;
        }
        let mut to = folder.join(name);
        let mut suffix = 2;
        while to.exists() {
            if same_file(&photo.path, photo.size, &to) {
                break;
            }
            let path = Path::new(name);
            let stem = path.file_stem().unwrap_or_default().to_string_lossy();
            let extension = path.extension().map(|ext| format!(".{}", ext.to_string_lossy())).unwrap_or_default();
            to = folder.join(format!("{stem}-{suffix}{extension}"));
            suffix += 1;
        }
        let shown = || to.file_name().unwrap_or_default().to_string_lossy().into_owned();
        if to.exists() {
            copied.existing.push(shown());
        } else {
            match copy_whole(&photo.path, &to) {
                Ok(()) => copied.photos += 1,
                Err(err) => {
                    log::warn!("{}: {err}", photo.path.display());
                    copied.failed.push(shown());
                }
            }
        }
        done(at + 1);
    }
    copied
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAY: i64 = 86_400;

    fn library(id: i64, path: &str) -> Library {
        Library { id, path: PathBuf::from(path), name: None }
    }

    fn at(when: i64) -> Found {
        Found { path: PathBuf::from(format!("DSCF{when}.RAF")), size: 1, when }
    }

    #[test]
    fn a_shoot_ends_where_the_camera_was_put_down_for_two_days() {

        let times = [0, 3600, DAY + 7200, 3 * DAY, 17 * DAY, 17 * DAY + 60];
        let found: Vec<Found> = times.iter().map(|&when| at(when)).collect();
        let all: Vec<usize> = (0..found.len()).collect();
        let shoots = shoots(&found, &all);
        assert_eq!(shoots.len(), 2);
        assert_eq!(shoots[0].photos, [0, 1, 2, 3]);
        assert_eq!((shoots[1].first, shoots[1].last), (17 * DAY, 17 * DAY + 60));
    }

    #[test]
    fn a_shoot_goes_to_the_library_its_dates_are_and_else_beside_the_others() {
        let september = 1_758_000_000;
        let libraries = vec![
            (library(7, "/Fotos/2025/Mallorca"), Some(september - 5 * DAY), Some(september)),
            (library(8, "/Fotos/2025/Noordwijk"), Some(september - 90 * DAY), Some(september - 80 * DAY)),
            (library(5, "/Fotos/2026/Japan"), Some(september + 200 * DAY), Some(september + 210 * DAY)),
        ];

        let flight = Shoot { first: september + DAY, last: september + DAY, photos: vec![0] };
        assert_eq!(suggest(&flight, &libraries, 0), Place::Library(libraries[0].0.clone()));

        let later = september + 370 * DAY;
        let new = Shoot { first: later, last: later + DAY, photos: vec![0] };
        assert_eq!(suggest(&new, &libraries, 0), Place::New(PathBuf::from(format!("/Fotos/2026/{}", day(later, 0)))));
    }

    #[test]
    fn with_no_year_folders_a_new_library_goes_beside_the_others() {
        let libraries = vec![(library(1, "/photos/Aad"), None, None), (library(2, "/photos/Eifel"), None, None)];
        let shoot = Shoot { first: 0, last: 0, photos: vec![0] };
        assert_eq!(suggest(&shoot, &libraries, 0), Place::New(PathBuf::from("/photos/1970-01-01")));
    }

    #[test]
    fn days_are_the_calendars() {
        assert_eq!(day(0, 0), "1970-01-01");
        assert_eq!(day(1_774_052_000, 0), "2026-03-21");

        assert_eq!(day(1_758_407_400, 0), "2025-09-20");
        assert_eq!(day(1_758_407_400, 2 * 3600), "2025-09-21");
        assert_eq!(day(951_782_400, 0), "2000-02-29");
    }

    #[test]
    fn a_pattern_renames_and_keeps_the_extension() {
        let photo = Found { path: PathBuf::from("/card/DSCF1267.RAF"), size: 1, when: 1_758_407_400 };
        assert_eq!(named("", &photo, 3, 7200), "DSCF1267.RAF");
        assert_eq!(named("{date}_{n}", &photo, 3, 7200), "2025-09-21_0003.RAF");
        assert_eq!(named("{date} {time} {name}", &photo, 1, 7200), "2025-09-21 003000 DSCF1267.RAF");
        assert_eq!(named("a/b", &photo, 1, 0), "a-b.RAF");
    }

    #[test]
    fn what_a_library_has_is_known_and_a_clash_of_names_keeps_both() {
        let dir = std::env::temp_dir().join(format!("numa-import-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let (card, library) = (dir.join("card/DCIM/100_FUJI"), dir.join("Mallorca"));
        std::fs::create_dir_all(&card).unwrap();
        std::fs::create_dir_all(&library).unwrap();
        let frame = |seed: u8| -> Vec<u8> { (0..200_000u32).map(|i| (i as u8).wrapping_mul(seed)).collect() };
        std::fs::write(card.join("DSCF0001.RAF"), frame(3)).unwrap();
        std::fs::write(card.join("DSCF0002.RAF"), frame(5)).unwrap();
        std::fs::write(card.join("notes.txt"), b"not a photograph").unwrap();

        std::fs::write(library.join("DSCF0001.RAF"), frame(3)).unwrap();
        std::fs::write(library.join("DSCF0002.RAF"), frame(7)).unwrap();

        let found = scan(&dir.join("card"));
        assert_eq!(found.len(), 2, "the camera's two photographs and nothing else");
        let known = by_name(vec![library.join("DSCF0001.RAF"), library.join("DSCF0002.RAF")]);
        let there = already(&found, &known);
        let names: Vec<_> = found.iter().map(|photo| photo.path.file_name().unwrap().to_owned()).collect();
        let first = names.iter().position(|name| name == "DSCF0001.RAF").unwrap();
        assert_eq!(there[first], Some(library.join("DSCF0001.RAF")));
        assert_eq!(there[1 - first], None, "the same name is not the same photograph");

        let new = &found[1 - first];
        let copied = copy(&[(new, new.path.file_name().unwrap().to_owned())], &library, || false, |_| {});
        assert_eq!(copied.photos, 1);
        assert_eq!(std::fs::read(library.join("DSCF0002-2.RAF")).unwrap(), frame(5), "copied beside, not over");
        assert_eq!(std::fs::read(library.join("DSCF0002.RAF")).unwrap(), frame(7), "the other camera's is untouched");

        let time = |path: &Path| std::fs::metadata(path).unwrap().modified().unwrap();
        assert_eq!(time(&library.join("DSCF0002-2.RAF")), time(&new.path));
        assert!(!library.join("DSCF0002-2.RAF.part").exists());

        let again = copy(&[(new, new.path.file_name().unwrap().to_owned())], &library, || false, |_| {});
        assert_eq!((again.photos, again.existing.len()), (0, 1), "found as the -2 already there, and no -3 made");
        assert!(!library.join("DSCF0002-3.RAF").exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
