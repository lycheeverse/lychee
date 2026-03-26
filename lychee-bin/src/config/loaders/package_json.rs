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

use super::ConfigLoader;
use crate::config::Config;
use anyhow::{Context, Result};
use serde::Deserialize;

pub(crate) const PACKAGE_JSON_CONFIG_FILE: &str = "package.json";

/// A minimal representation of `package.json` that only parses the `lychee` key
#[derive(Deserialize)]
struct PackageJson {
    lychee: Config,
}

pub(crate) struct PackageJsonLoader;

impl ConfigLoader for PackageJsonLoader {
    fn filename(&self) -> &str {
        PACKAGE_JSON_CONFIG_FILE
    }

    fn is_match(&self, contents: &str) -> bool {
        let Ok(value) = serde_json::from_str::<serde_json::Value>(contents) else {
            return false;
        };

        value
            .as_object()
            .is_some_and(|obj| obj.contains_key("lychee"))
    }

    fn load(&self, contents: &str) -> Result<Config> {
        let package_json = serde_json::from_str::<PackageJson>(contents)
            .with_context(|| "Failed to parse lychee config from package.json")?;

        Ok(package_json.lychee)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_match() {
        let json = r#"
        {
            "name": "my-project",
            "lychee": {
                "exclude": ["foo"]
            }
        }
        "#;
        assert!(PackageJsonLoader.is_match(json));
    }

    #[test]
    fn test_is_not_match() {
        let json = r#"
        {
            "name": "my-project",
            "version": "1.0.0"
        }
        "#;
        assert!(!PackageJsonLoader.is_match(json));
    }

    #[test]
    fn test_invalid_json() {
        let json = r#"
        {
            "name": "my-project",
            "lychee": {
                "exclude": ["foo",
        "#;
        assert!(!PackageJsonLoader.is_match(json));
    }

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
        let config = PackageJsonLoader.load(json).unwrap();
        assert_eq!(config.exclude, vec!["foo".to_string()]);
    }
}
