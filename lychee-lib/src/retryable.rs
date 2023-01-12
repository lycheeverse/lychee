use std::io;

use http::StatusCode;

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
        } else if status == StatusCode::REQUEST_TIMEOUT || status == StatusCode::TOO_MANY_REQUESTS {
            true
        } else {
            false
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

/// Classifies an io::Error into retryable or not.
fn classify_io_error(error: &io::Error) -> bool {
    match error.kind() {
        io::ErrorKind::ConnectionReset | io::ErrorKind::ConnectionAborted => true,
        _ => false,
    }
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
