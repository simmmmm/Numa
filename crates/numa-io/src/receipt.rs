use std::io;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use crate::catalog::LIBRARY_DIR;
use crate::offload::{hash_file, hex};

pub fn library_dir(root: &Path) -> PathBuf {
    root.join(LIBRARY_DIR).join("receipts")
}

pub fn second_dir(root: &Path) -> PathBuf {
    root.join(".numa-receipts")
}

pub fn write(root: &Path, dir: &Path, stamp: &str, header: &[String], files: &[(u128, PathBuf)]) -> io::Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let mut path = dir.join(format!("{stamp}.xxh128"));
    let mut suffix = 2;
    while path.exists() {
        path = dir.join(format!("{stamp}-{suffix}.xxh128"));
        suffix += 1;
    }
    let mut text: String = header.iter().map(|line| format!("# {line}\n")).collect();
    let receipt = path.strip_prefix(root).unwrap_or(&path);
    text.push_str(&format!("# Check: cd '{}' && xxh128sum -c '{}'\n", root.display(), receipt.display()));
    for (hash, file) in files {
        let file = file.strip_prefix(root).unwrap_or(file);

        text.push_str(&format!("{}  {}\n", hex(*hash), file.display()));
    }
    std::fs::write(&path, text)?;
    Ok(path)
}

pub type Entry = (u128, PathBuf, PathBuf, Option<PathBuf>);

pub fn write_all(files: &[Entry], second_root: Option<&Path>, stamp: &str, header: &[String]) -> Vec<PathBuf> {
    let mut roots: Vec<&Path> = Vec::new();
    for (_, root, _, _) in files {
        if !roots.contains(&root.as_path()) {
            roots.push(root);
        }
    }
    let mut written = Vec::new();
    for root in roots {
        let here: Vec<(u128, PathBuf)> =
            files.iter().filter(|(_, of, _, _)| of == root).map(|(hash, _, file, _)| (*hash, file.clone())).collect();
        written.push(write(root, &library_dir(root), stamp, header, &here));
    }
    if let Some(root) = second_root {
        let there: Vec<(u128, PathBuf)> = files.iter().filter_map(|(hash, _, _, second)| Some((*hash, second.clone()?))).collect();
        if !there.is_empty() {
            written.push(write(root, &second_dir(root), stamp, header, &there));
        }
    }
    written
        .into_iter()
        .filter_map(|result| result.map_err(|err| log::warn!("receipt: {err}")).ok())
        .collect()
}

pub fn read(receipt: &Path, root: &Path) -> Vec<(u128, PathBuf)> {
    let Ok(text) = std::fs::read_to_string(receipt) else { return Vec::new() };
    text.lines()
        .filter(|line| !line.starts_with('#'))
        .filter_map(|line| {
            let (hash, file) = line.split_once("  ")?;
            Some((u128::from_str_radix(hash, 16).ok()?, root.join(file)))
        })
        .collect()
}

pub fn all(root: &Path) -> Vec<PathBuf> {
    [library_dir(root), second_dir(root)]
        .iter()
        .filter_map(|dir| std::fs::read_dir(dir).ok())
        .flat_map(|entries| entries.flatten().map(|entry| entry.path()))
        .filter(|path| path.extension().is_some_and(|ext| ext == "xxh128"))
        .collect()
}

pub fn recheck(roots: &[PathBuf], stop: &AtomicBool) -> (usize, Vec<PathBuf>) {
    background();
    let (mut seen, mut differ) = (0, Vec::new());
    for root in roots {
        for receipt in all(root) {
            for (hash, file) in read(&receipt, root) {
                if stop.load(Ordering::Relaxed) {
                    return (seen, differ);
                }
                match hash_file(&file) {
                    Ok(now) if now == hash => seen += 1,
                    Ok(_) => {
                        seen += 1;
                        differ.push(file);
                    }
                    Err(err) if err.kind() == io::ErrorKind::NotFound => {}

                    Err(_) => differ.push(file),
                }
            }
        }
    }
    (seen, differ)
}

#[cfg(target_os = "linux")]
fn background() {
    const IOPRIO_WHO_PROCESS: libc::c_int = 1;
    const IOPRIO_CLASS_IDLE: libc::c_int = 3 << 13;

    unsafe {
        libc::setpriority(libc::PRIO_PROCESS, 0, 19);
        libc::syscall(libc::SYS_ioprio_set, IOPRIO_WHO_PROCESS, 0, IOPRIO_CLASS_IDLE);
    }
}

#[cfg(not(target_os = "linux"))]
fn background() {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_receipt_is_xxh128sums_and_a_recheck_finds_what_changed() {
        let root = std::env::temp_dir().join(format!("numa-receipt-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).unwrap();
        let files: Vec<(u128, PathBuf)> = ["a.RAF", "b.RAF", "c.RAF"]
            .iter()
            .map(|name| {
                std::fs::write(root.join(name), name.as_bytes()).unwrap();
                (hash_file(&root.join(name)).unwrap(), root.join(name))
            })
            .collect();
        let receipt = write(&root, &library_dir(&root), "2026-10-04-1530", &["Numa import".into()], &files).unwrap();
        let text = std::fs::read_to_string(&receipt).unwrap();
        assert!(text.starts_with("# Numa import\n# Check: cd "));
        assert!(text.contains(&format!("{}  a.RAF\n", hex(files[0].0))));

        if let Ok(out) = std::process::Command::new("xxh128sum").arg("-c").arg(&receipt).current_dir(&root).output() {
            assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stdout));
        }
        assert_eq!(read(&receipt, &root), files);
        assert_eq!(all(&root), vec![receipt.clone()]);
        assert_eq!(write(&root, &library_dir(&root), "2026-10-04-1530", &[], &[]).unwrap().file_name().unwrap(), "2026-10-04-1530-2.xxh128");

        std::fs::write(root.join("b.RAF"), b"bitrot").unwrap();
        std::fs::remove_file(root.join("c.RAF")).unwrap();
        let (read, differ) = recheck(&[root.clone()], &AtomicBool::new(false));
        assert_eq!((read, differ), (2, vec![root.join("b.RAF")]));
        std::fs::remove_dir_all(&root).unwrap();
    }
}
