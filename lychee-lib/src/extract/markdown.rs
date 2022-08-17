use pulldown_cmark::{Event, Parser, Tag};
use std::str;

use crate::{extract::plaintext::extract_plaintext, types::uri::raw::RawUri, Result};

use super::html5gum::extract_html;

/// Extract unparsed URL strings from a Markdown string.
pub(crate) fn extract_markdown<T: AsRef<[u8]>>(
    input: T,
    include_verbatim: bool,
) -> Result<Vec<RawUri>> {
    // In some cases it is undesirable to extract links from within code blocks,
    // which is why we keep track of entries and exits while traversing the input.
    let mut inside_code_block = false;

    // Parser expects a UTF-8 string.
    let parser = Parser::new(str::from_utf8(input.as_ref())?);
    let extracted = parser
        .filter_map(|event| match event {
            // A link. The first field is the link type, the second the destination URL and the third is a title.
            Event::Start(Tag::Link(_, uri, _)) => {
                Some(vec![RawUri {
                    text: uri.to_string(),
                    // Emulate `<a href="...">` tag here to be compatible with
                    // HTML links. We might consider using the actual Markdown
                    // `LinkType` for better granularity in the future
                    element: Some("a".to_string()),
                    attribute: Some("href".to_string()),
                }])
            }
            // An image. The first field is the link type, the second the destination URL and the third is a title.
            Event::Start(Tag::Image(_, uri, _)) => {
                Some(vec![RawUri {
                    text: uri.to_string(),
                    // Emulate `<img src="...">` tag here to be compatible with
                    // HTML links. We might consider using the actual Markdown
                    // `LinkType` for better granularity in the future
                    element: Some("img".to_string()),
                    attribute: Some("src".to_string()),
                }])
            }
            // A code block (inline or fenced).
            Event::Start(Tag::CodeBlock(_)) => {
                inside_code_block = true;
                None
            }
            Event::End(Tag::CodeBlock(_)) => {
                inside_code_block = false;
                None
            }

            // A text node.
            Event::Text(txt) => {
                if inside_code_block && !include_verbatim {
                    None
                } else {
                    extract_plaintext(txt.as_ref()).ok()
                }
            }

            // An HTML node
            Event::Html(html) => {
                // This won't exclude verbatim links right now, because HTML gets passed in chunks
                // by pulldown_cmark. So excluding `<pre>` and `<code>` is not handled right now.
                Some(extract_html(html.as_ref(), include_verbatim))
            }

            // An inline code node.
            Event::Code(code) => {
                if include_verbatim {
                    extract_plaintext(code.as_ref()).ok()
                } else {
                    None
                }
            }

            // Silently skip over other events
            _ => None,
        })
        .flatten()
        .collect();
    Ok(extracted)
}

#[cfg(test)]
mod tests {
    use super::*;

    const MD_INPUT: &str = r#"
# Test

Some link in text [here](https://foo.com)

Code:

```bash
https://bar.com/123
```

or inline like `https://bar.org` for instance.

[example](http://example.com)
        "#;

    #[test]
    fn test_skip_verbatim() {
        let expected = vec![
            RawUri {
                text: "https://foo.com".to_string(),
                element: Some("a".to_string()),
                attribute: Some("href".to_string()),
            },
            RawUri {
                text: "http://example.com".to_string(),
                element: Some("a".to_string()),
                attribute: Some("href".to_string()),
            },
        ];

        let uris = extract_markdown(MD_INPUT, false).unwrap();
        assert_eq!(uris, expected);
    }

    #[test]
    fn test_include_verbatim() {
        let expected = vec![
            RawUri {
                text: "https://foo.com".to_string(),
                element: Some("a".to_string()),
                attribute: Some("href".to_string()),
            },
            RawUri {
                text: "https://bar.com/123".to_string(),
                element: None,
                attribute: None,
            },
            RawUri {
                text: "https://bar.org".to_string(),
                element: None,
                attribute: None,
            },
            RawUri {
                text: "http://example.com".to_string(),
                element: Some("a".to_string()),
                attribute: Some("href".to_string()),
            },
        ];

        let uris = extract_markdown(MD_INPUT, true).unwrap();
        assert_eq!(uris, expected);
    }

    #[test]
    #[ignore]
    fn test_skip_verbatim_html() {
        let input = " 
<code>
http://link.com
</code>
<pre>
Some pre-formatted http://pre.com 
</pre>";

        let expected = vec![];

        let uris = extract_markdown(input, false).unwrap();
        assert_eq!(uris, expected);
    }
}
