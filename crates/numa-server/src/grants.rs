use std::collections::HashMap;
use std::path::Path;

use axum::http::StatusCode;
use numa_io::catalog::{library_of, Library};
use serde::{Deserialize, Serialize};

use crate::store::Account;
use crate::{Fail, Server};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Level {
    #[default]
    None,
    View,
    Edit,
}

impl Level {
    pub fn from_number(number: i64) -> Self {
        match number {
            2.. => Level::Edit,
            1 => Level::View,
            _ => Level::None,
        }
    }

    pub fn number(self) -> i64 {
        self as i64
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Grants {
    pub access: Level,
    pub libraries: HashMap<i64, Level>,
    pub albums: HashMap<String, Level>,
}

#[derive(Clone, Debug, Default)]
pub struct Sight {
    libraries: HashMap<i64, Level>,

    photos: HashMap<i64, Level>,

    pub household: bool,
}

impl Sight {
    pub fn library(&self, id: i64) -> Level {
        self.libraries.get(&id).copied().unwrap_or_default()
    }

    pub fn level(&self, photo: i64) -> Level {
        self.library(library_of(photo)).max(self.photos.get(&photo).copied().unwrap_or_default())
    }

    pub fn check(&self, photo: i64, need: Level) -> Result<(), Fail> {
        match self.level(photo) >= need && need > Level::None {
            true => Ok(()),
            false => Err(Fail(StatusCode::NOT_FOUND, "There is no such photograph.".into())),
        }
    }

    pub fn reaches(&self, library: i64) -> bool {
        self.library(library) >= Level::View || self.photos.keys().any(|photo| library_of(*photo) == library)
    }
}

pub fn resolve(admin: bool, own: &Path, archive: &Path, libraries: &[Library], grants: &Grants, albums: &[(Level, Vec<i64>)]) -> Sight {
    let level = |library: &Library| match () {
        _ if library.path == own => Level::Edit,
        _ if !library.path.starts_with(archive) => Level::None,
        _ if admin => Level::Edit,
        _ => grants.libraries.get(&library.id).copied().unwrap_or(grants.access),
    };
    let libraries: HashMap<i64, Level> = libraries.iter().map(|library| (library.id, level(library))).filter(|(_, level)| *level > Level::None).collect();
    let mut photos: HashMap<i64, Level> = HashMap::new();
    if !admin {
        for (level, ids) in albums {
            for id in ids {
                let at = photos.entry(*id).or_default();
                *at = (*at).max(*level);
            }
        }
    }
    Sight { libraries, photos, household: admin || grants.access == Level::Edit }
}

pub async fn sight(server: &Server, account: &Account) -> Result<Sight, Fail> {
    let grants = match account.admin {
        true => Grants::default(),
        false => server.store.grants(&account.name).map_err(Fail::internal)?,
    };
    let mut albums = Vec::new();
    for (key, level) in grants.albums.iter().filter(|(_, level)| **level > Level::None) {
        let ids = server.home.album(key).await?;
        albums.push((*level, server.home.with_twins(&ids)));
    }
    let libraries = server.home.all_libraries();

    for (_, ids) in &mut albums {
        ids.retain(|id| libraries.iter().any(|library| library.id == library_of(*id) && library.path.starts_with(&server.home.photos)));
    }
    Ok(resolve(account.admin, &server.home.sent_folder(&account.name), &server.home.photos, &libraries, &grants, &albums))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn library(id: i64, path: &str) -> Library {
        Library { id, path: PathBuf::from(path), name: None }
    }

    fn photo(library: i64, row: i64) -> i64 {
        (library << 32) | row
    }

    #[test]
    fn grants_resolve() {
        let shelf = [library(1, "/photos/2026/Holiday"), library(2, "/photos/Family"), library(3, "/sent/Sam"), library(4, "/sent/Alex")];
        let grants = Grants { access: Level::None, libraries: HashMap::from([(1, Level::View)]), albums: HashMap::new() };
        let album = [(Level::View, vec![photo(2, 7)])];
        let sight = resolve(false, Path::new("/sent/Sam"), Path::new("/photos"), &shelf, &grants, &album);
        assert_eq!(sight.library(1), Level::View);
        assert_eq!(sight.library(2), Level::None);
        assert_eq!(sight.level(photo(2, 7)), Level::View, "the album shows it");
        assert_eq!(sight.level(photo(2, 8)), Level::None, "not the rest of its library");
        assert_eq!(sight.library(3), Level::Edit, "its own Sent Home");
        assert_eq!(sight.library(4), Level::None, "another's");
        assert!(sight.reaches(2) && !sight.reaches(4) && !sight.household);

        let edit = Grants { access: Level::Edit, ..grants.clone() };
        let sight = resolve(false, Path::new("/sent/Sam"), Path::new("/photos"), &shelf, &edit, &[]);
        assert_eq!((sight.library(1), sight.library(2)), (Level::View, Level::Edit));
        assert!(sight.household);
        let admin = resolve(true, Path::new("/sent/Sam"), Path::new("/photos"), &shelf, &Grants::default(), &album);
        assert_eq!((admin.library(1), admin.library(2), admin.library(4)), (Level::Edit, Level::Edit, Level::None));
    }

    #[test]
    fn a_viewer_gets_404() {
        let shelf = [library(1, "/photos/Holiday"), library(2, "/photos/Work")];
        let grants = Grants { access: Level::None, libraries: HashMap::from([(1, Level::View)]), albums: HashMap::new() };
        let sight = resolve(false, Path::new("/sent/Sam"), Path::new("/photos"), &shelf, &grants, &[]);
        assert!(sight.check(photo(1, 3), Level::View).is_ok());
        let status = |result: Result<(), Fail>| result.err().map(|fail| fail.0);
        assert_eq!(status(sight.check(photo(2, 3), Level::View)), Some(StatusCode::NOT_FOUND), "another library");
        assert_eq!(status(sight.check(photo(1, 3), Level::Edit)), Some(StatusCode::NOT_FOUND), "/original asks Edit");
    }
}
