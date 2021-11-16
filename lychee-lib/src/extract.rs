use std::{collections::HashSet, convert::TryFrom, path::Path, path::PathBuf};

use html5ever::{
    parse_document,
    tendril::{StrTendril, TendrilSink},
};
use log::info;
use markup5ever_rcdom::{Handle, NodeData, RcDom};
use percent_encoding::percent_decode_str;
use pulldown_cmark::{Event as MDEvent, Parser, Tag};
use reqwest::Url;

use crate::{
    helpers::{path, url},
    types::{FileType, InputContent},
    Base, ErrorKind, Input, Request, Result, Uri,
};

/// A handler for extracting links from various input formats like Markdown and
/// HTML. Allocations are avoided if possible as this is a performance-critical
/// section of the library.
#[derive(Debug)]
pub struct Extractor {
    /// URLs extracted from input
    pub urls: Vec<StrTendril>,
    /// Base URL or Path
    pub base: Option<Base>,
}

impl Extractor {
    pub(crate) const fn new(base: Option<Base>) -> Self {
        Extractor {
            urls: Vec::new(),
            base,
        }
    }

    /// Main entrypoint for extracting links from various sources
    /// (Markdown, HTML, and plaintext)
    pub(crate) fn extract(&mut self, input_content: &InputContent) -> Result<HashSet<Request>> {
        match input_content.file_type {
            FileType::Markdown => self.extract_markdown(&input_content.content),
            FileType::Html => self.extract_html(&input_content.content),
            FileType::Plaintext => self.extract_plaintext(&input_content.content),
        };
        self.create_requests(input_content)
    }

    /// Create requests out of the collected URLs.
    /// Only keeps legit URLs. For example this filters out anchors.
    fn create_requests(&self, input_content: &InputContent) -> Result<HashSet<Request>> {
        let mut requests: HashSet<Request> = HashSet::with_capacity(self.urls.len());

        let base_input = match &input_content.input {
            Input::RemoteUrl(url) => Some(Url::parse(&format!(
                "{}://{}",
                url.scheme(),
                url.host_str().ok_or(ErrorKind::InvalidUrlHost)?
            ))?),
            _ => None,
            // other inputs do not have a URL to extract a base
        };

        for url in &self.urls {
            let req = if let Ok(uri) = Uri::try_from(url.as_ref()) {
                Request::new(uri, input_content.input.clone())
            } else if let Some(url) = self.base.as_ref().and_then(|u| u.join(url)) {
                Request::new(Uri { url }, input_content.input.clone())
            } else if let Input::FsPath(root) = &input_content.input {
                if url::is_anchor(url) {
                    // Silently ignore anchor links for now
                    continue;
                }
                match self.create_uri_from_path(root, url)? {
                    Some(url) => Request::new(Uri { url }, input_content.input.clone()),
                    None => {
                        // In case we cannot create a URI from a path but we didn't receive an error,
                        // it means that some preconditions were not met, e.g. the `base_url` wasn't set.
                        continue;
                    }
                }
            } else if let Some(url) = base_input.as_ref().map(|u| u.join(url)) {
                if self.base.is_some() {
                    continue;
                }
                Request::new(Uri { url: url? }, input_content.input.clone())
            } else {
                info!("Handling of {} not implemented yet", &url);
                continue;
            };
            requests.insert(req);
        }
        Ok(requests)
    }

    /// Extract unparsed URL strings from a Markdown string.
    fn extract_markdown(&mut self, input: &str) {
        let parser = Parser::new(input);
        for event in parser {
            match event {
                MDEvent::Start(Tag::Link(_, url, _) | Tag::Image(_, url, _)) => {
                    self.urls.push(StrTendril::from(url.as_ref()));
                }
                MDEvent::Text(txt) => self.extract_plaintext(&txt),
                MDEvent::Html(html) => self.extract_html(&html.to_string()),
                _ => {}
            }
        }
    }

    /// Extract unparsed URL strings from an HTML string.
    fn extract_html(&mut self, input: &str) {
        let tendril = StrTendril::from(input);
        let rc_dom = parse_document(RcDom::default(), html5ever::ParseOpts::default()).one(tendril);

        self.walk_html_links(&rc_dom.document);
    }

    /// Recursively walk links in a HTML document, aggregating URL strings in `urls`.
    fn walk_html_links(&mut self, node: &Handle) {
        match node.data {
            NodeData::Text { ref contents } => {
                self.extract_plaintext(&contents.borrow());
            }
            NodeData::Comment { ref contents } => {
                self.extract_plaintext(contents);
            }
            NodeData::Element {
                ref name,
                ref attrs,
                ..
            } => {
                for attr in attrs.borrow().iter() {
                    let urls = url::extract_links_from_elem_attr(
                        attr.name.local.as_ref(),
                        name.local.as_ref(),
                        attr.value.as_ref(),
                    );

                    if urls.is_empty() {
                        self.extract_plaintext(&attr.value);
                    } else {
                        self.urls.extend(urls.into_iter().map(StrTendril::from));
                    }
                }
            }
            _ => {}
        }

        // recursively traverse the document's nodes -- this doesn't need any extra
        // exit conditions, because the document is a tree
        for child in node.children.borrow().iter() {
            self.walk_html_links(child);
        }
    }

    /// Extract unparsed URL strings from plaintext
    fn extract_plaintext(&mut self, input: &str) {
        self.urls
            .extend(url::find_links(input).map(|l| StrTendril::from(l.as_str())));
    }

    fn create_uri_from_path(&self, src: &Path, dst: &str) -> Result<Option<Url>> {
        let dst = url::remove_get_params_and_fragment(dst);
        // Avoid double-encoding already encoded destination paths by removing any
        // potential encoding (e.g. `web%20site` becomes `web site`).
        // That's because Url::from_file_path will encode the full URL in the end.
        // This behavior cannot be configured.
        // See https://github.com/lycheeverse/lychee/pull/262#issuecomment-915245411
        // TODO: This is not a perfect solution.
        // Ideally, only `src` and `base` should be URL encoded (as is done by
        // `from_file_path` at the moment) while `dst` is left untouched and simply
        // appended to the end.
        let decoded = percent_decode_str(dst).decode_utf8()?;
        let resolved = path::resolve(src, &PathBuf::from(&*decoded), &self.base)?;
        match resolved {
            Some(path) => Url::from_file_path(&path)
                .map(Some)
                .map_err(|_e| ErrorKind::InvalidUrlFromPath(path)),
            None => Ok(None),
        }
    }
}

#[cfg(test)]
mod test {
    use std::{
        array,
        collections::HashSet,
        fs::File,
        io::{BufReader, Read},
        path::Path,
    };

    use pretty_assertions::assert_eq;

    use super::*;
    use crate::{
        helpers::url::find_links,
        test_utils::{mail, website},
        Uri,
    };
    use crate::{
        types::{FileType, InputContent},
        Base,
    };

    #[test]
    fn test_create_uri_from_path() {
        let extractor = Extractor::new(None);
        let result = extractor
            .create_uri_from_path(&PathBuf::from("/README.md"), "test+encoding")
            .unwrap();
        assert_eq!(result.unwrap().as_str(), "file:///test+encoding");
    }

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

    fn extract_uris(input: &str, file_type: FileType, base_url: Option<&str>) -> HashSet<Uri> {
        let base = base_url.map(|url| Base::Remote(Url::parse(url).unwrap()));
        let mut extractor = Extractor::new(base);
        extractor
            .extract(&InputContent::from_string(input, file_type))
            // unwrap is fine here as this helper function is only used in tests
            .unwrap()
            .into_iter()
            .map(|r| r.uri)
            .collect()
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
    fn test_extract_link_at_end_of_line() {
        let input = "https://www.apache.org/licenses/LICENSE-2.0\n";
        let link = input.trim_end();

        let mut extractor = Extractor::new(None);
        extractor.extract_markdown(input);
        assert_eq!(vec![StrTendril::from(link)], extractor.urls);

        let mut extractor = Extractor::new(None);
        extractor.extract_plaintext(input);
        assert_eq!(vec![StrTendril::from(link)], extractor.urls);

        let mut extractor = Extractor::new(None);
        extractor.extract_html(input);
        assert_eq!(vec![StrTendril::from(link)], extractor.urls);
    }

    #[test]
    fn test_extract_markdown_links() {
        let links = extract_uris(
            "This is [a test](https://endler.dev). This is a relative link test [Relative Link Test](relative_link)",
            FileType::Markdown,
            Some("https://github.com/hello-rust/lychee/"),
        );

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
        );

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
        );

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
        let links = extract_uris("This is [a test](#lol).", FileType::Markdown, None);

        assert!(links.is_empty());
    }

    #[test]
    fn test_skip_markdown_internal_urls() {
        let links = extract_uris("This is [a test](./internal).", FileType::Markdown, None);

        assert!(links.is_empty());
    }

    #[test]
    fn test_markdown_internal_url() {
        let base_url = "https://localhost.com/";
        let input = "This is [an internal url](@/internal.md) \
        This is [an internal url](@/internal.markdown) \
        This is [an internal url](@/internal.markdown#example) \
        This is [an internal url](@/internal.md#example)";

        let links = extract_uris(input, FileType::Markdown, Some(base_url));

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
        let links = extract_uris(input, FileType::Markdown, None);
        let expected = array::IntoIter::new([mail("test@test.com")]).collect::<HashSet<Uri>>();

        assert_eq!(links, expected);
    }

    #[test]
    fn test_non_markdown_links() {
        let input =
            "https://endler.dev and https://hello-rust.show/foo/bar?lol=1 at test@example.org";
        let links: HashSet<Uri> = extract_uris(input, FileType::Plaintext, None);

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
        let links = extract_uris(&input, FileType::Html, None);

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
        let links = extract_uris(&input, FileType::Html, Some("https://example.org"));

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
        let links = extractor.extract(input_content);
        let urls = links
            .unwrap()
            .iter()
            .map(|x| x.uri.url.as_str().to_string())
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
        let links = extract_uris(&input, FileType::Html, None);

        let expected_links =
            array::IntoIter::new([website("https://example.org/body/a")]).collect::<HashSet<Uri>>();

        assert_eq!(links, expected_links);
    }

    #[test]
    fn test_extract_html5_minified() {
        // minified HTML with some quirky elements such as href attribute values specified without quotes
        let input = load_fixture("TEST_HTML5_MINIFIED.html");
        let links = extract_uris(&input, FileType::Html, None);

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
        let links = extract_uris(&input, FileType::Html, None);

        let expected_links =
            array::IntoIter::new([website("https://example.org/valid")]).collect::<HashSet<Uri>>();

        assert_eq!(links, expected_links);
    }

    #[test]
    fn test_extract_html5_custom_elements() {
        // the element name shouldn't matter for attributes like href, src, cite etc
        let input = load_fixture("TEST_HTML5_CUSTOM_ELEMENTS.html");
        let links = extract_uris(&input, FileType::Html, None);

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
        let links = extract_uris(&input, FileType::Plaintext, None);

        let expected_links = array::IntoIter::new([
            website("https://example.com/@test/test"),
            website("http://otherdomain.com/test/@test"),
        ])
        .collect::<HashSet<Uri>>();

        assert_eq!(links, expected_links);
    }
}
