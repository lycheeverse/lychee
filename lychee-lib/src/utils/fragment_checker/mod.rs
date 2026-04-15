mod text;

use log::info;
use std::{
    borrow::Cow,
    collections::{HashMap, HashSet, hash_map::Entry},
    path::Path,
    sync::Arc,
};

use crate::{
    FragmentCheckerOptions, Result,
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

#[derive(Debug, PartialEq, Eq)]
struct ParsedFragment<'a> {
    // The element ID part of the fragment, e.g., "section" in `https://example.com/#section:~:text=example`.
    element_id: Option<&'a str>,
    // The raw value of the text directive, e.g., "The%20concept%20of-,end%2Duser,-first%20surfaced%20in" in `https://en.wikipedia.org/wiki/End_user#:~:text=The%20concept%20of-,end%2Duser,-first%20surfaced%20in`.
    // Dashes and commas have special meaning in the text directive, so we need to keep them percentage-encoded.
    // Full parsing of the text directive value into its components (prefix, start, end, suffix) is done later in the `TextDirective` struct.
    // See https://wicg.github.io/scroll-to-text-fragment/#syntax
    encoded_text_directive_value: Option<String>,
}

const FRAGMENT_DIRECTIVE_DELIMITER: &str = ":~:";
const TEXT_DIRECTIVE_KEY: &str = "text";

impl<'a> ParsedFragment<'a> {
    /// This method does top-level parsing of the fragment, separating the element id (if any) from the text directive (if any).
    fn parse(url: &'a Url) -> Self {
        let Some(fragment) = url.fragment() else {
            return Self {
                element_id: None,
                encoded_text_directive_value: None,
            };
        };

        // Split off the element id from the fragment directive
        // See https://wicg.github.io/scroll-to-text-fragment/#the-fragment-directive
        // See https://wicg.github.io/scroll-to-text-fragment/#determine-if-fragment-id-is-needed
        let Some((element_id, fragment_directive)) =
            fragment.split_once(FRAGMENT_DIRECTIVE_DELIMITER)
        else {
            return Self {
                element_id: Some(fragment),
                encoded_text_directive_value: None,
            };
        };

        let element_id = (!element_id.is_empty()).then_some(element_id);

        // The fragment directive may contain several components, separated by ampersant, such as https://example.com#:~:text=foo&text=bar&unknownDirective
        // We do not URL decode the text directive value yet, because comma and dashes have special meaning and need to be percentage encoded.
        // See Example 6 in https://wicg.github.io/scroll-to-text-fragment/#the-fragment-directive
        for (key, value) in fragment_directive.split('&').filter_map(|part| part.split_once('=')) {
            // The standard allows several directives, including serveral text directives. We only support the first text directive, and ignore other directives.
            // See https://wicg.github.io/scroll-to-text-fragment/#text-directives
            if key == TEXT_DIRECTIVE_KEY {
                return Self {
                    element_id,
                    encoded_text_directive_value: Some(value.to_owned()),
                };
            }
        }

        Self {
            element_id,
            encoded_text_directive_value: None,
        }
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

    /// Checks whether the fragments in the given URL are valid for the provided input.
    pub(crate) async fn check(
        &self,
        input: FragmentInput<'_>,
        url: &Url,
        options: FragmentCheckerOptions,
    ) -> Result<bool> {
        let parsed = ParsedFragment::parse(url);
        let FragmentInput { content, file_type } = input;

        if options.check_anchor_fragments
            && parsed.element_id.is_some()
            && !self
                .check_anchor_fragment(&content, file_type, url, parsed.element_id)
                .await?
        {
            return Ok(false);
        }

        if options.check_text_fragments
            && parsed.encoded_text_directive_value.is_some()
            && !text::check_text_fragments(url, &content, file_type)
        {
            return Ok(false);
        }

        Ok(true)
    }

    async fn check_anchor_fragment(
        &self,
        content: &str,
        file_type: FileType,
        url: &Url,
        anchor_fragment: Option<&str>,
    ) -> Result<bool> {
        let Some(fragment) = anchor_fragment else {
            return Ok(true);
        };

        if fragment.is_empty() || fragment.eq_ignore_ascii_case("top") {
            return Ok(true);
        }

        let url_without_frag = Self::remove_fragment(url.clone());
        let anchor_url = Self::with_element_fragment(url, anchor_fragment);

        let extractor = match file_type {
            FileType::Markdown => extract_markdown_fragments,
            FileType::Html => extract_html_fragments,
            FileType::Css | FileType::Plaintext | FileType::Xml => {
                info!("Skipping fragment check for {anchor_url} within a {file_type} file");
                return Ok(true);
            }
        };

        let fragment_candidates = FragmentBuilder::new(fragment, &anchor_url, file_type)?;
        match self.cache.lock().await.entry(url_without_frag) {
            Entry::Vacant(entry) => {
                let file_frags = extractor(content);
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

    fn with_element_fragment(url: &Url, fragment: Option<&str>) -> Url {
        let mut updated = url.clone();
        updated.set_fragment(fragment);
        updated
    }
}

#[cfg(test)]
mod tests {
    use url::Url;

    use super::ParsedFragment;

    #[test]
    fn parses_pure_text_fragment_directive() {
        let url = Url::parse("https://example.com/#:~:unknown&text=needle").unwrap();

        let parsed = ParsedFragment::parse(&url);

        assert_eq!(
            parsed,
            ParsedFragment {
                element_id: None,
                encoded_text_directive_value: Some("needle".to_string()),
            }
        );
    }

    #[test]
    fn parses_element_fragment_before_text_directive() {
        let url = Url::parse("https://example.com/#section:~:text=needle&unknown").unwrap();

        let parsed = ParsedFragment::parse(&url);

        assert_eq!(
            parsed,
            ParsedFragment {
                element_id: Some("section"),
                encoded_text_directive_value: Some("needle".to_string()),
            }
        );
    }

    #[test]
    fn parses_plain_element_fragment() {
        let url = Url::parse("https://example.com/#section").unwrap();

        let parsed = ParsedFragment::parse(&url);

        assert_eq!(
            parsed,
            ParsedFragment {
                element_id: Some("section"),
                encoded_text_directive_value: None,
            }
        );
    }

    #[test]
    fn parses_text_directive_with_encoded_values() {
        let url = Url::parse("https://en.wikipedia.org/wiki/End_user#:~:unknown&text=The%20concept%20of-,end%2Duser,-first%20surfaced%20in&unknown&text=ignored-in-lychee").unwrap();
        let parsed = ParsedFragment::parse(&url);

        assert_eq!(
            parsed,
            ParsedFragment {
                element_id: None,
                encoded_text_directive_value: Some("The%20concept%20of-,end%2Duser,-first%20surfaced%20in".to_string()),
            }
        );
    }
}
