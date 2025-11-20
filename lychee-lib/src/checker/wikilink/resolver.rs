use crate::{Base, ErrorKind, Uri, checker::wikilink::index::WikilinkIndex};
use log::trace;
use std::path::{Path, PathBuf};

#[derive(Clone, Debug)]
pub(crate) struct WikilinkResolver {
    checker: WikilinkIndex,
    fallback_extensions: Vec<String>,
}

/// Tries to resolve a `WikiLink` by searching for the filename in the `WikilinkIndex`
/// Returns the path of the found file if found, otherwise an Error
impl WikilinkResolver {
    pub(crate) fn new(basedir: Option<Base>, fallback_extensions: Vec<String>) -> Self {
        Self {
            checker: WikilinkIndex::new(basedir),
            fallback_extensions,
        }
    }
    /// Resolves a wikilink by searching the index with fallback extensions.
    pub(crate) fn resolve(&self, path: &Path, uri: &Uri) -> Result<PathBuf, ErrorKind> {
        for ext in &self.fallback_extensions {
            let mut candidate = path.to_path_buf();
            candidate.set_extension(ext);

            if let Some(resolved) = self.checker.contains_path(&candidate) {
                return Ok(resolved);
            }
            trace!("Wikilink not found: {} at {}", uri, candidate.display());
        }

        Err(ErrorKind::WikilinkNotFound(uri.clone()))
    }
}
