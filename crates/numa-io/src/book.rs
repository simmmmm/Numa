use std::path::Path;

pub struct Product {
    pub name: &'static str,
    pub long_mm: f32,
}

pub const PRODUCTS: [Product; 5] = [
    Product { name: "CEWE 21 × 28 cm", long_mm: 280.0 },
    Product { name: "CEWE 30 × 30 cm", long_mm: 300.0 },
    Product { name: "Albelli 30 × 30 cm", long_mm: 300.0 },
    Product { name: "Popsa 21 × 21 cm", long_mm: 210.0 },
    Product { name: "Blurb Large Landscape 33 × 28 cm", long_mm: 330.2 },
];

pub fn long_edge(mm: f32) -> u32 {
    (mm / 25.4 * 300.0).round() as u32
}

pub struct Shot {
    pub id: i64,
    pub taken: i64,
    pub albums: Vec<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum By {
    Day,
    Album,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Chapter {
    pub name: String,

    pub ids: Vec<i64>,
}

pub fn chapters(shots: &[Shot], by: By) -> Vec<Chapter> {
    let mut sorted: Vec<&Shot> = shots.iter().collect();
    sorted.sort_by_key(|shot| (shot.taken, shot.id));
    let mut chapters: Vec<Chapter> = Vec::new();
    let mut other = Vec::new();
    for shot in sorted {
        let home = match by {
            By::Day => {
                let name = day(shot.taken);
                match chapters.last() {
                    Some(last) if last.name == name => Some(chapters.len() - 1),
                    _ => Some(open(&mut chapters, name)),
                }
            }
            By::Album => shot
                .albums
                .iter()
                .filter_map(|album| chapters.iter().position(|chapter| chapter.name == *album))
                .min()
                .or_else(|| shot.albums.iter().min().map(|album| open(&mut chapters, album.clone()))),
        };
        match home {
            Some(at) => chapters[at].ids.push(shot.id),
            None => other.push(shot.id),
        }
    }
    if !other.is_empty() {
        chapters.push(Chapter { name: "Other".to_string(), ids: other });
    }
    chapters
}

fn open(chapters: &mut Vec<Chapter>, name: String) -> usize {
    chapters.push(Chapter { name, ids: Vec::new() });
    chapters.len() - 1
}

fn day(taken: i64) -> String {
    const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
    let (year, month, day) = crate::import::civil(taken);
    format!("{day} {} {year}", MONTHS[(month as usize).clamp(1, 12) - 1])
}

pub fn folder(number: usize, chapter: &str) -> String {
    format!("{number:02} {}", clean(chapter))
}

pub fn file(number: usize, original: &Path) -> String {
    let stem = original.file_stem().map(|stem| stem.to_string_lossy()).unwrap_or_default();
    format!("{number:03} {}", clean(&stem))
}

fn clean(name: &str) -> String {
    let cleaned: String = name.chars().map(|c| if "/\\:*?\"<>|".contains(c) || c.is_control() { '-' } else { c }).collect();
    cleaned.trim().trim_start_matches('.').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAY: i64 = 86_400;

    const SEPT_14: i64 = 20_710 * DAY;

    fn shot(id: i64, taken: i64, albums: &[&str]) -> Shot {
        Shot { id, taken, albums: albums.iter().map(|album| album.to_string()).collect() }
    }

    #[test]
    fn days_are_chapters_in_the_order_they_happened() {
        let shots = [shot(3, SEPT_14 + DAY + 60, &[]), shot(1, SEPT_14 + 23 * 3600, &[]), shot(2, SEPT_14 + 10, &[]), shot(4, SEPT_14 + DAY - 1, &[])];
        let chapters = chapters(&shots, By::Day);
        assert_eq!(chapters, vec![
            Chapter { name: "14 Sep 2026".into(), ids: vec![2, 1, 4] },
            Chapter { name: "15 Sep 2026".into(), ids: vec![3] },
        ]);
    }

    #[test]
    fn albums_are_chapters_and_the_rest_is_other() {
        let shots = [
            shot(1, 100, &["Getting Ready"]),
            shot(2, 200, &["Ceremony", "Getting Ready"]),
            shot(3, 300, &["Ceremony"]),
            shot(4, 150, &[]),
            shot(5, 400, &["Portraits", "Ceremony"]),
        ];
        let chapters = chapters(&shots, By::Album);
        let names: Vec<_> = chapters.iter().map(|chapter| chapter.name.as_str()).collect();
        assert_eq!(names, ["Getting Ready", "Ceremony", "Other"]);
        assert_eq!(chapters[0].ids, [1, 2], "in two albums: the earlier chapter");
        assert_eq!(chapters[1].ids, [3, 5]);
        assert_eq!(chapters[2].ids, [4]);
    }

    #[test]
    fn names_number_and_sort() {
        assert_eq!(folder(1, "Getting Ready"), "01 Getting Ready");
        assert_eq!(folder(12, "Dinner / Dancing: 2"), "12 Dinner - Dancing- 2");
        assert_eq!(file(7, Path::new("/cards/DSCF2141.RAF")), "007 DSCF2141");
        assert_eq!(long_edge(300.0), 3543);
        assert_eq!(long_edge(280.0), 3307);
    }
}
