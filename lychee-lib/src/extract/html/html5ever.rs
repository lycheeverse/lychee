use std::cell::RefCell;

use html5ever::{
    buffer_queue::BufferQueue,
    tendril::StrTendril,
    tokenizer::{Tag, TagKind, Token, TokenSink, TokenSinkResult, Tokenizer, TokenizerOpts},
};

use super::{
    super::plaintext::extract_raw_uri_from_plaintext, is_email_link, is_verbatim_elem, srcset,
};
use crate::types::uri::raw::RawUri;

#[derive(Clone, Default)]
struct LinkExtractor {
    links: RefCell<Vec<RawUri>>,
    include_verbatim: bool,
    current_verbatim_element_name: RefCell<Option<String>>,
}

impl TokenSink for LinkExtractor {
    type Handle = ();

    #[allow(clippy::match_same_arms)]
    fn process_token(&self, token: Token, _line_number: u64) -> TokenSinkResult<()> {
        match token {
            Token::CharacterTokens(raw) => {
                if self.current_verbatim_element_name.borrow().is_some() {
                    return TokenSinkResult::Continue;
                }
                if self.include_verbatim {
                    self.links
                        .borrow_mut()
                        .extend(extract_raw_uri_from_plaintext(&raw));
                }
            }
            Token::TagToken(tag) => {
                let Tag {
                    kind,
                    name,
                    self_closing: _self_closing,
                    attrs,
                } = tag;
                // Check if this is a verbatim element, which we want to skip.
                if !self.include_verbatim && is_verbatim_elem(&name) {
                    // Check if we're currently inside a verbatim block
                    let mut curr_verbatim_elem = self.current_verbatim_element_name.borrow_mut();

                    if curr_verbatim_elem.is_some() {
                        // Inside a verbatim block. Check if the verbatim
                        // element name matches with the current element name.
                        if curr_verbatim_elem.as_ref() == Some(&name.to_string()) {
                            // If so, we're done with the verbatim block,
                            // -- but only if this is an end tag.
                            if matches!(kind, TagKind::EndTag) {
                                *curr_verbatim_elem = None;
                            }
                        }
                    } else if matches!(kind, TagKind::StartTag) {
                        // We're not inside a verbatim block, but we just
                        // encountered a verbatim element. Remember the name
                        // of the element.
                        *curr_verbatim_elem = Some(name.to_string());
                    }
                }
                if self.current_verbatim_element_name.borrow().is_some() {
                    // We want to skip the content of this element
                    // as we're inside a verbatim block.
                    return TokenSinkResult::Continue;
                }

                // Check for rel=nofollow. We only extract the first `rel` attribute.
                // This is correct as per https://html.spec.whatwg.org/multipage/syntax.html#attributes-0, which states
                // "There must never be two or more attributes on the same start tag whose names are an ASCII case-insensitive match for each other."
                if let Some(rel) = attrs.iter().find(|attr| &attr.name.local == "rel") {
                    if rel.value.contains("nofollow") {
                        return TokenSinkResult::Continue;
                    }
                }

                // Check and exclude `rel=preconnect` and `rel=dns-prefetch`. Unlike `prefetch` and `preload`,
                // `preconnect` and `dns-prefetch` only perform DNS lookups and do not necessarily link to a resource
                if let Some(rel) = attrs.iter().find(|attr| &attr.name.local == "rel") {
                    if rel.value.contains("preconnect") || rel.value.contains("dns-prefetch") {
                        return TokenSinkResult::Continue;
                    }
                }

                // Check and exclude `prefix` attribute. This attribute is used to define a prefix
                // for the current element. It is not used to link to a resource.
                if let Some(_prefix) = attrs.iter().find(|attr| &attr.name.local == "prefix") {
                    return TokenSinkResult::Continue;
                }

                for attr in &attrs {
                    let urls = LinkExtractor::extract_urls_from_elem_attr(
                        &attr.name.local,
                        &name,
                        &attr.value,
                    );

                    let new_urls = match urls {
                        None => extract_raw_uri_from_plaintext(&attr.value),
                        Some(urls) => urls
                            .into_iter()
                            .filter(|url| {
                                // Only accept email addresses which
                                // - occur in `href` attributes
                                // - start with `mailto:`
                                //
                                // Technically, email addresses could
                                // also occur in plain text, but we don't want to extract those
                                // because of the high false positive rate.
                                //
                                // This ignores links like `<img srcset="v2@1.5x.png">`
                                let is_email = is_email_link(url);
                                let is_mailto = url.starts_with("mailto:");
                                let is_phone = url.starts_with("tel:");
                                let is_href = attr.name.local.as_ref() == "href";

                                if attrs.iter().any(|attr| {
                                    &attr.name.local == "rel" && attr.value.contains("stylesheet")
                                }) {
                                    // Skip virtual/framework-specific stylesheet paths that start with /@ or @
                                    // These are typically resolved by dev servers or build tools rather than being real URLs
                                    // Examples: /@global/style.css, @tailwind/base.css as in
                                    // `<link href="/@global/style.css" rel="stylesheet">`
                                    if url.starts_with("/@") || url.starts_with('@') {
                                        return false;
                                    }
                                }

                                !is_email || (is_mailto && is_href) || (is_phone && is_href)
                            })
                            .map(|url| RawUri {
                                text: url.to_string(),
                                element: Some(name.to_string()),
                                attribute: Some(attr.name.local.to_string()),
                            })
                            .collect::<Vec<_>>(),
                    };
                    self.links.borrow_mut().extend(new_urls);
                }
            }
            Token::ParseError(_err) => {
                // Silently ignore parse errors
            }
            Token::CommentToken(_raw) => (),
            Token::NullCharacterToken => (),
            Token::DoctypeToken(_doctype) => (),
            Token::EOFToken => (),
        }
        TokenSinkResult::Continue
    }
}

impl LinkExtractor {
    pub(crate) const fn new(include_verbatim: bool) -> Self {
        Self {
            links: RefCell::new(Vec::new()),
            include_verbatim,
            current_verbatim_element_name: RefCell::new(None),
        }
    }

    /// Extract all semantically known links from a given HTML attribute.
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
                Some(srcset::parse(attr_value).into_iter())
            }
            _ => None,
        }
    }
}

/// Extract unparsed URL strings from an HTML string.
pub(crate) fn extract_html(buf: &str, include_verbatim: bool) -> Vec<RawUri> {
    let input = BufferQueue::default();
    input.push_back(StrTendril::from(buf));

    let tokenizer = Tokenizer::new(
        LinkExtractor::new(include_verbatim),
        TokenizerOpts::default(),
    );
    let _handle = tokenizer.feed(&input);
    tokenizer.end();

    tokenizer.sink.links.into_inner()
}

#[cfg(test)]
mod tests {
    use super::*;

    const HTML_INPUT: &str = r#"
<html>
    <body>
        <p>This is a paragraph with some inline <code>https://example.com</code> and a normal <a href="https://example.org">example</a></p>
        <pre>
        Some random text
        https://foo.com and http://bar.com/some/path
        Something else
        <a href="https://baz.org">example link inside pre</a>
        </pre>
        <p><b>bold</b></p>
    </body>
</html>"#;

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
    fn test_include_verbatim_recursive() {
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

    #[test]
    fn test_include_nofollow() {
        let input = r#"
        <a rel="nofollow" href="https://foo.com">do not follow me</a>
        <a rel="canonical,nofollow,dns-prefetch" href="https://example.com">do not follow me</a>
        <a href="https://example.org">do not follow me</a>
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

        let expected = vec![];
        let uris = extract_html(input, false);
        assert_eq!(uris, expected);
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

        let expected = vec![];
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
