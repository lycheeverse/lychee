use std::{
    collections::{hash_map::Entry, HashMap, HashSet},
    path::Path,
    sync::Arc,
};

use crate::{extract::markdown::extract_markdown_fragments, types::FileType, Result, Uri};
use tokio::{fs, sync::Mutex};
use url::Url;

#[derive(Default, Clone, Debug)]
pub(crate) struct FragmentChecker {
    cache: Arc<Mutex<HashMap<String, HashSet<String>>>>,
}

impl FragmentChecker {
    /// Checks if the given path contains the given fragment.
    pub(crate) async fn check(&self, path: &Path, uri: &Uri) -> Result<bool> {
        match (FileType::from(path), uri.url.fragment()) {
            (FileType::Markdown, Some(fragment)) => {
                let url_without_frag = Self::remove_fragment(uri.url.clone());
                self.populate_cache_if_vacant(url_without_frag, path, fragment)
                    .await
            }
            _ => Ok(false),
        }
    }

    fn remove_fragment(url: Url) -> String {
        let mut url = url;
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
