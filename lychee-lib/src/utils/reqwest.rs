use std::error::Error;

/// A rule for matching error message patterns to human-readable messages
struct ErrorRule {
    patterns: &'static [&'static str],
    message: &'static str,
}

impl ErrorRule {
    /// Create a new error rule
    const fn new(patterns: &'static [&'static str], message: &'static str) -> Self {
        Self { patterns, message }
    }

    /// Check if any of the patterns match the given text
    fn matches(&self, text: &str) -> bool {
        self.patterns.iter().any(|pattern| text.contains(pattern))
    }

    /// Get the message for this rule
    const fn message(&self) -> &'static str {
        self.message
    }
}

/// A builder for creating and matching against multiple error rules
struct ErrorRules {
    rules: Vec<ErrorRule>,
    fallback: Option<String>,
}

impl ErrorRules {
    /// Create a new `ErrorRules` builder
    const fn new() -> Self {
        Self {
            rules: Vec::new(),
            fallback: None,
        }
    }

    /// Add a rule to the matcher
    fn rule(mut self, patterns: &'static [&'static str], message: &'static str) -> Self {
        self.rules.push(ErrorRule::new(patterns, message));
        self
    }

    /// Set a fallback message if no rules match
    fn fallback(mut self, message: impl Into<String>) -> Self {
        self.fallback = Some(message.into());
        self
    }

    /// Match against the error message and return the appropriate response
    fn match_error(self, error_msg: &str) -> String {
        for rule in &self.rules {
            if rule.matches(error_msg) {
                return rule.message().to_string();
            }
        }

        self.fallback
            .unwrap_or_else(|| format!("Unhandled error: {error_msg}"))
    }
}

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

/// Analyze I/O errors with kind "Other" using rule-based pattern matching
fn analyze_io_other_error(io_error: &std::io::Error) -> String {
    if let Some(inner) = io_error.get_ref() {
        let inner_msg = inner.to_string();

        // Special case: certificate errors need deeper analysis
        if inner_msg.contains("certificate") {
            return analyze_certificate_error(&inner_msg);
        }

        // Rule-based pattern matching for other inner error types
        ErrorRules::new()
            .rule(
                &["failed to lookup address", "nodename nor servname"],
                "DNS resolution failed. Check hostname and DNS settings",
            )
            .rule(
                &["Temporary failure in name resolution"],
                "DNS temporarily unavailable. Try again later",
            )
            .rule(
                &["handshake"],
                "TLS handshake failed. Check SSL/TLS configuration",
            )
            .fallback(format!("Network error: {inner_msg}"))
            .match_error(&inner_msg)
    } else {
        "Connection failed. Check network connectivity and firewall settings".to_string()
    }
}

/// Analyze certificate-related errors using pattern matching rules
fn analyze_certificate_error(error_msg: &str) -> String {
    ErrorRules::new()
        .rule(
            &[
                "expired",
                "NotValidAtThisTime",
                "certificate has expired",
                "certificate is not valid on",
            ],
            "SSL certificate expired. Site needs to renew certificate",
        )
        .rule(
            &["hostname", "NotValidForName"],
            "SSL certificate hostname mismatch. Check URL spelling",
        )
        .rule(
            &["self signed", "UnknownIssuer", "not trusted"],
            "SSL certificate not trusted. Use --insecure if site is trusted",
        )
        .rule(
            &["verify failed"],
            "SSL certificate verification failed. Check certificate validity",
        )
        .fallback("SSL certificate error. Check certificate validity")
        .match_error(error_msg)
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

    let hyper_msg = hyper_error.to_string();

    // Rule-based analysis of hyper error descriptions
    ErrorRules::new()
        .rule(
            &["connection error"],
            "Connection failed. Check network connectivity and firewall settings",
        )
        .rule(
            &["http2 error"],
            "HTTP/2 protocol error. Server may not support HTTP/2 properly",
        )
        .rule(
            &["channel closed"],
            "HTTP connection channel closed unexpectedly",
        )
        .rule(
            &["operation was canceled"],
            "HTTP operation was canceled before completion",
        )
        .rule(
            &["message head is too large"],
            "HTTP headers too large. Server response headers exceed limits",
        )
        .rule(
            &["invalid content-length"],
            "Invalid Content-Length header from server",
        )
        .fallback(format!("HTTP protocol error: {hyper_error}"))
        .match_error(&hyper_msg)
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

/// Analyze generic error strings using a rule-based pattern matching system
fn analyze_generic_error_string(error_msg: &str) -> Option<String> {
    // Special case: certificate errors need deeper analysis
    if error_msg.contains("certificate") {
        return Some(analyze_certificate_error(error_msg));
    }

    // Special case: protocol errors need compound condition check
    if error_msg.contains("protocol") && error_msg.contains("not supported") {
        return Some("Protocol not supported. Check URL scheme (http/https)".to_string());
    }

    // Try to match using our rule-based system
    let result = ErrorRules::new()
        .rule(
            &["handshake", "TLS", "SSL"],
            "TLS handshake failed. Check SSL/TLS configuration",
        )
        .rule(
            &["name resolution", "hostname"],
            "DNS resolution failed. Check hostname and DNS settings",
        )
        .rule(
            &["Connection refused", "connection refused"],
            "Connection refused. Server is not accepting connections (check if service is running)",
        )
        .rule(
            &["Connection reset", "connection reset"],
            "Connection reset by server. Server forcibly closed connection",
        )
        .rule(
            &["No route to host", "no route"],
            "No route to host. Check network routing or firewall configuration",
        )
        .rule(
            &["Network is unreachable", "network unreachable"],
            "Network unreachable. Check internet connection or VPN settings",
        )
        .rule(
            &["timed out", "timeout"],
            "Request timed out. Try increasing timeout or check server status",
        )
        .match_error(error_msg);

    // Only return Some if we actually matched a rule (not the fallback)
    if result.starts_with("Unhandled error:") {
        None
    } else {
        Some(result)
    }
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
