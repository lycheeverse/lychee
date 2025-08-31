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

use reqwest::Url;
use serde::{Deserialize, Serialize};
use std::fmt::Display;
use std::path::PathBuf;

/// Input types which lychee supports
#[derive(Debug, Clone, PartialEq, Eq, Hash, Deserialize)]
#[non_exhaustive]
pub enum InputSource {
    /// URL (of HTTP/HTTPS scheme).
    RemoteUrl(Box<Url>),
    /// Unix shell-style glob pattern.
    FsGlob {
        /// The glob pattern matching all input files
        pattern: String,
        /// Don't be case sensitive when matching files against a glob pattern
        ignore_case: bool,
    },
    /// File path.
    FsPath(PathBuf),
    /// Standard Input.
    Stdin,
    /// Raw string input.
    String(String),
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
    /// File path, possibly resolved from a [`InputSource::FsGlob`].
    ///
    /// The [`Option`] field, if specified, records the `pattern` and
    /// `ignore_case` of the [`InputSource::FsGlob`] which resolved
    /// to this `FsPath`. If not specified, this `FsPath` was previously
    /// a [`InputSource::FsPath`].
    FsPath(PathBuf, Option<(String, bool)>),
    /// Standard Input.
    Stdin,
    /// Raw string input.
    String(String),
}

impl From<ResolvedInputSource> for InputSource {
    fn from(resolved: ResolvedInputSource) -> Self {
        match resolved {
            ResolvedInputSource::RemoteUrl(url) => InputSource::RemoteUrl(url),
            ResolvedInputSource::FsPath(path, None) => InputSource::FsPath(path),
            ResolvedInputSource::FsPath(_path, Some((pattern, ignore_case))) => {
                InputSource::FsGlob {
                    pattern,
                    ignore_case,
                }
            }
            ResolvedInputSource::Stdin => InputSource::Stdin,
            ResolvedInputSource::String(s) => InputSource::String(s),
        }
    }
}

impl Display for ResolvedInputSource {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::RemoteUrl(url) => url.as_str(),
            Self::FsPath(path, _) => path.to_str().unwrap_or_default(),
            Self::Stdin => "stdin",
            Self::String(s) => s,
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
            Self::FsGlob { pattern, .. } => pattern,
            Self::FsPath(path) => path.to_str().unwrap_or_default(),
            Self::Stdin => "stdin",
            Self::String(s) => s,
        })
    }
}
