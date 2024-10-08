use crate::{types::uri::raw::RawUri, utils::url};

/// Extract unparsed URL strings from plaintext
pub(crate) fn extract_raw_uri_from_plaintext(input: &str) -> Vec<RawUri> {
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
        let links: Vec<RawUri> = extract_raw_uri_from_plaintext(input);
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

        let uris: Vec<RawUri> = extract_raw_uri_from_plaintext(input);
        assert_eq!(vec![uri], uris);
    }
}
