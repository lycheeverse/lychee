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

#[cfg(test)]
mod tests {
    use crate::{Base, ErrorKind, Uri, checker::wikilink::resolver::WikilinkResolver};
    use test_utils::{fixture_uri, fixtures_path};

    #[test]
    fn test_wikilink_resolves_to_filename() {
        let resolver = WikilinkResolver::new(
            Some(&Base::Local(fixtures_path!().join("wiki"))),
            vec!["md".to_string()],
        )
        .unwrap();
        let uri = Uri {
            url: fixture_uri!("wiki/Usage"),
        };
        let path = fixtures_path!().join("Usage");
        let expected_result = fixtures_path!().join("wiki/Usage.md");
        assert_eq!(resolver.resolve(&path, &uri), Ok(expected_result));
    }

    #[test]
    fn test_wikilink_not_found() {
        let resolver = WikilinkResolver::new(
            Some(&Base::Local(fixtures_path!().join("wiki"))),
            vec!["md".to_string()],
        )
        .unwrap();
        let uri = Uri {
            url: fixture_uri!("wiki/404"),
        };
        let path = fixtures_path!().join("404");
        assert!(matches!(
            resolver.resolve(&path, &uri),
            Err(ErrorKind::WikilinkNotFound(..))
        ));
    }
}
