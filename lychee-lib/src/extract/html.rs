use html5ever::{
    buffer_queue::BufferQueue,
    tendril::StrTendril,
    tokenizer::{Tag, Token, TokenSink, TokenSinkResult, Tokenizer, TokenizerOpts},
};

use super::plaintext::extract_plaintext;
use crate::types::raw_uri::RawUri;

#[derive(Clone)]
struct LinkExtractor {
    links: Vec<RawUri>,
}

impl TokenSink for LinkExtractor {
    type Handle = ();

    #[allow(clippy::match_same_arms)]
    fn process_token(&mut self, token: Token, _line_number: u64) -> TokenSinkResult<()> {
        match token {
            Token::CharacterTokens(raw) => self.links.append(&mut extract_plaintext(&raw)),
            Token::TagToken(tag) => {
                let Tag {
                    kind: _kind,
                    name,
                    self_closing: _self_closing,
                    attrs,
                } = tag;

                for attr in attrs {
                    let urls = extract_urls_from_elem_attr(
                        attr.name.local.as_ref(),
                        name.as_ref(),
                        attr.value.as_ref(),
                    );

                    if urls.is_empty() {
                        extract_plaintext(&attr.value);
                    } else {
                        self.links.extend(
                            urls.into_iter()
                                .map(|url| RawUri {
                                    text: url,
                                    element: Some(name.to_string()),
                                    attribute: Some(attr.name.local.to_string()),
                                })
                                .collect::<Vec<_>>(),
                        );
                    }
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

/// Extract all semantically known links from a given html attribute.
#[allow(clippy::unnested_or_patterns)]
pub(crate) fn extract_urls_from_elem_attr(
    attr_name: &str,
    elem_name: &str,
    attr_value: &str,
) -> Vec<String> {
    let mut urls = Vec::new();

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
            urls.push(attr_value.to_owned());
        }
        (_, "srcset") => {
            for image_candidate_string in attr_value.trim().split(',') {
                for part in image_candidate_string.split_ascii_whitespace() {
                    if part.is_empty() {
                        continue;
                    }

                    urls.push(part.to_owned());
                    break;
                }
            }
        }
        _ => (),
    }
    urls
}

/// Extract unparsed URL strings from an HTML string.
pub(crate) fn extract_html(buf: &str) -> Vec<RawUri> {
    let mut tokenizer = Tokenizer::new(
        LinkExtractor { links: Vec::new() },
        TokenizerOpts::default(),
    );

    let mut input = BufferQueue::new();
    input.push_back(StrTendril::from(buf));

    let _handle = tokenizer.feed(&mut input);
    tokenizer.end();

    tokenizer.sink.links
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
