use std::path::{Path, PathBuf};

use crate::catalog::LIBRARY_DIR;
use crate::raw;

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Copied {
    pub photos: usize,

    pub existing: Vec<String>,
    pub failed: Vec<String>,
}

pub fn copy_into(dropped: &[PathBuf], folder: &Path) -> Copied {
    let mut copied = Copied::default();
    let mut pending: Vec<(PathBuf, PathBuf)> =
        dropped.iter().filter_map(|from| Some((from.clone(), folder.join(from.file_name()?)))).collect();

    while let Some((from, to)) = pending.pop() {
        let name = || to.strip_prefix(folder).unwrap_or(&to).display().to_string();
        if from.is_dir() {

            if folder.starts_with(&from) || from.file_name().is_some_and(|name| name == LIBRARY_DIR) {
                continue;
            }
            let Ok(entries) = std::fs::read_dir(&from) else {
                copied.failed.push(name());
                continue;
            };
            pending.extend(entries.flatten().map(|entry| (entry.path(), to.join(entry.file_name()))));
        } else if raw::is_supported(&from) {
            if to.exists() {
                if from != to {
                    copied.existing.push(name());
                }
                continue;
            }
            match copy_whole(&from, &to) {
                Ok(()) => copied.photos += 1,
                Err(err) => {
                    log::warn!("{}: {err}", from.display());
                    copied.failed.push(name());
                }
            }
        }
    }
    copied
}

pub(crate) fn copy_whole(from: &Path, to: &Path) -> std::io::Result<()> {
    if let Some(parent) = to.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut part = to.as_os_str().to_owned();
    part.push(".part");
    let part = PathBuf::from(part);
    let result = std::fs::copy(from, &part).and_then(|_| {
        let modified = std::fs::metadata(from)?.modified()?;
        std::fs::File::options().write(true).open(&part)?.set_modified(modified)?;
        std::fs::rename(&part, to)
    });
    if result.is_err() {
        let _ = std::fs::remove_file(&part);
    }
    result
}
