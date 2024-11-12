use crate::{ErrorKind, Result};
use log::info;
use reqwest_cookie_store::{CookieStore as ReqwestCookieStore, CookieStoreMutex};
use std::io::ErrorKind as IoErrorKind;
use std::{path::PathBuf, sync::Arc};

/// A wrapper around `reqwest_cookie_store::CookieStore`
///
/// We keep track of the file path of the cookie store and
/// implement `PartialEq` to compare cookie jars by their path
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
    /// - the file cannot be opened (except for `NotFound`) or
    /// - if the file is not valid JSON in either new or legacy format
    pub fn load(path: PathBuf) -> Result<Self> {
        match std::fs::File::open(&path).map(std::io::BufReader::new) {
            Ok(mut reader) => {
                info!("Loading cookies from {}", path.display());

                // Try loading with new format first, fall back to legacy format
                #[allow(clippy::single_match_else)]
                let store = match cookie_store::serde::json::load(&mut reader) {
                    Ok(store) => store,
                    Err(_) => {
                        // Reopen file for legacy format attempt
                        let reader = std::fs::File::open(&path).map(std::io::BufReader::new)?;
                        #[allow(deprecated)]
                        ReqwestCookieStore::load_json(reader).map_err(|e| {
                            ErrorKind::Cookies(format!("Failed to load cookies: {e}"))
                        })?
                    }
                };

                Ok(Self {
                    path,
                    inner: Arc::new(CookieStoreMutex::new(store)),
                })
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
        info!("Saving cookies to {}", self.path.display());
        // Create parent directories if they don't exist
        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let mut file = std::fs::File::create(&self.path)?;
        let store = self
            .inner
            .lock()
            .map_err(|e| ErrorKind::Cookies(format!("Failed to lock cookie store: {e}")))?;
        cookie_store::serde::json::save(&store, &mut file)
            .map_err(|e| ErrorKind::Cookies(format!("Failed to save cookies: {e}")))
    }
}

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
