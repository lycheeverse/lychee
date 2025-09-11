use crate::types::{FileType, InputContent, uri::raw::RawUri};

pub mod html;
pub mod markdown;
mod plaintext;

use markdown::extract_markdown;
use plaintext::extract_raw_uri_from_plaintext;

/// A handler for extracting links from various input formats like Markdown and
/// HTML. Allocations should be avoided if possible as this is a
/// performance-critical section of the library.
#[derive(Default, Debug, Clone, Copy)]
pub struct Extractor {
    use_html5ever: bool,
    include_verbatim: bool,
    include_wikilinks: bool,
}

impl Extractor {
    /// Creates a new extractor
    ///
    /// The extractor can be configured with the following settings:
    ///
    /// - `use_html5ever` enables the alternative HTML parser engine html5ever, that
    ///   is also used in the Servo browser by Mozilla.
    ///   The default is `html5gum`, which is more performant and well maintained.
    ///
    /// - `include_verbatim` ignores links inside Markdown code blocks.
    ///   These can be denoted as a block starting with three backticks or an indented block.
    ///   For more information, consult the `pulldown_cmark` documentation about code blocks
    ///   [here](https://docs.rs/pulldown-cmark/latest/pulldown_cmark/enum.CodeBlockKind.html)
    #[must_use]
    pub const fn new(use_html5ever: bool, include_verbatim: bool, include_wikilinks: bool) -> Self {
        Self {
            use_html5ever,
            include_verbatim,
            include_wikilinks,
        }
    }

    /// Main entrypoint for extracting links from various sources
    /// (Markdown, HTML, and plaintext)
    #[must_use]
    pub fn extract(&self, input_content: &InputContent) -> Vec<RawUri> {
        match input_content.file_type {
            FileType::Markdown => extract_markdown(
                &input_content.content,
                self.include_verbatim,
                self.include_wikilinks,
            ),
            FileType::Html => {
                if self.use_html5ever {
                    html::html5ever::extract_html(&input_content.content, self.include_verbatim)
                } else {
                    html::html5gum::extract_html(&input_content.content, self.include_verbatim)
                }
            }
            FileType::Plaintext => extract_raw_uri_from_plaintext(&input_content.content),
        }
    }
}

#[cfg(test)]
mod tests {
    use pretty_assertions::assert_eq;
    use reqwest::Url;
    use std::{collections::HashSet, path::Path};
    use test_utils::{fixtures_path, load_fixture, mail, website};

    use super::*;
    use crate::{
        Uri,
        types::{FileType, InputContent, ResolvedInputSource},
        utils::url::find_links,
    };

    fn extract_uris(input: &str, file_type: FileType) -> HashSet<Uri> {
        let input_content = InputContent::from_string(input, file_type);

        let extractor = Extractor::new(false, false, false);
        let uris_html5gum: HashSet<Uri> = extractor
            .extract(&input_content)
            .into_iter()
            .filter_map(|raw_uri| Uri::try_from(raw_uri).ok())
            .collect();
        let uris_html5gum_sorted: Vec<Uri> = {
            let mut uris = uris_html5gum.clone().into_iter().collect::<Vec<_>>();
            uris.sort();
            uris
        };

        let extractor = Extractor::new(true, false, false);
        let uris_html5ever: HashSet<Uri> = extractor
            .extract(&input_content)
            .into_iter()
            .filter_map(|raw_uri| Uri::try_from(raw_uri).ok())
            .collect();
        let uris_html5ever_sorted: Vec<Uri> = {
            let mut uris = uris_html5ever.into_iter().collect::<Vec<_>>();
            uris.sort();
            uris
        };

        assert_eq!(
            uris_html5gum_sorted, uris_html5ever_sorted,
            "Mismatch between html5gum and html5ever"
        );
        uris_html5gum
    }

    #[test]
    fn verbatim_elem() {
        let input = "<pre>https://example.com</pre>";
        let uris = extract_uris(input, FileType::Html);
        assert!(uris.is_empty());
    }

    #[test]
    fn test_file_type() {
        assert_eq!(FileType::from(Path::new("/")), FileType::Plaintext);
        assert_eq!(FileType::from("test.md"), FileType::Markdown);
        assert_eq!(FileType::from("test.markdown"), FileType::Markdown);
        assert_eq!(FileType::from("test.html"), FileType::Html);
        assert_eq!(FileType::from("test.txt"), FileType::Plaintext);
        assert_eq!(FileType::from("test.something"), FileType::Plaintext);
        assert_eq!(
            FileType::from("/absolute/path/to/test.something"),
            FileType::Plaintext
        );
    }

    #[test]
    fn test_skip_markdown_anchors() {
        let links = extract_uris("This is [a test](#lol).", FileType::Markdown);

        assert!(links.is_empty());
    }

    #[test]
    fn test_skip_markdown_internal_urls() {
        let links = extract_uris("This is [a test](./internal).", FileType::Markdown);

        assert!(links.is_empty());
    }

    #[test]
    fn test_skip_markdown_email() {
        let input = "Get in touch - [Contact Us](mailto:test@test.com)";
        let links = extract_uris(input, FileType::Markdown);
        let expected = IntoIterator::into_iter([mail!("test@test.com")]).collect::<HashSet<Uri>>();

        assert_eq!(links, expected);
    }

    #[test]
    fn relative_urls() {
        let links = extract_uris("This is [a test](/internal).", FileType::Markdown);

        assert!(links.is_empty());
    }

    #[test]
    fn test_non_markdown_links() {
        let input =
            "https://endler.dev and https://hello-rust.show/foo/bar?lol=1 at test@example.com";
        let links: HashSet<Uri> = extract_uris(input, FileType::Plaintext);

        let expected = IntoIterator::into_iter([
            website!("https://endler.dev"),
            website!("https://hello-rust.show/foo/bar?lol=1"),
            mail!("test@example.com"),
        ])
        .collect::<HashSet<Uri>>();

        assert_eq!(links, expected);
    }

    #[test]
    fn test_md_escape() {
        let input = r"http://msdn.microsoft.com/library/ie/ms535874\(v=vs.85\).aspx";
        let links: Vec<_> = find_links(input).collect();
        let expected = "http://msdn.microsoft.com/library/ie/ms535874(v=vs.85).aspx)";

        matches!(&links[..], [link] if link.as_str() == expected);
    }

    #[test]
    fn test_extract_html5_not_valid_xml() {
        let input = load_fixture!("TEST_HTML5.html");
        let links = extract_uris(&input, FileType::Html);

        let expected_links = IntoIterator::into_iter([
            website!("https://example.com/head/home"),
            website!("https://example.com/css/style_full_url.css"),
            // the body links wouldn't be present if the file was parsed strictly as XML
            website!("https://example.com/body/a"),
            website!("https://example.com/body/div_empty_a"),
        ])
        .collect::<HashSet<Uri>>();

        assert_eq!(links, expected_links);
    }

    #[test]
    fn test_extract_relative_url() {
        let source = ResolvedInputSource::RemoteUrl(Box::new(
            Url::parse("https://example.com/some-post").unwrap(),
        ));

        let contents = r#"<html>
            <div class="row">
                <a href="https://github.com/lycheeverse/lychee/">GitHub</a>
                <a href="/about">About</a>
            </div>
        </html>"#;

        let input_content = &InputContent {
            source,
            file_type: FileType::Html,
            content: contents.to_string(),
        };

        for use_html5ever in [true, false] {
            let extractor = Extractor::new(use_html5ever, false, false);
            let links = extractor.extract(input_content);

            let urls = links
                .into_iter()
                .map(|raw_uri| raw_uri.text)
                .collect::<HashSet<_>>();

            let expected_urls = IntoIterator::into_iter([
                String::from("https://github.com/lycheeverse/lychee/"),
                String::from("/about"),
            ])
            .collect::<HashSet<_>>();

            assert_eq!(urls, expected_urls);
        }
    }

    #[test]
    fn test_extract_html5_lowercase_doctype() {
        // this has been problematic with previous XML based parser
        let input = load_fixture!("TEST_HTML5_LOWERCASE_DOCTYPE.html");
        let links = extract_uris(&input, FileType::Html);

        let expected_links = IntoIterator::into_iter([website!("https://example.com/body/a")])
            .collect::<HashSet<Uri>>();

        assert_eq!(links, expected_links);
    }

    #[test]
    fn test_extract_html5_minified() {
        // minified HTML with some quirky elements such as href attribute values specified without quotes
        let input = load_fixture!("TEST_HTML5_MINIFIED.html");
        let links = extract_uris(&input, FileType::Html);

        let expected_links = IntoIterator::into_iter([
            website!("https://example.com/"),
            website!("https://example.com/favicon.ico"),
            // Note that we exclude `preconnect` links:
            // website!("https://fonts.externalsite.com"),
            website!("https://example.com/docs/"),
            website!("https://example.com/forum"),
        ])
        .collect::<HashSet<Uri>>();

        assert_eq!(links, expected_links);
    }

    #[test]
    fn test_extract_html5_malformed() {
        // malformed links shouldn't stop the parser from further parsing
        let input = load_fixture!("TEST_HTML5_MALFORMED_LINKS.html");
        let links = extract_uris(&input, FileType::Html);

        let expected_links = IntoIterator::into_iter([website!("https://example.com/valid")])
            .collect::<HashSet<Uri>>();

        assert_eq!(links, expected_links);
    }

    #[test]
    fn test_extract_html5_custom_elements() {
        // the element name shouldn't matter for attributes like href, src, cite etc
        let input = load_fixture!("TEST_HTML5_CUSTOM_ELEMENTS.html");
        let links = extract_uris(&input, FileType::Html);

        let expected_links = IntoIterator::into_iter([
            website!("https://example.com/some-weird-element"),
            website!("https://example.com/even-weirder-src"),
            website!("https://example.com/even-weirder-href"),
            website!("https://example.com/citations"),
        ])
        .collect::<HashSet<Uri>>();

        assert_eq!(links, expected_links);
    }

    #[test]
    fn test_extract_urls_with_at_sign_properly() {
        // note that these used to parse as emails
        let input = "https://example.com/@test/test http://otherdomain.com/test/@test".to_string();
        let links = extract_uris(&input, FileType::Plaintext);

        let expected_links = IntoIterator::into_iter([
            website!("https://example.com/@test/test"),
            website!("http://otherdomain.com/test/@test"),
        ])
        .collect::<HashSet<Uri>>();

        assert_eq!(links, expected_links);
    }

    #[test]
    fn test_extract_link_at_end_of_line() {
        let input = "https://www.apache.org/licenses/LICENSE-2.0\n";
        let links = extract_uris(input, FileType::Plaintext);

        let expected_links =
            IntoIterator::into_iter([website!("https://www.apache.org/licenses/LICENSE-2.0")])
                .collect::<HashSet<Uri>>();

        assert_eq!(links, expected_links);
    }
}
