use http::StatusCode;
use log::warn;
use std::borrow::Cow;
use std::path::{Path, PathBuf};

use crate::checker::wikilink::resolver::WikilinkResolver;
use crate::{
    Base, ErrorKind, Result, Status, Uri,
    utils::fragment_checker::{FragmentChecker, FragmentInput},
};

/// A utility for checking the existence and validity of file-based URIs.
///
/// `FileChecker` resolves and validates file paths, handling both absolute and relative paths.
/// It supports base path resolution, fallback extensions for files without extensions,
/// and optional fragment checking for HTML files.
#[derive(Debug, Clone)]
pub(crate) struct FileChecker {
    /// List of file extensions to try if the original path doesn't exist.
    fallback_extensions: Vec<String>,
    /// If specified, resolves to one of the given index files if the original path
    /// is a directory.
    ///
    /// If non-`None`, a directory must contain at least one of the file names
    /// in order to be considered a valid link target. Index files names are
    /// required to match regular files, aside from the special `.` name which
    /// will match the directory itself.
    ///
    /// If `None`, index file checking is disabled and directory links are valid
    /// as long as the directory exists on disk.
    index_files: Option<Vec<String>>,
    /// Whether to check for the existence of fragments (e.g., `#section-id`) in HTML files.
    include_fragments: bool,
    /// Utility for performing fragment checks in HTML files.
    fragment_checker: FragmentChecker,
    /// Utility for optionally resolving Wikilinks.
    wikilink_resolver: Option<WikilinkResolver>,
}

impl FileChecker {
    /// Creates a new `FileChecker` with the given configuration.
    ///
    /// # Arguments
    ///
    /// * `base` - Optional base path or URL for resolving wikilinks.
    /// * `fallback_extensions` - List of extensions to try if the original file is not found.
    /// * `index_files` - Optional list of index file names to search for if the path is a directory.
    /// * `include_fragments` - Whether to check for fragment existence in HTML files.
    /// * `include_wikilinks` - Whether to check the existence of Wikilinks found in Markdown files .
    ///
    /// # Errors
    ///
    /// Fails if an invalid `base` is provided when including wikilinks.
    pub(crate) fn new(
        base: Option<&Base>,
        fallback_extensions: Vec<String>,
        index_files: Option<Vec<String>>,
        include_fragments: bool,
        include_wikilinks: bool,
    ) -> Result<Self> {
        let wikilink_resolver = if include_wikilinks {
            Some(WikilinkResolver::new(base, fallback_extensions.clone())?)
        } else {
            None
        };

        Ok(Self {
            fallback_extensions,
            index_files,
            include_fragments,
            fragment_checker: FragmentChecker::new(),
            wikilink_resolver,
        })
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

        let path = self.resolve_local_path(&path, uri);
        match path {
            Ok(path) => self.check_file(path.as_ref(), uri).await,
            Err(err) => err.into(),
        }
    }

    /// Resolves the given local path by applying logic which is specific to local file
    /// checking - currently, this includes fallback extensions and index files.
    ///
    /// # Arguments
    ///
    /// * `path` - The path to check. Need not exist.
    /// * `uri` - The original URI, used for error reporting.
    ///
    /// # Returns
    ///
    /// Returns `Ok` with the resolved path if it is valid, otherwise returns
    /// `Err` with an appropriate error. The returned path, if any, is guaranteed
    /// to exist and may be a file or a directory.
    fn resolve_local_path<'a>(&self, path: &'a Path, uri: &Uri) -> Result<Cow<'a, Path>> {
        let path = match path.metadata() {
            // for non-existing paths, attempt fallback extensions
            // if fallback extensions don't help, try wikilinks
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => self
                .apply_fallback_extensions(path, uri)
                .or_else(|_| {
                    if let Some(resolver) = &self.wikilink_resolver {
                        resolver.resolve(path, uri)
                    } else {
                        Err(ErrorKind::InvalidFilePath(uri.clone()))
                    }
                })
                .map(Cow::Owned),

            // other IO errors are unexpected and should fail the check
            Err(e) => Err(ErrorKind::ReadFileInput(e, path.to_path_buf())),

            // existing directories are resolved via index files
            Ok(meta) if meta.is_dir() => self.apply_index_files(path).map(Cow::Owned),

            // otherwise, path is an existing file - just return the path
            Ok(_) => Ok(Cow::Borrowed(path)),
        };

        // if initial resolution results in a directory, also attempts to apply
        // fallback extensions. probably, this always makes sense because
        // directories are treated as having no fragments, so a real file with
        // a fallback extension (if it exists) will potentially contain more
        // fragments and thus be "more useful".
        //
        // (currently, this case is only reachable if `.` is in the index_files list.)
        match path {
            Ok(dir_path) if dir_path.is_dir() => self
                .apply_fallback_extensions(&dir_path, uri)
                .map(Cow::Owned)
                .or(Ok(dir_path)),
            Ok(path) => Ok(path),
            Err(err) => Err(err),
        }
    }

    /// Resolves a path to a file, applying fallback extensions if necessary.
    ///
    /// This function will try to find a file, first by attempting the given path
    /// itself, then by attempting the path with each extension from
    /// [`FileChecker::fallback_extensions`]. The first existing file (not directory),
    /// if any, will be returned.
    ///
    /// # Arguments
    ///
    /// * `path` - The path to resolve.
    /// * `uri` - The original URI, used for error reporting.
    ///
    /// # Returns
    ///
    /// Returns `Ok(PathBuf)` with the resolved file path, or `Err` if no valid file is found.
    /// If `Ok` is returned, the contained `PathBuf` is guaranteed to exist and be a file.
    fn apply_fallback_extensions(&self, path: &Path, uri: &Uri) -> Result<PathBuf> {
        // If it's already a file, use it directly
        if path.is_file() {
            return Ok(path.to_path_buf());
        }

        // Try fallback extensions
        let mut path_buf = path.to_path_buf();
        for ext in &self.fallback_extensions {
            path_buf.set_extension(ext);
            if path_buf.is_file() {
                return Ok(path_buf);
            }
        }

        Err(ErrorKind::InvalidFilePath(uri.clone()))
    }

    /// Tries to find an index file in the given directory, returning the first match.
    /// The index file behavior is specified by [`FileChecker::index_files`].
    ///
    /// If this is non-`None`, index files must exist and resolved index files are
    /// required to be files, aside from the special name `.` - this will match the
    /// directory itself.
    ///
    /// If `None`, index file resolution is disabled and this function simply
    /// returns the given path.
    ///
    /// # Arguments
    ///
    /// * `dir_path` - The directory within which to search for index files.
    ///   This is assumed to be an existing directory.
    ///
    /// # Returns
    ///
    /// Returns `Ok(PathBuf)` pointing to the first existing index file, or
    /// `Err` if no index file is found. If `Ok` is returned, the contained `PathBuf`
    /// is guaranteed to exist. In most cases, the returned path will be a file path.
    ///
    /// If index files are disabled, simply returns `Ok(dir_path)`.
    fn apply_index_files(&self, dir_path: &Path) -> Result<PathBuf> {
        // this implements the "disabled" case by treating a directory as its
        // own index file.
        let index_names_to_try = match &self.index_files {
            Some(names) => &names[..],
            None => &[".".to_owned()],
        };

        let invalid_index_error = || {
            // Drop empty index file names. These will never be accepted as valid
            // index files, and doing this makes cleaner error reporting.
            let mut names = index_names_to_try.to_vec();
            names.retain(|x| !x.is_empty());

            ErrorKind::InvalidIndexFile(names)
        };

        index_names_to_try
            .iter()
            .find_map(|filename| {
                // for some special index file names, we accept directories as well
                // as files.
                let exists = match filename.as_str() {
                    "." => Path::exists,
                    _ => Path::is_file,
                };

                let path = dir_path.join(filename);
                exists(&path).then_some(path)
            })
            .ok_or_else(invalid_index_error)
    }

    /// Checks a resolved file, optionally verifying fragments for HTML files.
    ///
    /// # Arguments
    ///
    /// * `path` - The resolved path to check.
    /// * `uri` - The original URI, used for error reporting.
    ///
    /// # Returns
    ///
    /// Returns a `Status` indicating the result of the check.
    async fn check_file(&self, path: &Path, uri: &Uri) -> Status {
        if self.include_fragments {
            self.check_fragment(path, uri).await
        } else {
            Status::Ok(StatusCode::OK)
        }
    }

    /// Checks for the existence of a fragment in a path.
    ///
    /// The given path may be a file or a directory. A directory
    /// is treated as if it was an empty file with no fragments.
    ///
    /// # Arguments
    ///
    /// * `path` - The path to the file or directory. Assumed to exist.
    /// * `uri` - The original URI, containing the fragment to check.
    ///
    /// # Returns
    ///
    /// Returns a `Status` indicating the result of the fragment check.
    async fn check_fragment(&self, path: &Path, uri: &Uri) -> Status {
        // for absent or trivial fragments, always return success.
        if uri.url.fragment().is_none_or(str::is_empty) {
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

#[cfg(test)]
mod tests {
    use super::FileChecker;
    use crate::{
        ErrorKind::{InvalidFilePath, InvalidFragment, InvalidIndexFile},
        Status, Uri,
    };
    use test_utils::{fixture_uri, fixtures_path};

    /// Calls [`FileChecker::check`] on the given [`FileChecker`] with given URL
    /// path (relative to the fixtures directory).
    ///
    /// The result of checking the link is matched against the given pattern.
    macro_rules! assert_filecheck {
        ($checker:expr, $path:expr, $pattern:pat) => {
            let uri = Uri::from(fixture_uri!($path));
            let result = $checker.check(&uri).await;
            assert!(
                matches!(result, $pattern),
                "assertion failed: {} should be {} but was '{:?}'",
                &uri,
                stringify!($pattern),
                &result
            );
        };
    }

    /// Calls [`FileChecker::resolve_local_path`] on the given [`FileChecker`]
    /// with given URL path (relative to the fixtures directory).
    ///
    /// The result of resolving the link is matched against the given pattern.
    /// The pattern should match values of type `Result<&str, ErrorKind>`.
    macro_rules! assert_resolves {
        ($checker:expr, $subpath:expr, $expected:pat) => {
            let uri = Uri::from(fixture_uri!($subpath));
            let path = uri
                .url
                .to_file_path()
                .expect("fixture uri should be a valid path");
            let result = $checker.resolve_local_path(&path, &uri);
            let result_subpath = result
                .as_deref()
                .map(|p| p.strip_prefix(fixtures_path!()).unwrap())
                .map(|p| p.to_string_lossy());
            assert!(
                matches!(result_subpath.as_deref(), $expected),
                "{:?} resolved to {:?} but should be {}",
                $subpath,
                result_subpath,
                stringify!($expected)
            );
        };
    }

    #[tokio::test]
    async fn test_default() {
        // default behaviour accepts dir links as long as the directory exists.
        let checker = FileChecker::new(None, vec![], None, true, false).unwrap();

        assert_filecheck!(&checker, "filechecker/index_dir", Status::Ok(_));

        // empty dir is accepted with '.' in index_files, but it contains no fragments.
        assert_resolves!(
            &checker,
            "filechecker/empty_dir",
            Ok("filechecker/empty_dir")
        );
        assert_filecheck!(&checker, "filechecker/empty_dir", Status::Ok(_));
        assert_filecheck!(&checker, "filechecker/empty_dir#", Status::Ok(_));
        assert_filecheck!(
            &checker,
            "filechecker/empty_dir#fragment",
            Status::Error(InvalidFragment(_))
        );

        // even though index.html is present, it is not used because index_files is only
        // '.', so no fragments are found.
        assert_resolves!(
            &checker,
            "filechecker/index_dir",
            Ok("filechecker/index_dir")
        );
        assert_filecheck!(
            &checker,
            "filechecker/index_dir#fragment",
            Status::Error(InvalidFragment(_))
        );
        assert_filecheck!(
            &checker,
            "filechecker/index_dir#non-existingfragment",
            Status::Error(InvalidFragment(_))
        );

        assert_filecheck!(&checker, "filechecker/same_name", Status::Ok(_));

        // because no fallback extensions are configured
        assert_resolves!(
            &checker,
            "filechecker/same_name",
            Ok("filechecker/same_name")
        );
        assert_filecheck!(
            &checker,
            "filechecker/same_name#a",
            Status::Error(InvalidFragment(_))
        );
    }

    #[tokio::test]
    async fn test_index_files() {
        let checker = FileChecker::new(
            None,
            vec![],
            Some(vec!["index.html".to_owned(), "index.md".to_owned()]),
            true,
            false,
        )
        .unwrap();

        assert_resolves!(
            &checker,
            "filechecker/index_dir",
            Ok("filechecker/index_dir/index.html")
        );
        assert_resolves!(
            &checker,
            "filechecker/index_md",
            Ok("filechecker/index_md/index.md")
        );
        // empty is rejected because of no index.html
        assert_resolves!(&checker, "filechecker/empty_dir", Err(InvalidIndexFile(_)));

        // index.html is resolved and fragments are checked.
        assert_filecheck!(&checker, "filechecker/index_dir#fragment", Status::Ok(_));
        assert_filecheck!(
            &checker,
            "filechecker/index_dir#non-existingfragment",
            Status::Error(InvalidFragment(_))
        );

        // directories which look like files should still have index files applied
        assert_resolves!(
            &checker,
            "filechecker/dir_with_extension.html",
            Err(InvalidIndexFile(_))
        );
    }

    #[tokio::test]
    async fn test_both_fallback_and_index_corner() {
        let checker = FileChecker::new(
            None,
            vec!["html".to_owned()],
            Some(vec!["index".to_owned()]),
            false,
            false,
        )
        .unwrap();

        // this test case has a subdir 'same_name' and a file 'same_name.html'.
        // this shows that the index file resolving is applied in this case and
        // fallback extensions are not applied.
        assert_resolves!(&checker, "filechecker/same_name", Err(InvalidIndexFile(_)));

        // this directory has an index.html, but the index_files argument is only "index". this
        // shows that fallback extensions are not applied to index file names, as the index.html is
        // not found.
        assert_resolves!(&checker, "filechecker/index_dir", Err(InvalidIndexFile(_)));

        // a directory called 'dir_with_extension.html' exists. this test shows that fallback
        // extensions must resolve to a file not a directory.
        assert_resolves!(
            &checker,
            "filechecker/dir_with_extension",
            Err(InvalidFilePath(_))
        );
    }

    #[tokio::test]
    async fn test_empty_index_list_corner() {
        // empty index_files list will reject all directory links
        let checker_no_indexes =
            FileChecker::new(None, vec![], Some(vec![]), false, false).unwrap();
        assert_resolves!(
            &checker_no_indexes,
            "filechecker/index_dir",
            Err(InvalidIndexFile(_))
        );
        assert_resolves!(
            &checker_no_indexes,
            "filechecker/empty_dir",
            Err(InvalidIndexFile(_))
        );
    }

    #[tokio::test]
    async fn test_index_list_of_directories_corner() {
        // this test defines index_files to be a list of different names, all of which will
        // resolve to an existing directory. however, because they are directories and not
        // the special '.' name, these should not be accepted as valid index files.
        let dir_names = vec![
            String::new(),
            "./.".to_owned(),
            "..".to_owned(),
            "/".to_owned(),
        ];
        let checker_dir_indexes =
            FileChecker::new(None, vec![], Some(dir_names), false, false).unwrap();
        assert_resolves!(
            &checker_dir_indexes,
            "filechecker/index_dir",
            Err(InvalidIndexFile(_))
        );
        assert_resolves!(
            &checker_dir_indexes,
            "filechecker/empty_dir",
            Err(InvalidIndexFile(_))
        );
    }

    #[tokio::test]
    async fn test_index_file_traversal_corner() {
        // index file names can contain path fragments and they will be traversed.
        let checker_dotdot = FileChecker::new(
            None,
            vec![],
            Some(vec!["../index_dir/index.html".to_owned()]),
            true,
            false,
        )
        .unwrap();
        assert_resolves!(
            &checker_dotdot,
            "filechecker/empty_dir#fragment",
            Ok("filechecker/empty_dir/../index_dir/index.html")
        );

        // absolute paths to a file on disk should also work
        let absolute_html = fixtures_path!()
            .join("filechecker/index_dir/index.html")
            .to_str()
            .expect("expected utf-8 fixtures path")
            .to_owned();
        let checker_absolute =
            FileChecker::new(None, vec![], Some(vec![absolute_html]), true, false).unwrap();
        assert_resolves!(
            &checker_absolute,
            "filechecker/empty_dir#fragment",
            Ok("filechecker/index_dir/index.html")
        );
    }

    #[tokio::test]
    async fn test_fallback_extensions_on_directories() {
        let checker = FileChecker::new(None, vec!["html".to_owned()], None, true, false).unwrap();

        // fallback extensions should be applied when directory links are resolved
        // to directories (i.e., the default index_files behavior or if `.`
        // appears in index_files).
        assert_resolves!(
            &checker,
            "filechecker/same_name#a",
            Ok("filechecker/same_name.html")
        );

        // currently, trailing slashes are ignored and fallback extensions are
        // applied regardless. maybe links with trailing slash should be prevented
        // from resolving to files.
        assert_resolves!(
            &checker,
            "filechecker/same_name/",
            Ok("filechecker/same_name.html")
        );
    }
}
