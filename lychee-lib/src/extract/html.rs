use html5gum::{Emitter, Error, Tokenizer};

use super::plaintext::extract_plaintext;
use crate::types::raw_uri::RawUri;

#[derive(Clone)]
struct LinkExtractor {
    links: Vec<RawUri>,
    current_string: Vec<u8>,
    current_tag_name: Vec<u8>,
    current_tag_is_closing: bool,
    current_attribute_name: Vec<u8>,
    current_attribute_value: Vec<u8>,
    last_start_tag: Vec<u8>,
}

impl LinkExtractor {
    pub(crate) const fn new() -> Self {
        LinkExtractor {
            links: Vec::new(),
            current_string: Vec::new(),
            current_tag_name: Vec::new(),
            current_tag_is_closing: false,
            current_attribute_name: Vec::new(),
            current_attribute_value: Vec::new(),
            last_start_tag: Vec::new(),
        }
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

    fn flush_current_characters(&mut self) {
        // this won't panic as long as the original input was valid utf8
        let raw = std::str::from_utf8(&self.current_string).unwrap();
        self.links.extend(extract_plaintext(raw));
        self.current_string.clear();
    }

    fn flush_old_attribute(&mut self) {
        {
            // none of those will panic as long as the original input was valid utf8
            let name = std::str::from_utf8(&self.current_tag_name).unwrap();
            let attr = std::str::from_utf8(&self.current_attribute_name).unwrap();
            let value = std::str::from_utf8(&self.current_attribute_value).unwrap();

            let urls = LinkExtractor::extract_urls_from_elem_attr(&attr, &name, &value);

            let new_urls = match urls {
                None => extract_plaintext(&value),
                Some(urls) => urls
                    .into_iter()
                    .map(|url| RawUri {
                        text: url.to_string(),
                        element: Some(name.to_string()),
                        attribute: Some(attr.to_string()),
                    })
                    .collect::<Vec<_>>(),
            };

            self.links.extend(new_urls);
        }

        self.current_attribute_name.clear();
        self.current_attribute_value.clear();
    }
}

impl Emitter for &mut LinkExtractor {
    type Token = ();

    fn set_last_start_tag(&mut self, last_start_tag: Option<&[u8]>) {
        self.last_start_tag.clear();
        self.last_start_tag
            .extend(last_start_tag.unwrap_or_default());
    }

    fn emit_eof(&mut self) {
        self.flush_current_characters();
    }
    fn emit_error(&mut self, _: Error) {}
    fn pop_token(&mut self) -> Option<()> {
        None
    }

    fn emit_string(&mut self, c: &[u8]) {
        self.current_string.extend(c);
    }

    fn init_start_tag(&mut self) {
        self.flush_current_characters();
        self.current_tag_name.clear();
        self.current_tag_is_closing = false;
    }

    fn init_end_tag(&mut self) {
        self.flush_current_characters();
        self.current_tag_name.clear();
        self.current_tag_is_closing = true;
    }

    fn init_comment(&mut self) {
        self.flush_current_characters();
    }

    fn emit_current_tag(&mut self) {
        self.flush_old_attribute();
    }

    fn emit_current_doctype(&mut self) {}
    fn set_self_closing(&mut self) {
        self.current_tag_is_closing = true;
    }
    fn set_force_quirks(&mut self) {}

    fn push_tag_name(&mut self, s: &[u8]) {
        self.current_tag_name.extend(s);
    }

    fn push_comment(&mut self, _: &[u8]) {}
    fn push_doctype_name(&mut self, _: &[u8]) {}
    fn init_doctype(&mut self) {
        self.flush_current_characters();
    }
    fn init_attribute(&mut self) {
        self.flush_old_attribute();
    }
    fn push_attribute_name(&mut self, s: &[u8]) {
        self.current_attribute_name.extend(s);
    }
    fn push_attribute_value(&mut self, s: &[u8]) {
        self.current_attribute_value.extend(s);
    }

    fn set_doctype_public_identifier(&mut self, _: &[u8]) {}
    fn set_doctype_system_identifier(&mut self, _: &[u8]) {}
    fn push_doctype_public_identifier(&mut self, _: &[u8]) {}
    fn push_doctype_system_identifier(&mut self, _: &[u8]) {}
    fn current_is_appropriate_end_tag_token(&mut self) -> bool {
        self.current_tag_is_closing
            && !self.current_tag_name.is_empty()
            && self.current_tag_name == self.last_start_tag
    }

    fn emit_current_comment(&mut self) {}
}

/// Extract unparsed URL strings from an HTML string.
pub(crate) fn extract_html(buf: &str) -> Vec<RawUri> {
    let mut extractor = LinkExtractor::new();
    let mut tokenizer = Tokenizer::new_with_emitter(buf, &mut extractor).infallible();
    assert!(tokenizer.next().is_none());
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
