use html5gum::{
    Spanned, Tokenizer,
    emitters::callback::{Callback, CallbackEmitter, CallbackEvent},
};
use std::collections::{HashMap, HashSet};

use super::{is_email_link, is_verbatim_elem, srcset};
use crate::{
    extract::plaintext::extract_raw_uri_from_plaintext,
    types::uri::raw::{OffsetSpanProvider, RawUri, SourceSpanProvider, SpanProvider},
};

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
#[derive(Clone, Debug)]
struct LinkExtractor<S: SpanProvider> {
    /// The [`SpanProvider`] which will be used to compute spans for URIs.
    ///
    /// This is generic, since e.g. the markdown parser has already started, so we have to compute
    /// the span location in relation to the offset in the outer document.
    span_provider: S,
    /// Links extracted from the HTML document.
    links: Vec<RawUri>,
    /// Fragments extracted from the HTML document.
    fragments: HashSet<String>,
    /// Whether to include verbatim elements in the output.
    include_verbatim: bool,
    /// Current element name being processed.
    /// This is called a tag in html5gum.
    current_element: String,
    /// Current attributes being processed.
    /// This is a list of key-value pairs (in order of appearance), where the key is the attribute name
    /// and the value is the attribute value.
    current_attributes: HashMap<String, Spanned<String>>,
    /// Current attribute name being processed.
    current_attribute_name: String,
    /// Element name of the current verbatim block.
    /// Used to keep track of nested verbatim blocks.
    verbatim_stack: Vec<String>,
}

impl<S: SpanProvider> LinkExtractor<S> {
    /// Create a new `LinkExtractor`.
    ///
    /// Set `include_verbatim` to `true` if you want to include verbatim
    /// elements in the output.
    fn new(span_provider: S, include_verbatim: bool) -> Self {
        Self {
            span_provider,
            include_verbatim,
            links: Vec::default(),
            fragments: HashSet::default(),
            current_element: String::default(),
            current_attributes: HashMap::default(),
            current_attribute_name: String::default(),
            verbatim_stack: Vec::default(),
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
            let span = srcset.span;
            urls.extend(srcset::parse(srcset).into_iter().map(|url| RawUri {
                text: url.to_string(),
                element: Some(self.current_element.clone()),
                attribute: Some("srcset".to_string()),
                span: self.span_provider.span(span.start),
            }));
        }

        // Process other attributes
        for (attr_name, attr_value) in &self.current_attributes {
            #[allow(clippy::unnested_or_patterns)]
            match (self.current_element.as_str(), attr_name.as_str()) {
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
                        element: Some(self.current_element.clone()),
                        attribute: Some(attr_name.clone()),
                        span: self.span_provider.span(attr_value.span.start),
                    });
                }
                _ => {}
            }
        }

        urls
    }

    fn filter_verbatim_here(&self) -> bool {
        !self.include_verbatim
            && (is_verbatim_elem(&self.current_element) || !self.verbatim_stack.is_empty())
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
        if self.filter_verbatim_here() {
            self.current_attributes.clear();
            return;
        }

        if self.current_attributes.get("rel").is_some_and(|rel| {
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
            .is_some_and(|rel| rel.contains("stylesheet"))
        {
            if let Some(href) = self.current_attributes.get("href")
                && (href.starts_with("/@") || href.starts_with('@'))
            {
                self.current_attributes.clear();
                return;
            }
            // Skip disabled stylesheets
            // Ref: https://developer.mozilla.org/en-US/docs/Web/API/HTMLLinkElement/disabled
            if self.current_attribute_name == "disabled"
                || self.current_attributes.contains_key("disabled")
            {
                self.current_attributes.clear();
                return;
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

        // Also check for 'name' attributes for backward compatibility with older HTML
        // standards. In HTML 4.01, both id and name could be used. This is not valid HTML 5,
        // but it's still used by some widely deployed tools, for example:
        //
        // - JavaDoc - Oracle's tool generates <a name="anchor"> for method signatures and classes
        //   (see https://docs.oracle.com/javase/8/docs/technotes/tools/windows/javadoc.html)
        // - Doxygen - C++ documentation generator supports <A NAME="..."> in HTML commands
        //   (see https://www.doxygen.nl/manual/htmlcmds.html)
        //
        // See https://developer.mozilla.org/en-US/docs/Web/HTML/Reference/Elements/a#name
        // See https://stackoverflow.com/a/484781
        if let Some(name) = self.current_attributes.get("name") {
            self.fragments.insert(name.to_string());
        }

        self.current_attributes.clear();
    }
}

impl<S: SpanProvider> Callback<(), usize> for &mut LinkExtractor<S> {
    fn handle_event(
        &mut self,
        event: CallbackEvent<'_>,
        span: html5gum::Span<usize>,
    ) -> Option<()> {
        match event {
            CallbackEvent::OpenStartTag { name } => {
                self.current_element = String::from_utf8_lossy(name).into_owned();

                // Update the current verbatim element name.
                //
                // Keeps track of the last verbatim element name, so that we can
                // properly handle nested verbatim blocks.
                if self.filter_verbatim_here() && is_verbatim_elem(&self.current_element) {
                    self.verbatim_stack.push(self.current_element.clone());
                }
            }
            CallbackEvent::AttributeName { name } => {
                self.current_attribute_name = String::from_utf8_lossy(name).into_owned();
            }
            CallbackEvent::AttributeValue { value } => {
                let value = String::from_utf8_lossy(value);
                self.current_attributes
                    .entry(self.current_attribute_name.clone())
                    .and_modify(|v| v.push_str(&value))
                    .or_insert_with(|| Spanned {
                        value: value.into_owned(),
                        span,
                    });
            }
            CallbackEvent::CloseStartTag { self_closing } => {
                self.flush_links();

                // Update the current verbatim element name.
                //
                // Keeps track of the last verbatim element name, so that we can
                // properly handle nested verbatim blocks.
                if self_closing
                    && self.filter_verbatim_here()
                    && let Some(last_verbatim) = self.verbatim_stack.last()
                    && last_verbatim == &self.current_element
                {
                    self.verbatim_stack.pop();
                }
            }
            CallbackEvent::EndTag { name } => {
                let tag_name = String::from_utf8_lossy(name);
                // Update the current verbatim element name.
                //
                // Keeps track of the last verbatim element name, so that we can
                // properly handle nested verbatim blocks.
                if !self.include_verbatim
                    && let Some(last_verbatim) = self.verbatim_stack.last()
                    && last_verbatim == tag_name.as_ref()
                {
                    self.verbatim_stack.pop();
                }
            }
            CallbackEvent::String { value } => {
                if !self.filter_verbatim_here() {
                    // Extract links from the current string and add them to the links vector.
                    self.links.extend(extract_raw_uri_from_plaintext(
                        &String::from_utf8_lossy(value),
                        &OffsetSpanProvider {
                            offset: span.start,
                            inner: &self.span_provider,
                        },
                    ));
                }
            }
            CallbackEvent::Comment { .. }
            | CallbackEvent::Doctype { .. }
            | CallbackEvent::Error(_) => {}
        }
        None
    }
}

/// Extract unparsed URL strings from an HTML string.
pub(crate) fn extract_html(buf: &str, include_verbatim: bool) -> Vec<RawUri> {
    extract_html_with_span(buf, include_verbatim, SourceSpanProvider::from_input(buf))
}

pub(crate) fn extract_html_with_span<S: SpanProvider>(
    buf: &str,
    include_verbatim: bool,
    span_provider: S,
) -> Vec<RawUri> {
    let mut extractor = LinkExtractor::new(span_provider, include_verbatim);
    let mut tokenizer = Tokenizer::new_with_emitter(buf, CallbackEmitter::new(&mut extractor));
    assert!(tokenizer.next().is_none());
    extractor
        .links
        .into_iter()
        .filter(|link| link.attribute.is_some() || include_verbatim)
        .collect()
}

/// Extract fragments from id attributes within a HTML string.
pub(crate) fn extract_html_fragments(buf: &str) -> HashSet<String> {
    let span_provider = SourceSpanProvider::from_input(buf);
    let mut extractor = LinkExtractor::new(span_provider, true);
    let mut tokenizer = Tokenizer::new_with_emitter(buf, CallbackEmitter::new(&mut extractor));
    assert!(tokenizer.next().is_none());
    extractor.fragments
}

#[cfg(test)]
mod tests {
    use crate::types::uri::raw::span;

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
            span: span(4, 121),
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
                span: span(4, 72),
            },
            RawUri {
                text: "https://example.org".to_string(),
                element: Some("a".to_string()),
                attribute: Some("href".to_string()),
                span: span(4, 121),
            },
            RawUri {
                text: "https://foo.com".to_string(),
                element: None,
                attribute: None,
                span: span(7, 9),
            },
            RawUri {
                text: "http://bar.com/some/path".to_string(),
                element: None,
                attribute: None,
                span: span(7, 29),
            },
            RawUri {
                text: "https://baz.org".to_string(),
                element: Some("a".to_string()),
                attribute: Some("href".to_string()),
                span: span(9, 18),
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
            span: span(2, 18),
        }];

        let uris = extract_html(HTML_INPUT, false);
        assert_eq!(uris, expected);
    }

    #[test]
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
            span: span(4, 18),
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
            span: span(5, 18),
        }];
        let uris = extract_html(input, false);
        assert_eq!(uris, expected);
    }

    #[test]
    fn test_exclude_disabled_stylesheet() {
        let input = r#"
        <link rel="stylesheet" href="https://disabled.com" disabled>
        <link rel="stylesheet" href="https://disabled.com" disabled="disabled">
        <a href="https://example.org">i'm fine</a>
        "#;
        let expected = vec![RawUri {
            text: "https://example.org".to_string(),
            element: Some("a".to_string()),
            attribute: Some("href".to_string()),
            span: span(4, 18),
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
            span: span(8, 22),
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
            span: span(8, 22),
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
            span: span(2, 26),
        },
        RawUri {
            text: "/cdn-cgi/image/format=webp,width=750/https://img.youtube.com/vi/hVBl8_pgQf0/maxresdefault.jpg".to_string(),
            element: Some("img".to_string()),
            attribute: Some("srcset".to_string()),
            span: span(2, 26),
        },
        RawUri {
            text: "/cdn-cgi/image/format=webp,width=3840/https://img.youtube.com/vi/hVBl8_pgQf0/maxresdefault.jpg".to_string(),
            element: Some("img".to_string()),
            attribute: Some("src".to_string()),
            span: span(2, 231),
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
            span: span(2, 22),
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

    #[test]
    fn test_extract_fragments_with_name_attributes() {
        // Test for JavaDoc-style name attributes used for anchors
        let input = r#"
        <html>
        <body>
            <h1 id="title">Title</h1>
            <a name="skip.navbar.top"></a>
            <a name="method.summary"></a>
            <div>
                <a name="clear--"></a>
                <h2 id="section">Section</h2>
                <a name="method.detail"></a>
            </div>
            <a name="skip.navbar.bottom"></a>
        </body>
        </html>
        "#;

        let expected = HashSet::from([
            "title".to_string(),
            "section".to_string(),
            "skip.navbar.top".to_string(),
            "method.summary".to_string(),
            "clear--".to_string(),
            "method.detail".to_string(),
            "skip.navbar.bottom".to_string(),
        ]);
        let actual = extract_html_fragments(input);
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_extract_links_after_empty_verbatim_block() {
        // Test that links are correctly extracted after empty <pre><code> blocks
        let input = r#"
        <body>
            <div>
                See <a href="https://example.com/1">First</a>
            </div>
            <pre>
                <code></code>
            </pre>
            <div>
                See <a href="https://example.com/2">Second</a>
            </div>
        </body>
        "#;

        let expected = vec![
            RawUri {
                text: "https://example.com/1".to_string(),
                element: Some("a".to_string()),
                attribute: Some("href".to_string()),
                span: span(4, 30),
            },
            RawUri {
                text: "https://example.com/2".to_string(),
                element: Some("a".to_string()),
                attribute: Some("href".to_string()),
                span: span(10, 30),
            },
        ];

        let uris = extract_html(input, false);
        assert_eq!(uris, expected);
    }
}
