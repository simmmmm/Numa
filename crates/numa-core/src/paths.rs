use std::path::PathBuf;

pub const NAME: &str = "numa";

pub fn data_dir() -> PathBuf {
    outside_the_sandbox("HOST_XDG_DATA_HOME", ".local/share", dirs::data_dir()).unwrap_or_else(|| PathBuf::from(".")).join(NAME)
}

fn outside_the_sandbox(host: &str, under_home: &str, own: Option<PathBuf>) -> Option<PathBuf> {
    if std::env::var_os("FLATPAK_ID").is_none() {
        return own;
    }
    std::env::var_os(host).map(PathBuf::from).or_else(|| dirs::home_dir().map(|home| home.join(under_home)))
}

pub fn models_dir() -> PathBuf {
    std::env::var_os("NUMA_MODELS").map(PathBuf::from).unwrap_or_else(|| data_dir().join("models"))
}

pub fn model_file(names: &[&str]) -> Option<PathBuf> {
    let dir = models_dir();
    names.iter().map(|name| dir.join(name)).find(|path| path.is_file())
}

pub fn cache_dir() -> PathBuf {
    if let Some(root) = TEST_CACHE.get() {
        return root.clone();
    }
    outside_the_sandbox("HOST_XDG_CACHE_HOME", ".cache", dirs::cache_dir()).unwrap_or_else(std::env::temp_dir).join(NAME)
}

static TEST_CACHE: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

pub fn use_test_cache(root: PathBuf) {
    let _ = TEST_CACHE.set(root);
}
