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
    pub(crate) fn new(base: Base, fallback_extensions: Vec<String>) -> Result<Self, ErrorKind> {
        if let Base::Remote(_) = base {
            return Err(ErrorKind::WikilinkResolverInit(
                "The given base directory was recognized as Remote. A Local directory is needed."
                    .to_string(),
            ));
        }
        Ok(Self {
            checker: WikilinkIndex::new(base),
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
            trace!(
                "Wikilink {uri} not found at {candidate}",
                candidate = candidate.display()
            );
        }

        Err(ErrorKind::WikilinkNotFound(uri.clone()))
    }
}
