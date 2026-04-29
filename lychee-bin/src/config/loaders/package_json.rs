//! `package.json` configuration loader.
//!
//! This module allows configuring lychee via a `"lychee"` object inside
//! a `package.json` file, which is standard for Node.js / JavaScript projects.
//!
//! # Example
//!
//! ```json
//! {
//!   "name": "my-project",
//!   "version": "1.0.0",
//!   "lychee": {
//!     "exclude": ["foo", "bar"],
//!     "timeout": 10
//!   }
//! }
//! ```

use super::{ConfigLoader, ConfigMatch};
use crate::config::Config;
use anyhow::{Context, Result};
use serde::Deserialize;

pub(crate) const PACKAGE_JSON_CONFIG_FILE: &str = "package.json";

/// A minimal representation of `package.json` that only parses the `lychee` key
#[derive(Deserialize)]
struct PackageJson {
    lychee: Option<Config>,
}

pub(crate) struct PackageJsonLoader;

impl ConfigLoader for PackageJsonLoader {
    fn filename(&self) -> &str {
        PACKAGE_JSON_CONFIG_FILE
    }

    fn load(&self, contents: &str) -> Result<ConfigMatch> {
        let package_json = serde_json::from_str::<PackageJson>(contents)
            .with_context(|| "Failed to parse lychee config from package.json")?;

        match package_json.lychee {
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
        let json = r#"
        {
            "name": "my-project",
            "lychee": {
                "exclude": ["foo"]
            }
        }
        "#;
        let result = PackageJsonLoader.load(json).unwrap();
        match result {
            ConfigMatch::Found(config) => assert_eq!(config.exclude, vec!["foo".to_string()]),
            ConfigMatch::NotFound => panic!("Expected config to be found"),
        }
    }

    #[test]
    fn test_load_no_lychee_config() {
        let json = r#"
        {
            "name": "my-project",
            "version": "1.0.0"
        }
        "#;
        let result = PackageJsonLoader.load(json).unwrap();
        match result {
            ConfigMatch::NotFound => (),
            ConfigMatch::Found(_) => panic!("Expected no config to be found"),
        }
    }

    #[test]
    fn test_invalid_json() {
        let json = r#"
        {
            "name": "my-project",
            "lychee": {
                "exclude": ["foo",
        "#;
        assert!(PackageJsonLoader.load(json).is_err());
    }
}
