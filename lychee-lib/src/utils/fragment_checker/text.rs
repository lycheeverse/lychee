use super::parsed_fragment::TextDirective;
use crate::types::FileType;
use html5gum::{Spanned, Token, Tokenizer};
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
            regex_str.push_str(&regex::escape(&normalize_whitespace(&prefix)));
            regex_str.push_str(r"\s*");
        }

        regex_str.push_str(&regex::escape(&normalize_whitespace(&self.start)));

        if let Some(end) = &self.end {
            regex_str.push_str(".+?"); // lazy quantifier
            regex_str.push_str(&regex::escape(&normalize_whitespace(&end)));
        }

        if let Some(suffix) = &self.suffix {
            regex_str.push_str(r"\s*");
            regex_str.push_str(&regex::escape(&normalize_whitespace(&suffix)));
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
            | "progresss"
            | "video"
            | "audio"
            | "select"
    )
}

/// Extract visible text from the given HTML content.
///
/// Ensures that `Hell<i>o</i> <strong>world</strong>` is returned as a single string
/// of continuous text: `Hello world`.
///
/// This method is a good enough heuristic using html5gum. All [`Token::String`] is
/// considered visible text, except known hidden text (according to [`is_hidden_tag`]).
fn extract_visible_text(input: &str) -> String {
    let mut text: String = String::new();
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
                text.push_str(&String::from_utf8_lossy(&value));
            }

            _ => { /* Ignore other token types */ }
        }
    }

    normalize_whitespace(&text)
}

/// Normalizes whitespace in the given text by replacing adjacent (Unicode-aware)
/// whitespace characters with a single ASCII space. Leading and trailing whitespace
/// is not included in the output.
///
/// It's kind of wasteful to split and re-join the text. However, given that extra whitespace may
/// appear both inside a tag and between tags, this is a simple way to ensure that the extracted
/// text is normalized in a way that matches how browsers treat whitespace for text fragments.
///
/// # Alternatives
///
/// In space-separated langauges, it would be more efficient to consider words
/// individually (without re-joining) and use something like an [inverted index][]
/// that maps words to their indices.
///
/// [inverted index]: https://swtch.com/~rsc/regexp/regexp4.html
///
/// However, this relies on correct word boundaries and it breaks down for languages
/// without word separators. In its definition of [word boundary][], the spec says:
///
/// > Some languages do not have such a separator (notably, Chinese/Japanese/Korean).
/// > Languages such as these requires dictionaries to determine what a valid word in
/// > the given locale is.
///
/// It would be problematic to apply whitespace-based word splitting universally as,
/// for these langauges, we would detect the entire text as a single word and nothing
/// would match.
///
/// [word boundary]: https://wicg.github.io/scroll-to-text-fragment/#word-boundaries
fn normalize_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
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
        assert!(text.contains("Proin vulputate mi id sem pulvinar euismod."));
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
