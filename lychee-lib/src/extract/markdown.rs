//! Extract links and fragments from markdown documents
use std::collections::{HashMap, HashSet};

use log::warn;
use pulldown_cmark::{CowStr, Event, LinkType, Options, Parser, Tag, TagEnd, TextMergeWithOffset};

use crate::{
    checker::wikilink::wikilink,
    extract::{html::html5gum::extract_html_with_span, plaintext::extract_raw_uri_from_plaintext},
    types::uri::raw::{
        OffsetSpanProvider, RawUri, RawUriSpan, SourceSpanProvider, SpanProvider as _,
    },
};

use super::html::html5gum::extract_html_fragments;

/// Returns the default markdown extensions used by lychee.
/// Sadly, `|` is not const for `Options` so we can't use a const global.
fn md_extensions() -> Options {
    Options::ENABLE_HEADING_ATTRIBUTES
        | Options::ENABLE_MATH
        | Options::ENABLE_WIKILINKS
        | Options::ENABLE_FOOTNOTES
}

/// Extract unparsed URL strings from a Markdown string.
// TODO: Refactor the extractor to reduce the complexity and number of lines.
#[allow(clippy::too_many_lines)]
pub(crate) fn extract_markdown(
    input: &str,
    include_verbatim: bool,
    include_wikilinks: bool,
) -> Vec<RawUri> {
    // In some cases it is undesirable to extract links from within code blocks,
    // which is why we keep track of entries and exits while traversing the input.
    let mut inside_code_block = false;
    let mut inside_link_block = false;
    let mut inside_wikilink_block = false;

    // HTML blocks come in chunks from pulldown_cmark, so we need to accumulate them
    let mut inside_html_block = false;
    let mut html_block_buffer = String::new();
    let mut html_block_start_offset = 0;

    let span_provider = SourceSpanProvider::from_input(input);
    let parser =
        TextMergeWithOffset::new(Parser::new_ext(input, md_extensions()).into_offset_iter());
    parser
        .filter_map(|(event, span)| match event {
            // A link.
            Event::Start(Tag::Link {
                link_type,
                dest_url,
                ..
            }) => {
                // Note: Explicitly listing all link types below to make it easier to
                // change the behavior for a specific link type in the future.
                #[allow(clippy::match_same_arms)]
                match link_type {
                    // Inline link like `[foo](bar)`
                    // This is the most common link type
                    LinkType::Inline => {
                        inside_link_block = true;
                        Some(raw_uri(&dest_url, span_provider.span(span.start)))
                    }
                    // Reference without destination in the document, but resolved by the `broken_link_callback`
                    LinkType::Reference |
                    // Collapsed link like `[foo][]`
                    LinkType::ReferenceUnknown |
                    // Collapsed link like `[foo][]`
                    LinkType::Collapsed|
                    // Collapsed link without destination in the document, but resolved by the `broken_link_callback`
                    LinkType::CollapsedUnknown |
                    // Shortcut link like `[foo]`
                    LinkType::Shortcut |
                    // Shortcut without destination in the document, but resolved by the `broken_link_callback`
                    LinkType::ShortcutUnknown => {
                        inside_link_block = true;
                        // For reference links, create RawUri directly to handle relative file paths
                        // that linkify doesn't recognize as URLs
                        Some(raw_uri(&dest_url, span_provider.span(span.start)))
                    },
                    // Autolink like `<http://foo.bar/baz>`
                    LinkType::Autolink |
                    // Email address in autolink like `<john@example.org>`
                    LinkType::Email => {
                        let span_provider = get_email_span_provider(&span_provider, &span, link_type);
                        Some(extract_raw_uri_from_plaintext(&dest_url, &span_provider))
                    }
                    // Wiki URL (`[[http://example.com]]`)
                    LinkType::WikiLink { has_pothole } => {
                        // Exclude WikiLinks if not explicitly enabled
                        if !include_wikilinks {
                            return None;
                        }
                        inside_wikilink_block = true;
                        // Ignore gitlab toc notation: https://docs.gitlab.com/user/markdown/#table-of-contents
                        if ["_TOC_".to_string(), "TOC".to_string()].contains(&dest_url.to_string()) {
                            return None;
                        }

                        if let Ok(wikilink) = wikilink(&dest_url, has_pothole) {
                            Some(vec![RawUri {
                                text: wikilink.to_string(),
                                element: Some("a".to_string()),
                                attribute: Some("wikilink".to_string()),
                                // wiki links start with `[[`, so offset the span by `2`
                                span: span_provider.span(span.start + 2)
                            }])
                        } else {
                            warn!("The wikilink destination url {dest_url} could not be cleaned by removing potholes and fragments");
                            None
                        }
                    }
                }
            }

            Event::Start(Tag::Image { dest_url, .. }) => Some(extract_image(&dest_url, span_provider.span(span.start))),

            // A code block (inline or fenced).
            Event::Start(Tag::CodeBlock(_)) => {
                inside_code_block = true;
                None
            }
            Event::End(TagEnd::CodeBlock) => {
                inside_code_block = false;
                None
            }

            // A text node.
            Event::Text(txt) => {
                if inside_wikilink_block
                    || (inside_link_block && !include_verbatim)
                    || (inside_code_block && !include_verbatim) {
                    None
                } else {
                    Some(extract_raw_uri_from_plaintext(
                        &txt,
                        &OffsetSpanProvider { offset: span.start, inner: &span_provider }
                    ))
                }
            }

            // Start of an HTML block
            Event::Start(Tag::HtmlBlock) => {
                inside_html_block = true;
                html_block_buffer.clear();
                html_block_start_offset = span.start;
                None
            }

            // End of an HTML block - process accumulated HTML
            Event::End(TagEnd::HtmlBlock) => {
                inside_html_block = false;
                if html_block_buffer.is_empty() {
                    None
                } else {
                    Some(extract_html_with_span(
                        &html_block_buffer,
                        include_verbatim,
                        OffsetSpanProvider {
                            offset: html_block_start_offset,
                            inner: &span_provider
                        }
                    ))
                }
            }

            // An HTML node
            Event::Html(html) => {
                if inside_html_block {
                    // Accumulate HTML chunks within a block
                    html_block_buffer.push_str(&html);
                    None
                } else {
                    // Standalone HTML (not part of a block) - process immediately
                    Some(extract_html_with_span(
                        &html,
                        include_verbatim,
                        OffsetSpanProvider { offset: span.start, inner: &span_provider }
                    ))
                }
            }

            // Inline HTML (not part of a block)
            Event::InlineHtml(html) => {
                Some(extract_html_with_span(
                    &html,
                    include_verbatim,
                    OffsetSpanProvider { offset: span.start, inner: &span_provider }
                ))
            }

            // An inline code node.
            Event::Code(code) => {
                if include_verbatim {
                    // inline code starts with '`', so offset the span by `1`.
                    Some(extract_raw_uri_from_plaintext(
                        &code,
                        &OffsetSpanProvider { offset: span.start + 1, inner: &span_provider }
                    ))
                } else {
                    None
                }
            }

            Event::End(TagEnd::Link) => {
                inside_link_block = false;
                inside_wikilink_block = false;
                None
            }

            // Skip footnote references and definitions explicitly - they're not links to check
            #[allow(clippy::match_same_arms)]
            Event::FootnoteReference(_) | Event::Start(Tag::FootnoteDefinition(_)) | Event::End(TagEnd::FootnoteDefinition) => None,

            // Silently skip over other events
            _ => None,
        })
        .flatten()
        .collect()
}

fn get_email_span_provider<'a>(
    span_provider: &'a SourceSpanProvider<'_>,
    span: &std::ops::Range<usize>,
    link_type: LinkType,
) -> OffsetSpanProvider<'a> {
    let offset = match link_type {
        // We don't know how the link starts, so don't offset the span.
        LinkType::Reference | LinkType::CollapsedUnknown | LinkType::ShortcutUnknown => 0,
        // These start all with `[` or `<`, so offset the span by `1`.
        LinkType::ReferenceUnknown
        | LinkType::Collapsed
        | LinkType::Shortcut
        | LinkType::Autolink
        | LinkType::Email => 1,
        _ => {
            debug_assert!(false, "Unexpected email link type: {link_type:?}");
            0
        }
    };

    OffsetSpanProvider {
        offset: span.start + offset,
        inner: span_provider,
    }
}

/// Emulate `<img src="...">` tag to be compatible with HTML links.
/// We might consider using the actual Markdown `LinkType` for better granularity in the future.
fn extract_image(dest_url: &CowStr<'_>, span: RawUriSpan) -> Vec<RawUri> {
    vec![RawUri {
        text: dest_url.to_string(),
        element: Some("img".to_string()),
        attribute: Some("src".to_string()),
        span,
    }]
}

/// Emulate `<a href="...">` tag to be compatible with HTML links.
/// We might consider using the actual Markdown `LinkType` for better granularity in the future.
fn raw_uri(dest_url: &CowStr<'_>, span: RawUriSpan) -> Vec<RawUri> {
    vec![RawUri {
        text: dest_url.to_string(),
        element: Some("a".to_string()),
        attribute: Some("href".to_string()),
        // Sadly, we don't know how long the `foo` part in `[foo](bar)` is,
        // so the span points to the `[` and not to the `b`.
        span,
    }]
}

/// Extract fragments/anchors from a Markdown string.
///
/// Fragments are generated from headings using the same unique kebab case method as GitHub.
/// If a [heading attribute](https://github.com/raphlinus/pulldown-cmark/blob/master/specs/heading_attrs.txt)
/// is present,
/// this will be added to the fragment set **alongside** the other generated fragment.
/// It means a single heading such as `## Frag 1 {#frag-2}` would generate two fragments.
pub(crate) fn extract_markdown_fragments(input: &str) -> HashSet<String> {
    let mut in_heading = false;
    let mut heading_text = String::new();
    let mut heading_id: Option<CowStr<'_>> = None;
    let mut id_generator = HeadingIdGenerator::default();

    let mut out = HashSet::new();

    for event in Parser::new_ext(input, md_extensions()) {
        match event {
            Event::Start(Tag::Heading { id, .. }) => {
                heading_id = id;
                in_heading = true;
            }
            Event::End(TagEnd::Heading(_)) => {
                if let Some(frag) = heading_id.take() {
                    out.insert(frag.to_string());
                }

                if !heading_text.is_empty() {
                    let id = id_generator.generate(&heading_text);
                    out.insert(id);
                    heading_text.clear();
                }

                in_heading = false;
            }
            Event::Text(text) | Event::Code(text) => {
                if in_heading {
                    heading_text.push_str(&text);
                }
            }

            // An HTML node
            Event::Html(html) | Event::InlineHtml(html) => {
                out.extend(extract_html_fragments(&html));
            }

            // Silently skip over other events
            _ => (),
        }
    }
    out
}

#[derive(Default)]
struct HeadingIdGenerator {
    counter: HashMap<String, usize>,
}

impl HeadingIdGenerator {
    fn generate(&mut self, heading: &str) -> String {
        let mut id = Self::into_kebab_case(heading);
        let count = self.counter.entry(id.clone()).or_insert(0);
        if *count != 0 {
            id = format!("{}-{}", id, *count);
        }
        *count += 1;

        id
    }

    /// Converts text into kebab case
    #[must_use]
    fn into_kebab_case(text: &str) -> String {
        text.to_lowercase()
            .chars()
            .filter_map(|ch| {
                if ch.is_alphanumeric() || ch == '_' || ch == '-' {
                    Some(ch)
                } else if ch.is_whitespace() {
                    Some('-')
                } else {
                    None
                }
            })
            .collect::<String>()
    }
}

#[cfg(test)]
mod tests {
    use crate::types::uri::raw::span;

    use super::*;

    const MD_INPUT: &str = r#"
# A Test

Some link in text [here](https://foo.com)

## A test {#well-still-the-same-test}

Code:

```bash
https://bar.com/123
```

or inline like `https://bar.org` for instance.

### Some `code` in a heading.

[example](http://example.com)

<span id="the-end">The End</span>
        "#;

    #[test]
    fn test_extract_fragments() {
        let expected = HashSet::from([
            "a-test".to_string(),
            "a-test-1".to_string(),
            "well-still-the-same-test".to_string(),
            "some-code-in-a-heading".to_string(),
            "the-end".to_string(),
        ]);
        let actual = extract_markdown_fragments(MD_INPUT);
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_skip_verbatim() {
        let expected = vec![
            RawUri {
                text: "https://foo.com".to_string(),
                element: Some("a".to_string()),
                attribute: Some("href".to_string()),
                span: span(4, 19),
            },
            RawUri {
                text: "http://example.com".to_string(),
                element: Some("a".to_string()),
                attribute: Some("href".to_string()),
                span: span(18, 1),
            },
        ];

        let uris = extract_markdown(MD_INPUT, false, false);
        assert_eq!(uris, expected);
    }

    #[test]
    fn test_include_verbatim() {
        let expected = vec![
            RawUri {
                text: "https://foo.com".to_string(),
                element: Some("a".to_string()),
                attribute: Some("href".to_string()),
                span: span(4, 19),
            },
            RawUri {
                text: "https://bar.com/123".to_string(),
                element: None,
                attribute: None,
                span: span(11, 1),
            },
            RawUri {
                text: "https://bar.org".to_string(),
                element: None,
                attribute: None,
                span: span(14, 17),
            },
            RawUri {
                text: "http://example.com".to_string(),
                element: Some("a".to_string()),
                attribute: Some("href".to_string()),
                span: span(18, 1),
            },
        ];

        let uris = extract_markdown(MD_INPUT, true, false);
        assert_eq!(uris, expected);
    }

    #[test]
    fn test_skip_verbatim_html() {
        let input = "
<code>
http://link.com
</code>
<pre>
Some pre-formatted http://pre.com
</pre>";

        let expected = vec![];

        let uris = extract_markdown(input, false, false);
        assert_eq!(uris, expected);
    }

    #[test]
    fn test_kebab_case() {
        let check = |input, expected| {
            let actual = HeadingIdGenerator::into_kebab_case(input);
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
    fn test_markdown_math() {
        let input = r"
$$
[\psi](\mathbf{L})
$$
";
        let uris = extract_markdown(input, true, false);
        assert!(uris.is_empty());
    }

    #[test]
    fn test_single_word_footnote_is_not_detected_as_link() {
        let markdown = "This footnote is[^actually] a link.\n\n[^actually]: not";
        let expected = vec![];
        let uris = extract_markdown(markdown, true, false);
        assert_eq!(uris, expected);
    }

    #[test]
    fn test_underscore_in_urls_middle() {
        let markdown = r"https://example.com/_/foo";
        let expected = vec![RawUri {
            text: "https://example.com/_/foo".to_string(),
            element: None,
            attribute: None,
            span: span(1, 1),
        }];
        let uris = extract_markdown(markdown, true, false);
        assert_eq!(uris, expected);
    }

    #[test]
    fn test_underscore_in_urls_end() {
        let markdown = r"https://example.com/_";
        let expected = vec![RawUri {
            text: "https://example.com/_".to_string(),
            element: None,
            attribute: None,
            span: span(1, 1),
        }];
        let uris = extract_markdown(markdown, true, false);
        assert_eq!(uris, expected);
    }

    #[test]
    fn test_wiki_link() {
        let markdown = r"[[https://example.com/destination]]";
        let expected = vec![RawUri {
            text: "https://example.com/destination".to_string(),
            element: Some("a".to_string()),
            attribute: Some("wikilink".to_string()),
            span: span(1, 3),
        }];
        let uris = extract_markdown(markdown, true, true);
        assert_eq!(uris, expected);
    }

    #[test]
    fn test_multiple_wiki_links() {
        let markdown = r"[[https://example.com/destination]][[https://example.com/source]]";
        let expected = vec![
            RawUri {
                text: "https://example.com/destination".to_string(),
                element: Some("a".to_string()),
                attribute: Some("wikilink".to_string()),
                span: span(1, 3),
            },
            RawUri {
                text: "https://example.com/source".to_string(),
                element: Some("a".to_string()),
                attribute: Some("wikilink".to_string()),
                span: span(1, 38),
            },
        ];
        let uris = extract_markdown(markdown, true, true);
        assert_eq!(uris, expected);
    }

    #[test]
    fn test_ignore_gitlab_toc() {
        let markdown = r"[[_TOC_]][TOC]";
        let uris = extract_markdown(markdown, true, true);
        assert!(uris.is_empty());
    }

    #[test]
    fn test_link_text_not_checked() {
        // Test that link text is not extracted as a separate link by default
        let markdown =
            r"[https://lycheerepublic.gov/notexist (archive.org link)](https://example.com)";
        let uris = extract_markdown(markdown, false, false);

        // Should only extract the destination URL, not the link text
        let expected = vec![RawUri {
            text: "https://example.com".to_string(),
            element: Some("a".to_string()),
            attribute: Some("href".to_string()),
            span: span(1, 1),
        }];

        assert_eq!(uris, expected);
        assert_eq!(
            uris.len(),
            1,
            "Should only find destination URL, not link text"
        );
    }

    #[test]
    fn test_link_text_checked_with_include_verbatim() {
        // Test that link text IS extracted when include_verbatim is true
        let markdown =
            r"[https://lycheerepublic.gov/notexist (archive.org link)](https://example.com)";
        let uris = extract_markdown(markdown, true, false);

        // Should extract both the link text AND the destination URL
        let expected = vec![
            RawUri {
                text: "https://example.com".to_string(),
                element: Some("a".to_string()),
                attribute: Some("href".to_string()),
                span: span(1, 1),
            },
            RawUri {
                text: "https://lycheerepublic.gov/notexist".to_string(),
                element: None,
                attribute: None,
                span: span(1, 2),
            },
        ];

        assert_eq!(
            uris.len(),
            2,
            "Should find both destination URL and link text"
        );
        // Check that both expected URLs are present (order might vary)
        for expected_uri in expected {
            assert!(
                uris.contains(&expected_uri),
                "Missing expected URI: {expected_uri:?}"
            );
        }
    }

    #[test]
    fn test_reference_links_extraction() {
        // Test that all types of reference links are extracted correctly
        let markdown = r"
Inline link: [link1](target1.md)

Reference link: [link2][ref2]
Collapsed link: [link3][]
Shortcut link: [link4]

[ref2]: target2.md
[link3]: target3.md
[link4]: target4.md
";
        let uris = extract_markdown(markdown, false, false);

        let expected = vec![
            RawUri {
                text: "target1.md".to_string(),
                element: Some("a".to_string()),
                attribute: Some("href".to_string()),
                span: span(2, 14),
            },
            RawUri {
                text: "target2.md".to_string(),
                element: Some("a".to_string()),
                attribute: Some("href".to_string()),
                span: span(4, 17),
            },
            RawUri {
                text: "target3.md".to_string(),
                element: Some("a".to_string()),
                attribute: Some("href".to_string()),
                span: span(5, 17),
            },
            RawUri {
                text: "target4.md".to_string(),
                element: Some("a".to_string()),
                span: span(6, 16),
                attribute: Some("href".to_string()),
            },
        ];

        assert_eq!(uris.len(), 4, "Should extract all four link types");

        // Check that all expected URIs are present (order might vary)
        for expected_uri in expected {
            assert!(
                uris.contains(&expected_uri),
                "Missing expected URI: {expected_uri:?}. Found: {uris:?}"
            );
        }
    }

    #[test]
    fn test_clean_wikilink() {
        let markdown = r"
[[foo|bar]]
[[foo#bar]]
[[foo#bar|baz]]
";
        let uris = extract_markdown(markdown, true, true);
        let expected = vec![
            RawUri {
                text: "foo".to_string(),
                element: Some("a".to_string()),
                attribute: Some("wikilink".to_string()),
                span: span(2, 3),
            },
            RawUri {
                text: "foo".to_string(),
                element: Some("a".to_string()),
                attribute: Some("wikilink".to_string()),
                span: span(3, 3),
            },
            RawUri {
                text: "foo".to_string(),
                element: Some("a".to_string()),
                attribute: Some("wikilink".to_string()),
                span: span(4, 3),
            },
        ];
        assert_eq!(uris, expected);
    }

    #[test]
    fn test_nested_html() {
        let input = r#"<Foo>
          <Bar href="https://example.com" >
          Some text
          </Bar>
        </Foo>"#;

        let expected = vec![RawUri {
            text: "https://example.com".to_string(),
            element: Some("bar".to_string()),
            attribute: Some("href".to_string()),
            span: span(2, 22),
        }];

        let uris = extract_markdown(input, false, false);

        assert_eq!(uris, expected);
    }

    #[test]
    fn test_wikilink_extraction_returns_none_on_empty_links() {
        let markdown = r"
[[|bar]]
[[#bar]]
[[#bar|baz]]
";

        let uris = extract_markdown(markdown, true, true);
        assert!(uris.is_empty());
    }

    #[test]
    fn test_mdx_multiline_jsx() {
        let input = r#"<CardGroup cols={1}>
  <Card
    title="Example"
    href="https://example.com"
  >
    Some text
  </Card>
</CardGroup>"#;

        let expected = vec![RawUri {
            text: "https://example.com".to_string(),
            element: Some("card".to_string()),
            attribute: Some("href".to_string()),
            span: span(4, 11),
        }];

        let uris = extract_markdown(input, false, false);

        assert_eq!(uris, expected);
    }

    // Test that Markdown links inside HTML blocks are still parsed correctly.
    // pulldown_cmark parses block-level HTML tags as separate HTML blocks, so
    // Markdown content between them is processed normally.
    #[test]
    fn test_markdown_inside_html_block() {
        let input = r"<div>

[markdown link](https://example.com/markdown)

</div>

<span>[another link](https://example.com/another)</span>";

        let uris = extract_markdown(input, false, false);

        // Verify both Markdown links are extracted
        let expected_urls = vec![
            "https://example.com/markdown",
            "https://example.com/another",
        ];

        assert_eq!(uris.len(), 2, "Should extract both Markdown links");

        for expected_url in expected_urls {
            assert!(
                uris.iter().any(|u| u.text == expected_url),
                "Should find URL: {expected_url}"
            );
        }

        // Verify they're recognized as Markdown links (i.e. element: "a", attribute: "href")
        for uri in &uris {
            assert_eq!(uri.element, Some("a".to_string()));
            assert_eq!(uri.attribute, Some("href".to_string()));
        }
    }

    #[test]
    fn test_remove_wikilink_potholes_and_fragments() {
        let markdown = r"[[foo#bar|baz]]";
        let uris = extract_markdown(markdown, true, true);
        let expected = vec![RawUri {
            text: "foo".to_string(),
            element: Some("a".to_string()),
            attribute: Some("wikilink".to_string()),
            span: span(1, 3),
        }];
        assert_eq!(uris, expected);
    }
}
