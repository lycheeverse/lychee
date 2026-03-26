//! `pyproject.toml` configuration loader.
//!
//! This module allows configuring lychee via a `[tool.lychee]` section inside
//! a `pyproject.toml` file, which is standard for Python projects.
//!
//! # Tradeoffs
//!
//! We only care about the `tool.lychee` table. Instead of using a heavyweight
//! dependency to parse the entire `pyproject.toml` schema, we define a minimal
//! struct hierarchy using `serde`. This keeps our dependency tree small and
//! compile times fast, while safely discarding unneeded data.
//!
//! # Example
//!
//! ```toml
//! [tool.lychee]
//! exclude = ["foo", "bar"]
//! timeout = 10
//! ```

use super::ConfigLoader;
use crate::config::Config;
use anyhow::{Context, Result};
use serde::Deserialize;

pub(crate) const PYPROJECT_CONFIG_FILE: &str = "pyproject.toml";

#[derive(Deserialize)]
struct PyprojectToml {
    tool: Option<Tool>,
}

#[derive(Deserialize)]
struct Tool {
    lychee: Config,
}

pub(crate) struct PyprojectTomlLoader;

impl ConfigLoader for PyprojectTomlLoader {
    fn filename(&self) -> &str {
        PYPROJECT_CONFIG_FILE
    }

    fn is_match(&self, contents: &str) -> bool {
        let Ok(table) = toml::from_str::<toml::Table>(contents) else {
            return false;
        };

        table
            .get("tool")
            .and_then(|t| t.as_table())
            .is_some_and(|t| t.contains_key("lychee"))
    }

    fn load(&self, contents: &str) -> Result<Config> {
        let pyproject = toml::from_str::<PyprojectToml>(contents)
            .with_context(|| "Failed to parse [tool.lychee] from pyproject.toml")?;

        let config = pyproject.tool.map(|t| t.lychee).unwrap_or_default();

        Ok(config)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_match() {
        let toml = r"
        [tool.lychee]
        verbose = true
        ";
        assert!(PyprojectTomlLoader.is_match(toml));
    }

    #[test]
    fn test_is_not_match() {
        let toml = r"
        [tool.black]
        line-length = 88
        ";
        assert!(!PyprojectTomlLoader.is_match(toml));
    }

    #[test]
    fn test_load_config() {
        let toml = r#"
        [tool.lychee]
        exclude = ["foo"]
        "#;
        let config = PyprojectTomlLoader.load(toml).unwrap();
        assert_eq!(config.exclude, vec!["foo".to_string()]);
    }
}
