use std::{
    borrow::Cow,
    collections::{HashMap, HashSet, hash_map::Entry},
    path::Path,
    sync::Arc,
};

use crate::{
    Result,
    extract::{html::html5gum::extract_html_fragments, markdown::extract_markdown_fragments},
    types::{ErrorKind, FileType},
};
use percent_encoding::percent_decode_str;
use tokio::{fs, sync::Mutex};
use url::Url;

/// Holds the content and file type of the fragment input.
pub(crate) struct FragmentInput {
    pub content: String,
    pub file_type: FileType,
}

impl FragmentInput {
    pub(crate) async fn from_path(path: &Path) -> Result<Self> {
        let content = fs::read_to_string(path)
            .await
            .map_err(|err| ErrorKind::ReadFileInput(err, path.to_path_buf()))?;
        let file_type = FileType::from(path);
        Ok(Self { content, file_type })
    }
}

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

    /// Checks if the given [`FragmentInput`] contains the given fragment.
    ///
    /// Returns false, if there is a fragment in the link which is not empty or "top"
    /// and the path is to a Markdown file, which doesn't contain the given fragment.
    /// (Empty # and #top fragments are always valid, triggering the browser to scroll to top.)
    ///
    /// In all other cases, returns true.
    pub(crate) async fn check(&self, input: FragmentInput, url: &Url) -> Result<bool> {
        let Some(fragment) = url.fragment() else {
            return Ok(true);
        };
        if fragment.is_empty() || fragment.eq_ignore_ascii_case("top") {
            return Ok(true);
        }
        let mut fragment_candidates = vec![Cow::Borrowed(fragment)];
        // For GitHub links, add "user-content-" prefix to the fragments.
        // The following cases cannot be handled unless we simulate with a headless browser:
        // - markdown files from any specific path (includes "blob/master/README.md")
        // - "issuecomment" fragments from the GitHub issue pages
        if url
            .host_str()
            .is_some_and(|host| host.ends_with("github.com"))
        {
            fragment_candidates.push(Cow::Owned(format!("user-content-{fragment}")));
        }
        let url_without_frag = Self::remove_fragment(url.clone());

        let FragmentInput { content, file_type } = input;
        let extractor = match file_type {
            FileType::Markdown => extract_markdown_fragments,
            FileType::Html => extract_html_fragments,
            FileType::Plaintext => return Ok(true),
        };

        let mut all_fragments = Vec::with_capacity(2 * fragment_candidates.len());
        for fragment in &fragment_candidates {
            let mut fragment_decoded = percent_decode_str(fragment).decode_utf8()?;
            if file_type == FileType::Markdown {
                fragment_decoded = fragment_decoded.to_lowercase().into();
            }
            all_fragments.push(fragment_decoded);
        }
        all_fragments.extend(fragment_candidates.iter().cloned());

        match self.cache.lock().await.entry(url_without_frag) {
            Entry::Vacant(entry) => {
                let file_frags = extractor(&content);
                let contains_fragment = all_fragments
                    .iter()
                    .any(|frag| file_frags.contains(frag.as_ref()));
                entry.insert(file_frags);
                Ok(contains_fragment)
            }
            Entry::Occupied(entry) => {
                let file_frags = entry.get();
                Ok(all_fragments
                    .iter()
                    .any(|frag| file_frags.contains(frag.as_ref())))
            }
        }
    }

    fn remove_fragment(mut url: Url) -> String {
        url.set_fragment(None);
        url.into()
    }
}
