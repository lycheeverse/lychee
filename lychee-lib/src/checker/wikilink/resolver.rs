use crate::{
    BaseInfo, ErrorKind, Uri,
    checker::{fallback_candidates, wikilink::index::WikilinkIndex},
};
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
    /// Fails if the URL within `base` is not a file:// URL.
    pub(crate) fn new(
        base: &BaseInfo,
        fallback_extensions: Vec<String>,
    ) -> Result<Self, ErrorKind> {
        let base = match base {
            BaseInfo::None => Err(ErrorKind::WikilinkInvalidBase(
                "Base must be specified for wikilink checking".into(),
            ))?,
            base => base,
        };
        let base = base.to_file_path().ok_or(ErrorKind::WikilinkInvalidBase(
            "Base cannot be remote".to_string(),
        ))?;

        Ok(Self {
            checker: WikilinkIndex::new(base),
            fallback_extensions,
        })
    }
    /// Resolves a wikilink by searching the index for each
    /// [`fallback_candidates`] entry.
    pub(crate) fn resolve(&self, path: &Path, uri: &Uri) -> Result<PathBuf, ErrorKind> {
        for candidate in fallback_candidates(path, &self.fallback_extensions) {
            if let Some(resolved) = self.checker.contains_path(&candidate) {
                return Ok(resolved);
            }
        }

        Err(ErrorKind::WikilinkNotFound(uri.clone(), path.to_path_buf()))
    }
}

#[cfg(test)]
mod tests {
    use crate::{BaseInfo, ErrorKind, Uri, checker::wikilink::resolver::WikilinkResolver};
    use test_utils::{fixture_uri, fixtures_path};

    #[test]
    fn test_wikilink_resolves_to_filename() {
        let resolver = WikilinkResolver::new(
            &BaseInfo::from_path(&fixtures_path!().join("wiki")).unwrap(),
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
    fn test_wikilink_resolves_dotted_filename() {
        let dir = tempfile::tempdir().expect("temp dir should be creatable");
        for name in ["Page.v1.md", "Page.md"] {
            std::fs::write(dir.path().join(name), "").expect("fixture should be writable");
        }

        let resolver = WikilinkResolver::new(
            &BaseInfo::from_path(dir.path()).unwrap(),
            vec!["md".to_string()],
        )
        .unwrap();
        let uri = Uri {
            url: url::Url::from_directory_path(dir.path())
                .unwrap()
                .join("Page.v1")
                .unwrap(),
        };
        let path = dir.path().join("Page.v1");

        // `Page.md` exists only so the replaced candidate is a real alternative.
        assert_eq!(
            resolver.resolve(&path, &uri),
            Ok(dir.path().join("Page.v1.md"))
        );
    }

    #[test]
    fn test_wikilink_prefers_the_name_as_written() {
        let dir = tempfile::tempdir().expect("temp dir should be creatable");
        // `image.png` sits in a subdirectory so a match proves the index was
        // consulted; `image.png.md` is the appended candidate it must beat.
        std::fs::create_dir(dir.path().join("shadow")).unwrap();
        std::fs::write(dir.path().join("shadow/image.png"), "").unwrap();
        std::fs::write(dir.path().join("image.png.md"), "").unwrap();

        let resolver = WikilinkResolver::new(
            &BaseInfo::from_path(dir.path()).unwrap(),
            vec!["md".to_string()],
        )
        .unwrap();
        let uri = Uri {
            url: url::Url::from_directory_path(dir.path())
                .unwrap()
                .join("image.png")
                .unwrap(),
        };

        assert_eq!(
            resolver.resolve(&dir.path().join("image.png"), &uri),
            Ok(dir.path().join("shadow/image.png"))
        );
    }

    #[test]
    fn test_wikilink_index_ignores_directories() {
        let dir = tempfile::tempdir().expect("temp dir should be creatable");
        // A directory is the only thing named `Notes.md`, so the lookup can
        // only succeed if directories are indexed.
        std::fs::create_dir(dir.path().join("Notes.md")).unwrap();

        let resolver = WikilinkResolver::new(
            &BaseInfo::from_path(dir.path()).unwrap(),
            vec!["md".to_string()],
        )
        .unwrap();
        let uri = Uri {
            url: url::Url::from_directory_path(dir.path())
                .unwrap()
                .join("Notes.md")
                .unwrap(),
        };

        assert!(matches!(
            resolver.resolve(&dir.path().join("Notes.md"), &uri),
            Err(ErrorKind::WikilinkNotFound(..))
        ));
    }

    #[test]
    fn test_wikilink_not_found() {
        let resolver = WikilinkResolver::new(
            &BaseInfo::from_path(&fixtures_path!().join("wiki")).unwrap(),
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
