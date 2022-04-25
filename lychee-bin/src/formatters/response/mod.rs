use lychee_lib::{Response, Result};

pub(crate) mod color;
pub(crate) mod raw;

pub(crate) use color::Color;
pub(crate) use raw::Raw;

/// A `ResponseFormatter` knows how to format a response for different output
/// preferences based on user settings or the environment
pub(crate) trait ResponseFormatter: Send + Sync {
    /// Format a single link check response and write it to stdout
    fn write_response(&self, response: &Response) -> Result<String>;
}
