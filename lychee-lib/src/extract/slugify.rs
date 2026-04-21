//! Converts heading text into "slugs" for use as fragment identifiers, mimicking
//! the algorithm which GitHub uses for generating Markdown fragment links.
//!
//! The core algorithm is based on [Flet/github-slugger](https://github.com/Flet/github-slugger/).

use std::fmt::Write;
use std::{collections::HashMap, num::NonZeroUsize, sync::LazyLock};

use regex::Regex;

/// From <https://github.com/Flet/github-slugger/blob/master/script/generate-regex.js#L8>
static UNICODE_GENERAL_CATEGORIES_TO_REMOVE: &[&str] = &[
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
    let mut includes = String::new();
    for cat in UNICODE_GENERAL_CATEGORIES_TO_REMOVE {
        let _ = write!(includes, r"\p{{{cat}}}");
    }

    let excludes = r"\p{Alphabetic} -";

    Regex::new(&format!("[{includes}&&[^{excludes}]]+")).expect("slugify regex failed to build")
});

/// Slugifies the given header text, but does not guarantee that
/// the returned slugs are unique between calls. For most uses,
/// [`GithubSlugify`] should be used instead.
pub fn slugify_without_disambiguation(text: &str) -> String {
    REGEX_TO_REMOVE
        .replace_all(text, "")
        .replace(' ', "-")
        .to_lowercase()
}

/// A stateful type for generating "slugified" fragment identifiers in the style
/// of Github's markdown header links.
///
/// A new instance of [`GithubSlugify`] should be created for each document
/// containing headers, then [`GithubSlugify::slugify`] should be called for each
/// heading in the document.
#[derive(Debug, Clone, Default)]
pub struct GithubSlugify {
    /// Map of base slug to suffix which should be tried for the *next* occurrence
    /// of that base slug. If a slug is not present in this map, it means that it
    /// hasn't been seen before.
    count: HashMap<String, NonZeroUsize>,
}

impl GithubSlugify {
    /// Constructs a new [`GithubSlugify`].
    #[must_use]
    pub fn new() -> Self {
        Self {
            count: HashMap::new(),
        }
    }

    /// Determines if the given slug overlaps with a slug which has been previously
    /// returned by [`GithubSlugify::slugify`].
    fn seen(&self, slug: &str) -> bool {
        if self.count.contains_key(slug) {
            // Handles cases of direct repetition (e.g., `foo`, then `foo`).
            // Also handles when a new suffixed slug collides with an earlier non-suffixed
            // slug (e.g., `foo` and `foo` with an earlier `foo 1`).
            return true;
        }

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

    /// Disambiguates the given "base" slug by appending a hyphen and a number
    /// to the slug if it conflicts with a previously-generated slug. This function
    /// will continue trying successive numbers until a conflict is avoided.
    ///
    /// This function will mutate the [`GithubSlugify`] to record the returned
    /// string.
    ///
    /// # Implementation detail
    ///
    /// Compared to the [upstream](https://github.com/Flet/github-slugger/blob/master/index.js),
    /// this code is slightly more complicated. The upstream code is simpler because
    /// it adds every disambiguated slug as a new key into its "occurrences" map.
    /// We avoid doing that and only increment the counter of the existing entry,
    /// but this means we need to handle between headers that originally end with
    /// a digit and disambiguated suffixed slugs. Much of this code is in
    /// [`GithubSlugify::seen`].
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

    /// Slugifies the given header text into a slug (a lowercase hyphen-separated
    /// string suitable for use as a fragment identifier). Additionally, this
    /// function ensures returned slugs are distinct from any earlier slug returned
    /// by this [`GithubSlugify`].
    ///
    /// For example,
    /// ```
    /// # use lychee_lib::extract::slugify::GithubSlugify;
    /// let mut slugger = GithubSlugify::new();
    /// assert_eq!(slugger.slugify("foo bar"), "foo-bar");
    /// assert_eq!(slugger.slugify("foo bar"), "foo-bar-1");
    /// assert_eq!(slugger.slugify("foo, bar!"), "foo-bar-2");
    /// ```
    pub fn slugify(&mut self, text: &str) -> String {
        self.disambiguate(slugify_without_disambiguation(text))
    }
}

#[cfg(test)]
mod tests {
    use percent_encoding::percent_decode_str;

    use super::{GithubSlugify, slugify_without_disambiguation};

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
    fn test_slugify_kebab_case() {
        let check = |input, expected| {
            let actual = slugify_without_disambiguation(input);
            assert_eq!(actual, expected);
        };
        check("A Heading", "a-heading");
        check(
            "This header has a :thumbsup: in it",
            "this-header-has-a-thumbsup-in-it",
        );
        check(
            "Header with 한글 characters (using unicode)",
            "header-with-한글-characters-using-unicode",
        );
        check(
            "Underscores foo_bar_, dots . and numbers 1.7e-3",
            "underscores-foo_bar_-dots--and-numbers-17e-3",
        );
        check("Many          spaces", "many----------spaces");
    }

    #[test]
    fn test_github_slugify() {
        let headings = ["foo 1", "foo", "foo", "foo", "foo 1", "FOO 1"];
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
