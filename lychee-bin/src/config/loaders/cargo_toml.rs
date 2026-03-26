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

use super::ConfigLoader;
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

    /// We use a generic TOML table to check for the presence of the
    /// `lychee` configuration section. We don't want to strictly deserialize
    /// into `Config` here, because if the user has a typo in their config
    /// (e.g. `timeoutt = 10`), a strict deserialization would fail, and we
    /// would incorrectly return `false`, causing lychee to silently ignore
    /// the file instead of reporting the error.
    fn is_match(&self, contents: &str) -> bool {
        let Ok(table) = toml::from_str::<toml::Table>(contents) else {
            return false;
        };

        table
            .get("package")
            .or(table.get("workspace"))
            .and_then(|t| t.as_table())
            .and_then(|t| t.get("metadata"))
            .and_then(|m| m.as_table())
            .is_some_and(|m| m.contains_key("lychee"))
    }

    /// We strictly deserialize into our custom `CargoToml` envelope,
    /// which contains our `Config` struct with `#[serde(deny_unknown_fields)]`.
    /// Since we already know the section exists from `is_match`, any failure
    /// here is a genuine configuration error that we want to bubble up.
    fn load(&self, contents: &str) -> Result<Config> {
        let cargo = toml::from_str::<CargoToml>(contents)
            .with_context(|| "Failed to parse lychee config from Cargo.toml")?;

        // Package metadata takes precedence over workspace metadata
        // That's useful, because it allows users to define a
        // workspace-wide default config and then override it in
        // specific packages.
        let config = [cargo.package, cargo.workspace]
            .into_iter()
            .flatten()
            .find_map(|s| s.metadata.and_then(|m| m.lychee))
            .unwrap_or_default();

        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_match() {
        let toml = r"
        [package.metadata.lychee]
        verbose = true
        ";
        assert!(CargoTomlLoader.is_match(toml));

        let toml_workspace = r"
        [workspace.metadata.lychee]
        verbose = true
        ";
        assert!(CargoTomlLoader.is_match(toml_workspace));
    }

    #[test]
    fn test_is_not_match() {
        let toml = r#"
        [package]
        name = "lychee"
        "#;
        assert!(!CargoTomlLoader.is_match(toml));
    }

    #[test]
    fn test_load_package_config() {
        let toml = r#"
        [package.metadata.lychee]
        exclude = ["foo"]
        "#;
        let config = CargoTomlLoader.load(toml).unwrap();
        assert_eq!(config.exclude, vec!["foo".to_string()]);
    }

    #[test]
    fn test_load_workspace_config() {
        let toml = r#"
        [workspace.metadata.lychee]
        exclude = ["bar"]
        "#;
        let config = CargoTomlLoader.load(toml).unwrap();
        assert_eq!(config.exclude, vec!["bar".to_string()]);
    }

    #[test]
    fn test_load_package_takes_precedence() {
        let toml = r#"
        [workspace.metadata.lychee]
        exclude = ["bar"]

        [package.metadata.lychee]
        exclude = ["foo"]
        "#;
        let config = CargoTomlLoader.load(toml).unwrap();
        assert_eq!(config.exclude, vec!["foo".to_string()]);
    }
}
