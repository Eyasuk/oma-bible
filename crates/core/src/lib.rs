pub mod bible;
pub mod bundle;
pub mod direction;
pub mod fonts;
pub mod original;
pub mod remote;
pub mod storage;
pub mod study;
pub mod theme;

use std::path::PathBuf;

/// Directory where downloaded translations are written.
///
/// Every translation comes from the Bible API; none are built in. Downloads
/// live under the XDG data directory by default. `OMA_BIBLE_DATA_DIR` can
/// still take over so an existing override keeps working, and
/// `OMA_BIBLE_DOWNLOAD_DIR` wins when it is set.
pub fn user_data_dir() -> PathBuf {
    if let Some(path) = std::env::var_os("OMA_BIBLE_DOWNLOAD_DIR") {
        return PathBuf::from(path);
    }

    if let Some(path) = std::env::var_os("OMA_BIBLE_DATA_DIR") {
        return PathBuf::from(path);
    }

    xdg_dir("XDG_DATA_HOME", ".local/share").join("oma-bible")
}

/// Directory for downloaded study data (cross-references, commentaries).
pub fn cache_dir() -> PathBuf {
    if let Some(path) = std::env::var_os("OMA_BIBLE_CACHE_DIR") {
        return PathBuf::from(path);
    }

    xdg_dir("XDG_CACHE_HOME", ".cache").join("oma-bible")
}

/// Directory for user settings.
/// Sample translations used by the tests.
#[cfg(test)]
pub(crate) fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures")
}

pub fn config_dir() -> PathBuf {
    if let Some(path) = std::env::var_os("OMA_BIBLE_CONFIG_DIR") {
        return PathBuf::from(path);
    }

    xdg_dir("XDG_CONFIG_HOME", ".config").join("oma-bible")
}

/// Directory for bookmarks and other per-machine state.
pub fn state_dir() -> PathBuf {
    if let Some(path) = std::env::var_os("OMA_BIBLE_STATE_DIR") {
        return PathBuf::from(path);
    }

    xdg_dir("XDG_STATE_HOME", ".local/state").join("oma-bible")
}

fn xdg_dir(variable: &str, fallback: &str) -> PathBuf {
    std::env::var_os(variable)
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(fallback)))
        .unwrap_or_else(std::env::temp_dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_directory_has_a_name() {
        assert!(!user_data_dir().as_os_str().is_empty());
        assert!(!config_dir().as_os_str().is_empty());
        assert!(!state_dir().as_os_str().is_empty());
    }
}
