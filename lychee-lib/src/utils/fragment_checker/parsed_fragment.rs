use url::Url;

#[derive(Debug, PartialEq, Eq)]
pub(super) struct ParsedFragment<'a> {
    /// The element ID part of the fragment, e.g., "section" in `https://example.com/#section:~:text=example`.
    pub(super) element_id: Option<&'a str>,
    /// The text directives part of the fragment, e.g., "text=example" in `https://example.com/#section:~:text=example`.
    pub(super) text_directives: Vec<TextDirective>,
}

#[derive(Debug, PartialEq, Eq)]
pub(super) struct TextDirective {
    pub(super) prefix: Option<String>,
    pub(super) start: String,
    pub(super) end: Option<String>,
    pub(super) suffix: Option<String>,
}

const FRAGMENT_DIRECTIVE_DELIMITER: &str = ":~:";
const TEXT_DIRECTIVE_KEY: &str = "text";

impl Default for ParsedFragment<'_> {
    /// By default, there is no fragment.
    fn default() -> Self {
        Self {
            element_id: None,
            text_directives: Vec::new(),
        }
    }
}

impl<'a> ParsedFragment<'a> {
    /// This method parses the fragment, separating the element id (if any) from the text directives (if any).
    pub(super) fn parse(url: &'a Url) -> Self {
        let Some(fragment) = url.fragment() else {
            return Self::default();
        };

        // Split off the element id from the fragment directive
        // See https://wicg.github.io/scroll-to-text-fragment/#the-fragment-directive
        // See https://wicg.github.io/scroll-to-text-fragment/#determine-if-fragment-id-is-needed
        let Some((element_id, fragment_directive)) =
            fragment.split_once(FRAGMENT_DIRECTIVE_DELIMITER)
        else {
            return Self {
                element_id: Some(fragment),
                text_directives: Vec::new(),
            };
        };

        // Convert empty element id to None.
        let element_id = (!element_id.is_empty()).then_some(element_id);

        // The fragment directive may contain several components, separated by ampersant, such as https://example.com#:~:text=foo&text=bar&unknownDirective
        // See Example 6 in https://wicg.github.io/scroll-to-text-fragment/#the-fragment-directive
        let text_directives = fragment_directive
            .split('&')
            .filter_map(|component| component.split_once('='))
            .filter(|(key, _)| *key == TEXT_DIRECTIVE_KEY)
            .filter_map(|(_, value)| TextDirective::parse(value))
            .collect();

        Self {
            element_id,
            text_directives,
        }
    }
}

impl TextDirective {
    /// Helper function to strip a prefix from the start of the text directive parts, if present and correctly formatted.
    fn strip_prefix(parts: &mut Vec<&str>) -> Option<String> {
        if parts.len() >= 2 && parts.first().is_some_and(|p| p.ends_with('-')) {
            let part = parts.remove(0);
            Some(percentage_decode(&part[..part.len() - 1]))
        } else {
            None
        }
    }

    /// Helper function to strip a suffix from the end of the text directive parts, if present and correctly formatted.
    fn strip_suffix(parts: &mut Vec<&str>) -> Option<String> {
        if parts.len() >= 2 && parts.last().is_some_and(|p| p.starts_with('-')) {
            let part = parts.pop().expect("checked length above");
            Some(percentage_decode(&part[1..]))
        } else {
            None
        }
    }

    /// Parse a [text directive] from a string.
    ///
    /// Text directives follow the format:
    ///
    /// ```text
    /// prefix-,start,end,-suffix
    /// ```
    ///
    /// where `prefix-` and `-suffix` are optional context anchors and `end` is an
    /// optional range end. Only `start` is required.
    ///
    /// Returns `None` if the input is empty or contains more than two parts
    /// after stripping the optional prefix and suffix anchors.
    ///
    /// [text directive]: https://wicg.github.io/scroll-to-text-fragment/#the-text-directive
    fn parse(input: &str) -> Option<Self> {
        if input.is_empty() {
            return None;
        }

        let mut parts: Vec<&str> = input.split(',').collect();
        // Split over an empty string should yield `Vec<[""]>`, but I feel a strong need to check if parts is empty first.
        if parts.is_empty() || parts[0].is_empty() {
            return None;
        }

        let prefix = Self::strip_prefix(&mut parts);
        let suffix = Self::strip_suffix(&mut parts);

        let (start, end) = match parts.as_slice() {
            [start] => (percentage_decode(start), None),
            [start, end] => (percentage_decode(start), Some(percentage_decode(end))),
            _ => return None,
        };

        Some(Self {
            prefix,
            start,
            end,
            suffix,
        })
    }
}

fn percentage_decode(input: &str) -> String {
    use percent_encoding::percent_decode_str;
    percent_decode_str(input).decode_utf8_lossy().into_owned()
}

#[cfg(test)]
mod tests {
    use super::{ParsedFragment, TextDirective};
    use rstest::rstest;
    use url::Url;

    #[rstest]
    #[case(vec!["prefix-", "start", "-suffix"], Some("prefix".to_string()), vec!["start", "-suffix"])] // Prefix present
    #[case(vec!["start", "end"], None, vec!["start", "end"])] // No prefix
    #[case(vec!["prefix-"], None, vec!["prefix-"])] // Too short, no prefix
    #[case(vec!["-prefix","start", "end", "-suffix"], None, vec!["-prefix", "start", "end", "-suffix"])] // Incorrect prefix format
    fn test_strip_prefix(
        #[case] mut input_parts: Vec<&str>,
        #[case] expected_return: Option<String>,
        #[case] expected_remaining: Vec<&str>,
    ) {
        let result = TextDirective::strip_prefix(&mut input_parts);
        assert_eq!(result, expected_return);
        assert_eq!(input_parts, expected_remaining);
    }

    #[rstest]
    #[case(vec!["start", "-suffix"], Some("suffix".to_string()), vec!["start"])] // Suffix present
    #[case(vec!["start", "end"], None, vec!["start", "end"])] // No suffix
    #[case(vec!["-suffix"], None, vec!["-suffix"])] // Too short, no suffix
    #[case(vec!["start", "end", "suffix-"], None, vec!["start", "end", "suffix-"])] // Incorrect suffix format
    fn test_strip_suffix(
        #[case] mut input_parts: Vec<&str>,
        #[case] expected_return: Option<String>,
        #[case] expected_remaining: Vec<&str>,
    ) {
        let result = TextDirective::strip_suffix(&mut input_parts);
        assert_eq!(result, expected_return);
        assert_eq!(input_parts, expected_remaining);
    }

    #[test]
    fn parses_pure_text_fragment_directive() {
        let url = Url::parse("https://example.com/#:~:unknown&text=needle").unwrap();

        let parsed = ParsedFragment::parse(&url);

        assert_eq!(
            parsed,
            ParsedFragment {
                element_id: None,
                text_directives: vec![TextDirective {
                    prefix: None,
                    start: "needle".to_string(),
                    end: None,
                    suffix: None,
                }],
            }
        );
    }

    #[test]
    fn parses_element_fragment_before_text_directive() {
        let url = Url::parse("https://example.com/#section:~:text=needle&unknown").unwrap();

        let parsed = ParsedFragment::parse(&url);

        assert_eq!(
            parsed,
            ParsedFragment {
                element_id: Some("section"),
                text_directives: vec![TextDirective {
                    prefix: None,
                    start: "needle".to_string(),
                    end: None,
                    suffix: None,
                }],
            }
        );
    }

    #[test]
    fn parses_plain_element_fragment() {
        let url = Url::parse("https://example.com/#section").unwrap();

        let parsed = ParsedFragment::parse(&url);

        assert_eq!(
            parsed,
            ParsedFragment {
                element_id: Some("section"),
                text_directives: Vec::new(),
            }
        );
    }

    #[test]
    fn parses_all_text_directives_with_encoded_values() {
        let url = Url::parse("https://en.wikipedia.org/wiki/End_user#:~:unknown&text=The%20concept%20of-,end%2Duser,-first%20surfaced%20in&unknown&text=second%20text%20directive").unwrap();
        let parsed = ParsedFragment::parse(&url);

        assert_eq!(
            parsed,
            ParsedFragment {
                element_id: None,
                text_directives: vec![
                    TextDirective {
                        prefix: Some("The concept of".to_string()),
                        start: "end-user".to_string(),
                        end: None,
                        suffix: Some("first surfaced in".to_string()),
                    },
                    TextDirective {
                        prefix: None,
                        start: "second text directive".to_string(),
                        end: None,
                        suffix: None,
                    }
                ],
            }
        );
    }

    #[test]
    fn parses_text_directive_with_prefix_and_suffix() {
        let url = Url::parse(
            "https://example.com/#:~:text=consectetur%20adipiscing%20elit.-,Sed%20porta,-nisl%20sit%20amet",
        )
        .unwrap();

        let parsed = ParsedFragment::parse(&url);

        assert_eq!(
            parsed.text_directives,
            vec![TextDirective {
                prefix: Some("consectetur adipiscing elit.".into()),
                start: "Sed porta".into(),
                end: None,
                suffix: Some("nisl sit amet".into()),
            }]
        );
    }

    #[test]
    fn parses_text_directive_with_empty_values() {
        let url = Url::parse("https://example.com/#:~:text=").unwrap();
        let parsed = ParsedFragment::parse(&url);

        assert_eq!(parsed.text_directives, vec![],);
    }

    #[test]
    /// This test checks that percent-encoded non-breaking space (NBSP) in the text directive is correctly decoded.
    fn parses_text_directive_with_encoded_utf8() {
        const NBSP: &str = "\u{a0}";
        const NBSP_ENCODED: &str = "%C2%A0";

        let url = Url::parse(&format!(
            "http://127.0.0.1:8000/a.html#:~:text=b{NBSP_ENCODED}cd"
        ))
        .unwrap();
        let parsed = ParsedFragment::parse(&url);

        assert_eq!(
            parsed.text_directives,
            vec![TextDirective {
                prefix: None,
                start: format!("b{NBSP}cd"),
                end: None,
                suffix: None
            }]
        );
    }
}
