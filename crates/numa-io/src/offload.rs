use std::ffi::OsString;
use std::fs::File;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::thread::JoinHandle;

use rayon::prelude::*;
use twox_hash::XxHash3_128;

use crate::import::{free_name, same_file};
use crate::raw;

const CHUNK: usize = 4 << 20;

const DECODERS: usize = 4;

pub fn hex(hash: u128) -> String {
    format!("{hash:032x}")
}

pub fn copy_hashed(from: &Path, to: &Path) -> io::Result<u128> {
    if let Some(parent) = to.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let mut part = to.as_os_str().to_owned();
    part.push(".part");
    let part = PathBuf::from(part);
    let result = (|| {
        let mut source = File::open(from)?;
        let meta = source.metadata()?;
        let mut out = File::create(&part)?;
        let (hash, length) = hash_through(&mut source, Some(&mut out))?;
        if length != meta.len() {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                format!("{length} of its {} bytes could be read", meta.len()),
            ));
        }
        out.set_modified(meta.modified()?)?;
        out.sync_all()?;
        drop(out);
        std::fs::rename(&part, to)?;
        Ok(hash)
    })();
    if result.is_err() {
        let _ = std::fs::remove_file(&part);
    }
    result
}

pub fn hash_file(path: &Path) -> io::Result<u128> {
    let mut file = File::open(path)?;
    uncache(&file);
    hash_through(&mut file, None).map(|(hash, _)| hash)
}

fn hash_through(reader: &mut impl Read, mut out: Option<&mut File>) -> io::Result<(u128, u64)> {
    let mut hasher = XxHash3_128::new();
    let mut buffer = vec![0u8; CHUNK];
    let mut length = 0u64;
    loop {
        let read = match reader.read(&mut buffer) {
            Ok(0) => break,
            Ok(read) => read,
            Err(err) if err.kind() == io::ErrorKind::Interrupted => continue,
            Err(err) => return Err(err),
        };
        hasher.write(&buffer[..read]);
        if let Some(out) = out.as_mut() {
            out.write_all(&buffer[..read])?;
        }
        length += read as u64;
    }
    Ok((hasher.finish_128(), length))
}

#[cfg(target_os = "linux")]
fn uncache(file: &File) {
    use std::os::fd::AsRawFd;

    unsafe {
        libc::posix_fadvise(file.as_raw_fd(), 0, 0, libc::POSIX_FADV_DONTNEED);
    }
}

#[cfg(not(target_os = "linux"))]
fn uncache(_: &File) {}

pub fn second_folder(root: &Path, folder: &Path) -> PathBuf {
    let name = folder.file_name().unwrap_or_default();
    let year = folder
        .parent()
        .and_then(Path::file_name)
        .filter(|year| year.len() == 4 && year.to_str().is_some_and(|year| year.chars().all(|c| c.is_ascii_digit())));
    match year {
        Some(year) => root.join(year).join(name),
        None => root.join(name),
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum Target {

    Free(PathBuf, OsString),

    Exactly(PathBuf),
}

#[derive(Debug, Clone)]
pub struct Item {
    pub card: PathBuf,
    pub size: u64,
    pub to: Target,
    pub second: Option<Target>,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Fault {
    NotCopied(String),

    Differs,
    SecondNotCopied(String),
    SecondDiffers,

    Unreadable(String),

    NoMatch,

    Unchecked(String),
}

#[derive(Debug, Clone)]
pub struct Problem {
    pub card: PathBuf,
    pub to: PathBuf,
    pub fault: Fault,
}

#[derive(Debug, Clone)]
pub struct Landed {
    pub card: PathBuf,
    pub size: u64,
    pub to: PathBuf,
    pub hash: u128,

    pub second: Option<PathBuf>,

    pub second_target: Option<Target>,

    pub rating: Option<u8>,
}

#[derive(Debug, Default)]
pub struct Report {

    pub check: bool,

    pub second: bool,
    pub landed: Vec<Landed>,

    pub existing: Vec<(PathBuf, PathBuf, u64)>,
    pub problems: Vec<Problem>,

    pub decoded: usize,
    pub stopped: bool,
}

impl Report {

    pub fn safe_to_format(&self) -> bool {
        self.check
            && self.second
            && !self.stopped
            && self.problems.is_empty()
            && !self.landed.is_empty()
            && self.landed.iter().all(|landed| landed.second.is_some())
    }

    pub fn again(&self) -> Vec<Item> {
        let mut items: Vec<Item> = Vec::new();
        for problem in &self.problems {
            if items.iter().any(|item| item.card == problem.card) {
                continue;
            }
            let landed = self.landed.iter().find(|landed| landed.to == problem.to);
            let second = landed.and_then(|landed| {
                landed.second.clone().map(Target::Exactly).or_else(|| landed.second_target.clone())
            });
            items.push(Item {
                card: problem.card.clone(),
                size: landed.map_or(0, |landed| landed.size),
                to: Target::Exactly(problem.to.clone()),
                second,
            });
        }
        items
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Stage {
    Library,
    Second,
    Card,
}

pub type Step = Arc<dyn Fn(Stage) + Send + Sync>;

pub struct Later {
    second: JoinHandle<Vec<Result<(PathBuf, PathBuf), Problem>>>,
    card: JoinHandle<(usize, Vec<(PathBuf, String)>)>,
}

type Onward = (PathBuf, u128, Target, PathBuf);

pub fn run(items: Vec<Item>, check: bool, stop: Arc<AtomicBool>, step: Step) -> (Report, Later) {
    let mut report = Report {
        check,
        second: items.iter().any(|item| item.second.is_some()),
        ..Default::default()
    };
    let (onward, second_rx) = mpsc::channel::<Onward>();
    let (decode, card_rx) = mpsc::channel::<PathBuf>();
    let second_hand = {
        let (stop, step) = (stop.clone(), step.clone());
        std::thread::spawn(move || second_copies(second_rx, check, &stop, &step))
    };
    let card_hand = {
        let (stop, step) = (stop.clone(), step.clone());
        std::thread::spawn(move || decode_all(card_rx, &stop, &step))
    };
    let skip = |stages: &[Stage]| stages.iter().for_each(|stage| step(*stage));

    for Item { card, size, to, second } in items {
        if stop.load(Ordering::Relaxed) {
            report.stopped = true;
            break;
        }
        let to = match to {
            Target::Exactly(path) => path,
            Target::Free(folder, name) => match free_name(&folder, &name, |there| same_file(&card, size, there)) {
                (path, true) => {
                    report.existing.push((card, path, size));
                    skip(&[Stage::Library, Stage::Second, Stage::Card]);
                    continue;
                }
                (path, false) => path,
            },
        };
        let checked = copy_hashed(&card, &to).map_err(|err| Fault::NotCopied(err.to_string())).and_then(|hash| {
            match check.then(|| hash_file(&to)) {
                None => Ok(hash),
                Some(Ok(back)) if back == hash => Ok(hash),
                Some(Ok(_)) => {

                    let _ = std::fs::remove_file(&to);
                    Err(Fault::Differs)
                }
                Some(Err(err)) => Err(Fault::NotCopied(format!("read back: {err}"))),
            }
        });
        let hash = match checked {
            Ok(hash) => hash,
            Err(fault) => {
                log::warn!("{}: {fault:?}", card.display());
                report.problems.push(Problem { card, to, fault });
                skip(&[Stage::Library, Stage::Second, Stage::Card]);
                continue;
            }
        };
        step(Stage::Library);
        match raw::is_raw(&to) && decode.send(to.clone()).is_ok() {
            true => {}
            false => step(Stage::Card),
        }
        match &second {
            Some(target) if onward.send((to.clone(), hash, target.clone(), card.clone())).is_ok() => {}
            _ => step(Stage::Second),
        }
        let rating = crate::exif::rating(&to);
        report.landed.push(Landed { card, size, to, hash, second: None, second_target: second, rating });
    }
    (report, Later { second: second_hand, card: card_hand })
}

fn second_copies(onward: mpsc::Receiver<Onward>, check: bool, stop: &AtomicBool, step: &Step) -> Vec<Result<(PathBuf, PathBuf), Problem>> {
    let mut out = Vec::new();
    for (from, hash, target, card) in onward {
        if stop.load(Ordering::Relaxed) {
            step(Stage::Second);
            continue;
        }
        let (there, already) = match target {
            Target::Exactly(path) => (path, false),

            Target::Free(folder, _) => {
                free_name(&folder, from.file_name().unwrap_or_default(), |there| hash_file(there).is_ok_and(|other| other == hash))
            }
        };
        let fault = if already {
            None
        } else {
            match copy_hashed(&from, &there) {
                Err(err) => Some(Fault::SecondNotCopied(err.to_string())),
                Ok(copied) if copied != hash => Some(Fault::SecondDiffers),
                Ok(_) => match check.then(|| hash_file(&there)) {
                    None => None,
                    Some(Ok(back)) if back == hash => None,
                    Some(Ok(_)) => Some(Fault::SecondDiffers),
                    Some(Err(err)) => Some(Fault::SecondNotCopied(format!("read back: {err}"))),
                },
            }
        };
        out.push(match fault {
            None => Ok((from, there)),
            Some(fault) => {
                log::warn!("{}: {fault:?}", there.display());
                if fault == Fault::SecondDiffers {
                    let _ = std::fs::remove_file(&there);
                }
                Err(Problem { card, to: from, fault })
            }
        });
        step(Stage::Second);
    }
    out
}

fn decode_all(paths: mpsc::Receiver<PathBuf>, stop: &AtomicBool, step: &Step) -> (usize, Vec<(PathBuf, String)>) {
    let one = |path: PathBuf| {
        let result = match stop.load(Ordering::Relaxed) {
            true => None,
            false => Some(raw::readable(&path).map_err(|err| {
                log::warn!("{} does not decode: {err}", path.display());
                (path, err)
            })),
        };
        step(Stage::Card);
        result
    };
    let results: Vec<Result<(), (PathBuf, String)>> = match rayon::ThreadPoolBuilder::new().num_threads(DECODERS).build() {
        Ok(pool) => pool.install(|| paths.into_iter().par_bridge().filter_map(one).collect()),
        Err(_) => paths.into_iter().filter_map(one).collect(),
    };
    let decoded = results.iter().filter(|result| result.is_ok()).count();
    (decoded, results.into_iter().filter_map(Result::err).collect())
}

#[derive(Debug, Default)]
pub struct Older {

    pub checked: Vec<(u128, PathBuf, Option<PathBuf>)>,
    pub problems: Vec<Problem>,
    pub stopped: bool,
}

impl Older {

    pub fn safe(&self) -> bool {
        !self.stopped && self.problems.is_empty()
    }
}

pub fn check_older(older: &[(PathBuf, PathBuf)], second_root: Option<&Path>, stop: &AtomicBool, step: impl Fn()) -> Older {
    let mut out = Older::default();
    for (card, library) in older {
        if stop.load(Ordering::Relaxed) {
            out.stopped = true;
            break;
        }
        let problem = |fault| Problem { card: card.clone(), to: library.clone(), fault };
        let (on_card, in_library) = rayon::join(|| hash_file(card), || hash_file(library));
        let hash = match on_card {
            Ok(hash) => hash,
            Err(err) => {
                out.problems.push(problem(Fault::Unchecked(format!("the card did not read: {err}"))));
                step();
                continue;
            }
        };
        let second = second_root
            .and_then(|root| Some(second_folder(root, library.parent()?).join(library.file_name()?)))
            .filter(|there| there.is_file());
        let in_second = second.as_ref().map(|there| hash_file(there).ok() == Some(hash));
        match (in_library.ok() == Some(hash), in_second) {
            (true, None | Some(true)) => out.checked.push((hash, library.clone(), second)),
            (false, Some(true)) => out.problems.push(problem(Fault::Differs)),
            (true, Some(false)) => out.problems.push(problem(Fault::SecondDiffers)),
            (false, _) => out.problems.push(problem(Fault::NoMatch)),
        }
        step();
    }
    out
}

impl Later {

    pub fn join(self, mut report: Report) -> Report {
        for result in self.second.join().unwrap_or_default() {
            match result {
                Ok((to, there)) => {
                    if let Some(landed) = report.landed.iter_mut().find(|landed| landed.to == to) {
                        landed.second = Some(there);
                    }
                }
                Err(problem) => report.problems.push(problem),
            }
        }
        let (decoded, unreadable) = self.card.join().unwrap_or_default();
        report.decoded = decoded;
        for (to, err) in unreadable {
            let card = report.landed.iter().find(|landed| landed.to == to).map(|landed| landed.card.clone());
            report.problems.push(Problem { card: card.unwrap_or_else(|| to.clone()), to, fault: Fault::Unreadable(err) });
        }
        if report.problems.is_empty() && report.landed.iter().any(|landed| landed.second.is_none() && landed.second_target.is_some()) {

            report.stopped = true;
        }
        report
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[ignore]
    fn measure_a_card() {
        let (Some(card), Some(to)) = (std::env::var_os("NUMA_CARD"), std::env::var_os("NUMA_CARD_TO")) else {
            println!("NUMA_CARD and NUMA_CARD_TO are not set");
            return;
        };
        let (card, to) = (PathBuf::from(card), PathBuf::from(to));
        let files: Vec<PathBuf> = std::fs::read_dir(&card).unwrap().flatten().map(|entry| entry.path()).filter(|path| raw::is_raw(path)).collect();
        let bytes: u64 = files.iter().map(|path| std::fs::metadata(path).unwrap().len()).sum();
        let cold = || {
            for path in files.iter().chain(std::fs::read_dir(&to).into_iter().flatten().flatten().map(|entry| entry.path()).collect::<Vec<_>>().iter()) {
                if let Ok(file) = File::open(path) {
                    uncache(&file);
                }
            }
        };
        let fresh = || {
            let _ = std::fs::remove_dir_all(&to);
            std::fs::create_dir_all(to.join("library")).unwrap();
            cold();
        };
        println!("{} raws, {:.2} GB", files.len(), bytes as f64 / 1e9);
        fresh();
        let started = std::time::Instant::now();
        for path in &files {
            std::fs::copy(path, to.join("library").join(path.file_name().unwrap())).unwrap();
        }
        let plain = started.elapsed();
        println!("plain copy               {plain:>8.2?}");
        for (label, second, check) in [("checked, one place", false, true), ("checked, two places", true, true), ("two places, no read-back", true, false)] {
            fresh();
            let items: Vec<Item> = files
                .iter()
                .map(|path| Item {
                    card: path.clone(),
                    size: 0,
                    to: Target::Free(to.join("library"), path.file_name().unwrap().into()),
                    second: second.then(|| Target::Free(to.join("second"), OsString::new())),
                })
                .collect();
            let started = std::time::Instant::now();
            let (report, later) = run(items, check, Arc::default(), Arc::new(|_| {}));
            let library = started.elapsed();
            let report = later.join(report);
            let all = started.elapsed();
            assert!(report.problems.is_empty(), "{:?}", report.problems);
            println!("{label:<24} {library:>8.2?} library, {all:>8.2?} with the second place and the decode");
        }
        let _ = std::fs::remove_dir_all(&to);
    }

    #[test]
    fn a_second_copy_keeps_the_year() {
        let root = Path::new("/backup");
        assert_eq!(second_folder(root, Path::new("/Fotos/2026/Mallorca")), Path::new("/backup/2026/Mallorca"));
        assert_eq!(second_folder(root, Path::new("/photos/Aad")), Path::new("/backup/Aad"));
    }

    #[test]
    fn the_hash_is_xxh128sums() {
        let dir = std::env::temp_dir().join(format!("numa-offload-hash-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("a"), b"hello").unwrap();

        assert_eq!(hex(hash_file(&dir.join("a")).unwrap()), "b5e9c1ad071b3e7fc779cfaa5e523818");
        assert_eq!(hex(copy_hashed(&dir.join("a"), &dir.join("b/a")).unwrap()), "b5e9c1ad071b3e7fc779cfaa5e523818");
        assert_eq!(std::fs::read(dir.join("b/a")).unwrap(), b"hello");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn two_copies_both_checked_and_a_bad_frame_named() {
        let dir = std::env::temp_dir().join(format!("numa-offload-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let (card, library, second) = (dir.join("card/DCIM/100_FUJI"), dir.join("Mallorca"), dir.join("backup/Mallorca"));
        std::fs::create_dir_all(&card).unwrap();

        std::fs::write(card.join("DSCF0001.RAF"), vec![7u8; 300_000]).unwrap();
        std::fs::write(card.join("DSCF0002.JPG"), vec![9u8; 1000]).unwrap();
        let item = |name: &str| Item {
            card: card.join(name),
            size: std::fs::metadata(card.join(name)).unwrap().len(),
            to: Target::Free(library.clone(), name.into()),
            second: Some(Target::Free(second.clone(), name.into())),
        };
        let counted = Arc::new(std::sync::atomic::AtomicUsize::new(0));
        let step: Step = {
            let counted = counted.clone();
            Arc::new(move |_| {
                counted.fetch_add(1, Ordering::Relaxed);
            })
        };
        let (report, later) = run(vec![item("DSCF0001.RAF"), item("DSCF0002.JPG")], true, Arc::default(), step.clone());
        assert_eq!(report.landed.len(), 2);
        let report = later.join(report);
        assert_eq!(counted.load(Ordering::Relaxed), 6, "one step per photograph per hand");
        assert_eq!(std::fs::read(second.join("DSCF0002.JPG")).unwrap(), vec![9u8; 1000]);
        assert!(report.landed.iter().all(|landed| landed.second.is_some()), "both in the second place");
        assert_eq!(report.problems.len(), 1);
        assert!(matches!(report.problems[0].fault, Fault::Unreadable(_)));
        assert!(!report.safe_to_format(), "a frame that does not decode is never safe");

        let again = report.again();
        assert_eq!(again.len(), 1);
        assert_eq!(again[0].to, Target::Exactly(library.join("DSCF0001.RAF")));
        assert_eq!(again[0].second, Some(Target::Exactly(second.join("DSCF0001.RAF"))));
        let (retried, later) = run(again, true, Arc::default(), step);
        let retried = later.join(retried);
        assert_eq!(retried.landed.len(), 1);
        assert!(!library.join("DSCF0001-2.RAF").exists() && !second.join("DSCF0001-2.RAF").exists());

        std::fs::write(card.join("DSCF0004.JPG"), b"older").unwrap();
        std::fs::write(library.join("DSCF0004.JPG"), b"older").unwrap();
        std::fs::write(card.join("DSCF0005.JPG"), b"older too").unwrap();
        std::fs::write(library.join("DSCF0005.JPG"), b"bit rot!!").unwrap();
        let older = [(card.join("DSCF0004.JPG"), library.join("DSCF0004.JPG")), (card.join("DSCF0005.JPG"), library.join("DSCF0005.JPG"))];
        let checked = check_older(&older, None, &AtomicBool::new(false), || {});
        assert_eq!(checked.checked.len(), 1);
        assert_eq!(checked.problems.len(), 1);
        assert_eq!(checked.problems[0].fault, Fault::NoMatch);
        assert!(!checked.safe());

        std::fs::write(card.join("DSCF0003.JPG"), b"good").unwrap();
        std::fs::write(library.join("DSCF0003.JPG"), b"half").unwrap();
        let only = Item { to: Target::Exactly(library.join("DSCF0003.JPG")), second: None, ..item("DSCF0003.JPG") };
        let (fine, later) = run(vec![only], true, Arc::default(), Arc::new(|_| {}));
        let fine = later.join(fine);
        assert!(fine.problems.is_empty(), "overwritten whole and read back the same");
        assert!(!fine.safe_to_format(), "one place is not safe to format");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
