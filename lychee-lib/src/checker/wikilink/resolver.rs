use crate::{Base, ErrorKind, Uri, checker::wikilink::index::WikilinkIndex};
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub(crate) struct WikilinkResolver {
    checker: WikilinkIndex,
    fallback_extensions: Vec<String>,
}

/// Tries to resolve a `WikiLink` by searching for the filename in the `WikilinkIndex`
/// Returns the path of the found file if found, otherwise an Error
impl WikilinkResolver {
    /// # Errors
    ///
    /// Fails if `base` is not `Some(Base::Local(_))`.
    pub(crate) fn new(
        base: Option<&Base>,
        fallback_extensions: Vec<String>,
    ) -> Result<Self, ErrorKind> {
        let base = match base {
            None => Err(ErrorKind::WikilinkInvalidBase(
                "Base must be specified for wikilink checking".into(),
            ))?,
            Some(base) => match base {
                Base::Local(p) => p,
                Base::Remote(_) => Err(ErrorKind::WikilinkInvalidBase(
                    "Base cannot be remote".to_string(),
                ))?,
            },
        };

        Ok(Self {
            checker: WikilinkIndex::new(base.clone()),
            fallback_extensions,
        })
    }
    /// Resolves a wikilink by searching the index with fallback extensions.
    pub(crate) fn resolve(&self, path: &Path, uri: &Uri) -> Result<PathBuf, ErrorKind> {
        for ext in &self.fallback_extensions {
            let mut candidate = path.to_path_buf();
            candidate.set_extension(ext);

            if let Some(resolved) = self.checker.contains_path(&candidate) {
                return Ok(resolved);
            }
        }

        Err(ErrorKind::WikilinkNotFound(uri.clone(), path.to_path_buf()))
    }
}
