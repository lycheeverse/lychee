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

impl<'a> ParsedFragment<'a> {
    /// This method parses the fragment, separating the element id (if any) from the text directives (if any).
    pub(super) fn parse(url: &'a Url) -> Self {
        let Some(fragment) = url.fragment() else {
            return Self {
                element_id: None,
                text_directives: Vec::new(),
            };
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

        let element_id = (!element_id.is_empty()).then_some(element_id);

        // The fragment directive may contain several components, separated by ampersant, such as https://example.com#:~:text=foo&text=bar&unknownDirective
        // See Example 6 in https://wicg.github.io/scroll-to-text-fragment/#the-fragment-directive
        let text_directives = url::form_urlencoded::parse(fragment_directive.as_bytes())
            .filter(|(key, _)| key == TEXT_DIRECTIVE_KEY)
            .filter_map(|(_, value)| TextDirective::parse(value.as_ref()))
            .collect();

        Self {
            element_id,
            text_directives,
        }
    }
}

impl TextDirective {
    fn parse(input: &str) -> Option<Self> {
        let mut parts: Vec<&str> = input.split(',').collect();
        if parts.is_empty() {
            return None;
        }

        let prefix = if parts.first().is_some_and(|part| part.ends_with('-')) && parts.len() >= 2 {
            let prefix = parts.remove(0);
            Some(normalize_whitespace(&prefix[..prefix.len() - 1]))
        } else {
            None
        };

        let suffix = if parts.last().is_some_and(|part| part.starts_with('-')) && parts.len() >= 2 {
            let suffix = parts.pop().expect("checked length above");
            Some(normalize_whitespace(&suffix[1..]))
        } else {
            None
        };

        let [start] = parts.as_slice() else {
            let [start, end] = parts.as_slice() else {
                return None;
            };

            return Some(Self {
                prefix,
                start: normalize_whitespace(start),
                end: Some(normalize_whitespace(end)),
                suffix,
            });
        };

        Some(Self {
            prefix,
            start: normalize_whitespace(start),
            end: None,
            suffix,
        })
    }
}

fn normalize_whitespace(input: &str) -> String {
    input.split_whitespace().collect::<Vec<_>>().join(" ")
}

#[cfg(test)]
mod tests {
    use super::{ParsedFragment, TextDirective};
    use url::Url;

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
}
