use std::io;

use http::StatusCode;

use crate::{ErrorKind, Status};

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
    #[allow(clippy::if_same_then_else)]
    fn should_retry(&self) -> bool {
        let status = *self;
        if status.is_server_error() {
            true
        } else if status.is_client_error()
            && status != StatusCode::REQUEST_TIMEOUT
            && status != StatusCode::TOO_MANY_REQUESTS
        {
            false
        } else if status.is_success() {
            false
        } else {
            status == StatusCode::REQUEST_TIMEOUT || status == StatusCode::TOO_MANY_REQUESTS
        }
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
            if let Some(hyper_error) = get_source_error_type::<hyper::Error>(&self) {
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
                } else if let Some(io_error) = get_source_error_type::<io::Error>(hyper_error) {
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
    #[allow(clippy::match_same_arms)]
    fn should_retry(&self) -> bool {
        match self {
            Status::Ok(_) => false,
            Status::Error(err) => err.should_retry(),
            Status::RequestError(_) => false,
            Status::Timeout(_) => true,
            Status::Redirected(_, _) => false,
            Status::UnknownStatusCode(_) => false,
            Status::Excluded => false,
            Status::Unsupported(_) => false,
            Status::Cached(_) => false,
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

/// Downcasts the given err source into T.
fn get_source_error_type<T: std::error::Error + 'static>(
    err: &dyn std::error::Error,
) -> Option<&T> {
    let mut source = err.source();

    while let Some(err) = source {
        if let Some(hyper_err) = err.downcast_ref::<T>() {
            return Some(hyper_err);
        }

        source = err.source();
    }
    None
}

#[cfg(test)]
mod tests {
    use http::StatusCode;

    use super::RetryExt;

    #[test]
    fn test_should_retry() {
        assert!(StatusCode::REQUEST_TIMEOUT.should_retry());
        assert!(StatusCode::TOO_MANY_REQUESTS.should_retry());
        assert!(!StatusCode::FORBIDDEN.should_retry());
        assert!(StatusCode::INTERNAL_SERVER_ERROR.should_retry());
    }
}
