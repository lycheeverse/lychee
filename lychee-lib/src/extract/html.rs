use html5ever::{
    buffer_queue::BufferQueue,
    tendril::StrTendril,
    tokenizer::{Tag, Token, TokenSink, TokenSinkResult, Tokenizer, TokenizerOpts},
};

use super::plaintext::extract_plaintext;
use crate::types::raw_uri::RawUri;

#[derive(Clone, Default)]
struct LinkExtractor {
    links: Vec<RawUri>,
    no_scheme: bool,
}

impl TokenSink for LinkExtractor {
    type Handle = ();

    #[allow(clippy::match_same_arms)]
    fn process_token(&mut self, token: Token, _line_number: u64) -> TokenSinkResult<()> {
        match token {
            Token::CharacterTokens(raw) => {
                self.links.extend(extract_plaintext(&raw, self.no_scheme));
            }
            Token::TagToken(tag) => {
                let Tag {
                    kind: _kind,
                    name,
                    self_closing: _self_closing,
                    attrs,
                } = tag;

                for attr in attrs {
                    let urls = LinkExtractor::extract_urls_from_elem_attr(
                        attr.name.local.as_ref(),
                        name.as_ref(),
                        attr.value.as_ref(),
                    );

                    let new_urls = match urls {
                        None => extract_plaintext(&attr.value, self.no_scheme),
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
    pub(crate) fn new() -> Self {
        LinkExtractor::default()
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
pub(crate) fn extract_html(buf: &str) -> Vec<RawUri> {
    let mut input = BufferQueue::new();
    input.push_back(StrTendril::from(buf));

    let mut tokenizer = Tokenizer::new(LinkExtractor::new(), TokenizerOpts::default());
    let _handle = tokenizer.feed(&mut input);
    tokenizer.end();

    tokenizer.sink.links
}
