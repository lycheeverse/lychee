use lychee_lib::{CacheStatus, ResponseBody, Status};

use super::ResponseBodyFormatter;

/// An emoji formatter for the response body
///
/// This formatter replaces certain textual elements with emojis for a more
/// visual output.
pub(crate) struct EmojiFormatter;

impl ResponseBodyFormatter for EmojiFormatter {
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
