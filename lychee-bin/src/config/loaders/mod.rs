pub(crate) mod cargo_toml;
pub(crate) mod lychee_toml;
pub(crate) mod package_json;
pub(crate) mod pyproject_toml;

use super::Config;
use anyhow::Result;
use std::fs;
use std::path::{Path, PathBuf};

pub(crate) trait ConfigLoader {
    /// Returns the expected filename for this configuration type.
    fn filename(&self) -> &str;

    /// Fast check to see if the file contents contain the necessary sections
    /// for this configuration type (e.g., `[tool.lychee]`, `[package.metadata.lychee]`,
    /// or a `"lychee"` object in JSON).
    ///
    /// # Why separate `is_match` from `load`?
    /// If we strictly deserialized the entire file directly into our `Config` struct
    /// (which uses `#[serde(deny_unknown_fields)]`), we wouldn't be able to distinguish
    /// between two scenarios:
    /// 1. A file that simply doesn't contain any lychee configuration (which we should silently ignore).
    /// 2. A file that *does* contain a lychee configuration section, but has a typo in one of the fields (which we should report as an error).
    ///
    /// `is_match` performs a loose structural check to safely handle the first scenario,
    /// allowing `load` to be strict and bubble up actual configuration errors to the user.
    fn is_match(&self, contents: &str) -> bool;

    /// Strictly read and parse the configuration from the file.
    ///
    /// This assumes `is_match` returned true. If `is_match` is false, it is
    /// not recommended to call `load`, as it will likely fail with a parsing error.
    fn load(&self, contents: &str) -> Result<Config>;
}

const LOADERS: [&dyn ConfigLoader; 4] = [
    &lychee_toml::LycheeTomlLoader,
    &pyproject_toml::PyprojectTomlLoader,
    &cargo_toml::CargoTomlLoader,
    &package_json::PackageJsonLoader,
];

/// Find the first matching default configuration file in the current directory.
///
/// This checks for files like `lychee.toml`, `pyproject.toml`, `Cargo.toml`,
/// and `package.json` in a defined order of precedence.
pub(crate) fn find_default_config_file() -> Option<PathBuf> {
    for loader in LOADERS {
        let path = PathBuf::from(loader.filename());
        if path.is_file() {
            let contents = fs::read_to_string(&path).unwrap_or_default();
            if loader.is_match(&contents) {
                return Some(path);
            }
        }
    }
    None
}

/// Load the configuration from the given file path.
///
/// If the file matches one of the known configuration formats, it is loaded
/// using the corresponding loader. Otherwise, it falls back to the default
/// TOML loader.
pub(crate) fn load_from_file(path: &Path) -> Result<Config> {
    let contents = fs::read_to_string(path).map_err(|e| {
        log::warn!("Failed to read config file {}: {e}", path.display());
        anyhow::Error::from(e)
    })?;

    if let Some(filename) = path.file_name().and_then(|n| n.to_str()) {
        for loader in LOADERS {
            if loader.filename() == filename {
                if loader.is_match(&contents) {
                    return loader.load(&contents).map_err(|e| {
                        log::warn!("Failed to load config from {}: {e}", path.display());
                        e
                    });
                }
                return Err(anyhow::anyhow!(
                    "No valid lychee configuration found in {}",
                    path.display()
                ));
            }
        }
    }

    lychee_toml::LycheeTomlLoader.load(&contents).map_err(|e| {
        log::warn!("Failed to load config from {}: {e}", path.display());
        e
    })
}
