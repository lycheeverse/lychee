/// Text Fragment Error codes
use thiserror::Error;

#[derive(Debug, Clone, Error, PartialEq, Eq)]
pub enum TextFragmentError {
    #[error("Regex construction error for pattern: {0}")]
    RegexConsructionError(String),

    #[error("Fragment Directive delimiter missing")]
    FragmentDirectiveDelimiterMissing,

    #[error("Not a Text Directive")]
    NotTextDirective,

    #[error("Regex capture error for directive: {0} using pattern: {1}")]
    RegexCaptureError(String, String),

    #[error("Regex no match found error: {0}")]
    RegexNoMatchFoundError(String),

    #[error("Start directive is missing error")]
    StartDirectiveMissingError,

    #[error("Percent decode error")]
    PercentDecodeError(String),

    #[error("Text directive {0} not found")]
    TextDirectiveNotFound(String),

    #[error("Suffix match error - expected {0} but matched {1}")]
    TextDirectiveRangeError(String, String),

    #[error("Partial text directive match found!")]
    TextDirectivePartialMatchFoundError,
}
