use std::collections::{HashMap, HashSet};

use crate::catalog::{AlbumChapter, AlbumKind};
use crate::chapters::{self, day_of, DayOf, PlaceOffered, Run, Shot, Town, SHOWN};

pub const MOST_OFFERED: usize = 2_000;

pub const VISIT_GAP: i64 = 2;

pub const SAME_TOWN: f64 = 0.01;

pub struct Facts<'a> {

    pub shots: &'a [Shot],

    pub chapter_of: &'a HashMap<i64, String>,
    pub chapters: &'a [AlbumChapter],

    pub outside: &'a [Shot],

    pub unread: &'a HashSet<i64>,
    pub trip: bool,
    pub may_change: bool,

    pub filtered: bool,
}

pub struct Offer {

    pub why: &'static str,

    pub ids: Vec<i64>,
    pub first: i64,
    pub last: i64,

    pub after: Option<String>,
}

pub struct ChapterLayout {
    pub count: usize,
    pub first: Option<i64>,
    pub last: Option<i64>,

    pub days: Vec<(i64, usize)>,
    pub shown: Vec<i64>,
    pub offers: Vec<Offer>,
}

pub struct DayLayout {
    pub day: i64,
    pub towns: Vec<String>,
    pub count: usize,
    pub shown: Vec<i64>,
}

pub struct Layout {
    pub count: usize,
    pub picks: usize,
    pub first: Option<i64>,
    pub last: Option<i64>,
    pub chapters: Vec<ChapterLayout>,

    pub days: Vec<DayLayout>,
    pub loose: usize,
    pub places: Vec<PlaceOffered>,
}

fn span(shots: &[Shot]) -> Option<(i64, i64)> {
    Some((shots.iter().map(|shot| shot.taken).min()?, shots.iter().map(|shot| shot.taken).max()?))
}

pub fn lay_out(facts: &Facts) -> Layout {
    let keys: HashSet<&str> = facts.chapters.iter().map(|chapter| chapter.key.as_str()).collect();
    let mut in_chapter: HashMap<&str, Vec<Shot>> = HashMap::new();
    let mut loose: Vec<Shot> = Vec::new();
    for shot in facts.shots {

        match facts.chapter_of.get(&shot.id).map(String::as_str).filter(|key| keys.contains(key)) {
            Some(key) => in_chapter.entry(key).or_default().push(shot.clone()),
            None => loose.push(shot.clone()),
        }
    }

    let offering = facts.may_change && !facts.filtered;
    let pool: Vec<&Shot> = match offering {
        true => loose.iter().chain(facts.outside).filter(|shot| !facts.unread.contains(&shot.id)).collect(),
        false => Vec::new(),
    };
    let by_pause = if offering { pauses(facts.chapters, &in_chapter, &loose) } else { HashMap::new() };
    let album_span = span(facts.shots);
    let chapters = facts
        .chapters
        .iter()
        .map(|chapter| {
            let mine = in_chapter.get(chapter.key.as_str()).map_or(&[][..], Vec::as_slice);
            let (first, last) = span(mine).unzip();
            let offers = match offering {
                true => offers_of(chapter, mine.is_empty(), &pool, album_span, &by_pause),
                false => Vec::new(),
            };
            ChapterLayout { count: mine.len(), first, last, days: days_of(mine), shown: chapters::shown(mine, SHOWN), offers }
        })
        .collect();
    let places = match facts.trip && offering {
        true => chapters::places_of(&loose).into_iter().filter(|offered| !facts.chapters.iter().any(|chapter| is_place_of(chapter, &offered.town))).collect(),
        false => Vec::new(),
    };
    let (first, last) = span(facts.shots).unzip();
    Layout {
        count: facts.shots.len(),
        picks: facts.shots.iter().filter(|shot| shot.picked).count(),
        first,
        last,
        chapters,
        days: by_day(&loose),
        loose: loose.len(),
        places,
    }
}

fn days_of(shots: &[Shot]) -> Vec<(i64, usize)> {
    let mut days: Vec<(i64, usize)> = Vec::new();
    let mut each: Vec<i64> = shots.iter().map(|shot| day_of(shot.taken)).collect();
    each.sort_unstable();
    for day in each {
        match days.last_mut() {
            Some((last, count)) if *last == day => *count += 1,
            _ => days.push((day, 1)),
        }
    }
    days
}

fn is_place_of(chapter: &AlbumChapter, town: &Town) -> bool {
    chapter.place.as_ref().is_some_and(|place| (place.latitude - town.latitude).abs() <= SAME_TOWN && (place.longitude - town.longitude).abs() <= SAME_TOWN)
}

fn by_day(loose: &[Shot]) -> Vec<DayLayout> {
    let mut of_day: HashMap<i64, Vec<Shot>> = HashMap::new();
    for shot in loose {
        of_day.entry(day_of(shot.taken)).or_default().push(shot.clone());
    }
    let day = |day: DayOf| {
        let shots = &of_day[&day.day];
        DayLayout { day: day.day, towns: day.towns, count: day.ids.len(), shown: chapters::shown(shots, SHOWN) }
    };
    chapters::days(loose).into_iter().map(day).collect()
}

fn pauses(chapters: &[AlbumChapter], in_chapter: &HashMap<&str, Vec<Shot>>, loose: &[Shot]) -> HashMap<String, (Run, Option<String>)> {
    let spans: Vec<(String, Option<(i64, i64)>)> = chapters
        .iter()
        .filter_map(|chapter| {
            let taken = in_chapter.get(chapter.key.as_str()).and_then(|shots| span(shots));
            (taken.is_some() || chapter.place.is_none()).then(|| (chapter.key.clone(), taken))
        })
        .collect();
    let times: Vec<(i64, i64)> = loose.iter().map(|shot| (shot.id, shot.taken)).collect();
    chapters::offered_by_pauses(&spans, &times)
}

fn offers_of(chapter: &AlbumChapter, empty: bool, pool: &[&Shot], album_span: Option<(i64, i64)>, pauses: &HashMap<String, (Run, Option<String>)>) -> Vec<Offer> {
    let mut offers = Vec::new();
    match &chapter.place {
        Some(place) => {
            let here = (place.latitude, place.longitude);

            let window = chapter.days.or_else(|| match album_span {
                Some((first, last)) => Some((day_of(first) - 1, day_of(last) + 1)),
                None => newest_visit(pool, here),
            });
            offers.extend(offer("place", pool.iter().copied().filter(|shot| chapters::taken_there(shot, here, window)).collect()));
            if chapter.days.is_some() {
                offers.extend(offer("days", pool.iter().copied().filter(|shot| chapters::from_those_days(shot, chapter.days)).collect()));
            }
        }
        None if empty => {
            if let Some((run, after)) = pauses.get(&chapter.key) {
                let ids = run.ids.iter().copied().take(MOST_OFFERED).collect();
                offers.push(Offer { why: "pause", ids, first: run.first, last: run.last, after: after.clone() });
            }
        }
        None => {}
    }
    offers
}

fn newest_visit(pool: &[&Shot], here: (f64, f64)) -> Option<(i64, i64)> {
    let mut days: Vec<i64> = pool.iter().filter(|shot| chapters::taken_there(shot, here, None)).map(|shot| day_of(shot.taken)).collect();
    days.sort_unstable_by(|a, b| b.cmp(a));
    days.dedup();
    let newest = *days.first()?;
    let mut first = newest;
    for day in days {
        if first - day > VISIT_GAP {
            break;
        }
        first = day;
    }
    Some((first, newest))
}

fn offer(why: &'static str, mut shots: Vec<&Shot>) -> Option<Offer> {
    shots.sort_by_key(|shot| (shot.taken, shot.id));
    shots.truncate(MOST_OFFERED);
    let (first, last) = (shots.first()?.taken, shots.last()?.taken);
    Some(Offer { why, ids: shots.iter().map(|shot| shot.id).collect(), first, last, after: None })
}

pub fn short(number: f32) -> f64 {
    number.to_string().parse().unwrap_or(f64::from(number))
}

pub fn town_at(at: Option<(f32, f32)>, memo: &mut HashMap<(u32, u32), Option<Town>>) -> Option<Town> {
    let at = at?;
    memo.entry((at.0.to_bits(), at.1.to_bits()))
        .or_insert_with(|| {
            crate::places::nearest(at).map(|place| Town {
                name: place.name.clone(),
                region: place.region.clone(),
                country: place.country.clone(),
                latitude: short(place.latitude),
                longitude: short(place.longitude),
            })
        })
        .clone()
}

pub const GRID_SPAN: i64 = 62 * 86_400;

pub fn is_grid(kind: AlbumKind, chapters: usize, times: &[i64]) -> bool {
    let span = times.iter().min().zip(times.iter().max()).map(|(first, last)| last - first);
    kind == AlbumKind::Album && chapters == 0 && span.is_some_and(|span| span >= GRID_SPAN)
}

pub fn route_km(places: &[(f64, f64)]) -> i64 {
    let leg = |from: (f64, f64), to: (f64, f64)| {
        let (a, b) = (from.0.to_radians(), to.0.to_radians());
        let half = ((b - a) / 2.0).sin().powi(2) + a.cos() * b.cos() * ((to.1 - from.1).to_radians() / 2.0).sin().powi(2);
        2.0 * 6371.0 * half.sqrt().asin()
    };
    places.windows(2).map(|pair| leg(pair[0], pair[1])).sum::<f64>().round() as i64
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::ChapterPlace;

    const DAY: i64 = 20_548 * 86_400;

    fn at(day: i64, hours: i64, minutes: i64) -> i64 {
        DAY + day * 86_400 + hours * 3600 + minutes * 60
    }

    fn town(name: &str, latitude: f64, longitude: f64) -> Town {
        Town { name: name.into(), region: String::new(), country: String::new(), latitude, longitude }
    }

    fn shot(id: i64, taken: i64) -> Shot {
        Shot { id, taken, town: None, captioned: false, picked: false }
    }

    fn shot_in(id: i64, taken: i64, name: &str, latitude: f64, longitude: f64) -> Shot {
        Shot { town: Some(town(name, latitude, longitude)), ..shot(id, taken) }
    }

    fn chapter(key: &str, place: Option<(&str, f64, f64)>, days: Option<(i64, i64)>) -> AlbumChapter {
        let place = place.map(|(name, latitude, longitude)| ChapterPlace { name: name.into(), latitude, longitude });
        AlbumChapter { key: key.into(), album: "a".into(), name: format!("Chapter {key}"), words: String::new(), place, days }
    }

    struct Setup {
        shots: Vec<Shot>,
        outside: Vec<Shot>,
        chapters: Vec<AlbumChapter>,
        of: HashMap<i64, String>,
        trip: bool,
        may_change: bool,
        filtered: bool,
    }

    impl Setup {
        fn new(shots: Vec<Shot>) -> Self {
            Setup { shots, outside: Vec::new(), chapters: Vec::new(), of: HashMap::new(), trip: false, may_change: true, filtered: false }
        }

        fn in_chapter(mut self, key: &str, ids: &[i64]) -> Self {
            self.of.extend(ids.iter().map(|id| (*id, key.to_string())));
            self
        }

        fn laid(&self) -> Layout {
            let unread = HashSet::new();
            lay_out(&Facts {
                shots: &self.shots,
                chapter_of: &self.of,
                chapters: &self.chapters,
                outside: &self.outside,
                unread: &unread,
                trip: self.trip,
                may_change: self.may_change,
                filtered: self.filtered,
            })
        }
    }

    const KYOTO: (&str, f64, f64) = ("Kyoto", 35.02, 135.75);
    const TOKYO: (&str, f64, f64) = ("Tokyo", 35.69, 139.69);

    fn in_town(id: i64, taken: i64, (name, latitude, longitude): (&str, f64, f64)) -> Shot {
        shot_in(id, taken, name, latitude, longitude)
    }

    fn offered<'a>(layout: &'a Layout, chapter: usize, why: &str) -> Option<&'a Offer> {
        layout.chapters[chapter].offers.iter().find(|offer| offer.why == why)
    }

    #[test]
    fn an_album_by_day_shows_twelve_at_most_the_captioned_and_picks_first() {
        let mut shots: Vec<Shot> = (0..20).map(|n| shot(n, at(0, 9, n))).collect();
        shots[17].captioned = true;
        shots[18].picked = true;
        shots.push(in_town(100, at(1, 9, 0), ("Annecy", 45.9, 6.12)));
        shots.push(in_town(101, at(1, 9, 5), ("Annecy", 45.9, 6.12)));
        shots.push(in_town(102, at(1, 9, 9), ("Talloires", 45.84, 6.21)));
        let layout = Setup::new(shots).laid();
        assert_eq!((layout.count, layout.loose, layout.picks, layout.chapters.len()), (23, 23, 1, 0));
        assert_eq!(layout.days.iter().map(|day| day.day).collect::<Vec<_>>(), [20_548, 20_549], "the oldest day first");
        let first = &layout.days[0];
        assert_eq!((first.count, first.shown.len()), (20, 12));
        assert!(first.shown.contains(&17) && first.shown.contains(&18), "the captioned and the picked are shown: {:?}", first.shown);
        assert!(first.shown.windows(2).all(|pair| pair[0] < pair[1]), "in time order");
        assert_eq!(layout.days[1].towns, ["Annecy", "Talloires"], "the town with most photographs first");
        assert_eq!((layout.first, layout.last), (Some(at(0, 9, 0)), Some(at(1, 9, 9))));
    }

    #[test]
    fn a_chapter_counts_its_photographs_by_day() {
        let shots = vec![shot(1, at(2, 9, 0)), shot(2, at(0, 23, 59)), shot(3, at(2, 0, 0)), shot(4, at(0, 0, 0)), shot(5, at(5, 12, 0)), shot(6, at(1, 9, 0))];
        let mut setup = Setup::new(shots).in_chapter("a", &[1, 2, 3, 4, 5]);
        setup.chapters = vec![chapter("a", None, None), chapter("b", None, None)];
        let layout = setup.laid();
        assert_eq!(layout.chapters[0].days, [(20_548, 2), (20_550, 2), (20_553, 1)], "the oldest first, a day without photographs left out, 6 is in no chapter");
        assert!(layout.chapters[1].days.is_empty());
    }

    #[test]
    fn chapters_show_their_own_and_an_empty_one_is_offered_the_run_after_its_neighbour_in_their_order() {
        let mut shots: Vec<Shot> = (0..4).map(|n| shot(n, at(0, 9, n))).collect();
        shots.extend((10..13).map(|n| shot(n, at(0, 11, n))));
        shots.extend((20..22).map(|n| shot(n, at(0, 13, n))));
        shots.extend((30..32).map(|n| shot(n, at(0, 16, n))));
        let mut setup = Setup::new(shots).in_chapter("a", &[0, 1, 2, 3]).in_chapter("d", &[30, 31]);
        setup.chapters = vec![chapter("a", None, None), chapter("b", None, None), chapter("c", None, None), chapter("d", None, None)];
        let layout = setup.laid();
        assert_eq!(layout.chapters.iter().map(|laid| laid.count).collect::<Vec<_>>(), [4, 0, 0, 2]);
        assert_eq!((layout.chapters[0].first, layout.chapters[0].last), (Some(at(0, 9, 0)), Some(at(0, 9, 3))));
        assert_eq!((layout.chapters[1].first, layout.chapters[1].last), (None, None));
        let (b, c) = (offered(&layout, 1, "pause").unwrap(), offered(&layout, 2, "pause").unwrap());
        assert_eq!((b.ids.clone(), b.after.as_deref()), (vec![10, 11, 12], Some("a")));
        assert_eq!((c.ids.clone(), c.after.as_deref()), (vec![20, 21], Some("a")));
        assert!(layout.chapters[0].offers.is_empty() && layout.chapters[3].offers.is_empty(), "a chapter with photographs is offered no pause");
        assert_eq!(layout.loose, 5);
        assert_eq!(layout.days.len(), 1, "the five loose ones are the one day's");
    }

    #[test]
    fn a_trips_chapter_is_offered_what_was_taken_there_and_what_has_no_position_on_its_days() {
        let mut setup = Setup::new(vec![
            in_town(1, at(0, 9, 0), KYOTO),
            in_town(2, at(1, 9, 0), KYOTO),
            in_town(3, at(9, 9, 0), KYOTO),
            shot(4, at(1, 12, 0)),
            shot(5, at(8, 12, 0)),
            in_town(6, at(1, 15, 0), TOKYO),
        ])
        .in_chapter("k", &[1]);
        setup.outside = vec![in_town(50, at(2, 8, 0), KYOTO), shot(51, at(2, 9, 0)), in_town(52, at(2, 10, 0), TOKYO)];
        setup.chapters = vec![chapter("k", Some(KYOTO), Some((20_548, 20_550)))];
        let layout = setup.laid();
        let (place, days) = (offered(&layout, 0, "place").unwrap(), offered(&layout, 0, "days").unwrap());
        assert_eq!(place.ids, [2, 50], "taken there on its days, from the album's loose and from outside, in time order");
        assert_eq!((place.first, place.last), (at(1, 9, 0), at(2, 8, 0)));
        assert_eq!(days.ids, [4, 51]);
        assert!(place.after.is_none() && days.after.is_none());
        assert_eq!(layout.chapters[0].count, 1, "what is in the chapter is not offered to it");
        assert!(!place.ids.contains(&1) && !days.ids.contains(&1));
    }

    #[test]
    fn a_place_without_days_is_kept_to_the_albums_span_or_to_the_newest_visit() {
        let mut with_album = Setup::new(vec![in_town(1, at(10, 9, 0), TOKYO), in_town(2, at(12, 9, 0), TOKYO), in_town(3, at(30, 9, 0), TOKYO)]);
        with_album.outside = vec![in_town(50, at(9, 9, 0), KYOTO), in_town(51, at(11, 9, 0), KYOTO), in_town(52, at(13, 9, 0), KYOTO), in_town(53, at(40, 9, 0), KYOTO)];
        with_album.chapters = vec![chapter("k", Some(KYOTO), None)];

        let layout = with_album.laid();
        assert_eq!(offered(&layout, 0, "place").unwrap().ids, [50, 51, 52], "day 9 to day 31: not the visit of day 40");

        let mut empty = Setup::new(Vec::new());

        empty.outside = vec![
            in_town(1, at(3, 9, 0), KYOTO),
            in_town(2, at(4, 9, 0), KYOTO),
            in_town(3, at(30, 9, 0), KYOTO),
            in_town(4, at(31, 9, 0), KYOTO),
            in_town(5, at(33, 9, 0), KYOTO),
            in_town(6, at(36, 9, 0), KYOTO),
        ];
        empty.chapters = vec![chapter("k", Some(KYOTO), None)];
        let layout = empty.laid();
        assert_eq!(offered(&layout, 0, "place").unwrap().ids, [6], "the newest visit is day 36, alone: 33 is three days before");
        empty.outside.pop();
        assert_eq!(offered(&empty.laid(), 0, "place").unwrap().ids, [3, 4, 5]);
        assert!(offered(&empty.laid(), 0, "days").is_none(), "a place without days has none to offer by");
    }

    #[test]
    fn a_trip_offers_its_places_less_those_that_are_a_chapters_already() {
        let mut setup = Setup::new(vec![
            in_town(1, at(0, 9, 0), TOKYO),
            in_town(2, at(1, 9, 0), TOKYO),
            in_town(3, at(3, 9, 0), KYOTO),
            in_town(4, at(4, 9, 0), KYOTO),
            in_town(5, at(5, 9, 0), ("Osaka", 34.69, 135.5)),
            shot(6, at(5, 10, 0)),
            in_town(7, at(6, 9, 0), TOKYO),
        ])
        .in_chapter("t", &[1, 2]);
        setup.trip = true;
        setup.chapters = vec![chapter("t", Some(TOKYO), Some((20_548, 20_549)))];
        let layout = setup.laid();
        let names: Vec<(&str, usize, (i64, i64))> = layout.places.iter().map(|place| (place.town.name.as_str(), place.ids.len(), (place.first_day, place.last_day))).collect();
        assert_eq!(names, [("Kyoto", 2, (20_551, 20_552)), ("Osaka", 1, (20_553, 20_553))], "Tokyo is a chapter's place already");
        setup.trip = false;
        assert!(setup.laid().places.is_empty(), "only a trip is offered places");
        setup.trip = true;
        setup.may_change = false;
        assert!(setup.laid().places.is_empty(), "and only to who may change it");
    }

    #[test]
    fn a_chip_or_no_right_to_change_gives_no_offers_but_the_lists_stay() {
        let mut setup = Setup::new(vec![in_town(1, at(1, 9, 0), KYOTO), shot(2, at(1, 12, 0))]);
        setup.outside = vec![in_town(50, at(1, 8, 0), KYOTO)];
        setup.chapters = vec![chapter("k", Some(KYOTO), Some((20_548, 20_550))), chapter("p", None, None)];
        setup.trip = true;
        assert_eq!(setup.laid().chapters[0].offers.len(), 2);
        setup.filtered = true;
        let layout = setup.laid();
        assert!(layout.chapters.iter().all(|laid| laid.offers.is_empty()) && layout.places.is_empty(), "a chip narrows: the page is being read");
        assert_eq!((layout.count, layout.loose, layout.chapters.len()), (2, 2, 2));
        setup.filtered = false;
        setup.may_change = false;
        let layout = setup.laid();
        assert!(layout.chapters.iter().all(|laid| laid.offers.is_empty()) && layout.places.is_empty(), "a reader is offered nothing");
    }

    #[test]
    fn a_photograph_whose_position_is_not_read_is_not_one_without_a_position() {
        let shots = vec![shot(1, at(1, 9, 0))];
        let outside = vec![shot(50, at(1, 8, 0))];
        let chapters = vec![chapter("k", Some(KYOTO), Some((20_548, 20_550)))];
        let (of, unread) = (HashMap::new(), HashSet::from([50]));
        let layout = lay_out(&Facts { shots: &shots, chapter_of: &of, chapters: &chapters, outside: &outside, unread: &unread, trip: false, may_change: true, filtered: false });
        assert_eq!(offered(&layout, 0, "days").unwrap().ids, [1], "50 may have a position yet to be read");
    }

    #[test]
    fn a_plain_album_of_sixty_two_days_without_chapters_is_the_grid_and_the_rest_is_not() {
        let day = 86_400;
        assert!(is_grid(AlbumKind::Album, 0, &[5 * day, 67 * day, 9 * day]), "62 days exactly");
        assert!(!is_grid(AlbumKind::Album, 0, &[5 * day, 66 * day]), "61 days");
        assert!(!is_grid(AlbumKind::Album, 1, &[0, 100 * day]), "it has a chapter");
        assert!(!is_grid(AlbumKind::Trip, 0, &[0, 100 * day]) && !is_grid(AlbumKind::Wedding, 0, &[0, 100 * day]));
        assert!(!is_grid(AlbumKind::Album, 0, &[]), "no photographs");
    }

    #[test]
    fn a_route_is_the_sum_of_its_legs_as_the_crow_flies() {
        assert_eq!(route_km(&[]), 0);
        assert_eq!(route_km(&[(35.0, 135.0)]), 0);

        assert!((350..400).contains(&route_km(&[(35.0116, 135.7681), (35.6895, 139.6917)])));
        assert_eq!(route_km(&[(35.0, 135.0), (35.0, 135.0), (35.0, 135.0)]), 0, "no leg where none moves");
    }
}
