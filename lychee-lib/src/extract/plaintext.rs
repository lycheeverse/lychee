use std::str;

use crate::{helpers::url, types::uri::raw::RawUri, Result};

/// Extract unparsed URL strings from plaintext
pub(crate) fn extract_plaintext<T: AsRef<[u8]>>(input: T) -> Result<Vec<RawUri>> {
    // linkify only supports utf-8
    Ok(url::find_links(str::from_utf8(input.as_ref())?)
        .map(|uri| RawUri::from(uri.as_str()))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_local_links() {
        let input = "http://127.0.0.1/ and http://127.0.0.1:8888/ are local links.";
        let links: Vec<RawUri> = extract_plaintext(input).unwrap();
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

        let uris: Vec<RawUri> = extract_plaintext(input.as_bytes()).unwrap();
        assert_eq!(vec![uri], uris);
    }
}
