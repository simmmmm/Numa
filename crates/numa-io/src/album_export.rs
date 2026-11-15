use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};

use crate::catalog::{joined, AlbumChapter, AlbumDay, AlbumKind, Catalog, Filter};
use crate::chapters::day_of;

pub struct BookSize {
    pub name: &'static str,
    pub long_mm: f32,
}

pub const BOOK_SIZES: [BookSize; 4] = [
    BookSize { name: "21 × 28 cm", long_mm: 280.0 },
    BookSize { name: "30 × 30 cm", long_mm: 300.0 },
    BookSize { name: "21 × 21 cm", long_mm: 210.0 },
    BookSize { name: "33 × 28 cm", long_mm: 330.2 },
];

pub fn long_edge(mm: f32) -> u32 {
    (mm / 25.4 * 300.0).round() as u32
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Folders {

    ByPlaceAndDay,
    ByChapterAndDay,
    ByDay,
    OneFolder,
}

impl Folders {

    pub fn first_for(kind: AlbumKind, chapters: usize) -> Self {
        match kind {
            AlbumKind::Trip => Folders::ByPlaceAndDay,
            _ if chapters > 0 => Folders::ByChapterAndDay,
            _ => Folders::ByDay,
        }
    }
}

pub struct Shot {
    pub id: i64,

    pub taken: i64,
    pub path: PathBuf,
}

pub struct Album<'a> {
    pub name: &'a str,
    pub text: &'a str,
    pub chapters: &'a [AlbumChapter],

    pub chapter_of: &'a HashMap<i64, String>,
    pub shots: &'a [Shot],
    pub days: &'a [AlbumDay],
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlannedFile {
    pub id: i64,
    pub source: PathBuf,
    pub folder: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlannedText {
    pub folder: String,
    pub name: String,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Plan {

    pub root: String,

    pub folders: Vec<String>,
    pub files: Vec<PlannedFile>,
    pub texts: Vec<PlannedText>,
}

const SEPARATOR: &str = " · ";

pub fn clean(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| match c {
            '{' => '(',
            '}' => ')',
            c if "/\\:*?\"<>|".contains(c) || c.is_control() => '-',
            c => c,
        })
        .collect();
    cleaned.trim().trim_start_matches('.').trim().to_string()
}

pub fn day_name(day: i64) -> String {
    const WEEKDAYS: [&str; 7] = ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"];
    const MONTHS: [&str; 12] = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];
    let (_, month, date) = crate::import::civil(day * 86_400);

    format!("{} {date} {}", WEEKDAYS[(day + 3).rem_euclid(7) as usize], MONTHS[(month as usize).clamp(1, 12) - 1])
}

fn numbered(number: usize, name: &str) -> String {
    format!("{number:02}{SEPARATOR}{name}")
}

struct Day {
    day: i64,
    shots: Vec<usize>,
    text: String,
}

fn days_of(album: &Album, shots: &[usize], reports: &[&AlbumDay]) -> Vec<Day> {
    let mut by_day: BTreeMap<i64, Day> = BTreeMap::new();
    for &at in shots {
        let day = day_of(album.shots[at].taken);
        by_day.entry(day).or_insert_with(|| Day { day, shots: Vec::new(), text: String::new() }).shots.push(at);
    }
    for report in reports {
        let day = by_day.entry(report.day).or_insert_with(|| Day { day: report.day, shots: Vec::new(), text: String::new() });
        day.text = joined(&day.text, &report.text);
    }
    let mut days: Vec<Day> = by_day.into_values().collect();
    for day in &mut days {
        day.shots.sort_by_key(|&at| (album.shots[at].taken, album.shots[at].id));
    }
    days
}

fn report_in<'a>(day: &'a AlbumDay, keys: &[&str]) -> Option<&'a str> {
    day.chapter.as_deref().filter(|key| keys.contains(key))
}

fn headed(head: &str, words: &str) -> String {
    match words.trim().is_empty() {
        true => String::new(),
        false => format!("**{head}**\n\n{words}"),
    }
}

struct Planner<'a> {
    album: &'a Album<'a>,
    texts: bool,
    width: usize,
    plan: Plan,
}

impl Planner<'_> {
    fn folder(&mut self, folder: String, text: &str) {
        if self.texts && !text.trim().is_empty() {
            let name = folder.rsplit('/').next().unwrap_or(&folder);
            self.plan.texts.push(PlannedText { folder: folder.clone(), name: format!("{name}.md"), text: text.to_string() });
        }
        self.plan.folders.push(folder);
    }

    fn files(&mut self, folder: &str, shots: &[usize]) {
        for &at in shots {
            let shot = &self.album.shots[at];
            let stem = shot.path.file_stem().map(|stem| stem.to_string_lossy().to_string()).unwrap_or_default();
            let name = format!("{:0width$}{SEPARATOR}{}", self.plan.files.len() + 1, clean(&stem), width = self.width);
            self.plan.files.push(PlannedFile { id: shot.id, source: shot.path.clone(), folder: folder.to_string(), name });
        }
    }

    fn days(&mut self, parent: &str, days: &[Day]) {
        for (at, day) in days.iter().enumerate() {
            let name = numbered(at + 1, &day_name(day.day));
            let folder = if parent.is_empty() { name } else { format!("{parent}/{name}") };
            self.folder(folder.clone(), &day.text);
            self.files(&folder, &day.shots);
        }
    }
}

pub fn plan(album: &Album, folders: Folders, texts: bool) -> Plan {
    let root = Some(clean(album.name)).filter(|name| !name.is_empty()).unwrap_or_else(|| "Album".into());
    let width = album.shots.len().to_string().len().max(4);
    let mut planner = Planner { album, texts, width, plan: Plan { root: root.clone(), folders: Vec::new(), files: Vec::new(), texts: Vec::new() } };
    let keys: Vec<&str> = album.chapters.iter().map(|chapter| chapter.key.as_str()).collect();
    let chapter_of = |at: &usize| album.chapter_of.get(&album.shots[*at].id).map(String::as_str).filter(|key| keys.contains(key));
    let mut everything: Vec<usize> = (0..album.shots.len()).collect();
    everything.sort_by_key(|&at| (album.shots[at].taken, album.shots[at].id));
    let mut top = album.text.to_string();
    match folders {
        Folders::ByPlaceAndDay | Folders::ByChapterAndDay => {
            for (at, chapter) in album.chapters.iter().enumerate() {
                let shots: Vec<usize> = everything.iter().copied().filter(|shot| chapter_of(shot) == Some(chapter.key.as_str())).collect();
                let reports: Vec<&AlbumDay> = album.days.iter().filter(|day| report_in(day, &keys) == Some(chapter.key.as_str())).collect();
                if shots.is_empty() && reports.is_empty() && (!texts || chapter.words.trim().is_empty()) {
                    continue;
                }
                let folder = numbered(at + 1, &clean(&chapter.name));
                planner.folder(folder.clone(), &chapter.words);
                planner.days(&folder, &days_of(album, &shots, &reports));
            }
            let loose: Vec<usize> = everything.iter().copied().filter(|shot| chapter_of(shot).is_none()).collect();
            let reports: Vec<&AlbumDay> = album.days.iter().filter(|day| report_in(day, &keys).is_none()).collect();
            let days = days_of(album, &loose, &reports);

            let first = album.chapters.len();
            for (at, day) in days.iter().enumerate() {
                let folder = numbered(first + at + 1, &day_name(day.day));
                planner.folder(folder.clone(), &day.text);
                planner.files(&folder, &day.shots);
            }
        }
        Folders::ByDay => {

            let mut reports: Vec<&AlbumDay> = album.days.iter().collect();
            reports.sort_by_key(|day| (day.day, report_in(day, &keys).and_then(|key| keys.iter().position(|have| *have == key)).unwrap_or(keys.len())));
            for (at, chapter) in album.chapters.iter().enumerate() {
                top = joined(&top, &headed(&numbered(at + 1, &chapter.name), &chapter.words));
            }
            planner.days("", &days_of(album, &everything, &reports));
        }
        Folders::OneFolder => {
            for (at, chapter) in album.chapters.iter().enumerate() {
                top = joined(&top, &headed(&numbered(at + 1, &chapter.name), &chapter.words));
                for day in album.days.iter().filter(|day| report_in(day, &keys) == Some(chapter.key.as_str())) {
                    top = joined(&top, &headed(&day_name(day.day), &day.text));
                }
            }
            for day in album.days.iter().filter(|day| report_in(day, &keys).is_none()) {
                top = joined(&top, &headed(&day_name(day.day), &day.text));
            }
            planner.files("", &everything);
        }
    }
    if texts && !top.trim().is_empty() {
        planner.plan.texts.insert(0, PlannedText { folder: String::new(), name: format!("{root}.md"), text: top });
    }
    planner.plan
}

pub fn make_folders(plan: &Plan, into: &Path) -> Result<PathBuf, String> {
    let mut root = into.join(&plan.root);
    for n in 2.. {
        if !root.exists() {
            break;
        }
        root = into.join(format!("{} {n}", plan.root));
    }
    std::fs::create_dir_all(&root).map_err(|err| format!("{}: {err}", root.display()))?;
    for folder in &plan.folders {
        std::fs::create_dir_all(root.join(folder)).map_err(|err| format!("{folder}: {err}"))?;
    }
    for text in &plan.texts {
        let path = root.join(&text.folder).join(&text.name);
        std::fs::write(&path, format!("{}\n", text.text)).map_err(|err| format!("{}: {err}", path.display()))?;
    }
    Ok(root)
}

impl Catalog {

    pub fn album_export_plan(&self, key: &str, folders: Folders, texts: bool) -> Result<Plan, String> {
        let name = self.albums()?.into_iter().find(|(have, _)| have == key).map(|(_, name)| name).ok_or("There is no such album.")?;
        let photos = self.photos_everywhere(&Filter { album: Some(key.to_string()), ..Filter::default() })?;
        let shots: Vec<Shot> = photos
            .into_iter()
            .filter(|photo| !crate::video::is_video(&photo.path))
            .map(|photo| Shot { id: photo.id, taken: photo.taken.unwrap_or(photo.mtime), path: photo.path })
            .collect();
        let (chapters, chapter_of, days, text) = (self.chapters(key)?, self.chapter_of(key)?, self.album_days(key)?, self.album_text(key)?);
        Ok(plan(&Album { name: &name, text: &text, chapters: &chapters, chapter_of: &chapter_of, shots: &shots, days: &days }, folders, texts))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FRIDAY: i64 = 19_664 * 86_400 + 12 * 3600;
    const DAY: i64 = 86_400;

    fn chapter(key: &str, name: &str, words: &str) -> AlbumChapter {
        AlbumChapter { key: key.into(), album: "a".into(), name: name.into(), words: words.into(), place: None, days: None }
    }

    fn report(chapter: Option<&str>, day: i64, text: &str) -> AlbumDay {
        AlbumDay { chapter: chapter.map(str::to_string), day, text: text.into(), changed: 0 }
    }

    struct Trip {
        chapters: Vec<AlbumChapter>,
        of: HashMap<i64, String>,
        shots: Vec<Shot>,
        days: Vec<AlbumDay>,
    }

    fn trip() -> Trip {
        let shot = |id: i64, taken: i64, name: &str| Shot { id, taken, path: PathBuf::from(format!("/cards/{name}")) };
        let shots = vec![shot(3, FRIDAY + DAY, "DSC_4130.NEF"), shot(1, FRIDAY, "DSC_4127.NEF"), shot(2, FRIDAY + 60, "DSC_4128.JPG"), shot(4, FRIDAY + 3 * DAY, "IMG_{1}.HEIC"), shot(5, FRIDAY + 4 * DAY, "DSC_4200.NEF")];
        let of = HashMap::from([(1, "t".to_string()), (2, "t".into()), (3, "t".into()), (4, "r".into())]);
        let chapters = vec![chapter("t", "Tallinn", "The old town."), chapter("x", "Tartu", ""), chapter("r", "Riga", "")];
        let friday = FRIDAY / DAY;
        let days = vec![report(Some("t"), friday, "Rain, then *sun*."), report(Some("r"), friday + 2, "A day on the train."), report(Some("gone"), friday + 4, "From a chapter that went.")];
        Trip { chapters, of, shots, days }
    }

    fn planned(trip: &Trip, folders: Folders, texts: bool) -> Plan {
        plan(&Album { name: "Baltic: Trip", text: "Ten days, **north**.", chapters: &trip.chapters, chapter_of: &trip.of, shots: &trip.shots, days: &trip.days }, folders, texts)
    }

    fn files(plan: &Plan) -> Vec<String> {
        plan.files.iter().map(|file| if file.folder.is_empty() { file.name.clone() } else { format!("{}/{}", file.folder, file.name) }).collect()
    }

    fn texts(plan: &Plan) -> Vec<(String, String)> {
        plan.texts.iter().map(|text| (format!("{}/{}", text.folder, text.name), text.text.clone())).collect()
    }

    #[test]
    fn by_place_and_day_numbers_places_days_and_files_as_the_card_names_them() {
        let trip = trip();
        let plan = planned(&trip, Folders::ByPlaceAndDay, true);
        assert_eq!(plan.root, "Baltic- Trip");
        assert_eq!(
            files(&plan),
            [
                "01 · Tallinn/01 · Friday 3 November/0001 · DSC_4127",
                "01 · Tallinn/01 · Friday 3 November/0002 · DSC_4128",
                "01 · Tallinn/02 · Saturday 4 November/0003 · DSC_4130",
                "03 · Riga/02 · Monday 6 November/0004 · IMG_(1)",
                "04 · Tuesday 7 November/0005 · DSC_4200",
            ]
        );
        assert_eq!(plan.folders, ["01 · Tallinn", "01 · Tallinn/01 · Friday 3 November", "01 · Tallinn/02 · Saturday 4 November", "03 · Riga", "03 · Riga/01 · Sunday 5 November", "03 · Riga/02 · Monday 6 November", "04 · Tuesday 7 November"], "Tartu, empty, is left out and keeps its number; the written day has its folder");
        assert_eq!(
            texts(&plan),
            [
                ("/Baltic- Trip.md".to_string(), "Ten days, **north**.".to_string()),
                ("01 · Tallinn/01 · Tallinn.md".into(), "The old town.".into()),
                ("01 · Tallinn/01 · Friday 3 November/01 · Friday 3 November.md".into(), "Rain, then *sun*.".into()),
                ("03 · Riga/01 · Sunday 5 November/01 · Sunday 5 November.md".into(), "A day on the train.".into()),
                ("04 · Tuesday 7 November/04 · Tuesday 7 November.md".into(), "From a chapter that went.".into()),
            ]
        );
        let none = planned(&trip, Folders::ByChapterAndDay, false);
        assert!(none.texts.is_empty() && files(&none) == files(&plan), "the switch off writes no text and moves nothing");
    }

    #[test]
    fn by_day_and_one_folder_keep_every_word() {
        let trip = trip();
        let by_day = planned(&trip, Folders::ByDay, true);
        assert_eq!(by_day.folders, ["01 · Friday 3 November", "02 · Saturday 4 November", "03 · Sunday 5 November", "04 · Monday 6 November", "05 · Tuesday 7 November"]);
        assert_eq!(files(&by_day)[3], "04 · Monday 6 November/0004 · IMG_(1)");
        assert_eq!(texts(&by_day)[0].1, "Ten days, **north**.\n\n**01 · Tallinn**\n\nThe old town.", "the places' words on top");
        let one = planned(&trip, Folders::OneFolder, true);
        assert!(one.folders.is_empty());
        assert_eq!(files(&one), ["0001 · DSC_4127", "0002 · DSC_4128", "0003 · DSC_4130", "0004 · IMG_(1)", "0005 · DSC_4200"]);
        let all = &one.texts[0].text;
        for words in ["Ten days", "The old town.", "**Friday 3 November**\n\nRain", "A day on the train.", "From a chapter that went."] {
            assert!(all.contains(words), "{words:?} in {all:?}");
        }
        assert_eq!(one.texts.len(), 1);
        assert_eq!(crate::markdown::clean(all), *all, "the one text is in the subset too");
    }

    #[test]
    fn an_album_without_chapters_is_its_days_and_the_folders_are_made_beside_what_is_there() {
        let trip = trip();
        let shots = &trip.shots[..3];
        let plain = plan(&Album { name: ".hidden", text: "", chapters: &[], chapter_of: &HashMap::new(), shots, days: &[] }, Folders::first_for(AlbumKind::Album, 0), true);
        assert_eq!((plain.root.as_str(), plain.folders.len(), plain.texts.len()), ("hidden", 2, 0));
        assert_eq!(Folders::first_for(AlbumKind::Trip, 0), Folders::ByPlaceAndDay);
        assert_eq!(Folders::first_for(AlbumKind::Wedding, 6), Folders::ByChapterAndDay);
        let dir = std::env::temp_dir().join(format!("numa-album-export-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let plan = planned(&trip, Folders::ByPlaceAndDay, true);
        let first = make_folders(&plan, &dir).unwrap();
        let second = make_folders(&plan, &dir).unwrap();
        assert_eq!((first.file_name().unwrap(), second.file_name().unwrap()), (std::ffi::OsStr::new("Baltic- Trip"), std::ffi::OsStr::new("Baltic- Trip 2")));
        assert_eq!(std::fs::read_to_string(first.join("01 · Tallinn/01 · Tallinn.md")).unwrap(), "The old town.\n");
        assert!(first.join("03 · Riga/01 · Sunday 5 November").is_dir());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_book_page_is_its_long_edge_at_three_hundred_dpi() {
        assert_eq!(BOOK_SIZES.iter().map(|size| long_edge(size.long_mm)).collect::<Vec<_>>(), [3307, 3543, 2480, 3900]);
        assert_eq!(day_name(0), "Thursday 1 January");
    }
}
