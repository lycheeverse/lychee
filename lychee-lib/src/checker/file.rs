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
    /// List of index file names to search for if the path is a directory.
    index_files: Vec<String>,
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
    /// * `index_files` - List of index file names to search for if the path is a directory.
    /// * `include_fragments` - Whether to check for fragment existence in HTML files.
    pub(crate) fn new(
        base: Option<Base>,
        fallback_extensions: Vec<String>,
        index_files: Vec<String>,
        include_fragments: bool,
    ) -> Self {
        Self {
            base,
            fallback_extensions,
            index_files,
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
        let file_path = match path.metadata() {
            // for non-existing paths, attempt fallback extensions
            Err(ref e) if e.kind() == std::io::ErrorKind::NotFound => {
                self.apply_fallback_extensions(path, uri)
            }
            // other io errors are unexpected and should fail the check
            Err(e) => Err(ErrorKind::ReadFileInput(e, path.to_path_buf())),
            // existing directories are resolved via index files
            Ok(ref meta) if meta.is_dir() => self.apply_index_files(path),
            // otherwise (i.e., path is an existing file), just return the path
            Ok(_) => Ok(path.to_path_buf()),
        };

        match file_path {
            Ok(ref file_path) => self.check_file(file_path, uri).await,
            Err(err) => err.into(),
        }
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
    fn apply_fallback_extensions(&self, path: &Path, uri: &Uri) -> Result<PathBuf, ErrorKind> {
        // If it's already a file, use it directly
        if path.is_file() {
            return Ok(path.to_path_buf());
        }

        // Try fallback extensions
        let mut path_buf = path.to_path_buf();
        for ext in &self.fallback_extensions {
            path_buf.set_extension(ext);
            if path_buf.exists() && path_buf.is_file() {
                return Ok(path_buf);
            }
        }

        Err(ErrorKind::InvalidFilePath(uri.clone()))
    }

    /// Tries to find an index file in the given directory, returning the first match.
    ///
    /// Searches for files using the specified index file names. This does *not*
    /// consider fallback extensions.
    ///
    /// # Arguments
    ///
    /// * `path` - The directory within which to search for index files
    ///
    /// # Returns
    ///
    /// Returns `Some(PathBuf)` pointing to the first existing index file, or `None` if no index file is found.
    fn apply_index_files(&self, dir_path: &Path) -> Result<PathBuf, ErrorKind> {
        if dir_path.is_file() {
            return Ok(dir_path.to_path_buf());
        }

        // deliberately uses `.exists()` to permit returning a directory
        // if `.` is specified in `index_files`.
        self.index_files
            .iter()
            .map(|ref filename| dir_path.join(filename))
            .find(|ref p| p.exists())
            .ok_or_else(|| ErrorKind::InvalidIndexFile(dir_path.to_path_buf()))
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
        if self.include_fragments {
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

        // for absent or trivial fragments, always return success.
        if uri.url.fragment().is_none_or(|x| x.is_empty()) {
            return Status::Ok(StatusCode::OK);
        }

        // directories are treated as if they were a file with no fragments.
        // reaching here means we have a non-trivial fragment on a directory,
        // so return error.
        if path.is_dir() {
            return ErrorKind::InvalidFragment(uri.clone()).into();
        }

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
