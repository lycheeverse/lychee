pub(crate) mod duration;
pub(crate) mod response;
pub(crate) mod stats;

use lychee_lib::{CacheStatus, ResponseBody, Status};
use supports_color::Stream;

use crate::{
    color::{DIM, GREEN, NORMAL, PINK, YELLOW},
    options::{self, Format},
};

use self::response::ResponseFormatter;

/// Detects whether a terminal supports color, and gives details about that
/// support. It takes into account the `NO_COLOR` environment variable.
fn supports_color() -> bool {
    supports_color::on(Stream::Stdout).is_some()
}

/// Color the response body for TTYs that support it
pub(crate) fn color_response(body: &ResponseBody) -> String {
    if supports_color() {
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
    } else {
        body.to_string()
    }
}

/// Create a response formatter based on the given format option
pub(crate) fn get_formatter(format: &options::Format) -> Box<dyn ResponseFormatter> {
    if matches!(format, Format::Raw) || !supports_color() {
        return Box::new(response::Raw::new());
    }
    Box::new(response::Color::new())
}
