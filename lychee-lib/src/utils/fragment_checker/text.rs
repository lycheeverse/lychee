use std::iter;

use super::parsed_fragment::TextDirective;
use crate::types::FileType;
use html5gum::{Token, Tokenizer};
use log::warn;
use regex::Regex;

/// Check if the text fragments in the given URL are valid for the provided content and file type.
pub(super) fn check_text_fragments(
    directives: &[TextDirective],
    content: &str,
    file_type: FileType,
) -> bool {
    if file_type != FileType::Html {
        // We currently don't support text fragments for e.g. Markdown files,
        // because realistically there is no standard for them.
        // If text fragments ever become standardized for non-HTML files, we can implement support for them here.
        return true;
    }

    if directives.is_empty() {
        return true;
    }

    // The algorithm to find a range in a document likely requires a full implementation of a browser.
    // See https://wicg.github.io/scroll-to-text-fragment/#finding-ranges-in-a-document
    // Here, we try to approximate it by extracting visible text.
    let document = extract_visible_text(content);
    directives
        .iter()
        .all(|directive| directive.matches(&document))
}

impl TextDirective {
    /// Builds a regex to find the current [`TextDirective`] within normalised
    /// HTML text content.
    ///
    /// # Errors
    ///
    /// The regex built is always syntactically correct, so errors are rare. Errors
    /// could happen if the text directive size exceeds the regex size limit.
    fn to_regex(&self) -> Result<Regex, regex::Error> {
        let mut regex_str = String::new();

        if let Some(prefix) = &self.prefix {
            regex_str.push_str(&regex::escape(&normalize_whitespace(prefix.trim())));
            regex_str.push_str(r"\s*");
        }

        regex_str.push_str(&regex::escape(&normalize_whitespace(self.start.trim())));

        if let Some(end) = &self.end {
            regex_str.push_str(".+?"); // lazy quantifier
            regex_str.push_str(&regex::escape(&normalize_whitespace(end.trim())));
        }

        if let Some(suffix) = &self.suffix {
            regex_str.push_str(r"\s*");
            regex_str.push_str(&regex::escape(&normalize_whitespace(suffix.trim())));
        }

        Regex::new(&regex_str)
    }

    fn matches(&self, document: &str) -> bool {
        match self.to_regex() {
            Ok(regex) => regex.is_match(document),
            Err(e) => {
                warn!("Failed to create regex for text fragment {self:?}. {e:?}");
                false
            }
        }
    }
}

/// Returns whether text within the given HTML tag is hidden from text fragment searching.
///
/// This is loosely based on (and is a superset of) the spec's [search invisible][] elements.
/// Technically, the spec also calls for computing the CSS `display` property, but we do not
/// do that here.
///
/// [search invisible]: https://wicg.github.io/scroll-to-text-fragment/#search-invisible
fn is_hidden_tag(name: &str) -> bool {
    matches!(
        name,
        "head"
            | "script"
            | "style"
            | "template"
            | "iframe"
            | "img"
            | "meter"
            | "object"
            | "progress" // https://developer.mozilla.org/en-US/docs/Web/HTML/Reference/Elements/progress
            | "video"
            | "audio"
            | "select"
    )
}

/// Extract visible text from the given HTML content. Whitespace is normalized by
/// replacing adjacent (Unicode-aware) whitespace characters with a single ASCII
/// space.
///
/// Ensures that `Hell<i>o</i> <strong>world</strong>` is returned as a single string
/// of continuous text: `Hello world`.
///
/// This method is a good enough heuristic using html5gum. All [`Token::String`] is
/// considered visible text, except known hidden text (according to [`is_hidden_tag`]).
fn extract_visible_text(input: &str) -> String {
    /// Pushes a space if not already ending in a space.
    fn push_space(text: &mut String) {
        if !text.ends_with(char::is_whitespace) {
            text.push(' ');
        }
    }

    let mut text = String::new();
    let mut hidden_stack: Vec<String> = Vec::new();

    for Ok(token) in Tokenizer::new(input) {
        match token {
            Token::StartTag(tag) => {
                let tag_name = String::from_utf8_lossy(&tag.name).into_owned();
                if is_hidden_tag(&tag_name) {
                    hidden_stack.push(tag_name);
                }
            }
            Token::EndTag(tag) => {
                let tag_name = String::from_utf8_lossy(&tag.name).into_owned();
                if hidden_stack.last().is_some_and(|last| last == &tag_name) {
                    hidden_stack.pop();
                }
            }
            Token::String(value) if hidden_stack.is_empty() => {
                let string = String::from_utf8_lossy(&value);
                if string.starts_with(char::is_whitespace) {
                    push_space(&mut text);
                }

                text.extend(intersperse_whitespace(&string));

                if string.ends_with(char::is_whitespace) {
                    push_space(&mut text);
                }
            }

            _ => { /* Ignore other token types */ }
        }
    }

    text
}

/// Returns an iterator of whitespace-separated words in the given string, interspersed
/// with exactly one ASCII space between each word. Leading or trailing whitespace
/// is discarded.
fn intersperse_whitespace(text: &str) -> impl Iterator<Item = &str> {
    text.split_whitespace().enumerate().flat_map(|(i, word)| {
        let space: Option<&str> = (i > 0).then_some(" ");
        space.into_iter().chain(iter::once(word))
    })
}

/// Normalizes whitespace in the given text by replacing adjacent (Unicode-aware)
/// whitespace characters with a single ASCII space. Leading and trailing whitespace
/// is removed in the output.
fn normalize_whitespace(text: &str) -> String {
    intersperse_whitespace(text).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::fragment_checker::parsed_fragment::ParsedFragment;
    use url::Url;

    const INDEX_HTML: &str = include_str!("../../../../fixtures/text_fragments/index.html");

    #[test]
    fn extracts_visible_text_without_style_or_attributes() {
        let text = extract_visible_text(INDEX_HTML);

        assert!(text.contains("Sed porta nisl sit amet quam ornare rutrum."));
        assert!(
            text.contains("Proin vulputate mi id sem pulvinar euismod."),
            "{}",
            text
        );
        assert!(!text.contains("my-style-property"));
        assert!(!text.contains("my-element-attribute-value"));
    }

    #[test]
    fn extract_visible_text_capital_tags() {
        assert!(!extract_visible_text("<STYLE> inside </STYLE>").contains("inside"));
        assert!(!extract_visible_text("<STYLE> inside </style>").contains("inside"));
        assert!(extract_visible_text("<STYLE> inside </style> after").contains("after"));
        assert!(extract_visible_text("<style> inside </STYLE> after").contains("after"));
    }

    #[test]
    fn extract_visible_text_whitespace_implied_by_adjacent_tags() {
        assert!(
            extract_visible_text("a<span>b</span>").contains("ab"),
            "span is an inline tag, so should /not/ create whitespace"
        );
    }

    #[test]
    fn extract_visible_text_alternative_whitespaces() {
        assert!(
            extract_visible_text("a\n\t           b").contains("a b"),
            "all spaces should be collapsed"
        );
        assert!(
            extract_visible_text("a&nbsp;b").contains("a b"),
            "encoded &nbsp; space should be interpreted as space"
        );
        assert!(
            extract_visible_text("a\u{00A0}b").contains("a b"),
            "inline nbsp should also be interpreted as space"
        );
    }

    #[test]
    fn check_text_fragments_from_chrome() {
        // This list was generated by using the "Copy link to highlight" feature in Chrome Version 147.0.7727.55 (Official Build) (64-bit)
        let url =
            Url::parse("http://localhost:8080/#:~:text=Proin-,vulputate,-mi%20id%20sem").unwrap();
        let parsed = ParsedFragment::parse(&url);
        assert!(check_text_fragments(
            &parsed.text_directives,
            INDEX_HTML,
            FileType::Html
        ));

        let url =
            Url::parse("http://localhost:8080/#:~:text=Proin-,vulputate,-mi%20id%20sema").unwrap();
        let parsed = ParsedFragment::parse(&url);
        assert!(!check_text_fragments(
            &parsed.text_directives,
            INDEX_HTML,
            FileType::Html
        ));

        let url = Url::parse("http://localhost:8080/#:~:text=massa.,Proin").unwrap();
        let parsed = ParsedFragment::parse(&url);
        assert!(check_text_fragments(
            &parsed.text_directives,
            INDEX_HTML,
            FileType::Html
        ));

        let url =
            Url::parse("http://localhost:8080/#:~:text=sit%20amet%20dignissim-,massa,-.").unwrap();
        let parsed = ParsedFragment::parse(&url);
        assert!(check_text_fragments(
            &parsed.text_directives,
            INDEX_HTML,
            FileType::Html
        ));

        let url =
            Url::parse("http://localhost:8080/#:~:text=sit%20amet%20dignissim-,massa,-.").unwrap();
        let parsed = ParsedFragment::parse(&url);
        assert!(check_text_fragments(
            &parsed.text_directives,
            INDEX_HTML,
            FileType::Html
        ));

        let url =
            Url::parse("http://localhost:8080/#:~:text=sit%20amet%20dignissim-,massam,-.").unwrap();
        let parsed = ParsedFragment::parse(&url);
        assert!(!check_text_fragments(
            &parsed.text_directives,
            INDEX_HTML,
            FileType::Html
        ));
    }

    #[test]
    fn check_text_fragments_alternative_whitespaces() {
        // chrome/firefox will generate this from html with nbsp.
        let url = Url::parse("http://127.0.0.1:8000/a.html#:~:text=b%C2%A0cd").unwrap();
        let parsed = ParsedFragment::parse(&url);
        assert!(
            check_text_fragments(&parsed.text_directives, "b\u{00a0}cd", FileType::Html),
            "percent encoded nbsp in fragment should be decoded"
        );

        // chrome/firefox don't generate this, but they do highlight it correctly when it's
        // typed in manually.
        let url = Url::parse("http://127.0.0.1:8000/a.html#:~:text=b%20cd").unwrap();
        let parsed = ParsedFragment::parse(&url);
        assert!(
            check_text_fragments(&parsed.text_directives, "b\u{00a0}cd", FileType::Html),
            "%20 space in fragment should match any space in the text"
        );
    }

    #[test]
    fn check_text_fragments_prefix_and_suffix() {
        let url = Url::parse(
            "https://en.wikipedia.org/wiki/Most_common_words_in_English#:~:text=in-,the,the,-texts",
        )
        .unwrap();
        let parsed = ParsedFragment::parse(&url);

        let html =
            "<p>written      in     the English language.</p>\n\n<p>In total, the      texts</p>";
        assert!(
            check_text_fragments(&parsed.text_directives, html, FileType::Html),
            "whitespace should be skipped between the match and prefix/suffix"
        );

        let html = "<p>in</p><p>the English language.</p>\n<p>In total, the</p><p>texts</p>";
        assert!(
            check_text_fragments(&parsed.text_directives, html, FileType::Html),
            "prefix/suffix should match across block tags"
        );
    }

    /// Prefix and suffix should be used to disambiguate when start/end occur multiple times
    /// in the HTML. In these test cases, the expected valid highlight range is indicated in
    /// `[ ... ]`.
    #[test]
    fn check_text_fragments_multiple_occurrences() {
        let url =
            Url::parse("https://en.wikipedia.org/wiki/#:~:text=prefix-,start,end,-suffix").unwrap();
        let parsed = ParsedFragment::parse(&url);
        let html = "start [ prefix start end suffix ]";
        assert!(
            check_text_fragments(&parsed.text_directives, html, FileType::Html),
            "should work with multiple occurrences of start, only one has prefix"
        );

        let html = "[ prefix start end end suffix ]";
        assert!(
            check_text_fragments(&parsed.text_directives, html, FileType::Html),
            "should work with multiple occurrences of end, only one has suffix"
        );

        let html = "start [ prefix start end end suffix ]";
        assert!(
            check_text_fragments(&parsed.text_directives, html, FileType::Html),
            "should work with multiple occurrences of both start and end"
        );
    }
}
