use html5ever::{
    buffer_queue::BufferQueue,
    tendril::StrTendril,
    tokenizer::{Tag, TagKind, Token, TokenSink, TokenSinkResult, Tokenizer, TokenizerOpts},
};

use super::{is_verbatim_elem, plaintext::extract_plaintext};
use crate::types::uri::raw::RawUri;

#[derive(Clone, Default)]
struct LinkExtractor {
    links: Vec<RawUri>,
    include_verbatim: bool,
    inside_excluded_element: bool,
}

impl TokenSink for LinkExtractor {
    type Handle = ();

    #[allow(clippy::match_same_arms)]
    fn process_token(&mut self, token: Token, _line_number: u64) -> TokenSinkResult<()> {
        match token {
            Token::CharacterTokens(raw) => {
                if self.inside_excluded_element {
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
                if !self.include_verbatim && is_verbatim_elem(&name) {
                    // Skip content inside excluded elements until we see the end tag.
                    self.inside_excluded_element = matches!(kind, TagKind::StartTag);
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
            inside_excluded_element: false,
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
}
