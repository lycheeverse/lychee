use std::{error::Error, io};

use http::StatusCode;

use crate::{ErrorKind, Status};

/// Returns whether an error can be retried using the HTTP/1.1 fallback client.
pub(crate) fn requires_http1_fallback(error: &ErrorKind) -> bool {
    let (ErrorKind::NetworkRequest(error) | ErrorKind::ReadResponseBody(error)) = error else {
        return false;
    };

    is_http2_protocol_error(error)
}

fn is_http2_protocol_error(error: &(dyn Error + 'static)) -> bool {
    find_error::<h2::Error>(error)
        .and_then(h2::Error::reason)
        .and_then(http1_fallback_for_reason)
        .unwrap_or(false)
}

/// Classifies every standard HTTP/2 reason known to `h2`.
///
/// Unknown extension codes return `None` and do not trigger a protocol downgrade.
#[allow(clippy::match_same_arms)]
const fn http1_fallback_for_reason(reason: h2::Reason) -> Option<bool> {
    match reason {
        // Generic protocol or implementation failures can come from a broken peer or intermediary.
        h2::Reason::PROTOCOL_ERROR | h2::Reason::INTERNAL_ERROR => Some(true),
        // These mechanisms do not exist in HTTP/1.1: flow control, settings, streams, framing, and
        // HPACK compression respectively.
        h2::Reason::FLOW_CONTROL_ERROR
        | h2::Reason::SETTINGS_TIMEOUT
        | h2::Reason::STREAM_CLOSED
        | h2::Reason::FRAME_SIZE_ERROR
        | h2::Reason::COMPRESSION_ERROR => Some(true),
        // HTTP/1.1 can use TLS parameters that HTTP/2 rejects. See RFC 9113 section 9.2.2:
        // <https://www.rfc-editor.org/rfc/rfc9113.html#section-9.2.2>.
        h2::Reason::INADEQUATE_SECURITY => Some(true),
        // The peer explicitly requires HTTP/1.1.
        h2::Reason::HTTP_1_1_REQUIRED => Some(true),
        // Graceful shutdown is not an error.
        h2::Reason::NO_ERROR => Some(false),
        // Safe to retry, but not evidence of an HTTP/2 incompatibility.
        h2::Reason::REFUSED_STREAM => Some(false),
        // The stream is no longer needed.
        h2::Reason::CANCEL => Some(false),
        // Specific to a failed CONNECT tunnel, which HTTP/1.1 does not fix.
        h2::Reason::CONNECT_ERROR => Some(false),
        // The peer is signaling excessive load; changing protocols should not bypass that signal.
        h2::Reason::ENHANCE_YOUR_CALM => Some(false),
        _ => None,
    }
}

/// An extension trait to help determine if a given HTTP request
/// is retryable.
///
/// Modified from `Retryable` in [reqwest-middleware].
/// We vendor this code to avoid a dependency on `reqwest-middleware` and
/// to easily customize the logic.
///
/// [reqwest-middleware]: https://github.com/TrueLayer/reqwest-middleware/blob/f854725791ccf4a02c401a26cab3d9db753f468c/reqwest-retry/src/retryable.rs
pub(crate) trait RetryExt {
    fn should_retry(&self) -> bool;
}

impl RetryExt for reqwest::StatusCode {
    /// Try to map a `reqwest` response into `Retryable`.
    fn should_retry(&self) -> bool {
        self.is_server_error()
            || self == &StatusCode::REQUEST_TIMEOUT
            || self == &StatusCode::TOO_MANY_REQUESTS
    }
}

impl RetryExt for reqwest::Error {
    #[allow(clippy::if_same_then_else)]
    fn should_retry(&self) -> bool {
        if self.is_timeout() {
            true
        } else if self.is_connect() {
            false
        } else if self.is_body() || self.is_decode() || self.is_builder() || self.is_redirect() {
            false
        } else if self.is_request() {
            // It seems that hyper::Error(IncompleteMessage) is not correctly handled by reqwest.
            // Here we check if the Reqwest error was originated by hyper and map it consistently.
            if let Some(hyper_error) = find_error::<hyper::Error>(self) {
                // The hyper::Error(IncompleteMessage) is raised if the HTTP
                // response is well formatted but does not contain all the
                // bytes. This can happen when the server has started sending
                // back the response but the connection is cut halfway through.
                // We can safely retry the call, hence marking this error as
                // transient.
                //
                // Instead hyper::Error(Canceled) is raised when the connection is
                // gracefully closed on the server side.
                if hyper_error.is_incomplete_message() || hyper_error.is_canceled() {
                    true

                // Try and downcast the hyper error to [`io::Error`] if that is the
                // underlying error, and try and classify it.
                } else if let Some(io_error) = find_error::<io::Error>(hyper_error) {
                    should_retry_io(io_error)
                } else {
                    false
                }
            } else {
                false
            }
        } else if let Some(status) = self.status() {
            status.should_retry()
        } else {
            // We omit checking if error.is_status() since we check that already.
            // However, if Response::error_for_status is used the status will still
            // remain in the response object.
            false
        }
    }
}

impl RetryExt for http::Error {
    fn should_retry(&self) -> bool {
        let inner = self.get_ref();
        inner
            .source()
            .and_then(<dyn std::error::Error + 'static>::downcast_ref)
            .is_some_and(should_retry_io)
    }
}

impl RetryExt for ErrorKind {
    fn should_retry(&self) -> bool {
        // If the error is a `reqwest::Error`, delegate to that
        if let Some(r) = self.reqwest_error() {
            r.should_retry()
        // GitHub errors sometimes wrap `reqwest` errors.
        // In that case, delegate to the underlying error.
        } else if let Some(octocrab::Error::Http {
            source,
            backtrace: _,
        }) = self.github_error()
        {
            source.should_retry()
        } else {
            matches!(
                self,
                Self::RejectedStatusCode(StatusCode::TOO_MANY_REQUESTS)
            )
        }
    }
}

impl RetryExt for Status {
    fn should_retry(&self) -> bool {
        match self {
            Status::Timeout(_) => true,
            Status::Error(err) => err.should_retry(),
            Status::Ok(_)
            | Status::RequestError(_)
            | Status::UnknownStatusCode(_)
            | Status::UnknownMailStatus(_)
            | Status::Excluded
            | Status::Unsupported(_)
            | Status::Cached(_) => false,
        }
    }
}

/// Classifies an `io::Error` into retryable or not.
fn should_retry_io(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::ConnectionReset | io::ErrorKind::ConnectionAborted | io::ErrorKind::TimedOut
    )
}

/// Finds an error of type `T` in an error chain.
fn find_error<'a, T: Error + 'static>(error: &'a (dyn Error + 'static)) -> Option<&'a T> {
    let mut current = Some(error);

    while let Some(error) = current {
        if let Some(error) = error.downcast_ref::<T>() {
            return Some(error);
        }
        current = error.source();
    }

    None
}

#[cfg(test)]
mod tests {
    use http::StatusCode;

    use super::{RetryExt, http1_fallback_for_reason, is_http2_protocol_error};

    #[test]
    fn classifies_every_reason_known_to_h2() {
        // `Reason` is an open integer type without an iterator. Standard codes are allocated from
        // the low end of the registry, so scan the first byte for newly recognized codes.
        for code in 0..=u8::MAX {
            let reason = h2::Reason::from(u32::from(code));
            if reason.description() != "unknown reason" {
                assert!(
                    http1_fallback_for_reason(reason).is_some(),
                    "unclassified HTTP/2 reason: {reason:?}"
                );
            }
        }
    }

    #[test]
    fn identifies_http2_protocol_errors() {
        for reason in [
            h2::Reason::PROTOCOL_ERROR,
            h2::Reason::INTERNAL_ERROR,
            h2::Reason::FLOW_CONTROL_ERROR,
            h2::Reason::SETTINGS_TIMEOUT,
            h2::Reason::STREAM_CLOSED,
            h2::Reason::FRAME_SIZE_ERROR,
            h2::Reason::COMPRESSION_ERROR,
            h2::Reason::INADEQUATE_SECURITY,
            h2::Reason::HTTP_1_1_REQUIRED,
        ] {
            assert!(is_http2_protocol_error(&h2::Error::from(reason)));
        }
    }

    #[test]
    fn ignores_other_http2_and_network_errors() {
        for reason in [
            h2::Reason::NO_ERROR,
            h2::Reason::REFUSED_STREAM,
            h2::Reason::CANCEL,
            h2::Reason::CONNECT_ERROR,
            h2::Reason::ENHANCE_YOUR_CALM,
            h2::Reason::from(0xff),
        ] {
            assert!(!is_http2_protocol_error(&h2::Error::from(reason)));
        }

        let error = std::io::Error::other("connection reset by peer");
        assert!(!is_http2_protocol_error(&error));
    }

    #[test]
    fn test_should_retry() {
        assert!(StatusCode::REQUEST_TIMEOUT.should_retry());
        assert!(StatusCode::TOO_MANY_REQUESTS.should_retry());
        assert!(!StatusCode::FORBIDDEN.should_retry());
        assert!(StatusCode::INTERNAL_SERVER_ERROR.should_retry());
    }
}
