use crate::{helpers::url, types::raw_uri::RawUri};

/// Shortest valid URI that lychee extracts from plaintext.
///
/// The shortest valid URI without a scheme might be g.cn (Google China)
/// At least I am not aware of a shorter one. We set this as a lower threshold
/// for parsing URIs from plaintext to avoid false-positives and as a slight
/// performance optimization, which could add up for big files.
/// This threshold might be adjusted in the future.
const MIN_URI_LENGTH: usize = 4;

/// Extract unparsed URL strings from plaintext
pub(crate) fn extract_plaintext(input: &str) -> Vec<RawUri> {
    if input.len() < MIN_URI_LENGTH {
        // Immediately return for very small strings which cannot be valid URIs
        return vec![];
    }

    url::find_links(input)
        .map(|uri| RawUri::from(uri.as_str()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_local_links() {
        let input = "http://127.0.0.1/ and http://127.0.0.1:8888/ are local links.";
        let links: Vec<RawUri> = extract_plaintext(input);
        assert_eq!(
            links,
            [
                RawUri::from("http://127.0.0.1/"),
                RawUri::from("http://127.0.0.1:8888/")
            ]
        );
    }

    #[test]
    fn test_extract_link_at_end_of_line() {
        let input = "https://www.apache.org/licenses/LICENSE-2.0\n";
        let uri = RawUri::from(input.trim_end());

        let uris: Vec<RawUri> = extract_plaintext(input);
        assert_eq!(vec![uri], uris);
    }
}
