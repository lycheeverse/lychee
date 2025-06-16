use std::error::Error;

/// Analyze the error chain of a reqwest error and return a concise, actionable message.
///
/// This traverses the error chain to extract specific failure details and provides
/// user-friendly explanations with actionable suggestions when possible.
#[allow(clippy::too_many_lines)]
pub(crate) fn analyze_error_chain(error: &reqwest::Error) -> String {
    // First check reqwest's built-in categorization
    if error.is_timeout() {
        return "Request timed out - try increasing timeout or check server status".to_string();
    }

    if error.is_redirect() {
        return "Too many redirects - check for redirect loops".to_string();
    }

    if let Some(status) = error.status() {
        let reason = status.canonical_reason().unwrap_or("Unknown");
        return format!(
            "HTTP {}: {} - check URL and server status",
            status.as_u16(),
            reason
        );
    }

    // Traverse error chain for detailed analysis
    let mut source = error.source();
    while let Some(err) = source {
        // Check for I/O errors (most network issues)
        if let Some(io_error) = err.downcast_ref::<std::io::Error>() {
            return match io_error.kind() {
                std::io::ErrorKind::ConnectionRefused => {
                    "Connection refused - server may be down or port blocked".to_string()
                }
                std::io::ErrorKind::TimedOut => {
                    "Connection timed out - server may be overloaded or unreachable".to_string()
                }
                std::io::ErrorKind::NotFound => {
                    "DNS resolution failed - check hostname spelling".to_string()
                }
                std::io::ErrorKind::PermissionDenied => {
                    "Permission denied - check firewall or proxy settings".to_string()
                }
                std::io::ErrorKind::Other => {
                    // Check inner error for more specific details
                    if let Some(inner) = io_error.get_ref() {
                        let inner_msg = inner.to_string();

                        // Certificate errors
                        if inner_msg.contains("certificate") {
                            if inner_msg.contains("expired")
                                || inner_msg.contains("NotValidAtThisTime")
                            {
                                return "SSL certificate expired - site needs to renew certificate"
                                    .to_string();
                            }
                            if inner_msg.contains("hostname")
                                || inner_msg.contains("NotValidForName")
                            {
                                return "SSL certificate hostname mismatch - check URL spelling"
                                    .to_string();
                            }
                            if inner_msg.contains("self signed")
                                || inner_msg.contains("UnknownIssuer")
                            {
                                return "SSL certificate not trusted - use --accept-invalid-certs if site is trusted".to_string();
                            }
                            if inner_msg.contains("verify failed") {
                                return "SSL certificate verification failed - check certificate validity".to_string();
                            }
                            return "SSL certificate error - check certificate validity"
                                .to_string();
                        }

                        // DNS errors
                        if inner_msg.contains("failed to lookup address")
                            || inner_msg.contains("nodename nor servname")
                        {
                            return "DNS resolution failed - check hostname and DNS settings"
                                .to_string();
                        }
                        if inner_msg.contains("Temporary failure in name resolution") {
                            return "DNS temporarily unavailable - try again later".to_string();
                        }

                        // TLS/SSL handshake errors
                        if inner_msg.contains("handshake") {
                            return "TLS handshake failed - check SSL/TLS configuration"
                                .to_string();
                        }

                        // Return the inner error message if it's more specific
                        format!("Network error: {inner_msg}")
                    } else {
                        "Network connection failed - check internet connectivity".to_string()
                    }
                }
                _ => format!(
                    "I/O error: {:?} - check network connectivity",
                    io_error.kind()
                ),
            };
        }

        // Check for hyper-specific errors
        if let Some(hyper_error) = err.downcast_ref::<hyper::Error>() {
            if hyper_error.is_parse() {
                if hyper_error.is_parse_status() {
                    return "Invalid HTTP status code from server".to_string();
                }
                return "Invalid HTTP response format - server may be misconfigured".to_string();
            }
            if hyper_error.is_timeout() {
                return "HTTP request timed out - increase timeout or check server".to_string();
            }
            if hyper_error.is_user() {
                if hyper_error.is_body_write_aborted() {
                    return "Request body upload was aborted".to_string();
                }
                return "Invalid request format - check request parameters".to_string();
            }
            if hyper_error.is_canceled() {
                return "Request was canceled".to_string();
            }
            if hyper_error.is_closed() {
                return "Connection was closed unexpectedly".to_string();
            }
            if hyper_error.is_incomplete_message() {
                return "Connection closed before response completed".to_string();
            }

            // More detailed analysis of hyper error description
            let hyper_msg = hyper_error.to_string();
            if hyper_msg.contains("connection error") {
                return "HTTP connection failed - check network and firewall settings".to_string();
            }
            if hyper_msg.contains("http2 error") {
                return "HTTP/2 protocol error - server may not support HTTP/2 properly"
                    .to_string();
            }
            if hyper_msg.contains("channel closed") {
                return "HTTP connection channel closed unexpectedly".to_string();
            }
            if hyper_msg.contains("operation was canceled") {
                return "HTTP operation was canceled before completion".to_string();
            }
            if hyper_msg.contains("message head is too large") {
                return "HTTP headers too large - server response headers exceed limits"
                    .to_string();
            }
            if hyper_msg.contains("invalid content-length") {
                return "Invalid Content-Length header from server".to_string();
            }

            return format!("HTTP protocol error: {hyper_error}");
        }

        // Check for URL parsing errors
        if let Some(url_error) = err.downcast_ref::<url::ParseError>() {
            return match url_error {
                url::ParseError::EmptyHost => "Invalid URL: empty hostname".to_string(),
                url::ParseError::InvalidDomainCharacter => {
                    "Invalid URL: invalid characters in domain".to_string()
                }
                url::ParseError::InvalidPort => "Invalid URL: invalid port number".to_string(),
                url::ParseError::RelativeUrlWithoutBase => {
                    "Invalid URL: relative URL without base".to_string()
                }
                _ => format!("Invalid URL format: {url_error}"),
            };
        }

        source = err.source();
    }

    // Fallback to basic reqwest error categorization
    if error.is_connect() {
        "Connection failed - check network connectivity and firewall settings".to_string()
    } else if error.is_request() {
        "Request failed - check URL format and parameters".to_string()
    } else if error.is_decode() {
        "Response decoding failed - server returned invalid data".to_string()
    } else {
        format!("Request failed: {error}")
    }
}

#[cfg(test)]
mod tests {
    use crate::ErrorKind;

    #[test]
    fn test_error_kind_details_integration() {
        // Test that ErrorKind::details() properly uses the new analysis

        // Test rejected status code
        let status_error = ErrorKind::RejectedStatusCode(http::StatusCode::NOT_FOUND);
        assert_eq!(status_error.details(), Some("Not Found".to_string()));

        // Test that network request errors would use analyze_error_chain
        // (actual reqwest::Error creation is complex, so we test the integration point)

        // For other error types, ensure they still work
        let empty_url_error = ErrorKind::EmptyUrl;
        assert_eq!(empty_url_error.details(), None);
    }
}
