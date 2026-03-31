//! `lychee.toml` configuration loader.
//!
//! This module allows configuring lychee via a standard `lychee.toml` file.
//! This is the default configuration file format for lychee.
//!
//! Unlike `Cargo.toml` and `pyproject.toml` which require the configuration
//! to be scoped under a specific section (like `[package.metadata.lychee]`),
//! `lychee.toml` defines the configuration at the root level of the document.
//!
//! # Example
//!
//! ```toml
//! exclude = ["foo", "bar"]
//! timeout = 10
//! ```

use super::{ConfigLoader, ConfigMatch};
use anyhow::{Context, Result};

pub(crate) const LYCHEE_CONFIG_FILE: &str = "lychee.toml";

pub(crate) struct LycheeTomlLoader;

impl ConfigLoader for LycheeTomlLoader {
    fn filename(&self) -> &str {
        LYCHEE_CONFIG_FILE
    }

    /// We strictly deserialize the entire file directly into our `Config` struct.
    /// Any failure here is a genuine configuration error that we want to bubble up.
    /// A dedicated `lychee.toml` file is assumed to always contain lychee configuration.
    fn load(&self, contents: &str) -> Result<ConfigMatch> {
        let config =
            toml::from_str(contents).with_context(|| "Failed to parse configuration file")?;
        Ok(ConfigMatch::Found(Box::new(config)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_config() {
        let toml = r#"
        exclude = ["foo"]
        "#;
        let result = LycheeTomlLoader.load(toml).unwrap();
        match result {
            ConfigMatch::Found(config) => assert_eq!(config.exclude, vec!["foo".to_string()]),
            ConfigMatch::NotFound => panic!("Expected config to be found"),
        }
    }

    #[test]
    fn test_load_invalid_config() {
        let toml = r#"
        exclude = "foo" # should be an array
        "#;
        assert!(LycheeTomlLoader.load(toml).is_err());
    }
}
