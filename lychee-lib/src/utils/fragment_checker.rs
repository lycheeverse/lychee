use std::{
    collections::{hash_map::Entry, HashMap, HashSet},
    path::Path,
    sync::Arc,
};

use crate::{
    extract::{html::html5gum::extract_html_fragments, markdown::extract_markdown_fragments},
    types::FileType,
    Result,
};
use tokio::{fs, sync::Mutex};
use url::Url;

/// Holds a cache of fragments for a given URL.
///
/// Fragments, also known as anchors, are used to link to a specific
/// part of a page. For example, the URL `https://example.com#foo`
/// will link to the element with the `id` of `foo`.
///
/// This cache is used to avoid having to re-parse the same file
/// multiple times when checking if a given URL contains a fragment.
///
/// The cache is stored in a `HashMap` with the URL as the key and
/// a `HashSet` of fragments as the value.
#[derive(Default, Clone, Debug)]
pub(crate) struct FragmentChecker {
    cache: Arc<Mutex<HashMap<String, HashSet<String>>>>,
}

impl FragmentChecker {
    /// Creates a new `FragmentChecker`.
    pub(crate) fn new() -> Self {
        Self {
            cache: Arc::default(),
        }
    }

    /// Checks the given path contains the given fragment.
    ///
    /// Returns false, if there is a fragment in the link and the path is to a markdown file which
    /// doesn't contain the given fragment.
    ///
    /// In all other cases, returns true.
    pub(crate) async fn check(&self, path: &Path, url: &Url) -> Result<bool> {
        let Some(fragment) = url.fragment() else {
            return Ok(true)
        };
        let url_without_frag = Self::remove_fragment(url.clone());

        let extractor = match FileType::from(path) {
            FileType::Markdown => extract_markdown_fragments,
            FileType::Html => extract_html_fragments,
            FileType::Plaintext => return Ok(true),
        };
        match self.cache.lock().await.entry(url_without_frag) {
            Entry::Vacant(entry) => {
                let content = fs::read_to_string(path).await?;
                let file_frags = extractor(&content);
                Ok(entry.insert(file_frags).contains(fragment))
            }
            Entry::Occupied(entry) => Ok(entry.get().contains(fragment)),
        }
    }

    fn remove_fragment(mut url: Url) -> String {
        url.set_fragment(None);
        url.into()
    }
}
