use std::{collections::HashSet, convert::TryFrom, path::PathBuf};

use html5ever::{
    parse_document,
    tendril::{StrTendril, TendrilSink},
};
use linkify::LinkFinder;
use log::info;
use markup5ever_rcdom::{Handle, NodeData, RcDom};
use pulldown_cmark::{Event as MDEvent, Parser, Tag};
use reqwest::Url;

use crate::{
    fs,
    types::{FileType, InputContent},
    Base, ErrorKind, Input, Request, Result, Uri,
};

// Use LinkFinder here to offload the actual link searching in plaintext.
fn find_links(input: &str) -> Vec<linkify::Link> {
    let finder = LinkFinder::new();
    finder.links(input).collect()
}

/// Extract unparsed URL strings from a markdown string.
fn extract_links_from_markdown(input: &str) -> Vec<String> {
    let parser = Parser::new(input);
    parser
        .flat_map(|event| match event {
            MDEvent::Start(Tag::Link(_, url, _) | Tag::Image(_, url, _)) => vec![url.to_string()],
            MDEvent::Text(txt) => extract_links_from_plaintext(&txt.to_string()),
            MDEvent::Html(html) => extract_links_from_html(&html.to_string()),
            _ => vec![],
        })
        .collect()
}

/// Extract unparsed URL strings from a HTML string.
fn extract_links_from_html(input: &str) -> Vec<String> {
    let tendril = StrTendril::from(input);
    let rc_dom = parse_document(RcDom::default(), html5ever::ParseOpts::default()).one(tendril);

    let mut urls = Vec::new();

    // we pass mutable urls reference to avoid extra allocations in each
    // recursive descent
    walk_html_links(&mut urls, &rc_dom.document);

    urls
}

/// Recursively walk links in a HTML document, aggregating URL strings in `urls`.
fn walk_html_links(mut urls: &mut Vec<String>, node: &Handle) {
    match node.data {
        NodeData::Text { ref contents } => {
            urls.append(&mut extract_links_from_plaintext(&contents.borrow()));
        }

        NodeData::Comment { ref contents } => {
            urls.append(&mut extract_links_from_plaintext(contents));
        }

        NodeData::Element {
            ref name,
            ref attrs,
            ..
        } => {
            for attr in attrs.borrow().iter() {
                let attr_value = attr.value.to_string();

                if elem_attr_is_link(attr.name.local.as_ref(), name.local.as_ref()) {
                    urls.push(attr_value);
                } else {
                    urls.append(&mut extract_links_from_plaintext(&attr_value));
                }
            }
        }

        _ => {}
    }

    // recursively traverse the document's nodes -- this doesn't need any extra
    // exit conditions because the document is a tree
    for child in node.children.borrow().iter() {
        walk_html_links(&mut urls, child);
    }
}

/// Determine if element's attribute contains a link / URL.
fn elem_attr_is_link(attr_name: &str, elem_name: &str) -> bool {
    // See a comprehensive list of attributes that might contain URLs/URIs
    // over at: https://developer.mozilla.org/en-US/docs/Web/HTML/Attributes
    matches!(
        (attr_name, elem_name),
        ("href" | "src" | "srcset" | "cite", _) | ("data", "object") | ("onhashchange", "body")
    )
}

/// Extract unparsed URL strings from a plaintext.
fn extract_links_from_plaintext(input: &str) -> Vec<String> {
    find_links(input)
        .iter()
        .map(|l| String::from(l.as_str()))
        .collect()
}

pub(crate) fn extract_links(
    input_content: &InputContent,
    base: &Option<Base>,
) -> Result<HashSet<Request>> {
    let links = match input_content.file_type {
        FileType::Markdown => extract_links_from_markdown(&input_content.content),
        FileType::Html => extract_links_from_html(&input_content.content),
        FileType::Plaintext => extract_links_from_plaintext(&input_content.content),
    };

    // Only keep legit URLs. For example this filters out anchors.
    let mut requests: HashSet<Request> = HashSet::new();
    for link in links {
        let req = if let Ok(uri) = Uri::try_from(link.as_str()) {
            Request::new(uri, input_content.input.clone())
        } else if let Some(new_url) = base.as_ref().and_then(|u| u.join(&link)) {
            Request::new(Uri { inner: new_url }, input_content.input.clone())
        } else if let Input::FsPath(root) = &input_content.input {
            let link = fs::sanitize(&link);
            if link.starts_with('#') {
                // Silently ignore anchors for now.
                continue;
            }
            let path = fs::resolve(&root, &PathBuf::from(&link), base)?;
            let uri = Url::from_file_path(&path).map_err(|_e| ErrorKind::InvalidPath(path))?;
            Request::new(Uri { inner: uri }, input_content.input.clone())
        } else {
            info!("Handling of {} not implemented yet", &link);
            continue;
        };
        requests.insert(req);
    }
    Ok(requests)
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
    use url::Url;

    use super::*;
    use crate::{
        test_utils::{mail, website},
        Uri,
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

    fn extract_uris(input: &str, file_type: FileType, base_url: Option<&str>) -> HashSet<Uri> {
        let base = base_url.map(|url| Base::Remote(Url::parse(url).unwrap()));
        extract_links(&InputContent::from_string(input, file_type), &base)
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
        let input = "http://www.apache.org/licenses/LICENSE-2.0\n";
        let link = input.trim_end();

        assert_eq!(vec![link], extract_links_from_markdown(&input));
        assert_eq!(vec![link], extract_links_from_plaintext(&input));
        assert_eq!(vec![link], extract_links_from_html(&input));
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

        assert_eq!(links, expected_links)
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
    fn test_skip_markdown_anchors() {
        let links = extract_uris("This is [a test](#lol).", FileType::Markdown, None);

        assert!(links.is_empty())
    }

    #[test]
    fn test_skip_markdown_internal_urls() {
        let links = extract_uris("This is [a test](./internal).", FileType::Markdown, None);

        assert!(links.is_empty())
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

        assert_eq!(links, expected)
    }

    #[test]
    fn test_skip_markdown_email() {
        let input = "Get in touch - [Contact Us](mailto:test@test.com)";
        let links = extract_uris(input, FileType::Markdown, None);
        let expected = array::IntoIter::new([mail("test@test.com")]).collect::<HashSet<Uri>>();

        assert_eq!(links, expected)
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

        assert_eq!(links, expected)
    }

    #[test]
    fn test_md_escape() {
        let input = r#"http://msdn.microsoft.com/library/ie/ms535874\(v=vs.85\).aspx"#;
        let links = find_links(input);
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
