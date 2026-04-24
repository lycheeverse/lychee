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
    #[case("a\u{1e2ff}b", "ab")]
    #[case(
        // spellchecker:ignore-next-line
        "a\u{378}-\u{105cb}-\u{11841}-\u{12822}-\u{13486}-\u{13cb1}-\u{14724}-\u{14f4f}-\u{1577b}-\u{15fa6}-\u{167d2}-\u{18ee3}-\u{1970f}-\u{19f3a}-\u{1a766}-\u{1af91}-\u{1ba6f}-\u{1c32d}-\u{1cb59}-\u{1dcaf}-\u{1e583}-\u{1efeb}-\u{2ec58}-\u{2f483}-\u{2fecd}-\u{306f8}-\u{30f24}-\u{3174f}-\u{31f7b}-\u{337fd}-\u{34029}-\u{34854}-\u{35080}-\u{358ab}-\u{360d7}-\u{36902}-\u{3712e}-\u{37959}-\u{38185}-\u{389b0}-\u{391dc}-\u{39a07}-\u{3a233}-\u{3aa5e}-\u{3b28a}-\u{3bab5}-\u{3c2e1}-\u{3cb0c}-\u{3d338}-\u{3db63}-\u{3e38f}-\u{3ebba}-\u{3f3e6}-\u{3fc11}-\u{4043d}-\u{40c68}-\u{41494}-\u{41cbf}-\u{424eb}-\u{42d16}-\u{43542}-\u{43d6d}-\u{44599}-\u{44dc4}-\u{455f0}-\u{45e1b}-\u{46647}-\u{46e72}-\u{4769e}-\u{47ec9}-\u{486f5}-\u{48f20}-\u{4974c}-\u{49f77}-\u{4a7a3}-\u{4afce}-\u{4b7fa}-\u{4c025}-\u{4c851}-\u{4d07c}-\u{4d8a8}-\u{4e0d3}-\u{4e8ff}-\u{4f12a}-\u{4f956}-\u{50181}-\u{509ad}-\u{511d8}-\u{51a04}-\u{5222f}-\u{52a5b}-\u{53286}-\u{53ab2}-\u{542dd}-\u{54b09}-\u{55334}-\u{55b60}-\u{5638b}-\u{56bb7}-\u{573e3}-\u{57c0e}-\u{5843a}-\u{58c65}-\u{59491}-\u{59cbc}-\u{5a4e8}-\u{5ad13}-\u{5b53f}-\u{5bd6a}-\u{5c596}-\u{5cdc1}-\u{5d5ed}-\u{5de18}-\u{5e644}-\u{5ee6f}-\u{5f69b}-\u{5fec6}-\u{606f2}-\u{60f1d}-\u{61749}-\u{61f74}-\u{627a0}-\u{62fcb}-\u{637f7}-\u{64022}-\u{6484e}-\u{65079}-\u{658a5}-\u{660d0}-\u{668fc}-\u{67127}-\u{67953}-\u{6817e}-\u{689aa}-\u{691d5}-\u{69a01}-\u{6a22c}-\u{6aa58}-\u{6b283}-\u{6baaf}-\u{6c2da}-\u{6cb06}-\u{6d331}-\u{6db5d}-\u{6e388}-\u{6ebb4}-\u{6f3df}-\u{6fc0b}-\u{70436}-\u{70c62}-\u{7148d}-\u{71cb9}-\u{724e4}-\u{72d10}-\u{7353b}-\u{73d67}-\u{74592}-\u{74dbe}-\u{755e9}-\u{75e15}-\u{76640}-\u{76e6c}-\u{77697}-\u{77ec3}-\u{786ee}-\u{78f1a}-\u{79745}-\u{79f71}-\u{7a79c}-\u{7afc8}-\u{7b7f3}-\u{7c01f}-\u{7c84a}-\u{7d076}-\u{7d8a1}-\u{7e0cd}-\u{7e8f8}-\u{7f124}-\u{7f94f}-\u{8017b}-\u{809a6}-\u{811d2}-\u{819fd}-\u{82229}-\u{82a54}-\u{83280}-\u{83aab}-\u{842d7}-\u{84b02}-\u{8532e}-\u{85b59}-\u{86385}-\u{86bb0}-\u{873dc}-\u{87c07}-\u{88433}-\u{88c5e}-\u{8948a}-\u{89cb6}-\u{8a4e1}-\u{8ad0d}-\u{8b538}-\u{8bd64}-\u{8c58f}-\u{8cdbb}-\u{8d5e6}-\u{8de12}-\u{8e63d}-\u{8ee69}-\u{8f694}-\u{8fec0}-\u{906eb}-\u{90f17}-\u{91742}-\u{91f6e}-\u{92799}-\u{92fc5}-\u{937f0}-\u{9401c}-\u{94847}-\u{95073}-\u{9589e}-\u{960ca}-\u{968f5}-\u{97121}-\u{9794c}-\u{98178}-\u{989a3}-\u{991cf}-\u{999fa}-\u{9a226}-\u{9aa51}-\u{9b27d}-\u{9baa8}-\u{9c2d4}-\u{9caff}-\u{9d32b}-\u{9db56}-\u{9e382}-\u{9ebad}-\u{9f3d9}-\u{9fc04}-\u{a0430}-\u{a0c5b}-\u{a1487}-\u{a1cb2}-\u{a24de}-\u{a2d09}-\u{a3535}-\u{a3d60}-\u{a458c}-\u{a4db7}-\u{a55e3}-\u{a5e0e}-\u{a663a}-\u{a6e65}-\u{a7691}-\u{a7ebc}-\u{a86e8}-\u{a8f13}-\u{a973f}-\u{a9f6a}-\u{aa796}-\u{aafc1}-\u{ab7ed}-\u{ac018}-\u{ac844}-\u{ad06f}-\u{ad89b}-\u{ae0c6}-\u{ae8f2}-\u{af11d}-\u{af949}-\u{b0174}-\u{b09a0}-\u{b11cb}-\u{b19f7}-\u{b2222}-\u{b2a4e}-\u{b3279}-\u{b3aa5}-\u{b42d0}-\u{b4afc}-\u{b5327}-\u{b5b53}-\u{b637e}-\u{b6baa}-\u{b73d5}-\u{b7c01}-\u{b842c}-\u{b8c58}-\u{b9483}-\u{b9caf}-\u{ba4da}-\u{bad06}-\u{bb531}-\u{bbd5d}-\u{bc588}-\u{bcdb4}-\u{bd5e0}-\u{bde0b}-\u{be637}-\u{bee62}-\u{bf68e}-\u{bfeb9}-\u{c06e5}-\u{c0f10}-\u{c173c}-\u{c1f67}-\u{c2793}-\u{c2fbe}-\u{c37ea}-\u{c4015}-\u{c4841}-\u{c506c}-\u{c5898}-\u{c60c3}-\u{c68ef}-\u{c711a}-\u{c7946}-\u{c8171}-\u{c899d}-\u{c91c8}-\u{c99f4}-\u{ca21f}-\u{caa4b}-\u{cb276}-\u{cbaa2}-\u{cc2cd}-\u{ccaf9}-\u{cd324}-\u{cdb50}-\u{ce37b}-\u{ceba7}-\u{cf3d2}-\u{cfbfe}-\u{d0429}-\u{d0c55}-\u{d1480}-\u{d1cac}-\u{d24d7}-\u{d2d03}-\u{d352e}-\u{d3d5a}-\u{d4585}-\u{d4db1}-\u{d55dc}-\u{d5e08}-\u{d6633}-\u{d6e5f}-\u{d768a}-\u{d7eb6}-\u{d86e1}-\u{d8f0d}-\u{d9738}-\u{d9f64}-\u{da78f}-\u{dafbb}-\u{db7e6}-\u{dc012}-\u{dc83d}-\u{dd069}-\u{dd894}-\u{de0c0}-\u{de8eb}-\u{df117}-\u{df942}-\u{e02bf}-\u{e0aea}-\u{e1316}-\u{e1b41}-\u{e236d}-\u{e2b98}-\u{e33c4}-\u{e3bef}-\u{e441b}-\u{e4c46}-\u{e5472}-\u{e5c9d}-\u{e64c9}-\u{e6cf4}-\u{e7520}-\u{e7d4b}-\u{e8577}-\u{e8da2}-\u{e95ce}-\u{e9df9}-\u{ea625}-\u{eae50}-\u{eb67c}-\u{ebea7}-\u{ec6d3}-\u{ecefe}-\u{ed72a}-\u{edf55}-\u{ee781}-\u{eefac}-\u{ef7d8}-\u{10ffff}b",
        unpercent(
            "a-%F0%90%97%8B---%F0%93%92%86-%F0%93%B2%B1-----------------%F0%AE%B1%98---%F0%B0%9B%B8-%F0%B0%BC%A4-%F0%B1%9D%8F-%F0%B1%BD%BB----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------b"
        )
    )]
    // These codepoints were assigned as Letter in Unicode 17.0, but Rust regex
    // currently uses 16.0. When it's updated to 17.0, this test will break and
    // the expected string should be replaced with the commented text.
    #[case::unicode_17_letters(
        // spellchecker:ignore-next-line
        "a\u{327a6}-\u{32fd2}b",
        "a-b" // unpercent("a%F0%B2%9E%A6-%F0%B2%BF%92b")
    )]
    fn test_generate_without_disambiguation(#[case] input: String, #[case] expected: String) {
        let actual = generate_without_disambiguation(&input);

        let p = |s: &str| {
            if s.is_empty() {
                "<empty>".to_string()
            } else {
                s.chars().flat_map(char::escape_default).collect::<String>()
            }
        };
        if expected != actual {
            for (exp, act) in expected.split('-').zip(actual.split('-')) {
                if exp != act {
                    println!("expected={}, actual={}", p(exp), p(act));
                }
            }
        }
        assert_eq!(expected, actual);
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
