use http::StatusCode;
use log::warn;
use std::path::{Path, PathBuf};

use crate::{utils::fragment_checker::FragmentChecker, Base, ErrorKind, Status, Uri};

/// A utility for checking the existence and validity of file-based URIs.
///
/// `FileChecker` is responsible for resolving and validating file paths,
/// handling both absolute and relative paths. It supports base path resolution,
/// fallback extensions for files without extensions, and optional fragment checking.
///
/// This creates a `FileChecker` with a base path, fallback extensions for HTML files,
/// and fragment checking enabled.
#[derive(Debug, Clone)]
pub(crate) struct FileChecker {
    /// An optional base path or URL used for resolving relative paths.
    base: Option<Base>,
    /// A list of file extensions to try if the original path doesn't exist.
    fallback_extensions: Vec<String>,
    /// Whether to check for the existence of fragments (e.g., #section-id) in HTML files.
    include_fragments: bool,
    /// A utility for performing fragment checks in HTML files.
    fragment_checker: FragmentChecker,
}

impl FileChecker {
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

    pub(crate) async fn check(&self, uri: &Uri) -> Status {
        let Ok(path) = uri.url.to_file_path() else {
            return ErrorKind::InvalidFilePath(uri.clone()).into();
        };

        let resolved_path = self.resolve_path(&path);
        self.check_path(&resolved_path, uri).await
    }

    fn resolve_path(&self, path: &Path) -> PathBuf {
        if let Some(Base::Local(base_path)) = &self.base {
            if path.is_absolute() {
                let absolute_base_path = if base_path.is_relative() {
                    std::env::current_dir()
                        .unwrap_or_else(|_| PathBuf::new())
                        .join(base_path)
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

    async fn check_path(&self, path: &Path, uri: &Uri) -> Status {
        if path.exists() {
            return self.check_existing_path(path, uri).await;
        }

        self.check_with_fallback_extensions(path, uri).await
    }

    async fn check_existing_path(&self, path: &Path, uri: &Uri) -> Status {
        if self.include_fragments {
            self.check_fragment(path, uri).await
        } else {
            Status::Ok(StatusCode::OK)
        }
    }

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
