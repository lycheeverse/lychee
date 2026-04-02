//! `WikiLink` Module
//!
//! This module contains a Indexer and a Resolver for `WikiLinks`
//! The Indexer recursively indexes the subdirectories and files in a given base-directory mapping
//! the filename to the full path
//! The Resolver looks for found `WikiLinks` in the Index thus resolving the `WikiLink` to a full
//! filepath

pub(crate) mod index;
pub(crate) mod resolver;

use crate::ErrorKind;
use pulldown_cmark::CowStr;

/// In Markdown Links both '#' and '|' act as modifiers
/// '#' links to a specific Header in a file
/// '|' is used to modify the link name, a so called "pothole"
const MARKDOWN_FRAGMENT_MARKER: char = '#';
const MARKDOWN_POTHOLE_MARKER: char = '|';

/// Clean a `WikiLink` by removing potholes and fragments from a `&str`
pub(crate) fn wikilink(input: &str, has_pothole: bool) -> Result<CowStr<'_>, ErrorKind> {
    // Strip pothole marker (|) and pothole (text after marker) from wikilinks
    let mut stripped_input = if has_pothole {
        pulldown_cmark::CowStr::Borrowed(
            &input[0..input.find(MARKDOWN_POTHOLE_MARKER).unwrap_or(input.len())],
        )
    } else {
        CowStr::Borrowed(input)
    };

    // Strip fragments (#) from wikilinks, according to the obsidian spec
    // fragments always come before potholes
    // https://help.obsidian.md/links#Change+the+link+display+text
    if stripped_input.contains(MARKDOWN_FRAGMENT_MARKER) {
        stripped_input = pulldown_cmark::CowStr::Borrowed(
            // In theory a second '#' could be inserted into the pothole, so searching for the
            // first occurrence from the left should yield the correct result
            &input[0..input.find(MARKDOWN_FRAGMENT_MARKER).unwrap_or(input.len())],
        );
    }
    if stripped_input.is_empty() {
        return Err(ErrorKind::EmptyUrl);
    }
    Ok(stripped_input)
}

#[cfg(test)]
mod tests {
    use pulldown_cmark::CowStr;
    use rstest::rstest;

    use crate::checker::wikilink::wikilink;

    // All these Links are missing the targetname itself but contain valid fragment- and
    // pothole-modifications. They would be parsed as an empty Link
    #[rstest]
    #[case("|foo", true)]
    #[case("|foo#bar", true)]
    #[case("|foo#bar|foo#bar", true)]
    #[case("#baz", false)]
    #[case("#baz#baz|foo", false)]
    fn test_empty_wikilinks_are_detected(#[case] input: &str, #[case] has_pothole: bool) {
        let result = wikilink(input, has_pothole);
        assert!(result.is_err());
    }

    #[rstest]
    #[case("link with spaces", true, "link with spaces")]
    #[case("foo.fileextension", true, "foo.fileextension")]
    #[case("specialcharacters !_@$&(){}", true, "specialcharacters !_@$&(){}")]
    fn test_valid_wikilinks(#[case] input: &str, #[case] has_pothole: bool, #[case] actual: &str) {
        let result = wikilink(input, has_pothole).unwrap();
        let actual = CowStr::Borrowed(actual);
        assert_eq!(result, actual);
    }

    #[rstest]
    #[case("foo|bar", true, "foo")]
    #[case("foo#bar", true, "foo")]
    #[case("foo#bar|baz", false, "foo")]
    #[case("foo#bar|baz#hashtag_in_pothole", false, "foo")]
    #[case("foo with spaces#bar|baz#hashtag_in_pothole", false, "foo with spaces")]
    #[case(
        "specialcharacters !_@$&(){}#bar|baz#hashtag_in_pothole",
        true,
        "specialcharacters !_@$&(){}"
    )]
    fn test_fragment_and_pothole_removal(
        #[case] input: &str,
        #[case] has_pothole: bool,
        #[case] actual: &str,
    ) {
        let result = wikilink(input, has_pothole).unwrap();
        let actual = CowStr::Borrowed(actual);
        assert_eq!(result, actual);
    }
}
