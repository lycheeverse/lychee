use crate::types::FileType;
use crate::Result;
use async_stream::try_stream;
use futures::stream::Stream;
use glob::glob_with;
use jwalk::WalkDir;
use reqwest::Url;
use serde::Serialize;
use shellexpand::tilde;
use std::fmt::Display;
use std::path::{Path, PathBuf};
use tokio::io::{stdin, AsyncReadExt};

use super::RecursionLevel;

const STDIN: &str = "-";

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

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
/// Input types which lychee supports
pub enum InputSource {
    /// URL (of HTTP/HTTPS scheme).
    RemoteUrl(Box<Url>),
    /// Unix shell-style glob pattern.
    FsGlob {
        /// The glob pattern matching all input files
        pattern: String,
        /// Don't be case sensitive when matching files against a glob
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
            Self::FsGlob { pattern, .. } => pattern,
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
    /// Current recursion level
    pub recursion_level: RecursionLevel,
}

impl Input {
    #[must_use]
    /// Construct a new `Input` source with a default recursion level of 0. In
    /// case the input is a `glob` pattern, `glob_ignore_case` decides whether
    /// matching files against the `glob` is case-insensitive or not
    pub fn new(value: &str, file_type_hint: Option<FileType>, glob_ignore_case: bool) -> Self {
        Self::with_recursion(value, file_type_hint, glob_ignore_case, 0)
    }

    #[must_use]
    /// Construct a new `Input` source with the given recursion level.
    pub fn with_recursion(
        value: &str,
        file_type_hint: Option<FileType>,
        glob_ignore_case: bool,
        recursion_level: RecursionLevel,
    ) -> Self {
        let source = if value == STDIN {
            InputSource::Stdin
        } else if let Ok(url) = Url::parse(value) {
            InputSource::RemoteUrl(Box::new(url))
        } else {
            // this seems to be the only way to determine if this is a glob pattern
            let is_glob = glob::Pattern::escape(value) != value;

            if is_glob {
                InputSource::FsGlob {
                    pattern: value.to_owned(),
                    ignore_case: glob_ignore_case,
                }
            } else {
                InputSource::FsPath(value.into())
            }
        };
        Self {
            source,
            file_type_hint,
            recursion_level,
        }
    }

    #[allow(clippy::missing_panics_doc)]
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
                    let contents: InputContent = Self::url_contents(url).await?;
                    yield contents;
                },
                InputSource::FsGlob {
                    ref pattern,
                    ignore_case,
                } => {
                    for await content in Self::glob_contents(pattern, ignore_case).await {
                        let content = content?;
                        yield content;
                    }
                }
                InputSource::FsPath(ref path) => {
                    if path.is_dir() {
                        for entry in WalkDir::new(path).skip_hidden(true)
                        .process_read_dir(|_, _, _, children| {
                            children.retain(|child| {
                                let entry = match child.as_ref() {
                                    Ok(x) => x,
                                    Err(_) => return true,
                                };

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
                                return valid_extension(&entry.path());
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
                        let content = Self::path_content(path).await;
                        match content {
                            Err(_) if skip_missing => (),
                            Err(e) => Err(e)?,
                            Ok(content) => yield content,
                        };
                    }
                },
                InputSource::Stdin => {
                    let content = Self::stdin_content(self.file_type_hint).await?;
                    yield content;
                },
                InputSource::String(ref s) => {
                    let content = Self::string_content(s, self.file_type_hint);
                    yield content;
                },
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

        let res = reqwest::get(url.clone()).await?;
        let input_content = InputContent {
            source: InputSource::RemoteUrl(Box::new(url.clone())),
            file_type,
            content: res.text().await?,
        };

        Ok(input_content)
    }

    async fn glob_contents(
        path_glob: &str,
        ignore_case: bool,
    ) -> impl Stream<Item = Result<InputContent>> + '_ {
        let glob_expanded = tilde(&path_glob).to_string();
        let mut match_opts = glob::MatchOptions::new();

        match_opts.case_sensitive = !ignore_case;

        try_stream! {
            for entry in glob_with(&glob_expanded, match_opts)? {
                match entry {
                    Ok(path) => {
                        // Directories can have a suffix which looks like
                        // a file extension (like `foo.html`). This can lead to
                        // unexpected behavior with glob patterns like
                        // `**/*.html`. Therefore filter these out.
                        // See https://github.com/lycheeverse/lychee/pull/262#issuecomment-913226819
                        if path.is_dir() {
                            continue;
                        }
                        let content: InputContent = Self::path_content(&path).await?;
                        yield content;
                    }
                    Err(e) => println!("{:?}", e),
                }
            }
        }
    }

    /// Get the input content of a given path
    /// # Errors
    ///
    /// Will return `Err` if file contents can't be read
    pub async fn path_content<P: Into<PathBuf> + AsRef<Path> + Clone>(
        path: P,
    ) -> Result<InputContent> {
        let content = tokio::fs::read_to_string(&path)
            .await
            .map_err(|e| (path.clone().into(), e))?;
        let input_content = InputContent {
            file_type: FileType::from(path.as_ref()),
            source: InputSource::FsPath(path.into()),
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

#[cfg(test)]
mod tests {
    use super::*;

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
}
