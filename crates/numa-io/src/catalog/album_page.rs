use std::collections::{HashMap, HashSet};

use super::{AlbumChapter, AlbumKind, Catalog, ChapterPlace, Filter, Flag, Photo};
use crate::album_page::{is_grid, lay_out, town_at, Facts, Layout};
use crate::chapters::{Shot, Town};

pub const NO_CHAPTER: &str = "none";

#[derive(Debug, Clone, Copy, Default)]
pub struct PageAsked {
    pub picks_only: bool,
    pub min_rating: u8,
    pub outside: bool,
}

pub struct AlbumPage {
    pub key: String,
    pub name: String,
    pub kind: AlbumKind,

    pub grid: bool,

    pub photos: Vec<Photo>,
    pub captions: HashMap<i64, String>,
    pub chapters: Vec<AlbumChapter>,

    pub layout: Layout,

    pub unread: Vec<i64>,
}

fn when(photo: &Photo) -> i64 {
    photo.taken.unwrap_or(photo.mtime)
}

type Memo = HashMap<(u32, u32), Option<Town>>;

impl Catalog {

    pub(super) fn chapter_clause(&self, album: &str, chapter: &str) -> Result<String, String> {
        let plain = |key: &str| !key.is_empty() && key.chars().all(|c| c.is_ascii_alphanumeric());
        let keys: Vec<String> = self.chapters(album)?.into_iter().map(|chapter| chapter.key).filter(|key| plain(key)).collect();
        if chapter == NO_CHAPTER {

            return Ok(match keys.is_empty() {
                true => String::new(),
                false => format!(" AND p.id NOT IN (SELECT photo_id FROM chapter_photos WHERE chapter IN ('{}'))", keys.join("','")),
            });
        }
        Ok(match keys.iter().any(|key| key == chapter) {
            true => format!(" AND p.id IN (SELECT photo_id FROM chapter_photos WHERE chapter = '{chapter}')"),
            false => " AND 0".to_string(),
        })
    }

    fn album_photos(&self, key: &str, asked: &PageAsked) -> Result<Vec<Photo>, String> {
        let filter = Filter { album: Some(key.to_string()), flag: asked.picks_only.then_some(Flag::Picked), min_rating: asked.min_rating, ..Filter::default() };
        let mut photos = self.photos_everywhere(&filter)?;
        photos.sort_by_key(|photo| (when(photo), photo.id));
        Ok(photos)
    }

    fn outside_album(&self, inside: &HashSet<i64>) -> Result<Vec<Photo>, String> {
        let mut outside = Vec::new();
        for library in self.libraries()? {

            let Ok(photos) = self.photos(library.id, &Filter::default()) else { continue };
            outside.extend(photos.into_iter().filter(|photo| !inside.contains(&photo.id)));
        }
        Ok(outside)
    }

    fn shots(&self, photos: &[Photo], captions: &HashMap<i64, String>, memo: &mut Memo, unread: &mut HashSet<i64>) -> Result<Vec<Shot>, String> {
        let (spots, waiting) = self.where_taken(photos)?;
        unread.extend(waiting.iter().map(|photo| photo.id));
        let shot = |photo: &Photo| Shot {
            id: photo.id,
            taken: when(photo),
            town: town_at(spots.get(&photo.id).map(|&(latitude, longitude)| (latitude as f32, longitude as f32)), memo),
            captioned: captions.contains_key(&photo.id),
            picked: photo.flag == Flag::Picked,
        };
        Ok(photos.iter().map(shot).collect())
    }

    pub fn album_page(&self, key: &str, asked: &PageAsked) -> Result<AlbumPage, String> {
        let name = self.albums()?.into_iter().find(|(have, _)| have == key).map(|(_, name)| name).ok_or("There is no such album.")?;
        let (kind, chapters) = (self.album_kind(key)?, self.chapters(key)?);
        let filtered = asked.picks_only || asked.min_rating > 0;
        let photos = self.album_photos(key, asked)?;

        let all: Vec<Photo> = if filtered { self.album_photos(key, &PageAsked::default())? } else { Vec::new() };
        let whole = if filtered { &all } else { &photos };
        let times: Vec<i64> = whole.iter().map(when).collect();
        if is_grid(kind, chapters.len(), &times) {
            let layout = Layout { count: photos.len(), picks: 0, first: times.iter().min().copied(), last: times.iter().max().copied(), chapters: Vec::new(), days: Vec::new(), loose: 0, places: Vec::new() };
            return Ok(AlbumPage { key: key.to_string(), name, kind, grid: true, photos, captions: HashMap::new(), chapters, layout, unread: Vec::new() });
        }
        let ids: Vec<i64> = photos.iter().map(|photo| photo.id).collect();
        let captions = self.captions_of(&ids)?;
        let chapter_of = self.chapter_of(key)?;

        let outside = match asked.outside && !filtered && chapters.iter().any(|chapter| chapter.place.is_some()) {
            true => self.outside_album(&ids.iter().copied().collect())?,
            false => Vec::new(),
        };
        let (mut memo, mut unread) = (Memo::new(), HashSet::new());
        let shots = self.shots(&photos, &captions, &mut memo, &mut unread)?;
        let others = self.shots(&outside, &HashMap::new(), &mut memo, &mut unread)?;
        let layout = lay_out(&Facts { shots: &shots, chapter_of: &chapter_of, chapters: &chapters, outside: &others, unread: &unread, trip: kind == AlbumKind::Trip, may_change: true, filtered });
        let mut unread: Vec<i64> = unread.into_iter().collect();
        unread.sort_unstable();
        Ok(AlbumPage { key: key.to_string(), name, kind, grid: false, photos, captions, chapters, layout, unread })
    }

    pub fn take_places(&self, key: &str) -> Result<Vec<AlbumChapter>, String> {
        let page = self.album_page(key, &PageAsked::default())?;
        let mut made = Vec::new();
        for offered in &page.layout.places {
            let place = ChapterPlace { name: offered.town.name.clone(), latitude: offered.town.latitude, longitude: offered.town.longitude };
            let chapter = self.add_chapter(key, &offered.town.name, Some(place), Some((offered.first_day, offered.last_day)))?;
            self.put_in_chapter(key, Some(&chapter.key), &offered.ids)?;
            made.push(chapter);
        }
        Ok(made)
    }

    fn chapter_named(&self, chapter: &str) -> Result<AlbumChapter, String> {
        self.chapter(chapter)?.ok_or_else(|| "There is no such chapter.".to_string())
    }

    pub fn add_to_chapter(&self, chapter: &str, photo_ids: &[i64]) -> Result<Vec<i64>, String> {
        let chapter = self.chapter_named(chapter)?;
        let before: HashSet<i64> = self.album_photos(&chapter.album, &PageAsked::default())?.iter().map(|photo| photo.id).collect();
        let mut new: Vec<i64> = photo_ids.iter().copied().filter(|id| !before.contains(id)).collect();
        new.sort_unstable();
        new.dedup();
        self.put_in_chapter(&chapter.album, Some(&chapter.key), photo_ids)?;
        Ok(new)
    }

    pub fn take_from_chapter(&self, chapter: &str, photo_ids: &[i64]) -> Result<(), String> {
        let chapter = self.chapter_named(chapter)?;
        let there = self.chapter_of(&chapter.album)?;
        let out: Vec<i64> = photo_ids.iter().copied().filter(|id| there.get(id) == Some(&chapter.key)).collect();
        self.put_in_chapter(&chapter.album, None, &out)
    }
}

#[cfg(test)]
mod tests {
    use super::super::Library;
    use super::*;
    use std::path::{Path, PathBuf};
    use std::time::{Duration, UNIX_EPOCH};

    const DAY: i64 = 86_400;

    const START: i64 = 1_775_304_000;
    const TOKYO: (f64, f64) = (35.6895, 139.6917);
    const KYOTO: (f64, f64) = (35.0211, 135.7538);

    fn frame(folder: &Path, name: &str, at: i64) {
        std::fs::create_dir_all(folder).unwrap();
        let file = std::fs::File::create(folder.join(name)).unwrap();
        file.set_modified(UNIX_EPOCH + Duration::from_secs(at as u64)).unwrap();
    }

    fn trip(tag: &str) -> (Catalog, Vec<i64>, PathBuf) {
        let dir = std::env::temp_dir().join(format!("numa-album-page-{}-{tag}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        for (folder, name, at) in [("One", "T1.JPG", START), ("One", "T2.JPG", START + 600), ("Two", "T3.JPG", START + DAY), ("Two", "K1.JPG", START + 7 * DAY), ("One", "K2.JPG", START + 7 * DAY + 300), ("One", "N1.JPG", START + 7 * DAY + 900)] {
            frame(&dir.join(folder), name, at);
        }
        let catalog = Catalog::open(&dir.join("home.db")).unwrap();
        let libraries: Vec<Library> = ["One", "Two"].iter().map(|folder| catalog.add_library(&dir.join(folder)).unwrap()).collect();
        let mut ids = HashMap::new();
        for library in &libraries {
            catalog.sync_library(library).unwrap();
            for photo in catalog.photos(library.id, &Filter::default()).unwrap() {
                ids.insert(photo.path.file_name().unwrap().to_string_lossy().to_string(), (photo.id, photo.mtime));
            }
        }
        let at = |name: &str| ids[name];
        let placed = |names: &[&str], (latitude, longitude): (f64, f64)| -> Vec<(i64, i64, Option<(f64, f64)>)> { names.iter().map(|name| (at(name).0, at(name).1, Some((latitude, longitude)))).collect() };
        catalog.keep_where(&placed(&["T1.JPG", "T2.JPG", "T3.JPG"], TOKYO)).unwrap();
        catalog.keep_where(&placed(&["K1.JPG", "K2.JPG"], KYOTO)).unwrap();
        catalog.keep_where(&[(at("N1.JPG").0, at("N1.JPG").1, None)]).unwrap();
        let order = ["T1.JPG", "T2.JPG", "T3.JPG", "K1.JPG", "K2.JPG", "N1.JPG"].iter().map(|name| at(name).0).collect();
        (catalog, order, dir)
    }

    fn asked() -> PageAsked {
        PageAsked { outside: true, ..PageAsked::default() }
    }

    #[test]
    fn a_short_album_reads_by_day_with_its_towns_and_a_long_plain_one_is_the_grid() {
        let (catalog, ids, dir) = trip("days");
        let album = catalog.create_album("Japan").unwrap();
        catalog.set_in_album(&album, &ids, true).unwrap();
        catalog.set_caption(ids[0], "From the hotel roof.").unwrap();
        let page = catalog.album_page(&album, &asked()).unwrap();
        assert!(!page.grid && page.unread.is_empty());
        assert_eq!((page.name.as_str(), page.kind, page.layout.count, page.layout.loose), ("Japan", AlbumKind::Album, 6, 6));
        let days: Vec<(usize, Vec<String>)> = page.layout.days.iter().map(|day| (day.count, day.towns.clone())).collect();
        assert_eq!(days, [(2, vec!["Tokyo".to_string()]), (1, vec!["Tokyo".to_string()]), (3, vec!["Kyoto".to_string()])]);
        assert_eq!(page.captions.get(&ids[0]).map(String::as_str), Some("From the hotel roof."));
        assert_eq!(page.photos.iter().map(|photo| photo.id).collect::<Vec<_>>(), ids, "the oldest first, across libraries");
        assert!(catalog.album_page("no-such-album", &asked()).is_err());

        frame(&dir.join("One"), "LATER.JPG", START + 90 * DAY);
        let one = catalog.libraries().unwrap().into_iter().find(|library| library.path.ends_with("One")).unwrap();
        catalog.sync_library(&one).unwrap();
        let later = catalog.photos(one.id, &Filter::default()).unwrap().into_iter().find(|photo| photo.path.ends_with("LATER.JPG")).unwrap().id;
        catalog.set_in_album(&album, &[later], true).unwrap();
        let long = catalog.album_page(&album, &asked()).unwrap();
        assert!(long.grid && long.layout.days.is_empty() && long.layout.count == 7);

        catalog.add_chapter(&album, "Tokyo", None, None).unwrap();
        assert!(!catalog.album_page(&album, &asked()).unwrap().grid);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_trip_offers_its_places_and_taking_them_makes_the_chapters() {
        let (catalog, ids, dir) = trip("places");
        let album = catalog.create_album("Japan").unwrap();
        catalog.set_in_album(&album, &ids, true).unwrap();
        assert!(catalog.album_page(&album, &asked()).unwrap().layout.places.is_empty(), "an album offers no places");
        catalog.set_album_kind(&album, AlbumKind::Trip).unwrap();
        let offered: Vec<(String, usize)> = catalog.album_page(&album, &asked()).unwrap().layout.places.iter().map(|place| (place.town.name.clone(), place.ids.len())).collect();
        assert_eq!(offered, [("Tokyo".to_string(), 3), ("Kyoto".to_string(), 2)]);
        let made = catalog.take_places(&album).unwrap();
        assert_eq!(made.iter().map(|chapter| chapter.name.as_str()).collect::<Vec<_>>(), ["Tokyo", "Kyoto"]);
        let page = catalog.album_page(&album, &asked()).unwrap();
        assert_eq!(page.layout.chapters.iter().map(|chapter| chapter.count).collect::<Vec<_>>(), [3, 2]);
        assert!(page.layout.places.is_empty() && page.chapters.iter().all(|chapter| chapter.place.is_some() && chapter.days.is_some()));

        assert_eq!((page.layout.loose, page.layout.chapters[0].offers.len()), (1, 0));
        let offer = &page.layout.chapters[1].offers[0];
        assert_eq!((offer.why, offer.ids.clone()), ("days", vec![ids[5]]));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_place_is_offered_what_was_taken_there_outside_the_album_only_when_that_is_read() {
        let (catalog, ids, dir) = trip("outside");
        let album = catalog.create_album("Japan").unwrap();

        catalog.set_in_album(&album, &ids[..1], true).unwrap();
        catalog.set_album_kind(&album, AlbumKind::Trip).unwrap();
        catalog.take_places(&album).unwrap();
        let offered = |outside: bool| -> Vec<i64> { catalog.album_page(&album, &PageAsked { outside, ..PageAsked::default() }).unwrap().layout.chapters[0].offers.iter().flat_map(|offer| offer.ids.clone()).collect() };
        assert_eq!(offered(true), [ids[1]], "taken there on the chapter's day; the frame of the day after is not");
        assert!(offered(false).is_empty(), "nothing outside the album is read");

        let tokyo = catalog.chapters(&album).unwrap()[0].key.clone();
        assert_eq!(catalog.add_to_chapter(&tokyo, &[ids[0], ids[1]]).unwrap(), [ids[1]]);
        assert_eq!(catalog.album_page(&album, &asked()).unwrap().layout.chapters[0].count, 2);
        catalog.take_from_chapter(&tokyo, &[ids[1]]).unwrap();
        let page = catalog.album_page(&album, &asked()).unwrap();
        assert_eq!((page.layout.chapters[0].count, page.layout.loose, page.layout.count), (1, 1, 2));
        assert!(catalog.add_to_chapter("no-such-chapter", &ids).is_err());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_wedding_is_offered_the_parts_of_its_day_and_a_chip_offers_nothing() {
        let (catalog, ids, dir) = trip("wedding");
        let album = catalog.create_album("Emma & Noah").unwrap();
        catalog.set_in_album(&album, &ids[..2], true).unwrap();
        catalog.set_album_kind(&album, AlbumKind::Wedding).unwrap();
        let page = catalog.album_page(&album, &asked()).unwrap();
        assert_eq!(page.chapters.len(), 6);

        let offer = &page.layout.chapters[0].offers[0];
        assert_eq!((offer.why, offer.ids.clone()), ("pause", ids[..2].to_vec()));
        assert!(page.layout.chapters[1].offers.is_empty());
        catalog.set_flag(ids[0], Flag::Picked).unwrap();
        let picks = catalog.album_page(&album, &PageAsked { picks_only: true, outside: true, min_rating: 0 }).unwrap();
        assert_eq!((picks.photos.len(), picks.layout.count), (1, 1));
        assert!(picks.layout.chapters.iter().all(|chapter| chapter.offers.is_empty()), "a narrowed page is read, not built");
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn a_photograph_not_read_for_its_place_yet_is_said_and_is_in_no_town() {
        let (catalog, ids, dir) = trip("unread");
        frame(&dir.join("Two"), "NEW.JPG", START + 2 * DAY);
        let two = catalog.libraries().unwrap().into_iter().find(|library| library.path.ends_with("Two")).unwrap();
        catalog.sync_library(&two).unwrap();
        let new = catalog.photos(two.id, &Filter::default()).unwrap().into_iter().find(|photo| photo.path.ends_with("NEW.JPG")).unwrap().id;
        let album = catalog.create_album("Japan").unwrap();
        catalog.set_in_album(&album, &[ids[0], new], true).unwrap();
        let page = catalog.album_page(&album, &asked()).unwrap();
        assert_eq!(page.unread, [new]);
        assert!(page.layout.days[1].towns.is_empty());
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    #[ignore]
    fn reading_a_page_among_fifteen_thousand_photographs() {
        use super::super::AtHome;
        let dir = std::env::temp_dir().join(format!("numa-album-page-{}-many", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let catalog = Catalog::open(&dir.join("home.db")).unwrap();
        let mut ids = Vec::new();
        for folder in ["One", "Two"] {
            std::fs::create_dir_all(dir.join(folder)).unwrap();
            let library = catalog.add_library(&dir.join(folder)).unwrap();
            let rows: Vec<AtHome> = (0..7_500i64)
                .map(|n| AtHome { path: format!("{folder}{n:05}.JPG"), id: format!("{folder}{n}"), size: 1, mtime: START + n * 600, taken: Some(START + n * 600), rating: 0, flag: Flag::None, caption: String::new() })
                .collect();
            catalog.add_at_home(library.id, &rows).unwrap();
            let photos = catalog.photos(library.id, &Filter::default()).unwrap();

            let placed: Vec<(i64, f64, f64)> = photos.iter().enumerate().filter(|(n, _)| n % 3 != 2).map(|(n, photo)| if n % 3 == 0 { (photo.id, TOKYO.0, TOKYO.1) } else { (photo.id, KYOTO.0, KYOTO.1) }).collect();
            catalog.set_positions(&placed).unwrap();
            ids.extend(photos.iter().map(|photo| photo.id));
        }
        let album = catalog.create_album("Japan").unwrap();
        catalog.set_in_album(&album, &ids[..300], true).unwrap();
        catalog.set_album_kind(&album, AlbumKind::Trip).unwrap();
        catalog.take_places(&album).unwrap();
        for outside in [false, true] {
            let mut times: Vec<u128> = (0..5)
                .map(|_| {
                    let start = std::time::Instant::now();
                    let page = catalog.album_page(&album, &PageAsked { outside, ..PageAsked::default() }).unwrap();
                    assert_eq!(page.layout.count, 300);
                    start.elapsed().as_millis()
                })
                .collect();
            times.sort_unstable();
            println!("album_page among {} photographs, outside {outside}: {} ms (median of 5: {times:?})", ids.len(), times[2]);
        }
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn the_grid_shows_one_chapter_or_what_is_in_none() {
        let (catalog, ids, dir) = trip("filter");
        let album = catalog.create_album("Japan").unwrap();
        catalog.set_in_album(&album, &ids, true).unwrap();
        let tokyo = catalog.add_chapter(&album, "Tokyo", None, None).unwrap().key;
        let kyoto = catalog.add_chapter(&album, "Kyoto", None, None).unwrap().key;
        catalog.add_to_chapter(&tokyo, &ids[..3]).unwrap();
        catalog.add_to_chapter(&kyoto, &ids[3..5]).unwrap();
        let shown = |chapter: &str| -> Vec<i64> {
            let mut found: Vec<i64> = catalog.photos_everywhere(&Filter { album: Some(album.clone()), chapter: Some(chapter.to_string()), ..Filter::default() }).unwrap().iter().map(|photo| photo.id).collect();
            found.sort_by_key(|id| ids.iter().position(|have| have == id));
            found
        };
        assert_eq!(shown(&tokyo), ids[..3], "across both libraries");
        assert_eq!(shown(&kyoto), ids[3..5]);
        assert_eq!(shown(NO_CHAPTER), ids[5..]);
        assert!(shown("x' OR 1=1 --").is_empty() && shown("another-album's").is_empty());

        catalog.remove_chapter(&kyoto).unwrap();
        assert_eq!(shown(NO_CHAPTER), ids[3..]);

        let everything = catalog.photos_everywhere(&Filter { chapter: Some(tokyo), ..Filter::default() }).unwrap();
        assert_eq!(everything.len(), 6);
        let _ = std::fs::remove_dir_all(dir);
    }
}
