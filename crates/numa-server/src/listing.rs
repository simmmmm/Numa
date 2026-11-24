use std::ops::Range;
use std::sync::{Arc, Mutex};

use numa_io::catalog::Photo;

use crate::grants::Sight;
use crate::home::month as month_of;

pub const EVERYTHING: &str = "all";

pub struct Listed {
    pub photos: Vec<Photo>,
    pub months: Vec<(String, Range<usize>)>,
}

impl Listed {

    pub fn of(photos: Vec<Photo>, flat: bool) -> Self {
        let mut months: Vec<(String, Range<usize>)> = Vec::new();
        for (at, photo) in photos.iter().enumerate() {
            let month = if flat { EVERYTHING.to_string() } else { month_of(photo) };
            match months.last_mut() {
                Some((last, run)) if *last == month => run.end = at + 1,
                _ => months.push((month, at..at + 1)),
            }
        }
        Listed { photos, months }
    }

    pub fn month(&self, month: &str) -> Vec<Photo> {
        if month == EVERYTHING {
            return self.photos.clone();
        }
        self.months.iter().filter(|(name, _)| name == month).flat_map(|(_, run)| self.photos[run.clone()].iter().cloned()).collect()
    }
}

struct Kept {

    sight: Arc<Sight>,
    query: String,
    turn: u64,
    listed: Arc<Listed>,
}

#[derive(Default)]
pub struct Listings(Mutex<Vec<Kept>>);

const KEPT: usize = 4;

impl Listings {
    pub fn kept(&self, sight: &Arc<Sight>, query: &str, turn: u64) -> Option<Arc<Listed>> {
        let kept = self.0.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        kept.iter().find(|kept| kept.turn == turn && kept.query == query && Arc::ptr_eq(&kept.sight, sight)).map(|kept| kept.listed.clone())
    }

    pub fn keep(&self, sight: Arc<Sight>, query: String, turn: u64, listed: Arc<Listed>) {
        let mut kept = self.0.lock().unwrap_or_else(|poisoned| poisoned.into_inner());
        kept.retain(|kept| kept.turn == turn);
        if kept.len() >= KEPT {
            kept.remove(0);
        }
        kept.push(Kept { sight, query, turn, listed });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn photo(id: i64, taken: i64) -> Photo {
        Photo {
            id,
            path: PathBuf::from(format!("/photos/Holiday/{id}.jpg")),
            mtime: 0,
            taken: Some(taken),
            rating: 0,
            flag: numa_io::catalog::Flag::None,
            sharpness: None,
            blown: None,
            best_of_burst: false,
            burst: None,
            echo: None,
            exposure: None,
            focal35: None,
            raw_clipped: None,
            raw_dark: None,
            eyes_closed: None,
            faces: None,
            face_sharpness: None,
            brightness: None,
            contrast: None,
            suggested: None,
            edited: false,
            aspect: None,
        }
    }

    const DAY: i64 = 86_400;

    #[test]
    fn a_month_is_its_run_of_the_list() {

        let listed = Listed::of(vec![photo(1, 0), photo(2, DAY), photo(3, 40 * DAY)], false);
        let names: Vec<&str> = listed.months.iter().map(|(name, _)| name.as_str()).collect();
        assert_eq!(names, ["1970-01", "1970-02"]);
        assert_eq!(listed.month("1970-01").iter().map(|photo| photo.id).collect::<Vec<_>>(), [1, 2]);
        assert_eq!(listed.month("1970-02").len(), 1);
        assert!(listed.month("1970-03").is_empty());
        assert_eq!(listed.month(EVERYTHING).len(), 3);

        let flat = Listed::of(vec![photo(3, 40 * DAY), photo(1, 0)], true);
        assert_eq!(flat.months.len(), 1);
        assert_eq!(flat.month(EVERYTHING).len(), 2);
    }

    #[test]
    fn a_list_is_kept_for_its_sight_its_question_and_its_shelf() {
        let listings = Listings::default();
        let (sam, alex) = (Arc::new(Sight::default()), Arc::new(Sight::default()));
        listings.keep(sam.clone(), "library=1".into(), 7, Arc::new(Listed::of(vec![photo(1, 0)], false)));
        assert!(listings.kept(&sam, "library=1", 7).is_some());
        assert!(listings.kept(&alex, "library=1", 7).is_none(), "another account's sight");
        assert!(listings.kept(&sam, "library=2", 7).is_none(), "another question");
        assert!(listings.kept(&sam, "library=1", 8).is_none(), "the shelf has changed");

        listings.keep(sam.clone(), "library=1".into(), 8, Arc::new(Listed::of(Vec::new(), false)));
        assert!(listings.kept(&sam, "library=1", 7).is_none());
    }
}
