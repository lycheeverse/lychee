use html5gum::{Emitter, Error, State, Tokenizer};
use std::collections::{HashMap, HashSet};

use super::{is_email_link, is_verbatim_elem, srcset};
use crate::{extract::plaintext::extract_raw_uri_from_plaintext, types::uri::raw::RawUri};

#[derive(Clone, Default, Debug)]
struct Element {
    /// Current element name being processed.
    /// This is called a tag in html5gum.
    name: String,
    /// Whether the current element is a closing tag.
    is_closing: bool,
}

/// Extract links from HTML documents.
///
/// This is the main driver for the html5gum tokenizer.
/// It implements the `Emitter` trait, which is used by the tokenizer to
/// communicate with the caller.
///
/// The `LinkExtractor` keeps track of the current element being processed,
/// the current attribute being processed, and a bunch of plain characters
/// currently being processed.
///
/// The `links` vector contains all links extracted from the HTML document and
/// the `fragments` set contains all fragments extracted from the HTML document.
#[derive(Clone, Default, Debug)]
struct LinkExtractor {
    /// Links extracted from the HTML document.
    links: Vec<RawUri>,
    /// Fragments extracted from the HTML document.
    fragments: HashSet<String>,
    /// Whether to include verbatim elements in the output.
    include_verbatim: bool,
    /// Current element being processed.
    current_element: Element,
    /// Current attributes being processed.
    /// This is a list of key-value pairs (in order of appearance), where the key is the attribute name
    /// and the value is the attribute value.
    current_attributes: HashMap<String, String>,
    /// Current attribute name being processed.
    current_attribute_name: String,
    /// A bunch of plain characters currently being processed.
    current_raw_string: String,
    /// Element name of the current verbatim block.
    /// Used to keep track of nested verbatim blocks.
    verbatim_stack: Vec<String>,
}

impl LinkExtractor {
    /// Create a new `LinkExtractor`.
    ///
    /// Set `include_verbatim` to `true` if you want to include verbatim
    /// elements in the output.
    fn new(include_verbatim: bool) -> Self {
        Self {
            include_verbatim,
            ..Default::default()
        }
    }

    /// Extract all semantically known links from a given HTML attribute.
    // For a comprehensive list of elements that might contain URLs/URIs
    // see https://www.w3.org/TR/REC-html40/index/attributes.html
    // and https://html.spec.whatwg.org/multipage/indices.html#attributes-1
    fn extract_urls_from_elem_attr(&self) -> Vec<RawUri> {
        let mut urls = Vec::new();

        // Process 'srcset' attribute first
        if let Some(srcset) = self.current_attributes.get("srcset") {
            urls.extend(srcset::parse(srcset).into_iter().map(|url| RawUri {
                text: url.to_string(),
                element: Some(self.current_element.name.clone()),
                attribute: Some("srcset".to_string()),
            }));
        }

        // Process other attributes
        for (attr_name, attr_value) in &self.current_attributes {
            #[allow(clippy::unnested_or_patterns)]
            match (self.current_element.name.as_str(), attr_name.as_str()) {
                // Common element/attribute combinations for links
                (_, "href" | "src" | "cite" | "usemap") |
                // Less common (but still valid!) combinations
                ("applet", "codebase") |
                ("body", "background") |
                ("button", "formaction") |
                ("command", "icon") |
                ("form", "action") |
                ("frame", "longdesc") |
                ("head", "profile") |
                ("html", "manifest") |
                ("iframe", "longdesc") |
                ("img", "longdesc") |
                ("input", "formaction") |
                ("object", "classid" | "codebase" | "data") |
                ("video", "poster") => {
                    urls.push(RawUri {
                        text: attr_value.to_string(),
                        element: Some(self.current_element.name.clone()),
                        attribute: Some(attr_name.to_string()),
                    });
                }
                _ => {}
            }
        }

        urls
    }

    /// Extract links from the current string and add them to the links vector.
    fn flush_current_characters(&mut self) {
        if !self.include_verbatim
            && (is_verbatim_elem(&self.current_element.name) || !self.verbatim_stack.is_empty())
        {
            self.update_verbatim_element();
            // Early return since we don't want to extract links from verbatim
            // blocks according to the configuration.
            self.current_raw_string.clear();
            return;
        }

        self.links
            .extend(extract_raw_uri_from_plaintext(&self.current_raw_string));
        self.current_raw_string.clear();
    }

    /// Update the current verbatim element name.
    ///
    /// Keeps track of the last verbatim element name, so that we can
    /// properly handle nested verbatim blocks.
    fn update_verbatim_element(&mut self) {
        if self.current_element.is_closing {
            if let Some(last_verbatim) = self.verbatim_stack.last() {
                if last_verbatim == &self.current_element.name {
                    self.verbatim_stack.pop();
                }
            }
        } else if !self.include_verbatim && is_verbatim_elem(&self.current_element.name) {
            self.verbatim_stack.push(self.current_element.name.clone());
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
    /// - If the current element has a `rel=preconnect` or `rel=dns-prefetch`
    ///   attribute, the current attribute value is ignored.
    /// - If the current attribute value is not a URL, it is treated as plain text and
    ///   added to the links vector.
    /// - If the current attribute name is `id`, the current attribute value is added
    ///   to the fragments set.
    ///
    /// The current attribute name and value are cleared after processing.
    fn flush_links(&mut self) {
        self.update_verbatim_element();

        if !self.include_verbatim
            && (!self.verbatim_stack.is_empty() || is_verbatim_elem(&self.current_element.name))
        {
            self.current_attributes.clear();
            return;
        }

        if self.current_attributes.get("rel").map_or(false, |rel| {
            rel.split(',').any(|r| {
                r.trim() == "nofollow" || r.trim() == "preconnect" || r.trim() == "dns-prefetch"
            })
        }) {
            self.current_attributes.clear();
            return;
        }

        if self.current_attributes.contains_key("prefix") {
            self.current_attributes.clear();
            return;
        }

        // Skip virtual/framework-specific stylesheet paths that start with /@ or @
        // These are typically resolved by dev servers or build tools rather than being real URLs
        // Examples: /@global/style.css, @tailwind/base.css
        if self
            .current_attributes
            .get("rel")
            .map_or(false, |rel| rel.contains("stylesheet"))
        {
            if let Some(href) = self.current_attributes.get("href") {
                if href.starts_with("/@") || href.starts_with('@') {
                    self.current_attributes.clear();
                    return;
                }
            }
        }

        let new_urls = self
            .extract_urls_from_elem_attr()
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
                let is_email = is_email_link(&url.text);
                let is_mailto = url.text.starts_with("mailto:");
                let is_phone = url.text.starts_with("tel:");
                let is_href = url.attribute.as_deref() == Some("href");

                !is_email || (is_mailto && is_href) || (is_phone && is_href)
            })
            .collect::<Vec<_>>();

        self.links.extend(new_urls);

        if let Some(id) = self.current_attributes.get("id") {
            self.fragments.insert(id.to_string());
        }

        self.current_attributes.clear();
    }
}

impl Emitter for &mut LinkExtractor {
    type Token = ();

    fn set_last_start_tag(&mut self, last_start_tag: Option<&[u8]>) {
        self.current_element.name =
            String::from_utf8_lossy(last_start_tag.unwrap_or_default()).into_owned();
    }

    fn emit_eof(&mut self) {
        self.flush_current_characters();
    }

    fn emit_error(&mut self, _: Error) {}

    fn should_emit_errors(&mut self) -> bool {
        false
    }

    fn pop_token(&mut self) -> Option<()> {
        None
    }

    /// Emit a bunch of plain characters as character tokens.
    fn emit_string(&mut self, c: &[u8]) {
        self.current_raw_string
            .push_str(&String::from_utf8_lossy(c));
    }

    fn init_start_tag(&mut self) {
        self.flush_current_characters();
        self.current_element = Element::default();
    }

    fn init_end_tag(&mut self) {
        self.flush_current_characters();
        self.current_element = Element {
            name: String::new(),
            is_closing: true,
        };
    }

    fn init_comment(&mut self) {
        self.flush_current_characters();
    }

    fn emit_current_tag(&mut self) -> Option<State> {
        self.flush_links();

        let next_state = if self.current_element.is_closing {
            None
        } else {
            html5gum::naive_next_state(self.current_element.name.as_bytes())
        };

        next_state
    }

    fn emit_current_doctype(&mut self) {}

    fn set_self_closing(&mut self) {
        self.current_element.is_closing = true;
    }

    fn set_force_quirks(&mut self) {}

    fn push_tag_name(&mut self, s: &[u8]) {
        self.current_element
            .name
            .push_str(&String::from_utf8_lossy(s));
    }

    fn push_comment(&mut self, _: &[u8]) {}

    fn push_doctype_name(&mut self, _: &[u8]) {}

    fn init_doctype(&mut self) {
        self.flush_current_characters();
    }

    fn init_attribute(&mut self) {
        self.current_attribute_name.clear();
    }

    fn push_attribute_name(&mut self, s: &[u8]) {
        self.current_attribute_name
            .push_str(&String::from_utf8_lossy(s));
    }

    fn push_attribute_value(&mut self, s: &[u8]) {
        let value = String::from_utf8_lossy(s);
        self.current_attributes
            .entry(self.current_attribute_name.clone())
            .and_modify(|v| v.push_str(&value))
            .or_insert_with(|| value.into_owned());
    }

    fn set_doctype_public_identifier(&mut self, _: &[u8]) {}

    fn set_doctype_system_identifier(&mut self, _: &[u8]) {}

    fn push_doctype_public_identifier(&mut self, _: &[u8]) {}

    fn push_doctype_system_identifier(&mut self, _: &[u8]) {}

    fn current_is_appropriate_end_tag_token(&mut self) -> bool {
        self.current_element.is_closing && !self.current_element.name.is_empty()
    }

    fn emit_current_comment(&mut self) {}
}

/// Extract unparsed URL strings from an HTML string.
pub(crate) fn extract_html(buf: &str, include_verbatim: bool) -> Vec<RawUri> {
    let mut extractor = LinkExtractor::new(include_verbatim);
    let mut tokenizer = Tokenizer::new_with_emitter(buf, &mut extractor).infallible();
    assert!(tokenizer.next().is_none());
    extractor
        .links
        .into_iter()
        .filter(|link| link.attribute.is_some() || include_verbatim)
        .collect()
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
        let input = r#"<img srcset="v2@1.5x.png" alt="Wikipedia" width="200" height="183">"#;
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

    #[test]
    fn test_skip_prefix() {
        let input = r#"
            <html lang="en-EN" prefix="og: https://ogp.me/ns#">
        "#;

        let uris = extract_html(input, false);
        assert!(uris.is_empty());
    }

    #[test]
    fn test_ignore_text_content_links() {
        let input = r#"
            <a href="https://example.com">https://ignoreme.com</a>
        "#;
        let expected = vec![RawUri {
            text: "https://example.com".to_string(),
            element: Some("a".to_string()),
            attribute: Some("href".to_string()),
        }];

        let uris = extract_html(input, false);
        assert_eq!(uris, expected);
    }

    #[test]
    fn test_skip_dns_prefetch() {
        let input = r#"
            <link rel="dns-prefetch" href="https://example.com">
        "#;

        let uris = extract_html(input, false);
        assert!(uris.is_empty());
    }

    #[test]
    fn test_skip_dns_prefetch_reverse_order() {
        let input = r#"
            <link href="https://example.com" rel="dns-prefetch">
        "#;

        let uris = extract_html(input, false);
        assert!(uris.is_empty());
    }

    #[test]
    fn test_skip_emails_in_stylesheets() {
        let input = r#"
            <link href="/@global/global.css" rel="stylesheet">
        "#;

        let uris = extract_html(input, false);
        assert!(uris.is_empty());
    }
}
