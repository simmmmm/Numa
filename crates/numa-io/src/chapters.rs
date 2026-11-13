use std::collections::{BTreeMap, HashMap};

pub const PAUSE: i64 = 15 * 60;

pub const SHOWN: usize = 12;

const SAME_TOWN: f64 = 0.01;

#[derive(Debug, Clone, PartialEq)]
pub struct Shot {
    pub id: i64,
    pub taken: i64,

    pub town: Option<Town>,
    pub captioned: bool,
    pub picked: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Town {
    pub name: String,
    pub region: String,
    pub country: String,
    pub latitude: f64,
    pub longitude: f64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Run {
    pub ids: Vec<i64>,
    pub first: i64,
    pub last: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct DayOf {
    pub day: i64,

    pub ids: Vec<i64>,

    pub towns: Vec<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct PlaceOffered {
    pub town: Town,

    pub ids: Vec<i64>,
    pub first_day: i64,
    pub last_day: i64,
}

pub fn day_of(taken: i64) -> i64 {
    taken.div_euclid(86_400)
}

fn in_time_order(shots: &[Shot]) -> Vec<&Shot> {
    let mut sorted: Vec<&Shot> = shots.iter().collect();
    sorted.sort_by_key(|shot| (shot.taken, shot.id));
    sorted
}

pub fn days(shots: &[Shot]) -> Vec<DayOf> {
    let mut by_day: BTreeMap<i64, Vec<&Shot>> = BTreeMap::new();
    for shot in in_time_order(shots) {
        by_day.entry(day_of(shot.taken)).or_default().push(shot);
    }
    by_day
        .into_iter()
        .map(|(day, shots)| {
            let mut towns: Vec<(&str, usize)> = Vec::new();
            for name in shots.iter().filter_map(|shot| shot.town.as_ref()).map(|town| town.name.as_str()) {
                match towns.iter_mut().find(|(have, _)| *have == name) {
                    Some((_, count)) => *count += 1,
                    None => towns.push((name, 1)),
                }
            }

            towns.sort_by_key(|(_, count)| std::cmp::Reverse(*count));
            DayOf { day, ids: shots.iter().map(|shot| shot.id).collect(), towns: towns.into_iter().take(2).map(|(name, _)| name.to_string()).collect() }
        })
        .collect()
}

pub fn runs(shots: &[(i64, i64)], pause: i64) -> Vec<Run> {
    let mut sorted: Vec<(i64, i64)> = shots.to_vec();
    sorted.sort_by_key(|&(id, taken)| (taken, id));
    let mut found: Vec<Run> = Vec::new();
    for (id, taken) in sorted {
        match found.last_mut() {
            Some(run) if taken - run.last <= pause => {
                run.ids.push(id);
                run.last = taken;
            }
            _ => found.push(Run { ids: vec![id], first: taken, last: taken }),
        }
    }
    found
}

pub fn shown(shots: &[Shot], most: usize) -> Vec<i64> {
    let sorted = in_time_order(shots);
    let mut chosen: Vec<&Shot> = if sorted.len() <= most {
        sorted
    } else {
        let captioned = sorted.iter().copied().filter(|shot| shot.captioned);
        let picked = sorted.iter().copied().filter(|shot| shot.picked && !shot.captioned);
        let rest = sorted.iter().copied().filter(|shot| !shot.picked && !shot.captioned);
        captioned.chain(picked).chain(rest).take(most).collect()
    };
    chosen.sort_by_key(|shot| (shot.taken, shot.id));
    chosen.into_iter().map(|shot| shot.id).collect()
}

pub fn offered_by_pauses(chapters: &[(String, Option<(i64, i64)>)], loose: &[(i64, i64)]) -> HashMap<String, (Run, Option<String>)> {
    let runs = runs(loose, PAUSE);
    let mut offered = HashMap::new();
    let mut after: Option<(&str, i64)> = None;
    let mut cursor = i64::MIN;
    for (at, (key, span)) in chapters.iter().enumerate() {
        if let Some((_, last)) = span {
            after = Some((key.as_str(), *last));
            cursor = *last;
            continue;
        }
        let before = chapters[at + 1..].iter().find_map(|(_, span)| span.map(|(first, _)| first)).unwrap_or(i64::MAX);
        if let Some(run) = runs.iter().find(|run| run.first > cursor && run.last < before) {
            cursor = run.last;
            offered.insert(key.clone(), (run.clone(), after.map(|(key, _)| key.to_string())));
        }
    }
    offered
}

fn near(a: f64, b: f64) -> bool {
    (a - b).abs() <= SAME_TOWN
}

fn on_days(shot: &Shot, days: Option<(i64, i64)>) -> bool {
    days.is_none_or(|(first, last)| (first..=last).contains(&day_of(shot.taken)))
}

pub fn taken_there(shot: &Shot, place: (f64, f64), days: Option<(i64, i64)>) -> bool {
    shot.town.as_ref().is_some_and(|town| near(town.latitude, place.0) && near(town.longitude, place.1)) && on_days(shot, days)
}

pub fn from_those_days(shot: &Shot, days: Option<(i64, i64)>) -> bool {
    shot.town.is_none() && days.is_some() && on_days(shot, days)
}

pub fn places_of(shots: &[Shot]) -> Vec<PlaceOffered> {
    let mut found: Vec<PlaceOffered> = Vec::new();
    for shot in in_time_order(shots) {
        let Some(town) = &shot.town else { continue };
        let day = day_of(shot.taken);
        match found.iter_mut().find(|have| have.town.name == town.name && have.town.latitude == town.latitude && have.town.longitude == town.longitude) {
            Some(have) => {
                have.ids.push(shot.id);
                have.last_day = day;
            }
            None => found.push(PlaceOffered { town: town.clone(), ids: vec![shot.id], first_day: day, last_day: day }),
        }
    }
    found.sort_by(|a, b| {
        (a.first_day, &a.town.name).cmp(&(b.first_day, &b.town.name)).then(a.town.latitude.total_cmp(&b.town.latitude)).then(a.town.longitude.total_cmp(&b.town.longitude))
    });
    found
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAY: i64 = 20_548 * 86_400;

    fn at(hours: i64, minutes: i64) -> i64 {
        DAY + hours * 3600 + minutes * 60
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

    fn spread(first: i64, from: i64, to: i64, every: i64) -> Vec<(i64, i64)> {
        (from..=to).step_by(every as usize).enumerate().map(|(n, minute)| (first + n as i64, DAY + minute * 60)).collect()
    }

    fn minute(hours: i64, minutes: i64) -> i64 {
        hours * 60 + minutes
    }

    #[test]
    fn a_day_runs_from_midnight_to_midnight_by_the_cameras_clock() {
        assert_eq!(day_of(DAY), 20_548);
        assert_eq!(day_of(DAY + 86_399), 20_548);
        assert_eq!(day_of(DAY + 86_400), 20_549);
        assert_eq!(day_of(-1), -1, "before 1970 too");
    }

    #[test]
    fn a_run_ends_where_the_next_photograph_is_more_than_the_pause_later() {
        let shots = [(3, at(10, 20)), (1, at(10, 0)), (2, at(10, 15)), (4, at(11, 36))];
        let found = runs(&shots, PAUSE);
        assert_eq!(found.iter().map(|run| run.ids.clone()).collect::<Vec<_>>(), [vec![1, 2, 3], vec![4]], "fifteen minutes exactly is no pause");
        assert_eq!((found[0].first, found[0].last), (at(10, 0), at(10, 20)));
        assert!(runs(&[], PAUSE).is_empty());

        assert_eq!(runs(&[(9, 5), (2, 5)], 1)[0].ids, [2, 9]);
    }

    #[test]
    fn days_are_the_oldest_first_with_their_two_busiest_towns() {
        let mut shots = vec![
            shot_in(1, at(9, 0), "Annecy", 45.9, 6.12),
            shot_in(2, at(10, 0), "Talloires", 45.84, 6.21),
            shot_in(3, at(11, 0), "Talloires", 45.84, 6.21),
            shot_in(4, at(12, 0), "Menthon", 45.86, 6.19),
            shot(5, at(13, 0)),
            shot_in(6, at(8, 0) + 86_400, "Kyoto", 35.02, 135.75),
        ];
        shots.reverse();
        let found = days(&shots);
        assert_eq!(found.iter().map(|day| day.day).collect::<Vec<_>>(), [20_548, 20_549]);
        assert_eq!(found[0].ids, [1, 2, 3, 4, 5], "in time order, towns or not");
        assert_eq!(found[0].towns, ["Talloires", "Annecy"], "most photographs first, two at most");
        assert_eq!(found[1].towns, ["Kyoto"]);
        assert!(days(&[shot(1, DAY)])[0].towns.is_empty());
    }

    #[test]
    fn what_is_shown_is_everything_when_it_is_few_and_captioned_and_picked_first_when_not() {
        let few: Vec<Shot> = (1..=5).map(|id| shot(id, at(10, id))).collect();
        assert_eq!(shown(&few, SHOWN), [1, 2, 3, 4, 5]);

        let mut many: Vec<Shot> = (1..=30).map(|id| shot(id, at(10, id))).collect();
        many[25].captioned = true;
        many[27].captioned = true;
        many[19].picked = true;
        many[29].picked = true;
        many[27].picked = true;
        many.reverse();
        let ids = shown(&many, SHOWN);
        assert_eq!(ids.len(), SHOWN);
        assert_eq!(ids, [1, 2, 3, 4, 5, 6, 7, 8, 20, 26, 28, 30], "the four first, then time fills the rest, shown in time order");
        assert!(shown(&many, 0).is_empty());

        let mut marked: Vec<Shot> = (1..=6).map(|id| shot(id, at(10, id))).collect();
        marked[5].captioned = true;
        marked[0].picked = true;
        marked[1].picked = true;
        assert_eq!(shown(&marked, 2), [1, 6]);
    }

    fn wedding() -> (Vec<(String, Option<(i64, i64)>)>, Vec<(i64, i64)>) {
        let chapters = vec![
            ("ready".to_string(), Some((at(11, 20), at(13, 5)))),
            ("ceremony".to_string(), Some((at(14, 2), at(14, 48)))),
            ("portraits".to_string(), None),
            ("dinner".to_string(), None),
            ("dance".to_string(), None),
            ("party".to_string(), None),
        ];
        let mut loose = spread(100, minute(15, 10), minute(16, 2), 7);
        loose.extend(spread(200, minute(18, 30), minute(21, 10), 10));
        loose.extend(spread(300, minute(21, 40), minute(21, 52), 4));
        (chapters, loose)
    }

    #[test]
    fn the_pauses_of_a_wedding_day_offer_each_empty_chapter_one_run() {
        let (chapters, loose) = wedding();
        assert_eq!(loose.iter().filter(|(id, _)| *id < 200).count(), 8);
        let offered = offered_by_pauses(&chapters, &loose);

        let (portraits, after) = &offered["portraits"];
        assert_eq!((portraits.first, portraits.last, portraits.ids.len()), (at(15, 10), at(16, 2) - 3 * 60, 8), "{portraits:?}");
        assert_eq!(after.as_deref(), Some("ceremony"));
        let (dinner, after) = &offered["dinner"];
        assert_eq!((dinner.first, dinner.last), (at(18, 30), at(21, 10)));
        assert_eq!(after.as_deref(), Some("ceremony"), "still the nearest chapter with photographs");
        let (dance, _) = &offered["dance"];
        assert_eq!((dance.first, dance.last, dance.ids.len()), (at(21, 40), at(21, 52), 4));
        assert!(!offered.contains_key("party"), "no run is left for it");
        assert_eq!(offered.len(), 3);
    }

    #[test]
    fn an_empty_chapter_before_the_first_with_photographs_is_offered_only_what_ends_before_it() {
        let chapters = vec![
            ("prep".to_string(), None),
            ("ceremony".to_string(), Some((at(14, 2), at(14, 48)))),
            ("party".to_string(), None),
        ];
        let mut loose = spread(1, minute(9, 0), minute(9, 30), 10);
        loose.extend(spread(10, minute(13, 50), minute(14, 10), 10));
        loose.extend(spread(20, minute(20, 0), minute(20, 20), 10));
        let offered = offered_by_pauses(&chapters, &loose);
        assert_eq!(offered["prep"].0.ids, [1, 2, 3, 4], "the morning, and not the run that reaches the ceremony");
        assert_eq!(offered["prep"].1, None, "after no chapter");
        assert_eq!(offered["party"].0.ids, [20, 21, 22]);
        assert_eq!(offered["party"].1.as_deref(), Some("ceremony"));
    }

    #[test]
    fn with_no_chapter_holding_photographs_the_first_empty_one_gets_the_first_run() {
        let chapters = vec![("one".to_string(), None), ("two".to_string(), None)];
        let mut loose = spread(1, minute(9, 0), minute(9, 20), 10);
        loose.extend(spread(10, minute(12, 0), minute(12, 10), 10));
        let offered = offered_by_pauses(&chapters, &loose);
        assert_eq!(offered["one"].0.ids, [1, 2, 3]);
        assert_eq!(offered["two"].0.ids, [10, 11]);
        assert!(offered_by_pauses(&chapters, &[]).is_empty());
        assert!(offered_by_pauses(&[], &loose).is_empty());
    }

    #[test]
    fn a_chapter_with_photographs_is_offered_nothing_by_pauses() {
        let chapters = vec![("one".to_string(), Some((at(9, 0), at(9, 30))))];
        assert!(offered_by_pauses(&chapters, &spread(1, minute(10, 0), minute(10, 20), 10)).is_empty());
    }

    #[test]
    fn a_photograph_is_taken_there_when_its_town_is_the_chapters_and_it_is_on_its_days() {
        let kyoto = shot_in(1, DAY + 3600, "Kyoto", 35.0116, 135.7681);
        let place = (35.0116, 135.7681);
        assert!(taken_there(&kyoto, place, None));
        assert!(taken_there(&kyoto, (35.019, 135.775), None), "within 0.01 degree each way");
        assert!(!taken_there(&kyoto, (35.03, 135.7681), None));
        assert!(!taken_there(&kyoto, (35.0116, 135.79), None));
        assert!(taken_there(&kyoto, place, Some((20_548, 20_550))));
        assert!(!taken_there(&kyoto, place, Some((20_549, 20_550))), "another day");
        assert!(!taken_there(&shot(2, DAY), place, None), "no town");
    }

    #[test]
    fn a_photograph_without_a_position_comes_from_the_days_of_a_chapter() {
        let plain = shot(1, DAY + 5000);
        assert!(from_those_days(&plain, Some((20_548, 20_548))));
        assert!(!from_those_days(&plain, Some((20_549, 20_551))));
        assert!(!from_those_days(&plain, None), "a chapter without days has none");
        assert!(!from_those_days(&shot_in(2, DAY, "Kyoto", 35.0, 135.7), Some((20_548, 20_548))), "it has a position");
    }

    #[test]
    fn the_places_an_album_offers_are_its_towns_in_the_order_they_were_reached() {
        let shots = [
            shot_in(5, DAY + 2 * 86_400 + 60, "Osaka", 34.69, 135.50),
            shot_in(3, DAY + 86_400 + 60, "Kyoto", 35.01, 135.77),
            shot_in(2, DAY + 60, "Tokyo", 35.69, 139.69),
            shot_in(4, DAY + 3 * 86_400, "Kyoto", 35.01, 135.77),
            shot_in(1, DAY, "Tokyo", 35.69, 139.69),
            shot_in(6, DAY, "Alpha", 1.0, 2.0),
            shot(7, DAY),
        ];
        let found = places_of(&shots);
        let summary: Vec<(&str, Vec<i64>, i64, i64)> = found.iter().map(|place| (place.town.name.as_str(), place.ids.clone(), place.first_day, place.last_day)).collect();
        assert_eq!(
            summary,
            [
                ("Alpha", vec![6], 20_548, 20_548),
                ("Tokyo", vec![1, 2], 20_548, 20_548),
                ("Kyoto", vec![3, 4], 20_549, 20_551),
                ("Osaka", vec![5], 20_550, 20_550)
            ],
            "by first day, then name"
        );
        assert!(places_of(&[shot(1, DAY)]).is_empty());
    }
}
