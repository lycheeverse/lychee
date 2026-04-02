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

use super::{ConfigLoader, ConfigMatch};
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
    lychee: Option<Config>,
}

pub(crate) struct PyprojectTomlLoader;

impl ConfigLoader for PyprojectTomlLoader {
    fn filename(&self) -> &str {
        PYPROJECT_CONFIG_FILE
    }

    fn load(&self, contents: &str) -> Result<ConfigMatch> {
        let pyproject = toml::from_str::<PyprojectToml>(contents)
            .with_context(|| "Failed to parse [tool.lychee] from pyproject.toml")?;

        match pyproject.tool.and_then(|t| t.lychee) {
            Some(config) => Ok(ConfigMatch::Found(Box::new(config))),
            None => Ok(ConfigMatch::NotFound),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_load_config() {
        let toml = r#"
        [tool.lychee]
        exclude = ["foo"]
        "#;
        let result = PyprojectTomlLoader.load(toml).unwrap();
        match result {
            ConfigMatch::Found(config) => assert_eq!(config.exclude, vec!["foo".to_string()]),
            ConfigMatch::NotFound => panic!("Expected config to be found"),
        }
    }

    #[test]
    fn test_load_no_lychee_config() {
        let toml = r"
        [tool.black]
        line-length = 88
        ";
        let result = PyprojectTomlLoader.load(toml).unwrap();
        match result {
            ConfigMatch::NotFound => (),
            ConfigMatch::Found(_) => panic!("Expected no config to be found"),
        }
    }

    #[test]
    fn test_load_no_tool_section() {
        let toml = r#"
        [build-system]
        requires = ["setuptools"]
        "#;
        let result = PyprojectTomlLoader.load(toml).unwrap();
        match result {
            ConfigMatch::NotFound => (),
            ConfigMatch::Found(_) => panic!("Expected no config to be found"),
        }
    }
}
