use pulldown_cmark::{Event, Parser, Tag};
use std::collections::HashSet;
use url::Url;

pub(crate) fn extract_links(md: &str) -> HashSet<Url> {
    let mut links: Vec<String> = Vec::new();
    Parser::new(md).for_each(|event| match event {
        Event::Start(Tag::Link(_, link, _)) => links.push(link.into_string()),
        Event::Start(Tag::Image(_, link, _)) => links.push(link.into_string()),
        _ => (),
    });

    // Only keep legit URLs. This sorts out things like anchors.
    // Silently ignore the parse failures for now.
    // TODO: Log errors in verbose mode
    let links: HashSet<Url> = links.iter().flat_map(|l| Url::parse(&l)).collect();
    debug!("Testing links: {:#?}", links);

    links
}
