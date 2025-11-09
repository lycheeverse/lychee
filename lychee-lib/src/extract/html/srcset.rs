//! Extract all image URLs from a srcset.
//!
//! A `srcset` is a string containing a comma-separated list of one or more
//! image candidate strings to be used when determining which image resource to
//! present inside an `<img>` element.
//!
//! Each image candidate string must begin with a valid URL referencing a
//! non-interactive graphic resource. This is followed by whitespace and then a
//! condition descriptor that indicates the circumstances in which the indicated
//! image should be used. Space characters, other than the whitespace separating
//! the URL and the corresponding condition descriptor, are ignored; this
//! includes both leading and trailing space, as well as space before or after
//! each comma.
//!
//! Note: this handles cases where a URL contains a comma, which should be
//! escaped, but is a valid character in a URL and occurs in the wild.
//! Note: we cannot assume that commas within URLs are encoded as `%2C`, as they
//! should be according to RFC 3986.
//! Thus, the parsing process becomes significantly more complex and we need to
//! use a state machine to keep track of the current state.

use log::info;
use std::result::Result;

enum State {
    InsideDescriptor,
    AfterDescriptor,
    InsideParens,
}

/// Split an input string at the first character for which
/// the predicate returns false.
///
/// In other words, returns the longest prefix span where `predicate` is
/// satisfied, along with the rest of the string.
fn split_at<F>(input: &str, predicate: F) -> (&str, &str)
where
    F: Fn(&char) -> bool,
{
    for (i, ch) in input.char_indices() {
        if !predicate(&ch) {
            return input.split_at(i);
        }
    }
    (input, "")
}

/// Parse a srcset string into a list of URLs.
//
// This state-machine is a bit convoluted, but we keep everything in one place
// for simplicity so we have to please clippy.
pub(crate) fn parse(input: &str) -> Vec<&str> {
    let mut candidates: Vec<&str> = Vec::new();
    let mut remaining = input;
    while !remaining.is_empty() {
        remaining = match parse_one_url(remaining) {
            Ok((rem, None)) => rem,
            Ok((rem, Some(url))) => {
                candidates.push(url);
                rem
            }
            Err(e) => {
                info!("{e}");
                return vec![];
            }
        }
    }

    candidates
}

/// Implements one iteration of the "splitting loop" from the reference algorithm.
/// This is intended to be repeatedly called until the remaining string is empty.
///
/// Returns a tuple of remaining string and an optional parsed URL, if successful.
/// Otherwise, in case of srcset syntax errors, returns Err.
///
/// <https://html.spec.whatwg.org/multipage/images.html#parsing-a-srcset-attribute>
fn parse_one_url(remaining: &str) -> Result<(&str, Option<&str>), String> {
    let (start, remaining) = split_at(remaining, |c| *c == ',' || c.is_ascii_whitespace());

    if start.find(',').is_some() {
        return Err("srcset parse error (too many commas)".to_string());
    }

    if remaining.is_empty() {
        return Ok(("", None));
    }

    let (url, remaining) = split_at(remaining, |c| !c.is_ascii_whitespace());

    let comma_count = url.chars().rev().take_while(|c| *c == ',').count();
    if comma_count > 1 {
        return Err("srcset parse error (trailing commas)".to_string());
    }

    let url = url.get(..url.len() - comma_count);

    let (_spaces, remaining) = split_at(remaining, char::is_ascii_whitespace);

    let remaining = skip_descriptor(remaining);

    Ok((remaining, url))
}

/// Helper function to skip over a descriptor. Returns the string remaining
/// after the descriptor (i.e. a string beginning after the next comma or an
/// empty string).
#[allow(clippy::single_match)]
fn skip_descriptor(remaining: &str) -> &str {
    let mut state = State::InsideDescriptor;

    for (i, c) in remaining.char_indices() {
        match state {
            State::InsideDescriptor => match c {
                c if c.is_ascii_whitespace() => state = State::AfterDescriptor,
                '(' => state = State::InsideParens,
                ',' => return &remaining[i + c.len_utf8()..], // returns string after this comma
                _ => (),
            },
            State::InsideParens => match c {
                ')' => state = State::InsideDescriptor,
                _ => (),
            },
            State::AfterDescriptor => match c {
                c if c.is_ascii_whitespace() => (),
                _ => state = State::InsideDescriptor,
            },
        }
    }

    ""
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_collect_sequence_characters_with_empty_string() {
        let (sequence, remainder) = split_at("", |c| c.is_alphabetic());
        assert_eq!(sequence, "");
        assert_eq!(remainder, "");
    }

    #[test]
    fn test_collect_sequence_characters_with_alphabetic_predicate() {
        let (sequence, remainder) = split_at("abc123", |c| c.is_alphabetic());
        assert_eq!(sequence, "abc");
        assert_eq!(remainder, "123");
    }

    #[test]
    fn test_collect_sequence_characters_with_digit_predicate() {
        let (sequence, remainder) = split_at("123abc", char::is_ascii_digit);
        assert_eq!(sequence, "123");
        assert_eq!(remainder, "abc");
    }

    #[test]
    fn test_collect_sequence_characters_with_no_match() {
        let (sequence, remainder) = split_at("123abc", |c| c.is_whitespace());
        assert_eq!(sequence, "");
        assert_eq!(remainder, "123abc");
    }

    #[test]
    fn test_collect_sequence_characters_with_all_match() {
        let (sequence, remainder) = split_at("123abc", |c| !c.is_whitespace());
        assert_eq!(sequence, "123abc");
        assert_eq!(remainder, "");
    }

    #[test]
    fn test_parse_no_value() {
        assert!(parse("").is_empty());
    }

    #[test]
    fn test_parse_url_one_value() {
        let candidates = vec!["test-img-320w.jpg".to_string()];
        assert_eq!(parse("test-img-320w.jpg 320w"), candidates);
    }

    #[test]
    fn test_parse_srcset_two_values() {
        assert_eq!(
            parse("test-img-320w.jpg 320w, test-img-480w.jpg 480w"),
            vec![
                "test-img-320w.jpg".to_string(),
                "test-img-480w.jpg".to_string(),
            ]
        );
    }

    #[test]
    fn test_parse_srcset_with_unencoded_comma() {
        assert_eq!(
            parse(
                "/cdn-cgi/image/format=webp,width=640/https://img.youtube.com/vi/hVBl8_pgQf0/maxresdefault.jpg 640w, /cdn-cgi/image/format=webp,width=750/https://img.youtube.com/vi/hVBl8_pgQf0/maxresdefault.jpg 750w"
            ),
            vec![
                "/cdn-cgi/image/format=webp,width=640/https://img.youtube.com/vi/hVBl8_pgQf0/maxresdefault.jpg".to_string(),
                "/cdn-cgi/image/format=webp,width=750/https://img.youtube.com/vi/hVBl8_pgQf0/maxresdefault.jpg".to_string(),
            ]
        );
    }

    #[test]
    fn test_parse_srcset_url() {
        assert_eq!(
            parse("https://example.com/image1.jpg 1x, https://example.com/image2.jpg 2x"),
            vec![
                "https://example.com/image1.jpg",
                "https://example.com/image2.jpg"
            ]
        );
    }

    #[test]
    fn test_parse_srcset_with_commas() {
        assert_eq!(
            parse(
                "/cdn-cgi/image/format=webp,width=640/https://img.youtube.com/vi/hVBl8_pgQf0/maxresdefault.jpg 640w, /cdn-cgi/image/format=webp,width=750/https://img.youtube.com/vi/hVBl8_pgQf0/maxresdefault.jpg 750w"
            ),
            vec![
                "/cdn-cgi/image/format=webp,width=640/https://img.youtube.com/vi/hVBl8_pgQf0/maxresdefault.jpg",
                "/cdn-cgi/image/format=webp,width=750/https://img.youtube.com/vi/hVBl8_pgQf0/maxresdefault.jpg"
            ]
        );
    }

    #[test]
    fn test_parse_srcset_without_spaces() {
        assert_eq!(
            parse(
                "/300.png 300w,/600.png 600w,/900.png 900w,https://x.invalid/a.png 1000w,relative.png 10w"
            ),
            vec![
                "/300.png",
                "/600.png",
                "/900.png",
                "https://x.invalid/a.png",
                "relative.png"
            ]
        );
    }
}
