//! `Cargo.toml` configuration loader.
//!
//! This module allows configuring lychee via a `[package.metadata.lychee]` or
//! `[workspace.metadata.lychee]` section in a `Cargo.toml` file.
//!
//! # Tradeoffs
//!
//! While there are crates like `cargo_toml` available for parsing `Cargo.toml`
//! files, we deliberately avoid using them. Such crates bring in heavy dependencies
//! and extensive struct hierarchies to represent the entire Cargo schema (dependencies,
//! targets, profiles, etc.).
//!
//! Instead, we define a lightweight, custom set of structs to extract exactly
//! what we need using `serde` and `toml`.
//!
//! # Example
//!
//! ```toml
//! [package.metadata.lychee]
//! exclude = ["foo", "bar"]
//! max_redirects = 5
//! ```

use super::{ConfigLoader, ConfigMatch};
use crate::config::Config;
use anyhow::{Context, Result};
use serde::Deserialize;

pub(crate) const CARGO_CONFIG_FILE: &str = "Cargo.toml";

/// The lychee config can be defined in either
/// `[package.metadata.lychee]` or `[workspace.metadata.lychee]`.
#[derive(Deserialize)]
struct CargoToml {
    package: Option<CargoSection>,
    workspace: Option<CargoSection>,
}

#[derive(Deserialize)]
struct CargoSection {
    metadata: Option<Metadata>,
}

#[derive(Deserialize)]
struct Metadata {
    lychee: Option<Config>,
}

pub(crate) struct CargoTomlLoader;

impl ConfigLoader for CargoTomlLoader {
    fn filename(&self) -> &str {
        CARGO_CONFIG_FILE
    }

    fn load(&self, contents: &str) -> Result<ConfigMatch> {
        let cargo = toml::from_str::<CargoToml>(contents)
            .with_context(|| "Failed to parse lychee config from Cargo.toml")?;

        // Package metadata fully replaces workspace metadata (instead of merging).
        // That's useful, because it allows users to define a workspace-wide
        // default config and then override it in specific packages.
        let config = [cargo.package, cargo.workspace]
            .into_iter()
            .flatten()
            .find_map(|s| s.metadata.and_then(|m| m.lychee));

        match config {
            Some(config) => Ok(ConfigMatch::Found(Box::new(config))),
            None => Ok(ConfigMatch::NotFound),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_package_config() {
        let toml = r#"
        [package.metadata.lychee]
        exclude = ["foo"]
        "#;
        let result = CargoTomlLoader.load(toml).unwrap();
        match result {
            ConfigMatch::Found(config) => assert_eq!(config.exclude, vec!["foo".to_string()]),
            ConfigMatch::NotFound => panic!("Expected config to be found"),
        }
    }

    #[test]
    fn test_load_workspace_config() {
        let toml = r#"
        [workspace.metadata.lychee]
        exclude = ["bar"]
        "#;
        let result = CargoTomlLoader.load(toml).unwrap();
        match result {
            ConfigMatch::Found(config) => assert_eq!(config.exclude, vec!["bar".to_string()]),
            ConfigMatch::NotFound => panic!("Expected config to be found"),
        }
    }

    #[test]
    fn test_load_package_takes_precedence() {
        let toml = r#"
        [workspace.metadata.lychee]
        exclude = ["bar"]

        [package.metadata.lychee]
        exclude = ["foo"]
        "#;
        let result = CargoTomlLoader.load(toml).unwrap();
        match result {
            ConfigMatch::Found(config) => assert_eq!(config.exclude, vec!["foo".to_string()]),
            ConfigMatch::NotFound => panic!("Expected config to be found"),
        }
    }

    #[test]
    fn test_load_no_lychee_config() {
        let toml = r#"
        [package]
        name = "lychee"
        version = "1.0.0"

        [workspace]
        members = ["lychee-bin"]
        "#;
        let result = CargoTomlLoader.load(toml).unwrap();
        match result {
            ConfigMatch::NotFound => (),
            ConfigMatch::Found(_) => panic!("Expected no config to be found"),
        }
    }
}
