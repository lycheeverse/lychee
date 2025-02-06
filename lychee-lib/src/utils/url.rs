use linkify::LinkFinder;

use once_cell::sync::Lazy;

static LINK_FINDER: Lazy<LinkFinder> = Lazy::new(LinkFinder::new);

// Use `LinkFinder` to offload the raw link searching in plaintext
pub(crate) fn find_links(input: &str) -> impl Iterator<Item = linkify::Link> {
    LINK_FINDER.links(input)
}
