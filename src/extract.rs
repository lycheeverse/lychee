use crate::types::Uri;
use linkify::LinkFinder;
use pulldown_cmark::{Event as MDEvent, Parser, Tag};
use quick_xml::{events::Event as HTMLEvent, Reader};
use std::net::IpAddr;
use std::path::Path;
use std::{collections::HashSet, fmt::Display};
use url::Url;

#[derive(Clone, Debug)]
pub(crate) enum FileType {
    HTML,
    Markdown,
    Plaintext,
}

impl Uri {
    pub fn as_str(&self) -> &str {
        match self {
            Uri::Website(url) => url.as_str(),
            Uri::Mail(address) => address.as_str(),
        }
    }

    pub fn scheme(&self) -> Option<String> {
        match self {
            Uri::Website(url) => Some(url.scheme().to_string()),
            Uri::Mail(_address) => None,
        }
    }

    pub fn host_ip(&self) -> Option<IpAddr> {
        match self {
            Self::Website(url) => match url.host()? {
                url::Host::Ipv4(v4_addr) => Some(v4_addr.into()),
                url::Host::Ipv6(v6_addr) => Some(v6_addr.into()),
                _ => None,
            },
            Self::Mail(_) => None,
        }
    }
}

impl Display for Uri {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

// Use LinkFinder here to offload the actual link searching
fn find_links(input: &str) -> Vec<linkify::Link> {
    let finder = LinkFinder::new();
    finder.links(input).collect()
}

// Extracting unparsed URL strings from a markdown string
fn extract_links_from_markdown(input: &str) -> Vec<String> {
    let parser = Parser::new(input);
    parser
        .flat_map(|event| match event {
            MDEvent::Start(tag) => match tag {
                Tag::Link(_, url, _) | Tag::Image(_, url, _) => vec![url.to_string()],
                _ => vec![],
            },
            MDEvent::Text(txt) => extract_links_from_plaintext(&txt.to_string()),
            MDEvent::Html(html) => extract_links_from_html(&html.to_string()),
            _ => vec![],
        })
        .collect()
}

// Extracting unparsed URL strings from a HTML string
fn extract_links_from_html(input: &str) -> Vec<String> {
    let mut reader = Reader::from_str(input);
    let mut buf = Vec::new();
    let mut urls = Vec::new();
    while let Ok(e) = reader.read_event(&mut buf) {
        match e {
            HTMLEvent::Start(ref e) => {
                for attr in e.attributes() {
                    if let Ok(attr) = attr {
                        match (attr.key, e.name()) {
                            (b"href", b"a")
                            | (b"href", b"area")
                            | (b"href", b"base")
                            | (b"href", b"link")
                            | (b"src", b"audio")
                            | (b"src", b"embed")
                            | (b"src", b"iframe")
                            | (b"src", b"img")
                            | (b"src", b"input")
                            | (b"src", b"script")
                            | (b"src", b"source")
                            | (b"src", b"track")
                            | (b"src", b"video")
                            | (b"srcset", b"img")
                            | (b"srcset", b"source")
                            | (b"cite", b"blockquote")
                            | (b"cite", b"del")
                            | (b"cite", b"ins")
                            | (b"cite", b"q")
                            | (b"data", b"object")
                            | (b"onhashchange", b"body") => {
                                urls.push(String::from_utf8_lossy(attr.value.as_ref()).to_string());
                            }
                            _ => {
                                for link in extract_links_from_plaintext(
                                    &String::from_utf8_lossy(attr.value.as_ref()).to_string(),
                                ) {
                                    urls.push(link);
                                }
                            }
                        }
                    }
                }
            }
            HTMLEvent::Text(txt) | HTMLEvent::Comment(txt) => {
                for link in extract_links_from_plaintext(
                    &String::from_utf8_lossy(txt.escaped()).to_string(),
                ) {
                    urls.push(link);
                }
            }
            HTMLEvent::Eof => {
                break;
            }
            _ => {}
        }
        buf.clear();
    }
    urls
}

// Extracting unparsed URL strings from a plaintext
fn extract_links_from_plaintext(input: &str) -> Vec<String> {
    find_links(input)
        .iter()
        .map(|l| String::from(l.as_str()))
        .collect()
}

pub(crate) fn extract_links(
    file_type: FileType,
    input: &str,
    base_url: Option<Url>,
) -> HashSet<Uri> {
    let links = match file_type {
        FileType::Markdown => extract_links_from_markdown(input),
        FileType::HTML => extract_links_from_html(input),
        FileType::Plaintext => extract_links_from_plaintext(input),
    };

    // Only keep legit URLs. This sorts out things like anchors.
    // Silently ignore the parse failures for now.
    let mut uris = HashSet::new();
    for link in links {
        match Url::parse(&link) {
            Ok(url) => {
                uris.insert(Uri::Website(url));
            }
            Err(_) => {
                if link.contains('@') {
                    uris.insert(Uri::Mail(link));
                } else if !Path::new(&link).exists() {
                    if let Some(base_url) = &base_url {
                        if let Ok(new_url) = base_url.clone().join(&link) {
                            uris.insert(Uri::Website(new_url));
                        }
                    }
                }
            }
        };
    }

    debug!("Found: {:#?}", uris);
    uris
}

#[cfg(test)]
mod test {
    use super::*;
    use std::iter::FromIterator;
    use std::net::{Ipv4Addr, Ipv6Addr};

    #[test]
    fn test_extract_markdown_links() {
        let input = "This is [a test](https://endler.dev). This is a relative link test [Relative Link Test](relative_link)";
        let links = extract_links(
            FileType::Markdown,
            input,
            Some(Url::parse("https://github.com/hello-rust/lychee/").unwrap()),
        );
        assert_eq!(
            links,
            HashSet::from_iter(
                [
                    Uri::Website(Url::parse("https://endler.dev").unwrap()),
                    Uri::Website(
                        Url::parse("https://github.com/hello-rust/lychee/relative_link").unwrap()
                    )
                ]
                .iter()
                .cloned()
            )
        )
    }

    #[test]
    fn test_extract_html_links() {
        let input = r#"<html>
                <div class="row">
                    <a href="https://github.com/hello-rust/lychee/">
                    <a href="blob/master/README.md">README</a>
                </div>
            </html>"#;

        let links = extract_links(
            FileType::HTML,
            input,
            Some(Url::parse("https://github.com/hello-rust/").unwrap()),
        );

        assert_eq!(
            links
                .get(&Uri::Website(
                    Url::parse("https://github.com/hello-rust/blob/master/README.md").unwrap()
                ))
                .is_some(),
            true
        );
    }

    #[test]
    fn test_skip_markdown_anchors() {
        let input = "This is [a test](#lol).";
        let links = extract_links(FileType::Markdown, input, None);
        assert_eq!(links, HashSet::new())
    }

    #[test]
    fn test_skip_markdown_internal_urls() {
        let input = "This is [a test](./internal).";
        let links = extract_links(FileType::Markdown, input, None);
        assert_eq!(links, HashSet::new())
    }

    #[test]
    fn test_non_markdown_links() {
        let input =
            "https://endler.dev and https://hello-rust.show/foo/bar?lol=1 at test@example.com";
        let links = extract_links(FileType::Plaintext, input, None);
        let expected = HashSet::from_iter(
            [
                Uri::Website(Url::parse("https://endler.dev").unwrap()),
                Uri::Website(Url::parse("https://hello-rust.show/foo/bar?lol=1").unwrap()),
                Uri::Mail("test@example.com".to_string()),
            ]
            .iter()
            .cloned(),
        );
        assert_eq!(links, expected)
    }

    #[test]
    #[ignore]
    // TODO: Does this escaping need to work properly?
    // See https://github.com/tcort/markdown-link-check/issues/37
    fn test_md_escape() {
        let input = r#"http://msdn.microsoft.com/library/ie/ms535874\(v=vs.85\).aspx"#;
        let links = find_links(input);
        let expected = "http://msdn.microsoft.com/library/ie/ms535874(v=vs.85).aspx)";
        assert!(links.len() == 1);
        assert_eq!(links[0].as_str(), expected);
    }

    #[test]
    fn test_uri_host_ip_v4() {
        let uri =
            Uri::Website(Url::parse("http://127.0.0.1").expect("Expected URI with valid IPv4"));
        let ip = uri.host_ip().expect("Expected a valid IPv4");
        assert_eq!(ip, IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)));
    }

    #[test]
    fn test_uri_host_ip_v6() {
        let uri =
            Uri::Website(Url::parse("https://[2020::0010]").expect("Expected URI with valid IPv6"));
        let ip = uri.host_ip().expect("Expected a valid IPv6");
        assert_eq!(
            ip,
            IpAddr::V6(Ipv6Addr::new(0x2020, 0, 0, 0, 0, 0, 0, 0x10))
        );
    }

    #[test]
    fn test_uri_host_ip_no_ip() {
        let uri = Uri::Website(Url::parse("https://some.cryptic/url").expect("Expected valid URI"));
        let ip = uri.host_ip();
        assert!(ip.is_none());
    }
}
