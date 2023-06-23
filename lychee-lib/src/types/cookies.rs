use std::path::PathBuf;

use crate::{ErrorKind, Result};
use reqwest_cookie_store::CookieStore as ReqwestCookieStore;
use serde::{Deserialize, Serialize};

/// Create our own wrapper struct for `CookieStore` which implements `Eq` for
/// serde
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CookieJar {
    pub(crate) jar: ReqwestCookieStore,
    pub(crate) path: PathBuf,
}

impl CookieJar {
    /// Load a cookie store from a file
    ///
    /// Currently only JSON files are supported
    ///
    /// # Errors
    ///
    /// This function will return an error if
    /// - the file cannot be opened or
    /// - if the file is not valid JSON
    pub fn load(path: PathBuf) -> Result<Self> {
        let file = std::fs::File::open(&path).map(std::io::BufReader::new)?;
        let jar = ReqwestCookieStore::load_json(file)
            .map_err(|e| ErrorKind::Cookies(format!("Failed to load cookies: {e}")))?;
        Ok(Self { jar, path })
    }

    /// Save the cookie store to file as JSON
    /// This will overwrite the file, which was loaded if any
    ///
    /// # Errors
    ///
    /// This function will return an error if
    /// - the file cannot be opened or
    /// - if the file cannot be written to or
    /// - if the file cannot be serialized to JSON
    pub fn save(&self) -> Result<()> {
        let mut file = std::fs::File::create(&self.path)?;
        self.jar
            .save_json(&mut file)
            .map_err(|e| ErrorKind::Cookies(format!("Failed to save cookies: {e}")))
    }
}

impl PartialEq for CookieJar {
    fn eq(&self, other: &Self) -> bool {
        // Assume that the cookie store is the same if the json is the same
        serde_json::to_string(&self.jar).unwrap() == serde_json::to_string(&other.jar).unwrap()
    }
}
