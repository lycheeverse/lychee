use std::{
    collections::{hash_map::Entry, HashMap, HashSet},
    path::Path,
    sync::Arc,
};

use crate::{extract::markdown::extract_markdown_fragments, types::FileType, Uri};
use tokio::{fs::File, io::AsyncReadExt, sync::Mutex};
use url::Url;

#[derive(Default, Clone, Debug)]
pub(crate) struct FragmentChecker {
    cache: Arc<Mutex<HashMap<String, HashSet<String>>>>,
}

impl FragmentChecker {
    pub(crate) async fn check(&self, path: &Path, uri: &Uri) -> Result<bool, std::io::Error> {
        let (FileType::Markdown, Some(fragment)) = (FileType::from(path), uri.url.fragment()) else {
            // If it is not a markdown file or if there is no fragment, return early.
            return Ok(true)
        };
        let url_without_frag = Self::remove_fragment(uri.url.clone());

        let frag_exists = self
            .check_cache_if_vacant_populate(url_without_frag, path, fragment)
            .await?;
        Ok(frag_exists)
    }

    fn remove_fragment(url: Url) -> String {
        let mut url = url;
        url.set_fragment(None);
        url.into()
    }

    async fn check_cache_if_vacant_populate(
        &self,
        url_without_frag: String,
        path: &Path,
        fragment: &str,
    ) -> Result<bool, std::io::Error> {
        let mut fragment_cache = self.cache.lock().await;
        match fragment_cache.entry(url_without_frag.clone()) {
            Entry::Vacant(entry) => {
                let content = Self::read_file_content(path).await?;
                let file_frags = extract_markdown_fragments(&content);
                Ok(entry.insert(file_frags).contains(fragment))
            }
            Entry::Occupied(entry) => Ok(entry.get().contains(fragment)),
        }
    }

    async fn read_file_content(path: &Path) -> Result<String, std::io::Error> {
        let mut content = String::new();
        File::open(path).await?.read_to_string(&mut content).await?;
        Ok(content)
    }
}
