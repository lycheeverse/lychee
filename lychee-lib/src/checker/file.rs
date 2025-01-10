use http::StatusCode;
use log::warn;
use std::path::{Path, PathBuf};

use crate::{utils::fragment_checker::FragmentChecker, ErrorKind, Result, Status, Uri};

/// A utility for checking the existence and validity of file-based URIs.
///
/// `FileChecker` resolves and validates file paths, handling both absolute and relative paths.
/// It supports base path resolution, fallback extensions for files without extensions,
/// and optional fragment checking for HTML files.
#[derive(Debug, Clone)]
pub(crate) struct FileChecker {
    /// List of file extensions to try if the original path doesn't exist.
    fallback_extensions: Vec<String>,
    /// Whether to check for the existence of fragments (e.g., `#section-id`) in HTML files.
    include_fragments: bool,
    /// Utility for performing fragment checks in HTML files.
    fragment_checker: FragmentChecker,
}

impl FileChecker {
    /// Creates a new `FileChecker` with the given configuration.
    ///
    /// # Arguments
    ///
    /// * `base` - Optional base path or URL for resolving relative paths.
    /// * `fallback_extensions` - List of extensions to try if the original file is not found.
    /// * `include_fragments` - Whether to check for fragment existence in HTML files.
    pub(crate) fn new(fallback_extensions: Vec<String>, include_fragments: bool) -> Self {
        Self {
            fallback_extensions,
            include_fragments,
            fragment_checker: FragmentChecker::new(),
        }
    }

    /// Checks the given file URI for existence and validity.
    ///
    /// This method resolves the URI to a file path, checks if the file exists,
    /// and optionally checks for the existence of fragments in HTML files.
    ///
    /// # Arguments
    ///
    /// * `uri` - The URI to check.
    ///
    /// # Returns
    ///
    /// Returns a `Status` indicating the result of the check.
    pub(crate) async fn check(&self, uri: &Uri) -> Status {
        let path = match uri_to_path(uri) {
            Ok(path) => path,
            Err(err) => return err.into(),
        };
        self.check_path(&path, uri).await
    }

    /// Checks if the given path exists
    ///
    /// Two special cases are considered:
    ///
    /// - If the path exists, it will check for a fragment if enabled.
    /// - If the path does not exist, it will try to find the file with
    ///   fallback extensions.
    ///
    /// # Arguments
    ///
    /// * `path` - The path to check.
    /// * `uri` - The original URI, used for error reporting.
    ///
    /// # Returns
    ///
    /// Returns a `Status` indicating the result of the check.
    async fn check_path(&self, path: &Path, uri: &Uri) -> Status {
        if path.exists() {
            return self.check_fragment_if_enabled(path, uri).await;
        }

        self.check_with_fallback_extensions(path, uri).await
    }

    /// Checks for the existence of a fragment in a file if fragment checking is
    /// enabled.
    ///
    /// # Arguments
    ///
    /// * `path` - The path to check.
    /// * `uri` - The original URI, used for error reporting.
    ///
    /// # Returns
    ///
    /// Returns a `Status` indicating whether the fragment check was successful.
    async fn check_fragment_if_enabled(&self, path: &Path, uri: &Uri) -> Status {
        if self.include_fragments {
            self.check_fragment(path, uri).await
        } else {
            Status::Ok(StatusCode::OK)
        }
    }

    /// Attempts to find a file by trying different extensions specified in
    /// `fallback_extensions`.
    ///
    /// # Arguments
    ///
    /// * `path` - The original path to check.
    /// * `uri` - The original URI, used for error reporting.
    ///
    /// # Returns
    ///
    /// Returns a `Status` indicating the result of the check.
    async fn check_with_fallback_extensions(&self, path: &Path, uri: &Uri) -> Status {
        let mut path_buf = path.to_path_buf();

        // If the path already has an extension, try it first
        if path_buf.extension().is_some() && path_buf.exists() {
            return self.check_fragment_if_enabled(&path_buf, uri).await;
        }

        // Try fallback extensions
        for ext in &self.fallback_extensions {
            path_buf.set_extension(ext);
            if path_buf.exists() {
                return self.check_fragment_if_enabled(&path_buf, uri).await;
            }
        }

        ErrorKind::InvalidFilePath(uri.clone()).into()
    }

    /// Checks for the existence of a fragment in an HTML file.
    ///
    /// # Arguments
    ///
    /// * `path` - The path to the HTML file.
    /// * `uri` - The original URI, containing the fragment to check.
    ///
    /// # Returns
    ///
    /// Returns a `Status` indicating the result of the fragment check.
    async fn check_fragment(&self, path: &Path, uri: &Uri) -> Status {
        match self.fragment_checker.check(path, &uri.url).await {
            Ok(true) => Status::Ok(StatusCode::OK),
            Ok(false) => ErrorKind::InvalidFragment(uri.clone()).into(),
            Err(err) => {
                warn!("Skipping fragment check due to the following error: {err}");
                Status::Ok(StatusCode::OK)
            }
        }
    }
}

/// Converts the given URI to a file path.
fn uri_to_path(uri: &Uri) -> Result<PathBuf> {
    // Make a clone without the fragment for file path checking
    let mut url_without_fragment = uri.url.clone();
    url_without_fragment.set_fragment(None);

    let Ok(path) = url_without_fragment.to_file_path() else {
        return Err(ErrorKind::InvalidFilePath(uri.clone()));
    };

    Ok(path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_uri_to_path() {
        let cases = vec![
            (
                "file:///path/to/file",
                PathBuf::from("/path/to/file"),
                "file URI with absolute path",
            ),
            (
                "file:///path/to/file#fragment",
                PathBuf::from("/path/to/file"),
                "file URI with absolute path and fragment",
            ),
            (
                "file:///path/to/file%20with%20spaces",
                PathBuf::from("/path/to/file with spaces"),
                "file URI with absolute path and spaces",
            ),
            (
                "file:/path/to/file",
                PathBuf::from("/path/to/file"),
                "file URI with absolute path (no slashes)",
            ),
            (
                "file:/path/to/file#fragment",
                PathBuf::from("/path/to/file"),
                "file URI with absolute path and fragment (no slashes)",
            ),
            (
                "file:/path/to/file%20with%20spaces",
                PathBuf::from("/path/to/file with spaces"),
                "file URI with absolute path and spaces (no slashes)",
            ),
        ];

        for (uri, expected, name) in cases {
            let url = url::Url::parse(uri).unwrap();
            let uri = Uri::from(url);
            let path = uri_to_path(&uri).unwrap();
            assert_eq!(path, expected, "{}", name);
        }
    }
}
