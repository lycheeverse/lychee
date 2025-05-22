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

enum State {
    InsideDescriptor,
    AfterDescriptor,
    InsideParens,
}

/// Split an input string at the first character for which
/// the predicate returns false.
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
    let mut index = 0;

    while index < input.len() {
        let position = &input[index..];
        let (start, remaining) = split_at(position, |c| *c == ',' || c.is_whitespace());

        if start.find(',').is_some() {
            info!("srcset parse Error");
            return vec![];
        }
        index += start.chars().count();

        if remaining.is_empty() {
            return candidates;
        }

        let (url, remaining) = split_at(remaining, |c| !c.is_whitespace());
        index += url.chars().count();

        let comma_count = url.chars().rev().take_while(|c| *c == ',').count();

        if let Some(url) = url.get(..url.len() - comma_count) {
            candidates.push(url);
        }

        if comma_count > 1 {
            info!("srcset parse error (trailing commas)");
            return vec![];
        }

        index += 1;

        let (space, remaining) = split_at(remaining, |c| c.is_whitespace());
        index += space.len();

        index = skip_descriptor(index, remaining);
    }

    candidates
}

/// Helper function to skip over a descriptor.
/// Returns the index of the next character after the descriptor
/// (i.e. pointing at the comma or the end of the string)
fn skip_descriptor(mut index: usize, remaining: &str) -> usize {
    let mut state = State::InsideDescriptor;

    for c in remaining.chars() {
        index += 1;

        match state {
            State::InsideDescriptor => match c {
                ' ' => state = State::AfterDescriptor,
                '(' => state = State::InsideParens,
                ',' => return index,
                _ => {}
            },
            State::InsideParens => {
                if c == ')' {
                    state = State::InsideDescriptor;
                }
            }
            State::AfterDescriptor => {
                if c != ' ' {
                    state = State::InsideDescriptor;
                }
            }
        }
    }

    index
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
}
