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
#[cfg(windows)]
use std::path::Path;
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
    /// # Validation Strategy
    ///
    /// This function uses two validation approaches:
    ///
    /// Immediate validation for explicit, unambiguous file paths, including:
    /// - Absolute paths (`/foo/bar`, `C:\foo`)
    /// - Explicit relative paths (`./`, `../`, `~`)
    ///
    /// The goal is to catch typos and invalid paths early to provide immediate
    /// feedback. However, this is not always possible due to ambiguities in input formats.
    /// That's why we also have deferred validation for ambiguous cases.
    ///
    /// Deferred validation for ambiguous inputs, including:
    /// - Hidden files (`.gitignore`)
    /// - Relative paths without explicit notation (`path/to/file`)
    /// - Inputs that might be file paths (`some-file`)
    ///
    /// This allows the `--skip-missing` flag to work correctly by deferring
    /// existence checks until processing time when the flag can be consulted.
    ///
    /// If the file is indeed missing, but `--skip-missing` is set, the error
    /// will be ignored. However, if the file is missing and `--skip-missing` is
    /// not set, an error will be raised at that time. This is less ideal than
    /// immediate validation, but necessary due to the mentioned ambiguity.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - An explicit file path doesn't exist (immediate validation)
    /// - The input looks like a domain or word without a URL scheme (e.g., `example.com`)
    /// - The input is a URL with an unsupported scheme (only `http://` and `https://` are supported)
    /// - The glob pattern syntax is invalid
    pub fn new(input: &str, glob_ignore_case: bool) -> Result<Self, ErrorKind> {
        if input == Self::STDIN {
            return Ok(InputSource::Stdin);
        }

        // Detect drive-letter paths with `Path::is_absolute()` This handles
        // Windows absolute paths (e.g., C:\path) before URL parsing
        //
        // Drive letters can be mistaken for URL schemes, so we need to check
        // this first. This is only necessary on Windows, as Unix absolute paths
        // always start with `/`, which cannot be confused with URLs.
        #[cfg(windows)]
        {
            let path = Path::new(input);
            if path.is_absolute() {
                return if path.exists() {
                    Ok(InputSource::FsPath(path.to_path_buf()))
                } else {
                    Err(ErrorKind::InvalidFile(path.to_path_buf()))
                };
            }
        }

        // We use [`reqwest::Url::parse`] because it catches some other edge
        // cases that [`http::Request:builder`] does not
        if let Ok(url) = Url::parse(input) {
            // Only accept HTTP and HTTPS URLs
            return match url.scheme() {
                "http" | "https" => Ok(InputSource::RemoteUrl(Box::new(url))),
                _ => Err(ErrorKind::InvalidFile(PathBuf::from(input))),
            };
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

        // On Windows, a filepath can never be mistaken for a
        // URL, because Windows filepaths use `\` and URLs use
        // `/`
        #[cfg(windows)]
        if path.exists() {
            // The file exists, so we return the path
            Ok(InputSource::FsPath(path))
        } else if input.contains('\\') || input.contains('/') || input.starts_with('.') {
            // These look like file paths, parse as path and let skip_missing
            // handle them later
            Ok(InputSource::FsPath(path))
        } else if input.contains('.') || input.chars().all(|c| c.is_ascii_alphabetic()) {
            // Looks like it could be a domain name or simple word without scheme
            Err(ErrorKind::InvalidInput(format!(
                "Input '{input}' not found as file and not a valid URL. \
                     Use full URL (e.g., https://example.com) or check file path."
            )))
        } else {
            // Treat as potential file path, parse as path and let skip_missing
            // handle it later
            Ok(InputSource::FsPath(path))
        }

        #[cfg(unix)]
        if path.exists() {
            Ok(InputSource::FsPath(path))
        } else if path.is_absolute() {
            // Absolute paths (e.g., `/foo/bar`) are unambiguous file paths.
            // Validate immediately to provide early feedback for typos, consistent
            // with how Windows handles absolute paths.
            Err(ErrorKind::InvalidFile(path))
        } else if input.starts_with('~') || input.starts_with("./") || input.starts_with("../") {
            // Paths using explicit relative path notation (~, ./, ../) are unambiguously
            // file paths and should be validated immediately to catch typos early.
            //
            // Immediate validation means these will error during input parsing, before
            // the --skip-missing flag can be checked. This is intentional because:
            //
            // 1. These syntaxes are explicit file path notation that cannot be URLs
            // 2. Users benefit from immediate feedback when they mistype a path
            // 3. The --skip-missing flag is meant for "discovered" files (e.g.,
            //    through glob expansion) rather than explicitly specified paths
            //
            // Examples of immediate validation:
            // - `/absolute/path/file.txt` → absolute path
            // - `./documents/readme.md` → explicit current dir
            // - `../parent/file.txt` → explicit parent dir
            // - `~/config/settings.yaml` → explicit home dir
            Err(ErrorKind::InvalidFile(path))
        } else if input.starts_with('.') {
            // Starts with `.` but not `./` or `../`, which means this could be
            // a hidden file (e.g., `.gitignore`). Since we're unsure, treat as
            // a file path and defer validation to respect --skip-missing.
            //
            // Examples of deferred validation:
            // - `.hidden` → hidden file, not relative path notation
            // - `some-file` → ambiguous input
            // - `path/to/file` → this looks like path but not explicit (i.e.
            //   not `./path/to/file`, so there's still some ambiguity)
            Ok(InputSource::FsPath(path))
        } else if input.contains('/') {
            // Contains a slash but doesn't use explicit relative path notation.
            // Looks like a file path, parse as path and let skip_missing handle
            // validation later. Example: `path/to/file`
            Ok(InputSource::FsPath(path))
        } else if input.contains('.') || input.chars().all(|c| c.is_ascii_alphabetic()) {
            // Looks like it could be a domain name or simple word without scheme
            Err(ErrorKind::InvalidInput(format!(
                "Input '{input}' not found as file and not a valid URL. \
                     Use full URL (e.g., https://example.com) or check file path."
            )))
        } else {
            // Treat as potential file path, parse as path and let skip_missing handle it later
            Ok(InputSource::FsPath(path))
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
