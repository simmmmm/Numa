const STAR: &str = "#f2c35b";
const PICK: &str = "#7fd6a0";

pub(super) fn markup(text: &str) -> String {
    let span = |colour: &str, part: &str| format!("<span foreground=\"{colour}\">{part}</span>");
    text.split("  ")
        .map(|part| match part {
            part if part.starts_with('★') => span(STAR, part),
            "⚑" => span(PICK, part),
            part => part.to_string(),
        })
        .collect::<Vec<_>>()
        .join("  ")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_colours_are_the_tokens() {
        let tokens: serde_json::Value = serde_json::from_str(include_str!("../../../data/design/tokens.json")).unwrap();
        let dark = |name: &str| {
            let all = tokens["color"]["tokens"].as_array().unwrap();
            all.iter().find(|t| t["name"] == name).unwrap()["value"]["dark"].as_str().unwrap().to_string()
        };
        assert_eq!(dark("star"), STAR);
        assert_eq!(dark("pick"), PICK);
    }

    #[test]
    fn a_pill_colours_its_stars_and_its_pick() {
        assert_eq!(markup("★ 4  ⚑"), format!("<span foreground=\"{STAR}\">★ 4</span>  <span foreground=\"{PICK}\">⚑</span>"));
        assert_eq!(markup("✕"), "✕");
        assert_eq!(markup("☆ 3  soft"), "☆ 3  soft");
    }
}
