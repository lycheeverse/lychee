use pulldown_cmark::{Event, Parser, Tag};

use crate::{extract::plaintext::extract_plaintext, types::raw_uri::RawUri};

/// Extract unparsed URL strings from a Markdown string.
pub(crate) fn extract_markdown(input: &str, skip_code_blocks: bool) -> Vec<RawUri> {
    // In some cases it is undesirable to extract links from within code blocks,
    // which is why we keep track of entries and exits while traversing the input.
    let mut inside_code_block = false;

    let parser = Parser::new(input);
    parser
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
                if inside_code_block && skip_code_blocks {
                    None
                } else {
                    Some(extract_plaintext(&txt))
                }
            }

            // An HTML node
            Event::Html(html) => Some(extract_plaintext(&html.to_string())),

            // An inline code node.
            Event::Code(code) => None,

            // Silently skip over other events
            _ => None,
        })
        .flatten()
        .collect()
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

[example](http://example.com)
        "#;

    #[test]
    fn test_skip_code_block() {
        let links = vec![
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

        let uris = extract_markdown(MD_INPUT, true);
        assert_eq!(uris, links);
    }

    #[test]
    fn test_code_block() {
        let links = vec![
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
                text: "http://example.com".to_string(),
                element: Some("a".to_string()),
                attribute: Some("href".to_string()),
            },
        ];

        let uris = extract_markdown(MD_INPUT, false);
        assert_eq!(uris, links);
    }
}
