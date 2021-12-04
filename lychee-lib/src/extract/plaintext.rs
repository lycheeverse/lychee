use crate::{
    helpers::url,
    types::raw_uri::{RawUri, UriKind},
};

/// Extract unparsed URL strings from plaintext
// Allow &self here for consistency with the other extractors
// Links in plaintext always get treated as strict
// as there are no hidden elements in text files
pub(crate) fn extract_plaintext(input: &str) -> Vec<RawUri> {
    url::find_links(input)
        .map(|uri| RawUri {
            text: uri.as_str().to_owned(),
            kind: UriKind::Strict,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_link_at_end_of_line() {
        let input = "https://www.apache.org/licenses/LICENSE-2.0\n";
        let uri = RawUri {
            text: input.trim_end().to_string(),
            kind: UriKind::Unknown,
        };

        let uris: Vec<RawUri> = extract_plaintext(input);
        assert_eq!(vec![uri], uris);
    }
}
