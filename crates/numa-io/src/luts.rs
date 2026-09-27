use std::path::Path;

use numa_core::lut::Lut;

pub fn list() -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(numa_core::paths::luts_dir())
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| readable(name))
        .collect();
    names.sort_by_key(|name| name.to_lowercase());
    names
}

fn readable(name: &str) -> bool {
    let lower = name.to_lowercase();
    lower.ends_with(".cube") || lower.ends_with(".3dl")
}

pub fn import(path: &Path) -> Result<String, String> {
    let name = path.file_name().and_then(|name| name.to_str()).ok_or("a file without a name")?.to_string();
    if !readable(&name) {
        return Err(format!("{name} is not a .cube or .3dl file"));
    }
    let text = std::fs::read_to_string(path).map_err(|err| format!("{name}: {err}"))?;
    Lut::parse(&name, &text).map_err(|err| format!("{name}: {err}"))?;
    let dir = numa_core::paths::luts_dir();
    std::fs::create_dir_all(&dir).map_err(|err| format!("{}: {err}", dir.display()))?;
    std::fs::write(dir.join(&name), text).map_err(|err| format!("{name}: {err}"))?;
    Ok(name)
}
