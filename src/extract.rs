use linkify::LinkFinder;

use std::collections::HashSet;
use url::Url;

pub(crate) fn extract_links(input: &str) -> HashSet<Url> {
    let finder = LinkFinder::new();
    let links: Vec<_> = finder.links(input).collect();

    // Only keep legit URLs. This sorts out things like anchors.
    // Silently ignore the parse failures for now.
    // TODO: Log errors in verbose mode
    let links: HashSet<Url> = links.iter().flat_map(|l| Url::parse(l.as_str())).collect();
    debug!("Found links: {:#?}", links);

    links
}

#[cfg(test)]
mod test {
    use super::*;
    use std::iter::FromIterator;

    #[test]
    fn test_extract_markdown_links() {
        let input = "This is [a test](https://endler.dev).";
        let links = extract_links(input);
        assert_eq!(
            links,
            HashSet::from_iter([Url::parse("https://endler.dev").unwrap()].iter().cloned())
        )
    }

    #[test]
    fn test_skip_markdown_anchors() {
        let input = "This is [a test](#lol).";
        let links = extract_links(input);
        assert_eq!(links, HashSet::new())
    }

    #[test]
    fn test_skip_markdown_iternal_urls() {
        let input = "This is [a test](./internal).";
        let links = extract_links(input);
        assert_eq!(links, HashSet::new())
    }

    #[test]
    fn test_non_markdown_links() {
        let input = "https://endler.dev and https://hello-rust.show/foo/bar?lol=1";
        let links = extract_links(input);
        let expected = HashSet::from_iter(
            [
                Url::parse("https://endler.dev").unwrap(),
                Url::parse("https://hello-rust.show/foo/bar?lol=1").unwrap(),
            ]
            .iter()
            .cloned(),
        );
        assert_eq!(links, expected)
    }

    #[test]
    fn test_skip_emails() {
        let input = "matthias@example.com";
        let links = extract_links(input);
        assert_eq!(links, HashSet::new())
    }
}
