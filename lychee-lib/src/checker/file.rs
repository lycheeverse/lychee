use crate::{utils::fragment_checker::FragmentChecker, Base, ErrorKind, Status, Uri};
use http::StatusCode;
use log::warn;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub(crate) struct FileChecker {
    base: Option<Base>,
    fallback_extensions: Vec<String>,
    include_fragments: bool,
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

        if path.is_absolute() {
            let resolved_path = self.resolve_absolute_path(&path);
            return self.check_resolved_path(&resolved_path, uri).await;
        }

        self.check_path(&path, uri).await
    }

    async fn check_resolved_path(&self, path: &Path, uri: &Uri) -> Status {
        if path.exists() {
            if self.include_fragments {
                self.check_fragment(path, uri).await
            } else {
                Status::Ok(StatusCode::OK)
            }
        } else {
            ErrorKind::InvalidFilePath(uri.clone()).into()
        }
    }

    async fn check_path(&self, path: &Path, uri: &Uri) -> Status {
        if path.exists() {
            return self.check_existing_path(path, uri).await;
        }

        if path.extension().is_some() {
            return ErrorKind::InvalidFilePath(uri.clone()).into();
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
        for ext in &self.fallback_extensions {
            path_buf.set_extension(ext);
            if path_buf.exists() {
                return self.check_existing_path(&path_buf, uri).await;
            }
        }
        ErrorKind::InvalidFilePath(uri.clone()).into()
    }

    fn resolve_absolute_path(&self, path: &Path) -> PathBuf {
        if let Some(Base::Local(base_path)) = &self.base {
            let absolute_base_path = if base_path.is_relative() {
                std::env::current_dir()
                    .unwrap_or_else(|_| PathBuf::new())
                    .join(base_path)
            } else {
                base_path.to_path_buf()
            };

            let stripped = path.strip_prefix("/").unwrap_or(path);
            absolute_base_path.join(stripped)
        } else {
            path.to_path_buf()
        }
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
