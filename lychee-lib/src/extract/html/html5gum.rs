use std::collections::HashSet;

use html5gum::{Emitter, Error, State, Tokenizer};

use super::{is_email_link, is_verbatim_elem, srcset};
use crate::{extract::plaintext::extract_raw_uri_from_plaintext, types::uri::raw::RawUri};

/// Extract links from HTML documents.
///
/// This is the main driver for the HTML5Gum tokenizer.
/// It implements the `Emitter` trait, which is used by the tokenizer to
/// communicate with the caller.
///
/// The `LinkExtractor` keeps track of the current element being processed,
/// the current attribute being processed, and a bunch of plain characters
/// currently being processed.
///
/// The `links` vector contains all links extracted from the HTML document and
/// the `fragments` set contains all fragments extracted from the HTML document.
#[derive(Clone)]
#[allow(clippy::struct_excessive_bools)]
struct LinkExtractor {
    /// Links extracted from the HTML document.
    links: Vec<RawUri>,
    /// Fragments extracted from the HTML document.
    fragments: HashSet<String>,
    /// Whether to include verbatim elements in the output.
    include_verbatim: bool,

    /// Current element being processed.
    /// This is called a tag in html5gum.
    current_element_name: Vec<u8>,
    /// Whether the current element is a closing tag.
    current_element_is_closing: bool,
    /// Whether the current element has a `rel=nofollow` attribute.
    current_element_nofollow: bool,
    /// Whether the current element has a `rel=preconnect` attribute.
    current_element_preconnect: bool,
    /// Element name of the current verbatim block.
    /// Used to keep track of nested verbatim blocks.
    current_verbatim_element_name: Option<Vec<u8>>,
    /// Last start element, i.e. the last element (tag) that was opened.
    last_start_element: Vec<u8>,

    /// Current attribute name being processed.
    current_attribute_name: Vec<u8>,
    /// Current attribute value being processed.
    current_attribute_value: Vec<u8>,

    /// A bunch of plain characters currently being processed.
    current_raw_string: Vec<u8>,
}

/// This is the same as `std::str::from_utf8_unchecked`, but with extra debug assertions for ease
/// of debugging
unsafe fn from_utf8_unchecked(s: &[u8]) -> &str {
    debug_assert!(std::str::from_utf8(s).is_ok());
    std::str::from_utf8_unchecked(s)
}

impl LinkExtractor {
    /// Create a new `LinkExtractor`.
    ///
    /// Set `include_verbatim` to `true` if you want to include verbatim
    /// elements in the output.
    pub(crate) fn new(include_verbatim: bool) -> Self {
        LinkExtractor {
            links: Vec::new(),
            fragments: HashSet::new(),
            include_verbatim,
            current_element_name: Vec::new(),
            current_element_is_closing: false,
            current_element_nofollow: false,
            current_element_preconnect: false,
            current_verbatim_element_name: None,
            last_start_element: Vec::new(),
            current_attribute_name: Vec::new(),
            current_attribute_value: Vec::new(),
            current_raw_string: Vec::new(),
        }
    }

    /// Extract all semantically known links from a given HTML attribute.
    #[allow(clippy::unnested_or_patterns)]
    pub(crate) fn extract_urls_from_elem_attr<'a>(
        elem_name: &str,
        attr_name: &str,
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
            // Extract URLs from `srcset` attributes used
            // in <img> and <source> elements.
            (_, "srcset") => {
                Some(srcset::parse(attr_value).into_iter())
            }
            // Otherwise, we don't know how to extract URLs from this
            // element/attribute combination
            _ => None,
        }
    }

    /// Extract links from the current string and add them to the links vector.
    fn flush_current_characters(&mut self) {
        // SAFETY: since we feed html5gum tokenizer with a &str, this must be a &str as well.
        let current_element_name = unsafe { from_utf8_unchecked(&self.current_element_name) };
        if !self.include_verbatim
            && (is_verbatim_elem(current_element_name) || self.inside_verbatim_block())
        {
            self.update_verbatim_element_name();
            // Early return since we don't want to extract links from verbatim
            // blocks according to the configuration.
            self.current_raw_string.clear();
            return;
        }

        let current_string = unsafe { from_utf8_unchecked(&self.current_raw_string) };
        self.links
            .extend(extract_raw_uri_from_plaintext(current_string));
        self.current_raw_string.clear();
    }

    /// Check if we are currently inside a verbatim element.
    const fn inside_verbatim_block(&self) -> bool {
        self.current_verbatim_element_name.is_some()
    }

    /// Update the current verbatim element name.
    ///
    /// Keeps track of the last verbatim element name, so that we can
    /// properly handle nested verbatim blocks.
    fn update_verbatim_element_name(&mut self) {
        if self.current_element_is_closing {
            if self.inside_verbatim_block() {
                // If we are closing a verbatim element, we need to check if it is the
                // top-level verbatim element. If it is, we reset the verbatim block element name.
                if Some(&self.current_element_name) == self.current_verbatim_element_name.as_ref() {
                    self.current_verbatim_element_name = None;
                    self.current_attribute_name.clear();
                    self.current_attribute_value.clear();
                }
            }
        } else if !self.include_verbatim
            && is_verbatim_elem(unsafe { from_utf8_unchecked(&self.current_element_name) })
        {
            // If we are opening a verbatim element, we need to check if we are already
            // inside a verbatim element. If so, we need to ignore this element.
            if !self.inside_verbatim_block() {
                self.current_verbatim_element_name = Some(self.current_element_name.clone());
            }
        }
    }

    /// Flush the current element and attribute values to the links vector.
    ///
    /// This function is called whenever a new element is encountered or when the
    /// current element is closing. It extracts URLs from the current attribute value
    /// and adds them to the links vector.
    ///
    /// Here are the rules for extracting links:
    /// - If the current element has a `rel=nofollow` attribute, the current attribute
    ///   value is ignored.
    /// - If the current element has a `rel=preconnect` attribute, the current attribute
    ///   value is ignored.
    /// - If the current attribute value is not a URL, it is treated as plain text and
    ///   added to the links vector.
    /// - If the current attribute name is `id`, the current attribute value is added
    ///   to the fragments set.
    ///
    /// The current attribute name and value are cleared after processing.
    fn flush_links(&mut self) {
        {
            // SAFETY: since we feed html5gum tokenizer with a &str, this must be a &str as well.
            let current_element_name = unsafe { from_utf8_unchecked(&self.current_element_name) };

            // Early return if we don't want to extract links from verbatim
            // blocks (like preformatted text)
            if !self.include_verbatim
                && (is_verbatim_elem(current_element_name) || self.inside_verbatim_block())
            {
                self.update_verbatim_element_name();
                return;
            }

            let current_attribute_name =
                unsafe { from_utf8_unchecked(&self.current_attribute_name) };
            let current_attribute_value =
                unsafe { from_utf8_unchecked(&self.current_attribute_value) };

            // Ignore links with rel=nofollow
            // This may be set on a different iteration on the same element/tag before,
            // so we check the boolean separately right after
            if current_attribute_name == "rel" && current_attribute_value.contains("nofollow") {
                self.current_element_nofollow = true;
            }

            // Ignore links with rel=preconnect
            // Other than prefetch and preload, `preconnect` only makes
            // a DNS lookup, so we don't want to extract those links.
            if current_attribute_name == "rel" && current_attribute_value.contains("preconnect") {
                self.current_element_preconnect = true;
            }

            if self.current_element_nofollow || self.current_element_preconnect {
                self.current_attribute_name.clear();
                self.current_attribute_value.clear();
                return;
            }

            let urls = LinkExtractor::extract_urls_from_elem_attr(
                current_element_name,
                current_attribute_name,
                current_attribute_value,
            );

            let new_urls = match urls {
                None => extract_raw_uri_from_plaintext(current_attribute_value),
                Some(urls) => urls
                    .into_iter()
                    .filter(|url| {
                        // Only accept email addresses or phone numbers, which
                        // occur in `href` attributes and start with `mailto:`
                        // or `tel:`, respectively
                        //
                        // Technically, email addresses could also occur in
                        // plain text, but we don't want to extract those
                        // because of the high false-positive rate.
                        //
                        // This skips links like `<img srcset="v2@1.5x.png">`
                        let is_email = is_email_link(url);
                        let is_mailto = url.starts_with("mailto:");
                        let is_phone = url.starts_with("tel:");
                        let is_href = current_attribute_name == "href";

                        !is_email || (is_mailto && is_href) || (is_phone && is_href)
                    })
                    .map(|url| RawUri {
                        text: url.to_string(),
                        element: Some(current_element_name.to_string()),
                        attribute: Some(current_attribute_name.to_string()),
                    })
                    .collect::<Vec<_>>(),
            };

            self.links.extend(new_urls);

            if current_attribute_name == "id" {
                self.fragments.insert(current_attribute_value.to_string());
            }
        }

        self.current_attribute_name.clear();
        self.current_attribute_value.clear();
    }
}

impl Emitter for &mut LinkExtractor {
    type Token = ();

    fn set_last_start_tag(&mut self, last_start_tag: Option<&[u8]>) {
        self.last_start_element.clear();
        self.last_start_element
            .extend(last_start_tag.unwrap_or_default());
    }

    fn emit_eof(&mut self) {
        self.flush_current_characters();
    }

    fn emit_error(&mut self, _: Error) {}

    #[inline]
    fn should_emit_errors(&mut self) -> bool {
        false
    }

    fn pop_token(&mut self) -> Option<()> {
        None
    }

    /// Emit a bunch of plain characters as character tokens.
    fn emit_string(&mut self, c: &[u8]) {
        self.current_raw_string.extend(c);
    }

    fn init_start_tag(&mut self) {
        self.flush_current_characters();
        self.current_element_name.clear();
        self.current_element_nofollow = false;
        self.current_element_preconnect = false;
        self.current_element_is_closing = false;
    }

    fn init_end_tag(&mut self) {
        self.flush_current_characters();
        self.current_element_name.clear();
        self.current_element_nofollow = false;
        self.current_element_preconnect = false;
        self.current_element_is_closing = true;
    }

    fn init_comment(&mut self) {
        self.flush_current_characters();
    }

    fn emit_current_tag(&mut self) -> Option<State> {
        let next_state = if self.current_element_is_closing {
            None
        } else {
            self.last_start_element.clear();
            self.last_start_element.extend(&self.current_element_name);
            html5gum::naive_next_state(&self.current_element_name)
        };

        // We reached the end of the tag, so we can flush the current characters
        self.flush_links();
        next_state
    }

    fn emit_current_doctype(&mut self) {}

    fn set_self_closing(&mut self) {
        self.current_element_is_closing = true;
    }

    fn set_force_quirks(&mut self) {}

    fn push_tag_name(&mut self, s: &[u8]) {
        self.current_element_name.extend(s);
    }

    fn push_comment(&mut self, _: &[u8]) {}

    fn push_doctype_name(&mut self, _: &[u8]) {}

    fn init_doctype(&mut self) {
        self.flush_current_characters();
    }

    fn init_attribute(&mut self) {
        self.flush_links();
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
        self.current_element_is_closing
            && !self.current_element_name.is_empty()
            && self.current_element_name == self.last_start_element
    }

    fn emit_current_comment(&mut self) {}
}

/// Extract unparsed URL strings from an HTML string.
pub(crate) fn extract_html(buf: &str, include_verbatim: bool) -> Vec<RawUri> {
    let mut extractor = LinkExtractor::new(include_verbatim);
    let mut tokenizer = Tokenizer::new_with_emitter(buf, &mut extractor).infallible();
    assert!(tokenizer.next().is_none());
    extractor.links
}

/// Extract fragments from id attributes within a HTML string.
pub(crate) fn extract_html_fragments(buf: &str) -> HashSet<String> {
    let mut extractor = LinkExtractor::new(true);
    let mut tokenizer = Tokenizer::new_with_emitter(buf, &mut extractor).infallible();
    assert!(tokenizer.next().is_none());
    extractor.fragments
}

#[cfg(test)]
mod tests {
    use super::*;

    const HTML_INPUT: &str = r#"
<html>
    <body id="content">
        <p>This is a paragraph with some inline <code id="inline-code">https://example.com</code> and a normal <a href="https://example.org">example</a></p>
        <pre>
        Some random text
        https://foo.com and http://bar.com/some/path
        Something else
        <a href="https://baz.org">example link inside pre</a>
        </pre>
        <p id="emphasis"><b>bold</b></p>
    </body>
</html>"#;

    #[test]
    fn test_extract_fragments() {
        let expected = HashSet::from([
            "content".to_string(),
            "inline-code".to_string(),
            "emphasis".to_string(),
        ]);
        let actual = extract_html_fragments(HTML_INPUT);
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_skip_verbatim() {
        let expected = vec![RawUri {
            text: "https://example.org".to_string(),
            element: Some("a".to_string()),
            attribute: Some("href".to_string()),
        }];

        let uris = extract_html(HTML_INPUT, false);
        assert_eq!(uris, expected);
    }

    #[test]
    fn test_include_verbatim() {
        let expected = vec![
            RawUri {
                text: "https://example.com".to_string(),
                element: None,
                attribute: None,
            },
            RawUri {
                text: "https://example.org".to_string(),
                element: Some("a".to_string()),
                attribute: Some("href".to_string()),
            },
            RawUri {
                text: "https://foo.com".to_string(),
                element: None,
                attribute: None,
            },
            RawUri {
                text: "http://bar.com/some/path".to_string(),
                element: None,
                attribute: None,
            },
            RawUri {
                text: "https://baz.org".to_string(),
                element: Some("a".to_string()),
                attribute: Some("href".to_string()),
            },
        ];

        let uris = extract_html(HTML_INPUT, true);
        assert_eq!(uris, expected);
    }

    #[test]
    fn test_include_verbatim_nested() {
        const HTML_INPUT: &str = r#"
        <a href="https://example.com/">valid link</a>
        <code>
            <pre>
                <span>https://example.org</span>
            </pre>
        </code>
        "#;

        let expected = vec![RawUri {
            text: "https://example.com/".to_string(),
            element: Some("a".to_string()),
            attribute: Some("href".to_string()),
        }];

        let uris = extract_html(HTML_INPUT, false);
        assert_eq!(uris, expected);
    }

    // TODO: This test is currently failing because we don't handle nested
    // verbatim elements of the same type correctly. The first closing tag will
    // lift the verbatim flag. This is a known issue and could be handled by
    // keeping a stack of verbatim flags.
    #[test]
    #[ignore]
    fn test_include_verbatim_nested_identical() {
        const HTML_INPUT: &str = r#"
        <pre>
            <pre>
            </pre>
            <a href="https://example.org">invalid link</a>
        </pre>
        "#;

        let uris = extract_html(HTML_INPUT, false);
        assert!(uris.is_empty());
    }

    #[test]
    fn test_exclude_nofollow() {
        let input = r#"
        <a rel="nofollow" href="https://foo.com">do not follow me</a>
        <a rel="canonical,nofollow,dns-prefetch" href="https://example.com">do not follow me</a>
        <a href="https://example.org">i'm fine</a>
        "#;
        let expected = vec![RawUri {
            text: "https://example.org".to_string(),
            element: Some("a".to_string()),
            attribute: Some("href".to_string()),
        }];
        let uris = extract_html(input, false);
        assert_eq!(uris, expected);
    }

    #[test]
    fn test_exclude_nofollow_change_order() {
        let input = r#"
        <a href="https://foo.com" rel="nofollow">do not follow me</a>
        "#;
        let uris = extract_html(input, false);
        assert!(uris.is_empty());
    }

    #[test]
    fn test_exclude_script_tags() {
        let input = r#"
        <script>
        var foo = "https://example.com";
        </script>
        <a href="https://example.org">i'm fine</a>
        "#;
        let expected = vec![RawUri {
            text: "https://example.org".to_string(),
            element: Some("a".to_string()),
            attribute: Some("href".to_string()),
        }];
        let uris = extract_html(input, false);
        assert_eq!(uris, expected);
    }

    #[test]
    fn test_valid_tel() {
        let input = r#"<!DOCTYPE html>
        <html lang="en-US">
          <head>
            <meta charset="utf-8">
            <title>Test</title>
          </head>
          <body>
            <a href="tel:1234567890">
          </body>
        </html>"#;

        let expected = vec![RawUri {
            text: "tel:1234567890".to_string(),
            element: Some("a".to_string()),
            attribute: Some("href".to_string()),
        }];
        let uris = extract_html(input, false);
        assert_eq!(uris, expected);
    }

    #[test]
    fn test_valid_email() {
        let input = r#"<!DOCTYPE html>
        <html lang="en-US">
          <head>
            <meta charset="utf-8">
            <title>Test</title>
          </head>
          <body>
            <a href="mailto:foo@bar.com">
          </body>
        </html>"#;

        let expected = vec![RawUri {
            text: "mailto:foo@bar.com".to_string(),
            element: Some("a".to_string()),
            attribute: Some("href".to_string()),
        }];
        let uris = extract_html(input, false);
        assert_eq!(uris, expected);
    }

    #[test]
    fn test_exclude_email_without_mailto() {
        let input = r#"<!DOCTYPE html>
        <html lang="en-US">
          <head>
            <meta charset="utf-8">
            <title>Test</title>
          </head>
          <body>
            <a href="foo@bar.com">
          </body>
        </html>"#;

        let uris = extract_html(input, false);
        assert!(uris.is_empty());
    }

    #[test]
    fn test_email_false_positive() {
        let input = r#"<!DOCTYPE html>
        <html lang="en-US">
          <head>
            <meta charset="utf-8">
            <title>Test</title>
          </head>
          <body>
            <img srcset="v2@1.5x.png" alt="Wikipedia" width="200" height="183">
          </body>
        </html>"#;

        let uris = extract_html(input, false);
        assert!(uris.is_empty());
    }

    #[test]
    fn test_extract_srcset() {
        let input = r#"
            <img srcset="/cdn-cgi/image/format=webp,width=640/https://img.youtube.com/vi/hVBl8_pgQf0/maxresdefault.jpg 640w, /cdn-cgi/image/format=webp,width=750/https://img.youtube.com/vi/hVBl8_pgQf0/maxresdefault.jpg 750w" src="/cdn-cgi/image/format=webp,width=3840/https://img.youtube.com/vi/hVBl8_pgQf0/maxresdefault.jpg">
        "#;

        let expected = vec![RawUri {
            text: "/cdn-cgi/image/format=webp,width=640/https://img.youtube.com/vi/hVBl8_pgQf0/maxresdefault.jpg".to_string(),
            element: Some("img".to_string()),
            attribute: Some("srcset".to_string()),
        },
        RawUri {
            text: "/cdn-cgi/image/format=webp,width=750/https://img.youtube.com/vi/hVBl8_pgQf0/maxresdefault.jpg".to_string(),
            element: Some("img".to_string()),
            attribute: Some("srcset".to_string()),
        },
        RawUri {
            text: "/cdn-cgi/image/format=webp,width=3840/https://img.youtube.com/vi/hVBl8_pgQf0/maxresdefault.jpg".to_string(),
            element: Some("img".to_string()),
            attribute: Some("src".to_string()),
        }

        ];
        let uris = extract_html(input, false);
        assert_eq!(uris, expected);
    }

    #[test]
    fn test_skip_preconnect() {
        let input = r#"
            <link rel="preconnect" href="https://example.com">
        "#;

        let uris = extract_html(input, false);
        assert!(uris.is_empty());
    }

    #[test]
    fn test_skip_preconnect_reverse_order() {
        let input = r#"
            <link href="https://example.com" rel="preconnect">
        "#;

        let uris = extract_html(input, false);
        assert!(uris.is_empty());
    }
}
