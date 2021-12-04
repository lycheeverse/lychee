use crate::{
    types::{raw_uri::RawUri, FileType, InputContent},
    Base, Result,
};

mod html;
mod markdown;
mod plaintext;

use html::extract_html;
use markdown::extract_markdown;
use plaintext::extract_plaintext;

/// A handler for extracting links from various input formats like Markdown and
/// HTML. Allocations are avoided if possible as this is a performance-critical
/// section of the library.
#[derive(Debug, Clone)]
pub struct Extractor {
    /// Base URL or Path
    pub base: Option<Base>,
}

impl Extractor {
    /// Create a new extractor
    /// Relative links will be prefixed with `base`
    pub const fn new(base: Option<Base>) -> Self {
        Extractor { base }
    }

    /// Main entrypoint for extracting links from various sources
    /// (Markdown, HTML, and plaintext)
    pub fn extract(&mut self, input_content: &InputContent) -> Result<Vec<RawUri>> {
        let urls = match input_content.file_type {
            FileType::Markdown => extract_markdown(&input_content.content),
            FileType::Html => extract_html(&input_content.content)?,
            FileType::Plaintext => extract_plaintext(&input_content.content),
        };
        Ok(urls)
    }
}

#[cfg(test)]
mod test {
    use std::{
        array,
        collections::HashSet,
        convert::TryFrom,
        fs::File,
        io::{BufReader, Read},
        path::Path,
    };

    use pretty_assertions::assert_eq;
    use reqwest::Url;

    use super::*;
    use crate::{
        helpers::url::find_links,
        test_utils::{mail, website},
        Input, Uri,
    };
    use crate::{
        types::{FileType, InputContent},
        Base,
    };

    fn load_fixture(filename: &str) -> String {
        let fixture_path = Path::new(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .unwrap()
            .join("fixtures")
            .join(filename);

        let file = File::open(fixture_path).expect("Unable to open fixture file");
        let mut buf_reader = BufReader::new(file);
        let mut content = String::new();

        buf_reader
            .read_to_string(&mut content)
            .expect("Unable to read fixture file contents");

        content
    }

    fn extract_uris(
        input: &str,
        file_type: FileType,
        base_url: Option<&str>,
    ) -> Result<HashSet<Uri>> {
        let base = base_url.map(|url| Base::Remote(Url::parse(url).unwrap()));
        let input_content = InputContent::from_string(input, file_type);
        let mut extractor = Extractor::new(base);
        let uris: Result<HashSet<_>> = extractor
            .extract(&input_content)?
            .into_iter()
            .map(|raw_uri| Uri::try_from(raw_uri))
            .collect();
        Ok(uris?)
    }

    #[test]
    fn test_file_type() {
        // FIXME: Assume plaintext in case a path has no extension
        // assert_eq!(FileType::from(Path::new("/")), FileType::Plaintext);
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
    fn test_extract_markdown_links() {
        let links = extract_uris(
            "This is [a test](https://endler.dev). This is a relative link test [Relative Link Test](relative_link)",
            FileType::Markdown,
            Some("https://github.com/hello-rust/lychee/"),
        ).unwrap();

        let expected_links = array::IntoIter::new([
            website("https://endler.dev"),
            website("https://github.com/hello-rust/lychee/relative_link"),
        ])
        .collect::<HashSet<Uri>>();

        assert_eq!(links, expected_links);
    }

    #[test]
    fn test_extract_html_links() {
        let links = extract_uris(
            r#"<html>
                <div class="row">
                    <a href="https://github.com/lycheeverse/lychee/">
                    <a href="blob/master/README.md">README</a>
                </div>
            </html>"#,
            FileType::Html,
            Some("https://github.com/lycheeverse/"),
        )
        .unwrap();

        let expected_links = array::IntoIter::new([
            website("https://github.com/lycheeverse/lychee/"),
            website("https://github.com/lycheeverse/blob/master/README.md"),
        ])
        .collect::<HashSet<Uri>>();

        assert_eq!(links, expected_links);
    }

    #[test]
    fn test_extract_html_srcset() {
        let links = extract_uris(
            r#"
            <img
                src="/static/image.png"
                srcset="
                /static/image300.png  300w,
                /static/image600.png  600w,
                "
            />
          "#,
            FileType::Html,
            Some("https://example.com/"),
        )
        .unwrap();

        let expected_links = array::IntoIter::new([
            website("https://example.com/static/image.png"),
            website("https://example.com/static/image300.png"),
            website("https://example.com/static/image600.png"),
        ])
        .collect::<HashSet<Uri>>();

        assert_eq!(links, expected_links);
    }

    #[test]
    fn test_skip_markdown_anchors() {
        let links = extract_uris("This is [a test](#lol).", FileType::Markdown, None).unwrap();

        assert!(links.is_empty());
    }

    #[test]
    fn test_skip_markdown_internal_urls() {
        let links =
            extract_uris("This is [a test](./internal).", FileType::Markdown, None).unwrap();

        assert!(links.is_empty());
    }

    #[test]
    fn test_markdown_internal_url() {
        let base_url = "https://localhost.com/";
        let input = "This is [an internal url](@/internal.md) \
        This is [an internal url](@/internal.markdown) \
        This is [an internal url](@/internal.markdown#example) \
        This is [an internal url](@/internal.md#example)";

        let links = extract_uris(input, FileType::Markdown, Some(base_url)).unwrap();

        let expected = array::IntoIter::new([
            website("https://localhost.com/@/internal.md"),
            website("https://localhost.com/@/internal.markdown"),
            website("https://localhost.com/@/internal.md#example"),
            website("https://localhost.com/@/internal.markdown#example"),
        ])
        .collect::<HashSet<Uri>>();

        assert_eq!(links, expected);
    }

    #[test]
    fn test_skip_markdown_email() {
        let input = "Get in touch - [Contact Us](mailto:test@test.com)";
        let links = extract_uris(input, FileType::Markdown, None).unwrap();
        let expected = array::IntoIter::new([mail("test@test.com")]).collect::<HashSet<Uri>>();

        assert_eq!(links, expected);
    }

    #[test]
    fn test_non_markdown_links() {
        let input =
            "https://endler.dev and https://hello-rust.show/foo/bar?lol=1 at test@example.org";
        let links: HashSet<Uri> = extract_uris(input, FileType::Plaintext, None).unwrap();

        let expected = array::IntoIter::new([
            website("https://endler.dev"),
            website("https://hello-rust.show/foo/bar?lol=1"),
            mail("test@example.org"),
        ])
        .collect::<HashSet<Uri>>();

        assert_eq!(links, expected);
    }

    #[test]
    fn test_md_escape() {
        let input = r#"http://msdn.microsoft.com/library/ie/ms535874\(v=vs.85\).aspx"#;
        let links: Vec<_> = find_links(input).collect();
        let expected = "http://msdn.microsoft.com/library/ie/ms535874(v=vs.85).aspx)";

        matches!(&links[..], [link] if link.as_str() == expected);
    }

    #[test]
    fn test_extract_html5_not_valid_xml() {
        let input = load_fixture("TEST_HTML5.html");
        let links = extract_uris(&input, FileType::Html, None).unwrap();

        let expected_links = array::IntoIter::new([
            website("https://example.org/head/home"),
            website("https://example.org/css/style_full_url.css"),
            // the body links wouldn't be present if the file was parsed strictly as XML
            website("https://example.org/body/a"),
            website("https://example.org/body/div_empty_a"),
        ])
        .collect::<HashSet<Uri>>();

        assert_eq!(links, expected_links);
    }

    #[test]
    fn test_extract_html5_not_valid_xml_relative_links() {
        let input = load_fixture("TEST_HTML5.html");
        let links = extract_uris(&input, FileType::Html, Some("https://example.org")).unwrap();

        let expected_links = array::IntoIter::new([
            website("https://example.org/head/home"),
            website("https://example.org/images/icon.png"),
            website("https://example.org/css/style_relative_url.css"),
            website("https://example.org/css/style_full_url.css"),
            website("https://example.org/js/script.js"),
            // the body links wouldn't be present if the file was parsed strictly as XML
            website("https://example.org/body/a"),
            website("https://example.org/body/div_empty_a"),
        ])
        .collect::<HashSet<Uri>>();

        assert_eq!(links, expected_links);
    }

    #[test]
    fn test_relative_url_with_base_extracted_from_input() {
        let input = Input::RemoteUrl(Box::new(
            Url::parse("https://example.org/some-post").unwrap(),
        ));

        let contents = r#"<html>
            <div class="row">
                <a href="https://github.com/lycheeverse/lychee/">Github</a>
                <a href="/about">About</a>
            </div>
        </html>"#;

        let input_content = &InputContent {
            input,
            file_type: FileType::Html,
            content: contents.to_string(),
        };

        let mut extractor = Extractor::new(None);
        let links = extractor.extract(input_content).unwrap();
        let urls = links
            .into_iter()
            .map(|raw_uri| raw_uri.text)
            .collect::<HashSet<_>>();

        let expected_urls = array::IntoIter::new([
            String::from("https://github.com/lycheeverse/lychee/"),
            String::from("https://example.org/about"),
        ])
        .collect::<HashSet<_>>();

        assert_eq!(urls, expected_urls);
    }

    #[test]
    fn test_extract_html5_lowercase_doctype() {
        // this has been problematic with previous XML based parser
        let input = load_fixture("TEST_HTML5_LOWERCASE_DOCTYPE.html");
        let links = extract_uris(&input, FileType::Html, None).unwrap();

        let expected_links =
            array::IntoIter::new([website("https://example.org/body/a")]).collect::<HashSet<Uri>>();

        assert_eq!(links, expected_links);
    }

    #[test]
    fn test_extract_html5_minified() {
        // minified HTML with some quirky elements such as href attribute values specified without quotes
        let input = load_fixture("TEST_HTML5_MINIFIED.html");
        let links = extract_uris(&input, FileType::Html, None).unwrap();

        let expected_links = array::IntoIter::new([
            website("https://example.org/"),
            website("https://example.org/favicon.ico"),
            website("https://fonts.externalsite.com"),
            website("https://example.org/docs/"),
            website("https://example.org/forum"),
        ])
        .collect::<HashSet<Uri>>();

        assert_eq!(links, expected_links);
    }

    #[test]
    fn test_extract_html5_malformed() {
        // malformed links shouldn't stop the parser from further parsing
        let input = load_fixture("TEST_HTML5_MALFORMED_LINKS.html");
        let links = extract_uris(&input, FileType::Html, None).unwrap();

        let expected_links =
            array::IntoIter::new([website("https://example.org/valid")]).collect::<HashSet<Uri>>();

        assert_eq!(links, expected_links);
    }

    #[test]
    fn test_extract_html5_custom_elements() {
        // the element name shouldn't matter for attributes like href, src, cite etc
        let input = load_fixture("TEST_HTML5_CUSTOM_ELEMENTS.html");
        let links = extract_uris(&input, FileType::Html, None).unwrap();

        let expected_links = array::IntoIter::new([
            website("https://example.org/some-weird-element"),
            website("https://example.org/even-weirder-src"),
            website("https://example.org/even-weirder-href"),
            website("https://example.org/citations"),
        ])
        .collect::<HashSet<Uri>>();

        assert_eq!(links, expected_links);
    }

    #[test]
    fn test_extract_urls_with_at_sign_properly() {
        // note that these used to parse as emails
        let input = "https://example.com/@test/test http://otherdomain.com/test/@test".to_string();
        let links = extract_uris(&input, FileType::Plaintext, None).unwrap();

        let expected_links = array::IntoIter::new([
            website("https://example.com/@test/test"),
            website("http://otherdomain.com/test/@test"),
        ])
        .collect::<HashSet<Uri>>();

        assert_eq!(links, expected_links);
    }
}
