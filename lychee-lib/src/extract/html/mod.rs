//! Extract links and fragments from html documents
pub(crate) mod html5ever;
pub(crate) mod html5gum;
mod srcset;

use linkify::{LinkFinder, LinkKind};

/// Check if the given URL is an email link.
///
/// This operates on the raw URL strings, not the linkified version because it
/// gets used in the HTML extractors, which parse the HTML attributes directly
/// and return the raw strings.
///
/// Note that `LinkFinder::links()` is lazy and traverses the input in `O(n)`,
/// so there should be no big performance penalty for calling this function.
pub(crate) fn is_email_link(input: &str) -> bool {
    let mut findings = LinkFinder::new().kinds(&[LinkKind::Email]).links(input);
    let email = match findings.next() {
        None => return false,
        Some(email) => email.as_str(),
    };

    // Email needs to match the full string.
    // Strip the "mailto:" prefix if it exists.
    input.strip_prefix("mailto:").unwrap_or(input) == email
}

/// Check if the given element is in the list of preformatted ("verbatim") tags.
///
/// These will be excluded from link checking by default.
// Including the <script> tag is debatable, but the alternative is to
// have a separate list of tags which need a separate config setting and that
// seems worse.
pub(crate) fn is_verbatim_elem(name: &str) -> bool {
    matches!(
        name,
        "code"
            | "kbd"
            | "listing"
            | "noscript"
            | "plaintext"
            | "pre"
            | "samp"
            | "script"
            | "textarea"
            | "var"
            | "xmp"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_email_link() {
        assert!(is_email_link("mailto:steve@apple.com"));
        assert!(!is_email_link("mailto:steve@apple.com in a sentence"));

        assert!(is_email_link("foo@example.org"));
        assert!(!is_email_link("foo@example.org in sentence"));
        assert!(!is_email_link("https://example.org"));
    }

    #[test]
    fn test_verbatim_matching() {
        assert!(is_verbatim_elem("pre"));
        assert!(is_verbatim_elem("code"));
        assert!(is_verbatim_elem("listing"));
        assert!(is_verbatim_elem("script"));
    }
}
