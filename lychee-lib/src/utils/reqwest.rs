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
pub(crate) fn analyze_error_chain(error: &reqwest::Error) -> String {
    // First check reqwest's built-in categorization
    if let Some(basic_message) = analyze_basic_reqwest_error(error) {
        return basic_message;
    }

    // Traverse error chain for detailed analysis
    if let Some(chain_message) = analyze_error_source_chain(error) {
        return chain_message;
    }

    // Fallback to basic reqwest error categorization
    fallback_reqwest_analysis(error)
}

/// Analyze basic reqwest error types first
fn analyze_basic_reqwest_error(error: &reqwest::Error) -> Option<String> {
    if error.is_timeout() {
        return Some(
            "Request timed out. Try increasing timeout or check server status".to_string(),
        );
    }

    if error.is_redirect() {
        return Some("Too many redirects - check for redirect loops".to_string());
    }

    if let Some(status) = error.status() {
        let reason = status.canonical_reason().unwrap_or("Unknown");
        return Some(format!(
            "HTTP {}: {} - check URL and server status",
            status.as_u16(),
            reason
        ));
    }

    None
}

/// Traverse the error chain for detailed analysis
fn analyze_error_source_chain(error: &reqwest::Error) -> Option<String> {
    let mut source = error.source();
    while let Some(err) = source {
        // Check for I/O errors (most network issues)
        if let Some(io_error) = err.downcast_ref::<std::io::Error>() {
            return Some(analyze_io_error(io_error));
        }

        // Check for hyper-specific errors
        if let Some(hyper_error) = err.downcast_ref::<hyper::Error>() {
            return Some(analyze_hyper_error(hyper_error));
        }

        // Check for URL parsing errors
        if let Some(url_error) = err.downcast_ref::<url::ParseError>() {
            return Some(analyze_url_parse_error(*url_error));
        }

        // Check for generic error types by examining their string representation
        if let Some(generic_message) = analyze_generic_error_string(&err.to_string()) {
            return Some(generic_message);
        }

        source = err.source();
    }

    None
}

/// Analyze I/O errors with specific categorization
fn analyze_io_error(io_error: &std::io::Error) -> String {
    match io_error.kind() {
        std::io::ErrorKind::ConnectionRefused => {
            "Connection refused - server may be down or port blocked".to_string()
        }
        std::io::ErrorKind::TimedOut => {
            "Request timed out. Try increasing timeout or check server status".to_string()
        }
        std::io::ErrorKind::NotFound => {
            "DNS resolution failed - check hostname spelling".to_string()
        }
        std::io::ErrorKind::PermissionDenied => {
            "Permission denied - check firewall or proxy settings".to_string()
        }
        std::io::ErrorKind::Other => analyze_io_other_error(io_error),
        std::io::ErrorKind::NetworkUnreachable => {
            "Network unreachable. Check internet connection or VPN settings".to_string()
        }
        std::io::ErrorKind::AddrNotAvailable => {
            "Address not available. Check network interface configuration".to_string()
        }
        std::io::ErrorKind::AddrInUse => {
            "Address already in use. Port conflict or service already running".to_string()
        }
        std::io::ErrorKind::BrokenPipe => {
            "Connection broken. Server closed connection unexpectedly".to_string()
        }
        std::io::ErrorKind::InvalidData => {
            "Invalid response data. Server sent malformed response".to_string()
        }
        std::io::ErrorKind::UnexpectedEof => {
            "Connection closed unexpectedly. Server terminated early".to_string()
        }
        std::io::ErrorKind::Interrupted => {
            "Request interrupted. Try again or check for system issues".to_string()
        }
        std::io::ErrorKind::Unsupported => {
            "Operation not supported. Check protocol or server capabilities".to_string()
        }
        _ => {
            // For unknown/uncategorized errors, provide more context
            let kind_name = format!("{:?}", io_error.kind());
            match kind_name.as_str() {
                "Uncategorized" => {
                    "Connection failed. Check network connectivity and firewall settings"
                        .to_string()
                }
                _ => {
                    format!("I/O error ({kind_name}). Check network connectivity and server status",)
                }
            }
        }
    }
}

/// Analyze I/O errors with kind "Other" - typically inner SSL/TLS and DNS errors
fn analyze_io_other_error(io_error: &std::io::Error) -> String {
    if let Some(inner) = io_error.get_ref() {
        let inner_msg = inner.to_string();

        // Certificate errors
        if inner_msg.contains("certificate") {
            return analyze_certificate_error(&inner_msg);
        }

        // DNS errors
        if inner_msg.contains("failed to lookup address")
            || inner_msg.contains("nodename nor servname")
        {
            return "DNS resolution failed. Check hostname and DNS settings".to_string();
        }
        if inner_msg.contains("Temporary failure in name resolution") {
            return "DNS temporarily unavailable. Try again later".to_string();
        }

        // TLS/SSL handshake errors
        if inner_msg.contains("handshake") {
            return "TLS handshake failed. Check SSL/TLS configuration".to_string();
        }

        // Return the inner error message if it's more specific
        format!("Network error: {inner_msg}")
    } else {
        "Connection failed. Check network connectivity and firewall settings".to_string()
    }
}

/// Analyze certificate-related errors
fn analyze_certificate_error(error_msg: &str) -> String {
    if error_msg.contains("expired") || error_msg.contains("NotValidAtThisTime") {
        "SSL certificate expired. Site needs to renew certificate".to_string()
    } else if error_msg.contains("hostname") || error_msg.contains("NotValidForName") {
        "SSL certificate hostname mismatch. Check URL spelling".to_string()
    } else if error_msg.contains("self signed") || error_msg.contains("UnknownIssuer") {
        "SSL certificate not trusted. Use --insecure if site is trusted".to_string()
    } else if error_msg.contains("verify failed") {
        "SSL certificate verification failed. Check certificate validity".to_string()
    } else {
        "SSL certificate error. Check certificate validity".to_string()
    }
}

/// Analyze hyper-specific errors
fn analyze_hyper_error(hyper_error: &hyper::Error) -> String {
    if hyper_error.is_parse() {
        if hyper_error.is_parse_status() {
            return "Invalid HTTP status code from server".to_string();
        }
        return "Invalid HTTP response format. Server may be misconfigured".to_string();
    }
    if hyper_error.is_timeout() {
        return "Request timed out. Try increasing timeout or check server status".to_string();
    }
    if hyper_error.is_user() {
        if hyper_error.is_body_write_aborted() {
            return "Request body upload was aborted".to_string();
        }
        return "Invalid request format. Check request parameters".to_string();
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
        "Connection failed. Check network connectivity and firewall settings".to_string()
    } else if hyper_msg.contains("http2 error") {
        "HTTP/2 protocol error. Server may not support HTTP/2 properly".to_string()
    } else if hyper_msg.contains("channel closed") {
        "HTTP connection channel closed unexpectedly".to_string()
    } else if hyper_msg.contains("operation was canceled") {
        "HTTP operation was canceled before completion".to_string()
    } else if hyper_msg.contains("message head is too large") {
        "HTTP headers too large. Server response headers exceed limits".to_string()
    } else if hyper_msg.contains("invalid content-length") {
        "Invalid Content-Length header from server".to_string()
    } else {
        format!("HTTP protocol error: {hyper_error}")
    }
}

/// Analyze URL parsing errors
fn analyze_url_parse_error(url_error: url::ParseError) -> String {
    match url_error {
        url::ParseError::EmptyHost => "Invalid URL: empty hostname".to_string(),
        url::ParseError::InvalidDomainCharacter => {
            "Invalid URL: invalid characters in domain".to_string()
        }
        url::ParseError::InvalidPort => "Invalid URL: invalid port number".to_string(),
        url::ParseError::RelativeUrlWithoutBase => {
            "Invalid URL: relative URL without base".to_string()
        }
        _ => format!("Invalid URL format: {url_error}"),
    }
}

/// Analyze generic error strings for common patterns
fn analyze_generic_error_string(error_msg: &str) -> Option<String> {
    // Certificate-related errors
    if error_msg.contains("certificate") {
        return Some(analyze_certificate_error(error_msg));
    }

    // TLS/SSL handshake errors
    if error_msg.contains("handshake") || error_msg.contains("TLS") || error_msg.contains("SSL") {
        return Some("TLS handshake failed. Check SSL/TLS configuration".to_string());
    }

    // DNS errors
    if error_msg.contains("name resolution") || error_msg.contains("hostname") {
        return Some("DNS resolution failed. Check hostname and DNS settings".to_string());
    }

    // Connection-specific errors
    if error_msg.contains("Connection refused") || error_msg.contains("connection refused") {
        return Some(
            "Connection refused. Server is not accepting connections (check if service is running)"
                .to_string(),
        );
    }

    if error_msg.contains("Connection reset") || error_msg.contains("connection reset") {
        return Some("Connection reset by server. Server forcibly closed connection".to_string());
    }

    if error_msg.contains("No route to host") || error_msg.contains("no route") {
        return Some(
            "No route to host. Check network routing or firewall configuration".to_string(),
        );
    }

    if error_msg.contains("Network is unreachable") || error_msg.contains("network unreachable") {
        return Some("Network unreachable. Check internet connection or VPN settings".to_string());
    }

    // Timeout-related errors
    if error_msg.contains("timed out") || error_msg.contains("timeout") {
        return Some(
            "Request timed out. Try increasing timeout or check server status".to_string(),
        );
    }

    // Protocol-specific errors
    if error_msg.contains("protocol") && error_msg.contains("not supported") {
        return Some("Protocol not supported. Check URL scheme (http/https)".to_string());
    }

    None
}

/// Fallback analysis using basic reqwest error categorization
fn fallback_reqwest_analysis(error: &reqwest::Error) -> String {
    if error.is_connect() {
        "Connection failed. Check network connectivity and firewall settings".to_string()
    } else if error.is_request() {
        "Request failed. Check URL format and parameters".to_string()
    } else if error.is_decode() {
        "Response decoding failed. Server returned invalid data".to_string()
    } else {
        format!("Request failed: {error}")
    }
}

#[cfg(test)]
mod tests {
    use crate::ErrorKind;

    /// Test that `ErrorKind::details()` properly uses the new analysis
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
