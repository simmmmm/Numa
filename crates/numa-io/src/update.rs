use serde::Deserialize;

pub const RELEASES: &str = "https://api.github.com/repos/simmmmm/Numa/releases?per_page=30";

#[derive(Deserialize)]
struct Release {
    tag_name: String,
    html_url: String,
    #[serde(default)]
    draft: bool,
    #[serde(default)]
    prerelease: bool,
}

pub fn newer_release(json: &str, current: &str) -> Option<(String, String)> {
    let releases: Vec<Release> = serde_json::from_str(json).ok()?;
    let current = version(current)?;
    releases
        .into_iter()
        .filter(|release| !release.draft && !release.prerelease)
        .filter_map(|release| Some((version(&release.tag_name)?, release)))
        .filter(|(found, _)| *found > current)
        .max_by_key(|(found, _)| *found)
        .map(|((major, minor, patch), release)| (format!("{major}.{minor}.{patch}"), release.html_url))
}

fn version(tag: &str) -> Option<(u32, u32, u32)> {
    let mut parts = tag.trim().trim_start_matches(['v', 'V']).split('.');
    let number = |part: Option<&str>| part?.parse::<u32>().ok();
    let found = (number(parts.next())?, number(parts.next())?, number(parts.next().or(Some("0")))?);
    parts.next().is_none().then_some(found)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_newer_numbered_release_counts() {
        let json = r#"[
            {"tag_name": "models", "html_url": "m"},
            {"tag_name": "v0.10.0", "html_url": "ten"},
            {"tag_name": "v0.9.1", "html_url": "same"},
            {"tag_name": "v1.0.0", "html_url": "draft", "draft": true},
            {"tag_name": "0.11.2", "html_url": "eleven"}
        ]"#;
        assert_eq!(newer_release(json, "0.9.1"), Some(("0.11.2".into(), "eleven".into())));
        assert_eq!(newer_release(json, "0.11.2"), None, "nothing newer");
        assert_eq!(newer_release(r#"[{"tag_name": "models", "html_url": "m"}]"#, "0.9.1"), None);
        assert_eq!(newer_release("not json", "0.9.1"), None);
    }
}
