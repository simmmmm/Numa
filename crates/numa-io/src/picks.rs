use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

pub struct Index {
    photos: Vec<(i64, PathBuf)>,

    by_stem: HashMap<String, Vec<usize>>,

    templates: Vec<String>,

    widest: usize,
}

#[derive(Debug, Default, PartialEq)]
pub struct Found {

    pub ids: Vec<i64>,

    pub found: usize,

    pub missing: Vec<String>,

    pub folders: Vec<(String, usize)>,
}

impl Found {

    pub fn total(&self) -> usize {
        self.found + self.missing.len()
    }
}

impl Index {

    pub fn new(photos: Vec<(i64, PathBuf)>, templates: &[String]) -> Self {
        let mut by_stem: HashMap<String, Vec<usize>> = HashMap::new();
        for (at, (_, path)) in photos.iter().enumerate() {
            if let Some(stem) = path.file_stem() {
                by_stem.entry(stem.to_string_lossy().to_lowercase()).or_default().push(at);
            }
        }
        let mut lowered: Vec<String> = Vec::new();
        for template in templates.iter().map(|template| template.trim().to_lowercase()) {
            if template.contains("{stem}") && !lowered.contains(&template) {
                lowered.push(template);
            }
        }
        let widest = lowered.iter().map(|template| template.split_whitespace().count() + 1).max().unwrap_or(0).max(2);
        Self { photos, by_stem, templates: lowered, widest }
    }

    pub fn find(&self, text: &str) -> Found {
        let mut found = Found::default();
        let (mut seen, mut unseen) = (HashSet::new(), HashSet::new());

        let mut folders: Vec<(String, usize, usize)> = Vec::new();
        for name in self.names(text) {
            let Some(stem) = self.resolve(&name) else {
                if unseen.insert(stem_of(&name)) {
                    found.missing.push(cleaned(&name).to_string());
                }
                continue;
            };
            if !seen.insert(stem.clone()) {
                continue;
            }
            found.found += 1;
            let at = &self.by_stem[&stem];
            found.ids.extend(at.iter().map(|&index| self.photos[index].0));
            let first = at[0];
            let folder = folder_of(&self.photos[first].1);
            match folders.iter_mut().find(|(name, _, _)| *name == folder) {
                Some((_, count, earliest)) => {
                    *count += 1;
                    *earliest = (*earliest).min(first);
                }
                None => folders.push((folder, 1, first)),
            }
        }
        folders.sort_by_key(|(_, _, first)| *first);
        found.folders = folders.into_iter().map(|(name, count, _)| (name, count)).collect();
        found
    }

    fn names(&self, text: &str) -> Vec<String> {
        let mut names = Vec::new();
        for chunk in text.split(['\n', '\r', ',', ';', '\t', '|']) {
            let chunk = chunk.trim();
            if chunk.is_empty() {
                continue;
            }
            if self.resolve(chunk).is_some() {
                names.push(chunk.to_string());
                continue;
            }

            let words: Vec<&str> = chunk
                .split_whitespace()
                .filter(|word| !matches!(word.to_lowercase().as_str(), "or" | "and" | "+" | "&"))
                .collect();
            let mut at = 0;
            while at < words.len() {
                let take = (2..=self.widest.min(words.len() - at))
                    .rev()
                    .find(|&count| self.one_name(&words[at..at + count]))
                    .unwrap_or(1);

                if take > 1 || words[at].chars().any(char::is_alphanumeric) {
                    names.push(words[at..at + take].join(" "));
                }
                at += take;
            }
        }
        names
    }

    fn one_name(&self, words: &[&str]) -> bool {
        let joined = words.join(" ");
        if self.resolve(&joined).is_some() {
            return true;
        }
        if words.len() == 2 && strip_copy(&format!("x{}", stem_of(words[1]))) == Some("x") {
            return true;
        }

        let stem = stem_of(&joined);
        let bare = strip_copy(&stem).unwrap_or(&stem).to_string();
        self.templates
            .iter()
            .any(|template| template_stems(template, &bare).iter().any(|found| !found.contains(char::is_whitespace)))
    }

    fn resolve(&self, name: &str) -> Option<String> {
        let stem = stem_of(name);
        let whole = cleaned(name).to_lowercase();
        let copy = strip_copy(&stem).map(str::to_string);
        let mut candidates = vec![stem.clone(), whole];
        candidates.extend(copy.clone());
        for template in &self.templates {
            for source in std::iter::once(&stem).chain(copy.as_ref()) {
                candidates.extend(template_stems(template, source).into_iter().map(str::to_string));
            }
        }
        candidates.into_iter().find(|candidate| self.by_stem.contains_key(candidate))
    }
}

pub fn file_names<'a>(paths: impl IntoIterator<Item = &'a Path>) -> Vec<String> {
    let mut seen = HashSet::new();
    paths
        .into_iter()
        .filter_map(|path| path.file_stem())
        .map(|stem| stem.to_string_lossy().to_string())
        .filter(|stem| seen.insert(stem.to_lowercase()))
        .collect()
}

const QUOTES: &[char] = &['"', '\'', '`', '“', '”', '‘', '’', '«', '»'];

fn cleaned(name: &str) -> &str {
    let name = name.trim().trim_matches(QUOTES).trim();
    name.rsplit(['/', '\\']).next().unwrap_or(name)
}

fn stem_of(name: &str) -> String {
    let lower = cleaned(name).to_lowercase();
    match lower.rsplit_once('.') {
        Some((base, extension))
            if !base.trim().is_empty()
                && (1..=5).contains(&extension.len())
                && extension.chars().all(|c| c.is_ascii_alphanumeric()) =>
        {
            base.trim_end().to_string()
        }
        _ => lower,
    }
}

fn strip_copy(stem: &str) -> Option<&str> {
    let count = |digits: &str| (1..=2).contains(&digits.len()) && digits.bytes().all(|b| b.is_ascii_digit());
    if let Some(inner) = stem.strip_suffix(')') {
        let (base, digits) = inner.rsplit_once('(')?;
        let base = base.trim_end();
        return (count(digits) && !base.is_empty()).then_some(base);
    }
    let (base, digits) = stem.rsplit_once(['-', '_'])?;
    (count(digits) && !base.is_empty()).then_some(base)
}

fn folder_of(path: &Path) -> String {
    path.parent().and_then(Path::file_name).map(|name| name.to_string_lossy().to_string()).unwrap_or_default()
}

fn template_stems<'a>(template: &str, name: &'a str) -> Vec<&'a str> {
    let Some((before, after)) = template.split_once("{stem}") else { return Vec::new() };
    let cuts: Vec<usize> = (0..=name.len()).filter(|&at| name.is_char_boundary(at)).collect();
    let mut stems = Vec::new();
    for &start in cuts.iter().filter(|&&start| fits(before, &name[..start])) {
        for &end in cuts.iter().filter(|&&end| end > start && fits(after, &name[end..])) {
            stems.push(&name[start..end]);
        }
    }
    stems
}

fn fits(pattern: &str, text: &str) -> bool {
    match pattern.split_once("{index}") {
        None => pattern == text,
        Some((literal, rest)) => {
            let Some(after) = text.strip_prefix(literal) else { return false };
            let digits = after.bytes().take_while(u8::is_ascii_digit).count();
            (1..=digits).any(|count| fits(rest, &after[count..]))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn library() -> Index {
        let photos = [
            "/w/Ceremony/DSCF2210.RAF",
            "/w/Ceremony/DSCF2210.JPG",
            "/w/Ceremony/DSCF2214.RAF",
            "/w/Portraits/DSCF2231.RAF",
            "/w/Portraits/A7IV_0412.ARW",
            "/w/Portraits/Smith Wedding-001.jpg",
        ];
        let photos = photos.iter().enumerate().map(|(id, path)| (id as i64 + 1, PathBuf::from(path))).collect();
        Index::new(photos, &["{stem} - edited #{index}".to_string(), "Client {stem}".to_string()])
    }

    #[test]
    fn reads_the_lists_galleries_give() {
        let index = library();
        let table: &[(&str, &str, usize, &[&str])] = &[
            ("commas", "DSCF2210, DSCF2214", 2, &[]),
            ("commas, no space (Pixieset)", "DSCF2210.jpg,DSCF2214.jpg", 2, &[]),
            ("a name a line", "DSCF2210.jpg\nDSCF2214.jpg\n", 2, &[]),
            ("Windows lines", "DSCF2210.jpg\r\nDSCF2214.jpg\r\n", 2, &[]),
            ("spaces", "DSCF2210 DSCF2214  DSCF2231", 3, &[]),
            ("semicolons", "DSCF2210;DSCF2214; DSCF2231", 3, &[]),
            ("tabs from a spreadsheet", "DSCF2210\tDSCF2214", 2, &[]),
            ("quoted", "\"DSCF2210.jpg\", \"DSCF2214.jpg\" 'DSCF2231.jpg'", 3, &[]),
            ("curly quotes", "“DSCF2210”, “DSCF2214”", 2, &[]),
            ("Lightroom", "DSCF2210 OR DSCF2214 OR A7IV_0412", 3, &[]),
            ("any case and extension", "dscf2210.raf, DSCF2214.Jpeg, a7iv_0412.HIF", 3, &[]),
            ("a gallery's copies", "DSCF2210-2.jpg, DSCF2214 (1).jpg, DSCF2231_1.jpg", 3, &[]),
            ("copies, spaced", "DSCF2214 (1).jpg DSCF2210", 2, &[]),
            ("Numa's exports", "DSCF2210 - edited #007.jpg, DSCF2214 - edited #012.jpg", 2, &[]),
            ("Numa's exports, spaced", "DSCF2210 - edited #007.jpg DSCF2214 - edited #012.jpg", 2, &[]),
            ("an export's copy", "DSCF2210 - edited #007 (1).jpg", 1, &[]),
            ("another template", "Client DSCF2231.jpg", 1, &[]),
            ("a name with a space", "Smith Wedding-001.jpg, DSCF2210", 2, &[]),
            ("paths", "C:\\Users\\me\\Downloads\\DSCF2210.jpg, /tmp/picks/DSCF2214.jpg", 2, &[]),
            ("each name once", "DSCF2210.jpg, DSCF2210.RAF, dscf2210, DSCF2210-2", 1, &[]),
            ("missing, named", "DSCF2210, A7IV_0399.jpg, DSCF2214", 2, &["A7IV_0399.jpg"]),
            ("missing once", "A7IV_0399.jpg, A7IV_0399.ARW", 0, &["A7IV_0399.jpg"]),
            ("a missing export stays whole", "DSCF9999 - edited #001.jpg", 0, &["DSCF9999 - edited #001.jpg"]),
            ("a missing copy stays whole", "DSCF9999 (2).jpg DSCF2210", 1, &["DSCF9999 (2).jpg"]),
            ("dashes between", "DSCF2210 - DSCF2214 • DSCF2231", 3, &[]),
            ("nothing", "  \n , ;", 0, &[]),
        ];
        for (shape, text, found, missing) in table {
            let result = index.find(text);
            assert_eq!(result.found, *found, "{shape}: {text:?} gave {result:?}");
            assert_eq!(result.missing, *missing, "{shape}: {text:?}");
        }
    }

    #[test]
    fn a_raw_and_its_jpeg_are_one_name_and_both_marked() {
        let result = library().find("DSCF2210");
        assert_eq!(result.found, 1);
        assert_eq!(result.ids, vec![1, 2]);
    }

    #[test]
    fn says_which_folder_they_are_in() {
        let result = library().find("DSCF2231, DSCF2210, DSCF2214, Missing_1");
        assert_eq!(result.folders, vec![("Ceremony".to_string(), 2), ("Portraits".to_string(), 1)]);
        assert_eq!(result.total(), 4);
    }

    #[test]
    fn copied_names_come_back_whole() {
        let paths = ["/w/Ceremony/DSCF2210.RAF", "/w/Ceremony/DSCF2210.JPG", "/w/Portraits/Smith Wedding-001.jpg", "/w/Portraits/A7IV_0412.ARW"]
            .map(PathBuf::from);
        let names = file_names(paths.iter().map(PathBuf::as_path));
        assert_eq!(names, ["DSCF2210", "Smith Wedding-001", "A7IV_0412"]);
        let result = library().find(&names.join(", "));
        assert_eq!((result.found, result.missing.len()), (3, 0));
    }
}
