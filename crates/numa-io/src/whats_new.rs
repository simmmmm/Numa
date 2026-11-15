use serde::Deserialize;

const SOURCE: &str = include_str!("../../../data/whats-new.json");

pub const READ_MORE: &str = "https://numa.photo/whats-new";

pub const DOWNLOAD_LINUX: &str = "https://numa.photo/download/linux";

pub const EARLIER: &str = "";

#[derive(Deserialize)]
struct Source {
    versions: Vec<Version>,
}

#[derive(Deserialize)]
struct Version {
    version: String,
    lines: Vec<String>,
}

pub fn lines_for(version: &str) -> Option<Vec<String>> {
    let version = version.split('-').next().unwrap_or(version);
    let source: Source = serde_json::from_str(SOURCE).ok()?;
    source.versions.into_iter().find(|one| one.version == version).map(|one| one.lines).filter(|lines| !lines.is_empty())
}

#[derive(Debug, PartialEq)]
pub struct Start {

    pub show: Option<Vec<String>>,

    pub store: Option<String>,
}

pub fn at_start(seen: Option<&str>, running: &str) -> Start {
    match seen {
        Some(seen) if seen == running => Start { show: None, store: None },
        Some(_) => Start { show: lines_for(running), store: Some(running.to_string()) },
        None => Start { show: None, store: Some(running.to_string()) },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_file_reads_and_each_version_says_one_to_six_lines() {
        let source: Source = serde_json::from_str(SOURCE).unwrap();
        assert!(!source.versions.is_empty());
        for one in &source.versions {
            assert!((1..=6).contains(&one.lines.len()), "{}", one.version);
        }
        assert_eq!(lines_for("0.51.0").unwrap().len(), 4);
        assert_eq!(lines_for("0.51.0-home.2"), lines_for("0.51.0"));
        assert_eq!(lines_for("0.1.0"), None);
    }

    #[test]
    fn once_per_version_and_never_on_a_first_install() {
        let first = at_start(None, "0.51.0");
        assert_eq!(first, Start { show: None, store: Some("0.51.0".into()) }, "first install: kept, not shown");
        let update = at_start(Some("0.50.0"), "0.51.0");
        assert_eq!(update.show, lines_for("0.51.0"));
        assert_eq!(update.store.as_deref(), Some("0.51.0"));
        assert_eq!(at_start(Some("0.51.0"), "0.51.0"), Start { show: None, store: None }, "once");
        assert!(at_start(Some(EARLIER), "0.51.0").show.is_some(), "from before the setting");
        let quiet = at_start(Some("0.51.0"), "0.1.0");
        assert_eq!(quiet, Start { show: None, store: Some("0.1.0".into()) }, "a version without lines");
    }
}
