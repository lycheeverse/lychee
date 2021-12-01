use html5ever::tendril::StrTendril;

use crate::helpers::url;

/// Extract unparsed URL strings from plaintext
// Allow &self here for consistency with the other extractors
pub(crate) fn extract_plaintext(input: &str) -> Vec<StrTendril> {
    url::find_links(input)
        .map(|l| StrTendril::from(l.as_str()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_link_at_end_of_line() {
        let input = "https://www.apache.org/licenses/LICENSE-2.0\n";
        let link = input.trim_end();

        let urls = extract_plaintext(input);
        assert_eq!(vec![StrTendril::from(link)], urls);
    }
}
