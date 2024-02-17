use std::io::Write;

use futures::io::BufWriter;
use http::StatusCode;
use lychee_lib::{CacheStatus, ResponseBody, Status};

use crate::color::{DIM, GREEN, NORMAL, PINK, YELLOW};

/// A `ResponseFormatter` knows how to format a response for different output
/// preferences based on user settings or the environment
// pub(crate) trait ResponseFormatter: Send + Sync {
//     /// Format a single link check response and write it to stdout
//     fn write_response(&self, response: &Response) -> Result<String>;
// }

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
        format!("{}", body)
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
                // GREEN.apply_to(body),

                // Format the status code in green
                // and the response body in normal text
                // format!("{}{}", GREEN.apply_to(&body.status), NORMAL.apply_to(body))
                let status = format!("{} [{}]", body.status.icon(), body.status.code_as_string(),);

                let mut output = format!("{} {}", GREEN.apply_to(status), body.uri);

                if let Status::Ok(StatusCode::OK) = body.status {
                    // Don't print anything else if the status code is 200.
                    // The output gets too verbose then.
                    return output;
                }

                // Add a separator between the URI and the additional details below.
                // Note: To make the links clickable in some terminals,
                // we add a space before the separator.
                // write!(f, " | {}", self.status)?;
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
