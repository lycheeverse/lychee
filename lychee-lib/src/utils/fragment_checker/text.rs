use super::parsed_fragment::TextDirective;
use crate::types::FileType;
use html5gum::{
    Tokenizer,
    emitters::callback::{Callback, CallbackEmitter, CallbackEvent},
};

/// Check if the text fragments in the given URL are valid for the provided content and file type.
pub(super) fn check_text_fragments(
    directives: &[TextDirective],
    content: &str,
    file_type: FileType,
) -> bool {
    if file_type != FileType::Html {
        // We currently don't support text fragments for e.g. Markdown files,
        // because realistically there is no standard for them.
        // If text fragments ever become standardized for non-HTML files, we can implement support for them here.
        return true;
    }

    if directives.is_empty() {
        return true;
    }

    // The algorithm to find a range in a document likely requires a full implementation of a browser.
    // See https://wicg.github.io/scroll-to-text-fragment/#finding-ranges-in-a-document
    // Here, we try to approximate it by extracting visible text.
    // This ensures that `Hell<i>o</i> <strong>world</strong>` is matched.
    let document = extract_visible_text(content);
    directives
        .iter()
        .all(|directive| directive.matches(&document))
}

impl TextDirective {
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

/// Helper to extract text from HTML, ignoring hidden elements (e.g., `<head>`, `<script>`, `<style>`, and `<template>`).
#[derive(Default)]
struct TextExtractor {
    text: String,
    hidden_stack: Vec<String>,
}

impl TextExtractor {
    fn new() -> Self {
        Self::default()
    }

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

/// Extract visible text from the given HTML content.
///
/// This method is a good enough heuristic using html5gum. All `CallbackEvent::String` is considered visible text,
/// except known hidden (e.g., `<head>`, `<script>`, `<style>`, and `<template>`).
fn extract_visible_text(input: &str) -> String {
    let mut extractor = TextExtractor::new();
    let emitter = CallbackEmitter::new(&mut extractor);
    let _: Result<(), _> = Tokenizer::new_with_emitter(input, emitter).collect();
    // It's kind of wasteful to split and re-join the text. However, given that extra whitespace may appear both inside
    // a tag and between tags, this is a simple way to ensure that the extracted text is normalized in a way that matches
    // how browsers treat whitespace for text fragments.
    extractor
        .text
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::fragment_checker::parsed_fragment::ParsedFragment;
    use url::Url;

    const INDEX_HTML: &str = include_str!("../../../../fixtures/text_fragments/index.html");

    #[test]
    fn extracts_visible_text_without_style_or_attributes() {
        let text = extract_visible_text(INDEX_HTML);

        assert!(text.contains("Sed porta nisl sit amet quam ornare rutrum."));
        assert!(text.contains("Proin vulputate mi id sem pulvinar euismod."));
        assert!(!text.contains("my-style-property"));
        assert!(!text.contains("my-element-attribute-value"));
    }

    #[test]
    fn check_text_fragments_from_chrome() {
        // This list was generated by using the "Copy link to highlight" feature in Chrome Version 147.0.7727.55 (Official Build) (64-bit)
        let url =
            Url::parse("http://localhost:8080/#:~:text=Proin-,vulputate,-mi%20id%20sem").unwrap();
        let parsed = ParsedFragment::parse(&url);
        assert!(check_text_fragments(
            &parsed.text_directives,
            INDEX_HTML,
            FileType::Html
        ));

        let url =
            Url::parse("http://localhost:8080/#:~:text=Proin-,vulputate,-mi%20id%20sema").unwrap();
        let parsed = ParsedFragment::parse(&url);
        assert!(!check_text_fragments(
            &parsed.text_directives,
            INDEX_HTML,
            FileType::Html
        ));

        let url = Url::parse("http://localhost:8080/#:~:text=massa.,Proin").unwrap();
        let parsed = ParsedFragment::parse(&url);
        assert!(check_text_fragments(
            &parsed.text_directives,
            INDEX_HTML,
            FileType::Html
        ));

        let url =
            Url::parse("http://localhost:8080/#:~:text=sit%20amet%20dignissim-,massa,-.").unwrap();
        let parsed = ParsedFragment::parse(&url);
        assert!(check_text_fragments(
            &parsed.text_directives,
            INDEX_HTML,
            FileType::Html
        ));

        let url =
            Url::parse("http://localhost:8080/#:~:text=sit%20amet%20dignissim-,massa,-.").unwrap();
        let parsed = ParsedFragment::parse(&url);
        assert!(check_text_fragments(
            &parsed.text_directives,
            INDEX_HTML,
            FileType::Html
        ));

        let url =
            Url::parse("http://localhost:8080/#:~:text=sit%20amet%20dignissim-,massam,-.").unwrap();
        let parsed = ParsedFragment::parse(&url);
        assert!(!check_text_fragments(
            &parsed.text_directives,
            INDEX_HTML,
            FileType::Html
        ));
    }
}
