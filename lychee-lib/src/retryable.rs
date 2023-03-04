use std::io;

use http::StatusCode;

use crate::ErrorKind;

/// An extension trait to help determine if a given HTTP request
/// is retryable.
///
/// Inspired by `Retryable` from
/// [reqwest-middleware](https://github.com/TrueLayer/reqwest-middleware/blob/f854725791ccf4a02c401a26cab3d9db753f468c/reqwest-retry/src/retryable.rs)
pub(crate) trait RetryExt {
    fn should_retry(&self) -> bool;
}

impl RetryExt for reqwest::Response {
    /// Try to map a `reqwest` response into `Retryable`.
    #[allow(clippy::if_same_then_else)]
    fn should_retry(&self) -> bool {
        let status = self.status();
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
    fn should_retry(&self) -> bool {
        if self.is_timeout() || self.is_connect() {
            true
        } else if self.is_body() || self.is_decode() || self.is_builder() || self.is_redirect() {
            false
        } else if self.is_request() {
            // It seems that hyper::Error(IncompleteMessage) is not correctly handled by reqwest.
            // Here we check if the Reqwest error was originated by hyper and map it consistently.
            if let Some(hyper_error) = get_source_error_type::<hyper::Error>(&self) {
                // The hyper::Error(IncompleteMessage) is raised if the HTTP response is well formatted but does not contain all the bytes.
                // This can happen when the server has started sending back the response but the connection is cut halfway thorugh.
                // We can safely retry the call, hence marking this error as [`Retryable::Transient`].
                // Instead hyper::Error(Canceled) is raised when the connection is
                // gracefully closed on the server side.
                if hyper_error.is_incomplete_message() || hyper_error.is_canceled() {
                    true

                // Try and downcast the hyper error to io::Error if that is the
                // underlying error, and try and classify it.
                } else if let Some(io_error) = get_source_error_type::<io::Error>(hyper_error) {
                    classify_io_error(io_error)
                } else {
                    false
                }
            } else {
                false
            }
        } else {
            // We omit checking if error.is_status() since we check that already.
            // However, if Response::error_for_status is used the status will still
            // remain in the response object.
            false
        }
    }
}

impl RetryExt for ErrorKind {
    fn should_retry(&self) -> bool {
        // If the error is a `reqwest::Error`, delegate to that
        if let Some(r) = self.reqwest_error() {
            r.should_retry()
        // Github errors are sometimes reqwest errors.
        // In that case, delegate to that.
        } else if let Some(octocrab::Error::Http {
            source,
            backtrace: _,
        }) = self.github_error()
        {
            source.should_retry()
        } else {
            false
        }
    }
}

/// Classifies an `io::Error` into retryable or not.
fn classify_io_error(error: &io::Error) -> bool {
    matches!(
        error.kind(),
        io::ErrorKind::ConnectionReset | io::ErrorKind::ConnectionAborted
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
