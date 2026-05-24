use crate::{
    types::uri::raw::{RawUri, SpanProvider},
    utils::url,
};

/// Fullwidth and CJK punctuation that delimits a URL in prose but is not part
/// of the URL itself. The underlying link finder treats these as URL
/// characters, so an extracted link is truncated at the first occurrence.
const DELIMITER_PUNCTUATION: &[char] = &[
    '\u{3001}', // ideographic comma `、`
    '\u{3002}', // ideographic full stop `。`
    '\u{FF0C}', // fullwidth comma `，`
    '\u{FF0E}', // fullwidth full stop `．`
    '\u{FF01}', // fullwidth exclamation mark `！`
    '\u{FF1F}', // fullwidth question mark `？`
    '\u{FF1B}', // fullwidth semicolon `；`
    '\u{FF1A}', // fullwidth colon `：`
];

/// Extract unparsed URL strings from plaintext
pub(crate) fn extract_raw_uri_from_plaintext(
    input: &str,
    span_provider: &impl SpanProvider,
) -> Vec<RawUri> {
    url::find_links(input)
        .map(|uri| {
            let text = uri.as_str();
            let end = text.find(DELIMITER_PUNCTUATION).unwrap_or(text.len());
            RawUri {
                text: text[..end].to_owned(),
                element: None,
                attribute: None,
                span: span_provider.span(uri.start()),
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use crate::types::uri::raw::{SourceSpanProvider, span};

    use super::*;

    fn extract(input: &str) -> Vec<RawUri> {
        extract_raw_uri_from_plaintext(input, &SourceSpanProvider::from_input(input))
    }

    #[test]
    fn test_extract_local_links() {
        let input = "http://127.0.0.1/ and http://127.0.0.1:8888/ are local links.";
        let links: Vec<RawUri> = extract(input);
        assert_eq!(
            links,
            [
                RawUri::from(("http://127.0.0.1/", span(1, 1))),
                RawUri::from(("http://127.0.0.1:8888/", span(1, 23),)),
            ]
        );
    }

    #[test]
    fn test_extract_link_at_end_of_line() {
        let input = "https://www.apache.org/licenses/LICENSE-2.0\n";
        let uri = RawUri::from((input.trim_end(), span(1, 1)));

        let uris: Vec<RawUri> = extract(input);
        assert_eq!(vec![uri], uris);
    }

    #[test]
    fn test_fullwidth_punctuation_delimits_link() {
        // The link finder treats fullwidth/CJK delimiters as URL characters, so
        // a link must be truncated at the first `，`, `。` or `、`.
        let cases = [
            ("see https://example.com\u{FF0C}next", 5),
            ("a https://example.com\u{3002} b", 3),
            ("x https://example.com\u{3001}y", 3),
        ];
        for (input, column) in cases {
            let links: Vec<RawUri> = extract(input);
            assert_eq!(
                links,
                [RawUri::from(("https://example.com", span(1, column)))],
                "input={input:?}"
            );
        }
    }
}
