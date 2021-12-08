use pulldown_cmark::{Event as MDEvent, Parser, Tag};

use crate::{extract::plaintext::extract_plaintext, types::raw_uri::RawUri};

/// Extract unparsed URL strings from a Markdown string.
pub(crate) fn extract_markdown(input: &str) -> Vec<RawUri> {
    let parser = Parser::new(input);
    parser
        .flat_map(|event| match event {
            MDEvent::Start(Tag::Link(_, uri, _)) => {
                vec![RawUri {
                    text: uri.to_string(),
                    // Emulate `<a href="...">` tag here to be compatible with HTML
                    // links. We might consider using the actual Markdown
                    // `LinkType` for better granularity
                    element: Some("a".to_string()),
                    attribute: Some("href".to_string()),
                }]
            }
            MDEvent::Start(Tag::Image(_, uri, _)) => {
                vec![RawUri {
                    text: uri.to_string(),
                    // Emulate `<img src="...">` tag here to be compatible with HTML
                    // links. We might consider using the actual Markdown
                    // `LinkType` for better granularity
                    element: Some("img".to_string()),
                    attribute: Some("src".to_string()),
                }]
            }
            MDEvent::Text(txt) => extract_plaintext(&txt),
            MDEvent::Html(html) => extract_plaintext(&html.to_string()),
            _ => vec![],
        })
        .collect()
}
