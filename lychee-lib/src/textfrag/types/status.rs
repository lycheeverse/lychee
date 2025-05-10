/// Defines the status of the Text Fragment search and extraction/search operation status
use std::{
    collections::HashMap,
    fmt::{Display, Formatter, Result},
};

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
/// Defines the `FragmentDirective` check status
pub enum FragmentDirectiveStatus {
    /// Text Fragment search was successful for all directives
    Ok,
}

impl Display for FragmentDirectiveStatus {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        match self {
            FragmentDirectiveStatus::Ok => write!(f, "Ok"),
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
/// `FragmentDirective` check error status
pub enum FragmentDirectiveError {
    /// Text Fragment search found one or more directives and so was partially successful
    /// - check individual text directive status for more details
    PartialOk(HashMap<String, TextDirectiveStatus>),
    /// Failed to find the `TextDirective`s
    NotFoundError,
    /// Error processing `FragmentDirective` in the `[url:Url]`'s fragment string
    DirectiveProcessingError,
}

impl Display for FragmentDirectiveError {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        match self {
            FragmentDirectiveError::PartialOk(m) => write!(f, "Partial Ok {m:?}"),
            FragmentDirectiveError::NotFoundError => write!(f, "Directives not found Error"),
            FragmentDirectiveError::DirectiveProcessingError => write!(
                f,
                "Error processing the fragment directive in fragment string"
            ),
        }
    }
}

/// Text Directive check status
#[derive(PartialEq, Clone, Copy, Eq, Debug, Default)]
pub enum TextDirectiveStatus {
    /// Text Directive check Not started
    #[default]
    NotStarted,
    /// Text Directive is found in the content
    /// and return start offset and end index of the search string
    Found((usize, usize)),
    /// Text directive Not Found in the content
    NotFound,
    /// Word distance breached - returned when the allowed word distance
    /// from the start offset is exceeded when finding the word in the
    /// block element's content and return the offset in the content
    WordDistanceExceeded(usize),
    /// End of content - this status indicates the searc
    /// next block element
    EndOfContent,
    /// Completed directive checks successfully
    Completed,
}

impl Display for TextDirectiveStatus {
    fn fmt(&self, f: &mut Formatter<'_>) -> Result {
        match self {
            TextDirectiveStatus::NotStarted => write!(f, "Not Started"),
            TextDirectiveStatus::Found((start, end)) => write!(f, "Found: ({start}, {end})"), // start, end),
            TextDirectiveStatus::NotFound => write!(f, "Not Found"),
            TextDirectiveStatus::EndOfContent => write!(f, "End of Content"),
            TextDirectiveStatus::Completed => write!(f, "Completed"),
            TextDirectiveStatus::WordDistanceExceeded(offset) => {
                write!(f, "Word distance exceeded at: {offset}") //, offset)
            }
        }
    }
}
