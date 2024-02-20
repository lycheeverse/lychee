use http::StatusCode;
use lychee_lib::{CacheStatus, ResponseBody, Status};

use crate::color::{DIM, GREEN, NORMAL, PINK, YELLOW};

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
/// Under the hood, it calls the `Display` implementation of the `ResponseBody`
/// type.
///
/// This formatter is used when the user has requested raw output
/// or when the terminal does not support color.
pub(crate) struct PlainFormatter;

impl ResponseBodyFormatter for PlainFormatter {
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
        match body.status {
            Status::Ok(_) | Status::Cached(CacheStatus::Ok(_)) => {
                let status = format!("[{}]", body.status.code_as_string());
                let mut output = format!("{} {}", GREEN.apply_to(status), body.uri);

                if let Status::Ok(StatusCode::OK) = body.status {
                    // Don't print anything else if the status code is 200.
                    // The output gets too verbose then.
                    return output;
                }

                // Add a separator between the URI and the additional details below.
                // Note: To make the links clickable in some terminals,
                // we add a space before the separator.
                output.push_str(&format!(" | {}", body.status));

                if let Some(details) = body.status.details() {
                    // write!(f, ": {details}")
                    output.push_str(&format!(": {}", details))
                }
                output
            }
            Status::Excluded
            | Status::Unsupported(_)
            | Status::Cached(CacheStatus::Excluded | CacheStatus::Unsupported) => {
                DIM.apply_to(body).to_string()
            }
            Status::Redirected(_) => NORMAL.apply_to(body).to_string(),
            Status::UnknownStatusCode(_) | Status::Timeout(_) => YELLOW.apply_to(body).to_string(),
            Status::Error(_) | Status::Cached(CacheStatus::Error(_)) => {
                PINK.apply_to(body).to_string()
            }
        }
    }
}

/// An emoji formatter for the response body
///
/// This formatter replaces certain textual elements with emojis for a more
/// visual output.
pub(crate) struct FancyFormatter;

impl ResponseBodyFormatter for FancyFormatter {
    fn format_response(&self, body: &ResponseBody) -> String {
        let emoji = match body.status {
            Status::Ok(_) | Status::Cached(CacheStatus::Ok(_)) => "✅",
            Status::Excluded
            | Status::Unsupported(_)
            | Status::Cached(CacheStatus::Excluded | CacheStatus::Unsupported) => "🚫",
            Status::Redirected(_) => "↪️",
            Status::UnknownStatusCode(_) | Status::Timeout(_) => "⚠️",
            Status::Error(_) | Status::Cached(CacheStatus::Error(_)) => "❌",
        };
        format!("{} {}", emoji, body.uri)
    }
}
