//! Converts heading text into hyphen-separated strings for use as fragment identifiers,
//! mimicking the algorithm which GitHub uses for generating Markdown fragment IDs.
//!
//! This is from the [`html-pipeline` Ruby library][1], which is cited in [markdownlint].
//! markdownlint doesn't provide a justification for this citation, but the change was
//! [committed][] by someone who works at Microsoft so I think it's likely this is the
//! real algorithm which is used. This also lines up with GitHub being built in Ruby.
//!
//! There is also [Flet/github-slugger][], but their regex is based on observation and
//! experiments, and we find there are discrepancies which suggest `html-pipeline` is
//! more likely.
//!
//! [1]: https://github.com/gjtorikian/html-pipeline/blob/f13a1534cb650ba17af400d1acd3a22c28004c09/lib/html/pipeline/toc_filter.rb#L30
//! [markdownlint]: https://github.com/DavidAnson/markdownlint/blob/v0.40.0/doc/md051.md
//! [committed]: https://github.com/DavidAnson/markdownlint/commit/30353cc733561af72bf5d226105429c07b43a666
//! [Flet/github-slugger]: https://github.com/Flet/github-slugger
use std::{collections::HashMap, num::NonZeroUsize, sync::LazyLock};

use regex::Regex;

/// From <https://github.com/gjtorikian/html-pipeline/blob/f13a1534cb650ba17af400d1acd3a22c28004c09/lib/html/pipeline/toc_filter.rb#L30>
static REGEX_TO_REMOVE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"[^\w -]").expect("fragment regex failed"));

/// Converts the given header text into a hyphen-separated fragment ID, mimicking
/// the algorithm used by GitHub. However, does not guarantee that the returned
/// IDs are unique between calls. For most uses, [`GithubHeadingIdGenerator`]
/// should be used instead.
pub fn generate_without_disambiguation(text: &str) -> String {
    // Rust's to_lowercase handles the special cases as in
    // <https://www.unicode.org/Public/3.2-Update/SpecialCasing-3.2.0.txt>,
    // but GitHub's algorithm does not, presumably because it is implemented
    // in Ruby: https://ruby-doc.org/3.4.1/case_mapping_rdoc.html#label-Default+Case+Mapping
    REGEX_TO_REMOVE
        .split(text)
        .flat_map(str::chars)
        .map(|c| {
            match c {
                ' ' => '-',
                '\u{0130}' => 'i', // U+0130 LATIN CAPITAL LETTER I WITH DOT ABOVE
                'Σ' => 'σ',        // U+03A3 GREEK CAPITAL LETTER SIGMA
                c => c,
            }
        })
        .collect::<String>()
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
    use rstest::rstest;

    use super::{GithubHeadingIdGenerator, generate_without_disambiguation};

    fn unpercent(percent_str: &str) -> String {
        percent_decode_str(percent_str)
            .decode_utf8()
            .expect("percent string had invalid utf-8")
            .into_owned()
    }

    #[rstest]
    #[case(" a b", "-a-b")]
    #[case("A Heading", "a-heading")]
    #[case(
        "This header has a :thumbsup: in it",
        "this-header-has-a-thumbsup-in-it"
    )]
    #[case(
        "Header with 한글 characters (using unicode)",
        "header-with-한글-characters-using-unicode"
    )]
    #[case(
        "Underscores foo_bar_, dots . and numbers 1.7e-3",
        "underscores-foo_bar_-dots--and-numbers-17e-3"
    )]
    #[case("Many          spaces", "many----------spaces")]
    #[case("À, Á, Â, Ã, Ä, Å or à, á, â, ã, ä, å", "à-á-â-ã-ä-å-or-à-á-â-ã-ä-å")]
    // Regression tests for https://github.com/lycheeverse/lychee/issues/2112
    #[case::emoji_variation_selector_kept("#️⃣ b", unpercent("%EF%B8%8F⃣-b"))]
    #[case::emoji_variation_selector_kept("☔️ c", unpercent("%EF%B8%8F-c"))]
    #[case::alphabetic_emoji_kept("🅰️ d", unpercent("🅰%EF%B8%8F-d"))]
    // Should NOT apply Unicode's special casing rules: https://www.unicode.org/Public/3.2-Update/SpecialCasing-3.2.0.txt
    #[case::capital_dotted_i("aİb", "aib")]
    #[case::sigma_final_position(
        "ΝΑΤΟΥ, ΓΙΑΝΝΗΣ",
        unpercent("%CE%BD%CE%B1%CF%84%CE%BF%CF%85-%CE%B3%CE%B9%CE%B1%CE%BD%CE%BD%CE%B7%CF%83")
    )]
    #[case::sigma_nonfinal_position(
        "Σκοπός κάθε",
        unpercent("%CF%83%CE%BA%CE%BF%CF%80%CF%8C%CF%82-%CE%BA%CE%AC%CE%B8%CE%B5")
    )]
    // Case missed by github-slugger's algorithm
    #[case::zero_width_joiners(
        "joiners a\u{200c} b\u{200d} end",
        unpercent("joiners-a%E2%80%8C-b%E2%80%8D-end")
    )]
    fn test_generate_without_disambiguation(#[case] input: String, #[case] expected: String) {
        assert_eq!(expected, generate_without_disambiguation(&input));
    }

    /// Tests suffixes when repeated IDs occur.
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
