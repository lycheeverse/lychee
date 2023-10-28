use html5ever::{
    buffer_queue::BufferQueue,
    tendril::StrTendril,
    tokenizer::{Tag, TagKind, Token, TokenSink, TokenSinkResult, Tokenizer, TokenizerOpts},
};

use super::{super::plaintext::extract_plaintext, is_email_link, is_verbatim_elem, srcset};
use crate::types::uri::raw::RawUri;

#[derive(Clone, Default)]
struct LinkExtractor {
    links: Vec<RawUri>,
    include_verbatim: bool,
    current_verbatim_element_name: Option<String>,
}

impl TokenSink for LinkExtractor {
    type Handle = ();

    #[allow(clippy::match_same_arms)]
    fn process_token(&mut self, token: Token, _line_number: u64) -> TokenSinkResult<()> {
        match token {
            Token::CharacterTokens(raw) => {
                if self.current_verbatim_element_name.is_some() {
                    return TokenSinkResult::Continue;
                }
                self.links.extend(extract_plaintext(&raw));
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
                    if let Some(current_verbatim_element_name) = &self.current_verbatim_element_name
                    {
                        // Inside a verbatim block. Check if the verbatim
                        // element name matches with the current element name.
                        if current_verbatim_element_name == name.as_ref() {
                            // If so, we're done with the verbatim block,
                            // -- but only if this is an end tag.
                            if matches!(kind, TagKind::EndTag) {
                                self.current_verbatim_element_name = None;
                            }
                        }
                    } else if matches!(kind, TagKind::StartTag) {
                        // We're not inside a verbatim block, but we just
                        // encountered a verbatim element. Remember the name
                        // of the element.
                        self.current_verbatim_element_name = Some(name.to_string());
                    }
                }
                if self.current_verbatim_element_name.is_some() {
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

                // Check and exclude rel=preconnect. Other than prefetch and preload,
                // preconnect only does DNS lookups and might not be a link to a resource
                if let Some(rel) = attrs.iter().find(|attr| &attr.name.local == "rel") {
                    if rel.value.contains("preconnect") {
                        return TokenSinkResult::Continue;
                    }
                }

                for attr in attrs {
                    let urls = LinkExtractor::extract_urls_from_elem_attr(
                        &attr.name.local,
                        &name,
                        &attr.value,
                    );

                    let new_urls = match urls {
                        None => extract_plaintext(&attr.value),
                        Some(urls) => urls
                            .into_iter()
                            .filter(|url| {
                                // Only accept email addresses, which occur in `href` attributes
                                // and start with `mailto:`. Technically, email addresses could
                                // also occur in plain text, but we don't want to extract those
                                // because of the high false positive rate.
                                //
                                // This ignores links like `<img srcset="v2@1.5x.png">`
                                let is_email = is_email_link(url);
                                let is_mailto = url.starts_with("mailto:");
                                let is_href = attr.name.local.as_ref() == "href";

                                !is_email || (is_mailto && is_href)
                            })
                            .map(|url| RawUri {
                                text: url.to_string(),
                                element: Some(name.to_string()),
                                attribute: Some(attr.name.local.to_string()),
                            })
                            .collect::<Vec<_>>(),
                    };
                    self.links.extend(new_urls);
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
            links: vec![],
            include_verbatim,
            current_verbatim_element_name: None,
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
                Some(srcset::parse(attr_value).into_iter())
            }
            _ => None,
        }
    }
}

/// Extract unparsed URL strings from an HTML string.
pub(crate) fn extract_html(buf: &str, include_verbatim: bool) -> Vec<RawUri> {
    let mut input = BufferQueue::new();
    input.push_back(StrTendril::from(buf));

    let mut tokenizer = Tokenizer::new(
        LinkExtractor::new(include_verbatim),
        TokenizerOpts::default(),
    );
    let _handle = tokenizer.feed(&mut input);
    tokenizer.end();

    tokenizer.sink.links
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
}
