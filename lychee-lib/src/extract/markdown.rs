use pulldown_cmark::{Event as MDEvent, Parser, Tag};

use crate::{extract::plaintext::extract_plaintext, types::raw_uri::RawUri};

/// Extract unparsed URL strings from a Markdown string.
pub(crate) fn extract_markdown(input: &str, no_scheme: bool) -> Vec<RawUri> {
    let parser = Parser::new(input);
    parser
        .flat_map(|event| match event {
            MDEvent::Start(Tag::Link(_, uri, _)) => {
                vec![RawUri {
                    text: uri.to_string(),
                    // Emulate `<a href="...">` tag here to be compatible with
                    // HTML links. We might consider using the actual Markdown
                    // `LinkType` for better granularity in the future
                    element: Some("a".to_string()),
                    attribute: Some("href".to_string()),
                }]
            }
            MDEvent::Start(Tag::Image(_, uri, _)) => {
                vec![RawUri {
                    text: uri.to_string(),
                    // Emulate `<img src="...">` tag here to be compatible with
                    // HTML links. We might consider using the actual Markdown
                    // `LinkType` for better granularity in the future
                    element: Some("img".to_string()),
                    attribute: Some("src".to_string()),
                }]
            }
            MDEvent::Text(txt) => extract_plaintext(&txt, no_scheme),
            MDEvent::Html(html) => extract_plaintext(&html.to_string(), no_scheme),
            _ => vec![],
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_markdown() {
        let input =
            "[docs.rs] is great, and so is [example](https://example.com) or free-form google.com";
        let links = extract_markdown(input, false);
        let expected = vec![RawUri {
            text: "https://example.com".to_string(),
            element: Some("a".to_string()),
            attribute: Some("href".to_string()),
        }];

        assert_eq!(expected, links);
    }

    #[test]
    fn test_markdown_no_schema() {
        let input =
            "[docs.rs] is great, and so is [example](https://example.com) or free-form google.com";
        let links = extract_markdown(input, true);
        let expected = vec![
            RawUri::from("docs.rs"),
            RawUri {
                text: "https://example.com".to_string(),
                element: Some("a".to_string()),
                attribute: Some("href".to_string()),
            },
            RawUri::from("google.com"),
        ];

        assert_eq!(expected, links);
    }
}
