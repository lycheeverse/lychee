use crate::types::FileType;
use crate::{utils, ErrorKind, Result};
use async_stream::try_stream;
use futures::stream::Stream;
use jwalk::WalkDir;
use reqwest::Url;
use serde::{Deserialize, Serialize};
use shellexpand::tilde;
use std::env;
use std::fmt::Display;
use std::fs;
use std::path::{Path, PathBuf};
use tokio::io::{stdin, AsyncReadExt};

const STDIN: &str = "-";
const GLOB_MAX_OPEN_FILES: usize = 4092;

// Check the extension of the given path against the list of known/accepted
// file extensions
fn valid_extension(p: &Path) -> bool {
    matches!(FileType::from(p), FileType::Markdown | FileType::Html)
}

#[derive(Debug)]
/// Encapsulates the content for a given input
pub struct InputContent {
    /// Input source
    pub source: InputSource,
    /// File type of given input
    pub file_type: FileType,
    /// Raw UTF-8 string content
    pub content: String,
}

impl InputContent {
    #[must_use]
    /// Create an instance of `InputContent` from an input string
    pub fn from_string(s: &str, file_type: FileType) -> Self {
        // TODO: consider using Cow (to avoid one .clone() for String types)
        Self {
            source: InputSource::String(s.to_owned()),
            file_type,
            content: s.to_owned(),
        }
    }
}

impl TryFrom<&PathBuf> for InputContent {
    type Error = crate::ErrorKind;

    fn try_from(path: &PathBuf) -> std::result::Result<Self, Self::Error> {
        let input =
            fs::read_to_string(path).map_err(|e| ErrorKind::ReadFileInput(e, path.clone()))?;

        Ok(Self {
            source: InputSource::String(input.clone()),
            file_type: FileType::from(path),
            content: input,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize)]
#[non_exhaustive]
/// Input types which lychee supports
pub enum InputSource {
    /// URL (of HTTP/HTTPS scheme).
    RemoteUrl(Box<Url>),
    /// Unix shell-style glob pattern.
    FsGlob {
        /// The base directory of the glob pattern
        base: PathBuf,
        /// The raw glob patterns matching all input files
        // Note: we cannot use `GlobWalker` directly because it does not
        // implement `Debug`
        patterns: Vec<String>,
        /// Whether the glob pattern is case-insensitive or not
        ignore_case: bool,
    },
    /// File path.
    FsPath(PathBuf),
    /// Standard Input.
    Stdin,
    /// Raw string input.
    String(String),
}

// Custom serialization for enum is needed
// Otherwise we get "key must be a string" when using the JSON writer
// Related: https://github.com/serde-rs/json/issues/45
impl Serialize for InputSource {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.collect_str(self)
    }
}

impl Display for InputSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::RemoteUrl(url) => url.as_str(),
            Self::FsGlob {
                base: _base,
                patterns: _patterns,
                ignore_case: _ignore_case,
            } => {
                "TODO"
                // patterns.join(", ").as_str(),
            }
            Self::FsPath(path) => path.to_str().unwrap_or_default(),
            Self::Stdin => "stdin",
            Self::String(s) => s,
        })
    }
}

/// Lychee Input with optional file hint for parsing
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Input {
    /// Origin of input
    pub source: InputSource,
    /// Hint to indicate which extractor to use
    pub file_type_hint: Option<FileType>,
    /// Excluded paths that will be skipped when reading content
    pub excluded_paths: Option<Vec<PathBuf>>,
}

impl Input {
    /// Construct a new `Input` source. In case the input is a `glob` pattern,
    /// `glob_ignore_case` decides whether matching files against the `glob` is
    /// case-insensitive or not
    ///
    /// # Errors
    ///
    /// Returns an error if the input does not exist (i.e. invalid path)
    /// and the input cannot be parsed as a URL.
    pub fn try_new(
        value: &str,
        file_type_hint: Option<FileType>,
        glob_ignore_case: bool,
        excluded_paths: Option<Vec<PathBuf>>,
    ) -> Result<Self> {
        println!("value: {:?}", value);

        let source = if value == STDIN {
            InputSource::Stdin
        } else if let Ok(url) = Url::parse(value) {
            InputSource::RemoteUrl(Box::new(url))
        } else {
            let base = env::current_dir()?;
            println!("base: {:?}", base);
            println!("value: {:?}", value);
            let raw_patterns = value
                .split_whitespace()
                .clone()
                .map(|s| s.to_owned())
                .collect();

            InputSource::FsGlob {
                base,
                patterns: raw_patterns,
                ignore_case: glob_ignore_case,
            }
        };

        Ok(Self {
            source,
            file_type_hint,
            excluded_paths,
        })
    }

    /// Retrieve the contents from the input
    ///
    /// # Errors
    ///
    /// Returns an error if the contents can not be retrieved
    /// because of an underlying I/O error (e.g. an error while making a
    /// network request or retrieving the contents from the file system)
    pub async fn get_contents(
        self,
        skip_missing: bool,
    ) -> impl Stream<Item = Result<InputContent>> {
        try_stream! {
            match self.source {
                InputSource::RemoteUrl(ref url) => {
                    let content = Self::url_contents(url).await;
                    match content {
                        Err(_) if skip_missing => (),
                        Err(e) => Err(e)?,
                        Ok(content) => yield content,
                    }
                }
                InputSource::FsGlob {
                    base,
                    patterns,
                    ignore_case,
                } => {
                    for await content in Self::glob_contents(self.excluded_paths, base, patterns, ignore_case).await {
                        let content = content?;
                        yield content;
                    }
                }
                InputSource::FsPath(ref path) => {
                    if path.is_dir() {
                        for entry in WalkDir::new(path).skip_hidden(true)
                        .process_read_dir(move |_, _, _, children| {
                            children.retain(|child| {
                                let Ok(entry) = child.as_ref() else { return true };

                                if self.is_excluded_path(&entry.path()) {
                                    return false;
                                }

                                let file_type = entry.file_type();

                                if file_type.is_dir() {
                                    // Required for recursion
                                    return true;
                                }
                                if file_type.is_symlink() {
                                    return false;
                                }
                                if !file_type.is_file() {
                                    return false;
                                }
                                valid_extension(&entry.path())
                            });
                        }) {
                            let entry = entry?;
                            if entry.file_type().is_dir() {
                                continue;
                            }
                            let content = Self::path_content(entry.path()).await?;
                            yield content
                        }
                    } else {
                        if self.is_excluded_path(path) {
                            return;
                        }
                        let content = Self::path_content(path).await;
                        match content {
                            Err(_) if skip_missing => (),
                            Err(e) => Err(e)?,
                            Ok(content) => yield content,
                        };
                    }
                }
                InputSource::Stdin => {
                    let content = Self::stdin_content(self.file_type_hint).await?;
                    yield content;
                }
                InputSource::String(ref s) => {
                    let content = Self::string_content(s, self.file_type_hint);
                    yield content;
                }
            }
        }
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
    /// Returns an error if the globbing fails with the expanded pattern.
    pub async fn get_sources(self) -> impl Stream<Item = Result<String>> {
        try_stream! {
            match self.source {
                InputSource::RemoteUrl(url) => yield url.to_string(),
                InputSource::FsGlob { base, patterns, ignore_case } => {
                    todo!()
                    // let glob_expanded = tilde(&pattern).to_string();
                    // let mut match_opts = glob::MatchOptions::new();

                    // match_opts.case_sensitive = !ignore_case;

                    // for entry in glob_with(&glob_expanded, match_opts)? {
                    //     match entry {
                    //         Ok(path) => yield path.to_string_lossy().to_string(),
                    //         Err(e) => eprintln!("{e:?}")
                    //     }
                    // }
                },
                InputSource::FsPath(path) => yield path.to_string_lossy().to_string(),
                InputSource::Stdin => yield "Stdin".into(),
                InputSource::String(_) => yield "Raw String".into(),
            }
        }
    }

    async fn url_contents(url: &Url) -> Result<InputContent> {
        // Assume HTML for default paths
        let file_type = if url.path().is_empty() || url.path() == "/" {
            FileType::Html
        } else {
            FileType::from(url.as_str())
        };

        let res = reqwest::get(url.clone())
            .await
            .map_err(ErrorKind::NetworkRequest)?;
        let input_content = InputContent {
            source: InputSource::RemoteUrl(Box::new(url.clone())),
            file_type,
            content: res.text().await.map_err(ErrorKind::ReadResponseBody)?,
        };

        Ok(input_content)
    }

    async fn glob_contents(
        excluded_paths: Option<Vec<PathBuf>>,
        base: PathBuf,
        patterns: Vec<String>,
        ignore_case: bool,
    ) -> impl Stream<Item = Result<InputContent>> {
        let entries = globwalk::GlobWalkerBuilder::from_patterns(base, &patterns)
            .case_insensitive(ignore_case)
            .max_open(GLOB_MAX_OPEN_FILES)
            .build()
            .unwrap();

        try_stream! {
            for entry in entries {
                match entry {
                    Ok(entry) => {
                        let path = entry.path();

                        // Directories can have a suffix which looks like
                        // a file extension (like `foo.html`). This can lead to
                        // unexpected behavior with glob patterns like
                        // `**/*.html`. Therefore filter these out.
                        // See <https://github.com/lycheeverse/lychee/pull/262#issuecomment-913226819>
                        if path.is_dir() {
                            continue;
                        }
                        if let Some(ref excluded_paths) = excluded_paths {
                            if is_excluded_path(&excluded_paths, &path.to_path_buf()) {
                                continue;
                            }
                        }
                        let content: InputContent = Self::path_content(entry.path()).await?;
                        yield content;
                    }
                    Err(e) => eprintln!("{e:?}"),
                }
            }
        }
    }

    /// Check if the given path was excluded from link checking
    fn is_excluded_path(&self, path: &PathBuf) -> bool {
        let Some(excluded_paths) = &self.excluded_paths else {
            return false
        };
        is_excluded_path(excluded_paths, path)
    }

    /// Get the input content of a given path
    /// # Errors
    ///
    /// Will return `Err` if file contents can't be read
    pub async fn path_content<P: Into<PathBuf> + AsRef<Path> + Clone>(
        path: P,
    ) -> Result<InputContent> {
        let path = path.into();
        let content = tokio::fs::read_to_string(&path)
            .await
            .map_err(|e| ErrorKind::ReadFileInput(e, path.clone()))?;
        let input_content = InputContent {
            file_type: FileType::from(&path),
            source: InputSource::FsPath(path),
            content,
        };

        Ok(input_content)
    }

    async fn stdin_content(file_type_hint: Option<FileType>) -> Result<InputContent> {
        let mut content = String::new();
        let mut stdin = stdin();
        stdin.read_to_string(&mut content).await?;

        let input_content = InputContent {
            source: InputSource::Stdin,
            file_type: file_type_hint.unwrap_or_default(),
            content,
        };

        Ok(input_content)
    }

    fn string_content(s: &str, file_type_hint: Option<FileType>) -> InputContent {
        InputContent::from_string(s, file_type_hint.unwrap_or_default())
    }
}

/// Function for path exclusion tests
///
/// This is a standalone function to allow for easier testing
fn is_excluded_path(excluded_paths: &[PathBuf], path: &PathBuf) -> bool {
    for excluded in excluded_paths {
        if let Ok(true) = utils::path::contains(excluded, path) {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_input_handles_real_relative_paths() {
        let test_file = "./Cargo.toml";
        let path = Path::new(test_file);

        assert!(path.exists());
        assert!(path.is_relative());

        let input = Input::try_new(test_file, None, false, None);
        assert!(input.is_ok());
        assert!(matches!(
            input,
            Ok(Input {
                source: InputSource::FsPath(PathBuf { .. }),
                file_type_hint: None,
                excluded_paths: None
            })
        ));
    }

    #[test]
    fn test_input_handles_nonexistent_relative_paths() {
        let test_file = "./nonexistent/relative/path";
        let path = Path::new(test_file);

        assert!(!path.exists());
        assert!(path.is_relative());

        let input = Input::try_new(test_file, None, false, None);
        assert!(input.is_err());
        assert!(matches!(
            input,
            Err(ErrorKind::FileNotFound(PathBuf { .. }))
        ));
    }

    #[test]
    fn test_valid_extension() {
        assert!(valid_extension(Path::new("file.md")));
        assert!(valid_extension(Path::new("file.markdown")));
        assert!(valid_extension(Path::new("file.html")));
        assert!(valid_extension(Path::new("file.htm")));
        assert!(valid_extension(Path::new("file.HTM")));
        assert!(!valid_extension(Path::new("file.txt")));
        assert!(!valid_extension(Path::new("file")));
    }

    #[test]
    fn test_no_exclusions() {
        let dir = tempfile::tempdir().unwrap();
        assert!(!is_excluded_path(&[], &dir.path().to_path_buf()));
    }

    #[test]
    fn test_excluded() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().to_path_buf();
        assert!(is_excluded_path(&[path.clone()], &path));
    }

    #[test]
    fn test_excluded_subdir() {
        let parent_dir = tempfile::tempdir().unwrap();
        let parent = parent_dir.path();
        let child_dir = tempfile::tempdir_in(parent).unwrap();
        let child = child_dir.path();
        assert!(is_excluded_path(
            &[parent.to_path_buf()],
            &child.to_path_buf()
        ));
    }

    #[test]
    fn test_url_without_scheme() {
        let input = Input::try_new("example.com", None, false, None);
        assert_eq!(
            input.unwrap().source.to_string(),
            String::from("http://example.com/")
        );
    }
}
