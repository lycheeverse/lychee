use lychee_lib::{CacheStatus, Response, ResponseBody, Result, Status};

use crate::color::{DIM, GREEN, NORMAL, PINK, YELLOW};

/// A `ResponseFormatter` knows how to format a response for different output
/// preferences based on user settings or the environment
pub(crate) trait ResponseFormatter: Send + Sync {
    /// Format a single link check response and write it to stdout
    fn write_response(&self, response: &Response) -> Result<String>;
}

/// A trait for formatting a response body
///
/// This trait is used to format a response body into a string.
/// It can be implemented for different formatting styles such as
/// colorized output or plain text.
pub(crate) trait ResponseBodyFormatter: Send + Sync {
    fn format_response(&self, body: &ResponseBody) -> String;
}

/// A basic formatter that just returns the response body as a string
/// without any color codes or other formatting.
///
/// This formatter is used when the user has requested raw output
/// or when the terminal does not support color.
pub(crate) struct BasicFormatter;

impl ResponseBodyFormatter for BasicFormatter {
    fn format_response(&self, body: &ResponseBody) -> String {
        body.to_string()
    }
}

/// A colorized formatter for the response body
///
/// This formatter is used when the terminal supports color and the user
/// has not explicitly requested raw, uncolored output.
pub(crate) struct ColorFormatter;

impl ResponseBodyFormatter for ColorFormatter {
    fn format_response(&self, body: &ResponseBody) -> String {
        let out = match body.status {
            Status::Ok(_) | Status::Cached(CacheStatus::Ok(_)) => GREEN.apply_to(body),
            Status::Excluded
            | Status::Unsupported(_)
            | Status::Cached(CacheStatus::Excluded | CacheStatus::Unsupported) => {
                DIM.apply_to(body)
            }
            Status::Redirected(_) => NORMAL.apply_to(body),
            Status::UnknownStatusCode(_) | Status::Timeout(_) => YELLOW.apply_to(body),
            Status::Error(_) | Status::Cached(CacheStatus::Error(_)) => PINK.apply_to(body),
        };
        out.to_string()
    }
}
