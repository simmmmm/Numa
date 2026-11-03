use super::sync::{digest, Kept};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

pub const SAMPLE: usize = 24;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Held {
    File(PathBuf),

    Home(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Print {

    pub path: String,

    pub size: u64,
    pub held: Held,
}

pub fn holds(same: usize, of: usize) -> bool {
    of > 0 && same * 10 >= of * 9
}

pub fn matched(a: &[Print], b: &[Print]) -> Vec<(usize, usize)> {
    let name = |print: &Print| Path::new(&print.path).file_name().map(|name| name.to_os_string());
    let mut theirs: HashMap<_, Vec<usize>> = HashMap::new();
    for (at, print) in b.iter().enumerate().filter(|(_, print)| print.size > 0) {
        theirs.entry((name(print), print.size)).or_default().push(at);
    }
    let found = a.iter().enumerate().filter(|(_, print)| print.size > 0);
    found.filter_map(|(at, print)| Some((at, theirs.get_mut(&(name(print), print.size))?.pop()?))).collect()
}

pub fn alike(pairs: &[(usize, usize)], a: usize, b: usize) -> bool {
    holds(pairs.len(), a.max(b))
}

pub fn sample(pairs: &[(usize, usize)]) -> Vec<(usize, usize)> {
    let step = pairs.len().div_ceil(SAMPLE).max(1);
    pairs.iter().step_by(step).copied().collect()
}

pub fn seeded(here: &[Print], home: &[Print], pairs: &[(usize, usize)]) -> BTreeMap<String, Kept> {
    let tied = pairs.iter().filter_map(|(ours, theirs)| {
        let (ours, theirs) = (&here[*ours], &home[*theirs]);
        match &theirs.held {
            Held::Home(id) if ours.path != theirs.path => Some((ours.path.clone(), Kept { id: id.clone(), edits: digest(None), flag: "none".to_string(), canon: Some(String::new()), ..Kept::default() })),
            _ => None,
        }
    });
    tied.collect()
}

pub fn hash_here(path: &Path) -> Option<String> {
    crate::offload::hash_file(path).ok().map(crate::offload::hex)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Origin {
    Here,
    Home,
    Both,
}

impl Origin {

    pub fn with(self, other: Origin) -> Origin {
        if self == other {
            self
        } else {
            Origin::Both
        }
    }

    pub fn of(places: impl IntoIterator<Item = Origin>) -> Option<Origin> {
        places.into_iter().reduce(Origin::with)
    }

    pub fn words(self) -> &'static str {
        match self {
            Origin::Here => "This computer",
            Origin::Home => "Numa Home",
            Origin::Both => "This computer and Numa Home",
        }
    }
}

pub fn prints_in(folder: &Path) -> Vec<Print> {
    let mut prints = Vec::new();
    let mut pending = vec![folder.to_path_buf()];
    while let Some(dir) = pending.pop() {
        for entry in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
            let path = entry.path();
            if entry.file_name().to_string_lossy().starts_with('.') {
                continue;
            }
            match entry.metadata() {
                Ok(meta) if meta.is_dir() => pending.push(path),
                Ok(meta) if crate::raw::is_supported(&path) => {
                    let inside = path.strip_prefix(folder).unwrap_or(&path).to_string_lossy().into_owned();
                    prints.push(Print { path: inside, size: meta.len(), held: Held::File(path) });
                }
                _ => {}
            }
        }
    }
    prints
}

pub fn named_twice<'a>(names: impl IntoIterator<Item = &'a str>) -> HashSet<String> {
    let mut seen = HashSet::new();
    names.into_iter().map(str::to_lowercase).filter(|name| !seen.insert(name.clone())).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn here(path: &str, size: u64) -> Print {
        Print { path: path.to_string(), size, held: Held::File(PathBuf::from("/x").join(path)) }
    }

    fn home(path: &str, size: u64, id: &str) -> Print {
        Print { path: path.to_string(), size, held: Held::Home(id.to_string()) }
    }

    #[test]
    fn the_same_trip_laid_out_otherwise_pairs_up() {
        let ours = [here("a.raf", 10), here("b.raf", 20)];
        let theirs = [home("holiday/b.raf", 20, "2"), home("holiday/a.raf", 10, "1")];
        assert_eq!(matched(&ours, &theirs), [(0, 1), (1, 0)]);
    }

    #[test]
    fn another_size_or_an_unknown_one_is_another_photograph() {
        assert!(matched(&[here("a.raf", 10)], &[home("a.raf", 11, "1")]).is_empty());
        assert!(matched(&[here("a.raf", 0)], &[home("a.raf", 0, "1")]).is_empty());
    }

    #[test]
    fn one_file_pairs_once_however_many_share_its_name() {
        let ours = [here("one/a.raf", 10), here("two/a.raf", 10)];
        assert_eq!(matched(&ours, &[home("a.raf", 10, "1")]).len(), 1);
    }

    #[test]
    fn nine_in_ten_of_the_larger_is_the_same() {
        let pairs: Vec<(usize, usize)> = (0..9).map(|at| (at, at)).collect();
        assert!(alike(&pairs, 9, 10));
        assert!(!alike(&pairs, 9, 11));

        assert!(!alike(&pairs[..1], 1, 100));
        assert!(!alike(&[], 0, 0));
    }

    #[test]
    fn a_sample_is_spread_and_small() {
        let pairs: Vec<(usize, usize)> = (0..1000).map(|at| (at, at)).collect();
        let taken = sample(&pairs);
        assert!(taken.len() <= SAMPLE && taken.len() > SAMPLE / 2, "{}", taken.len());
        assert!(taken.last().unwrap().0 > 900);
        assert_eq!(sample(&pairs[..3]).len(), 3);
    }

    #[test]
    fn only_a_pair_at_another_path_is_tied_and_as_if_nothing_was_agreed() {
        let ours = [here("a.raf", 10), here("b.raf", 20)];
        let theirs = [home("a.raf", 10, "1"), home("holiday/b.raf", 20, "2")];
        let tied = seeded(&ours, &theirs, &[(0, 0), (1, 1)]);
        assert_eq!(tied.keys().collect::<Vec<_>>(), ["b.raf"]);
        let kept = &tied["b.raf"];
        assert_eq!(kept.id, "2");
        assert_eq!((kept.rating, kept.flag.as_str(), kept.updated, kept.albums.as_ref()), (0, "none", 0, None));
    }

    #[test]
    fn a_seeded_pair_keeps_the_marks_of_the_side_that_has_them() {
        use crate::home::sync::{plan, Link, Local, Remote, Step};
        let tied = seeded(&[here("b.raf", 20)], &[home("holiday/b.raf", 20, "2")], &[(0, 0)]);
        let link = Link { photos: tied, ..Link::default() };
        let local = Local { path: "b.raf".to_string(), rating: 4, flag: "none".to_string(), ..Local::default() };
        let remote = Remote { id: "2".to_string(), path: "holiday/b.raf".to_string(), size: 20, mtime: 0, taken: None, rating: 0, flag: "none".to_string(), edited: false, updated: 5, albums: None, deleted: None };
        let steps = plan(&link, &[remote], &[local], &BTreeMap::new());

        let [Step::Both { path, up, down, .. }] = steps.as_slice() else { panic!("{steps:?}") };
        assert_eq!(path, "b.raf");
        assert!(up.marks && !down.marks);
    }

    #[test]
    fn an_album_of_both_kinds_says_both() {
        assert_eq!(Origin::of([Origin::Here, Origin::Here]), Some(Origin::Here));
        assert_eq!(Origin::of([Origin::Here, Origin::Home]), Some(Origin::Both));
        assert_eq!(Origin::of([Origin::Home, Origin::Both]), Some(Origin::Both));
        assert_eq!(Origin::of([]), None);
    }

    #[test]
    fn a_folder_shows_what_home_does_not_have() {
        let folder = std::env::temp_dir().join(format!("numa-prints-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&folder);
        std::fs::create_dir_all(folder.join("day/.numa")).unwrap();
        std::fs::write(folder.join("day/a.raf"), b"aaa").unwrap();
        std::fs::write(folder.join("day/.numa/hidden.raf"), b"x").unwrap();
        std::fs::write(folder.join("b.raf"), b"bb").unwrap();
        std::fs::write(folder.join("notes.txt"), b"n").unwrap();
        let prints = prints_in(&folder);
        assert_eq!(prints.len(), 2);
        let at_home = [home("elsewhere/a.raf", 3, "1")];
        assert_eq!(prints.len() - matched(&prints, &at_home).len(), 1);
        std::fs::remove_dir_all(&folder).unwrap();
        let twice = [here("one/a.raf", 3), here("two/a.raf", 3)];
        assert_eq!(matched(&twice, &at_home).len(), 1);
    }

    #[test]
    fn only_a_name_two_albums_share_is_named_twice() {
        let doubled = named_twice(["Japan", "Italy", "japan"]);
        assert_eq!(doubled.into_iter().collect::<Vec<_>>(), ["japan"]);
        assert!(named_twice(["Japan", "Italy"]).is_empty());
    }
}
