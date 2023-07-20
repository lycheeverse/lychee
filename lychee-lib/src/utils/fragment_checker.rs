use std::{
    collections::{hash_map::Entry, HashMap, HashSet},
    path::Path,
    sync::Arc,
};

use crate::{extract::markdown::extract_markdown_fragments, types::FileType, Result};
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
        match (FileType::from(path), url.fragment()) {
            (FileType::Markdown, Some(fragment)) => {
                let url_without_frag = Self::remove_fragment(url.clone());
                self.populate_cache_if_vacant(url_without_frag, path, fragment)
                    .await
            }
            _ => Ok(true),
        }
    }

    fn remove_fragment(mut url: Url) -> String {
        url.set_fragment(None);
        url.into()
    }

    /// Populates the fragment cache with the given URL if it
    /// is not already in the cache.
    async fn populate_cache_if_vacant(
        &self,
        url_without_frag: String,
        path: &Path,
        fragment: &str,
    ) -> Result<bool> {
        let mut fragment_cache = self.cache.lock().await;
        match fragment_cache.entry(url_without_frag.clone()) {
            Entry::Vacant(entry) => {
                let content = fs::read_to_string(path).await?;
                let file_frags = extract_markdown_fragments(&content);
                Ok(entry.insert(file_frags).contains(fragment))
            }
            Entry::Occupied(entry) => Ok(entry.get().contains(fragment)),
        }
    }
}
