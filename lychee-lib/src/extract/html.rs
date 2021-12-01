use html5ever::{
    parse_document,
    tendril::{StrTendril, TendrilSink},
};
use markup5ever_rcdom::{Handle, NodeData, RcDom};

use crate::{helpers::url, Result};

use super::plaintext::extract_plaintext;

/// Extract unparsed URL strings from an HTML string.
pub(crate) fn extract_html(input: &str) -> Result<Vec<StrTendril>> {
    let rc_dom = parse_document(RcDom::default(), html5ever::ParseOpts::default())
        .from_utf8()
        .read_from(&mut input.as_bytes())?;

    Ok(walk_html_links(&rc_dom.document))
}

/// Recursively walk links in a HTML document, aggregating URL strings in `urls`.
fn walk_html_links(node: &Handle) -> Vec<StrTendril> {
    let mut all_urls = Vec::new();
    match node.data {
        NodeData::Text { ref contents } => {
            all_urls.append(&mut extract_plaintext(&contents.borrow()));
        }
        NodeData::Comment { ref contents } => {
            all_urls.append(&mut extract_plaintext(contents));
        }
        NodeData::Element {
            ref name,
            ref attrs,
            ..
        } => {
            for attr in attrs.borrow().iter() {
                let urls = url::extract_links_from_elem_attr(
                    attr.name.local.as_ref(),
                    name.local.as_ref(),
                    attr.value.as_ref(),
                );

                if urls.is_empty() {
                    extract_plaintext(&attr.value);
                } else {
                    all_urls.extend(urls.into_iter().map(StrTendril::from).collect::<Vec<_>>());
                }
            }
        }
        _ => {}
    }

    // recursively traverse the document's nodes -- this doesn't need any extra
    // exit conditions, because the document is a tree
    for child in node.children.borrow().iter() {
        let urls = walk_html_links(child);
        all_urls.extend(urls);
    }

    all_urls
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_extract_link_at_end_of_line() {
        let input = "https://www.apache.org/licenses/LICENSE-2.0\n";
        let link = input.trim_end();

        let urls = extract_html(input).unwrap();
        assert_eq!(vec![StrTendril::from(link)], urls);
    }
}
