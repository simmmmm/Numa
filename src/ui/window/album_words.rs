use numa::io::album_page::{route_km, ChapterLayout, Offer};
use numa::io::catalog::{AlbumChapter, AlbumKind, AlbumPage};

use super::places;

const DAY: i64 = 86_400;
const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
const WEEKDAYS: [&str; 7] = ["Thu", "Fri", "Sat", "Sun", "Mon", "Tue", "Wed"];

fn date(day: i64) -> (i64, usize, i64) {

    let z = day + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(m <= 2), m as usize, d)
}

pub(super) fn day_name(day: i64) -> String {
    let (_, month, date) = date(day);
    format!("{} {date} {}", WEEKDAYS[day.rem_euclid(7) as usize], MONTHS[month - 1])
}

pub(super) fn days_span(first: i64, last: i64, year: bool) -> String {
    let ((y1, m1, d1), (y2, m2, d2)) = (date(first), date(last));
    let end = if year { format!(" {y2}") } else { String::new() };
    if y1 != y2 {
        return format!("{d1} {} {y1} – {d2} {} {y2}", MONTHS[m1 - 1], MONTHS[m2 - 1]);
    }
    match (first == last, m1 == m2) {
        (true, _) => format!("{d1} {}{end}", MONTHS[m1 - 1]),
        (false, true) => format!("{d1}–{d2} {}{end}", MONTHS[m1 - 1]),
        (false, false) => format!("{d1} {} – {d2} {}{end}", MONTHS[m1 - 1], MONTHS[m2 - 1]),
    }
}

fn clock(when: i64) -> String {
    let seconds = when.rem_euclid(DAY);
    format!("{:02}:{:02}", seconds / 3_600, seconds % 3_600 / 60)
}

fn hours(first: i64, last: i64) -> String {
    format!("{}–{}", clock(first), clock(last))
}

fn nights(n: i64) -> String {
    match n {
        1 => "1 night".to_string(),
        n => format!("{n} nights"),
    }
}

pub(super) struct Figure {
    pub(super) label: &'static str,
    pub(super) value: String,
    pub(super) of: String,
}

pub(super) fn here_now(page: &AlbumPage, today: i64) -> Option<usize> {
    page.chapters.iter().position(|chapter| chapter.place.is_some() && chapter.days.is_some_and(|(first, last)| first <= today && today <= last))
}

pub(super) fn figures(page: &AlbumPage, today: i64) -> Vec<Figure> {
    let layout = &page.layout;
    let figure = |label, value: String, of: &str| Figure { label, value, of: of.to_string() };
    let mut days: Vec<i64> = page.chapters.iter().filter_map(|chapter| chapter.days).flat_map(|(first, last)| [first, last]).collect();
    days.extend(layout.first.iter().chain(&layout.last).map(|when| when.div_euclid(DAY)));
    let span = days.iter().min().zip(days.iter().max()).map(|(first, last)| (*first, *last));
    let mut all = Vec::new();
    if let Some((first, last)) = span {
        let long = last - first + 1;
        if page.kind == AlbumKind::Trip && first <= today && today <= last {
            all.push(figure("Day", places::grouped(today - first + 1), &format!("of {}", places::grouped(long))));
        } else if long > 1 {
            all.push(figure("Days", places::grouped(long), ""));
        }
    }
    let filled = layout.chapters.iter().filter(|chapter| chapter.count > 0).count();
    if page.kind == AlbumKind::Trip {
        if !page.chapters.is_empty() {
            all.push(figure("Places", places::grouped(filled as i64), &format!("of {}", places::grouped(page.chapters.len() as i64))));
        }

        let been: Vec<(f64, f64)> = page.chapters.iter().zip(&layout.chapters).filter(|(_, laid)| laid.count > 0).filter_map(|(chapter, _)| chapter.place.as_ref()).map(|place| (place.latitude, place.longitude)).collect();
        if been.len() > 1 {
            all.push(figure("Travelled", places::grouped(route_km(&been)), "km"));
        }
    } else if !page.chapters.is_empty() {
        all.push(figure("Chapters", places::grouped(filled as i64), &format!("of {}", places::grouped(page.chapters.len() as i64))));
    }
    all.push(figure("Photos", places::grouped(layout.count as i64), ""));
    all
}

pub(super) fn over_name(page: &AlbumPage) -> String {
    let layout = &page.layout;
    let mut days: Vec<i64> = page.chapters.iter().filter_map(|chapter| chapter.days).flat_map(|(first, last)| [first, last]).collect();
    days.extend(layout.first.iter().chain(&layout.last).map(|when| when.div_euclid(DAY)));
    let kind = match page.kind {
        AlbumKind::Trip => "Trip",
        AlbumKind::Wedding => "Wedding",
        AlbumKind::Album => "Album",
    };
    match days.iter().min().zip(days.iter().max()) {
        Some((first, last)) => format!("{kind} · {}", days_span(*first, *last, true)),
        None => kind.to_string(),
    }
}

pub(super) fn leg(from: &AlbumChapter, to: &AlbumChapter) -> Option<String> {
    let (from, to) = (from.place.as_ref()?, to.place.as_ref()?);
    Some(format!("{} km", places::grouped(route_km(&[(from.latitude, from.longitude), (to.latitude, to.longitude)]))))
}

pub(super) fn chapter_line(chapter: &AlbumChapter, laid: &ChapterLayout) -> String {
    if let Some((first, last)) = chapter.days {
        let span = days_span(first, last, false);
        let mut line = if last > first { format!("{span} · {}", nights(last - first)) } else { span };
        if laid.count > 0 {
            line = format!("{line} · {}", places::photos(laid.count));
        }
        return line;
    }
    match (laid.first, laid.last) {
        (Some(first), Some(last)) if first.div_euclid(DAY) == last.div_euclid(DAY) => format!("{} · {}", hours(first, last), places::photos(laid.count)),
        (Some(first), Some(last)) => format!("{} · {}", days_span(first.div_euclid(DAY), last.div_euclid(DAY), false), places::photos(laid.count)),
        _ => "No photos yet".to_string(),
    }
}

pub(super) fn route_line(chapter: &AlbumChapter, laid: &ChapterLayout, here: bool) -> String {
    let when = match (here, chapter.days) {
        (true, _) => Some("Here Now".to_string()),
        (false, Some((first, last))) => Some(days_span(first, last, false)),
        (false, None) => None,
    };
    let what = match (laid.count, chapter.days) {
        (0, Some((first, last))) if last > first => Some(nights(last - first)),
        (0, _) => None,
        (count, _) => Some(places::grouped(count as i64)),
    };
    when.into_iter().chain(what).collect::<Vec<_>>().join(" · ")
}

pub(super) fn offer_lines(page: &AlbumPage, chapter: &AlbumChapter, has_photos: bool, offer: &Offer) -> (String, String) {
    let n = places::grouped(offer.ids.len() as i64);
    match offer.why {
        "place" => {
            let line = if has_photos { format!("{n} more taken here") } else { format!("{n} taken here") };
            (line, format!("Taken in {}", chapter.place.as_ref().map_or(chapter.name.as_str(), |place| place.name.as_str())))
        }
        "days" => (format!("{n} from those days"), "Without a place, on its days".to_string()),
        _ => {
            let after = offer.after.as_ref().and_then(|key| page.chapters.iter().find(|one| &one.key == key));
            let first = page.chapters.first().is_some_and(|one| one.key == chapter.key);
            let why = match after {
                Some(after) => format!("After {}, before the day's next pause", after.name),
                None if first => "The first part of the day".to_string(),
                None => "The next part of the day".to_string(),
            };
            (format!("{n} taken {}", hours(offer.first, offer.last)), why)
        }
    }
}

pub(super) fn place_line(first_day: i64, last_day: i64, count: usize) -> String {
    format!("{} · {}", days_span(first_day, last_day, false), places::photos(count))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn days_are_said_as_numa_home_says_them() {

        let fri = 20_651;
        assert_eq!(date(fri), (2026, 7, 17));
        assert_eq!(day_name(fri), "Fri 17 Jul");
        assert_eq!(days_span(fri, fri + 2, true), "17–19 Jul 2026");
        assert_eq!(days_span(fri, fri, false), "17 Jul");
        assert_eq!(days_span(fri, fri + 20, false), "17 Jul – 6 Aug");
        assert_eq!(days_span(fri + 160, fri + 175, true), "24 Dec 2026 – 8 Jan 2027");
        assert_eq!(date(0), (1970, 1, 1));
        assert_eq!(day_name(0), "Thu 1 Jan");
        assert_eq!(hours(15 * 3_600 + 10 * 60, 16 * 3_600 + 2 * 60 + 59), "15:10–16:02");
        assert_eq!((nights(1), nights(3)), ("1 night".to_string(), "3 nights".to_string()));
        assert_eq!(place_line(fri, fri + 3, 14), "17–20 Jul · 14 photos");
    }

    use numa::io::album_page::Layout;
    use numa::io::catalog::ChapterPlace;

    const FRI: i64 = 20_651;
    const AMSTERDAM: (f64, f64) = (52.37, 4.9);
    const TOKYO: (f64, f64) = (35.69, 139.69);

    fn chapter(place: Option<(f64, f64)>, days: Option<(i64, i64)>) -> AlbumChapter {
        let place = place.map(|(latitude, longitude)| ChapterPlace { name: "A place".into(), latitude, longitude });
        AlbumChapter { key: String::new(), album: "a".into(), name: "A chapter".into(), words: String::new(), place, days }
    }

    fn page(kind: AlbumKind, chapters: Vec<AlbumChapter>, counts: &[usize], count: usize, taken: Option<(i64, i64)>) -> AlbumPage {
        let laid = counts.iter().map(|&count| ChapterLayout { count, first: None, last: None, days: Vec::new(), shown: Vec::new(), offers: Vec::new() }).collect();
        let (first, last) = taken.unzip();
        let layout = Layout { count, picks: 0, first, last, chapters: laid, days: Vec::new(), loose: 0, places: Vec::new() };
        AlbumPage { key: "a".into(), name: "A".into(), kind, grid: false, photos: Vec::new(), captions: Default::default(), chapters, layout, unread: Vec::new() }
    }

    fn said(page: &AlbumPage, today: i64) -> Vec<(&'static str, String, String)> {
        figures(page, today).into_iter().map(|figure| (figure.label, figure.value, figure.of)).collect()
    }

    fn one(label: &'static str, value: &str, of: &str) -> (&'static str, String, String) {
        (label, value.to_string(), of.to_string())
    }

    fn trip() -> AlbumPage {
        let chapters = vec![
            chapter(Some(AMSTERDAM), Some((FRI, FRI + 3))),
            chapter(Some(TOKYO), Some((FRI + 4, FRI + 8))),
            chapter(Some(AMSTERDAM), Some((FRI + 9, FRI + 11))),
            chapter(None, None),
            chapter(Some(TOKYO), Some((FRI + 12, FRI + 15))),
        ];
        page(AlbumKind::Trip, chapters, &[10, 2, 0, 0, 0], 12, Some((FRI * DAY + 3_600, (FRI + 5) * DAY)))
    }

    #[test]
    fn a_trip_under_way_counts_its_day_its_places_and_how_far_it_has_come() {
        let trip = trip();
        let far = places::grouped(route_km(&[AMSTERDAM, TOKYO]));
        assert_eq!(said(&trip, FRI + 8), [one("Day", "9", "of 16"), one("Places", "2", "of 5"), one("Travelled", &far, "km"), one("Photos", "12", "")]);
        assert_eq!(said(&trip, FRI)[0], one("Day", "1", "of 16"), "the first day is day 1");
        assert_eq!(said(&trip, FRI + 15)[0], one("Day", "16", "of 16"));

        assert_eq!(said(&trip, FRI - 1)[0], one("Days", "16", ""));
        assert_eq!(said(&trip, FRI + 16)[0], one("Days", "16", ""));
    }

    #[test]
    fn a_trip_travels_only_between_places_with_photographs() {
        let mut trip = trip();
        trip.layout.chapters[1].count = 0;
        let labels: Vec<&str> = figures(&trip, FRI).iter().map(|figure| figure.label).collect();
        assert_eq!(labels, ["Day", "Places", "Photos"], "one place been to is no distance");
    }

    #[test]
    fn here_now_is_the_place_whose_days_hold_today() {
        let trip = trip();
        assert_eq!(here_now(&trip, FRI + 8), Some(1));
        assert_eq!(here_now(&trip, FRI + 9), Some(2));
        assert_eq!(here_now(&trip, FRI - 1), None);
        assert_eq!(here_now(&trip, FRI + 16), None);

        let mut placeless = trip;
        placeless.chapters[1].place = None;
        assert_eq!(here_now(&placeless, FRI + 8), None);
    }

    #[test]
    fn a_wedding_or_an_album_counts_its_days_its_chapters_and_its_photos() {
        let six = (0..6).map(|_| chapter(None, None)).collect();
        let wedding = page(AlbumKind::Wedding, six, &[4, 3, 0, 0, 0, 0], 7, Some((FRI * DAY + 3_600, FRI * DAY + 7_200)));
        assert_eq!(said(&wedding, FRI), [one("Chapters", "2", "of 6"), one("Photos", "7", "")], "one day is no Days, and a wedding has no Day n");
        let album = page(AlbumKind::Album, Vec::new(), &[], 30, Some((FRI * DAY, (FRI + 2) * DAY)));
        assert_eq!(said(&album, FRI + 1), [one("Days", "3", ""), one("Photos", "30", "")], "an album has no Day n of m, and no Chapters without them");
        let empty = page(AlbumKind::Album, Vec::new(), &[], 0, None);
        assert_eq!(said(&empty, FRI), [one("Photos", "0", "")]);
    }

    #[test]
    fn every_figure_groups_its_digits_as_the_kilometres_do() {
        let long = page(AlbumKind::Trip, vec![chapter(Some(AMSTERDAM), Some((FRI, FRI + 4_724)))], &[3_694], 3_694, None);
        assert_eq!(said(&long, FRI + 3_693)[0], one("Day", "3\u{2009}694", "of 4\u{2009}725"));
        assert_eq!(said(&long, FRI - 1)[0], one("Days", "4\u{2009}725", ""));
        assert_eq!(said(&long, FRI)[2], one("Photos", "3\u{2009}694", ""));
    }

    #[test]
    fn a_places_days_count_in_the_span() {

        let planned = page(AlbumKind::Trip, vec![chapter(Some(AMSTERDAM), Some((FRI + 10, FRI + 12)))], &[0], 1, Some((FRI * DAY, FRI * DAY)));
        assert_eq!(said(&planned, FRI + 30)[0], one("Days", "13", ""));
    }
}
