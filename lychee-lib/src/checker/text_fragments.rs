use crate::{ErrorKind, FileType, Status};
use html5gum::{
    Tokenizer,
    emitters::callback::{Callback, CallbackEmitter, CallbackEvent},
};
use url::Url;

#[derive(Debug, PartialEq, Eq)]
struct TextDirective {
    prefix: Option<String>,
    start: String,
    end: Option<String>,
    suffix: Option<String>,
}

pub(crate) fn check_text_fragments(
    url: &Url,
    status: Status,
    content: &str,
    file_type: FileType,
) -> Status {
    if !status.is_success() || file_type != FileType::Html {
        return status;
    }

    let directives = parse_text_directives(url);
    if directives.is_empty() {
        return status;
    }

    let document = normalize_whitespace(&extract_visible_text(content));
    let all_match = directives
        .iter()
        .all(|directive| directive.matches(&document));

    if all_match {
        status
    } else {
        Status::Error(ErrorKind::InvalidFragment(url.clone().into()))
    }
}

impl TextDirective {
    fn parse(input: &str) -> Option<Self> {
        let mut parts: Vec<&str> = input.split(',').collect();
        if parts.is_empty() {
            return None;
        }

        let prefix = if parts.first().is_some_and(|part| part.ends_with('-')) && parts.len() >= 2 {
            let prefix = parts.remove(0);
            Some(prefix[..prefix.len() - 1].to_owned())
        } else {
            None
        };

        let suffix = if parts.last().is_some_and(|part| part.starts_with('-')) && parts.len() >= 2 {
            let suffix = parts.pop().expect("checked length above");
            Some(suffix[1..].to_owned())
        } else {
            None
        };

        let [start] = parts.as_slice() else {
            let [start, end] = parts.as_slice() else {
                return None;
            };

            return Some(Self {
                prefix,
                start: normalize_whitespace(start),
                end: Some(normalize_whitespace(end)),
                suffix: suffix.map(|s| normalize_whitespace(&s)),
            });
        };

        Some(Self {
            prefix: prefix.map(|s| normalize_whitespace(&s)),
            start: normalize_whitespace(start),
            end: None,
            suffix: suffix.map(|s| normalize_whitespace(&s)),
        })
    }

    fn matches(&self, document: &str) -> bool {
        if self.start.is_empty() {
            return false;
        }

        let mut start_offset = 0;
        while let Some(relative_start) = document[start_offset..].find(&self.start) {
            let match_start = start_offset + relative_start;
            let mut match_end = match_start + self.start.len();

            if let Some(end) = &self.end {
                let Some(relative_end) = document[match_end..].find(end) else {
                    return false;
                };
                match_end += relative_end + end.len();
            }

            let prefix_matches = self
                .prefix
                .as_ref()
                .is_none_or(|prefix| document[..match_start].trim_end().ends_with(prefix));
            let suffix_matches = self
                .suffix
                .as_ref()
                .is_none_or(|suffix| document[match_end..].trim_start().starts_with(suffix));

            if prefix_matches && suffix_matches {
                return true;
            }

            start_offset = match_start + 1;
        }

        false
    }
}

fn parse_text_directives(url: &Url) -> Vec<TextDirective> {
    let Some(fragment) = url.fragment() else {
        return Vec::new();
    };
    let Some((_, directive)) = fragment.split_once(":~:") else {
        return Vec::new();
    };

    url::form_urlencoded::parse(directive.as_bytes())
        .filter(|(key, _)| key == "text")
        .filter_map(|(_, value)| TextDirective::parse(value.as_ref()))
        .collect()
}

fn normalize_whitespace(input: &str) -> String {
    input.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn extract_visible_text(input: &str) -> String {
    #[derive(Default)]
    struct TextExtractor {
        text: String,
        hidden_stack: Vec<String>,
    }

    impl TextExtractor {
        fn is_hidden_tag(name: &str) -> bool {
            matches!(name, "head" | "script" | "style" | "template")
        }

        const fn in_hidden_context(&self) -> bool {
            !self.hidden_stack.is_empty()
        }
    }

    impl Callback<(), usize> for &mut TextExtractor {
        fn handle_event(
            &mut self,
            event: CallbackEvent<'_>,
            _span: html5gum::Span<usize>,
        ) -> Option<()> {
            match event {
                CallbackEvent::OpenStartTag { name } => {
                    let tag = String::from_utf8_lossy(name).into_owned();
                    if TextExtractor::is_hidden_tag(&tag) {
                        self.hidden_stack.push(tag);
                    }
                }
                CallbackEvent::EndTag { name } => {
                    let tag = String::from_utf8_lossy(name);
                    if self
                        .hidden_stack
                        .last()
                        .is_some_and(|last| last == tag.as_ref())
                    {
                        self.hidden_stack.pop();
                    }
                }
                CallbackEvent::String { value } => {
                    if !self.in_hidden_context() {
                        self.text.push_str(&String::from_utf8_lossy(value));
                        self.text.push(' ');
                    }
                }
                CallbackEvent::AttributeName { .. }
                | CallbackEvent::AttributeValue { .. }
                | CallbackEvent::CloseStartTag { .. }
                | CallbackEvent::Comment { .. }
                | CallbackEvent::Doctype { .. }
                | CallbackEvent::Error(_) => {}
            }
            None
        }
    }

    let mut extractor = TextExtractor::default();
    let emitter = CallbackEmitter::new(&mut extractor);
    let _: Result<Vec<_>, _> = Tokenizer::new_with_emitter(input, emitter).collect();
    extractor.text
}

#[cfg(test)]
mod tests {
    use http::StatusCode;

    use super::*;

    const INDEX_HTML: &str = include_str!("../../../fixtures/text_fragments/index.html");

    fn ok_status() -> Status {
        Status::Ok(StatusCode::OK)
    }

    #[test]
    fn parses_single_text_directive() {
        let url = Url::parse("https://example.com/#:~:text=rutrum").unwrap();

        let directives = parse_text_directives(&url);

        assert_eq!(
            directives,
            vec![TextDirective {
                prefix: None,
                start: "rutrum".into(),
                end: None,
                suffix: None,
            }]
        );
    }

    #[test]
    fn parses_text_directive_with_prefix_and_suffix() {
        let url = Url::parse(
            "https://example.com/#:~:text=consectetur%20adipiscing%20elit.-,Sed%20porta,-nisl%20sit%20amet",
        )
        .unwrap();

        let directives = parse_text_directives(&url);

        assert_eq!(
            directives,
            vec![TextDirective {
                prefix: Some("consectetur adipiscing elit.".into()),
                start: "Sed porta".into(),
                end: None,
                suffix: Some("nisl sit amet".into()),
            }]
        );
    }

    #[test]
    fn extracts_visible_text_without_style_or_attributes() {
        let text = normalize_whitespace(&extract_visible_text(INDEX_HTML));

        assert!(text.contains("Sed porta nisl sit amet quam ornare rutrum."));
        assert!(!text.contains("my-style-property"));
        assert!(!text.contains("my-element-attribute-value"));
    }

    #[tokio::test]
    async fn matches_fixture_text_fragments() {
        let urls = [
            "https://example.com/#:~:text=rutrum",
            "https://example.com/#:~:text=Pellentesque%20accumsan%20blandit%20ex%20iaculis%20pretium",
            "https://example.com/#:~:text=malesuada.,Duis",
            "https://example.com/#:~:text=consectetur%20adipiscing%20elit.-,Sed%20porta,-nisl%20sit%20amet",
        ];

        for raw_url in urls {
            let url = Url::parse(raw_url).unwrap();
            let status = check_text_fragments(&url, ok_status(), INDEX_HTML, FileType::Html);
            assert!(
                status.is_success(),
                "expected success for {raw_url}, got {status}"
            );
        }
    }

    #[tokio::test]
    async fn rejects_non_matching_fixture_text_fragments() {
        let urls = [
            "https://example.com/#:~:text=non-existent",
            "https://example.com/#:~:text=my-style",
            "https://example.com/#:~:text=my-element",
            "https://example.com/#:~:text=my-script",
        ];

        for raw_url in urls {
            let url = Url::parse(raw_url).unwrap();
            let status = check_text_fragments(&url, ok_status(), INDEX_HTML, FileType::Html);
            assert!(
                status.is_error(),
                "expected error for {raw_url}, got {status}"
            );
        }
    }
}
