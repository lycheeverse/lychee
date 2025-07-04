use http::StatusCode;
use log::warn;
use std::path::{Path, PathBuf};

use crate::{
    Base, ErrorKind, Status, Uri,
    utils::fragment_checker::{FragmentChecker, FragmentInput},
};

/// A utility for checking the existence and validity of file-based URIs.
///
/// `FileChecker` resolves and validates file paths, handling both absolute and relative paths.
/// It supports base path resolution, fallback extensions for files without extensions,
/// and optional fragment checking for HTML files.
#[derive(Debug, Clone)]
pub(crate) struct FileChecker {
    /// Base path or URL used for resolving relative paths.
    base: Option<Base>,
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
    pub(crate) fn new(
        base: Option<Base>,
        fallback_extensions: Vec<String>,
        include_fragments: bool,
    ) -> Self {
        Self {
            base,
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
        let Ok(path) = uri.url.to_file_path() else {
            return ErrorKind::InvalidFilePath(uri.clone()).into();
        };

        let resolved_path = self.resolve_path(&path);
        self.check_path(&resolved_path, uri).await
    }

    /// Resolves the given path using the base path, if one is set.
    ///
    /// # Arguments
    ///
    /// * `path` - The path to resolve.
    ///
    /// # Returns
    ///
    /// Returns the resolved path as a `PathBuf`.
    fn resolve_path(&self, path: &Path) -> PathBuf {
        if let Some(Base::Local(base_path)) = &self.base {
            if path.is_absolute() {
                let absolute_base_path = if base_path.is_relative() {
                    std::env::current_dir().unwrap_or_default().join(base_path)
                } else {
                    base_path.clone()
                };

                let stripped = path.strip_prefix("/").unwrap_or(path);
                absolute_base_path.join(stripped)
            } else {
                base_path.join(path)
            }
        } else {
            path.to_path_buf()
        }
    }

    /// Checks if the given path exists and performs additional checks if necessary.
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
        let file_path = self.resolve_file_path(path);

        // If file_path exists, check this file
        if file_path.is_some() {
            return self.check_file(&file_path.unwrap(), uri).await;
        }

        ErrorKind::InvalidFilePath(uri.clone()).into()
    }

    /// Resolves a path to an actual file, applying fallback extensions and directory index resolution.
    ///
    /// # Arguments
    ///
    /// * `path` - The path to resolve.
    ///
    /// # Returns
    ///
    /// Returns `Some(PathBuf)` with the resolved file path, or `None` if no valid file is found.
    fn resolve_file_path(&self, path: &Path) -> Option<PathBuf> {
        // If it's already a file, use it directly
        if path.is_file() {
            return Some(path.to_path_buf());
        }

        // Try fallback extensions
        let mut path_buf = path.to_path_buf();
        for ext in &self.fallback_extensions {
            path_buf.set_extension(ext);
            if path_buf.exists() && path_buf.is_file() {
                return Some(path_buf);
            }
        }

        // If it's a directory, try to find an index file
        if path.is_dir() {
            return self.get_index_file_path(path);
        }

        None
    }

    /// Tries to find an index file in the given directory, returning the first match.
    ///
    /// Searches for `index.{ext}` files using fallback extensions, defaulting to `index.html`
    /// if no fallback extensions are configured. This encapsulates both the "index" filename
    /// convention and the extension resolution logic.
    ///
    /// # Arguments
    ///
    /// * `dir_path` - The directory to search for index files
    ///
    /// # Returns
    ///
    /// Returns `Some(PathBuf)` pointing to the first existing index file, or `None` if no index file is found.
    fn get_index_file_path(&self, dir_path: &Path) -> Option<PathBuf> {
        // In this function, we hardcode the filename `index` and the extension
        // `.html` since `index.html` is the most common scenario when serving a
        // page from a directory. However, various servers may support other
        // filenames and extensions, such as `README.md`. We could enhance this by
        // giving users the option to configure the index filename and extension.

        let extensions_to_try = if self.fallback_extensions.is_empty() {
            vec!["html".to_string()]
        } else {
            self.fallback_extensions.clone()
        };

        for ext in &extensions_to_try {
            let index_path = dir_path.join(format!("index.{ext}"));
            if index_path.is_file() {
                return Some(index_path);
            }
        }
        None
    }

    /// Checks a resolved file, optionally verifying fragments for HTML files.
    ///
    /// # Arguments
    ///
    /// * `file_path` - The resolved file path to check.
    /// * `uri` - The original URI, used for error reporting.
    ///
    /// # Returns
    ///
    /// Returns a `Status` indicating the result of the check.
    async fn check_file(&self, file_path: &Path, uri: &Uri) -> Status {
        if !file_path.is_file() {
            return ErrorKind::InvalidFilePath(uri.clone()).into();
        }

        // Check if we need to verify fragments
        if self.include_fragments && uri.url.fragment().is_some_and(|x| !x.is_empty()) {
            self.check_fragment(file_path, uri).await
        } else {
            Status::Ok(StatusCode::OK)
        }
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
        match FragmentInput::from_path(path).await {
            Ok(input) => match self.fragment_checker.check(input, &uri.url).await {
                Ok(true) => Status::Ok(StatusCode::OK),
                Ok(false) => ErrorKind::InvalidFragment(uri.clone()).into(),
                Err(err) => {
                    warn!("Skipping fragment check for {uri} due to the following error: {err}");
                    Status::Ok(StatusCode::OK)
                }
            },
            Err(err) => {
                warn!("Skipping fragment check for {uri} due to the following error: {err}");
                Status::Ok(StatusCode::OK)
            }
        }
    }
}
