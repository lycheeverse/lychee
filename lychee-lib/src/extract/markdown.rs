use html5ever::tendril::StrTendril;
use pulldown_cmark::{Event as MDEvent, Parser, Tag};

use crate::extract::plaintext::extract_plaintext;

/// Extract unparsed URL strings from a Markdown string.
pub(crate) fn extract_markdown(input: &str) -> Vec<StrTendril> {
    let parser = Parser::new(input);
    parser
        .flat_map(|event| match event {
            MDEvent::Start(Tag::Link(_, url, _) | Tag::Image(_, url, _)) => {
                vec![StrTendril::from(url.as_ref())]
            }
            MDEvent::Text(txt) => extract_plaintext(&txt),
            MDEvent::Html(html) => extract_plaintext(&html.to_string()),
            _ => vec![],
        })
        .collect()
}
