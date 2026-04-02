/// Text Fragment Error codes
use thiserror::Error;

#[derive(Debug, Clone, Error, PartialEq, Eq)]
/// `TextDirective` check error statuses returned during the construction from
/// text directive passed in the `[url:Url]`'s fragment
pub enum TextFragmentError {
    /// Regex construction error
    #[error("Regex construction error for pattern: {0}")]
    RegexConsructionError(String),

    /// Error indicating `FragmentDirective` delimiter is missing in the
    /// `[url:Url]`'s fragment string
    #[error("Fragment Directive delimiter missing")]
    FragmentDirectiveDelimiterMissing,

    /// Not a text directive error
    #[error("Not a Text Directive")]
    NotTextDirective,

    /// When the text delimiter string format is incorrect and regex match
    /// fails to capture the directives
    #[error("Regex capture error for directive: {0} using pattern: {1}")]
    RegexCaptureError(String, String),

    /// No match is found by the text delimiter regex
    #[allow(dead_code)]
    #[error("Regex no match found error: {0}")]
    RegexNoMatchFoundError(String),

    /// Start directive is mandatory - returns this error if it is missing
    #[error("Start directive is missing error")]
    StartDirectiveMissingError,

    /// `TextDirective` is percent encoded - the error is returned if the decoding fails
    #[error("Percent decode error")]
    PercentDecodeError(String),

    /// Returns when the Text directive is not found in the content
    #[error("Text directive {0} not found")]
    TextDirectiveNotFound(String),

    /// Text directive suffix match failed error
    #[error("Suffix match error - expected {0} but matched {1}")]
    TextDirectiveRangeError(String, String),

    /// Returned when partial match is found
    #[allow(dead_code)]
    #[error("Partial text directive match found!")]
    TextDirectivePartialMatchFoundError,
}
