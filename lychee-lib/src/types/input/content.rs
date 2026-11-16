//! Input content representation and construction.
//!
//! The `InputContent` type represents the actual content extracted from various
//! input sources, along with metadata about the source and file type.

use super::source::ResolvedInputSource;
use crate::types::FileType;
use std::borrow::Cow;

/// Encapsulates the content for a given input
#[derive(Debug)]
pub struct InputContent {
    /// Input source used for resolving relative links in `content`.
    ///
    /// For remote inputs this is the *final* URL after any redirects, since
    /// relative links must resolve against the URL the content actually came
    /// from. See <https://github.com/lycheeverse/lychee/issues/2018>.
    pub source: ResolvedInputSource,
    /// Input source as originally requested by the user, used for reporting.
    ///
    /// This differs from [`Self::source`] only for remote inputs which were
    /// redirected. Keeping the original here means the URL shown to the user is
    /// the one they actually asked for, not the redirect target (which may
    /// contain opaque or temporary query parameters).
    pub original_source: Option<ResolvedInputSource>,
    /// File type of given input
    pub file_type: FileType,
    /// Raw UTF-8 string content
    pub content: String,
}

impl InputContent {
    #[must_use]
    /// Create an instance of `InputContent` from an input string
    pub fn from_string(s: &str, file_type: FileType) -> Self {
        Self {
            source: ResolvedInputSource::String(Cow::Owned(s.to_owned())),
            original_source: None,
            file_type,
            content: s.to_owned(),
        }
    }

    /// Create an instance of `InputContent` from an input string
    #[must_use]
    pub fn from_str<S: Into<Cow<'static, str>>>(s: S, file_type: FileType) -> Self {
        let cow = s.into();
        Self {
            source: ResolvedInputSource::String(cow.clone()),
            original_source: None,
            file_type,
            content: cow.into_owned(),
        }
    }
}
