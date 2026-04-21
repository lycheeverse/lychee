//! Converts heading text into "slugs" for use as fragment identifiers, mimicking
//! the algorithm which GitHub uses for generating Markdown fragment links.
//!
//! The core algorithm is based on [Flet/github-slugger](https://github.com/Flet/github-slugger/).

use std::{
    borrow::Cow,
    collections::{HashMap, HashSet, hash_map::Entry},
    num::NonZeroUsize,
    sync::LazyLock,
};

use regex::Regex;

/// From <https://github.com/Flet/github-slugger/blob/master/script/generate-regex.js#L8>
static UNICODE_GENERAL_CATEGORIES_TO_REMOVE: &'static [&'static str] = &[
    // Some numbers:
    "Other_Number",
    // Some punctuation:
    "Close_Punctuation",
    "Final_Punctuation",
    "Initial_Punctuation",
    "Open_Punctuation",
    "Other_Punctuation",
    // All except a normal `-` (dash)
    "Dash_Punctuation",
    // All:
    "Symbol",
    "Control",
    "Private_Use",
    "Format",
    "Unassigned",
    // All except a normal ` ` (space)
    "Separator",
];

static REGEX_TO_REMOVE: LazyLock<Regex> = LazyLock::new(|| {
    let include_character_class = UNICODE_GENERAL_CATEGORIES_TO_REMOVE
        .iter()
        .map(|cls| format!(r"\p{{{cls}}}"))
        .collect::<String>();

    let exclude_character_class = r"\p{Alphabetic} -";

    Regex::new(&format!(
        "[{}&&[^{}]]+",
        include_character_class, exclude_character_class
    ))
    .expect("slugify regex failed to build")
});

pub fn slugify_without_disambiguation(text: &str) -> String {
    REGEX_TO_REMOVE
        .replace_all(text, "")
        .replace(' ', "-")
        .to_lowercase()
}

#[derive(Debug, Clone, Default)]
pub struct GithubSlugify {
    /// Map of base slug to suffix which should be used for the *next* occurrence
    /// of that base slug. If a slug is not present in this map, it means that it
    /// hasn't been seen before and no suffix is necessary.
    ///
    /// This allows headings with the same text to be disambiguated by an
    /// incrementing suffix.
    count: HashMap<String, NonZeroUsize>,
}

impl GithubSlugify {
    pub fn new() -> Self {
        Self {
            count: HashMap::new(),
        }
    }

    fn seen(&self, slug: &str) -> bool {
        if self.count.contains_key(slug) {
            // Handles cases of direct repetition (e.g., `foo`, then `foo`).
            // Also handles when a new suffixed slug collides with an earlier non-suffixed
            // slug (e.g., `foo` and `foo` with an earlier `foo 1`).
            return true;
        };

        if let Some((slug, n)) = slug.rsplit_once('-')
            && let Ok(n) = str::parse::<NonZeroUsize>(n)
        {
            // Handles cases where the new slug already ends in a number
            // and it overlaps with an earlier suffixed slug (e.g., `foo 1` when
            // two `foo` were seen earlier).
            self.count
                .get(slug)
                .is_some_and(|&next_suffix| n < next_suffix)
        } else {
            false
        }
    }

    fn disambiguate(&mut self, base_slug: String) -> String {
        let mut suffix = self.count.get(&base_slug).copied();
        let mut slug = base_slug.clone();

        let next_suffix = loop {
            slug.truncate(base_slug.len());

            let next_suffix = match suffix {
                Some(non_zero) => {
                    slug.push('-');
                    slug.push_str(&non_zero.to_string());
                    non_zero.saturating_add(1)
                }
                None => NonZeroUsize::MIN,
            };

            if !self.seen(&slug) || next_suffix == NonZeroUsize::MAX {
                break next_suffix;
            }
            suffix = Some(next_suffix);
        };

        self.count.entry(base_slug).insert_entry(next_suffix);
        slug
    }

    pub fn slugify(&mut self, text: &str) -> String {
        self.disambiguate(slugify_without_disambiguation(text))
    }
}

#[cfg(test)]
mod tests {
    use percent_encoding::percent_decode_str;

    use crate::extract::slugify::GithubSlugify;

    use super::slugify_without_disambiguation;

    fn unpercent(percent_str: &str) -> String {
        percent_decode_str(percent_str)
            .decode_utf8()
            .expect("percent string had invalid utf-8")
            .into_owned()
    }

    #[test]
    fn test_slugify_without_disambiguation() {
        assert_eq!("a-b", slugify_without_disambiguation("a b"));
        assert_eq!(
            unpercent("%EF%B8%8F⃣-b"),
            slugify_without_disambiguation("#️⃣ b")
        );
        assert_eq!(
            unpercent("%EF%B8%8F-c"),
            slugify_without_disambiguation("☔️ c")
        );
        assert_eq!(
            unpercent("🅰%EF%B8%8F-d"),
            slugify_without_disambiguation("🅰️ d")
        );

        assert_eq!(
            unpercent("à-á-â-ã-ä-å-or-à-á-â-ã-ä-å"),
            slugify_without_disambiguation("À, Á, Â, Ã, Ä, Å or à, á, â, ã, ä, å")
        );
    }

    #[test]
    fn test_github_slugify() {
        let headings = vec!["foo 1", "foo", "foo", "foo", "foo 1", "FOO 1"];
        let expected = vec!["foo-1", "foo", "foo-2", "foo-3", "foo-1-1", "foo-1-2"];
        let mut slugger = GithubSlugify::new();
        assert_eq!(
            expected,
            headings
                .iter()
                .map(|h| slugger.slugify(h))
                .collect::<Vec<_>>()
        );
    }
}
