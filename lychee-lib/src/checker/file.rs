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
        if path.exists() {
            return self.check_existing_path(path, uri).await;
        }

        self.check_with_fallback_extensions(path, uri).await
    }

    /// Checks an existing path, optionally verifying fragments for HTML files.
    ///
    /// # Arguments
    ///
    /// * `path` - The path to check.
    /// * `uri` - The original URI, used for error reporting.
    ///
    /// # Returns
    ///
    /// Returns a `Status` indicating the result of the check.
    async fn check_existing_path(&self, path: &Path, uri: &Uri) -> Status {
        // only files can contain content with fragments
        if self.include_fragments && path.is_file() {
            self.check_fragment(path, uri).await
        } else {
            Status::Ok(StatusCode::OK)
        }
    }

    /// Attempts to find a file by trying different extensions specified in `fallback_extensions`.
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
            return self.check_existing_path(&path_buf, uri).await;
        }

        // Try fallback extensions
        for ext in &self.fallback_extensions {
            path_buf.set_extension(ext);
            if path_buf.exists() {
                return self.check_existing_path(&path_buf, uri).await;
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
