use std::collections::HashMap;
use std::panic::AssertUnwindSafe;
use std::path::{Path, PathBuf};
use std::sync::{mpsc, RwLock};

use axum::http::StatusCode;
use numa_io::catalog::{library_of, Catalog, Filter, Library, Photo, Sort};

use crate::Fail;

type Job = Box<dyn FnOnce(&Catalog) + Send>;

pub struct Home {
    tx: mpsc::Sender<Job>,
    pub photos: PathBuf,
    pub sent: PathBuf,

    found: RwLock<Vec<PathBuf>>,
    shelf: RwLock<Shelf>,
}

#[derive(Default)]
struct Shelf {
    libraries: Vec<Library>,
    photos: HashMap<i64, Photo>,

    order: HashMap<i64, Vec<i64>>,

    files: HashMap<i64, Vec<i64>>,

    twins: HashMap<i64, Vec<i64>>,
}

pub fn found_under(root: &Path) -> Vec<PathBuf> {
    let folders = |dir: &Path| -> Vec<PathBuf> {
        let mut folders: Vec<PathBuf> = std::fs::read_dir(dir)
            .map(|entries| entries.flatten().map(|entry| entry.path()).filter(|path| path.is_dir()).collect())
            .unwrap_or_default();
        folders.retain(|folder| !folder.file_name().is_some_and(|name| name.to_string_lossy().starts_with('.')));
        folders.sort();
        folders
    };
    let year = |dir: &Path| dir.file_name().and_then(|name| name.to_str()).is_some_and(|name| name.len() == 4 && name.chars().all(|c| c.is_ascii_digit()));
    let mut found = Vec::new();
    for folder in folders(root) {
        if folder.join(numa_io::catalog::LIBRARY_DIR).is_dir() || !year(&folder) {
            found.push(folder);
        } else {
            found.extend(folders(&folder));
        }
    }
    found
}

pub fn when(photo: &Photo) -> i64 {
    photo.taken.unwrap_or(photo.mtime)
}

pub fn month(photo: &Photo) -> String {
    numa_io::import::day(when(photo), 0)[..7].to_string()
}

fn newest_first(a: &Photo, b: &Photo) -> std::cmp::Ordering {
    when(b).cmp(&when(a)).then_with(|| a.path.cmp(&b.path))
}

impl Home {

    pub fn start(photos: PathBuf, sent: PathBuf) -> Result<Self, String> {
        let (tx, rx) = mpsc::channel::<Job>();
        let (opened_tx, opened) = mpsc::channel();
        std::thread::Builder::new()
            .name("numa-catalog".into())
            .spawn(move || {
                let catalog = match Catalog::open_default() {
                    Ok(catalog) => {
                        let _ = opened_tx.send(Ok(()));
                        catalog
                    }
                    Err(err) => return drop(opened_tx.send(Err(err))),
                };

                for job in rx {
                    if std::panic::catch_unwind(AssertUnwindSafe(|| job(&catalog))).is_err() {
                        log::error!("a catalog job panicked");
                    }
                }
            })
            .map_err(|err| err.to_string())?;
        opened.recv().map_err(|err| err.to_string())??;
        Ok(Self { tx, photos, sent, found: RwLock::default(), shelf: RwLock::default() })
    }

    pub async fn ask<R: Send + 'static>(&self, work: impl FnOnce(&Catalog) -> R + Send + 'static) -> Result<R, Fail> {
        let (back, answer) = tokio::sync::oneshot::channel();
        self.tx.send(Box::new(move |catalog| drop(back.send(work(catalog))))).map_err(|_| Fail::internal("the catalog stopped"))?;
        answer.await.map_err(|_| Fail::internal("the catalog could not answer"))
    }

    pub fn blocking<R: Send + 'static>(&self, work: impl FnOnce(&Catalog) -> R + Send + 'static) -> Option<R> {
        let (back, answer) = tokio::sync::oneshot::channel();
        self.tx.send(Box::new(move |catalog| drop(back.send(work(catalog))))).ok()?;
        answer.blocking_recv().ok()
    }

    pub fn sent_folder(&self, account: &str) -> PathBuf {
        self.sent.join(account)
    }

    pub fn set_found(&self, found: Vec<PathBuf>) {
        *self.found.write().unwrap_or_else(|poisoned| poisoned.into_inner()) = found;
    }

    fn ours(&self, library: &Library) -> bool {
        let found = self.found.read().unwrap_or_else(|poisoned| poisoned.into_inner());
        (found.contains(&library.path) || library.path.parent() == Some(&self.sent)) && library.path.is_dir()
    }

    pub fn sees(&self, account: &str, library: &Library) -> bool {
        library.path.starts_with(&self.photos) || library.path == self.sent_folder(account)
    }

    pub fn libraries(&self, account: &str) -> Vec<Library> {
        let shelf = self.shelf.read().unwrap_or_else(|poisoned| poisoned.into_inner());
        shelf.libraries.iter().filter(|library| self.sees(account, library)).cloned().collect()
    }

    pub fn all_libraries(&self) -> Vec<Library> {
        self.shelf.read().unwrap_or_else(|poisoned| poisoned.into_inner()).libraries.clone()
    }

    pub fn count(&self, library: i64) -> usize {
        self.shelf.read().unwrap_or_else(|poisoned| poisoned.into_inner()).order.get(&library).map_or(0, Vec::len)
    }

    pub fn newest(&self, library: i64) -> Option<i64> {
        let shelf = self.shelf.read().unwrap_or_else(|poisoned| poisoned.into_inner());
        shelf.order.get(&library)?.first().and_then(|id| shelf.photos.get(id)).map(when)
    }

    pub fn photo(&self, account: &str, id: i64) -> Result<Photo, Fail> {
        let shelf = self.shelf.read().unwrap_or_else(|poisoned| poisoned.into_inner());
        let library = shelf.libraries.iter().find(|library| library.id == library_of(id));
        match (library, shelf.photos.get(&id)) {
            (Some(library), Some(photo)) if self.sees(account, library) => Ok(photo.clone()),
            _ => Err(Fail(StatusCode::NOT_FOUND, "There is no such photograph.".into())),
        }
    }

    pub fn photos(&self, libraries: &[i64]) -> Vec<Photo> {
        let shelf = self.shelf.read().unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut photos: Vec<Photo> =
            libraries.iter().filter_map(|id| shelf.order.get(id)).flatten().filter_map(|id| shelf.photos.get(id)).cloned().collect();
        if libraries.len() > 1 {
            photos.sort_by(newest_first);
        }
        photos
    }

    pub fn files(&self, library: i64) -> Vec<Photo> {
        let shelf = self.shelf.read().unwrap_or_else(|poisoned| poisoned.into_inner());
        shelf.files.get(&library).into_iter().flatten().filter_map(|id| shelf.photos.get(id)).cloned().collect()
    }

    pub fn any_photo(&self, id: i64) -> Option<Photo> {
        self.shelf.read().unwrap_or_else(|poisoned| poisoned.into_inner()).photos.get(&id).cloned()
    }

    pub fn with_twins(&self, ids: &[i64]) -> Vec<i64> {
        let shelf = self.shelf.read().unwrap_or_else(|poisoned| poisoned.into_inner());
        let mut all = ids.to_vec();
        all.extend(ids.iter().filter_map(|id| shelf.twins.get(id)).flatten());
        all.sort_unstable();
        all.dedup();
        all
    }

    pub fn library(&self, id: i64) -> Option<Library> {
        self.all_libraries().into_iter().find(|library| library.id == id)
    }

    pub fn at(&self, path: &Path) -> Option<i64> {
        let shelf = self.shelf.read().unwrap_or_else(|poisoned| poisoned.into_inner());
        shelf.photos.values().find(|photo| photo.path == path).map(|photo| photo.id)
    }

    pub fn shelve(&self, only: Option<i64>, catalog: &Catalog) -> Vec<i64> {
        let libraries: Vec<Library> = catalog.libraries().unwrap_or_default().into_iter().filter(|library| self.ours(library)).collect();
        let mut read = Vec::new();
        for library in libraries.iter().filter(|library| only.is_none_or(|only| only == library.id)) {
            let filter = Filter { sort: Sort::Captured, reversed: true, ..Filter::default() };
            match catalog.photos(library.id, &filter) {
                Ok(photos) => read.push((library.id, photos)),
                Err(err) => log::warn!("{}: {err}", library.path.display()),
            }
        }
        let mut ids = Vec::new();
        let mut shelf = self.shelf.write().unwrap_or_else(|poisoned| poisoned.into_inner());
        shelf.libraries = libraries;
        for (library, photos) in read {
            if let Some(old) = shelf.files.remove(&library) {
                old.iter().for_each(|id| drop(shelf.photos.remove(id)));
            }

            let twins = numa_io::analysis::twins(photos.iter().map(|photo| (photo.id, photo.path.as_path(), photo.taken)), numa_io::raw::is_raw);
            shelf.order.insert(library, photos.iter().map(|photo| photo.id).filter(|id| !twins.contains_key(id)).collect());
            shelf.twins.retain(|raw, _| library_of(*raw) != library);
            for (twin, raw) in twins {
                shelf.twins.entry(raw).or_default().push(twin);
            }
            shelf.files.insert(library, photos.iter().map(|photo| photo.id).collect());
            ids.extend(photos.iter().map(|photo| photo.id));
            shelf.photos.extend(photos.into_iter().map(|photo| (photo.id, photo)));
        }
        ids
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn libraries_are_found_as_the_desktop_has_them() {
        let root = std::env::temp_dir().join(format!("numa-home-found-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        for dir in ["2026/Holiday/.numa", "2026/Weekend", "2025/Summer", "Phone", "2019/.numa", ".hidden", "2024/.trash"] {
            std::fs::create_dir_all(root.join(dir)).unwrap();
        }
        let found: Vec<String> = found_under(&root).iter().map(|path| path.strip_prefix(&root).unwrap().display().to_string()).collect();

        assert_eq!(found, ["2019", "2025/Summer", "2026/Holiday", "2026/Weekend", "Phone"]);
        let _ = std::fs::remove_dir_all(&root);
    }
}
