//! Input source type definitions.
//!
//! lychee can handle different kinds of input sources:
//! - URLs (of HTTP/HTTPS scheme)
//! - File system paths (to files or directories)
//! - Unix shell-style glob patterns (e.g. `./docs/**/*.md`)
//! - Standard input (`stdin`)
//! - Raw strings (UTF-8 only for now)
//!
//! Each input source is handled differently:
//! - File paths are walked (if they are directories) and filtered by
//!   extension
//! - Glob patterns are expanded to matching file paths, which are then walked
//!   and filtered by extension
//! - URLs, raw strings, and standard input (`stdin`) are read directly

use crate::ErrorKind;

use glob::Pattern;
use reqwest::Url;
use serde::{Deserialize, Deserializer, Serialize};
use std::borrow::Cow;
use std::fmt::Display;
use std::path::PathBuf;
use std::result::Result;

/// Input types which lychee supports
#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize)]
#[non_exhaustive]
pub enum InputSource {
    /// URL (of HTTP/HTTPS scheme).
    RemoteUrl(Box<Url>),
    /// Unix shell-style glob pattern.
    FsGlob {
        /// The glob pattern matching all input files
        #[serde(deserialize_with = "InputSource::deserialize_pattern")]
        pattern: Pattern,
        /// Don't be case sensitive when matching files against a glob pattern
        ignore_case: bool,
    },
    /// File path.
    FsPath(PathBuf),
    /// Standard Input.
    Stdin,
    /// Raw string input.
    String(Cow<'static, str>),
}

impl InputSource {
    const STDIN: &str = "-";

    /// Parses a [`InputSource`] from the given string. The kind of input source will be
    /// automatically detected according to certain rules and precedences.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - the input does not exist (i.e. the path is invalid)
    /// - the input cannot be parsed as a URL
    pub fn new(input: &str, glob_ignore_case: bool) -> Result<Self, ErrorKind> {
        if input == Self::STDIN {
            return Ok(InputSource::Stdin);
        }

        // We use [`reqwest::Url::parse`] because it catches some other edge cases that [`http::Request:builder`] does not
        if let Ok(url) = Url::parse(input) {
            // Weed out Windows drive letters which will be incorrectly accepted as URLs.
            match url.scheme() {
                scheme if scheme.len() > 1 => return Ok(InputSource::RemoteUrl(Box::new(url))),
                _ => {
                    // Single character scheme (likely Windows drive letter)
                    // Continue to file path handling
                }
            }
        }

        // This seems to be the only way to determine if this is a glob pattern
        let is_glob = glob::Pattern::escape(input) != input;

        if is_glob {
            return Ok(InputSource::FsGlob {
                pattern: Pattern::new(input)?,
                ignore_case: glob_ignore_case,
            });
        }

        // It might be a file path; check if it exists
        let path = PathBuf::from(input);

        // Assume all non-URL non-glob things are intended to be file paths.
        // The error message should be general enough to suggest solutions to the user.
        //
        // For missing files, we defer validation to the stream processing level where
        // --skip-missing can take effect. Domain-like inputs still get immediate
        // InvalidInput errors for better user experience.
        if path.exists() {
            // The file exists, so we return the path
            Ok(InputSource::FsPath(path))
        } else {
            Err(ErrorKind::InvalidInput(input.to_owned()))
        }
    }

    fn deserialize_pattern<'de, D>(deserializer: D) -> Result<Pattern, D::Error>
    where
        D: Deserializer<'de>,
    {
        use serde::de::Error;
        let s = String::deserialize(deserializer)?;
        Pattern::new(&s).map_err(D::Error::custom)
    }
}

/// Resolved input sources that can be processed for content.
///
/// This represents input sources after glob pattern expansion.
/// It is identical to `InputSource`, except that glob patterns
/// have been resolved to concrete file paths.
///
/// We use a separate type to avoid handling the (no longer applicable)
/// glob case in downstream processing.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum ResolvedInputSource {
    /// URL (of HTTP/HTTPS scheme).
    RemoteUrl(Box<Url>),
    /// File path.
    FsPath(PathBuf),
    /// Standard Input.
    Stdin,
    /// Raw string input.
    String(Cow<'static, str>),
}

impl From<ResolvedInputSource> for InputSource {
    fn from(resolved: ResolvedInputSource) -> Self {
        match resolved {
            ResolvedInputSource::RemoteUrl(url) => InputSource::RemoteUrl(url),
            ResolvedInputSource::FsPath(path) => InputSource::FsPath(path),
            ResolvedInputSource::Stdin => InputSource::Stdin,
            ResolvedInputSource::String(s) => InputSource::String(s),
        }
    }
}

impl Display for ResolvedInputSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::RemoteUrl(url) => url.as_str(),
            Self::FsPath(path) => path.to_str().unwrap_or_default(),
            Self::Stdin => "stdin",
            Self::String(s) => s.as_ref(),
        })
    }
}

/// Custom serialization for the `InputSource` enum.
///
/// This implementation serializes all variants as strings to ensure
/// compatibility with JSON serialization, which requires string keys for enums.
///
/// Without this custom implementation, attempting to serialize `InputSource` to
/// JSON would result in a "key must be a string" error.
///
/// See: <https://github.com/serde-rs/json/issues/45>
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
            Self::FsGlob { pattern, .. } => pattern.as_str(),
            Self::FsPath(path) => path.to_str().unwrap_or_default(),
            Self::Stdin => "stdin",
            Self::String(s) => s.as_ref(),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Serialization of `FsGlob` relies on [`glob::Pattern::to_string`].
    /// Here, we check that the `to_string` works as we require.
    #[test]
    fn test_pattern_serialization_is_original_pattern() {
        let pat = "asd[f]*";
        assert_eq!(
            serde_json::to_string(&InputSource::FsGlob {
                pattern: Pattern::new(pat).unwrap(),
                ignore_case: false,
            })
            .unwrap(),
            serde_json::to_string(pat).unwrap(),
        );
    }
}
