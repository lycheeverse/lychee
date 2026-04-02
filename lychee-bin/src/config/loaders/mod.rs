pub(crate) mod cargo_toml;
pub(crate) mod lychee_toml;
pub(crate) mod package_json;
pub(crate) mod pyproject_toml;

use super::Config;
use anyhow::{Context, Result, bail};
use std::fs;
use std::path::{Path, PathBuf};

/// Result of attempting to load configuration from a file
#[derive(Debug)]
pub(crate) enum ConfigMatch {
    /// The file contains a valid lychee configuration
    Found(Box<Config>),
    /// The file was parsed successfully but doesn't
    /// contain a lychee configuration section
    NotFound,
}

pub(crate) trait ConfigLoader {
    /// Returns the expected filename for this configuration type.
    fn filename(&self) -> &str;

    /// Attempts to load configuration from the file contents.
    ///
    /// Returns:
    /// - `Ok(ConfigMatch::Found(config))` if the file contains valid lychee configuration
    /// - `Ok(ConfigMatch::NotFound)` if the file is valid but doesn't contain lychee configuration
    /// - `Err(error)` if the file is invalid or contains malformed lychee configuration
    fn load(&self, contents: &str) -> Result<ConfigMatch>;
}

const LOADERS: [&dyn ConfigLoader; 4] = [
    &lychee_toml::LycheeTomlLoader,
    &cargo_toml::CargoTomlLoader,
    &pyproject_toml::PyprojectTomlLoader,
    &package_json::PackageJsonLoader,
];

/// Find the first matching default configuration file in the current directory
/// and return the parsed configuration it contains.
pub(crate) fn default_config_file() -> Result<Option<Config>> {
    for loader in LOADERS {
        let path = PathBuf::from(loader.filename());
        if path.is_file() {
            let contents = fs::read_to_string(&path).unwrap_or_default();
            match loader.load(&contents) {
                Ok(ConfigMatch::Found(config)) => return Ok(Some(*config)),
                Ok(ConfigMatch::NotFound) => (),
                Err(e) => {
                    return Err(e.context(format!(
                        "Cannot load configuration file: {}",
                        path.display()
                    )));
                }
            }
        }
    }
    Ok(None)
}

/// Load the configuration from the given file path.
///
/// If the file matches one of the known configuration formats, it is loaded
/// using the corresponding loader. Otherwise, it falls back to the default
/// TOML loader.
pub(crate) fn load_from_file(path: &Path) -> Result<Config> {
    let contents = fs::read_to_string(path)
        .with_context(|| format!("Failed to read config file {}", path.display()))?;

    let filename = path.file_name().and_then(|n| n.to_str());
    let loader = if let Some(filename) = filename {
        LOADERS
            .iter()
            .find(|loader| filename == loader.filename())
            .copied()
            .unwrap_or(&lychee_toml::LycheeTomlLoader)
    } else {
        &lychee_toml::LycheeTomlLoader
    };

    match loader
        .load(&contents)
        .with_context(|| format!("Failed to load config from {}", path.display()))?
    {
        ConfigMatch::Found(config) => Ok(*config),
        ConfigMatch::NotFound => bail!("No valid lychee configuration found in {}", path.display()),
    }
}
