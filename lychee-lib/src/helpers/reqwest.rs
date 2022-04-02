/// Extract the most relevant parts from a reqwest error
///
/// The reqwest `Error` fields aren't public as they are an implementation
/// detail. Instead, a human-readable error message can be obtained. However,
/// the error message is quite verbose and the information is redundant. For
/// example it contains the `URL`, which is already part of our `ResponseBody`.
/// Therefore we try to trim away the redundant parts so that the `ResponseBody`
/// output is cleaner.
pub(crate) fn trim_error_output(e: &reqwest::Error) -> String {
    // Defer to separate function for easier testability.
    // Otherwise a `reqwest::Error` object would have to be created.
    trim_inner(e.to_string())
}

/// Get meaningful information from a reqwest error string.
///
/// At the moment we only extract everything after "error trying to connect",
/// which is the most common error string in our tests.
fn trim_inner(text: String) -> String {
    if let Some((_before, after)) = text.split_once("error trying to connect:") {
        return after.trim().to_string();
    }
    text
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_reqwest_error() {
        let reqwest_error = "error sending request for url (https://example.com): error trying to connect: The certificate was not trusted.".to_string();

        assert_eq!(
            trim_inner(reqwest_error),
            "The certificate was not trusted."
        );
    }
}
