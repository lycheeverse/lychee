use crate::types::FileType;
use crate::Result;
use glob::glob_with;
use reqwest::Url;
use serde::Serialize;
use shellexpand::tilde;
use std::path::{Path, PathBuf};
use std::{fmt::Display, fs::read_to_string};
use tokio::io::{stdin, AsyncReadExt};

const STDIN: &str = "-";
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[non_exhaustive]
/// An exhaustive list of input sources, which lychee accepts
pub enum Input {
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

impl Serialize for Input {
    fn serialize<S>(&self, serializer: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        serializer.collect_str(self)
    }
}

impl Display for Input {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Input::RemoteUrl(url) => url.as_str(),
            Input::FsGlob { pattern, .. } => pattern,
            Input::FsPath(path) => path.to_str().unwrap_or_default(),
            Input::Stdin => "stdin",
            Input::String(_) => "raw input string",
        })
    }
}

#[derive(Debug)]
/// Encapsulates the content for a given input
pub struct InputContent {
    /// Input source
    pub input: Input,
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
            input: Input::String(s.to_owned()),
            file_type,
            content: s.to_owned(),
        }
    }
}

impl Input {
    #[must_use]
    /// Construct a new `Input` source. In case the input is a `glob` pattern,
    /// `glob_ignore_case` decides whether matching files against the `glob` is
    /// case-insensitive or not
    pub fn new(value: &str, glob_ignore_case: bool) -> Self {
        if value == STDIN {
            Self::Stdin
        } else if let Ok(url) = Url::parse(&value) {
            Self::RemoteUrl(Box::new(url))
        } else {
            // this seems to be the only way to determine if this is a glob pattern
            let is_glob = glob::Pattern::escape(value) != value;

            if is_glob {
                Self::FsGlob {
                    pattern: value.to_owned(),
                    ignore_case: glob_ignore_case,
                }
            } else {
                Self::FsPath(value.into())
            }
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
        &self,
        file_type_hint: Option<FileType>,
        skip_missing: bool,
    ) -> Result<Vec<InputContent>> {
        match *self {
            // TODO: should skip_missing also affect URLs?
            Input::RemoteUrl(ref url) => Ok(vec![Self::url_contents(url).await?]),
            Input::FsGlob {
                ref pattern,
                ignore_case,
            } => Ok(Self::glob_contents(pattern, ignore_case).await?),
            Input::FsPath(ref path) => {
                let content = Self::path_content(path);
                match content {
                    Ok(input_content) => Ok(vec![input_content]),
                    Err(_) if skip_missing => Ok(vec![]),
                    Err(e) => Err(e),
                }
            }
            Input::Stdin => Ok(vec![Self::stdin_content(file_type_hint).await?]),
            Input::String(ref s) => Ok(vec![Self::string_content(s, file_type_hint)]),
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
            input: Input::RemoteUrl(Box::new(url.clone())),
            file_type,
            content: res.text().await?,
        };

        Ok(input_content)
    }

    async fn glob_contents(path_glob: &str, ignore_case: bool) -> Result<Vec<InputContent>> {
        let mut contents = vec![];
        let glob_expanded = tilde(&path_glob);
        let mut match_opts = glob::MatchOptions::new();

        match_opts.case_sensitive = !ignore_case;

        for entry in glob_with(&glob_expanded, match_opts)? {
            match entry {
                Ok(path) => {
                    let content = Self::path_content(&path)?;
                    contents.push(content);
                }
                Err(e) => println!("{:?}", e),
            }
        }

        Ok(contents)
    }

    /// Get the input content of a given path
    pub fn path_content<P: Into<PathBuf> + AsRef<Path> + Clone>(path: P) -> Result<InputContent> {
        let content = read_to_string(&path).map_err(|e| (path.clone().into(), e))?;
        let input_content = InputContent {
            file_type: FileType::from(path.as_ref()),
            content,
            input: Input::FsPath(path.into()),
        };

        Ok(input_content)
    }

    async fn stdin_content(file_type_hint: Option<FileType>) -> Result<InputContent> {
        let mut content = String::new();
        let mut stdin = stdin();
        stdin.read_to_string(&mut content).await?;

        let input_content = InputContent {
            input: Input::Stdin,
            file_type: file_type_hint.unwrap_or_default(),
            content,
        };

        Ok(input_content)
    }

    fn string_content(s: &str, file_type_hint: Option<FileType>) -> InputContent {
        InputContent::from_string(s, file_type_hint.unwrap_or_default())
    }
}
