use std::collections::HashSet;

use crate::catalog::{Catalog, Library};

const JOINING: [&str; 8] = ["and", "en", "with", "met", "&", "+", "und", "et"];

pub fn people_in(catalog: &Catalog, libraries: &[Library], text: &str) -> (Option<HashSet<i64>>, String) {
    let named = catalog.named_faces().unwrap_or_default();
    let mut names: Vec<String> = Vec::new();
    for (_, name, _) in &named {
        if !name.is_empty() && !names.iter().any(|have| have.eq_ignore_ascii_case(name)) {
            names.push(name.clone());
        }
    }
    let (groups, rest) = split_names(text, &names);
    if groups.is_empty() {
        return (None, text.to_string());
    }
    let known = catalog.known_from(&named).unwrap_or_default();
    let mut sets = vec![HashSet::new(); groups.len()];
    for library in libraries {
        for (name, ids) in catalog.people_among(library.id, &named, &known).unwrap_or_default() {
            for (group, set) in groups.iter().zip(sets.iter_mut()) {
                if group.iter().any(|one| one.eq_ignore_ascii_case(&name)) {
                    set.extend(ids.iter().copied());
                }
            }
        }
    }
    let everyone = sets.into_iter().reduce(|a, b| a.intersection(&b).copied().collect()).unwrap_or_default();
    (Some(everyone), rest)
}

fn split_names(text: &str, names: &[String]) -> (Vec<Vec<String>>, String) {
    let mut left: Vec<&str> = text.split_whitespace().collect();
    let mut groups = Vec::new();

    let mut whole: Vec<&String> = names.iter().filter(|name| name.split_whitespace().count() > 1).collect();
    whole.sort_by_key(|name| std::cmp::Reverse(name.split_whitespace().count()));
    for name in whole {
        let parts: Vec<String> = name.split_whitespace().map(folded).collect();
        if let Some(at) = left.windows(parts.len()).position(|run| run.iter().map(|word| folded(word)).eq(parts.iter().cloned())) {
            left.drain(at..at + parts.len());
            groups.push(vec![name.clone()]);
        }
    }
    let mut rest = Vec::new();
    for word in left {
        let plain = folded(word);
        let first = |name: &&String| name.split_whitespace().next().map(folded).as_deref() == Some(plain.as_str());
        let who: Vec<String> = names.iter().filter(first).cloned().collect();
        if !who.is_empty() {
            groups.push(who);
        } else if !JOINING.contains(&plain.as_str()) {
            rest.push(word);
        }
    }
    (groups, rest.join(" "))
}

pub fn folded(text: &str) -> String {
    const FROM: &str = concat!("àáâãäåāăą", "çćč", "ď", "èéêëēėęě", "ìíîïīį", "ł", "ñńň", "òóôõöøōő", "ř", "śšş", "ť", "ùúûüūůűų", "ýÿ", "źżž");
    const TO: &str = concat!("aaaaaaaaa", "ccc", "d", "eeeeeeee", "iiiiii", "l", "nnn", "oooooooo", "r", "sss", "t", "uuuuuuuu", "yy", "zzz");
    text.to_lowercase().chars().map(|c| FROM.chars().position(|from| from == c).and_then(|at| TO.chars().nth(at)).unwrap_or(c)).collect()
}

#[cfg(test)]
mod tests {
    use super::{folded, split_names};

    #[test]
    fn a_name_is_found_among_the_words_and_the_rest_is_left_for_the_model() {
        let names: Vec<String> = ["Amélie", "Sam Jansen", "Amelia"].map(String::from).to_vec();
        assert_eq!(folded("Amélie ŁÓDŹ"), "amelie lodz");
        let (groups, rest) = split_names("amelie op het strand", &names);
        assert_eq!(groups, vec![vec!["Amélie".to_string()]]);
        assert_eq!(rest, "op het strand");
        let (groups, rest) = split_names("Sam Jansen en amélie", &names);
        assert_eq!(groups, vec![vec!["Sam Jansen".to_string()], vec!["Amélie".to_string()]]);
        assert_eq!(rest, "");

        let (groups, _) = split_names("tijmen", &names);
        assert_eq!(groups, vec![vec!["Sam Jansen".to_string()]]);
        assert!(split_names("bride with bouquet", &names).0.is_empty());
    }
}
