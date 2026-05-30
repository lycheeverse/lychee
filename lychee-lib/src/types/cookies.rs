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
    /// - if the file is not valid JSON
    pub fn load(path: PathBuf) -> Result<Self> {
        let store = match std::fs::read_to_string(&path) {
            // A missing or empty jar (e.g. a freshly `touch`ed file) simply
            // starts out with an empty cookie store.
            Err(e) if e.kind() == IoErrorKind::NotFound => ReqwestCookieStore::default(),
            Ok(contents) if contents.trim().is_empty() => ReqwestCookieStore::default(),
            Ok(contents) => {
                info!("Loading cookies from {}", path.display());
                cookie_store::serde::json::load(contents.as_bytes())
                    .map_err(|e| ErrorKind::Cookies(format!("Failed to load cookies: {e}")))?
            }
            // Propagate other IO errors (like permission denied) to the caller
            Err(e) => return Err(e.into()),
        };

        Ok(Self {
            path,
            inner: Arc::new(CookieStoreMutex::new(store)),
        })
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

#[cfg(test)]
mod tests {
    use super::CookieJar;
    use std::io::Write;

    #[test]
    fn test_load_missing_file_creates_empty_jar() {
        // A path inside a fresh temp dir is guaranteed not to exist yet.
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("does-not-exist.json");
        let jar = CookieJar::load(path).expect("missing file should yield an empty jar");
        assert_eq!(jar.inner.lock().unwrap().iter_any().count(), 0);
    }

    #[test]
    fn test_load_empty_file_creates_empty_jar() {
        // A pre-created but empty cookie jar (e.g. via `touch`) must load
        // gracefully as an empty store rather than failing to parse.
        let mut file = tempfile::NamedTempFile::new().unwrap();
        file.flush().unwrap();
        let jar = CookieJar::load(file.path().to_path_buf())
            .expect("empty file should yield an empty jar");
        assert_eq!(jar.inner.lock().unwrap().iter_any().count(), 0);
    }

    #[test]
    fn test_load_new_format_array() {
        let mut file = tempfile::NamedTempFile::new().unwrap();
        write!(
            file,
            r#"[{{"raw_cookie":"foo=bar; Path=/; Domain=example.com; Expires=Tue, 03 Aug 2100 00:38:37 GMT","path":["/",true],"domain":{{"Suffix":"example.com"}},"expires":{{"AtUtc":"2100-08-03T00:38:37Z"}}}}]"#
        )
        .unwrap();
        file.flush().unwrap();
        let jar =
            CookieJar::load(file.path().to_path_buf()).expect("valid new-format jar should load");
        assert_eq!(jar.inner.lock().unwrap().iter_any().count(), 1);
    }
}
