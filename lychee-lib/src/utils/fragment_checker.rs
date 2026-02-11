use log::info;
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
pub(crate) struct FragmentInput<'a> {
    pub content: Cow<'a, str>,
    pub file_type: FileType,
}

impl FragmentInput<'_> {
    pub(crate) async fn from_path(path: &Path) -> Result<Self> {
        let content = fs::read_to_string(path)
            .await
            .map_err(|err| ErrorKind::ReadFileInput(err, path.to_path_buf()))?;
        let file_type = FileType::from(path);
        Ok(Self {
            content: Cow::Owned(content),
            file_type,
        })
    }
}

/// A fragment builder that expands the given fragments into a list of candidates.
struct FragmentBuilder {
    variants: Vec<String>,
    decoded: Vec<String>,
}

impl FragmentBuilder {
    fn new(fragment: &str, url: &Url, file_type: FileType) -> Result<Self> {
        let mut variants = vec![fragment.into()];
        // For GitHub links, add "user-content-" prefix to the fragments.
        // The following cases cannot be handled unless we simulate with a headless browser:
        // - markdown files from any specific path (includes "blob/master/README.md")
        // - "issuecomment" fragments from the GitHub issue pages
        if url
            .host_str()
            .is_some_and(|host| host.ends_with("github.com"))
        {
            variants.push(format!("user-content-{fragment}"));
        }

        // Only store the percent-decoded variants if it's different from the original
        // fragment. This avoids storing and comparing the same fragment twice.
        let mut decoded = Vec::new();
        for frag in &variants {
            let mut require_alloc = false;
            let mut fragment_decoded: Cow<'_, str> = match percent_decode_str(frag).decode_utf8()? {
                Cow::Borrowed(s) => s.into(),
                Cow::Owned(s) => {
                    require_alloc = true;
                    s.into()
                }
            };
            if file_type == FileType::Markdown {
                let lowercase = fragment_decoded.to_lowercase();
                if lowercase != fragment_decoded {
                    fragment_decoded = lowercase.into();
                    require_alloc = true;
                }
            }
            if require_alloc {
                decoded.push(fragment_decoded.into());
            }
        }

        Ok(Self { variants, decoded })
    }

    fn any_matches(&self, fragments: &HashSet<String>) -> bool {
        self.variants
            .iter()
            .chain(self.decoded.iter())
            .any(|frag| fragments.contains(frag))
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
    pub(crate) async fn check(&self, input: FragmentInput<'_>, url: &Url) -> Result<bool> {
        let Some(fragment) = url.fragment() else {
            return Ok(true);
        };
        if fragment.is_empty() || fragment.eq_ignore_ascii_case("top") {
            return Ok(true);
        }

        let url_without_frag = Self::remove_fragment(url.clone());

        let FragmentInput { content, file_type } = input;
        let extractor = match file_type {
            FileType::Markdown => extract_markdown_fragments,
            FileType::Html => extract_html_fragments,
            FileType::Css | FileType::Plaintext => {
                info!("Skipping fragment check for {url} within a {file_type} file");
                return Ok(true);
            }
        };

        let fragment_candidates = FragmentBuilder::new(fragment, url, file_type)?;
        match self.cache.lock().await.entry(url_without_frag) {
            Entry::Vacant(entry) => {
                let file_frags = extractor(&content);
                let contains_fragment = fragment_candidates.any_matches(&file_frags);
                entry.insert(file_frags);
                Ok(contains_fragment)
            }
            Entry::Occupied(entry) => {
                let file_frags = entry.get();
                Ok(fragment_candidates.any_matches(file_frags))
            }
        }
    }

    fn remove_fragment(mut url: Url) -> String {
        url.set_fragment(None);
        url.into()
    }
}
