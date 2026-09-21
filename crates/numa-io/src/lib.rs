pub mod avif;
pub mod catalog;
pub mod dcp;
pub mod denoised;
pub mod dng;
pub mod exif;
pub mod export;
pub mod foreign;
pub mod gainmap;
pub mod icc;
pub mod jxl;
pub mod import;
pub mod lensfun;
pub mod presets;
pub mod raw;
pub mod thumbs;
pub mod update;
pub mod upscale;
pub mod xmp;

use numa_core::paths::NAME;

const FORMER_NAME: &str = "photoeditor";

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
