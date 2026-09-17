pub mod catalog;
pub mod dcp;
pub mod denoised;
pub mod export;
pub mod foreign;
pub mod lensfun;
pub mod presets;
pub mod raw;
pub mod thumbs;
pub mod update;

use std::path::PathBuf;

pub(crate) const NAME: &str = "numa";

const FORMER_NAME: &str = "photoeditor";

pub fn data_dir() -> PathBuf {
    dirs::data_dir().unwrap_or_else(|| PathBuf::from(".")).join(NAME)
}

pub fn models_dir() -> PathBuf {
    std::env::var_os("NUMA_MODELS").map(PathBuf::from).unwrap_or_else(|| data_dir().join("models"))
}

pub fn model_file(names: &[&str]) -> Option<PathBuf> {
    let dir = models_dir();
    names.iter().map(|name| dir.join(name)).find(|path| path.is_file())
}

pub fn cache_dir() -> PathBuf {
    #[cfg(test)]
    if let Some(root) = TEST_CACHE.get() {
        return root.clone();
    }
    dirs::cache_dir().unwrap_or_else(std::env::temp_dir).join(NAME)
}

#[cfg(test)]
static TEST_CACHE: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

#[cfg(test)]
pub(crate) fn use_test_cache(root: PathBuf) {
    let _ = TEST_CACHE.set(root);
}

pub fn adopt_former_name() {
    for base in [dirs::data_dir(), dirs::cache_dir()].into_iter().flatten() {
        adopt_within(&base);
    }
}

fn adopt_within(base: &std::path::Path) {
    let (old, new) = (base.join(FORMER_NAME), base.join(NAME));

    if new.exists() || !old.is_dir() {
        return;
    }
    match std::fs::rename(&old, &new) {
        Ok(()) => log::info!("moved {} to {}", old.display(), new.display()),

        Err(err) => log::warn!(
            "could not move {} to {}: {err} — the previous catalog is still there",
            old.display(),
            new.display()
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_old_name_takes_its_catalog_with_it() {
        let base = std::env::temp_dir().join("numa-adopt-test");
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join(FORMER_NAME).join("models")).unwrap();
        std::fs::write(base.join(FORMER_NAME).join("catalog.db"), b"ratings and edits").unwrap();

        adopt_within(&base);

        assert!(!base.join(FORMER_NAME).exists(), "the old directory is gone");
        assert_eq!(
            std::fs::read(base.join(NAME).join("catalog.db")).unwrap(),
            b"ratings and edits",
            "the catalog came across whole"
        );
        assert!(base.join(NAME).join("models").is_dir(), "and so did everything beside it");

        adopt_within(&base);
        assert!(base.join(NAME).join("catalog.db").exists());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn a_catalog_under_the_new_name_wins() {
        let base = std::env::temp_dir().join("numa-adopt-keeps");
        let _ = std::fs::remove_dir_all(&base);
        std::fs::create_dir_all(base.join(FORMER_NAME)).unwrap();
        std::fs::create_dir_all(base.join(NAME)).unwrap();
        std::fs::write(base.join(FORMER_NAME).join("catalog.db"), b"old").unwrap();
        std::fs::write(base.join(NAME).join("catalog.db"), b"current").unwrap();

        adopt_within(&base);

        assert_eq!(std::fs::read(base.join(NAME).join("catalog.db")).unwrap(), b"current");
        assert!(base.join(FORMER_NAME).exists(), "and the old one is left where it is");
        let _ = std::fs::remove_dir_all(&base);
    }
}
