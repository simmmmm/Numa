use std::path::Path;
use std::process::Command;

use numa_io::models::{self, ModelFile, MIRROR};

use crate::activity::Download;

const WANTED: [&str; 4] = ["YuNet", "SFace", "Open-closed eye", numa_io::words::MODEL];

pub fn fetch(server: &crate::Server) {
    let mut files = models::files_of(&WANTED);
    if !numa_io::dcp::downloaded_profiles_dir().is_dir() {
        files.push(models::PROFILES);
    }
    files.retain(|entry| entry.0.ends_with(".tar.gz") || models::on_disk(entry).is_none());

    *server.activity.downloads() = files
        .iter()
        .map(|entry @ (file, _, size, _)| Download { name: name_of(file), total: *size, parts: parts(entry), state: "waiting" })
        .collect();
    for (at, entry) in files.iter().enumerate() {
        server.activity.hold();
        server.activity.downloads()[at].state = "fetching";
        let started = std::time::Instant::now();
        let fetched = fetch_one(entry);
        server.activity.downloads()[at].state = if fetched.is_ok() { "done" } else { "failed" };
        match fetched {
            Ok(()) => log::info!("fetched {} in {:.0} s", entry.0, started.elapsed().as_secs_f32()),
            Err(err) => log::warn!("could not fetch {}: {err}; what needs it waits for the next start", entry.0),
        }
    }
}

fn name_of(file: &str) -> String {
    let model = models::MODELS.iter().find(|(_, _, files)| files.iter().any(|entry| entry.0 == file));
    model.map_or_else(|| "Camera profiles".to_string(), |(name, _, _)| name.to_string())
}

fn parts((file, _, _, _): &ModelFile) -> Vec<std::path::PathBuf> {
    let dir = if file.ends_with(".tar.gz") { numa_core::paths::data_dir() } else { numa_core::paths::models_dir() };
    let mut parts = vec![dir.join(format!("{file}.part"))];
    if let Some((_, name, ..)) = models::packed(file) {
        parts.push(dir.join(format!("{name}.part")));
    }
    parts
}

fn fetch_one(entry @ (file, _, _, sha256): &ModelFile) -> Result<(), String> {
    let archive = file.ends_with(".tar.gz");
    let dir = if archive { numa_core::paths::data_dir() } else { numa_core::paths::models_dir() };
    std::fs::create_dir_all(&dir).map_err(|err| err.to_string())?;
    let part = dir.join(format!("{file}.part"));
    let mut fetched = false;
    if let Some((_, name, _, packed_sha, unpacked_sha)) = models::packed(file) {
        let zst = dir.join(format!("{name}.part"));
        let unpacked = curl(&zst, &format!("{MIRROR}/{name}"))
            .and_then(|()| checked(&zst, packed_sha))
            .and_then(|()| models::unpack(&zst, &part).map_err(|err| err.to_string()))
            .and_then(|()| checked(&part, unpacked_sha));
        let _ = std::fs::remove_file(&zst);
        match unpacked {
            Ok(()) => fetched = true,
            Err(err) => {
                log::info!("{name}: {err}; fetching {file} itself");
                let _ = std::fs::remove_file(&part);
            }
        }
    }
    let mut error = String::new();
    let links = if fetched { Vec::new() } else { models::links(entry) };
    for url in &links {
        match curl(&part, url).and_then(|()| checked(&part, sha256)) {
            Ok(()) => {
                fetched = true;
                break;
            }

            Err(err) => {
                let _ = std::fs::remove_file(&part);
                error = err;
            }
        }
    }
    if !fetched {
        return Err(error);
    }
    if archive {
        let unpacked = run(Command::new("tar").arg("-xzf").arg(&part).arg("-C").arg(&dir));
        let _ = std::fs::remove_file(&part);
        return unpacked;
    }
    std::fs::rename(&part, dir.join(file)).map_err(|err| err.to_string())
}

fn curl(part: &Path, url: &str) -> Result<(), String> {
    run(Command::new("curl").args(["--fail", "--location", "--silent", "--show-error", "--retry", "3", "--continue-at", "-", "--output"]).arg(part).arg(url))
}

fn run(command: &mut Command) -> Result<(), String> {
    let out = command.output().map_err(|err| format!("{:?} would not start: {err}", command.get_program()))?;
    match out.status.success() {
        true => Ok(()),
        false => Err(String::from_utf8_lossy(&out.stderr).trim().to_string()),
    }
}

fn checked(path: &Path, sha256: &str) -> Result<(), String> {
    match crate::auth::sha256_of(path) {
        Ok(got) if got == sha256 => Ok(()),
        Ok(got) => Err(format!("checksum mismatch ({got})")),
        Err(err) => Err(err.to_string()),
    }
}
