//! Core input types and construction logic.
//!
//! The `Input` type handles the construction and validation of various input
//! sources including URLs, file paths, glob patterns, and stdin.

use super::InputResolver;
use super::content::InputContent;
use super::source::InputSource;
use super::source::ResolvedInputSource;
use crate::filter::PathExcludes;
use crate::types::FileType;
use crate::types::file::FileExtensions;
use crate::types::resolver::UrlContentResolver;
use crate::{ErrorKind, Result};
use async_stream::try_stream;
use futures::stream::{Stream, StreamExt};
use glob::glob_with;
use ignore::WalkBuilder;
use reqwest::Url;
use shellexpand::tilde;
use std::path::{Path, PathBuf};
use tokio::io::{AsyncReadExt, stdin};

const STDIN: &str = "-";

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
    ) -> Result<Self> {
        let source = if input == STDIN {
            InputSource::Stdin
        } else {
            // We use [`reqwest::Url::parse`] because it catches some other edge cases that [`http::Request:builder`] does not
            match Url::parse(input) {
                // Weed out non-HTTP schemes, including Windows drive
                // specifiers, which can be parsed by the
                // [url](https://crates.io/crates/url) crate
                Ok(url) if url.scheme() == "http" || url.scheme() == "https" => {
                    InputSource::RemoteUrl(Box::new(url))
                }
                Ok(_) => {
                    // URL parsed successfully, but it's not HTTP or HTTPS
                    return Err(ErrorKind::InvalidFile(PathBuf::from(input)));
                }
                _ => {
                    // This seems to be the only way to determine if this is a glob pattern
                    let is_glob = glob::Pattern::escape(input) != input;

                    if is_glob {
                        InputSource::FsGlob {
                            pattern: input.to_owned(),
                            ignore_case: glob_ignore_case,
                        }
                    } else {
                        // It might be a file path; check if it exists
                        let path = PathBuf::from(input);

                        // On Windows, a filepath can never be mistaken for a
                        // URL, because Windows filepaths use `\` and URLs use
                        // `/`
                        #[cfg(windows)]
                        if path.exists() {
                            // The file exists, so we return the path
                            InputSource::FsPath(path)
                        } else {
                            // We have a valid filepath, but the file does not
                            // exist so we return an error
                            return Err(ErrorKind::InvalidFile(path));
                        }

                        #[cfg(unix)]
                        if path.exists() {
                            InputSource::FsPath(path)
                        } else if input.starts_with('~') || input.starts_with('.') {
                            // The path is not valid, but it might still be a
                            // valid URL.
                            //
                            // Check if the path starts with a tilde (`~`) or a
                            // dot and exit early if it does.
                            //
                            // This check might not be sufficient to cover all cases
                            // but it catches the most common ones
                            return Err(ErrorKind::InvalidFile(path));
                        } else {
                            // Invalid path; check if a valid URL can be constructed from the input
                            // by prefixing it with a `http://` scheme.
                            //
                            // Curl also uses http (i.e. not https), see
                            // https://github.com/curl/curl/blob/70ac27604a2abfa809a7b2736506af0da8c3c8a9/lib/urlapi.c#L1104-L1124
                            //
                            // TODO: We should get rid of this heuristic and
                            // require users to provide a full URL with scheme.
                            // This is a big source of confusion to users.
                            let url = Url::parse(&format!("http://{input}")).map_err(|e| {
                                ErrorKind::ParseUrl(e, "Input is not a valid URL".to_string())
                            })?;
                            InputSource::RemoteUrl(Box::new(url))
                        }
                    }
                }
            }
        };
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
    pub fn from_value(value: &str) -> Result<Self> {
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
    pub fn get_contents(
        self,
        skip_missing: bool,
        skip_hidden: bool,
        skip_gitignored: bool,
        file_extensions: FileExtensions,
        resolver: UrlContentResolver,
        excluded_paths: PathExcludes,
    ) -> impl Stream<Item = Result<InputContent>> {
        try_stream! {
            // Handle simple cases that don't need resolution
            match self.source {
                InputSource::RemoteUrl(url) => {
                    match resolver.url_contents(*url).await {
                        Err(_) if skip_missing => (),
                        Err(e) => Err(e)?,
                        Ok(content) => yield content,
                    }
                    return;
                }
                InputSource::Stdin => {
                    yield Self::stdin_content(self.file_type_hint).await?;
                    return;
                }
                InputSource::String(ref s) => {
                    yield Self::string_content(s, self.file_type_hint);
                    return;
                }
                _ => {}
            }

            // Handle complex cases that need resolution (FsPath, FsGlob)
            let mut sources_stream = Box::pin(InputResolver::resolve(
                &self,
                file_extensions,
                skip_hidden,
                skip_gitignored,
                &excluded_paths,
            ));

            while let Some(source_result) = sources_stream.next().await {
                match source_result {
                    Ok(source) => {
                        let content_result = match source {
                            ResolvedInputSource::FsPath(path) => {
                                Self::path_content(&path).await
                            },
                            ResolvedInputSource::RemoteUrl(url) => {
                                resolver.url_contents(*url).await
                            },
                            ResolvedInputSource::Stdin => {
                                Self::stdin_content(self.file_type_hint).await
                            },
                            ResolvedInputSource::String(s) => {
                                Ok(Self::string_content(&s, self.file_type_hint))
                            },
                        };

                        match content_result {
                            Err(_) if skip_missing => (),
                            Err(e) if matches!(&e, ErrorKind::ReadFileInput(io_err, _) if io_err.kind() == std::io::ErrorKind::InvalidData) => {
                                // If the file contains invalid UTF-8 (e.g. binary), we skip it
                                if let ErrorKind::ReadFileInput(_, path) = &e {
                                    log::warn!("Skipping file with invalid UTF-8 content: {}", path.display());
                                }
                            },
                            Err(e) => Err(e)?,
                            Ok(content) => yield content,
                        }
                    },
                    Err(e) => Err(e)?,
                }
            }
        }
    }

    /// Create a `WalkBuilder` for directory traversal
    fn walk_entries(
        path: &Path,
        file_extensions: FileExtensions,
        skip_hidden: bool,
        skip_gitignored: bool,
    ) -> Result<ignore::Walk> {
        Ok(WalkBuilder::new(path)
            // Enable standard filters if `skip_gitignored `is true.
            // This will skip files ignored by `.gitignore` and other VCS ignore files.
            .standard_filters(skip_gitignored)
            // Override hidden file behavior to be controlled by the separate skip_hidden parameter
            .hidden(skip_hidden)
            // Configure the file types filter to only include files with matching extensions
            .types(file_extensions.try_into()?)
            .build())
    }

    /// Retrieve all sources from this input. The output depends on the type of
    /// input:
    ///
    /// - Remote URLs are returned as is, in their full form
    /// - Filepath Glob Patterns are expanded and each matched entry is returned
    /// - Absolute or relative filepaths are returned as is
    /// - All other input types are not returned
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The glob pattern is invalid or expansion encounters I/O errors
    /// - Directory traversal fails, including:
    ///   - Permission denied when accessing directories or files
    ///   - I/O errors while reading directory contents
    ///   - Filesystem errors (disk errors, network filesystem issues, etc.)
    ///   - Invalid file paths or symbolic link resolution failures
    /// - Errors when reading or evaluating `.gitignore` or `.ignore` files
    /// - Errors occur during file extension or path exclusion evaluation
    ///
    /// Note: Individual glob match failures are logged to stderr but don't terminate the stream.
    /// However, directory traversal errors will stop processing and return the error immediately.
    pub fn get_sources(
        self,
        file_extensions: FileExtensions,
        skip_hidden: bool,
        skip_gitignored: bool,
        excluded_paths: &PathExcludes,
    ) -> impl Stream<Item = Result<String>> {
        try_stream! {
            match self.source {
                InputSource::RemoteUrl(url) => yield url.to_string(),
                InputSource::FsGlob {
                    ref pattern,
                    ignore_case,
                } => {
                    let glob_expanded = tilde(&pattern).to_string();
                    let mut match_opts = glob::MatchOptions::new();
                    match_opts.case_sensitive = !ignore_case;
                    for entry in glob_with(&glob_expanded, match_opts)? {
                        match entry {
                            Ok(path) => {
                                if !Self::is_excluded_path(&path, excluded_paths) {
                                    yield path.to_string_lossy().to_string();
                                }
                            },
                            Err(e) => eprintln!("{e:?}"),
                        }
                    }
                }
                InputSource::FsPath(ref path) => {
                    if path.is_dir() {
                        for entry in Input::walk_entries(
                            path,
                            file_extensions,
                            skip_hidden,
                            skip_gitignored,
                        )? {
                            let entry = entry?;
                            if !Self::is_excluded_path(entry.path(), excluded_paths) {
                                // Only yield files, not directories
                                if entry.file_type().is_some_and(|ft| ft.is_file()) {
                                    yield entry.path().to_string_lossy().to_string();
                                }
                            }
                        }
                    } else if !Self::is_excluded_path(path, excluded_paths) {
                        yield path.to_string_lossy().to_string();
                    }
                }
                InputSource::Stdin => yield "<stdin>".into(),
                InputSource::String(_) => yield "<raw string>".into(),
            }
        }
    }

    /// Check if the given path was excluded from link checking
    fn is_excluded_path(path: &Path, excluded_paths: &PathExcludes) -> bool {
        excluded_paths.is_match(&path.to_string_lossy())
    }

    /// Get the content for a given path.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be read
    pub async fn path_content<P: Into<PathBuf> + AsRef<Path> + Clone>(
        path: P,
    ) -> Result<InputContent> {
        let path = path.into();

        let content = tokio::fs::read_to_string(&path)
            .await
            .map_err(|e| ErrorKind::ReadFileInput(e, path.clone()))?;

        let input_content = InputContent {
            file_type: FileType::from(&path),
            source: ResolvedInputSource::FsPath(path),
            content,
        };

        Ok(input_content)
    }

    /// Create `InputContent` from stdin.
    ///
    /// # Errors
    ///
    /// Returns an error if stdin cannot be read
    pub async fn stdin_content(file_type_hint: Option<FileType>) -> Result<InputContent> {
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
}

impl TryFrom<&str> for Input {
    type Error = crate::ErrorKind;

    fn try_from(value: &str) -> std::result::Result<Self, Self::Error> {
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
