//! Converts heading text into hyphen-separated strings for use as fragment identifiers,
//! mimicking the algorithm which GitHub uses for generating Markdown fragment IDs.
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

    Regex::new(&format!("[{includes}&&[^{excludes}]]+")).expect("fragment regex failed to build")
});

/// Converts the given header text into a hyphen-separated fragment ID, mimicking
/// the algorithm used by GitHub. However, does not guarantee that the returned
/// IDs are unique between calls. For most uses, [`GithubHeadingIdGenerator`]
/// should be used instead.
pub fn generate_without_disambiguation(text: &str) -> String {
    REGEX_TO_REMOVE
        .replace_all(text, "")
        .replace(' ', "-")
        .to_lowercase()
}

/// A stateful type for generating fragment identifiers in the style
/// of Github's markdown header links.
///
/// A new instance of [`GithubHeadingIdGenerator`] should be created for each document
/// containing headers, then [`GithubHeadingIdGenerator::generate`] should be called
/// for each heading in the document.
#[derive(Debug, Clone, Default)]
pub struct GithubHeadingIdGenerator {
    /// Map of ID to suffix which should be used for the *next* occurrence
    /// of that ID. If an ID is not present in this map, it means that it
    /// hasn't been seen before and no suffix is necessary.
    next_suffixes: HashMap<String, NonZeroUsize>,
}

impl GithubHeadingIdGenerator {
    /// Constructs a new [`GithubHeadingIdGenerator`].
    #[must_use]
    pub fn new() -> Self {
        Self {
            next_suffixes: HashMap::new(),
        }
    }

    /// Disambiguates the given "base" ID by appending a hyphen and a number
    /// to the ID if it conflicts with a previously-generated ID. This function
    /// will continue trying successive numbers until a conflict is avoided.
    ///
    /// This function will mutate the [`GithubHeadingIdGenerator`] to record
    /// the returned string.
    fn disambiguate(&mut self, base_id: String) -> String {
        const ONE: NonZeroUsize = NonZeroUsize::MIN;
        let mut candidate = base_id.clone();

        let this_suffix = self.next_suffixes.get(&base_id).map(|&initial_suffix| {
            (initial_suffix.into()..=usize::MAX)
                .find(|suffix| {
                    candidate.truncate(base_id.len());
                    candidate.push('-');
                    candidate.push_str(&suffix.to_string());

                    !self.next_suffixes.contains_key(&candidate)
                })
                .unwrap_or(/* in case of overflow only */ usize::MAX)
        });

        let next_suffix = ONE.saturating_add(this_suffix.unwrap_or(0));
        self.next_suffixes.insert(base_id, next_suffix);

        if this_suffix.is_some() {
            self.next_suffixes.insert(candidate.clone(), ONE);
        }

        candidate
    }

    /// Converts the given header text into a lowercase hyphen-separated
    /// string suitable for use as a fragment identifier. Additionally, this
    /// function ensures returned IDs are distinct from any earlier ID returned
    /// by this [`GithubHeadingIdGenerator::generate`].
    ///
    /// For example,
    /// ```
    /// # use lychee_lib::extract::fragments::GithubHeadingIdGenerator;
    /// let mut generator = GithubHeadingIdGenerator::new();
    /// assert_eq!(generator.generate("foo bar"), "foo-bar");
    /// assert_eq!(generator.generate("foo bar"), "foo-bar-1");
    /// assert_eq!(generator.generate("foo, bar!"), "foo-bar-2");
    /// ```
    pub fn generate(&mut self, text: &str) -> String {
        self.disambiguate(generate_without_disambiguation(text))
    }
}

#[cfg(test)]
mod tests {
    use percent_encoding::percent_decode_str;

    use super::{GithubHeadingIdGenerator, generate_without_disambiguation};

    fn unpercent(percent_str: &str) -> String {
        percent_decode_str(percent_str)
            .decode_utf8()
            .expect("percent string had invalid utf-8")
            .into_owned()
    }

    #[test]
    fn test_generate_without_disambiguation() {
        assert_eq!("a-b", generate_without_disambiguation("a b"));
        assert_eq!(
            unpercent("%EF%B8%8F⃣-b"),
            generate_without_disambiguation("#️⃣ b")
        );
        assert_eq!(
            unpercent("%EF%B8%8F-c"),
            generate_without_disambiguation("☔️ c")
        );
        assert_eq!(
            unpercent("🅰%EF%B8%8F-d"),
            generate_without_disambiguation("🅰️ d")
        );

        assert_eq!(
            unpercent("à-á-â-ã-ä-å-or-à-á-â-ã-ä-å"),
            generate_without_disambiguation("À, Á, Â, Ã, Ä, Å or à, á, â, ã, ä, å")
        );
    }

    #[test]
    fn test_generate_kebab_case() {
        let check = |input, expected| {
            let actual = generate_without_disambiguation(input);
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
    fn test_github_generate_with_repeats() {
        let headings = ["foo 1", "foo", "foo", "foo", "foo 1", "FOO 1"];
        let expected = vec!["foo-1", "foo", "foo-2", "foo-3", "foo-1-1", "foo-1-2"];
        let mut generator = GithubHeadingIdGenerator::new();
        assert_eq!(
            expected,
            headings
                .iter()
                .map(|h| generator.generate(h))
                .collect::<Vec<_>>()
        );
    }
}
