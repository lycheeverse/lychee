use std::io::ErrorKind as IoErrorKind;
use std::{path::PathBuf, sync::Arc};

use crate::{ErrorKind, Result};
use log::info;
use reqwest_cookie_store::{CookieStore as ReqwestCookieStore, CookieStoreMutex};

/// Create our own wrapper struct for `CookieStore` which implements `Eq` for
/// serde
#[derive(Debug, Clone)]
pub struct CookieJar {
    pub(crate) path: PathBuf,
    pub(crate) inner: Arc<CookieStoreMutex>,
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
        match std::fs::File::open(&path).map(std::io::BufReader::new) {
            Ok(reader) => {
                info!("Loading cookies from {}", path.display());
                let inner = Arc::new(CookieStoreMutex::new(
                    ReqwestCookieStore::load_json(reader)
                        .map_err(|e| ErrorKind::Cookies(format!("Failed to load cookies: {e}")))?,
                ));
                Ok(Self { path, inner })
            }
            // Create a new cookie store if the file does not exist
            Err(e) if e.kind() == IoErrorKind::NotFound => Ok(Self {
                path,
                inner: Arc::new(CookieStoreMutex::new(ReqwestCookieStore::default())),
            }),
            // Propagate other IO errors (like permission denied) to the caller
            Err(e) => Err(e.into()),
        }
    }

    /// Save the cookie store to file as JSON
    /// This will overwrite the file, which was loaded if any
    ///
    /// # Errors
    ///
    /// This function will return an error if
    /// - the cookie store is locked or
    /// - the file cannot be opened or
    /// - if the file cannot be written to or
    /// - if the file cannot be serialized to JSON
    pub fn save(&self) -> Result<()> {
        let mut file = std::fs::File::create(&self.path)?;
        self.inner
            .lock()
            .map_err(|e| ErrorKind::Cookies(format!("Failed to lock cookie store: {e}")))?
            .save_json(&mut file)
            .map_err(|e| ErrorKind::Cookies(format!("Failed to save cookies: {e}")))
    }
}

// Deref to inner cookie store
impl std::ops::Deref for CookieJar {
    type Target = Arc<CookieStoreMutex>;

    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}

impl PartialEq for CookieJar {
    fn eq(&self, other: &Self) -> bool {
        // Assume that the cookie jar is the same if the path is the same
        // Comparing the cookie stores directly is not possible because the
        // `CookieStore` struct does not implement `Eq`
        self.path == other.path
    }
}
