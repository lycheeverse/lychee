use std::error::Error;

/// Analyze the error chain of a reqwest error and return a concise, actionable message.
///
/// This traverses the error chain to extract specific failure details and provides
/// user-friendly explanations with actionable suggestions when possible.
///
/// The advantage of this approach is that we can be way more specific about the
/// errors which can occur, rather than just returning a generic error message.
/// The downside is that we have to maintain this code as reqwest and hyper
/// evolve. However, this is a trade-off we are willing to make for better user
/// experience.
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
                    "Request timed out - try increasing timeout or check server status".to_string()
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
                                return "SSL certificate not trusted - use --insecure if site is trusted".to_string();
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
                        "Connection failed - check network connectivity and firewall settings"
                            .to_string()
                    }
                }
                std::io::ErrorKind::NetworkUnreachable => {
                    "Network unreachable - check internet connection or VPN settings".to_string()
                }
                std::io::ErrorKind::AddrNotAvailable => {
                    "Address not available - check network interface configuration".to_string()
                }
                std::io::ErrorKind::AddrInUse => {
                    "Address already in use - port conflict or service already running".to_string()
                }
                std::io::ErrorKind::BrokenPipe => {
                    "Connection broken - server closed connection unexpectedly".to_string()
                }
                std::io::ErrorKind::InvalidData => {
                    "Invalid response data - server sent malformed response".to_string()
                }
                std::io::ErrorKind::UnexpectedEof => {
                    "Connection closed unexpectedly - server terminated early".to_string()
                }
                std::io::ErrorKind::Interrupted => {
                    "Request interrupted - try again or check for system issues".to_string()
                }
                std::io::ErrorKind::Unsupported => {
                    "Operation not supported - check protocol or server capabilities".to_string()
                }
                _ => {
                    // For unknown/uncategorized errors, provide more context
                    let kind_name = format!("{:?}", io_error.kind());
                    match kind_name.as_str() {
                        "Uncategorized" => {
                            "Connection failed - check network connectivity and firewall settings"
                                .to_string()
                        }
                        _ => format!(
                            "I/O error ({kind_name}): check network connectivity and server status",
                        ),
                    }
                }
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
                return "Request timed out - try increasing timeout or check server status"
                    .to_string();
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
                return "Connection failed - check network connectivity and firewall settings"
                    .to_string();
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

        // Check for generic error types by examining their string representation
        // This catches SSL/TLS errors that come through as generic Error trait objects
        let error_msg = err.to_string();

        // Certificate-related errors
        if error_msg.contains("certificate") {
            if error_msg.contains("expired") || error_msg.contains("not valid") {
                return "SSL certificate expired - site needs to renew certificate".to_string();
            }
            if error_msg.contains("not trusted") || error_msg.contains("untrusted") {
                return "SSL certificate not trusted - use --insecure if site is trusted"
                    .to_string();
            }
            if error_msg.contains("hostname") || error_msg.contains("name mismatch") {
                return "SSL certificate hostname mismatch - check URL spelling".to_string();
            }
            if error_msg.contains("self signed") || error_msg.contains("self-signed") {
                return "SSL certificate not trusted - use --insecure if site is trusted"
                    .to_string();
            }
            return "SSL certificate error - check certificate validity".to_string();
        }

        // TLS/SSL handshake errors
        if error_msg.contains("handshake") || error_msg.contains("TLS") || error_msg.contains("SSL")
        {
            return "TLS handshake failed - check SSL/TLS configuration".to_string();
        }

        // DNS errors
        if error_msg.contains("name resolution") || error_msg.contains("hostname") {
            return "DNS resolution failed - check hostname and DNS settings".to_string();
        }

        // Connection-specific errors
        if error_msg.contains("Connection refused") || error_msg.contains("connection refused") {
            return "Connection refused - server is not accepting connections (check if service is running)".to_string();
        }

        if error_msg.contains("Connection reset") || error_msg.contains("connection reset") {
            return "Connection reset by server - server forcibly closed connection".to_string();
        }

        if error_msg.contains("No route to host") || error_msg.contains("no route") {
            return "No route to host - check network routing or firewall configuration"
                .to_string();
        }

        if error_msg.contains("Network is unreachable") || error_msg.contains("network unreachable")
        {
            return "Network unreachable - check internet connection or VPN settings".to_string();
        }

        // Timeout-related errors
        if error_msg.contains("timed out") || error_msg.contains("timeout") {
            return "Request timed out - try increasing timeout or check server status".to_string();
        }

        // Protocol-specific errors
        if error_msg.contains("protocol") && error_msg.contains("not supported") {
            return "Protocol not supported - check URL scheme (http/https)".to_string();
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

    /// Test that ErrorKind::details() properly uses the new analysis
    #[test]
    fn test_error_kind_details_integration() {
        // Test rejected status code
        let status_error = ErrorKind::RejectedStatusCode(http::StatusCode::NOT_FOUND);
        assert_eq!(status_error.details(), Some("Not Found".to_string()));

        // Test that network request errors would use analyze_error_chain
        // (actual reqwest::Error creation is complex, so we test the integration point)

        // For other error types, ensure they still work
        let test_error = ErrorKind::TestError;
        assert_eq!(
            test_error.details(),
            Some("Test error for formatter testing".to_string())
        );
    }
}
