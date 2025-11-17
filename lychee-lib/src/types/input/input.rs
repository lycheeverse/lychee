//! Core input types and construction logic.
//!
//! The `Input` type handles the construction and validation of various input
//! sources including URLs, file paths, glob patterns, and stdin.

use super::InputResolver;
use super::content::InputContent;
use super::source::{InputSource, ResolvedInputSource};
use crate::Preprocessor;
use crate::filter::PathExcludes;
use crate::types::{FileType, RequestError, file::FileExtensions, resolver::UrlContentResolver};
use crate::{ErrorKind, LycheeResult};
use async_stream::try_stream;
use futures::stream::{Stream, StreamExt};
use std::path::{Path, PathBuf};
use tokio::io::{AsyncReadExt, stdin};

/// Lychee Input with optional file hint for parsing
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Input {
    /// Origin of input
    pub source: InputSource,

    /// Hint to indicate which extractor to use
    ///
    /// If this is not provided, the extractor will be guessed from the input
    /// (e.g. file extension or URL path)
    pub file_type_hint: Option<FileType>,
}

impl Input {
    /// Construct a new `Input` source. In case the input is a `glob` pattern,
    /// `glob_ignore_case` decides whether matching files against the `glob` is
    /// case-insensitive or not
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - the input does not exist (i.e. the path is invalid)
    /// - the input cannot be parsed as a URL
    pub fn new(
        input: &str,
        file_type_hint: Option<FileType>,
        glob_ignore_case: bool,
    ) -> LycheeResult<Self> {
        let source = InputSource::new(input, glob_ignore_case)?;
        Ok(Self {
            source,
            file_type_hint,
        })
    }

    /// Convenience constructor with default settings
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - the input does not exist (i.e. the path is invalid)
    /// - the input cannot be parsed as a URL
    pub fn from_value(value: &str) -> LycheeResult<Self> {
        Self::new(value, None, false)
    }

    /// Create an `Input` from an existing `InputSource`
    ///
    /// The file type will be determined later when processing the input.
    #[must_use]
    pub const fn from_input_source(source: InputSource) -> Self {
        Self {
            source,
            file_type_hint: None,
        }
    }

    /// Retrieve the contents from the input
    ///
    /// If the input is a path, only search through files that match the given
    /// file extensions.
    ///
    /// # Errors
    ///
    /// Returns an error if the contents can not be retrieved because of an
    /// underlying I/O error (e.g. an error while making a network request or
    /// retrieving the contents from the file system)
    #[allow(
        clippy::too_many_arguments,
        reason = "https://github.com/lycheeverse/lychee/issues/1898"
    )]
    pub fn get_contents(
        self,
        skip_missing: bool,
        skip_hidden: bool,
        skip_ignored: bool,
        file_extensions: FileExtensions,
        resolver: UrlContentResolver,
        excluded_paths: PathExcludes,
        preprocessor: Option<Preprocessor>,
    ) -> impl Stream<Item = Result<InputContent, RequestError>> {
        try_stream! {
            let source = self.source.clone();

            let user_input_error =
                move |e: ErrorKind| RequestError::UserInputContent(source.clone(), e);
            let discovered_input_error =
                |e: ErrorKind| RequestError::GetInputContent(self.source.clone(), e);

            // Handle simple cases that don't need resolution. Also, perform
            // simple *stateful* checks for more complex input sources.
            //
            // Stateless well-formedness checks (e.g., checking glob syntax)
            // are done in InputSource::new.
            match self.source {
                InputSource::RemoteUrl(url) => {
                    match resolver.url_contents(*url).await {
                        Err(_) if skip_missing => (),
                        Err(e) => Err(user_input_error(e))?,
                        Ok(content) => yield content,
                    }
                    return;
                }
                InputSource::FsPath(ref path) => {
                    let is_readable = if path.is_dir() {
                        path.read_dir()
                            .map(|_| ())
                            .map_err(|e| ErrorKind::DirTraversal(ignore::Error::Io(e)))
                    } else {
                        // This checks existence without requiring an open. Opening here,
                        // then re-opening later, might cause problems with pipes. This
                        // does not validate permissions.
                        path.metadata()
                            .map(|_| ())
                            .map_err(|e| ErrorKind::ReadFileInput(e, path.clone()))
                    };

                    is_readable.map_err(user_input_error)?;
                }
                InputSource::Stdin => {
                    yield Self::stdin_content(self.file_type_hint)
                        .await
                        .map_err(user_input_error)?;
                    return;
                }
                InputSource::String(ref s) => {
                    yield Self::string_content(s, self.file_type_hint);
                    return;
                }
                _ => {}
            }

            // Handle complex cases that need resolution (FsPath, FsGlob)
            let mut sources_stream = InputResolver::resolve(
                &self,
                file_extensions,
                skip_hidden,
                skip_ignored,
                &excluded_paths,
            );

            let mut sources_empty = true;

            while let Some(source_result) = sources_stream.next().await {
                match source_result {
                    Ok(source) => {
                        let content_result = match source {
                            ResolvedInputSource::FsPath(path) => {
                                Self::path_content(&path, preprocessor.as_ref()).await
                            },
                            ResolvedInputSource::RemoteUrl(url) => {
                                resolver.url_contents(*url).await
                            }
                            ResolvedInputSource::Stdin => {
                                Self::stdin_content(self.file_type_hint).await
                            }
                            ResolvedInputSource::String(s) => {
                                Ok(Self::string_content(&s, self.file_type_hint))
                            }
                        };

                        match content_result {
                            Err(_) if skip_missing => (),
                            Err(e) if matches!(&e, ErrorKind::ReadFileInput(io_err, _) if io_err.kind() == std::io::ErrorKind::InvalidData) =>
                            {
                                // If the file contains invalid UTF-8 (e.g. binary), we skip it
                                if let ErrorKind::ReadFileInput(_, path) = &e {
                                    log::warn!(
                                        "Skipping file with invalid UTF-8 content: {}",
                                        path.display()
                                    );
                                }
                            }
                            Err(e) => Err(discovered_input_error(e))?,
                            Ok(content) => {
                                sources_empty = false;
                                yield content
                            }
                        }
                    }
                    Err(e) => Err(discovered_input_error(e))?,
                }
            }

            if sources_empty {
                log::warn!("{}: No files found for this input source", self.source);
            }
        }
    }

    /// Retrieve all sources from this input. The output depends on the type of
    /// input:
    ///
    /// - Remote URLs are returned as is, in their full form
    /// - Glob patterns are expanded and each matched entry is returned
    /// - Absolute or relative filepaths are returned as-is
    /// - Stdin input is returned as the special string "<stdin>"
    /// - A raw string input is returned as the special string "<raw string>"
    ///
    /// # Errors
    ///
    /// Returns an error if [`InputResolver::resolve`] returns an error.
    pub fn get_sources(
        self,
        file_extensions: FileExtensions,
        skip_hidden: bool,
        skip_ignored: bool,
        excluded_paths: &PathExcludes,
    ) -> impl Stream<Item = LycheeResult<String>> {
        InputResolver::resolve(
            &self,
            file_extensions,
            skip_hidden,
            skip_ignored,
            excluded_paths,
        )
        .map(|res| {
            res.map(|src| match src {
                ResolvedInputSource::FsPath(path) => path.to_string_lossy().to_string(),
                ResolvedInputSource::RemoteUrl(url) => url.to_string(),
                ResolvedInputSource::Stdin => "<stdin>".to_string(),
                ResolvedInputSource::String(_) => "<raw string>".to_string(),
            })
        })
    }

    /// Get the content for a given path.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be read
    /// or [`Preprocessor`] failed
    pub async fn path_content<P: Into<PathBuf> + AsRef<Path> + Clone>(
        path: P,
        preprocessor: Option<&Preprocessor>,
    ) -> LycheeResult<InputContent> {
        let path = path.into();
        let content = Self::get_content(&path, preprocessor).await?;

        Ok(InputContent {
            file_type: FileType::from(&path),
            source: ResolvedInputSource::FsPath(path),
            content,
        })
    }

    /// Create `InputContent` from stdin.
    ///
    /// # Errors
    ///
    /// Returns an error if stdin cannot be read
    pub async fn stdin_content(file_type_hint: Option<FileType>) -> LycheeResult<InputContent> {
        let mut content = String::new();
        let mut stdin = stdin();
        stdin.read_to_string(&mut content).await?;

        let input_content = InputContent {
            source: ResolvedInputSource::Stdin,
            file_type: file_type_hint.unwrap_or_default(),
            content,
        };

        Ok(input_content)
    }

    /// Create `InputContent` from a string.
    #[must_use]
    pub fn string_content(s: &str, file_type_hint: Option<FileType>) -> InputContent {
        InputContent::from_string(s, file_type_hint.unwrap_or_default())
    }

    /// Get content of file.
    /// Get preprocessed file content if [`Preprocessor`] is [`Some`]
    async fn get_content(
        path: &PathBuf,
        preprocessor: Option<&Preprocessor>,
    ) -> LycheeResult<String> {
        if let Some(pre) = preprocessor {
            pre.process(path)
        } else {
            Ok(tokio::fs::read_to_string(path)
                .await
                .map_err(|e| ErrorKind::ReadFileInput(e, path.clone()))?)
        }
    }
}

impl TryFrom<&str> for Input {
    type Error = crate::ErrorKind;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::from_value(value)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::filter::PathExcludes;

    /// A standalone function to allow for easier testing of path exclusion logic
    pub fn is_excluded_path(excluded_paths: &PathExcludes, path: &Path) -> bool {
        excluded_paths.is_match(&path.to_string_lossy())
    }

    #[test]
    fn test_input_handles_real_relative_paths() {
        let test_file = "./Cargo.toml";
        let path = Path::new(test_file);

        assert!(path.exists());
        assert!(path.is_relative());

        let input = Input::new(test_file, None, false);
        assert!(input.is_ok());
        assert!(matches!(
            input,
            Ok(Input {
                source: InputSource::FsPath(PathBuf { .. }),
                file_type_hint: None,
            })
        ));
    }

    #[test]
    fn test_input_handles_nonexistent_relative_paths() {
        let test_file = "./nonexistent/relative/path";
        let path = Path::new(test_file);

        assert!(!path.exists());
        assert!(path.is_relative());

        let input = Input::from_value(test_file);
        assert!(input.is_err());
        assert!(matches!(input, Err(ErrorKind::InvalidFile(PathBuf { .. }))));
    }

    #[test]
    fn test_no_exclusions() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!is_excluded_path(&PathExcludes::empty(), dir.path()));
    }

    #[test]
    fn test_excluded() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path();
        let excludes = PathExcludes::new([path.to_string_lossy()]).unwrap();
        assert!(is_excluded_path(&excludes, path));
    }

    #[test]
    fn test_excluded_subdir() {
        let parent_dir = tempfile::tempdir().unwrap();
        let parent = parent_dir.path();
        let child_dir = tempfile::tempdir_in(parent).unwrap();
        let child = child_dir.path();

        let excludes = PathExcludes::new([parent.to_string_lossy()]).unwrap();
        assert!(is_excluded_path(&excludes, child));
    }

    #[test]
    fn test_url_without_scheme() {
        let input = Input::from_value("example.com");
        assert_eq!(
            input.unwrap().source.to_string(),
            String::from("http://example.com/")
        );
    }

    // Ensure that a Windows file path is not mistaken for a URL.
    #[cfg(windows)]
    #[test]
    fn test_windows_style_filepath_not_existing() {
        let input = Input::from_value("C:\\example\\project\\here");
        assert!(input.is_err());
        let input = input.unwrap_err();

        match input {
            ErrorKind::InvalidFile(_) => (),
            _ => panic!("Should have received InvalidFile error"),
        }
    }

    // Ensure that a Windows-style file path to an existing file is recognized
    #[cfg(windows)]
    #[test]
    fn test_windows_style_filepath_existing() {
        use std::env::temp_dir;
        use tempfile::NamedTempFile;

        let dir = temp_dir();
        let file = NamedTempFile::new_in(dir).unwrap();
        let path = file.path();
        let input = Input::from_value(path.to_str().unwrap()).unwrap();

        match input.source {
            InputSource::FsPath(_) => (),
            _ => panic!("Input source should be FsPath but was not"),
        }
    }

    #[test]
    fn test_url_scheme_check_succeeding() {
        // Valid http and https URLs
        assert!(matches!(
            Input::from_value("http://example.com"),
            Ok(Input {
                source: InputSource::RemoteUrl(_),
                ..
            })
        ));
        assert!(matches!(
            Input::from_value("https://example.com"),
            Ok(Input {
                source: InputSource::RemoteUrl(_),
                ..
            })
        ));
        assert!(matches!(
            Input::from_value("http://subdomain.example.com/path?query=value",),
            Ok(Input {
                source: InputSource::RemoteUrl(_),
                ..
            })
        ));
        assert!(matches!(
            Input::from_value("https://example.com:8080"),
            Ok(Input {
                source: InputSource::RemoteUrl(_),
                ..
            })
        ));
    }

    #[test]
    fn test_url_scheme_check_failing() {
        // Invalid schemes
        assert!(matches!(
            Input::from_value("ftp://example.com"),
            Err(ErrorKind::InvalidFile(_))
        ));
        assert!(matches!(
            Input::from_value("httpx://example.com"),
            Err(ErrorKind::InvalidFile(_))
        ));
        assert!(matches!(
            Input::from_value("file:///path/to/file"),
            Err(ErrorKind::InvalidFile(_))
        ));
        assert!(matches!(
            Input::from_value("mailto:user@example.com"),
            Err(ErrorKind::InvalidFile(_))
        ));
    }

    #[test]
    fn test_non_url_inputs() {
        // Non-URL inputs
        assert!(matches!(
            Input::from_value("./local/path"),
            Err(ErrorKind::InvalidFile(_))
        ));
        assert!(matches!(
            Input::from_value("*.md"),
            Ok(Input {
                source: InputSource::FsGlob { .. },
                ..
            })
        ));
        // Assuming the current directory exists
        assert!(matches!(
            Input::from_value("."),
            Ok(Input {
                source: InputSource::FsPath(_),
                ..
            })
        ));
    }
}
