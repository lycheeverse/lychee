use pulldown_cmark::{Event as MDEvent, Parser, Tag};

use crate::{
    extract::plaintext::extract_plaintext,
    types::raw_uri::{RawUri, UriKind},
};

/// Extract unparsed URL strings from a Markdown string.
pub(crate) fn extract_markdown(input: &str) -> Vec<RawUri> {
    let parser = Parser::new(input);
    parser
        .flat_map(|event| match event {
            MDEvent::Start(Tag::Link(_, uri, _) | Tag::Image(_, uri, _)) => {
                vec![RawUri {
                    text: uri.to_string(),
                    kind: UriKind::Strict,
                }]
            }
            // TODO: Treat plaintext as fuzzy in a Markdown context?
            MDEvent::Text(txt) => extract_plaintext(&txt),
            MDEvent::Html(html) => extract_plaintext(&html.to_string()),
            _ => vec![],
        })
        .collect()
}
