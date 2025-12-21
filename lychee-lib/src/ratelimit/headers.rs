//! Handle rate limiting headers.
//! Note that we might want to replace this module with
//! <https://github.com/mre/rate-limits> at some point in the future.

use http::HeaderValue;
use std::time::{Duration, SystemTime};
use thiserror::Error;

#[derive(Debug, Error, PartialEq, Eq)]
pub(crate) enum RetryAfterParseError {
    #[error("Unable to parse value '{0}'")]
    ValueError(String),

    #[error("Header value contains invalid chars")]
    HeaderValueError,
}

/// Parse the "Retry-After" header as specified per
/// [RFC 7231 section 7.1.3](https://www.rfc-editor.org/rfc/rfc7231#section-7.1.3)
pub(crate) fn parse_retry_after(value: &HeaderValue) -> Result<Duration, RetryAfterParseError> {
    let value = value
        .to_str()
        .map_err(|_| RetryAfterParseError::HeaderValueError)?;

    // RFC 7231: Retry-After = HTTP-date / delay-seconds
    value.parse::<u64>().map(Duration::from_secs).or_else(|_| {
        httpdate::parse_http_date(value)
            .map(|s| {
                s.duration_since(SystemTime::now())
                    // if date is in the past, we can use ZERO
                    .unwrap_or(Duration::ZERO)
            })
            .map_err(|_| RetryAfterParseError::ValueError(value.into()))
    })
}

/// Parse the common "X-RateLimit" header fields.
/// Unfortunately, this is not standardised yet, but there is an
/// [IETF draft](https://datatracker.ietf.org/doc/draft-ietf-httpapi-ratelimit-headers/).
pub(crate) fn parse_common_rate_limit_header_fields(
    headers: &http::HeaderMap,
) -> (Option<usize>, Option<usize>) {
    let remaining = self::parse_header_value(
        headers,
        &[
            "x-ratelimit-remaining",
            "x-rate-limit-remaining",
            "ratelimit-remaining",
        ],
    );

    let limit = self::parse_header_value(
        headers,
        &["x-ratelimit-limit", "x-rate-limit-limit", "ratelimit-limit"],
    );

    (remaining, limit)
}

/// Helper method to parse numeric header values from common rate limit headers
fn parse_header_value(headers: &http::HeaderMap, header_names: &[&str]) -> Option<usize> {
    for header_name in header_names {
        if let Some(value) = headers.get(*header_name)
            && let Ok(value_str) = value.to_str()
            && let Ok(number) = value_str.parse::<usize>()
        {
            return Some(number);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use http::HeaderValue;

    use crate::ratelimit::headers::{RetryAfterParseError, parse_retry_after};

    #[test]
    fn test_retry_after() {
        assert_eq!(parse_retry_after(&value("1")), Ok(Duration::from_secs(1)));
        assert_eq!(
            parse_retry_after(&value("-1")),
            Err(RetryAfterParseError::ValueError("-1".into()))
        );

        assert_eq!(
            parse_retry_after(&value("Fri, 15 May 2015 15:34:21 GMT")),
            Ok(Duration::ZERO)
        );

        let result = parse_retry_after(&value("Fri, 15 May 4099 15:34:21 GMT"));
        let is_in_future = matches!(result, Ok(d) if d.as_secs() > 0);
        assert!(is_in_future);
    }

    fn value(v: &str) -> HeaderValue {
        HeaderValue::from_str(v).unwrap()
    }
}
