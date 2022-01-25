use html5gum::{Tokenizer, Token};

use super::plaintext::extract_plaintext;
use crate::types::raw_uri::RawUri;

#[derive(Clone)]
struct LinkExtractor {
    links: Vec<RawUri>,
}

impl LinkExtractor {
    pub(crate) const fn new() -> Self {
        Self { links: Vec::new() }
    }

    /// Extract all semantically known links from a given html attribute.
    #[allow(clippy::unnested_or_patterns)]
    pub(crate) fn extract_urls_from_elem_attr<'a>(
        attr_name: &str,
        elem_name: &str,
        attr_value: &'a str,
    ) -> Option<impl Iterator<Item = &'a str>> {
        // For a comprehensive list of elements that might contain URLs/URIs
        // see https://www.w3.org/TR/REC-html40/index/attributes.html
        // and https://html.spec.whatwg.org/multipage/indices.html#attributes-1
        match (elem_name, attr_name) {
            // Common element/attribute combinations for links
            (_, "href" | "src" | "cite" | "usemap")
            // Less common (but still valid!) combinations
            | ("applet", "codebase")
            | ("body", "background")
            | ("button", "formaction")
            | ("command", "icon")
            | ("form", "action")
            | ("frame", "longdesc")
            | ("head", "profile")
            | ("html", "manifest")
            | ("iframe", "longdesc")
            | ("img", "longdesc")
            | ("input", "formaction")
            | ("object", "classid")
            | ("object", "codebase")
            | ("object", "data")
            | ("video", "poster") => {
                Some(vec![attr_value].into_iter())
            }
            (_, "srcset") => {
                let mut urls = Vec::new();
                for image_candidate_string in attr_value.trim().split(',') {
                    for part in image_candidate_string.split_ascii_whitespace() {
                        if part.is_empty() {
                            continue;
                        }
                        urls.push(part);
                        break;
                    }
                }
                Some(urls.into_iter())
            }
            _ => None,
        }
    }

    pub(crate) fn run(&mut self, input: &str) {
        for token in Tokenizer::new(input).infallible() {
            match token {
                Token::StartTag(tag) => {
                    for (attr, value) in tag.attributes {
                        let urls = LinkExtractor::extract_urls_from_elem_attr(
                            &attr,
                            &tag.name,
                            &value
                        );

                        let new_urls = match urls {
                            None => extract_plaintext(&value),
                            Some(urls) => urls
                                .into_iter()
                                .map(|url| RawUri {
                                    text: url.to_string(),
                                    element: Some(tag.name.to_string()),
                                    attribute: Some(attr.to_string()),
                                })
                                .collect::<Vec<_>>(),
                        };
                        self.links.extend(new_urls);
                    }
                }
                Token::EndTag(_) => (),
                Token::String(raw) => self.links.extend(extract_plaintext(&raw)),
                Token::Comment(_) => (),
                Token::Doctype(_) => (),
                Token::Error(_) => (),
            }
        }
    }
}

/// Extract unparsed URL strings from an HTML string.
pub(crate) fn extract_html(buf: &str) -> Vec<RawUri> {
    let mut extractor = LinkExtractor::new();
    extractor.run(buf);
    extractor.links
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_link_at_end_of_line() {
        let input = "https://www.apache.org/licenses/LICENSE-2.0\n";
        let link = input.trim_end();

        let uris: Vec<String> = extract_html(input)
            .into_iter()
            .map(|raw_uri| raw_uri.text)
            .collect();
        assert_eq!(vec![link.to_string()], uris);
    }
}
