//! Extract links and fragments from markdown documents
use std::collections::{HashMap, HashSet};

use pulldown_cmark::{Event, Options, Parser, Tag};

use crate::{extract::plaintext::extract_plaintext, types::uri::raw::RawUri};

use super::html::html5gum::{extract_html, extract_html_fragments};

/// Extract unparsed URL strings from a Markdown string.
pub(crate) fn extract_markdown(input: &str, include_verbatim: bool) -> Vec<RawUri> {
    // In some cases it is undesirable to extract links from within code blocks,
    // which is why we keep track of entries and exits while traversing the input.
    let mut inside_code_block = false;

    let parser = Parser::new(input);
    parser
        .filter_map(|event| match event {
            // A link. The first field is the link type, the second the destination URL and the third is a title.
            Event::Start(Tag::Link(_, uri, _)) => {
                Some(vec![RawUri {
                    text: uri.to_string(),
                    // Emulate `<a href="...">` tag here to be compatible with
                    // HTML links. We might consider using the actual Markdown
                    // `LinkType` for better granularity in the future
                    element: Some("a".to_string()),
                    attribute: Some("href".to_string()),
                }])
            }
            // An image. The first field is the link type, the second the destination URL and the third is a title.
            Event::Start(Tag::Image(_, uri, _)) => {
                Some(vec![RawUri {
                    text: uri.to_string(),
                    // Emulate `<img src="...">` tag here to be compatible with
                    // HTML links. We might consider using the actual Markdown
                    // `LinkType` for better granularity in the future
                    element: Some("img".to_string()),
                    attribute: Some("src".to_string()),
                }])
            }
            // A code block (inline or fenced).
            Event::Start(Tag::CodeBlock(_)) => {
                inside_code_block = true;
                None
            }
            Event::End(Tag::CodeBlock(_)) => {
                inside_code_block = false;
                None
            }

            // A text node.
            Event::Text(txt) => {
                if inside_code_block && !include_verbatim {
                    None
                } else {
                    Some(extract_plaintext(&txt))
                }
            }

            // An HTML node
            Event::Html(html) => {
                // This won't exclude verbatim links right now, because HTML gets passed in chunks
                // by pulldown_cmark. So excluding `<pre>` and `<code>` is not handled right now.
                Some(extract_html(&html, include_verbatim))
            }

            // An inline code node.
            Event::Code(code) => {
                if include_verbatim {
                    Some(extract_plaintext(&code))
                } else {
                    None
                }
            }

            // Silently skip over other events
            _ => None,
        })
        .flatten()
        .collect()
}

/// Extract fragments/anchors/fragments from a Markdown string.
///
/// Fragments are generated from headings using the same unique kebab case method as GitHub.
/// If a [heading attribute](https://github.com/raphlinus/pulldown-cmark/blob/master/specs/heading_attrs.txt)
/// is present,
/// this will be added to the fragment set **alongside** the other generated fragment.
/// It means a single heading such as `## Frag 1 {#frag-2}` would generate two fragments.
pub(crate) fn extract_markdown_fragments(input: &str) -> HashSet<String> {
    let mut in_heading = false;
    let mut heading = String::new();
    let mut id_generator = HeadingIdGenerator::default();

    let mut out = HashSet::new();

    for event in Parser::new_ext(input, Options::ENABLE_HEADING_ATTRIBUTES) {
        match event {
            Event::Start(Tag::Heading(..)) => {
                in_heading = true;
            }
            Event::End(Tag::Heading(_level, id, _classes)) => {
                if let Some(frag) = id {
                    out.insert(frag.to_string());
                }

                if !heading.is_empty() {
                    let id = id_generator.generate(&heading);
                    out.insert(id);
                    heading.clear();
                }

                in_heading = false;
            }
            Event::Text(text) => {
                if in_heading {
                    heading.push_str(&text);
                };
            }

            // An HTML node
            Event::Html(html) => {
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
        text.chars()
            .filter_map(|ch| {
                if ch.is_alphanumeric() || ch == '_' || ch == '-' {
                    Some(ch.to_ascii_lowercase())
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

[example](http://example.com)

<span id="the-end">The End</span>
        "#;

    #[test]
    fn test_extract_fragments() {
        let expected = HashSet::from([
            "a-test".to_string(),
            "a-test-1".to_string(),
            "well-still-the-same-test".to_string(),
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
            },
            RawUri {
                text: "http://example.com".to_string(),
                element: Some("a".to_string()),
                attribute: Some("href".to_string()),
            },
        ];

        let uris = extract_markdown(MD_INPUT, false);
        assert_eq!(uris, expected);
    }

    #[test]
    fn test_include_verbatim() {
        let expected = vec![
            RawUri {
                text: "https://foo.com".to_string(),
                element: Some("a".to_string()),
                attribute: Some("href".to_string()),
            },
            RawUri {
                text: "https://bar.com/123".to_string(),
                element: None,
                attribute: None,
            },
            RawUri {
                text: "https://bar.org".to_string(),
                element: None,
                attribute: None,
            },
            RawUri {
                text: "http://example.com".to_string(),
                element: Some("a".to_string()),
                attribute: Some("href".to_string()),
            },
        ];

        let uris = extract_markdown(MD_INPUT, true);
        assert_eq!(uris, expected);
    }

    #[test]
    #[ignore]
    fn test_skip_verbatim_html() {
        let input = "
<code>
http://link.com
</code>
<pre>
Some pre-formatted http://pre.com
</pre>";

        let expected = vec![];

        let uris = extract_markdown(input, false);
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
}
