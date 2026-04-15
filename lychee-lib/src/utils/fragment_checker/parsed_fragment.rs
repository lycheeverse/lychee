use url::Url;

#[derive(Debug, PartialEq, Eq)]
pub(super) struct ParsedFragment<'a> {
    // The element ID part of the fragment, e.g., "section" in `https://example.com/#section:~:text=example`.
    pub(super) element_id: Option<&'a str>,
    // The raw value of the text directive, e.g., "The%20concept%20of-,end%2Duser,-first%20surfaced%20in" in `https://en.wikipedia.org/wiki/End_user#:~:text=The%20concept%20of-,end%2Duser,-first%20surfaced%20in`.
    // Dashes and commas have special meaning in the text directive, so we need to keep them percentage-encoded.
    // Full parsing of the text directive value into its components (prefix, start, end, suffix) is done later in the `TextDirective` struct.
    // See https://wicg.github.io/scroll-to-text-fragment/#syntax
    pub(super) encoded_text_directive_value: Option<String>,
}

const FRAGMENT_DIRECTIVE_DELIMITER: &str = ":~:";
const TEXT_DIRECTIVE_KEY: &str = "text";

impl<'a> ParsedFragment<'a> {
    /// This method does top-level parsing of the fragment, separating the element id (if any) from the text directive (if any).
    pub(super) fn parse(url: &'a Url) -> Self {
        let Some(fragment) = url.fragment() else {
            return Self {
                element_id: None,
                encoded_text_directive_value: None,
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
                encoded_text_directive_value: None,
            };
        };

        let element_id = (!element_id.is_empty()).then_some(element_id);

        // The fragment directive may contain several components, separated by ampersant, such as https://example.com#:~:text=foo&text=bar&unknownDirective
        // We do not URL decode the text directive value yet, because comma and dashes have special meaning and need to be percentage encoded.
        // See Example 6 in https://wicg.github.io/scroll-to-text-fragment/#the-fragment-directive
        for (key, value) in fragment_directive
            .split('&')
            .filter_map(|part| part.split_once('='))
        {
            // The standard allows several directives, including serveral text directives. We only support the first text directive, and ignore other directives.
            // See https://wicg.github.io/scroll-to-text-fragment/#text-directives
            if key == TEXT_DIRECTIVE_KEY {
                return Self {
                    element_id,
                    encoded_text_directive_value: Some(value.to_owned()),
                };
            }
        }

        Self {
            element_id,
            encoded_text_directive_value: None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ParsedFragment;
    use url::Url;

    #[test]
    fn parses_pure_text_fragment_directive() {
        let url = Url::parse("https://example.com/#:~:unknown&text=needle").unwrap();

        let parsed = ParsedFragment::parse(&url);

        assert_eq!(
            parsed,
            ParsedFragment {
                element_id: None,
                encoded_text_directive_value: Some("needle".to_string()),
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
                encoded_text_directive_value: Some("needle".to_string()),
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
                encoded_text_directive_value: None,
            }
        );
    }

    #[test]
    fn parses_text_directive_with_encoded_values() {
        let url = Url::parse("https://en.wikipedia.org/wiki/End_user#:~:unknown&text=The%20concept%20of-,end%2Duser,-first%20surfaced%20in&unknown&text=ignored-in-lychee").unwrap();
        let parsed = ParsedFragment::parse(&url);

        assert_eq!(
            parsed,
            ParsedFragment {
                element_id: None,
                encoded_text_directive_value: Some(
                    "The%20concept%20of-,end%2Duser,-first%20surfaced%20in".to_string()
                ),
            }
        );
    }
}
