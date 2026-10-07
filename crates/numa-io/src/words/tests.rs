use super::*;

#[test]
fn a_search_keeps_what_is_above_the_floor_and_near_the_best_best_first() {
    let answer = ranked(vec![("dull", 0.05), ("near", 0.08), ("best", 0.11), ("far", 0.065)]);
    assert_eq!(answer.iter().map(|(name, _)| *name).collect::<Vec<_>>(), ["best", "near"]);

    let answer = ranked(vec![("a", 0.072), ("b", 0.071), ("c", 0.069)]);
    assert_eq!(answer.len(), 2);
    assert!(ranked(Vec::<((), f32)>::new()).is_empty());
}

#[test]
fn a_photograph_has_its_few_best_things_where_they_score_high_enough() {
    let unit = |at: usize| {
        let mut numbers = vec![0.0; 4];
        numbers[at] = 1.0;
        numbers
    };
    let things = vec![unit(0), unit(1), unit(2), unit(3)];
    let photo = [0.5, 0.12, 0.3, 0.2];

    assert_eq!(saw(&photo, &things), [0, 2, 3]);
    assert_eq!(saw(&[0.05, 0.09, 0.0, 0.0], &things), Vec::<usize>::new());
}

#[test]
fn the_index_keeps_what_was_read_until_the_file_changes() {
    let root = std::env::temp_dir().join(format!("numa-words-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).unwrap();
    let numbers = |value: f32| (0..LENGTH).map(|at| value * (at % 7) as f32 / 7.0).collect::<Vec<f32>>();
    let mut index = Index::open(&root);
    assert!(index.is_empty());
    index.add(vec![(root.join("a/one.raf"), 10, numbers(0.5)), (root.join("two.jpg"), 20, numbers(-0.25))]).unwrap();
    index.add(vec![(root.join("two.jpg"), 21, numbers(0.75))]).unwrap();

    let again = Index::open(&root);
    assert_eq!(again.len(), 2);
    let one = again.get(&root.join("a/one.raf"), 10).unwrap();
    assert!(one.iter().zip(numbers(0.5)).all(|(got, want)| (got - want).abs() < 1e-3), "half floats, near enough");
    assert!(again.get(&root.join("two.jpg"), 20).is_none(), "the file changed since");
    assert!(again.get(&root.join("two.jpg"), 21).is_some(), "the latest record counts");

    let file = root.join(".numa/words.bin");
    let bytes = std::fs::read(&file).unwrap();
    std::fs::write(&file, &bytes[..bytes.len() - 10]).unwrap();
    let cut = Index::open(&root);
    assert!(cut.get(&root.join("two.jpg"), 20).is_some() && cut.get(&root.join("two.jpg"), 21).is_none());

    std::fs::write(&file, b"NUMAWS0Xwhatever").unwrap();
    let mut other = Index::open(&root);
    assert!(other.is_empty() && !file.exists());
    other.add(vec![(root.join("two.jpg"), 21, numbers(0.75))]).unwrap();
    assert_eq!(Index::open(&root).len(), 1);
    let _ = std::fs::remove_dir_all(&root);
}

const ENGLISH: [(&str, &str); 22] = [
    ("a bride", "wedding8 wedding15 wedding16"), ("wedding bouquet", "wedding9"), ("heron", "wildlife0 wildlife3 wildlife4 wildlife10"),
    ("bird", "wildlife0 wildlife3 wildlife4 wildlife10"), ("kitchen", "realestate15 realestate19 realestate21 realestate23"),
    ("living room", "realestate1 realestate3"), ("sneakers", "product13 product14 product16 product17 product18"),
    ("perfume bottle", "product9"), ("cupcakes", "food2 food3 food7"), ("eggs for breakfast", "food8 food9 food10"),
    ("basketball game", "sports15"), ("rugby", "sports11"), ("football match", "sports10"), ("band on stage", "concert4 concert10 concert23"),
    ("sparklers at night", "wedding4 event13"), ("mountain lake", "travel0 travel1 travel3 travel5"), ("old town street", "travel13"),
    ("family portrait", "portrait22"), ("sunset", "wedding5"), ("people at a party", "event6"), ("church ceremony", "wedding23 wedding15"),
    ("forest", "travel9"),
];
const DUTCH: [(&str, &str); 18] = [
    ("een bruid", "wedding8 wedding15 wedding16"), ("bruidsboeket", "wedding9"), ("reiger", "wildlife0 wildlife3 wildlife4 wildlife10"),
    ("keuken", "realestate15 realestate19 realestate21 realestate23"), ("woonkamer", "realestate1 realestate3"),
    ("sportschoenen", "product13 product14 product16 product17 product18"), ("parfumflesje", "product9"), ("cupcakes", "food2 food3 food7"),
    ("eieren bij het ontbijt", "food8 food9 food10"), ("basketbalwedstrijd", "sports15"), ("band op het podium", "concert4 concert10 concert23"),
    ("sterretjes in de nacht", "wedding4 event13"), ("bergmeer", "travel0 travel1 travel3 travel5"), ("oude straat in de stad", "travel13"),
    ("familieportret", "portrait22"), ("zonsondergang", "wedding5"), ("kerkelijke ceremonie", "wedding23 wedding15"), ("bos", "travel9"),
];

fn peak_memory_mb() -> u64 {
    let status = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
    status.lines().find(|line| line.starts_with("VmHWM:")).and_then(|line| line.split_whitespace().nth(1)?.parse::<u64>().ok()).unwrap_or(0) / 1024
}

fn read_folder(dir: &Path, batch: usize) -> Vec<(String, Vec<f32>)> {
    use rayon::prelude::*;
    let mut files: Vec<PathBuf> = walk(dir);
    files.sort();
    let started = std::time::Instant::now();
    let mut out = Vec::new();

    for chunk in files.chunks(32) {
        let images: Vec<RgbImage> = numa_core::power::background(|| {
            chunk.par_iter().map(|path| crate::raw::load_thumbnail(path, crate::thumbs::LIBRARY_EDGE).unwrap()).collect()
        });
        let numbers: Vec<Vec<f32>> = numa_core::power::background(|| images.chunks(batch).flat_map(|run| embed_images(run).unwrap()).collect());
        for (path, numbers) in chunk.iter().zip(numbers) {
            out.push((path.file_stem().unwrap().to_string_lossy().into_owned(), numbers));
        }
    }
    let took = started.elapsed().as_secs_f64();
    println!(
        "{}: {} photographs in {took:.1} s = {:.1} a second, batch {batch}, {} model threads, peak {} MB",
        dir.display(),
        files.len(),
        files.len() as f64 / took,
        std::env::var("NUMA_MODEL_THREADS").unwrap_or_else(|_| "all".into()),
        peak_memory_mb()
    );
    out
}

fn walk(dir: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(dir)
        .unwrap()
        .flatten()
        .map(|entry| entry.path())
        .flat_map(|path| match path.is_dir() {
            true if path.file_name().is_some_and(|name| name != crate::catalog::LIBRARY_DIR) => walk(&path),
            true => vec![],
            false => vec![path],
        })
        .filter(|path| crate::raw::is_raw(path) || path.extension().is_some_and(|ext| ext.eq_ignore_ascii_case("jpg")))
        .collect()
}

#[test]
#[ignore]
fn words_measure() {
    let batch = std::env::var("WORDS_BATCH").ok().and_then(|value| value.parse().ok()).unwrap_or(1);
    for dir in std::env::var("WORDS_FOLDERS").unwrap_or_default().split(':').filter(|dir| !dir.is_empty()) {
        read_folder(Path::new(dir), batch);
    }
    let photos = read_folder(Path::new(&std::env::var("WORDS_CC0").unwrap()), batch);
    VISION_MODEL.release();
    let names: Vec<&str> = photos.iter().map(|(name, _)| name.as_str()).collect();
    let started = std::time::Instant::now();
    let mut asked = 0;
    let mut nothing = 0.0;
    for (language, searches) in [("English", &ENGLISH[..]), ("Dutch", &DUTCH[..])] {
        let (mut first, mut reciprocal, mut shown, mut right, mut wanted, mut found) = (0, 0.0, 0, 0, 0, 0);
        for (text, answers) in searches {
            let query = embed_text(text).unwrap();
            asked += 1;
            let answers: Vec<&str> = answers.split(' ').collect();
            let mut order: Vec<(usize, f32)> = photos.iter().map(|(_, numbers)| cosine(numbers, &query)).enumerate().collect();
            order.sort_by(|a, b| b.1.total_cmp(&a.1));
            let rank = order.iter().position(|(at, _)| answers.contains(&names[*at])).unwrap() + 1;
            first += (rank == 1) as usize;
            reciprocal += 1.0 / rank as f64;
            let kept = ranked(order);
            shown += kept.len();
            right += kept.iter().filter(|(at, _)| answers.contains(&names[*at])).count();
            wanted += answers.len();
            found += answers.iter().filter(|name| kept.iter().any(|(at, _)| names[*at] == **name)).count();
        }
        println!(
            "{language}: top-1 {first}/{}, MRR {:.2}; above the floor: {shown} shown, {:.0} % of them right, {:.0} % of the right ones shown",
            searches.len(),
            reciprocal / searches.len() as f64,
            right as f64 * 100.0 / shown.max(1) as f64,
            found as f64 * 100.0 / wanted as f64
        );
    }
    for text in ["a dog", "a cat", "a car", "a bicycle", "a horse", "a computer", "a baby", "christmas tree", "hond", "kat", "fiets", "paard"] {
        let query = embed_text(text).unwrap();
        nothing += ranked(photos.iter().map(|(_, numbers)| ((), cosine(numbers, &query))).collect()).len() as f64;
        asked += 1;
    }
    println!(
        "nothing to find: {:.1} photographs shown a search; text {:.1} ms a search; peak {} MB",
        nothing / 12.0,
        started.elapsed().as_secs_f64() * 1000.0 / asked as f64,
        peak_memory_mb()
    );
    let started = std::time::Instant::now();
    let things = things().unwrap();
    println!("the {} things: {:.2} s", things.len(), started.elapsed().as_secs_f64());
    for (name, numbers) in &photos {
        let seen: Vec<&str> = saw(numbers, &things).into_iter().map(|at| THINGS[at]).collect();
        println!("  {name:13} {}", seen.join(", "));
    }
}

#[test]
fn the_things_a_photograph_has_are_looked_at_once_and_again_when_it_changes() {
    let root = std::env::temp_dir().join(format!("numa-words-seen-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&root);
    let unit = |at: usize| (0..LENGTH).map(|index| (index == at) as u8 as f32).collect::<Vec<f32>>();
    let things = vec![unit(0), unit(1)];
    let mut index = Index::open(&root);
    index.add(vec![(root.join("a.jpg"), 1, unit(1))]).unwrap();
    assert!(index.saw(&root.join("a.jpg"), 1).is_none(), "not looked at yet");
    index.see(&things);
    assert_eq!(index.saw(&root.join("a.jpg"), 1), Some(&[1][..]));
    assert!(index.saw(&root.join("a.jpg"), 2).is_none(), "the file changed since");
    index.add(vec![(root.join("a.jpg"), 2, unit(0))]).unwrap();
    index.see(&things);
    assert_eq!(index.saw(&root.join("a.jpg"), 2), Some(&[0][..]));
    assert_eq!(index.seen().count(), 1);
    let _ = std::fs::remove_dir_all(&root);
}
