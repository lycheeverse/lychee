use crate::{
    types::uri::raw::{RawUri, SpanProvider},
    utils::url,
};

/// Extract unparsed URL strings from plaintext
pub(crate) fn extract_raw_uri_from_plaintext(
    input: &str,
    span_provider: &impl SpanProvider,
) -> Vec<RawUri> {
    url::find_links(input)
        .map(|uri| RawUri {
            text: uri.as_str().to_owned(),
            element: None,
            attribute: None,
            span: span_provider.span(uri.start()),
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
}
